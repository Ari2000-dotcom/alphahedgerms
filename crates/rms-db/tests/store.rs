//! The data layer's promises, checked on a real DuckDB (in memory or in a temp folder).

use chrono::{TimeZone, Utc};
use duckdb::params;
use rms_db::model::{NewFill, Side, Source};
use rms_db::Db;
use rust_decimal::Decimal;
use uuid::Uuid;

fn db() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.migrate().unwrap();
    db
}

/// A client with one Zerodha account, as every fill needs.
fn account(db: &Db, user: &str) -> (Uuid, Uuid) {
    let (client, acct) = (Uuid::new_v4(), Uuid::new_v4());
    db.with(|c| {
        c.execute("INSERT INTO clients (id, client_code, name) VALUES (CAST(? AS UUID), ?, 'Test client')", params![client.to_string(), format!("C-{user}")])?;
        c.execute(
            "INSERT INTO broker_accounts (id, client_id, broker, broker_user_id, label, credentials) VALUES (CAST(? AS UUID), CAST(? AS UUID), 'zerodha', ?, 'test', '\\x00'::BLOB)",
            params![acct.to_string(), client.to_string(), user],
        )?;
        Ok(())
    })
    .unwrap();
    (client, acct)
}

fn fill(client: Uuid, acct: Uuid, key: &str) -> NewFill {
    NewFill {
        account_id: acct,
        client_id: client,
        broker_order_id: "ORD1".into(),
        exchange: "NFO".into(),
        exchange_order_id: Some("EX1".into()),
        exchange_trade_id: Some(key.into()),
        dedupe_key: key.into(),
        segment: Some("NSE_FO".into()),
        token: Some("43721".into()),
        trading_symbol: "NIFTY2611324500CE".into(),
        side: Side::Sell,
        product: "NRML".into(),
        quantity: 75,
        price: Decimal::new(10125, 2),
        exchange_ts: Some(Utc.with_ymd_and_hms(2026, 11, 17, 4, 30, 0).unwrap()),
        broker_ts: None,
        source: Source::Live,
        raw: serde_json::json!({"trade_id": key}),
    }
}

#[test]
fn the_schema_applies_once_and_again_safely() {
    let db = db();
    assert_eq!(db.applied_migrations().unwrap(), vec![(1, "init".to_string()), (2, "position_tags".to_string())]);
    db.migrate().unwrap(); // a second run changes nothing and does not fail
    assert_eq!(db.applied_migrations().unwrap().len(), 2);
}

#[test]
fn the_same_trade_is_stored_once() {
    let db = db();
    let (client, acct) = account(&db, "AB1234");
    assert!(db.insert_fill(&fill(client, acct, "T1")).unwrap().is_some(), "a new trade is stored");
    // Seen again (live, then the end-of-day sweep): not stored twice.
    let mut again = fill(client, acct, "T1");
    again.source = Source::Sweep;
    assert!(db.insert_fill(&again).unwrap().is_none());
    assert!(db.insert_fill(&fill(client, acct, "T2")).unwrap().is_some());
    assert_eq!(db.fill_count().unwrap(), 2);
}

#[test]
fn two_accounts_may_share_a_trade_id() {
    let db = db();
    let (c1, a1) = account(&db, "AB1234");
    let (c2, a2) = account(&db, "CD5678");
    // Brokers reuse trade ids across accounts, so the key is per account.
    assert!(db.insert_fill(&fill(c1, a1, "T1")).unwrap().is_some());
    assert!(db.insert_fill(&fill(c2, a2, "T1")).unwrap().is_some());
}

#[test]
fn prices_keep_their_exact_decimals() {
    let db = db();
    let (client, acct) = account(&db, "AB1234");
    let mut f = fill(client, acct, "T1");
    f.price = Decimal::new(10_000_005, 4); // 1000.0005: a float would not hold this
    db.insert_fill(&f).unwrap();
    let back: String = db.with(|c| Ok(c.query_row("SELECT CAST(price AS VARCHAR) FROM fills", [], |r| r.get(0))?)).unwrap();
    assert_eq!(back, "1000.0005");
}

#[test]
fn bad_fills_are_refused() {
    let db = db();
    let (client, acct) = account(&db, "AB1234");
    let mut zero = fill(client, acct, "T1");
    zero.quantity = 0;
    assert!(db.insert_fill(&zero).is_err(), "quantity must be above zero");
    let bad_side = db.with(|c| {
        Ok(c.execute(
            "INSERT INTO fills (account_id, client_id, broker_order_id, exchange, dedupe_key, trading_symbol, side, product, quantity, price, source)
             VALUES (CAST(? AS UUID), CAST(? AS UUID), 'O', 'NFO', 'K', 'X', 'long', 'NRML', 1, 1, 'live')",
            params![acct.to_string(), client.to_string()],
        ))
    });
    assert!(bad_side.unwrap().is_err(), "side must be buy or sell");
    assert_eq!(db.fill_count().unwrap(), 0);
}

#[test]
fn a_strategy_keeps_its_tag_per_client() {
    let db = db();
    let (client, _) = account(&db, "AB1234");
    let add = |tag: &str| db.with(|c| Ok(c.execute("INSERT INTO strategies (client_id, tag, name) VALUES (CAST(? AS UUID), ?, 'Straddle')", params![client.to_string(), tag])));
    assert!(add("S1").unwrap().is_ok());
    assert!(add("S1").unwrap().is_err(), "a client cannot have two strategies with one tag");
    assert!(add("S2").unwrap().is_ok());
}

#[test]
fn tampering_with_history_is_detected() {
    let db = db();
    let (client, acct) = account(&db, "AB1234");
    db.insert_fill(&fill(client, acct, "T1")).unwrap();
    db.insert_fill(&fill(client, acct, "T2")).unwrap();
    let before = db.history_fingerprint().unwrap();
    assert_eq!(before, db.history_fingerprint().unwrap(), "stable while nothing changes");

    // Someone edits a price behind the program's back.
    db.with(|c| Ok(c.execute("UPDATE fills SET price = 1 WHERE dedupe_key = 'T1'", [])?)).unwrap();
    let after_edit = db.history_fingerprint().unwrap();
    assert_ne!(before, after_edit, "an edited price changes the fingerprint");

    // Or removes a trade.
    db.with(|c| Ok(c.execute("DELETE FROM fills WHERE dedupe_key = 'T2'", [])?)).unwrap();
    assert_ne!(after_edit, db.history_fingerprint().unwrap(), "a removed trade changes it too");
}

#[test]
fn orphans_are_found() {
    let db = db();
    let (client, acct) = account(&db, "AB1234");
    db.insert_fill(&fill(client, acct, "T1")).unwrap();
    assert!(db.orphans().unwrap().is_empty());
    db.with(|c| Ok(c.execute("DELETE FROM broker_accounts", [])?)).unwrap();
    let found = db.orphans().unwrap();
    assert!(found.iter().any(|(what, n)| what == "fills without an account" && *n == 1), "{found:?}");
}

#[test]
fn data_survives_closing_and_reopening_the_file() {
    let dir = std::env::temp_dir().join(format!("rms-test-{}", Uuid::new_v4().simple()));
    let path = dir.join("rms.duckdb");
    let (client, acct);
    {
        let db = Db::open(&path).unwrap();
        db.migrate().unwrap();
        (client, acct) = account(&db, "AB1234");
        db.insert_fill(&fill(client, acct, "T1")).unwrap();
    }
    let db = Db::open(&path).unwrap();
    db.migrate().unwrap();
    assert_eq!(db.fill_count().unwrap(), 1);
    // And the same trade is still recognised after the restart.
    assert!(db.insert_fill(&fill(client, acct, "T1")).unwrap().is_none());
    drop(db);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_day_of_fills_exports_to_a_small_parquet_file_that_reads_back() {
    let dir = std::env::temp_dir().join(format!("rms-test-{}", Uuid::new_v4().simple()));
    let db = db();
    let (client, acct) = account(&db, "AB1234");
    let n: i64 = 3000;
    for i in 0..n {
        let mut f = fill(client, acct, &format!("T{i}"));
        f.quantity = 75 + (i % 7) * 75;
        f.price = Decimal::new(9000 + (i % 400) * 5, 2);
        f.raw = serde_json::json!({"trade_id": format!("T{i}"), "order_id": "ORD1", "exchange": "NFO", "note": "a typical broker reply body, repeated"});
        db.insert_fill(&f).unwrap();
    }

    let (file, rows) = db.export_fills_day(&dir, "2026-11-17").unwrap().expect("there were fills that day");
    assert_eq!(rows, n);
    assert!(file.ends_with("fills/date=2026-11-17/fills.parquet"), "{file:?}");
    assert!(!file.with_extension("parquet.tmp").exists(), "no half-written file is left behind");

    // It reads back through DuckDB, with the same rows and the exact prices.
    let p = file.to_string_lossy().replace('\\', "/");
    let (count, sum): (i64, String) = db
        .with(|c| Ok(c.query_row(&format!("SELECT count(*), CAST(sum(price) AS VARCHAR) FROM read_parquet('{p}')"), [], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .unwrap();
    let live: String = db.with(|c| Ok(c.query_row("SELECT CAST(sum(price) AS VARCHAR) FROM fills", [], |r| r.get(0))?)).unwrap();
    assert_eq!(count, n);
    assert_eq!(sum, live);

    // Compressed: each fill is a few tens of bytes, not a few hundred.
    let bytes = std::fs::metadata(&file).unwrap().len();
    let per_fill = bytes as f64 / n as f64;
    eprintln!("{n} fills -> {bytes} bytes ({per_fill:.1} bytes per fill)");
    assert!(per_fill < 60.0, "{per_fill} bytes per fill is not small enough");

    // A day with no fills writes nothing.
    assert!(db.export_fills_day(&dir, "2026-01-01").unwrap().is_none());
    assert!(db.export_fills_day(&dir, "not-a-day").is_err());
    let _ = std::fs::remove_dir_all(dir);
}

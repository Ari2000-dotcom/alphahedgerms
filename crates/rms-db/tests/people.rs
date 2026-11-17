//! Clients, accounts and the latest positions: what the admin screen is built on.

use rms_db::people::{MarginRow, NewAccount, NewClient, PositionRow};
use rms_db::{Db, Error};

fn db() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.migrate().unwrap();
    db
}

fn client(db: &Db, code: &str, name: &str) -> uuid::Uuid {
    db.add_client(&NewClient { client_code: code.into(), name: name.into(), email: None, phone: None, notes: None }).unwrap()
}

fn account(db: &Db, client_id: uuid::Uuid, broker: &str, user: &str) -> uuid::Uuid {
    db.add_account(&NewAccount { client_id, broker: broker.into(), broker_user_id: user.into(), label: format!("{broker} {user}"), credentials: vec![1, 2, 3] }).unwrap()
}

fn pos(symbol: &str, qty: i64, pnl: f64) -> PositionRow {
    PositionRow { trading_symbol: symbol.into(), segment: None, token: None, product: "NRML".into(), net_quantity: qty, average_price: Some(101.25), last_price: Some(99.5), pnl: Some(pnl) }
}

#[test]
fn clients_are_listed_with_their_account_counts() {
    let db = db();
    let ravi = client(&db, "C001", "Ravi");
    client(&db, "C002", "Asha");
    account(&db, ravi, "zerodha", "AB1234");
    account(&db, ravi, "kotak", "K77AB");
    let list = db.list_clients().unwrap();
    assert_eq!(list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Asha", "Ravi"], "sorted by name");
    assert_eq!(list.iter().find(|c| c.name == "Ravi").unwrap().accounts, 2);
    assert_eq!(list.iter().find(|c| c.name == "Asha").unwrap().accounts, 0);
}

#[test]
fn a_client_code_or_an_account_cannot_be_added_twice() {
    let db = db();
    let ravi = client(&db, "C001", "Ravi");
    let again = db.add_client(&NewClient { client_code: "C001".into(), name: "Other".into(), email: None, phone: None, notes: None });
    assert!(matches!(again, Err(Error::Duplicate(m)) if m.contains("C001")));
    account(&db, ravi, "zerodha", "AB1234");
    let dup = db.add_account(&NewAccount { client_id: ravi, broker: "zerodha".into(), broker_user_id: "AB1234".into(), label: "x".into(), credentials: vec![] });
    assert!(matches!(dup, Err(Error::Duplicate(_))));
    let bad_broker = db.add_account(&NewAccount { client_id: ravi, broker: "binance".into(), broker_user_id: "X".into(), label: "x".into(), credentials: vec![] });
    assert!(bad_broker.is_err(), "only the five known brokers");
}

#[test]
fn sealed_credentials_come_back_as_stored() {
    let db = db();
    let a = account(&db, client(&db, "C001", "Ravi"), "angel", "A123");
    let (broker, _client, blob, user) = db.account_credentials(a).unwrap();
    assert_eq!((broker.as_str(), user.as_str(), blob), ("angel", "A123", vec![1, 2, 3]));
    assert!(matches!(db.account_credentials(uuid::Uuid::new_v4()), Err(Error::NotFound(_))));
}

#[test]
fn the_latest_read_replaces_the_one_before_even_when_empty() {
    let db = db();
    let ravi = client(&db, "C001", "Ravi");
    let a = account(&db, ravi, "zerodha", "AB1234");
    db.save_positions(a, &[pos("NIFTY2611324500CE", -75, 120.5), pos("NIFTY2611324500PE", -75, -30.0)]).unwrap();
    assert_eq!(db.latest_positions(None).unwrap().len(), 2);

    db.save_positions(a, &[pos("NIFTY2611324500CE", -75, 200.0)]).unwrap();
    let now = db.latest_positions(None).unwrap();
    assert_eq!(now.len(), 1, "the PE was closed, so only the CE remains");
    assert_eq!(now[0].position.pnl, Some(200.0));
    assert_eq!((now[0].client_name.as_str(), now[0].broker.as_str()), ("Ravi", "zerodha"));

    // A flat book is a real answer: yesterday's positions must not linger.
    db.save_positions(a, &[]).unwrap();
    assert!(db.latest_positions(None).unwrap().is_empty());
}

#[test]
fn positions_can_be_limited_to_one_client_and_skip_disabled_accounts() {
    let db = db();
    let (ravi, asha) = (client(&db, "C001", "Ravi"), client(&db, "C002", "Asha"));
    let (a1, a2) = (account(&db, ravi, "zerodha", "AB1234"), account(&db, asha, "angel", "A123"));
    db.save_positions(a1, &[pos("X", 75, 1.0)]).unwrap();
    db.save_positions(a2, &[pos("Y", -50, 2.0)]).unwrap();
    assert_eq!(db.latest_positions(Some(ravi)).unwrap().len(), 1);
    assert_eq!(db.latest_positions(None).unwrap().len(), 2);
    db.set_account_enabled(a2, false).unwrap();
    assert_eq!(db.latest_positions(None).unwrap().len(), 1, "a disabled account drops out");
}

#[test]
fn the_newest_margin_wins() {
    let db = db();
    let a = account(&db, client(&db, "C001", "Ravi"), "zerodha", "AB1234");
    let m = |available: f64| MarginRow { available, used: 10.0, total: available + 10.0, cash: Some(5.0), collateral: Some(available - 5.0), taken_at: String::new() };
    db.save_margin(a, &m(1000.0)).unwrap();
    db.save_margin(a, &m(2500.5)).unwrap();
    let all = db.latest_margins().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].1.available, 2500.5);
}

#[test]
fn an_account_with_history_cannot_be_deleted_but_a_fresh_one_can() {
    let db = db();
    let ravi = client(&db, "C001", "Ravi");
    let fresh = account(&db, ravi, "zerodha", "AB1234");
    db.save_positions(fresh, &[pos("X", 1, 0.0)]).unwrap();
    db.delete_account(fresh).unwrap();
    assert!(db.list_accounts(None).unwrap().is_empty());

    let kept = account(&db, ravi, "kotak", "K77AB");
    db.with(|c| {
        Ok(c.execute(
            "INSERT INTO fills (account_id, client_id, broker_order_id, exchange, dedupe_key, trading_symbol, side, product, quantity, price, source)
             VALUES (CAST(? AS UUID), CAST(? AS UUID), 'O', 'NFO', 'K', 'X', 'buy', 'NRML', 1, 1, 'live')",
            duckdb::params![kept.to_string(), ravi.to_string()],
        )?)
    })
    .unwrap();
    let e = db.delete_account(kept).unwrap_err().to_string();
    assert!(e.contains("disable it instead"), "{e}");
}

#[test]
fn a_client_is_found_by_name_and_codes_count_up() {
    let db = db();
    assert_eq!(db.next_client_code().unwrap(), "C001");
    let ravi = client(&db, "C001", "Ravi Menon");
    client(&db, "C007", "Asha Rao");
    assert_eq!(db.next_client_code().unwrap(), "C008", "one past the highest number, not the count");
    client(&db, "FAMILY-A", "Other"); // a code in another style does not confuse the count
    assert_eq!(db.next_client_code().unwrap(), "C008");
    assert_eq!(db.find_client_by_name("  ravi MENON ").unwrap(), Some(ravi), "case and spaces are ignored");
    assert_eq!(db.find_client_by_name("Nobody").unwrap(), None);
}

#[test]
fn an_empty_client_can_be_undone_but_one_with_an_account_stays() {
    let db = db();
    let empty = client(&db, "C001", "Ravi");
    assert!(db.delete_client_if_empty(empty).unwrap());
    assert!(db.list_clients().unwrap().is_empty());
    let kept = client(&db, "C002", "Asha");
    account(&db, kept, "zerodha", "AB1234");
    assert!(!db.delete_client_if_empty(kept).unwrap());
    assert_eq!(db.list_clients().unwrap().len(), 1);
}

#[test]
fn account_details_can_be_replaced_without_losing_the_account() {
    let db = db();
    let ravi = client(&db, "C001", "Ravi");
    let a = account(&db, ravi, "zerodha", "AB1234");
    let other = account(&db, ravi, "zerodha", "CD5678");
    assert!(db.account_exists("zerodha", "AB1234").unwrap());
    assert!(!db.account_exists("kotak", "AB1234").unwrap());

    db.update_account_credentials(a, "AB9999", &[9, 9]).unwrap();
    let (_, _, blob, user) = db.account_credentials(a).unwrap();
    assert_eq!((blob, user.as_str()), (vec![9, 9], "AB9999"));
    // The id cannot be changed to one another account already has.
    let e = db.update_account_credentials(other, "AB9999", &[1]).unwrap_err();
    assert!(matches!(e, Error::Duplicate(_)), "{e:?}");
    assert!(db.account_blobs().unwrap().len() == 2);
}

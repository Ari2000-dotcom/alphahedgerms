//! The schema's promises, checked against a real Postgres.
//!
//! These need a database. Set `RMS_TEST_DATABASE_URL` (the local cluster from
//! scripts/setup-db.ps1 works) and run `cargo test -p rms-db`. Without it every
//! test says so and passes, so a machine with no database still builds and tests.
//! Each test works in its own throwaway schema and leaves nothing behind.

use rms_db::fills::insert_fill;
use rms_db::model::{NewFill, Side, Source};
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use uuid::Uuid;

/// A fresh, migrated schema, or None when no test database is configured.
async fn fresh() -> Option<(PgPool, String, PgPool)> {
    let url = std::env::var("RMS_TEST_DATABASE_URL").ok().filter(|u| !u.is_empty());
    let Some(url) = url else {
        eprintln!("RMS_TEST_DATABASE_URL is not set: skipping this database test");
        return None;
    };
    let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("connect");
    let schema = format!("t_{}", Uuid::new_v4().simple());
    admin.execute(format!("CREATE SCHEMA {schema}").as_str()).await.unwrap();
    let s = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |conn, _| {
            let s = s.clone();
            Box::pin(async move {
                conn.execute(format!("SET search_path TO {s}").as_str()).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("connect to the test schema");
    rms_db::migrate(&pool).await.expect("migrations apply");
    Some((pool, schema, admin))
}

async fn drop_schema(admin: &PgPool, schema: &str) {
    admin.execute(format!("DROP SCHEMA {schema} CASCADE").as_str()).await.unwrap();
}

/// A client with one Zerodha account, as every fill needs.
async fn account(pool: &PgPool, user: &str) -> (Uuid, Uuid) {
    let client: (Uuid,) = sqlx::query_as("INSERT INTO clients (client_code, name) VALUES ($1, 'Test client') RETURNING id")
        .bind(format!("C-{user}"))
        .fetch_one(pool)
        .await
        .unwrap();
    let acct: (Uuid,) = sqlx::query_as(
        "INSERT INTO broker_accounts (client_id, broker, broker_user_id, label, credentials) VALUES ($1, 'zerodha', $2, 'test', '\\x00') RETURNING id",
    )
    .bind(client.0)
    .bind(user)
    .fetch_one(pool)
    .await
    .unwrap();
    (client.0, acct.0)
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
        exchange_ts: None,
        broker_ts: None,
        source: Source::Live,
        raw: serde_json::json!({"trade_id": key}),
    }
}

#[tokio::test]
async fn the_same_trade_is_stored_once() {
    let Some((pool, schema, admin)) = fresh().await else { return };
    let (client, acct) = account(&pool, "AB1234").await;

    let first = insert_fill(&pool, &fill(client, acct, "T1")).await.unwrap();
    assert!(first.is_some(), "a new trade is stored");
    // Seen again (live, then the end-of-day sweep): not stored twice.
    let mut again = fill(client, acct, "T1");
    again.source = Source::Sweep;
    assert!(insert_fill(&pool, &again).await.unwrap().is_none());
    // A different trade is stored.
    assert!(insert_fill(&pool, &fill(client, acct, "T2")).await.unwrap().is_some());

    let n: (i64,) = sqlx::query_as("SELECT count(*) FROM fills").fetch_one(&pool).await.unwrap();
    assert_eq!(n.0, 2);
    drop_schema(&admin, &schema).await;
}

#[tokio::test]
async fn two_accounts_may_share_a_trade_id() {
    let Some((pool, schema, admin)) = fresh().await else { return };
    let (c1, a1) = account(&pool, "AB1234").await;
    let (c2, a2) = account(&pool, "CD5678").await;
    // Brokers reuse trade ids across accounts, so the key is per account.
    assert!(insert_fill(&pool, &fill(c1, a1, "T1")).await.unwrap().is_some());
    assert!(insert_fill(&pool, &fill(c2, a2, "T1")).await.unwrap().is_some());
    drop_schema(&admin, &schema).await;
}

#[tokio::test]
async fn history_cannot_be_edited_or_deleted() {
    let Some((pool, schema, admin)) = fresh().await else { return };
    let (client, acct) = account(&pool, "AB1234").await;
    insert_fill(&pool, &fill(client, acct, "T1")).await.unwrap();

    let upd = sqlx::query("UPDATE fills SET price = 1").execute(&pool).await;
    let del = sqlx::query("DELETE FROM fills").execute(&pool).await;
    for (what, r) in [("update", upd), ("delete", del)] {
        let e = r.expect_err(&format!("a fill {what} must be refused")).to_string();
        assert!(e.contains("append-only"), "{what}: {e}");
    }
    // The fill is untouched.
    let p: (Decimal,) = sqlx::query_as("SELECT price FROM fills").fetch_one(&pool).await.unwrap();
    assert_eq!(p.0, Decimal::new(10125, 2));
    drop_schema(&admin, &schema).await;
}

#[tokio::test]
async fn the_audit_trail_and_assignment_history_are_append_only_too() {
    let Some((pool, schema, admin)) = fresh().await else { return };
    sqlx::query("INSERT INTO audit_log (actor_kind, action) VALUES ('system', 'test')").execute(&pool).await.unwrap();
    assert!(sqlx::query("DELETE FROM audit_log").execute(&pool).await.is_err());
    assert!(sqlx::query("UPDATE audit_log SET action = 'x'").execute(&pool).await.is_err());

    let (client, _acct) = account(&pool, "AB1234").await;
    sqlx::query("INSERT INTO assignment_events (client_id, subject_kind, subject_id) VALUES ($1, 'order', 1)")
        .bind(client)
        .execute(&pool)
        .await
        .unwrap();
    assert!(sqlx::query("DELETE FROM assignment_events").execute(&pool).await.is_err());
    drop_schema(&admin, &schema).await;
}

#[tokio::test]
async fn a_strategy_keeps_its_tag_per_client() {
    let Some((pool, schema, admin)) = fresh().await else { return };
    let (client, _acct) = account(&pool, "AB1234").await;
    let ins = |tag: &'static str| sqlx::query("INSERT INTO strategies (client_id, tag, name) VALUES ($1, $2, 'Straddle')").bind(client).bind(tag);
    ins("S1").execute(&pool).await.unwrap();
    assert!(ins("S1").execute(&pool).await.is_err(), "a client cannot have two strategies with one tag");
    ins("S2").execute(&pool).await.unwrap();
    drop_schema(&admin, &schema).await;
}

#[tokio::test]
async fn a_fill_needs_a_positive_quantity_and_a_known_side() {
    let Some((pool, schema, admin)) = fresh().await else { return };
    let (client, acct) = account(&pool, "AB1234").await;
    let mut bad = fill(client, acct, "T9");
    bad.quantity = 0;
    assert!(insert_fill(&pool, &bad).await.is_err());
    let e = sqlx::query(
        "INSERT INTO fills (account_id, client_id, broker_order_id, exchange, dedupe_key, trading_symbol, side, product, quantity, price, source, raw)
         VALUES ($1, $2, 'O', 'NFO', 'K', 'X', 'long', 'NRML', 1, 1, 'live', '{}')",
    )
    .bind(acct)
    .bind(client)
    .execute(&pool)
    .await;
    assert!(e.is_err(), "side must be buy or sell");
    drop_schema(&admin, &schema).await;
}

//! Strategy tags: names, tags, who may join, and that they outlive a day's snapshot.

use duckdb::params;
use rms_db::strategies::Leg;
use rms_db::Db;
use uuid::Uuid;

fn db() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.migrate().unwrap();
    db
}

fn client_with_account(db: &Db, code: &str) -> (String, String) {
    let (client, acct) = (Uuid::new_v4().to_string(), Uuid::new_v4().to_string());
    db.with(|c| {
        c.execute("INSERT INTO clients (id, client_code, name) VALUES (CAST(? AS UUID), ?, ?)", params![client, code, code])?;
        c.execute(
            "INSERT INTO broker_accounts (id, client_id, broker, broker_user_id, label, credentials) VALUES (CAST(? AS UUID), CAST(? AS UUID), 'zerodha', ?, 'test', '\\x00'::BLOB)",
            params![acct, client, code],
        )?;
        Ok(())
    })
    .unwrap();
    (client, acct)
}

fn leg(account: &str, symbol: &str) -> Leg {
    Leg { account_id: account.into(), trading_symbol: symbol.into(), product: "NRML".into() }
}

#[test]
fn strategies_get_numbered_tags_and_unique_names_per_client() {
    let db = db();
    let (c1, _) = client_with_account(&db, "C1");
    let (c2, _) = client_with_account(&db, "C2");
    let a = db.create_strategy(&c1, "Iron condor").unwrap();
    let b = db.create_strategy(&c1, "Straddle").unwrap();
    assert_eq!((a.tag.as_str(), b.tag.as_str()), ("S1", "S2"));
    assert!(db.create_strategy(&c1, "  iron CONDOR ").is_err(), "same name, ignoring case, for the same client");
    assert_eq!(db.create_strategy(&c2, "Iron condor").unwrap().tag, "S1", "another client may reuse the name");
    assert!(db.create_strategy(&c1, "   ").is_err());
}

#[test]
fn legs_join_move_and_leave_a_strategy() {
    let db = db();
    let (c1, a1) = client_with_account(&db, "C1");
    let s1 = db.create_strategy(&c1, "Condor").unwrap();
    let s2 = db.create_strategy(&c1, "Hedge").unwrap();
    let legs = [leg(&a1, "NIFTY26DEC22600CE"), leg(&a1, "NIFTY26DEC23000PE")];
    assert_eq!(db.tag_positions(&s1.id, &legs).unwrap(), 2);
    assert_eq!(db.strategy(&s1.id).unwrap().legs, 2);
    db.tag_positions(&s2.id, &legs[..1]).unwrap();
    assert_eq!((db.strategy(&s1.id).unwrap().legs, db.strategy(&s2.id).unwrap().legs), (1, 1), "a leg moves, it is not copied");
    assert_eq!(db.untag_positions(&legs[1..]).unwrap(), 1);
    assert_eq!(db.strategy(&s1.id).unwrap().legs, 0);
}

#[test]
fn only_the_clients_own_positions_may_join() {
    let db = db();
    let (c1, _) = client_with_account(&db, "C1");
    let (_, a2) = client_with_account(&db, "C2");
    let s = db.create_strategy(&c1, "Condor").unwrap();
    assert!(db.tag_positions(&s.id, &[leg(&a2, "NIFTY26DEC22600CE")]).is_err());
    assert_eq!(db.strategy(&s.id).unwrap().legs, 0, "a refused tag leaves nothing behind");
}

#[test]
fn deleting_a_strategy_frees_its_legs_and_leaves_no_orphans() {
    let db = db();
    let (c1, a1) = client_with_account(&db, "C1");
    let s = db.create_strategy(&c1, "Condor").unwrap();
    db.tag_positions(&s.id, &[leg(&a1, "NIFTY26DEC22600CE")]).unwrap();
    db.delete_strategy(&s.id).unwrap();
    assert!(db.orphans().unwrap().is_empty());
    assert!(db.list_strategies().unwrap().is_empty());
}

#[test]
fn rename_keeps_names_unique() {
    let db = db();
    let (c1, _) = client_with_account(&db, "C1");
    let a = db.create_strategy(&c1, "Condor").unwrap();
    db.create_strategy(&c1, "Hedge").unwrap();
    assert!(db.rename_strategy(&a.id, "hedge").is_err());
    db.rename_strategy(&a.id, "Iron condor").unwrap();
    assert_eq!(db.strategy(&a.id).unwrap().name, "Iron condor");
}

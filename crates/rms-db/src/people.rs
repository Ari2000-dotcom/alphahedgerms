//! Clients, their broker accounts, and the latest positions and margin read for each account.

use crate::{Db, Error, Result};
use duckdb::{params, OptionalExt};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewClient {
    pub client_code: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientRow {
    pub id: String,
    pub client_code: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub notes: Option<String>,
    pub status: String,
    pub created_at: String,
    pub accounts: i64,
}

#[derive(Debug, Clone)]
pub struct NewAccount {
    pub client_id: Uuid,
    pub broker: String,
    pub broker_user_id: String,
    pub label: String,
    /// Already sealed with the vault. This crate never sees the plain credentials.
    pub credentials: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountRow {
    pub id: String,
    pub client_id: String,
    pub client_code: String,
    pub client_name: String,
    pub broker: String,
    pub broker_user_id: String,
    pub label: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PositionRow {
    pub trading_symbol: String,
    pub segment: Option<String>,
    pub token: Option<String>,
    pub product: String,
    pub net_quantity: i64,
    pub average_price: Option<f64>,
    pub last_price: Option<f64>,
    pub pnl: Option<f64>,
}

/// A position together with whose it is and when it was read.
#[derive(Debug, Clone, Serialize)]
pub struct PositionView {
    pub account_id: String,
    pub client_id: String,
    pub client_name: String,
    pub broker: String,
    pub account_label: String,
    pub taken_at: String,
    #[serde(flatten)]
    pub position: PositionRow,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarginRow {
    pub available: f64,
    pub used: f64,
    pub total: f64,
    pub cash: Option<f64>,
    pub collateral: Option<f64>,
    pub taken_at: String,
}

/// DuckDB reports a clash on a unique column as "Duplicate key ..."; say so plainly.
fn clash(e: duckdb::Error, what: String) -> Error {
    if e.to_string().contains("Duplicate key") || e.to_string().contains("violates unique") {
        Error::Duplicate(what)
    } else {
        Error::Db(e)
    }
}

impl Db {
    pub fn add_client(&self, c: &NewClient) -> Result<Uuid> {
        let id = Uuid::new_v4();
        self.with(|conn| {
            conn.execute(
                "INSERT INTO clients (id, client_code, name, email, phone, notes) VALUES (CAST(? AS UUID), ?, ?, ?, ?, ?)",
                params![id.to_string(), c.client_code, c.name, c.email, c.phone, c.notes],
            )
            .map_err(|e| clash(e, format!("client code {} is already used", c.client_code)))?;
            Ok(id)
        })
    }

    pub fn list_clients(&self) -> Result<Vec<ClientRow>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT CAST(c.id AS VARCHAR), c.client_code, c.name, c.email, c.phone, c.notes, c.status, CAST(c.created_at AS VARCHAR),
                        (SELECT count(*) FROM broker_accounts a WHERE a.client_id = c.id)
                 FROM clients c ORDER BY c.name",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(ClientRow {
                    id: r.get(0)?,
                    client_code: r.get(1)?,
                    name: r.get(2)?,
                    email: r.get(3)?,
                    phone: r.get(4)?,
                    notes: r.get(5)?,
                    status: r.get(6)?,
                    created_at: r.get(7)?,
                    accounts: r.get(8)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn client_exists(&self, id: Uuid) -> Result<bool> {
        self.with(|c| Ok(c.query_row("SELECT count(*) FROM clients WHERE id = CAST(? AS UUID)", params![id.to_string()], |r| r.get::<_, i64>(0))? > 0))
    }

    pub fn add_account(&self, a: &NewAccount) -> Result<Uuid> {
        let id = Uuid::new_v4();
        self.with(|conn| {
            conn.execute(
                "INSERT INTO broker_accounts (id, client_id, broker, broker_user_id, label, credentials)
                 VALUES (CAST(? AS UUID), CAST(? AS UUID), ?, ?, ?, ?)",
                params![id.to_string(), a.client_id.to_string(), a.broker, a.broker_user_id, a.label, a.credentials],
            )
            .map_err(|e| clash(e, format!("a {} account {} is already added", a.broker, a.broker_user_id)))?;
            Ok(id)
        })
    }

    pub fn list_accounts(&self, client: Option<Uuid>) -> Result<Vec<AccountRow>> {
        self.with(|conn| {
            let sql = "SELECT CAST(a.id AS VARCHAR), CAST(a.client_id AS VARCHAR), c.client_code, c.name, a.broker, a.broker_user_id, a.label, a.enabled, CAST(a.created_at AS VARCHAR)
                       FROM broker_accounts a JOIN clients c ON c.id = a.client_id
                       WHERE (? IS NULL OR a.client_id = CAST(? AS UUID))
                       ORDER BY c.name, a.label";
            let key = client.map(|c| c.to_string());
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map(params![key, key], |r| {
                Ok(AccountRow {
                    id: r.get(0)?,
                    client_id: r.get(1)?,
                    client_code: r.get(2)?,
                    client_name: r.get(3)?,
                    broker: r.get(4)?,
                    broker_user_id: r.get(5)?,
                    label: r.get(6)?,
                    enabled: r.get(7)?,
                    created_at: r.get(8)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    /// An account's sealed credentials, and who owns it.
    pub fn account_credentials(&self, id: Uuid) -> Result<(String, String, Vec<u8>, String)> {
        self.with(|c| {
            c.query_row(
                "SELECT broker, CAST(client_id AS VARCHAR), credentials, broker_user_id FROM broker_accounts WHERE id = CAST(? AS UUID)",
                params![id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("account {id}")))
        })
    }

    pub fn set_account_enabled(&self, id: Uuid, enabled: bool) -> Result<()> {
        self.with(|c| {
            let n = c.execute("UPDATE broker_accounts SET enabled = ? WHERE id = CAST(? AS UUID)", params![enabled, id.to_string()])?;
            if n == 0 {
                return Err(Error::NotFound(format!("account {id}")));
            }
            Ok(())
        })
    }

    /// Removes an account that has no history. One with trades or orders on record is kept (the
    /// record is append-only) and should be disabled instead.
    pub fn delete_account(&self, id: Uuid) -> Result<()> {
        self.with(|c| {
            let key = id.to_string();
            let history: i64 = c.query_row(
                "SELECT (SELECT count(*) FROM fills WHERE account_id = CAST(? AS UUID)) + (SELECT count(*) FROM orders WHERE account_id = CAST(? AS UUID))",
                params![key, key],
                |r| r.get(0),
            )?;
            if history > 0 {
                return Err(Error::Other("this account has trades or orders on record, so it cannot be deleted: disable it instead".into()));
            }
            c.execute("DELETE FROM position_snapshots WHERE account_id = CAST(? AS UUID)", params![key])?;
            c.execute("DELETE FROM position_runs WHERE account_id = CAST(? AS UUID)", params![key])?;
            c.execute("DELETE FROM margin_snapshots WHERE account_id = CAST(? AS UUID)", params![key])?;
            let n = c.execute("DELETE FROM broker_accounts WHERE id = CAST(? AS UUID)", params![key])?;
            if n == 0 {
                return Err(Error::NotFound(format!("account {id}")));
            }
            Ok(())
        })
    }

    /// Stores what the broker said an account's positions are right now (even an empty book).
    pub fn save_positions(&self, account: Uuid, rows: &[PositionRow]) -> Result<()> {
        self.with(|c| {
            let key = account.to_string();
            c.execute_batch("BEGIN TRANSACTION")?;
            let result = (|| -> Result<()> {
                let run: i64 = c.query_row(
                    "INSERT INTO position_runs (account_id, row_count) VALUES (CAST(? AS UUID), ?) RETURNING id",
                    params![key, rows.len() as i64],
                    |r| r.get(0),
                )?;
                for p in rows {
                    c.execute(
                        "INSERT INTO position_snapshots (run_id, account_id, segment, token, trading_symbol, product, net_quantity, average_price, last_price, pnl)
                         VALUES (?, CAST(? AS UUID), ?, ?, ?, ?, ?, CAST(? AS DECIMAL(18, 4)), CAST(? AS DECIMAL(18, 4)), CAST(? AS DECIMAL(18, 4)))",
                        params![
                            run,
                            key,
                            p.segment,
                            p.token,
                            p.trading_symbol,
                            p.product,
                            p.net_quantity,
                            p.average_price.map(|v| format!("{v:.4}")),
                            p.last_price.map(|v| format!("{v:.4}")),
                            p.pnl.map(|v| format!("{v:.4}")),
                        ],
                    )?;
                }
                Ok(())
            })();
            match result {
                Ok(()) => c.execute_batch("COMMIT")?,
                Err(e) => {
                    let _ = c.execute_batch("ROLLBACK");
                    return Err(e);
                }
            }
            Ok(())
        })
    }

    /// The latest positions read for every enabled account (or one client's), with whose they are.
    pub fn latest_positions(&self, client: Option<Uuid>) -> Result<Vec<PositionView>> {
        self.with(|conn| {
            let sql = "SELECT CAST(a.id AS VARCHAR), CAST(a.client_id AS VARCHAR), c.name, a.broker, a.label, CAST(p.taken_at AS VARCHAR),
                              p.trading_symbol, p.segment, p.token, p.product, p.net_quantity,
                              CAST(p.average_price AS DOUBLE), CAST(p.last_price AS DOUBLE), CAST(p.pnl AS DOUBLE)
                       FROM position_snapshots p
                       JOIN broker_accounts a ON a.id = p.account_id
                       JOIN clients c ON c.id = a.client_id
                       WHERE a.enabled AND p.run_id = (SELECT max(r.id) FROM position_runs r WHERE r.account_id = p.account_id)
                         AND (? IS NULL OR a.client_id = CAST(? AS UUID))
                       ORDER BY c.name, a.label, p.trading_symbol";
            let key = client.map(|c| c.to_string());
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map(params![key, key], |r| {
                Ok(PositionView {
                    account_id: r.get(0)?,
                    client_id: r.get(1)?,
                    client_name: r.get(2)?,
                    broker: r.get(3)?,
                    account_label: r.get(4)?,
                    taken_at: r.get(5)?,
                    position: PositionRow {
                        trading_symbol: r.get(6)?,
                        segment: r.get(7)?,
                        token: r.get(8)?,
                        product: r.get(9)?,
                        net_quantity: r.get(10)?,
                        average_price: r.get(11)?,
                        last_price: r.get(12)?,
                        pnl: r.get(13)?,
                    },
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    pub fn save_margin(&self, account: Uuid, m: &MarginRow) -> Result<()> {
        self.with(|c| {
            let dec = |v: f64| format!("{v:.4}");
            c.execute(
                "INSERT INTO margin_snapshots (account_id, available, used, total, cash, collateral)
                 VALUES (CAST(? AS UUID), CAST(? AS DECIMAL(18, 4)), CAST(? AS DECIMAL(18, 4)), CAST(? AS DECIMAL(18, 4)), CAST(? AS DECIMAL(18, 4)), CAST(? AS DECIMAL(18, 4)))",
                params![account.to_string(), dec(m.available), dec(m.used), dec(m.total), m.cash.map(dec), m.collateral.map(dec)],
            )?;
            Ok(())
        })
    }

    /// The newest margin read of every enabled account, by account id.
    pub fn latest_margins(&self) -> Result<Vec<(String, MarginRow)>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT CAST(m.account_id AS VARCHAR), CAST(m.available AS DOUBLE), CAST(m.used AS DOUBLE), CAST(m.total AS DOUBLE),
                        CAST(m.cash AS DOUBLE), CAST(m.collateral AS DOUBLE), CAST(m.taken_at AS VARCHAR)
                 FROM margin_snapshots m
                 WHERE m.id = (SELECT max(x.id) FROM margin_snapshots x WHERE x.account_id = m.account_id)",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, MarginRow { available: r.get(1)?, used: r.get(2)?, total: r.get(3)?, cash: r.get(4)?, collateral: r.get(5)?, taken_at: r.get(6)? }))
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }
}

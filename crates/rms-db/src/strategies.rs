//! Strategies and the tags that put open positions into them.
//!
//! A strategy belongs to one client and has a name the admin chooses and a short tag (S1, S2, ...)
//! made for it. A position joins a strategy by its identity (account, symbol, product), and only if
//! the account is that client's own.

use crate::{Db, Error, Result};
use duckdb::{params, OptionalExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct StrategyRow {
    pub id: String,
    pub client_id: String,
    pub tag: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    /// How many positions are tagged to it now.
    pub legs: i64,
}

/// One position, named the way the tag table keys it.
#[derive(Debug, Clone, Deserialize)]
pub struct Leg {
    pub account_id: String,
    pub trading_symbol: String,
    pub product: String,
}

fn uuid(s: &str, what: &str) -> Result<String> {
    Uuid::parse_str(s).map(|u| u.to_string()).map_err(|_| Error::Other(format!("{what} is not a valid id")))
}

impl Db {
    pub fn list_strategies(&self) -> Result<Vec<StrategyRow>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT CAST(s.id AS VARCHAR), CAST(s.client_id AS VARCHAR), s.tag, s.name, s.status, CAST(s.created_at AS VARCHAR),
                        (SELECT count(*) FROM position_tags t WHERE t.strategy_id = s.id)
                 FROM strategies s ORDER BY s.created_at, s.tag",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(StrategyRow { id: r.get(0)?, client_id: r.get(1)?, tag: r.get(2)?, name: r.get(3)?, status: r.get(4)?, created_at: r.get(5)?, legs: r.get(6)? })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    /// Makes a strategy for a client. The name must be new for that client (ignoring case); the tag is made here.
    pub fn create_strategy(&self, client_id: &str, name: &str) -> Result<StrategyRow> {
        let client = uuid(client_id, "the client")?;
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(Error::Other("give the strategy a name".into()));
        }
        if name.chars().count() > 60 {
            return Err(Error::Other("a strategy name can be at most 60 characters".into()));
        }
        let id = Uuid::new_v4().to_string();
        self.with(|c| {
            let exists: Option<i64> = c.query_row("SELECT 1 FROM clients WHERE id = CAST(? AS UUID)", params![client], |r| r.get(0)).optional()?;
            if exists.is_none() {
                return Err(Error::NotFound("client".into()));
            }
            let same: i64 = c.query_row("SELECT count(*) FROM strategies WHERE client_id = CAST(? AS UUID) AND lower(name) = lower(?)", params![client, name], |r| r.get(0))?;
            if same > 0 {
                return Err(Error::Duplicate(format!("this client already has a strategy called {name}")));
            }
            // S1, S2, ... : one more than the highest number this client has used.
            let used: i64 = c.query_row(
                "SELECT coalesce(max(try_cast(substr(tag, 2) AS INTEGER)), 0) FROM strategies WHERE client_id = CAST(? AS UUID) AND tag LIKE 'S%'",
                params![client],
                |r| r.get(0),
            )?;
            let tag = format!("S{}", used + 1);
            c.execute("INSERT INTO strategies (id, client_id, tag, name) VALUES (CAST(? AS UUID), CAST(? AS UUID), ?, ?)", params![id, client, tag, name])?;
            Ok(())
        })?;
        self.strategy(&id)
    }

    pub fn strategy(&self, id: &str) -> Result<StrategyRow> {
        let id = uuid(id, "the strategy")?;
        self.list_strategies()?.into_iter().find(|s| s.id == id).ok_or_else(|| Error::NotFound("strategy".into()))
    }

    pub fn rename_strategy(&self, id: &str, name: &str) -> Result<()> {
        let s = self.strategy(id)?;
        let name = name.trim().to_string();
        if name.is_empty() || name.chars().count() > 60 {
            return Err(Error::Other("a strategy name needs 1 to 60 characters".into()));
        }
        self.with(|c| {
            let same: i64 = c.query_row(
                "SELECT count(*) FROM strategies WHERE client_id = CAST(? AS UUID) AND lower(name) = lower(?) AND id <> CAST(? AS UUID)",
                params![s.client_id, name, s.id],
                |r| r.get(0),
            )?;
            if same > 0 {
                return Err(Error::Duplicate(format!("this client already has a strategy called {name}")));
            }
            c.execute("UPDATE strategies SET name = ? WHERE id = CAST(? AS UUID)", params![name, s.id])?;
            Ok(())
        })
    }

    /// Removes a strategy and un-tags its positions (the positions themselves are untouched).
    pub fn delete_strategy(&self, id: &str) -> Result<()> {
        let s = self.strategy(id)?;
        self.with(|c| {
            c.execute("DELETE FROM position_tags WHERE strategy_id = CAST(? AS UUID)", params![s.id])?;
            c.execute("DELETE FROM strategies WHERE id = CAST(? AS UUID)", params![s.id])?;
            Ok(())
        })
    }

    /// Puts positions into a strategy as its legs. Every account must belong to the strategy's client;
    /// a position already in another strategy moves. Returns how many were tagged.
    pub fn tag_positions(&self, strategy_id: &str, legs: &[Leg]) -> Result<usize> {
        let s = self.strategy(strategy_id)?;
        if legs.is_empty() {
            return Err(Error::Other("choose at least one position".into()));
        }
        let mut clean = Vec::with_capacity(legs.len());
        for l in legs {
            clean.push((uuid(&l.account_id, "the account")?, l.trading_symbol.clone(), l.product.clone()));
        }
        self.with(|c| {
            c.execute_batch("BEGIN TRANSACTION")?;
            let done = (|| -> Result<usize> {
                for (account, symbol, product) in &clean {
                    let owner: Option<String> = c.query_row("SELECT CAST(client_id AS VARCHAR) FROM broker_accounts WHERE id = CAST(? AS UUID)", params![account], |r| r.get(0)).optional()?;
                    if owner.as_deref() != Some(s.client_id.as_str()) {
                        return Err(Error::Other(format!("{symbol} is not this client's position, so it cannot join {}", s.name)));
                    }
                    c.execute(
                        "INSERT INTO position_tags (account_id, trading_symbol, product, strategy_id) VALUES (CAST(? AS UUID), ?, ?, CAST(? AS UUID))
                         ON CONFLICT (account_id, trading_symbol, product) DO UPDATE SET strategy_id = excluded.strategy_id, tagged_at = now()",
                        params![account, symbol, product, s.id],
                    )?;
                }
                Ok(clean.len())
            })();
            match done {
                Ok(n) => {
                    c.execute_batch("COMMIT")?;
                    Ok(n)
                }
                Err(e) => {
                    let _ = c.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })
    }

    /// Takes positions out of whatever strategy they are in. Returns how many were tagged before.
    pub fn untag_positions(&self, legs: &[Leg]) -> Result<usize> {
        let mut n = 0;
        for l in legs {
            let account = uuid(&l.account_id, "the account")?;
            n += self.with(|c| Ok(c.execute("DELETE FROM position_tags WHERE account_id = CAST(? AS UUID) AND trading_symbol = ? AND product = ?", params![account, l.trading_symbol, l.product])?))?;
        }
        Ok(n)
    }
}

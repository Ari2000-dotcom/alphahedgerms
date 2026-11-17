//! Alphahedge RMS data layer, on DuckDB with Parquet for finished history.
//!
//! * One DuckDB file holds everything that changes and everything still active:
//!   clients, accounts, strategies, tags, and the live fills and orders.
//! * Finished history is exported to compressed Parquet files, which are never
//!   edited, so they make a tamper-evident archive and back up by copying.
//! * DuckDB has no triggers, so the record tables (fills, order events, position
//!   events, assignment events, the audit log) are append-only by construction:
//!   this crate offers only insert for them, and [`Db::history_fingerprint`] lets
//!   a nightly check prove that nothing changed behind its back.
//! * Only one program may write to the file. That program is `rms-server`; the
//!   admin screen and the later client portal talk to the server, never to the file.

use duckdb::{params, Connection, OptionalExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub mod fills;
pub mod model;

/// Schema versions, applied in order, each once. Compiled into the program.
const MIGRATIONS: &[(i32, &str, &str)] = &[(1, "init", include_str!("../sql/0001_init.sql"))];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] duckdb::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A handle to the RMS database. Cheap to clone; all clones share one connection.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Opens (or creates) the database file. The folder is created if missing.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Other(format!("cannot create {}: {e}", dir.display())))?;
        }
        Ok(Self { conn: Arc::new(Mutex::new(Connection::open(path)?)) })
    }

    /// A throwaway database in memory (tests).
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self { conn: Arc::new(Mutex::new(Connection::open_in_memory()?)) })
    }

    /// Runs `f` with the connection. Keep `f` short: all callers share it.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self.conn.lock().map_err(|_| Error::Other("database lock poisoned".into()))?;
        f(&guard)
    }

    /// Brings the schema up to the newest version. Safe to run on every start.
    pub fn migrate(&self) -> Result<()> {
        self.with(|c| {
            c.execute_batch("CREATE TABLE IF NOT EXISTS _migrations (version INTEGER PRIMARY KEY, name VARCHAR NOT NULL, applied_at TIMESTAMPTZ NOT NULL DEFAULT now())")?;
            for (version, name, sql) in MIGRATIONS {
                let done: Option<i32> = c.query_row("SELECT version FROM _migrations WHERE version = ?", params![version], |r| r.get(0)).optional()?;
                if done.is_some() {
                    continue;
                }
                // One transaction per migration: it is applied whole or not at all.
                c.execute_batch(&format!("BEGIN TRANSACTION;\n{sql}\nCOMMIT;"))
                    .map_err(|e| {
                        let _ = c.execute_batch("ROLLBACK");
                        Error::Other(format!("migration {version} ({name}) failed: {e}"))
                    })?;
                c.execute("INSERT INTO _migrations (version, name) VALUES (?, ?)", params![version, name])?;
            }
            Ok(())
        })
    }

    /// Which schema versions are applied, oldest first.
    pub fn applied_migrations(&self) -> Result<Vec<(i32, String)>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT version, name FROM _migrations ORDER BY version")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    /// A fingerprint of the whole append-only record. Store it each night (in the audit
    /// log); if a later check gives a different value for the same rows, something edited
    /// history. Over ids and the values that matter, in id order, so a changed price,
    /// quantity, side, time or a removed row all change it.
    pub fn history_fingerprint(&self) -> Result<String> {
        self.with(|c| {
            let fill: String = c.query_row(
                "SELECT coalesce(md5(string_agg(concat_ws('|', id, account_id, dedupe_key, side, quantity, price, exchange_ts, trading_symbol), ',' ORDER BY id)), 'empty') FROM fills",
                [],
                |r| r.get(0),
            )?;
            let ev: String = c.query_row(
                "SELECT coalesce(md5(string_agg(concat_ws('|', id, account_id, event_key, status, filled_quantity, average_price), ',' ORDER BY id)), 'empty') FROM order_events",
                [],
                |r| r.get(0),
            )?;
            let asg: String = c.query_row(
                "SELECT coalesce(md5(string_agg(concat_ws('|', id, subject_kind, subject_id, strategy_id, occurred_at), ',' ORDER BY id)), 'empty') FROM assignment_events",
                [],
                |r| r.get(0),
            )?;
            Ok(format!("{fill}.{ev}.{asg}"))
        })
    }

    /// Rows that point at something that does not exist. There are no foreign keys
    /// (DuckDB cannot update a referenced row cleanly), so this is the integrity check.
    pub fn orphans(&self) -> Result<Vec<(String, i64)>> {
        const CHECKS: &[(&str, &str)] = &[
            ("fills without an account", "SELECT count(*) FROM fills f WHERE NOT EXISTS (SELECT 1 FROM broker_accounts a WHERE a.id = f.account_id)"),
            ("orders without an account", "SELECT count(*) FROM orders o WHERE NOT EXISTS (SELECT 1 FROM broker_accounts a WHERE a.id = o.account_id)"),
            ("accounts without a client", "SELECT count(*) FROM broker_accounts a WHERE NOT EXISTS (SELECT 1 FROM clients c WHERE c.id = a.client_id)"),
            ("assignments to a missing strategy", "SELECT count(*) FROM current_assignments x WHERE NOT EXISTS (SELECT 1 FROM strategies s WHERE s.id = x.strategy_id)"),
            ("lots without a fill", "SELECT count(*) FROM lots l WHERE NOT EXISTS (SELECT 1 FROM fills f WHERE f.id = l.open_fill_id)"),
        ];
        self.with(|c| {
            let mut out = Vec::new();
            for (what, sql) in CHECKS {
                let n: i64 = c.query_row(sql, [], |r| r.get(0))?;
                if n > 0 {
                    out.push((what.to_string(), n));
                }
            }
            Ok(out)
        })
    }

    /// Writes one day's fills to `<dir>/fills/date=YYYY-MM-DD/fills.parquet`, ZSTD compressed,
    /// by writing a temporary file and renaming it, so a crash never leaves a half file. Returns
    /// the file and how many rows it holds, or `None` when there were no fills that day.
    pub fn export_fills_day(&self, dir: &Path, day: &str) -> Result<Option<(PathBuf, i64)>> {
        if day.len() != 10 || !day.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return Err(Error::Other("day must look like 2026-11-17".into()));
        }
        self.with(|c| {
            let n: i64 = c.query_row("SELECT count(*) FROM fills WHERE CAST(coalesce(exchange_ts, ingested_at) AS DATE) = CAST(? AS DATE)", params![day], |r| r.get(0))?;
            if n == 0 {
                return Ok(None);
            }
            let folder = dir.join("fills").join(format!("date={day}"));
            std::fs::create_dir_all(&folder).map_err(|e| Error::Other(format!("cannot create {}: {e}", folder.display())))?;
            let target = folder.join("fills.parquet");
            let tmp = folder.join("fills.parquet.tmp");
            // The path goes into the SQL text; forward slashes and doubled quotes keep it literal.
            let lit = |p: &Path| p.to_string_lossy().replace('\\', "/").replace('\'', "''");
            c.execute_batch(&format!(
                "COPY (SELECT * FROM fills WHERE CAST(coalesce(exchange_ts, ingested_at) AS DATE) = DATE '{day}' ORDER BY id) TO '{}' (FORMAT PARQUET, COMPRESSION ZSTD)",
                lit(&tmp)
            ))?;
            std::fs::rename(&tmp, &target).map_err(|e| Error::Other(format!("cannot move {} into place: {e}", tmp.display())))?;
            Ok(Some((target, n)))
        })
    }
}

//! Alphahedge RMS database layer: connecting, migrating, and the plain row types.
//!
//! The record of what happened (fills, order events, position events, strategy
//! assignments, the audit log) is append-only; the schema enforces that with
//! triggers, so no bug in the application can rewrite history.

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

pub mod fills;
pub mod model;

/// All migrations, compiled into the program.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Opens a small pool. The RMS is write-light and read-moderate, so a handful of connections is plenty.
pub async fn connect(url: &str) -> Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await?)
}

/// Brings the database up to the newest schema. Safe to run on every start.
pub async fn migrate(pool: &PgPool) -> Result<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

/// Which migrations are applied, newest last. For the health check and `rms-server status`.
pub async fn applied_migrations(pool: &PgPool) -> Result<Vec<(i64, String)>> {
    Ok(sqlx::query_as::<_, (i64, String)>("SELECT version, description FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await?)
}

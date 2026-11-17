//! rms-server: `rms-server migrate` applies the schema; `rms-server` (or `serve`) runs the server.
//!
//! Phase 1 starts small: open the database, migrate, and answer a health check
//! that says which schema is in place and whether the stored history is intact.
//! The collectors, reconciler, strategy engine and admin API are added step by
//! step (see docs/DESIGN.md, section 12).

use axum::{extract::State, routing::get, Json, Router};
use rms_db::Db;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::PathBuf;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    std::process::exit(2)
}

/// Reads KEY=VALUE lines from `.env` in the current folder into the environment, without overriding real variables.
fn load_dotenv() {
    let Ok(text) = std::fs::read_to_string(".env") else { return };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if std::env::var_os(k.trim()).is_none() {
                std::env::set_var(k.trim(), v.trim());
            }
        }
    }
}

async fn health(State(db): State<Db>) -> Json<Value> {
    // The database is blocking code, so it runs off the async threads.
    let out = tokio::task::spawn_blocking(move || -> rms_db::Result<Value> {
        let schema = db.applied_migrations()?.last().map(|(v, n)| format!("{v} {n}"));
        let orphans = db.orphans()?;
        Ok(json!({
            "ok": orphans.is_empty(),
            "schema": schema,
            "fills": db.fill_count()?,
            "history_fingerprint": db.history_fingerprint()?,
            "orphans": orphans,
        }))
    })
    .await;
    match out {
        Ok(Ok(v)) => Json(v),
        Ok(Err(e)) => Json(json!({"ok": false, "error": e.to_string()})),
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    load_dotenv();

    // Everything lives in one folder: the DuckDB file, and the Parquet archive beside it.
    let data_dir = PathBuf::from(env("RMS_DATA_DIR").unwrap_or_else(|| "data".into()));
    let file = data_dir.join("rms.duckdb");
    let db = Db::open(&file).unwrap_or_else(|e| die(format!("cannot open {}: {e}", file.display())));
    db.migrate().unwrap_or_else(|e| die(format!("cannot apply the schema: {e}")));

    if std::env::args().nth(1).as_deref() == Some("migrate") {
        let applied = db.applied_migrations().unwrap_or_default();
        println!("schema is up to date: {} migration(s) applied, database at {}", applied.len(), file.display());
        return;
    }

    let bind: SocketAddr = env("RMS_BIND")
        .unwrap_or_else(|| "127.0.0.1:8090".into())
        .parse()
        .unwrap_or_else(|_| die("RMS_BIND is not a valid address like 127.0.0.1:8090"));
    let app = Router::new().route("/health", get(health)).with_state(db);
    let listener = tokio::net::TcpListener::bind(bind).await.unwrap_or_else(|e| die(format!("cannot listen on {bind}: {e}")));
    tracing::info!("listening on http://{bind}, data in {}", data_dir.display());
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server error");
}

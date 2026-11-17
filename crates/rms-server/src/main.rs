//! rms-server: `rms-server migrate` applies the schema; `rms-server` runs the server.
//!
//! One process serves the admin API (under /api) and the admin screen (the built files in
//! `web/dist`). Phase 1 is read-only: it signs in to broker accounts and reads positions
//! and margin; it never places an order.

mod api;
mod brokers;
mod setup;

use axum::{extract::State, routing::get, Json, Router};
use kite_engine::crypto::Vault;
use kite_engine::login::BrokerUrls;
use rms_db::Db;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tower_http::services::{ServeDir, ServeFile};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    std::process::exit(2)
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

    let secrets = setup::ensure();
    if let Some(p) = &secrets.new_password {
        println!("\nFirst run: the admin password is\n\n    {p}\n\nIt is saved in .env (RMS_ADMIN_PASSWORD). Keep .env private: it also holds the key that seals broker passwords.\n");
    }

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
    let state = api::AppState {
        db: db.clone(),
        vault: Arc::new(Vault::new(&secrets.vault_key)),
        admin_password: Arc::new(secrets.admin_password),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        live: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        status: Arc::new(Mutex::new(HashMap::new())),
        urls: Arc::new(BrokerUrls::default()),
    };

    // The admin screen: built files from web/dist, with index.html for any other path.
    let dist = PathBuf::from(env("RMS_WEB_DIR").unwrap_or_else(|| "web/dist".into()));
    let screen = ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html")));

    let app = Router::new()
        .route("/health", get(health).with_state(db))
        .nest("/api", api::router(state))
        .fallback_service(screen);
    let listener = tokio::net::TcpListener::bind(bind).await.unwrap_or_else(|e| die(format!("cannot listen on {bind}: {e}")));
    tracing::info!("admin screen and API on http://{bind}, data in {}", data_dir.display());
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server error");
}

//! rms-server: `rms-server migrate` applies the schema; `rms-server` (or `serve`) runs the server.
//!
//! Phase 1 starts small: connect, migrate, and answer a health check that says
//! which schema is in place. The collectors, reconciler, strategy engine and
//! admin API are added step by step (see docs/DESIGN.md, section 12).

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::net::SocketAddr;

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

async fn health(State(pool): State<PgPool>) -> Json<Value> {
    let migrations = rms_db::applied_migrations(&pool).await;
    match migrations {
        Ok(m) => Json(json!({"ok": true, "schema": m.last().map(|(v, d)| format!("{v} {d}"))})),
        Err(e) => Json(json!({"ok": false, "error": e.to_string()})),
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    load_dotenv();

    let Some(url) = env("DATABASE_URL") else { die("DATABASE_URL is required (see .env.example)") };
    let pool = rms_db::connect(&url).await.unwrap_or_else(|e| die(format!("cannot connect to the database: {e}")));
    rms_db::migrate(&pool).await.unwrap_or_else(|e| die(format!("cannot apply the schema: {e}")));

    if std::env::args().nth(1).as_deref() == Some("migrate") {
        let applied = rms_db::applied_migrations(&pool).await.unwrap_or_default();
        println!("schema is up to date: {} migration(s) applied", applied.len());
        return;
    }

    let bind: SocketAddr = env("RMS_BIND")
        .unwrap_or_else(|| "127.0.0.1:8090".into())
        .parse()
        .unwrap_or_else(|_| die("RMS_BIND is not a valid address like 127.0.0.1:8090"));
    let app = Router::new().route("/health", get(health)).with_state(pool);
    let listener = tokio::net::TcpListener::bind(bind).await.unwrap_or_else(|e| die(format!("cannot listen on {bind}: {e}")));
    tracing::info!("listening on http://{bind}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server error");
}

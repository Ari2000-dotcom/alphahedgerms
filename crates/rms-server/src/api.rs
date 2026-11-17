//! The admin API. Everything under /api needs a signed-in admin, except login and health.

use crate::brokers::{self, StoredCreds};
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use kite_engine::crypto::Vault;
use kite_engine::login::{BrokerSession, BrokerUrls};
use rms_db::people::{NewAccount, NewClient};
use rms_db::{Db, Error as DbError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

const SESSION_HOURS: u64 = 12;
const COOKIE: &str = "rms_session";

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub vault: Arc<Vault>,
    pub admin_password: Arc<String>,
    pub sessions: Arc<Mutex<HashMap<String, Instant>>>,
    /// Signed-in broker sessions, in memory only: tokens expire daily, so they are never stored.
    pub live: Arc<tokio::sync::Mutex<HashMap<Uuid, BrokerSession>>>,
    pub status: Arc<Mutex<HashMap<Uuid, AccountStatus>>>,
    pub urls: Arc<BrokerUrls>,
}

/// What the last refresh of an account found.
#[derive(Clone, Debug, Serialize)]
pub struct AccountStatus {
    pub signed_in: bool,
    pub error: Option<String>,
    pub margin_error: Option<String>,
    pub at: String,
}

// ------------------------------------------------------------------ errors

pub struct ApiError(StatusCode, String);

impl ApiError {
    fn bad(msg: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.into())
    }
}

impl From<DbError> for ApiError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::Duplicate(m) => Self(StatusCode::CONFLICT, m),
            DbError::NotFound(m) => Self(StatusCode::NOT_FOUND, format!("not found: {m}")),
            DbError::Other(m) => Self(StatusCode::BAD_REQUEST, m),
            other => Self(StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

type ApiResult = Result<Json<Value>, ApiError>;

/// Runs blocking database work off the async threads.
async fn blk<T: Send + 'static>(db: &Db, f: impl FnOnce(&Db) -> rms_db::Result<T> + Send + 'static) -> Result<T, ApiError> {
    let db = db.clone();
    tokio::task::spawn_blocking(move || f(&db))
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(ApiError::from)
}

// -------------------------------------------------------------------- auth

fn cookie_token(h: &HeaderMap) -> Option<String> {
    let raw = h.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').filter_map(|p| p.trim().split_once('=')).find(|(k, _)| *k == COOKIE).map(|(_, v)| v.to_string())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn valid_session(st: &AppState, h: &HeaderMap) -> bool {
    let Some(t) = cookie_token(h) else { return false };
    let mut all = st.sessions.lock().unwrap();
    all.retain(|_, born| born.elapsed() < Duration::from_secs(SESSION_HOURS * 3600));
    all.contains_key(&t)
}

async fn require_admin(State(st): State<AppState>, req: Request, next: Next) -> Response {
    if !valid_session(&st, req.headers()) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "sign in first"}))).into_response();
    }
    next.run(req).await
}

#[derive(Deserialize)]
struct LoginBody {
    password: String,
}

async fn login(State(st): State<AppState>, Json(b): Json<LoginBody>) -> Response {
    if !constant_time_eq(b.password.as_bytes(), st.admin_password.as_bytes()) {
        // A wrong guess waits a second, which makes guessing pointless.
        tokio::time::sleep(Duration::from_secs(1)).await;
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "wrong password"}))).into_response();
    }
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    st.sessions.lock().unwrap().insert(token.clone(), Instant::now());
    let cookie = format!("{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}", SESSION_HOURS * 3600);
    ([(header::SET_COOKIE, cookie)], Json(json!({"ok": true}))).into_response()
}

async fn logout(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(t) = cookie_token(&headers) {
        st.sessions.lock().unwrap().remove(&t);
    }
    ([(header::SET_COOKIE, format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"))], Json(json!({"ok": true}))).into_response()
}

async fn me(State(st): State<AppState>, headers: HeaderMap) -> Json<Value> {
    Json(json!({"signed_in": valid_session(&st, &headers)}))
}

// ------------------------------------------------------------------ router

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/overview", get(overview))
        .route("/clients", post(add_client))
        .route("/clients/:id/accounts", post(add_account))
        .route("/accounts/:id", delete(delete_account))
        .route("/accounts/:id/enabled", post(set_enabled))
        .route("/accounts/:id/refresh", post(refresh_one))
        .route("/refresh", post(refresh_all))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_admin));
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .merge(protected)
        .with_state(state)
}

// --------------------------------------------------------------- refreshing

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn open_creds(st: &AppState, sealed: &[u8]) -> Result<StoredCreds, String> {
    let plain = st.vault.open(sealed).map_err(|e| e.to_string())?;
    serde_json::from_slice(&plain).map_err(|e| e.to_string())
}

/// Signs the account in if needed, reads its positions and margin, and stores both. Never
/// returns an error: what went wrong is recorded in the account's status for the screen.
pub async fn refresh_account(st: &AppState, id: Uuid) -> AccountStatus {
    let result = refresh_inner(st, id).await;
    let status = match result {
        Ok(s) => s,
        Err(e) => AccountStatus { signed_in: false, error: Some(e), margin_error: None, at: now() },
    };
    st.status.lock().unwrap().insert(id, status.clone());
    status
}

async fn refresh_inner(st: &AppState, id: Uuid) -> Result<AccountStatus, String> {
    let (broker, _client, sealed, _user) = blk(&st.db, move |db| db.account_credentials(id)).await.map_err(|e| e.1)?;
    let creds = open_creds(st, &sealed)?;

    // A held session is tried first; if the broker refuses it, sign in again once.
    let held = st.live.lock().await.get(&id).cloned();
    let mut fresh = false;
    let mut session = match held {
        Some(s) => s,
        None => {
            fresh = true;
            brokers::sign_in(&broker, &creds, &st.urls).await?
        }
    };
    let positions = match brokers::read_positions(&broker, &creds, &session, &st.urls).await {
        Ok(p) => p,
        Err(_) if !fresh => {
            session = brokers::sign_in(&broker, &creds, &st.urls).await?;
            fresh = true;
            brokers::read_positions(&broker, &creds, &session, &st.urls).await?
        }
        Err(e) => return Err(e),
    };
    let _ = fresh;
    st.live.lock().await.insert(id, session.clone());

    let margin = brokers::read_margin(&broker, &creds, &session, &st.urls).await;
    let margin_error = margin.as_ref().err().cloned();
    blk(&st.db, move |db| {
        db.save_positions(id, &positions)?;
        if let Ok(m) = &margin {
            db.save_margin(id, m)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.1)?;
    Ok(AccountStatus { signed_in: true, error: None, margin_error, at: now() })
}

async fn refresh_every_account(st: &AppState) {
    let accounts = match blk(&st.db, |db| db.list_accounts(None)).await {
        Ok(a) => a,
        Err(_) => return,
    };
    let jobs = accounts.iter().filter(|a| a.enabled).filter_map(|a| Uuid::parse_str(&a.id).ok()).map(|id| async move { refresh_account(st, id).await });
    futures_util::future::join_all(jobs).await;
}

// ---------------------------------------------------------------- handlers

async fn overview(State(st): State<AppState>) -> ApiResult {
    let (clients, accounts, positions, margins) = blk(&st.db, |db| Ok((db.list_clients()?, db.list_accounts(None)?, db.latest_positions(None)?, db.latest_margins()?))).await?;
    let status = st.status.lock().unwrap().clone();
    let margins: HashMap<String, _> = margins.into_iter().collect();
    let accounts: Vec<Value> = accounts
        .into_iter()
        .map(|a| {
            let s = Uuid::parse_str(&a.id).ok().and_then(|u| status.get(&u).cloned());
            let m = margins.get(&a.id);
            json!({"account": a, "status": s, "margin": m})
        })
        .collect();
    Ok(Json(json!({"clients": clients, "accounts": accounts, "positions": positions})))
}

#[derive(Deserialize)]
struct NewClientBody {
    client_code: String,
    name: String,
    email: Option<String>,
    phone: Option<String>,
    notes: Option<String>,
}

fn clean(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

async fn add_client(State(st): State<AppState>, Json(b): Json<NewClientBody>) -> ApiResult {
    let (code, name) = (b.client_code.trim().to_string(), b.name.trim().to_string());
    if code.is_empty() || name.is_empty() {
        return Err(ApiError::bad("a client needs a code and a name"));
    }
    let c = NewClient { client_code: code, name, email: clean(b.email), phone: clean(b.phone), notes: clean(b.notes) };
    let id = blk(&st.db, move |db| db.add_client(&c)).await?;
    Ok(Json(json!({"id": id})))
}

#[derive(Deserialize)]
struct NewAccountBody {
    broker: String,
    #[serde(default)]
    user_id: String,
    label: Option<String>,
    #[serde(default)]
    secrets: BTreeMap<String, String>,
    #[serde(default)]
    fields: BTreeMap<String, String>,
}

async fn add_account(State(st): State<AppState>, Path(client): Path<Uuid>, Json(b): Json<NewAccountBody>) -> ApiResult {
    if !brokers::BROKERS.contains(&b.broker.as_str()) {
        return Err(ApiError::bad(format!("unknown broker {}", b.broker)));
    }
    if !blk(&st.db, move |db| db.client_exists(client)).await? {
        return Err(ApiError(StatusCode::NOT_FOUND, "no such client".into()));
    }
    let creds = StoredCreds { user_id: b.user_id.trim().to_string(), secrets: b.secrets.into_iter().map(|(k, v)| (k, v.trim().to_string())).collect(), fields: b.fields.into_iter().map(|(k, v)| (k, v.trim().to_string())).collect() };
    let missing = brokers::missing(&b.broker, &creds);
    if !missing.is_empty() {
        return Err(ApiError::bad(format!("{} needs: {}", b.broker, missing.join(", "))));
    }
    let broker_user_id = brokers::broker_user_id(&creds);
    let label = clean(b.label).unwrap_or_else(|| format!("{} {}", b.broker, broker_user_id));
    let sealed = st.vault.seal(&serde_json::to_vec(&creds).map_err(|e| ApiError::bad(e.to_string()))?);
    let acct = NewAccount { client_id: client, broker: b.broker, broker_user_id, label, credentials: sealed };
    let id = blk(&st.db, move |db| db.add_account(&acct)).await?;
    // Saved either way; the first read tells the admin at once whether the details work.
    let status = refresh_account(&st, id).await;
    Ok(Json(json!({"id": id, "status": status})))
}

async fn delete_account(State(st): State<AppState>, Path(id): Path<Uuid>) -> ApiResult {
    blk(&st.db, move |db| db.delete_account(id)).await?;
    st.live.lock().await.remove(&id);
    st.status.lock().unwrap().remove(&id);
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct EnabledBody {
    enabled: bool,
}

async fn set_enabled(State(st): State<AppState>, Path(id): Path<Uuid>, Json(b): Json<EnabledBody>) -> ApiResult {
    blk(&st.db, move |db| db.set_account_enabled(id, b.enabled)).await?;
    Ok(Json(json!({"ok": true})))
}

async fn refresh_one(State(st): State<AppState>, Path(id): Path<Uuid>) -> ApiResult {
    Ok(Json(json!({"status": refresh_account(&st, id).await})))
}

async fn refresh_all(State(st): State<AppState>) -> ApiResult {
    refresh_every_account(&st).await;
    Ok(Json(json!({"ok": true})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_cookie_is_found_among_others() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, "a=1; rms_session=abc123; b=2".parse().unwrap());
        assert_eq!(cookie_token(&h).as_deref(), Some("abc123"));
        assert_eq!(cookie_token(&HeaderMap::new()), None);
        let mut other = HeaderMap::new();
        other.insert(header::COOKIE, "x_rms_session=1".parse().unwrap());
        assert_eq!(cookie_token(&other), None, "a cookie that only ends the same is not ours");
    }

    #[test]
    fn passwords_compare_without_stopping_at_the_first_difference() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secre"));
    }
}

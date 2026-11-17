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
use rms_db::strategies::Leg;
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
    pub admin_password: Arc<std::sync::RwLock<String>>,
    /// A password reset in progress: made by "forgot password", seen only by whoever can read the server's folder.
    pub reset: Arc<Mutex<Option<ResetCode>>>,
    pub data_dir: Arc<std::path::PathBuf>,
    /// Wrong sign-in passwords per address, for the lockout.
    pub guard: Arc<Mutex<LoginGuard>>,
    pub sessions: Arc<Mutex<HashMap<String, Instant>>>,
    /// Signed-in broker sessions, in memory only: tokens expire daily, so they are never stored.
    pub live: Arc<tokio::sync::Mutex<HashMap<Uuid, BrokerSession>>>,
    pub status: Arc<Mutex<HashMap<Uuid, AccountStatus>>>,
    pub urls: Arc<BrokerUrls>,
}

/// A one-time reset code. It lives in memory and in a file in the data folder, for ten minutes and five tries.
pub struct ResetCode {
    code: String,
    expires: Instant,
    asked: Instant,
    tries: u8,
}

const RESET_MINUTES: u64 = 10;
const RESET_TRIES: u8 = 5;

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

/// Wrong passwords per address: five wrong in ten minutes lock that address out for fifteen. Kept in memory.
#[derive(Default)]
pub struct LoginGuard {
    seen: HashMap<String, (u8, Instant, Option<Instant>)>,
}

const LOGIN_TRIES: u8 = 5;
const LOGIN_WINDOW: Duration = Duration::from_secs(10 * 60);
const LOGIN_LOCK: Duration = Duration::from_secs(15 * 60);

impl LoginGuard {
    /// How long this address must wait, if it is locked out.
    fn locked_for(&mut self, who: &str) -> Option<Duration> {
        self.seen.retain(|_, (_, first, lock)| lock.map_or(first.elapsed() < LOGIN_WINDOW, |l| l > Instant::now()));
        self.seen.get(who).and_then(|(_, _, lock)| lock.and_then(|l| l.checked_duration_since(Instant::now())))
    }
    fn failed(&mut self, who: &str) {
        let e = self.seen.entry(who.to_string()).or_insert((0, Instant::now(), None));
        if e.1.elapsed() >= LOGIN_WINDOW {
            *e = (0, Instant::now(), None);
        }
        e.0 += 1;
        if e.0 >= LOGIN_TRIES {
            e.2 = Some(Instant::now() + LOGIN_LOCK);
        }
    }
    fn succeeded(&mut self, who: &str) {
        self.seen.remove(who);
    }
}

/// Who is asking. Behind the website's proxy (Caddy) that is the address it passes along; with no proxy there is only "direct".
fn client_key(h: &HeaderMap) -> String {
    h.get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.split(',').next_back()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).unwrap_or_else(|| "direct".into())
}

/// The session cookie. Marked Secure when the request came in over HTTPS (the proxy says so), so a browser never sends it in clear.
fn session_cookie(token: &str, max_age: u64, h: &HeaderMap) -> String {
    let https = h.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).is_some_and(|v| v.eq_ignore_ascii_case("https"));
    format!("{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{}", if https { "; Secure" } else { "" })
}

async fn login(State(st): State<AppState>, headers: HeaderMap, Json(b): Json<LoginBody>) -> Response {
    let who = client_key(&headers);
    if let Some(wait) = st.guard.lock().unwrap().locked_for(&who) {
        let mins = wait.as_secs().div_ceil(60).max(1);
        return (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error": format!("too many wrong passwords from this address. Try again in {mins} minute{}", if mins == 1 { "" } else { "s" })}))).into_response();
    }
    let ok = constant_time_eq(b.password.as_bytes(), st.admin_password.read().unwrap().as_bytes());
    if !ok {
        st.guard.lock().unwrap().failed(&who);
        // A wrong guess waits a second, which makes guessing slow even before the lockout.
        tokio::time::sleep(Duration::from_secs(1)).await;
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "wrong password"}))).into_response();
    }
    st.guard.lock().unwrap().succeeded(&who);
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    st.sessions.lock().unwrap().insert(token.clone(), Instant::now());
    ([(header::SET_COOKIE, session_cookie(&token, SESSION_HOURS * 3600, &headers))], Json(json!({"ok": true}))).into_response()
}

async fn logout(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(t) = cookie_token(&headers) {
        st.sessions.lock().unwrap().remove(&t);
    }
    ([(header::SET_COOKIE, session_cookie("", 0, &headers))], Json(json!({"ok": true}))).into_response()
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
        .route("/accounts", post(create_account))
        .route("/accounts/:id", delete(delete_account).put(update_account))
        .route("/accounts/:id/enabled", post(set_enabled))
        .route("/accounts/:id/refresh", post(refresh_one))
        .route("/refresh", post(refresh_all))
        .route("/password", post(change_password))
        .route("/strategies", post(create_strategy))
        .route("/strategies/:id", delete(delete_strategy).put(rename_strategy))
        .route("/strategies/tag", post(tag_legs))
        .route("/strategies/untag", post(untag_legs))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_admin));
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/forgot", post(forgot))
        .route("/reset", post(reset_password))
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
    let (clients, accounts, positions, margins, strategies) = blk(&st.db, |db| Ok((db.list_clients()?, db.list_accounts(None)?, db.latest_positions(None)?, db.latest_margins()?, db.list_strategies()?))).await?;
    let blobs: HashMap<String, Vec<u8>> = blk(&st.db, |db| db.account_blobs()).await?.into_iter().collect();
    let status = st.status.lock().unwrap().clone();
    let margins: HashMap<String, _> = margins.into_iter().collect();
    let accounts: Vec<Value> = accounts
        .into_iter()
        .map(|a| {
            let s = Uuid::parse_str(&a.id).ok().and_then(|u| status.get(&u).cloned());
            let m = margins.get(&a.id);
            // Which secrets are saved and the plain fields (mobile, redirect URL). Never a secret's value.
            let (saved, fields) = blobs.get(&a.id).and_then(|b| open_creds(&st, b).ok()).map(|c| brokers::saved_view(&c)).unwrap_or_default();
            json!({"account": a, "status": s, "margin": m, "saved": saved, "fields": fields})
        })
        .collect();
    Ok(Json(json!({"clients": clients, "accounts": accounts, "positions": positions, "strategies": strategies})))
}

// ---------------------------------------------------------------- password

#[derive(Deserialize)]
struct PasswordBody {
    current: String,
    new: String,
}

fn check_new_password(new: &str, current: Option<&str>) -> Result<(), ApiError> {
    if new.chars().count() < 10 {
        return Err(ApiError::bad("the new password needs at least 10 characters"));
    }
    if new.chars().count() > 128 {
        return Err(ApiError::bad("the new password can be at most 128 characters"));
    }
    if new.trim() != new {
        return Err(ApiError::bad("the new password cannot start or end with a space"));
    }
    if current == Some(new) {
        return Err(ApiError::bad("the new password is the same as the current one"));
    }
    Ok(())
}

/// Changes the admin password: the current one must be right, the new one long enough and different.
/// It is saved in `.env`, takes effect at once, and every other signed-in session is ended.
async fn change_password(State(st): State<AppState>, headers: HeaderMap, Json(b): Json<PasswordBody>) -> ApiResult {
    if !constant_time_eq(b.current.as_bytes(), st.admin_password.read().unwrap().as_bytes()) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        return Err(ApiError(StatusCode::FORBIDDEN, "the current password is wrong".into()));
    }
    let new = b.new;
    check_new_password(&new, Some(&b.current))?;
    // Disk first: if the file cannot be written the old password stays, so a restart cannot undo the change.
    let to_save = new.clone();
    tokio::task::spawn_blocking(move || crate::setup::save_admin_password(&to_save))
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("could not save the new password to .env: {e}")))?;
    *st.admin_password.write().unwrap() = new;
    let mine = cookie_token(&headers);
    st.sessions.lock().unwrap().retain(|t, _| Some(t) == mine.as_ref());
    Ok(Json(json!({"ok": true})))
}

/// "Forgot password": makes a one-time code and puts it where only someone with access to the server will see
/// it, in the server's window and in `reset-code.txt` in its data folder. The code is never sent to the page.
async fn forgot(State(st): State<AppState>) -> ApiResult {
    let file = st.data_dir.join("reset-code.txt");
    let code = {
        let mut slot = st.reset.lock().unwrap();
        if let Some(r) = slot.as_ref() {
            if r.asked.elapsed() < Duration::from_secs(30) {
                return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "a reset code was just made. Wait half a minute before asking for another".into()));
            }
        }
        let code = format!("{:08}", Uuid::new_v4().as_u128() % 100_000_000);
        *slot = Some(ResetCode { code: code.clone(), expires: Instant::now() + Duration::from_secs(RESET_MINUTES * 60), asked: Instant::now(), tries: 0 });
        code
    };
    let text = format!("{code}\n\nThis is the Alphahedge RMS password reset code. It works for {RESET_MINUTES} minutes. Delete this file afterwards.\n");
    let path = file.clone();
    let wrote = tokio::task::spawn_blocking(move || std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new("."))).and_then(|_| std::fs::write(&path, text))).await;
    if !matches!(wrote, Ok(Ok(()))) {
        tracing::warn!("could not write {}", file.display());
    }
    // The full path, so the screen can say exactly which file to open (Windows adds a verbatim prefix; drop it).
    let shown = std::fs::canonicalize(&file).unwrap_or(file.clone()).to_string_lossy().trim_start_matches(r"\\?\").to_string();
    println!("\nPassword reset code: {code}   (valid {RESET_MINUTES} minutes, also saved in {shown})\n");
    Ok(Json(json!({"ok": true, "minutes": RESET_MINUTES, "file": shown})))
}

#[derive(Deserialize)]
struct ResetBody {
    code: String,
    new: String,
}

/// Sets a new password from a reset code. Five wrong codes cancel it; it ends every signed-in session and
/// signs this browser in, since the code already proved the person can reach the server.
async fn reset_password(State(st): State<AppState>, headers: HeaderMap, Json(b): Json<ResetBody>) -> Result<Response, ApiError> {
    let wrong = |msg: &str| ApiError(StatusCode::FORBIDDEN, msg.to_string());
    // Decide under the lock, answer after it is released: a lock must never be held across a wait.
    let verdict: Result<(), (&str, bool)> = {
        let mut slot = st.reset.lock().unwrap();
        match slot.as_mut() {
            None => Err(("ask for a reset code first", false)),
            Some(r) if r.expires < Instant::now() => {
                *slot = None;
                Err(("that reset code has expired. Ask for a new one", false))
            }
            Some(r) if !constant_time_eq(b.code.trim().as_bytes(), r.code.as_bytes()) => {
                r.tries += 1;
                if r.tries >= RESET_TRIES {
                    *slot = None;
                    Err(("too many wrong codes. That reset code is cancelled. Ask for a new one", true))
                } else {
                    Err(("that reset code is wrong", true))
                }
            }
            Some(_) => Ok(()),
        }
    };
    if let Err((msg, slow)) = verdict {
        if slow {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        return Err(wrong(msg));
    }
    check_new_password(&b.new, None)?;
    let to_save = b.new.clone();
    tokio::task::spawn_blocking(move || crate::setup::save_admin_password(&to_save))
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("could not save the new password to .env: {e}")))?;
    *st.admin_password.write().unwrap() = b.new;
    *st.reset.lock().unwrap() = None;
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    {
        let mut all = st.sessions.lock().unwrap();
        all.clear();
        all.insert(token.clone(), Instant::now());
    }
    let _ = std::fs::remove_file(st.data_dir.join("reset-code.txt"));
    Ok(([(header::SET_COOKIE, session_cookie(&token, SESSION_HOURS * 3600, &headers))], Json(json!({"ok": true}))).into_response())
}

// -------------------------------------------------------------- strategies

#[derive(Deserialize)]
struct NewStrategyBody {
    client_id: String,
    name: String,
}

async fn create_strategy(State(st): State<AppState>, Json(b): Json<NewStrategyBody>) -> ApiResult {
    let s = blk(&st.db, move |db| db.create_strategy(&b.client_id, &b.name)).await?;
    Ok(Json(json!(s)))
}

#[derive(Deserialize)]
struct RenameBody {
    name: String,
}

async fn rename_strategy(State(st): State<AppState>, Path(id): Path<String>, Json(b): Json<RenameBody>) -> ApiResult {
    blk(&st.db, move |db| db.rename_strategy(&id, &b.name)).await?;
    Ok(Json(json!({"ok": true})))
}

async fn delete_strategy(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult {
    blk(&st.db, move |db| db.delete_strategy(&id)).await?;
    Ok(Json(json!({"ok": true})))
}

/// Either an existing strategy (`strategy_id`) or a new one (`client_id` and `new_name`), then the legs to put in it.
#[derive(Deserialize)]
struct TagBody {
    strategy_id: Option<String>,
    client_id: Option<String>,
    new_name: Option<String>,
    legs: Vec<Leg>,
}

async fn tag_legs(State(st): State<AppState>, Json(b): Json<TagBody>) -> ApiResult {
    let out = blk(&st.db, move |db| {
        let strategy = match (b.strategy_id, b.client_id, b.new_name) {
            (Some(id), _, _) => db.strategy(&id)?,
            (None, Some(client), Some(name)) => db.create_strategy(&client, &name)?,
            _ => return Err(DbError::Other("choose a strategy, or name a new one".into())),
        };
        // A refused tag on a just-made strategy leaves an empty strategy behind; take it back.
        match db.tag_positions(&strategy.id, &b.legs) {
            Ok(n) => Ok((strategy, n)),
            Err(e) => {
                if strategy.legs == 0 {
                    let _ = db.delete_strategy(&strategy.id);
                }
                Err(e)
            }
        }
    })
    .await?;
    Ok(Json(json!({"strategy": out.0, "tagged": out.1})))
}

#[derive(Deserialize)]
struct UntagBody {
    legs: Vec<Leg>,
}

async fn untag_legs(State(st): State<AppState>, Json(b): Json<UntagBody>) -> ApiResult {
    let n = blk(&st.db, move |db| db.untag_positions(&b.legs)).await?;
    Ok(Json(json!({"untagged": n})))
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

#[derive(Deserialize)]
struct CreateBody {
    /// An existing client, or a name: the same name (any case) finds the same client.
    client_id: Option<Uuid>,
    client_name: Option<String>,
    client_code: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    broker: String,
    #[serde(default)]
    user_id: String,
    label: Option<String>,
    #[serde(default)]
    secrets: BTreeMap<String, String>,
    #[serde(default)]
    fields: BTreeMap<String, String>,
}

fn trimmed(m: BTreeMap<String, String>) -> BTreeMap<String, String> {
    m.into_iter().map(|(k, v)| (k, v.trim().to_string())).collect()
}

/// Adds a client and their broker account in one step, the way the copy-trading desk adds an
/// account: one row, Enter, signed in. Everything is checked before anything is created, so a
/// typo never leaves an empty client behind.
async fn create_account(State(st): State<AppState>, Json(b): Json<CreateBody>) -> ApiResult {
    if !brokers::BROKERS.contains(&b.broker.as_str()) {
        return Err(ApiError::bad(format!("unknown broker {}", b.broker)));
    }
    let creds = StoredCreds { user_id: b.user_id.trim().to_string(), secrets: trimmed(b.secrets), fields: trimmed(b.fields) };
    let missing = brokers::missing(&b.broker, &creds);
    if !missing.is_empty() {
        return Err(ApiError::bad(format!("{} needs: {}", b.broker, missing.join(", "))));
    }
    let broker_user_id = brokers::broker_user_id(&creds);
    let (broker, id_check) = (b.broker.clone(), broker_user_id.clone());
    if blk(&st.db, move |db| db.account_exists(&broker, &id_check)).await? {
        return Err(ApiError(StatusCode::CONFLICT, format!("a {} account {} is already added", b.broker, broker_user_id)));
    }

    // Which client: given, or found by name, or made now.
    let name = clean(b.client_name);
    let mut created = false;
    let client_id = match (b.client_id, name) {
        (Some(id), _) => {
            if !blk(&st.db, move |db| db.client_exists(id)).await? {
                return Err(ApiError(StatusCode::NOT_FOUND, "no such client".into()));
            }
            id
        }
        (None, None) => return Err(ApiError::bad("enter the client's name")),
        (None, Some(name)) => {
            let n = name.clone();
            match blk(&st.db, move |db| db.find_client_by_name(&n)).await? {
                Some(id) => id,
                None => {
                    let code = clean(b.client_code);
                    let (phone, email) = (clean(b.phone), clean(b.email));
                    created = true;
                    blk(&st.db, move |db| {
                        let code = match code {
                            Some(c) => c,
                            None => db.next_client_code()?,
                        };
                        db.add_client(&NewClient { client_code: code, name, email, phone, notes: None })
                    })
                    .await?
                }
            }
        }
    };

    let label = clean(b.label).unwrap_or_else(|| format!("{} {}", b.broker, broker_user_id));
    let sealed = st.vault.seal(&serde_json::to_vec(&creds).map_err(|e| ApiError::bad(e.to_string()))?);
    let acct = NewAccount { client_id, broker: b.broker, broker_user_id, label, credentials: sealed };
    let id = match blk(&st.db, move |db| db.add_account(&acct)).await {
        Ok(id) => id,
        Err(e) => {
            if created {
                let _ = blk(&st.db, move |db| db.delete_client_if_empty(client_id)).await;
            }
            return Err(e);
        }
    };
    // Saved either way; the first read tells the admin at once whether the details work.
    let status = refresh_account(&st, id).await;
    Ok(Json(json!({"id": id, "client_id": client_id, "created_client": created, "status": status})))
}

#[derive(Deserialize)]
struct UpdateBody {
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    secrets: BTreeMap<String, String>,
    #[serde(default)]
    fields: BTreeMap<String, String>,
}

/// Changes an account's saved details. Only what was typed replaces anything, so fixing one
/// wrong cell does not mean retyping the rest. Then it signs in again with the new details.
async fn update_account(State(st): State<AppState>, Path(id): Path<Uuid>, Json(b): Json<UpdateBody>) -> ApiResult {
    let (broker, _client, sealed, _user) = blk(&st.db, move |db| db.account_credentials(id)).await?;
    let mut creds = open_creds(&st, &sealed).map_err(ApiError::bad)?;
    brokers::merge_creds(&mut creds, &b.user_id, &b.secrets, &b.fields);
    let missing = brokers::missing(&broker, &creds);
    if !missing.is_empty() {
        return Err(ApiError::bad(format!("{broker} needs: {}", missing.join(", "))));
    }
    let broker_user_id = brokers::broker_user_id(&creds);
    let resealed = st.vault.seal(&serde_json::to_vec(&creds).map_err(|e| ApiError::bad(e.to_string()))?);
    blk(&st.db, move |db| db.update_account_credentials(id, &broker_user_id, &resealed)).await?;
    // New details mean a new sign-in: the held session belongs to the old ones.
    st.live.lock().await.remove(&id);
    let status = refresh_account(&st, id).await;
    Ok(Json(json!({"status": status})))
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
    fn five_wrong_passwords_lock_that_address_out_and_a_right_one_clears_the_count() {
        let mut g = LoginGuard::default();
        for _ in 0..4 {
            g.failed("1.2.3.4");
        }
        assert!(g.locked_for("1.2.3.4").is_none(), "four wrong is not yet a lockout");
        g.failed("1.2.3.4");
        assert!(g.locked_for("1.2.3.4").is_some(), "the fifth wrong one locks it");
        assert!(g.locked_for("5.6.7.8").is_none(), "another address is not affected");
        g.succeeded("1.2.3.4");
        assert!(g.locked_for("1.2.3.4").is_none());
    }

    #[test]
    fn the_cookie_is_secure_only_over_https_and_the_address_is_the_proxys() {
        let plain = HeaderMap::new();
        assert!(!session_cookie("t", 60, &plain).contains("Secure"));
        let mut https = HeaderMap::new();
        https.insert("x-forwarded-proto", "https".parse().unwrap());
        let c = session_cookie("t", 60, &https);
        assert!(c.contains("; Secure") && c.contains("HttpOnly") && c.contains("SameSite=Strict"));
        let mut via = HeaderMap::new();
        via.insert("x-forwarded-for", "9.9.9.9, 203.0.113.7".parse().unwrap());
        assert_eq!(client_key(&via), "203.0.113.7", "the last address is the one the proxy added; earlier ones can be forged");
        assert_eq!(client_key(&HeaderMap::new()), "direct");
    }

    #[test]
    fn passwords_compare_without_stopping_at_the_first_difference() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secre"));
    }
}

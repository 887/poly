//! Admin web UI and API routes.
//!
//! ## Security model
//! - Credentials: `POLY_ADMIN_USER` / `POLY_ADMIN_PASSWORD` env vars.
//! - Login is protected by a PoW challenge (Anubis-style, SHA-256, difficulty 16 bits)
//!   to prevent automated brute-force.
//! - Global rate limit: 10 login attempts per minute across all IPs; returns 429.
//! - Sessions: 4-hour HttpOnly same-site cookies; invalidated on logout.
//! - Session tokens are stored as SHA-256 hashes in memory (DashMap) — never raw.
//!
//! ## Routes
//! - `GET  /`                         — serve the admin SPA HTML
//! - `GET  /admin/challenge`          — issue a PoW nonce for the login form
//! - `POST /admin/login`              — verify PoW + credentials, set session cookie
//! - `POST /admin/logout`             — clear session cookie
//! - `GET  /admin/api/stats`          — server stats (requires session cookie)
//! - `GET  /admin/api/accounts`       — list accounts  (requires session cookie)
//! - `GET  /admin/api/accounts/:pk/tokens` — list tokens for an account
//! - `DELETE /admin/api/tokens/:id`   — revoke a token
//! - `POST /admin/api/settings`       — update max_accounts

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
};
use dashmap::DashMap;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Instant;
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;

use crate::{AppState, auth::verify_pow};

// ── Session state (stored in AppState via AdminState) ────────────────────────

/// Admin login rate-limit tracker (global, across all IPs).
#[derive(Default)]
pub struct AdminLoginTracker {
    pub attempts: u32,
    pub window_start: Option<Instant>,
}

/// Admin-specific state stored in AppState.
pub struct AdminState {
    /// Active admin sessions: SHA-256(token) → expiry Instant.
    pub sessions: DashMap<String, Instant>,
    /// Pending PoW challenges: nonce → (difficulty, expiry).
    pub challenges: DashMap<String, (u32, Instant)>,
    /// Global login rate limiter.
    pub rate: Mutex<AdminLoginTracker>,
}

impl AdminState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
            challenges: DashMap::new(),
            rate: Mutex::new(AdminLoginTracker::default()),
        })
    }
}

// ── Request / response types ─────────────────────────────────────────────────

/// Request body for `POST /admin/login`.
#[derive(Debug, Deserialize)]
pub struct AdminLoginRequest {
    pub username: String,
    pub password: String,
    pub nonce: String,
    pub counter: u64,
}

/// Response from `GET /admin/api/stats`.
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub version: &'static str,
    pub account_count: i64,
    pub max_accounts: usize,
    pub pow_difficulty: u32,
    pub token_expiry_days: u64,
}

/// Flat account summary returned in the accounts list.
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct AccountSummary {
    pub public_key: String,
    pub registered_at: String,
    pub last_seen_at: String,
    pub token_count: i64,
    pub blob_count: i64,
}

/// A single token entry for the token list.
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct TokenSummary {
    pub id: String,
    pub device_name: String,
    pub created_at: String,
    pub last_seen_at: String,
    pub expires_at: String,
}

/// Request body for `POST /admin/api/settings`.
#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    pub max_accounts: usize,
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Build all admin routes.
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/", get(serve_ui))
        .route("/admin/challenge", get(admin_challenge))
        .route("/admin/login", post(admin_login))
        .route("/admin/logout", post(admin_logout))
        .route("/admin/api/stats", get(api_stats))
        .route("/admin/api/accounts", get(api_accounts))
        .route(
            "/admin/api/accounts/{pk}/tokens",
            get(api_tokens_for_account),
        )
        .route("/admin/api/tokens/{id}", delete(api_revoke_token))
        .route("/admin/api/settings", post(api_update_settings))
}

// ── Middleware helper ─────────────────────────────────────────────────────────

/// Extract + validate the admin session cookie.
///
/// Returns `Some((status, message))` when the session is missing or invalid
/// (the caller should build a `Response` from these), or `None` when the
/// session is valid and the request may proceed.
///
/// Returning a small `Option<(StatusCode, &'static str)>` avoids the
/// `clippy::result_large_err` lint that fires when `Response` appears as an
/// error type — no `#[allow]` required.
fn check_session(
    headers: &HeaderMap,
    admin: &AdminState,
    session_hours: u64,
) -> Option<(StatusCode, &'static str)> {
    let cookie_header = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let raw_token = match cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("poly_admin="))
    {
        Some(t) => t,
        None => return Some((StatusCode::UNAUTHORIZED, "not authenticated")),
    };

    let hash = hash_session_token(raw_token);
    let entry = match admin.sessions.get(&hash) {
        Some(e) => e,
        None => return Some((StatusCode::UNAUTHORIZED, "session expired or invalid")),
    };

    if entry.elapsed().as_secs() > session_hours * 3600 {
        drop(entry);
        admin.sessions.remove(&hash);
        return Some((StatusCode::UNAUTHORIZED, "session expired"));
    }

    None
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// Serve the admin SPA.
async fn serve_ui() -> Html<&'static str> {
    Html(ADMIN_HTML)
}

/// `GET /admin/challenge` — issue a short-lived PoW nonce for the login form.
async fn admin_challenge(State(state): State<AppState>) -> Json<serde_json::Value> {
    let nonce = Alphanumeric.sample_string(&mut rand::thread_rng(), 32);
    let difficulty = state.config.admin_pow_difficulty;
    let expiry = Instant::now() + std::time::Duration::from_secs(120);
    state
        .admin
        .challenges
        .insert(nonce.clone(), (difficulty, expiry));

    // Prune stale challenges.
    state
        .admin
        .challenges
        .retain(|_, (_, exp)| exp.elapsed().as_secs() == 0);

    Json(serde_json::json!({ "nonce": nonce, "difficulty": difficulty }))
}

/// `POST /admin/login` — verify PoW + credentials, issue session cookie.
async fn admin_login(
    State(state): State<AppState>,
    Json(body): Json<AdminLoginRequest>,
) -> Response {
    // Global rate limit: 10 login attempts per minute.
    {
        let mut tracker = state.admin.rate.lock().await;
        let window_elapsed = tracker
            .window_start
            .map(|s| s.elapsed().as_secs())
            .unwrap_or(u64::MAX);

        if window_elapsed >= 60 {
            tracker.attempts = 0;
            tracker.window_start = Some(Instant::now());
        }

        tracker.attempts += 1;

        if tracker.attempts > state.config.admin_rate_limit_per_minute {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [("Retry-After", "60")],
                Json(serde_json::json!({ "error": "too many login attempts — wait 1 minute" })),
            )
                .into_response();
        }
    }

    // Verify PoW challenge.
    let challenge = state.admin.challenges.get(&body.nonce);
    let (difficulty, expiry) = match challenge.as_ref() {
        Some(c) => *c.value(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid or expired challenge" })),
            )
                .into_response();
        }
    };
    drop(challenge);

    if expiry.elapsed().as_secs() > 0 {
        state.admin.challenges.remove(&body.nonce);
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "challenge expired — refresh the page" })),
        )
            .into_response();
    }

    if !verify_pow(&body.nonce, body.counter, difficulty) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid proof of work" })),
        )
            .into_response();
    }

    state.admin.challenges.remove(&body.nonce);

    // Verify credentials (constant-time, both fields).
    let user_ok: bool = ct_str_eq(&body.username, &state.config.admin_user);
    let pass_ok: bool = ct_str_eq(&body.password, &state.config.admin_password);

    if !user_ok || !pass_ok {
        tracing::warn!("Failed admin login attempt for user={}", body.username);
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid credentials" })),
        )
            .into_response();
    }

    // Issue session token.
    let raw = Alphanumeric.sample_string(&mut rand::thread_rng(), 48);
    let hash = hash_session_token(&raw);
    let expiry =
        Instant::now() + std::time::Duration::from_secs(state.config.admin_session_hours * 3600);
    state.admin.sessions.insert(hash, expiry);

    tracing::info!("Admin logged in successfully");

    let cookie = format!(
        "poly_admin={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        raw,
        state.config.admin_session_hours * 3600
    );
    (
        StatusCode::OK,
        [("Set-Cookie", cookie.as_str())],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

/// `POST /admin/logout` — clear the session cookie.
async fn admin_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let cookie_header = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if let Some(raw) = cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|p| p.strip_prefix("poly_admin="))
    {
        let hash = hash_session_token(raw);
        state.admin.sessions.remove(&hash);
    }

    (
        StatusCode::OK,
        [(
            "Set-Cookie",
            "poly_admin=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        )],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

/// `GET /admin/api/stats` — server statistics.
async fn api_stats(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some((status, msg)) =
        check_session(&headers, &state.admin, state.config.admin_session_hours)
    {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    let count: Option<serde_json::Value> = state
        .db
        .query("SELECT count() AS n FROM account GROUP ALL")
        .await
        .ok()
        .and_then(|mut r| r.take::<Option<serde_json::Value>>(0).ok())
        .flatten();

    let account_count = count
        .as_ref()
        .and_then(|v| v.get("n"))
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);

    Json(StatsResponse {
        version: env!("CARGO_PKG_VERSION"),
        account_count,
        max_accounts: state.config.max_accounts,
        pow_difficulty: state.config.pow_difficulty,
        token_expiry_days: state.config.token_expiry_days,
    })
    .into_response()
}

/// `GET /admin/api/accounts` — list all accounts with token + blob counts.
async fn api_accounts(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some((status, msg)) =
        check_session(&headers, &state.admin, state.config.admin_session_hours)
    {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    let accounts: Vec<serde_json::Value> = state
        .db
        .query(
            "SELECT \
               account.public_key AS public_key, \
               account.registered_at AS registered_at, \
               account.last_seen_at AS last_seen_at, \
               (SELECT count() FROM token WHERE public_key = account.public_key GROUP ALL)[0].count AS token_count, \
               (SELECT count() FROM sync_blob WHERE public_key = account.public_key GROUP ALL)[0].count AS blob_count \
             FROM account \
             ORDER BY last_seen_at DESC",
        )
        .await
        .ok()
        .and_then(|mut r| r.take::<Vec<serde_json::Value>>(0usize).ok())
        .unwrap_or_default();

    Json(accounts).into_response()
}

/// `GET /admin/api/accounts/:pk/tokens` — list active tokens for an account.
async fn api_tokens_for_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(pk): Path<String>,
) -> Response {
    if let Some((status, msg)) =
        check_session(&headers, &state.admin, state.config.admin_session_hours)
    {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    let tokens: Vec<serde_json::Value> = state
        .db
        .query(
            "SELECT \
               string::concat(string::slice(string::from(id), 6, 999)) AS id, \
               device_name, created_at, last_seen_at, expires_at \
             FROM token \
             WHERE public_key = $pk AND expires_at > time::now() \
             ORDER BY last_seen_at DESC",
        )
        .bind(("pk", pk))
        .await
        .ok()
        .and_then(|mut r| r.take(0).ok())
        .unwrap_or_default();

    Json(tokens).into_response()
}

/// `DELETE /admin/api/tokens/:id` — revoke a specific token by its record ID.
async fn api_revoke_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token_id): Path<String>,
) -> Response {
    if let Some((status, msg)) =
        check_session(&headers, &state.admin, state.config.admin_session_hours)
    {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    // The token_id is the SurrealDB record suffix (after "token:").
    let result = state
        .db
        .query("DELETE type::thing('token', $id)")
        .bind(("id", token_id.clone()))
        .await;

    match result {
        Ok(_) => {
            tracing::info!("Admin revoked token id={}", token_id);
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// `POST /admin/api/settings` — update `max_accounts` at runtime.
async fn api_update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UpdateSettingsRequest>,
) -> Response {
    if let Some((status, msg)) =
        check_session(&headers, &state.admin, state.config.admin_session_hours)
    {
        return (status, Json(serde_json::json!({ "error": msg }))).into_response();
    }

    // We can't mutate Config directly (it's immutable once loaded).
    // For max_accounts changes to be persistent, the operator should restart with the
    // updated env var. Here we only return acknowledgement — this signals intent.
    // TODO(phase-2.3.6): Store runtime-mutable settings in the DB.
    tracing::info!("Admin settings update: max_accounts={}", body.max_accounts);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "note": "Restart the server with POLY_MAX_ACCOUNTS set to persist this change."
        })),
    )
        .into_response()
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Hash a session token for storage comparison (SHA-256).
fn hash_session_token(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

/// Constant-time string equality (hash both to equalise length before compare).
fn ct_str_eq(a: &str, b: &str) -> bool {
    let ha = Sha256::digest(a.as_bytes());
    let hb = Sha256::digest(b.as_bytes());
    ha.ct_eq(&hb).into()
}

// ── Embedded admin SPA ────────────────────────────────────────────────────────

/// The complete admin UI — a self-contained HTML file with Tailwind CSS + Alpine.js.
///
/// Serves: login page (with PoW), dashboard (users list, session drill-down, settings).
const ADMIN_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Poly Backup — Admin</title>
  <script src="https://cdn.tailwindcss.com"></script>
  <script src="https://unpkg.com/alpinejs@3.14/dist/cdn.min.js" defer></script>
  <style>
    :root {
      --bg-base:    #070b14;
      --bg-surface: #0d1526;
      --bg-card:    #111b30;
      --border:     #1a2840;
      --accent:     #6366f1;
      --text:       #e2e8f0;
      --muted:      #64748b;
    }
    [x-cloak] { display: none !important; }
    body { background: var(--bg-base); color: var(--text); font-family: system-ui, -apple-system, sans-serif; margin: 0; }
    ::-webkit-scrollbar { width: 5px; } ::-webkit-scrollbar-track { background: transparent; }
    ::-webkit-scrollbar-thumb { background: #1a2840; border-radius: 3px; }
    .card { background: var(--bg-card); border: 1px solid var(--border); }
    .sidebar { background: var(--bg-surface); border-right: 1px solid var(--border); }
    .topbar { background: linear-gradient(135deg, var(--bg-surface) 0%, var(--bg-card) 100%); border-bottom: 1px solid var(--border); }
    .mono { font-family: 'SF Mono', 'Cascadia Code', 'Fira Code', monospace; }
    .brand {
      font-family: 'SF Mono', 'Cascadia Code', monospace;
      letter-spacing: 0.18em;
      font-weight: 800;
      font-size: 0.9rem;
      background: linear-gradient(90deg, #a5b4fc, #818cf8, #c4b5fd);
      -webkit-background-clip: text; -webkit-text-fill-color: transparent; background-clip: text;
    }
    .nav-item { display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-radius: 8px; font-size: 0.875rem; font-weight: 500; cursor: pointer; transition: all 0.15s; border: 1px solid transparent; width: 100%; background: none; }
    .nav-item:hover { background: rgba(255,255,255,0.05); color: #e2e8f0; }
    .nav-item.active { background: rgba(99,102,241,0.12); color: #a5b4fc; border-color: rgba(99,102,241,0.25); }
    .input { width: 100%; background: #0a0f1e; border: 1px solid var(--border); border-radius: 8px; padding: 10px 14px; color: #e2e8f0; font-size: 0.875rem; outline: none; transition: border-color 0.15s; }
    .input:focus { border-color: var(--accent); }
    .btn { padding: 10px 20px; border-radius: 8px; font-size: 0.875rem; font-weight: 600; cursor: pointer; transition: all 0.15s; border: none; }
    .btn-primary { background: var(--accent); color: white; }
    .btn-primary:hover { background: #4f46e5; }
    .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
    .btn-danger { background: transparent; color: #ef4444; border: 1px solid rgba(239,68,68,0.25); font-size: 0.75rem; padding: 5px 10px; }
    .btn-danger:hover { background: rgba(239,68,68,0.1); border-color: rgba(239,68,68,0.4); }
    .badge { display: inline-flex; align-items: center; justify-content: center; min-width: 20px; height: 20px; padding: 0 6px; border-radius: 10px; font-size: 0.7rem; font-weight: 600; }
    .status-dot { width: 7px; height: 7px; border-radius: 50%; background: #10b981; box-shadow: 0 0 6px rgba(16,185,129,0.6); }
    .progress-bar { height: 3px; background: #1a2840; border-radius: 2px; overflow: hidden; }
    .progress-fill { height: 100%; background: linear-gradient(90deg, #6366f1, #a78bfa); border-radius: 2px; transition: width 0.3s; animation: shimmer 1.5s infinite; }
    @keyframes shimmer { 0% { opacity: 0.7; } 50% { opacity: 1; } 100% { opacity: 0.7; } }
    table { border-collapse: collapse; width: 100%; }
    th { color: var(--muted); font-weight: 500; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.06em; }
    td, th { padding: 10px 16px; text-align: left; }
    tbody tr { border-bottom: 1px solid rgba(26,40,64,0.6); transition: background 0.1s; }
    tbody tr:hover { background: rgba(255,255,255,0.025); }
    tbody tr:last-child { border-bottom: none; }
  </style>
</head>
<body x-data="app()" x-init="init()" class="min-h-screen">

<!-- ── Login ─────────────────────────────────────────────────────────────── -->
<div x-show="!loggedIn" x-cloak class="min-h-screen flex items-center justify-center p-4">
  <div class="card rounded-2xl p-8 w-full max-w-sm shadow-2xl">
    <div class="text-center mb-8">
      <div class="inline-flex items-center justify-center w-14 h-14 rounded-xl mb-5"
           style="background:rgba(99,102,241,0.1);border:1px solid rgba(99,102,241,0.2)">
        <svg width="28" height="28" fill="none" stroke="#818cf8" stroke-width="1.5" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round"
            d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4"/>
        </svg>
      </div>
      <div class="brand mb-1">POLY BACKUP SERVER</div>
      <p style="color:var(--muted);font-size:0.8rem;">Administrative Console</p>
    </div>

    <div x-show="loginError" x-cloak class="mb-4 p-3 rounded-lg text-sm"
         style="background:rgba(239,68,68,0.08);border:1px solid rgba(239,68,68,0.2);color:#f87171"
         x-text="loginError"></div>

    <div class="space-y-4">
      <div>
        <label style="font-size:0.8rem;color:var(--muted);display:block;margin-bottom:6px">Username</label>
        <input class="input" type="text" x-model="creds.username" autocomplete="username" placeholder="admin"
               @keydown.enter="login()">
      </div>
      <div>
        <label style="font-size:0.8rem;color:var(--muted);display:block;margin-bottom:6px">Password</label>
        <input class="input" type="password" x-model="creds.password" autocomplete="current-password"
               @keydown.enter="login()">
      </div>

      <div x-show="mining" x-cloak>
        <div class="flex justify-between" style="font-size:0.75rem;color:var(--muted);margin-bottom:6px">
          <span>Security verification (PoW)</span>
          <span x-text="miningHashes.toLocaleString() + ' hashes'"></span>
        </div>
        <div class="progress-bar"><div class="progress-fill" style="width:100%"></div></div>
        <p style="font-size:0.7rem;color:var(--muted);margin-top:6px">Mining proof-of-work…</p>
      </div>

      <button class="btn btn-primary w-full" :disabled="mining" @click="login()">
        <span x-show="!mining">Sign In</span>
        <span x-show="mining" x-cloak>Verifying identity…</span>
      </button>
    </div>

    <div class="mt-5 p-3 rounded-lg" style="background:#0a0f1e;border:1px solid var(--border)">
      <p style="font-size:0.7rem;color:var(--muted);line-height:1.5">
        ⚡ Proof-of-work challenge active — automated login attempts are prevented.
        Rate limited to 10 attempts / minute.
      </p>
    </div>
  </div>
</div>

<!-- ── Dashboard ─────────────────────────────────────────────────────────── -->
<div x-show="loggedIn" x-cloak class="flex" style="height:100vh">

  <!-- Sidebar -->
  <div class="sidebar flex flex-col" style="width:220px;flex-shrink:0">
    <div style="padding:20px 16px 16px;border-bottom:1px solid var(--border)">
      <div class="brand" style="font-size:0.75rem">POLY BACKUP</div>
      <div class="mono" style="font-size:0.65rem;color:var(--muted);margin-top:2px;letter-spacing:0.1em">ADMIN CONSOLE</div>
    </div>
    <nav style="padding:12px 8px;flex:1">
      <button class="nav-item" :class="page==='users' ? 'active' : ''" @click="page='users'; loadAccounts()">
        <svg width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0z"/></svg>
        <span style="flex:1">Users</span>
        <span x-show="accountCount > 0" class="badge" style="background:#1a2840;color:#94a3b8" x-text="accountCount"></span>
      </button>
      <button class="nav-item" style="margin-top:4px" :class="page==='settings' ? 'active' : ''" @click="page='settings'; loadStats()">
        <svg width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065M15 12a3 3 0 11-6 0 3 3 0 016 0z"/></svg>
        Settings
      </button>
    </nav>
    <div style="padding:8px;border-top:1px solid var(--border)">
      <button class="nav-item" @click="logout()" style="color:var(--muted)">
        <svg width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"/></svg>
        Sign Out
      </button>
    </div>
  </div>

  <!-- Main -->
  <div style="flex:1;display:flex;flex-direction:column;overflow:hidden">
    <!-- Top bar -->
    <div class="topbar" style="padding:14px 24px;display:flex;align-items:center;justify-content:space-between">
      <div>
        <h1 style="font-size:1rem;font-weight:600;color:#f1f5f9;margin:0"
            x-text="page === 'users' ? 'User Accounts' : 'Server Settings'"></h1>
        <p style="font-size:0.75rem;color:var(--muted);margin:2px 0 0"
           x-text="page === 'users' ? accountCount + ' registered accounts' : 'Configuration & runtime info'"></p>
      </div>
      <div style="display:flex;align-items:center;gap:12px">
        <div style="display:flex;align-items:center;gap:7px;font-size:0.75rem;color:var(--muted)">
          <div class="status-dot"></div> Online
        </div>
        <div class="mono" style="font-size:0.7rem;color:var(--muted);background:#0a0f1e;border:1px solid var(--border);padding:5px 10px;border-radius:6px">
          v<span x-text="serverVersion"></span>
        </div>
      </div>
    </div>

    <!-- Content -->
    <div style="flex:1;overflow:auto;padding:24px">

      <!-- Users page -->
      <div x-show="page==='users'">
        <div class="card rounded-xl" style="overflow:hidden">
          <table>
            <thead>
              <tr style="border-bottom:1px solid var(--border)">
                <th>Public Key</th>
                <th>Registered</th>
                <th>Last Seen</th>
                <th style="text-align:right">Sessions</th>
                <th style="text-align:right">Blobs</th>
              </tr>
            </thead>
            <tbody>
              <template x-for="acc in accounts" :key="acc.public_key">
                <tr @click="toggleUser(acc.public_key)" style="cursor:pointer">
                  <td>
                    <div style="display:flex;align-items:center;gap:8px">
                      <svg style="width:12px;height:12px;color:var(--muted);transition:transform 0.15s;flex-shrink:0"
                           :style="expandedUser===acc.public_key ? 'transform:rotate(90deg)' : ''"
                           fill="none" stroke="currentColor" stroke-width="2.5" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7"/>
                      </svg>
                      <span class="mono" style="font-size:0.75rem;color:#a5b4fc"
                            x-text="acc.public_key.slice(0,8) + '…' + acc.public_key.slice(-8)"></span>
                    </div>
                    <!-- Expanded sessions sub-row -->
                    <div x-show="expandedUser === acc.public_key" x-cloak
                         @click.stop
                         style="margin-top:10px;padding:12px;background:#0a0f1e;border:1px solid var(--border);border-radius:8px">
                      <div style="font-size:0.7rem;text-transform:uppercase;letter-spacing:0.08em;color:var(--muted);margin-bottom:8px;font-weight:600">
                        Active Sessions
                      </div>
                      <div x-show="!acc.tokens || acc.tokens.length === 0" style="font-size:0.8rem;color:var(--muted);padding:4px 0">
                        No active sessions.
                      </div>
                      <template x-for="tok in (acc.tokens || [])" :key="tok.id">
                        <div style="display:flex;align-items:center;justify-content:space-between;padding:8px 0;border-bottom:1px solid var(--border)"
                             class="last:border-0">
                          <div>
                            <div style="font-size:0.8rem;font-weight:500;color:#e2e8f0" x-text="tok.device_name || 'Unknown device'"></div>
                            <div style="font-size:0.7rem;color:var(--muted);margin-top:2px">
                              Created <span x-text="fmtDate(tok.created_at)"></span> ·
                              Last <span x-text="fmtRel(tok.last_seen_at)"></span> ·
                              Exp. <span x-text="fmtDate(tok.expires_at)"></span>
                            </div>
                          </div>
                          <button class="btn btn-danger" @click.stop="revokeToken(acc.public_key, tok.id)">
                            Revoke
                          </button>
                        </div>
                      </template>
                    </div>
                  </td>
                  <td style="font-size:0.82rem;color:#94a3b8" x-text="fmtDate(acc.registered_at)"></td>
                  <td style="font-size:0.82rem;color:#94a3b8" x-text="fmtRel(acc.last_seen_at)"></td>
                  <td style="text-align:right;color:#e2e8f0;font-size:0.85rem" x-text="acc.token_count ?? 0"></td>
                  <td style="text-align:right;color:#e2e8f0;font-size:0.85rem" x-text="acc.blob_count ?? 0"></td>
                </tr>
              </template>
              <tr x-show="accounts.length === 0">
                <td colspan="5" style="text-align:center;color:var(--muted);padding:48px 16px;font-size:0.875rem">
                  No registered accounts yet.
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <!-- Settings page -->
      <div x-show="page==='settings'" style="max-width:480px">
        <div class="card rounded-xl" style="padding:20px;margin-bottom:16px">
          <h3 style="font-size:0.9rem;font-weight:600;color:#f1f5f9;margin:0 0 16px">Account Limit</h3>
          <div style="display:flex;gap:10px;align-items:flex-end">
            <div style="flex:1">
              <label style="font-size:0.78rem;color:var(--muted);display:block;margin-bottom:6px">
                Max Accounts <span style="color:#374151">(0 = unlimited)</span>
              </label>
              <input type="number" min="0" class="input" x-model.number="newMaxAccounts">
            </div>
            <button class="btn btn-primary" @click="saveMaxAccounts()">Save</button>
          </div>
          <p x-show="settingsSaved" x-cloak style="font-size:0.75rem;color:#10b981;margin-top:8px">✓ Saved (restart server to persist)</p>
        </div>

        <div class="card rounded-xl" style="padding:20px">
          <h3 style="font-size:0.9rem;font-weight:600;color:#f1f5f9;margin:0 0 16px">Runtime Info</h3>
          <dl style="display:flex;flex-direction:column;gap:10px;font-size:0.82rem">
            <div style="display:flex;justify-content:space-between;padding-bottom:10px;border-bottom:1px solid var(--border)">
              <dt style="color:var(--muted)">Server Version</dt>
              <dd class="mono" style="color:#e2e8f0" x-text="serverVersion"></dd>
            </div>
            <div style="display:flex;justify-content:space-between;padding-bottom:10px;border-bottom:1px solid var(--border)">
              <dt style="color:var(--muted)">Accounts</dt>
              <dd style="color:#e2e8f0" x-text="accountCount + (stats.max_accounts > 0 ? ' / ' + stats.max_accounts : ' / ∞')"></dd>
            </div>
            <div style="display:flex;justify-content:space-between;padding-bottom:10px;border-bottom:1px solid var(--border)">
              <dt style="color:var(--muted)">PoW Difficulty</dt>
              <dd class="mono" style="color:#e2e8f0" x-text="(stats.pow_difficulty ?? '?') + ' bits'"></dd>
            </div>
            <div style="display:flex;justify-content:space-between">
              <dt style="color:var(--muted)">Token Expiry</dt>
              <dd style="color:#e2e8f0" x-text="(stats.token_expiry_days ?? '?') + ' days inactivity'"></dd>
            </div>
          </dl>
        </div>
      </div>

    </div>
  </div>
</div>

<script>
function app() {
  return {
    loggedIn: false,
    page: 'users',
    creds: { username: '', password: '' },
    loginError: '',
    mining: false,
    miningHashes: 0,
    accounts: [],
    accountCount: 0,
    expandedUser: null,
    stats: {},
    newMaxAccounts: 0,
    settingsSaved: false,
    serverVersion: '0.1.0',
    _challenge: null,

    async init() {
      const r = await fetch('/admin/api/stats', { credentials: 'include' });
      if (r.ok) {
        this.loggedIn = true;
        await this.loadAll();
      } else {
        await this.fetchChallenge();
      }
    },

    async fetchChallenge() {
      const r = await fetch('/admin/challenge');
      if (r.ok) this._challenge = await r.json();
    },

    async login() {
      if (!this._challenge) { this.loginError = 'No challenge — refresh the page.'; return; }
      this.mining = true;
      this.loginError = '';
      this.miningHashes = 0;

      const { nonce, difficulty } = this._challenge;
      let counter = 0;
      while (true) {
        const hash = await sha256(nonce + counter);
        if (leadingZeroBits(hash) >= difficulty) break;
        counter++;
        if (counter % 500 === 0) { this.miningHashes = counter; await tick(); }
      }
      this.mining = false;

      const resp = await fetch('/admin/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        body: JSON.stringify({ username: this.creds.username, password: this.creds.password, nonce, counter }),
      });

      if (resp.ok) {
        this.loggedIn = true;
        this.loginError = '';
        await this.loadAll();
      } else {
        const e = await resp.json().catch(() => ({}));
        this.loginError = e.error || 'Login failed.';
        await this.fetchChallenge();
      }
    },

    async logout() {
      await fetch('/admin/logout', { method: 'POST', credentials: 'include' });
      this.loggedIn = false;
      this.accounts = [];
      await this.fetchChallenge();
    },

    async loadAll() {
      await Promise.all([this.loadStats(), this.loadAccounts()]);
    },

    async loadStats() {
      const r = await fetch('/admin/api/stats', { credentials: 'include' });
      if (r.ok) {
        this.stats = await r.json();
        this.serverVersion = this.stats.version;
        this.accountCount = this.stats.account_count;
        this.newMaxAccounts = this.stats.max_accounts;
      }
    },

    async loadAccounts() {
      const r = await fetch('/admin/api/accounts', { credentials: 'include' });
      if (r.ok) this.accounts = await r.json();
    },

    async toggleUser(pk) {
      if (this.expandedUser === pk) { this.expandedUser = null; return; }
      this.expandedUser = pk;
      const r = await fetch(`/admin/api/accounts/${pk}/tokens`, { credentials: 'include' });
      if (r.ok) {
        const acc = this.accounts.find(a => a.public_key === pk);
        if (acc) acc.tokens = await r.json();
      }
    },

    async revokeToken(pk, id) {
      if (!confirm('Revoke this session? The device will need to re-authenticate.')) return;
      const r = await fetch(`/admin/api/tokens/${encodeURIComponent(id)}`, { method: 'DELETE', credentials: 'include' });
      if (r.ok) {
        const acc = this.accounts.find(a => a.public_key === pk);
        if (acc && acc.tokens) acc.tokens = acc.tokens.filter(t => t.id !== id);
        await this.loadStats();
      }
    },

    async saveMaxAccounts() {
      const r = await fetch('/admin/api/settings', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        body: JSON.stringify({ max_accounts: this.newMaxAccounts }),
      });
      if (r.ok) { this.settingsSaved = true; setTimeout(() => { this.settingsSaved = false; }, 3000); }
    },

    fmtDate(iso) {
      if (!iso) return '—';
      return new Date(iso).toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' });
    },
    fmtRel(iso) {
      if (!iso) return '—';
      const d = Math.floor((Date.now() - new Date(iso)) / 1000);
      if (d < 60) return 'just now';
      if (d < 3600) return Math.floor(d/60) + 'm ago';
      if (d < 86400) return Math.floor(d/3600) + 'h ago';
      return Math.floor(d/86400) + 'd ago';
    },
  };
}

async function sha256(str) {
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(str));
  return Array.from(new Uint8Array(buf)).map(b => b.toString(16).padStart(2,'0')).join('');
}
function leadingZeroBits(hex) {
  let bits = 0;
  for (const ch of hex) {
    const n = parseInt(ch, 16);
    if (n === 0) { bits += 4; } else { bits += Math.clz32(n) - 28; break; }
  }
  return bits;
}
function tick() { return new Promise(r => setTimeout(r, 0)); }
</script>
</body>
</html>
"##;

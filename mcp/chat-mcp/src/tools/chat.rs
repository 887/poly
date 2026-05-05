//! Chat backend tool handlers: login, logout, list_*, get_*, send_*, test_*.

use crate::state::{BackendEntry, BackendPool};
use serde_json::Value;

use super::{err_result, ok_result, parse_backend_type, str_arg, u64_arg};

use poly_client::{
    AuthCredentials, DmSupport, FriendModel, MessageContent, MessageQuery, MessagingModel,
    NotificationSupport, PluginManifest, VoiceSupport,
};

// ─── Backend lookup ───────────────────────────────────────────────────────────

/// Find the backend entry for a tool call (by type + optional account_id).
pub(super) fn find_backend<'a>(
    args: &Value,
    pool: &'a BackendPool,
) -> Result<&'a BackendEntry, Value> {
    let backend_str = str_arg(args, "backend").ok_or_else(|| err_result("missing 'backend'"))?;
    let bt = parse_backend_type(backend_str)
        .ok_or_else(|| err_result(format!("unknown backend: {backend_str}")))?;

    if let Some(account_id) = str_arg(args, "account_id") {
        pool.get(&bt, account_id)
            .ok_or_else(|| err_result(format!("no session for {backend_str}:{account_id}")))
    } else {
        pool.find_by_type(&bt)
            .ok_or_else(|| err_result(format!("no active {backend_str} session. Call 'login' first.")))
    }
}

// ─── Login / logout ───────────────────────────────────────────────────────────

pub(super) async fn handle_login(args: &Value, pool: &mut BackendPool) -> Value {
    let backend = match str_arg(args, "backend") {
        Some(b) => b,
        None => return err_result("missing 'backend' argument"),
    };
    let url = match str_arg(args, "url") {
        Some(u) => u,
        None => return err_result("missing 'url' argument"),
    };

    let credentials = if backend == "poly" {
        let is_signup = args.get("is_signup").and_then(serde_json::Value::as_bool).unwrap_or(false);
        let key: [u8; 32] = rand::random();
        AuthCredentials::PolyServer {
            server_url: url.to_string(),
            private_key_bytes: key.to_vec(),
            username: str_arg(args, "username").map(std::string::ToString::to_string),
            email: None,
            display_name: str_arg(args, "display_name").map(std::string::ToString::to_string),
            selected_user_id: str_arg(args, "user_id").map(std::string::ToString::to_string),
            is_signup,
        }
    } else {
        let username = str_arg(args, "username").unwrap_or("");
        let password = str_arg(args, "password").unwrap_or("");
        AuthCredentials::EmailPassword {
            email: username.to_string(),
            password: password.to_string(),
        }
    };

    match pool.login(backend, url, credentials).await {
        Ok(session) => {
            let result = serde_json::to_string_pretty(&session).unwrap_or_default();
            ok_result(format!("Logged in successfully.\n{result}"))
        }
        Err(e) => err_result(format!("Login failed: {e}")),
    }
}

pub(super) fn handle_logout(args: &Value, pool: &mut BackendPool) -> Value {
    let backend_str = match str_arg(args, "backend") {
        Some(b) => b,
        None => return err_result("missing 'backend'"),
    };
    let account_id = match str_arg(args, "account_id") {
        Some(a) => a,
        None => return err_result("missing 'account_id'"),
    };
    let bt = match parse_backend_type(backend_str) {
        Some(b) => b,
        None => return err_result(format!("unknown backend: {backend_str}")),
    };
    match pool.remove(&bt, account_id) {
        Some(_) => ok_result(format!("Disconnected {backend_str}:{account_id}")),
        None => err_result(format!("No active session for {backend_str}:{account_id}")),
    }
}

// ─── Read tools ───────────────────────────────────────────────────────────────

pub(super) async fn handle_list_servers(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_servers().await {
        Ok(servers) => ok_result(serde_json::to_string_pretty(&servers).unwrap_or_default()),
        Err(e) => err_result(format!("get_servers failed: {e}")),
    }
}

pub(super) async fn handle_list_channels(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    let server_id = match str_arg(args, "server_id") {
        Some(s) => s,
        None => return err_result("missing 'server_id'"),
    };
    match entry.backend.get_channels(server_id).await {
        Ok(channels) => ok_result(serde_json::to_string_pretty(&channels).unwrap_or_default()),
        Err(e) => err_result(format!("get_channels failed: {e}")),
    }
}

pub(super) async fn handle_get_messages(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let limit = u32::try_from(u64_arg(args, "limit").unwrap_or(50)).unwrap_or(u32::MAX);
    match entry
        .backend
        .get_messages(
            channel_id,
            MessageQuery {
                limit: Some(limit),
                ..Default::default()
            },
        )
        .await
    {
        Ok(messages) => ok_result(serde_json::to_string_pretty(&messages).unwrap_or_default()),
        Err(e) => err_result(format!("get_messages failed: {e}")),
    }
}

pub(super) async fn handle_list_dms(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_dm_channels().await {
        Ok(dms) => ok_result(serde_json::to_string_pretty(&dms).unwrap_or_default()),
        Err(e) => err_result(format!("get_dm_channels failed: {e}")),
    }
}

pub(super) async fn handle_list_friends(args: &Value, pool: &BackendPool) -> Value {
    // Capability guard: backends with no friends concept (HN, Lemmy, GitHub)
    // return an explicit NotSupported error instead of `Ok([])`, which would
    // silently mislead the caller into thinking the user has zero friends.
    if let Some(slug) = str_arg(args, "backend") {
        let caps = poly_client::capabilities_for_slug_static(slug);
        if matches!(caps.friends, FriendModel::None) {
            return err_result(format!(
                "list_friends not supported: backend '{slug}' has no friends concept"
            ));
        }
    }
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_friends().await {
        Ok(friends) => ok_result(serde_json::to_string_pretty(&friends).unwrap_or_default()),
        Err(e) => err_result(format!("get_friends failed: {e}")),
    }
}

pub(super) async fn handle_get_user(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    let user_id = match str_arg(args, "user_id") {
        Some(u) => u,
        None => return err_result("missing 'user_id'"),
    };
    match entry.backend.get_user(user_id).await {
        Ok(user) => ok_result(serde_json::to_string_pretty(&user).unwrap_or_default()),
        Err(e) => err_result(format!("get_user failed: {e}")),
    }
}

// ─── Write tools ──────────────────────────────────────────────────────────────

pub(super) async fn handle_send_message(args: &Value, pool: &BackendPool) -> Value {
    // Capability guard: backends without Full messaging (HN, GitHub) return
    // NotSupported up-front. Prevents the MCP from plumbing writes into a
    // backend whose API physically cannot accept them.
    if let Some(slug) = str_arg(args, "backend") {
        let caps = poly_client::capabilities_for_slug_static(slug);
        if !matches!(caps.messaging, MessagingModel::Full) {
            return err_result(format!(
                "send_message not supported: backend '{slug}' is read-only"
            ));
        }
    }
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let text = match str_arg(args, "text") {
        Some(t) => t,
        None => return err_result("missing 'text'"),
    };
    match entry
        .backend
        .send_message(channel_id, MessageContent::Text(text.to_string()))
        .await
    {
        Ok(msg) => ok_result(serde_json::to_string_pretty(&msg).unwrap_or_default()),
        Err(e) => err_result(format!("send_message failed: {e}")),
    }
}

pub(super) async fn handle_send_typing(args: &Value, pool: &BackendPool) -> Value {
    // Capability guard: only expose to backends that advertise typing indicators.
    if let Some(slug) = str_arg(args, "backend") {
        let caps = poly_client::capabilities_for_slug_static(slug);
        if !caps.supports_typing_indicators {
            return err_result(format!(
                "send_typing not supported: backend '{slug}' does not advertise typing indicators"
            ));
        }
    }
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    match entry.backend.send_typing(channel_id).await {
        Ok(()) => ok_result("typing indicator sent"),
        Err(e) => err_result(format!("send_typing failed: {e}")),
    }
}

// ─── Plugin tools ─────────────────────────────────────────────────────────────

/// Compute the subset of MCP tool names that are honest for a backend slug.
///
/// Read-only backends drop `send_message`. Backends with no DMs drop
/// `list_dms`. Backends with no friends drop `list_friends`. Backends with
/// no notifications drop `list_notifications`. The client uses this to
/// pick the narrowest sensible tool surface for an account.
pub(super) fn handle_list_plugin_tools(args: &Value) -> Value {
    let Some(slug) = str_arg(args, "backend") else {
        return err_result("missing 'backend'");
    };
    let caps = poly_client::capabilities_for_slug_static(slug);
    let mut tools: Vec<&'static str> = vec![
        "list_plugins",
        "list_accounts",
        "list_servers",
        "list_channels",
        "get_messages",
        "get_user",
    ];
    if matches!(caps.messaging, MessagingModel::Full) {
        tools.push("send_message");
    }
    if caps.supports_typing_indicators {
        tools.push("send_typing");
    }
    if !matches!(caps.dms, DmSupport::None) {
        tools.push("list_dms");
    }
    if !matches!(caps.friends, FriendModel::None) {
        tools.push("list_friends");
    }
    if !matches!(caps.notifications, NotificationSupport::None) {
        tools.push("list_notifications");
    }
    if matches!(caps.voice, VoiceSupport::Full) {
        tools.push("list_voice_participants");
    }
    ok_result(serde_json::to_string_pretty(&tools).unwrap_or_default())
}

// ─── List compiled-in plugins ─────────────────────────────────────────────────

/// Snapshot one plugin's identity + declared manifest.
// poly-lint: manifest fields are moved into json! by value.
#[allow(clippy::needless_pass_by_value)]
fn plugin_entry(id: &str, name: &str, manifest: PluginManifest) -> Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "description": manifest.description,
        "exec_programs": manifest.exec_programs,
        "http_hosts": manifest.http_hosts,
        "homepage": manifest.homepage,
    })
}

/// List every chat backend compiled into this MCP, with its declared manifest.
///
/// Each plugin is instantiated unauthenticated just long enough to read its
/// `plugin_manifest()` and `backend_name()`. The instances are dropped before
/// the function returns — no network or filesystem access happens.
pub(super) fn handle_list_plugins() -> Value {
    use poly_client::ClientBackend;
    let plugins: Vec<Value> = vec![
        {
            let c = poly_stoat::StoatClient::with_base_url("http://localhost").ok();
            match c {
                Some(c) => plugin_entry("stoat", c.backend_name(), c.plugin_manifest()),
                None => serde_json::json!({ "id": "stoat", "error": "failed to construct" }),
            }
        },
        {
            let c = poly_matrix::MatrixClient::with_homeserver("http://localhost").ok();
            match c {
                Some(c) => plugin_entry("matrix", c.backend_name(), c.plugin_manifest()),
                None => serde_json::json!({ "id": "matrix", "error": "failed to construct" }),
            }
        },
        {
            let c = poly_discord::DiscordClient::new();
            plugin_entry("discord", c.backend_name(), c.plugin_manifest())
        },
        {
            let c = poly_teams::TeamsClient::new();
            plugin_entry("teams", c.backend_name(), c.plugin_manifest())
        },
        {
            let c = poly_lemmy::LemmyClient::new("https://lemmy.world");
            plugin_entry("lemmy", c.backend_name(), c.plugin_manifest())
        },
        {
            let c = poly_hackernews::HackerNewsClient::new();
            plugin_entry("hackernews", c.backend_name(), c.plugin_manifest())
        },
        {
            let c = poly_github::GitHubClient::dotcom();
            plugin_entry("github", c.backend_name(), c.plugin_manifest())
        },
        {
            let key: [u8; 32] = [0; 32];
            let c = poly_server_client::PolyServerBackend::new("http://localhost", key);
            plugin_entry("poly", c.backend_name(), c.plugin_manifest())
        },
    ];
    ok_result(serde_json::to_string_pretty(&plugins).unwrap_or_default())
}

// ─── Test server easy-signin ──────────────────────────────────────────────────

/// Sign in to a localhost test server without a password.
/// Calls `POST /test/auth/token`, then logs in with the returned token.
pub(super) async fn handle_test_signin(args: &Value, pool: &mut BackendPool) -> Value {
    let backend = match str_arg(args, "backend") {
        Some(b) => b,
        None => return err_result("missing 'backend'"),
    };
    let url = match str_arg(args, "url") {
        Some(u) => u,
        None => return err_result("missing 'url'"),
    };
    let username = match str_arg(args, "username") {
        Some(u) => u,
        None => return err_result("missing 'username'"),
    };

    // Safety guard: only allow localhost/loopback URLs.
    if !url.contains("localhost") && !url.contains("127.0.0.1") {
        return err_result("test_signin is only allowed on localhost/127.0.0.1 URLs");
    }

    // Call /test/auth/token on the test server.
    let token_url = format!("{}/test/auth/token", url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .post(&token_url)
        .json(&serde_json::json!({ "username": username }))
        .send()
        .await;

    let token = match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = match r.json().await {
                Ok(v) => v,
                Err(e) => return err_result(format!("failed to parse token response: {e}")),
            };
            // Accept "token" (discord/stoat), "jwt" (lemmy), or "access_token" (matrix).
            let token_val = body.get("token")
                .or_else(|| body.get("jwt"))
                .or_else(|| body.get("access_token"))
                .and_then(|t| t.as_str())
                .map(std::string::ToString::to_string);
            match token_val {
                Some(t) => t,
                None => return err_result("test server did not return a token or jwt"),
            }
        }
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            return err_result(format!("test server returned {status}: {body}"));
        }
        Err(e) => return err_result(format!("failed to reach test server: {e}")),
    };

    // Log in using the token (skips password verification).
    match pool.login(backend, url, poly_client::AuthCredentials::Token(token)).await {
        Ok(session) => {
            let result = serde_json::to_string_pretty(&session).unwrap_or_default();
            ok_result(format!("Signed in as {username} (no password).\n{result}"))
        }
        Err(e) => err_result(format!("Login with token failed: {e}")),
    }
}

// ─── Test server lifecycle ────────────────────────────────────────────────────

pub(super) const TEST_PORTS: &[(&str, u16)] = &[
    ("matrix", 9100),
    ("stoat", 9101),
    ("discord", 9102),
    ("teams", 9103),
    ("poly", 9104),
    ("lemmy", 8536),
    ("hackernews", 8537),
];

pub(super) async fn handle_test_lifecycle(args: &Value, endpoint: &str) -> Value {
    let client = reqwest::Client::new();
    let backend = str_arg(args, "backend");

    let targets: Vec<(&str, u16)> = if let Some(b) = backend {
        if b == "all" {
            TEST_PORTS.to_vec()
        } else {
            match TEST_PORTS.iter().find(|(name, _)| *name == b) {
                Some(entry) => vec![*entry],
                None => return err_result(format!("unknown test backend: {b}")),
            }
        }
    } else {
        TEST_PORTS.to_vec()
    };

    let mut results = Vec::new();
    for (name, port) in &targets {
        let url = if endpoint == "health" {
            format!("http://localhost:{port}/health")
        } else {
            format!("http://localhost:{port}/{endpoint}")
        };

        let resp = if endpoint == "health" {
            client.get(&url).send().await
        } else {
            client.post(&url).send().await
        };

        let result = match resp {
            Ok(r) => {
                let status = r.status().as_u16();
                let body = r.text().await.unwrap_or_default();
                serde_json::json!({ "backend": name, "status": status, "response": body })
            }
            Err(e) => serde_json::json!({ "backend": name, "error": e.to_string() }),
        };
        results.push(result);
    }

    ok_result(serde_json::to_string_pretty(&results).unwrap_or_default())
}

//! MCP tool definitions and dispatch.

use serde_json::{Value, json};

use crate::state::BackendPool;
use poly_client::{
    AuthCredentials, BackendCapabilities, BackendType, ClientBackend, Cursor, CursorKind,
    DmSupport, FriendModel, MenuTargetKind, MessageContent, MessageQuery, MessagingModel,
    NotificationSupport, PluginManifest, SettingsScope, VoiceSupport,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn ok_result(text: impl ToString) -> Value {
    json!({ "content": [{"type": "text", "text": text.to_string()}], "isError": false })
}

fn err_result(text: impl ToString) -> Value {
    json!({ "content": [{"type": "text", "text": text.to_string()}], "isError": true })
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

fn u64_arg(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(|v| v.as_u64())
}

fn parse_menu_target(s: &str) -> Option<MenuTargetKind> {
    match s {
        "category" => Some(MenuTargetKind::Category),
        "channel" => Some(MenuTargetKind::Channel),
        "dm" => Some(MenuTargetKind::Dm),
        "message" => Some(MenuTargetKind::Message),
        "server" => Some(MenuTargetKind::Server),
        "user" => Some(MenuTargetKind::User),
        _ => None,
    }
}

fn parse_settings_scope(s: &str) -> Option<SettingsScope> {
    match s {
        "account-global" | "account_global" => Some(SettingsScope::AccountGlobal),
        "per-server" | "per_server" => Some(SettingsScope::PerServer),
        "per-channel" | "per_channel" => Some(SettingsScope::PerChannel),
        "per-user" | "per_user" => Some(SettingsScope::PerUser),
        _ => None,
    }
}

fn parse_cursor_kind(s: &str) -> Option<CursorKind> {
    match s {
        "offset" => Some(CursorKind::Offset),
        "timestamp" => Some(CursorKind::Timestamp),
        "id" => Some(CursorKind::Id),
        "opaque" => Some(CursorKind::Opaque),
        _ => None,
    }
}

fn parse_backend_type(s: &str) -> Option<BackendType> {
    match s {
        "stoat" => Some(BackendType::from("stoat")),
        "matrix" => Some(BackendType::from("matrix")),
        "discord" => Some(BackendType::from("discord")),
        "teams" => Some(BackendType::from("teams")),
        "poly" => Some(BackendType::from("poly")),
        "lemmy" => Some(BackendType::from("lemmy")),
        "hackernews" | "hn" => Some(BackendType::from("hackernews")),
        _ => None,
    }
}

// ─── Capability-driven tool filtering (polish plan P51) ─────────────────────

/// Return `true` if a tool name is meaningful for the given backend's declared
/// capabilities.
///
/// * Legacy Discord-shaped tools (`list_friends`, `list_dms`, `send_message`,
///   …) are gated on the relevant capability field — e.g. `list_friends`
///   disappears for Hacker News / GitHub because they have no friend concept.
/// * The new client-ui surface tools (`context_menu_*`, `invoke_context_action`,
///   `plugin_settings_*`, `sidebar_*`, `channel_view`, `view_rows`,
///   `composer_buttons`, `message_actions`, `invoke_*_action`) are always
///   exposed: per D9 of `plan-client-ui-surface.md` the plugin returns an
///   empty declaration list when a surface is unsupported, so the tool is
///   always safe to call.
/// * Account-management and test-harness tools (`login`, `logout`,
///   `list_accounts`, `list_plugins`, `list_plugin_tools`, `test_*`) are
///   backend-agnostic and always exposed.
#[must_use]
pub fn should_expose_tool(tool_name: &str, caps: &BackendCapabilities) -> bool {
    match tool_name {
        // Account management and meta — always advertised.
        "login" | "logout" | "list_accounts" | "list_plugins" | "list_plugin_tools"
        | "test_signin" | "test_health" | "test_reseed" => true,

        // Legacy Discord-shaped read tools gated on capability.
        "list_servers" | "list_channels" | "get_messages" | "get_user" => true,
        "list_friends" => !matches!(caps.friends, FriendModel::None),
        "list_dms" => !matches!(caps.dms, DmSupport::None),

        // Legacy write tool gated on messaging model.
        "send_message" => matches!(caps.messaging, MessagingModel::Full),

        // Typing indicator — gated on backend capability.
        "send_typing" => caps.supports_typing_indicators,

        // New client-ui surface — always exposed; plugins return empty
        // lists for unsupported surfaces per D9.
        "context_menu_server"
        | "context_menu_channel"
        | "context_menu_user"
        | "context_menu_message"
        | "context_menu_dm"
        | "context_menu_category"
        | "invoke_context_action"
        | "plugin_settings_sections"
        | "plugin_setting_get"
        | "plugin_setting_set"
        | "sidebar_declaration"
        | "invoke_sidebar_action"
        | "channel_view"
        | "view_rows"
        | "composer_buttons"
        | "message_actions"
        | "invoke_composer_action"
        | "invoke_message_action" => true,

        // Unknown tool names are excluded by default — this prevents a
        // future-added handler from being silently exposed before it has
        // a capability entry here.
        _ => false,
    }
}

/// Return the subset of [`tool_list`] appropriate for a backend slug.
/// Used by MCP clients that want the narrowest honest tool surface per
/// account. `tool_list()` itself stays unfiltered so generic `tools/list`
/// RPCs keep advertising every callable tool.
#[must_use]
pub fn tool_list_for_backend(slug: &str) -> Vec<Value> {
    let caps = poly_client::capabilities_for_slug(slug);
    tool_list()
        .into_iter()
        .filter(|t| {
            t.get("name")
                .and_then(|n| n.as_str())
                .map(|n| should_expose_tool(n, &caps))
                .unwrap_or(false)
        })
        .collect()
}

// ─── Tool list ───────────────────────────────────────────────────────────────

pub fn tool_list() -> Vec<Value> {
    vec![
        // Account management
        json!({
            "name": "login",
            "description": "Authenticate with a chat backend. Supported backends: stoat, matrix, poly. \
                            For stoat/matrix: provide username + password. \
                            For poly: provide username for signup (is_signup=true) or user_id for signin.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type: stoat, matrix, discord, teams, poly" },
                    "url": { "type": "string", "description": "Server URL (e.g. http://localhost:9101)" },
                    "username": { "type": "string", "description": "Username or email" },
                    "password": { "type": "string", "description": "Password (for stoat/matrix)" },
                    "user_id": { "type": "string", "description": "User ID to select (for poly signin)" },
                    "is_signup": { "type": "boolean", "description": "true = create new account (poly only)" },
                    "display_name": { "type": "string", "description": "Display name (for poly signup)" }
                },
                "required": ["backend", "url"]
            }
        }),
        json!({
            "name": "logout",
            "description": "Disconnect an account from the backend pool.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type" },
                    "account_id": { "type": "string", "description": "Account/user ID to disconnect" }
                },
                "required": ["backend", "account_id"]
            }
        }),
        json!({
            "name": "list_accounts",
            "description": "List all connected accounts across all backends.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "list_plugins",
            "description": "List all chat plugins compiled into this MCP server, with each plugin's \
                            declared manifest (description, external programs, HTTP hosts, homepage). \
                            Useful for verifying which backends are available without needing to log in.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "list_plugin_tools",
            "description": "Return the MCP tools this MCP server is willing to honour for a given \
                            backend slug — i.e. the subset of `tools/list` that the backend's \
                            declared capabilities actually support. Call this first to avoid \
                            invoking `list_friends` on Hacker News etc.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend slug (e.g. hackernews, lemmy, discord)" }
                },
                "required": ["backend"]
            }
        }),

        // Read tools
        json!({
            "name": "list_servers",
            "deprecated": true,
            "description": "[DEPRECATED — prefer sidebar_declaration via the client-ui surface] \
                            List servers/guilds/teams/spaces for a connected backend account.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type" },
                    "account_id": { "type": "string", "description": "Account ID (optional, uses first of type)" }
                },
                "required": ["backend"]
            }
        }),
        json!({
            "name": "list_channels",
            "deprecated": true,
            "description": "[DEPRECATED — prefer sidebar_declaration via the client-ui surface] \
                            List channels in a server.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type" },
                    "server_id": { "type": "string", "description": "Server/Space/Guild ID" },
                    "account_id": { "type": "string", "description": "Account ID (optional)" }
                },
                "required": ["backend", "server_id"]
            }
        }),
        json!({
            "name": "get_messages",
            "deprecated": true,
            "description": "[DEPRECATED — prefer view_rows via the client-ui surface] \
                            Get messages from a channel (paginated).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type" },
                    "channel_id": { "type": "string", "description": "Channel/Room ID" },
                    "limit": { "type": "integer", "description": "Max messages (default 50)" },
                    "account_id": { "type": "string", "description": "Account ID (optional)" }
                },
                "required": ["backend", "channel_id"]
            }
        }),
        json!({
            "name": "list_dms",
            "deprecated": true,
            "description": "[DEPRECATED — prefer sidebar_declaration via the client-ui surface] \
                            List DM channels for an account.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type" },
                    "account_id": { "type": "string", "description": "Account ID (optional)" }
                },
                "required": ["backend"]
            }
        }),
        json!({
            "name": "list_friends",
            "deprecated": true,
            "description": "[DEPRECATED — prefer context_menu_user + invoke_context_action via the client-ui surface] \
                            List friends for an account.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type" },
                    "account_id": { "type": "string", "description": "Account ID (optional)" }
                },
                "required": ["backend"]
            }
        }),
        json!({
            "name": "get_user",
            "deprecated": true,
            "description": "[DEPRECATED — prefer context_menu_user via the client-ui surface] \
                            Get user profile by ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type" },
                    "user_id": { "type": "string", "description": "User ID to look up" },
                    "account_id": { "type": "string", "description": "Account ID (optional)" }
                },
                "required": ["backend", "user_id"]
            }
        }),

        // Typing indicator
        json!({
            "name": "send_typing",
            "description": "Broadcast a typing indicator for a channel. Fire-and-forget — \
                            call this before send_message to make the AI's presence visible. \
                            Only available for backends that support typing indicators \
                            (discord, matrix, stoat, poly, demo).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type" },
                    "account_id": { "type": "string", "description": "Account ID (optional)" },
                    "channel_id": { "type": "string", "description": "Channel/Room ID" }
                },
                "required": ["backend", "channel_id"]
            }
        }),

        // Write tools
        json!({
            "name": "send_message",
            "deprecated": true,
            "description": "[DEPRECATED — prefer invoke_composer_action via the client-ui surface] \
                            Send a message to a channel.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type" },
                    "channel_id": { "type": "string", "description": "Channel/Room ID" },
                    "text": { "type": "string", "description": "Message text" },
                    "account_id": { "type": "string", "description": "Account ID (optional)" }
                },
                "required": ["backend", "channel_id", "text"]
            }
        }),

        // Test server easy-signin (localhost only, no password required)
        json!({
            "name": "test_signin",
            "description": "Sign in to a localhost test server without a password. \
                            Only works on localhost/127.0.0.1 URLs. \
                            Calls /test/auth/token to get a session token.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend type: stoat, matrix, etc." },
                    "url": { "type": "string", "description": "Test server URL (must be localhost or 127.0.0.1)" },
                    "username": { "type": "string", "description": "Username (no password required)" }
                },
                "required": ["backend", "url", "username"]
            }
        }),

        // Test server tools
        json!({
            "name": "test_health",
            "description": "Check test server health. Omit 'backend' to check all 5.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend name (optional, omit for all)" }
                }
            }
        }),
        json!({
            "name": "test_reseed",
            "description": "Reset and reseed a test server's demo data. Use 'all' to reseed all servers.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string", "description": "Backend name or 'all'" }
                },
                "required": ["backend"]
            }
        }),

        // ─── Client-provided UI surface (WP 8, plan-client-ui-surface §7) ────
        // Capability-driven filtering lives in `should_expose_tool` /
        // `tool_list_for_backend` (polish plan P51). These surface tools
        // always stay in `tool_list()` — they're safe on any backend per D9.
        json!({
            "name": "context_menu_server",
            "description": "Return plugin-declared context-menu items for a server target.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "target_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "target_id"]
            }
        }),
        json!({
            "name": "context_menu_channel",
            "description": "Return plugin-declared context-menu items for a channel target.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "target_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "target_id"]
            }
        }),
        json!({
            "name": "context_menu_user",
            "description": "Return plugin-declared context-menu items for a user target.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "target_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "target_id"]
            }
        }),
        json!({
            "name": "context_menu_message",
            "description": "Return plugin-declared context-menu items for a message target.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "target_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "target_id"]
            }
        }),
        json!({
            "name": "context_menu_dm",
            "description": "Return plugin-declared context-menu items for a DM target.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "target_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "target_id"]
            }
        }),
        json!({
            "name": "context_menu_category",
            "description": "Return plugin-declared context-menu items for a category target.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "target_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "target_id"]
            }
        }),
        json!({
            "name": "invoke_context_action",
            "description": "Invoke a plugin-declared context-menu action. Returns ActionOutcome as JSON.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "action_id": { "type": "string", "description": "Plugin-defined opaque action id (kebab-case)." },
                    "target_kind": { "type": "string", "description": "One of: category, channel, dm, message, server, user." },
                    "target_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "action_id", "target_kind", "target_id"]
            }
        }),
        json!({
            "name": "plugin_settings_sections",
            "description": "Return the plugin-declared settings sections.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend"]
            }
        }),
        json!({
            "name": "plugin_setting_get",
            "description": "Get a single plugin-declared setting value.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "scope": { "type": "string", "description": "One of: account-global, per-server, per-channel, per-user." },
                    "scope_id": { "type": "string" },
                    "key": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "scope", "scope_id", "key"]
            }
        }),
        json!({
            "name": "plugin_setting_set",
            "description": "Set a single plugin-declared setting value.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "scope": { "type": "string", "description": "One of: account-global, per-server, per-channel, per-user." },
                    "scope_id": { "type": "string" },
                    "key": { "type": "string" },
                    "value": { "type": "string", "description": "Serialized JSON value as a string." },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "scope", "scope_id", "key", "value"]
            }
        }),
        json!({
            "name": "sidebar_declaration",
            "description": "Return the plugin-declared sidebar layout + sections.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend"]
            }
        }),
        json!({
            "name": "invoke_sidebar_action",
            "description": "Invoke a plugin-declared sidebar action.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "action_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "action_id"]
            }
        }),
        json!({
            "name": "channel_view",
            "description": "Return the plugin-declared view descriptor for a channel.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "channel_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "channel_id"]
            }
        }),
        json!({
            "name": "view_rows",
            "description": "Paged row fetch for a plugin-declared view.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "channel_id": { "type": "string" },
                    "cursor_kind": { "type": "string", "description": "Optional; one of offset/timestamp/id/opaque." },
                    "cursor_value": { "type": "string", "description": "Optional; cursor payload." },
                    "sort_id": { "type": "string" },
                    "filter_id": { "type": "string" },
                    "tab_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "channel_id"]
            }
        }),
        json!({
            "name": "composer_buttons",
            "description": "Return plugin-declared composer-toolbar buttons for a channel.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "channel_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "channel_id"]
            }
        }),
        json!({
            "name": "message_actions",
            "description": "Return plugin-declared per-message action items.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "channel_id": { "type": "string" },
                    "message_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "channel_id", "message_id"]
            }
        }),
        json!({
            "name": "invoke_composer_action",
            "description": "Invoke a plugin-declared composer action.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "action_id": { "type": "string" },
                    "channel_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "action_id", "channel_id"]
            }
        }),
        json!({
            "name": "invoke_message_action",
            "description": "Invoke a plugin-declared per-message action.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend": { "type": "string" },
                    "action_id": { "type": "string" },
                    "channel_id": { "type": "string" },
                    "message_id": { "type": "string" },
                    "account_id": { "type": "string" }
                },
                "required": ["backend", "action_id", "channel_id", "message_id"]
            }
        }),
    ]
}

// ─── Dispatch ────────────────────────────────────────────────────────────────

pub async fn dispatch(tool: &str, args: &Value, pool: &mut BackendPool) -> Value {
    match tool {
        "login" => handle_login(args, pool).await,
        "logout" => handle_logout(args, pool),
        "list_accounts" => ok_result(serde_json::to_string_pretty(&pool.list_accounts()).unwrap_or_default()),
        "list_plugins" => handle_list_plugins(),
        "list_plugin_tools" => handle_list_plugin_tools(args),

        "list_servers" => handle_list_servers(args, pool).await,
        "list_channels" => handle_list_channels(args, pool).await,
        "get_messages" => handle_get_messages(args, pool).await,
        "list_dms" => handle_list_dms(args, pool).await,
        "list_friends" => handle_list_friends(args, pool).await,
        "get_user" => handle_get_user(args, pool).await,
        "send_message" => handle_send_message(args, pool).await,
        "send_typing" => handle_send_typing(args, pool).await,

        "test_signin" => handle_test_signin(args, pool).await,
        "test_health" => handle_test_lifecycle(args, "health").await,
        "test_reseed" => handle_test_lifecycle(args, "reseed").await,

        // Client-provided UI surface (WP 8).
        "context_menu_server" => handle_context_menu(args, pool, MenuTargetKind::Server).await,
        "context_menu_channel" => handle_context_menu(args, pool, MenuTargetKind::Channel).await,
        "context_menu_user" => handle_context_menu(args, pool, MenuTargetKind::User).await,
        "context_menu_message" => handle_context_menu(args, pool, MenuTargetKind::Message).await,
        "context_menu_dm" => handle_context_menu(args, pool, MenuTargetKind::Dm).await,
        "context_menu_category" => handle_context_menu(args, pool, MenuTargetKind::Category).await,
        "invoke_context_action" => handle_invoke_context_action(args, pool).await,
        "plugin_settings_sections" => handle_plugin_settings_sections(args, pool).await,
        "plugin_setting_get" => handle_plugin_setting_get(args, pool).await,
        "plugin_setting_set" => handle_plugin_setting_set(args, pool).await,
        "sidebar_declaration" => handle_sidebar_declaration(args, pool).await,
        "invoke_sidebar_action" => handle_invoke_sidebar_action(args, pool).await,
        "channel_view" => handle_channel_view(args, pool).await,
        "view_rows" => handle_view_rows(args, pool).await,
        "composer_buttons" => handle_composer_buttons(args, pool).await,
        "message_actions" => handle_message_actions(args, pool).await,
        "invoke_composer_action" => handle_invoke_composer_action(args, pool).await,
        "invoke_message_action" => handle_invoke_message_action(args, pool).await,

        _ => err_result(format!("unknown tool: {tool}")),
    }
}

// ─── Handler implementations ─────────────────────────────────────────────────

async fn handle_login(args: &Value, pool: &mut BackendPool) -> Value {
    let backend = match str_arg(args, "backend") {
        Some(b) => b,
        None => return err_result("missing 'backend' argument"),
    };
    let url = match str_arg(args, "url") {
        Some(u) => u,
        None => return err_result("missing 'url' argument"),
    };

    let credentials = if backend == "poly" {
        let is_signup = args.get("is_signup").and_then(|v| v.as_bool()).unwrap_or(false);
        let key: [u8; 32] = rand::random();
        AuthCredentials::PolyServer {
            server_url: url.to_string(),
            private_key_bytes: key.to_vec(),
            username: str_arg(args, "username").map(|s| s.to_string()),
            email: None,
            display_name: str_arg(args, "display_name").map(|s| s.to_string()),
            selected_user_id: str_arg(args, "user_id").map(|s| s.to_string()),
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

fn handle_logout(args: &Value, pool: &mut BackendPool) -> Value {
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
    match pool.remove(bt, account_id) {
        Some(_) => ok_result(format!("Disconnected {backend_str}:{account_id}")),
        None => err_result(format!("No active session for {backend_str}:{account_id}")),
    }
}

/// Find the backend entry for a tool call (by type + optional account_id).
fn find_backend<'a>(args: &Value, pool: &'a BackendPool) -> Result<&'a crate::state::BackendEntry, Value> {
    let backend_str = str_arg(args, "backend")
        .ok_or_else(|| err_result("missing 'backend'"))?;
    let bt = parse_backend_type(backend_str)
        .ok_or_else(|| err_result(format!("unknown backend: {backend_str}")))?;

    if let Some(account_id) = str_arg(args, "account_id") {
        pool.get(bt, account_id)
            .ok_or_else(|| err_result(format!("no session for {backend_str}:{account_id}")))
    } else {
        pool.find_by_type(bt)
            .ok_or_else(|| err_result(format!("no active {backend_str} session. Call 'login' first.")))
    }
}

async fn handle_list_servers(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_servers().await {
        Ok(servers) => ok_result(serde_json::to_string_pretty(&servers).unwrap_or_default()),
        Err(e) => err_result(format!("get_servers failed: {e}")),
    }
}

async fn handle_list_channels(args: &Value, pool: &BackendPool) -> Value {
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

async fn handle_get_messages(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let limit = u64_arg(args, "limit").unwrap_or(50) as u32;
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

async fn handle_list_dms(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_dm_channels().await {
        Ok(dms) => ok_result(serde_json::to_string_pretty(&dms).unwrap_or_default()),
        Err(e) => err_result(format!("get_dm_channels failed: {e}")),
    }
}

async fn handle_list_friends(args: &Value, pool: &BackendPool) -> Value {
    // Capability guard: backends with no friends concept (HN, Lemmy, GitHub)
    // return an explicit NotSupported error instead of `Ok([])`, which would
    // silently mislead the caller into thinking the user has zero friends.
    if let Some(slug) = str_arg(args, "backend") {
        let caps = poly_client::capabilities_for_slug(slug);
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

async fn handle_get_user(args: &Value, pool: &BackendPool) -> Value {
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

async fn handle_send_message(args: &Value, pool: &BackendPool) -> Value {
    // Capability guard: backends without Full messaging (HN, GitHub) return
    // NotSupported up-front. Prevents the MCP from plumbing writes into a
    // backend whose API physically cannot accept them.
    if let Some(slug) = str_arg(args, "backend") {
        let caps = poly_client::capabilities_for_slug(slug);
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

async fn handle_send_typing(args: &Value, pool: &BackendPool) -> Value {
    // Capability guard: only expose to backends that advertise typing indicators.
    if let Some(slug) = str_arg(args, "backend") {
        let caps = poly_client::capabilities_for_slug(slug);
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

/// Compute the subset of MCP tool names that are honest for a backend slug.
///
/// Read-only backends drop `send_message`. Backends with no DMs drop
/// `list_dms`. Backends with no friends drop `list_friends`. Backends with
/// no notifications drop `list_notifications`. The client uses this to
/// pick the narrowest sensible tool surface for an account.
fn handle_list_plugin_tools(args: &Value) -> Value {
    let Some(slug) = str_arg(args, "backend") else {
        return err_result("missing 'backend'");
    };
    let caps = poly_client::capabilities_for_slug(slug);
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

// ─── List compiled-in plugins ────────────────────────────────────────────────

/// Snapshot one plugin's identity + declared manifest.
fn plugin_entry(id: &str, name: &str, manifest: PluginManifest) -> Value {
    json!({
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
fn handle_list_plugins() -> Value {
    let plugins: Vec<Value> = vec![
        {
            let c = poly_stoat::StoatClient::with_base_url("http://localhost").ok();
            match c {
                Some(c) => plugin_entry("stoat", c.backend_name(), c.plugin_manifest()),
                None => json!({ "id": "stoat", "error": "failed to construct" }),
            }
        },
        {
            let c = poly_matrix::MatrixClient::with_homeserver("http://localhost").ok();
            match c {
                Some(c) => plugin_entry("matrix", c.backend_name(), c.plugin_manifest()),
                None => json!({ "id": "matrix", "error": "failed to construct" }),
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

// ─── Test server easy-signin ─────────────────────────────────────────────────

/// Sign in to a localhost test server without a password.
/// Calls `POST /test/auth/token`, then logs in with the returned token.
async fn handle_test_signin(args: &Value, pool: &mut BackendPool) -> Value {
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
                .map(|s| s.to_string());
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

// ─── Test server lifecycle ───────────────────────────────────────────────────

const TEST_PORTS: &[(&str, u16)] = &[
    ("matrix", 9100),
    ("stoat", 9101),
    ("discord", 9102),
    ("teams", 9103),
    ("poly", 9104),
    ("lemmy", 8536),
    ("hackernews", 8537),
];

async fn handle_test_lifecycle(args: &Value, endpoint: &str) -> Value {
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
                json!({ "backend": name, "status": status, "response": body })
            }
            Err(e) => json!({ "backend": name, "error": e.to_string() }),
        };
        results.push(result);
    }

    ok_result(serde_json::to_string_pretty(&results).unwrap_or_default())
}

// ─── Client-provided UI surface handlers (WP 8) ──────────────────────────────

async fn handle_context_menu(args: &Value, pool: &BackendPool, target: MenuTargetKind) -> Value {
    let target_id = match str_arg(args, "target_id") {
        Some(t) => t,
        None => return err_result("missing 'target_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_context_menu_items(target, target_id).await {
        Ok(items) => ok_result(serde_json::to_string_pretty(&items).unwrap_or_default()),
        Err(e) => err_result(format!("get_context_menu_items failed: {e}")),
    }
}

async fn handle_invoke_context_action(args: &Value, pool: &BackendPool) -> Value {
    let action_id = match str_arg(args, "action_id") {
        Some(a) => a,
        None => return err_result("missing 'action_id'"),
    };
    let target_kind_str = match str_arg(args, "target_kind") {
        Some(k) => k,
        None => return err_result("missing 'target_kind'"),
    };
    let target = match parse_menu_target(target_kind_str) {
        Some(t) => t,
        None => return err_result(format!("unknown target_kind: {target_kind_str}")),
    };
    let target_id = match str_arg(args, "target_id") {
        Some(t) => t,
        None => return err_result("missing 'target_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.invoke_context_action(action_id, target, target_id).await {
        Ok(outcome) => ok_result(serde_json::to_string_pretty(&outcome).unwrap_or_default()),
        Err(e) => err_result(format!("invoke_context_action failed: {e}")),
    }
}

async fn handle_plugin_settings_sections(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_settings_sections().await {
        Ok(sections) => ok_result(serde_json::to_string_pretty(&sections).unwrap_or_default()),
        Err(e) => err_result(format!("get_settings_sections failed: {e}")),
    }
}

async fn handle_plugin_setting_get(args: &Value, pool: &BackendPool) -> Value {
    let scope_str = match str_arg(args, "scope") {
        Some(s) => s,
        None => return err_result("missing 'scope'"),
    };
    let scope = match parse_settings_scope(scope_str) {
        Some(s) => s,
        None => return err_result(format!("unknown scope: {scope_str}")),
    };
    let scope_id = match str_arg(args, "scope_id") {
        Some(s) => s,
        None => return err_result("missing 'scope_id'"),
    };
    let key = match str_arg(args, "key") {
        Some(k) => k,
        None => return err_result("missing 'key'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_setting_value(scope, scope_id, key).await {
        Ok(v) => ok_result(v),
        Err(e) => err_result(format!("get_setting_value failed: {e}")),
    }
}

async fn handle_plugin_setting_set(args: &Value, pool: &BackendPool) -> Value {
    let scope_str = match str_arg(args, "scope") {
        Some(s) => s,
        None => return err_result("missing 'scope'"),
    };
    let scope = match parse_settings_scope(scope_str) {
        Some(s) => s,
        None => return err_result(format!("unknown scope: {scope_str}")),
    };
    let scope_id = match str_arg(args, "scope_id") {
        Some(s) => s,
        None => return err_result("missing 'scope_id'"),
    };
    let key = match str_arg(args, "key") {
        Some(k) => k,
        None => return err_result("missing 'key'"),
    };
    let value = match str_arg(args, "value") {
        Some(v) => v,
        None => return err_result("missing 'value'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.set_setting_value(scope, scope_id, key, value).await {
        Ok(()) => ok_result("ok"),
        Err(e) => err_result(format!("set_setting_value failed: {e}")),
    }
}

async fn handle_sidebar_declaration(args: &Value, pool: &BackendPool) -> Value {
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_sidebar_declaration().await {
        Ok(d) => ok_result(serde_json::to_string_pretty(&d).unwrap_or_default()),
        Err(e) => err_result(format!("get_sidebar_declaration failed: {e}")),
    }
}

async fn handle_invoke_sidebar_action(args: &Value, pool: &BackendPool) -> Value {
    let action_id = match str_arg(args, "action_id") {
        Some(a) => a,
        None => return err_result("missing 'action_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.invoke_sidebar_action(action_id).await {
        Ok(outcome) => ok_result(serde_json::to_string_pretty(&outcome).unwrap_or_default()),
        Err(e) => err_result(format!("invoke_sidebar_action failed: {e}")),
    }
}

async fn handle_channel_view(args: &Value, pool: &BackendPool) -> Value {
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_channel_view(channel_id).await {
        Ok(d) => ok_result(serde_json::to_string_pretty(&d).unwrap_or_default()),
        Err(e) => err_result(format!("get_channel_view failed: {e}")),
    }
}

async fn handle_view_rows(args: &Value, pool: &BackendPool) -> Value {
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let cursor = match (str_arg(args, "cursor_kind"), str_arg(args, "cursor_value")) {
        (Some(kind_s), Some(val)) => match parse_cursor_kind(kind_s) {
            Some(kind) => Some(Cursor { kind, value: val.to_string() }),
            None => return err_result(format!("unknown cursor_kind: {kind_s}")),
        },
        (None, None) => None,
        _ => return err_result("cursor_kind and cursor_value must both be present or both absent"),
    };
    let sort_id = str_arg(args, "sort_id");
    let filter_id = str_arg(args, "filter_id");
    let tab_id = str_arg(args, "tab_id");
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_view_rows(channel_id, cursor, sort_id, filter_id, tab_id).await {
        Ok(page) => ok_result(serde_json::to_string_pretty(&page).unwrap_or_default()),
        Err(e) => err_result(format!("get_view_rows failed: {e}")),
    }
}

async fn handle_composer_buttons(args: &Value, pool: &BackendPool) -> Value {
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_composer_buttons(channel_id).await {
        Ok(btns) => ok_result(serde_json::to_string_pretty(&btns).unwrap_or_default()),
        Err(e) => err_result(format!("get_composer_buttons failed: {e}")),
    }
}

async fn handle_message_actions(args: &Value, pool: &BackendPool) -> Value {
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let message_id = match str_arg(args, "message_id") {
        Some(m) => m,
        None => return err_result("missing 'message_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.get_message_actions(channel_id, message_id).await {
        Ok(items) => ok_result(serde_json::to_string_pretty(&items).unwrap_or_default()),
        Err(e) => err_result(format!("get_message_actions failed: {e}")),
    }
}

async fn handle_invoke_composer_action(args: &Value, pool: &BackendPool) -> Value {
    let action_id = match str_arg(args, "action_id") {
        Some(a) => a,
        None => return err_result("missing 'action_id'"),
    };
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.invoke_composer_action(action_id, channel_id).await {
        Ok(outcome) => ok_result(serde_json::to_string_pretty(&outcome).unwrap_or_default()),
        Err(e) => err_result(format!("invoke_composer_action failed: {e}")),
    }
}

async fn handle_invoke_message_action(args: &Value, pool: &BackendPool) -> Value {
    let action_id = match str_arg(args, "action_id") {
        Some(a) => a,
        None => return err_result("missing 'action_id'"),
    };
    let channel_id = match str_arg(args, "channel_id") {
        Some(c) => c,
        None => return err_result("missing 'channel_id'"),
    };
    let message_id = match str_arg(args, "message_id") {
        Some(m) => m,
        None => return err_result("missing 'message_id'"),
    };
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };
    match entry.backend.invoke_message_action(action_id, channel_id, message_id).await {
        Ok(outcome) => ok_result(serde_json::to_string_pretty(&outcome).unwrap_or_default()),
        Err(e) => err_result(format!("invoke_message_action failed: {e}")),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    //! Table-driven coverage for `should_expose_tool` (polish plan P51).
    //! Every known backend slug × every known tool name is exercised against
    //! the capability-derived truth table.

    use super::*;

    /// All backend slugs `capabilities_for_slug` recognises plus the
    /// fallback branch (any unknown slug returns READ_ONLY_FEED).
    const KNOWN_SLUGS: &[&str] = &[
        "demo", "stoat", "matrix", "discord", "teams", "poly",
        "lemmy", "hackernews", "github", "forgejo", "demo_forum",
    ];

    /// Backend-agnostic tools — exposed regardless of slug.
    const ALWAYS_EXPOSED: &[&str] = &[
        "login", "logout", "list_accounts", "list_plugins", "list_plugin_tools",
        "test_signin", "test_health", "test_reseed",
        "list_servers", "list_channels", "get_messages", "get_user",
        "context_menu_server", "context_menu_channel", "context_menu_user",
        "context_menu_message", "context_menu_dm", "context_menu_category",
        "invoke_context_action", "plugin_settings_sections",
        "plugin_setting_get", "plugin_setting_set",
        "sidebar_declaration", "invoke_sidebar_action",
        "channel_view", "view_rows", "composer_buttons", "message_actions",
        "invoke_composer_action", "invoke_message_action",
    ];

    #[test]
    fn always_exposed_tools_pass_on_every_backend() {
        for slug in KNOWN_SLUGS {
            let caps = poly_client::capabilities_for_slug(slug);
            for tool in ALWAYS_EXPOSED {
                assert!(
                    should_expose_tool(tool, &caps),
                    "tool '{tool}' should be exposed on backend '{slug}'"
                );
            }
        }
    }

    #[test]
    fn list_friends_gated_on_friend_model() {
        for slug in KNOWN_SLUGS {
            let caps = poly_client::capabilities_for_slug(slug);
            let exposed = should_expose_tool("list_friends", &caps);
            let expected = !matches!(caps.friends, FriendModel::None);
            assert_eq!(exposed, expected, "list_friends on '{slug}'");
        }
    }

    #[test]
    fn list_dms_gated_on_dm_support() {
        for slug in KNOWN_SLUGS {
            let caps = poly_client::capabilities_for_slug(slug);
            let exposed = should_expose_tool("list_dms", &caps);
            let expected = !matches!(caps.dms, DmSupport::None);
            assert_eq!(exposed, expected, "list_dms on '{slug}'");
        }
    }

    #[test]
    fn send_message_gated_on_messaging_model() {
        for slug in KNOWN_SLUGS {
            let caps = poly_client::capabilities_for_slug(slug);
            let exposed = should_expose_tool("send_message", &caps);
            let expected = matches!(caps.messaging, MessagingModel::Full);
            assert_eq!(exposed, expected, "send_message on '{slug}'");
        }
    }

    #[test]
    fn hackernews_is_read_only_feed_shape() {
        // Concrete expectations — HN is the canonical read-only slug.
        let caps = poly_client::capabilities_for_slug("hackernews");
        assert!(!should_expose_tool("send_message", &caps));
        assert!(!should_expose_tool("list_friends", &caps));
        assert!(!should_expose_tool("list_dms", &caps));
        assert!(should_expose_tool("get_messages", &caps));
        assert!(should_expose_tool("view_rows", &caps));
    }

    #[test]
    fn discord_is_full_social_chat_shape() {
        let caps = poly_client::capabilities_for_slug("discord");
        assert!(should_expose_tool("send_message", &caps));
        assert!(should_expose_tool("list_friends", &caps));
        assert!(should_expose_tool("list_dms", &caps));
    }

    #[test]
    fn unknown_tool_name_not_exposed() {
        let caps = poly_client::capabilities_for_slug("discord");
        assert!(!should_expose_tool("not_a_real_tool", &caps));
        assert!(!should_expose_tool("", &caps));
    }

    #[test]
    fn send_typing_gated_on_supports_typing_indicators() {
        // Backends with typing support should expose the tool.
        for slug in ["discord", "matrix", "stoat", "poly", "demo"] {
            let caps = poly_client::capabilities_for_slug(slug);
            assert!(
                should_expose_tool("send_typing", &caps),
                "send_typing should be exposed on backend '{slug}'"
            );
        }
        // Backends without typing support must not expose the tool.
        for slug in ["hackernews", "lemmy", "teams", "github"] {
            let caps = poly_client::capabilities_for_slug(slug);
            assert!(
                !should_expose_tool("send_typing", &caps),
                "send_typing must NOT be exposed on backend '{slug}'"
            );
        }
    }

    fn tool_names(list: &[Value]) -> std::collections::HashSet<String> {
        list.iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect()
    }

    #[test]
    fn tool_list_for_backend_filters_hn() {
        let list = tool_list_for_backend("hackernews");
        let names = tool_names(&list);
        assert!(!names.contains("send_message"));
        assert!(!names.contains("list_friends"));
        assert!(!names.contains("list_dms"));
        // Client-ui surface tools stay exposed on HN.
        assert!(names.contains("context_menu_server"));
        assert!(names.contains("view_rows"));
    }

    #[test]
    fn tool_list_unfiltered_still_advertises_all_legacy_tools() {
        // Guard: `tool_list()` itself must stay unfiltered so generic
        // `tools/list` RPCs keep every tool name callable.
        let list = tool_list();
        let names = tool_names(&list);
        for t in ["send_message", "list_friends", "list_dms", "send_typing"] {
            assert!(names.contains(t), "'{t}' dropped from tool_list()");
        }
    }
}

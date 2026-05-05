//! MCP tool definitions and dispatch.

mod chat;
mod chat_style;
mod client_settings;
mod client_ui;
mod drafts;
mod events;
mod memory_ops;
mod persona;

use crate::memory::MemoryDb;
use crate::state::BackendPool;
use serde_json::{Value, json};
use poly_client::{
    BackendCapabilities, BackendType, CursorKind, DmSupport, FriendModel,
    MenuTargetKind, MessagingModel, SettingsScope,
};

// ─── Helpers ──────────────────────────────────────────────────────────────────

// poly-lint: takes impl ToString by value to support both owned String and &str callers ergonomically.
#[allow(clippy::needless_pass_by_value)]
fn ok_result(text: impl ToString) -> Value {
    json!({ "content": [{"type": "text", "text": text.to_string()}], "isError": false })
}

#[allow(clippy::needless_pass_by_value)]
fn err_result(text: impl ToString) -> Value {
    json!({ "content": [{"type": "text", "text": text.to_string()}], "isError": true })
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

fn u64_arg(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(serde_json::Value::as_u64)
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

// ─── Capability-driven tool filtering (polish plan P51) ──────────────────────

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
    // poly-lint: arms are intentionally separated by category for readability;
    // merging would make the gating policy harder to audit.
    #[allow(clippy::match_same_arms)]
    match tool_name {
        // Account management and meta — always advertised.
        // Legacy Discord-shaped read tools gated on capability.
        "login" | "logout" | "list_accounts" | "list_plugins" | "list_plugin_tools"
        | "test_signin" | "test_health" | "test_reseed"
        | "list_servers" | "list_channels" | "get_messages" | "get_user" => true,
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

        // Phase A memory tools — always exposed; memory is Poly's own concern,
        // independent of which backend a chat uses (A.7).
        "remember_fact"
        | "recall_facts"
        | "forget_fact"
        | "search_facts"
        | "store_chat_note"
        | "get_chat_notes"
        | "forget_chat_note"
        | "store_chat_summary"
        | "get_chat_summary"
        | "get_reply_context"
        // Phase D typing-simulation tools — library always present, runtime
        // wiring is a TODO but the tools should advertise.
        | "start_typing_simulation"
        | "stop_typing_simulation"
        // Phase F catch-me-up bundler.
        | "get_unread_summary" => true,

        // Phase B draft tools — always exposed; draft queue is Poly's own concern.
        "draft_create"
        | "draft_list"
        | "draft_approve"
        | "draft_edit"
        | "draft_discard"
        | "draft_cancel_autosend" => true,

        // Phase E per-chat style tools — always exposed; style is
        // Poly's own concern, not per-backend (mirrors A.7 rationale).
        "set_chat_style"
        | "get_chat_style"
        | "list_chat_styles"
        | "forget_chat_style" => true,

        // Phase D (client-version plan) — client config tools; always exposed;
        // host-side concern, independent of which backend a chat uses.
        "client_settings_list"
        | "client_settings_get_version"
        | "client_settings_set_version_override"
        | "client_settings_list_mechanisms"
        | "client_settings_set_mechanism" => true,

        // Phase B (meta-personas) — always exposed; persona state is
        // Poly-side only, independent of which backend a chat uses.
        "meta_persona_list"
        | "meta_persona_get"
        | "meta_persona_create"
        | "meta_persona_update"
        | "meta_persona_delete"
        | "meta_persona_set_sources"
        | "meta_persona_set_tool_whitelist"
        | "meta_persona_invoke"
        | "meta_persona_set_heartbeat"
        | "meta_persona_get_memory"
        | "meta_persona_set_memory"
        | "meta_persona_forget_memory"
        | "meta_persona_recent_actions"
        | "meta_persona_set_outbound_allow"
        | "meta_persona_audit_query"
        | "meta_persona_audit_export" => true,

        // Phase C — event subscription / poll (always exposed; backend-agnostic).
        "poll_events" | "subscribe_events" | "unsubscribe_events" => true,

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
    let caps = poly_client::capabilities_for_slug_static(slug);
    tool_list()
        .into_iter()
        .filter(|t| {
            t.get("name")
                .and_then(|n| n.as_str())
                .is_some_and(|n| should_expose_tool(n, &caps))
        })
        .collect()
}

// ─── Tool list ────────────────────────────────────────────────────────────────

#[must_use]
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

        // ─── Phase A — Memory tools ──────────────────────────────────────────
        json!({
            "name": "forget_chat_note",
            "description": "Delete a per-chat note by its id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "note_id": { "type": "integer", "description": "Note ID returned by store_chat_note" }
                },
                "required": ["note_id"]
            }
        }),
        json!({
            "name": "forget_fact",
            "description": "Delete a contact fact by its id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "fact_id": { "type": "integer", "description": "Fact ID returned by remember_fact" }
                },
                "required": ["fact_id"]
            }
        }),
        json!({
            "name": "get_chat_notes",
            "description": "Return all running notes for a chat thread.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string" },
                    "chat_id":    { "type": "string" }
                },
                "required": ["account_id", "chat_id"]
            }
        }),
        json!({
            "name": "get_chat_summary",
            "description": "Return the rolling summary for a chat, or null if none stored.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string" },
                    "chat_id":    { "type": "string" }
                },
                "required": ["account_id", "chat_id"]
            }
        }),
        json!({
            "name": "get_reply_context",
            "description": "Bundle everything needed to draft a reply: recent messages, \
                            contact info + facts, per-chat notes, rolling summary. \
                            Returns a single JSON object. Gracefully omits sections for \
                            which no data is available (no contact found is not an error).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend":       { "type": "string" },
                    "account_id":    { "type": "string" },
                    "chat_id":       { "type": "string", "description": "Channel / DM / room ID" },
                    "contact_id":    { "type": "string", "description": "User ID of the primary contact (for DMs). Omit for group chats." },
                    "message_limit": { "type": "integer", "description": "How many recent messages to include (default 20)" }
                },
                "required": ["backend", "account_id", "chat_id"]
            }
        }),
        json!({
            "name": "recall_facts",
            "description": "Return stored facts about a contact. Optionally filter by category.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string" },
                    "contact_id": { "type": "string" },
                    "category":   { "type": "string", "description": "Optional category filter" }
                },
                "required": ["account_id", "contact_id"]
            }
        }),
        json!({
            "name": "remember_fact",
            "description": "Store a free-form fact about a contact (e.g. preference, schedule, relationship context). Returns fact_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string" },
                    "contact_id": { "type": "string" },
                    "category":   { "type": "string", "description": "Organisational label, e.g. 'preference', 'schedule', 'relationship'" },
                    "fact":       { "type": "string", "description": "The fact to remember" }
                },
                "required": ["account_id", "contact_id", "category", "fact"]
            }
        }),
        json!({
            "name": "search_facts",
            "description": "Search all stored facts using SQL LIKE over fact_text. Optionally scope to one account.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query":      { "type": "string", "description": "Search term (case-insensitive LIKE)" },
                    "account_id": { "type": "string", "description": "Optional: restrict to this account" }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "store_chat_note",
            "description": "Append a running note for a chat thread. Returns note_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string" },
                    "chat_id":    { "type": "string" },
                    "note":       { "type": "string" }
                },
                "required": ["account_id", "chat_id", "note"]
            }
        }),
        json!({
            "name": "store_chat_summary",
            "description": "Upsert a rolling summary of older chat history (one record per account+chat).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id":          { "type": "string" },
                    "chat_id":             { "type": "string" },
                    "summary":             { "type": "string" },
                    "window_start_msg_id": { "type": "string", "description": "ID of the oldest message included in this summary" },
                    "window_end_msg_id":   { "type": "string", "description": "ID of the newest message included in this summary" }
                },
                "required": ["account_id", "chat_id", "summary", "window_start_msg_id", "window_end_msg_id"]
            }
        }),

        // ─── Phase B — Draft queue tools ─────────────────────────────────────
        json!({
            "name": "draft_create",
            "description": "Create a pending draft reply for a chat. Returns the draft_id. \
                            If auto_send_in_secs is provided AND the per-chat auto-approve KV key is set, \
                            the draft will auto-send after that many seconds; otherwise auto-send is disabled.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id":        { "type": "string" },
                    "chat_id":           { "type": "string" },
                    "body":              { "type": "string", "description": "Draft message body" },
                    "suggested_by":      { "type": "string", "description": "Free-form agent label, e.g. 'Claude Desktop'" },
                    "auto_send_in_secs": { "type": "integer", "description": "Optional countdown seconds for auto-send (requires per-chat opt-in)" }
                },
                "required": ["account_id", "chat_id", "body", "suggested_by"]
            }
        }),
        json!({
            "name": "draft_list",
            "description": "List drafts, optionally filtered by account, chat, and/or status. \
                            Status values: pending | approved | sent | discarded | expired.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string", "description": "Optional account filter" },
                    "chat_id":    { "type": "string", "description": "Optional chat filter" },
                    "status":     { "type": "string", "description": "Optional status filter" }
                }
            }
        }),
        json!({
            "name": "draft_approve",
            "description": "Approve and immediately send a pending draft. \
                            Calls send_message on the active backend, then sets status=sent.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "draft_id": { "type": "integer", "description": "Draft ID returned by draft_create" }
                },
                "required": ["draft_id"]
            }
        }),
        json!({
            "name": "draft_edit",
            "description": "Edit the body of a pending draft. Only allowed while status=pending.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "draft_id": { "type": "integer", "description": "Draft ID" },
                    "new_body": { "type": "string", "description": "Replacement body text" }
                },
                "required": ["draft_id", "new_body"]
            }
        }),
        json!({
            "name": "draft_discard",
            "description": "Discard a pending draft (sets status=discarded). No-op if already discarded.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "draft_id": { "type": "integer", "description": "Draft ID" }
                },
                "required": ["draft_id"]
            }
        }),
        json!({
            "name": "draft_cancel_autosend",
            "description": "Cancel the auto-send timer for a draft. Clears auto_send_at, keeps status=pending.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "draft_id": { "type": "integer", "description": "Draft ID" }
                },
                "required": ["draft_id"]
            }
        }),

        // Phase E — per-chat style tools
        json!({
            "name": "set_chat_style",
            "description": "Set or update the reply style for a specific chat. All style fields are optional — omitted fields retain their current value.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id":    { "type": "string" },
                    "chat_id":       { "type": "string" },
                    "tone":          { "type": "string", "description": "Free-form tone label, e.g. 'casual', 'professional', 'snarky', 'warm', 'direct'" },
                    "formality":     { "type": "string", "description": "'tu', 'vous', or 'neutral'" },
                    "emoji_allowed": { "type": "boolean", "description": "Whether emoji are appropriate in this chat (default true)" },
                    "signature":     { "type": "string", "description": "Optional sign-off appended to replies" },
                    "extra_notes":   { "type": "string", "description": "Free-form style notes the AI should honor" }
                },
                "required": ["account_id", "chat_id"]
            }
        }),
        json!({
            "name": "get_chat_style",
            "description": "Return the reply style configured for a chat, or null if none is set.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string" },
                    "chat_id":    { "type": "string" }
                },
                "required": ["account_id", "chat_id"]
            }
        }),
        json!({
            "name": "list_chat_styles",
            "description": "Return all per-chat style records, optionally filtered to one account.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string", "description": "Optional: restrict to this account" }
                }
            }
        }),
        json!({
            "name": "forget_chat_style",
            "description": "Delete the style record for a specific chat. No-op if not present.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string" },
                    "chat_id":    { "type": "string" }
                },
                "required": ["account_id", "chat_id"]
            }
        }),

        // ─── Phase B (meta-personas) — 14 MCP tools ─────────────────────────
        json!({
            "name": "meta_persona_list",
            "description": "List all defined meta-personalities with summary fields (slug, name, avatar, enabled, proactivity, heartbeat interval).",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "meta_persona_get",
            "description": "Return the full persona row for a single persona by slug.",
            "inputSchema": {
                "type": "object",
                "required": ["slug"],
                "properties": {
                    "slug": { "type": "string", "description": "Persona slug, e.g. 'broker-bob'" }
                }
            }
        }),
        json!({
            "name": "meta_persona_create",
            "description": "Create a new meta-personality. Returns the slug on success.",
            "inputSchema": {
                "type": "object",
                "required": ["slug", "name", "system_prompt"],
                "properties": {
                    "slug":                    { "type": "string", "description": "URL-safe identifier, e.g. 'broker-bob'" },
                    "name":                    { "type": "string", "description": "Display name, e.g. 'Broker Bob'" },
                    "avatar_emoji":            { "type": "string", "description": "Single emoji avatar (default 🤖)" },
                    "system_prompt":           { "type": "string", "description": "The persona's system / role prompt" },
                    "style_notes":             { "type": "string", "description": "Optional free-form voice notes" },
                    "heartbeat_interval_secs": { "type": ["integer", "null"], "minimum": 60, "maximum": 86400 },
                    "proactivity":             { "type": "string", "enum": ["drafts-only", "notify", "outbound-allowlisted"], "default": "drafts-only" },
                    "rate_limit_per_hour":     { "type": "integer", "default": 4, "minimum": 0 }
                }
            }
        }),
        json!({
            "name": "meta_persona_update",
            "description": "Update fields on an existing persona. Only supplied fields are written.",
            "inputSchema": {
                "type": "object",
                "required": ["slug"],
                "properties": {
                    "slug":                    { "type": "string" },
                    "name":                    { "type": "string" },
                    "avatar_emoji":            { "type": "string" },
                    "system_prompt":           { "type": "string" },
                    "style_notes":             { "type": ["string", "null"] },
                    "heartbeat_interval_secs": { "type": ["integer", "null"], "minimum": 60, "maximum": 86400 },
                    "proactivity":             { "type": "string", "enum": ["drafts-only", "notify", "outbound-allowlisted"] },
                    "rate_limit_per_hour":     { "type": "integer", "minimum": 0 },
                    "enabled":                 { "type": "boolean" }
                }
            }
        }),
        json!({
            "name": "meta_persona_delete",
            "description": "Delete a persona and cascade-remove all child rows (sources, facts, audit, outbound allowlist).",
            "inputSchema": {
                "type": "object",
                "required": ["slug"],
                "properties": {
                    "slug": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "meta_persona_set_sources",
            "description": "Atomically replace the source bindings for a persona. Each source entry specifies which account + chat set the persona may read. Deny rows (include=false) win over allow rows.",
            "inputSchema": {
                "type": "object",
                "required": ["slug", "sources"],
                "properties": {
                    "slug": { "type": "string" },
                    "sources": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["account_id", "selector_kind"],
                            "properties": {
                                "account_id":     { "type": "string" },
                                "selector_kind":  { "type": "string", "enum": ["all", "server", "channel", "dm", "tag"] },
                                "selector_value": { "type": "string" },
                                "include":        { "type": "boolean", "default": true }
                            }
                        }
                    }
                }
            }
        }),
        json!({
            "name": "meta_persona_set_tool_whitelist",
            "description": "Atomically replace the allowed-tool set for a persona. Empty list = read-only defaults.",
            "inputSchema": {
                "type": "object",
                "required": ["slug", "tool_names"],
                "properties": {
                    "slug":       { "type": "string" },
                    "tool_names": { "type": "array", "items": { "type": "string" } }
                }
            }
        }),
        json!({
            "name": "meta_persona_invoke",
            "description": "Invoke a meta-personality. Returns a context bundle (bundle_v1) containing the persona's system prompt, slug, name, source IDs, pinned facts, and recent chat messages. Claude composes the reply using this bundle. When dry_run=true the bundle is built identically but memory_read audit rows are suppressed; use this for bundle-shape inspection without polluting audit history.",
            "inputSchema": {
                "type": "object",
                "required": ["slug"],
                "properties": {
                    "slug":                  { "type": "string", "description": "Persona slug, e.g. 'broker-bob'" },
                    "user_prompt":           { "type": "string", "description": "Freeform user instruction; optional" },
                    "max_messages_per_chat": { "type": "integer", "default": 30, "minimum": 1, "maximum": 200 },
                    "max_chats":             { "type": "integer", "default": 25, "minimum": 1, "maximum": 100 },
                    "include_summaries":     { "type": "boolean", "default": true },
                    "dry_run":               { "type": "boolean", "default": false, "description": "When true: build the full bundle_v1 but skip memory_read audit-row writes. The user-initiated invoke audit row still fires. Returns bundle with top-level dry_run=true field." }
                }
            }
        }),
        json!({
            "name": "meta_persona_set_heartbeat",
            "description": "Set or clear the heartbeat interval for a persona. NULL/0 disables heartbeat.",
            "inputSchema": {
                "type": "object",
                "required": ["slug"],
                "properties": {
                    "slug":          { "type": "string" },
                    "interval_secs": { "type": ["integer", "null"], "minimum": 60, "maximum": 86400,
                                       "description": "60s minimum, 24h maximum; null or 0 disables" }
                }
            }
        }),
        json!({
            "name": "meta_persona_get_memory",
            "description": "Read facts from a persona's private memory partition. Optionally filter to pinned-only facts.",
            "inputSchema": {
                "type": "object",
                "required": ["slug"],
                "properties": {
                    "slug":        { "type": "string" },
                    "pinned_only": { "type": "boolean", "default": false }
                }
            }
        }),
        json!({
            "name": "meta_persona_set_memory",
            "description": "Store a fact in a persona's private memory partition. Persona memory is separate from contact_facts.",
            "inputSchema": {
                "type": "object",
                "required": ["slug", "fact_text"],
                "properties": {
                    "slug":      { "type": "string" },
                    "category":  { "type": "string", "description": "Free-form label, e.g. 'observation', 'preference', 'reminder'" },
                    "fact_text": { "type": "string", "maxLength": 2000 },
                    "pinned":    { "type": "boolean", "default": false }
                }
            }
        }),
        json!({
            "name": "meta_persona_forget_memory",
            "description": "Delete a single fact from a persona's memory by fact_id, or wipe ALL facts for the persona when forget_all=true.",
            "inputSchema": {
                "type": "object",
                "required": ["slug"],
                "properties": {
                    "slug":       { "type": "string" },
                    "fact_id":    { "type": "integer", "description": "Fact ID returned by meta_persona_set_memory" },
                    "forget_all": { "type": "boolean", "default": false,
                                    "description": "Set true to delete ALL facts for this persona (requires typed confirmation in UI)" }
                }
            }
        }),
        json!({
            "name": "meta_persona_recent_actions",
            "description": "Return the most recent audit-log entries for a persona (newest first). Default limit 50.",
            "inputSchema": {
                "type": "object",
                "required": ["slug"],
                "properties": {
                    "slug":  { "type": "string" },
                    "limit": { "type": "integer", "default": 50, "minimum": 1, "maximum": 500 }
                }
            }
        }),
        json!({
            "name": "meta_persona_set_outbound_allow",
            "description": "Upsert or remove an entry in the persona's outbound allowlist. Only consulted when proactivity=outbound-allowlisted.",
            "inputSchema": {
                "type": "object",
                "required": ["slug", "account_id", "chat_id"],
                "properties": {
                    "slug":                 { "type": "string" },
                    "account_id":           { "type": "string" },
                    "chat_id":              { "type": "string" },
                    "max_messages_per_day": { "type": "integer", "default": 1, "minimum": 1, "maximum": 100 },
                    "remove":               { "type": "boolean", "default": false,
                                              "description": "Set true to remove this entry from the allowlist" }
                }
            }
        }),

        // ─── Phase T — audit surface ──────────────────────────────────────────

        json!({
            "name": "meta_persona_audit_query",
            "description": "Query the persona_audit log with optional filters. \
                            All filters are optional and ANDed together. \
                            Returns matching rows newest-first, up to limit (default 100, max 500). \
                            Use this instead of meta_persona_recent_actions when you need to filter by \
                            action, actor, result, time range, or target account/chat.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug":           { "type": "string",  "description": "Persona slug to filter by" },
                    "action":         { "type": "string",  "description": "Exact action name (e.g. invoke, outbound_send, heartbeat_run)" },
                    "actor":          { "type": "string",  "description": "Exact actor value (e.g. user, heartbeat)" },
                    "since":          { "type": "string",  "description": "ISO-8601 lower bound for occurred_at (inclusive)" },
                    "until":          { "type": "string",  "description": "ISO-8601 upper bound for occurred_at (inclusive)" },
                    "target_account": { "type": "string",  "description": "Exact target_account value" },
                    "target_chat":    { "type": "string",  "description": "Exact target_chat value" },
                    "result":         { "type": "string",  "description": "Exact result value (e.g. ok, denied, error)" },
                    "limit":          { "type": "integer", "default": 100, "minimum": 1, "maximum": 500,
                                        "description": "Maximum number of rows to return" }
                },
                "required": []
            }
        }),

        json!({
            "name": "meta_persona_audit_export",
            "description": "Export the complete audit history for a persona as a JSONL string (oldest first). \
                            Use before deleting a persona to preserve its audit trail: \
                            poly-cli call meta_persona_audit_export --slug=foo > audit.jsonl",
            "inputSchema": {
                "type": "object",
                "required": ["slug"],
                "properties": {
                    "slug": { "type": "string", "description": "Persona slug whose audit history to export" }
                }
            }
        }),

        // ─── Phase C — event subscription / poll ─────────────────────────────
        // Added concurrently with Phase A agent; rebase-safe insertion at end.
        json!({
            "name": "poll_events",
            "description": "Poll real-time events from connected accounts since a given timestamp. \
                            This is the primary event-delivery path — call it on a timer (e.g. \
                            every 2–5 seconds) to receive new messages, typing indicators, and \
                            presence changes without polling individual channels. \
                            Pass since_ms=0 on first call to get events from the last 5 minutes. \
                            Use the max seq_ms from the returned events as since_ms on the next call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "since_ms": {
                        "type": "integer",
                        "description": "Unix timestamp in milliseconds. Only events with seq_ms > since_ms are returned. \
                                        Use 0 to get all buffered events (up to 5 min old)."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of events to return (default 100, max 500)."
                    },
                    "account_ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of account ID substrings to filter by (e.g. [\"koala\"]). \
                                        Matched against the internal account key."
                    },
                    "chat_ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of channel/chat IDs to filter by."
                    },
                    "event_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of event kind slugs: message_received, message_edited, \
                                        message_deleted, typing_started, presence_changed, friend_request, reaction_added."
                    },
                    "subscription_id": {
                        "type": "string",
                        "description": "Optional: use a pre-registered subscription filter (from subscribe_events)."
                    }
                },
                "required": []
            }
        }),
        json!({
            "name": "subscribe_events",
            "description": "Register a named event subscription with optional filters. Returns a \
                            subscription_id to pass to poll_events. Useful when you want to track \
                            a specific set of chats or event types without repeating filter args \
                            on every poll call. Subscriptions persist until unsubscribe_events is \
                            called or the MCP server restarts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Account ID substrings to filter by (optional)."
                    },
                    "chat_ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Channel/chat IDs to filter by (optional)."
                    },
                    "event_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Event kind slugs to filter by (optional)."
                    }
                },
                "required": []
            }
        }),
        json!({
            "name": "unsubscribe_events",
            "description": "Remove a previously registered event subscription.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "subscription_id": { "type": "string", "description": "ID from subscribe_events." }
                },
                "required": ["subscription_id"]
            }
        }),
        json!({
            "name": "get_unread_summary",
            "description": "Phase F — Return every unread channel and DM for the account bundled with \
                their most recent N messages. Zero-LLM bundler; Claude Desktop composes the digest. \
                Use this to power a 'catch me up' flow: one MCP call, one prompt to the LLM.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": { "type": "string", "description": "Account ID (must be logged in)." },
                    "message_limit": { "type": "integer", "description": "Messages per chat (default 10).", "default": 10 }
                },
                "required": ["account_id"]
            }
        }),

        // ─── Phase D (client-version plan) — client settings tools ───────────
        // Always exposed; client settings are a host-side concern independent
        // of which backend a chat uses. Known backend IDs are hardcoded (10 IDs)
        // to avoid a live BackendPool dependency in the schema layer — simpler
        // and more reliable than deriving from the pool at list time.
        json!({
            "name": "client_settings_list",
            "description": "List current client settings (version override + mechanism states) for \
                one or all backends. If backend_id is omitted, returns an array covering the 10 \
                known backend IDs. Read-only — no audit row.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "backend_id": {
                        "type": "string",
                        "description": "Optional backend slug (e.g. 'discord'). \
                            Omit to list all 10 known backends."
                    }
                }
            }
        }),
        json!({
            "name": "client_settings_get_version",
            "description": "Return the effective client version for a backend — the override string \
                if one is set, otherwise the backend default. Read-only — no audit row.",
            "inputSchema": {
                "type": "object",
                "required": ["backend_id"],
                "properties": {
                    "backend_id": { "type": "string", "description": "Backend slug, e.g. 'discord'" }
                }
            }
        }),
        json!({
            "name": "client_settings_set_version_override",
            "description": "Set or clear the client-version override for a backend. Passing null/absent \
                clears the override (backend reverts to its default version). Writes an audit row.",
            "inputSchema": {
                "type": "object",
                "required": ["backend_id"],
                "properties": {
                    "backend_id": { "type": "string", "description": "Backend slug, e.g. 'discord'" },
                    "override":   {
                        "type": ["string", "null"],
                        "description": "Version string, e.g. 'poly-discord/0.0.0'. Null clears the override."
                    }
                }
            }
        }),
        json!({
            "name": "client_settings_list_mechanisms",
            "description": "Return all known mechanism states for a backend (mechanism_id + enabled). \
                Only mechanisms that have been explicitly set appear; the backend default applies \
                for any omitted mechanism. Read-only — no audit row.",
            "inputSchema": {
                "type": "object",
                "required": ["backend_id"],
                "properties": {
                    "backend_id": { "type": "string", "description": "Backend slug, e.g. 'discord'" }
                }
            }
        }),
        json!({
            "name": "client_settings_set_mechanism",
            "description": "Enable or disable a named mechanism for a backend. Writes an audit row.",
            "inputSchema": {
                "type": "object",
                "required": ["backend_id", "mechanism_id", "enabled"],
                "properties": {
                    "backend_id":    { "type": "string", "description": "Backend slug, e.g. 'discord'" },
                    "mechanism_id":  { "type": "string", "description": "Mechanism identifier, e.g. 'captcha-sandbox'" },
                    "enabled":       { "type": "boolean", "description": "true to enable, false to disable" }
                }
            }
        }),
    ]
        .into_iter()
        .chain(crate::typing_simulation::tool_definitions())
        .collect()
}

// ─── Dispatch ─────────────────────────────────────────────────────────────────

pub async fn dispatch(tool: &str, args: &Value, pool: &mut BackendPool, mem: &MemoryDb) -> Value {
    match tool {
        "login" => chat::handle_login(args, pool).await,
        "logout" => chat::handle_logout(args, pool),
        "list_accounts" => ok_result(serde_json::to_string_pretty(&pool.list_accounts()).unwrap_or_default()),
        "list_plugins" => chat::handle_list_plugins(),
        "list_plugin_tools" => chat::handle_list_plugin_tools(args),

        "list_servers" => chat::handle_list_servers(args, pool).await,
        "list_channels" => chat::handle_list_channels(args, pool).await,
        "get_messages" => chat::handle_get_messages(args, pool).await,
        "list_dms" => chat::handle_list_dms(args, pool).await,
        "list_friends" => chat::handle_list_friends(args, pool).await,
        "get_user" => chat::handle_get_user(args, pool).await,
        "send_message" => chat::handle_send_message(args, pool).await,
        "send_typing" => chat::handle_send_typing(args, pool).await,

        "test_signin" => chat::handle_test_signin(args, pool).await,
        "test_health" => chat::handle_test_lifecycle(args, "health").await,
        "test_reseed" => chat::handle_test_lifecycle(args, "reseed").await,

        // Client-provided UI surface (WP 8).
        "context_menu_server" => client_ui::handle_context_menu(args, pool, MenuTargetKind::Server).await,
        "context_menu_channel" => client_ui::handle_context_menu(args, pool, MenuTargetKind::Channel).await,
        "context_menu_user" => client_ui::handle_context_menu(args, pool, MenuTargetKind::User).await,
        "context_menu_message" => client_ui::handle_context_menu(args, pool, MenuTargetKind::Message).await,
        "context_menu_dm" => client_ui::handle_context_menu(args, pool, MenuTargetKind::Dm).await,
        "context_menu_category" => client_ui::handle_context_menu(args, pool, MenuTargetKind::Category).await,
        "invoke_context_action" => client_ui::handle_invoke_context_action(args, pool).await,
        "plugin_settings_sections" => client_ui::handle_plugin_settings_sections(args, pool).await,
        "plugin_setting_get" => client_ui::handle_plugin_setting_get(args, pool).await,
        "plugin_setting_set" => client_ui::handle_plugin_setting_set(args, pool).await,
        "sidebar_declaration" => client_ui::handle_sidebar_declaration(args, pool).await,
        "invoke_sidebar_action" => client_ui::handle_invoke_sidebar_action(args, pool).await,
        "channel_view" => client_ui::handle_channel_view(args, pool).await,
        "view_rows" => client_ui::handle_view_rows(args, pool).await,
        "composer_buttons" => client_ui::handle_composer_buttons(args, pool).await,
        "message_actions" => client_ui::handle_message_actions(args, pool).await,
        "invoke_composer_action" => client_ui::handle_invoke_composer_action(args, pool).await,
        "invoke_message_action" => client_ui::handle_invoke_message_action(args, pool).await,

        // Phase A — memory tools (A.7: always exposed, backend-agnostic).
        "forget_chat_note"  => memory_ops::handle_forget_chat_note(args, mem),
        "forget_fact"       => memory_ops::handle_forget_fact(args, mem),
        "get_chat_notes"    => memory_ops::handle_get_chat_notes(args, mem),
        "get_chat_summary"  => memory_ops::handle_get_chat_summary(args, mem),
        "get_reply_context" => memory_ops::handle_get_reply_context(args, pool, mem).await,
        "recall_facts"      => memory_ops::handle_recall_facts(args, mem),
        "remember_fact"     => memory_ops::handle_remember_fact(args, mem),
        "search_facts"      => memory_ops::handle_search_facts(args, mem),
        "store_chat_note"   => memory_ops::handle_store_chat_note(args, mem),
        "store_chat_summary" => memory_ops::handle_store_chat_summary(args, mem),

        // Phase B — draft queue tools.
        "draft_create"        => drafts::handle_draft_create(args, pool, mem).await,
        "draft_list"          => drafts::handle_draft_list(args, mem),
        "draft_approve"       => drafts::handle_draft_approve(args, pool, mem).await,
        "draft_edit"          => drafts::handle_draft_edit(args, mem),
        "draft_discard"       => drafts::handle_draft_discard(args, mem),
        "draft_cancel_autosend" => drafts::handle_draft_cancel_autosend(args, mem),

        // Phase E — per-chat style tools.
        "set_chat_style"    => chat_style::handle_set_chat_style(args, mem),
        "get_chat_style"    => chat_style::handle_get_chat_style(args, mem),
        "list_chat_styles"  => chat_style::handle_list_chat_styles(args, mem),
        "forget_chat_style" => chat_style::handle_forget_chat_style(args, mem),

        // Phase B (meta-personas) — always exposed.
        "meta_persona_list"          => persona::handle_meta_persona_list(mem),
        "meta_persona_get"           => persona::handle_meta_persona_get(args, mem),
        "meta_persona_create"        => persona::handle_meta_persona_create(args, mem),
        "meta_persona_update"        => persona::handle_meta_persona_update(args, mem),
        "meta_persona_delete"        => persona::handle_meta_persona_delete(args, mem),
        "meta_persona_set_sources"   => persona::handle_meta_persona_set_sources(args, mem),
        "meta_persona_set_tool_whitelist" => persona::handle_meta_persona_set_tool_whitelist(args, mem),
        "meta_persona_invoke"        => persona::handle_meta_persona_invoke(args, pool, mem).await,
        "meta_persona_set_heartbeat" => persona::handle_meta_persona_set_heartbeat(args, mem),
        "meta_persona_get_memory"    => persona::handle_meta_persona_get_memory(args, mem),
        "meta_persona_set_memory"    => persona::handle_meta_persona_set_memory(args, mem),
        "meta_persona_forget_memory" => persona::handle_meta_persona_forget_memory(args, mem),
        "meta_persona_recent_actions" => persona::handle_meta_persona_recent_actions(args, mem),
        "meta_persona_set_outbound_allow" => persona::handle_meta_persona_set_outbound_allow(args, mem),
        "meta_persona_audit_query"    => persona::handle_meta_persona_audit_query(args, mem),
        "meta_persona_audit_export"   => persona::handle_meta_persona_audit_export(args, mem),

        // Phase D (client-version plan) — client settings tools (always exposed).
        "client_settings_list"               => client_settings::handle_client_settings_list(args, pool, mem).await,
        "client_settings_get_version"        => client_settings::handle_client_settings_get_version(args, pool, mem).await,
        "client_settings_set_version_override" => client_settings::handle_client_settings_set_version_override(args, pool, mem).await,
        "client_settings_list_mechanisms"    => client_settings::handle_client_settings_list_mechanisms(args, pool, mem).await,
        "client_settings_set_mechanism"      => client_settings::handle_client_settings_set_mechanism(args, pool, mem).await,

        // Phase C — added concurrently, rebase-safe insertion
        "poll_events" => events::handle_poll_events(args, pool).await,
        "subscribe_events" => events::handle_subscribe_events(args, pool).await,
        "unsubscribe_events" => events::handle_unsubscribe_events(args, pool).await,

        // Phase D — typing simulation, fully wired.
        "start_typing_simulation" => events::handle_start_typing_simulation(args, pool).await,
        "stop_typing_simulation" => events::handle_stop_typing_simulation(args, pool).await,

        // Phase F — get_unread_summary bundles unread messages across all
        // chats for an account so Claude Desktop can compose a digest.
        "get_unread_summary" => events::handle_get_unread_summary(args, pool).await,

        _ => err_result(format!("unknown tool: {tool}")),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
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
        // Phase A memory tools — always exposed (A.7)
        "remember_fact", "recall_facts", "forget_fact", "search_facts",
        "store_chat_note", "get_chat_notes", "forget_chat_note",
        "store_chat_summary", "get_chat_summary",
        "get_reply_context",
        // Phase B draft queue tools — always exposed
        "draft_create", "draft_list", "draft_approve",
        "draft_edit", "draft_discard", "draft_cancel_autosend",
        // Phase E per-chat style tools — always exposed.
        "set_chat_style", "get_chat_style", "list_chat_styles", "forget_chat_style",
        // Phase D typing-simulation tool stubs (always exposed).
        "start_typing_simulation", "stop_typing_simulation",
        // Phase F catch-me-up.
        "get_unread_summary",
    ];

    #[test]
    fn always_exposed_tools_pass_on_every_backend() {
        for slug in KNOWN_SLUGS {
            let caps = poly_client::capabilities_for_slug_static(slug);
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
            let caps = poly_client::capabilities_for_slug_static(slug);
            let exposed = should_expose_tool("list_friends", &caps);
            let expected = !matches!(caps.friends, FriendModel::None);
            assert_eq!(exposed, expected, "list_friends on '{slug}'");
        }
    }

    #[test]
    fn list_dms_gated_on_dm_support() {
        for slug in KNOWN_SLUGS {
            let caps = poly_client::capabilities_for_slug_static(slug);
            let exposed = should_expose_tool("list_dms", &caps);
            let expected = !matches!(caps.dms, DmSupport::None);
            assert_eq!(exposed, expected, "list_dms on '{slug}'");
        }
    }

    #[test]
    fn send_message_gated_on_messaging_model() {
        for slug in KNOWN_SLUGS {
            let caps = poly_client::capabilities_for_slug_static(slug);
            let exposed = should_expose_tool("send_message", &caps);
            let expected = matches!(caps.messaging, MessagingModel::Full);
            assert_eq!(exposed, expected, "send_message on '{slug}'");
        }
    }

    #[test]
    fn hackernews_is_read_only_feed_shape() {
        // Concrete expectations — HN is the canonical read-only slug.
        let caps = poly_client::capabilities_for_slug_static("hackernews");
        assert!(!should_expose_tool("send_message", &caps));
        assert!(!should_expose_tool("list_friends", &caps));
        assert!(!should_expose_tool("list_dms", &caps));
        assert!(should_expose_tool("get_messages", &caps));
        assert!(should_expose_tool("view_rows", &caps));
    }

    #[test]
    fn discord_is_full_social_chat_shape() {
        let caps = poly_client::capabilities_for_slug_static("discord");
        assert!(should_expose_tool("send_message", &caps));
        assert!(should_expose_tool("list_friends", &caps));
        assert!(should_expose_tool("list_dms", &caps));
    }

    #[test]
    fn unknown_tool_name_not_exposed() {
        let caps = poly_client::capabilities_for_slug_static("discord");
        assert!(!should_expose_tool("not_a_real_tool", &caps));
        assert!(!should_expose_tool("", &caps));
    }

    #[test]
    fn send_typing_gated_on_supports_typing_indicators() {
        // Backends with typing support should expose the tool.
        for slug in ["discord", "matrix", "stoat", "poly", "demo"] {
            let caps = poly_client::capabilities_for_slug_static(slug);
            assert!(
                should_expose_tool("send_typing", &caps),
                "send_typing should be exposed on backend '{slug}'"
            );
        }
        // Backends without typing support must not expose the tool.
        for slug in ["hackernews", "lemmy", "teams", "github"] {
            let caps = poly_client::capabilities_for_slug_static(slug);
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

    #[test]
    fn memory_tools_in_tool_list() {
        let list = tool_list();
        let names = tool_names(&list);
        for t in [
            "remember_fact", "recall_facts", "forget_fact", "search_facts",
            "store_chat_note", "get_chat_notes", "forget_chat_note",
            "store_chat_summary", "get_chat_summary", "get_reply_context",
        ] {
            assert!(names.contains(t), "'{t}' missing from tool_list()");
        }
    }

    #[test]
    fn draft_tools_in_tool_list_and_always_exposed() {
        let list = tool_list();
        let names = tool_names(&list);
        let draft_tools = [
            "draft_create", "draft_list", "draft_approve",
            "draft_edit", "draft_discard", "draft_cancel_autosend",
        ];
        for t in draft_tools {
            assert!(names.contains(t), "'{t}' missing from tool_list()");
        }
        // Draft tools must be exposed on every backend.
        for slug in KNOWN_SLUGS {
            let caps = poly_client::capabilities_for_slug_static(slug);
            for t in draft_tools {
                assert!(
                    should_expose_tool(t, &caps),
                    "'{t}' should be exposed on backend '{slug}'"
                );
            }
        }
    }

    #[test]
    fn style_tools_in_tool_list() {
        let list = tool_list();
        let names = tool_names(&list);
        for t in ["set_chat_style", "get_chat_style", "list_chat_styles", "forget_chat_style"] {
            assert!(names.contains(t), "'{t}' missing from tool_list()");
        }
    }

    #[test]
    fn style_tools_exposed_on_every_backend() {
        for slug in KNOWN_SLUGS {
            let caps = poly_client::capabilities_for_slug_static(slug);
            for t in ["set_chat_style", "get_chat_style", "list_chat_styles", "forget_chat_style"] {
                assert!(
                    should_expose_tool(t, &caps),
                    "'{t}' not exposed for slug '{slug}'"
                );
            }
        }
    }

    // ── B.8 — meta-persona capability tests ──────────────────────────────────

    const META_PERSONA_TOOLS: &[&str] = &[
        "meta_persona_list",
        "meta_persona_get",
        "meta_persona_create",
        "meta_persona_update",
        "meta_persona_delete",
        "meta_persona_set_sources",
        "meta_persona_set_tool_whitelist",
        "meta_persona_invoke",
        "meta_persona_set_heartbeat",
        "meta_persona_get_memory",
        "meta_persona_set_memory",
        "meta_persona_forget_memory",
        "meta_persona_recent_actions",
        "meta_persona_set_outbound_allow",
    ];

    #[test]
    fn meta_persona_tools_in_tool_list() {
        let list = tool_list();
        let names = tool_names(&list);
        for t in META_PERSONA_TOOLS {
            assert!(names.contains(*t), "'{t}' missing from tool_list()");
        }
    }

    #[test]
    fn meta_persona_tools_always_exposed_on_every_backend() {
        for slug in KNOWN_SLUGS {
            let caps = poly_client::capabilities_for_slug_static(slug);
            for t in META_PERSONA_TOOLS {
                assert!(
                    should_expose_tool(t, &caps),
                    "'{t}' should be exposed on backend '{slug}'"
                );
            }
        }
    }

    // ── B.7 — integration tests (direct dispatch against in-memory DB) ────────

    fn fresh_mem() -> crate::memory::MemoryDb {
        crate::memory::MemoryDb::open(":memory:").expect("in-memory db")
    }

    #[allow(clippy::needless_pass_by_value)] // args constructed inline by test callers
    fn dispatch_sync(tool: &str, args: serde_json::Value, mem: &crate::memory::MemoryDb) -> Value {
        // Spin up a minimal Tokio runtime so we can call the async dispatch.
        // The persona handlers are all sync but the top-level dispatch is async.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("tokio runtime");
        let mut pool = crate::state::BackendPool::new();
        rt.block_on(super::dispatch(tool, &args, &mut pool, mem))
    }

    #[test]
    fn integration_create_list_get() {
        let mem = fresh_mem();

        // Create
        let r = dispatch_sync("meta_persona_create", json!({
            "slug": "test-bob",
            "name": "Test Bob",
            "system_prompt": "You are Test Bob."
        }), &mem);
        assert_eq!(r["isError"], false, "create failed: {r}");
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("test-bob"), "slug not in response: {text}");

        // List — should find 1 persona
        let r = dispatch_sync("meta_persona_list", json!({}), &mem);
        assert_eq!(r["isError"], false);
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("test-bob"));

        // Get
        let r = dispatch_sync("meta_persona_get", json!({"slug": "test-bob"}), &mem);
        assert_eq!(r["isError"], false);
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("You are Test Bob."));
    }

    #[test]
    fn integration_update_and_delete() {
        let mem = fresh_mem();
        dispatch_sync("meta_persona_create", json!({
            "slug": "del-me",
            "name": "Del Me",
            "system_prompt": "temp"
        }), &mem);

        // Update name
        let r = dispatch_sync("meta_persona_update", json!({
            "slug": "del-me",
            "name": "Del Me Renamed"
        }), &mem);
        assert_eq!(r["isError"], false);

        let persona = mem.get_persona("del-me").unwrap().unwrap();
        assert_eq!(persona["name"], "Del Me Renamed");

        // Delete
        let r = dispatch_sync("meta_persona_delete", json!({"slug": "del-me"}), &mem);
        assert_eq!(r["isError"], false);
        assert!(mem.get_persona("del-me").unwrap().is_none());
    }

    #[test]
    fn integration_set_sources_atomic_replace() {
        let mem = fresh_mem();
        dispatch_sync("meta_persona_create", json!({
            "slug": "src-test",
            "name": "Src Test",
            "system_prompt": "test"
        }), &mem);

        // Set two sources
        let r = dispatch_sync("meta_persona_set_sources", json!({
            "slug": "src-test",
            "sources": [
                {"account_id": "acc1", "selector_kind": "server", "selector_value": "srv1"},
                {"account_id": "acc1", "selector_kind": "channel", "selector_value": "ch1", "include": false}
            ]
        }), &mem);
        assert_eq!(r["isError"], false);
        assert_eq!(mem.list_persona_sources("src-test").unwrap().len(), 2);

        // Atomic replace with one source
        dispatch_sync("meta_persona_set_sources", json!({
            "slug": "src-test",
            "sources": [
                {"account_id": "acc2", "selector_kind": "all"}
            ]
        }), &mem);
        let sources = mem.list_persona_sources("src-test").unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["account_id"], "acc2");
    }

    #[test]
    fn integration_memory_set_get_forget() {
        let mem = fresh_mem();
        dispatch_sync("meta_persona_create", json!({
            "slug": "mem-test",
            "name": "Mem Test",
            "system_prompt": "test"
        }), &mem);

        // Set a fact
        let r = dispatch_sync("meta_persona_set_memory", json!({
            "slug": "mem-test",
            "fact_text": "User prefers morning meetings",
            "category": "preference",
            "pinned": true
        }), &mem);
        assert_eq!(r["isError"], false);
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("fact_id"));

        // Get memory
        let r = dispatch_sync("meta_persona_get_memory", json!({
            "slug": "mem-test",
            "pinned_only": true
        }), &mem);
        assert_eq!(r["isError"], false);
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("morning meetings"));

        // Forget all
        let r = dispatch_sync("meta_persona_forget_memory", json!({
            "slug": "mem-test",
            "forget_all": true
        }), &mem);
        assert_eq!(r["isError"], false);
        assert!(mem.list_persona_facts("mem-test", false).unwrap().is_empty());
    }

    #[test]
    fn integration_invoke_returns_bundle_v1() {
        let mem = fresh_mem();
        dispatch_sync("meta_persona_create", json!({
            "slug": "invoke-test",
            "name": "Invoke Test",
            "system_prompt": "You are an invoice test persona."
        }), &mem);

        // Add a pinned fact
        mem.add_persona_fact("invoke-test", Some("test"), "Important pinned fact", true).unwrap();

        let r = dispatch_sync("meta_persona_invoke", json!({
            "slug": "invoke-test",
            "user_prompt": "tell me what's up"
        }), &mem);
        assert_eq!(r["isError"], false);
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("bundle_version"), "bundle_version missing: {text}");
        // Phase C: bundle_version is now "v1".
        assert!(text.contains("v1"), "bundle v1 marker missing: {text}");
        assert!(text.contains("invoke-test"), "slug missing: {text}");
        assert!(text.contains("pinned_facts"), "pinned_facts missing: {text}");
        // With an empty BackendPool no backends are connected, so chats is empty.
        assert!(text.contains("\"chats\""), "chats field missing: {text}");

        // Audit row should have been written
        let audit_rows = mem.list_persona_audit("invoke-test", 10).unwrap();
        assert!(!audit_rows.is_empty(), "no audit row written");
    }

    #[test]
    fn integration_invoke_disabled_persona_denied() {
        let mem = fresh_mem();
        dispatch_sync("meta_persona_create", json!({
            "slug": "disabled-persona",
            "name": "Disabled",
            "system_prompt": "test"
        }), &mem);
        dispatch_sync("meta_persona_update", json!({
            "slug": "disabled-persona",
            "enabled": false
        }), &mem);

        let r = dispatch_sync("meta_persona_invoke", json!({"slug": "disabled-persona"}), &mem);
        assert_eq!(r["isError"], true);
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("disabled"), "expected disabled error: {text}");
    }

    #[test]
    fn integration_set_heartbeat() {
        let mem = fresh_mem();
        dispatch_sync("meta_persona_create", json!({
            "slug": "hb-test",
            "name": "HB Test",
            "system_prompt": "test"
        }), &mem);

        // Set heartbeat interval
        let r = dispatch_sync("meta_persona_set_heartbeat", json!({
            "slug": "hb-test",
            "interval_secs": 3600_i64
        }), &mem);
        assert_eq!(r["isError"], false);
        let p = mem.get_persona("hb-test").unwrap().unwrap();
        assert_eq!(p["heartbeat_interval_secs"], 3600_i64);

        // Clear heartbeat
        let r = dispatch_sync("meta_persona_set_heartbeat", json!({
            "slug": "hb-test",
            "interval_secs": null
        }), &mem);
        assert_eq!(r["isError"], false);
        let p = mem.get_persona("hb-test").unwrap().unwrap();
        assert!(p["heartbeat_interval_secs"].is_null());
    }

    #[test]
    fn integration_outbound_allow_set_and_remove() {
        let mem = fresh_mem();
        dispatch_sync("meta_persona_create", json!({
            "slug": "ob-test",
            "name": "OB Test",
            "system_prompt": "test"
        }), &mem);

        // Set allowlist entry
        let r = dispatch_sync("meta_persona_set_outbound_allow", json!({
            "slug": "ob-test",
            "account_id": "acc1",
            "chat_id": "ch1",
            "max_messages_per_day": 2_i64
        }), &mem);
        assert_eq!(r["isError"], false);
        let entries = mem.list_persona_outbound_allows("ob-test").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["max_messages_per_day"], 2_i64);

        // Remove it
        let r = dispatch_sync("meta_persona_set_outbound_allow", json!({
            "slug": "ob-test",
            "account_id": "acc1",
            "chat_id": "ch1",
            "remove": true
        }), &mem);
        assert_eq!(r["isError"], false);
        assert!(mem.list_persona_outbound_allows("ob-test").unwrap().is_empty());
    }

    #[test]
    fn integration_recent_actions_audit_trail() {
        let mem = fresh_mem();
        dispatch_sync("meta_persona_create", json!({
            "slug": "audit-test",
            "name": "Audit Test",
            "system_prompt": "test"
        }), &mem);
        dispatch_sync("meta_persona_invoke", json!({"slug": "audit-test"}), &mem);
        dispatch_sync("meta_persona_set_memory", json!({
            "slug": "audit-test",
            "fact_text": "A memory"
        }), &mem);

        let r = dispatch_sync("meta_persona_recent_actions", json!({
            "slug": "audit-test",
            "limit": 10_i32
        }), &mem);
        assert_eq!(r["isError"], false);
        let text = r["content"][0]["text"].as_str().unwrap();
        // Should have at least the invoke + memory_write rows
        assert!(text.contains("invoke") || text.contains("memory"), "no audit rows: {text}");
    }
}

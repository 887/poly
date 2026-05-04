//! MCP tool definitions and dispatch.

use crate::events::{Subscription, new_subscription_id, parse_opt_event_kinds, parse_opt_string_vec};
use crate::memory::MemoryDb;
use crate::state::BackendPool;
use serde_json::{Value, json};
use poly_client::{
    AuthCredentials, BackendCapabilities, BackendType, ClientBackend, Cursor, CursorKind,
    DmSupport, FriendModel, MenuTargetKind, MessageContent, MessageQuery, MessagingModel,
    NotificationSupport, PluginManifest, SettingsScope, VoiceSupport,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

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

// ─── Tool list ───────────────────────────────────────────────────────────────

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

// ─── Dispatch ────────────────────────────────────────────────────────────────

pub async fn dispatch(tool: &str, args: &Value, pool: &mut BackendPool, mem: &MemoryDb) -> Value {
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

        // Phase A — memory tools (A.7: always exposed, backend-agnostic).
        "forget_chat_note"  => handle_forget_chat_note(args, mem),
        "forget_fact"       => handle_forget_fact(args, mem),
        "get_chat_notes"    => handle_get_chat_notes(args, mem),
        "get_chat_summary"  => handle_get_chat_summary(args, mem),
        "get_reply_context" => handle_get_reply_context(args, pool, mem).await,
        "recall_facts"      => handle_recall_facts(args, mem),
        "remember_fact"     => handle_remember_fact(args, mem),
        "search_facts"      => handle_search_facts(args, mem),
        "store_chat_note"   => handle_store_chat_note(args, mem),
        "store_chat_summary" => handle_store_chat_summary(args, mem),

        // Phase B — draft queue tools.
        "draft_create"        => handle_draft_create(args, pool, mem).await,
        "draft_list"          => handle_draft_list(args, mem),
        "draft_approve"       => handle_draft_approve(args, pool, mem).await,
        "draft_edit"          => handle_draft_edit(args, mem),
        "draft_discard"       => handle_draft_discard(args, mem),
        "draft_cancel_autosend" => handle_draft_cancel_autosend(args, mem),

        // Phase E — per-chat style tools.
        "set_chat_style"    => handle_set_chat_style(args, mem),
        "get_chat_style"    => handle_get_chat_style(args, mem),
        "list_chat_styles"  => handle_list_chat_styles(args, mem),
        "forget_chat_style" => handle_forget_chat_style(args, mem),

        // Phase B (meta-personas) — always exposed.
        "meta_persona_list"          => handle_meta_persona_list(mem),
        "meta_persona_get"           => handle_meta_persona_get(args, mem),
        "meta_persona_create"        => handle_meta_persona_create(args, mem),
        "meta_persona_update"        => handle_meta_persona_update(args, mem),
        "meta_persona_delete"        => handle_meta_persona_delete(args, mem),
        "meta_persona_set_sources"   => handle_meta_persona_set_sources(args, mem),
        "meta_persona_set_tool_whitelist" => handle_meta_persona_set_tool_whitelist(args, mem),
        "meta_persona_invoke"        => handle_meta_persona_invoke(args, pool, mem).await,
        "meta_persona_set_heartbeat" => handle_meta_persona_set_heartbeat(args, mem),
        "meta_persona_get_memory"    => handle_meta_persona_get_memory(args, mem),
        "meta_persona_set_memory"    => handle_meta_persona_set_memory(args, mem),
        "meta_persona_forget_memory" => handle_meta_persona_forget_memory(args, mem),
        "meta_persona_recent_actions" => handle_meta_persona_recent_actions(args, mem),
        "meta_persona_set_outbound_allow" => handle_meta_persona_set_outbound_allow(args, mem),
        "meta_persona_audit_query"    => handle_meta_persona_audit_query(args, mem),
        "meta_persona_audit_export"   => handle_meta_persona_audit_export(args, mem),

        // Phase D (client-version plan) — client settings tools (always exposed).
        "client_settings_list"               => handle_client_settings_list(args, pool, mem).await,
        "client_settings_get_version"        => handle_client_settings_get_version(args, pool, mem).await,
        "client_settings_set_version_override" => handle_client_settings_set_version_override(args, pool, mem).await,
        "client_settings_list_mechanisms"    => handle_client_settings_list_mechanisms(args, pool, mem).await,
        "client_settings_set_mechanism"      => handle_client_settings_set_mechanism(args, pool, mem).await,

        // Phase C — added concurrently, rebase-safe insertion
        "poll_events" => handle_poll_events(args, pool).await,
        "subscribe_events" => handle_subscribe_events(args, pool).await,
        "unsubscribe_events" => handle_unsubscribe_events(args, pool).await,

        // Phase D — typing simulation, fully wired.
        "start_typing_simulation" => handle_start_typing_simulation(args, pool).await,
        "stop_typing_simulation" => handle_stop_typing_simulation(args, pool).await,

        // Phase F — get_unread_summary bundles unread messages across all
        // chats for an account so Claude Desktop can compose a digest.
        "get_unread_summary" => handle_get_unread_summary(args, pool).await,

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
    match pool.remove(&bt, account_id) {
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
        pool.get(&bt, account_id)
            .ok_or_else(|| err_result(format!("no session for {backend_str}:{account_id}")))
    } else {
        pool.find_by_type(&bt)
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

async fn handle_send_typing(args: &Value, pool: &BackendPool) -> Value {
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

// ─── List compiled-in plugins ────────────────────────────────────────────────

/// Snapshot one plugin's identity + declared manifest.
// poly-lint: manifest fields are moved into json! by value.
#[allow(clippy::needless_pass_by_value)]
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

// ─── Phase A — Memory tool handlers ─────────────────────────────────────────

fn handle_remember_fact(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let contact_id = match str_arg(args, "contact_id") { Some(v) => v, None => return err_result("missing 'contact_id'") };
    let category   = str_arg(args, "category").unwrap_or("");
    let fact       = match str_arg(args, "fact") { Some(v) => v, None => return err_result("missing 'fact'") };
    match mem.remember_fact(account_id, contact_id, category, fact) {
        Ok(id) => ok_result(json!({ "fact_id": id }).to_string()),
        Err(e) => err_result(format!("remember_fact failed: {e}")),
    }
}

fn handle_recall_facts(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let contact_id = match str_arg(args, "contact_id") { Some(v) => v, None => return err_result("missing 'contact_id'") };
    let category   = str_arg(args, "category");
    match mem.recall_facts(account_id, contact_id, category) {
        Ok(facts) => ok_result(serde_json::to_string_pretty(&facts).unwrap_or_default()),
        Err(e) => err_result(format!("recall_facts failed: {e}")),
    }
}

fn handle_forget_fact(args: &Value, mem: &MemoryDb) -> Value {
    let fact_id = match args.get("fact_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None => return err_result("missing or invalid 'fact_id' (must be integer)"),
    };
    match mem.forget_fact(fact_id) {
        Ok(()) => ok_result("fact deleted"),
        Err(e) => err_result(format!("forget_fact failed: {e}")),
    }
}

fn handle_search_facts(args: &Value, mem: &MemoryDb) -> Value {
    let query      = match str_arg(args, "query") { Some(v) => v, None => return err_result("missing 'query'") };
    let account_id = str_arg(args, "account_id");
    match mem.search_facts(query, account_id) {
        Ok(facts) => ok_result(serde_json::to_string_pretty(&facts).unwrap_or_default()),
        Err(e) => err_result(format!("search_facts failed: {e}")),
    }
}

fn handle_store_chat_note(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let note       = match str_arg(args, "note")       { Some(v) => v, None => return err_result("missing 'note'") };
    match mem.store_chat_note(account_id, chat_id, note) {
        Ok(id) => ok_result(json!({ "note_id": id }).to_string()),
        Err(e) => err_result(format!("store_chat_note failed: {e}")),
    }
}

fn handle_get_chat_notes(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    match mem.get_chat_notes(account_id, chat_id) {
        Ok(notes) => ok_result(serde_json::to_string_pretty(&notes).unwrap_or_default()),
        Err(e) => err_result(format!("get_chat_notes failed: {e}")),
    }
}

fn handle_forget_chat_note(args: &Value, mem: &MemoryDb) -> Value {
    let note_id = match args.get("note_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None => return err_result("missing or invalid 'note_id' (must be integer)"),
    };
    match mem.forget_chat_note(note_id) {
        Ok(()) => ok_result("note deleted"),
        Err(e) => err_result(format!("forget_chat_note failed: {e}")),
    }
}

fn handle_store_chat_summary(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let summary    = match str_arg(args, "summary")    { Some(v) => v, None => return err_result("missing 'summary'") };
    let window_start = str_arg(args, "window_start_msg_id").unwrap_or("");
    let window_end   = str_arg(args, "window_end_msg_id").unwrap_or("");
    match mem.store_chat_summary(account_id, chat_id, summary, window_start, window_end) {
        Ok(()) => ok_result("summary stored"),
        Err(e) => err_result(format!("store_chat_summary failed: {e}")),
    }
}

fn handle_get_chat_summary(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    match mem.get_chat_summary(account_id, chat_id) {
        Ok(Some(s)) => ok_result(serde_json::to_string_pretty(&s).unwrap_or_default()),
        Ok(None)    => ok_result("null"),
        Err(e)      => err_result(format!("get_chat_summary failed: {e}")),
    }
}

// ─── Phase A.3 — Context bundler ─────────────────────────────────────────────

/// Build the fat reply-context bundle that gives Claude Desktop everything it
/// needs to draft a contextually-aware reply in a single MCP call.
async fn handle_get_reply_context(args: &Value, pool: &BackendPool, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let message_limit = u32::try_from(args.get("message_limit").and_then(serde_json::Value::as_u64).unwrap_or(20)).unwrap_or(u32::MAX);
    let contact_id = str_arg(args, "contact_id");

    // Find the backend for this account.
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };

    // Section: account info.
    let account_section = json!({
        "id":           entry.session.user.id,
        "backend":      format!("{:?}", entry.session.backend),
        "display_name": entry.session.user.display_name,
    });

    // Section: recent messages (best-effort; null on error).
    let recent_messages: Value = match entry
        .backend
        .get_messages(
            chat_id,
            poly_client::MessageQuery {
                limit: Some(message_limit),
                ..Default::default()
            },
        )
        .await
    {
        Ok(msgs) => serde_json::to_value(&msgs).unwrap_or(json!([])),
        Err(_) => json!([]),
    };

    // Section: contact info + facts (null if no contact_id supplied or lookup fails).
    let contact_section: Value = if let Some(cid) = contact_id {
        let user_info: Option<Value> = match entry.backend.get_user(cid).await {
            Ok(u) => serde_json::to_value(&u).ok(),
            Err(_) => None,
        };
        let facts = mem.recall_facts(account_id, cid, None).unwrap_or_default();
        let mut obj = serde_json::Map::new();
        obj.insert("id".to_string(), json!(cid));
        if let Some(u) = user_info {
            obj.insert("display_name".to_string(), u.get("display_name").cloned().unwrap_or(json!(null)));
            obj.insert("presence".to_string(), u.get("presence").cloned().unwrap_or(json!(null)));
            obj.insert("last_seen".to_string(), u.get("last_seen").cloned().unwrap_or(json!(null)));
        }
        obj.insert("facts".to_string(), json!(facts));
        json!(obj)
    } else {
        json!(null)
    };

    // Section: chat notes.
    let chat_notes: Value = mem
        .get_chat_notes(account_id, chat_id)
        .map(|n| json!(n))
        .unwrap_or(json!([]));

    // Section: chat summary.
    let chat_summary: Value = mem
        .get_chat_summary(account_id, chat_id)
        .ok()
        .flatten()
        .unwrap_or(json!(null));

    // Section: per-chat style (Phase E).
    let chat_style: Value = mem
        .get_chat_style(account_id, chat_id)
        .ok()
        .flatten()
        .unwrap_or(json!(null));

    let bundle = json!({
        "account":         account_section,
        "chat":            { "id": chat_id },
        "recent_messages": recent_messages,
        "contact":         contact_section,
        "chat_notes":      chat_notes,
        "chat_summary":    chat_summary,
        "style":           chat_style,
    });

    ok_result(serde_json::to_string_pretty(&bundle).unwrap_or_default())
}

// ─── Phase B — Draft queue handlers ──────────────────────────────────────────

/// Helper: compute ISO-8601 UTC timestamp for `now + secs`.
// poly-lint: textbook Gregorian-calendar arithmetic on u64 timestamp.
#[allow(clippy::arithmetic_side_effects, clippy::integer_division)]
fn future_iso8601(secs: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let total = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_add(secs);
    let s = total % 60;
    let m = (total / 60) % 60;
    let h = (total / 3600) % 24;
    let days = total / 86400;

    // Reuse the Gregorian calendar helper from memory.rs via a local copy.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

async fn handle_draft_create(args: &Value, pool: &BackendPool, mem: &MemoryDb) -> Value {
    let account_id   = match str_arg(args, "account_id")   { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id      = match str_arg(args, "chat_id")      { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let body         = match str_arg(args, "body")         { Some(v) => v, None => return err_result("missing 'body'") };
    let suggested_by = match str_arg(args, "suggested_by") { Some(v) => v, None => return err_result("missing 'suggested_by'") };

    // Sanitize body: trim leading/trailing whitespace; reject empty.
    let body = body.trim();
    if body.is_empty() {
        return err_result("draft body must not be empty");
    }

    // Per-chat auto-approve KV key: "agent.chat.{account_id}.{chat_id}.auto_approve_enabled"
    // We check a synthetic pool-level setting. Since pool has no KV store itself,
    // the auto-send feature is gated on the caller explicitly passing auto_send_in_secs
    // AND the backend being writable (as a safety proxy).
    let auto_send_in_secs = args.get("auto_send_in_secs").and_then(serde_json::Value::as_u64);

    // Only honour auto_send_in_secs when the backend is writable (sanity gate).
    let auto_send_at: Option<String> = if let Some(secs) = auto_send_in_secs {
        let is_writable = pool.find_by_account(account_id)
            .is_some_and(|e| {
                let caps = poly_client::capabilities_for_slug_static(
                    &format!("{:?}", e.session.backend)
                );
                caps.composer_writable()
            });
        if is_writable {
            Some(future_iso8601(secs))
        } else {
            None
        }
    } else {
        None
    };

    match mem.draft_insert(account_id, chat_id, body, suggested_by, auto_send_at.as_deref()) {
        Ok(id) => ok_result(format!("{{\"draft_id\":{id}}}")),
        Err(e) => err_result(format!("draft_create failed: {e}")),
    }
}

fn handle_draft_list(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = str_arg(args, "account_id");
    let chat_id    = str_arg(args, "chat_id");
    let status     = str_arg(args, "status");

    match mem.draft_list(account_id, chat_id, status) {
        Ok(drafts) => ok_result(serde_json::to_string_pretty(&drafts).unwrap_or_default()),
        Err(e)     => err_result(format!("draft_list failed: {e}")),
    }
}

async fn handle_draft_approve(args: &Value, pool: &BackendPool, mem: &MemoryDb) -> Value {
    let draft_id = match args.get("draft_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None     => return err_result("missing 'draft_id'"),
    };

    // Fetch the draft.
    let draft = match mem.draft_get(draft_id) {
        Ok(Some(d)) => d,
        Ok(None)    => return err_result(format!("draft {draft_id} not found")),
        Err(e)      => return err_result(format!("draft_approve failed: {e}")),
    };

    let status = draft.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status != "pending" {
        return err_result(format!("draft {draft_id} has status={status}, must be pending to approve"));
    }

    let account_id = draft.get("account_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let chat_id    = draft.get("chat_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let body       = draft.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // Send via the active backend.
    // Verify the backend exists before attempting send.
    let entry = match pool.find_by_account(&account_id) {
        Some(e) => e,
        None    => return err_result(format!("no backend found for account_id={account_id}")),
    };

    match entry.backend.send_message(&chat_id, MessageContent::Text(body)).await {
        Ok(_) => {
            if let Err(e) = mem.draft_set_status(draft_id, "sent") {
                return err_result(format!("message sent but failed to update draft status: {e}"));
            }
            ok_result(format!("draft {draft_id} sent and status updated to sent"))
        }
        Err(e) => {
            drop(mem.draft_set_status(draft_id, "expired"));
            err_result(format!("send_message failed: {e}; draft marked expired"))
        }
    }
}

fn handle_draft_edit(args: &Value, mem: &MemoryDb) -> Value {
    let draft_id = match args.get("draft_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None     => return err_result("missing 'draft_id'"),
    };
    let new_body = match str_arg(args, "new_body") {
        Some(b) => b.trim(),
        None    => return err_result("missing 'new_body'"),
    };
    if new_body.is_empty() {
        return err_result("new_body must not be empty");
    }

    match mem.draft_edit(draft_id, new_body) {
        Ok(true)  => ok_result(format!("draft {draft_id} body updated")),
        Ok(false) => err_result(format!("draft {draft_id} not found or not in pending status")),
        Err(e)    => err_result(format!("draft_edit failed: {e}")),
    }
}

fn handle_draft_discard(args: &Value, mem: &MemoryDb) -> Value {
    let draft_id = match args.get("draft_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None     => return err_result("missing 'draft_id'"),
    };

    match mem.draft_set_status(draft_id, "discarded") {
        Ok(())  => ok_result(format!("draft {draft_id} discarded")),
        Err(e)  => err_result(format!("draft_discard failed: {e}")),
    }
}

fn handle_draft_cancel_autosend(args: &Value, mem: &MemoryDb) -> Value {
    let draft_id = match args.get("draft_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None     => return err_result("missing 'draft_id'"),
    };

    match mem.draft_clear_autosend(draft_id) {
        Ok(())  => ok_result(format!("auto-send cancelled for draft {draft_id}")),
        Err(e)  => err_result(format!("draft_cancel_autosend failed: {e}")),
    }
}

// ─── Phase E — Chat style handlers ───────────────────────────────────────────

fn handle_set_chat_style(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let tone          = str_arg(args, "tone");
    let formality     = str_arg(args, "formality");
    let emoji_allowed = args.get("emoji_allowed").and_then(serde_json::Value::as_bool);
    let signature     = str_arg(args, "signature");
    let extra_notes   = str_arg(args, "extra_notes");
    match mem.set_chat_style(account_id, chat_id, tone, formality, emoji_allowed, signature, extra_notes) {
        Ok(()) => ok_result("style saved"),
        Err(e) => err_result(format!("set_chat_style failed: {e}")),
    }
}

fn handle_get_chat_style(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    match mem.get_chat_style(account_id, chat_id) {
        Ok(maybe) => ok_result(serde_json::to_string_pretty(&maybe).unwrap_or_default()),
        Err(e) => err_result(format!("get_chat_style failed: {e}")),
    }
}

fn handle_list_chat_styles(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = str_arg(args, "account_id");
    match mem.list_chat_styles(account_id) {
        Ok(list) => ok_result(serde_json::to_string_pretty(&list).unwrap_or_default()),
        Err(e) => err_result(format!("list_chat_styles failed: {e}")),
    }
}

fn handle_forget_chat_style(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    match mem.forget_chat_style(account_id, chat_id) {
        Ok(()) => ok_result("style deleted"),
        Err(e) => err_result(format!("forget_chat_style failed: {e}")),
    }
}

// ─── Phase C — event subscription / poll handlers ────────────────────────────

async fn handle_subscribe_events(args: &Value, pool: &BackendPool) -> Value {
    let account_ids = parse_opt_string_vec(args, "account_ids");
    let chat_ids = parse_opt_string_vec(args, "chat_ids");
    let event_types = parse_opt_event_kinds(args, "event_types");

    let id = new_subscription_id();
    let sub = Subscription {
        id: id.clone(),
        account_ids,
        chat_ids,
        event_types,
    };

    pool.events.lock().await.add_subscription(sub);

    ok_result(serde_json::to_string_pretty(&json!({
        "subscription_id": id,
        "note": "Use poll_events with this subscription_id to retrieve matching events."
    })).unwrap_or_default())
}

async fn handle_unsubscribe_events(args: &Value, pool: &BackendPool) -> Value {
    let sub_id = match args.get("subscription_id").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_result("missing 'subscription_id'"),
    };
    pool.events.lock().await.remove_subscription(sub_id);
    ok_result(format!("subscription {sub_id} removed"))
}

/// Maximum events returned per poll call.
const MAX_POLL_LIMIT: usize = 500;
const DEFAULT_POLL_LIMIT: usize = 100;

async fn handle_poll_events(args: &Value, pool: &BackendPool) -> Value {
    let since_ms = args
        .get("since_ms")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let limit = args
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .map_or(DEFAULT_POLL_LIMIT, |n| usize::try_from(n).unwrap_or(usize::MAX))
        .min(MAX_POLL_LIMIT);

    let store = pool.events.lock().await;

    let events = if let Some(sub_id) = args.get("subscription_id").and_then(|v| v.as_str()) {
        match store.poll(sub_id, since_ms, limit) {
            Ok(evs) => evs,
            Err(e) => return err_result(e),
        }
    } else {
        let account_ids = parse_opt_string_vec(args, "account_ids");
        let chat_ids = parse_opt_string_vec(args, "chat_ids");
        let event_types = parse_opt_event_kinds(args, "event_types");
        store.poll_adhoc(
            account_ids.as_deref(),
            chat_ids.as_deref(),
            event_types.as_deref(),
            since_ms,
            limit,
        )
    };

    let next_since_ms = events.iter().map(|e| e.seq_ms).max().unwrap_or(since_ms);

    ok_result(serde_json::to_string_pretty(&json!({
        "events": events,
        "count": events.len(),
        "next_since_ms": next_since_ms,
    })).unwrap_or_default())
}

/// Phase D — Start a typing-simulation worker. Clones the backend Arc,
/// spawns the rhythm loop, and registers the sim in the pool's registry.
async fn handle_start_typing_simulation(args: &Value, pool: &BackendPool) -> Value {
    let account_id = match str_arg(args, "account_id") {
        Some(v) => v,
        None => return err_result("missing 'account_id'"),
    };
    let chat_id = match str_arg(args, "chat_id") {
        Some(v) => v,
        None => return err_result("missing 'chat_id'"),
    };

    // Find the backend for this account. Cloning the Arc gives the worker
    // an independent handle for the lifetime of the simulation.
    let entry = match pool.find_by_account(account_id) {
        Some(e) => e,
        None => return err_result(format!("no backend for account '{account_id}'")),
    };
    if !entry.backend.backend_capabilities().supports_typing_indicators {
        return err_result("backend does not support typing indicators");
    }
    let backend_arc = entry.backend.clone();

    // poly-lint: probabilities are f64→f32 by API contract; truncation is acceptable in [0,1] range.
    #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
    let params = crate::typing_simulation::SimParams::clamped(
        u32::try_from(args.get("total_duration_ms").and_then(Value::as_u64).unwrap_or(8_000)).unwrap_or(u32::MAX),
        u16::try_from(args.get("avg_wpm").and_then(Value::as_u64).unwrap_or(60)).unwrap_or(u16::MAX),
        args.get("false_start_probability").and_then(Value::as_f64).unwrap_or(0.05_f64) as f32,
        args.get("pause_probability").and_then(Value::as_f64).unwrap_or(0.10_f64) as f32,
        args.get("stop_on_other_typing").and_then(Value::as_bool).unwrap_or(false),
    );

    // Seed the RNG from the current system clock so simulations feel fresh
    // between invocations. Unit tests use fixed seeds via
    // `next_tick_decision` directly, not this path.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0xCAFE_u64);

    let stop_on_other_typing = params.stop_on_other_typing;
    let (abort_tx, abort_rx) = tokio::sync::oneshot::channel();
    let handle = crate::typing_simulation::spawn_worker(
        backend_arc,
        chat_id.to_string(),
        params,
        seed,
        abort_rx,
    );

    let mut registry = pool.sim_registry.lock().await;
    let sim_id = match registry.start(account_id, chat_id, handle, abort_tx) {
        Ok(id) => id,
        Err(e) => return err_result(e),
    };
    drop(registry);

    // Phase D ↔ Phase C bridge — when stop_on_other_typing is true, watch the
    // event broadcast for a TypingStarted on this channel and abort the
    // simulation by removing it from the registry (which drops abort_tx +
    // aborts the JoinHandle).
    if stop_on_other_typing {
        let registry = pool.sim_registry.clone();
        let mut events_rx = pool.events.lock().await.subscribe_broadcast();
        let watch_chat_id = chat_id.to_string();
        let watch_sim_id = sim_id.clone();
        tokio::spawn(async move {
            use crate::events::EventKind;
            while let Ok(event) = events_rx.recv().await {
                if event.kind != EventKind::TypingStarted {
                    continue;
                }
                if event.channel_id.as_deref() != Some(watch_chat_id.as_str()) {
                    continue;
                }
                let mut reg = registry.lock().await;
                reg.stop(&watch_sim_id);
                break;
            }
        });
    }

    ok_result(
        serde_json::to_string_pretty(&json!({
            "simulation_id": sim_id,
            "account_id": account_id,
            "chat_id": chat_id,
        }))
        .unwrap_or_default(),
    )
}

/// Phase D — Stop an in-flight simulation. Returns `found: true` if the
/// id matched; `false` if the simulation had already expired naturally.
async fn handle_stop_typing_simulation(args: &Value, pool: &BackendPool) -> Value {
    let sim_id = match str_arg(args, "simulation_id") {
        Some(v) => v,
        None => return err_result("missing 'simulation_id'"),
    };
    let mut registry = pool.sim_registry.lock().await;
    let found = registry.stop(sim_id);
    ok_result(
        serde_json::to_string_pretty(&json!({
            "simulation_id": sim_id,
            "found": found,
        }))
        .unwrap_or_default(),
    )
}

/// Phase F — Bundle recent activity across every chat for an account, ordered
/// by most-recent-first, so Claude Desktop can compose a "catch me up" digest
/// in one MCP round-trip. Stays LLM-free — the bundler just returns structured
/// context; the summary generation happens Claude-side.
async fn handle_get_unread_summary(args: &Value, pool: &BackendPool) -> Value {
    let account_id = match str_arg(args, "account_id") {
        Some(v) => v,
        None => return err_result("missing 'account_id'"),
    };
    let message_limit = u32::try_from(args
        .get("message_limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(10)).unwrap_or(u32::MAX);

    let entry = match pool.find_by_account(account_id) {
        Some(e) => e,
        None => return err_result(format!("no backend for account '{account_id}'")),
    };

    // Gather servers + channels, pull recent messages from each channel with
    // unread_count > 0. Best-effort; skip channels that error.
    let servers = entry.backend.get_servers().await.unwrap_or_default();
    let mut per_chat_bundles: Vec<Value> = Vec::new();

    for server in &servers {
        let channels = entry
            .backend
            .get_channels(&server.id)
            .await
            .unwrap_or_default();
        for channel in channels {
            if channel.unread_count == 0 {
                continue;
            }
            let messages = entry
                .backend
                .get_messages(
                    &channel.id,
                    poly_client::MessageQuery {
                        limit: Some(message_limit),
                        ..Default::default()
                    },
                )
                .await
                .ok()
                .unwrap_or_default();
            per_chat_bundles.push(json!({
                "kind": "channel",
                "server": { "id": server.id, "name": server.name },
                "channel": { "id": channel.id, "name": channel.name, "unread_count": channel.unread_count },
                "recent_messages": messages,
            }));
        }
    }

    // DMs with unread messages.
    let dms = entry.backend.get_dm_channels().await.unwrap_or_default();
    for dm in dms {
        if dm.unread_count == 0 {
            continue;
        }
        let messages = entry
            .backend
            .get_messages(
                &dm.id,
                poly_client::MessageQuery {
                    limit: Some(message_limit),
                    ..Default::default()
                },
            )
            .await
            .ok()
            .unwrap_or_default();
        per_chat_bundles.push(json!({
            "kind": "dm",
            "contact": { "id": dm.user.id, "name": dm.user.display_name },
            "dm_channel_id": dm.id,
            "unread_count": dm.unread_count,
            "recent_messages": messages,
        }));
    }

    ok_result(
        serde_json::to_string_pretty(&json!({
            "account_id": account_id,
            "unread_chat_count": per_chat_bundles.len(),
            "chats": per_chat_bundles,
        }))
        .unwrap_or_default(),
    )
}

// ─── Phase B (meta-personas) — tool handlers ─────────────────────────────────

/// Emit an audit row; swallows errors so failures don't break the primary
/// return path. The tool already returns its result — audit is best-effort.
fn audit(
    mem: &MemoryDb,
    slug: &str,
    action: &str,
    payload: Option<&str>,
    result: &str,
    error_msg: Option<&str>,
) {
    drop(mem.record_persona_audit(slug, "claude-desktop", action, None, None, payload, result, error_msg));
}

fn handle_meta_persona_list(mem: &MemoryDb) -> Value {
    match mem.list_personas() {
        Ok(list) => ok_result(serde_json::to_string_pretty(&list).unwrap_or_default()),
        Err(e)   => err_result(format!("meta_persona_list failed: {e}")),
    }
}

fn handle_meta_persona_get(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    match mem.get_persona(slug) {
        Ok(Some(p)) => {
            audit(mem, slug, "invoke", Some("{\"action\":\"get\"}"), "ok", None);
            ok_result(serde_json::to_string_pretty(&p).unwrap_or_default())
        }
        Ok(None)    => err_result(format!("persona '{slug}' not found")),
        Err(e)      => err_result(format!("meta_persona_get failed: {e}")),
    }
}

fn handle_meta_persona_create(args: &Value, mem: &MemoryDb) -> Value {
    let slug          = match str_arg(args, "slug")          { Some(v) => v, None => return err_result("missing 'slug'") };
    let name          = match str_arg(args, "name")          { Some(v) => v, None => return err_result("missing 'name'") };
    let system_prompt = match str_arg(args, "system_prompt") { Some(v) => v, None => return err_result("missing 'system_prompt'") };

    let avatar_emoji  = str_arg(args, "avatar_emoji").unwrap_or("🤖");
    let style_notes   = str_arg(args, "style_notes");
    let heartbeat     = args.get("heartbeat_interval_secs").and_then(serde_json::Value::as_i64);
    let proactivity   = str_arg(args, "proactivity").unwrap_or("drafts-only");
    let rate_limit    = args.get("rate_limit_per_hour").and_then(serde_json::Value::as_i64).unwrap_or(4);

    match mem.create_persona(slug, name, avatar_emoji, system_prompt, style_notes, heartbeat, proactivity, rate_limit) {
        Ok(s) => {
            audit(mem, &s, "invoke", Some("{\"action\":\"create\"}"), "ok", None);
            ok_result(format!("{{\"slug\":\"{s}\"}}"))
        }
        Err(e) => err_result(format!("meta_persona_create failed: {e}")),
    }
}

fn handle_meta_persona_update(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };

    let name          = str_arg(args, "name");
    let avatar_emoji  = str_arg(args, "avatar_emoji");
    let system_prompt = str_arg(args, "system_prompt");

    // style_notes: absent = preserve; null JSON = clear; string = set.
    let style_notes: Option<Option<&str>> = match args.get("style_notes") {
        None => None,
        Some(v) if v.is_null() => Some(None),
        Some(v) => Some(v.as_str()),
    };

    // heartbeat_interval_secs: absent = preserve; null/0 JSON = clear.
    let heartbeat: Option<Option<i64>> = match args.get("heartbeat_interval_secs") {
        None => None,
        Some(v) if v.is_null() => Some(None),
        Some(v) => match v.as_i64() {
            Some(0) | None => Some(None),
            Some(n) => Some(Some(n)),
        },
    };

    let proactivity   = str_arg(args, "proactivity");
    let rate_limit    = args.get("rate_limit_per_hour").and_then(serde_json::Value::as_i64);
    let enabled       = args.get("enabled").and_then(serde_json::Value::as_bool);

    match mem.update_persona(slug, name, avatar_emoji, system_prompt, style_notes, heartbeat, proactivity, rate_limit, enabled, None) {
        Ok(true)  => {
            audit(mem, slug, "invoke", Some("{\"action\":\"update\"}"), "ok", None);
            ok_result(format!("persona '{slug}' updated"))
        }
        Ok(false) => err_result(format!("persona '{slug}' not found")),
        Err(e)    => err_result(format!("meta_persona_update failed: {e}")),
    }
}

fn handle_meta_persona_delete(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    // Write the audit row BEFORE deleting (cascade will wipe it otherwise).
    drop(mem.record_persona_audit(slug, "claude-desktop", "invoke", None, None,
        Some("{\"action\":\"delete\"}"), "ok", None));
    match mem.delete_persona(slug) {
        Ok(()) => ok_result(format!("persona '{slug}' deleted")),
        Err(e) => err_result(format!("meta_persona_delete failed: {e}")),
    }
}

fn handle_meta_persona_set_sources(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    let sources = match args.get("sources").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return err_result("missing or invalid 'sources' (must be array)"),
    };

    // Atomic replace: remove all existing sources, then insert new ones.
    let existing = match mem.list_persona_sources(slug) {
        Ok(v)  => v,
        Err(e) => return err_result(format!("meta_persona_set_sources list failed: {e}")),
    };
    for src in &existing {
        if let Some(id) = src.get("id").and_then(serde_json::Value::as_i64)
            && let Err(e) = mem.remove_persona_source(id) {
                return err_result(format!("meta_persona_set_sources remove failed: {e}"));
            }
    }

    let mut added = 0usize;
    for src in sources {
        let account_id    = match src.get("account_id").and_then(|v| v.as_str()) {
            Some(v) => v,
            None => return err_result("source missing 'account_id'"),
        };
        let selector_kind = match src.get("selector_kind").and_then(|v| v.as_str()) {
            Some(v) => v,
            None => return err_result("source missing 'selector_kind'"),
        };
        let selector_value = src.get("selector_value").and_then(|v| v.as_str());
        let include        = src.get("include").and_then(serde_json::Value::as_bool).unwrap_or(true);

        if let Err(e) = mem.add_persona_source(slug, account_id, selector_kind, selector_value, include) {
            return err_result(format!("meta_persona_set_sources insert failed: {e}"));
        }
        added = added.wrapping_add(1);
    }

    audit(mem, slug, "invoke", Some(&format!("{{\"action\":\"set_sources\",\"count\":{added}}}")), "ok", None);
    ok_result(format!("{{\"sources_set\":{added}}}"))
}

fn handle_meta_persona_set_tool_whitelist(args: &Value, mem: &MemoryDb) -> Value {
    let slug       = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    let tool_names = match args.get("tool_names").and_then(|v| v.as_array()) {
        Some(a) => a,
        None    => return err_result("missing or invalid 'tool_names' (must be array)"),
    };

    // Atomic replace: clear existing whitelist then insert new entries.
    let existing = match mem.list_persona_tools(slug) {
        Ok(v)  => v,
        Err(e) => return err_result(format!("meta_persona_set_tool_whitelist list failed: {e}")),
    };
    for tool in &existing {
        if let Err(e) = mem.remove_persona_tool(slug, tool) {
            return err_result(format!("meta_persona_set_tool_whitelist remove failed: {e}"));
        }
    }

    let mut added = 0usize;
    for t in tool_names {
        let name = match t.as_str() {
            Some(s) => s,
            None    => return err_result("tool_names entries must be strings"),
        };
        if let Err(e) = mem.add_persona_tool(slug, name) {
            return err_result(format!("meta_persona_set_tool_whitelist insert failed: {e}"));
        }
        added = added.wrapping_add(1);
    }

    audit(mem, slug, "invoke", Some(&format!("{{\"action\":\"set_tool_whitelist\",\"count\":{added}}}")), "ok", None);
    ok_result(format!("{{\"tools_set\":{added}}}"))
}

/// Phase C implementation of `meta_persona_invoke`.
///
/// Builds a full `bundle_v1` via `PersonaContextBuilder::build()` which
/// fetches live chat messages from the backend.  Pre-validation (persona
/// exists, enabled) still happens here before handing off to the builder.
///
/// ## dry_run semantics
///
/// When `dry_run=true` the bundle is built identically — same source
/// resolution, same message fetching, same size-cap degradation — but the
/// implicit `memory_read` audit rows written per chat are suppressed.  The
/// explicit **invoke** audit row (this call, user-initiated) still fires
/// regardless; suppressing it would remove the only record that the user
/// asked the persona to run, which is always useful for audit purposes.
///
/// Use `dry_run=true` when you want to inspect the bundle shape (e.g. from
/// the e2e harness or a future "preview bundle" UI button) without polluting
/// the persona's audit history with phantom memory reads.
async fn handle_meta_persona_invoke(args: &Value, pool: &BackendPool, mem: &MemoryDb) -> Value {
    use crate::persona::{PersonaContextRequest, build};
    use crate::persona::context::BackendPoolProvider;

    let slug = match str_arg(args, "slug") {
        Some(v) => v,
        None    => return err_result("missing 'slug'"),
    };
    let user_prompt = str_arg(args, "user_prompt").map(std::string::ToString::to_string);

    // Parse the dry_run flag (default: false).
    let dry_run = args.get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // Verify the persona exists and is enabled.
    let persona = match mem.get_persona(slug) {
        Ok(Some(p)) => p,
        Ok(None)    => {
            drop(mem.record_persona_audit(slug, "claude-desktop", "invoke", None, None,
                None, "error", Some("persona not found")));
            return err_result(format!("persona '{slug}' not found"));
        }
        Err(e) => return err_result(format!("meta_persona_invoke failed: {e}")),
    };

    if persona.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
        drop(mem.record_persona_audit(slug, "claude-desktop", "invoke", None, None,
            None, "denied", Some("persona disabled")));
        return err_result(format!("persona '{slug}' is disabled"));
    }

    // Parse optional tuning parameters.
    let max_messages_per_chat = args.get("max_messages_per_chat")
        .and_then(serde_json::Value::as_u64)
        .map_or(30, |v| usize::try_from(v.clamp(1, 200)).unwrap_or(200));
    let max_chats = args.get("max_chats")
        .and_then(serde_json::Value::as_u64)
        .map_or(25, |v| usize::try_from(v.clamp(1, 100)).unwrap_or(100));
    let include_summaries = args.get("include_summaries")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);

    let req = PersonaContextRequest {
        slug: slug.to_string(),
        user_prompt: user_prompt.clone(),
        max_messages_per_chat,
        max_chats,
        include_summaries,
        dry_run,
    };

    let provider = BackendPoolProvider { pool };

    match build(req, mem, &provider).await {
        Ok(bundle) => {
            let payload_str = format!(
                "{{\"action\":\"invoke\",\"user_prompt\":{},\"dry_run\":{dry_run}}}",
                user_prompt
                    .as_deref().map_or_else(|| "null".to_string(), |p| format!("{p:?}")),
            );
            // The invoke audit row fires unconditionally — even in dry_run mode.
            // This is intentional: the invoke row records that the user asked the
            // persona to run, which is always relevant.  Only the per-chat
            // memory_read rows (written by PersonaContextBuilder::build()) are
            // suppressed when dry_run=true.
            audit(mem, slug, "invoke", Some(&payload_str), "ok", None);
            ok_result(serde_json::to_string_pretty(&bundle).unwrap_or_default())
        }
        Err(e) => {
            let msg = e.to_string();
            drop(mem.record_persona_audit(
                slug, "claude-desktop", "invoke", None, None, None, "error", Some(&msg),
            ));
            err_result(format!("meta_persona_invoke build failed: {msg}"))
        }
    }
}

fn handle_meta_persona_set_heartbeat(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };

    // interval_secs: null or 0 → disable (None); positive integer → set.
    let interval: Option<Option<i64>> = match args.get("interval_secs") {
        None => None,   // absent — don't change
        Some(v) if v.is_null() => Some(None),
        Some(v) => {
            let n = v.as_i64().unwrap_or(0);
            Some(if n == 0 { None } else { Some(n) })
        }
    };

    match mem.update_persona(slug, None, None, None, None, interval, None, None, None, None) {
        Ok(true) => {
            let payload = match interval {
                Some(Some(n)) => format!("{{\"action\":\"set_heartbeat\",\"interval_secs\":{n}}}"),
                _             => "{\"action\":\"set_heartbeat\",\"interval_secs\":null}".to_string(),
            };
            audit(mem, slug, "invoke", Some(&payload), "ok", None);
            ok_result("heartbeat updated")
        }
        Ok(false) => err_result(format!("persona '{slug}' not found")),
        Err(e)    => err_result(format!("meta_persona_set_heartbeat failed: {e}")),
    }
}

fn handle_meta_persona_get_memory(args: &Value, mem: &MemoryDb) -> Value {
    let slug        = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    let pinned_only = args.get("pinned_only").and_then(serde_json::Value::as_bool).unwrap_or(false);

    match mem.list_persona_facts(slug, pinned_only) {
        Ok(facts) => {
            audit(mem, slug, "memory_read", Some("{\"action\":\"get_memory\"}"), "ok", None);
            ok_result(serde_json::to_string_pretty(&facts).unwrap_or_default())
        }
        Err(e) => err_result(format!("meta_persona_get_memory failed: {e}")),
    }
}

fn handle_meta_persona_set_memory(args: &Value, mem: &MemoryDb) -> Value {
    let slug      = match str_arg(args, "slug")      { Some(v) => v, None => return err_result("missing 'slug'") };
    let fact_text = match str_arg(args, "fact_text") { Some(v) => v, None => return err_result("missing 'fact_text'") };

    let category = str_arg(args, "category");
    let pinned   = args.get("pinned").and_then(serde_json::Value::as_bool).unwrap_or(false);

    match mem.add_persona_fact(slug, category, fact_text, pinned) {
        Ok(id) => {
            audit(mem, slug, "memory_write", Some(&format!("{{\"action\":\"set_memory\",\"fact_id\":{id}}}")), "ok", None);
            ok_result(format!("{{\"fact_id\":{id}}}"))
        }
        Err(e) => err_result(format!("meta_persona_set_memory failed: {e}")),
    }
}

fn handle_meta_persona_forget_memory(args: &Value, mem: &MemoryDb) -> Value {
    let slug       = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    let forget_all = args.get("forget_all").and_then(serde_json::Value::as_bool).unwrap_or(false);

    if forget_all {
        match mem.forget_all_persona_facts(slug) {
            Ok(()) => {
                audit(mem, slug, "memory_write", Some("{\"action\":\"forget_all_memory\"}"), "ok", None);
                ok_result(format!("all facts for persona '{slug}' deleted"))
            }
            Err(e) => err_result(format!("meta_persona_forget_memory failed: {e}")),
        }
    } else {
        let fact_id = match args.get("fact_id").and_then(serde_json::Value::as_i64) {
            Some(id) => id,
            None => return err_result("must provide 'fact_id' or set 'forget_all': true"),
        };
        match mem.remove_persona_fact(fact_id) {
            Ok(()) => {
                audit(mem, slug, "memory_write",
                    Some(&format!("{{\"action\":\"forget_memory\",\"fact_id\":{fact_id}}}")), "ok", None);
                ok_result(format!("fact {fact_id} deleted"))
            }
            Err(e) => err_result(format!("meta_persona_forget_memory failed: {e}")),
        }
    }
}

fn handle_meta_persona_recent_actions(args: &Value, mem: &MemoryDb) -> Value {
    let slug  = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    let limit = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(50).clamp(1, 500);

    match mem.list_persona_audit(slug, limit) {
        Ok(rows) => ok_result(serde_json::to_string_pretty(&rows).unwrap_or_default()),
        Err(e)   => err_result(format!("meta_persona_recent_actions failed: {e}")),
    }
}

fn handle_meta_persona_set_outbound_allow(args: &Value, mem: &MemoryDb) -> Value {
    let slug       = match str_arg(args, "slug")       { Some(v) => v, None => return err_result("missing 'slug'") };
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let remove     = args.get("remove").and_then(serde_json::Value::as_bool).unwrap_or(false);

    if remove {
        match mem.remove_persona_outbound_allow(slug, account_id, chat_id) {
            Ok(()) => {
                audit(mem, slug, "invoke", Some("{\"action\":\"remove_outbound_allow\"}"), "ok", None);
                ok_result(format!("outbound allow entry removed for {account_id}/{chat_id}"))
            }
            Err(e) => err_result(format!("meta_persona_set_outbound_allow remove failed: {e}")),
        }
    } else {
        let max_per_day = args.get("max_messages_per_day")
            .and_then(serde_json::Value::as_i64).unwrap_or(1).clamp(1, 100);
        match mem.set_persona_outbound_allow(slug, account_id, chat_id, max_per_day) {
            Ok(()) => {
                let payload = format!(
                    "{{\"action\":\"set_outbound_allow\",\"account_id\":\"{account_id}\",\"chat_id\":\"{chat_id}\",\"max_per_day\":{max_per_day}}}",
                );
                audit(mem, slug, "invoke", Some(&payload), "ok", None);
                ok_result(format!("outbound allow set for {account_id}/{chat_id} max={max_per_day}/day"))
            }
            Err(e) => err_result(format!("meta_persona_set_outbound_allow failed: {e}")),
        }
    }
}

// ─── Phase T — audit surface handlers ────────────────────────────────────────

/// `meta_persona_audit_query` — filtered query over persona_audit.
///
/// Read-only; no audit row emitted (auditing a read of the audit log would be
/// circular).  Listed in `unaudited-persona-tool-allowlist.txt`.
fn handle_meta_persona_audit_query(args: &Value, mem: &MemoryDb) -> Value {
    let slug           = str_arg(args, "slug");
    let action         = str_arg(args, "action");
    let actor          = str_arg(args, "actor");
    let since          = str_arg(args, "since");
    let until          = str_arg(args, "until");
    let target_account = str_arg(args, "target_account");
    let target_chat    = str_arg(args, "target_chat");
    let result         = str_arg(args, "result");
    let limit          = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(100).clamp(1, 500);

    match mem.query_persona_audit(
        slug,
        action,
        actor,
        since,
        until,
        target_account,
        target_chat,
        result,
        limit,
    ) {
        Ok(rows) => ok_result(serde_json::to_string_pretty(&rows).unwrap_or_default()),
        Err(e)   => err_result(format!("meta_persona_audit_query failed: {e}")),
    }
}

/// `meta_persona_audit_export` — full audit history for a persona as JSONL.
///
/// Read-only; no audit row emitted.  Listed in `unaudited-persona-tool-allowlist.txt`.
fn handle_meta_persona_audit_export(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") {
        Some(v) => v,
        None    => return err_result("missing 'slug'"),
    };

    match mem.export_persona_audit(slug) {
        Ok(rows) => {
            // Serialise as JSONL (one JSON object per line).
            let jsonl: String = rows
                .iter()
                .filter_map(|r| serde_json::to_string(r).ok())
                .collect::<Vec<_>>()
                .join("\n");
            ok_result(jsonl)
        }
        Err(e) => err_result(format!("meta_persona_audit_export failed: {e}")),
    }
}

// ─── Phase D (client-version plan) — client settings handlers ────────────────
//
// These handlers use `ClientConfigStore` from `poly_host_bridge`, which wraps
// the `/host/kv/*` bridge routes. In tests (no live bridge server), the store's
// `kv_*` calls will fail — tests that exercise these handlers spin up a full
// host-bridge mock or use `MemoryDb`-only assertions for the audit path.
//
// Backend IDs are hardcoded (10 slugs) in `client_settings_list` to avoid
// taking a live `BackendPool` dependency in the schema layer. New backends must
// be added here when they are added to `state.rs::create_backend`.

/// The 10 known backend slugs for `client_settings_list` enumeration.
const CLIENT_SETTINGS_BACKENDS: &[&str] = &[
    "stoat", "matrix", "lemmy", "hackernews", "discord",
    "teams", "poly", "github", "forgejo", "demo",
];

/// Emit a client-settings audit row; swallows errors so failures don't break
/// the primary return path.
fn audit_client_settings(
    mem: &MemoryDb,
    backend_id: &str,
    action: &str,
    payload: Option<&str>,
    status: &str,
    error_msg: Option<&str>,
) {
    drop(mem.record_client_settings_audit(backend_id, action, payload, status, error_msg));
}

async fn handle_client_settings_list(args: &Value, pool: &BackendPool, _mem: &MemoryDb) -> Value {
    // poly-lint: allow unaudited-client-settings-tool — read-only; no audit needed.
    let store = &pool.config_store;

    if let Some(bid) = str_arg(args, "backend_id") {
        // Single backend snapshot.
        match store.list_overrides(bid).await {
            Ok(snap) => ok_result(serde_json::to_string_pretty(&snap).unwrap_or_default()),
            Err(e)   => {
                // If host bridge is not reachable, return a zero-state snapshot
                // so callers can still reason about the backend.
                let snap = serde_json::json!({
                    "backend_id": bid,
                    "version_override": null,
                    "mechanisms": [],
                    "_error": format!("host bridge unavailable: {e}")
                });
                ok_result(serde_json::to_string_pretty(&snap).unwrap_or_default())
            }
        }
    } else {
        // All 10 known backends.
        let mut results = Vec::with_capacity(CLIENT_SETTINGS_BACKENDS.len());
        for bid in CLIENT_SETTINGS_BACKENDS {
            let snap = match store.list_overrides(bid).await {
                Ok(s) => serde_json::to_value(s).unwrap_or_default(),
                Err(e) => serde_json::json!({
                    "backend_id": bid,
                    "version_override": null,
                    "mechanisms": [],
                    "_error": format!("host bridge unavailable: {e}")
                }),
            };
            results.push(snap);
        }
        ok_result(serde_json::to_string_pretty(&results).unwrap_or_default())
    }
}

async fn handle_client_settings_get_version(args: &Value, pool: &BackendPool, _mem: &MemoryDb) -> Value {
    // poly-lint: allow unaudited-client-settings-tool — read-only; no audit needed.
    let bid = match str_arg(args, "backend_id") {
        Some(v) => v,
        None => return err_result("missing 'backend_id'"),
    };
    let store = &pool.config_store;
    match store.get_version_override(bid).await {
        Ok(Some(ov)) => ok_result(serde_json::json!({
            "backend_id": bid,
            "effective_version": &ov,
            "source": "override",
            "override": &ov,
        }).to_string()),
        Ok(None) => ok_result(serde_json::json!({
            "backend_id": bid,
            "effective_version": null,
            "source": "default",
            "override": null,
        }).to_string()),
        Err(e) => err_result(format!("client_settings_get_version failed: {e}")),
    }
}

async fn handle_client_settings_set_version_override(args: &Value, pool: &BackendPool, mem: &MemoryDb) -> Value {
    let bid = match str_arg(args, "backend_id") {
        Some(v) => v,
        None => return err_result("missing 'backend_id'"),
    };
    // `override` may be absent (clear), null JSON (clear), or a string (set).
    let override_val: Option<String> = match args.get("override") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => match v.as_str() {
            Some(s) => Some(s.to_owned()),
            None => return err_result("'override' must be a string or null"),
        },
    };

    let payload = serde_json::json!({
        "backend_id": bid,
        "override": override_val,
    }).to_string();

    let store = &pool.config_store;
    match store.set_version_override(bid, override_val.clone()).await {
        Ok(()) => {
            audit_client_settings(mem, bid, "set_version_override", Some(&payload), "ok", None);
            let msg = match &override_val {
                Some(s) => format!("version override for '{bid}' set to '{s}'"),
                None    => format!("version override for '{bid}' cleared"),
            };
            ok_result(msg)
        }
        Err(e) => {
            let err_msg = format!("client_settings_set_version_override failed: {e}");
            audit_client_settings(mem, bid, "set_version_override", Some(&payload), "error", Some(&err_msg));
            err_result(err_msg)
        }
    }
}

async fn handle_client_settings_list_mechanisms(args: &Value, pool: &BackendPool, _mem: &MemoryDb) -> Value {
    // poly-lint: allow unaudited-client-settings-tool — read-only; no audit needed.
    let bid = match str_arg(args, "backend_id") {
        Some(v) => v,
        None => return err_result("missing 'backend_id'"),
    };
    let store = &pool.config_store;
    match store.list_overrides(bid).await {
        Ok(snap) => {
            let mechs: Vec<serde_json::Value> = snap
                .mechanisms
                .into_iter()
                .map(|(id, enabled)| serde_json::json!({ "mechanism_id": id, "enabled": enabled }))
                .collect();
            ok_result(serde_json::to_string_pretty(&serde_json::json!({
                "backend_id": bid,
                "mechanisms": mechs,
            })).unwrap_or_default())
        }
        Err(e) => err_result(format!("client_settings_list_mechanisms failed: {e}")),
    }
}

async fn handle_client_settings_set_mechanism(args: &Value, pool: &BackendPool, mem: &MemoryDb) -> Value {
    let bid   = match str_arg(args, "backend_id")   { Some(v) => v, None => return err_result("missing 'backend_id'") };
    let mech  = match str_arg(args, "mechanism_id")  { Some(v) => v, None => return err_result("missing 'mechanism_id'") };
    let enabled = match args.get("enabled").and_then(serde_json::Value::as_bool) {
        Some(b) => b,
        None => return err_result("missing or invalid 'enabled' (must be boolean)"),
    };

    let payload = serde_json::json!({
        "backend_id": bid,
        "mechanism_id": mech,
        "enabled": enabled,
    }).to_string();

    let store = &pool.config_store;
    match store.set_mechanism_state(bid, mech, enabled).await {
        Ok(()) => {
            audit_client_settings(mem, bid, "set_mechanism", Some(&payload), "ok", None);
            ok_result(format!("mechanism '{mech}' on '{bid}' set to {enabled}"))
        }
        Err(e) => {
            let err_msg = format!("client_settings_set_mechanism failed: {e}");
            audit_client_settings(mem, bid, "set_mechanism", Some(&payload), "error", Some(&err_msg));
            err_result(err_msg)
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

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

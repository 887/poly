//! Integrations section — MCP server controls and AI feature toggles.
//!
//! Poly runs as an MCP server so any AI client can connect to all your chat
//! backends without needing API keys. The toggle and port are persisted via
//! the host KV bridge under the `agent.mcp.*` key namespace.
//!
//! # TODO H.7 — /agent/access nuclear wipe (deferred)
//!
//! Phase H.7 requires a `/agent/access` nuclear-wipe page that clears all four
//! persona tables (personas, persona_sources, persona_facts, persona_audit) in
//! addition to the existing contact_facts / chat_notes / drafts wipe.  The page
//! does not exist yet — it was never created by Phases A–F.  When it is added,
//! wire the following MCP calls into the "Wipe all agent data" action:
//!
//!   - `meta_persona_delete` for each slug returned by `meta_persona_list`
//!   - OR a new `meta_persona_wipe_all` tool (single-call bulk delete)
//!
//! Blocked on: the access page itself does not exist (search for "AgentAccess"
//! or "/agent/access" in routes.rs).  Logging as deferred so it doesn't block
//! Phases G + H ship.

use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

const DEFAULT_MCP_PORT: u16 = 3010;
const KV_MCP_ENABLED: &str = "agent.mcp.enabled";
const KV_MCP_PORT: &str = "agent.mcp.port";
/// Selected MCP transport for the displayed config snippet. Both endpoints
/// (`POST /mcp` polling, `GET /mcp/sse` server-push) are always served by
/// the MCP server — the toggle only changes which URL we recommend in the
/// "Add Poly to your AI client" snippet. Default `"polling"` because Claude
/// Desktop's server-push consumer is broken as of 2026-04
/// (anthropics/claude-code#4118 — duplicate of #13646).
const KV_MCP_TRANSPORT: &str = "agent.mcp.transport";

/// Actions for the Integrations section.
pub enum IntegrationsAction {
    /// Toggle MCP server on or off.
    ToggleMcp(bool),
    /// Change the MCP server port.
    SetMcpPort(u16),
    /// Switch the recommended MCP transport between polling and SSE.
    SetMcpTransport(McpTransport),
}

/// Recommended MCP transport for the displayed config snippet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpTransport {
    /// `POST /mcp` request/response — works with every MCP host today.
    Polling,
    /// `GET /mcp/sse` server-push — needs a host that consumes server-initiated
    /// frames (Continue.dev, Zed, Goose). Claude Desktop advertises support
    /// but doesn't actually refresh on notifications — see
    /// anthropics/claude-code#4118.
    Sse,
}

impl McpTransport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Polling => "polling",
            Self::Sse => "sse",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "sse" => Self::Sse,
            // "polling" + unknown both fall through to the safe default.
            _ => Self::Polling,
        }
    }
}

impl UiAction for IntegrationsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::ToggleMcp(enabled) => {
                spawn(async move {
                    if let Some(storage) = crate::STORAGE.get()
                        && let Err(e) = storage.set(KV_MCP_ENABLED, serde_json::json!(enabled)).await {
                            tracing::warn!("Failed to persist agent.mcp.enabled: {e}");
                        }
                });
            }
            Self::SetMcpPort(port) => {
                spawn(async move {
                    if let Some(storage) = crate::STORAGE.get()
                        && let Err(e) = storage.set(KV_MCP_PORT, serde_json::json!(port)).await {
                            tracing::warn!("Failed to persist agent.mcp.port: {e}");
                        }
                });
            }
            Self::SetMcpTransport(transport) => {
                let s = transport.as_str();
                spawn(async move {
                    if let Some(storage) = crate::STORAGE.get()
                        && let Err(e) = storage.set(KV_MCP_TRANSPORT, serde_json::json!(s)).await {
                            tracing::warn!("Failed to persist agent.mcp.transport: {e}");
                        }
                });
            }
        }
    }
}

/// One integration row: icon + label + description.
///
/// Reuses `.setup-feature` / `.setup-feature-icon` from wizard.css and
/// `.agent-feature-body` (defined in settings-layout.css) for the two-line text.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn IntegrationItem(icon: &'static str, label_key: &'static str, desc_key: &'static str) -> Element {
    rsx! {
        div { class: "setup-feature",
            span { class: "setup-feature-icon", "{icon}" }
            div { class: "agent-feature-body",
                span { class: "setup-feature-text", "{t(label_key)}" }
                span { class: "settings-toggle-desc", "{t(desc_key)}" }
            }
        }
    }
}

/// MCP enable toggle + port input.
#[context_menu(allow_default)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn McpToggleRow(mcp_enabled: Signal<bool>, mcp_port: Signal<u16>) -> Element {
    rsx! {
        div { class: "settings-toggle-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{t(\"settings-mcp-enable\")}" }
            }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked: *mcp_enabled.read(),
                    onchange: move |e| {
                        let val = e.checked();
                        mcp_enabled.set(val);
                        spawn(async move {
                            if let Some(storage) = crate::STORAGE.get()
                                && let Err(err) = storage.set(KV_MCP_ENABLED, serde_json::json!(val)).await {
                                    tracing::warn!("Failed to persist agent.mcp.enabled: {err}");
                                }
                        });
                    },
                }
                span { class: "toggle-slider" }
            }
        }
        div { class: "settings-toggle-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{t(\"settings-mcp-port\")}" }
            }
            input {
                class: "settings-text-input settings-input-short",
                r#type: "number",
                min: "1024",
                max: "65535",
                value: "{mcp_port.read()}",
                onchange: move |e| {
                    if let Ok(p) = e.value().parse::<u16>()
                        && p >= 1024 {
                            mcp_port.set(p);
                            spawn(async move {
                                if let Some(storage) = crate::STORAGE.get()
                                    && let Err(err) = storage.set(KV_MCP_PORT, serde_json::json!(p)).await {
                                        tracing::warn!("Failed to persist agent.mcp.port: {err}");
                                    }
                            });
                        }
                },
            }
        }
    }
}

/// MCP transport selector — picks which URL the config snippet recommends.
///
/// The MCP server always serves both `/mcp` (polling) and `/mcp/sse` (server
/// push); this toggle just changes the recommended snippet. Default is
/// polling because Claude Desktop's server-push consumer is broken as of
/// April 2026 — see anthropics/claude-code#4118.
#[context_menu(allow_default)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn McpTransportRow(transport: Signal<McpTransport>) -> Element {
    let is_sse = *transport.read() == McpTransport::Sse;
    rsx! {
        div { class: "settings-toggle-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{t(\"settings-mcp-transport-label\")}" }
                p { class: "settings-toggle-desc",
                    "{t(\"settings-mcp-transport-desc\")} "
                    a {
                        href: "https://github.com/anthropics/claude-code/issues/4118",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "anthropics/claude-code#4118"
                    }
                }
            }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked: is_sse,
                    onchange: move |e| {
                        let next = if e.checked() { McpTransport::Sse } else { McpTransport::Polling };
                        transport.set(next);
                        let s = next.as_str();
                        spawn(async move {
                            if let Some(storage) = crate::STORAGE.get()
                                && let Err(err) = storage.set(KV_MCP_TRANSPORT, serde_json::json!(s)).await {
                                    tracing::warn!("Failed to persist agent.mcp.transport: {err}");
                                }
                        });
                    },
                }
                span { class: "toggle-slider" }
            }
        }
    }
}

/// MCP config snippet + links block.
///
/// `transport` selects the URL path shown in the snippet:
/// - `Polling` → `/mcp`     (POST request/response, universally compatible)
/// - `Sse`     → `/mcp/sse` (GET server-sent-events stream, MCP Streamable HTTP)
///
/// Both endpoints are always served by the MCP server. The toggle is purely
/// the recommendation we put in the user's clipboard.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn McpConfigBlock(mcp_port: Signal<u16>, transport: Signal<McpTransport>) -> Element {
    let port = *mcp_port.read();
    let path = match *transport.read() {
        McpTransport::Polling => "/mcp",
        McpTransport::Sse => "/mcp/sse",
    };
    let config_json = format!(
        "{{\n  \"mcpServers\": {{\n    \"poly\": {{\n      \"url\": \"http://127.0.0.1:{port}{path}\"\n    }}\n  }}\n}}"
    );
    let copy_js = format!("navigator.clipboard.writeText({config_json:?}).catch(()=>{{}})");
    rsx! {
        div { class: "mcp-config-example",
            h4 { class: "settings-subsection-title", "{t(\"settings-mcp-config-title\")}" }
            p { class: "settings-description", "{t(\"settings-mcp-config-description\")}" }
            pre { class: "mcp-config-block", "{config_json}" }
            button {
                class: "btn btn-secondary btn-sm",
                onclick: move |_| { let _ = document::eval(&copy_js); },
                "Copy"
            }
        }
        div { class: "mcp-links",
            h4 { class: "settings-subsection-title", "{t(\"settings-mcp-links-title\")}" }
            div { class: "mcp-links-list",
                a {
                    class: "mcp-link",
                    href: "https://modelcontextprotocol.io/docs/tools/inspector",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "{t(\"settings-mcp-link-docs\")}"
                }
                a {
                    class: "mcp-link",
                    href: "https://en.wikipedia.org/wiki/Model_Context_Protocol",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "{t(\"settings-mcp-link-wikipedia\")}"
                }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(IntegrationsAction)]
#[component]
pub(super) fn Integrations() -> Element {
    let mut mcp_enabled = use_signal(|| true);
    let mut mcp_port = use_signal(|| DEFAULT_MCP_PORT);
    let mut mcp_transport = use_signal(|| McpTransport::Polling);

    // Load persisted values from KV on mount.
    use_future(move || async move {
        let Some(storage) = crate::STORAGE.get() else { return };
        if let Ok(Some(v)) = storage.get(KV_MCP_ENABLED).await
            && let Some(b) = v.as_bool() {
                mcp_enabled.set(b);
            }
        if let Ok(Some(v)) = storage.get(KV_MCP_PORT).await
            && let Some(p) = v.as_u64().and_then(|n| u16::try_from(n).ok())
                && p >= 1024 {
                    mcp_port.set(p);
                }
        if let Ok(Some(v)) = storage.get(KV_MCP_TRANSPORT).await
            && let Some(s) = v.as_str() {
                mcp_transport.set(McpTransport::from_str(s));
            }
    });

    rsx! {
        div { class: "settings-section",
            h2 { id: "agent-section-integrations", "{t(\"agent-section-integrations\")}" }
            p { class: "settings-description", "{t(\"agent-section-integrations-desc\")}" }

            h3 { class: "settings-subsection-title", "{t(\"settings-mcp\")}" }
            p { class: "settings-description", "{t(\"settings-mcp-description\")}" }
            McpToggleRow { mcp_enabled, mcp_port }
            McpTransportRow { transport: mcp_transport }
            McpConfigBlock { mcp_port, transport: mcp_transport }

            h3 { class: "settings-subsection-title", "{t(\"agent-section-integrations\")}" }
            div { class: "setup-features",
                IntegrationItem {
                    icon: "💬",
                    label_key: "agent-integration-responses",
                    desc_key: "agent-integration-responses-desc",
                }
                IntegrationItem {
                    icon: "📋",
                    label_key: "agent-integration-summaries",
                    desc_key: "agent-integration-summaries-desc",
                }
                IntegrationItem {
                    icon: "🌐",
                    label_key: "agent-integration-translate",
                    desc_key: "agent-integration-translate-desc",
                }
                IntegrationItem {
                    icon: "🧠",
                    label_key: "agent-integration-memory",
                    desc_key: "agent-integration-memory-desc",
                }
                IntegrationItem {
                    icon: "📅",
                    label_key: "agent-integration-outreach",
                    desc_key: "agent-integration-outreach-desc",
                }
                IntegrationItem {
                    icon: "🎨",
                    label_key: "agent-integration-image-gen",
                    desc_key: "agent-integration-image-gen-desc",
                }
            }
        }
    }
}

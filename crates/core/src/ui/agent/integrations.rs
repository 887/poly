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

/// Actions for the Integrations section.
pub enum IntegrationsAction {
    /// Toggle MCP server on or off.
    ToggleMcp(bool),
    /// Change the MCP server port.
    SetMcpPort(u16),
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

/// MCP config snippet + links block.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn McpConfigBlock(mcp_port: Signal<u16>) -> Element {
    let port = *mcp_port.read();
    let config_json = format!(
        "{{\n  \"mcpServers\": {{\n    \"poly\": {{\n      \"url\": \"http://127.0.0.1:{port}/mcp\"\n    }}\n  }}\n}}"
    );
    let copy_js = format!("navigator.clipboard.writeText({:?}).catch(()=>{{}})", config_json);
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
    });

    rsx! {
        div { class: "settings-section",
            h2 { id: "agent-section-integrations", "{t(\"agent-section-integrations\")}" }
            p { class: "settings-description", "{t(\"agent-section-integrations-desc\")}" }

            h3 { class: "settings-subsection-title", "{t(\"settings-mcp\")}" }
            p { class: "settings-description", "{t(\"settings-mcp-description\")}" }
            McpToggleRow { mcp_enabled, mcp_port }
            McpConfigBlock { mcp_port }

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

//! Integrations section — MCP server controls and AI feature toggles.
//!
//! Poly runs as an MCP server so any AI client can connect to all your chat
//! backends without needing API keys. The toggle and port here are local
//! Signal state for now; persist via host KV bridge in Phase 5.

use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

const DEFAULT_MCP_PORT: u16 = 3010;

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
            Self::ToggleMcp(_enabled) => {
                // TODO: persist via KV (key: agent.mcp.enabled) in Phase 5.
            }
            Self::SetMcpPort(_port) => {
                // TODO: persist via KV (key: agent.mcp.port) in Phase 5.
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
#[context_menu(inherit)]
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
                        mcp_enabled.set(e.checked());
                        // TODO: persist via KV in Phase 5
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
                    if let Ok(p) = e.value().parse::<u16>() {
                        if p >= 1024 {
                            mcp_port.set(p);
                            // TODO: persist via KV in Phase 5
                        }
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
    let mcp_enabled = use_signal(|| true);
    let mcp_port = use_signal(|| DEFAULT_MCP_PORT);

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

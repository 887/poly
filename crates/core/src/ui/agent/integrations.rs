//! Integrations section — MCP server controls and AI feature toggles.
//!
//! Content previously sketched in `settings/ai.rs` now lives here, reframed
//! around the MCP hand-off model: Poly runs as an MCP server so any AI client
//! can connect to all your chat backends without needing API keys.

use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

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
                // Phase 5: persist via host KV bridge.
            }
            Self::SetMcpPort(_port) => {
                // Phase 5: persist via host KV bridge.
            }
        }
    }
}

/// One integration row: icon + label + description.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn IntegrationItem(icon: &'static str, label_key: &'static str, desc_key: &'static str) -> Element {
    rsx! {
        div { class: "integration-item",
            span { class: "integration-icon", "{icon}" }
            div { class: "integration-text",
                span { class: "integration-label", "{t(label_key)}" }
                span { class: "integration-desc", "{t(desc_key)}" }
            }
        }
    }
}

/// MCP server status/config block (placeholder until Phase 5 wires up the
/// host KV bridge and the actual server process).
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn McpServerBlock() -> Element {
    rsx! {
        div { class: "mcp-server-block",
            h3 { class: "settings-subsection-title", "{t(\"settings-mcp\")}" }
            p { class: "settings-description", "{t(\"settings-mcp-description\")}" }

            div { class: "mcp-status-row",
                span { class: "mcp-status-label", "{t(\"settings-mcp-status-stopped\")}" }
            }

            div { class: "mcp-config-block",
                h4 { class: "mcp-config-title", "{t(\"settings-mcp-config-title\")}" }
                p { class: "settings-description", "{t(\"settings-mcp-config-description\")}" }
            }

            div { class: "mcp-links",
                h4 { class: "mcp-links-title", "{t(\"settings-mcp-links-title\")}" }
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
    rsx! {
        div { class: "settings-section integrations-section",
            h2 { id: "agent-section-integrations", "{t(\"agent-section-integrations\")}" }
            p { class: "settings-description", "{t(\"agent-section-integrations-desc\")}" }

            McpServerBlock {}

            div { class: "integrations-list",
                h3 { class: "settings-subsection-title", "{t(\"settings-mcp-config-title\")}" }
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

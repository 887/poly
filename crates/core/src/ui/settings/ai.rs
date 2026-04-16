//! MCP Server settings — configure the built-in poly-chat-mcp sidecar.
//!
//! The Electron app spawns `poly-chat-mcp` on a configurable port so that
//! AI tools (Claude Desktop, etc.) can connect to all your chat backends.

use crate::i18n::t;
use dioxus::prelude::*;

const DEFAULT_MCP_PORT: u16 = 3010;

/// MCP server status returned by the Electron IPC bridge.
#[derive(Clone, Debug, Default)]
struct McpStatus {
    running: bool,
    port: u16,
    is_electron: bool,
}

#[rustfmt::skip]
#[component]
pub(super) fn AiSettings() -> Element {
    let mut mcp_enabled = use_signal(|| true);
    let mut mcp_port = use_signal(|| DEFAULT_MCP_PORT);
    let mut status = use_signal(McpStatus::default);

    // Poll MCP status via Electron IPC bridge (no-op in web mode)
    use_future(move || async move {
        let mut eval = document::eval(
            r#"
            (async () => {
                const pe = window.polyElectron;
                if (!pe?.mcpStatus) {
                    dioxus.send({ running: false, port: 3010, is_electron: false });
                    return;
                }
                try {
                    const s = await pe.mcpStatus();
                    dioxus.send({ running: s.running, port: s.port, is_electron: true });
                } catch(e) {
                    dioxus.send({ running: false, port: 3010, is_electron: true });
                }
            })();
            "#,
        );
        if let Ok(val) = eval.recv::<serde_json::Value>().await {
            let running = val.get("running").and_then(|v| v.as_bool()).unwrap_or(false);
            let port = val.get("port").and_then(|v| v.as_u64()).unwrap_or(3010) as u16;
            let is_electron = val.get("is_electron").and_then(|v| v.as_bool()).unwrap_or(false);
            status.set(McpStatus { running, port, is_electron });
            if val.get("port").and_then(|v| v.as_u64()).is_some() {
                mcp_port.set(port);
            }
        }
    });

    let s = status.read();
    let port = *mcp_port.read();
    let config_example = format!(
        r#"{{
  "mcpServers": {{
    "poly": {{
      "url": "http://localhost:{}/mcp"
    }}
  }}
}}"#,
        port
    );

    let status_class = if s.is_electron {
        if s.running { "mcp-status mcp-status-running" } else { "mcp-status mcp-status-stopped" }
    } else {
        "mcp-status mcp-status-web"
    };

    let status_text = if !s.is_electron {
        t("settings-mcp-status-web")
    } else if s.running {
        format!("{} {}", t("settings-mcp-status-running"), s.port)
    } else {
        t("settings-mcp-status-stopped")
    };

    rsx! {
        div { class: "settings-section mcp-settings",
            h2 { id: "settings-section-ai", "{t(\"settings-mcp\")}" }
            p { class: "settings-description", "{t(\"settings-mcp-description\")}" }

            // Enable toggle + port
            div { class: "settings-row",
                label { class: "settings-label",
                    input {
                        r#type: "checkbox",
                        checked: *mcp_enabled.read(),
                        onchange: move |e| mcp_enabled.set(e.checked()),
                    }
                    " {t(\"settings-mcp-enable\")}"
                }
            }
            div { class: "settings-row",
                label { class: "settings-label", "{t(\"settings-mcp-port\")}" }
                input {
                    r#type: "number",
                    class: "settings-input settings-input-short",
                    value: "{port}",
                    min: "1024",
                    max: "65535",
                    onchange: move |e| {
                        if let Ok(p) = e.value().parse::<u16>() {
                            mcp_port.set(p);
                        }
                    },
                }
            }

            // Status indicator
            div { class: "settings-row",
                span { class: status_class,
                    if s.is_electron && s.running { "● " } else { "○ " }
                    "{status_text}"
                }
                if s.is_electron {
                    button {
                        class: "btn btn-sm settings-mcp-restart-btn",
                        onclick: move |_| {
                            let _ = document::eval("window.polyElectron?.mcpRestart?.();");
                        },
                        "{t(\"settings-mcp-restart\")}"
                    }
                }
            }

            // Config example
            div { class: "settings-subsection mcp-config-example",
                h3 { class: "settings-subsection-title", "{t(\"settings-mcp-config-title\")}" }
                p { class: "settings-description", "{t(\"settings-mcp-config-description\")}" }
                pre { class: "mcp-config-block",
                    "{config_example}"
                }
            }

            // Links
            div { class: "settings-subsection mcp-links",
                h3 { class: "settings-subsection-title", "{t(\"settings-mcp-links-title\")}" }
                div { class: "mcp-links-list",
                    a {
                        class: "mcp-link",
                        href: "https://modelcontextprotocol.io/docs/develop/connect-local-servers",
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
}

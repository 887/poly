//! PersonaToolWhitelistEditor — checkbox grid grouped by tool category.
//!
//! Four categories:
//! - **read**: `get_*`, `list_*` — read-only tools (default on)
//! - **memory**: `memory_*`, `meta_persona_*` read-side (default on)
//! - **draft**: `*draft*` (default on)
//! - **outbound**: `send_*`, `meta_persona_invoke` (default off)
//!
//! Save via `meta_persona_set_tool_whitelist`.

use super::mcp::call_persona_mcp;
use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};
use std::collections::BTreeSet;

// ─── Tool categories ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ToolCategory {
    Read,
    Memory,
    Draft,
    Outbound,
}

impl ToolCategory {
    fn label(self) -> &'static str {
        match self {
            Self::Read => "Read (get_*, list_*)",
            Self::Memory => "Memory",
            Self::Draft => "Draft",
            Self::Outbound => "Outbound (send_*)",
        }
    }

    fn ftl_key(self) -> &'static str {
        match self {
            Self::Read => "persona-tools-cat-read",
            Self::Memory => "persona-tools-cat-memory",
            Self::Draft => "persona-tools-cat-draft",
            Self::Outbound => "persona-tools-cat-outbound",
        }
    }
}

fn categorise(tool: &str) -> ToolCategory {
    if tool.starts_with("send_") || tool == "meta_persona_invoke" {
        ToolCategory::Outbound
    } else if tool.contains("draft") {
        ToolCategory::Draft
    } else if tool.starts_with("memory_")
        || tool.starts_with("meta_persona_get")
        || tool.starts_with("meta_persona_list")
        || tool.starts_with("meta_persona_recent")
        || tool.starts_with("meta_persona_get_memory")
    {
        ToolCategory::Memory
    } else {
        ToolCategory::Read
    }
}

/// All known chat-mcp tools (subset exposed to personas).
const KNOWN_TOOLS: &[&str] = &[
    "get_messages",
    "get_reply_context",
    "get_chat_style",
    "list_servers",
    "list_channels",
    "list_dms",
    "list_drafts",
    "recall_facts",
    "memory_store",
    "memory_recall",
    "memory_forget",
    "draft_create",
    "draft_approve",
    "draft_discard",
    "meta_persona_list",
    "meta_persona_get",
    "meta_persona_get_memory",
    "meta_persona_recent_actions",
    "meta_persona_invoke",
    "send_message",
    "send_typing",
];

// ─── ToolRow ─────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ToolRow(
    tool_name: String,
    checked: bool,
    on_toggle: EventHandler<String>,
) -> Element {
    let tool = tool_name.clone();
    rsx! {
        label { class: "persona-tool-row",
            input {
                r#type: "checkbox",
                checked,
                onchange: move |_| on_toggle.call(tool.clone()),
            }
            span { class: "persona-tool-name", "{tool_name}" }
        }
    }
}

// ─── PersonaToolWhitelistEditor ──────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct PersonaToolWhitelistEditorProps {
    pub persona_slug: String,
    /// Currently whitelisted tools (empty = read-only defaults).
    pub existing_whitelist: Vec<String>,
    pub on_saved: EventHandler<()>,
}

/// Checkbox grid for the tool whitelist.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn PersonaToolWhitelistEditor(props: PersonaToolWhitelistEditorProps) -> Element {
    let slug = props.persona_slug.clone();

    // Bootstrap from existing whitelist; if empty, default-on read+memory+draft.
    let initial: BTreeSet<String> = if props.existing_whitelist.is_empty() {
        KNOWN_TOOLS
            .iter()
            .filter(|t| categorise(t) != ToolCategory::Outbound)
            .map(std::string::ToString::to_string)
            .collect()
    } else {
        props.existing_whitelist.iter().cloned().collect()
    };
    let whitelist: Signal<BTreeSet<String>> = use_signal(move || initial);
    let mut saving = use_signal(|| false);
    let mut save_error: Signal<Option<String>> = use_signal(|| None);

    let on_saved = props.on_saved;

    // Group tools by category for rendering.
    let mut by_category: Vec<(ToolCategory, Vec<&str>)> = vec![
        (ToolCategory::Read, vec![]),
        (ToolCategory::Memory, vec![]),
        (ToolCategory::Draft, vec![]),
        (ToolCategory::Outbound, vec![]),
    ];
    for tool in KNOWN_TOOLS {
        let cat = categorise(tool);
        if let Some(entry) = by_category.iter_mut().find(|(c, _)| *c == cat) {
            entry.1.push(tool);
        }
    }

    rsx! {
        div { class: "persona-tool-whitelist-editor",
            for (category, tools) in by_category {
                div { class: "persona-tool-category",
                    h5 { class: "persona-tool-category-label", {t(category.ftl_key())} }
                    for tool in tools {
                        {
                            let tool_str = tool.to_string();
                            let is_checked = whitelist.read().contains(&tool_str);
                            rsx! {
                                ToolRow {
                                    key: "{tool_str}",
                                    tool_name: tool_str.clone(),
                                    checked: is_checked,
                                    on_toggle: move |name: String| {
                                        let mut wl = whitelist;
                                        let mut current = wl.read().clone();
                                        if current.contains(&name) {
                                            current.remove(&name);
                                        } else {
                                            current.insert(name);
                                        }
                                        wl.set(current);
                                    },
                                }
                            }
                        }
                    }
                }
            }

            div { class: "persona-editor-actions",
                button {
                    class: "btn btn-primary btn-sm",
                    disabled: *saving.read(),
                    onclick: {
                        let slug_save = slug.clone();
                        move |_| {
                            let slug_save = slug_save.clone();
                            let tools: Vec<String> = whitelist.read().iter().cloned().collect();
                            let on_saved = on_saved;
                            saving.set(true);
                            save_error.set(None);
                            spawn(async move {
                                match call_persona_mcp("meta_persona_set_tool_whitelist", serde_json::json!({
                                    "slug": slug_save,
                                    "tools": tools,
                                })).await {
                                    Ok(_) => on_saved.call(()),
                                    Err(e) => {
                                        tracing::warn!("set_tool_whitelist failed: {e}");
                                        save_error.set(Some(e));
                                    }
                                }
                                saving.set(false);
                            });
                        }
                    },
                    {t("persona-tools-save")}
                }
                if let Some(err) = save_error.read().clone() {
                    span { class: "persona-save-error", "{err}" }
                }
            }
        }
    }
}

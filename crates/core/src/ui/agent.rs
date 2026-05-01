//! Agent page — AI integrations and agent profile.
//!
//! Mirrors the layout of the settings page. Sections:
//! - Integrations: MCP server controls + AI feature overview (moved from settings/ai.rs)
//! - Profile: shareable agent handshake card
//!
//! ## 150-line component rule
//! Every `#[component]` fn body MUST stay under **150 lines** of RSX + logic.
//! Extract sub-components rather than growing any file.

mod chat_style_editor;
mod integrations;
pub mod persona;
mod profile;

pub use chat_style_editor::ChatStyleEditor;
pub use persona::PersonaManagementRoute;

use crate::i18n::t;
use crate::ui::routes::Route;
use crate::ui::settings::scroll_spy::scroll_to_settings_section;
use crate::ui::split_shell::SplitMenuShell;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

use integrations::Integrations;
use profile::AgentProfile;

/// Navigation sections for the agent page sidebar.
const NAV_SECTIONS: [(&str, AgentSection); 2] = [
    ("agent-section-integrations", AgentSection::Integrations),
    ("agent-section-profile", AgentSection::Profile),
];

/// All searchable nodes in the agent settings tree.
pub const AGENT_NODES: &[(&str, AgentSection)] = &[
    // Integrations
    ("agent-section-integrations", AgentSection::Integrations),
    ("settings-mcp", AgentSection::Integrations),
    ("settings-mcp-enable", AgentSection::Integrations),
    ("settings-mcp-port", AgentSection::Integrations),
    ("settings-mcp-config-title", AgentSection::Integrations),
    ("agent-integration-responses", AgentSection::Integrations),
    ("agent-integration-summaries", AgentSection::Integrations),
    ("agent-integration-translate", AgentSection::Integrations),
    ("agent-integration-memory", AgentSection::Integrations),
    ("agent-integration-outreach", AgentSection::Integrations),
    ("agent-integration-image-gen", AgentSection::Integrations),
    // Profile
    ("agent-section-profile", AgentSection::Profile),
    ("agent-profile-textarea-label", AgentSection::Profile),
];

/// The sections of the agent page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSection {
    /// MCP server controls and AI feature integrations.
    Integrations,
    /// Shareable agent handshake card.
    Profile,
}

impl AgentSection {
    pub fn to_slug(self) -> &'static str {
        match self {
            Self::Integrations => "integrations",
            Self::Profile => "profile",
        }
    }

    pub fn from_slug(slug: &str) -> Self {
        match slug {
            "profile" => Self::Profile,
            _ => Self::Integrations,
        }
    }
}

fn section_has_search_match(section: AgentSection, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    AGENT_NODES
        .iter()
        .any(|(key, s)| *s == section && t(key).to_lowercase().contains(q))
}

fn section_match_count(section: AgentSection, q: &str) -> usize {
    if q.is_empty() {
        return 0;
    }
    AGENT_NODES
        .iter()
        .filter(|(key, s)| *s == section && t(key).to_lowercase().contains(q))
        .count()
}

fn total_match_count(q: &str) -> usize {
    if q.is_empty() {
        return 0;
    }
    AGENT_NODES
        .iter()
        .filter(|(key, _)| t(key).to_lowercase().contains(q))
        .count()
}

fn scroll_to_section_anchor(slug: &str) {
    scroll_to_settings_section("agent-section-", slug);
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AgentSearchBar(search_text: Signal<String>) -> Element {
    let current = search_text.read().clone();
    let total = total_match_count(&current.to_lowercase());

    rsx! {
        div { class: "settings-search-bar",
            input {
                r#type: "text",
                class: "settings-search-input",
                placeholder: "{t(\"agent-search-placeholder\")}",
                value: "{current}",
                oninput: move |e| search_text.set(e.value()),
            }
            if !current.is_empty() {
                span { class: "settings-search-count",
                    "{total} {t(\"settings-search-found\")}"
                }
                button {
                    class: "settings-search-clear",
                    onclick: move |_| search_text.set(String::new()),
                    "×"
                }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AgentContentHeader(search_text: Signal<String>) -> Element {
    rsx! {
        div { class: "special-page-header settings-page-header",
            h2 { class: "special-page-title", "{t(\"agent-page-title\")}" }
            AgentSearchBar { search_text }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AgentNavigation(
    current: AgentSection,
    search_text: Signal<String>,
    on_select: EventHandler<AgentSection>,
) -> Element {
    let filter = search_text.read().to_lowercase();
    let nav_for_personas = use_navigator();

    rsx! {
        nav { class: "settings-nav",
            div { class: "settings-nav-header",
                h3 { class: "settings-nav-title", "{t(\"agent-page-title\")}" }
            }
            for (label_key, section) in NAV_SECTIONS {
                {
                    let label = t(label_key);
                    let has_match = section_has_search_match(section, &filter);
                    let count = section_match_count(section, &filter);
                    let active = current == section;
                    let searching = !filter.is_empty();
                    let class = match (searching, has_match, active) {
                        (true, false, _) => "settings-nav-item settings-nav-item-hidden",
                        (_, _, true) => "settings-nav-item active",
                        _ => "settings-nav-item",
                    };
                    rsx! {
                        div {
                            class,
                            onclick: move |_| {
                                on_select.call(section);
                            },
                            "data-settings-slug": "{section.to_slug()}",
                            "{label}"
                            if searching && count > 0 {
                                span { class: "settings-nav-match-count", "({count})" }
                            }
                        }
                    }
                }
            }
            // Personas — full-page management route.
            div {
                class: "settings-nav-item",
                onclick: move |_| {
                    nav_for_personas.push(Route::PersonasRoute);
                },
                {t("persona-panel-title")}
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AgentAllSections(search_query: String) -> Element {
    let q = search_query.to_lowercase();
    rsx! {
        for (_label_key, section) in NAV_SECTIONS {
            {
                let slug = section.to_slug();
                let id = format!("agent-section-{slug}");
                let has_match = section_has_search_match(section, &q);
                let searching = !q.is_empty();
                let class = if searching && !has_match {
                    "settings-section-block settings-section-hidden"
                } else {
                    "settings-section-block"
                };
                rsx! {
                    div { id, class,
                        match section {
                            AgentSection::Integrations => rsx! { Integrations {} },
                            AgentSection::Profile => rsx! { AgentProfile {} },
                        }
                    }
                }
            }
        }
        div { class: "settings-scroll-spacer" }
    }
}

/// Agent page component.
///
/// Mirrors the settings page layout: navigation sidebar on the left,
/// scrollable content on the right. Sections: Integrations and Profile.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub fn AgentPage() -> Element {
    let locale_key = crate::i18n::use_locale().read().clone();
    let mut search_text = use_signal(String::new);
    let mut active_section = use_signal(|| AgentSection::Integrations);
    let nav = use_navigator();

    use_effect(move || { // poly-lint: allow stale-effect-capture — body reads search_text via signal.read(), no captured local
        let q = search_text.read().to_lowercase();
        if q.is_empty() {
            return;
        }
        let _ = document::eval(
            "{ const c = document.querySelector('.agent-content'); if (c) c.scrollTop = 0; }"
        );
    });

    use_effect(move || { // poly-lint: allow stale-effect-capture — body reads active_section via signal.read(), no captured local
        let slug = active_section.read().to_slug().to_string();
        let js = format!(
            "setTimeout(() => {{ \
                const el = document.getElementById('agent-section-{slug}'); \
                if (el) el.scrollIntoView({{ block: 'start', behavior: 'smooth' }}); \
            }}, 0)"
        );
        let _ = document::eval(&js);
    });

    let section = *active_section.read();
    let query = search_text.read().clone();

    rsx! {
        SplitMenuShell {
            root_class: "settings-page agent-page".to_string(),
            sidebar_class: "settings-page-sidebar".to_string(),
            content_class: "settings-content agent-content".to_string(),
            sidebar: rsx! {
                AgentNavigation {
                    key: "agent-nav-{locale_key}",
                    current: section,
                    search_text,
                    on_select: move |next: AgentSection| {
                        *search_text.write() = String::new();
                        active_section.set(next);
                        scroll_to_section_anchor(next.to_slug());
                        nav.push(Route::AgentSectionRoute { section: next.to_slug().to_string() });
                    },
                }
            },
            content: rsx! {
                div { class: "settings-page-panel", key: "agent-panel-{locale_key}",
                    AgentContentHeader { search_text }
                    div { class: "settings-sections-stack",
                        AgentAllSections { search_query: query }
                    }
                }
            },
        }
    }
}

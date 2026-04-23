//! Server/repo overview page — landing page for forge-style backends.
//!
//! Shows a searchable grid of all repos (servers) belonging to the active
//! account, with open issue/PR counts and quick-nav to each repo's channels.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;

use crate::state::ChatData;
use crate::ui::routes::Route;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(inherit)]
#[context_menu(None)]
/// Server overview landing page for forge backends (GitHub, Forgejo).
#[component]
pub fn ServerOverviewPage(
    backend: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let chat_data = use_context::<Signal<ChatData>>();
    let mut search_query = use_signal(String::new);

    let query = search_query.read().to_lowercase();
    let servers: Vec<_> = chat_data
        .read()
        .servers
        .iter()
        .filter(|s| s.account_id == account_id)
        .cloned()
        .collect();

    let filtered: Vec<_> = if query.is_empty() {
        servers.clone()
    } else {
        servers
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&query))
            .cloned()
            .collect()
    };

    // Servers with unread/mention activity (for "Needs Attention" section)
    let attention: Vec<_> = servers
        .iter()
        .filter(|s| s.unread_count > 0 || s.mention_count > 0)
        .cloned()
        .collect();

    rsx! {
        main { class: "server-overview",
            div { class: "server-overview-header",
                h2 { class: "server-overview-title", "Repositories" }
                div { class: "server-overview-search",
                    input {
                        class: "settings-input",
                        r#type: "text",
                        placeholder: "Search repos…",
                        value: "{search_query}",
                        oninput: move |e: Event<FormData>| search_query.set(e.value()),
                    }
                }
            }

            // ── Needs Attention ──
            if !attention.is_empty() && query.is_empty() {
                div { class: "server-overview-section",
                    h3 { class: "server-overview-section-title", "Needs Attention" }
                    div { class: "server-overview-grid",
                        for server in attention.iter() {
                            RepoCard {
                                key: "{server.id}",
                                server: server.clone(),
                                backend: backend.clone(),
                                instance_id: instance_id.clone(),
                                account_id: account_id.clone(),
                                highlight: true,
                            }
                        }
                    }
                }
            }

            // ── All Repos ──
            div { class: "server-overview-section",
                h3 { class: "server-overview-section-title",
                    if query.is_empty() {
                        "All Repos ({servers.len()})"
                    } else {
                        "Results ({filtered.len()})"
                    }
                }
                if filtered.is_empty() {
                    p { class: "server-overview-empty",
                        if query.is_empty() {
                            "No repositories found for this account."
                        } else {
                            "No repos matching your search."
                        }
                    }
                } else {
                    div { class: "server-overview-grid",
                        for server in filtered.iter() {
                            RepoCard {
                                key: "{server.id}",
                                server: server.clone(),
                                backend: backend.clone(),
                                instance_id: instance_id.clone(),
                                account_id: account_id.clone(),
                                highlight: false,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Return a human-readable age string from an RFC 3339 timestamp.
///
/// Examples: "just now", "5m ago", "3h ago", "2d ago", "4mo ago", "1y ago"
fn humanize_age(ts: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(ts) else {
        return String::new();
    };
    let secs = (Utc::now() - dt.with_timezone(&Utc)).num_seconds().max(0);
    match secs {
        s if s < 60 => "just now".to_string(),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86400 => format!("{}h ago", s / 3600),
        s if s < 86400 * 30 => format!("{}d ago", s / 86400),
        s if s < 86400 * 365 => format!("{}mo ago", s / (86400 * 30)),
        s => format!("{}y ago", s / (86400 * 365)),
    }
}

#[ui_action(inherit)]
#[context_menu(inherit)]
/// A single repo card in the overview grid.
#[component]
fn RepoCard(
    server: poly_client::Server,
    backend: String,
    instance_id: String,
    account_id: String,
    highlight: bool,
) -> Element {
    let nav = navigator();
    let server_id = server.id.clone();
    let card_class = if highlight {
        "repo-card repo-card-attention"
    } else {
        "repo-card"
    };

    rsx! {
        div {
            class: "{card_class}",
            onclick: {
                let backend = backend.clone();
                let instance_id = instance_id.clone();
                let account_id = account_id.clone();
                let server_id = server_id.clone();
                move |_| {
                    nav.push(Route::ServerHome {
                        backend: backend.clone(),
                        instance_id: instance_id.clone(),
                        account_id: account_id.clone(),
                        server_id: server_id.clone(),
                    });
                }
            },
            // Icon
            if let Some(url) = &server.icon_url {
                img {
                    class: "repo-card-icon",
                    src: "{url}",
                    alt: "{server.name}",
                }
            } else {
                div { class: "repo-card-icon-placeholder",
                    "{server.name.chars().next().unwrap_or('?')}"
                }
            }
            // Info
            div { class: "repo-card-info",
                div { class: "repo-card-name", "{server.name}" }
                if let Some(desc) = &server.description {
                    if !desc.is_empty() {
                        div { class: "repo-card-description", "{desc}" }
                    }
                }
                div { class: "repo-card-meta",
                    if let Some(lang) = &server.language {
                        span { class: "repo-card-lang", "{lang}" }
                    }
                    if let Some(stars) = server.star_count {
                        span { class: "repo-card-stars",
                            span { class: "repo-card-star-icon", "★" }
                            "{stars}"
                        }
                    }
                    if let Some(ts) = &server.updated_at {
                        span { class: "repo-card-updated",
                            "Updated {humanize_age(ts)}"
                        }
                    }
                    if server.unread_count > 0 {
                        span { class: "repo-card-badge unread",
                            "{server.unread_count} updates"
                        }
                    }
                    if server.mention_count > 0 {
                        span { class: "repo-card-badge mention",
                            "@{server.mention_count}"
                        }
                    }
                }
            }
        }
    }
}

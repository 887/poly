//! `ChatUtilityRail` — the right-side utility panel shown when the user opens
//! Search, Pinned, Threads, Settings, Drafts, or the Agent tab.
//!
//! Also contains the card components used inside the rail:
//! `SearchResultCard`, `PinnedMessageCard`, `ChatSettingsPanel`, `SearchPreviewText`.

use crate::i18n::t;
use crate::state::{AppState, BatchedSignal};
use dioxus::prelude::*;
use poly_client::{Message, MessageContent, MessageSearchHit};
use poly_ui_macros::{context_menu, ui_action};
use super::ChatUtilityPanel;
use super::super::agent_panel::AgentPanel;
use super::super::draft_banner::DraftsSidebar;
use super::message_plain_text;
use super::persist_member_list_display_settings;

// ── Main component ────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
pub(super) fn ChatUtilityRail(
    panel: ChatUtilityPanel,
    search_ui: Element,
    search_query: String,
    search_hits: Vec<MessageSearchHit>,
    search_terms: Vec<String>,
    pinned_messages: Vec<Message>,
    current_channel_name: String,
    on_open_search_hit: EventHandler<MessageSearchHit>,
    on_open_pinned: EventHandler<Message>,
    on_close: EventHandler<()>,
    notifications_muted: Signal<bool>,
    mut pinned_filter_open: Signal<bool>,
    mut pinned_filter_query: Signal<String>,
    mut threads_filter_open: Signal<bool>,
    mut threads_filter_query: Signal<String>,
) -> Element {
    let rail_nav: BatchedSignal<crate::state::NavState> = use_context();
    let title = if panel == ChatUtilityPanel::Search {
        if search_query.is_empty() {
            t("search-messages")
        } else {
            format!("{} {}", search_hits.len(), t("search-results"))
        }
    } else if panel == ChatUtilityPanel::Pinned {
        t("pinned-messages")
    } else if panel == ChatUtilityPanel::Settings {
        t("chat-settings")
    } else if panel == ChatUtilityPanel::Drafts {
        t("agent-drafts-sidebar-title")
    } else if panel == ChatUtilityPanel::Agent {
        t("agent-panel-title")
    } else {
        t("threads")
    };
    let empty_label = if panel == ChatUtilityPanel::Pinned {
        format!("📌 {}", t("no-pinned-messages"))
    } else {
        format!("🧵 {}", t("no-threads"))
    };
    // Per-tab filter visibility and queries
    let filter_open = if panel == ChatUtilityPanel::Pinned {
        *pinned_filter_open.read()
    } else {
        *threads_filter_open.read()
    };
    let filter_query = if panel == ChatUtilityPanel::Pinned {
        pinned_filter_query.read().clone()
    } else {
        threads_filter_query.read().clone()
    };
    // Filtered pinned messages by query
    let filtered_pinned: Vec<Message> = if filter_query.is_empty() {
        pinned_messages.clone()
    } else {
        let q = filter_query.to_lowercase();
        pinned_messages
            .iter()
            .filter(|m| {
                let text = match &m.content {
                    poly_client::MessageContent::Text(t) => t.as_str(),
                    poly_client::MessageContent::WithAttachments { text, .. } => text.as_str(),
                };
                text.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    };

    rsx! {
        aside { class: "chat-utility-rail",
            div { class: "chat-utility-header",
                h3 { class: "chat-utility-title", "{title}" }
                // Per-tab filter toggle — shown on Pinned and Threads tabs only
                if panel == ChatUtilityPanel::Pinned || panel == ChatUtilityPanel::Threads {
                    button {
                        class: if filter_open { "header-btn active chat-utility-filter-btn" } else { "header-btn chat-utility-filter-btn" },
                        title: t("action-search"),
                        onclick: move |_| {
                            if panel == ChatUtilityPanel::Pinned {
                                let was_open = *pinned_filter_open.read();
                                pinned_filter_open.set(!was_open);
                                if was_open {
                                    pinned_filter_query.set(String::new());
                                }
                            } else {
                                let was_open = *threads_filter_open.read();
                                threads_filter_open.set(!was_open);
                                if was_open {
                                    threads_filter_query.set(String::new());
                                }
                            }
                        },
                        "🔍"
                    }
                }
            }
            // Per-tab filter input — shown when toggled on for Pinned/Threads
            if (panel == ChatUtilityPanel::Pinned || panel == ChatUtilityPanel::Threads) && filter_open {
                div { class: "chat-utility-filter-row",
                    input {
                        class: "chat-utility-filter-input",
                        r#type: "text",
                        placeholder: t("action-search"),
                        value: "{filter_query}",
                        oninput: move |e: Event<FormData>| {
                            let val = e.value();
                            if panel == ChatUtilityPanel::Pinned {
                                pinned_filter_query.set(val);
                            } else {
                                threads_filter_query.set(val);
                            }
                        },
                    }
                }
            }
            if panel == ChatUtilityPanel::Search {
                div { class: "chat-utility-body",
                    div { class: "chat-utility-search-box",
                        {search_ui}
                    }
                    if search_query.is_empty() || search_hits.is_empty() {
                        div { class: "utility-empty-state",
                            p { {t("search-no-results")} }
                        }
                    } else {
                        div { class: "search-results-list",
                            for hit in &search_hits {
                                SearchResultCard {
                                    hit: hit.clone(),
                                    search_terms: search_terms.clone(),
                                    on_open: move |hit| on_open_search_hit.call(hit),
                                }
                            }
                        }
                    }
                }
            } else if panel == ChatUtilityPanel::Pinned {
                div { class: "chat-utility-body",
                    if filtered_pinned.is_empty() {
                        div { class: "utility-empty-state",
                            p { "{empty_label}" }
                        }
                    } else {
                        div { class: "search-results-list",
                            for message in &filtered_pinned {
                                PinnedMessageCard {
                                    message: message.clone(),
                                    channel_name: current_channel_name.clone(),
                                    on_open: move |message| on_open_pinned.call(message),
                                }
                            }
                        }
                    }
                }
            } else if panel == ChatUtilityPanel::Settings {
                ChatSettingsPanel { notifications_muted }
            } else if panel == ChatUtilityPanel::Drafts {
                // B.5 — Pending drafts across all chats for the active account.
                {
                    let rail_app_state: BatchedSignal<AppState> = use_context();
                    let active_account_id = rail_nav.read().active_account_id
                        .as_deref()
                        .unwrap_or("")
                        .to_string();
                    rsx! {
                        DraftsSidebar {
                            account_id: active_account_id,
                            on_open_chat: move |(_account_id, _chat_id): (String, String)| {
                                // TODO: wire navigation when B.5 route is established.
                            },
                        }
                    }
                }
            } else if panel == ChatUtilityPanel::Agent {
                // Per-chat agent panel — same component as the standalone
                // wing-takeover used to mount, just rendered inside the
                // utility-rail tab so Search/Members/etc remain accessible.
                {
                    let rail_app_state: BatchedSignal<AppState> = use_context();
                    let active_account_id = rail_nav.read().active_account_id
                        .as_deref()
                        .unwrap_or("")
                        .to_string();
                    let active_chat_id = rail_nav.read().selected_channel
                        .as_deref()
                        .unwrap_or("")
                        .to_string();
                    rsx! {
                        AgentPanel {
                            account_id: active_account_id,
                            chat_id: active_chat_id,
                            chat_name: current_channel_name.clone(),
                        }
                    }
                }
            } else {
                div { class: "chat-utility-body",
                    div { class: "utility-empty-state",
                        p { "{empty_label}" }
                    }
                }
            }
        }
    }
}

// ── Card components (only used inside ChatUtilityRail) ────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn SearchResultCard(
    hit: MessageSearchHit,
    search_terms: Vec<String>,
    on_open: EventHandler<MessageSearchHit>,
) -> Element {
    let preview = message_plain_text(&hit.message.content);
    let preview_short = if preview.chars().count() > 140 {
        format!("{}…", preview.chars().take(140).collect::<String>())
    } else {
        preview
    };
    let timestamp = hit.message.timestamp.format("%d/%m/%Y, %H:%M").to_string();
    let avatar_url = hit.message.author.avatar_url.clone();
    let author_name = hit.message.author.display_name.clone();
    let fallback = author_name.chars().next().unwrap_or('?').to_string();
    let channel_label = hit
        .channel_name
        .clone()
        .unwrap_or_else(|| hit.channel_id.clone());

    rsx! {
        button {
            class: "search-result-card",
            onclick: move |_| on_open.call(hit.clone()),
            div { class: "search-result-channel", "# {channel_label}" }
            div { class: "search-result-content",
                div { class: "search-result-avatar",
                    if let Some(ref url) = avatar_url {
                        img {
                            class: "search-result-avatar-image",
                            src: "{url}",
                            alt: "{author_name}",
                        }
                    } else {
                        span { class: "search-result-avatar-fallback", "{fallback}" }
                    }
                }
                div { class: "search-result-copy",
                    div { class: "search-result-meta",
                        span { class: "search-result-author", "{author_name}" }
                        span { class: "search-result-time", "{timestamp}" }
                    }
                    div { class: "search-result-preview",
                        SearchPreviewText { text: preview_short, search_terms }
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn PinnedMessageCard(
    message: Message,
    channel_name: String,
    on_open: EventHandler<Message>,
) -> Element {
    let preview = message_plain_text(&message.content);
    let preview_short = if preview.chars().count() > 140 {
        format!("{}…", preview.chars().take(140).collect::<String>())
    } else {
        preview
    };
    let timestamp = message.timestamp.format("%d/%m/%Y, %H:%M").to_string();
    let avatar_url = message.author.avatar_url.clone();
    let author_name = message.author.display_name.clone();
    let fallback = author_name.chars().next().unwrap_or('?').to_string();

    rsx! {
        button {
            class: "search-result-card",
            onclick: move |_| on_open.call(message.clone()),
            div { class: "search-result-channel", "# {channel_name}" }
            div { class: "search-result-content",
                div { class: "search-result-avatar",
                    if let Some(ref url) = avatar_url {
                        img {
                            class: "search-result-avatar-image",
                            src: "{url}",
                            alt: "{author_name}",
                        }
                    } else {
                        span { class: "search-result-avatar-fallback", "{fallback}" }
                    }
                }
                div { class: "search-result-copy",
                    div { class: "search-result-meta",
                        span { class: "search-result-author", "{author_name}" }
                        span { class: "search-result-time", "{timestamp}" }
                    }
                    div { class: "search-result-preview", "{preview_short}" }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn SearchPreviewText(text: String, search_terms: Vec<String>) -> Element {
    let lowercase_text = text.to_lowercase();
    let found_match = search_terms.into_iter().find_map(|term| {
        let lowercase_term = term.to_lowercase();
        lowercase_text
            .find(&lowercase_term)
            .map(|index| (index, index.saturating_add(lowercase_term.len())))
    });

    if let Some((start, end)) = found_match {
        let before = text.get(..start).unwrap_or_default().to_string();
        let matched = text.get(start..end).unwrap_or_default().to_string();
        let after = text.get(end..).unwrap_or_default().to_string();
        rsx! {
            span {
                "{before}"
                mark { class: "search-result-match", "{matched}" }
                "{after}"
            }
        }
    } else {
        rsx! { span { "{text}" } }
    }
}

/// Chat settings panel — shown inside the utility rail when the ⚙️ tab is open.
///
/// Contains per-channel notification settings and member display preferences.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn ChatSettingsPanel(mut notifications_muted: Signal<bool>) -> Element {
    use crate::ui::settings::common::{PolySelect, SelectOption};
    let app_state: BatchedSignal<AppState> = use_context();
    let user_prefs: BatchedSignal<crate::state::UserPrefs> = use_context();
    let muted    = *notifications_muted.read();
    let grouping = user_prefs.read().member_list_grouping;
    let sort     = user_prefs.read().member_list_sort_order;
    let show_off = user_prefs.read().member_list_show_offline;

    let grouping_options = vec![
        SelectOption { value: "by-status", label: t("chat-settings-grouping-by-status") },
        SelectOption { value: "none",      label: t("chat-settings-grouping-none") },
    ];
    let sort_options = vec![
        SelectOption { value: "alphabetical", label: t("chat-settings-sort-alphabetical") },
        SelectOption { value: "online-first", label: t("chat-settings-sort-online-first") },
        SelectOption { value: "join-order",   label: t("chat-settings-sort-join-order") },
    ];

    rsx! {
        div { class: "chat-utility-body chat-settings-panel",

            // ── Notifications ────────────────────────────────────────────
            div { class: "chat-settings-section",
                h4 { class: "chat-settings-section-title", {t("chat-settings-notifications")} }
                label { class: "chat-settings-toggle-row",
                    button {
                        class: if muted { "chat-settings-mute-btn chat-settings-mute-btn-active" } else { "chat-settings-mute-btn" },
                        title: if muted { t("unmute-notifications") } else { t("mute-notifications") },
                        onclick: move |_| notifications_muted.set(!muted),
                        span { class: "chat-mute-bell-icon",
                            span { class: "chat-mute-bell-base", "🔔" }
                            if muted {
                                span { class: "chat-mute-bell-strike" }
                            }
                        }
                        span { class: "chat-settings-toggle-label",
                            if muted { {t("unmute-notifications")} } else { {t("mute-notifications")} }
                        }
                    }
                }
            }

            // ── Member List ──────────────────────────────────────────────
            div { class: "chat-settings-section",
                h4 { class: "chat-settings-section-title", {t("chat-settings-member-list")} }

                // Grouping
                div { class: "chat-settings-row",
                    label { class: "chat-settings-label", {t("chat-settings-grouping")} }
                    PolySelect {
                        options: grouping_options,
                        value: grouping.as_str().to_string(),
                        onchange: move |v: String| {
                            let g = crate::state::MemberListGrouping::from_slug(&v);
                            let (s, o) = user_prefs.map(|p| (p.member_list_sort_order, p.member_list_show_offline));
                            user_prefs.batch(|p| p.member_list_grouping = g);
                            spawn(async move { persist_member_list_display_settings(g, s, o).await; });
                        },
                    }
                }

                // Sort order
                div { class: "chat-settings-row",
                    label { class: "chat-settings-label", {t("chat-settings-sort-order")} }
                    PolySelect {
                        options: sort_options,
                        value: sort.as_str().to_string(),
                        onchange: move |v: String| {
                            let s = crate::state::MemberListSortOrder::from_slug(&v);
                            let (g, o) = user_prefs.map(|p| (p.member_list_grouping, p.member_list_show_offline));
                            user_prefs.batch(|p| p.member_list_sort_order = s);
                            spawn(async move { persist_member_list_display_settings(g, s, o).await; });
                        },
                    }
                }

                // Show offline toggle
                div { class: "chat-settings-row",
                    label { class: "chat-settings-label", {t("chat-settings-show-offline")} }
                    button {
                        class: if show_off { "chat-settings-toggle-btn chat-settings-toggle-btn-on" } else { "chat-settings-toggle-btn" },
                        onclick: move |_| {
                            let (prev, g, s) = user_prefs.map(|p| (p.member_list_show_offline, p.member_list_grouping, p.member_list_sort_order));
                            let new_val = !prev;
                            user_prefs.batch(|p| p.member_list_show_offline = new_val);
                            spawn(async move { persist_member_list_display_settings(g, s, new_val).await; });
                        },
                        span { class: "chat-settings-toggle-track",
                            span { class: if show_off { "chat-settings-toggle-knob on" } else { "chat-settings-toggle-knob" } }
                        }
                    }
                }
            }
        }
    }
}

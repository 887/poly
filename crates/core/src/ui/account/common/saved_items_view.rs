//! Saved items view — aggregated pinned messages across DMs and groups.
//!
//! Unlike the old self-DM shortcut, this page behaves more like notifications:
//! it shows pinned messages from all conversations for the active account and
//! lets the user jump directly back to the source DM/group message.

use crate::state::BatchedSignal;
use super::VoiceAccountFooter;
use super::chat_view::{highlight_message, open_message_hit};
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{AppState, ChatData};
use crate::ui::split_shell::SplitMenuShell;
use dioxus::prelude::*;
use poly_client::{MessageContent, MessageSearchHit};
use poly_ui_macros::{context_menu, ui_action};

#[derive(Clone, PartialEq)]
struct SavedPinnedItem {
    channel_name: String,
    hit: MessageSearchHit,
}

#[derive(Clone, PartialEq)]
struct SavedSourceSummary {
    channel_id: String,
    channel_name: String,
    latest_timestamp: chrono::DateTime<chrono::Utc>,
    count: usize,
}

fn channel_preview_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::WithAttachments { text, .. } => text.clone(),
    }
}

fn build_highlight_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[ui_action(None)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn HighlightedSavedText(text: String, search_terms: Vec<String>) -> Element {
    let lowercase_text = text.to_lowercase();
    let found_match = search_terms.into_iter().find_map(|term| {
        let lowercase_term = term.to_lowercase();
        lowercase_text
            .find(&lowercase_term)
            .map(|index| (index, index + lowercase_term.len()))
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

fn build_saved_sources(items: &[SavedPinnedItem]) -> Vec<SavedSourceSummary> {
    let mut sources = Vec::<SavedSourceSummary>::new();

    for item in items {
        if let Some(existing) = sources
            .iter_mut()
            .find(|source| source.channel_id == item.hit.channel_id)
        {
            existing.count += 1;
            if item.hit.message.timestamp > existing.latest_timestamp {
                existing.latest_timestamp = item.hit.message.timestamp;
            }
            continue;
        }

        sources.push(SavedSourceSummary {
            channel_id: item.hit.channel_id.clone(),
            channel_name: item.channel_name.clone(),
            latest_timestamp: item.hit.message.timestamp,
            count: 1,
        });
    }

    sources.sort_by(|a, b| b.latest_timestamp.cmp(&a.latest_timestamp));
    sources
}

#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
pub fn SavedItemsView() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let nav = navigator();
    let title = t("saved-items-title");
    let description = t("saved-items-description");
    let empty_text = t("saved-items-empty");
    let loading_text = t("chat-loading");
    let mut source_search = use_signal(String::new);
    let mut selected_source = use_signal(|| None::<String>);

    let account_id = app_state.read().nav.active_account_id.cloned().unwrap_or_default();
    let active_user_id = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .map(|session| session.user.id.clone());
    let dm_channels: Vec<_> = chat_data
        .read()
        .dm_channels
        .iter()
        .filter(|dm| {
            dm.account_id == account_id
                && active_user_id.as_deref() != Some(dm.user.id.as_str())
        })
        .cloned()
        .collect();
    let groups: Vec<_> = chat_data
        .read()
        .groups
        .iter()
        .filter(|group| group.account_id == account_id)
        .cloned()
        .collect();

    let saved_items = use_resource(move || {
        let account_id = account_id.clone();
        let dm_channels = dm_channels.clone();
        let groups = groups.clone();
        async move {
            let Some(backend) = client_manager.read().get_backend(&account_id) else {
                return Vec::<SavedPinnedItem>::new();
            };

            let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                Ok(g) => g,
                Err(_) => {
                    tracing::warn!("saved_items: backend read timed out");
                    return Vec::new();
                }
            };
            let mut items = Vec::new();

            for dm in dm_channels {
                if let Ok(messages) = guard.get_pinned_messages(&dm.id).await {
                    for message in messages {
                        items.push(SavedPinnedItem {
                            channel_name: dm.user.display_name.clone(),
                            hit: MessageSearchHit {
                                channel_id: dm.id.clone(),
                                channel_name: Some(dm.user.display_name.clone()),
                                server_id: None,
                                message,
                            },
                        });
                    }
                }
            }

            for group in groups {
                if let Ok(messages) = guard.get_pinned_messages(&group.id).await {
                    let group_name = group.name.clone().unwrap_or_else(|| {
                        group
                            .members
                            .iter()
                            .map(|member| member.display_name.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    });

                    for message in messages {
                        items.push(SavedPinnedItem {
                            channel_name: group_name.clone(),
                            hit: MessageSearchHit {
                                channel_id: group.id.clone(),
                                channel_name: Some(group_name.clone()),
                                server_id: None,
                                message,
                            },
                        });
                    }
                }
            }

            items.sort_by(|a, b| b.hit.message.timestamp.cmp(&a.hit.message.timestamp));
            items
        }
    });

    let loaded_items = saved_items.read().as_ref().cloned();
    let selected_source_id = selected_source.read().clone();
    let source_query = source_search.read().clone();
    let source_filter_text = source_query.to_lowercase();
    let highlight_terms = build_highlight_terms(&source_query);
    let filtered_sources = loaded_items.as_ref().map(|items| {
        build_saved_sources(items)
            .into_iter()
            .filter(|source| {
                source_filter_text.is_empty()
                    || source.channel_name.to_lowercase().contains(&source_filter_text)
            })
            .collect::<Vec<_>>()
    });
    let visible_items = loaded_items.as_ref().map(|items| {
        items.iter()
            .filter(|item| {
                let preview = channel_preview_text(&item.hit.message.content).to_lowercase();
                selected_source_id
                    .as_ref()
                    .is_none_or(|source_id| item.hit.channel_id == *source_id)
                    && (source_filter_text.is_empty() || preview.contains(&source_filter_text))
            })
            .cloned()
            .collect::<Vec<_>>()
    });

    rsx! {
        SplitMenuShell {
            root_class: "saved-items-shell".to_string(),
            sidebar_class: "special-page-sidebar saved-items-sidebar".to_string(),
            content_class: "special-page-content saved-items-content".to_string(),
            sidebar: rsx! {
                div { class: "special-page-sidebar-header",
                    h2 { class: "special-page-sidebar-title", "{title}" }
                    p { class: "special-page-sidebar-description", "{description}" }
                }
                // sidebar header only; no filter input here (mobile-friendly)
                div { class: "special-page-sidebar-nav saved-sources-list",
                    if let Some(items) = &loaded_items {
                        SidebarSourceButton {
                            label: t("saved-items-all-sources"),
                            count: items.len(),
                            active: selected_source_id.is_none(),
                            onclick: move |_| selected_source.set(None),
                        }
                        if let Some(sources) = &filtered_sources {
                            if sources.is_empty() {
                                div { class: "special-page-sidebar-empty", "{t(\"saved-items-sources-empty\")}" }
                            } else {
                                for source in sources {
                                    {
                                        let source_id = source.channel_id.clone();
                                        rsx! {
                                            SidebarSourceButton {
                                                key: "{source.channel_id}",
                                                label: source.channel_name.clone(),
                                                count: source.count,
                                                active: selected_source_id.as_deref() == Some(source_id.as_str()),
                                                onclick: move |_| selected_source.set(Some(source_id.clone())),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "special-page-sidebar-empty", "{loading_text}" }
                    }
                }
                VoiceAccountFooter {}
            },
            content: rsx! {
                div { class: "special-page-panel",
                    div { class: "special-page-header",
                        h2 { class: "special-page-title", "{title}" }
                        p { class: "settings-description", "{description}" }
                        div { class: "search-page-input-bar special-page-search-bar",
                            input {
                                r#type: "text",
                                class: "search-page-input special-page-search",
                                placeholder: "{t(\"saved-items-filter-placeholder\")}",
                                value: "{source_query}",
                                oninput: move |evt| source_search.set(evt.value().clone()),
                            }
                            if !source_query.is_empty() {
                                button {
                                    class: "search-page-clear",
                                    onclick: move |_| source_search.set(String::new()),
                                    "×"
                                }
                            }
                        }
                    }
                    div { class: "notification-list saved-items-results",
                        if let Some(items) = &visible_items {
                            if items.is_empty() {
                                div { class: "notifications-empty special-page-empty-state",
                                    p { "{empty_text}" }
                                }
                            } else {
                                for item in items {
                                    SavedPinnedItemCard {
                                        key: "{item.hit.channel_id}-{item.hit.message.id}",
                                        item: item.clone(),
                                        highlight_terms: highlight_terms.clone(),
                                        on_open: move |hit: MessageSearchHit| {
                                            let current_channel_id = app_state.read().nav.selected_channel.cloned();
                                            let current_server_id = app_state.read().nav.selected_server.cloned();
                                            spawn(async move {
                                                if let Some((route, message_id)) = open_message_hit(
                                                    hit,
                                                    current_channel_id,
                                                    current_server_id,
                                                    client_manager,
                                                    chat_data,
                                                    app_state,
                                                ).await {
                                                    nav.push(route);
                                                    highlight_message(&message_id);
                                                }
                                            });
                                            crate::ui::main_layout::close_mobile_drawer();
                                        },
                                    }
                                }
                            }
                        } else {
                            div { class: "notifications-empty special-page-empty-state",
                                p { "{loading_text}" }
                            }
                        }
                    }
                }
            },
        }
    }
}

#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn SidebarSourceButton(
    label: String,
    count: usize,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if active {
        "special-page-sidebar-button special-page-sidebar-button-with-count active"
    } else {
        "special-page-sidebar-button special-page-sidebar-button-with-count"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |evt| onclick.call(evt),
            span { class: "special-page-sidebar-button-label", "{label}" }
            span { class: "special-page-sidebar-count", "{count}" }
        }
    }
}

#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn SavedPinnedItemCard(
    item: SavedPinnedItem,
    highlight_terms: Vec<String>,
    on_open: EventHandler<MessageSearchHit>,
) -> Element {
    let preview = channel_preview_text(&item.hit.message.content);
    let preview_short = if preview.chars().count() > 140 {
        format!("{}…", preview.chars().take(140).collect::<String>())
    } else {
        preview
    };
    let timestamp = item.hit.message.timestamp.format("%d/%m/%Y, %H:%M").to_string();
    let avatar_url = item.hit.message.author.avatar_url.clone();
    let author_name = item.hit.message.author.display_name.clone();
    let fallback = author_name.chars().next().unwrap_or('?').to_string();

    rsx! {
        button {
            class: "search-result-card",
            onclick: move |_| on_open.call(item.hit.clone()),
            div { class: "search-result-channel", "# {item.channel_name}" }
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
                        HighlightedSavedText { text: preview_short, search_terms: highlight_terms }
                    }
                }
            }
        }
    }
}

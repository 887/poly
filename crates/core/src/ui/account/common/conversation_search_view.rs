//! Account-scoped conversation search view.
//!
//! Unlike the global `/search` route, this page lives inside the DM layout so
//! the left-side direct-message shell remains visible. Results are scoped to the
//! currently active account and only include DMs and group conversations.

use crate::state::BatchedSignal;
use crate::i18n::{t, t_args};
use crate::state::{AppState, ChatData};
use crate::ui::main_layout::close_mobile_drawer;
use crate::ui::routes::Route;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

fn user_color(account_id: &str) -> String {
    let hash: u32 = account_id.bytes().fold(5381_u32, |h, b| {
        h.wrapping_mul(33).wrapping_add(u32::from(b))
    });
    let hue = hash % 360;
    format!("hsl({hue}, 65%, 55%)")
}

fn dm_last_incoming_timestamp(dm: &poly_client::DmChannel) -> Option<DateTime<Utc>> {
    dm.last_message
        .as_ref()
        .filter(|message| message.author.id == dm.user.id)
        .map(|message| message.timestamp)
}

fn group_last_incoming_timestamp(
    group: &poly_client::Group,
    active_user_id: Option<&str>,
) -> Option<DateTime<Utc>> {
    group
        .last_message
        .as_ref()
        .filter(|message| active_user_id.is_none_or(|user_id| message.author.id != user_id))
        .map(|message| message.timestamp)
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ConversationSearchInput(query: Signal<String>) -> Element {
    let current = query.read().clone();

    use_effect(|| {
        let _ = document::eval(
            "setTimeout(() => { const el = document.querySelector('.search-page-input'); if (el) el.focus(); }, 50)"
        );
    });

    rsx! {
        div { class: "search-page-input-bar",
            input {
                r#type: "text",
                class: "search-page-input",
                placeholder: "{t(\"search-page-placeholder\")}",
                value: "{current}",
                oninput: move |e| query.set(e.value()),
            }
            if !current.is_empty() {
                button {
                    class: "search-page-clear",
                    onclick: move |_| query.set(String::new()),
                    "×"
                }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn AvatarIcon(url: Option<String>, label: String, color: String) -> Element {
    let initial = label
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    rsx! {
        if let Some(img_url) = url {
            img {
                class: "search-avatar-icon",
                src: "{img_url}",
                alt: "{label}",
            }
        } else {
            div {
                class: "search-avatar-icon search-avatar-icon-fallback",
                style: "background: {color};",
                "{initial}"
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AvatarNodeRow(
    avatar_url: Option<String>,
    avatar_label: String,
    avatar_color: String,
    label: String,
    sublabel: String,
    on_click: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "search-node-row",
            onclick: move |evt| on_click.call(evt),
            AvatarIcon {
                url: avatar_url,
                label: avatar_label,
                color: avatar_color,
            }
            div { class: "search-node-info",
                span { class: "search-node-label", "{label}" }
                if !sublabel.is_empty() {
                    span { class: "search-node-sublabel", "{sublabel}" }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ConversationTypeFilters(enabled_types: Signal<std::collections::HashSet<String>>) -> Element {
    let types: &[(&str, &str)] = &[("dms", "search-type-dms"), ("groups", "search-type-groups")];
    rsx! {
        div { class: "search-type-filters",
            span { class: "search-type-filters-label", "{t(\"search-page-type-filter\")}" }
            for (type_key, i18n_key) in types {
                {
                    let tk = type_key.to_string();
                    let checked = enabled_types.read().contains(*type_key);
                    rsx! {
                        label { class: "search-type-filter-item",
                            input {
                                r#type: "checkbox",
                                checked,
                                onchange: move |_| {
                                    let mut set = enabled_types.write();
                                    if set.contains(&tk) {
                                        set.remove(&tk);
                                    } else {
                                        set.insert(tk.clone());
                                    }
                                },
                            }
                            span { "{t(i18n_key)}" }
                        }
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ConversationSearchView() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();
    let query = use_signal(String::new);
    let enabled_types: Signal<std::collections::HashSet<String>> = use_signal(|| {
        ["dms", "groups"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    });
    let mut dm_visible: Signal<usize> = use_signal(|| 20_usize);
    let mut grp_visible: Signal<usize> = use_signal(|| 20_usize);

    let active_account_id = app_state.read().nav.active_account_id.cloned().unwrap_or_default();
    let active_user_id = chat_data
        .read()
        .account_sessions
        .get(&active_account_id)
        .map(|session| session.user.id.clone());
    let q_lower = query.read().to_lowercase();

    let mut dm_channels: Vec<_> = chat_data
        .read()
        .dm_channels
        .iter()
        .filter(|dm| dm.account_id == active_account_id)
        .cloned()
        .collect();
    let mut groups: Vec<_> = chat_data
        .read()
        .groups
        .iter()
        .filter(|group| group.account_id == active_account_id)
        .cloned()
        .collect();

    dm_channels.sort_by(|a, b| {
        dm_last_incoming_timestamp(b)
            .cmp(&dm_last_incoming_timestamp(a))
            .then_with(|| b.last_message.as_ref().map(|m| m.timestamp).cmp(&a.last_message.as_ref().map(|m| m.timestamp)))
            .then_with(|| a.user.display_name.cmp(&b.user.display_name))
    });
    groups.sort_by(|a, b| {
        group_last_incoming_timestamp(b, active_user_id.as_deref())
            .cmp(&group_last_incoming_timestamp(a, active_user_id.as_deref()))
            .then_with(|| b.last_message.as_ref().map(|m| m.timestamp).cmp(&a.last_message.as_ref().map(|m| m.timestamp)))
            .then_with(|| a.name.cmp(&b.name))
    });

    let visible_dms: Vec<_> = dm_channels
        .iter()
        .filter(|dm| q_lower.is_empty() || dm.user.display_name.to_lowercase().contains(&q_lower))
        .cloned()
        .collect();
    let visible_grps: Vec<_> = groups
        .iter()
        .filter(|group| {
            let name = group.name.clone().unwrap_or_else(|| {
                group.members.iter().map(|m| m.display_name.clone()).collect::<Vec<_>>().join(", ")
            });
            q_lower.is_empty() || name.to_lowercase().contains(&q_lower)
        })
        .cloned()
        .collect();

    let dm_total = visible_dms.len();
    let grp_total = visible_grps.len();
    let account_label = chat_data
        .read()
        .account_sessions
        .get(&active_account_id)
        .map(|session| session.user.display_name.clone())
        .unwrap_or_else(|| active_account_id.clone());
    let instance_id = chat_data
        .read()
        .account_sessions
        .get(&active_account_id)
        .map(|session| session.instance_id.clone())
        .unwrap_or_default();
    let backend_slug = chat_data
        .read()
        .account_sessions
        .get(&active_account_id)
        .map(|session| session.user.backend.slug().to_string())
        .unwrap_or_else(|| "demo".to_string());

    rsx! {
        main { class: "search-page-results search-page-results-embedded",
            div { class: "search-page-header",
                h2 { "{t(\"conversation-search-title\")}" }
                p { class: "search-node-sublabel", "{t_args(\"conversation-search-description\", &[(\"account\", &account_label)])}" }
                ConversationSearchInput { query }
                ConversationTypeFilters { enabled_types }
            }
            div {
                class: "search-page-results-tree",
                onscroll: move |_| {
                    spawn(async move {
                        let js = "(() => { const el = document.querySelector('.search-page-results-embedded'); if (!el) return false; return el.scrollTop + el.clientHeight >= el.scrollHeight - 300; })()";
                        let near_bottom = dioxus::document::eval(js)
                            .await
                            .ok()
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if near_bottom {
                            *dm_visible.write() += 20;
                            *grp_visible.write() += 20;
                        }
                    });
                },

                if enabled_types.read().contains("dms") && !visible_dms.is_empty() {
                    div { class: "search-section-header",
                        span { "{t(\"search-page-dms\")}" }
                        span { class: "search-section-count",
                            {
                                let shown = (*dm_visible.read()).min(dm_total);
                                t_args("search-showing-of", &[("count", &shown.to_string()), ("total", &dm_total.to_string())])
                            }
                        }
                    }
                    for dm in visible_dms.iter().take(*dm_visible.read()) {
                        {
                            let dm_id = dm.id.clone();
                            let aid = dm.account_id.clone();
                            let row_backend_slug = backend_slug.clone();
                            let row_instance_id = instance_id.clone();
                            let dm_avatar_url = dm.user.avatar_url.clone();
                            let dm_display_name = dm.user.display_name.clone();
                            let dm_color = user_color(&dm.user.id);
                            rsx! {
                                AvatarNodeRow {
                                    key: "{dm.id}-{aid}",
                                    avatar_url: dm_avatar_url,
                                    avatar_label: dm_display_name.clone(),
                                    avatar_color: dm_color,
                                    label: dm_display_name,
                                    sublabel: account_label.clone(),
                                    on_click: move |_| {
                                        close_mobile_drawer();
                                        crate::nav!(Route::DmChat {
                                            backend: row_backend_slug.clone(),
                                            instance_id: row_instance_id.clone(),
                                            account_id: aid.clone(),
                                            dm_id: dm_id.clone(),
                                        });
                                    },
                                }
                            }
                        }
                    }
                }

                if enabled_types.read().contains("groups") && !visible_grps.is_empty() {
                    div { class: "search-section-header",
                        span { "{t(\"search-page-groups\")}" }
                        span { class: "search-section-count",
                            {
                                let shown = (*grp_visible.read()).min(grp_total);
                                t_args("search-showing-of", &[("count", &shown.to_string()), ("total", &grp_total.to_string())])
                            }
                        }
                    }
                    for group in visible_grps.iter().take(*grp_visible.read()) {
                        {
                            let name = group.name.clone().unwrap_or_else(|| {
                                group.members.iter().map(|m| m.display_name.clone()).collect::<Vec<_>>().join(", ")
                            });
                            let gid = group.id.clone();
                            let aid = group.account_id.clone();
                            let row_backend_slug = backend_slug.clone();
                            let row_instance_id = instance_id.clone();
                            let grp_avatar_url = group.members.first().and_then(|m| m.avatar_url.clone());
                            let grp_color = user_color(&group.id);
                            rsx! {
                                AvatarNodeRow {
                                    key: "{group.id}-{aid}",
                                    avatar_url: grp_avatar_url,
                                    avatar_label: name.clone(),
                                    avatar_color: grp_color,
                                    label: name,
                                    sublabel: account_label.clone(),
                                    on_click: move |_| {
                                        close_mobile_drawer();
                                        crate::nav!(Route::DmChat {
                                            backend: row_backend_slug.clone(),
                                            instance_id: row_instance_id.clone(),
                                            account_id: aid.clone(),
                                            dm_id: gid.clone(),
                                        });
                                    },
                                }
                            }
                        }
                    }
                }

                if visible_dms.is_empty() && visible_grps.is_empty() {
                    div { class: "notifications-empty",
                        p { "{t(\"dm-no-results\")}" }
                    }
                }
            }
        }
    }
}

//! Content & Social settings page.
//!
//! Per-account controls for:
//! - Sensitive media filters (DMs from friends, DMs from others, server channels)
//! - DM spam filter aggressiveness
//! - Age-restricted content access
//! - Social permissions (who can DM, message requests)
//! - Friend request origin filters
//! - Blocked users list with unblock action
//!
//! All settings are read from `ChatData::content_policy` / `ChatData::blocked_users`
//! and written back immediately on change.
//!
//! # Backend sync
//! Writing to `ChatData` is the source of truth for the running session.
//! TODO(phase-3.x): call `set_content_policy` / `unblock_user` on the active backend
//! handle to persist changes server-side, mirroring the `toggle_demo` pattern in
//! `crates/core/src/ui/demo.rs`.
//!
//! # 150-line component rule
//! Every `#[component]` fn body in this module MUST stay under **150 lines**
//! of RSX + logic. Extract sub-components rather than growing any file.

use crate::i18n::t;
use crate::state::ChatData;
use dioxus::prelude::*;
use poly_client::{DmSpamFilterLevel, SensitiveContentLevel};

// ─── sub-components ─────────────────────────────────────────────────────────

/// A single select-row inside the Sensitive Media section.
///
/// Renders a label + `<select>` for a [`SensitiveContentLevel`].
#[rustfmt::skip]
#[component]
fn SensitiveMediaRow(
    label: String,
    value: SensitiveContentLevel,
    on_change: EventHandler<SensitiveContentLevel>,
) -> Element {
    rsx! {
        div { class: "content-social-select-row",
            span { class: "content-social-select-label", "{label}" }
            select {
                class: "content-social-select",
                onchange: move |e| {
                    let level = match e.value().as_str() {
                        "show" => SensitiveContentLevel::Show,
                        "warn" => SensitiveContentLevel::WarnFirst,
                        _ => SensitiveContentLevel::Hide,
                    };
                    on_change.call(level);
                },
                option { value: "hide", selected: value == SensitiveContentLevel::Hide, "{t(\"content-social-hide\")}" }
                option { value: "show", selected: value == SensitiveContentLevel::Show, "{t(\"content-social-show\")}" }
                option { value: "warn", selected: value == SensitiveContentLevel::WarnFirst, "{t(\"content-social-warn\")}" }
            }
        }
    }
}

/// A labeled checkbox toggle row.
#[rustfmt::skip]
#[component]
fn ToggleRow(label: String, checked: bool, on_change: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "content-social-toggle-row",
            span { class: "content-social-toggle-label", "{label}" }
            input {
                r#type: "checkbox",
                class: "content-social-toggle",
                checked,
                onchange: move |e| on_change.call(e.checked()),
            }
        }
    }
}

/// Sensitive Media section — three select rows.
#[rustfmt::skip]
#[component]
fn SensitiveMediaSection(mut chat_data: Signal<ChatData>) -> Element {
    let policy = chat_data.read().content_policy.clone();
    rsx! {
        div { class: "content-social-section",
            div { class: "content-social-section-header",
                h3 { class: "content-social-section-title", "{t(\"content-social-sensitive-media\")}" }
                p { class: "content-social-section-desc", "{t(\"content-social-sensitive-media-desc\")}" }
            }
            SensitiveMediaRow {
                label: t("content-social-dm-friends"),
                value: policy.sensitive_content_dm_friends,
                on_change: move |level| {
                    chat_data.write().content_policy.sensitive_content_dm_friends = level;
                },
            }
            SensitiveMediaRow {
                label: t("content-social-dm-others"),
                value: policy.sensitive_content_dm_others,
                on_change: move |level| {
                    chat_data.write().content_policy.sensitive_content_dm_others = level;
                },
            }
            SensitiveMediaRow {
                label: t("content-social-server-channels"),
                value: policy.sensitive_content_server_channels,
                on_change: move |level| {
                    chat_data.write().content_policy.sensitive_content_server_channels = level;
                },
            }
        }
    }
}

/// DM Spam Filter section — three radio options.
#[rustfmt::skip]
#[component]
fn SpamFilterSection(mut chat_data: Signal<ChatData>) -> Element {
    let current = chat_data.read().content_policy.dm_spam_filter;
    rsx! {
        div { class: "content-social-section",
            div { class: "content-social-section-header",
                h3 { class: "content-social-section-title", "{t(\"content-social-spam-filter\")}" }
                p  { class: "content-social-section-desc", "{t(\"content-social-spam-filter-desc\")}" }
            }
            div { class: "content-social-radio-group",
                for (value, label_key) in [
                    (DmSpamFilterLevel::FilterAll, "content-social-filter-all"),
                    (DmSpamFilterLevel::FilterNonFriends, "content-social-filter-non-friends"),
                    (DmSpamFilterLevel::DoNotFilter, "content-social-filter-none"),
                ] {
                    {
                        let is_checked = current == value;
                        rsx! {
                            label { class: "content-social-radio-row",
                                input {
                                    r#type: "radio",
                                    name: "dm-spam-filter",
                                    checked: is_checked,
                                    onchange: move |_| {
                                        chat_data.write().content_policy.dm_spam_filter = value;
                                    },
                                }
                                span { "{t(label_key)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Age-Restricted Content + Social Permissions (grouped for compactness).
#[rustfmt::skip]
#[component]
fn AgeAndSocialSection(mut chat_data: Signal<ChatData>) -> Element {
    let policy = chat_data.read().content_policy.clone();
    rsx! {
        div { class: "content-social-section",
            h3 { class: "content-social-section-title", "{t(\"content-social-age-restricted\")}" }
            ToggleRow {
                label: t("content-social-age-restricted-servers"),
                checked: policy.allow_age_restricted_servers,
                on_change: move |val| {
                    chat_data.write().content_policy.allow_age_restricted_servers = val;
                },
            }
            ToggleRow {
                label: t("content-social-age-restricted-commands"),
                checked: policy.allow_age_restricted_commands_in_dms,
                on_change: move |val| {
                    chat_data.write().content_policy.allow_age_restricted_commands_in_dms = val;
                },
            }
        }
        div { class: "content-social-section",
            div { class: "content-social-section-header",
                h3 { class: "content-social-section-title", "{t(\"content-social-social-perms\")}" }
                p  { class: "content-social-section-desc", "{t(\"content-social-social-perms-desc\")}" }
            }
            ToggleRow {
                label: t("content-social-dms-from-members"),
                checked: policy.allow_dms_from_server_members,
                on_change: move |val| {
                    chat_data.write().content_policy.allow_dms_from_server_members = val;
                },
            }
            ToggleRow {
                label: t("content-social-message-requests"),
                checked: policy.allow_message_requests,
                on_change: move |val| {
                    chat_data.write().content_policy.allow_message_requests = val;
                },
            }
        }
    }
}

/// Friend Requests section — three permission checkboxes.
#[rustfmt::skip]
#[component]
fn FriendRequestsSection(mut chat_data: Signal<ChatData>) -> Element {
    let policy = chat_data.read().content_policy.clone();
    rsx! {
        div { class: "content-social-section",
            h3 { class: "content-social-section-title", "{t(\"content-social-friend-requests\")}" }
            ToggleRow {
                label: t("content-social-fr-everyone"),
                checked: policy.friend_request_from_everyone,
                on_change: move |val| {
                    chat_data.write().content_policy.friend_request_from_everyone = val;
                },
            }
            ToggleRow {
                label: t("content-social-fr-friends-of-friends"),
                checked: policy.friend_request_from_friends_of_friends,
                on_change: move |val| {
                    chat_data.write().content_policy.friend_request_from_friends_of_friends = val;
                },
            }
            ToggleRow {
                label: t("content-social-fr-server-members"),
                checked: policy.friend_request_from_server_members,
                on_change: move |val| {
                    chat_data.write().content_policy.friend_request_from_server_members = val;
                },
            }
        }
    }
}

/// Blocked Users section — list each blocked user with an Unblock button.
#[rustfmt::skip]
#[component]
fn BlockedUsersSection(mut chat_data: Signal<ChatData>) -> Element {
    let blocked = chat_data.read().blocked_users.clone();
    rsx! {
        div { class: "content-social-section",
            div { class: "content-social-section-header",
                h3 { class: "content-social-section-title", "{t(\"content-social-blocked\")}" }
                p  { class: "content-social-section-desc", "{t(\"content-social-blocked-desc\")}" }
            }
            if blocked.is_empty() {
                p { class: "content-social-no-blocked", "{t(\"content-social-no-blocked\")}" }
            } else {
                div { class: "content-social-blocked-list",
                    for user in &blocked {
                        {
                            let uid = user.user_id.clone();
                            let display = user.display_name.clone();
                            let avatar = user.avatar_url.clone();
                            let first_char = display.chars().next().unwrap_or('?');
                            rsx! {
                                div { class: "content-social-blocked-row", key: "{uid}",
                                    if let Some(ref url) = avatar {
                                        img {
                                            class: "content-social-blocked-avatar",
                                            src: "{url}",
                                            alt: "",
                                        }
                                    } else {
                                        div { class: "content-social-blocked-avatar content-social-blocked-avatar-placeholder",
                                            "{first_char}"
                                        }
                                    }
                                    span { class: "content-social-blocked-name", "{display}" }
                                    button {
                                        class: "content-social-unblock-btn",
                                        onclick: move |_| {
                                            // TODO(phase-3.x): call unblock_user on the active backend
                                            chat_data.write().blocked_users.retain(|u| u.user_id != uid);
                                        },
                                        "{t(\"content-social-unblock\")}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── entry point ────────────────────────────────────────────────────────────

/// Content & Social settings page for a single account.
///
/// Reads policy from `ChatData::content_policy` and blocked users from
/// `ChatData::blocked_users`. All writes go directly back to `ChatData`.
///
/// Rendered by [`crate::ui::account::settings::AccountSettingsPage`] when the
/// "content-social" section is active.
#[rustfmt::skip]
#[component]
pub fn ContentSocialSettings(_account_id: String) -> Element {
    let chat_data = use_context::<Signal<ChatData>>();
    rsx! {
        div { class: "settings-section-content",
            h2 { class: "settings-section-title", "{t(\"content-social-title\")}" }
            SensitiveMediaSection { chat_data }
            SpamFilterSection { chat_data }
            AgeAndSocialSection { chat_data }
            FriendRequestsSection { chat_data }
            BlockedUsersSection { chat_data }
        }
    }
}

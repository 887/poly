//! Content & Social settings page.
//!
//! Per-account controls for:
//! - Sensitive media filters (DMs from friends, DMs from others, server channels)
//! - DM spam filter aggressiveness
//! - Social permissions (who can DM, message requests)
//! - Friend request origin filters
//! - Age-restricted content access
//!
//! All settings are read from `AccountSessions::content_policy` and written back immediately on change.
//!
//! Blocked users are managed in the People page, not in account settings.
//!
//! # Backend sync
//! Writing to `AccountSessions` is the source of truth for the running session.
//! TODO(phase-3.x): call `set_content_policy` on the active backend
//! handle to persist changes server-side, mirroring the `toggle_demo` pattern in
//! `crates/core/src/ui/demo.rs`.
//!
//! # 150-line component rule
//! Every `#[component]` fn body in this module MUST stay under **150 lines**
//! of RSX + logic. Extract sub-components rather than growing any file.

use crate::client_manager::ClientManager;
use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::state::AccountSessions;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_client::{DmSpamFilterLevel, SensitiveContentLevel};
use poly_ui_macros::{context_menu, ui_action};

// ─── sub-components ─────────────────────────────────────────────────────────

/// A single select-row inside the Sensitive Media section.
///
/// Renders a label + `<select>` for a [`SensitiveContentLevel`].
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
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
                value: match value {
                    SensitiveContentLevel::Hide => "hide",
                    SensitiveContentLevel::WarnFirst => "warn",
                    SensitiveContentLevel::Show => "show",
                },
                onchange: move |evt| {
                    let v = match evt.value().as_str() {
                        "show" => SensitiveContentLevel::Show,
                        "warn" => SensitiveContentLevel::WarnFirst,
                        _ => SensitiveContentLevel::Hide,
                    };
                    on_change.call(v);
                },
                option { value: "hide", "{t(\"content-social-level-hide\")}" }
                option { value: "warn", "{t(\"content-social-level-blur\")}" }
                option { value: "show", "{t(\"content-social-level-show\")}" }
            }
        }
    }
}

/// A toggle row with a label + checkbox.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn ToggleRow(label: String, checked: bool, on_change: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "content-social-toggle-row",
            input {
                r#type: "checkbox",
                checked,
                onchange: move |evt| on_change.call(evt.checked()),
            }
            span { "{label}" }
        }
    }
}

pub enum SensitiveMediaSectionAction {
    SetDmFriends(SensitiveContentLevel),
    SetDmOthers(SensitiveContentLevel),
    SetServerChannels(SensitiveContentLevel),
}

impl UiAction for SensitiveMediaSectionAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetDmFriends(_) => todo!("phase-E: update sensitive_content_dm_friends"),
            Self::SetDmOthers(_) => todo!("phase-E: update sensitive_content_dm_others"),
            Self::SetServerChannels(_) => todo!("phase-E: update sensitive_content_server_channels"),
        }
    }
}

/// Sensitive Media section — three select rows.
///
/// `show_dm_rows` gates the two DM-related rows (needs `dms` capability).
/// `show_dm_friends_row` additionally requires the `friends` capability.
#[rustfmt::skip]
#[ui_action(SensitiveMediaSectionAction)]
#[context_menu(none)]
#[component]
fn SensitiveMediaSection(
    account_sessions: BatchedSignal<AccountSessions>,
    /// Show DM-related rows (dm-friends and dm-others). Requires `dms` capability.
    show_dm_rows: bool,
    /// Show the dm-friends row specifically. Requires `dms && friends`.
    show_dm_friends_row: bool,
) -> Element {
    let policy = account_sessions.read().content_policy.clone();
    rsx! {
        div { class: "content-social-section",
            div { class: "content-social-section-header",
                h3 { class: "content-social-section-title", "{t(\"content-social-sensitive-media\")}" }
                p { class: "content-social-section-desc", "{t(\"content-social-sensitive-media-desc\")}" }
            }
            if show_dm_rows && show_dm_friends_row {
                SensitiveMediaRow {
                    label: t("content-social-dm-friends"),
                    value: policy.sensitive_content_dm_friends,
                    on_change: move |level| {
                        account_sessions.batch(|as_| as_.content_policy.sensitive_content_dm_friends = level);
                    },
                }
            }
            if show_dm_rows {
                SensitiveMediaRow {
                    label: t("content-social-dm-others"),
                    value: policy.sensitive_content_dm_others,
                    on_change: move |level| {
                        account_sessions.batch(|as_| as_.content_policy.sensitive_content_dm_others = level);
                    },
                }
            }
            SensitiveMediaRow {
                label: t("content-social-server-channels"),
                value: policy.sensitive_content_server_channels,
                on_change: move |level| {
                    account_sessions.batch(|as_| as_.content_policy.sensitive_content_server_channels = level);
                },
            }
        }
    }
}

pub enum SpamFilterSectionAction {
    SetLevel(DmSpamFilterLevel),
}

impl UiAction for SpamFilterSectionAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetLevel(_) => todo!("phase-E: update dm_spam_filter level"),
        }
    }
}

/// DM Spam Filter section — three radio options.
#[rustfmt::skip]
#[ui_action(SpamFilterSectionAction)]
#[context_menu(none)]
#[component]
fn SpamFilterSection(mut account_sessions: BatchedSignal<AccountSessions>) -> Element {
    let current = account_sessions.read().content_policy.dm_spam_filter;
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
                                        account_sessions.batch(|as_| as_.content_policy.dm_spam_filter = value);
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

pub enum AgeRestrictedSectionAction {
    SetAllowServers(bool),
    SetAllowCommands(bool),
}

impl UiAction for AgeRestrictedSectionAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetAllowServers(_) => todo!("phase-E: update allow_age_restricted_servers"),
            Self::SetAllowCommands(_) => todo!("phase-E: update allow_age_restricted_commands_in_dms"),
        }
    }
}

/// Age-Restricted Content section — age access toggles.
///
/// `show_dm_commands` gates the "Allow age-restricted commands in DMs" row.
/// Hidden for backends without DM support (Lemmy, HackerNews, GitHub, Forgejo).
#[rustfmt::skip]
#[ui_action(AgeRestrictedSectionAction)]
#[context_menu(none)]
#[component]
fn AgeRestrictedSection(mut account_sessions: BatchedSignal<AccountSessions>, show_dm_commands: bool) -> Element {
    let policy = account_sessions.read().content_policy.clone();
    rsx! {
        div { class: "content-social-section",
            h3 { class: "content-social-section-title", "{t(\"content-social-age-restricted\")}" }
            ToggleRow {
                label: t("content-social-age-restricted-servers"),
                checked: policy.allow_age_restricted_servers,
                on_change: move |val| {
                    account_sessions.batch(|as_| as_.content_policy.allow_age_restricted_servers = val);
                },
            }
            if show_dm_commands {
                ToggleRow {
                    label: t("content-social-age-restricted-commands"),
                    checked: policy.allow_age_restricted_commands_in_dms,
                    on_change: move |val| {
                        account_sessions.batch(|as_| as_.content_policy.allow_age_restricted_commands_in_dms = val);
                    },
                }
            }
        }
    }
}

pub enum SocialPermissionsSectionAction {
    SetAllowDmsFromMembers(bool),
    SetAllowMessageRequests(bool),
}

impl UiAction for SocialPermissionsSectionAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetAllowDmsFromMembers(_) => todo!("phase-E: update allow_dms_from_server_members"),
            Self::SetAllowMessageRequests(_) => todo!("phase-E: update allow_message_requests"),
        }
    }
}

/// Social Permissions section — DM and message request controls.
#[rustfmt::skip]
#[ui_action(SocialPermissionsSectionAction)]
#[context_menu(none)]
#[component]
fn SocialPermissionsSection(mut account_sessions: BatchedSignal<AccountSessions>) -> Element {
    let policy = account_sessions.read().content_policy.clone();
    rsx! {
        div { class: "content-social-section",
            div { class: "content-social-section-header",
                h3 { class: "content-social-section-title", "{t(\"content-social-social-perms\")}" }
                p  { class: "content-social-section-desc", "{t(\"content-social-social-perms-desc\")}" }
            }
            ToggleRow {
                label: t("content-social-dms-from-members"),
                checked: policy.allow_dms_from_server_members,
                on_change: move |val| {
                    account_sessions.batch(|as_| as_.content_policy.allow_dms_from_server_members = val);
                },
            }
            ToggleRow {
                label: t("content-social-message-requests"),
                checked: policy.allow_message_requests,
                on_change: move |val| {
                    account_sessions.batch(|as_| as_.content_policy.allow_message_requests = val);
                },
            }
        }
    }
}

pub enum FriendRequestsSectionAction {
    SetFromEveryone(bool),
    SetFromFriendsOfFriends(bool),
    SetFromServerMembers(bool),
}

impl UiAction for FriendRequestsSectionAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetFromEveryone(_) => todo!("phase-E: update friend_request_from_everyone"),
            Self::SetFromFriendsOfFriends(_) => todo!("phase-E: update friend_request_from_friends_of_friends"),
            Self::SetFromServerMembers(_) => todo!("phase-E: update friend_request_from_server_members"),
        }
    }
}

/// Friend Requests section — three permission checkboxes.
#[rustfmt::skip]
#[ui_action(FriendRequestsSectionAction)]
#[context_menu(none)]
#[component]
fn FriendRequestsSection(mut account_sessions: BatchedSignal<AccountSessions>) -> Element {
    let policy = account_sessions.read().content_policy.clone();
    rsx! {
        div { class: "content-social-section",
            h3 { class: "content-social-section-title", "{t(\"content-social-friend-requests\")}" }
            ToggleRow {
                label: t("content-social-fr-everyone"),
                checked: policy.friend_request_from_everyone,
                on_change: move |val| {
                    account_sessions.batch(|as_| as_.content_policy.friend_request_from_everyone = val);
                },
            }
            ToggleRow {
                label: t("content-social-fr-friends-of-friends"),
                checked: policy.friend_request_from_friends_of_friends,
                on_change: move |val| {
                    account_sessions.batch(|as_| as_.content_policy.friend_request_from_friends_of_friends = val);
                },
            }
            ToggleRow {
                label: t("content-social-fr-server-members"),
                checked: policy.friend_request_from_server_members,
                on_change: move |val| {
                    account_sessions.batch(|as_| as_.content_policy.friend_request_from_server_members = val);
                },
            }
        }
    }
}

// ─── entry point ────────────────────────────────────────────────────────────

pub enum ContentSocialSettingsAction {
    /// Placeholder — all real mutations are dispatched by sub-section components.
    Noop,
}

impl UiAction for ContentSocialSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::Noop => {}
        }
    }
}

/// Content & Social settings page for a single account.
///
/// `backend` is the backend slug used to gate sections on `BackendCapabilities`.
/// Sections that require DMs, friends, or voice are hidden for backends that
/// don't support those features (e.g. Lemmy, Forgejo, GitHub, HackerNews).
///
/// Reads policy from `AccountSessions::content_policy`. All writes go directly back to `AccountSessions`.
///
/// Blocked users are managed in the People page, not here.
///
/// Rendered by [`crate::ui::account::settings::AccountSettingsPage`] when the
/// "content-social" section is active.
#[rustfmt::skip]
#[ui_action(ContentSocialSettingsAction)]
#[context_menu(none)]
#[component]
pub fn ContentSocialSettings(_account_id: String, backend: String) -> Element {
    let account_sessions = use_context::<BatchedSignal<AccountSessions>>();
    let client_manager = use_context::<BatchedSignal<ClientManager>>();
    let caps = client_manager.peek().capabilities_for_slug(&backend);
    let has_dms = caps.should_show_dms();
    let has_friends = caps.should_show_friends();
    rsx! {
        div { class: "settings-section-content",
            h2 { class: "settings-section-title", "{t(\"content-social-title\")}" }
            SensitiveMediaSection {
                account_sessions,
                show_dm_rows: has_dms,
                show_dm_friends_row: has_dms && has_friends,
            }
            if has_dms {
                SpamFilterSection { account_sessions }
            }
            if has_dms || has_friends {
                SocialPermissionsSection { account_sessions }
            }
            if has_friends {
                FriendRequestsSection { account_sessions }
            }
            AgeRestrictedSection { account_sessions, show_dm_commands: has_dms }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn content_social_settings_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<ContentSocialSettingsAction>();
        let _ = ContentSocialSettingsAction::Noop;
    }

    #[test]
    fn sensitive_media_section_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<SensitiveMediaSectionAction>();
    }
}

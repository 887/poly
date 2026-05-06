//! Mod Log (audit log) tab for per-server settings.
//!
//! Fetches and displays moderation log entries.
//! Gated by `BackendCapabilities::has_moderation_log`.

use crate::i18n::t;
use crate::ui::client_ui::use_view_resource::{use_view_resource, ViewQuery};
use dioxus::prelude::*;
use poly_client::{ClientBackend, ClientError, ClientResult, ModerationAction, ModerationLogEntry};
use poly_ui_macros::{context_menu, ui_action};

// ── ViewQuery impl ────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct ServerModLogQuery {
    account_id: String,
    server_id: String,
    limit: usize,
}

impl ViewQuery for ServerModLogQuery {
    type Output = Vec<ModerationLogEntry>;
    fn account_id(&self) -> &str { &self.account_id }
    async fn fetch(&self, b: &dyn ClientBackend) -> ClientResult<Self::Output> {
        match b.as_moderation() {
            Some(m) => m.get_moderation_log(&self.server_id, self.limit).await,
            None => Err(ClientError::NotSupported("get_moderation_log".to_string())),
        }
    }
}

const MODLOG_LIMIT: usize = 50;

/// Mod Log tab component — shows the moderation/audit log for a server.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn ModLogTab(server_id: String, account_id: String) -> Element {
    let log_resource: Resource<ClientResult<Vec<ModerationLogEntry>>> =
        use_view_resource(ServerModLogQuery {
            account_id,
            server_id,
            limit: MODLOG_LIMIT,
        });

    // `ClientError` is not `Clone`, so we can't snapshot via `.cloned()`.
    // Read through `read_unchecked` directly.
    let log = log_resource.read_unchecked();

    rsx! {
        div { class: "modlog-tab",
            match &*log {
                None => rsx! {
                    p { class: "tab-loading", "{t(\"modlog-tab-loading\")}" }
                },
                Some(Err(e)) => rsx! {
                    p { class: "tab-error", "{e}" }
                },
                Some(Ok(entries)) if entries.is_empty() => rsx! {
                    p { class: "tab-empty", "{t(\"modlog-tab-empty\")}" }
                },
                Some(Ok(entries)) => rsx! {
                    div { class: "modlog-list",
                        for entry in entries.iter() {
                            {render_modlog_row(entry.clone())}
                        }
                    }
                },
            }
        }
    }
}

fn action_label(action: &ModerationAction) -> String {
    match action {
        ModerationAction::MemberKicked => t("modlog-action-kicked"),
        ModerationAction::MemberBanned => t("modlog-action-banned"),
        ModerationAction::MemberUnbanned => t("modlog-action-unbanned"),
        ModerationAction::MemberTimedOut => t("modlog-action-timed-out"),
        ModerationAction::MemberRoleUpdated => t("modlog-action-role-updated"),
        ModerationAction::MessageDeleted => t("modlog-action-message-deleted"),
        ModerationAction::ChannelUpdated => t("modlog-action-channel-updated"),
        ModerationAction::Other(detail) => {
            t("modlog-action-other")
                .replace("{ $detail }", detail)
                .replace("{$detail}", detail)
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_modlog_row(entry: ModerationLogEntry) -> Element {
    let action = action_label(&entry.action);
    let moderator = entry.moderator.display_name.clone();
    let target = entry.target_display_name.clone().unwrap_or_default();
    let reason = entry.reason.clone().unwrap_or_default();
    // Format ISO timestamp to date only for compactness.
    let ts = entry.timestamp.get(..10).unwrap_or(&entry.timestamp).to_string();

    rsx! {
        div { class: "modlog-row",
            span { class: "modlog-ts", "{ts}" }
            span { class: "modlog-action", "{action}" }
            span { class: "modlog-moderator",
                span { class: "modlog-label", "{t(\"modlog-tab-moderator\")}: " }
                "{moderator}"
            }
            if !target.is_empty() {
                span { class: "modlog-target",
                    span { class: "modlog-label", "{t(\"modlog-tab-target\")}: " }
                    "{target}"
                }
            }
            if !reason.is_empty() {
                span { class: "modlog-reason",
                    span { class: "modlog-label", "{t(\"modlog-tab-reason\")}: " }
                    "{reason}"
                }
            }
        }
    }
}

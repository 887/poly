//! General server settings — server info and leave server action.
//!
//! The leave server action uses an inline confirm widget (no JS confirm())
//! to avoid triggering the browser native dialog.

use crate::state::BatchedSignal;
use super::super::super::super::routes::Route;
use crate::i18n::{t, t_args};
use crate::state::AppState;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

pub enum ServerGeneralSettingsAction {
    ShowLeaveConfirm,
    HideLeaveConfirm,
}

impl UiAction for ServerGeneralSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::ShowLeaveConfirm => todo!("phase-E: show inline leave-server confirm"),
            Self::HideLeaveConfirm => todo!("phase-E: hide inline leave-server confirm"),
        }
    }
}

/// General settings panel for a server.
///
/// Shows server info and a leave-server action with an inline confirm.
#[ui_action(ServerGeneralSettingsAction)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn ServerGeneralSettings(
    server_id: String,
    server_name: String,
    backend_slug: String,
    account_id: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    instance_id: String,
) -> Element {
    let mut show_confirm = use_signal(|| false);

    rsx! {
        h2 { class: "settings-section-title", "{t(\"server-settings-general\")}" }
        div { class: "settings-section",
            h3 { class: "settings-section-title", "{t(\"server-general-info\")}" }
            div { class: "settings-field",
                label { class: "settings-label", "ID" }
                p { class: "settings-value settings-monospace", "{server_id}" }
            }
            div { class: "settings-field",
                label { class: "settings-label", "Name" }
                p { class: "settings-value", "{server_name}" }
            }
        }

        div { class: "settings-section settings-danger-zone",
            h3 { class: "settings-section-title danger", "{t(\"server-general-danger\")}" }

            if show_confirm() {
                // Inline leave-server confirm widget
                LeaveServerConfirm {
                    server_name: server_name.clone(),
                    server_id: server_id.clone(),
                    backend_slug: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                    oncancel: move |_| show_confirm.set(false),
                }
            } else {
                button {
                    class: "btn-danger",
                    onclick: move |_| show_confirm.set(true),
                    "{t(\"server-menu-leave\")}"
                }
            }
        }
    }
}

/// Inline confirm widget for leaving a server.
///
/// Does NOT use `window.confirm()`. The confirm dialog is rendered in-DOM.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
fn LeaveServerConfirm(
    server_name: String,
    server_id: String,
    backend_slug: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    instance_id: String,
    account_id: String,
    oncancel: EventHandler<MouseEvent>,
) -> Element {
    let _app_state: BatchedSignal<AppState> = use_context();
    let chat_lists: BatchedSignal<crate::state::ChatLists> = use_context();
    let account_sessions: BatchedSignal<crate::state::AccountSessions> = use_context();

    let aid_nav = account_id.clone();
    let iid_nav = instance_id.clone();
    let bslug_nav = backend_slug.clone();
    let sid_remove = server_id.clone();
    // Pre-compute the title using t_args so the Fluent {$name} placeholder is filled
    let leave_title = t_args("leave-server-title", &[("name", server_name.as_str())]);

    rsx! {
        div { class: "leave-server-confirm",
            h4 { class: "leave-server-confirm-title", "{leave_title}" }
            p { class: "leave-server-confirm-body", "{t(\"leave-server-body\")}" }
            div { class: "leave-server-confirm-actions",
                button {
                    class: "btn-secondary",
                    onclick: move |evt| oncancel.call(evt),
                    "{t(\"leave-server-cancel\")}"
                }
                button {
                    class: "btn-danger",
                    onclick: move |_| {
                        // Remove server from chat data
                        let sid = sid_remove.clone();
                        let sid2 = sid.clone();
                        chat_lists.batch(move |cl| {
                            cl.set_servers(cl.servers.iter().filter(|s| s.id != sid).cloned().collect());
                        });
                        let new_favs = account_sessions.batch(|as_| {
                            as_.favorited_server_ids.retain(|id| id != &sid2);
                            as_.account_server_order
                                .values_mut()
                                .for_each(|v| {
                                    v.retain(|id| id != &sid2);
                                });
                            as_.favorited_server_ids.clone()
                        });
                        spawn(async move {
                            crate::ui::favorites_sidebar::persist_favorites(new_favs).await;
                        });
                        navigator()
                            .replace(Route::DmsHome {
                                backend: bslug_nav.clone(),
                                instance_id: iid_nav.clone(),
                                account_id: aid_nav.clone(),
                            });
                    },
                    "{t(\"leave-server-confirm\")}"
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn server_general_settings_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<ServerGeneralSettingsAction>();
        let _ = ServerGeneralSettingsAction::ShowLeaveConfirm;
        let _ = ServerGeneralSettingsAction::HideLeaveConfirm;
    }
}

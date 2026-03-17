//! General settings — app reset and nuke flows.
//!
//! # Architecture
//! - `GeneralSettings`: Main section container
//! - `ResetSection`: Handles reset button state and logic
//! - Helper: `run_reset_flow` async function

use crate::i18n::t;
use crate::state::AppState;
use crate::storage::AppSettings;
use dioxus::prelude::*;

async fn persist_force_mobile_layout(enabled: bool) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(mut settings) = storage.get_app_settings().await else {
        return;
    };
    if settings.force_mobile_layout == enabled {
        return;
    }
    settings.force_mobile_layout = enabled;
    if let Err(err) = storage.set_app_settings(&settings).await {
        tracing::warn!("Failed to persist force-mobile layout setting: {err}");
    }
}

fn load_general_settings(mut settings_sig: Signal<AppSettings>) {
    spawn(async move {
        let Some(storage) = crate::STORAGE.get() else {
            return;
        };
        match storage.get_app_settings().await {
            Ok(settings) => settings_sig.set(settings),
            Err(err) => tracing::warn!("Failed to load general settings: {err}"),
        }
    });
}

#[rustfmt::skip]
#[component]
fn MobileLayoutToggle() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let mut settings_sig = use_signal(AppSettings::default);
    let mut loaded = use_signal(|| false);

    use_effect(move || {
        if *loaded.read() {
            return;
        }
        loaded.set(true);
        load_general_settings(settings_sig);
    });

    let enabled = settings_sig.read().force_mobile_layout;

    rsx! {
        div { class: "settings-toggle-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label",
                    "{t(\"settings-force-mobile-layout\")}"
                }
                p { class: "settings-toggle-desc",
                    "{t(\"settings-force-mobile-layout-description\")}"
                }
            }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked: enabled,
                    onchange: move |evt| {
                        let next = evt.checked();
                        settings_sig.write().force_mobile_layout = next;
                        app_state.write().force_mobile_layout = next;
                        spawn(async move {
                            persist_force_mobile_layout(next).await;
                        });
                    },
                }
                span { class: "toggle-slider" }
            }
        }
    }
}

/// Controls which data to wipe in the reset flow.
#[derive(Clone, Copy, PartialEq)]
enum ResetKind {
    /// Wipe user data but keep identity key.
    User,
    /// Wipe everything including the identity key.
    Nuke,
}

/// Logout all active backends and wipe storage.
///
/// Navigates to setup wizard on success, returns `Err(msg)` on failure.
// DECISION(DX-2.5.1): Reset flow uses ClientManager context so all active
// backends can be logged out before storage is wiped.
async fn run_reset_flow(
    kind: ResetKind,
    mut client_manager: Signal<crate::client_manager::ClientManager>,
    mut chat_data: Signal<crate::state::ChatData>,
    mut app_state: Signal<AppState>,
) -> Result<(), String> {
    let account_ids = client_manager.read().active_account_ids();
    for account_id in account_ids {
        let backend = client_manager.read().get_backend(&account_id);
        if let Some(backend_handle) = backend {
            let mut guard = backend_handle.write().await;
            if let Err(err) = guard.logout().await {
                tracing::warn!("Logout failed for account {account_id}: {err}");
            }
        }
    }
    client_manager.write().clear_all_backends();

    chat_data.set(crate::state::ChatData::default());
    let nav = crate::state::NavigationState {
        view: crate::state::View::Setup,
        ..Default::default()
    };
    {
        let mut state = app_state.write();
        state.is_setup_complete = false;
        state.nav = nav;
    }

    let Some(storage) = crate::STORAGE.get() else {
        return Err(t("settings-reset-error-no-storage"));
    };

    match kind {
        ResetKind::User => storage
            .reset_user_data()
            .await
            .map_err(|e| format!("{}: {e}", t("settings-reset-error-failed")))?,
        ResetKind::Nuke => storage
            .nuke_all_data()
            .await
            .map_err(|e| format!("{}: {e}", t("settings-nuke-error-failed")))?,
    }

    document::eval("window.location.reload();");
    Ok(())
}

/// Reset button component.
#[rustfmt::skip]
#[component]
fn ResetButton(kind: ResetKind, busy: Signal<bool>, on_error: EventHandler<String>) -> Element {
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<crate::client_manager::ClientManager> = use_context();
    let chat_data: Signal<crate::state::ChatData> = use_context();
    let mut busy_signal = use_signal(|| *busy.read());

    let (label, class_name) = match kind {
        ResetKind::User => (t("settings-reset-app"), "btn btn-danger"),
        ResetKind::Nuke => (
            format!("☢️ {}", t("settings-nuke-app")),
            "btn btn-warning btn-nuke",
        ),
    };

    rsx! {
        button {
            class: "{class_name}",
            disabled: *busy_signal.read(),
            onclick: move |_| {
                if *busy_signal.read() {
                    return;
                }
                busy_signal.set(true);
                spawn(async move {
                    if let Err(err) = run_reset_flow(kind, client_manager, chat_data, app_state)
                        .await
                    {
                        on_error.call(err);
                        busy_signal.set(false);
                    }
                });
            },
            "{label}"
        }
    }
}

/// Error display component.
#[rustfmt::skip]
#[component]
fn ResetError(error: Signal<String>) -> Element {
    rsx! {
        if !error.read().is_empty() {
            p { class: "general-reset-error", "{error.read()}" }
        }
    }
}

/// Reset actions section with buttons and error handling.
#[rustfmt::skip]
#[component]
fn ResetSection() -> Element {
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);

    rsx! {
        div { class: "general-reset-actions",
            p { class: "settings-description", "{t(\"settings-reset-description\")}" }
            ResetButton {
                kind: ResetKind::User,
                busy,
                on_error: move |err: String| {
                    error.set(err);
                    busy.set(false);
                },
            }
            ResetButton {
                kind: ResetKind::Nuke,
                busy,
                on_error: move |err: String| {
                    error.set(err);
                    busy.set(false);
                },
            }
            ResetError { error }
        }
    }
}

/// General settings section.
///
/// Contains the app-reset and nuke-all-data danger zone.
// TODO(phase-2.7.9.10): Notification preferences, startup behavior
#[rustfmt::skip]
#[component]
pub(super) fn GeneralSettings() -> Element {
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-general\")}" }
            p { class: "settings-description", "{t(\"settings-general-description\")}" }
            MobileLayoutToggle {}
            ResetSection {}
        }
    }
}

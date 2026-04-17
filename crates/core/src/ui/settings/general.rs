//! General + layout settings — app layout controls plus reset and nuke flows.
//!
//! # Architecture
//! - `LayoutSettings`: Layout behavior / mirroring controls
//! - `GeneralSettings`: Reset / nuke section container
//! - `ResetSection`: Handles reset button state and logic
//! - Helper: `run_reset_flow` async function

use crate::i18n::t;
use crate::state::{AppState, LayoutMode};
use crate::storage::AppSettings;
use dioxus::prelude::*;
use poly_ui_macros::context_menu;

async fn persist_layout_mode(mode: LayoutMode) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(mut settings) = storage.get_app_settings().await else {
        return;
    };
    if settings.layout_mode == mode
        && settings.force_mobile_layout == matches!(mode, LayoutMode::ForceMobile)
    {
        return;
    }
    settings.layout_mode = mode;
    settings.force_mobile_layout = matches!(mode, LayoutMode::ForceMobile);
    if let Err(err) = storage.set_app_settings(&settings).await {
        tracing::warn!("Failed to persist layout mode setting: {err}");
    }
}

async fn persist_mirror_menu_layout(enabled: bool) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(mut settings) = storage.get_app_settings().await else {
        return;
    };
    if settings.mirror_menu_layout == enabled {
        return;
    }
    settings.mirror_menu_layout = enabled;
    if let Err(err) = storage.set_app_settings(&settings).await {
        tracing::warn!("Failed to persist menu mirror setting: {err}");
    }
}

async fn persist_mirror_chat_messages(enabled: bool) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(mut settings) = storage.get_app_settings().await else {
        return;
    };
    if settings.mirror_chat_messages == enabled {
        return;
    }
    settings.mirror_chat_messages = enabled;
    if let Err(err) = storage.set_app_settings(&settings).await {
        tracing::warn!("Failed to persist chat mirror setting: {err}");
    }
}

fn load_general_settings(mut settings_sig: Signal<AppSettings>) {
    spawn(async move {
        let Some(storage) = crate::STORAGE.get() else {
            return;
        };
        match storage.get_app_settings().await {
            Ok(mut settings) => {
                if settings.layout_mode == LayoutMode::AutoWidth && settings.force_mobile_layout {
                    settings.layout_mode = LayoutMode::ForceMobile;
                }
                settings_sig.set(settings);
            }
            Err(err) => tracing::warn!("Failed to load general settings: {err}"),
        }
    });
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn LayoutModeButton(label: String, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: if active { "settings-choice-button active" } else { "settings-choice-button" },
            onclick: move |evt| onclick.call(evt),
            "{label}"
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn LayoutModeSelector() -> Element {
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

    let selected_mode = settings_sig.read().layout_mode;

    rsx! {
        div { class: "settings-toggle-row settings-toggle-row-column",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label",
                    "{t(\"settings-layout-mode\")}"
                }
                p { class: "settings-toggle-desc",
                    "{t(\"settings-layout-mode-description\")}"
                }
            }
            div { class: "settings-choice-group",
                LayoutModeButton {
                    label: t("settings-layout-auto-width"),
                    active: selected_mode == LayoutMode::AutoWidth,
                    onclick: move |_| {
                        settings_sig.write().layout_mode = LayoutMode::AutoWidth;
                        app_state.write().layout_mode = LayoutMode::AutoWidth;
                        spawn(async move { persist_layout_mode(LayoutMode::AutoWidth).await; });
                    },
                }
                LayoutModeButton {
                    label: t("settings-layout-auto-portrait"),
                    active: selected_mode == LayoutMode::AutoPortrait,
                    onclick: move |_| {
                        settings_sig.write().layout_mode = LayoutMode::AutoPortrait;
                        app_state.write().layout_mode = LayoutMode::AutoPortrait;
                        spawn(async move { persist_layout_mode(LayoutMode::AutoPortrait).await; });
                    },
                }
                LayoutModeButton {
                    label: t("settings-layout-force-desktop"),
                    active: selected_mode == LayoutMode::ForceDesktop,
                    onclick: move |_| {
                        settings_sig.write().layout_mode = LayoutMode::ForceDesktop;
                        app_state.write().layout_mode = LayoutMode::ForceDesktop;
                        spawn(async move { persist_layout_mode(LayoutMode::ForceDesktop).await; });
                    },
                }
                LayoutModeButton {
                    label: t("settings-layout-force-mobile"),
                    active: selected_mode == LayoutMode::ForceMobile,
                    onclick: move |_| {
                        settings_sig.write().layout_mode = LayoutMode::ForceMobile;
                        app_state.write().layout_mode = LayoutMode::ForceMobile;
                        spawn(async move { persist_layout_mode(LayoutMode::ForceMobile).await; });
                    },
                }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn MirrorMenuToggle() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let enabled = app_state.read().mirror_menu_layout;

    rsx! {
        div { class: "settings-toggle-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{t(\"settings-mirror-menu-layout\")}" }
                p { class: "settings-toggle-desc", "{t(\"settings-mirror-menu-layout-description\")}" }
            }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked: enabled,
                    onchange: move |evt| {
                        let next = evt.checked();
                        app_state.write().mirror_menu_layout = next;
                        spawn(async move { persist_mirror_menu_layout(next).await; });
                    },
                }
                span { class: "toggle-slider" }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn MirrorChatMessagesToggle() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let enabled = app_state.read().mirror_chat_messages;

    rsx! {
        div { class: "settings-toggle-row",
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{t(\"settings-mirror-chat-messages\")}" }
                p { class: "settings-toggle-desc", "{t(\"settings-mirror-chat-messages-description\")}" }
            }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked: enabled,
                    onchange: move |evt| {
                        let next = evt.checked();
                        app_state.write().mirror_chat_messages = next;
                        spawn(async move { persist_mirror_chat_messages(next).await; });
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
#[context_menu(inherit)]
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
#[context_menu(inherit)]
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
#[context_menu(inherit)]
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
/// Contains shell layout and mirroring preferences.
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub(super) fn LayoutSettings() -> Element {
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-layout\")}" }
            p { class: "settings-description", "{t(\"settings-layout-description\")}" }
            LayoutModeSelector {}
            MirrorMenuToggle {}
            MirrorChatMessagesToggle {}
        }
    }
}

/// General settings section.
///
/// Contains the app-reset and nuke-all-data danger zone.
// TODO(phase-2.7.9.10): Notification preferences, startup behavior
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub(super) fn GeneralSettings() -> Element {
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-general\")}" }
            p { class: "settings-description", "{t(\"settings-general-description\")}" }
            ResetSection {}
        }
    }
}

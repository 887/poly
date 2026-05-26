//! General + layout settings — app layout controls plus reset and nuke flows.
//!
//! # Architecture
//! - `LayoutSettings`: Layout behavior / mirroring controls
//! - `GeneralSettings`: Reset / nuke section container
//! - `ResetSection`: Handles reset button state and logic
//! - Helper: `run_reset_flow` async function

use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::state::{LayoutMode};
use crate::storage::AppSettings;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the layout settings section.
pub enum LayoutSettingsAction {
    /// Change the app layout mode.
    SetLayoutMode(LayoutMode),
    /// Toggle mirroring of the menu layout.
    SetMirrorMenu(bool),
    /// Toggle mirroring of chat messages.
    SetMirrorChatMessages(bool),
}

impl UiAction for LayoutSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // layout_mode / mirror_* fields moved to UiLayout; components apply these
        // directly via ui_layout.batch(). This action stub satisfies the ui-action
        // coverage lint; the variants are not dispatched via cx.apply().
        match self {
            Self::SetLayoutMode(_) => todo!("route via ui_layout.batch if action dispatch is wired"),
            Self::SetMirrorMenu(_) => todo!("route via ui_layout.batch if action dispatch is wired"),
            Self::SetMirrorChatMessages(_) => todo!("route via ui_layout.batch if action dispatch is wired"),
        }
    }
}

/// Actions for the general settings section.
pub enum GeneralSettingsAction {
    /// Wipe user data and return to setup wizard.
    Reset,
    /// Wipe all data including the identity key.
    NukeAllData,
}

impl UiAction for GeneralSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::Reset => todo!("phase-E: run reset flow"),
            Self::NukeAllData => todo!("phase-E: nuke all data"),
        }
    }
}

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

fn load_general_settings(settings_sig: BatchedSignal<AppSettings>) {
    spawn(async move {
        let Some(storage) = crate::STORAGE.get() else {
            return;
        };
        match storage.get_app_settings().await {
            Ok(mut settings) => {
                if settings.layout_mode == LayoutMode::AutoWidth && settings.force_mobile_layout {
                    settings.layout_mode = LayoutMode::ForceMobile;
                }
                settings_sig.batch(|s| *s = settings);
            }
            Err(err) => tracing::warn!("Failed to load general settings: {err}"),
        }
    });
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
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

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn LayoutModeSelector() -> Element {
    let ui_layout: crate::state::BatchedSignal<crate::state::UiLayout> = use_context();
    let settings_sig = BatchedSignal::use_batched(AppSettings::default);
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
                        settings_sig.batch(|s| s.layout_mode = LayoutMode::AutoWidth);
                        ui_layout.batch(|l| l.layout_mode = LayoutMode::AutoWidth);
                        spawn(async move { persist_layout_mode(LayoutMode::AutoWidth).await; });
                    },
                }
                LayoutModeButton {
                    label: t("settings-layout-auto-portrait"),
                    active: selected_mode == LayoutMode::AutoPortrait,
                    onclick: move |_| {
                        settings_sig.batch(|s| s.layout_mode = LayoutMode::AutoPortrait);
                        ui_layout.batch(|l| l.layout_mode = LayoutMode::AutoPortrait);
                        spawn(async move { persist_layout_mode(LayoutMode::AutoPortrait).await; });
                    },
                }
                LayoutModeButton {
                    label: t("settings-layout-force-desktop"),
                    active: selected_mode == LayoutMode::ForceDesktop,
                    onclick: move |_| {
                        settings_sig.batch(|s| s.layout_mode = LayoutMode::ForceDesktop);
                        ui_layout.batch(|l| l.layout_mode = LayoutMode::ForceDesktop);
                        spawn(async move { persist_layout_mode(LayoutMode::ForceDesktop).await; });
                    },
                }
                LayoutModeButton {
                    label: t("settings-layout-force-mobile"),
                    active: selected_mode == LayoutMode::ForceMobile,
                    onclick: move |_| {
                        settings_sig.batch(|s| s.layout_mode = LayoutMode::ForceMobile);
                        ui_layout.batch(|l| l.layout_mode = LayoutMode::ForceMobile);
                        spawn(async move { persist_layout_mode(LayoutMode::ForceMobile).await; });
                    },
                }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn MirrorMenuToggle() -> Element {
    let ui_layout: crate::state::BatchedSignal<crate::state::UiLayout> = use_context();
    let enabled = ui_layout.read().mirror_menu_layout;

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
                        ui_layout.batch(|l| l.mirror_menu_layout = next);
                        spawn(async move { persist_mirror_menu_layout(next).await; });
                    },
                }
                span { class: "toggle-slider" }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn MirrorChatMessagesToggle() -> Element {
    let ui_layout: crate::state::BatchedSignal<crate::state::UiLayout> = use_context();
    let enabled = ui_layout.read().mirror_chat_messages;

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
                        ui_layout.batch(|l| l.mirror_chat_messages = next);
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
    client_manager: BatchedSignal<crate::client_manager::ClientManager>,
    chat_lists: BatchedSignal<crate::state::ChatLists>,
    account_sessions: BatchedSignal<crate::state::AccountSessions>,
) -> Result<(), String> {
    let account_ids = client_manager.peek().active_account_ids();
    for account_id in account_ids {
        let backend = client_manager.peek().get_backend(&account_id);
        if let Some(backend_handle) = backend {
            let mut guard = backend_handle.write().await;
            if let Err(err) = guard.logout().await {
                tracing::warn!("Logout failed for account {account_id}: {err}");
            }
        }
    }
    client_manager.batch(crate::client_manager::ClientManager::clear_all_backends);

    chat_lists.batch(|cl| *cl = crate::state::ChatLists::default());
    // Resets both account-session state AND the is_setup_complete flag
    // (now lives on AccountSessions, Phase C.3 — default is false).
    account_sessions.batch(|as_| *as_ = crate::state::AccountSessions::default());

    let Some(storage) = crate::STORAGE.get() else {
        return Err(t("settings-reset-error-no-storage"));
    };

    match kind {
        ResetKind::User => storage
            .reset_user_data()
            .await
            .map_err(|e| format!("{}: {e}", t("settings-reset-error-failed")))?,
        ResetKind::Nuke => {
            storage
                .nuke_all_data()
                .await
                .map_err(|e| format!("{}: {e}", t("settings-nuke-error-failed")))?;
            // A3.2: write the autoseed-disabled marker AFTER nuke_all_data
            // wipes the KV table, so it persists across the next boot. Without
            // this, dev-plugins re-seeds 25 demo accounts on the next boot
            // and the user can never reach a truly-empty state.
            if let Err(e) = storage
                .set(crate::storage::keys::DEV_AUTOSEED_DISABLED, serde_json::json!(true))
                .await
            {
                tracing::warn!("Failed to write DEV_AUTOSEED_DISABLED marker post-nuke: {e}");
            }
        }
    }

    // A3.1: For a Nuke we navigate to `/` so the no-account branch lands at
    // the Welcome wizard. A plain `reload()` keeps the user on whatever URL
    // they were on when they clicked the button (e.g. /settings/general),
    // which the wizard then bounces off of when "Get Started" is clicked.
    // For a soft User reset we keep `reload()` so the user stays in context.
    match kind {
        ResetKind::Nuke => document::eval("window.location.href = '/';"),
        ResetKind::User => document::eval("window.location.reload();"),
    };
    Ok(())
}

/// Reset button component.
///
/// Click opens a confirmation modal — neither button fires the destructive
/// flow on a single click. Nuke additionally requires typing the literal
/// word DELETE before the execute button enables.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ResetButton(kind: ResetKind, busy: Signal<bool>, on_error: EventHandler<String>) -> Element {
    let client_manager: BatchedSignal<crate::client_manager::ClientManager> = use_context();
    let chat_lists: BatchedSignal<crate::state::ChatLists> = use_context();
    let account_sessions: BatchedSignal<crate::state::AccountSessions> = use_context();
    let mut busy_signal = use_signal(|| *busy.read()); // poly-lint: allow render-time-read — initial value snapshot from the parent's busy signal
    let mut confirm_open = use_signal(|| false);
    // A2.3: countdown for the soft User reset. None = inactive, Some(n) = n
    // seconds remaining before the reset fires. User reset path only — Nuke
    // is immediate (irreversible by definition; no useful undo window).
    let mut undo_countdown: Signal<Option<u8>> = use_signal(|| None);

    let (label, class_name) = match kind {
        ResetKind::User => (t("settings-reset-app"), "btn btn-danger"),
        ResetKind::Nuke => (
            format!("☢️ {}", t("settings-nuke-app")),
            "btn btn-warning btn-nuke",
        ),
    };

    rsx! {
        // Hide the trigger button while a countdown is in flight; show the
        // undo row instead. Avoids the user re-clicking the same button.
        if undo_countdown.read().is_none() { // poly-lint: allow render-time-read — toggles visibility of trigger vs undo row
            button {
                class: "{class_name}",
                disabled: *busy_signal.read(), // poly-lint: allow render-time-read — subscription IS the intent; button must redraw when busy flips
                onclick: move |_| {
                    if *busy_signal.peek() {
                        return;
                    }
                    confirm_open.set(true);
                },
                "{label}"
            }
        }
        if let Some(secs) = *undo_countdown.read() { // poly-lint: allow render-time-read — countdown must re-render each tick
            div { class: "reset-undo-row",
                span { class: "reset-undo-text",
                    "Resetting in {secs}s…"
                }
                button {
                    class: "btn btn-secondary btn-sm",
                    onclick: move |_| {
                        // A2.3 undo: set the countdown to None; the in-flight
                        // tick loop checks this each iteration and aborts.
                        undo_countdown.set(None);
                        busy_signal.set(false);
                    },
                    "Undo"
                }
            }
        }
        if *confirm_open.read() { // poly-lint: allow render-time-read — modal visibility must re-render on toggle
            ResetConfirmModal {
                kind,
                on_cancel: move |_| confirm_open.set(false),
                on_confirm: move |_| {
                    confirm_open.set(false);
                    busy_signal.set(true);
                    match kind {
                        ResetKind::Nuke => {
                            // Nuke executes immediately — no undo window.
                            spawn(async move {
                                if let Err(err) = run_reset_flow(kind, client_manager, chat_lists, account_sessions)
                                    .await
                                {
                                    on_error.call(err);
                                    busy_signal.set(false);
                                }
                            });
                        }
                        ResetKind::User => {
                            // A2.3 soft Reset: start a 10-second undo window.
                            undo_countdown.set(Some(10));
                            spawn(async move {
                                for n in (0..10).rev() {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        // lint-allow-unused: fire-and-forget JS timer; recv() ignored.
                                        #[allow(clippy::let_underscore_must_use)]
                                        let _ = dioxus::document::eval(
                                            "setTimeout(() => dioxus.send(true), 1000);",
                                        ).recv::<bool>().await;
                                    }
                                    #[cfg(not(target_arch = "wasm32"))]
                                    {
                                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                    }
                                    // Undo guard: if user cancelled, abort.
                                    if undo_countdown.peek().is_none() {
                                        tracing::info!("Reset undone by user within 10s window");
                                        return;
                                    }
                                    undo_countdown.set(Some(n));
                                }
                                // Countdown expired — execute the reset.
                                undo_countdown.set(None);
                                if let Err(err) = run_reset_flow(kind, client_manager, chat_lists, account_sessions)
                                    .await
                                {
                                    on_error.call(err);
                                    busy_signal.set(false);
                                }
                            });
                        }
                    }
                },
            }
        }
    }
}

/// Confirmation modal for the reset / nuke buttons. The Reset variant is a
/// soft Yes/Cancel; the Nuke variant requires typing the literal word DELETE
/// before the execute button enables.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ResetConfirmModal(
    kind: ResetKind,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut typed = use_signal(String::new);
    let phrase = t("settings-nuke-confirm-phrase");
    let typed_matches = typed.read().as_str() == phrase.as_str(); // poly-lint: allow render-time-read — typed input drives the execute button's enabled state every keystroke
    let requires_typing = matches!(kind, ResetKind::Nuke);
    let execute_enabled = !requires_typing || typed_matches;

    let (title_key, body_key, execute_class) = match kind {
        ResetKind::User => (
            "settings-reset-confirm-title",
            "settings-reset-confirm-body",
            "btn btn-danger",
        ),
        ResetKind::Nuke => (
            "settings-nuke-confirm-title",
            "settings-nuke-confirm-body",
            "btn btn-warning btn-nuke",
        ),
    };

    rsx! {
        div {
            class: "persona-modal-overlay",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "persona-modal persona-confirm-modal",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "persona-modal-header",
                    h3 { class: "persona-modal-title persona-danger-title", "{t(title_key)}" }
                }
                div { class: "persona-modal-body",
                    p { class: "persona-confirm-description", "{t(body_key)}" }
                    if requires_typing {
                        div { class: "settings-field",
                            label { class: "settings-label", "{t(\"settings-nuke-confirm-input-label\")}" }
                            input {
                                r#type: "text",
                                class: "settings-input persona-confirm-input",
                                placeholder: "{phrase}",
                                value: "{typed.read()}", // poly-lint: allow render-time-read — input value must follow the typed signal every keystroke
                                oninput: move |e| typed.set(e.value()),
                            }
                        }
                    }
                }
                div { class: "persona-modal-footer",
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| on_cancel.call(()),
                        "{t(\"settings-confirm-cancel\")}"
                    }
                    button {
                        class: "{execute_class}",
                        disabled: !execute_enabled,
                        onclick: move |_| {
                            if execute_enabled {
                                on_confirm.call(());
                            }
                        },
                        "{t(\"settings-confirm-execute\")}"
                    }
                }
            }
        }
    }
}

/// Error display component.
#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn ResetError(error: Signal<String>) -> Element {
    rsx! {
        if !error.read().is_empty() { // poly-lint: allow render-time-read — subscription IS the intent; error display must re-render when set
            p { class: "general-reset-error", "{error.read()}" } // poly-lint: allow render-time-read — same signal, text body
        }
    }
}

/// Reset actions section with buttons and error handling.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ResetSection() -> Element {
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);

    let mut danger_open = use_signal(|| false);

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
            ResetError { error }
            LoadDemoButton {}

            // A2.2: the Nuke button now lives behind a collapsed "Danger Zone"
            // disclosure so a stray click on the everyday Reset row can't
            // destroy the user's whole install. Confirm modal (A2.1) still
            // gates the actual nuke; this just adds a second visual barrier.
            div { class: "danger-zone",
                button {
                    class: "danger-zone-toggle",
                    onclick: move |_| {
                        let cur = *danger_open.peek();
                        danger_open.set(!cur);
                    },
                    if *danger_open.read() { // poly-lint: allow render-time-read — subscription IS the intent; chevron must redraw when open flips
                        "▾ Danger Zone"
                    } else {
                        "▸ Danger Zone"
                    }
                }
                if *danger_open.read() { // poly-lint: allow render-time-read — same signal, controls visibility of the inner block
                    div { class: "danger-zone-body",
                        p { class: "settings-description",
                            "Irreversible destructive actions. Wipes every account, every cached message, every setting. Type DELETE in the confirm modal to proceed."
                        }
                        ResetButton {
                            kind: ResetKind::Nuke,
                            busy,
                            on_error: move |err: String| {
                                error.set(err);
                                busy.set(false);
                            },
                        }
                    }
                }
            }
        }
    }
}

/// A3.3: dev-only button. After a Nuke (or a reset_app MCP call) the
/// `dev.autoseed_disabled` marker stays in KV, so the test-account auto-signin
/// loop skips on every subsequent boot. Click to clear the marker and reload —
/// the next boot will re-seed all dev test accounts. Only rendered in debug
/// builds since the autoseed flow itself is `cfg(debug_assertions)`.
#[cfg(debug_assertions)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(allow_default)]
#[component]
fn LoadDemoButton() -> Element {
    rsx! {
        button {
            class: "btn btn-secondary",
            title: "Clears the dev.autoseed_disabled marker and reloads. Next boot re-seeds dev test accounts.",
            onclick: move |_| {
                spawn(async move {
                    if let Some(storage) = crate::STORAGE.get() {
                        if let Err(e) = storage.delete(crate::storage::keys::DEV_AUTOSEED_DISABLED).await {
                            tracing::warn!("Failed to clear DEV_AUTOSEED_DISABLED: {e}");
                        }
                    }
                    document::eval("window.location.reload();");
                });
            },
            "Load demo accounts"
        }
    }
}

#[cfg(not(debug_assertions))]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn LoadDemoButton() -> Element {
    rsx! {}
}

/// General settings section.
///
/// Contains shell layout and mirroring preferences.
#[rustfmt::skip]
#[ui_action(LayoutSettingsAction)]
#[context_menu(none)]
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
#[rustfmt::skip]
#[ui_action(GeneralSettingsAction)]
#[context_menu(none)]
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


//! Host hooks for plugin-declared composer buttons and per-message actions.
//!
//! Two components live here (WP 6 — plan-client-ui-surface §7, §4.5):
//!
//! - [`ComposerHooks`] — renders plugin-contributed buttons in the composer
//!   toolbar, filtered to a single [`ComposerSlot`]. The chat view mounts
//!   one instance per slot (left-of-input, right-of-input, above-input).
//! - [`MessageActions`] — renders plugin-contributed per-message action rows,
//!   merged into the existing per-message action bar.
//!
//! Both components fetch fresh from the backend on mount (D24). Errors are
//! silently logged and the component renders nothing — the host-universal
//! items in the surrounding chat view continue to work.

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::ui::account::server::context_menu::ContextMenuItem;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::action_outcome::{handle_action_outcome, ActionOutcomeCx};
use crate::ui::client_ui::toast::ToastMessage;
use dioxus::prelude::*;
use poly_client::{
    ActionOutcome, ClientError, ComposerButton, ComposerSlot, MenuItem, MenuItemVariant,
};
use poly_ui_macros::{context_menu, ui_action};

// ─── Typed actions ───────────────────────────────────────────────────

/// D14 — plugin-declared composer button action.
///
/// Dispatched when the user clicks a `composer-button` declared via
/// `client-composer::get-composer-buttons`. Carries the opaque plugin
/// action-id plus the channel it was invoked against.
#[derive(Debug, Clone)]
pub enum ClientComposerAction {
    /// Invoke a composer-toolbar button on the plugin side.
    InvokeButton {
        account_id: String,
        action_id: String,
        channel_id: String,
    },
}

impl UiAction for ClientComposerAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::InvokeButton {
                account_id,
                action_id,
                channel_id,
            } => {
                dioxus::core::spawn_forever(async move {
                    invoke_composer(&account_id, &action_id, &channel_id).await;
                });
            }
        }
    }
}

/// D14 — plugin-declared per-message action.
///
/// Dispatched when the user clicks an item contributed by
/// `client-composer::get-message-actions`.
#[derive(Debug, Clone)]
pub enum ClientMessageAction {
    /// Invoke a per-message action on the plugin side.
    Invoke {
        account_id: String,
        action_id: String,
        channel_id: String,
        message_id: String,
    },
}

impl UiAction for ClientMessageAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::Invoke {
                account_id,
                action_id,
                channel_id,
                message_id,
            } => {
                dioxus::core::spawn_forever(async move {
                    invoke_message(&account_id, &action_id, &channel_id, &message_id).await;
                });
            }
        }
    }
}

// ─── Components ──────────────────────────────────────────────────────

/// Renders plugin-contributed buttons in the composer toolbar for a single
/// [`ComposerSlot`]. Mount one instance per slot in `chat_view.rs`.
///
/// Fetches items fresh on mount via `get_composer_buttons`; silently renders
/// nothing on plugin error (log-only — matches the `ClientMenu` error policy).
#[ui_action(ClientComposerAction)]
#[context_menu(inherit)]
#[component]
pub fn ComposerHooks(
    account_id: String,
    channel_id: String,
    slot: ComposerSlot,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let buttons_res = {
        let account_id = account_id.clone();
        let channel_id = channel_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            let channel_id = channel_id.clone();
            async move {
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Err(ClientError::NotFound(format!(
                        "no backend for account {account_id}"
                    )));
                };
                let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                    Ok(g) => g,
                    Err(_) => {
                        tracing::warn!("composer: backend read timed out loading composer buttons");
                        return Err(ClientError::Internal("backend read timed out".into()));
                    }
                };
                guard.get_composer_buttons(&channel_id).await
            }
        })
    };

    let buttons: Vec<ComposerButton> = match &*buttons_res.read_unchecked() {
        None => return rsx! {}, // still loading
        Some(Err(err)) => {
            tracing::warn!("ComposerHooks: plugin fetch failed: {err:?}");
            return rsx! {};
        }
        Some(Ok(items)) => items.iter().filter(|b| b.position == slot).cloned().collect(),
    };

    if buttons.is_empty() {
        return rsx! {};
    }

    rsx! {
        for button in buttons {
            {render_composer_button(button, account_id.clone(), channel_id.clone())}
        }
    }
}

fn render_composer_button(
    button: ComposerButton,
    account_id: String,
    channel_id: String,
) -> Element {
    // Plugin FTL bundles are merged into the host i18n store under the
    // `plugin-<id>-*` namespace — resolve via `t()` with raw-key fallback.
    let title = t(&button.label_key);
    let icon = button.icon.clone();
    let action_id = button.id.clone();
    let button_id_attr = format!("composer-button-{}", button.id);

    let onclick = move |_evt: MouseEvent| {
        let action = ClientComposerAction::InvokeButton {
            account_id: account_id.clone(),
            action_id: action_id.clone(),
            channel_id: channel_id.clone(),
        };
        spawn(async move {
            // Apply via the UiAction pipeline. We can't use `ActionCx` here
            // (it borrows component scope), so we just invoke the backend
            // directly — matching the `ClientMenu` dispatch path.
            match action {
                ClientComposerAction::InvokeButton {
                    account_id,
                    action_id,
                    channel_id,
                } => invoke_composer(&account_id, &action_id, &channel_id).await,
            }
        });
    };

    rsx! {
        button {
            // P48: explicit type + aria-label so the button is accessible.
            r#type: "button",
            id: "{button_id_attr}",
            class: "toolbar-btn composer-hook-button",
            title: "{title}",
            aria_label: "{title}",
            onclick,
            // P15: wrap icon in a span so CSS can independently style it.
            span { class: "composer-button-icon", "{icon}" }
        }
    }
}

/// Renders plugin-contributed per-message action rows, inserted at the end
/// of the existing per-message action bar so host universal items appear
/// first.
///
/// Items are fetched fresh on mount. On plugin error, nothing is rendered
/// (same policy as `ComposerHooks`).
#[ui_action(ClientMessageAction)]
#[context_menu(inherit)]
#[component]
pub fn MessageActions(
    account_id: String,
    channel_id: String,
    message_id: String,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let items_res = {
        let account_id = account_id.clone();
        let channel_id = channel_id.clone();
        let message_id = message_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            let channel_id = channel_id.clone();
            let message_id = message_id.clone();
            async move {
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Err(ClientError::NotFound(format!(
                        "no backend for account {account_id}"
                    )));
                };
                let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                    Ok(g) => g,
                    Err(_) => {
                        tracing::warn!("composer: backend read timed out loading message actions");
                        return Err(ClientError::Internal("backend read timed out".into()));
                    }
                };
                guard
                    .get_message_actions(&channel_id, &message_id)
                    .await
            }
        })
    };

    let items: Vec<MenuItem> = match &*items_res.read_unchecked() {
        None => return rsx! {},
        Some(Err(err)) => {
            tracing::warn!("MessageActions: plugin fetch failed: {err:?}");
            return rsx! {};
        }
        Some(Ok(items)) => items.clone(),
    };

    if items.is_empty() {
        return rsx! {};
    }

    rsx! {
        // P33: prepend a separator before plugin-contributed items so they
        // are visually distinct from the host-universal action bar items.
        div { class: "message-action-separator" }
        for item in items {
            {render_message_action_item(
                item,
                account_id.clone(),
                channel_id.clone(),
                message_id.clone(),
            )}
        }
    }
}

fn render_message_action_item(
    item: MenuItem,
    account_id: String,
    channel_id: String,
    message_id: String,
) -> Element {
    // Skip info-block and submenu-header items — per-message action bars
    // are flat buttons, not dropdowns. Plugins wanting submenus on messages
    // should use the context-menu surface (`client-menus`) instead.
    if matches!(
        item.item_variant,
        MenuItemVariant::InfoBlock | MenuItemVariant::SubmenuHeader
    ) {
        return rsx! {};
    }

    let danger = item.item_variant == MenuItemVariant::Destructive;
    let label = t(&item.label_key);
    let action_id = item.id.clone();

    let onclick = move |_evt: MouseEvent| {
        let account_id = account_id.clone();
        let channel_id = channel_id.clone();
        let message_id = message_id.clone();
        let action_id = action_id.clone();
        spawn(async move {
            invoke_message(&account_id, &action_id, &channel_id, &message_id).await;
        });
    };

    rsx! {
        ContextMenuItem {
            label,
            danger,
            onclick,
        }
    }
}

// ─── Dispatch helpers ───────────────────────────────────────────────

async fn invoke_composer(account_id: &str, action_id: &str, channel_id: &str) {
    let client_manager: BatchedSignal<ClientManager> = match try_consume_context() {
        Some(cm) => cm,
        None => {
            tracing::warn!("ComposerHooks: no ClientManager in context during dispatch");
            return;
        }
    };

    let Some(backend) = client_manager.read().get_backend(account_id) else {
        tracing::warn!("ComposerHooks: no backend for account {account_id}");
        return;
    };

    let outcome = {
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("composer: backend read timed out invoking composer action");
                return;
            }
        };
        guard.invoke_composer_action(action_id, channel_id).await
    };

    dispatch_outcome(account_id, outcome, client_manager);
}

async fn invoke_message(
    account_id: &str,
    action_id: &str,
    channel_id: &str,
    message_id: &str,
) {
    let client_manager: BatchedSignal<ClientManager> = match try_consume_context() {
        Some(cm) => cm,
        None => {
            tracing::warn!("MessageActions: no ClientManager in context during dispatch");
            return;
        }
    };

    let Some(backend) = client_manager.read().get_backend(account_id) else {
        tracing::warn!("MessageActions: no backend for account {account_id}");
        return;
    };

    let outcome = {
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("composer: backend read timed out invoking message action");
                return;
            }
        };
        guard
            .invoke_message_action(action_id, channel_id, message_id)
            .await
    };

    dispatch_outcome(account_id, outcome, client_manager);
}

/// Pack B: replace the prior `log_outcome` shim with the shared
/// [`handle_action_outcome`] dispatcher. If the toast queue / refresh signal
/// aren't in context (e.g. snapshot test harness), fall back to a log-only
/// path so the component never panics in limited mounts.
fn dispatch_outcome(
    account_id: &str,
    outcome: Result<ActionOutcome, ClientError>,
    client_manager: BatchedSignal<ClientManager>,
) {
    let Some(toast_queue) = try_consume_context::<Signal<Vec<ToastMessage>>>() else {
        tracing::info!(
            "composer: action outcome (no-toast-ctx) account={account_id}: {outcome:?}"
        );
        return;
    };
    let Some(refresh_sidebar) = try_consume_context::<Signal<u32>>() else {
        tracing::debug!("composer: no sidebar refresh signal in context");
        return;
    };
    let cx = ActionOutcomeCx {
        toast_queue,
        refresh_sidebar,
        refresh_target: None,
        client_manager,
        account_id: account_id.to_string(),
    };
    handle_action_outcome(outcome, cx);
}

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

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::ui::account::server::context_menu::ContextMenuItem;
use crate::ui::actions::{ActionCx, UiAction};
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
    let client_manager: Signal<ClientManager> = use_context();

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
                let guard = backend.read().await;
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
            id: "{button_id_attr}",
            class: "toolbar-btn composer-hook-button",
            title: "{title}",
            onclick,
            "{icon}"
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
    let client_manager: Signal<ClientManager> = use_context();

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
                let guard = backend.read().await;
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
    let client_manager: Signal<ClientManager> = match try_consume_context() {
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
        let guard = backend.read().await;
        guard.invoke_composer_action(action_id, channel_id).await
    };

    log_outcome("invoke_composer_action", action_id, outcome);
}

async fn invoke_message(
    account_id: &str,
    action_id: &str,
    channel_id: &str,
    message_id: &str,
) {
    let client_manager: Signal<ClientManager> = match try_consume_context() {
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
        let guard = backend.read().await;
        guard
            .invoke_message_action(action_id, channel_id, message_id)
            .await
    };

    log_outcome("invoke_message_action", action_id, outcome);
}

fn log_outcome(what: &str, action_id: &str, outcome: Result<ActionOutcome, ClientError>) {
    match outcome {
        Ok(ActionOutcome::Navigate(route)) => {
            // Route-routing wires up in WP 7 / WP 8 (see ClientMenu::dispatch_action).
            tracing::info!("{what}({action_id}): Navigate({route}) — wiring pending");
        }
        Ok(ActionOutcome::Toast(payload)) => {
            tracing::info!("{what}({action_id}): toast {payload:?}");
        }
        Ok(other) => {
            tracing::debug!("{what}({action_id}): outcome {other:?}");
        }
        Err(err) => {
            tracing::warn!("{what}({action_id}) failed: {err:?}");
        }
    }
}

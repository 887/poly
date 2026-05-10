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
use crate::ui::client_ui::use_view_resource::{use_view_resource, ViewQuery};
use dioxus::prelude::*;
use poly_client::{
    ActionOutcome, IsBackend, ClientError, ClientResult, ComposerButton, ComposerSlot,
    MenuItem, MenuItemVariant,
};
use poly_ui_macros::{context_menu, ui_action};

// ── ViewQuery impls for this module ──────────────────────────────────────────

/// Query: fetch plugin-declared composer buttons for a channel.
#[derive(Clone, PartialEq)]
struct ComposerButtonsQuery {
    account_id: String,
    channel_id: String,
}

impl ViewQuery for ComposerButtonsQuery {
    type Output = Vec<ComposerButton>;
    fn account_id(&self) -> &str { &self.account_id }
    async fn fetch(&self, b: &dyn IsBackend) -> ClientResult<Self::Output> {
        b.get_composer_buttons(&self.channel_id).await
    }
}

/// Query: fetch plugin-declared per-message actions for a channel + message.
#[derive(Clone, PartialEq)]
struct MessageActionsQuery {
    account_id: String,
    channel_id: String,
    message_id: String,
}

impl ViewQuery for MessageActionsQuery {
    type Output = Vec<MenuItem>;
    fn account_id(&self) -> &str { &self.account_id }
    async fn fetch(&self, b: &dyn IsBackend) -> ClientResult<Self::Output> {
        b.get_message_actions(&self.channel_id, &self.message_id).await
    }
}

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
    let buttons_res = use_view_resource(ComposerButtonsQuery {
        account_id: account_id.clone(),
        channel_id: channel_id.clone(),
    });

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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
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
    let items_res = use_view_resource(MessageActionsQuery {
        account_id: account_id.clone(),
        channel_id: channel_id.clone(),
        message_id: message_id.clone(),
    });

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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
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

    let outcome = client_manager.peek().with_backend(account_id, async |b| {
        b.invoke_composer_action(action_id, channel_id).await
    }).await;

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

    let outcome = client_manager.peek().with_backend(account_id, async |b| {
        b.invoke_message_action(action_id, channel_id, message_id).await
    }).await;

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

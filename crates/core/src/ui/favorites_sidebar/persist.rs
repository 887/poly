//! Async persistence helpers for the favorites sidebar (Bar 1).
//!
//! All functions are pure-async — they only touch `STORAGE` and do NOT
//! write any `BatchedSignal`. Callers are responsible for updating state
//! before or after invoking these.
//!
//! Extracted from `favorites_sidebar.rs` (C.3 — single-responsibility split).

use crate::state::{BatchedSignal, ChatLists, ChatViewState};

// ── Public persistence helpers ────────────────────────────────────────────

/// Persist the user-defined favorite server order to `AppSettings.favorited_server_ids`.
///
/// Called after every mutation of `ChatData.favorited_server_ids` to survive
/// page reloads, app restarts, and offline periods.
/// No-ops silently if storage is not yet initialised.
pub(crate) async fn persist_favorites(ids: Vec<String>) {
    let Some(s) = crate::STORAGE.get() else {
        return;
    };
    match s.get_app_settings().await {
        Ok(mut settings) => {
            settings.favorited_server_ids = ids;
            if let Err(e) = s.set_app_settings(&settings).await {
                tracing::warn!("Failed to persist favorites: {e}");
            }
        }
        Err(e) => tracing::warn!("Failed to read app_settings for favorites persist: {e}"),
    }
}

/// Persist the Bar-1 account icon order to `AppSettings.account_order`.
///
/// Called after every drag-drop reorder on account icons so users get a
/// stable, restorable layout across page reloads and app restarts.
pub(crate) async fn persist_account_order(order: Vec<String>) {
    let Some(s) = crate::STORAGE.get() else {
        return;
    };
    match s.get_app_settings().await {
        Ok(mut settings) => {
            settings.account_order = order;
            if let Err(e) = s.set_app_settings(&settings).await {
                tracing::warn!("Failed to persist account_order: {e}");
            }
        }
        Err(e) => tracing::warn!("Failed to read app_settings for account_order persist: {e}"),
    }
}

/// Apply user icon and banner overrides from `AppSettings` to all servers in
/// `chat_data`.
///
/// Called after every `load_server_data` and `restore_server_channel` so that
/// overrides entered in the server settings Overview panel survive across page
/// navigations and app restarts.
///
/// No-ops silently if storage is not yet initialised.
pub(crate) async fn apply_server_icon_overrides(
    chat_lists: BatchedSignal<ChatLists>,
    chat_view_state: BatchedSignal<ChatViewState>,
) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(settings) = storage.get_app_settings().await else {
        return;
    };
    if settings.server_icon_overrides.is_empty() && settings.server_banner_overrides.is_empty() {
        return;
    }
    chat_lists.batch(|cl| {
        for server in &mut cl.servers {
            if let Some(url) = settings.server_icon_overrides.get(&server.id) {
                server.icon_url = Some(url.clone());
            }
            if let Some(url) = settings.server_banner_overrides.get(&server.id) {
                server.banner_url = Some(url.clone());
            }
        }
    });
    chat_view_state.batch(|cv| {
        if let Some(ref mut current) = cv.current_server {
            if let Some(url) = settings.server_icon_overrides.get(&current.id) {
                current.icon_url = Some(url.clone());
            }
            if let Some(url) = settings.server_banner_overrides.get(&current.id) {
                current.banner_url = Some(url.clone());
            }
        }
    });
}

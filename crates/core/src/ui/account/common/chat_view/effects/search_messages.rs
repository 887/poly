use dioxus::prelude::*;
use crate::state::use_spawn_once;
use super::super::signals::ChatViewSignals;
use super::super::markup_ctx::ChatViewMarkupCtx;
use super::super::ChatUtilityPanel;
use super::super::search_filter::build_search_query;

pub(in super::super) fn use_search_messages_effect(signals: &ChatViewSignals, ctx: &ChatViewMarkupCtx) {
    let app_state = signals.app_state;
    let client_manager = signals.client_manager;
    let mut search_hits = signals.search_hits;
    let utility_panel = signals.utility_panel;
    let search_query = signals.search_query;
    let current_channel = ctx.current_channel.clone();
    let current_server = ctx.current_server.clone();
    let self_user_id = ctx.self_user_id.clone();
    let is_dm_channel = ctx.is_dm_channel;
    let is_group_channel = ctx.is_group_channel;

    // Key on (query, channel_id) per plan. Panel-state and query changes
    // both drive re-spawn via the component re-rendering with a new key.
    // TODO(use_spawn_once): the hook does NOT debounce — each keystroke
    // issues a fresh search. A separate `use_debounced_effect`-style
    // primitive should wrap this call site. See plan-use-spawn-once §4.
    // **PEEK, not READ** — panel_is_search and raw_query are use_spawn_once
    // keys. A live .read() here subscribes ChatView to every write of
    // utility_panel / search_query; when load_server_data writes app_state
    // (which cascades a chat_data re-render), ChatView re-renders, this setup
    // re-runs, the subscriptions re-fire — perpetual loop (hang class #7).
    let panel_is_search = *utility_panel.peek() == Some(ChatUtilityPanel::Search);
    let raw_query = search_query.peek().trim().to_string();
    let channel_key = current_channel.as_ref().map(|c| c.id.clone());
    use_spawn_once(
        (panel_is_search, raw_query.clone(), channel_key),
        move |(panel_is_search, raw_query, _channel_key)| {
            // Clone captures per-call so the outer Fn closure stays reusable
            // (use_spawn_once requires Fn + Clone, not FnOnce).
            let current_channel = current_channel.clone();
            let current_server = current_server.clone();
            let self_user_id = self_user_id.clone();
            async move {
                if !panel_is_search {
                    return;
                }
                if raw_query.is_empty() {
                    search_hits.set(Vec::new());
                    return;
                }
                let account_id = app_state.peek().nav.active_account_id.cloned();
                let Some(account_id) = account_id else {
                    search_hits.set(Vec::new());
                    return;
                };
                let parsed_query = build_search_query(
                    &raw_query,
                    current_channel.as_ref(),
                    current_server.as_ref(),
                    &self_user_id,
                    is_dm_channel,
                    is_group_channel,
                );
                match client_manager.peek().with_backend(&account_id, async |b| {
                    b.search_messages(parsed_query).await
                }).await {
                    Ok(hits) => search_hits.set(hits),
                    Err(err) => {
                        tracing::warn!("search_messages failed: {err}");
                        search_hits.set(Vec::new());
                    }
                }
            }
        },
    );
}

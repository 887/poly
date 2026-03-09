//! Demo-client lifecycle helpers.
//!
//! Houses the async [`toggle_demo`] function and the background
//! [`spawn_event_stream_listener`] task.
//!
//! These functions previously lived in `favorites_sidebar` when the demo toggle
//! was a button in the sidebar UI. Now that the toggle lives in the
//! dynamically-registered [`crate::ui::settings::plugin_settings::DemoPluginSettings`]
//! page, this module is the correct home.
//!
//! Neither function contains hard-coded demo account IDs. They query
//! [`ClientManager::demo_account_ids`] at runtime so the UI layer has zero
//! knowledge of which specific accounts the demo client creates.

use crate::client_manager::{BackendHandle, ClientManager, PluginSettingsEntry};
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;
// ClientBackend trait must be in scope for `.authenticate()` to be callable on
// DemoClient / DemoClient2 inside the #[cfg(feature = "demo")] activation path.
#[cfg(feature = "demo")]
use poly_client::ClientBackend as _;

/// Toggle the demo client on/off and refresh all data.
///
/// Called from the [`crate::ui::settings::plugin_settings::DemoPluginSettings`]
/// checkbox and by [`super::init_storage`] on startup to restore a previously
/// active demo session. Does NOT navigate — the caller is responsible for
/// routing after this returns.
///
/// # Signal / RefCell discipline — CRITICAL
///
/// This function deliberately uses a **two-phase** approach to avoid
/// `"RefCell already borrowed"` panics in Dioxus WASM:
///
/// - **Phase 1 (async):** All async work (client construction, auth, server
///   map building) is performed WITHOUT holding any Dioxus `Signal` lock.
///   The Dioxus runtime is free to re-render subscribed components during
///   yield points.
/// - **Phase 2 (sync):** State is committed to `ClientManager` via
///   [`ClientManager::commit_demo_activation`] inside a brief `.write()`
///   block with **no** subsequent `.await` on the same guard.
///
/// Never call `signal.write().async_method().await` — the `SignalMut` write
/// guard must never be held across an await boundary.
pub(crate) async fn toggle_demo(
    mut client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    app_state: Signal<AppState>,
) {
    #[cfg(feature = "demo")]
    {
        let is_active = client_manager.read().demo_active;
        if is_active {
            // ── Signal / RefCell discipline — three-phase deactivation ───────────────────
            //
            // Dioxus `spawn` ties the task to the current component scope
            // (`DemoPluginSettings`). If we call `unregister_plugin_settings` before
            // all awaits, the parent (`SettingsAllSections`) re-renders without the demo
            // entry and unmounts `DemoPluginSettings` — which tries to drop/abort the
            // running scope task, causing "RefCell already borrowed" in Dioxus's diff.
            //
            // Fix: keep `plugin_settings` intact (demo entry present) until AFTER the
            // last await. `DemoPluginSettings` stays mounted, the scope borrow is alive,
            // no conflict. We unregister as the final synchronous step before returning.

            // Phase 1: collect data with brief read locks — all guards dropped before
            // any write or await.
            let demo_ids = client_manager.read().demo_account_ids();
            let (demo_server_ids, new_fav_ids) = {
                let cd = chat_data.read();
                let sids: Vec<String> = cd
                    .servers
                    .iter()
                    .filter(|s| s.backend == poly_client::BackendType::Demo)
                    .map(|s| s.id.clone())
                    .collect();
                let fav_ids: Vec<String> = cd
                    .favorited_server_ids
                    .iter()
                    .filter(|id| !sids.contains(*id))
                    .cloned()
                    .collect();
                (sids, fav_ids)
            };
            let _ = demo_server_ids; // used only to compute new_fav_ids above

            // Phase 2: synchronous state writes — batched into the fewest possible
            // `write()` guards to minimise dirty notifications.
            // No `.await` may appear between the first write and the end of this block.
            client_manager.write().deactivate_demo();
            {
                let mut cd = chat_data.write();
                cd.servers
                    .retain(|s| s.backend != poly_client::BackendType::Demo);
                cd.dm_channels
                    .retain(|d| d.backend != poly_client::BackendType::Demo);
                cd.groups
                    .retain(|g| g.backend != poly_client::BackendType::Demo);
                cd.notifications
                    .retain(|n| n.backend != poly_client::BackendType::Demo);
                cd.friends
                    .retain(|u| u.backend != poly_client::BackendType::Demo);
                cd.channels.clear();
                cd.messages.clear();
                cd.members.clear();
                cd.current_server = None;
                cd.current_channel = None;
                cd.voice_channel_participants.clear();
                cd.voice_connection = None;
                for aid in &demo_ids {
                    cd.account_sessions.remove(aid.as_str());
                }
                cd.favorited_server_ids = new_fav_ids.clone();
                cd.dragging_server_id = None;
            }

            // Phase 3: async storage persist.
            // At this point `plugin_settings` still contains the demo entry, so
            // `SettingsAllSections` continues to render `DemoPluginSettings` through
            // these await points — the scope remains valid and no RefCell conflict arises.
            if let Some(s) = crate::STORAGE.get() {
                let mut settings = s.get_app_settings().await.unwrap_or_default();
                settings.demo_active = false;
                settings.favorited_server_ids = new_fav_ids;
                if let Err(e) = s.set_app_settings(&settings).await {
                    tracing::warn!("Failed to persist demo_active=false: {e}");
                }
            }

            // Phase 4: unregister the demo settings page — the LAST operation.
            // By the time Dioxus processes this signal change and unmounts
            // `DemoPluginSettings`, this task has returned and there is no active
            // borrow on the component scope.
            client_manager.write().unregister_plugin_settings("demo");
        } else {
            // ── Phase 1: async auth WITHOUT holding any Dioxus Signal lock ──────────────
            //
            // Create and authenticate both demo backends locally, with no borrow on
            // `client_manager` signal. Only plain Rust / tokio async here — no Signal
            // reads or writes until Phase 2.

            // Guard: bail out if demo activated concurrently (safe to re-enter).
            if client_manager.read().demo_active {
                return;
            }

            // Authenticate the cat demo account.
            let cat_result: Result<(poly_client::Session, BackendHandle), String> = async {
                let mut client = poly_demo::DemoClient::new();
                let session = client
                    .authenticate(poly_client::AuthCredentials::Token(
                        "demo-token".to_string(),
                    ))
                    .await
                    .map_err(|e| format!("Demo (cat) auth failed: {e}"))?;
                let handle: BackendHandle = std::sync::Arc::new(tokio::sync::RwLock::new(
                    Box::new(client) as Box<dyn poly_client::ClientBackend>,
                ));
                Ok((session, handle))
            }
            .await;

            // Authenticate the dog demo account.
            let dog_result: Result<(poly_client::Session, BackendHandle), String> = async {
                let mut client = poly_demo::DemoClient2::new();
                let session = client
                    .authenticate(poly_client::AuthCredentials::Token(
                        "demo2-token".to_string(),
                    ))
                    .await
                    .map_err(|e| format!("Demo (dog) auth failed: {e}"))?;
                let handle: BackendHandle = std::sync::Arc::new(tokio::sync::RwLock::new(
                    Box::new(client) as Box<dyn poly_client::ClientBackend>,
                ));
                Ok((session, handle))
            }
            .await;

            let (cat_session, cat_handle) = match cat_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Failed to activate demo (cat): {e}");
                    return;
                }
            };
            let (dog_session, dog_handle) = match dog_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Failed to activate demo (dog): {e}");
                    return;
                }
            };

            // Build the server→account map from our locally-owned backend handles.
            // tokio RwLock reads are fine to await here — they are NOT Dioxus Signal locks.
            let mut server_map = std::collections::HashMap::new();
            {
                let guard = cat_handle.read().await;
                if let Ok(servers) = guard.get_servers().await {
                    for server in servers {
                        server_map.insert(server.id, "demo-cat".to_string());
                    }
                }
            }
            {
                let guard = dog_handle.read().await;
                if let Ok(servers) = guard.get_servers().await {
                    for server in servers {
                        server_map.insert(server.id, "demo-dog".to_string());
                    }
                }
            }

            // ── Phase 2: commit all state synchronously (brief write, NO await) ─────────
            let entries = vec![
                ("demo-cat".to_string(), cat_session, cat_handle),
                ("demo-dog".to_string(), dog_session, dog_handle),
            ];
            client_manager
                .write()
                .commit_demo_activation(entries, server_map);

            // Register the demo settings page in the nav sidebar.
            // This mirrors what the WASM plugin host does via the `plugin-metadata`
            // WIT interface — the host calls `register_plugin_settings` so the settings
            // UI is entirely decoupled from knowing about any specific plugin.
            client_manager
                .write()
                .register_plugin_settings(PluginSettingsEntry {
                    slug: "demo",
                    nav_label_key: "plugin-demo-title",
                    nav_icon: "🧪",
                    render: crate::ui::demo_settings_render_fn,
                });

            // ── Phase 3: load data from the committed backends ───────────────────────────
            //
            // For all async reads below: snapshot backend Arc handles first (brief read,
            // lock released), then do async work without any Signal lock held.
            let demo_ids = client_manager.read().demo_account_ids();

            // Copy sessions from ClientManager into chat_data.
            // Collect owned values first so the signal read lock is released before writing.
            let sessions_to_insert: Vec<(String, poly_client::Session)> = {
                let cm = client_manager.read();
                demo_ids
                    .iter()
                    .filter_map(|aid| cm.sessions.get(aid).map(|s| (aid.clone(), s.clone())))
                    .collect()
            };
            for (aid, sess) in sessions_to_insert {
                chat_data.write().account_sessions.insert(aid, sess);
            }

            // Clone backend Arc handles so no Signal read lock is held across awaits.
            let backend_handles: Vec<(String, BackendHandle)> = {
                let cm = client_manager.read();
                demo_ids
                    .iter()
                    .filter_map(|aid| cm.get_backend(aid).map(|b| (aid.clone(), b)))
                    .collect()
            };

            // Load all servers.
            let mut servers = Vec::new();
            for (_, backend) in &backend_handles {
                let guard = backend.read().await;
                if let Ok(mut s) = guard.get_servers().await {
                    servers.append(&mut s);
                }
            }
            // Pre-populate favorites with all demo servers so Bar 1 shows them immediately.
            // Users can remove entries by rearranging; dragging from Bar 2 adds to this list.
            for sid in servers.iter().map(|s| s.id.clone()) {
                if !chat_data.read().favorited_server_ids.contains(&sid) {
                    chat_data.write().favorited_server_ids.push(sid);
                }
            }
            chat_data.write().servers = servers;

            // Load DMs, groups, notifications, friends from all demo accounts.
            for (aid, backend) in &backend_handles {
                let guard = backend.read().await;
                if let Ok(dms) = guard.get_dm_channels().await {
                    chat_data.write().dm_channels.extend(dms);
                }
                if let Ok(groups) = guard.get_groups().await {
                    chat_data.write().groups.extend(groups);
                }
                if let Ok(notifs) = guard.get_notifications().await {
                    chat_data.write().notifications.extend(notifs);
                }
                if let Ok(friends) = guard.get_friends().await {
                    // Deduplicate friends by ID.
                    for friend in friends {
                        if !chat_data.read().friends.iter().any(|f| f.id == friend.id) {
                            chat_data.write().friends.push(friend);
                        }
                    }
                }
                // Load voice participants for all voice channels.
                let servers_snapshot = chat_data.read().servers.clone();
                for server in &servers_snapshot {
                    if server.account_id != *aid {
                        continue;
                    }
                    if let Ok(channels) = guard.get_channels(&server.id).await {
                        for ch in channels {
                            if matches!(
                                ch.channel_type,
                                poly_client::ChannelType::Voice | poly_client::ChannelType::Video
                            ) && let Ok(participants) =
                                guard.get_voice_participants(&ch.id).await
                                && !participants.is_empty()
                            {
                                chat_data
                                    .write()
                                    .voice_channel_participants
                                    .insert(ch.id.clone(), participants);
                            }
                        }
                    }
                }
            }

            // Persist demo_active=true and setup_complete=true to storage so
            // the demo client is restored on next app launch without re-toggling.
            if let Some(s) = crate::STORAGE.get() {
                let mut settings = s.get_app_settings().await.unwrap_or_default();
                settings.demo_active = true;
                settings.setup_complete = true;
                settings.favorited_server_ids = chat_data.read().favorited_server_ids.clone();
                if let Err(e) = s.set_app_settings(&settings).await {
                    tracing::warn!("Failed to persist demo_active=true: {e}");
                }
            }

            // Start real-time event stream listeners for each demo account.
            // Re-use the already-cloned handles — no extra Signal read needed.
            for (aid, backend) in backend_handles {
                spawn_event_stream_listener(aid, backend, app_state, chat_data, client_manager);
            }
        }
    }
}

/// Start a background event-stream listener for a single backend account.
///
/// Spawns a Dioxus task that polls the backend's
/// [`poly_client::ClientBackend::event_stream`] and processes each incoming
/// [`poly_client::ClientEvent`]:
///
/// - [`poly_client::ClientEvent::MessageReceived`] — appends the message to
///   `chat_data.messages` when the current channel is selected.
/// - [`poly_client::ClientEvent::PresenceChanged`] — updates presence on matching members.
/// - Other events are silently ignored for now.
///
/// The task exits automatically when `client_manager.demo_active` becomes false
/// (checked after each event) so there is no orphan task after demo is toggled off.
pub(crate) fn spawn_event_stream_listener(
    account_id: String,
    backend: BackendHandle,
    app_state: Signal<AppState>,
    mut chat_data: Signal<ChatData>,
    client_manager: Signal<ClientManager>,
) {
    use futures::StreamExt as _;
    use poly_client::ClientEvent;

    spawn(async move {
        // Acquire the stream without holding the lock for the duration of polling.
        let stream = {
            let guard = backend.read().await;
            guard.event_stream()
        };
        let mut stream = stream;

        tracing::debug!("Event stream started for account: {account_id}");

        while let Some(event) = stream.next().await {
            // Stop the listener when demo is deactivated (or account removed).
            let still_active = {
                let cm = client_manager.read();
                cm.demo_active && cm.get_backend(&account_id).is_some()
            };
            if !still_active {
                break;
            }

            match event {
                ClientEvent::MessageReceived {
                    ref channel_id,
                    ref message,
                } => {
                    let selected = app_state.read().nav.selected_channel.clone();
                    if selected.as_deref() == Some(channel_id.as_str()) {
                        // Currently viewing this channel — append message live.
                        chat_data.write().messages.push(message.clone());
                        tracing::trace!(
                            "Live message in #{channel_id}: {}",
                            message.author.display_name
                        );
                    }
                    // TODO(phase-3): increment unread count for other channels
                }
                ClientEvent::PresenceChanged {
                    ref user_id,
                    status,
                } => {
                    let mut cd = chat_data.write();
                    for member in &mut cd.members {
                        if member.id == *user_id {
                            member.presence = status;
                            break;
                        }
                    }
                }
                ClientEvent::TypingStarted { .. } => {
                    // TODO(phase-3): show typing indicator in chat view
                }
                _ => {}
            }
        }

        tracing::debug!("Event stream ended for account: {account_id}");
    });
}

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

use crate::state::BatchedSignal;
use crate::client_manager::{BackendHandle, BackendHandleExt, ClientManager, PluginSettingsEntry};
use crate::state::{AccountSessions, AppState, ChatAction, DragState, NavState, UiLayout, UiOverlays, UserPrefs, VoiceState};
use dioxus::prelude::*;
// ClientBackend trait must be in scope for `.authenticate()` to be callable on
// DemoClient / DemoClient2 inside the #[cfg(feature = "demo")] activation path.
#[cfg(feature = "demo")]
use poly_client::IsBackend as _;

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
    client_manager: BatchedSignal<ClientManager>,
    voice_state: BatchedSignal<VoiceState>,
    drag_state: BatchedSignal<DragState>,
    app_state: BatchedSignal<AppState>,
    nav: BatchedSignal<NavState>,
    _ui_layout: BatchedSignal<UiLayout>,
    _ui_overlays: BatchedSignal<UiOverlays>,
    _user_prefs: BatchedSignal<UserPrefs>,
    chat_lists: BatchedSignal<crate::state::ChatLists>,
    account_sessions: BatchedSignal<crate::state::AccountSessions>,
    chat_view_state: BatchedSignal<crate::state::ChatViewState>,
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
                let sids: Vec<String> = chat_lists
                    .peek()
                    .servers
                    .iter()
                    .filter(|s| s.backend == "demo")
                    .map(|s| s.id.clone())
                    .collect();
                let fav_ids: Vec<String> = account_sessions
                    .peek()
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
            client_manager.batch(super::super::client_manager::ClientManager::deactivate_demo);
            {
                let new_fav_ids_c = new_fav_ids.clone();
                let demo_ids_c = demo_ids.clone();
                let demo_ids_as = demo_ids.clone();
                chat_lists.batch(move |cl| {
                    cl.set_servers(cl.servers.iter().filter(|s| {
                        s.backend != "demo" && s.backend != "demo_forum"
                    }).cloned().collect());
                    cl.dm_channels.retain(|d| {
                        d.backend != "demo" && d.backend != "demo_forum"
                    });
                    cl.groups.retain(|g| {
                        g.backend != "demo" && g.backend != "demo_forum"
                    });
                    cl.notifications.retain(|n| {
                        n.backend != "demo" && n.backend != "demo_forum"
                    });
                    for aid in &demo_ids_c {
                        cl.friends.remove(aid.as_str());
                    }
                });
                account_sessions.batch(move |as_| {
                    for aid in &demo_ids_as {
                        as_.blocked_users.remove(aid.as_str());
                        as_.account_sessions.remove(aid.as_str());
                    }
                    as_.favorited_server_ids = new_fav_ids_c;
                });
                // clear channel context in ChatViewState (already have chat_lists from param)
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                drag_state.batch(|d| { d.dragging_server_id = None; });
                voice_state.batch(|v| {
                    v.voice_channel_participants.clear();
                    v.voice_connection = None;
                });
            }

            // Phase 3: async storage persist.
            if let Some(s) = crate::STORAGE.get() {
                let mut settings = s.get_app_settings().await.unwrap_or_default();
                settings.demo_active = false;
                settings.favorited_server_ids = new_fav_ids;
                if let Err(e) = s.set_app_settings(&settings).await {
                    tracing::warn!("Failed to persist demo_active=false: {e}");
                }
            }

            // NOTE: We intentionally do NOT call unregister_plugin_settings("demo").
            // The Demo Settings page must remain accessible even when demo data is
            // disabled so the user can re-enable it from the same page.  Removing
            // the unregister call also eliminates the "RefCell already borrowed"
            // risk that previously required this to be the very last operation.
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
                // lint-allow-unused: trait-object up-cast for type erasure into BackendHandle
                #[allow(clippy::as_conversions)]
                let handle: BackendHandle = std::sync::Arc::new(tokio::sync::RwLock::new(
                    Box::new(client) as Box<dyn poly_client::IsBackend>,
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
                // lint-allow-unused: trait-object up-cast for type erasure into BackendHandle
                #[allow(clippy::as_conversions)]
                let handle: BackendHandle = std::sync::Arc::new(tokio::sync::RwLock::new(
                    Box::new(client) as Box<dyn poly_client::IsBackend>,
                ));
                Ok((session, handle))
            }
            .await;

            // Authenticate the platypus demo_forum account.
            let platypus_result: Result<(poly_client::Session, BackendHandle), String> = async {
                let mut client = poly_demo::DemoClient3::new();
                let session = client
                    .authenticate(poly_client::AuthCredentials::Token(
                        "demo3-token".to_string(),
                    ))
                    .await
                    .map_err(|e| format!("Demo (platypus) auth failed: {e}"))?;
                // lint-allow-unused: trait-object up-cast for type erasure into BackendHandle
                #[allow(clippy::as_conversions)]
                let handle: BackendHandle = std::sync::Arc::new(tokio::sync::RwLock::new(
                    Box::new(client) as Box<dyn poly_client::IsBackend>,
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
            let (platypus_session, platypus_handle) = match platypus_result {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Failed to activate demo (platypus): {e}");
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
            {
                let guard = platypus_handle.read().await;
                if let Ok(servers) = guard.get_servers().await {
                    for server in servers {
                        server_map.insert(server.id, "demo-platypus".to_string());
                    }
                }
            }

            // ── Phase 2: commit all state synchronously (brief write, NO await) ─────────
            let entries = vec![
                ("demo-cat".to_string(), cat_session, cat_handle),
                ("demo-dog".to_string(), dog_session, dog_handle),
                ("demo-platypus".to_string(), platypus_session, platypus_handle),
            ];
            // Batch both sync writes — one guard, one cascade.
            client_manager.batch(move |cm| {
                cm.commit_demo_activation(entries, server_map);
                // Register the demo settings page in the nav sidebar.
                // This mirrors what the WASM plugin host does via the `plugin-metadata`
                // WIT interface — the host calls `register_plugin_settings` so the settings
                // UI is entirely decoupled from knowing about any specific plugin.
                cm.register_plugin_settings(PluginSettingsEntry {
                    slug: "demo",
                    nav_label_key: "plugin-demo-title",
                    nav_icon: "🧪",
                    render: crate::ui::demo_settings_render_fn,
                });
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
            account_sessions.batch(|as_| {
                for (aid, sess) in sessions_to_insert {
                    as_.account_sessions.insert(aid, sess);
                }
            });

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
                let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                    Ok(g) => g,
                    Err(_) => {
                        tracing::warn!("demo: backend read timed out in load_all_servers loop");
                        continue;
                    }
                };
                if let Ok(mut s) = guard.get_servers().await {
                    servers.append(&mut s);
                }
            }
            // Pre-populate favorites with all demo servers so Bar 1 shows them immediately.
            // Users can remove entries by rearranging; dragging from Bar 2 adds to this list.
            {
                let fav_sids: Vec<String> = servers.iter().map(|s| s.id.clone()).collect();
                chat_lists.batch(move |cl| {
                    cl.set_servers(servers);
                });
                account_sessions.batch(move |as_| {
                    for sid in fav_sids {
                        if !as_.favorited_server_ids.contains(&sid) {
                            as_.favorited_server_ids.push(sid);
                        }
                    }
                });
            }

            // Load DMs, groups, notifications, friends from all demo accounts.
            for (aid, backend) in &backend_handles {
                let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                    Ok(g) => g,
                    Err(_) => {
                        tracing::warn!("demo: backend read timed out in load_dm_groups_friends loop");
                        continue;
                    }
                };
                let (dms, groups) = if let Some(dg) = guard.as_dms_and_groups() {
                    (dg.get_dm_channels().await.ok(), dg.get_groups().await.ok())
                } else {
                    (None, None)
                };
                let is_forum = {
                    let slug = account_sessions.peek().account_sessions.get(aid)
                        .map(|s| s.backend.slug().to_string());
                    slug.is_some_and(|sl| client_manager.peek().capabilities_for_slug(&sl).is_forum_layout())
                };
                let notifs = if !is_forum {
                    guard.get_notifications().await.ok()
                } else {
                    None
                };
                let friends = if let Some(sg) = guard.as_social_graph() {
                    sg.get_friends().await.ok()
                } else {
                    None
                };
                // Split writes by sub-signal.
                let aid_c = aid.clone();
                let aid_as = aid.clone();
                chat_lists.batch(move |cl| {
                    if let Some(dms) = dms {
                        cl.dm_channels.extend(dms);
                    }
                    if let Some(groups) = groups {
                        cl.groups.extend(groups);
                    }
                    if let Some(notifs) = notifs {
                        cl.notifications.extend(notifs.into_iter().filter(|n| !n.read));
                    }
                    if let Some(friends) = friends {
                        for friend in friends {
                            let already = cl.friends.get(aid_c.as_str()).is_some_and(|v| v.iter().any(|f| f.id == friend.id));
                            if !already {
                                cl.friends.entry(aid_c.clone()).or_default().push(friend);
                            }
                        }
                    }
                });
                // Load content policy and blocked users for the first account only.
                account_sessions.batch(move |as_| {
                    if !as_.blocked_users.contains_key(aid_as.as_str()) {
                        as_.content_policy = poly_demo::data::demo_content_policy();
                        as_.blocked_users.insert(aid_as.clone(), poly_demo::data::demo_blocked_users());
                    }
                });
                // Load voice participants for all voice channels.
                let servers_snapshot = chat_lists.peek().servers.clone();
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
                                let chid = ch.id.clone();
                                voice_state.batch(move |v| {
                                    v.voice_channel_participants.insert(chid, participants);
                                });
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
                settings.favorited_server_ids = account_sessions.peek().favorited_server_ids.clone();
                if let Err(e) = s.set_app_settings(&settings).await {
                    tracing::warn!("Failed to persist demo_active=true: {e}");
                }
            }

            // Start real-time event stream listeners for each demo account.
            // Re-use the already-cloned handles — no extra Signal read needed.
            for (aid, backend) in backend_handles {
                spawn_event_stream_listener(aid, backend, app_state, nav, client_manager, chat_view_state, account_sessions, voice_state);
            }
        }
    }
}

/// Re-activate the Forum Demo (DemoClient3 / demo-platypus) after it was toggled off
/// independently of the main demo toggle.
///
/// Mirrors the platypus activation branch inside [`toggle_demo`] but operates on
/// the forum account alone. Safe to call when the main demo (cat + dog) is active or
/// inactive — it only touches the `demo-platypus` entry.
#[cfg(feature = "demo")]
pub(crate) async fn toggle_demo_forum_on(
    client_manager: BatchedSignal<ClientManager>,
    chat_lists: BatchedSignal<crate::state::ChatLists>,
    account_sessions: BatchedSignal<crate::state::AccountSessions>,
) {
    // Guard: bail if platypus is already registered.
    if client_manager.read().sessions.contains_key("demo-platypus") {
        return;
    }

    // Phase 1: async auth — no Signal lock held.
    let result: Result<(poly_client::Session, BackendHandle), String> = async {
        let mut client = poly_demo::DemoClient3::new();
        let session = client
            .authenticate(poly_client::AuthCredentials::Token(
                "demo3-token".to_string(),
            ))
            .await
            .map_err(|e| format!("Forum Demo auth failed: {e}"))?;
        // lint-allow-unused: trait-object up-cast for type erasure into BackendHandle
        #[allow(clippy::as_conversions)]
        let handle: BackendHandle = std::sync::Arc::new(tokio::sync::RwLock::new(
            Box::new(client) as Box<dyn poly_client::IsBackend>,
        ));
        Ok((session, handle))
    }
    .await;

    let (session, handle) = match result {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("{e}");
            return;
        }
    };

    // Fetch servers (no Signal lock — plain tokio RwLock).
    let servers = {
        let guard = handle.read().await;
        guard.get_servers().await.unwrap_or_default()
    };
    let server_map: std::collections::HashMap<String, String> = servers
        .iter()
        .map(|s| (s.id.clone(), "demo-platypus".to_string()))
        .collect();

    // Phase 2: commit synchronously (brief write, no await).
    {
        let sess = session.clone();
        client_manager.batch(move |cm| {
            cm.commit_backend_account("demo-platypus".to_string(), sess, handle, server_map);
        });
    }
    account_sessions.batch(move |as_| {
        as_.account_sessions.insert("demo-platypus".to_string(), session);
    });
    chat_lists.batch(move |cl| {
        // Populate servers + favorites.
        for srv in servers {
            if !cl.servers.iter().any(|s| s.id == srv.id) {
                cl.push_server(srv.clone());
            }
        }
    });
    account_sessions.batch(move |as_| {
        let cl = chat_lists.peek();
        for srv in cl.servers.iter().filter(|s| s.account_id == "demo-platypus") {
            if !as_.favorited_server_ids.contains(&srv.id) {
                as_.favorited_server_ids.push(srv.id.clone());
            }
        }
    });

    tracing::info!("Forum Demo (demo-platypus) re-activated");
}

/// Start a background event-stream listener for a single backend account.
///
/// Delegates to [`crate::event_stream::spawn_event_stream_listener`].
/// Kept here for backwards-compat with the demo-toggle call site.
pub(crate) fn spawn_event_stream_listener(
    account_id: String,
    backend: BackendHandle,
    app_state: BatchedSignal<AppState>,
    nav: BatchedSignal<NavState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_view_state: BatchedSignal<crate::state::ChatViewState>,
    account_sessions: BatchedSignal<AccountSessions>,
    voice_state: BatchedSignal<VoiceState>,
) {
    crate::event_stream::spawn_event_stream_listener(
        account_id,
        backend,
        app_state,
        nav,
        client_manager,
        chat_view_state,
        account_sessions,
        voice_state,
    );
}


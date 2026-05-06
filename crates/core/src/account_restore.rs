//! Generic native-backend restore for boot and toggle-on.
//!
//! Mirrors [`crate::ui::restore_poly_accounts`] but handles every non-poly
//! native backend: matrix, discord, teams, stoat, lemmy, github, forgejo,
//! hackernews.
//!
//! ## Entry points
//!
//! - Boot: called from `init_storage` with `slug_filter = None` to restore all
//!   native accounts that have persisted tokens.
//! - Toggle-on: called from the plugin-settings toggle handler with
//!   `slug_filter = Some("matrix")` (or whichever slug was re-enabled).
//!
//! ## Hang-safety
//!
//! - All `Signal` mutations go through `BatchedSignal::batch` (one guard per
//!   call, no cascades) — CLAUDE.md hang class #1.
//! - No `Signal` guard is held across an `.await` — hang class #2.
//! - All `BackendHandle::read().await` calls use `read_with_timeout` —
//!   hang class #4.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::ReadableExt as _;
use poly_client::{AuthCredentials, BackendType, ClientBackend, PresenceStatus, Session, User};
use tokio::sync::RwLock;

use crate::client_manager::{BackendHandle, BackendHandleExt};
use crate::state::{AccountSessions, BatchedSignal, ChatLists};
use crate::storage::{OfflineServerRecord, Storage};
use crate::client_manager::ClientManager;

// ── Per-slug factory ──────────────────────────────────────────────────────────

/// Construct a fresh, unauthenticated backend for the given slug.
///
/// Returns `None` for slugs that have their own restore path (`poly`,
/// `demo`, `demo_chat`, `demo_forum`) or that are not compiled in.
fn build_backend_for_slug(
    slug: &str,
    instance_id: Option<&str>,
) -> Option<Box<dyn ClientBackend + Send + Sync>> {
    // `instance_id` is only consumed by url-bound backends (matrix, lemmy,
    // forgejo). When none of those features are compiled in, the binding
    // appears unused — silence the warning rather than introducing a
    // banned `#[allow(unused_variables)]`.
    let _ = instance_id;
    match slug {
        #[cfg(feature = "matrix")]
        "matrix" => {
            let client: Box<dyn ClientBackend + Send + Sync> =
                match instance_id {
                    Some(url) => match poly_matrix::MatrixClient::with_homeserver(url) {
                        Ok(c) => Box::new(c),
                        Err(e) => {
                            tracing::warn!(
                                "account_restore: matrix with_homeserver({url}) failed: {e}"
                            );
                            return None;
                        }
                    },
                    None => Box::new(poly_matrix::MatrixClient::new()),
                };
            Some(client)
        }

        #[cfg(feature = "discord")]
        "discord" => Some(Box::new(poly_discord::DiscordClient::new())),

        #[cfg(feature = "teams")]
        "teams" => Some(Box::new(poly_teams::TeamsClient::new())),

        #[cfg(feature = "stoat")]
        "stoat" => Some(Box::new(poly_stoat::StoatClient::new())),

        #[cfg(feature = "hackernews")]
        "hackernews" => Some(Box::new(poly_hackernews::HackerNewsClient::new())),

        #[cfg(feature = "lemmy")]
        "lemmy" => {
            let url = instance_id?;
            Some(Box::new(poly_lemmy::LemmyClient::new(url)))
        }

        #[cfg(feature = "github")]
        "github" => Some(Box::new(poly_github::GitHubClient::default())),

        #[cfg(feature = "forgejo")]
        "forgejo" => {
            let url = instance_id?;
            Some(Box::new(poly_forgejo::ForgejoClient::new(url)))
        }

        #[cfg(feature = "reddit")]
        "reddit" => {
            let client = match instance_id {
                Some(url) => poly_reddit::RedditClient::with_base_url(url.to_string()),
                None => poly_reddit::RedditClient::new(),
            };
            match client {
                Ok(c) => Some(Box::new(poly_reddit::backend::RedditBackend::new(c))),
                Err(e) => {
                    tracing::warn!(
                        "account_restore: reddit client construction failed: {e}"
                    );
                    None
                }
            }
        }

        // poly has its own restore path; demo slugs are ephemeral.
        "poly" | "demo" | "demo_chat" | "demo_forum" => None,

        other => {
            tracing::debug!("account_restore: no factory for slug {other:?}");
            None
        }
    }
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Restore persisted native (non-poly) backend accounts.
///
/// - `slug_filter = None`          → restore all native backends (boot path).
/// - `slug_filter = Some("matrix")` → restore only that backend (toggle-on path).
///
/// Skips accounts that:
/// - match a slug in `client_manager.disabled_native_backends`.
/// - are already present in `client_manager.sessions`.
/// - belong to the `poly` / demo backends (those have dedicated paths).
pub async fn restore_native_accounts(
    storage: &Storage,
    client_manager: BatchedSignal<ClientManager>,
    chat_lists: BatchedSignal<ChatLists>,
    account_sessions: BatchedSignal<AccountSessions>,
    slug_filter: Option<&str>,
) {
    let Ok(tokens) = storage.get_account_tokens().await else {
        return;
    };

    // Pre-populate `expected_account_ids` so route guards (`route_targets_
    // unknown_account`) can defer the "unknown account → bounce to /settings"
    // verdict on cold boot when a deep link targets an account that's
    // about to be restored. The set is consulted alongside `active_account_ids`.
    client_manager.batch(|c| {
        for t in &tokens {
            c.expected_account_ids.insert(t.account_id.clone());
        }
    });

    // Snapshot the current disabled list and already-restored sessions so
    // we don't hold any Signal guard across awaits.
    let (disabled, already_restored): (Vec<String>, Vec<String>) = {
        let cm = client_manager.peek();
        let disabled = cm.disabled_native_backends.clone();
        let already_restored = cm.sessions.keys().cloned().collect();
        (disabled, already_restored)
    };

    // Filter to tokens we should actually process.
    let candidate_tokens: Vec<_> = tokens
        .into_iter()
        .filter(|t| {
            // Skip poly + demo — they have their own paths.
            if matches!(t.backend.as_str(), "poly" | "demo" | "demo_chat" | "demo_forum") {
                return false;
            }
            // Apply the optional slug filter.
            if let Some(slug) = slug_filter
                && t.backend != slug {
                    return false;
                }
            // Skip disabled backends.
            if disabled.contains(&t.backend) {
                return false;
            }
            // Skip already-restored accounts.
            if already_restored.contains(&t.account_id) {
                return false;
            }
            true
        })
        .collect();

    if candidate_tokens.is_empty() {
        return;
    }

    tracing::info!(
        "account_restore: restoring {} native account(s) (filter={:?})",
        candidate_tokens.len(),
        slug_filter,
    );

    for token in candidate_tokens {
        let Some(mut backend) = build_backend_for_slug(
            &token.backend,
            token.instance_id.as_deref(),
        ) else {
            continue;
        };

        let credentials = AuthCredentials::Token(token.token.clone());
        match backend.authenticate(credentials).await {
            Ok(mut session) => {
                // Mirror the signup-time avatar overlay: when a backend
                // returns no avatar (or a non-http one) and the display
                // name matches a known animal test account, swap in the
                // bundled cute portrait so restored accounts don't degrade
                // to letter-fallback bubbles.
                #[cfg(feature = "demo")]
                if (session.user.avatar_url.is_none()
                    || session
                        .user
                        .avatar_url
                        .as_deref()
                        .is_some_and(|u| !u.starts_with("http")))
                    && let Some(url) =
                        poly_demo::data::test_animal_avatar(&session.user.display_name)
                {
                    session.user.avatar_url = Some(url);
                }
                let account_id = session.id.clone();
                // lint-allow-unused: trait-object up-cast from Box<dyn T> with
                // additional auto-trait bounds; safe because backend is `Send + Sync`.
                #[allow(clippy::as_conversions)]
                let backend_handle: BackendHandle =
                    Arc::new(RwLock::new(backend as Box<dyn ClientBackend + Send + Sync>));

                // Build server → account map.
                let mut server_map = HashMap::new();
                let servers = match backend_handle
                    .read_with_timeout(Duration::from_secs(30))
                    .await
                {
                    Ok(guard) => guard.get_servers().await.unwrap_or_default(),
                    Err(e) => {
                        tracing::warn!("account_restore: get_servers timeout for {account_id}: {e}");
                        Vec::new()
                    }
                };
                for srv in &servers {
                    server_map.insert(srv.id.clone(), account_id.clone());
                }

                // Commit session + handle — drop guard before any await.
                {
                    let aid = account_id.clone();
                    let sess = session.clone();
                    let bh = backend_handle.clone();
                    client_manager.batch(move |cm| {
                        cm.commit_backend_account(aid.clone(), sess, bh, server_map);
                        // Account is now active; remove from the
                        // expected-pending set so it stops deferring
                        // route-guard checks.
                        cm.expected_account_ids.remove(&aid);
                    });
                }
                {
                    let aid = account_id.clone();
                    account_sessions.batch(move |as_| {
                        as_.account_sessions.insert(aid, session.clone());
                    });
                }

                // Build OfflineServerRecord cache records before consuming servers.
                let backend_slug = token.backend.clone();
                let cache_records: Vec<OfflineServerRecord> = servers
                    .iter()
                    .map(|srv| OfflineServerRecord {
                        id: srv.id.clone(),
                        name: srv.name.clone(),
                        icon_url: srv.icon_url.clone(),
                        banner_url: srv.banner_url.clone(),
                        backend: backend_slug.clone(),
                        account_id: account_id.clone(),
                        account_display_name: srv.account_display_name.clone(),
                    })
                    .collect();
                let new_fav_ids: Vec<String> = servers.iter().map(|s| s.id.clone()).collect();

                chat_lists.batch(|cl| {
                    for srv in servers {
                        if !cl.servers.iter().any(|s| s.id == srv.id) {
                            cl.push_server(srv);
                        }
                    }
                });
                let all_fav_ids = account_sessions.batch(|as_| {
                    for id in &new_fav_ids {
                        if !as_.favorited_server_ids.contains(id) {
                            as_.favorited_server_ids.push(id.clone());
                        }
                    }
                    as_.favorited_server_ids.clone()
                });

                if let Err(e) = storage.upsert_offline_server_cache(&cache_records).await {
                    tracing::warn!("account_restore: failed to cache server metadata: {e}");
                }
                crate::ui::favorites_sidebar::persist_favorites(all_fav_ids).await;

                // Fetch DMs and friends in background.
                {
                    match backend_handle
                        .read_with_timeout(Duration::from_secs(30))
                        .await
                    {
                        Ok(guard) => {
                            let dms = guard.get_dm_channels().await.ok();
                            let friends = if let Some(sg) = guard.as_social_graph() {
                                sg.get_friends().await.ok()
                            } else {
                                None
                            };
                            let aid = account_id.clone();
                            chat_lists.batch(move |cl| {
                                if let Some(dms) = dms {
                                    cl.dm_channels.extend(dms);
                                }
                                if let Some(friends) = friends {
                                    for friend in friends {
                                        let already = cl
                                            .friends
                                            .get(&aid)
                                            .is_some_and(|v| v.iter().any(|f| f.id == friend.id));
                                        if !already {
                                            cl.friends
                                                .entry(aid.clone())
                                                .or_default()
                                                .push(friend);
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            tracing::warn!(
                                "account_restore: get_dm_channels timeout for {account_id}: {e}"
                            );
                        }
                    }
                }

                tracing::info!("account_restore: restored {} account: {account_id}", token.backend);
            }

            Err(e) => {
                // Auth failed — show as offline so the account is still visible.
                tracing::warn!(
                    "account_restore: {} account {} failed to authenticate: {e}. Showing as offline.",
                    token.backend,
                    token.account_id,
                );

                let backend_slug = token.backend.clone();
                // Same animal-avatar overlay as the success path so an
                // offline-restored test account still gets its portrait.
                #[cfg(feature = "demo")]
                let avatar_url = poly_demo::data::test_animal_avatar(&token.display_name);
                #[cfg(not(feature = "demo"))]
                let avatar_url: Option<String> = None;
                let offline_session = Session {
                    id: token.account_id.clone(),
                    user: User {
                        id: token.account_id.clone(),
                        display_name: token.display_name.clone(),
                        avatar_url,
                        presence: PresenceStatus::Offline,
                        backend: BackendType::from(backend_slug.as_str()),
                    },
                    token: token.token.clone(),
                    backend: BackendType::from(backend_slug.as_str()),
                    icon_emoji: None,
                    // Strip the URL scheme so `instance_id` is a bare
                    // "host:port" (e.g. "localhost:9103") not
                    // "http://localhost:9103". Route path segments cannot
                    // contain "://" — a scheme-inclusive instance_id causes
                    // the Dioxus router to parse every route as PageNotFound,
                    // triggering an unconditional app_state.write() loop on
                    // every navigation and freezing the WASM main thread
                    // (CLAUDE.md hang class #1 / PageNotFound redirect cascade).
                    instance_id: token.instance_id.as_deref().map_or_else(
                        || backend_slug.clone(),
                        |u| {
                            u.trim_start_matches("https://")
                                .trim_start_matches("http://")
                                .trim_end_matches('/')
                                .to_string()
                        },
                    ),
                    backend_url: token.instance_id.clone(),
                };

                {
                    let aid = token.account_id.clone();
                    let sess = offline_session.clone();
                    client_manager.batch(move |cm| cm.register_offline_session(aid, sess));
                }
                {
                    let aid = token.account_id.clone();
                    account_sessions.batch(move |as_| {
                        as_.account_sessions.insert(aid, offline_session.clone());
                    });
                }

                // Restore cached server metadata for offline rendering.
                let cached = storage.get_offline_server_cache().await.unwrap_or_default();
                let account_servers: Vec<poly_client::Server> = cached
                    .into_iter()
                    .filter(|r| r.account_id == token.account_id)
                    .map(|r| poly_client::Server {
                        id: r.id,
                        name: r.name,
                        icon_url: r.icon_url,
                        banner_url: r.banner_url,
                        categories: Vec::new(),
                        backend: BackendType::from(r.backend.as_str()),
                        unread_count: 0,
                        mention_count: 0,
                        account_id: r.account_id,
                        account_display_name: r.account_display_name,
                        default_channel_id: None,
                        description: None,
                        star_count: None,
                        language: None,
                        forks_count: None,
                        open_issues_count: None,
                    })
                    .collect();

                if !account_servers.is_empty() {
                    chat_lists.batch(move |cl| {
                        for srv in account_servers {
                            if !cl.servers.iter().any(|s| s.id == srv.id) {
                                cl.push_server(srv);
                            }
                        }
                    });
                }
            }
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(all(test, not(target_arch = "wasm32"), not(feature = "storage-surreal")))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    // `use dioxus::prelude::*` is required so that the `rsx! {}` macro can
    // resolve `dioxus_signals` / `dioxus_core` internals via the glob. We
    // qualify `Storage` as `crate::storage::Storage` everywhere to avoid the
    // ambiguity caused by the glob shadowing our storage type.
    use dioxus::prelude::*;
    use super::*;
    use crate::storage::AccountToken;

    async fn make_storage() -> crate::storage::Storage {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        // Intentionally leak the TempDir so the path stays valid for the test.
        std::mem::forget(dir);
        crate::storage::Storage::init_with_path(path).await.unwrap()
    }

    fn make_vdom() -> VirtualDom {
        fn empty() -> Element {
            rsx! {}
        }
        VirtualDom::new(empty)
    }

    fn make_signals_in_runtime(
        vdom: &VirtualDom,
    ) -> (BatchedSignal<ClientManager>, BatchedSignal<crate::state::ChatLists>, BatchedSignal<crate::state::AccountSessions>) {
        vdom.in_scope(ScopeId::ROOT, || {
            let cm = BatchedSignal::from_signal(Signal::new(ClientManager::default()));
            let cl = BatchedSignal::from_signal(Signal::new(crate::state::ChatLists::default()));
            let as_ = BatchedSignal::from_signal(Signal::new(crate::state::AccountSessions::default()));
            (cm, cl, as_)
        })
    }

    /// Factory returns None for unknown slugs — should not panic.
    #[test]
    fn unknown_slug_returns_none() {
        let result = build_backend_for_slug("some_future_backend", None);
        assert!(result.is_none());
    }

    /// Factory returns None for poly/demo slugs.
    #[test]
    fn poly_slug_returns_none() {
        assert!(build_backend_for_slug("poly", None).is_none());
        assert!(build_backend_for_slug("demo", None).is_none());
        assert!(build_backend_for_slug("demo_chat", None).is_none());
        assert!(build_backend_for_slug("demo_forum", None).is_none());
    }

    /// Helper: build a single-threaded tokio runtime, create a Dioxus VirtualDom
    /// (for Signal arena), then run the async test body with both active.
    ///
    /// `Signal::write()` (called by `BatchedSignal::batch`) needs a Dioxus
    /// runtime scope; Storage init and the restore function need a tokio runtime.
    /// The VirtualDom is created first (establishes the generational arena), then
    /// the async closure is run via `block_on`. The closure receives the vdom
    /// reference so it can open `in_scope` whenever it needs to call `batch`.
    fn run_test<F, Fut>(f: F)
    where
        F: FnOnce(VirtualDom) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // Create the VirtualDom (and therefore the Dioxus runtime) on the same
        // thread so `Signal` arenas are correctly scoped.
        let vdom = make_vdom();
        rt.block_on(f(vdom));
    }

    /// With no tokens in storage, restore is a no-op.
    #[test]
    fn restore_empty_storage_is_noop() {
        run_test(|vdom| async move {
            let storage = make_storage().await;
            let (cm, cl, as_) = make_signals_in_runtime(&vdom);
            vdom.in_scope(ScopeId::ROOT, || {
                let storage = storage.clone();
                // We're inside in_scope — batch is valid. But restore is async,
                // so we just prime state here and run restore outside.
                let _ = (storage, cm, cl, as_);
            });
            // restore_native_accounts calls batch internally; it runs outside
            // in_scope but on the same thread as the vdom, so the arena is valid.
            restore_native_accounts(&storage, cm, cl, as_, None).await;
        });
    }

    /// A token whose slug is in `disabled_native_backends` is skipped.
    #[test]
    fn disabled_backend_is_skipped() {
        run_test(|vdom| async move {
            let storage = make_storage().await;
            storage
                .upsert_account_token(&AccountToken {
                    backend: "discord".to_string(),
                    account_id: "discord-user-1".to_string(),
                    token: "fake-token".to_string(),
                    display_name: "Test User".to_string(),
                    instance_id: None,
                    refresh_token: None,
                    token_expires_at: None,
                    scope: None,
                })
                .await
                .unwrap();

            let (cm, cl, as_) = make_signals_in_runtime(&vdom);
            // Set disabled backends while in scope.
            vdom.in_scope(ScopeId::ROOT, || {
                cm.batch(|c| c.set_disabled_native_backends(vec!["discord".to_string()]));
            });

            restore_native_accounts(&storage, cm, cl, as_, None).await;

            let sessions_count = vdom.in_scope(ScopeId::ROOT, || {
                as_.peek().account_sessions.len()
            });
            assert_eq!(sessions_count, 0, "disabled backend should be skipped");
        });
    }

    /// A token for an account already in sessions is skipped (idempotent).
    #[test]
    fn already_restored_account_is_skipped() {
        run_test(|vdom| async move {
            let storage = make_storage().await;
            storage
                .upsert_account_token(&AccountToken {
                    backend: "discord".to_string(),
                    account_id: "discord-user-2".to_string(),
                    token: "fake-token".to_string(),
                    display_name: "Test User".to_string(),
                    instance_id: None,
                    refresh_token: None,
                    token_expires_at: None,
                    scope: None,
                })
                .await
                .unwrap();

            let (cm, cl, as_) = make_signals_in_runtime(&vdom);

            let dummy_session = Session {
                id: "discord-user-2".to_string(),
                user: User {
                    id: "discord-user-2".to_string(),
                    display_name: "Test User".to_string(),
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from("discord"),
                },
                token: "fake-token".to_string(),
                backend: BackendType::from("discord"),
                icon_emoji: None,
                instance_id: "discord.com".to_string(),
                backend_url: None,
            };

            vdom.in_scope(ScopeId::ROOT, || {
                let sess = dummy_session.clone();
                cm.batch(move |c| {
                    c.sessions.insert("discord-user-2".to_string(), sess);
                });
                let ds = dummy_session.clone();
                as_.batch(move |a| {
                    a.account_sessions.insert("discord-user-2".to_string(), ds);
                });
            });

            restore_native_accounts(&storage, cm, cl, as_, None).await;

            let count = vdom.in_scope(ScopeId::ROOT, || as_.peek().account_sessions.len());
            assert_eq!(count, 1, "already-restored account should not be duplicated");
        });
    }

    /// `slug_filter = Some("stoat")` skips tokens for other backends.
    #[test]
    fn slug_filter_skips_other_backends() {
        run_test(|vdom| async move {
            let storage = make_storage().await;
            storage
                .upsert_account_token(&AccountToken {
                    backend: "discord".to_string(),
                    account_id: "discord-user-3".to_string(),
                    token: "fake-token".to_string(),
                    display_name: "Test User".to_string(),
                    instance_id: None,
                    refresh_token: None,
                    token_expires_at: None,
                    scope: None,
                })
                .await
                .unwrap();

            let (cm, cl, as_) = make_signals_in_runtime(&vdom);

            restore_native_accounts(&storage, cm, cl, as_, Some("stoat")).await;

            let count = vdom.in_scope(ScopeId::ROOT, || as_.peek().account_sessions.len());
            assert_eq!(count, 0, "slug filter should skip non-matching backends");
        });
    }
}

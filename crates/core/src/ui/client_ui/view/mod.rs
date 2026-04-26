//! Plugin-declared non-chat view dispatcher.
//!
//! `ClientView` fetches the per-channel `ViewDescriptor` from the account's
//! `ClientBackend::get_channel_view` (D5) and dispatches to one of four body
//! engines:
//!
//! - [`ListBody`] — paged flat list (HN stories, issues).
//! - [`CardBody`] — grid of cards (Reddit / Mastodon).
//! - [`TreeBody`] — threaded list with depth indentation (Lemmy comments).
//! - [`SplitBody`] — master-detail (GitHub issue + body).
//!
//! If the backend returns `Err(NotSupported(_))` (or any other error) we
//! render a small fallback "no view declared" message. WP 5.C fills in the
//! real view descriptors for Lemmy / HN / GitHub / Forgejo in parallel.

pub mod card_body;
pub mod header;
pub mod list_body;
pub mod split_body;
pub mod toolbar;
pub mod tree_body;

pub use card_body::CardBody;
pub use header::ViewHeader;
pub use list_body::ListBody;
pub use split_body::SplitBody;
pub use toolbar::ViewToolbar;
pub use tree_body::TreeBody;

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{BatchedSignal, use_reactive_effect};
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_client::{ClientError, ViewBody, ViewDescriptor};
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the account-overview header search input.
#[derive(Debug, Clone)]
pub enum AccountOverviewAction {
    /// Search query was edited.
    SetSearchQuery(String),
}

impl UiAction for AccountOverviewAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // The search input writes a local Signal; this enum exists only to
        // satisfy the action-coverage lint.
    }
}

/// Host-rendered non-chat view. Reads the active backend's declared
/// `ViewDescriptor` for `channel_id` and routes to the matching body engine.
///
/// `initial_tab` — if provided, the toolbar's `selected_tab` signal is
/// pre-seeded with this value on mount. Used by `ForumView` to propagate
/// the sidebar scope (Subscribed / Local / All) into `get_view_rows`.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ClientView(
    channel_id: String,
    account_id: String,
    #[props(default)]
    initial_tab: Option<String>,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let desc_res = {
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
                        tracing::warn!("view: backend read timed out loading channel view");
                        return Err(ClientError::Internal("backend read timed out".into()));
                    }
                };
                guard.get_channel_view(&channel_id).await
            }
        })
    };

    match &*desc_res.read_unchecked() {
        None => rsx! {
            div { class: "client-view client-view-loading",
                span { "Loading view…" }
            }
        },
        Some(Err(err)) => {
            tracing::debug!("ClientView: get_channel_view failed: {err:?}");
            rsx! {
                div { class: "client-view client-view-error",
                    div { class: "view-error", "No view declared" }
                }
            }
        }
        Some(Ok(desc)) => {
            let desc: ViewDescriptor = desc.clone();
            render_descriptor(channel_id.clone(), account_id.clone(), desc, initial_tab)
        }
    }
}

/// Account-level overview view — same body-engine dispatch as `ClientView`
/// but reads the descriptor from `get_account_overview_view()` instead of
/// `get_channel_view(channel_id)`. Used by `ServerOverviewRoute` to render
/// each backend's plugin-supplied overview at
/// `/{backend}/{instance}/{account}/overview`.
#[ui_action(AccountOverviewAction)]
#[context_menu(inherit)]
#[component]
pub fn AccountOverviewView(account_id: String) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let mut search_query = use_signal(String::new);

    let desc_res = {
        let account_id = account_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            async move {
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Err(ClientError::NotFound(format!(
                        "no backend for account {account_id}"
                    )));
                };
                let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                    Ok(g) => g,
                    Err(_) => {
                        tracing::warn!("AccountOverviewView: backend read timed out");
                        return Err(ClientError::Internal("backend read timed out".into()));
                    }
                };
                guard.get_account_overview_view().await
            }
        })
    };

    // Extract the per-backend header strings from the plugin's descriptor
    // so the host-rendered title/subtitle/placeholder use backend-native
    // wording (Discord = "Your Servers", GitHub = "My Repositories",
    // Lemmy = "My Communities", Teams = "Teams Overview", etc.) instead
    // of a hardcoded "Your Servers". Falls back to a generic key when
    // the plugin doesn't supply one.
    let (header_title, header_subtitle) = match &*desc_res.read_unchecked() {
        Some(Ok(desc)) => {
            let title = desc
                .header
                .as_ref()
                .and_then(|h| h.title_key.clone())
                .map(|k| t(&k))
                .unwrap_or_else(|| t("overview-default-title"));
            let subtitle = desc
                .header
                .as_ref()
                .and_then(|h| h.subtitle_key.clone())
                .map(|k| t(&k))
                .unwrap_or_default();
            (title, subtitle)
        }
        _ => (t("overview-default-title"), String::new()),
    };

    let body = match &*desc_res.read_unchecked() {
        None => rsx! {
            div { class: "client-view client-view-loading",
                span { "Loading overview…" }
            }
        },
        Some(Err(err)) => {
            tracing::debug!("AccountOverviewView: get_account_overview_view failed: {err:?}");
            rsx! {
                div { class: "client-view client-view-error",
                    div { class: "view-error", "Overview unavailable" }
                }
            }
        }
        Some(Ok(desc)) => {
            // Reuse the same body-engine dispatcher; pass empty channel_id
            // since overview-rows callbacks don't carry a channel context.
            // Strip the plugin's header from the descriptor — the host
            // already renders title/subtitle above so we don't show a
            // duplicate.
            let mut desc: ViewDescriptor = desc.clone();
            desc.header = None;
            render_descriptor_with_filter(
                String::new(),
                account_id.clone(),
                desc,
                None,
                search_query.read().clone(),
            )
        }
    };

    let q = search_query.read().clone();
    let search_placeholder = t("overview-search-placeholder");
    rsx! {
        div { class: "overview-page overview-general-page",
            // Mirrors the People/Friends layout:
            //   row 1: plugin-supplied title + subtitle (backend-native
            //          wording: "Your Servers" / "My Repositories" / etc.)
            //   row 2: full-width search input.
            //   row 3: body (cards).
            header { class: "overview-page-header",
                h2 { "{header_title}" }
                if !header_subtitle.is_empty() {
                    p { class: "overview-page-subtitle", "{header_subtitle}" }
                }
            }
            div { class: "overview-page-search-row",
                input {
                    class: "overview-page-search-input overview-page-search-input-fullwidth",
                    r#type: "text",
                    placeholder: "{search_placeholder}",
                    value: "{q}",
                    oninput: move |e| search_query.set(e.value()),
                }
            }
            {body}
        }
    }
}

/// Wrapper used by `AccountOverviewView` to thread the host-side search
/// input down to the body engines (currently only `CardBody` honors it).
/// Other views call `render_descriptor` directly with no extra filter.
fn render_descriptor_with_filter(
    channel_id: String,
    account_id: String,
    desc: ViewDescriptor,
    initial_tab: Option<String>,
    extra_filter: String,
) -> Element {
    render_descriptor_inner(channel_id, account_id, desc, initial_tab, Some(extra_filter))
}

fn render_descriptor(
    channel_id: String,
    account_id: String,
    desc: ViewDescriptor,
    initial_tab: Option<String>,
) -> Element {
    render_descriptor_inner(channel_id, account_id, desc, initial_tab, None)
}

fn render_descriptor_inner(
    channel_id: String,
    account_id: String,
    desc: ViewDescriptor,
    initial_tab: Option<String>,
    extra_filter: Option<String>,
) -> Element {
    let header = desc.header.clone();
    let toolbar = desc.toolbar.clone();
    let body = desc.body.clone();
    // D30 — parent-owned filter + refresh signals; toolbar writes, bodies
    // read. A non-forum view that never shows the filter input still has
    // these signals sitting at their defaults (empty string / tick=0) and
    // the body engines short-circuit their filter pass.
    let filter = use_signal(String::new);
    let refresh_tick = use_signal(|| 0u32);
    // P4 — parent-owned toolbar selection signals. Toolbar writes on
    // click; body engines read and pass into `get_view_rows`.
    // `initial_tab` (from the forum sidebar scope buttons via ForumView)
    // pre-seeds the signal so the first `get_view_rows` uses the right scope.
    let selected_sort = use_signal(|| None::<String>);
    let selected_filter = use_signal(|| None::<String>);
    let selected_tab = use_signal(|| initial_tab.clone());
    // Dioxus' `key:` attribute does NOT remount a single component when its
    // key changes — `use_signal` therefore keeps the value from first mount,
    // ignoring later prop changes. This sync effect bridges that gap so a
    // sidebar scope click (which updates `initial_tab` via ForumView's key)
    // actually propagates into `selected_tab` and the body engine refetches.
    // Without this, demo-forum's Subscribed/Local/All buttons did nothing
    // even after the body_key was made tab-aware (witnessed 2026-04-25).
    {
        let initial_tab_for_sync = initial_tab.clone();
        use_reactive_effect(initial_tab_for_sync, move |new_tab| {
            // Signal<T>: Copy; clone the handle so the Fn closure can mutate
            // through a fresh local binding.
            let mut t = selected_tab;
            t.set(new_tab);
        });
    }
    let filter_str = filter.read().clone();
    rsx! {
        div { class: "client-view",
            if let Some(h) = header {
                ViewHeader { header: h }
            }
            if let Some(t) = toolbar {
                ViewToolbar {
                    toolbar: t,
                    filter,
                    refresh_tick,
                    selected_sort,
                    selected_filter,
                    selected_tab,
                }
            }
            div { class: "client-view-body",
                {
                    // Force a full remount of the body engine when channel_id,
                    // account_id, OR any of the toolbar selections (sort,
                    // filter, scope) change. use_resource inside the body
                    // captures these as plain Strings/Options, not Signals,
                    // so Dioxus can't track reactivity on them; without a
                    // key-based remount the resource keeps the stale values
                    // and the user's Local/All click does nothing.
                    let body_key = format!(
                        "{}:{}:{:?}:{:?}:{:?}",
                        channel_id,
                        account_id,
                        selected_sort.read(),
                        selected_filter.read(),
                        selected_tab.read(),
                    );
                    match body {
                        ViewBody::ListBody(spec) => rsx! {
                            ListBody {
                                key: "{body_key}",
                                channel_id: channel_id.clone(),
                                account_id: account_id.clone(),
                                spec,
                                filter: filter_str.clone(),
                                selected_sort,
                                selected_filter,
                                selected_tab,
                            }
                        },
                        ViewBody::CardBody(spec) => rsx! {
                            CardBody {
                                key: "{body_key}",
                                channel_id: channel_id.clone(),
                                account_id: account_id.clone(),
                                spec,
                                filter: extra_filter.clone().unwrap_or_default(),
                            }
                        },
                        ViewBody::TreeBody(spec) => rsx! {
                            TreeBody {
                                key: "{body_key}",
                                channel_id: channel_id.clone(),
                                account_id: account_id.clone(),
                                spec,
                                filter: filter_str.clone(),
                                selected_sort,
                                selected_filter,
                                selected_tab,
                            }
                        },
                        ViewBody::SplitBody(spec) => rsx! {
                            SplitBody {
                                key: "{body_key}",
                                channel_id: channel_id.clone(),
                                account_id: account_id.clone(),
                                spec,
                            }
                        },
                    }
                }
            }
        }
    }
}

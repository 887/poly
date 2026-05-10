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

/// Returns `true` when the browser reports a bandwidth-constrained
/// connection (`navigator.connection.effectiveType` ∈ {`slow-2g`, `2g`}).
///
/// Used by the forum-row preview rendering path to suppress thumbnail
/// downloads on slow connections — orthogonal to the per-backend
/// `render-previews` mechanism (which the user toggles manually).
///
/// On native (and on browsers that don't expose the Network Information
/// API — currently Firefox + Safari) this always returns `false`,
/// preserving the previous always-show behaviour.
#[must_use]
pub fn is_bandwidth_constrained() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        // navigator.connection is non-standard but supported in
        // Chromium-based browsers. Access entirely through js_sys to
        // avoid pulling the web-sys `Navigator` feature flag (would
        // bloat the WASM bundle for one read).
        use wasm_bindgen::JsValue;
        let win: JsValue = match web_sys::window() {
            Some(w) => w.into(),
            None => return false,
        };
        // window.navigator (via Reflect since web-sys' Navigator feature
        // isn't enabled workspace-wide).
        let nav = match js_sys::Reflect::get(&win, &JsValue::from_str("navigator")) {
            Ok(v) if !v.is_undefined() && !v.is_null() => v,
            _ => return false,
        };
        // navigator.connection
        let conn = match js_sys::Reflect::get(&nav, &JsValue::from_str("connection")) {
            Ok(v) if !v.is_undefined() && !v.is_null() => v,
            _ => return false,
        };
        // connection.effectiveType
        let effective = match js_sys::Reflect::get(&conn, &JsValue::from_str("effectiveType")) {
            Ok(v) => v.as_string().unwrap_or_default(),
            _ => return false,
        };
        matches!(effective.as_str(), "slow-2g" | "2g")
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

use crate::i18n::t;
use crate::state::{BatchedSignal, use_reactive_effect};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::use_view_resource::{use_view_resource, ViewQuery};
use dioxus::prelude::*;
use poly_client::{ClientResult, IsBackend, ViewBody, ViewDescriptor};

use poly_ui_macros::{context_menu, ui_action};

// ── ViewQuery impls for this module ──────────────────────────────────────────

/// Query: fetch the declared `ViewDescriptor` for a specific channel.
#[derive(Clone, PartialEq)]
struct ChannelViewQuery {
    account_id: String,
    channel_id: String,
}

impl ViewQuery for ChannelViewQuery {
    type Output = ViewDescriptor;
    fn account_id(&self) -> &str { &self.account_id }
    async fn fetch(&self, b: &dyn IsBackend) -> ClientResult<Self::Output> {
        b.get_channel_view(&self.channel_id).await
    }
}

/// Query: fetch the account-level overview `ViewDescriptor`.
#[derive(Clone, PartialEq)]
struct AccountOverviewViewQuery {
    account_id: String,
}

impl ViewQuery for AccountOverviewViewQuery {
    type Output = ViewDescriptor;
    fn account_id(&self) -> &str { &self.account_id }
    async fn fetch(&self, b: &dyn IsBackend) -> ClientResult<Self::Output> {
        b.get_account_overview_view().await
    }
}

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
    /// Phase D — external filter injected by ForumView (Posts|Comments toggle +
    /// debounced text filter). When `Some`, overrides the toolbar's own filter
    /// input so the parent can control filtering without touching the descriptor.
    #[props(default)]
    forum_filter: Option<String>,
    /// Optional leading slot for the body's toolbar — ForumView passes the
    /// Posts|Comments pill toggle here so it sits inline next to Hot /
    /// Filter… instead of stacking above the toolbar.
    #[props(default)]
    toolbar_leading: Option<Element>,
) -> Element {
    let desc_res = use_view_resource(ChannelViewQuery {
        account_id: account_id.clone(),
        channel_id: channel_id.clone(),
    });

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
            render_descriptor_inner(
                channel_id.clone(),
                account_id.clone(),
                desc,
                initial_tab,
                forum_filter.filter(|f| !f.is_empty()),
                toolbar_leading.clone(),
            )
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
    let mut search_query = use_signal(String::new);

    let desc_res = use_view_resource(AccountOverviewViewQuery {
        account_id: account_id.clone(),
    });

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
                .and_then(|h| h.title_key.clone()).map_or_else(|| t("overview-default-title"), |k| t(&k));
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
                search_query.read().clone(), // poly-lint: allow render-time-read — local Signal; subscription re-renders filtered overview on query change
            )
        }
    };

    let q = search_query.read().clone(); // poly-lint: allow render-time-read — local Signal; subscription re-renders search input on change
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
    render_descriptor_inner(channel_id, account_id, desc, initial_tab, Some(extra_filter), None)
}

fn render_descriptor(
    channel_id: String,
    account_id: String,
    desc: ViewDescriptor,
    initial_tab: Option<String>,
) -> Element {
    render_descriptor_inner(channel_id, account_id, desc, initial_tab, None, None)
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_descriptor_inner(
    channel_id: String,
    account_id: String,
    desc: ViewDescriptor,
    initial_tab: Option<String>,
    extra_filter: Option<String>,
    toolbar_leading: Option<Element>,
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
    let filter_str = filter.read().clone(); // poly-lint: allow render-time-read — local Signal; subscription re-renders on filter change
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
                    leading: toolbar_leading.clone(),
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
                    // Subscribe to the global sidebar-invalidated tick so
                    // sidebar-driven sort changes (SortModes click → backend
                    // settings_storage write → ActionOutcome::RefreshTarget)
                    // actually re-fire the body's use_resource. Without
                    // including the tick in body_key, the resource captures
                    // stale Strings and the click does nothing visible.
                    let app_state_for_tick: BatchedSignal<crate::state::AppState>
                        = use_context();
                    let sidebar_tick = app_state_for_tick.read().sidebar_invalidated_tick; // poly-lint: allow render-time-read — scoped snapshot for body_key composition; subscription ensures key updates on sidebar invalidation
                    let body_key = format!(
                        "{}:{}:{:?}:{:?}:{:?}:{}",
                        channel_id,
                        account_id,
                        selected_sort.read(), // poly-lint: allow render-time-read — local Signal; body_key composition triggers re-mount on sort change
                        selected_filter.read(), // poly-lint: allow render-time-read — local Signal; body_key composition triggers re-mount on filter change
                        selected_tab.read(), // poly-lint: allow render-time-read — local Signal; body_key composition triggers re-mount on tab change
                        sidebar_tick,
                    );
                    // Phase D: when an external filter is provided (ForumView
                    // Posts|Comments debounce), prefer it over the toolbar's
                    // filter signal for ListBody and TreeBody.
                    let effective_filter = extra_filter
                        .as_deref()
                        .filter(|f| !f.is_empty())
                        .map(|f| f.to_string())
                        .unwrap_or_else(|| filter_str.clone());
                    match body {
                        ViewBody::ListBody(spec) => rsx! {
                            ListBody {
                                key: "{body_key}",
                                channel_id: channel_id.clone(),
                                account_id: account_id.clone(),
                                spec,
                                filter: effective_filter.clone(),
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
                                filter: effective_filter.clone(),
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

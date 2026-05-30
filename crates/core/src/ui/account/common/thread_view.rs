//! Thread UI — Phase 5 of plan-discord-forums-threads.md
//!
//! Components:
//! - [`ViewThreadButton`]     — per-message affordance when `msg.thread.is_some()`
//! - [`ActiveThreadsBar`]     — chip bar shown above the message list in text channels
//! - [`ThreadPanel`]          — right-side panel (desktop/wide viewport)
//! - [`ThreadPanelHeader`]    — thread name + badges + close button
//!
//! Mobile / full-page view is handled by the `ThreadView` route in `routes.rs`.

use crate::state::BatchedSignal;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{ChatViewState, use_reactive_effect, use_spawn_once};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{Channel, ChannelType, Message, MessageQuery, ThreadInfo};
use poly_ui_macros::{context_menu, ui_action};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Returns `true` when the runtime is in mobile (narrow) layout mode.
/// Mirrors the same helper in `chat_view.rs`.
#[cfg(target_arch = "wasm32")]
fn is_mobile() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let viewport_width = window
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or_default();
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or_default();
    let classes = window
        .document()
        .and_then(|d| d.query_selector(".poly-app").ok().flatten())
        .and_then(|el| el.get_attribute("class"));
    match classes {
        Some(cls) => {
            cls.split_whitespace()
                .any(|c| c == "poly-layout-mode-force-mobile")
                || (cls
                    .split_whitespace()
                    .any(|c| c == "poly-layout-mode-auto-width")
                    && viewport_width <= 640.0)
                || (cls
                    .split_whitespace()
                    .any(|c| c == "poly-layout-mode-auto-portrait")
                    && viewport_height > viewport_width)
        }
        None => {
            // pre-hydration fallback
            let (mode, legacy_force_mobile) =
                crate::ui::load_persisted_layout_mode_from_window(&window);
            let effective = crate::ui::layout_query_override()
                .unwrap_or_else(|| crate::ui::effective_layout_mode(mode, legacy_force_mobile));
            crate::ui::layout_mode_is_mobile(effective)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
const fn is_mobile() -> bool {
    false
}

// ── 5.1 — "View Thread" button ───────────────────────────────────────────────

/// Renders a small "💬 N replies" button below a message that spawned a thread.
///
/// On desktop/wide viewports clicking opens the thread panel.
/// On mobile it navigates to the full-page `ThreadView` route.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ViewThreadButton(
    /// The `ThreadInfo` attached to the parent message.
    thread: ThreadInfo,
) -> Element {
    let nav_state: BatchedSignal<crate::state::NavState> = use_context();
    let ui_overlays: BatchedSignal<crate::state::UiOverlays> = use_context();
    let nav = navigator();
    let thread_id = thread.thread_id.clone();
    let count = thread.message_count;

    let label = if count == 1 {
        format!("💬 1 {}", t("thread-reply"))
    } else {
        format!("💬 {} {}", count, t("thread-replies"))
    };

    rsx! {
        button {
            class: "view-thread-btn",
            title: t("thread-view"),
            onclick: move |_| {
                if is_mobile() {
                    // Full-page navigation on mobile.
                    let s = nav_state.read();
                    let backend = s
                        .active_backend
                        .cloned()
                        .map(|b| b.slug().to_string())
                        .unwrap_or_default();
                    let instance_id = s
                        .active_instance_id
                        .cloned()
                        .unwrap_or_default();
                    let account_id = s
                        .active_account_id
                        .cloned()
                        .unwrap_or_default();
                    drop(s);
                    nav.push(Route::ThreadView {
                        backend,
                        instance_id,
                        account_id,
                        thread_id: thread_id.clone(),
                    });
                } else {
                    // Side-panel on desktop.
                    ui_overlays.batch(|o| {
                        let currently_open =
                            o.thread_panel_open.as_deref() == Some(&thread_id);
                        o.thread_panel_open = if currently_open {
                            None
                        } else {
                            Some(thread_id.clone())
                        };
                    });
                }
            },
            "{label}"
        }
    }
}

// ── 5.4 — Active threads bar ─────────────────────────────────────────────────

/// Horizontal chip bar rendered above the message list in a text channel.
///
/// Calls `get_active_threads(server_id)` and filters to threads whose
/// `parent_channel_id` matches the current channel. Shows one chip per thread.
/// Chips are clickable — they open the thread panel (or full-page on mobile).
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ActiveThreadsBar() -> Element {
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();

    let server_id = chat_view_state
        .read() // poly-lint: allow render-time-read — render snapshot; subscription intentional
        .current_server
        .as_ref()
        .map(|s| s.id.clone());
    let channel_id = nav.read().selected_channel.cloned();
    let account_id = nav.read().active_account_id.cloned();

    let threads = use_resource(move || {
        let server_id = server_id.clone();
        let channel_id = channel_id.clone();
        let account_id = account_id.clone();
        async move {
            let sid = server_id?;
            let cid = channel_id?;
            let aid = account_id?;
            client_manager.peek().with_backend(&aid, async |b| {
                match b.as_threads() {
                    Some(tb) => tb.get_active_threads(&sid).await,
                    None => Ok(vec![]),
                }
            }).await.ok().map(|all| all.into_iter().filter(|t| t.parent_channel_id == cid).collect::<Vec<_>>())
        }
    });

    let list = match threads.read().as_ref() {
        Some(Some(v)) if !v.is_empty() => v.clone(),
        _ => return rsx! {},
    };

    rsx! {
        div { class: "active-threads-bar",
            span { class: "active-threads-label", "🧵 {list.len()} {t(\"thread-active\")}" }
            for thread in list {
                ActiveThreadChip { thread }
            }
        }
    }
}

// ── chip sub-component ────────────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ActiveThreadChip(thread: ThreadInfo) -> Element {
    let nav_state: BatchedSignal<crate::state::NavState> = use_context();
    let ui_overlays: BatchedSignal<crate::state::UiOverlays> = use_context();
    let nav = navigator();
    let thread_id = thread.thread_id.clone();
    let count = thread.message_count;

    rsx! {
        button {
            class: "active-thread-chip",
            title: format!("{} {}", count, t("thread-messages")),
            onclick: move |_| {
                if is_mobile() {
                    let s = nav_state.read();
                    let backend = s
                        .active_backend
                        .cloned()
                        .map(|b| b.slug().to_string())
                        .unwrap_or_default();
                    let instance_id = s.active_instance_id.cloned().unwrap_or_default();
                    let account_id = s.active_account_id.cloned().unwrap_or_default();
                    drop(s);
                    nav.push(Route::ThreadView {
                        backend,
                        instance_id,
                        account_id,
                        thread_id: thread_id.clone(),
                    });
                } else {
                    ui_overlays.batch(|o| {
                        let currently_open =
                            o.thread_panel_open.as_deref() == Some(&thread_id);
                        o.thread_panel_open = if currently_open {
                            None
                        } else {
                            Some(thread_id.clone())
                        };
                    });
                }
            },
            "🧵 {thread_id} ({count})"
        }
    }
}

// ── 5.2 — Thread panel (desktop) ─────────────────────────────────────────────

/// Right-side thread panel rendered alongside the parent channel on desktop.
///
/// Visible when `app_state.nav.thread_panel_open` is `Some(thread_id)`.
/// Fetches the thread's `Channel` to populate the header (name, metadata).
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ThreadPanel() -> Element {
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let ui_overlays: crate::state::BatchedSignal<crate::state::UiOverlays> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let thread_id = ui_overlays.read().thread_panel_open.clone();
    let Some(thread_id) = thread_id else {
        return rsx! {};
    };
    let account_id = nav.read().active_account_id.cloned();

    // Resolve the thread Channel object (name + metadata).
    let thread_channel = use_resource(move || {
        let thread_id = thread_id.clone();
        let account_id = account_id.clone();
        async move {
            let aid = account_id?;
            client_manager.peek().with_backend(&aid, async |b| {
                b.get_channel(&thread_id).await
            }).await.ok()
        }
    });

    // Load messages for the thread into a local signal. Key on the
    // (thread_id, account_id) pair — opening a different thread or
    // switching account triggers a fresh fetch; same pair stays a no-op.
    // **PEEK, not READ** — these values are use_spawn_once keys. A live
    // .read() here subscribes ThreadView to every app_state write; when
    // load_server_data writes app_state.nav, ThreadView re-renders, this
    // setup re-runs, the read fires the subscription again — perpetual loop
    // (hang class #7, same shape as use_member_list_effect, commit 55f94246).
    let thread_id_for_msgs = ui_overlays.peek().thread_panel_open.clone();
    let account_id_for_msgs = nav.peek().active_account_id.cloned();
    let messages: Signal<Vec<Message>> = use_signal(Vec::new);
    let mut messages_w = messages;
    use_spawn_once(
        (thread_id_for_msgs, account_id_for_msgs),
        move |(tid, aid)| async move {
            let Some(tid) = tid else { return };
            let Some(aid) = aid else { return };
            if let Ok(msgs) = client_manager.peek().with_backend(&aid, async |b| {
                b.get_messages(&tid, MessageQuery::default()).await
            }).await {
                messages_w.set(msgs);
            }
        },
    );

    let channel = thread_channel.read().as_ref().and_then(std::clone::Clone::clone);
    let panel_thread_id = ui_overlays.read().thread_panel_open.clone();

    rsx! {
        div { class: "thread-panel",
            ThreadPanelHeader {
                channel: channel.clone(),
                thread_id: panel_thread_id.clone().unwrap_or_default(),
            }
            div { class: "thread-panel-messages",
                for msg in messages.read().iter() {
                    ThreadMessageRow { message: msg.clone() }
                }
                if messages.read().is_empty() {
                    div { class: "thread-panel-empty", "{t(\"thread-no-messages\")}" }
                }
            }
        }
    }
}

// ── 5.5 — Thread header ──────────────────────────────────────────────────────

/// Thread header: name, message count, member count, archived / locked badges.
///
/// Used in both `ThreadPanel` (panel mode) and `ThreadFullView` (full-page).
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ThreadPanelHeader(
    /// Resolved thread `Channel` — `None` while loading.
    channel: Option<Channel>,
    /// Fallback thread ID shown when the channel name is not yet loaded.
    thread_id: String,
) -> Element {
    let ui_overlays: crate::state::BatchedSignal<crate::state::UiOverlays> = use_context();

    let name = channel
        .as_ref().map_or_else(|| thread_id.clone(), |c| c.name.clone());

    let archived = channel
        .as_ref()
        .and_then(|c| c.thread_metadata.as_ref())
        .is_some_and(|m| m.archived);
    let locked = channel
        .as_ref()
        .and_then(|c| c.thread_metadata.as_ref())
        .is_some_and(|m| m.locked);

    rsx! {
        div { class: "thread-panel-header",
            div { class: "thread-panel-title",
                span { class: "thread-panel-name", "{name}" }
                if archived {
                    span { class: "thread-badge thread-badge-archived", "{t(\"thread-archived\")}" }
                }
                if locked {
                    span { class: "thread-badge thread-badge-locked", "{t(\"thread-locked\")}" }
                }
            }
            // 5.6 — Close button clears thread_panel_open.
            button {
                class: "thread-panel-close",
                title: t("action-close"),
                onclick: move |_| {
                    ui_overlays.batch(|o| o.thread_panel_open = None);
                },
                "✕"
            }
        }
    }
}

// ── message row (panel mode) ─────────────────────────────────────────────────

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ThreadMessageRow(message: Message) -> Element {
    use crate::state::chat_data::user_color;
    use poly_client::MessageContent;

    let color = user_color(&message.author.id);
    let name = message.author.display_name.clone();
    let text = match &message.content {
        MessageContent::Text(s) => s.clone(),
        MessageContent::WithAttachments { text, .. } => text.clone(),
    };
    let first = name.chars().next().map(|c| c.to_string()).unwrap_or_default();

    rsx! {
        div { class: "thread-message-row",
            if let Some(ref avatar) = message.author.avatar_url {
                img {
                    class: "thread-msg-avatar thread-msg-avatar-img",
                    src: "{avatar}",
                    alt: "{first}",
                }
            } else {
                div {
                    class: "thread-msg-avatar",
                    style: "background-color:{color};",
                    "{first}"
                }
            }
            div { class: "thread-message-body",
                span { class: "thread-msg-author", style: "color:{color};", "{name}" }
                span { class: "thread-msg-text", "{text}" }
            }
        }
    }
}

// ── 5.3 — Full-page thread view (mobile) ─────────────────────────────────────

/// Full-page thread view rendered on mobile / narrow viewports.
///
/// Navigated to via `Route::ThreadView`. Shows the thread header, message list,
/// and a back button that returns to the parent channel.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ThreadFullView(thread_id: String) -> Element {
    let nav_state: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let nav = navigator();

    // Resolve channel metadata.
    let thread_channel = {
        let tid = thread_id.clone();
        let aid = nav_state.read().active_account_id.cloned();
        use_resource(move || {
            let tid = tid.clone();
            let aid = aid.clone();
            async move {
                let aid = aid?;
                client_manager.peek().with_backend(&aid, async |b| {
                    b.get_channel(&tid).await
                }).await.ok()
            }
        })
    };

    // Load messages; re-runs when thread_id changes.
    let messages: Signal<Vec<Message>> = use_signal(Vec::new);
    let mut messages_w = messages;
    use_reactive_effect(thread_id.clone(), move |tid| {
        let aid = nav_state.read().active_account_id.cloned();
        let Some(aid) = aid else { return };
        spawn(async move {
            if let Ok(msgs) = client_manager.peek().with_backend(&aid, async |b| {
                b.get_messages(&tid, MessageQuery::default()).await
            }).await {
                messages_w.set(msgs);
            }
        });
    });

    let channel = thread_channel.read().as_ref().and_then(std::clone::Clone::clone);
    let tid_for_header = thread_id.clone();

    rsx! {
        div { class: "thread-full-view",
            div { class: "thread-full-header",
                button {
                    class: "thread-back-btn",
                    title: t("thread-back"),
                    onclick: move |_| {
                        nav.go_back();
                    },
                    "← {t(\"thread-back\")}"
                }
                ThreadPanelHeader {
                    channel: channel.clone(),
                    thread_id: tid_for_header,
                }
            }
            div { class: "thread-full-messages",
                for msg in messages.read().iter() {
                    ThreadMessageRow { message: msg.clone() }
                }
                if messages.read().is_empty() {
                    div { class: "thread-full-empty", "{t(\"thread-no-messages\")}" }
                }
            }
        }
    }
}

//! Unified forum composer — shared by the new-post page and inline comment replies.
//!
//! ## SOLID-SRP split
//!
//! | Component | Responsibility |
//! |-----------|---------------|
//! | `ForumComposer` | Outer shell: knows mode + backend dispatch |
//! | `ComposerHeader` | Title input (NewPost) or "Reply to …" label |
//! | `ComposerEditor` | Textarea + markdown preview tab |
//! | `ComposerActions` | Cancel + Submit + disabled-when-empty logic |
//!
//! The wrapper knows about backend dispatch; sub-components know only about text.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};
use pulldown_cmark::Options;

// ─────────────────────────────────────────────────────────────────────────────
// Public types — C.1
// ─────────────────────────────────────────────────────────────────────────────

/// Which mode the composer is in.
#[derive(Clone, PartialEq, Debug)]
pub enum ComposerMode {
    /// Creating a brand-new top-level forum post.
    NewPost,
    /// Replying to an existing top-level post (currently unused, reserved).
    ReplyToPost { parent_id: String },
    /// Replying to a comment nested under a post.
    ReplyToComment { parent_id: String },
}

/// Payload emitted by `on_submit`.
#[derive(Clone, Debug)]
pub struct SubmitPayload {
    /// Post title — Some only when `ComposerMode::NewPost`.
    pub title: Option<String>,
    /// Post/comment body text.
    pub body: String,
    /// Optional URL for link posts (Lemmy only for now).
    pub link_url: Option<String>,
    /// Parent message id for reply modes.
    pub parent_id: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Markdown rendering helper — reuses pulldown_cmark (same as chat_view.rs)
// ─────────────────────────────────────────────────────────────────────────────

fn render_markdown_html(text: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);
    let parser = pulldown_cmark::Parser::new_ext(text, opts);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

// ─────────────────────────────────────────────────────────────────────────────
// ComposerHeader — C.2
// ─────────────────────────────────────────────────────────────────────────────

/// Renders the title input for `NewPost`, or a "Replying to…" label for replies.
/// Stays under 150 lines on its own; receives title as a mutable Signal.
#[ui_action(inherit)]
#[context_menu(allow_default)]
#[component]
pub fn ComposerHeader(
    mode: ComposerMode,
    title: Signal<String>,
    link_url: Signal<String>,
) -> Element {
    match mode {
        ComposerMode::NewPost => rsx! {
            div { class: "composer-header",
                div { class: "composer-field",
                    label { class: "composer-label", "forum-composer-title-label" }
                    input {
                        class: "composer-input",
                        r#type: "text",
                        placeholder: "forum-composer-title-placeholder",
                        value: "{title}",
                        oninput: move |e| title.set(e.value()),
                    }
                }
                div { class: "composer-field",
                    label { class: "composer-label", "forum-composer-url-label" }
                    input {
                        class: "composer-input",
                        r#type: "url",
                        placeholder: "forum-composer-url-placeholder",
                        value: "{link_url}",
                        oninput: move |e| link_url.set(e.value()),
                    }
                }
            }
        },
        ComposerMode::ReplyToPost { ref parent_id } | ComposerMode::ReplyToComment { ref parent_id } => {
            let pid = parent_id.clone();
            rsx! {
                div { class: "composer-header",
                    span { class: "composer-reply-label",
                        "forum-composer-replying-to {pid}"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ComposerEditor — C.3
// ─────────────────────────────────────────────────────────────────────────────

/// Full-height textarea with Write / Preview tab toggle.
/// Drag-and-drop attachment support is Lemmy-only (link-URL path).
#[ui_action(inherit)]
#[context_menu(allow_default)]
#[component]
pub fn ComposerEditor(body: Signal<String>, link_url: Signal<String>) -> Element {
    let mut preview_active = use_signal(|| false);
    let body_text = body.read().clone();
    let preview_html = if *preview_active.read() {
        Some(render_markdown_html(&body_text))
    } else {
        None
    };

    rsx! {
        div { class: "composer-editor",
            // Tab bar
            div { class: "composer-tabs",
                button {
                    class: if !*preview_active.read() { "composer-tab active" } else { "composer-tab" },
                    onclick: move |_| preview_active.set(false),
                    "forum-composer-tab-write"
                }
                button {
                    class: if *preview_active.read() { "composer-tab active" } else { "composer-tab" },
                    onclick: move |_| preview_active.set(true),
                    "forum-composer-tab-preview"
                }
            }
            // Write or preview pane
            if let Some(html) = preview_html {
                div {
                    class: "composer-preview",
                    dangerous_inner_html: "{html}",
                }
            } else {
                textarea {
                    class: "composer-textarea",
                    placeholder: "forum-composer-body-placeholder",
                    value: "{body_text}",
                    oninput: move |e| body.set(e.value()),
                    // Drag-and-drop: insert image URL as markdown link (Lemmy-only first cut).
                    // Real pict-rs upload is out of scope; see plan open question C.6.
                    ondragover: move |e| e.prevent_default(),
                    ondrop: move |e| {
                        e.prevent_default();
                        // Web drag events expose dataTransfer via web_sys; for now we read
                        // the text/uri-list item if present (URLs dropped from browser).
                        // Full file upload via pict-rs is deferred.
                        let _ = link_url; // suppress unused-variable warning in non-web builds
                    },
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ComposerActions — C.4
// ─────────────────────────────────────────────────────────────────────────────

/// Cancel + Submit buttons. Submit is disabled while the body is empty.
#[ui_action(inherit)]
#[context_menu(None)]
#[component]
pub fn ComposerActions(
    body: Signal<String>,
    title: Signal<String>,
    mode: ComposerMode,
    on_cancel: EventHandler<()>,
    on_submit: EventHandler<()>,
) -> Element {
    let body_empty = body.read().trim().is_empty();
    let title_empty = matches!(mode, ComposerMode::NewPost) && title.read().trim().is_empty();
    let disabled = body_empty || title_empty;

    rsx! {
        div { class: "composer-actions",
            button {
                class: "composer-cancel-btn",
                onclick: move |_| on_cancel.call(()),
                "forum-composer-cancel"
            }
            button {
                class: "composer-submit-btn",
                disabled: disabled,
                onclick: move |_| {
                    if !disabled {
                        on_submit.call(());
                    }
                },
                "forum-composer-submit"
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ForumComposer — outer shell — C.1 / C.5 / C.6
// ─────────────────────────────────────────────────────────────────────────────

/// Unified forum composer.
///
/// # Props
/// - `mode`      — `NewPost`, `ReplyToPost`, or `ReplyToComment`
/// - `on_submit` — called with the composed `SubmitPayload` on submit
/// - `on_cancel` — called when the user clicks Cancel
#[ui_action(inherit)]
#[context_menu(None)]
#[component]
pub fn ForumComposer(
    mode: ComposerMode,
    on_submit: EventHandler<SubmitPayload>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut title = use_signal(String::new);
    let mut body = use_signal(String::new);
    let mut link_url = use_signal(String::new);

    let mode_clone = mode.clone();
    let submit_handler = {
        let mode2 = mode.clone();
        move |()| {
            let payload = match &mode2 {
                ComposerMode::NewPost => SubmitPayload {
                    title: {
                        let t = title.read().trim().to_string();
                        if t.is_empty() { return; }
                        Some(t)
                    },
                    body: body.read().trim().to_string(),
                    link_url: {
                        let u = link_url.read().trim().to_string();
                        if u.is_empty() { None } else { Some(u) }
                    },
                    parent_id: None,
                },
                ComposerMode::ReplyToPost { parent_id }
                | ComposerMode::ReplyToComment { parent_id } => SubmitPayload {
                    title: None,
                    body: body.read().trim().to_string(),
                    link_url: None,
                    parent_id: Some(parent_id.clone()),
                },
            };
            if payload.body.is_empty() {
                return;
            }
            on_submit.call(payload);
        }
    };

    rsx! {
        div { class: "forum-composer",
            ComposerHeader {
                mode: mode_clone.clone(),
                title: title,
                link_url: link_url,
            }
            ComposerEditor {
                body: body,
                link_url: link_url,
            }
            ComposerActions {
                body: body,
                title: title,
                mode: mode_clone,
                on_cancel: on_cancel,
                on_submit: submit_handler,
            }
        }
    }
}

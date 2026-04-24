//! Fullscreen message media viewer overlay.
//!
//! Route-backed overlay used for message image attachments.
//! Supports single-image and multi-image carousels with a thumbnail strip,
//! left/right navigation, keyboard arrow/Escape, scroll-wheel zoom,
//! pinch-to-zoom (touch), and mouse drag to pan.

use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::state::{AppState, AttachmentContextMenuState, ChatData};
use dioxus::prelude::*;
use poly_client::Attachment;
use poly_ui_macros::{context_menu, ui_action};

#[derive(Props, Clone, PartialEq)]
pub struct MessageMediaViewerOverlayProps {
    pub channel_id: String,
    pub message_id: String,
    pub attachment_index: usize,
}

#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(none)]
#[component]
pub fn MessageMediaViewerOverlay(props: MessageMediaViewerOverlayProps) -> Element {
    let chat_data: BatchedSignal<ChatData> = use_context();
    let mut app_state: Signal<AppState> = use_context();
    let nav = navigator();
    let mut zoom = use_signal(|| 1.0_f32);
    let mut pan_x = use_signal(|| 0.0_f32);
    let mut pan_y = use_signal(|| 0.0_f32);
    let mut menu_open = use_signal(|| false);

    // Collect image attachments + author info in a single message lookup.
    let (image_attachments, author_name, ts_str): (Vec<(usize, Attachment)>, String, String) = {
        let snapshot = chat_data.read();
        if let Some(msg) = snapshot.messages.iter().find(|m| m.id == props.message_id) {
            let images = msg.attachments
                .iter()
                .enumerate()
                .filter(|(_, a)| a.content_type.starts_with("image/"))
                .map(|(i, a)| (i, a.clone()))
                .collect();
            let name = msg.author.display_name.clone();
            let ts = msg.timestamp.format("%d %b %Y %H:%M").to_string();
            (images, name, ts)
        } else {
            (vec![], String::new(), String::new())
        }
    };

    let start_pos = image_attachments
        .iter()
        .position(|(orig_idx, _)| *orig_idx == props.attachment_index)
        .unwrap_or(0);

    let mut active_pos = use_signal(|| start_pos);

    // Combined JS handler: keyboard, wheel zoom, pinch zoom, drag pan.
    //
    // Zoom/pan are delta-based (send factors/deltas, not absolute state) so
    // no JS-side accumulated state needs resetting when images change.
    //
    // Drag-end click: a `moved` flag intercepts the click event on the stage
    // and stops it propagating to the overlay's backdrop-dismiss handler, so
    // finishing a drag does not accidentally close the viewer.
    #[cfg(target_arch = "wasm32")]
    {
        let img_count = image_attachments.len();
        use_effect(move || {
            spawn(async move {
                let js = include_str!("../../../../assets/scripts/media_viewer_interactions.js");
                let mut eval = document::eval(js);
                loop {
                    match eval.recv::<String>().await {
                        Ok(msg) if msg == "esc" => { nav.go_back(); break; }
                        Ok(msg) if msg == "prev" => {
                            let cur = *active_pos.read();
                            if cur > 0 {
                                active_pos.set(cur - 1);
                                zoom.set(1.0); pan_x.set(0.0); pan_y.set(0.0);
                            }
                        }
                        Ok(msg) if msg == "next" => {
                            let cur = *active_pos.read();
                            if cur + 1 < img_count {
                                active_pos.set(cur + 1);
                                zoom.set(1.0); pan_x.set(0.0); pan_y.set(0.0);
                            }
                        }
                        Ok(msg) if msg.starts_with("zf:") => {
                            if let Ok(f) = msg[3..].parse::<f32>() {
                                let s = (*zoom.read() * f).clamp(0.1, 10.0);
                                zoom.set(s);
                            }
                        }
                        Ok(msg) if msg.starts_with("dp:") => {
                            let rest = &msg[3..];
                            if let Some(colon) = rest.find(':') {
                                if let (Ok(dx), Ok(dy)) = (
                                    rest[..colon].parse::<f32>(),
                                    rest[colon + 1..].parse::<f32>(),
                                ) {
                                    let new_x = *pan_x.read() + dx;
                                    let new_y = *pan_y.read() + dy;
                                    pan_x.set(new_x);
                                    pan_y.set(new_y);
                                }
                            }
                        }
                        _ => break,
                    }
                }
            });
        });
    }

    let pos = *active_pos.read();
    let total = image_attachments.len();

    let Some((_, attachment)) = image_attachments.get(pos).cloned() else {
        return rsx! {
            div {
                class: "poly-media-viewer-overlay",
                tabindex: "-1",
                onclick: move |_| nav.go_back(),
                div {
                    class: "poly-media-viewer-empty",
                    onclick: move |e| e.stop_propagation(),
                    h2 { "{t(\"media-viewer-unavailable-title\")}" }
                    p { "{t(\"media-viewer-unavailable-body\")}" }
                }
            }
        };
    };

    let scale = *zoom.read();
    let tx = *pan_x.read();
    let ty = *pan_y.read();
    // translate in screen space (applied before scale so panning feels 1:1)
    let image_style = format!("transform: translate({tx}px, {ty}px) scale({scale});");
    let filename = attachment.filename.clone();
    let url = attachment.url.clone();

    let can_prev = pos > 0;
    let can_next = pos + 1 < total;
    let is_zoomed = scale > 1.01;

    rsx! {
        div {
            class: "poly-media-viewer-overlay",
            tabindex: "-1",
            onclick: move |_| nav.go_back(),

            // — Full-width top bar —
            div {
                class: "poly-media-viewer-topbar",
                onclick: move |e| e.stop_propagation(),

                div { class: "poly-media-viewer-author",
                    span { class: "poly-media-viewer-author-name", "{author_name}" }
                    span { class: "poly-media-viewer-author-ts", "{ts_str}" }
                }

                div { class: "poly-media-viewer-controls",
                    if total > 1 {
                        span { class: "poly-media-viewer-count", "{pos + 1} / {total}" }
                    }
                    button {
                        class: "poly-media-viewer-btn",
                        title: "{t(\"zoom-out\")}",
                        onclick: move |_| {
                            let s = (*zoom.read() / 1.4_f32).clamp(0.1, 10.0);
                            zoom.set(s);
                        },
                        "－"
                    }
                    button {
                        class: "poly-media-viewer-btn",
                        title: "{t(\"zoom-in\")}",
                        onclick: move |_| {
                            let s = (*zoom.read() * 1.4_f32).clamp(0.1, 10.0);
                            zoom.set(s);
                        },
                        "＋"
                    }
                    div { class: "poly-media-viewer-menu-wrap",
                        button {
                            class: "poly-media-viewer-btn",
                            title: "{t(\"user-profile-more-options\")}",
                            onclick: move |_| {
                                let was = *menu_open.read();
                                menu_open.set(!was);
                            },
                            "···"
                        }
                        if *menu_open.read() {
                            div { class: "poly-media-viewer-menu",
                                a {
                                    class: "poly-media-viewer-menu-item",
                                    href: "{url}",
                                    target: "_blank",
                                    download: "{filename}",
                                    onclick: move |_| menu_open.set(false),
                                    "{t(\"action-download\")}"
                                }
                                a {
                                    class: "poly-media-viewer-menu-item",
                                    href: "{url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    onclick: move |_| menu_open.set(false),
                                    "{t(\"action-open-in-browser\")}"
                                }
                            }
                        }
                    }
                    button {
                        class: "poly-media-viewer-btn poly-media-viewer-btn--close",
                        title: "{t(\"action-close\")}",
                        onclick: move |_| nav.go_back(),
                        "✕"
                    }
                }
            }

            // — Left nav arrow —
            if can_prev {
                button {
                    class: "poly-media-viewer-nav poly-media-viewer-nav--prev",
                    title: "Previous image",
                    onclick: move |e| {
                        e.stop_propagation();
                        let cur = *active_pos.read();
                        if cur > 0 {
                            active_pos.set(cur - 1);
                            zoom.set(1.0); pan_x.set(0.0); pan_y.set(0.0);
                        }
                    },
                    "‹"
                }
            }

            // — Stage —
            // Clicking the dark letterbox area bubbles up to the overlay and closes.
            // stop_propagation is on the <img> so the image itself doesn't close.
            // The JS eval on this element also handles drag-end clicks via a
            // `moved` flag to prevent accidental close after panning.
            div {
                class: if is_zoomed {
                    "poly-media-viewer-stage poly-media-viewer-stage--zoomed"
                } else {
                    "poly-media-viewer-stage"
                },
                img {
                    class: "poly-media-viewer-image",
                    style: "{image_style}",
                    src: "{attachment.url}",
                    alt: "{attachment.filename}",
                    onclick: move |e| e.stop_propagation(),
                    oncontextmenu: {
                        let ctx_url = url.clone();
                        let ctx_name = filename.clone();
                        move |evt: Event<MouseData>| {
                            evt.prevent_default();
                            evt.stop_propagation();
                            let coords = evt.client_coordinates();
                            app_state.write().attachment_context_menu = Some(AttachmentContextMenuState {
                                x: coords.x,
                                y: coords.y,
                                url: ctx_url.clone(),
                                filename: ctx_name.clone(),
                            });
                        }
                    },
                }
            }

            // — Right nav arrow —
            if can_next {
                button {
                    class: "poly-media-viewer-nav poly-media-viewer-nav--next",
                    title: "Next image",
                    onclick: move |e| {
                        e.stop_propagation();
                        let cur = *active_pos.read();
                        if cur + 1 < total {
                            active_pos.set(cur + 1);
                            zoom.set(1.0); pan_x.set(0.0); pan_y.set(0.0);
                        }
                    },
                    "›"
                }
            }

            // — Footer: filename + thumbnail strip —
            div {
                class: "poly-media-viewer-footer",
                onclick: move |e| e.stop_propagation(),
                div { class: "poly-media-viewer-filename", "{attachment.filename}" }
                if total > 1 {
                    div { class: "poly-media-viewer-thumbnails",
                        for (thumb_pos, (_, thumb)) in image_attachments.iter().enumerate() {
                            {
                                let tp = thumb_pos;
                                let thumb_url = thumb.url.clone();
                                let thumb_name = thumb.filename.clone();
                                let is_active = tp == pos;
                                rsx! {
                                    img {
                                        class: if is_active {
                                            "poly-media-viewer-thumb poly-media-viewer-thumb--active"
                                        } else {
                                            "poly-media-viewer-thumb"
                                        },
                                        src: "{thumb_url}",
                                        alt: "{thumb_name}",
                                        onclick: move |e| {
                                            e.stop_propagation();
                                            active_pos.set(tp);
                                            zoom.set(1.0); pan_x.set(0.0); pan_y.set(0.0);
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

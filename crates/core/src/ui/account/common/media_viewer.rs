//! Fullscreen message media viewer overlay.
//!
//! Route-backed overlay used for message image attachments.
//! Supports single-image and multi-image carousels with a thumbnail strip,
//! left/right navigation, and keyboard arrow/Escape handling.
//!
//! Layout: Discord-style full-width topbar (author + time left, controls right,
//! ✕ at far right near window controls). Clicking the dark background around
//! the image dismisses; clicking the image itself does not.

use crate::i18n::t;
use crate::state::ChatData;
use dioxus::prelude::*;
use poly_client::Attachment;

#[derive(Props, Clone, PartialEq)]
pub struct MessageMediaViewerOverlayProps {
    pub channel_id: String,
    pub message_id: String,
    pub attachment_index: usize,
}

#[rustfmt::skip]
#[component]
pub fn MessageMediaViewerOverlay(props: MessageMediaViewerOverlayProps) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let nav = navigator();
    let mut zoom = use_signal(|| 1.0_f32);
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

    // Find starting position from the route's attachment_index.
    let start_pos = image_attachments
        .iter()
        .position(|(orig_idx, _)| *orig_idx == props.attachment_index)
        .unwrap_or(0);

    let mut active_pos = use_signal(|| start_pos);

    // Keyboard: Escape closes, ArrowLeft/ArrowRight navigate.
    #[cfg(target_arch = "wasm32")]
    {
        let img_count = image_attachments.len();
        use_effect(move || {
            spawn(async move {
                let mut eval = document::eval(
                    "(function(){\
                        function onKey(e) {\
                            if (e.key === 'Escape') {\
                                document.removeEventListener('keydown', onKey, true);\
                                dioxus.send('escape');\
                            } else if (e.key === 'ArrowLeft') {\
                                dioxus.send('prev');\
                            } else if (e.key === 'ArrowRight') {\
                                dioxus.send('next');\
                            }\
                        }\
                        document.addEventListener('keydown', onKey, true);\
                    })()",
                );
                loop {
                    match eval.recv::<String>().await {
                        Ok(msg) if msg == "escape" => { nav.go_back(); break; }
                        Ok(msg) if msg == "prev" => {
                            let cur = *active_pos.read();
                            if cur > 0 { active_pos.set(cur - 1); }
                        }
                        Ok(msg) if msg == "next" => {
                            let cur = *active_pos.read();
                            if cur + 1 < img_count { active_pos.set(cur + 1); }
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
    let image_style = format!("transform: scale({scale});");
    let filename = attachment.filename.clone();
    let url = attachment.url.clone();

    let can_prev = pos > 0;
    let can_next = pos + 1 < total;

    rsx! {
        div {
            class: "poly-media-viewer-overlay",
            tabindex: "-1",
            onclick: move |_| nav.go_back(),  // Backdrop dismiss

            // — Full-width top bar —
            // Author info on the left, controls on the right.
            // ✕ is the rightmost button so it sits near the window close control.
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
                            let next = (*zoom.read() - 0.25_f32).max(0.25_f32);
                            zoom.set(next);
                        },
                        "－"
                    }
                    button {
                        class: "poly-media-viewer-btn",
                        title: "{t(\"zoom-in\")}",
                        onclick: move |_| {
                            let next = (*zoom.read() + 0.25_f32).min(5.0_f32);
                            zoom.set(next);
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
                    // Close is rightmost — near Electron window controls
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
                        if cur > 0 { active_pos.set(cur - 1); }
                    },
                    "‹"
                }
            }

            // — Stage: dark background clicks bubble up and dismiss the viewer.
            //   The image itself stops propagation so clicking on it doesn't close. —
            div {
                class: "poly-media-viewer-stage",
                img {
                    class: "poly-media-viewer-image",
                    style: "{image_style}",
                    src: "{attachment.url}",
                    alt: "{attachment.filename}",
                    onclick: move |e| e.stop_propagation(),
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
                        if cur + 1 < total { active_pos.set(cur + 1); }
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

//! Fullscreen message media viewer overlay.
//!
//! Route-backed overlay used for message image attachments.
//! Phase 2.19 starts with a single-image viewer; multi-image carousel
//! controls and thumbnail strip arrive in a follow-up checklist item.

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

    // Escape key: document-level capture so it fires regardless of which
    // element currently holds keyboard focus (e.g. the underlying message input).
    // The listener is one-shot — it removes itself after the first Escape.
    #[cfg(target_arch = "wasm32")]
    {
        use_effect(move || {
            spawn(async move {
                let mut eval = document::eval(
                    "(function(){\
                        function onKey(e) {\
                            if (e.key === 'Escape') {\
                                document.removeEventListener('keydown', onKey, true);\
                                dioxus.send(true);\
                            }\
                        }\
                        document.addEventListener('keydown', onKey, true);\
                    })()",
                );
                if eval.recv::<bool>().await.is_ok() {
                    nav.go_back();
                }
            });
        });
    }

    let attachment: Option<Attachment> = chat_data
        .read()
        .messages
        .iter()
        .find(|msg| msg.id == props.message_id)
        .and_then(|msg| msg.attachments.get(props.attachment_index))
        .cloned();

    let Some(attachment) = attachment else {
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

    rsx! {
        div {
            class: "poly-media-viewer-overlay",
            tabindex: "-1",
            onclick: move |_| nav.go_back(),

            div {
                class: "poly-media-viewer-toolbar",
                onclick: move |e| e.stop_propagation(),
                button {
                    class: "poly-media-viewer-btn",
                    title: "{t(\"action-close\")}",
                    onclick: move |_| nav.go_back(),
                    "✕"
                }
                button {
                    class: "poly-media-viewer-btn",
                    title: "{t(\"zoom-out\")}",
                    onclick: move |_| {
                        let next = (*zoom.read() - 0.2_f32).max(0.6_f32);
                        zoom.set(next);
                    },
                    "－"
                }
                button {
                    class: "poly-media-viewer-btn",
                    title: "{t(\"zoom-in\")}",
                    onclick: move |_| {
                        let next = (*zoom.read() + 0.2_f32).min(3.0_f32);
                        zoom.set(next);
                    },
                    "＋"
                }
                div { class: "poly-media-viewer-menu-wrap",
                    button {
                        class: "poly-media-viewer-btn",
                        title: "{t(\"user-profile-more-options\")}",
                        onclick: move |_| {
                            let was_open = *menu_open.read();
                            menu_open.set(!was_open);
                        },
                        "···"
                    }
                    if *menu_open.read() {
                        div {
                            class: "poly-media-viewer-menu",
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
            }

            div {
                class: "poly-media-viewer-stage",
                onclick: move |e| e.stop_propagation(),
                img {
                    class: "poly-media-viewer-image",
                    style: "{image_style}",
                    src: "{attachment.url}",
                    alt: "{attachment.filename}",
                }
            }

            div {
                class: "poly-media-viewer-footer",
                onclick: move |e| e.stop_propagation(),
                div { class: "poly-media-viewer-filename", "{attachment.filename}" }
                div { class: "poly-media-viewer-meta", "{props.channel_id} • {props.message_id}" }
            }
        }
    }
}

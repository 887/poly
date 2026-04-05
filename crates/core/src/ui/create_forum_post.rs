//! Create Forum Post — full-page form rendered inside `ServerLayout`.
//!
//! Navigated to from the "+ Create Post" button in the channel list sidebar for
//! forum channels. The `ChannelList` stays visible on the left.
//! For now this is a UI-only stub form (no backend API call yet).

use crate::ui::routes::Route;
use dioxus::prelude::*;

/// Full-page Create Forum Post form.
#[rustfmt::skip]
#[component]
pub(crate) fn CreateForumPostPage(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    let nav = navigator();
    let mut title = use_signal(String::new);
    let mut url = use_signal(String::new);
    let mut body = use_signal(String::new);

    let back_route = Route::ServerChat {
        backend: backend.clone(),
        instance_id: instance_id.clone(),
        account_id: account_id.clone(),
        server_id: server_id.clone(),
        channel_id: channel_id.clone(),
    };

    rsx! {
        div { class: "create-forum-post-page",
            div { class: "create-forum-post-card",
                h1 { class: "create-forum-post-title", "Create Post" }

                div { class: "create-forum-post-field",
                    label { class: "create-forum-post-label", "Title" }
                    input {
                        class: "create-forum-post-input",
                        r#type: "text",
                        placeholder: "Post title",
                        value: "{title}",
                        oninput: move |e| title.set(e.value()),
                    }
                }

                div { class: "create-forum-post-field",
                    label { class: "create-forum-post-label", "URL" }
                    input {
                        class: "create-forum-post-input",
                        r#type: "text",
                        placeholder: "Optional",
                        value: "{url}",
                        oninput: move |e| url.set(e.value()),
                    }
                }

                div { class: "create-forum-post-field",
                    label { class: "create-forum-post-label", "Body" }
                    textarea {
                        class: "create-forum-post-textarea",
                        placeholder: "Optional",
                        value: "{body}",
                        oninput: move |e| body.set(e.value()),
                    }
                }

                div { class: "create-forum-post-actions",
                    button {
                        class: "create-forum-post-cancel",
                        onclick: {
                            let route = back_route.clone();
                            move |_| { nav.push(route.clone()); }
                        },
                        "Cancel"
                    }
                    button {
                        class: "create-forum-post-submit",
                        disabled: title.read().trim().is_empty(),
                        onclick: move |_| {
                            // TODO: call backend create_post when API is available
                            nav.push(back_route.clone());
                        },
                        "Create"
                    }
                }
            }
        }
    }
}

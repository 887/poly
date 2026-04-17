//! Create Forum Post and Forum Search pages, rendered inside `ServerLayout`.

use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// Full-page Create Forum Post form.
#[context_menu(None)]
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

/// Forum search page — search posts within a community.
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub(crate) fn ForumSearchPage(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    let nav = navigator();
    let mut query = use_signal(String::new);
    let mut scope = use_signal(|| "All".to_string());

    let back_route = Route::ServerChat {
        backend: backend.clone(),
        instance_id: instance_id.clone(),
        account_id: account_id.clone(),
        server_id: server_id.clone(),
        channel_id: channel_id.clone(),
    };

    rsx! {
        div { class: "forum-search-page",
            div { class: "forum-search-card",
                h1 { class: "forum-search-title", "Search" }

                div { class: "forum-search-scope-row",
                    for label in ["Subscribed", "Local", "All"] {
                        button {
                            class: if *scope.read() == label { "forum-filter-btn active" } else { "forum-filter-btn" },
                            onclick: {
                                let label = label.to_string();
                                move |_| scope.set(label.clone())
                            },
                            "{label}"
                        }
                    }
                }

                div { class: "forum-search-input-row",
                    input {
                        class: "create-forum-post-input forum-search-input",
                        r#type: "text",
                        placeholder: "Search…",
                        autofocus: true,
                        value: "{query}",
                        oninput: move |e| query.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                // TODO: trigger actual search when backend API available
                            }
                        },
                    }
                    button {
                        class: "forum-search-btn",
                        onclick: move |_| {
                            // TODO: execute search
                        },
                        "Search"
                    }
                }

                div { class: "forum-search-results",
                    span { class: "forum-search-hint", "Enter a query above to search posts." }
                }

                button {
                    class: "create-forum-post-cancel",
                    onclick: move |_| { nav.push(back_route.clone()); },
                    "← Back"
                }
            }
        }
    }
}

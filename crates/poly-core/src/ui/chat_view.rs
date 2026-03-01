//! Chat view — message list and message input.

use crate::i18n::t;
use crate::state::AppState;
use dioxus::prelude::*;

/// Chat view component.
///
/// Shows the channel header, scrollable message list, and message input area.
#[component]
pub fn ChatView(app_state: Signal<AppState>) -> Element {
    let mut message_input = use_signal(String::new);
    let channel_id = app_state.read().nav.selected_channel.clone();

    rsx! {
        main { class: "chat-view",
            // Channel header bar
            div { class: "chat-header",
                span { class: "chat-channel-name",
                    if let Some(ref _ch) = channel_id {
                        // TODO: Get channel name
                        "# general"
                    } else {
                        "{t(\"chat-no-messages\")}"
                    }
                }
                div { class: "chat-header-actions",
                    button { class: "header-btn", title: "{t(\"action-search\")}", "🔍" }
                }
            }

            // Message list
            div { class: "message-list",
                // TODO(phase-2.7.6.1): Load messages from backend
                // Placeholder messages
                div { class: "message",
                    div { class: "message-avatar", "A" }
                    div { class: "message-content",
                        div { class: "message-header",
                            span { class: "message-author", "Alice" }
                            span { class: "message-timestamp", "12:34 PM" }
                        }
                        p { class: "message-text",
                            "Hey everyone! Welcome to the Poly Development server 👋"
                        }
                    }
                }
                div { class: "message",
                    div { class: "message-avatar", "B" }
                    div { class: "message-content",
                        div { class: "message-header",
                            span { class: "message-author", "Bob" }
                            span { class: "message-timestamp", "12:39 PM" }
                        }
                        p { class: "message-text",
                            "Thanks for having me! This project looks really cool."
                        }
                    }
                }
                div { class: "message",
                    div { class: "message-avatar", "C" }
                    div { class: "message-content",
                        div { class: "message-header",
                            span { class: "message-author", "Charlie" }
                            span { class: "message-timestamp", "12:44 PM" }
                        }
                        p { class: "message-text",
                            "Has anyone tried the new Dioxus 0.7 hot-reload? It's blazing fast!"
                        }
                    }
                }
            }

            // Message input area
            div { class: "message-input-area",
                input {
                    class: "message-input",
                    r#type: "text",
                    placeholder: "{t(\"chat-type-message\")}",
                    value: "{message_input}",
                    oninput: move |evt| message_input.set(evt.value()),
                    onkeypress: move |evt| {
                        if evt.key() == Key::Enter {
                            let text = message_input.read().clone();
                            if !text.is_empty() {
                                // TODO(phase-2.7.6.7): Send message via backend
                                tracing::info!("Send message: {text}");
                                message_input.set(String::new());
                            }
                        }
                    },
                }
                button {
                    class: "btn btn-send",
                    onclick: move |_| {
                        let text = message_input.read().clone();
                        if !text.is_empty() {
                            tracing::info!("Send message: {text}");
                            message_input.set(String::new());
                        }
                    },
                    "{t(\"chat-send\")}"
                }
            }
        }
    }
}

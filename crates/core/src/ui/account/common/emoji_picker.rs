//! Emoji picker — grid of emoji for reactions and message input.
//!
//! Shared between:
//! - Reaction picker (hover → add reaction to a message)
//! - Message input emoji button (insert emoji text at cursor)
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
//!
//! Shows a grid of commonly-used emoji organized by categories.
//! Clicking an emoji triggers the `on_select` callback.
// TODO(phase-2.5.15): Emoji picker for reactions and input

use crate::i18n::t;
use dioxus::prelude::*;

/// Categories of emoji with their contents.
pub(crate) const EMOJI_CATEGORIES: &[(&str, &str, &[&str])] = &[
    (
        "frecent",
        "⭐",
        &["👍", "❤️", "😂", "🎉", "🔥", "👀", "✅", "💯"],
    ),
    (
        "smileys",
        "😀",
        &[
            "😀",
            "😃",
            "😄",
            "😁",
            "😆",
            "😅",
            "🤣",
            "😂",
            "🙂",
            "😊",
            "😇",
            "🥰",
            "😍",
            "😘",
            "😗",
            "😙",
            "😚",
            "😋",
            "😛",
            "😜",
            "🤪",
            "😝",
            "🤑",
            "🤗",
            "🤭",
            "🤫",
            "🤔",
            "🤐",
            "😐",
            "😑",
            "😶",
            "😏",
            "😒",
            "🙄",
            "😬",
            "😮‍💨",
            "🤥",
            "😌",
            "😔",
            "😪",
            "🤤",
            "😴",
            "😷",
            "🤒",
            "🤕",
            "🤢",
            "🤮",
            "🤧",
            "🥵",
            "🥶",
            "🥴",
            "😵",
            "🤯",
            "🤠",
            "🥳",
            "🥸",
            "😎",
            "🤓",
            "🧐",
        ],
    ),
    (
        "gestures",
        "👋",
        &[
            "👋", "🤚", "🖐", "✋", "🖖", "👌", "🤌", "🤏", "✌️", "🤞", "🤟", "🤘", "🤙", "👈",
            "👉", "👆", "🖕", "👇", "☝️", "👍", "👎", "✊", "👊", "🤛", "🤜", "👏", "🙌", "🫶",
            "👐", "🤝", "🙏", "💪",
        ],
    ),
    (
        "hearts",
        "❤️",
        &[
            "❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "🤎", "💔", "❣️", "💕", "💞", "💓",
            "💗", "💖", "💘", "💝",
        ],
    ),
    (
        "objects",
        "🎉",
        &[
            "🎉", "🎊", "🎈", "🎁", "🏆", "🥇", "⭐", "🌟", "✨", "💫", "🔥", "💯", "✅", "❌",
            "⚠️", "💡", "🔔", "🔒", "🔑", "💎", "🎮", "🎲", "🎵", "🎶", "🎸", "📱", "💻", "🖥",
            "📷", "☕", "🍕", "🍔", "🌮",
        ],
    ),
    (
        "flags",
        "🏁",
        &[
            "🏁",
            "🚩",
            "🏳️",
            "🏴",
            "🏳️‍🌈",
            "🏳️‍⚧️",
            "🇺🇸",
            "🇬🇧",
            "🇩🇪",
            "🇫🇷",
            "🇪🇸",
            "🇯🇵",
            "🇰🇷",
            "🇨🇳",
            "🇧🇷",
            "🇦🇺",
        ],
    ),
];

/// Emoji picker component.
#[rustfmt::skip]
///
/// Renders a category-tabbed grid of emoji. Clicking one fires `on_select`.
#[component]
pub fn EmojiPicker(on_select: EventHandler<String>, on_close: EventHandler<()>) -> Element {
    let mut active_category = use_signal(|| 0usize);
    let mut search_text = use_signal(String::new);
    let cat_idx = *active_category.read();

    // Get the current category's emoji (or search results)
    let search = search_text.read().clone();
    let display_emoji: Vec<&str> = if search.is_empty() {
        EMOJI_CATEGORIES
            .get(cat_idx)
            .map(|(_, _, emojis)| emojis.to_vec())
            .unwrap_or_default()
    } else {
        // Simple search: show all emoji that match (basically just filter categories)
        EMOJI_CATEGORIES
            .iter()
            .flat_map(|(_, _, emojis)| emojis.iter().copied())
            .collect()
    };

    rsx! {
        div { class: "emoji-picker",
            // Click outside to close via a backdrop
            div {
                class: "emoji-picker-backdrop",
                onclick: move |_| on_close.call(()),
            }
            div { class: "emoji-picker-panel",
                // Search bar
                div { class: "emoji-search",
                    input {
                        r#type: "text",
                        class: "emoji-search-input",
                        placeholder: "{t(\"emoji-search\")}",
                        value: "{search_text}",
                        oninput: move |evt| search_text.set(evt.value()),
                    }
                }
                // Category tabs
                div { class: "emoji-category-tabs",
                    for (idx , (_ , icon , _)) in EMOJI_CATEGORIES.iter().enumerate() {
                        button {
                            class: if idx == cat_idx { "emoji-tab active" } else { "emoji-tab" },
                            onclick: move |_| active_category.set(idx),
                            "{icon}"
                        }
                    }
                }
                // Emoji grid
                div { class: "emoji-grid",
                    for emoji in &display_emoji {
                        {
                            let e = emoji.to_string();
                            let e2 = e.clone();
                            rsx! {
                                button { class: "emoji-item", onclick: move |_| on_select.call(e2.clone()), "{e}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

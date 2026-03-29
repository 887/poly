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

use super::media_picker::{EmojiSection, EmojiSectionItems, build_emoji_sections};

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

/// Emoji picker component (used for reactions).
///
/// Compact picker: left sidebar icons + scrollable section list.
#[rustfmt::skip]
#[component]
pub fn EmojiPicker(on_select: EventHandler<String>, on_close: EventHandler<()>) -> Element {
    let mut search_text = use_signal(String::new);
    let mut active_section = use_signal(|| 0usize);
    let search = search_text.read().clone();

    let sections = use_memo(|| build_emoji_sections(&[]));
    let sections_ref = sections.read().clone();

    let search_results: Vec<String> = if search.is_empty() {
        vec![]
    } else {
        let q = search.to_lowercase();
        sections_ref
            .iter()
            .flat_map(|s| match &s.items {
                EmojiSectionItems::Unicode(v) => v.clone(),
                EmojiSectionItems::Custom(_) => vec![],
            })
            .collect()
    };

    let active_idx = *active_section.read();

    rsx! {
        div { class: "emoji-picker",
            div { class: "emoji-picker-backdrop", onclick: move |_| on_close.call(()) }
            div { class: "emoji-picker-panel",
                div { class: "emoji-search",
                    input {
                        r#type: "text",
                        class: "emoji-search-input",
                        placeholder: "{t(\"emoji-search\")}",
                        value: "{search_text}",
                        oninput: move |evt| search_text.set(evt.value()),
                    }
                }
                div { class: "emoji-body",
                    div { class: "emoji-sidebar",
                        for (idx, section) in sections_ref.iter().enumerate() {
                            {
                                let sid = section.id.clone();
                                let label = section.label.clone();
                                let icon = section.icon.clone();
                                rsx! {
                                    button {
                                        class: if idx == active_idx { "emoji-sidebar-icon active" } else { "emoji-sidebar-icon" },
                                        title: "{label}",
                                        onclick: move |_| {
                                            active_section.set(idx);
                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                let js = format!("document.getElementById('{sid}')?.scrollIntoView({{block:'start',behavior:'smooth'}})");
                                                let _ = document::eval(&js);
                                            }
                                        },
                                        "{icon}"
                                    }
                                }
                            }
                        }
                    }
                    div {
                        class: "emoji-scroll-area",
                        id: "emoji-scroll-area-reaction",
                        if search.is_empty() {
                            for section in sections_ref.iter() {
                                div { class: "emoji-section",
                                    div { id: "{section.id}", class: "emoji-section-header", "{section.label}" }
                                    div { class: "emoji-grid",
                                        for e in match &section.items { EmojiSectionItems::Unicode(v) => v.clone(), _ => vec![] } {
                                            {
                                                let e2 = e.clone();
                                                let e3 = e.clone();
                                                rsx! {
                                                    button {
                                                        class: "emoji-item",
                                                        onclick: move |_| on_select.call(e3.clone()),
                                                        "{e2}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            div { class: "emoji-section",
                                div { class: "emoji-section-header", "{t(\"emoji-search-results\")}" }
                                div { class: "emoji-grid",
                                    for e in &search_results {
                                        {
                                            let e2 = e.clone(); let e3 = e.clone();
                                            rsx! { button { class: "emoji-item", onclick: move |_| on_select.call(e3.clone()), "{e2}" } }
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
}

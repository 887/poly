//! Unified media picker — emoji, GIF, stickers + markdown toggle.
//!
//! Opened by the single 😀 toolbar button in the message composer.
//! On desktop it appears as a floating panel (bottom-right); on mobile
//! it slides up from the bottom as a full-width sheet.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX + logic.

use super::emoji_picker::EMOJI_CATEGORIES;
use crate::i18n::t;
use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MediaTab {
    #[default]
    Emoji,
    Gif,
    Stickers,
}

/// Emoji tab content — grid of emoji organized by category.
#[rustfmt::skip]
#[component]
fn EmojiTabContent(on_select: EventHandler<String>) -> Element {
    let mut active_category = use_signal(|| 0usize);
    let mut search_text = use_signal(String::new);
    let cat_idx = *active_category.read();
    let search = search_text.read().clone();

    let display_emoji: Vec<&str> = if search.is_empty() {
        EMOJI_CATEGORIES
            .get(cat_idx)
            .map(|(_, _, emojis)| emojis.to_vec())
            .unwrap_or_default()
    } else {
        EMOJI_CATEGORIES
            .iter()
            .flat_map(|(_, _, emojis)| emojis.iter().copied())
            .collect()
    };

    rsx! {
        div { class: "media-picker-emoji-tab",
            div { class: "emoji-search",
                input {
                    r#type: "text",
                    class: "emoji-search-input",
                    placeholder: "{t(\"emoji-search\")}",
                    value: "{search_text}",
                    oninput: move |e| search_text.set(e.value()),
                }
            }
            div { class: "emoji-category-tabs",
                for (idx, (_, icon, _)) in EMOJI_CATEGORIES.iter().enumerate() {
                    button {
                        class: if idx == cat_idx { "emoji-tab active" } else { "emoji-tab" },
                        onclick: move |_| active_category.set(idx),
                        "{icon}"
                    }
                }
            }
            div { class: "emoji-grid",
                for emoji in &display_emoji {
                    {
                        let e = emoji.to_string();
                        let e2 = e.clone();
                        rsx! {
                            button {
                                class: "emoji-item",
                                onclick: move |_| on_select.call(e2.clone()),
                                "{e}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Placeholder tab shown when GIF or Sticker search is not yet implemented.
#[rustfmt::skip]
#[component]
fn PlaceholderTabContent(message: String) -> Element {
    rsx! {
        div { class: "media-picker-placeholder",
            span { class: "media-picker-placeholder-text", "{message}" }
        }
    }
}

/// Footer row with markdown toggle.
#[rustfmt::skip]
#[component]
fn MediaPickerFooter(markdown_enabled: Signal<bool>) -> Element {
    let enabled = *markdown_enabled.read();
    rsx! {
        div { class: "media-picker-footer",
            label { class: "media-picker-markdown-row",
                span { class: "media-picker-markdown-label", "{t(\"media-picker-markdown\")}" }
                div {
                    class: if enabled { "media-picker-toggle media-picker-toggle-on" } else { "media-picker-toggle" },
                    onclick: move |_| {
                        let current = *markdown_enabled.read();
                        markdown_enabled.set(!current);
                    },
                    div { class: "media-picker-toggle-thumb" }
                }
            }
        }
    }
}

/// Unified media picker popup.
///
/// Shows emoji, GIF, and sticker tabs plus a markdown formatting toggle.
/// Positioning is controlled by CSS: bottom-right panel on desktop,
/// full-width slide-up sheet on mobile.
#[rustfmt::skip]
#[component]
pub fn MediaPickerPopup(
    on_emoji_select: EventHandler<String>,
    on_close: EventHandler<()>,
    markdown_enabled: Signal<bool>,
) -> Element {
    let mut active_tab = use_signal(MediaTab::default);
    let tab = *active_tab.read();

    rsx! {
        div { class: "emoji-picker",
            div {
                class: "emoji-picker-backdrop",
                onclick: move |_| on_close.call(()),
            }
            div { class: "emoji-picker-panel media-picker-panel",
                // Tab bar
                div { class: "media-picker-tabs",
                    button {
                        class: if tab == MediaTab::Emoji { "media-picker-tab active" } else { "media-picker-tab" },
                        onclick: move |_| active_tab.set(MediaTab::Emoji),
                        "{t(\"emoji-picker\")}"
                    }
                    button {
                        class: if tab == MediaTab::Gif { "media-picker-tab active" } else { "media-picker-tab" },
                        onclick: move |_| active_tab.set(MediaTab::Gif),
                        "{t(\"gif-picker\")}"
                    }
                    button {
                        class: if tab == MediaTab::Stickers { "media-picker-tab active" } else { "media-picker-tab" },
                        onclick: move |_| active_tab.set(MediaTab::Stickers),
                        "{t(\"stickers-picker\")}"
                    }
                }
                // Tab content
                match tab {
                    MediaTab::Emoji => rsx! {
                        EmojiTabContent { on_select: move |e| on_emoji_select.call(e) }
                    },
                    MediaTab::Gif => rsx! {
                        PlaceholderTabContent { message: t("media-picker-gif-placeholder") }
                    },
                    MediaTab::Stickers => rsx! {
                        PlaceholderTabContent { message: t("media-picker-stickers-placeholder") }
                    },
                }
                MediaPickerFooter { markdown_enabled }
            }
        }
    }
}

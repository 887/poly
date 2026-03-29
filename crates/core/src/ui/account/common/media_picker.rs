//! Unified media picker — emoji, GIF, stickers + markdown toggle.
//!
//! Opened by the single 😀 toolbar button in the message composer.
//! On desktop it appears as a floating panel (bottom-right); on mobile
//! it slides up from the bottom as a full-width sheet.
//!
//! Layout mirrors Discord's emoji picker:
//! - Left sidebar: small icons for each emoji group (standard categories +
//!   per-server custom emoji packs). Clicking scrolls to that section.
//! - Right content: scrollable list of sections. Each section starts with a
//!   sticky header. Scrolling syncs the left sidebar active icon.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX + logic.

use super::emoji_picker::EMOJI_CATEGORIES;
use crate::i18n::t;
use dioxus::prelude::*;
use poly_client::CustomEmoji;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum MediaTab {
    #[default]
    Emoji,
    Gif,
    Stickers,
}

/// A logical section in the emoji scroll area.
#[derive(Clone, PartialEq)]
pub(crate) struct EmojiSection {
    /// Stable DOM id for the section header element.
    pub id: String,
    /// Display name for the sticky header and sidebar tooltip.
    pub label: String,
    /// Small icon shown in the left sidebar (emoji char or first emoji of set).
    pub icon: String,
    /// The emoji items in this section.
    pub items: EmojiSectionItems,
}

#[derive(Clone, PartialEq)]
pub(crate) enum EmojiSectionItems {
    /// Standard unicode emoji (text characters).
    Unicode(Vec<String>),
    /// Custom server emoji with image URLs.
    Custom(Vec<CustomEmoji>),
}

/// Build the full ordered list of emoji sections from standard categories
/// plus any custom emoji grouped by source_name.
pub(crate) fn build_emoji_sections(custom: &[CustomEmoji]) -> Vec<EmojiSection> {
    let mut sections: Vec<EmojiSection> = EMOJI_CATEGORIES
        .iter()
        .enumerate()
        .map(|(i, (id, icon, emojis))| EmojiSection {
            id: format!("emoji-section-{id}"),
            label: id.to_string(),
            icon: icon.to_string(),
            items: EmojiSectionItems::Unicode(emojis.iter().map(|e| e.to_string()).collect()),
        })
        .collect();

    // Group custom emoji by source_name
    let mut groups: Vec<(String, Vec<CustomEmoji>)> = Vec::new();
    for emoji in custom {
        let name = emoji
            .source_name
            .clone()
            .unwrap_or_else(|| "Server".to_string());
        if let Some(g) = groups.iter_mut().find(|(n, _)| n == &name) {
            g.1.push(emoji.clone());
        } else {
            groups.push((name, vec![emoji.clone()]));
        }
    }

    for (name, emojis) in groups {
        let icon = emojis
            .first()
            .and_then(|e| e.unicode_fallback.clone())
            .unwrap_or_else(|| "🖼".to_string());
        let id = name.to_lowercase().replace(' ', "-");
        sections.push(EmojiSection {
            id: format!("emoji-section-custom-{id}"),
            label: name,
            icon,
            items: EmojiSectionItems::Custom(emojis),
        });
    }

    sections
}

/// Left sidebar icon button for one emoji section.
#[rustfmt::skip]
#[component]
fn SidebarIcon(icon: String, label: String, section_id: String) -> Element {
    rsx! {
        button {
            class: "emoji-sidebar-icon",
            title: "{label}",
            "data-emoji-section": "{section_id}",
            onclick: move |_| {
                let id = section_id.clone();
                #[cfg(target_arch = "wasm32")]
                {
                    // Scroll to section and update active state immediately
                    let js = format!(r#"
                        (function() {{
                            const el = document.getElementById('{id}');
                            const area = document.querySelector('.emoji-scroll-area');
                            if (!el || !area) return;
                            area.scrollTop = el.offsetTop - area.offsetTop;
                            // Update active sidebar icon immediately
                            document.querySelectorAll('.emoji-sidebar-icon').forEach(b => {{
                                b.classList.toggle('active', b.dataset.emojiSection === '{id}');
                            }});
                        }})()
                    "#);
                    let _ = document::eval(&js);
                }
            },
            "{icon}"
        }
    }
}

/// One emoji section: sticky header + grid of items.
#[rustfmt::skip]
#[component]
fn EmojiSectionBlock(section: EmojiSection, on_select: EventHandler<String>) -> Element {
    rsx! {
        div { class: "emoji-section",
            div { id: "{section.id}", class: "emoji-section-header", "{section.label}" }
            div { class: "emoji-grid",
                match section.items {
                    EmojiSectionItems::Unicode(ref emojis) => rsx! {
                        for e in emojis {
                            {
                                let e2 = e.clone();
                                let e3 = e.clone();
                                rsx! {
                                    button {
                                        class: "emoji-item",
                                        title: "{e2}",
                                        onclick: move |_| on_select.call(e3.clone()),
                                        "{e2}"
                                    }
                                }
                            }
                        }
                    },
                    EmojiSectionItems::Custom(ref emojis) => rsx! {
                        for emoji in emojis {
                            {
                                let shortcode = format!(":{}:", emoji.shortcode);
                                let shortcode2 = shortcode.clone();
                                let display = emoji.unicode_fallback.clone().unwrap_or_else(|| shortcode.clone());
                                let display2 = display.clone();
                                let label = emoji.shortcode.clone();
                                rsx! {
                                    button {
                                        class: "emoji-item emoji-item-custom",
                                        title: ":{label}:",
                                        onclick: move |_| on_select.call(shortcode2.clone()),
                                        "{display2}"
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

/// Full emoji tab: left sidebar + scrollable section list with search + JS scroll spy.
///
/// The sidebar active-icon highlight is managed entirely in JS (via `__polyEmojiScrollSpy`)
/// to avoid async round-trips between Rust and the DOM on every scroll event.
#[rustfmt::skip]
#[component]
fn EmojiTabContent(
    on_select: EventHandler<String>,
    custom_emojis: Vec<CustomEmoji>,
) -> Element {
    let mut search_text = use_signal(String::new);
    let search = search_text.read().clone();

    let sections = use_memo(move || build_emoji_sections(&custom_emojis));
    let sections_ref = sections.read().clone();

    // Install JS scroll spy once the scroll area is in the DOM.
    // The spy reads .emoji-section-header positions and toggles .active on
    // .emoji-sidebar-icon[data-emoji-section] matching the topmost visible header.
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let _ = document::eval(r#"
            (function() {
                const area = document.querySelector('.emoji-scroll-area');
                if (!area || area.__polyEmojiSpyInstalled) return;
                area.__polyEmojiSpyInstalled = true;
                function update() {
                    const areaTop = area.getBoundingClientRect().top;
                    const headers = area.querySelectorAll('.emoji-section-header');
                    let activeId = headers.length > 0 ? headers[0].id : null;
                    for (const h of headers) {
                        if (h.getBoundingClientRect().top - areaTop <= 4) activeId = h.id;
                    }
                    document.querySelectorAll('.emoji-sidebar-icon').forEach(b => {
                        b.classList.toggle('active', b.dataset.emojiSection === activeId);
                    });
                }
                area.addEventListener('scroll', update, {passive: true});
                update();
            })()
        "#);
    });

    // Filtered emoji across all sections for search mode
    let search_results: Vec<String> = if search.is_empty() {
        vec![]
    } else {
        let q = search.to_lowercase();
        sections_ref
            .iter()
            .flat_map(|s| match &s.items {
                EmojiSectionItems::Unicode(v) => v.clone(),
                EmojiSectionItems::Custom(v) => v
                    .iter()
                    .filter(|e| e.shortcode.contains(&q))
                    .filter_map(|e| e.unicode_fallback.clone())
                    .collect(),
            })
            .collect()
    };

    rsx! {
        div { class: "media-picker-emoji-tab",
            // Search bar
            div { class: "emoji-search",
                input {
                    r#type: "text",
                    class: "emoji-search-input",
                    placeholder: "{t(\"emoji-search\")}",
                    value: "{search_text}",
                    oninput: move |e| search_text.set(e.value()),
                }
            }
            // Body: sidebar + scroll area
            div { class: "emoji-body",
                // Left sidebar with section icons
                div { class: "emoji-sidebar",
                    for section in sections_ref.iter() {
                        SidebarIcon {
                            icon: section.icon.clone(),
                            label: section.label.clone(),
                            section_id: section.id.clone(),
                        }
                    }
                }
                // Right: scrollable sections or search results
                div {
                    class: "emoji-scroll-area",
                    if search.is_empty() {
                        for section in sections_ref.iter() {
                            EmojiSectionBlock {
                                section: section.clone(),
                                on_select: move |e| on_select.call(e),
                            }
                        }
                    } else {
                        div { class: "emoji-section",
                            div { class: "emoji-section-header", "{t(\"emoji-search-results\")}" }
                            div { class: "emoji-grid",
                                for e in &search_results {
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
                                if search_results.is_empty() {
                                    span { class: "emoji-no-results", "{t(\"emoji-no-results\")}" }
                                }
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
    custom_emojis: Vec<CustomEmoji>,
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
                // Tab content — fixed-height area, no resize on tab switch
                div { class: "media-picker-content",
                    match tab {
                        MediaTab::Emoji => rsx! {
                            EmojiTabContent {
                                on_select: move |e| on_emoji_select.call(e),
                                custom_emojis: custom_emojis.clone(),
                            }
                        },
                        MediaTab::Gif => rsx! {
                            PlaceholderTabContent { message: t("media-picker-gif-placeholder") }
                        },
                        MediaTab::Stickers => rsx! {
                            PlaceholderTabContent { message: t("media-picker-stickers-placeholder") }
                        },
                    }
                }
                MediaPickerFooter { markdown_enabled }
            }
        }
    }
}

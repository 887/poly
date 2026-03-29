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

use super::media_picker::{EmojiSectionItems, build_emoji_sections};

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

/// Shortcode names for each unicode emoji (space-separated synonyms).
/// Used for search: typing `:joy:`, `laughing`, or `:cry:` all find 😂.
pub(crate) const EMOJI_SHORTCODES: &[(&str, &str)] = &[
    // frecent
    ("👍", "thumbsup +1 yes ok approve"),
    ("❤️", "heart love red"),
    ("😂", "joy laughing cry tears rofl"),
    ("🎉", "tada party celebrate confetti"),
    ("🔥", "fire hot flame"),
    ("👀", "eyes look see"),
    ("✅", "white_check_mark check done yes"),
    ("💯", "100 perfect score"),
    // smileys
    ("😀", "grinning smile happy"),
    ("😃", "smiley smile happy"),
    ("😄", "smile happy grin"),
    ("😁", "grin beaming"),
    ("😆", "laughing sweat_smile satisfied"),
    ("😅", "sweat_smile nervous"),
    ("🤣", "rofl rolling_on_floor laughing"),
    ("😂", "joy laughing cry tears"),
    ("🙂", "slightly_smiling_face smile"),
    ("😊", "blush happy"),
    ("😇", "innocent angel halo"),
    ("🥰", "smiling_face_with_hearts love adore"),
    ("😍", "heart_eyes love adore"),
    ("😘", "kissing_heart kiss love"),
    ("😗", "kissing"),
    ("😙", "kissing_smiling_eyes"),
    ("😚", "kissing_closed_eyes"),
    ("😋", "yum delicious"),
    ("😛", "stuck_out_tongue"),
    ("😜", "stuck_out_tongue_winking_eye wink"),
    ("🤪", "zany crazy"),
    ("😝", "stuck_out_tongue_closed_eyes"),
    ("🤑", "money_mouth rich"),
    ("🤗", "hugging hug"),
    ("🤭", "hand_over_mouth giggle"),
    ("🤫", "shushing quiet shh"),
    ("🤔", "thinking hmm"),
    ("🤐", "zipper_mouth quiet"),
    ("😐", "neutral_face meh"),
    ("😑", "expressionless"),
    ("😶", "no_mouth silent"),
    ("😏", "smirk"),
    ("😒", "unamused annoyed"),
    ("🙄", "eye_roll rolled_eyes"),
    ("😬", "grimacing nervous"),
    ("😮‍💨", "exhaling sigh"),
    ("🤥", "lying pinocchio"),
    ("😌", "relieved"),
    ("😔", "pensive sad"),
    ("😪", "sleepy tired"),
    ("🤤", "drooling"),
    ("😴", "sleeping zzz tired"),
    ("😷", "mask sick ill"),
    ("🤒", "sick thermometer ill"),
    ("🤕", "hurt bandage injured"),
    ("🤢", "nauseated sick green"),
    ("🤮", "vomiting puke sick"),
    ("🤧", "sneezing sneeze cold"),
    ("🥵", "hot_face overheating sweating"),
    ("🥶", "cold_face freezing"),
    ("🥴", "woozy drunk dizzy"),
    ("😵", "dizzy_face knocked_out"),
    ("🤯", "exploding_head mind_blown shocked"),
    ("🤠", "cowboy"),
    ("🥳", "partying celebrate party"),
    ("🥸", "disguised incognito"),
    ("😎", "sunglasses cool"),
    ("🤓", "nerd glasses"),
    ("🧐", "monocle serious"),
    // gestures
    ("👋", "wave hello hi bye"),
    ("🤚", "raised_back_of_hand stop"),
    ("🖐", "hand stop"),
    ("✋", "raised_hand stop"),
    ("🖖", "vulcan_salute spock live_long"),
    ("👌", "ok_hand perfect"),
    ("🤌", "pinched_fingers chef"),
    ("🤏", "pinching_hand small"),
    ("✌️", "peace victory v"),
    ("🤞", "crossed_fingers lucky hope"),
    ("🤟", "love_you ily"),
    ("🤘", "metal rock horns"),
    ("🤙", "call_me shaka"),
    ("👈", "point_left backhand_left"),
    ("👉", "point_right backhand_right"),
    ("👆", "point_up backhand_up"),
    ("🖕", "middle_finger fu"),
    ("👇", "point_down backhand_down"),
    ("☝️", "point_up index"),
    ("👎", "thumbsdown -1 no dislike"),
    ("✊", "fist raised"),
    ("👊", "punch oncoming_fist"),
    ("🤛", "left_facing_fist"),
    ("🤜", "right_facing_fist"),
    ("👏", "clap clapping"),
    ("🙌", "raised_hands hooray"),
    ("🫶", "heart_hands love"),
    ("👐", "open_hands"),
    ("🤝", "handshake deal"),
    ("🙏", "pray thanks please"),
    ("💪", "muscle flex strong arm"),
    // hearts
    ("🧡", "orange_heart"),
    ("💛", "yellow_heart"),
    ("💚", "green_heart"),
    ("💙", "blue_heart"),
    ("💜", "purple_heart"),
    ("🖤", "black_heart"),
    ("🤍", "white_heart"),
    ("🤎", "brown_heart"),
    ("💔", "broken_heart sad"),
    ("❣️", "heart_exclamation"),
    ("💕", "two_hearts"),
    ("💞", "revolving_hearts"),
    ("💓", "heartbeat"),
    ("💗", "heartpulse"),
    ("💖", "sparkling_heart"),
    ("💘", "cupid arrow"),
    ("💝", "gift_heart"),
    // objects
    ("🎊", "confetti_ball party"),
    ("🎈", "balloon party"),
    ("🎁", "gift present"),
    ("🏆", "trophy win"),
    ("🥇", "gold_medal first_place"),
    ("⭐", "star"),
    ("🌟", "star2 glowing_star"),
    ("✨", "sparkles"),
    ("💫", "dizzy sparkle"),
    ("💡", "bulb idea"),
    ("🔔", "bell notification"),
    ("🔒", "lock locked security"),
    ("🔑", "key unlock"),
    ("💎", "gem diamond"),
    ("🎮", "video_game controller gaming"),
    ("🎲", "game_die dice"),
    ("🎵", "musical_note music"),
    ("🎶", "notes music"),
    ("🎸", "guitar music"),
    ("📱", "mobile_phone iphone phone"),
    ("💻", "laptop computer"),
    ("🖥", "desktop_computer monitor"),
    ("📷", "camera photo"),
    ("☕", "coffee hot_beverage"),
    ("🍕", "pizza"),
    ("🍔", "hamburger burger"),
    ("🌮", "taco"),
    ("❌", "x cross no wrong"),
    ("⚠️", "warning caution"),
    // flags
    ("🏁", "checkered_flag racing"),
    ("🚩", "triangular_flag"),
    ("🏳️", "white_flag"),
    ("🏴", "black_flag"),
    ("🏳️‍🌈", "rainbow_flag pride lgbtq"),
    ("🏳️‍⚧️", "transgender_flag trans"),
    ("🇺🇸", "us united_states america flag"),
    ("🇬🇧", "gb united_kingdom britain england flag"),
    ("🇩🇪", "de germany german flag"),
    ("🇫🇷", "fr france french flag"),
    ("🇪🇸", "es spain spanish flag"),
    ("🇯🇵", "jp japan japanese flag"),
    ("🇰🇷", "kr south_korea korean flag"),
    ("🇨🇳", "cn china chinese flag"),
    ("🇧🇷", "br brazil flag"),
    ("🇦🇺", "au australia flag"),
];

/// Returns true if `emoji` matches the search query by shortcode name.
/// Strips leading/trailing `:` so `:joy:`, `joy`, and `jo` all match 😂.
pub(crate) fn emoji_shortcode_matches(emoji: &str, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    let q = query.trim_matches(':').to_lowercase();
    if q.is_empty() {
        return false;
    }
    EMOJI_SHORTCODES
        .iter()
        .find(|(e, _)| *e == emoji)
        .map(|(_, names)| names.split_whitespace().any(|n| n.contains(q.as_str())))
        .unwrap_or(false)
}

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
                EmojiSectionItems::Unicode(v) => v.iter()
                    .filter(|e| emoji_shortcode_matches(e, &q) || e.contains(q.trim_matches(':')))
                    .cloned()
                    .collect::<Vec<_>>(),
                EmojiSectionItems::Custom(_) | EmojiSectionItems::Divider => vec![],
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
                            if section.items != EmojiSectionItems::Divider {
                                {
                                    let label = section.label.clone();
                                    let icon = section.icon.clone();
                                    let _sid = section.id.clone();
                                    rsx! {
                                        button {
                                            class: if idx == active_idx { "emoji-sidebar-icon active" } else { "emoji-sidebar-icon" },
                                            title: "{label}",
                                            onclick: move |_| {
                                                active_section.set(idx);
                                                #[cfg(target_arch = "wasm32")]
                                                {
                                                    let js = format!("document.getElementById('{_sid}')?.scrollIntoView({{block:'start',behavior:'smooth'}})");
                                                    let _ = document::eval(&js);
                                                }
                                            },
                                            "{icon}"
                                        }
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
                                if section.items == EmojiSectionItems::Divider {
                                    div { class: "emoji-section-divider" }
                                } else {
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

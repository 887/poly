//! `SidebarLayoutKind::Feed` — HN / Mastodon feed tabs.
//!
//! For HN these are the six hard-coded feed categories
//! (Top / New / Best / Ask / Show / Jobs). Clicking a feed is a WP 5
//! concern (the view layer); this component just renders the navigation
//! list.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Static hard-coded feed tabs used by HN-style sidebars.
const FEEDS: &[(&str, &str)] = &[
    ("top", "Top"),
    ("new", "New"),
    ("best", "Best"),
    ("ask", "Ask"),
    ("show", "Show"),
    ("jobs", "Jobs"),
];

/// HN-style list of feed tabs.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn FeedLayout() -> Element {
    rsx! {
        aside { class: "client-sidebar feed-layout",
            h2 { class: "sidebar-header", "Feeds" }
            ul { class: "feed-list",
                {FEEDS.iter().map(|(id, label)| rsx! {
                    li {
                        key: "{id}",
                        class: "feed-row",
                        // Click handling is deferred to WP 5 (client-views
                        // navigates to the selected feed). Render inert for
                        // now so snapshots capture the list shape.
                        "{label}"
                    }
                })}
            }
        }
    }
}

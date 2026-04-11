#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! WP-2 parity: `uses_forum_layout()` (capability-derived) returns the same
//! answer as the old slug-based `is_forum()` for every known backend.

use poly_client::BackendType;

#[test]
fn forum_layout_matches_legacy_slug_list() {
    let legacy_forum_slugs = ["demo_forum", "hackernews", "lemmy", "github"];
    let non_forum_slugs = ["demo", "matrix", "discord", "teams", "stoat", "poly"];

    for slug in legacy_forum_slugs {
        let b = BackendType::from_slug(slug);
        assert!(
            b.uses_forum_layout(),
            "backend '{slug}' should use forum layout"
        );
    }

    for slug in non_forum_slugs {
        let b = BackendType::from_slug(slug);
        assert!(
            !b.uses_forum_layout(),
            "backend '{slug}' should NOT use forum layout"
        );
    }
}

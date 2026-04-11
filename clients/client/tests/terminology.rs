//! WP-6 — Per-plugin terminology regression test.
//!
//! Pins the FTL label keys returned by `container_label_key()` so host UI code
//! that depends on (e.g.) "Create community" for Lemmy doesn't silently regress
//! to the generic "Create server" when the mapping is touched.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::{ContainerLabelForm, container_label_key};

#[test]
fn lemmy_uses_community_terminology() {
    assert_eq!(
        container_label_key("lemmy", ContainerLabelForm::Singular),
        "term-container-community"
    );
    assert_eq!(
        container_label_key("lemmy", ContainerLabelForm::Plural),
        "term-container-community-plural"
    );
    assert_eq!(
        container_label_key("lemmy", ContainerLabelForm::CreateAction),
        "term-container-community-create"
    );
}

#[test]
fn matrix_uses_space_terminology() {
    assert_eq!(
        container_label_key("matrix", ContainerLabelForm::Singular),
        "term-container-space"
    );
    assert_eq!(
        container_label_key("matrix", ContainerLabelForm::CreateAction),
        "term-container-space-create"
    );
}

#[test]
fn teams_uses_team_terminology() {
    assert_eq!(
        container_label_key("teams", ContainerLabelForm::Singular),
        "term-container-team"
    );
}

#[test]
fn github_uses_repo_terminology() {
    assert_eq!(
        container_label_key("github", ContainerLabelForm::Singular),
        "term-container-repo"
    );
    assert_eq!(
        container_label_key("github", ContainerLabelForm::CreateAction),
        "term-container-repo-create"
    );
}

#[test]
fn hackernews_uses_feed_terminology() {
    assert_eq!(
        container_label_key("hackernews", ContainerLabelForm::Singular),
        "term-container-feed"
    );
}

#[test]
fn discord_falls_back_to_server_terminology() {
    assert_eq!(
        container_label_key("discord", ContainerLabelForm::Singular),
        "term-container-server"
    );
    assert_eq!(
        container_label_key("discord", ContainerLabelForm::CreateAction),
        "term-container-server-create"
    );
}

#[test]
fn unknown_slug_falls_back_to_server_terminology() {
    // New plugins that haven't declared container terminology yet default to
    // the generic "server" noun rather than crashing or returning an empty key.
    assert_eq!(
        container_label_key("some-future-plugin", ContainerLabelForm::Singular),
        "term-container-server"
    );
    assert_eq!(
        container_label_key("some-future-plugin", ContainerLabelForm::Plural),
        "term-container-server-plural"
    );
}

#[test]
fn demo_forum_shares_lemmy_terminology() {
    // demo_forum is the in-repo forum mock; it intentionally mirrors Lemmy so
    // UI previews exercise the same terminology path.
    assert_eq!(
        container_label_key("demo_forum", ContainerLabelForm::Singular),
        container_label_key("lemmy", ContainerLabelForm::Singular)
    );
}

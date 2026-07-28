//! Guards the Linux desktop-shell dependency pins against manifest drift.
//!
//! `poly-desktop-web` shares the `tao` / `wry` / `webkit2gtk` / `cairo-rs`
//! closure with `poly-desktop-devtools`, `poly-host-sandbox` and (transitively,
//! via `dioxus-desktop`) `poly-desktop`. Those four versions must move as one
//! unit or the resolver either errors outright or silently lands two majors in
//! the lock — see `docs/plans/plan-desktop-gtk-stack-bump.md` Phase A.
//!
//! This crate previously carried a literal `tao = "0.34"` instead of the
//! workspace pin, which meant editing only `[workspace.dependencies]` would
//! have left this crate behind. Re-declaring any of these four with a literal
//! version re-opens that hole, so assert they stay workspace-inherited.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// The gtk-rs-generation dependencies that must be workspace-inherited.
const LOCKSTEP_DEPS: [&str; 4] = ["tao", "wry", "webkit2gtk", "cairo-rs"];

/// Returns the `[dependencies]` section body of `manifest`, or `None` when the
/// section is absent.
fn dependencies_section(manifest: &str) -> Option<&str> {
    let after = manifest.split_once("\n[dependencies]\n")?.1;
    // The section ends at the next top-level table header.
    Some(match after.split_once("\n[") {
        Some((body, _)) => body,
        None => after,
    })
}

/// Returns the declaration line for `name` within `section`, or `None`.
fn declaration<'a>(section: &'a str, name: &str) -> Option<&'a str> {
    section.lines().map(str::trim).find(|line| {
        line.strip_prefix(name)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

#[test]
fn lockstep_deps_are_workspace_inherited() {
    let manifest = include_str!("../Cargo.toml");
    let section = dependencies_section(manifest).expect("[dependencies] section present");

    for name in LOCKSTEP_DEPS {
        let line = declaration(section, name)
            .unwrap_or_else(|| panic!("`{name}` should be declared in [dependencies]"));
        assert!(
            line.contains("workspace = true"),
            "`{name}` must inherit the workspace pin (found `{line}`); a literal version here \
             drifts from [workspace.dependencies] and can land two majors in the lock — see \
             docs/plans/plan-desktop-gtk-stack-bump.md Phase A"
        );
    }
}

#[test]
fn declaration_lookup_does_not_match_name_prefixes() {
    // `tao` must not be satisfied by a `tao-macros = …` line, and `wry` must not
    // be satisfied by `wry-sandbox = …`.
    let section = "tao-macros = \"0.1\"\nwry-sandbox = { workspace = true }\n";
    assert!(declaration(section, "tao").is_none());
    assert!(declaration(section, "wry").is_none());

    // Whitespace around `=` is still a match.
    assert_eq!(
        declaration("tao   = { workspace = true }\n", "tao"),
        Some("tao   = { workspace = true }")
    );
}

#[test]
fn dependencies_section_stops_at_the_next_table() {
    let manifest = "\n[dependencies]\ntao = { workspace = true }\n\n[lints]\nworkspace = true\n";
    let section = dependencies_section(manifest).expect("section present");
    assert!(section.contains("tao = { workspace = true }"));
    assert!(!section.contains("lints"));
}

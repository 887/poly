# iOS — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28


---

## Priority 2 — Use Jujutsu (jj) Instead of Git

- **Always use `jj` commands** for version control, never raw `git`
- `jj status`, `jj diff`, `jj log`, `jj show` for inspection
- `jj new`, `jj describe`, `jj commit` for creating changes
- `jj git push` to push to remote
- Only fall back to `git` if `jj` cannot accomplish the task

---

## Purpose

iOS entry point for Poly. Uses Dioxus mobile with iOS-specific configuration. Also supports iPad.

## How It Works

- `main.rs` initializes Dioxus mobile and mounts `poly_core::App`
- All logic lives in `poly-core`
- Uses WKWebView internally
- SurrealDB with SurrealKV for local storage (CRITICAL: verify SurrealKV compiles for iOS)

## Development

```bash
dx serve --platform ios  # iOS Simulator
```

## Build

```bash
dx build --release --platform ios  # Build for iOS
```

## Configuration

- `Dioxus.toml` — platform: ios
- `Info.plist` — customizable via Dioxus.toml
- Capabilities: Push Notifications, Background Modes, Camera, Microphone
- Deployment target: iOS 15+ (TBD)

## Mobile UI Notes

- Same 3-panel swipe layout as Android
- Safe area insets for notch/Dynamic Island
- iPadOS: may use a wider layout (similar to desktop) when screen size allows
- Camera/mic: need iOS-specific bridges for WebRTC (Phase 3.1)

## Build Requirements

- macOS with Xcode installed
- Apple Developer account for device deployment
- CocoaPods or SPM for native dependencies (if any)

## Known Concerns

- SurrealKV compilation on iOS — test early
- WebRTC camera/mic needs native iOS bridges
- App Store review: Discord integration could be flagged
- Background execution limits on iOS

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.

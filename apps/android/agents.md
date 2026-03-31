# Android — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28


## Purpose

Android entry point for Poly. Uses Dioxus mobile with Android-specific configuration.

## How It Works

- `main.rs` initializes Dioxus mobile and mounts `poly_core::App`
- All logic lives in `poly-core`
- Uses Android WebView internally
- SurrealDB with SurrealKV for local storage (CRITICAL: verify SurrealKV compiles for Android)

## Development

```bash
dx serve --platform android              # Emulator
dx serve --platform android --device     # Real device (ADB)
```

Dioxus 0.7.3 supports ADB reverse proxy for real-device hot-reload.

## Build

```bash
dx build --release --platform android  # Build APK
```

## Configuration

- `Dioxus.toml` — platform: android
- `AndroidManifest.xml` — customizable via Dioxus.toml
- Permissions: INTERNET, RECORD_AUDIO (voice), CAMERA (video), WRITE_EXTERNAL_STORAGE
- Min SDK version: TBD (target Android 8.0+ / API 26+)

## Mobile UI Notes

- Uses 3-panel swipe layout (defined in poly-core mobile_layout.rs)
- Touch-friendly: larger tap targets, swipe gestures
- Camera/mic access: need platform-specific bridges for WebRTC (Phase 3.1)
- Notifications: Android push notification integration (Phase 3+)

## Known Concerns

- SurrealKV compilation on Android — test early
- WebRTC camera/mic needs native Android bridges
- Background service for persistent connections

## ABSOLUTE PROHIBITION — `#[allow(...)]` is FORBIDDEN

**NEVER** add `#[allow(clippy::...)]`, `#[allow(warnings)]`, or any other lint suppression
attribute to source code. When `cargo cranky` reports a violation, **fix the code**.

**The ONLY exception**: inside `#[cfg(test)]` modules, `#[allow(clippy::unwrap_used)]`
and `#[allow(clippy::expect_used)]` are permitted for test assertions — nothing else.

See root `agents.md` § 7a for the full rationale.

# iOS — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

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

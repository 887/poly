# apps/ios

**Poly** iOS app entry point using [Dioxus Mobile](https://dioxuslabs.com/learn/0.7/getting_started/mobile/).

## Purpose

Renders `poly-core`'s UI on iOS using Dioxus's mobile target, which wraps the UI in a `WKWebView` and communicates via Dioxus's native bridge.

## How It Works

1. Initialises i18n and theme systems from `poly-core`
2. Boots the Dioxus mobile runtime
3. Mounts the `App` component from `poly-core`

## Building

```bash
# Requires Xcode + cargo-xcodebuild
dx build --platform ios

# Or via cargo-mobile2
cargo mobile ios build
```

## Requirements

- macOS with Xcode installed
- iOS Simulator or a physical device with a valid provisioning profile
- `cargo-mobile2` or `dioxus-cli` with iOS support

## License

MIT / Apache-2.0

# apps/android

**Poly** Android app entry point using [Dioxus Mobile](https://dioxuslabs.com/learn/0.7/getting_started/mobile/).

## Purpose

Renders `poly-core`'s UI on Android using Dioxus's mobile target, which wraps the UI in an Android `WebView` and communicates via Dioxus's native bridge.

## How It Works

1. Initialises i18n and theme systems from `poly-core`
2. Boots the Dioxus mobile runtime
3. Mounts the `App` component from `poly-core`

## Building

```bash
# Requires Android NDK + cargo-ndk
dx build --platform android

# For a specific ABI
cargo ndk -t arm64-v8a build
```

## Requirements

- Android NDK (via Android Studio or standalone)
- `cargo-ndk` or `dioxus-cli` with Android support
- API level 21+ (Android 5.0 Lollipop)

## License

MIT / Apache-2.0

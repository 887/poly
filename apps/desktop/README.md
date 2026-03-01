# apps/desktop

The primary **Poly** desktop app entry point using [Dioxus](https://dioxuslabs.com/) with the **Wry** system-webview renderer.

## Purpose

Renders `poly-core`'s UI inside the OS-native webview (WebKit2GTK on Linux, WebView2 on Windows, WKWebView on macOS) via Dioxus Desktop. This is the recommended desktop distribution target — it ships no bundled browser engine, keeping the binary small.

## How It Works

1. Initialises i18n and theme systems from `poly-core`
2. Boots the Dioxus desktop runtime with a configured `dioxus::desktop::Config`
3. Mounts the `App` component from `poly-core` — all UI and state lives there

## Running

```bash
# Development (with hot-reload)
dx serve --platform desktop

# Production build
dx build --release --platform desktop
```

The built app lands in `target/dx/poly-desktop/`.

## Key Dependencies

| Crate | Role |
|---|---|
| `poly-core` | All UI, state, DB, i18n, theming |
| `dioxus` (`desktop` feature) | Wry-backed desktop runtime |

## Platform Notes

- **Linux**: requires WebKit2GTK (`libwebkit2gtk-4.1`)
- **Windows**: requires WebView2 runtime (ships with Windows 11+)
- **macOS**: WKWebView (built-in)

## License

MIT / Apache-2.0

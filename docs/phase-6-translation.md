# Phase 6 — On-Device Translation (Bergamot)

> **Status:** Planned  
> **Owner:** AI agent

---

## Goal

Add optional on-device message translation powered by Mozilla's Bergamot WASM engine.
No API keys, no external servers, no token burning. Works identically in Electron,
WebKit (Wry), and Chrome. User downloads the engine + language pair models on demand;
everything runs locally after that.

---

## Architecture

```
Settings › Translation
  ├── Engine: Not installed  [Download ~5MB]   ← bergamot-translator.wasm
  └── Language pairs:
        DE → EN  [↓ ~20MB]
        FR → EN  [✓ installed · 18MB]  [×]
        Storage used: 18 MB

In chat: right-click message → Translate  (if engine installed)
         shows translated text inline, toggleable back
```

### Components

| Component | What it does |
|-----------|-------------|
| `translation-worker.js` | Web Worker: loads Bergamot WASM, exposes `translate(text, from, to)`, handles model file management via OPFS |
| `TranslationSettings` (`settings/translation.rs`) | Engine download/status UI, language pair list, storage indicator |
| `SettingsSection::Translation` | New settings nav entry, between Language and Layout |
| `TranslationBridge` (eval shim) | Dioxus ↔ Worker bridge via `document::eval` — send text, receive result |
| FTL strings | `settings-translation-*` keys |

### Storage
- Models cached in **OPFS** (`navigator.storage.getDirectory()`)
- IndexedDB fallback for older WebKit versions (check `typeof FileSystemDirectoryHandle`)
- Engine WASM cached separately from model files

---

## Sources (all Apache 2.0)

- WASM engine: `unpkg.com/@mozilla/bergamot-translator` — ~5MB
- Models: `github.com/mozilla/firefox-translations-models` releases
  - Per language pair: `model.*.intgemm.alphas.bin` + `lex.*.bin` + `vocab.*.spm` ≈ 15–25MB

---

## Tasks

### 1. Settings section stub ✅
- [x] Add `Translation` variant to `SettingsSection` in `crates/core/src/state/mod.rs`
- [x] Add slug `"translation"` to `to_slug` / `from_slug`
- [x] Add to `NAV_SECTIONS` and `SETTINGS_NODES` in `settings/mod.rs`
- [x] Add scroll spy section ID `"settings-section-translation"`
- [x] Create `crates/core/src/ui/settings/translation.rs` — browser API probe + Bergamot stub UI
- [x] Add FTL strings (`settings-translation`, `settings-translation-description`, etc.)

### 2. Translation worker
- [ ] Create `crates/core/assets/js/translation-worker.js`
  - Load `bergamot-translator.wasm` from given URL
  - `downloadModel(from, to, urls)` — fetch model files into OPFS
  - `translate(text, from, to)` → string
  - `listInstalled()` → `[{from, to, sizeBytes}]`
  - `deleteModel(from, to)`
  - Postmessage-based API (request/response with `id` correlation)
- [ ] OPFS helper with IndexedDB fallback

### 3. Dioxus bridge
- [ ] `crates/core/src/translation.rs` — module with:
  - `ensure_worker_running()` — spawns the worker via eval if not already alive
  - `async fn translate(text: &str, from: &str, to: &str) -> Result<String>`
  - `async fn list_installed() -> Vec<InstalledPair>`
  - `async fn delete_model(from: &str, to: &str)`
- [ ] Bridge uses `document::eval` with a JS promise that routes through the worker

### 4. Full settings UI
- [ ] Replace placeholder with real `TranslationSettings` component:
  - Engine status + download button with progress bar
  - Language pair list (hardcoded to common pairs: DE↔EN, FR↔EN, ES↔EN, NL↔EN, PT↔EN, IT↔EN, PL↔EN, RU↔EN)
  - Per-pair: installed size + delete, or download button with progress
  - Total storage used

### 5. In-chat translate
- [ ] Right-click message context menu → "Translate" option (shown when engine installed)
- [ ] Translated text shown inline below original, toggle to hide
- [ ] Auto-detect source language via Bergamot's language detector

---

## Files

```
crates/core/src/state/mod.rs           — Translation variant
crates/core/src/ui/settings/mod.rs    — nav + scroll spy + match arm
crates/core/src/ui/settings/translation.rs  — settings UI
crates/core/src/translation.rs        — bridge module
crates/core/assets/js/translation-worker.js — Web Worker
locales/en/main.ftl                   — FTL strings
```

---

## Notes

- The worker is spawned lazily on first translate action, not at app startup.
- Model downloads are resumable via HTTP Range requests if the server supports it.
- OPFS quota: browsers typically allow 60%+ of available disk. Model files are small enough that quota should never be an issue in practice.
- Electron: OPFS works normally (Chromium). No special handling needed.
- Wry/Linux: depends on GTK WebKit2 version. OPFS landed in WebKit 235 (2022). Most modern distros ship ≥235. IndexedDB fallback handles older installs.

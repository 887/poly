# Plan — Client-Provided UI Surface: Context Menus, Settings, Sidebars, Views

> **Created:** 2026-04-18
> **Status:** 🟢 APPROVED (decisions locked — ready for implementation)
> **Scope:** WIT spec, `ClientBackend` trait, `crates/core/src/ui/account/*`, `clients/*`
> **Supersedes the scope of:** `docs/plans/plan-context-menu-quality-control.md` (narrowly structural), `docs/archive/phases/phase-2.20-plugin-capabilities-plan.md` (narrowly flags-only)
> **Builds on:** `plan-ui-completeness`, `plan-ui-action-types` (both ✅ DONE)
> **Execution model:** All work packages will be executed by AI coding agents in a single integrated pass. Work-package structure exists solely to partition the graph for scheduling parallel agents and to define integration milestones — not as time estimates.

---

## 0. The Rant In One Paragraph

Right-clicking a **Lemmy** community currently shows "Invite to Server", "Privacy Settings", "Edit Per-server Profile" — items that only make sense on Discord. Setting menus show voice-codec options on a Hacker News feed. The sidebar channel list uses Discord's `text/voice/forum` icon vocabulary for every backend. This is because **every piece of backend-specific UI is a Rust match statement in `poly-core`**, not something the backend itself declares. We built the `BackendCapabilities` struct for exactly this problem — **and then never read it from any UI call site** (phase-2.20 documents this as D6). We added per-backend context-menu-extras Rust files and then filled them with `todo!()`. The empty-RSX lint caught that; the author used the escape hatch (`rsx! { // comment }`) instead of actually fixing it. This plan is the real fix: a **client-owned UI surface** where the backend declares its own context-menu items, its own settings sections, and its own sidebar/view layouts.

---

## 1. Locked Decisions

Everything below is a binding decision. The rest of this document describes how to execute them.

| # | Decision | Value |
|---|---|---|
| D1 | Architecture model | **Option C** — declarative primitives + sanitized `custom-block` escape hatch. Plugins do **not** ship Dioxus / VNodes / HTML trees. |
| D2 | Rollout order | **Skip the quick-wins phase.** Go directly to the WIT surface landing. No gradual "read existing capabilities" WP — it becomes obsolete because host items move to plugins (D10). |
| D3 | MCP (social-agent) surface | **Included** as WP 8. Menu items and settings sections exposed as MCP tools; capability-driven tool filtering replaces phase-2.20 D4. |
| D4 | `custom-block` escape hatch | **Ship in WP 5.** Sanitized HTML (ammonia allowlist) in shadow-root, scoped CSS, no scripts. |
| D5 | Forum/channel-view variants | **Plugin declares full layout.** A `view-descriptor` WIT record with list-shape, sort/filter options, sub-tabs, item-row primitives. No closed enum. |
| D6 | Menu item ordering | **Named slots.** Host context menus expose slots (`top`, `after-favorites`, `before-leave`, `bottom`); each plugin item declares its slot. Host inserts separators between slots. |
| D7 | Per-backend Rust UI files | **Deleted** after WP 2 / WP 3 land. No deprecated-but-present files. The 5 `context_menu.rs` files, the `backend_server_context_menu_extras` dispatcher, and any per-backend settings Rust files are removed. |
| D8 | Chat view participation | **Included** as WP 6. Plugins declare composer-toolbar buttons and per-message action items via the same declarative pattern. `chat_view.rs` otherwise stays untouched. |
| D9 | Default for new WIT methods | **Compile-time required.** Every backend must implement every new surface before the landing lands. No `NotSupported` escape. No empty-list default. If a backend has nothing to contribute, it implements the method and returns an empty list explicitly — that explicit empty list is part of the review signal. |
| D10 | Host-item gating | **Move host items into plugin declarations.** "Invite", "Privacy Settings", "Per-server Profile", notification categories — all relocated from poly-core into the plugins that actually have those concepts. Only truly universal items stay host-owned: `Copy Server ID`, `Add/Remove Favorites`, `Leave/Remove Account`, `Mark All Read`. Everything else is plugin-declared. |
| D11 | WIT interface organization | **Split per surface.** Separate interfaces: `client-menus`, `client-settings` (extends `plugin-metadata`), `client-sidebar`, `client-views`, `client-composer`. Each is independently versionable. |
| D12 | Capability flags | **Minimal flags; trust plugin declarations.** Keep the existing `backend-capabilities` record lean (voice, video, dms, groups, presence, search, reactions, typing, attachments, create-server, create-channel, friends, notifications-model). Do **not** add fine-grained flags like `supports-invites`, `supports-per-server-profile`, `supports-privacy-settings` — those decisions come from whether the plugin declares the corresponding item. |
| D13 | Plugin UI cannot touch the DOM | Enforced by construction — plugins have no DOM capability exposed via `host-api`. `custom-block` is the only path for custom markup, and it's sanitized + shadow-rooted. |
| D14 | Action IDs | **Opaque strings.** Plugin-defined, host-opaque. Routing handled by `invoke-*-action` per surface. |
| D15 | Settings storage | **Plugin owns via host-api storage.** `get-setting-value` / `set-setting-value` route through the plugin, which persists via existing `host-api.kv-set` / `kv-get` against the per-account KV. Host never stores plugin setting values directly. |
| D16 | Optimistic UI / async actions | **`action-outcome::pending(pending-handle)` from V1.** Plus `poll-action(handle) -> result<action-outcome, client-error>`. Host shows spinner/toast while slow plugin calls run. |
| D17 | `backend-type` WIT enum | **Remove immediately in WP 1.** Every backend already returns a slug string via `get-backend-type`. Closed enum deleted. All routing/matching uses slugs. Aligns with Principle 3 (no slug matches in UI code) and forces any remaining consumer to use the plugin-provided string. |
| D18 | Legacy `get-settings-schema` | **Removed.** Folded into `client-settings::get-settings-sections` with `scope: account-global`. Plugins migrate during WP 1. |
| D19 | Sidebar refresh cadence | **Event-driven via `ClientEvent::SidebarInvalidated`.** Plugin emits on the existing event stream when its sidebar content changes (subscriptions, spaces, etc.). Host re-fetches `get-sidebar-declaration` on receive. No polling. |
| D20 | `navigate()` target construction | **Plugin never builds raw route strings.** Host exposes `host-api.build-route(kind: route-kind, params: list<tuple<string, string>>) -> string`. Plugin calls it, passes result to `action-outcome::navigate`. Host validates and pushes. Prevents malformed route strings; keeps plugin decoupled from Dioxus Route shape. |
| D21 | FTL label-key validation | **Build-time lint, fail the build.** A new lint-gate scanner (`ftl_label_key_coverage`) reads each plugin's declared label-keys from its WIT-exported declarations and its FTL bundle. Missing keys → `cargo::error`. Matches the existing `ui_action_coverage` / `context_menu_coverage` pattern. |
| D22 | Unknown plugin action ID | **`ClientError::NotFound`.** Plugin's `invoke-*-action` returns `Err(NotFound(action-id))` for unknown IDs. Host logs + shows an error toast + menu re-opens with fresh items. Never panics. |
| D23 | Paged-data cursor format | **Structured record: `record cursor { kind: cursor-kind, value: string }`** where `cursor-kind` is `offset | timestamp | id | opaque`. Covers HN offset, Lemmy timestamp, Matrix event-id, plus a catch-all for plugins that need something exotic. |
| D24 | Context menu caching | **Fresh fetch every menu open.** No host-side cache. `get-context-menu-items` called on every right-click. Plugin always returns current state (e.g. "Mute" vs "Unmute" based on live server state). WIT call cost is negligible. |
| D25 | Action ID naming convention | **`kebab-case`, no plugin prefix.** `invite-user`, `mute-server`, `open-in-browser`. Lint (D21 sibling) enforces kebab-case; plugin-scoped by construction. |
| D26 | Plugin menu-fetch error | **Host items + inline error row.** Menu renders universal host items + one disabled info row ("Plugin error: failed to load items"). Signals the problem visibly without breaking Copy ID / Favorites. Errors logged. |
| D27 | Icon field format | **Emoji string OR sanitized SVG path-string.** `icon: option<icon-source>`; `variant icon-source { emoji(string), svg(string) }`. SVG string passes through ammonia's SVG-aware sanitizer (allowlist: `path`, `g`, `circle`, `rect`, `polygon`, `polyline`, `line`, attrs: `d`, `fill`, `stroke`, `viewBox`, etc.); no `<script>`, no `<foreignObject>`, no event handlers. |
| D28 | Submenu nesting depth | **Unbounded.** No WIT-level limit. Plugin authors are trusted. Host enforces a soft rendering limit (e.g. 10 levels triggers a scroll hint) but does not reject. |
| D29 | Multi-account semantics | **Per-instance.** Each connected account runs its own plugin instance and answers `get-context-menu-items` / `get-settings-sections` / etc. independently. Items can differ per account (permissions, bot-vs-user, etc.). Matches existing plugin-instantiation model. |
| D30 | `action-outcome::refresh-target` | **Refetch just the target object.** For `menu-target-kind::server` → refetch that server. Not its channels, not members. Plugin can emit additional `ClientEvent`s if rippling state changes need to propagate. |

---

## 2. Current State Snapshot

| Surface | Where it lives | Backend discriminator | Will be |
|---|---|---|---|
| Common server context menu | `crates/core/src/ui/account/server/context_menu.rs` | none — same items for every backend | Host keeps **only** universal items (Copy ID, Favorites, Leave, Mark Read). Everything else (Invite, Privacy, Profile, Notif Settings) moves to plugins (D10). |
| Backend-specific context-menu extras | `crates/core/src/ui/account/{demo,stoat,discord,matrix,teams,poly_native}/context_menu.rs` + `backend_server_context_menu_extras` dispatcher | `match bt.as_str()` | All 5 files + dispatcher **deleted** (D7). Replaced by the WIT `client-menus` interface. |
| Settings — per-account | `crates/core/src/ui/settings/*` | none — global | Truly global settings (language, layout, theme, keyboard) stay in host. Backend-specific (notification filter, identity, privacy) move to plugin via `client-settings`. |
| Settings — per-server | `crates/core/src/ui/account/server/settings/*` | none | Moves entirely to plugin (D10). Plugin declares its server-scoped sections via `client-settings::get-settings-sections`. |
| Sidebar / channel list | `crates/core/src/ui/account/common/channel_list.rs` (59 KB) | `match channel_type` only | Host provides 5 stock layouts (`channel-list`, `spaces-rooms`, `communities`, `feed`, `repo-tree`) + plugin-provided custom sections (D5, via `client-sidebar`). |
| Chat view | `crates/core/src/ui/account/common/chat_view.rs` (223 KB) | backend-agnostic | **Untouched internally.** WP 6 adds declarative hooks for composer-toolbar buttons and per-message action items only. |
| Forum view | `crates/core/src/ui/account/common/forum_view.rs` | `match ChannelType::{Forum,HackerNews}` | Replaced by plugin-declared `view-descriptor` (D5). Host ships generic list/card renderers; plugin supplies the descriptor. |

### Only working precedent today — the architectural template we extend

`plugin-metadata.get-settings-schema` already does this pattern for top-level plugin settings:

```wit
record setting-descriptor {
    key: string,           // storage + FTL key
    kind: setting-kind,    // toggle | text-input | select | slider | info-label
    default-value: string, // JSON-encoded
    extra: string,         // JSON, e.g. select options or slider bounds
}
get-settings-schema: func() -> list<setting-descriptor>;
```

**Plugin declares → host renders with host components → storage is keyed.** No Dioxus, no HTML, no component passing. **This plan scales this pattern to every surface.**

---

## 3. Design Principles

1. **Plugin declares, host renders.** The plugin never emits Dioxus VNodes, never touches the DOM, never knows about CSS class names. The host owns presentation.
2. **Closed sets of primitives.** Every WIT surface addition is a versioned, documented primitive. No open-ended string → tree deserializers.
3. **Plugin declarations drive behavior, not slug matches.** No `match bt.as_str()` branches in UI code after this plan lands.
4. **FTL keys for every user-visible string.** Plugin returns keys, host looks up in the merged i18n bundle. `plugin-<id>-*` convention (wit line 747) extends to `plugin-<id>-menu-<key>-label`, `plugin-<id>-setting-<key>-label`, etc.
5. **Routing is the host's job.** Plugin action IDs are opaque strings (D14); host routes them back to the plugin via one entry point per surface.
6. **Nothing dynamic at runtime.** Plugin declarations are re-fetched each time a menu opens, a section page mounts, or the sidebar rebuilds. No live push.
7. **No fallbacks. Every backend implements every surface.** Per D9 (compile-time required). If a backend has no items for a surface, it returns an explicit empty list — and that empty list is reviewable evidence.
8. **Preserve the chat golden path.** The 223 KB `chat_view.rs` stays intact. WP 6 only adds declarative extension points.
9. **Custom-block is a last resort.** A lint-gate counter tracks `custom-block` usage across plugins; rising numbers signal a missing declarative primitive.

---

## 4. Architecture — The New WIT Surface

Five new interfaces (D11). Each plugin exports all five (D9).

### 4.1 `client-menus`

```wit
interface client-menus {
    use types.{client-error};

    enum menu-target-kind {
        server, channel, user, message, dm, category,
    }

    /// Named slots in host context menus. Plugin items declare which slot
    /// they occupy; host inserts separators between occupied slots.
    enum menu-slot {
        top,                 // above host items
        after-favorites,     // after Favorites block, before middle separators
        before-leave,        // above Leave/Remove
        bottom,              // below host items
    }

    enum menu-item-variant {
        normal,
        destructive,
        submenu-header,      // `submenu` field populated
        info-block,          // renders an optional custom-block; non-clickable
    }

    record menu-item {
        id: string,                        // opaque plugin action id (D14); kebab-case (D25)
        slot: menu-slot,                   // D6
        label-key: string,                 // FTL: plugin-<id>-menu-<key>-label (D21 lint-checked)
        icon: option<icon-source>,         // D27: emoji OR sanitized SVG
        variant: menu-item-variant,
        submenu: list<menu-item>,          // D28: unbounded depth
        shortcut: option<string>,
        block: option<custom-block>,       // only with variant=info-block (D4)
    }

    /// D27 — icon format. Host sanitizes SVG via ammonia SVG allowlist.
    variant icon-source {
        emoji(string),
        svg(string),
    }

    /// Host calls this every time the user opens a context menu.
    /// Returns ALL plugin items for that target; host merges with its
    /// universal items (Copy ID, Favorites, Leave, Mark Read — D10).
    get-context-menu-items: func(
        target: menu-target-kind,
        target-id: string,
    ) -> result<list<menu-item>, client-error>;

    variant action-outcome {
        noop,
        pending(pending-handle),           // D16: slow op; host polls via poll-action
        completed,                         // D16: done, no UI change
        refresh-target,                    // D30: re-fetch just the target
        refresh-sidebar,                   // host rebuilds the sidebar
        navigate(string),                  // D20: must be built via host-api.build-route
        toast(toast-payload),              // show FTL-keyed toast
        open-settings(settings-anchor),    // jump into a settings-section
        open-modal(modal-ref),             // future: plugin-declared modal
    }

    /// D16 — handle for a pending async action.
    record pending-handle {
        action-ref: string,                // plugin-opaque; passed back to poll-action
        progress-hint: option<string>,     // FTL key for "Saving…" / "Uploading…"
    }

    poll-action: func(handle: pending-handle) -> result<action-outcome, client-error>;

    record toast-payload {
        label-key: string,
        tone: toast-tone,
    }
    enum toast-tone { info, success, warning, error }

    record settings-anchor {
        scope: string,                     // "account-global" | "per-server" | ...
        scope-id: string,
        section-key: string,
    }

    record modal-ref {
        modal-id: string,
        context: string,                   // opaque JSON
    }

    invoke-context-action: func(
        action-id: string,
        target: menu-target-kind,
        target-id: string,
    ) -> result<action-outcome, client-error>;
}
```

### 4.2 `client-settings` (extends existing `plugin-metadata`)

```wit
interface client-settings {
    use types.{client-error};
    use plugin-metadata.{setting-descriptor};

    enum settings-scope {
        account-global,     // plugin's account settings panel
        per-server,
        per-channel,
        per-user,
    }

    record settings-section {
        scope: settings-scope,
        section-key: string,                // FTL + URL anchor
        icon: option<string>,
        fields: list<setting-descriptor>,   // existing record reused
        info-block: option<custom-block>,   // optional, per D4
    }

    get-settings-sections: func() -> list<settings-section>;

    /// D15 — plugin owns storage via host-api.kv-*. This function is the
    /// plugin's accessor; internally it reads from its own KV namespace.
    get-setting-value: func(
        scope: settings-scope,
        scope-id: string,                   // server-id, channel-id, user-id, or ""
        key: string,
    ) -> result<string, client-error>;      // JSON-encoded value

    set-setting-value: func(
        scope: settings-scope,
        scope-id: string,
        key: string,
        value: string,                      // JSON-encoded
    ) -> result<_, client-error>;
}
```

Per D18, the existing `plugin-metadata.get-settings-schema` is **removed**. Plugins that had a top-level schema now return it as one `settings-section { scope: account-global }` via the new interface. One surface, not two.

### 4.3 `client-sidebar`

Per D5, plugins can fully declare layout. The host ships 5 stock layouts but plugins can also go fully custom.

```wit
interface client-sidebar {
    use types.{client-error};

    enum sidebar-layout-kind {
        channel-list,       // Discord/Stoat/Teams
        spaces-rooms,       // Matrix
        communities,        // Lemmy/Reddit
        feed,               // HN/Mastodon
        repo-tree,          // GitHub/Forgejo
        custom,             // plugin supplies sections explicitly
    }

    record sidebar-declaration {
        layout: sidebar-layout-kind,
        sections: list<sidebar-section>,    // populated when layout=custom
        header-block: option<custom-block>, // optional info panel at top (D4)
    }

    record sidebar-section {
        header-key: option<string>,         // FTL; None for anonymous group
        collapsible: bool,
        default-collapsed: bool,
        items: list<sidebar-item>,
    }

    record sidebar-item {
        id: string,
        label-key: string,
        icon: option<string>,
        badge: option<string>,              // "new", "3"
        route-kind: sidebar-route-kind,
        children: list<sidebar-item>,       // nesting for trees
    }

    enum sidebar-route-kind {
        channel,            // opens chat-view
        forum,              // opens view-descriptor layout
        feed,               // opens view-descriptor layout
        code,               // opens code-browser
        issue-tracker,      // opens issue-list view
        modal,              // invokes sidebar-modal-action
        external,           // opens in system browser
        custom-view,        // plugin returns a view-descriptor
    }

    get-sidebar-declaration: func() -> result<sidebar-declaration, client-error>;

    invoke-sidebar-action: func(
        action-id: string,                  // item id; kebab-case (D25)
    ) -> result<action-outcome, client-error>;
}
```

Per D19, plugins emit **`ClientEvent::SidebarInvalidated`** on the existing event stream when their sidebar content changes. Host re-calls `get-sidebar-declaration` on receipt. No polling.

### 4.4 `client-views`

Per D5 — plugins declare **full layouts** for non-chat views. The host ships a rendering engine for a closed set of list/card primitives; the plugin composes them.

```wit
interface client-views {
    use types.{client-error};

    /// Describes a non-chat view of a channel (forum, feed, issue tracker, etc.).
    record view-descriptor {
        kind: view-kind,
        header: option<view-header>,
        toolbar: option<view-toolbar>,
        body: view-body,
    }

    enum view-kind {
        list,               // generic list (posts, issues, stories)
        card-grid,          // post-card grid (Reddit, Mastodon)
        tree,               // nested (Lemmy threaded)
        split,              // master-detail
    }

    record view-header {
        title-key: option<string>,
        subtitle-key: option<string>,
        info-block: option<custom-block>,
    }

    record view-toolbar {
        sort-options: list<toolbar-option>,     // Hot / Top / New / Best / ...
        filter-options: list<toolbar-option>,   // Week / Month / Year / All
        tabs: list<toolbar-option>,             // Issues / PRs / Discussions
        action-items: list<menu-item>,          // reuse from client-menus
    }

    record toolbar-option {
        id: string,
        label-key: string,
        icon: option<string>,
        default-selected: bool,
    }

    variant view-body {
        list-body(list-spec),
        card-body(card-spec),
        tree-body(tree-spec),
        split-body(split-spec),
    }

    record list-spec {
        row-template: row-template,
        page-size: u32,
    }

    record row-template {
        primary-field: string,              // e.g. "title"
        secondary-field: option<string>,    // e.g. "url" / "author"
        meta-field: option<string>,         // e.g. "score · comments · age"
        icon-field: option<string>,
    }

    /// Paged data feed. Host calls this on scroll.
    record view-row {
        id: string,
        primary-text: string,               // NOT a key — raw content
        secondary-text: option<string>,
        meta-text: option<string>,
        icon: option<string>,
        badge: option<string>,
        context-menu-target-kind: menu-target-kind,
    }

    record card-spec { /* analogous */  primary-field: string, }
    record tree-spec { /* threaded */   root-page-size: u32, max-depth: u32, }
    record split-spec { /* list + detail */ list: list-spec, detail-view-kind: view-kind, }

    get-channel-view: func(channel-id: string) -> result<view-descriptor, client-error>;

    get-view-rows: func(
        channel-id: string,
        cursor: option<cursor>,              // D23: structured cursor
        sort-id: option<string>,
        filter-id: option<string>,
        tab-id: option<string>,
    ) -> result<view-rows-page, client-error>;

    /// D23 — structured paged-data cursor.
    record cursor {
        kind: cursor-kind,
        value: string,
    }
    enum cursor-kind { offset, timestamp, id, opaque }

    record view-rows-page {
        rows: list<view-row>,
        next-cursor: option<cursor>,
    }

    /// Detail for split views.
    get-view-detail: func(channel-id: string, row-id: string)
        -> result<view-detail, client-error>;

    record view-detail {
        body-block: custom-block,           // plugin authored content, sanitized
        comments-section: option<tree-spec>,
    }
}
```

This lets Lemmy declare a `tree` view with Hot/Top/New sort and threaded comments. HN declares a `list` view with `primary=title, secondary=url, meta="score · comments · age"`. GitHub declares a `split` with `list + detail`, tab `Issues / PRs / Discussions`. Each backend owns its shape; the host renders consistently.

### 4.5 `client-composer`

Per D8 — plugin-declared composer buttons and per-message action items. Everything else in `chat_view.rs` stays host-owned.

```wit
interface client-composer {
    use types.{client-error};
    use client-menus.{menu-item, action-outcome};

    record composer-button {
        id: string,
        label-key: string,
        icon: string,
        position: composer-slot,
    }

    enum composer-slot {
        left-of-input,      // file upload / attach
        right-of-input,     // send / emoji / voice
        above-input,        // sticker drawer toggle
    }

    get-composer-buttons: func(channel-id: string) -> list<composer-button>;

    /// Per-message actions — merged into the message hover/overflow menu.
    get-message-actions: func(channel-id: string, message-id: string)
        -> list<menu-item>;

    invoke-composer-action: func(
        action-id: string,
        channel-id: string,
    ) -> result<action-outcome, client-error>;

    invoke-message-action: func(
        action-id: string,
        channel-id: string,
        message-id: string,
    ) -> result<action-outcome, client-error>;
}
```

### 4.6 Shared `custom-block` primitive (D4)

Used by `menu-item`, `settings-section`, `sidebar-declaration::header-block`, `view-header`, `view-detail::body-block`.

```wit
interface client-ui-common {
    /// Sanitized HTML rendered in a shadow-root by the host.
    /// Plugin-supplied CSS is inlined into the shadow-root; no leakage.
    /// Allowlist: p, span, div, strong, em, a(href), ul, ol, li, img(src),
    /// br, table/thead/tbody/tr/td/th, pre, code, blockquote, h1-h6.
    /// Strictly forbidden: script, style (outside stylesheet field),
    /// iframe, form, input, event handlers, javascript: URLs.
    /// Host uses the `ammonia` crate (already a dep of poly-core).
    record custom-block {
        sanitized-html: string,
        stylesheet: option<string>,
        max-height-px: option<u32>,
    }
}
```

### 4.7 World wiring

```wit
world messenger-plugin {
    import host-api;                       // now includes build-route (D20)

    // required core:
    export messenger-client;               // D17: backend-type enum removed;
                                            //       get-backend-type now returns string (slug)

    // REQUIRED (D9 — compile-time required):
    export client-menus;
    export client-settings;                // D18: absorbs the old get-settings-schema
    export client-sidebar;
    export client-views;
    export client-composer;

    // unchanged except:
    export plugin-metadata;                // D18: get-settings-schema REMOVED from here
}
```

Per D18, `plugin-metadata.get-settings-schema` is removed. Per D17, the `backend-type` enum in `types` is removed; `messenger-client.get-backend-type` returns a slug string. Plugins that don't have items for a surface return an empty list explicitly (D9).

### 4.8 `host-api` additions (D20)

```wit
// In host-api interface:
enum route-kind {
    server-home, channel, dm, friends, notifications, settings-account,
    settings-server, settings-channel, voice, search,
    // plus plugin-declared sidebar item routes
}

/// D20 — Plugin builds all route strings through this host helper.
/// Host validates params and returns a well-formed route the plugin
/// then passes to action-outcome::navigate(...).
build-route: func(kind: route-kind, params: list<tuple<string, string>>)
    -> result<string, route-build-error>;

variant route-build-error {
    unknown-kind,
    missing-param(string),
    invalid-param(string),
}
```

---

## 5. `ClientBackend` Trait Evolution

Every WIT export becomes a trait method. With D9 (compile-time required), there are no default impls with stub bodies — removing the defaults forces every `impl ClientBackend for …` block to add the new methods before the workspace compiles. The 10 backend implementations migrate in lockstep within WP 1.

---

## 6. Host Components — New Code in poly-core

Two generic components subsume the 5 deleted per-backend Rust files + all the scattered settings files:

### 6.1 `ClientMenu<Target>`

Lives in `crates/core/src/ui/client_ui/menu.rs`. Consumes `Vec<MenuItem>`, renders using the existing `ContextMenuItem` / `ContextMenuToggle` / `ContextMenuSeparator` primitives, groups by `slot`, inserts separators between slots, dispatches `invoke-context-action` on click.

Used by `ServerContextMenu`, `ChannelContextMenu`, `UserContextMenu`, `MessageContextMenu`, `DmContextMenu`, `CategoryContextMenu`.

### 6.2 `PluginSettingsSection`

Lives in `crates/core/src/ui/client_ui/settings_section.rs`. Generalizes the existing top-level-settings-schema renderer to accept any `SettingsSection` (with scope + section-key). Reads/writes via `get-setting-value` / `set-setting-value`. Renders the same field widgets as today.

### 6.3 `ClientSidebar`

Lives in `crates/core/src/ui/client_ui/sidebar.rs`. Dispatches to one of:
- `ChannelListLayout` (existing code moved out of `channel_list.rs`)
- `SpacesRoomsLayout`
- `CommunitiesLayout`
- `FeedLayout`
- `RepoTreeLayout`
- `CustomSidebar` (renders a declared `sidebar-declaration` with custom sections)

### 6.4 `ClientView`

Lives in `crates/core/src/ui/client_ui/view.rs`. Consumes `view-descriptor`, renders header + toolbar + body. Body engines:
- `ListBody` (paged list with `row-template`)
- `CardBody` (grid of cards)
- `TreeBody` (threaded list with collapse/expand)
- `SplitBody` (master-detail)

### 6.5 `ComposerHooks`

Integrated into existing `chat_view.rs` at two points only: the composer toolbar and the per-message action menu. Everything else in `chat_view.rs` remains untouched (D8 preservation).

### 6.6 `CustomBlock`

Lives in `crates/core/src/ui/client_ui/custom_block.rs`. Sanitizes via `ammonia`, renders in shadow-root, inlines scoped CSS. Applies `max-height-px` with overflow handling.

---

## 7. Work Packages

Sequenced for agent scheduling. WP N ≥ 1 assumes WP 1 has landed. WPs 2–6 can run in parallel after WP 1. WP 7 (cleanup) is last.

### WP 0 — Baseline + snapshot harness

**Purpose:** measurable before/after. No code change to production yet.

- Enumerate every `match .. as_str()` on backend slug in `crates/core/src/ui/` — deliverable: `docs/plans/client-ui-surface-defects.csv`.
- Playwright snapshot tests that right-click a server on each backend and capture the menu DOM. These become regression checks.
- Inventory every `crates/core/src/ui/account/*/context_menu.rs` and per-backend settings file slated for deletion in WP 7.

### WP 1 — Foundation: WIT surface + trait migration

**Purpose:** land the five new interfaces and force every backend to implement them.

- Add `client-menus`, `client-settings`, `client-sidebar`, `client-views`, `client-composer`, `client-ui-common` to `wit/messenger-plugin.wit`. Update the `messenger-plugin` world to export all five (D11).
- **Remove** the `backend-type` enum from `types` (D17). Change `messenger-client.get-backend-type` to return `string` (slug).
- **Remove** `plugin-metadata.get-settings-schema` (D18). Migrate existing plugin schemas into `client-settings::get-settings-sections` with `scope: account-global`.
- **Add** `host-api.build-route` + `route-kind` + `route-build-error` (D20).
- **Add** `ClientEvent::SidebarInvalidated` to the event variant (D19).
- Regenerate WIT bindings.
- Add corresponding methods to the `ClientBackend` trait **without default implementations** (D9). The workspace stops compiling.
- Each backend crate (`clients/{demo,stoat,discord,matrix,teams,server-client,lemmy,hackernews,github,forgejo}`) implements all new methods. Empty lists where the backend has nothing to contribute; explicit declarations otherwise.
- Write the six host components (§6) as skeletons — wired up to the new trait, rendering nothing yet.
- Write two new lint-gate scanners (both in `crates/lint-gate/build/`):
  - `ftl_label_key_coverage` — fails if any declared `label-key` is missing from the plugin's FTL bundle (D21).
  - `action_id_naming` — fails if any declared action `id` isn't kebab-case (D25).
- Host keeps rendering the existing hardcoded menus/settings (the new components aren't yet called from anywhere in the tree).

**Exit:** workspace compiles; tests pass; new plugin declarations visible via `MCP: backend.get_context_menu_items(…)` (but unused by UI). Snapshot tests unchanged. `backend-type` enum gone; `get-settings-schema` gone; `build-route` wired.

### WP 2 — Context menu rollout

**Purpose:** plugins own their menu items; per-backend Rust files deleted.

- Wire `ClientMenu` into `ServerContextMenu`, `ChannelContextMenu`, `UserContextMenu`, `MessageContextMenu`, `DmContextMenu`, `CategoryContextMenu`.
- Merge: host-universal items (Copy ID, Favorites, Leave, Mark Read — D10) + plugin-declared items grouped by `slot` (D6).
- Per D24: fetch fresh on every open; no caching.
- Per D22: unknown action IDs return `NotFound`; host shows toast + reopens.
- Per D26: plugin fetch error → host items + inline error info-block.
- Each backend populates its menu items. Lemmy: Subscribe/Unsubscribe/Block Community. Discord: Invite, Privacy, Per-server Profile, Manage Bots (sub). Matrix: Space Settings, E2EE Verification. HN: nothing (explicit empty list). GitHub: "Open in Browser", "Star", "Watch".
- Per-instance dispatch (D29): every connected account calls its own plugin instance; items can differ.
- Delete the 5 per-backend `context_menu.rs` files and the `backend_server_context_menu_extras` dispatcher in `crates/core/src/ui/account/mod.rs` (D7).
- MCP smoke-tests exercise right-click on each target-kind on each backend.

**Exit:** the D7 files are gone. Snapshot tests now show per-backend menu content. The per-backend `BackendType` match in `mod.rs` is deleted.

### WP 3 — Settings sections rollout

**Purpose:** plugins own per-server / per-account / per-channel settings.

- Wire `PluginSettingsSection` into `SettingsPage` and `ServerSettingsPage`, `ChannelSettingsPage`.
- Each backend populates its sections. Discord: Per-server Profile, Notification Rules, Privacy. Lemmy: Community Mute, Block Community. HN: nothing (explicit empty list). GitHub: Per-repo Notification Settings.
- Host retains only truly global settings (language, layout, theme, keyboard).
- Delete per-backend Rust settings files slated in WP 0 (D7).

**Exit:** server-settings panel for Lemmy shows Lemmy-relevant options; for Discord shows Discord-relevant options. No backend-specific Rust settings files remain.

### WP 4 — Sidebar rollout

**Purpose:** backend-shaped navigation.

- Wire `ClientSidebar` with the 5 stock layout components + `CustomSidebar`.
- Each backend declares its layout. Discord/Stoat/Teams/poly-native → `channel-list`. Matrix → `spaces-rooms`. Lemmy → `communities`. HN/Mastodon → `feed`. GitHub/Forgejo → `repo-tree`. Backends with unusual shapes → `custom` + `sidebar-declaration.sections`.
- Move `channel_list.rs` (59 KB) → `crates/core/src/ui/client_ui/sidebar/layouts/channel_list.rs`. Similar for the other 4 layouts.
- `invoke-sidebar-action` routing wired through.

**Exit:** Matrix account shows spaces then rooms. Lemmy account shows subscribed communities. HN shows `Top/New/Best/Ask/Show/Jobs` feed tabs.

### WP 5 — View rollout (forum + feed + issue-tracker + custom-block)

**Purpose:** non-chat views are backend-declared; custom-block ships.

- Wire `ClientView` with the four body engines (`list`, `card-grid`, `tree`, `split`).
- Each forum/feed/issue backend declares its `view-descriptor`. Lemmy → `tree` with Hot/Top/New sort. HN → `list` with `score · comments · age` meta. GitHub → `split` with `Issues/PRs/Discussions` tabs.
- Paged data via `get-view-rows`.
- Delete `crates/core/src/ui/account/common/forum_view.rs` (the `HnFeedView` / `LemmyForumView` branching). Replaced entirely by `ClientView`.
- Ship `CustomBlock` component (D4). Sanitization path + scoped CSS in shadow-root. Applied in: view headers, view detail bodies, sidebar headers, settings info panels, menu info-block items.
- Lint-gate counter: `custom-block` usage per plugin.

**Exit:** GitHub issues render as an issue list. Lemmy renders threaded comments. Custom-block in use for at least one plugin (Lemmy vote-summary or GitHub CI-badge row) to prove the pattern.

### WP 6 — Chat composer hooks

**Purpose:** plugin-contributed composer buttons and per-message actions.

- Wire `client-composer` into `chat_view.rs` at two points (composer toolbar slot list, per-message action menu).
- Each backend declares its composer/message items. Discord: stickers button. Matrix: `/me` action. Lemmy: upvote/downvote inline.
- `chat_view.rs` remains untouched otherwise (D8 preservation).

**Exit:** composer toolbar in `chat_view.rs` has plugin-contributed buttons; per-message menu has plugin items. Snapshot tests confirm no regressions on chat rendering.

### WP 7 — Cleanup + deprecation pass

**Purpose:** physically remove everything the above packages replaced.

- Delete `crates/core/src/ui/account/{demo,stoat,discord,matrix,teams,poly_native}/context_menu.rs` (if not already gone in WP 2).
- Delete `backend_server_context_menu_extras` in `crates/core/src/ui/account/mod.rs`.
- Delete the per-backend `ServerContextMenuExtras` dispatch path entirely.
- Delete `crates/core/src/ui/account/common/forum_view.rs` branching logic (if not already gone in WP 5).
- Audit for any remaining `match bt.as_str()` on backend slug in UI code; flag any survivors as WP-8 candidates or as new defects.
- Retire the `DECISION(D20)` note in the codebase.
- Remove dead capability flags from `BackendCapabilities` that D12 obsoletes.

**Exit:** repo grep for `bt.as_str()` returns zero UI call sites.

### WP 8 — MCP integration (per D3)

**Purpose:** the AI agent (MCP social-agent) can right-click programmatically.

- Expose `client-menus::get-context-menu-items` as MCP tools per-target-kind. Tool name: `context_menu_<target>`. Argument: `target_id`.
- Expose `invoke-context-action` as MCP tool: `invoke_context_action`.
- Expose `client-settings::get-settings-sections` and `get-setting-value` / `set-setting-value` as MCP tools.
- Capability-driven tool filtering (the phase-2.20 D4 fix): the MCP only advertises tools whose backing WIT call returns non-empty on the current account.
- Drop the old tool set that was Discord-shaped; it's now fully declarative.

**Exit:** `mcp/chat-mcp/src/tools.rs` no longer has hardcoded Discord-shaped tools. Agent can enumerate right-click-able actions on any backend.

### WP 9 — Documentation + plugin authoring guide

- `docs/plugin-authoring.md` — the client-UI surface, how to declare items, FTL conventions, action-id rules.
- Example walkthrough: "Adding a 'View on GitHub' menu item in N lines."
- Deprecation notices retired.
- Architecture diagram updated — `docs/1-architecture/1.0-overview.md`.

---

## 8. What We Are NOT Doing

- **Not shipping Dioxus in plugins** (D1).
- **Not serializing VNode trees over WIT.** The `custom-block` escape hatch is sanitized HTML only; it cannot carry interactive components (D1).
- **Not implementing dynamic plugin loading yet.** Plugins remain compiled-in cdylibs. The client-UI surface works the same way when dynamic loading arrives.
- **Not rewriting `chat_view.rs`.** Its 223 KB are our best asset. WP 6 only adds two extension points (D8).
- **Not keeping `BackendCapabilities` flag-by-flag matching with plugin declarations.** D12 minimizes the flag set; plugin declarations are the source of truth.
- **Not letting plugins directly touch the DOM** (D13).
- **Not building a "plugin-to-plugin" API.** Plugins talk only to the host.
- **Not supporting a `NotSupported` return for the new methods.** Compile-time required (D9).

---

## 9. Risks and Mitigations

- **WIT version churn.** Every new primitive is a minor version bump. Mitigation: D11 split (per-surface interfaces) — a plugin only re-binds the surfaces that changed.
- **`custom-block` abuse.** Plugin authors jam everything into sanitized HTML. Mitigation: WP 5 ships the lint-gate counter; rising usage triggers a review for a missing declarative primitive.
- **Lemmy/HN UX parity pressure.** Real users compare Poly's Lemmy UI to lemmy.ml's. Mitigation: the `view-descriptor` from D5 lets plugins express richer layouts than a closed enum would; plugin authors have room.
- **One giant PR from WP 1.** Compile-time required (D9) means every backend changes in lockstep. Mitigation: a single agent run does all 10 backends at once; splitting into separate PRs doesn't work because the workspace doesn't compile in between.
- **Double menus during migration.** Host items currently hardcoded + plugin items arriving. Mitigation: D10 — host items that overlap (Invite, Privacy, Profile) move to plugins *in the same WP* they land on the plugin side. No overlap window.
- **`ammonia` allowlist surface.** A bad allowlist is an XSS vector. Mitigation: WP 5 includes a security audit of the allowlist with explicit deny-list for `javascript:` URLs, data URLs in `<a href>`, `<style>` outside the stylesheet field.

---

## 10. Open Questions (non-blocking — resolve during execution)

All major architectural questions are locked in §1. The following are implementation-level details agents can resolve during execution:

1. **`custom-block` accessibility.** Plugin-authored HTML needs ARIA. Document the convention in WP 9; no WIT change needed.
2. **Version-negotiation for backends pinned to old WIT.** Deferred — all backends are compiled-in for now, so versioning is a build-time concern not a runtime one.
3. **SVG icon sanitizer allowlist exact scope.** Lock during WP 1 security review; starting point in D27.
4. **`host-api.build-route` parameter conventions.** Kebab-case keys; URL-encode values. Detail in WP 1.
5. **Lint threshold for `custom-block` abuse.** TBD in WP 5.

---

## 11. Relationship to Existing Plans

| Plan | Status | How it relates |
|---|---|---|
| `plan-ui-completeness` | ✅ DONE | Structural: no empty handlers. This plan fills those handlers with real dispatches. |
| `plan-ui-action-types` | ✅ DONE | Structural: typed action enums. `invoke-*-action` on the host side becomes an action enum whose variants carry the plugin action ID as payload. |
| `plan-context-menu-quality-control` | ✅ DONE (narrowly structural) | Ensured every component has `#[context_menu(...)]`. This plan supersedes the *content* of the per-backend files: no more per-backend Rust files. |
| `plan-connected-routes-static-check` | ✅ DONE | Routes stay typed. Plugin-declared navigation uses typed routes via `action-outcome::navigate(string)` validated against the route registry at dispatch time. |
| `plan-component-lints` | ✅ DONE (150-line rule) | `ClientMenu`, `PluginSettingsSection`, `ClientSidebar`, `ClientView`, `CustomBlock` all stay small. |
| `phase-2.20-plugin-capabilities-plan` | 🟡 Superseded | This plan subsumes phase-2.20. The defect list (D1–D11) maps: D1/D2/D3 → WP 3+ (plugin-declared settings/menu items replace hardcoded Discord shape); D4 → WP 8; D5 → WP 4 (plugin-declared sidebar gates unsupported routes); D6 → D12 (capabilities trimmed, not expanded); D7–D11 → covered by plugin declarations. |

---

## 12. Execution Summary

For the agent runner: land in this order, each work package integrating before the next begins unless explicitly noted as parallelizable.

```
WP 0  → baseline harness (no production change)
WP 1  → foundation (one big change; workspace-wide)
WP 2  → context menus         ┐
WP 3  → settings sections     │
WP 4  → sidebar               ├── parallelizable after WP 1
WP 5  → views + custom-block  │
WP 6  → composer hooks        ┘
WP 7  → cleanup (after WP 2–6 complete)
WP 8  → MCP integration
WP 9  → docs
```

Agents may be scheduled as:
- 1 agent for WP 0
- 1 agent for WP 1 (single transaction — workspace must compile at exit)
- 5 parallel agents for WP 2–6 (different files; occasional rebase needed)
- 1 agent for WP 7
- 1 agent for WP 8
- 1 agent for WP 9

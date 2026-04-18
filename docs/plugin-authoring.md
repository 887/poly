# Plugin Authoring Guide — Client UI Surface

> Full decision record: [`docs/plans/plan-client-ui-surface.md`](plans/plan-client-ui-surface.md)

---

## What Is the Client UI Surface?

The client UI surface is the collection of five WIT interfaces that every Poly plugin
exports. These interfaces let a backend plugin declare its own context-menu items,
settings sections, sidebar layout, non-chat views, and composer buttons — rather than
having the host hard-code them in a `match backend_type` branch.

Before this surface existed, the host contained per-backend Rust files that returned
Discord-shaped context menus for every backend. Right-clicking a Lemmy community showed
"Invite to Server" and "Privacy Settings". The client UI surface eliminates that problem:
the plugin declares what belongs in its menus, settings, sidebar, views, and composer.
The host renders using its own components — plugins never emit Dioxus VNodes or touch
the DOM directly.

The surface is defined in `wit/messenger-plugin.wit`. The corresponding Rust mirror types
live in `clients/client/src/ui_surface.rs` under `poly_client::ui_surface`. The world
export for all five interfaces is required at compile time (decision D9 — there are no
`NotSupported` escapes; a backend with no items returns an explicit empty list).

---

## The Five WIT Interfaces

### 1. `client-menus`

Plugins declare right-click menu items for six target kinds: `Server`, `Channel`, `User`,
`Message`, `Dm`, and `Category`. The host calls `get-context-menu-items` fresh on every
menu open (D24 — no caching) and merges the result with its own universal items (Copy
ID, Favorites, Leave, Mark All Read). Plugin items are grouped into named slots (`top`,
`after-favorites`, `before-leave`, `bottom` — D6); the host inserts separators between
occupied slots. The host calls `invoke-context-action` when the user clicks an item.

```rust
// Minimal server context menu — clients/discord/src/lib.rs (excerpt)
async fn get_context_menu_items(
    &self, target: MenuTargetKind, _target_id: &str,
) -> Result<Vec<MenuItem>, ClientError> {
    match target {
        MenuTargetKind::Server => Ok(vec![
            MenuItem {
                id: "invite-people".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-discord-menu-invite-people-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
        ]),
        _ => Ok(Vec::new()),
    }
}

async fn invoke_context_action(
    &self, action_id: &str, _target: MenuTargetKind, _target_id: &str,
) -> Result<ActionOutcome, ClientError> {
    match action_id {
        "invite-people" => Ok(ActionOutcome::Noop),
        other => Err(ClientError::NotFound(format!("unknown action: {other}"))),
    }
}
```

Key rules:
- `id` must be kebab-case (D25) — `invite-people`, not `invite_people` or `InvitePeople`.
- `label_key` must match a key in the plugin's FTL bundle; build fails otherwise (D21).
- Unknown `action_id` must return `ClientError::NotFound` — never panic (D22).
- Submenus: set `item_variant: MenuItemVariant::SubmenuHeader` on the parent and
  `parent_id: Some("parent-id")` on each child. Nesting depth is unbounded (D28).

---

### 2. `client-settings`

Plugins declare settings sections with scope, fields, and optional info panels. Each
section maps to one page in the host's settings UI. Scopes: `AccountGlobal`, `PerServer`,
`PerChannel`, `PerUser`.

This interface absorbs the old `get-settings-schema` surface (D18 — that method is
removed). Plugin storage is owned by the plugin: reads and writes route through
`get_setting_value` / `set_setting_value` which must internally call the host-api KV
store.

```rust
// Minimal settings declaration — clients/lemmy/src/lib.rs (excerpt)
async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
    Ok(vec![SettingsSection {
        scope: SettingsScope::PerServer,
        section_key: "community".to_string(),
        icon: None,
        fields: vec![
            SettingDescriptor {
                key: "mute-community".to_string(),
                kind: SettingKind::Toggle,
                default_value: "false".to_string(),
                extra: String::new(),
            },
            SettingDescriptor {
                key: "show-nsfw".to_string(),
                kind: SettingKind::Toggle,
                default_value: "false".to_string(),
                extra: String::new(),
            },
        ],
        info_block: None,
    }])
}
```

Field `extra` encoding by kind:

| `SettingKind` | `extra` value |
|---|---|
| `Toggle`, `TextInput`, `InfoLabel` | `""` (empty) |
| `Select` | JSON array: `["option-a","option-b"]` |
| `Slider` | JSON object: `{"min":0,"max":100,"step":1}` |

`default_value` is always JSON-encoded (`"true"`, `"\"option-a\""`, `"42"`).

---

### 3. `client-sidebar`

Plugins declare a top-level layout kind and, optionally, explicit custom sections. The
host ships five stock layouts: `ChannelList`, `SpacesRooms`, `Communities`, `Feed`,
`RepoTree`. For most backends one of these is exact.

The host re-calls `get_sidebar_declaration` whenever the plugin emits a
`ClientEvent::SidebarInvalidated` on its event stream (D19 — event-driven, no polling).

```rust
// Discord: standard channel-list layout — clients/discord/src/lib.rs (excerpt)
async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
    Ok(SidebarDeclaration {
        layout: SidebarLayoutKind::ChannelList,
        sections: Vec::new(),
        header_block: None,
    })
}

// Lemmy: community list layout — clients/lemmy/src/lib.rs (excerpt)
async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
    Ok(SidebarDeclaration {
        layout: SidebarLayoutKind::Communities,
        sections: Vec::new(),
        header_block: None,
    })
}
```

Layout-to-backend canonical mapping:

| Layout | Backends |
|---|---|
| `ChannelList` | Discord, Stoat, Teams, poly-native |
| `SpacesRooms` | Matrix |
| `Communities` | Lemmy |
| `Feed` | HackerNews, Mastodon |
| `RepoTree` | GitHub, Forgejo |
| `Custom` | anything that needs explicit sections |

---

### 4. `client-views`

Plugins declare full non-chat view layouts: feed views, forum threads, issue lists, and
code repositories. The host renders using four body engines (`FlatList`, `CardGrid`,
`Tree`, `Split`). The plugin composes these primitives; it does not ship a renderer.

```rust
// Lemmy: threaded post view with Hot/New/Top sort — clients/lemmy/src/lib.rs (excerpt)
async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
    Ok(ViewDescriptor {
        kind: ViewKind::Tree,
        header: Some(ViewHeader {
            title_key: Some("plugin-lemmy-view-posts-title".to_string()),
            subtitle_key: None,
            info_block: None,
        }),
        toolbar: Some(ViewToolbar {
            sort_options: vec![
                ToolbarOption {
                    id: "hot".to_string(),
                    label_key: "plugin-lemmy-sort-hot".to_string(),
                    icon: None,
                    default_selected: true,
                },
                ToolbarOption {
                    id: "new".to_string(),
                    label_key: "plugin-lemmy-sort-new".to_string(),
                    icon: None,
                    default_selected: false,
                },
                ToolbarOption {
                    id: "top".to_string(),
                    label_key: "plugin-lemmy-sort-top".to_string(),
                    icon: None,
                    default_selected: false,
                },
            ],
            filter_options: vec![],
            tabs: vec![],
            action_items: vec![],
        }),
        body: ViewBody::TreeBody(TreeSpec {
            root_page_size: 25,
            max_depth: 8,
        }),
    })
}
```

`get_view_rows` is called by the host as the user scrolls. It receives a structured
cursor (D23) that the plugin echoes back as `next_cursor` in the response. Row fields
(`primary_text`, `secondary_text`, `meta_text`) are raw content strings — not FTL keys.

---

### 5. `client-composer`

Plugins contribute buttons to the message composer toolbar and additional items to the
per-message action menu. Everything else in `chat_view.rs` stays host-owned (D8).

```rust
// Discord: sticker button in the right-of-input slot — clients/discord/src/lib.rs (excerpt)
async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
    Ok(vec![ComposerButton {
        id: "stickers".to_string(),
        label_key: "plugin-discord-composer-stickers-label".to_string(),
        icon: "🎨".to_string(),
        position: ComposerSlot::RightOfInput,
    }])
}

// Lemmy: no composer (read/vote platform) — clients/lemmy/src/lib.rs (excerpt)
async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
    Ok(Vec::new())   // explicit empty list — not an error
}
```

Per-message actions follow the same `MenuItem` pattern as context menus. See
`get_message_actions` in `clients/lemmy/src/lib.rs` for upvote/downvote/report
declarations.

---

## FTL Key Conventions (D21)

Every user-visible string a plugin returns must be a key in the plugin's FTL bundle
(`clients/<name>/locales/en/plugin.ftl`). The build-time lint scanner
`ftl_label_key_coverage` fails the build if any declared key is absent.

| Context | Key pattern | Example |
|---|---|---|
| Menu item label | `plugin-<id>-menu-<key>-label` | `plugin-discord-menu-invite-people-label` |
| Settings section label | `plugin-<id>-setting-<section-key>-label` | `plugin-lemmy-setting-community-label` |
| Settings field label | `plugin-<id>-setting-<field-key>-label` | `plugin-lemmy-setting-mute-community-label` |
| Settings field description | `plugin-<id>-setting-<field-key>-desc` | `plugin-discord-setting-nickname-desc` |
| Sidebar section header | `plugin-<id>-sidebar-<key>-header` | `plugin-matrix-sidebar-spaces-header` |
| View title | `plugin-<id>-view-<key>-title` | `plugin-lemmy-view-posts-title` |
| Toolbar option | `plugin-<id>-sort-<id>` | `plugin-lemmy-sort-hot` |
| Composer button | `plugin-<id>-composer-<key>-label` | `plugin-discord-composer-stickers-label` |
| Toast | `plugin-<id>-toast-<key>` | `plugin-discord-toast-invite-sent` |

`<id>` is the backend slug returned by `backend_type()` — `discord`, `lemmy`, `matrix`,
etc. The FTL scanner reads this value at build time.

---

## Action ID Rules (D14, D22, D25)

- Action IDs are **kebab-case** only: `invite-people`, `block-community`.
  Snake_case, camelCase, and PascalCase all fail the `action_id_naming` lint.
- They are **opaque strings** from the host's perspective. The host passes them back
  verbatim to `invoke_context_action` / `invoke_composer_action` / `invoke_message_action`.
- **Unknown IDs must return `ClientError::NotFound`**, not panic and not silently no-op
  (D22). The host shows an error toast and reopens the menu.

```rust
async fn invoke_context_action(
    &self, action_id: &str, _target: MenuTargetKind, _target_id: &str,
) -> Result<ActionOutcome, ClientError> {
    match action_id {
        "view-community" | "subscribe-community" => Ok(ActionOutcome::Noop),
        other => Err(ClientError::NotFound(format!("unknown action: {other}"))),
    }
}
```

---

## The `custom-block` Escape Hatch (D4)

`CustomBlock` lets a plugin inject sanitized HTML into specific slots: menu info-block
items, settings section info panels, sidebar section headers, view headers, and
`view-detail` bodies.

```rust
// Structure from clients/client/src/ui_surface.rs
pub struct CustomBlock {
    pub sanitized_html: String,      // passes through ammonia before render
    pub stylesheet: Option<String>,  // scoped to shadow-root; no global leakage
    pub max_height_px: Option<u32>,  // host handles overflow
}
```

**Allowed tags:** `p`, `span`, `div`, `strong`, `em`, `a` (href only),
`ul`, `ol`, `li`, `img` (src only), `br`, `table`/`thead`/`tbody`/`tr`/`td`/`th`,
`pre`, `code`, `blockquote`, `h1`–`h6`.

**Strictly forbidden (stripped by the host, never reach the DOM):**
`<script>`, `<style>` (outside the `stylesheet` field), `<iframe>`, `<form>`,
`<input>`, `javascript:` URLs, `data:` URLs in `<a href>`, all event handler
attributes (`onclick`, `onload`, `onerror`, etc.), `<foreignObject>`.

The host renders the block in a shadow-root so the plugin's CSS cannot leak
into the surrounding page. `max_height_px` triggers the host's scroll container.

`custom-block` is a **last resort**. A lint-gate counter tracks total usage across
plugins. Rising numbers signal that a missing declarative primitive should be added to
the surface instead.

---

## Example Walkthrough: "View on GitHub" Menu Item

Goal: add a right-click item on a server (repository) that opens it in the browser.

### Step 1 — Declare the menu item

In `clients/github/src/lib.rs`, inside `get_context_menu_items`:

```rust
MenuTargetKind::Server => Ok(vec![
    MenuItem {
        id: "open-in-browser".to_string(),
        parent_id: None,
        slot: MenuSlot::Bottom,
        label_key: "plugin-github-menu-open-in-browser-label".to_string(),
        icon: Some(IconSource::Emoji("🔗".to_string())),
        item_variant: MenuItemVariant::Normal,
        shortcut: None,
        block: None,
    },
]),
```

### Step 2 — Handle the action

In `invoke_context_action`:

```rust
"open-in-browser" => {
    let route = self.build_browser_url(target_id)?;
    Ok(ActionOutcome::Navigate(route))
}
other => Err(ClientError::NotFound(format!("unknown action: {other}"))),
```

For opening in the system browser the plugin uses `ActionOutcome::Navigate` with a
validated route string. To build the route, call `host_api.build_route` with
`route_kind: RouteKind::External` and the URL as a param (D20).

### Step 3 — Add the FTL entries

In `clients/github/locales/en/plugin.ftl`:

```ftl
plugin-github-menu-open-in-browser-label = View on GitHub
plugin-github-toast-browser-opened = Opened in browser
```

Two entries: the label the build lint requires, and an optional toast for feedback.

### Step 4 — What happens on click

1. User right-clicks a repository in the server list.
2. Host calls `get_context_menu_items(Server, repo_id)`.
3. Host merges universal items (Copy ID, Favorites, Leave) with the plugin's items,
   grouped by slot with separators.
4. User clicks "View on GitHub".
5. Host calls `invoke_context_action("open-in-browser", Server, repo_id)`.
6. Plugin returns `ActionOutcome::Navigate(url)`.
7. Host validates the route, opens the system browser.

### Step 5 — Required tests

Per D31, the item cannot land without:

- A unit test in `clients/github/src/lib.rs` asserting the declared item is well-formed.
- An entry in `clients/github/tests/capabilities.rs` asserting the item appears for
  `Server` target and not for `Channel`/`Message`.
- A call to `harness::menus::invoke_action_roundtrip(backend, "open-in-browser", …)` in
  `crates/plugin-host-tests/tests/client_e2e/discord.rs` (use the GitHub driver file).
- A call to `harness::menus::invoke_action_unknown_returns_notfound` with a fabricated ID.

---

## Reference: `ActionOutcome` Variants

| Variant | When to use |
|---|---|
| `Noop` | No UI change needed |
| `Completed` | Long-running op finished; no UI change |
| `Pending(PendingHandle)` | Slow op; host polls via `poll_action`; shows spinner |
| `RefreshTarget` | Re-fetch the target object (e.g., muted server now shows muted icon) |
| `RefreshSidebar` | Host rebuilds the sidebar (e.g., unsubscribed from community) |
| `Navigate(String)` | Route string built via `host_api.build_route` (D20) |
| `Toast(ToastPayload)` | Show an FTL-keyed toast |
| `OpenSettings(SettingsAnchor)` | Jump to a settings section |
| `OpenModal(ModalRef)` | Open a plugin-declared modal (reserved) |

---

## Reference: Type Locations

| Type | Crate / file |
|---|---|
| `ClientBackend` trait + all UI surface methods | `clients/client/src/lib.rs` |
| `MenuItem`, `ActionOutcome`, `SettingsSection`, … | `clients/client/src/ui_surface.rs` |
| WIT contract | `wit/messenger-plugin.wit` |
| Plan + decision record | `docs/plans/plan-client-ui-surface.md` |
| Discord implementation | `clients/discord/src/lib.rs` |
| Lemmy implementation | `clients/lemmy/src/lib.rs` |

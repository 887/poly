//! Client-provided UI surface: Rust mirrors of the WIT types declared in
//! `wit/messenger-plugin.wit` for the `client-ui-common`, `client-menus`,
//! `client-settings`, `client-sidebar`, `client-views`, and `client-composer`
//! interfaces (see `docs/plans/plan-client-ui-surface.md`, §4).
//!
//! These are plain-Rust mirrors; the WIT↔Rust conversion lives in
//! `crates/plugin-host/src/bridge.rs`. Keeping the types here (not in the
//! plugin-host crate) lets native backends and the UI layer depend on them
//! without pulling in the WASM host stack.
//!
//! All field names are kebab-case → snake_case. Enum variants match the WIT
//! spelling. `Serialize`/`Deserialize` is derived across the board, matching
//! the convention in [`super::types`].

use serde::{Deserialize, Serialize};

// ─── Common (D4 / D23 / D27) ───────────────────────────────────────

/// D4 — sanitized HTML rendered in a shadow-root by the host. Plugin-
/// supplied CSS is inlined into the shadow-root; no leakage. Allowlist is
/// documented in `plan-client-ui-surface.md` §4.6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomBlock {
    /// HTML passed through the host's ammonia allowlist before render.
    pub sanitized_html: String,
    /// Optional stylesheet scoped to the shadow-root.
    pub stylesheet: Option<String>,
    /// Optional max-height in CSS pixels. Overflow is host-managed.
    pub max_height_px: Option<u32>,
}

/// D27 — icon for menu items, sidebar items, settings sections, composer
/// buttons. Either an emoji string OR a sanitized SVG path string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IconSource {
    /// Unicode emoji. Rendered as-is.
    Emoji(String),
    /// Raw SVG string. Host passes it through its SVG-aware sanitizer
    /// (allowlist: path/g/circle/rect/polygon/polyline/line; attrs:
    /// d/fill/stroke/viewBox; no script/foreignObject/event handlers).
    Svg(String),
}

/// D23 — cursor-kind payload classifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorKind {
    /// Numeric offset (e.g. HN `start` index).
    Offset,
    /// RFC3339 timestamp (e.g. Lemmy `published` field).
    Timestamp,
    /// Opaque backend-defined id (e.g. Matrix event id).
    Id,
    /// Fully opaque — host does not inspect.
    Opaque,
}

/// D23 — structured paged-data cursor. Covers offset, timestamp, id, and
/// catch-all opaque paging models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub kind: CursorKind,
    pub value: String,
}

// ─── Menus (D6 / D14 / D16 / D22 / D25 / D27 / D28 / D30) ──────────

/// D6 — what the context menu is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MenuTargetKind {
    Category,
    Channel,
    Dm,
    Message,
    Server,
    User,
}

/// D6 — named slots in host context menus. Plugin items declare which slot
/// they occupy; host inserts separators between occupied slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MenuSlot {
    /// Above host items.
    Top,
    /// After Favorites block, before middle separators.
    AfterFavorites,
    /// Above Leave/Remove.
    BeforeLeave,
    /// Below host items.
    Bottom,
}

/// Variant of a menu item — governs rendering + interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MenuItemVariant {
    /// Standard clickable row.
    Normal,
    /// Red / destructive styling (Leave, Block, Delete).
    Destructive,
    /// Row is a submenu header; children reference this item via `parent_id`.
    SubmenuHeader,
    /// Non-clickable row that renders an optional `CustomBlock`.
    InfoBlock,
}

/// D6 / D14 / D25 / D27 / D28 — a single plugin-declared menu item.
///
/// Submenus are expressed as a flat list with `parent_id` pointers (WIT
/// forbids recursive records). The host reconstructs the tree: items with
/// `parent_id == None` are top-level; children reference their parent by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuItem {
    /// Opaque plugin action id (D14); kebab-case (D25). Unique within a
    /// single `get_context_menu_items` response.
    pub id: String,
    /// `Some(parent.id)` to place this item under a submenu-header parent;
    /// `None` for top-level items.
    pub parent_id: Option<String>,
    /// Which host slot this item lives in (only meaningful for top-level
    /// items — ignored when `parent_id` is set).
    pub slot: MenuSlot,
    /// FTL key: `plugin-<id>-menu-<key>-label` (D21 lint-checked).
    pub label_key: String,
    /// D27 — emoji OR sanitized SVG.
    pub icon: Option<IconSource>,
    /// Rendering style: normal | destructive | submenu-header | info-block.
    pub item_variant: MenuItemVariant,
    /// Optional shortcut hint text (e.g. `"Ctrl+C"`).
    pub shortcut: Option<String>,
    /// Only populated when `item_variant == InfoBlock` (D4).
    pub block: Option<CustomBlock>,
}

/// D16 — toast tones for `ActionOutcome::Toast`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToastTone {
    Info,
    Success,
    Warning,
    Error,
}

/// D16 — toast payload emitted by an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToastPayload {
    /// FTL key resolved via the plugin's FTL bundle.
    pub label_key: String,
    pub tone: ToastTone,
}

/// D16 — jump target for `ActionOutcome::OpenSettings`.
///
/// `scope` is one of the string forms of [`SettingsScope`]
/// (`"account-global"`, `"per-server"`, …) — matches the WIT representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsAnchor {
    pub scope: String,
    pub scope_id: String,
    pub section_key: String,
}

/// D16 — reference to a plugin-declared modal (reserved for future use).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModalRef {
    pub modal_id: String,
    /// Opaque JSON blob passed back to the plugin on invocation.
    pub context: String,
}

/// D16 — handle for a pending async action. Host polls via
/// [`ClientBackend::poll_action`](crate::ClientBackend::poll_action).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingHandle {
    /// Plugin-opaque; the host returns it verbatim via `poll_action`.
    pub action_ref: String,
    /// FTL key for "Saving…" / "Uploading…".
    pub progress_hint: Option<String>,
}

/// D16 / D30 — outcome of an invoked plugin action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionOutcome {
    /// No UI change.
    Noop,
    /// D16 — slow op; host polls via `poll_action`.
    Pending(PendingHandle),
    /// D16 — done, no UI change.
    Completed,
    /// D30 — re-fetch just the target object.
    RefreshTarget,
    /// Host rebuilds the sidebar.
    RefreshSidebar,
    /// D20 — route string built via `host-api.build-route`.
    Navigate(String),
    /// Show an FTL-keyed toast.
    Toast(ToastPayload),
    /// Jump into a settings section.
    OpenSettings(SettingsAnchor),
    /// Open a plugin-declared modal.
    OpenModal(ModalRef),
}

// ─── Settings (D11 / D15 / D18) ────────────────────────────────────

/// D18 — kind of setting control to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingKind {
    /// Boolean toggle / checkbox.
    Toggle,
    /// Single-line text input.
    TextInput,
    /// Dropdown with a fixed list of options.
    Select,
    /// Numeric slider (min / max / step encoded in `extra`).
    Slider,
    /// Read-only informational label row.
    InfoLabel,
}

/// D18 — one entry in a plugin's settings schema.
///
/// `key` is the storage key AND the FTL key suffix for label/description.
/// FTL lookup: `plugin-<plugin-id>-setting-<key>-label`
///             `plugin-<plugin-id>-setting-<key>-desc`   (optional)
/// Storage is owned by the plugin via `host-api.storage-*` (D15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingDescriptor {
    /// Unique key within this plugin (snake_case). Used as storage key.
    pub key: String,
    /// The kind of widget to render.
    pub kind: SettingKind,
    /// JSON-serialized default value (e.g. `"true"`, `"\"option-a\""`, `"42"`).
    pub default_value: String,
    /// For `Select` kind: JSON array of option values. For `Slider`:
    /// JSON object `{"min":0,"max":100,"step":1}`. Empty otherwise.
    pub extra: String,
}

/// D11 — where a settings section applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsScope {
    /// The plugin's account-wide settings panel. Replaces the legacy
    /// `get-settings-schema` surface (D18).
    AccountGlobal,
    PerServer,
    PerChannel,
    PerUser,
}

impl SettingsScope {
    /// Stable string label matching the WIT representation.
    ///
    /// Used as the storage-key prefix in plugin-side settings persistence
    /// (Pack C P18) and as the string form in [`SettingsAnchor::scope`].
    #[must_use]
    pub fn as_label(self) -> &'static str {
        match self {
            Self::AccountGlobal => "account-global",
            Self::PerServer => "per-server",
            Self::PerChannel => "per-channel",
            Self::PerUser => "per-user",
        }
    }
}

/// Compose a plugin-side settings storage key from (scope, scope-id, key).
///
/// This is the canonical key format used by every `ClientBackend` impl that
/// keeps a per-instance `HashMap<String, String>` for its settings storage
/// (Pack C P18). Centralizing it here means scope isolation (setting on
/// per-server scope-id `"A"` vs `"B"`) is enforced uniformly across backends.
#[must_use]
pub fn settings_storage_key(scope: SettingsScope, scope_id: &str, key: &str) -> String {
    format!("{}:{}:{}", scope.as_label(), scope_id, key)
}

/// Shared in-memory settings storage cell used by `ClientBackend` impls
/// that don't yet persist to disk / host-api.kv (Pack C P18).
///
/// Wraps a `RwLock<HashMap<String, String>>` keyed by
/// [`settings_storage_key`]. Every demo/HTTP backend embeds one of these
/// and defers to [`Self::get`] / [`Self::set`] in its `get_setting_value` /
/// `set_setting_value` impls.
///
/// Each backend instance owns its own cell — cross-backend isolation is
/// guaranteed by construction. Cross-scope (and cross-scope-id) isolation
/// is guaranteed by the composite key format.
#[derive(Debug, Default)]
pub struct SettingsStorageCell {
    inner: std::sync::RwLock<std::collections::HashMap<String, String>>,
}

impl SettingsStorageCell {
    /// Construct an empty cell.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fetch a previously-set value for (scope, scope_id, key).
    ///
    /// Returns `None` when either (a) the key was never written or (b) the
    /// internal RwLock was poisoned (poisoning is treated as "absent" to
    /// keep backends free of panics — callers fall through to the declared
    /// default).
    #[must_use]
    pub fn get(&self, scope: SettingsScope, scope_id: &str, key: &str) -> Option<String> {
        let storage_key = settings_storage_key(scope, scope_id, key);
        self.inner.read().ok()?.get(&storage_key).cloned()
    }

    /// Store `value` under (scope, scope_id, key).
    ///
    /// Returns an [`ClientError::Internal`] if the RwLock is poisoned.
    pub fn set(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
        value: &str,
    ) -> Result<(), crate::ClientError> {
        let storage_key = settings_storage_key(scope, scope_id, key);
        self.inner
            .write()
            .map_err(|e| crate::ClientError::Internal(format!("settings lock: {e}")))?
            .insert(storage_key, value.to_string());
        Ok(())
    }
}

/// D11 — one settings section; scope + section-key + fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsSection {
    pub scope: SettingsScope,
    /// FTL key suffix + URL anchor.
    pub section_key: String,
    pub icon: Option<String>,
    pub fields: Vec<SettingDescriptor>,
    /// Optional informational panel (D4).
    pub info_block: Option<CustomBlock>,
}

// ─── Sidebar (D5 / D11 / D19) ──────────────────────────────────────

/// D5 — stock layout kinds the host renders natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SidebarLayoutKind {
    /// Discord/Stoat/Teams/poly-native.
    ChannelList,
    /// Matrix: spaces containing rooms.
    SpacesRooms,
    /// Lemmy/Reddit: subscribed communities.
    Communities,
    /// HN/Mastodon: feed tabs.
    Feed,
    /// GitHub/Forgejo: repo tree.
    RepoTree,
    /// Plugin supplies sections explicitly.
    Custom,
}

/// D5 — the shape a sidebar-item navigates to when clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SidebarRouteKind {
    /// Opens chat-view.
    Channel,
    /// Opens `client-views` forum layout.
    Forum,
    /// Opens `client-views` feed layout.
    Feed,
    /// Opens code-browser.
    Code,
    /// Opens issue-list view.
    IssueTracker,
    /// Invokes a plugin-declared modal.
    Modal,
    /// Opens in system browser.
    External,
    /// Plugin returns a `ViewDescriptor` on click.
    CustomView,
}

/// D5 — a single row in the sidebar.
///
/// Nesting is expressed via `parent_id` pointers (analogous to [`MenuItem`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarItem {
    pub id: String,
    /// `Some(parent.id)` to nest under another sidebar item; `None` for
    /// section-root rows.
    pub parent_id: Option<String>,
    pub label_key: String,
    pub icon: Option<IconSource>,
    /// "new", "3", etc. Raw string; not an FTL key.
    pub badge: Option<String>,
    pub route_kind: SidebarRouteKind,
}

/// D5 — a grouping of sidebar items (optionally with a header).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarSection {
    /// FTL key; `None` for anonymous (header-less) group.
    pub header_key: Option<String>,
    pub collapsible: bool,
    pub default_collapsed: bool,
    pub items: Vec<SidebarItem>,
}

/// D5 — top-level sidebar declaration returned by the plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidebarDeclaration {
    pub layout: SidebarLayoutKind,
    /// Populated when `layout == Custom`.
    pub sections: Vec<SidebarSection>,
    /// Optional info panel at top (D4).
    pub header_block: Option<CustomBlock>,
}

// ─── Views (D5 / D23) ──────────────────────────────────────────────

/// D5 — closed set of body engines the host can render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewKind {
    /// Generic list (posts, issues, stories).
    FlatList,
    /// Post-card grid (Reddit, Mastodon).
    CardGrid,
    /// Nested (Lemmy threaded).
    Tree,
    /// Master-detail.
    Split,
}

/// D5 — a single selectable option in a toolbar (sort/filter/tab).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolbarOption {
    pub id: String,
    pub label_key: String,
    pub icon: Option<String>,
    pub default_selected: bool,
}

/// D5 — view header: titles + optional info block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewHeader {
    pub title_key: Option<String>,
    pub subtitle_key: Option<String>,
    pub info_block: Option<CustomBlock>,
}

/// D5 — view toolbar: sort / filter / tabs / action items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewToolbar {
    /// Hot / Top / New / Best / …
    pub sort_options: Vec<ToolbarOption>,
    /// Week / Month / Year / All.
    pub filter_options: Vec<ToolbarOption>,
    /// Issues / PRs / Discussions.
    pub tabs: Vec<ToolbarOption>,
    /// Reuses [`MenuItem`] from the menus surface.
    pub action_items: Vec<MenuItem>,
}

/// D5 — bindings from feed-field names to row-template positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowTemplate {
    /// e.g. `"title"`.
    pub primary_field: String,
    /// e.g. `"url"` / `"author"`.
    pub secondary_field: Option<String>,
    /// e.g. `"score · comments · age"`.
    pub meta_field: Option<String>,
    pub icon_field: Option<String>,
}

/// D5 — list body: row template + page size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSpec {
    pub row_template: RowTemplate,
    pub page_size: u32,
}

/// D5 — card body (Reddit/Mastodon style).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSpec {
    pub primary_field: String,
}

/// D5 — threaded tree body (Lemmy-style comments).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeSpec {
    pub root_page_size: u32,
    pub max_depth: u32,
}

/// D5 — split body: list on one side, detail on the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitSpec {
    pub list_side: ListSpec,
    pub detail_view_kind: ViewKind,
}

/// D5 — which body engine to render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewBody {
    ListBody(ListSpec),
    CardBody(CardSpec),
    TreeBody(TreeSpec),
    SplitBody(SplitSpec),
}

/// D5 — top-level descriptor for a non-chat view of a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDescriptor {
    pub kind: ViewKind,
    pub header: Option<ViewHeader>,
    pub toolbar: Option<ViewToolbar>,
    pub body: ViewBody,
}

/// D5 — one row in a paged view. `primary_text` etc. are raw content,
/// NOT FTL keys. `context_menu_target_kind` is what right-clicking this
/// row targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewRow {
    pub id: String,
    pub primary_text: String,
    pub secondary_text: Option<String>,
    pub meta_text: Option<String>,
    pub icon: Option<String>,
    pub badge: Option<String>,
    pub context_menu_target_kind: MenuTargetKind,
    /// Optional preview thumbnail URL for forum post rows. Populated by the
    /// Lemmy backend when `thumbnail_url` is present on the post AND the
    /// per-account `render-previews` mechanism is enabled.
    /// Other backends leave this as `None`.
    #[serde(default)]
    pub preview_image_url: Option<String>,
}

/// D23 — one page of rows plus an optional next-cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewRowsPage {
    pub rows: Vec<ViewRow>,
    pub next_cursor: Option<Cursor>,
}

/// D5 — detail payload for `split` views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDetail {
    /// Plugin-authored content, sanitized by the host (D4).
    pub body_block: CustomBlock,
    /// Optional threaded comments section.
    pub comments_section: Option<TreeSpec>,
}

// ─── Composer (D8) ─────────────────────────────────────────────────

/// D8 — where a composer button appears relative to the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComposerSlot {
    /// File upload / attach.
    LeftOfInput,
    /// Send / emoji / voice.
    RightOfInput,
    /// Sticker drawer toggle.
    AboveInput,
}

/// D8 — a single plugin-contributed composer button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposerButton {
    pub id: String,
    pub label_key: String,
    pub icon: String,
    pub position: ComposerSlot,
}

// ─── WP 1 unit tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    // ── Cursor round-trip (D23) ─────────────────────────────────────

    #[test]
    fn cursor_offset_roundtrip() {
        let c = Cursor { kind: CursorKind::Offset, value: "42".to_string() };
        let json = serde_json::to_string(&c).unwrap();
        let decoded: Cursor = serde_json::from_str(&json).unwrap();
        assert_eq!(c, decoded);
    }

    #[test]
    fn cursor_timestamp_roundtrip() {
        let c = Cursor {
            kind: CursorKind::Timestamp,
            value: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&c).unwrap();
        let decoded: Cursor = serde_json::from_str(&json).unwrap();
        assert_eq!(c, decoded);
    }

    #[test]
    fn cursor_id_roundtrip() {
        let c = Cursor { kind: CursorKind::Id, value: "$event-id:matrix.org".to_string() };
        let json = serde_json::to_string(&c).unwrap();
        let decoded: Cursor = serde_json::from_str(&json).unwrap();
        assert_eq!(c, decoded);
    }

    #[test]
    fn cursor_opaque_roundtrip() {
        let c = Cursor {
            kind: CursorKind::Opaque,
            value: "eyJwYWdlIjozfQ==".to_string(),
        };
        let json = serde_json::to_string(&c).unwrap();
        let decoded: Cursor = serde_json::from_str(&json).unwrap();
        assert_eq!(c, decoded);
    }

    #[test]
    fn cursor_kind_variants_are_distinct() {
        // Ensure each CursorKind variant round-trips to itself and not to another.
        for (kind, name) in [
            (CursorKind::Offset, "Offset"),
            (CursorKind::Timestamp, "Timestamp"),
            (CursorKind::Id, "Id"),
            (CursorKind::Opaque, "Opaque"),
        ] {
            let c = Cursor { kind, value: "x".to_string() };
            let json = serde_json::to_string(&c).unwrap();
            let back: Cursor = serde_json::from_str(&json).unwrap();
            assert_eq!(back.kind, kind, "CursorKind::{name} did not survive roundtrip");
        }
    }

    // ── SVG sanitizer allowlist (D27) ─────────────────────────────
    //
    // WP 5 moved the live sanitizer tests to
    // `crates/core/src/ui/client_ui/custom_block.rs` (alongside
    // `sanitize_html`, `build_sanitizer`, `prefix_css_selectors`). That
    // crate is where `ammonia` lives — `poly-client` is a pure data-type
    // crate and would pull in the whole sanitizer stack just to test it.
    // See `svg_path_allowed`, `svg_script_stripped`, `foreign_object_stripped`
    // in that file for the migrated coverage.
}

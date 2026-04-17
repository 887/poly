//! Context-menu runtime.
//!
//! Home of the shared contract that every `#[context_menu(Foo)]` menu-type
//! implements. See `docs/plans/plan-context-menu-quality-control.md` §2.3
//! for the DSL and §3.1.4 for the diagnostic hook.
//!
//! Phase A scope (this file): just the trait and its
//! `#[diagnostic::on_unimplemented]` annotation. The stack runtime (§4.1)
//! and host component (§4.2) land in sibling files as they come online.
//! Long-press extraction (§4.4) lives in `long_press.rs`.

pub mod long_press;

use dioxus::prelude::{Element, EventHandler};
use dioxus::events::MouseEvent;

/// Contract for a menu type attached via `#[context_menu(Foo)]`.
///
/// The attribute macro generates a compile-time assertion that the named
/// menu type implements `ContextMenuFor<Props>` for the host component's
/// props. That binding is what lets `build_ctx` receive an honest
/// reference to the trigger component's props without an `Any` round-trip.
///
/// # Authoring a menu
///
/// 1. Define a zero-sized marker type (e.g. `pub struct ChannelMenu;`).
/// 2. Pick a `Ctx` shape carrying just the data the items need at render
///    time — channel id, server id, permissions. Keep it `Clone` and
///    `'static`; it sits in `AppState.context_menu_stack`.
/// 3. `build_ctx` runs at right-click / long-press time and snapshots the
///    props + cursor. `render` runs every frame the menu is open and
///    returns the overlay content.
///
/// # Submenus
///
/// A menu opens a submenu by pushing a fresh `ActiveContextMenu` onto the
/// stack (see `host.rs` once it lands). The `close` handler supplied to
/// `render` pops *this* menu; it does not touch submenus opened on top.
// lint-allow-unused: trait is consumed by downstream ServerContextMenu / ChannelContextMenu / MsgContextMenuOverlay impls landing in subsequent commits (plan §2.3.1)
#[allow(dead_code)]
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a context menu for `Props = {Props}`",
    label = "this type must impl `ContextMenuFor<{Props}>` to be used in `#[context_menu({Self})]`",
    note = "implement `poly_core::ui::context_menu::ContextMenuFor<{Props}>` on `{Self}` \
            (see docs/plans/plan-context-menu-quality-control.md §2.3)"
)]
pub trait ContextMenuFor<Props> {
    /// Data captured at open-time and carried across every `render` call.
    type Ctx: Clone + 'static;

    /// Snapshot the trigger's props + the event that opened the menu.
    ///
    /// Runs exactly once per open. The returned `Ctx` is stored on the
    /// stack; `render` receives a clone each frame the menu is visible.
    fn build_ctx(props: &Props, evt: &MouseEvent) -> Self::Ctx;

    /// Render the menu's items. `close` dismisses just this menu.
    fn render(ctx: Self::Ctx, close: EventHandler<()>) -> Element;

    /// Compile-time assertion hook. Downstream macros may shadow this
    /// const with a `const _: () = assert!(...)` to enforce additional
    /// invariants (e.g. "the ctx fits in N words"). No-op by default.
    const ASSERT_COMPATIBLE: () = ();
}

//! Typed UI action infrastructure.
//!
//! See `docs/plans/plan-ui-action-types.md`.

use crate::ui::dioxus_router::Navigator;
use crate::state::NavState;

/// Context available to every `UiAction::apply()` call.
///
/// Phase C.3 (plan-solid-audit-core-state.md): the former `state: &mut AppState`
/// field was removed when the `AppState` god-struct was deleted. No production
/// `UiAction::apply` impl ever read or wrote `cx.state`; the two surviving
/// `AppState` fields migrated to `AccountSessions` / `ChatLists` and any action
/// needing them now reaches the context directly via `try_consume_context`.
pub struct ActionCx<'a> {
    /// Read-only snapshot of navigation state (active account, backend, etc.)
    pub nav: &'a NavState,
    /// `None` when constructed via `ActionCx::test()` — no Dioxus runtime needed.
    pub navigator: Option<Navigator>,
}

impl<'a> ActionCx<'a> {
    pub fn live(nav: &'a NavState, navigator: Navigator) -> Self {
        Self { nav, navigator: Some(navigator) }
    }

    /// Construct a test context — no Dioxus runtime needed.
    /// Accepts optional nav state; pass `&NavState::default()` when nav fields
    /// are not relevant to the action under test.
    pub fn test(nav: &'a NavState) -> Self {
        Self { nav, navigator: None }
    }

    /// Construct a test context with default nav state — convenience for tests
    /// where the action under test does not read nav fields.
    pub fn test_no_nav() -> Self {
        static DEFAULT_NAV: std::sync::OnceLock<NavState> = std::sync::OnceLock::new();
        Self {
            nav: DEFAULT_NAV.get_or_init(NavState::default),
            navigator: None,
        }
    }
}

#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement `UiAction`",
    label = "add `impl UiAction for {Self} {{ fn apply(self, cx: ActionCx<'_>) {{ ... }} }}`",
    note = "every variant must be handled — use `todo!(\"phase-X: ...\")` for WIP items"
)]
pub trait UiAction: Sized + 'static {
    fn apply(self, cx: ActionCx<'_>);
}

/// Dispatch a typed action from a Dioxus event handler.
///
/// Phase C.3 (plan-solid-audit-core-state.md): the old four-arg signature
/// `dispatch_action!(action, app_state, nav_state, nav)` was reduced to
/// three args after `AppState` was deleted — the `app_state` BatchedSignal
/// it required no longer exists. All slot-state mutation happens via the
/// new sub-signal contexts (`AccountSessions`, `ChatLists`, etc.) which
/// actions reach directly via `try_consume_context` when needed.
///
/// # Example
/// ```ignore
/// let nav_state = use_context::<BatchedSignal<NavState>>();
/// let nav = navigator();
/// onclick: move |_| dispatch_action!(MyAction::Save, nav_state, nav),
/// ```
#[macro_export]
macro_rules! dispatch_action {
    ($action:expr, $nav_state:expr, $nav:expr) => {{
        fn _assert_ui_action<T: $crate::ui::actions::UiAction>(_: &T) {}
        _assert_ui_action(&$action);
        let _action = $action;
        let _nav_snap = $nav_state.peek().clone();
        let _nav_val = $nav.clone();
        _action.apply($crate::ui::actions::ActionCx::live(
            &_nav_snap,
            _nav_val,
        ));
    }};
}

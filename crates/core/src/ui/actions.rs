//! Typed UI action infrastructure.
//!
//! See `docs/plans/plan-ui-action-types.md`.

use crate::ui::dioxus_router::Navigator;
use crate::state::{AppState, BatchedSignal, NavState};

/// Context available to every `UiAction::apply()` call.
pub struct ActionCx<'a> {
    pub state: &'a mut AppState,
    /// Read-only snapshot of navigation state (active account, backend, etc.)
    pub nav: &'a NavState,
    /// `None` when constructed via `ActionCx::test()` — no Dioxus runtime needed.
    pub navigator: Option<Navigator>,
}

impl<'a> ActionCx<'a> {
    pub fn live(state: &'a mut AppState, nav: &'a NavState, navigator: Navigator) -> Self {
        Self { state, nav, navigator: Some(navigator) }
    }

    /// Construct a test context — no Dioxus runtime needed.
    /// Accepts optional nav state; pass `&NavState::default()` when nav fields
    /// are not relevant to the action under test.
    pub fn test(state: &'a mut AppState, nav: &'a NavState) -> Self {
        Self { state, nav, navigator: None }
    }

    /// Construct a test context with default nav state — convenience for tests
    /// where the action under test does not read nav fields.
    pub fn test_no_nav(state: &'a mut AppState) -> Self {
        static DEFAULT_NAV: std::sync::OnceLock<NavState> = std::sync::OnceLock::new();
        Self {
            state,
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
/// Acquires `app_state` via `.batch()` so the action can mutate `AppState`
/// if it needs to, while also being correct for actions that use
/// `try_consume_context` (e.g. `VoiceBannerAction`).
///
/// # Example
/// ```ignore
/// let app_state = use_context::<BatchedSignal<AppState>>();
/// let nav_state = use_context::<BatchedSignal<NavState>>();
/// let nav = navigator();
/// onclick: move |_| dispatch_action!(MyAction::Save, app_state, nav_state, nav),
/// ```
#[macro_export]
macro_rules! dispatch_action {
    ($action:expr, $state:expr, $nav_state:expr, $nav:expr) => {{
        fn _assert_ui_action<T: $crate::ui::actions::UiAction>(_: &T) {}
        _assert_ui_action(&$action);
        let _action = $action;
        let _nav_snap = $nav_state.peek().clone();
        let _nav_val = $nav.clone();
        $state.batch(move |state| {
            _action.apply($crate::ui::actions::ActionCx::live(
                state,
                &_nav_snap,
                _nav_val,
            ));
        });
    }};
}

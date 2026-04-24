//! Typed UI action infrastructure.
//!
//! See `docs/plans/plan-ui-action-types.md`.

use crate::ui::dioxus_router::Navigator;
use crate::state::{AppState, BatchedSignal};

/// Context available to every `UiAction::apply()` call.
pub struct ActionCx<'a> {
    pub state: &'a mut AppState,
    /// `None` when constructed via `ActionCx::test()` — no Dioxus runtime needed.
    pub navigator: Option<Navigator>,
}

impl<'a> ActionCx<'a> {
    pub fn live(state: &'a mut AppState, navigator: Navigator) -> Self {
        Self { state, navigator: Some(navigator) }
    }

    /// Construct a test context — no Dioxus runtime needed.
    pub fn test(state: &'a mut AppState) -> Self {
        Self { state, navigator: None }
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
/// # Example
/// ```ignore
/// let mut state = use_context::<BatchedSignal<AppState>>();
/// let nav = use_navigator();
/// onclick: move |_| dispatch_action!(MyAction::Save, state, nav),
/// ```
#[macro_export]
macro_rules! dispatch_action {
    ($action:expr, $state:expr, $nav:expr) => {{
        fn _assert_ui_action<T: $crate::ui::actions::UiAction>(_: &T) {}
        _assert_ui_action(&$action);
        $action.apply($crate::ui::actions::ActionCx::live(
            &mut $state.write(),
            $nav.clone(),
        ));
    }};
}

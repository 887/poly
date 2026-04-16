//! poly-ui-macros — proc-macros for UI quality gates.
//!
//! * `#[rsx_body_size]` — counts the lines inside the first `rsx! { ... }`
//!   block in the function body and fails expansion if the count exceeds
//!   `MAX_RSX_LINES`. See `docs/plans/plan-component-lints.md` §3.1.
//!
//! * `#[context_menu(...)]` — (Phase B) will declare the context-menu
//!   contract for a `#[component]`. Today this macro is a no-op pass-through
//!   so call sites can adopt the attribute without Phase B being complete.
//!   See `docs/plans/plan-context-menu-quality-control.md` §3.1.
//!
//! * `#[connected(...)]` — (Phase B) will declare route-graph edges for a
//!   `#[component]`. Pass-through today. See
//!   `docs/plans/plan-connected-routes-static-check.md` §3.1.1.

use proc_macro::TokenStream;

mod rsx_size;

/// `MAX_RSX_LINES` gate. Apply above `#[component]` (or any `fn`):
///
/// ```ignore
/// #[rsx_body_size]
/// #[component]
/// fn Thing() -> Element {
///     rsx! { div { "hi" } }
/// }
/// ```
///
/// Counts physical source lines between the `{` and matching `}` of the
/// outermost `rsx! { ... }` in the function body; emits `compile_error!`
/// if the count exceeds the hard cap.
#[proc_macro_attribute]
pub fn rsx_body_size(attr: TokenStream, item: TokenStream) -> TokenStream {
    rsx_size::expand(attr, item)
}

/// `#[context_menu(...)]` — contract declaration for a `#[component]`.
///
/// Phase A: a transparent pass-through. Phase B will parse the Foo /
/// None / allow_default / inherit variants and enforce coverage via
/// `crates/lint-gate/build/context_menu_coverage.rs`.
#[proc_macro_attribute]
pub fn context_menu(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// `#[connected(...)]` — route-graph edge declaration.
///
/// Phase A: a transparent pass-through. Phase B will parse the
/// `linked | entry_point | programmatic<T>` variants and contribute
/// callsites to `crates/lint-gate/build/route_graph.rs`.
#[proc_macro_attribute]
pub fn connected(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

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
//!
//! * `#[ui_action(...)]` — declares the semantic action contract for a
//!   `#[component]`. Coverage enforced by
//!   `crates/lint-gate/build/action_enum_coverage.rs`.

use proc_macro::TokenStream;

mod connected;
mod context_menu;
mod route_introspect;
mod rsx_size;
mod ui_action;

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
/// Parses one of: `Foo` (menu type), `None` (opt-out), `allow_default`
/// (native menu), `inherit` (forward to parent). Invalid forms emit a
/// `compile_error!` at the attribute span. The function body is passed
/// through unchanged; coverage is enforced by
/// `crates/lint-gate/build/context_menu_coverage.rs`.
#[proc_macro_attribute]
pub fn context_menu(attr: TokenStream, item: TokenStream) -> TokenStream {
    context_menu::expand(attr, item)
}

/// `#[connected(...)]` — route-graph edge declaration (item attribute).
///
/// Parses a comma-separated list of edges; each edge is one of
/// `linked` / `entry_point` / `programmatic<Tag>`. Malformed input emits
/// `compile_error!` at the attribute span. The wrapped enum variant is
/// passed through unchanged; Phase B/C will additionally emit a
/// `linkme::distributed_slice` entry per edge (see plan §3.4).
#[proc_macro_attribute]
pub fn connected(attr: TokenStream, item: TokenStream) -> TokenStream {
    connected::expand(attr, item)
}

/// `#[derive(Connected)]` — enables `#[connected(...)]` helper attributes on
/// enum variants and emits two route-introspection helpers on the enum:
///
/// - `pub fn route_account_id(&self) -> Option<&str>` — returns the
///   `account_id` field of account-scoped variants; `None` for app-level
///   routes and for any variant annotated `#[connected(skip_account_id)]`.
///
/// - `#[cfg(debug_assertions)] fn route_variant_name(&self) -> &'static str`
///   — short discriminant name used by the dev-mode route-coverage counter.
///
/// Phase B / C: will additionally emit a `linkme` distributed slice carrying
/// each variant's declared edges so the `crates/lint-gate/build/route_graph.rs`
/// BFS can read them at build time.
///
/// This derive is the companion that makes `#[connected(...)]` legal on a
/// variant — Rust requires helper attributes to be declared by a derive on
/// the enclosing enum.
#[proc_macro_derive(Connected, attributes(connected))]
pub fn derive_connected(input: TokenStream) -> TokenStream {
    route_introspect::expand(input)
}

/// `#[ui_action(...)]` — semantic action contract for a `#[component]`.
///
/// Declare what user-triggered actions this component can perform:
/// - `#[ui_action(MyActionEnum)]` — typed enum implementing `UiAction`
/// - `#[ui_action(None)]`         — display-only, no semantic actions
/// - `#[ui_action(inherit)]`      — sub-component, delegates to parent
///
/// Coverage is enforced by `crates/lint-gate/build/action_enum_coverage.rs`.
#[proc_macro_attribute]
pub fn ui_action(attr: TokenStream, item: TokenStream) -> TokenStream {
    ui_action::expand(attr, item)
}

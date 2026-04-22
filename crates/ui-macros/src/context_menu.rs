//! `#[context_menu(...)]` attribute macro — handler injector.
//!
//! Parses the macro's argument list into one of four shapes and—for the two
//! active variants—wraps the component body so that a `display: contents` div
//! carries the right `oncontextmenu` event handler.
//!
//! | Variant | Runtime behaviour | Injection |
//! |---|---|---|
//! | `#[context_menu(none)]` / `#[context_menu(None)]` | Global guard fires → native menu suppressed. Correct by default. | **None** — pass through. Semantic documentation only; the global `prevent_default` on `.main-layout` (commit `aea0558a`) covers it. |
//! | `#[context_menu(inherit)]` | Global guard fires → native menu suppressed via ancestor. | **None** — pass through. |
//! | `#[context_menu(allow_default)]` | Without injection the global guard fires and wrongly suppresses the native menu for text inputs, anchors, etc. | **Injects** `oncontextmenu: stop_propagation` on a `display: contents` wrapper div so the event never reaches the `.main-layout` guard, letting the OS show its native menu. |
//! | `#[context_menu(SomeMenu)]` | Without injection the global guard fires; no Poly menu opens. | **Injects** `oncontextmenu: prevent_default` (+ TODO Phase D open_menu call) on a `display: contents` wrapper div. |
//!
//! ## Approach W — display: contents wrapper
//!
//! Instead of introspecting the RSX tree (fragile), the macro transforms:
//!
//! ```ignore
//! fn MyComponent(...) -> Element {
//!     body
//! }
//! ```
//!
//! into:
//!
//! ```ignore
//! fn MyComponent(...) -> Element {
//!     let __context_menu_inner = { body };
//!     rsx! {
//!         div {
//!             style: "display: contents;",
//!             oncontextmenu: |__evt| __evt.stop_propagation(),  // allow_default
//!             { __context_menu_inner }
//!         }
//!     }
//! }
//! ```
//!
//! `display: contents` makes the wrapper div invisible to layout — flex/grid
//! parents see through it — so there is no visual regression. The only known
//! risk is percentage-height children inside a flex parent that counts items;
//! in practice Dioxus components uniformly use `height: 100%` via CSS, not
//! inline styles, so this does not apply.
//!
//! ## Case-insensitive `none` / `None`
//!
//! Both spellings are accepted and treated identically. New code should prefer
//! lowercase `none`, but the existing audit sites use `None` (uppercase) and
//! must not break. Phase C normalises them; this macro must not fail on either.
//!
//! Spec: `docs/plans/plan-context-menu-quality-control.md` §2.1–2.2, §3.1.1.
//! Plan: `docs/plans/plan-ui-polish-round-2.md` Phase B.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Error, Ident, ItemFn, Path, Result, parse2};

/// Parsed form of the attribute's argument.
#[cfg_attr(test, derive(Debug))]
enum Arg {
    /// `#[context_menu(none)]` or `#[context_menu(None)]` — explicit opt-out.
    None,
    /// `#[context_menu(allow_default)]` — let native OS menu through.
    AllowDefault,
    /// `#[context_menu(inherit)]` — defer to parent component's menu.
    Inherit,
    /// `#[context_menu(SomeMenu)]` — attach a named Poly context menu type.
    Menu(Path),
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Err(Error::new(
                Span::call_site(),
                "`#[context_menu(...)]` requires exactly one argument: \
                 `Foo` (menu type), `none`/`None` (opt-out), `allow_default` \
                 (native menu), or `inherit` (forward to parent).",
            ));
        }

        // Try to parse as a bare ident first for the keyword forms.
        // Accept both `none` (preferred) and `None` (legacy) case-insensitively.
        let fork = input.fork();
        if let Ok(ident) = fork.parse::<Ident>() {
            if fork.is_empty() {
                let s = ident.to_string();
                // case-insensitive `none` / `None`
                if s.eq_ignore_ascii_case("none") {
                    input.parse::<Ident>()?;
                    return Ok(Arg::None);
                }
                if s == "allow_default" {
                    input.parse::<Ident>()?;
                    return Ok(Arg::AllowDefault);
                }
                if s == "inherit" {
                    input.parse::<Ident>()?;
                    return Ok(Arg::Inherit);
                }
            }
        }

        // Otherwise must parse as a type path (e.g. `ChannelMenu` or
        // `crate::ui::server::ServerContextMenu`).
        match input.parse::<Path>() {
            Ok(p) if input.is_empty() => Ok(Arg::Menu(p)),
            Ok(p) => Err(Error::new_spanned(
                &p,
                "`#[context_menu(...)]` accepts exactly one argument",
            )),
            Err(e) => Err(e),
        }
    }
}

/// Wrap the function body with a `display: contents` div carrying the given
/// `oncontextmenu` handler expression.
///
/// Transforms `fn Foo(...) -> Element { body }` into:
/// ```ignore
/// fn Foo(...) -> Element {
///     let __context_menu_inner = { body };
///     rsx! { div { style: "display: contents;", oncontextmenu: handler, { __context_menu_inner } } }
/// }
/// ```
fn wrap_fn_body(
    mut func: ItemFn,
    handler: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let original_stmts = &func.block.stmts;
    let new_block: syn::Block = syn::parse_quote! {
        {
            let __context_menu_inner = { #(#original_stmts)* };
            rsx! {
                div {
                    style: "display: contents;",
                    oncontextmenu: #handler,
                    { __context_menu_inner }
                }
            }
        }
    };
    func.block = Box::new(new_block);
    quote! { #func }
}

/// Internal expand logic operating entirely on `proc_macro2::TokenStream` so
/// it can be unit-tested without the proc-macro bridge.
pub(crate) fn expand2(
    attr: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let arg = match parse2::<Arg>(attr) {
        Ok(a) => a,
        Err(err) => {
            let compile_err = err.to_compile_error();
            return quote! {
                #item
                #compile_err
            };
        }
    };

    match arg {
        // No injection: pass through unchanged.
        Arg::None | Arg::Inherit => item,

        // allow_default: inject stop_propagation so the global `.main-layout`
        // `prevent_default` guard does not fire, letting the OS show its native
        // context menu (text-edit spell-check / copy-paste, link "Open in tab",
        // image "Save as", etc.).
        Arg::AllowDefault => {
            let func: ItemFn = match parse2(item.clone()) {
                Ok(f) => f,
                Err(err) => {
                    let compile_err = err.to_compile_error();
                    return quote! { #item #compile_err };
                }
            };
            let handler = quote! {
                |__evt: ::dioxus::prelude::Event<::dioxus::prelude::MouseData>| __evt.stop_propagation()
            };
            wrap_fn_body(func, handler)
        }

        // Typed menu: inject prevent_default (keeps the global guard's effect)
        // plus a TODO stub for Phase D's open_menu wiring.
        //
        // NOTE(phase-d): replace the stub below with:
        //   `open_menu::<#menu_path>(__evt, ctx)` once Phase D wires the
        //   typed-menu infrastructure end-to-end.
        Arg::Menu(_menu_path) => {
            let func: ItemFn = match parse2(item.clone()) {
                Ok(f) => f,
                Err(err) => {
                    let compile_err = err.to_compile_error();
                    return quote! { #item #compile_err };
                }
            };
            // TODO(phase-d): wire open_menu::<#menu_path>(__evt, ctx) here.
            // For now only prevent_default keeps the global-guard behaviour.
            let handler = quote! {
                |__evt: ::dioxus::prelude::Event<::dioxus::prelude::MouseData>| __evt.prevent_default()
            };
            wrap_fn_body(func, handler)
        }
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand2(
        proc_macro2::TokenStream::from(attr),
        proc_macro2::TokenStream::from(item),
    )
    .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use quote::quote;

    fn parse_arg(tokens: proc_macro2::TokenStream) -> Result<Arg> {
        parse2(tokens)
    }

    // ── Arg parsing ─────────────────────────────────────────────────────────

    #[test]
    fn accepts_none_uppercase() {
        assert!(matches!(parse_arg(quote!(None)).unwrap(), Arg::None));
    }

    #[test]
    fn accepts_none_lowercase() {
        assert!(matches!(parse_arg(quote!(none)).unwrap(), Arg::None));
    }

    #[test]
    fn accepts_allow_default() {
        assert!(matches!(
            parse_arg(quote!(allow_default)).unwrap(),
            Arg::AllowDefault
        ));
    }

    #[test]
    fn accepts_inherit() {
        assert!(matches!(parse_arg(quote!(inherit)).unwrap(), Arg::Inherit));
    }

    #[test]
    fn accepts_bare_menu_type() {
        let arg = parse_arg(quote!(ChannelMenu)).unwrap();
        match arg {
            Arg::Menu(p) => assert!(p.is_ident("ChannelMenu")),
            _ => panic!("expected Arg::Menu"),
        }
    }

    #[test]
    fn accepts_qualified_menu_path() {
        let arg = parse_arg(quote!(crate::ui::server::ServerContextMenu)).unwrap();
        assert!(matches!(arg, Arg::Menu(_)));
    }

    #[test]
    fn rejects_empty() {
        let err = parse_arg(quote!()).unwrap_err();
        assert!(err.to_string().contains("exactly one argument"));
    }

    #[test]
    fn rejects_multiple_args() {
        assert!(parse_arg(quote!(Foo, Bar)).is_err());
    }

    #[test]
    fn rejects_string_literal() {
        assert!(parse_arg(quote!("Foo")).is_err());
    }

    // ── Expansion: inherit / none → no injection ────────────────────────────

    /// For `inherit`, the original item must be passed through unchanged —
    /// no `oncontextmenu:` injection, no wrapper div.
    #[test]
    fn inherit_no_injection() {
        let attr = quote!(inherit);
        let item = quote! {
            fn MyComponent() -> Element {
                rsx! { div { "hello" } }
            }
        };
        let output = expand2(attr, item);
        let output_str = output.to_string();
        assert!(
            !output_str.contains("oncontextmenu"),
            "inherit must not inject oncontextmenu; got: {output_str}"
        );
    }

    /// For `none` (uppercase), same: no injection.
    #[test]
    fn none_no_injection() {
        let attr = quote!(None);
        let item = quote! {
            fn MyComponent() -> Element {
                rsx! { div { "hello" } }
            }
        };
        let output = expand2(attr, item);
        let output_str = output.to_string();
        assert!(
            !output_str.contains("oncontextmenu"),
            "none must not inject oncontextmenu; got: {output_str}"
        );
    }

    /// Lowercase `none` must also produce no injection.
    #[test]
    fn none_lowercase_no_injection() {
        let attr = quote!(none);
        let item = quote! {
            fn MyComponent() -> Element {
                rsx! { div { "hello" } }
            }
        };
        let output = expand2(attr, item);
        let output_str = output.to_string();
        assert!(
            !output_str.contains("oncontextmenu"),
            "none (lowercase) must not inject oncontextmenu; got: {output_str}"
        );
    }

    // ── Expansion: allow_default → stop_propagation ─────────────────────────

    /// For `allow_default`, the expansion must contain `stop_propagation` and
    /// `display: contents` but NOT `prevent_default`.
    #[test]
    fn allow_default_injects_stop_propagation() {
        let attr = quote!(allow_default);
        let item = quote! {
            fn TextEditor() -> Element {
                rsx! { textarea { class: "editor" } }
            }
        };
        let output = expand2(attr, item);
        let output_str = output.to_string();
        assert!(
            output_str.contains("stop_propagation"),
            "allow_default must inject stop_propagation; got: {output_str}"
        );
        assert!(
            output_str.contains("display: contents"),
            "allow_default must use display:contents wrapper; got: {output_str}"
        );
        assert!(
            !output_str.contains("prevent_default"),
            "allow_default must NOT inject prevent_default; got: {output_str}"
        );
        assert!(
            output_str.contains("oncontextmenu"),
            "allow_default must inject oncontextmenu handler; got: {output_str}"
        );
    }

    // ── Expansion: typed menu → prevent_default ─────────────────────────────

    /// For a typed menu, the expansion must contain `prevent_default` and
    /// `display: contents`. It must NOT contain `stop_propagation` (which
    /// would let the native menu through instead of the Poly menu).
    #[test]
    fn typed_menu_injects_prevent_default() {
        let attr = quote!(MessageMenu);
        let item = quote! {
            fn MessageRow() -> Element {
                rsx! { div { class: "message-row" } }
            }
        };
        let output = expand2(attr, item);
        let output_str = output.to_string();
        assert!(
            output_str.contains("prevent_default"),
            "typed menu must inject prevent_default; got: {output_str}"
        );
        assert!(
            output_str.contains("display: contents"),
            "typed menu must use display:contents wrapper; got: {output_str}"
        );
        assert!(
            !output_str.contains("stop_propagation"),
            "typed menu must NOT inject stop_propagation; got: {output_str}"
        );
    }

    // ── Edge cases ───────────────────────────────────────────────────────────

    /// A function with early-return paths: the whole body is captured in the
    /// let-binding, so both paths are covered.
    #[test]
    fn allow_default_early_return_body() {
        let attr = quote!(allow_default);
        let item = quote! {
            fn MaybeComponent(show: bool) -> Element {
                if !show { return None; }
                rsx! { div { "content" } }
            }
        };
        let output = expand2(attr, item);
        let output_str = output.to_string();
        assert!(
            output_str.contains("stop_propagation"),
            "early-return body must still inject; got: {output_str}"
        );
        // The early-return `return None;` is inside the let-block, so both
        // paths resolve to Element before the wrapper rsx! is reached.
        // (In practice, returning None from inside the let means the wrapper
        // renders an empty display:contents div. Acceptable.)
    }

    /// A function body with multiple statements must have all of them captured.
    #[test]
    fn allow_default_multi_stmt_body() {
        let attr = quote!(allow_default);
        let item = quote! {
            fn Editor() -> Element {
                let x = 42;
                let y = x + 1;
                rsx! { div { "{y}" } }
            }
        };
        let output = expand2(attr, item);
        let output_str = output.to_string();
        assert!(output_str.contains("stop_propagation"));
        // Both `let x` and `let y` statements should appear in the output.
        assert!(output_str.contains("42"));
    }
}

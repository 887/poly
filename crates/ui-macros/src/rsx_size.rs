//! `#[rsx_body_size]` attribute macro.
//!
//! Counts the physical source lines inside the outermost `rsx! { ... }`
//! block in the annotated function's body. Emits `compile_error!` if
//! the count exceeds `MAX_RSX_LINES`.
//!
//! See `docs/plans/plan-component-lints.md` §3.1 for the rationale —
//! clippy's `too_many_lines` ignores tokens inside `rsx!` expansion, so
//! monster components like `FavoriteServerIcon` (684 lines) or
//! `ChatView` (1129 lines) slip past the default lint.

use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

/// Hard cap on rsx! body length (physical source lines between the `{`
/// on the `rsx!` invocation and its matching `}`).
const MAX_RSX_LINES: usize = 100;

pub fn expand(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);

    if let Some((start_line, end_line)) = find_first_rsx_span(&func) {
        let measured = end_line.saturating_sub(start_line).saturating_add(1);
        if measured > MAX_RSX_LINES {
            let msg = format!(
                "rsx! body is {measured} lines — cap is {MAX_RSX_LINES}. Split the component; see plan-component-lints.md §3.1."
            );
            return quote! {
                #func
                const _: () = {
                    ::core::compile_error!(#msg);
                };
            }
            .into();
        }
    }

    quote! { #func }.into()
}

/// Walk the function's tokens looking for the first `rsx ! { ... }` (or
/// `rsx! { ... }`) invocation and return (start_line, end_line) of the
/// brace group. Returns None if no rsx! invocation is found.
///
/// Uses `Span::source_text()` to get the brace group's source and counts
/// newlines — `Span::start()`/`end()` are nightly-only on proc-macro2.
fn find_first_rsx_span(func: &ItemFn) -> Option<(usize, usize)> {
    let body = &func.block;
    let tokens: proc_macro2::TokenStream = quote! { #body };
    scan_for_rsx(tokens)
}

fn scan_for_rsx(stream: proc_macro2::TokenStream) -> Option<(usize, usize)> {
    let mut iter = stream.into_iter().peekable();
    while let Some(tree) = iter.next() {
        match &tree {
            TokenTree::Ident(id) if id == "rsx" => {
                if let Some(TokenTree::Punct(p)) = iter.peek()
                    && p.as_char() == '!'
                {
                    let _bang = iter.next();
                    if let Some(TokenTree::Group(g)) = iter.peek() {
                        let text = g.span().source_text().unwrap_or_default();
                        let lines = text.lines().count().max(1);
                        return Some((1, lines));
                    }
                }
            }
            TokenTree::Group(g) => {
                if let Some(hit) = scan_for_rsx(g.stream()) {
                    return Some(hit);
                }
            }
            _ => {}
        }
    }
    None
}

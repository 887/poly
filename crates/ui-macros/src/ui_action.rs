//! `#[ui_action(...)]` attribute macro — argument validator.
//!
//! Parses the macro's argument list into one of three shapes:
//!
//! | Variant | Meaning |
//! |---|---|
//! | `#[ui_action(None)]` | display-only component, no semantic actions |
//! | `#[ui_action(inherit)]` | sub-component, delegates to parent's action enum |
//! | `#[ui_action(Foo)]` / `#[ui_action(path::Foo)]` | attach the named action enum implementing `UiAction` |
//!
//! Anything else emits `compile_error!` at the attribute span. The function
//! body is passed through unchanged — this is a marker-only macro; a scanner
//! enforces coverage at build time.
//!
//! Spec: `docs/plans/plan-ui-action-types.md`.
//
// TODO: trybuild tests (Phase B.3) — skipped; `.stderr` snapshots require a
// build run to generate. Add once the crate is wired into CI.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Error, Ident, Path, Result};

/// Parsed form of the attribute's argument.
// lint-allow-unused: variants carry information consumed by future scanner
#[allow(dead_code)]
#[cfg_attr(test, derive(Debug))]
enum Arg {
    None,
    Inherit,
    Action(Path),
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Err(Error::new(
                Span::call_site(),
                "`#[ui_action(...)]` requires exactly one argument: \
                 `MyActionEnum` (typed enum implementing `UiAction`), \
                 `None` (display-only, no semantic actions), or \
                 `inherit` (sub-component, delegates to parent).",
            ));
        }

        // Try to parse as a bare ident first for the two keyword forms.
        let fork = input.fork();
        if let Ok(ident) = fork.parse::<Ident>() && fork.is_empty() {
            let s = ident.to_string();
            if s == "None" {
                input.parse::<Ident>()?;
                return Ok(Self::None);
            }
            if s == "inherit" {
                input.parse::<Ident>()?;
                return Ok(Self::Inherit);
            }
        }

        // Otherwise must parse as a type path (e.g. `ChatAction` or
        // `crate::ui::server::ServerAction`).
        match input.parse::<Path>() {
            Ok(p) if input.is_empty() => Ok(Self::Action(p)),
            Ok(p) => Err(Error::new_spanned(
                &p,
                "`#[ui_action(...)]` accepts exactly one argument",
            )),
            Err(e) => Err(e),
        }
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2 = proc_macro2::TokenStream::from(attr);
    let item2 = proc_macro2::TokenStream::from(item);

    if let Err(err) = syn::parse2::<Arg>(attr2) {
        let compile_err = err.to_compile_error();
        return quote! {
            #item2
            #compile_err
        }
        .into();
    }

    item2.into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use quote::quote;

    fn parse(tokens: proc_macro2::TokenStream) -> Result<Arg> {
        syn::parse2(tokens)
    }

    #[test]
    fn accepts_none() {
        assert!(matches!(parse(quote!(None)).unwrap(), Arg::None));
    }

    #[test]
    fn accepts_inherit() {
        assert!(matches!(parse(quote!(inherit)).unwrap(), Arg::Inherit));
    }

    #[test]
    fn accepts_bare_action_type() {
        let arg = parse(quote!(ChatAction)).unwrap();
        match arg {
            Arg::Action(p) => assert!(p.is_ident("ChatAction")),
            _ => panic!("expected Arg::Action"),
        }
    }

    #[test]
    fn accepts_qualified_action_path() {
        let arg = parse(quote!(crate::ui::server::ServerAction)).unwrap();
        assert!(matches!(arg, Arg::Action(_)));
    }

    #[test]
    fn rejects_empty() {
        let err = parse(quote!()).unwrap_err();
        assert!(err.to_string().contains("exactly one argument"));
    }

    #[test]
    fn rejects_multiple_args() {
        assert!(parse(quote!(Foo, Bar)).is_err());
    }

    #[test]
    fn rejects_string_literal() {
        assert!(parse(quote!("Foo")).is_err());
    }
}

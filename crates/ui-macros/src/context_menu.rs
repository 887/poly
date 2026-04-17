//! `#[context_menu(...)]` attribute macro — argument validator.
//!
//! Parses the macro's argument list into one of four shapes:
//!
//! | Variant | Meaning |
//! |---|---|
//! | `#[context_menu(None)]` | explicit opt-out; `preventDefault` only, no menu |
//! | `#[context_menu(allow_default)]` | native menu fires (images, inputs) |
//! | `#[context_menu(inherit)]` | defer to the parent component's menu |
//! | `#[context_menu(Foo)]` / `#[context_menu(path::Foo)]` | attach the named menu type |
//!
//! Anything else emits `compile_error!` next to the item so the author sees
//! the same span rust-analyzer uses. The function body is passed through
//! unchanged — Phase A only validates; the DOM-level wrapper (§2.2 of the
//! context-menu plan) lands with the runtime refactor in Phase B.
//!
//! Spec: `docs/plans/plan-context-menu-quality-control.md` §2.1–2.2, §3.1.1.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Error, Ident, Path, Result};

/// Parsed form of the attribute's argument.
// lint-allow-unused: variants carry information consumed by Phase B runtime refactor
#[allow(dead_code)]
#[cfg_attr(test, derive(Debug))]
enum Arg {
    None,
    AllowDefault,
    Inherit,
    Menu(Path),
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Err(Error::new(
                Span::call_site(),
                "`#[context_menu(...)]` requires exactly one argument: \
                 `Foo` (menu type), `None` (opt-out), `allow_default` \
                 (native menu), or `inherit` (forward to parent).",
            ));
        }

        // Try to parse as a bare ident first for the three keyword forms.
        let fork = input.fork();
        if let Ok(ident) = fork.parse::<Ident>() && fork.is_empty() {
            let s = ident.to_string();
            if s == "None" {
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
    fn accepts_allow_default() {
        assert!(matches!(
            parse(quote!(allow_default)).unwrap(),
            Arg::AllowDefault
        ));
    }

    #[test]
    fn accepts_inherit() {
        assert!(matches!(parse(quote!(inherit)).unwrap(), Arg::Inherit));
    }

    #[test]
    fn accepts_bare_menu_type() {
        let arg = parse(quote!(ChannelMenu)).unwrap();
        match arg {
            Arg::Menu(p) => assert!(p.is_ident("ChannelMenu")),
            _ => panic!("expected Arg::Menu"),
        }
    }

    #[test]
    fn accepts_qualified_menu_path() {
        let arg = parse(quote!(crate::ui::server::ServerContextMenu)).unwrap();
        assert!(matches!(arg, Arg::Menu(_)));
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

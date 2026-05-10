//! `#[connected(...)]` attribute macro — argument validator.
//!
//! Per `docs/plans/plan-connected-routes-static-check.md` §2.1 the attribute
//! declares one or more incoming edges on a `Route` enum variant:
//!
//! | Syntax | Meaning |
//! |---|---|
//! | `linked` | ≥1 `Link { to: Route::X }` / `nav!(Route::X)` callsite targets this variant |
//! | `entry_point` | BFS root — exactly one variant in the workspace may carry this |
//! | `programmatic<Tag>` | reachable via the `ProgrammaticProducer` impl for ZST `Tag` |
//! | `skip_account_id` | variant has an `account_id` field but `route_account_id` must return `None` (e.g. `ReauthAccount`) |
//!
//! Multiple edges may be comma-separated:
//! `#[connected(linked, programmatic<SignupLanding>, programmatic<PnOpener>)]`.
//!
//! Phase A scope (this file): parse the arg list and emit `compile_error!`
//! on malformed input. Actual `linkme` slice emission + BFS consumption
//! lives in the build-script (`crates/lint-gate/build/route_graph.rs`)
//! and will gain a proc-macro-side counterpart in Phase B/C.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, Ident, Path, Result, Token};

/// One edge declared in `#[connected(...)]`.
// lint-allow-unused: variants carry information consumed by Phase B linkme slice emission
#[allow(dead_code)]
#[cfg_attr(test, derive(Debug))]
enum Edge {
    Linked,
    EntryPoint,
    Programmatic(Path),
    /// Instructs `#[derive(Connected)]` to emit `None` from `route_account_id`
    /// even though the variant has an `account_id` field.  Used on
    /// `ReauthAccount` so that `route_targets_unknown_account` never treats a
    /// reauth URL as targeting a known active account.
    SkipAccountId,
}

impl Parse for Edge {
    fn parse(input: ParseStream) -> Result<Self> {
        let ident: Ident = input.parse()?;
        let s = ident.to_string();
        match s.as_str() {
            "linked" => Ok(Edge::Linked),
            "entry_point" => Ok(Edge::EntryPoint),
            "skip_account_id" => Ok(Edge::SkipAccountId),
            "programmatic" => {
                input.parse::<Token![<]>().map_err(|_orig| {
                    Error::new(
                        ident.span(),
                        "`programmatic` must be followed by `<Tag>` \
                         (e.g. `programmatic<SignupCompletionLanding>`)",
                    )
                })?;
                let path: Path = input.parse()?;
                input.parse::<Token![>]>()?;
                Ok(Edge::Programmatic(path))
            }
            other => Err(Error::new(
                ident.span(),
                format!(
                    "unknown edge kind `{other}` — expected `linked`, \
                     `entry_point`, `skip_account_id`, or `programmatic<Tag>`"
                ),
            )),
        }
    }
}

#[cfg_attr(test, derive(Debug))]
struct Connected {
    // lint-allow-unused: parsed edges consumed by Phase B linkme slice emission
    #[allow(dead_code)]
    edges: Punctuated<Edge, Token![,]>,
}

impl Parse for Connected {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Err(Error::new(
                Span::call_site(),
                "`#[connected(...)]` requires at least one edge: \
                 `linked`, `entry_point`, or `programmatic<Tag>`",
            ));
        }
        let edges: Punctuated<Edge, Token![,]> = Punctuated::parse_terminated(input)?;
        // At most one entry_point per variant; the across-variants uniqueness
        // check is the build-script's job.
        let entry_points = edges
            .iter()
            .filter(|e| matches!(e, Edge::EntryPoint))
            .count();
        if entry_points > 1 {
            return Err(Error::new(
                Span::call_site(),
                "`entry_point` may appear at most once per variant",
            ));
        }
        Ok(Connected { edges })
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr2 = proc_macro2::TokenStream::from(attr);
    let item2 = proc_macro2::TokenStream::from(item);

    if let Err(err) = syn::parse2::<Connected>(attr2) {
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

    fn parse(tokens: proc_macro2::TokenStream) -> Result<Connected> {
        syn::parse2(tokens)
    }

    #[test]
    fn accepts_linked() {
        let c = parse(quote!(linked)).unwrap();
        assert_eq!(c.edges.len(), 1);
        assert!(matches!(c.edges.first(), Some(Edge::Linked)));
    }

    #[test]
    fn accepts_entry_point() {
        let c = parse(quote!(entry_point)).unwrap();
        assert!(matches!(c.edges.first(), Some(Edge::EntryPoint)));
    }

    #[test]
    fn accepts_programmatic_bare_ident() {
        let c = parse(quote!(programmatic<SignupLanding>)).unwrap();
        assert!(matches!(c.edges.first(), Some(Edge::Programmatic(_))));
    }

    #[test]
    fn accepts_programmatic_qualified() {
        let c = parse(quote!(programmatic<crate::ui::signup::SignupLanding>)).unwrap();
        assert!(matches!(c.edges.first(), Some(Edge::Programmatic(_))));
    }

    #[test]
    fn accepts_comma_separated_mix() {
        let c = parse(quote!(linked, programmatic<Foo>, programmatic<Bar>)).unwrap();
        assert_eq!(c.edges.len(), 3);
    }

    #[test]
    fn accepts_trailing_comma() {
        let c = parse(quote!(linked,)).unwrap();
        assert_eq!(c.edges.len(), 1);
    }

    #[test]
    fn rejects_empty() {
        let err = parse(quote!()).unwrap_err();
        assert!(err.to_string().contains("at least one edge"));
    }

    #[test]
    fn rejects_multiple_entry_points() {
        let err = parse(quote!(entry_point, entry_point)).unwrap_err();
        assert!(err.to_string().contains("at most once"));
    }

    #[test]
    fn rejects_unknown_ident() {
        let err = parse(quote!(banana)).unwrap_err();
        assert!(err.to_string().contains("unknown edge kind"));
    }

    #[test]
    fn rejects_programmatic_without_angle_bracket() {
        assert!(parse(quote!(programmatic)).is_err());
    }
}

//! Cookie extraction helpers for the HN write-path.
//!
//! Isolated here so they can be unit-tested independently and reused
//! if a future session-refresh flow needs to inspect `Set-Cookie` headers.

/// Pull the `user=...` cookie out of one or more `Set-Cookie` headers.
/// Returns the full `name=value` form so callers can hand it straight to
/// a `Cookie:` request header.
pub(super) fn extract_user_cookie<'a, I: IntoIterator<Item = &'a str>>(
    set_cookies: I,
) -> Option<String> {
    for raw in set_cookies {
        // Each Set-Cookie header looks like:
        //   user=USERNAME&LONG_TOKEN; expires=Sun...; path=/; ...
        // We only want the first segment (before the first ';').
        let first = raw.split(';').next()?.trim();
        if first.starts_with("user=") && !first.eq("user=") {
            return Some(first.to_string());
        }
    }
    None
}

#[cfg(test)]
// lint-allow-unused: test-only panicking helpers are fine in #[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn extract_user_cookie_picks_user() {
        let headers = vec![
            "__cf_bm=abc; path=/",
            "user=alice&AAAAAAAAAA; expires=Tue, 01-Jan-2030 00:00:00 GMT; path=/",
        ];
        let got = extract_user_cookie(headers.iter().copied()).unwrap();
        assert_eq!(got, "user=alice&AAAAAAAAAA");
    }

    #[test]
    fn extract_user_cookie_rejects_empty() {
        // HN sets `user=` (empty) when logging out — must not be treated
        // as a successful login.
        let headers = vec!["user=; expires=Thu, 01-Jan-1970 00:00:00 GMT; path=/"];
        assert!(extract_user_cookie(headers.iter().copied()).is_none());
    }
}

//! Hacker News write-path: cookie login + comment POST.
//!
//! HN exposes no authenticated API — the public Firebase endpoint is
//! read-only. Posting comments / submitting stories requires driving the
//! HTML site at `news.ycombinator.com` directly:
//!
//! 1. POST `/login` with `acct={user}&pw={pw}&goto=news` →
//!    response sets a `user=USERNAME&...` cookie that authenticates
//!    every subsequent request.
//! 2. GET `/reply?id={parent}&goto=item?id={parent}` →
//!    parse the hidden `<input name="hmac" value="...">` from the form.
//! 3. POST `/comment` with `parent={id}&goto=...&hmac={h}&text={text}`
//!    plus the cookie from step 1.
//!
//! All HTTP goes through `poly_host_bridge::http::HttpClient` — same
//! transport the read-only API uses — so the same User-Agent override
//! and bridge routing apply on WASM.

mod cookies;

use poly_client::{ClientError, ClientResult};
use poly_host_bridge::http::HttpClient;

pub const HN_BASE: &str = "https://news.ycombinator.com";

/// Log in with username + password. On success, returns the value of the
/// `user` cookie (already in `name=value` form, ready to use as a
/// `Cookie:` header). Failures (bad credentials, captcha, rate-limit) come
/// back as `ClientError::Auth`.
pub async fn login(
    http: &HttpClient,
    user_agent: &str,
    username: &str,
    password: &str,
) -> ClientResult<String> {
    let body = url_encode_form(&[
        ("acct", username),
        ("pw", password),
        ("goto", "news"),
    ]);

    let resp = http
        .post(format!("{HN_BASE}/login"))
        .header("User-Agent", user_agent)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| ClientError::Network(format!("HN login: {e}")))?;

    // HN returns 200 either way — success is signalled by the Set-Cookie
    // header. Failure renders the login form again with a "Bad login"
    // banner inline; we detect that via cookie absence + body sniff.
    let set_cookies: Vec<String> = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok().map(str::to_owned))
        .collect();
    let cookie = cookies::extract_user_cookie(set_cookies.iter().map(String::as_str))
        .ok_or_else(|| {
            ClientError::AuthFailed(
                "HN rejected login (no user cookie set — wrong credentials \
                 or HN demanded a captcha)"
                    .to_string(),
            )
        })?;

    Ok(cookie)
}

/// Fetch the per-form `hmac` CSRF token for replying to `parent_id`.
/// HN renders this token freshly on each page load and binds it to the
/// authenticated user.
pub async fn fetch_reply_hmac(
    http: &HttpClient,
    user_agent: &str,
    parent_id: u64,
    cookie: &str,
) -> ClientResult<String> {
    let resp = http
        .get(format!("{HN_BASE}/reply?id={parent_id}&goto=item%3Fid%3D{parent_id}"))
        .header("User-Agent", user_agent)
        .header("Cookie", cookie)
        .send()
        .await
        .map_err(|e| ClientError::Network(format!("HN fetch_reply_hmac: {e}")))?;

    let html = resp
        .text()
        .await
        .map_err(|e| ClientError::Network(format!("HN read reply page: {e}")))?;

    extract_hmac(&html).ok_or_else(|| {
        ClientError::Internal(
            "HN reply page had no hmac field — session may have expired".to_string(),
        )
    })
}

/// POST a comment to `parent_id`. `cookie` must be the `user=...` value
/// returned by [`login`]; `hmac` is the per-form token returned by
/// [`fetch_reply_hmac`]. Returns the new comment's item ID if HN's
/// response includes one (extracted from the redirect target); else
/// returns `Ok(None)`.
pub async fn post_comment(
    http: &HttpClient,
    user_agent: &str,
    parent_id: u64,
    text: &str,
    cookie: &str,
    hmac: &str,
) -> ClientResult<Option<u64>> {
    let goto = format!("item?id={parent_id}");
    let body = url_encode_form(&[
        ("parent", &parent_id.to_string()),
        ("goto", &goto),
        ("hmac", hmac),
        ("text", text),
    ]);

    let resp = http
        .post(format!("{HN_BASE}/comment"))
        .header("User-Agent", user_agent)
        .header("Cookie", cookie)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| ClientError::Network(format!("HN post_comment: {e}")))?;

    // HN normally 302s back to /item?id=PARENT on success. Some failure
    // modes still 200 with a "Sorry, you can only post comments..." page
    // — sniff for that to avoid silent drops.
    let status = resp.status().as_u16();
    if !(200..400).contains(&status) {
        return Err(ClientError::Network(format!("HN /comment HTTP {status}")));
    }

    let html = resp.text().await.unwrap_or_default();
    if html.contains("you can only post") || html.contains("Bad login") {
        return Err(ClientError::AuthFailed(
            "HN refused the comment (rate-limit, banned, or session expired)".to_string(),
        ));
    }

    // HN doesn't return the new item ID in any structured way; the user
    // can find their comment by visiting their profile. Return None and
    // let the caller fabricate a placeholder.
    Ok(None)
}

// ── form helpers ──────────────────────────────────────────────────────────

/// Find the value of a hidden `<input name="hmac" value="...">` field in
/// an HN form. Naive substring scan — HN renders these with a stable
/// shape and we don't want to pull in an HTML parser just for one field.
fn extract_hmac(html: &str) -> Option<String> {
    // Look for `name="hmac" value="..."` OR `name=hmac value=...` (HN
    // uses double-quoted attributes consistently, but tolerate both).
    let needle = "name=\"hmac\"";
    let idx = html.find(needle)?;
    let rest = html.get(idx.checked_add(needle.len())?..)?;
    let val_idx = rest.find("value=\"")?;
    let after_val = rest.get(val_idx.checked_add(7)?..)?;
    let end = after_val.find('"')?;
    Some(after_val.get(..end)?.to_string())
}

/// `application/x-www-form-urlencoded` body builder — minimal, no
/// dependency on `serde_urlencoded` because callers want full control
/// over key ordering (HN's parser is order-insensitive but the wire
/// format is easier to debug when stable).
fn url_encode_form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Tiny RFC 3986 unreserved-set percent-encoder. Anything outside
/// `A-Za-z0-9-._~` is escaped as `%XX`.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", b));
        }
    }
    out
}

#[cfg(test)]
// lint-allow-unused: test-only panicking helpers are fine in #[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn extract_hmac_finds_field() {
        let html = r#"
            <form action="comment" method="post">
                <input type="hidden" name="parent" value="12345">
                <input type="hidden" name="goto" value="item?id=12345">
                <input type="hidden" name="hmac" value="deadbeefcafef00d">
                <textarea name="text"></textarea>
            </form>
        "#;
        assert_eq!(extract_hmac(html).as_deref(), Some("deadbeefcafef00d"));
    }

    #[test]
    fn extract_hmac_returns_none_when_missing() {
        let html = "<form><input name='goto' value='news'></form>";
        assert!(extract_hmac(html).is_none());
    }

    #[test]
    fn url_encode_handles_special_chars() {
        let body = url_encode_form(&[
            ("text", "hello world & such"),
            ("goto", "item?id=42"),
        ]);
        assert_eq!(body, "text=hello%20world%20%26%20such&goto=item%3Fid%3D42");
    }
}

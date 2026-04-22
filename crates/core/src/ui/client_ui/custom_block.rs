//! Host component that renders plugin-authored sanitized HTML in a scoped
//! container. WP 5 of `docs/plans/plan-client-ui-surface.md` (§4.6, §6.6, D27).
//!
//! # Security model
//!
//! Plugins ship `sanitized_html` through WIT — but the **host** is the only
//! authority on safety. The plugin's input is passed through an `ammonia`
//! allowlist ([`sanitize_html`]) before being handed to Dioxus via
//! `dangerous_inner_html`. An optional `stylesheet` is namespaced via
//! [`prefix_css_selectors`] so it cannot leak out of the block.
//!
//! # Shadow-root upgrade (P38)
//!
//! Plan §4.6 / P38 calls for a real shadow-root so plugin CSS is fully
//! isolated from host CSS — not just prefixed. We ship **both** paths:
//!
//! 1. **Scoped-CSS fallback (SSR + initial hydration):** the sanitized HTML
//!    renders inside `<div class="custom-block cb-{id}">` and the
//!    stylesheet's selectors are rewritten with a `.cb-{id}` prefix. This
//!    gives a usable result before JS runs (SSR pre-paint) and as a
//!    permanent fallback if the `document::eval` call fails.
//! 2. **True shadow-root attach (post-mount JS):** a `use_effect` on the
//!    client runs `document::eval(…)` which finds the `.cb-{id}` host div,
//!    calls `host.attachShadow({ mode: 'open' })`, and moves the sanitized
//!    HTML + stylesheet into the shadow tree. Because the sanitized HTML is
//!    stashed in `data-*` attributes on the host div, the JS never has to
//!    receive large strings through `eval(…)` arguments — it reads them
//!    back from the attached node. This keeps eval snippets short and
//!    avoids the escaping pitfalls called out in the plan.
//!
//! Ammonia sanitization stays the **primary** defense. Shadow-root gives us
//! DOM isolation (host CSS can't bleed in, plugin CSS can't leak out), not
//! sanitization: a `<script>` tag inside a shadow root still executes. See
//! `docs/security/custom-block-audit.md` for the threat model.

use dioxus::prelude::*;
use poly_client::CustomBlock as CustomBlockData;
use poly_ui_macros::{context_menu, ui_action};
use std::sync::atomic::{AtomicU64, Ordering};

/// Build an [`ammonia::Builder`] with the WP 5 allowlist.
///
/// # Allowed tags
///
/// Text/flow: `p`, `span`, `div`, `strong`, `em`, `br`, `blockquote`, `pre`,
/// `code`, `h1`–`h6`.
/// Lists/tables: `ul`, `ol`, `li`, `table`, `thead`, `tbody`, `tr`, `td`, `th`.
/// Media: `a`, `img`.
/// SVG: `svg`, `path`, `g`, `circle`, `rect`, `polygon`, `polyline`, `line`.
///
/// # Allowed attributes
///
/// * `a`: `href`, `title` — URL schemes `http`, `https`, `mailto`.
///   **No** `javascript:`, `data:` on `<a href>`.
/// * `img`: `src`, `alt`, `width`, `height` — URL schemes `http`, `https`,
///   `data` (ammonia doesn't discriminate MIME inside `data:` URIs, but the
///   attack surface is limited since `<img>` can't execute scripts).
/// * `svg`: `viewBox`, `xmlns`, `width`, `height`.
/// * `path`, `g`, `circle`, `rect`, `polygon`, `polyline`, `line`: `d`,
///   `fill`, `stroke`, `stroke-width`, `cx`, `cy`, `r`, `x`, `y`, `width`,
///   `height`, `points`, `transform`.
///
/// Event-handler attributes (`onclick`, `onload`, …) are stripped — this is
/// `ammonia`'s default for any attribute not in the allowlist.
/// `<script>`, `<style>`, `<foreignObject>`, `<iframe>`, `<form>`, `<input>`
/// are stripped because they are not in the tag allowlist.
fn build_sanitizer() -> ammonia::Builder<'static> {
    use std::collections::{HashMap, HashSet};

    let mut builder = ammonia::Builder::default();

    builder.tags(HashSet::from([
        "p", "span", "div", "strong", "em", "a", "ul", "ol", "li", "img", "br",
        "table", "thead", "tbody", "tr", "td", "th",
        "pre", "code", "blockquote",
        "h1", "h2", "h3", "h4", "h5", "h6",
        "svg", "path", "g", "circle", "rect", "polygon", "polyline", "line",
    ]));

    let svg_shape_attrs: HashSet<&'static str> = HashSet::from([
        "d", "fill", "stroke", "stroke-width",
        "cx", "cy", "r", "x", "y", "width", "height",
        "points", "transform",
    ]);

    let mut tag_attrs: HashMap<&'static str, HashSet<&'static str>> = HashMap::new();
    tag_attrs.insert("a", HashSet::from(["href", "title"]));
    tag_attrs.insert("img", HashSet::from(["src", "alt", "width", "height"]));
    tag_attrs.insert("svg", HashSet::from(["viewBox", "xmlns", "width", "height"]));
    for tag in ["path", "g", "circle", "rect", "polygon", "polyline", "line"] {
        tag_attrs.insert(tag, svg_shape_attrs.clone());
    }
    builder.tag_attributes(tag_attrs);

    // Allowed URL schemes, applied globally. ammonia uses this for every
    // URL-valued attribute. `<a href="javascript:…">` is blocked because
    // `javascript` is not in the set; `<a href="data:…">` is blocked
    // because `data` is not in the set either. `<img src="data:…">` IS
    // allowed (to support inline SVG/PNG thumbnails), but `<img>` cannot
    // execute scripts so the attack surface is low.
    // NOTE: `data:` intentionally excluded. Ammonia's URL scheme allowlist is
    // global; allowing `data:` would permit `<a href="data:text/html,...">`
    // which is an XSS vector. Cost: `<img src="data:image/...">` thumbnails
    // don't render. Image thumbnails should be served via http(s) URLs from
    // the plugin's own HTTP host (declared via plugin-manifest.http-hosts).
    builder.url_schemes(HashSet::from(["http", "https", "mailto"]));

    // Lock down <a href> specifically to http/https/mailto. We can't easily
    // disallow `data:` per-tag in ammonia's current API, so we scrub it in
    // post-processing for `<a>` in `sanitize_html`.

    builder
}

/// Sanitize `input` using the WP 5 allowlist (see [`build_sanitizer`]).
///
/// Also strips `data:` URLs from `<a href>` as a post-pass (ammonia's
/// URL-scheme allowlist is global; we need `data:` on `<img>` but not on
/// `<a>`).
pub fn sanitize_html(input: &str) -> String {
    let cleaned = build_sanitizer().clean(input).to_string();
    let out = strip_data_href_on_anchors(&cleaned);
    // F8 — defence in depth: in debug builds, assert the sanitizer never
    // leaks a `<script` tag through. The trybuild fixture originally
    // proposed for this check is unworkable (Rust's type system can't
    // inspect string contents), so we enforce the invariant at runtime
    // where the sanitizer actually runs.
    debug_assert!(
        !out.to_ascii_lowercase().contains("<script"),
        "sanitize_html must strip <script> tags; output retained one"
    );
    out
}

/// Remove `href="data:…"` attributes from `<a>` tags. Called as a post-pass
/// after `ammonia::clean` because ammonia's URL allowlist is global.
fn strip_data_href_on_anchors(html: &str) -> String {
    // Tiny state machine: find `<a ` tags, within them find `href="data:`
    // and replace with `href=""`. Good enough for our threat model; the
    // primary defense is ammonia, this is belt-and-suspenders.
    let mut out = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<'
            && i + 2 < bytes.len()
            && bytes[i + 1].eq_ignore_ascii_case(&b'a')
            && (bytes[i + 2] == b' ' || bytes[i + 2] == b'\t' || bytes[i + 2] == b'\n')
        {
            // Find the closing '>'.
            if let Some(end_rel) = html[i..].find('>') {
                let tag = &html[i..i + end_rel + 1];
                let lower = tag.to_ascii_lowercase();
                if lower.contains("href=\"data:") || lower.contains("href='data:") {
                    // Replace the offending href with href="".
                    let patched = replace_href_scheme(tag, "data:");
                    out.push_str(&patched);
                } else {
                    out.push_str(tag);
                }
                i += end_rel + 1;
                continue;
            }
        }
        // SAFETY: `i` is always a valid char boundary. When we enter the
        // `<a …>` branch we advance `i` by `end_rel + 1` which lands on the
        // byte after `>` — still a char boundary because `>` is single-byte
        // ASCII. Outside that branch we advance by `c.len_utf8()`, which is
        // always the correct next boundary. Using `bytes[i] as char` was the
        // F12 mojibake bug: it treated every byte as an independent Latin-1
        // codepoint, corrupting multi-byte UTF-8 sequences (e.g. em-dash
        // 0xE2 0x80 0x94 → 'â' + two garbage chars).
        if let Some(c) = html[i..].chars().next() {
            out.push(c);
            i += c.len_utf8();
        } else {
            break;
        }
    }
    out
}

/// Replace `href="<scheme>…"` with `href=""` in `tag`.
fn replace_href_scheme(tag: &str, scheme: &str) -> String {
    let lower = tag.to_ascii_lowercase();
    for quote in ['"', '\''] {
        let needle = format!("href={}{}", quote, scheme);
        if let Some(start) = lower.find(&needle) {
            // Find the matching closing quote.
            let after_quote = start + needle.len();
            if let Some(end_rel) = tag[after_quote..].find(quote) {
                let mut out = String::with_capacity(tag.len());
                out.push_str(&tag[..start]);
                out.push_str("href=\"\"");
                out.push_str(&tag[after_quote + end_rel + 1..]);
                return out;
            }
        }
    }
    tag.to_string()
}

/// Rewrite every selector in `css` so it is prefixed with `.{scope_class}`.
/// E.g. `p { color: red }` → `.cb-xxx p { color: red }`.
///
/// This is a naive top-level-selector rewriter: it splits on `}` and
/// prepends the scope class to each rule's selector list. Nested selectors
/// and `@media` queries aren't stripped, but they ARE scoped (the prefix is
/// prepended to whatever selector appears). Comma-separated selector lists
/// are handled (each selector in the list gets the prefix).
///
/// This is a *best-effort* CSS scoper — a real CSS parser would be better,
/// but ammonia already sanitizes the HTML and the stylesheet cannot contain
/// script, so the blast radius of a malformed prefix is a broken layout,
/// not a security hole.
pub fn prefix_css_selectors(css: &str, scope_class: &str) -> String {
    let prefix = format!(".{}", scope_class);
    let mut out = String::with_capacity(css.len() + css.len() / 4);

    // Walk top-level rules. A rule = <selector-list> { <body> }. We don't
    // handle @media specially — its selector-list would start with `@media`
    // which we just leave alone (CSS accepts the prefix there as garbage,
    // but we avoid it by letting at-rules pass through unprefixed).
    for rule in css.split('}') {
        let rule = rule.trim_start();
        if rule.is_empty() {
            continue;
        }
        if let Some(brace) = rule.find('{') {
            let selectors = &rule[..brace];
            let body = &rule[brace..];

            if selectors.trim_start().starts_with('@') {
                // Leave at-rules alone (body may itself contain scoped rules,
                // but that's a corner case WP 5 doesn't need).
                out.push_str(selectors);
                out.push_str(body);
                out.push('}');
                continue;
            }

            let scoped: Vec<String> = selectors
                .split(',')
                .map(|s| {
                    let trimmed = s.trim();
                    if trimmed.is_empty() {
                        trimmed.to_string()
                    } else {
                        format!("{} {}", prefix, trimmed)
                    }
                })
                .collect();
            out.push_str(&scoped.join(", "));
            out.push_str(body);
            out.push('}');
        } else {
            // Trailing whitespace/garbage after the last '}'; drop.
        }
    }
    out
}

/// Counter for unique scope-class suffixes. `u64` is overkill, but it
/// removes any chance of collision within a session without needing `uuid`
/// as a new dep.
static SCOPE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_scope_id() -> u64 {
    SCOPE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// JS snippet for post-mount shadow-root attach. Kept short; reads the
/// sanitized payload from `data-*` attrs on the host so we don't have to
/// interpolate large strings into `eval()`.
const SHADOW_ATTACH_JS: &str = r#"(function(scope){var host=document.querySelector('.custom-block.'+scope);if(!host||host.dataset.shadowAttached==='1')return;if(typeof host.attachShadow!=='function')return;var html=host.getAttribute('data-sanitized-html')||'';var css=host.getAttribute('data-stylesheet')||'';var fb=host.querySelector('.custom-block-content');if(fb)fb.style.display='none';var root=host.attachShadow({mode:'open'});if(css){var s=document.createElement('style');s.textContent=css;root.appendChild(s);}var w=document.createElement('div');w.className='custom-block-content';w.innerHTML=html;root.appendChild(w);host.dataset.shadowAttached='1';})"#;

/// Stylesheet sanitizer: strip `javascript:` / `expression(...)` / `@import`
/// / `behavior:` / `-moz-binding` declarations before CSS enters either the
/// scoped-prefix path or the shadow-root. Ammonia doesn't own CSS (the WIT
/// stylesheet field is a raw string) so this is our only defense there.
pub fn sanitize_stylesheet(css: &str) -> String {
    if css.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(css.len());
    for decl in css.split(';') {
        let lower = decl.to_ascii_lowercase();
        if lower.contains("javascript:")
            || lower.contains("expression(")
            || lower.trim_start().starts_with("@import")
            || lower.contains("-moz-binding")
            || lower.contains("behavior:")
        {
            continue;
        }
        if !out.is_empty() {
            out.push(';');
        }
        out.push_str(decl);
    }
    out
}

#[ui_action(None)]
// allow_default so `<a>` and `<img>` rendered by plugin HTML inside
// `.custom-block-content` get the OS "Open link / Save link as" / "Save image"
// native context menu (see audit-native-rclick-leakage.md §3.5).
#[context_menu(allow_default)]
#[component]
pub fn CustomBlock(block: CustomBlockData) -> Element {
    let scope_id = use_hook(next_scope_id);
    let scope_class = format!("cb-{}", scope_id);

    // Primary defense: ammonia. Shadow-root is DOM isolation, not
    // sanitization — a `<script>` inside a shadow still runs.
    let sanitized = sanitize_html(&block.sanitized_html);
    let sanitized_css =
        sanitize_stylesheet(block.stylesheet.as_deref().unwrap_or(""));
    let scoped_css = if sanitized_css.is_empty() {
        None
    } else {
        Some(prefix_css_selectors(&sanitized_css, &scope_class))
    };

    let root_class = format!("custom-block {}", scope_class);
    let root_style = block
        .max_height_px
        .map(|h| format!("max-height: {}px; overflow: auto;", h));

    // Post-mount effect: upgrade from scoped-CSS (fallback) to real
    // shadow-root. `document::eval` only does anything on wasm/web;
    // native/SSR keeps the scoped-CSS fallback as the final render.
    let scope_for_effect = scope_class.clone();
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            let script = format!("{}(\"{}\");", SHADOW_ATTACH_JS, scope_for_effect);
            let _ = dioxus::document::eval(&script);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = &scope_for_effect;
        }
    });

    let data_html = sanitized.clone();
    let data_css = sanitized_css.clone();

    rsx! {
        div {
            class: "{root_class}",
            style: root_style,
            "data-sanitized-html": "{data_html}",
            "data-stylesheet": "{data_css}",
            if let Some(css) = scoped_css {
                style { dangerous_inner_html: "{css}" }
            }
            div { class: "custom-block-content", dangerous_inner_html: "{sanitized}" }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn script_tag_stripped() {
        let input = r#"<p>hi</p><script>alert('xss')</script>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("<script"));
        assert!(sanitized.contains("<p>hi</p>"));
    }

    #[test]
    fn javascript_url_stripped() {
        let input = r#"<a href="javascript:alert('xss')">click</a>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("javascript:"));
    }

    #[test]
    fn data_url_in_a_href_stripped() {
        let input = r#"<a href="data:text/html,<script>">click</a>"#;
        let sanitized = sanitize_html(input);
        assert!(
            !sanitized.contains("data:"),
            "expected data: stripped from <a href>, got: {sanitized}"
        );
    }

    #[test]
    fn onclick_attr_stripped() {
        let input = r#"<div onclick="evil()">x</div>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("onclick"));
    }

    #[test]
    fn svg_path_allowed() {
        let input = r#"<svg viewBox="0 0 10 10"><path d="M0 0L10 10"/></svg>"#;
        let sanitized = sanitize_html(input);
        assert!(sanitized.contains("<path"), "sanitized: {sanitized}");
        assert!(
            sanitized.contains(r#"d="M0 0L10 10""#),
            "sanitized: {sanitized}"
        );
    }

    #[test]
    fn svg_script_stripped() {
        let input = r#"<svg><script>alert('xss')</script><path d="M0 0"/></svg>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("<script"));
        assert!(sanitized.contains("<path"));
    }

    #[test]
    fn foreign_object_stripped() {
        let input = r#"<svg><foreignObject><div>nested</div></foreignObject></svg>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("<foreignObject"));
    }

    #[test]
    fn stylesheet_prefix_applied() {
        let css = "p { color: red; }";
        let prefixed = prefix_css_selectors(css, "cb-xxx");
        assert!(
            prefixed.contains(".cb-xxx p"),
            "expected scoped selector, got: {prefixed}"
        );
    }

    #[test]
    fn stylesheet_prefix_comma_list() {
        let css = "p, span { color: red; }";
        let prefixed = prefix_css_selectors(css, "cb-xxx");
        assert!(prefixed.contains(".cb-xxx p"), "got: {prefixed}");
        assert!(prefixed.contains(".cb-xxx span"), "got: {prefixed}");
    }

    #[test]
    fn stylesheet_prefix_multiple_rules() {
        let css = "p { color: red; } div { color: blue; }";
        let prefixed = prefix_css_selectors(css, "cb-1");
        assert!(prefixed.contains(".cb-1 p"), "got: {prefixed}");
        assert!(prefixed.contains(".cb-1 div"), "got: {prefixed}");
    }

    #[test]
    fn iframe_stripped() {
        let input = r#"<iframe src="http://evil"></iframe><p>hi</p>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("<iframe"), "got: {sanitized}");
    }

    #[test]
    fn form_and_input_stripped() {
        let input = r#"<form><input name="x"/></form><p>hi</p>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("<form"), "got: {sanitized}");
        assert!(!sanitized.contains("<input"), "got: {sanitized}");
        assert!(sanitized.contains("<p>hi</p>"));
    }

    #[test]
    fn style_tag_stripped() {
        // `<style>` is not in the tag allowlist — plugin stylesheets ride
        // the `stylesheet` field, not inline <style> inside the HTML.
        let input = r#"<p>hi</p><style>body{display:none}</style>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("<style"), "got: {sanitized}");
    }

    #[test]
    fn img_data_src_stripped() {
        // `data:` excluded globally from the URL allowlist (safer against
        // XSS via `<a href="data:text/html,...">`). Cost: data-URL image
        // thumbnails are stripped. Plugins should serve images via http(s).
        let input = r#"<img src="data:image/png;base64,iVBORw0KGgo=" alt="x"/>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("data:"), "got: {sanitized}");
    }

    #[test]
    fn a_http_href_preserved() {
        let input = r#"<a href="https://example.com">link</a>"#;
        let sanitized = sanitize_html(input);
        assert!(sanitized.contains(r#"href="https://example.com""#), "got: {sanitized}");
    }

    // ─── P38 / P39 — Pack G shadow-root + stylesheet hardening ─────────

    #[test]
    fn shadow_root_script_stripped_via_eval_path() {
        // The shadow-root JS reads from `data-sanitized-html`. Whatever
        // lands in that attribute has already been through ammonia.
        // Verify no `<script>` ever survives sanitization, regardless of
        // where it would be injected.
        let input = r#"<p>ok</p><script>window.stolen=document.cookie</script>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("<script"), "got: {sanitized}");
        assert!(!sanitized.contains("window.stolen"), "got: {sanitized}");
    }

    #[test]
    fn shadow_root_use_xlink_external_blocked() {
        // `<use xlink:href="http://attacker/sprite.svg#id">` is a known
        // SVG external-ref vector. `<use>` is NOT in the tag allowlist,
        // so ammonia strips it — neither the tag nor the attr survives.
        let input = r#"<svg><use xlink:href="http://attacker/sprite.svg#a"/></svg>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("<use"), "got: {sanitized}");
        assert!(!sanitized.contains("xlink:href"), "got: {sanitized}");
        assert!(!sanitized.contains("attacker"), "got: {sanitized}");
    }

    #[test]
    fn shadow_root_css_javascript_url_blocked() {
        let css = "p { background-image: url(javascript:alert(1)); color: red; }";
        let cleaned = sanitize_stylesheet(css);
        assert!(!cleaned.contains("javascript:"), "got: {cleaned}");
        // Legitimate declaration on the same rule is preserved.
        assert!(cleaned.contains("color: red"), "got: {cleaned}");
    }

    #[test]
    fn shadow_root_use_foreign_object_stripped() {
        let input = r#"<svg><foreignObject><img src="x" onerror="alert(1)"/></foreignObject></svg>"#;
        let sanitized = sanitize_html(input);
        assert!(!sanitized.contains("<foreignObject"), "got: {sanitized}");
        assert!(!sanitized.contains("onerror"), "got: {sanitized}");
    }

    #[test]
    fn shadow_root_css_expression_blocked() {
        // IE legacy. Modern browsers ignore, but strip anyway.
        let css = "div { width: expression(alert(1)); color: blue; }";
        let cleaned = sanitize_stylesheet(css);
        assert!(!cleaned.contains("expression("), "got: {cleaned}");
        assert!(cleaned.contains("color: blue"), "got: {cleaned}");
    }

    #[test]
    fn shadow_root_css_at_import_blocked() {
        let css = "@import url(http://attacker/evil.css); p { color: red; }";
        let cleaned = sanitize_stylesheet(css);
        assert!(!cleaned.contains("@import"), "got: {cleaned}");
        assert!(!cleaned.contains("attacker"), "got: {cleaned}");
    }

    #[test]
    fn shadow_root_css_moz_binding_blocked() {
        let css = "div { -moz-binding: url(http://x/xbl#evil); color: red; }";
        let cleaned = sanitize_stylesheet(css);
        assert!(!cleaned.contains("-moz-binding"), "got: {cleaned}");
    }

    #[test]
    fn stylesheet_sanitizer_preserves_safe_css() {
        let css = "p { color: red } div.foo { background: #fff; padding: 4px }";
        let cleaned = sanitize_stylesheet(css);
        assert!(cleaned.contains("color: red"));
        assert!(cleaned.contains("background: #fff"));
        assert!(cleaned.contains("padding: 4px"));
    }

    // ─── F12 — UTF-8 multibyte preservation through sanitize_html ──────────
    //
    // Root cause: `strip_data_href_on_anchors` iterated over bytes and cast
    // each byte to `char` (`bytes[i] as char`). For multi-byte UTF-8
    // sequences (em-dash = 0xE2 0x80 0x94, etc.) this misinterpreted each
    // byte as an independent Latin-1 codepoint, producing mojibake:
    //   — (U+2014, 3 bytes) → 'â' (U+00E2) + two garbage chars
    // Fixed by advancing through `html[i..].chars().next()` so we always
    // consume a full codepoint at a time.

    #[test]
    fn utf8_em_dash_preserved_through_sanitize() {
        // em-dash U+2014 — must survive the full sanitize_html pipeline
        let input = "<p>hello\u{2014}world</p>";
        let out = sanitize_html(input);
        assert!(
            out.contains("\u{2014}"),
            "em-dash was mangled; got: {out:?}"
        );
        assert!(
            !out.contains('\u{00E2}'),
            "Latin-1 mojibake 'â' present; got: {out:?}"
        );
    }

    #[test]
    fn utf8_multibyte_chars_preserved_through_sanitize() {
        // Broad UTF-8 family: accented, CJK, emoji, em-dash in same string
        let input = "<p>caf\u{00E9}\u{2014}日本語\u{2014}fiesta\u{00F1}ata\u{2014}\u{1F389}</p>";
        let out = sanitize_html(input);
        // Each character class must survive intact
        assert!(out.contains('\u{00E9}'), "é lost; got: {out:?}");
        assert!(out.contains('\u{2014}'), "em-dash lost; got: {out:?}");
        assert!(out.contains("日本語"), "CJK lost; got: {out:?}");
        assert!(out.contains('\u{00F1}'), "ñ lost; got: {out:?}");
        assert!(out.contains('\u{1F389}'), "🎉 lost; got: {out:?}");
        // No mojibake bytes
        assert!(!out.contains('\u{00E2}'), "mojibake 'â' present; got: {out:?}");
    }

    #[test]
    fn utf8_preserved_with_anchor_tag_nearby() {
        // Ensures the non-anchor byte-walk path also handles multibyte when
        // an `<a>` tag is present (exercises the fallback branch after a
        // tag match). Use a format! so em-dashes are real UTF-8 bytes, not
        // Rust escape sequences.
        let input = format!(
            "<p>hello{}<a href=\"https://example.com\">link{}end</a>{}tail</p>",
            "\u{2014}", "\u{2014}", "\u{2014}"
        );
        let out = sanitize_html(&input);
        assert!(
            out.matches('\u{2014}').count() == 3,
            "expected 3 em-dashes; got: {out:?}"
        );
    }
}

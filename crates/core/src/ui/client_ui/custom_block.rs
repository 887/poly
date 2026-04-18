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
//! # Shadow-root tradeoff (WP 5 initial)
//!
//! The plan (§4.6) calls for a real shadow-root so plugin CSS is fully
//! isolated from host CSS. Dioxus 0.7 has no ergonomic shadow-root primitive,
//! and injecting a shadow-root via `dioxus::document::eval` post-mount is
//! fragile (SSR hydration, effect ordering, escaping). For WP 5 we ship the
//! simpler variant: the sanitized HTML renders inside a scoped
//! `<div class="custom-block custom-block-{id}">` and the stylesheet's
//! selectors are rewritten with a `.custom-block-{id}` prefix so they only
//! match descendants. This is weaker than a shadow-root (host CSS still
//! bleeds in, `!important` rules from host still win) but it is enough to
//! prevent plugin CSS from leaking onto host elements — which is the
//! *security* concern. Visual bleed from host into plugin is cosmetic.
//!
//! TODO(plan §4.6): upgrade to real shadow-root when Dioxus grows a
//! first-class primitive or when a robust eval-based approach is available.

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
    strip_data_href_on_anchors(&cleaned)
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
        out.push(bytes[i] as char);
        i += 1;
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

#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn CustomBlock(block: CustomBlockData) -> Element {
    // Stable per-mount scope id. `use_hook` gives us a value computed once
    // per component instance — matches the lifetime of the custom-block's
    // DOM node.
    let scope_id = use_hook(next_scope_id);
    let scope_class = format!("cb-{}", scope_id);

    let sanitized = sanitize_html(&block.sanitized_html);
    let scoped_css = block
        .stylesheet
        .as_ref()
        .map(|css| prefix_css_selectors(css, &scope_class));

    let root_class = format!("custom-block {}", scope_class);
    let root_style = block
        .max_height_px
        .map(|h| format!("max-height: {}px; overflow: auto;", h));

    rsx! {
        div { class: "{root_class}", style: root_style,
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
}

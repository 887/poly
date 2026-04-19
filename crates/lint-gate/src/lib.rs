//! poly-lint-gate — workspace-wide lint gate.
//!
//! All enforcement happens in `build.rs`; this library exists only because
//! cargo needs something to compile for the crate to be a dependency.
//! See `docs/plans/plan-component-lints.md`, `plan-context-menu-quality-control.md`,
//! and `plan-connected-routes-static-check.md`.

pub const VERSION: &str = "1";

// ─────────────────────────────────────────────────────────────────────────────
// action_id_naming — D25 kebab-case validator + per-file scanner mirrored from
// build/action_id_naming.rs so that `cargo test -p poly-lint-gate` can
// exercise the logic without depending on the build-script module path.
// Keep in sync with the build/ copy.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[allow(dead_code)]
pub mod action_id_naming {
    pub struct Violation {
        pub rule: String,
        pub path: String,
        pub line: u32,
        pub detail: String,
    }

    const DECL_KEYWORDS: &[&str] = &["MenuItem", "ComposerButton", "SidebarItem"];

    /// Per-file scan — returns violations for `src` at `path`.
    pub fn scan_src(src: &str, path: &str) -> Vec<Violation> {
        let mut out = Vec::new();
        let lines: Vec<&str> = src.lines().collect();
        let mut in_decl_depth: Option<i32> = None;
        let mut brace_depth: i32 = 0;

        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            let line_delta: i32 = line
                .chars()
                .map(|c| match c {
                    '{' => 1,
                    '}' => -1,
                    _ => 0,
                })
                .sum();

            let opens_decl = DECL_KEYWORDS
                .iter()
                .any(|kw| trimmed.contains(kw) && trimmed.contains('{'));

            let was_in_decl = in_decl_depth.is_some();

            if opens_decl && in_decl_depth.is_none() {
                in_decl_depth = Some(brace_depth + 1);
            }

            let currently_in_decl = in_decl_depth.is_some() || was_in_decl;

            if currently_in_decl {
                if let Some(id) = extract_id_field(trimmed) {
                    if !id.is_empty() && !is_kebab_case(&id) {
                        out.push(Violation {
                            rule: "action_id_naming".into(),
                            path: path.to_string(),
                            line: (line_idx as u32) + 1,
                            detail: format!(
                                "action ID '{id}' is not kebab-case — use lowercase letters, \
                                 digits, and hyphens only, starting with a letter \
                                 (e.g. 'invite-user'); file: {path}:{}",
                                line_idx + 1
                            ),
                        });
                    }
                }
            }

            brace_depth += line_delta;

            if let Some(entry_depth) = in_decl_depth {
                if brace_depth < entry_depth {
                    in_decl_depth = None;
                }
            }
        }

        out
    }

    /// Extract the string literal value after `id:` on a line.
    pub fn extract_id_field(line: &str) -> Option<String> {
        let pos = line.find("id:")?;
        let after = &line[pos + 3..];
        let after = after.trim_start();
        let after = after.strip_prefix('"')?;
        let end = after.find('"')?;
        Some(after[..end].to_string())
    }

    /// Returns `true` iff `s` matches `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`.
    ///
    /// This is the D25 kebab-case convention: starts with lowercase letter,
    /// segments separated by single hyphens, no trailing hyphen, no digits
    /// as the first character, no underscores, no uppercase.
    pub fn is_kebab_case(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        let mut chars = s.chars().peekable();

        // First character must be a lowercase ASCII letter.
        match chars.next() {
            Some(c) if c.is_ascii_lowercase() => {}
            _ => return false,
        }

        // Remaining characters: lowercase letters, digits, or hyphens.
        // A hyphen must not be followed by another hyphen (no trailing hyphen).
        let mut prev_was_hyphen = false;
        for c in chars {
            if c == '-' {
                if prev_was_hyphen {
                    return false;
                }
                prev_was_hyphen = true;
            } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
                prev_was_hyphen = false;
            } else {
                return false;
            }
        }

        !prev_was_hyphen
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ui_action_coverage — per-file scan logic mirrored from
// build/ui_action_coverage.rs so that `cargo test -p poly-lint-gate` can
// exercise the scanner without depending on the build-script module path.
// Keep in sync with the build/ copy.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[allow(dead_code)]
pub mod ui_action_coverage {
    pub struct Violation {
        pub rule: String,
        pub path: String,
        pub line: u32,
        pub detail: String,
    }

    pub fn scan(src: &str, path: &str) -> Vec<Violation> {
        let mut out = Vec::new();
        scan_rule_a(src, path, &mut out);
        scan_rule_b(src, path, &mut out);
        scan_rule_c(src, path, &mut out);
        out
    }

    fn scan_rule_a(src: &str, path: &str, out: &mut Vec<Violation>) {
        let event_names = [
            "onclick",
            "onchange",
            "onsubmit",
            "oninput",
            "onkeydown",
            "onkeyup",
            "onmousedown",
            "onmouseup",
            "ondblclick",
            "onfocus",
            "onblur",
        ];
        for (line_idx, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            for ev in &event_names {
                if !trimmed.contains(ev) {
                    continue;
                }
                let Some(after_colon) = find_handler_start(trimmed, ev) else {
                    continue;
                };
                let rest = after_colon.trim_start();
                let rest = rest.strip_prefix("move").map(str::trim_start).unwrap_or(rest);
                if !rest.starts_with('|') {
                    continue;
                }
                let after_open = &rest[1..];
                let Some(close_pipe) = after_open.find('|') else {
                    continue;
                };
                let after_params = after_open[close_pipe + 1..].trim_start();
                if is_empty_body(after_params) {
                    out.push(Violation {
                        rule: "ui_action_coverage".into(),
                        path: path.to_string(),
                        line: (line_idx as u32) + 1,
                        detail: format!(
                            "empty event handler — use ui_noop!(UiNoopReason::X) for decorative \
                             elements or implement the handler; file: {path}:{}",
                            line_idx + 1
                        ),
                    });
                    break;
                }
            }
        }
    }

    fn find_handler_start<'a>(line: &'a str, ev: &str) -> Option<&'a str> {
        let mut search = line;
        loop {
            let pos = search.find(ev)?;
            let after = &search[pos + ev.len()..];
            let after_ws = after.trim_start();
            if after_ws.starts_with(':') {
                return Some(&after_ws[1..]);
            }
            search = &search[pos + 1..];
        }
    }

    fn is_empty_body(s: &str) -> bool {
        if let Some(inner) = s.strip_prefix('{') {
            inner.trim_start().starts_with('}')
        } else if let Some(inner) = s.strip_prefix('(') {
            inner.trim_start().starts_with(')')
        } else {
            false
        }
    }

    fn scan_rule_b(src: &str, path: &str, out: &mut Vec<Violation>) {
        let lines: Vec<&str> = src.lines().collect();
        let mut in_component = false;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[component]") {
                in_component = true;
                continue;
            }
            if !in_component {
                continue;
            }
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("#[")
                || trimmed.starts_with("pub fn")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("pub(crate) fn")
            {
                continue;
            }
            if is_empty_rsx_on_line(trimmed) {
                out.push(Violation {
                    rule: "ui_action_coverage".into(),
                    path: path.to_string(),
                    line: (i as u32) + 1,
                    detail: format!(
                        "empty rsx! view body — implement the view or remove the route entry; \
                         file: {path}:{}",
                        i + 1
                    ),
                });
            }
            in_component = false;
        }
    }

    fn is_empty_rsx_on_line(s: &str) -> bool {
        let Some(after_rsx) = s.find("rsx!").map(|p| &s[p + 4..]) else {
            return false;
        };
        let rest = after_rsx.trim_start();
        let rest = if rest.starts_with('(') { &rest[1..].trim_start() } else { rest };
        if !rest.starts_with('{') {
            return false;
        }
        let inner = rest[1..].trim();
        inner.starts_with('}')
    }

    fn scan_rule_c(src: &str, path: &str, out: &mut Vec<Violation>) {
        for (line_idx, line) in src.lines().enumerate() {
            if !line.contains("ui_noop!") {
                continue;
            }
            let trimmed = line.trim();
            let mut search = trimmed;
            while let Some(pos) = search.find("ui_noop!(") {
                let after_open = &search[pos + "ui_noop!(".len()..];
                let inner = after_open.trim_start();
                if inner.starts_with("UiNoopReason::") {
                    search = &search[pos + 1..];
                    continue;
                }
                out.push(Violation {
                    rule: "ui_action_coverage".into(),
                    path: path.to_string(),
                    line: (line_idx as u32) + 1,
                    detail: format!(
                        "ui_noop! argument must be UiNoopReason::X — use one of DragHandle, \
                         ReadOnlyIndicator, DecorativeIcon, LayoutSpacer, EventBarrier, \
                         ProgressIndicator; file: {path}:{}",
                        line_idx + 1
                    ),
                });
                break;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// §7.3 scanner integration tests — parse_variants / scan_callsites logic
// copied from build/route_graph.rs so that `cargo test -p poly-lint-gate`
// exercises the scanner with a miniature routes fixture. The build-script copy
// remains the authoritative runtime version; keep the two in sync.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    dead_code
)]
mod scanner_tests {
    use std::collections::HashSet;

    struct RouteVariant {
        name: String,
        has_connected: bool,
        entry_point: bool,
        programmatic: Vec<String>,
    }

    fn parse_variants(src: &str) -> Vec<RouteVariant> {
        let lines: Vec<&str> = src.lines().collect();
        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            if !t.starts_with("#[route(") {
                continue;
            }
            let mut has_connected = false;
            let mut entry_point = false;
            let mut programmatic: Vec<String> = Vec::new();
            let mut up = i;
            while up > 0 {
                up -= 1;
                let s = lines[up].trim_start();
                if s.starts_with("#[connected(") {
                    has_connected = true;
                    let block = collect_attr_block(&lines, up);
                    let (ep, progs) = parse_connected_args(block);
                    if ep { entry_point = true; }
                    programmatic.extend(progs);
                    continue;
                }
                if s.starts_with("#[") || s.is_empty() || s.starts_with("//") { continue; }
                break;
            }
            let mut j = i + 1;
            while let Some(line) = lines.get(j) {
                let s = line.trim_start();
                if s.starts_with("#[") || s.is_empty() || s.starts_with("//") { j += 1; continue; }
                if let Some(end) = s.find(|c: char| !c.is_alphanumeric() && c != '_') {
                    if end > 0 {
                        let name = &s[..end];
                        if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                            out.push(RouteVariant { name: name.to_string(), has_connected, entry_point, programmatic });
                        }
                    }
                }
                break;
            }
        }
        out
    }

    fn collect_attr_block(lines: &[&str], start: usize) -> String {
        let mut buf = String::new();
        let mut depth: i32 = 0;
        for line in lines.iter().skip(start) {
            buf.push_str(line);
            buf.push('\n');
            for ch in line.chars() {
                match ch { '[' => depth += 1, ']' => depth -= 1, _ => {} }
            }
            if depth <= 0 && !buf.trim().is_empty() { break; }
        }
        buf
    }

    fn parse_connected_args(block: String) -> (bool, Vec<String>) {
        let Some(start) = block.find("#[connected(").map(|i| i + "#[connected(".len()) else { return (false, Vec::new()); };
        let bytes = block.as_bytes();
        let mut depth = 1i32;
        let mut end = start;
        while depth > 0 {
            let Some(b) = bytes.get(end) else { break };
            match *b { b'(' => depth += 1, b')' => depth -= 1, _ => {} }
            end += 1;
        }
        let args = &block[start..end.saturating_sub(1)];
        let mut entry_point = false;
        let mut programmatic = Vec::new();
        for part in args.split(',') {
            let p = part.trim();
            if p == "entry_point" { entry_point = true; }
            else if let Some(inner) = p.strip_prefix("programmatic<") {
                if let Some(name) = inner.strip_suffix('>') {
                    programmatic.push(name.trim().to_string());
                }
            }
        }
        (entry_point, programmatic)
    }

    fn scan_callsites(content: &str) -> HashSet<String> {
        let mut out = HashSet::new();
        for (m, _) in content.match_indices("Route::") {
            let rest = &content[m + "Route::".len()..];
            let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(rest.len());
            if end == 0 { continue; }
            let name = &rest[..end];
            if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                out.insert(name.to_string());
            }
        }
        out
    }

    const MINI_ROUTES: &str = r#"
#[derive(Connected)]
enum Route {
    #[connected(entry_point)]
    #[route("/")]
    HomeRoute,
    #[connected(linked)]
    #[route("/chat")]
    ChatRoute,
    #[connected(programmatic<PushOpener>)]
    #[route("/notify")]
    NotifyRoute,
    #[route("/orphan")]
    OrphanRoute,
}
"#;

    #[test]
    fn parses_four_variants() {
        let v = parse_variants(MINI_ROUTES);
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn detects_entry_point() {
        let v = parse_variants(MINI_ROUTES);
        let home = v.iter().find(|x| x.name == "HomeRoute").unwrap();
        assert!(home.entry_point);
        assert!(home.has_connected);
    }

    #[test]
    fn detects_programmatic_producer() {
        let v = parse_variants(MINI_ROUTES);
        let n = v.iter().find(|x| x.name == "NotifyRoute").unwrap();
        assert_eq!(n.programmatic, vec!["PushOpener"]);
    }

    #[test]
    fn orphan_has_no_connected() {
        let v = parse_variants(MINI_ROUTES);
        let o = v.iter().find(|x| x.name == "OrphanRoute").unwrap();
        assert!(!o.has_connected);
    }

    #[test]
    fn scan_callsites_extracts_route_refs() {
        let src = "nav!(Route::ChatRoute {}); Link { to: Route::HomeRoute } navigator().push(Route::NotifyRoute);";
        let sites = scan_callsites(src);
        assert!(sites.contains("ChatRoute"));
        assert!(sites.contains("HomeRoute"));
        assert!(sites.contains("NotifyRoute"));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// action_enum_coverage — per-file scan logic mirrored from
// build/action_enum_coverage.rs so that `cargo test -p poly-lint-gate` can
// exercise the scanner without depending on the build-script module path.
// Keep in sync with the build/ copy.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[allow(dead_code)]
pub mod action_enum_coverage {
    pub struct Violation {
        pub rule: String,
        pub path: String,
        pub line: u32,
        pub detail: String,
    }

    #[derive(PartialEq, Eq, Debug)]
    enum UiActionKind {
        Typed,
        None,
        Inherit,
        Missing,
    }

    pub fn scan_src(src: &str, path: &str) -> Vec<Violation> {
        let mut out = Vec::new();
        let lines: Vec<&str> = src.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            if !line.trim().starts_with("#[component]") {
                continue;
            }

            let annotation = find_ui_action_above(&lines, i);
            let fn_name =
                extract_fn_name_below(&lines, i).unwrap_or_else(|| "<unknown>".into());

            if annotation == UiActionKind::Missing {
                out.push(Violation {
                    rule: "action_enum_coverage".into(),
                    path: path.to_string(),
                    line: (i as u32) + 1,
                    detail: format!(
                        "#[component] fn {fn_name} missing #[ui_action(...)] — add one of \
                         `(YourActionEnum)` (typed actions), `(None)` (display-only), or \
                         `(inherit)` (sub-component delegates to parent)"
                    ),
                });
            }

            if annotation == UiActionKind::None {
                scan_rule_b_for_component(&lines, i, &fn_name, path, &mut out);
            }
        }

        out
    }

    fn find_ui_action_above(lines: &[&str], component_idx: usize) -> UiActionKind {
        let mut i = component_idx;
        while i > 0 {
            i -= 1;
            let Some(line) = lines.get(i) else { break };
            let t = line.trim();
            if t.is_empty() || t.starts_with("//") || t.starts_with("///") {
                continue;
            }
            if t.starts_with("#[") {
                if let Some(inner) = t.strip_prefix("#[ui_action(") {
                    let arg = inner
                        .trim_end_matches(|c: char| c == ')' || c == ']')
                        .trim();
                    return if arg.eq_ignore_ascii_case("none") {
                        UiActionKind::None
                    } else if arg.eq_ignore_ascii_case("inherit") {
                        UiActionKind::Inherit
                    } else {
                        UiActionKind::Typed
                    };
                }
                continue;
            }
            break;
        }
        UiActionKind::Missing
    }

    fn scan_rule_b_for_component(
        lines: &[&str],
        component_idx: usize,
        fn_name: &str,
        path: &str,
        out: &mut Vec<Violation>,
    ) {
        let event_names = [
            "onclick:",
            "onchange:",
            "onsubmit:",
            "oninput:",
            "onkeydown:",
            "onkeyup:",
            "onmousedown:",
            "onmouseup:",
            "ondblclick:",
            "onfocus:",
            "onblur:",
        ];

        let start = component_idx + 1;
        let end = (start + 200).min(lines.len());

        for (offset, line) in lines[start..end].iter().enumerate() {
            let abs_line = start + offset;
            let t = line.trim();
            if t.starts_with("#[component]") {
                break;
            }
            for ev in &event_names {
                if !t.contains(ev) {
                    continue;
                }
                if !t.contains("ui_noop!") {
                    out.push(Violation {
                        rule: "action_enum_coverage_none_with_handler".into(),
                        path: path.to_string(),
                        line: (abs_line as u32) + 1,
                        detail: format!(
                            "#[ui_action(None)] component {fn_name} has a non-noop event \
                             handler at line {} — either change to #[ui_action(SomeEnum)] \
                             or use ui_noop!(UiNoopReason::X)",
                            abs_line + 1
                        ),
                    });
                    break;
                }
            }
        }
    }

    fn extract_fn_name_below(lines: &[&str], component_idx: usize) -> Option<String> {
        for line in lines.iter().skip(component_idx + 1).take(10) {
            let t = line.trim_start();
            if t.starts_with("#[") || t.starts_with("//") || t.is_empty() {
                continue;
            }
            let after_fn = t.split_once(" fn ")?.1;
            let name_end = after_fn.find(|c: char| !c.is_alphanumeric() && c != '_')?;
            return Some(after_fn[..name_end].to_string());
        }
        None
    }
}

#[cfg(test)]
mod action_enum_coverage_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    #[test]
    fn missing_ui_action_is_violation() {
        let src = r#"
            #[component]
            pub fn MyWidget() -> Element {
                rsx! { div { "hello" } }
            }
        "#;
        let violations = super::action_enum_coverage::scan_src(src, "test.rs");
        assert!(
            violations.iter().any(|v| v.rule == "action_enum_coverage"),
            "#[component] without #[ui_action] should be a Rule A violation"
        );
    }

    #[test]
    fn ui_action_none_is_ok() {
        let src = r#"
            #[ui_action(None)]
            #[component]
            pub fn DisplayOnly() -> Element {
                rsx! { div { "read-only" } }
            }
        "#;
        let violations = super::action_enum_coverage::scan_src(src, "test.rs");
        assert!(
            violations.iter().all(|v| v.rule != "action_enum_coverage"),
            "#[ui_action(None)] should satisfy Rule A"
        );
    }

    #[test]
    fn ui_action_inherit_is_ok() {
        let src = r#"
            #[ui_action(inherit)]
            #[component]
            pub fn SubWidget() -> Element {
                rsx! { div { "child" } }
            }
        "#;
        let violations = super::action_enum_coverage::scan_src(src, "test.rs");
        assert!(
            violations.iter().all(|v| v.rule != "action_enum_coverage"),
            "#[ui_action(inherit)] should satisfy Rule A"
        );
    }

    #[test]
    fn ui_action_typed_is_ok() {
        let src = r#"
            #[ui_action(MyActionEnum)]
            #[component]
            pub fn ActionButton() -> Element {
                rsx! { button { "click me" } }
            }
        "#;
        let violations = super::action_enum_coverage::scan_src(src, "test.rs");
        assert!(
            violations.iter().all(|v| v.rule != "action_enum_coverage"),
            "#[ui_action(MyActionEnum)] should satisfy Rule A"
        );
    }

    #[test]
    fn ui_action_none_with_onclick_is_violation() {
        let src = r#"
            #[ui_action(None)]
            #[component]
            pub fn BadDisplay() -> Element {
                rsx! {
                    button {
                        onclick: move |_| { do_something(); },
                        "click"
                    }
                }
            }
        "#;
        let violations = super::action_enum_coverage::scan_src(src, "test.rs");
        assert!(
            violations
                .iter()
                .any(|v| v.rule == "action_enum_coverage_none_with_handler"),
            "#[ui_action(None)] with onclick (no ui_noop!) should be a Rule B violation"
        );
    }

    #[test]
    fn ui_action_none_with_noop_onclick_is_ok() {
        let src = r#"
            #[ui_action(None)]
            #[component]
            pub fn DecorativeIcon() -> Element {
                rsx! {
                    div {
                        onclick: move |_| ui_noop!(UiNoopReason::DecorativeIcon),
                        "icon"
                    }
                }
            }
        "#;
        let violations = super::action_enum_coverage::scan_src(src, "test.rs");
        assert!(
            violations
                .iter()
                .all(|v| v.rule != "action_enum_coverage_none_with_handler"),
            "#[ui_action(None)] with ui_noop! onclick should not be a Rule B violation"
        );
    }
}

#[cfg(test)]
mod ui_action_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    #[test]
    fn empty_onclick_is_violation() {
        let src = r#"
            #[component]
            fn Foo() -> Element {
                rsx! { button { onclick: move |_| {} } }
            }
        "#;
        let violations = super::ui_action_coverage::scan(src, "test.rs");
        assert!(!violations.is_empty(), "empty onclick should be a violation");
    }

    #[test]
    fn ui_noop_with_reason_is_ok() {
        let src = r#"
            onclick: move |_| ui_noop!(UiNoopReason::DragHandle),
        "#;
        let violations = super::ui_action_coverage::scan(src, "test.rs");
        assert!(violations.is_empty(), "ui_noop! with valid reason should not be flagged");
    }

    #[test]
    fn ui_noop_with_string_is_violation() {
        let src = r#"
            onclick: move |_| ui_noop!("decorative"),
        "#;
        let violations = super::ui_action_coverage::scan(src, "test.rs");
        assert!(!violations.is_empty(), "ui_noop! with string should be a violation");
    }

    #[test]
    fn ui_noop_without_arg_is_violation() {
        let src = r#"
            onclick: move |_| ui_noop!(),
        "#;
        let violations = super::ui_action_coverage::scan(src, "test.rs");
        assert!(!violations.is_empty(), "ui_noop! without arg should be a violation");
    }

    #[test]
    fn nonempty_onclick_is_ok() {
        let src = r#"
            onclick: move |_| {
                do_something();
            },
        "#;
        let violations = super::ui_action_coverage::scan(src, "test.rs");
        assert!(violations.is_empty(), "non-empty onclick should not be flagged");
    }

    #[test]
    fn empty_rsx_body_is_violation() {
        let src = r#"
            #[component]
            fn EmptyView() -> Element {
                rsx! {}
            }
        "#;
        let violations = super::ui_action_coverage::scan(src, "test.rs");
        assert!(!violations.is_empty(), "rsx! {{}} body should be a violation");
    }

    #[test]
    fn rsx_with_content_is_ok() {
        let src = r#"
            #[component]
            fn RealView() -> Element {
                rsx! { div { "Hello" } }
            }
        "#;
        let violations = super::ui_action_coverage::scan(src, "test.rs");
        assert!(violations.is_empty(), "rsx with content should not be flagged");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// forbid_backend_slug_match — per-file scan logic mirrored from
// build/forbid_backend_slug_match.rs so that `cargo test -p poly-lint-gate`
// can exercise the scanner without depending on the build-script module path.
// Keep in sync with the build/ copy.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[allow(dead_code)]
pub mod forbid_backend_slug_match {
    pub struct Violation {
        pub rule: String,
        pub path: String,
        pub line: u32,
        pub detail: String,
    }

    const BACKEND_SLUG_LITERALS: &[&str] = &[
        "\"discord\"",
        "\"stoat\"",
        "\"matrix\"",
        "\"teams\"",
        "\"demo\"",
        "\"poly\"",
        "\"lemmy\"",
        "\"hackernews\"",
        "\"github\"",
        "\"forgejo\"",
    ];

    const LOOKAHEAD_LINES: usize = 30;

    pub fn scan_src(src: &str, path: &str) -> Vec<Violation> {
        let mut out = Vec::new();
        if !path.contains("crates/core/src/ui/") {
            return out;
        }
        let lines: Vec<&str> = src.lines().collect();
        for (idx, raw) in lines.iter().enumerate() {
            if !is_match_as_str_line(raw) {
                continue;
            }
            let start = idx + 1;
            let end = (start + LOOKAHEAD_LINES).min(lines.len());
            let follows_slug = lines
                .get(start..end)
                .unwrap_or(&[])
                .iter()
                .any(|l| {
                    BACKEND_SLUG_LITERALS.iter().any(|slug| l.contains(slug))
                });
            if !follows_slug {
                continue;
            }
            out.push(Violation {
                rule: "forbid_backend_slug_match_in_ui".to_string(),
                path: path.to_string(),
                line: (idx as u32) + 1,
                detail:
                    "slug-match found in UI — use plugin-declared items \
                     (see plan-client-ui-surface.md §4)"
                        .to_string(),
            });
        }
        out
    }

    pub fn is_match_as_str_line(line: &str) -> bool {
        let t = line.trim_start();
        if t.starts_with("//") {
            return false;
        }
        if !t.contains("match ") {
            return false;
        }
        if !t.contains(".as_str()") {
            return false;
        }
        let Some((_, after)) = t.split_once("match ") else {
            return false;
        };
        let Some(as_str_pos) = after.find(".as_str()") else {
            return false;
        };
        let expr = &after[..as_str_pos];
        if expr.contains('{') || expr.contains(';') {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod forbid_backend_slug_match_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    const UI_PATH: &str = "crates/core/src/ui/fixture.rs";

    #[test]
    fn forbid_slug_match_detects_discord_arm() {
        let src = r#"
            fn icon(bt: &BackendType) -> &'static str {
                match bt.as_str() {
                    "discord" => "🟣",
                    "matrix" => "🔵",
                    _ => "⬜",
                }
            }
        "#;
        let violations = super::forbid_backend_slug_match::scan_src(src, UI_PATH);
        assert!(
            violations
                .iter()
                .any(|v| v.rule == "forbid_backend_slug_match_in_ui"),
            "`match bt.as_str()` with slug arms in UI must be flagged"
        );
    }

    #[test]
    fn forbid_slug_match_allows_channel_type_match() {
        let src = r#"
            fn icon(ch: &Channel) -> &'static str {
                match ch.channel_type {
                    ChannelType::Forum => "📋",
                    ChannelType::Text => "💬",
                    _ => "❓",
                }
            }
        "#;
        let violations = super::forbid_backend_slug_match::scan_src(src, UI_PATH);
        assert!(
            violations.is_empty(),
            "typed enum match (ChannelType::…) must not be flagged; got {} violations",
            violations.len()
        );
    }

    #[test]
    fn forbid_slug_match_allows_out_of_ui_dir() {
        // Same offending source, but the path is outside `crates/core/src/ui/`.
        // State/plugin/bridge layers legitimately map slugs — they are out of
        // scope for this scanner.
        let src = r#"
            match slug.as_str() {
                "discord" => "Discord",
                "matrix" => "Matrix",
                _ => slug,
            }
        "#;
        let non_ui_path = "clients/client/src/types.rs";
        let violations = super::forbid_backend_slug_match::scan_src(src, non_ui_path);
        assert!(
            violations.is_empty(),
            "files outside crates/core/src/ui/ must not be flagged"
        );
    }

    #[test]
    fn forbid_slug_match_ignores_unrelated_as_str_match() {
        // `match X.as_str()` with no backend-slug literals in the next 30
        // lines must not fire — the scan is slug-specific, not a blanket
        // ban on string matching.
        let src = r#"
            fn level(v: &str) -> u8 {
                match v.as_str() {
                    "low" => 0,
                    "high" => 2,
                    _ => 1,
                }
            }
        "#;
        let violations = super::forbid_backend_slug_match::scan_src(src, UI_PATH);
        assert!(
            violations.is_empty(),
            "non-slug match on .as_str() must not be flagged"
        );
    }

    #[test]
    fn is_match_as_str_line_guards_against_false_positives() {
        assert!(super::forbid_backend_slug_match::is_match_as_str_line(
            "            match bt.as_str() {"
        ));
        assert!(!super::forbid_backend_slug_match::is_match_as_str_line(
            "// match bt.as_str() — description only"
        ));
        assert!(!super::forbid_backend_slug_match::is_match_as_str_line(
            "let hits: Vec<_> = text.match_indices(\"foo\").collect();"
        ));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ftl_label_key_coverage — per-file scan logic mirrored from
// build/ftl_label_key_coverage.rs so that `cargo test -p poly-lint-gate` can
// exercise the scanner without depending on the build-script module path.
// Keep in sync with the build/ copy.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[allow(dead_code)]
pub mod ftl_label_key_coverage {
    use std::collections::HashSet;

    pub struct Violation {
        pub rule: String,
        pub path: String,
        pub line: u32,
        pub detail: String,
    }

    /// Per-file scan. `ftl_keys`: message IDs from the plugin's English bundle.
    pub fn scan_src(
        src: &str,
        path: &str,
        ftl_keys: &HashSet<String>,
        plugin_name: &str,
    ) -> Vec<Violation> {
        let mut out = Vec::new();
        for (line_idx, line) in src.lines().enumerate() {
            let trimmed = line.trim();
            if !trimmed.contains("label_key") && !trimmed.contains("label-key") {
                continue;
            }
            if let Some(key) = extract_label_key(trimmed) {
                if key.is_empty() {
                    continue;
                }
                if !ftl_keys.contains(&key) {
                    let ftl_hint = format!("clients/{plugin_name}/locales/en/*.ftl");
                    out.push(Violation {
                        rule: "ftl_label_key_coverage".into(),
                        path: path.to_string(),
                        line: (line_idx as u32) + 1,
                        detail: format!(
                            "FTL key '{key}' declared but missing from bundle; \
                             expected in {ftl_hint}; file: {path}:{}",
                            line_idx + 1
                        ),
                    });
                }
            }
        }
        out
    }

    /// Extract the string literal value after `label_key:` or `label-key:`.
    pub fn extract_label_key(line: &str) -> Option<String> {
        for prefix in &["label_key", "label-key"] {
            let Some(pos) = line.find(prefix) else { continue };
            let after = &line[pos + prefix.len()..];
            let after = after.trim_start();
            let after = after.strip_prefix(':')?.trim_start();
            let after = after.strip_prefix('"')?;
            let end = after.find('"')?;
            return Some(after[..end].to_string());
        }
        None
    }

    /// Parse FTL message identifiers from raw FTL source text.
    pub fn parse_ftl_keys(ftl_source: &str) -> HashSet<String> {
        let mut keys = HashSet::new();
        for line in ftl_source.lines() {
            let first = line.chars().next().unwrap_or(' ');
            if !first.is_ascii_alphanumeric() {
                continue;
            }
            if let Some(eq_pos) = line.find(" =") {
                let id = &line[..eq_pos];
                if id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    keys.insert(id.to_string());
                }
            }
        }
        keys
    }
}

#[cfg(test)]
mod ftl_label_key_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    fn make_ftl_keys(keys: &[&str]) -> std::collections::HashSet<String> {
        keys.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ftl_label_key_missing_is_violation() {
        let src = r#"
            MenuItem {
                label_key: "bogus-key",
                id: "invite-user",
            }
        "#;
        let ftl_keys = make_ftl_keys(&["other-key"]);
        let violations =
            super::ftl_label_key_coverage::scan_src(src, "test.rs", &ftl_keys, "demo");
        assert!(
            violations.iter().any(|v| v.rule == "ftl_label_key_coverage"),
            "missing FTL key should be a violation"
        );
        let v = violations
            .iter()
            .find(|v| v.rule == "ftl_label_key_coverage")
            .unwrap();
        assert!(v.detail.contains("bogus-key"), "violation detail should name the missing key");
    }

    #[test]
    fn ftl_label_key_present_is_ok() {
        let src = r#"
            MenuItem {
                label_key: "invite-user-label",
                id: "invite-user",
            }
        "#;
        let ftl_keys = make_ftl_keys(&["invite-user-label", "other-key"]);
        let violations =
            super::ftl_label_key_coverage::scan_src(src, "test.rs", &ftl_keys, "demo");
        assert!(
            violations.iter().all(|v| v.rule != "ftl_label_key_coverage"),
            "present FTL key should not be a violation"
        );
    }

    #[test]
    fn parse_ftl_keys_extracts_message_ids() {
        let ftl = "invite-user = Invite User\nmute-server = Mute\n# comment\n.attr = nope\n";
        let keys = super::ftl_label_key_coverage::parse_ftl_keys(ftl);
        assert!(keys.contains("invite-user"));
        assert!(keys.contains("mute-server"));
        assert!(!keys.contains(".attr"));
    }
}

#[cfg(test)]
mod action_id_naming_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    #[test]
    fn action_id_kebab_case_is_ok() {
        let src = r#"
            MenuItem {
                id: "invite-user",
                label_key: "invite-user-label",
            }
        "#;
        let violations = super::action_id_naming::scan_src(src, "test.rs");
        assert!(
            violations.iter().all(|v| v.rule != "action_id_naming"),
            "kebab-case 'invite-user' should not be a violation"
        );
    }

    #[test]
    fn action_id_snake_case_is_violation() {
        let src = r#"
            MenuItem {
                id: "invite_user",
            }
        "#;
        let violations = super::action_id_naming::scan_src(src, "test.rs");
        assert!(
            violations.iter().any(|v| v.rule == "action_id_naming"),
            "snake_case 'invite_user' should be a violation"
        );
        let v = violations
            .iter()
            .find(|v| v.rule == "action_id_naming")
            .unwrap();
        assert!(v.detail.contains("invite_user"));
    }

    #[test]
    fn action_id_camel_case_is_violation() {
        let src = r#"
            SidebarItem {
                id: "inviteUser",
            }
        "#;
        let violations = super::action_id_naming::scan_src(src, "test.rs");
        assert!(
            violations.iter().any(|v| v.rule == "action_id_naming"),
            "camelCase 'inviteUser' should be a violation"
        );
    }

    #[test]
    fn action_id_with_leading_digit_is_violation() {
        let src = r#"
            ComposerButton {
                id: "2fa-setup",
            }
        "#;
        let violations = super::action_id_naming::scan_src(src, "test.rs");
        assert!(
            violations.iter().any(|v| v.rule == "action_id_naming"),
            "'2fa-setup' starts with digit — should be a violation"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WP 1 — D25 kebab-case validator tests (plan §7 item 3)
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod kebab_case_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::action_id_naming::is_kebab_case;

    #[test]
    fn invite_user_is_ok() {
        assert!(is_kebab_case("invite-user"), "'invite-user' must be valid kebab-case");
    }

    #[test]
    fn invite_underscore_user_is_fail() {
        assert!(!is_kebab_case("invite_user"), "'invite_user' must fail (underscore)");
    }

    #[test]
    fn invite_camel_user_is_fail() {
        assert!(!is_kebab_case("inviteUser"), "'inviteUser' must fail (camelCase)");
    }

    #[test]
    fn digit_start_is_fail() {
        assert!(!is_kebab_case("2fa-setup"), "'2fa-setup' must fail (starts with digit)");
    }

    #[test]
    fn empty_string_is_fail() {
        assert!(!is_kebab_case(""), "empty string must fail");
    }

    // Additional edge-case coverage.

    #[test]
    fn single_word_lowercase_is_ok() {
        assert!(is_kebab_case("mute"), "'mute' must be valid kebab-case");
    }

    #[test]
    fn trailing_hyphen_is_fail() {
        assert!(!is_kebab_case("mute-"), "trailing hyphen must fail");
    }

    #[test]
    fn double_hyphen_is_fail() {
        assert!(!is_kebab_case("mute--server"), "double hyphen must fail");
    }

    #[test]
    fn uppercase_letter_is_fail() {
        assert!(!is_kebab_case("Mute-server"), "uppercase first letter must fail");
    }

    #[test]
    fn digits_in_segment_are_ok() {
        assert!(is_kebab_case("enable-2fa"), "'enable-2fa' must be valid kebab-case");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// custom_block_usage — Pack G P40 threshold scanner mirrored from
// build/custom_block_usage.rs so that `cargo test -p poly-lint-gate` can
// exercise the counting logic without depending on the build-script module path.
// Keep in sync with the build/ copy.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
pub mod custom_block_usage {
    pub fn count_custom_block_literals(src: &str) -> usize {
        let bytes = src.as_bytes();
        let needle = b"CustomBlock";
        let mut count = 0usize;
        let mut i = 0usize;

        while i + needle.len() <= bytes.len() {
            if &bytes[i..i + needle.len()] == needle {
                let prev_ok = i == 0 || !is_ident_char(bytes[i - 1]);
                let mut j = i + needle.len();
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                let next_ok = j < bytes.len() && bytes[j] == b'{';
                if prev_ok && next_ok {
                    count += 1;
                    i = j + 1;
                    continue;
                }
            }
            i += 1;
        }

        count
    }

    fn is_ident_char(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_'
    }

    pub fn plugin_id_from_path(p: &str) -> Option<String> {
        let idx = p.find("clients/")?;
        let after = &p[idx + "clients/".len()..];
        let end = after.find('/')?;
        Some(after[..end].to_string())
    }
}

#[cfg(test)]
mod custom_block_usage_tests {
    use super::custom_block_usage::{count_custom_block_literals, plugin_id_from_path};

    #[test]
    fn counts_single_literal() {
        let src = "let x = CustomBlock { sanitized_html: \"\".into() };";
        assert_eq!(count_custom_block_literals(src), 1);
    }

    #[test]
    fn counts_multiple_literals() {
        let src = r#"
            let a = CustomBlock { x: 1 };
            let b = CustomBlock { y: 2 };
            let c = CustomBlock{z:3};
        "#;
        assert_eq!(count_custom_block_literals(src), 3);
    }

    #[test]
    fn ignores_substring_match() {
        // `MyCustomBlock {` should not count.
        let src = "struct MyCustomBlock { field: u32 }";
        assert_eq!(count_custom_block_literals(src), 0);
    }

    #[test]
    fn ignores_type_alias_no_brace() {
        // `CustomBlock;` (no `{`) should not count.
        let src = "type X = CustomBlock;";
        assert_eq!(count_custom_block_literals(src), 0);
    }

    #[test]
    fn ignores_type_position_in_signature() {
        // `fn foo() -> CustomBlock` — no `{` follows the identifier on the
        // same scan, since the next non-whitespace char is end-of-line.
        let src = "fn foo() -> CustomBlock\n{ unimplemented!() }";
        // The newline counts as non-whitespace under our scanner (only spaces
        // and tabs are skipped). Confirmed by the asserted count.
        assert_eq!(count_custom_block_literals(src), 0);
    }

    #[test]
    fn plugin_id_extracted_from_path() {
        assert_eq!(
            plugin_id_from_path("clients/lemmy/src/lib.rs").as_deref(),
            Some("lemmy")
        );
        assert_eq!(
            plugin_id_from_path("/abs/path/clients/discord/src/api.rs").as_deref(),
            Some("discord")
        );
        assert_eq!(plugin_id_from_path("crates/core/src/lib.rs"), None);
    }
}

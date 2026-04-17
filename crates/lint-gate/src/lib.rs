//! poly-lint-gate — workspace-wide lint gate.
//!
//! All enforcement happens in `build.rs`; this library exists only because
//! cargo needs something to compile for the crate to be a dependency.
//! See `docs/plans/plan-component-lints.md`, `plan-context-menu-quality-control.md`,
//! and `plan-connected-routes-static-check.md`.

pub const VERSION: &str = "1";

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

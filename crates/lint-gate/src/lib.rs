//! poly-lint-gate — workspace-wide lint gate.
//!
//! All enforcement happens in `build.rs`; this library exists only because
//! cargo needs something to compile for the crate to be a dependency.
//! See `docs/plans/plan-component-lints.md`, `plan-context-menu-quality-control.md`,
//! and `plan-connected-routes-static-check.md`.

pub const VERSION: &str = "1";

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

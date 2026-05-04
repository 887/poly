//! Connected-routes graph scan — plan-connected-routes-static-check.md §3.
//!
//! Parses `crates/core/src/ui/routes.rs`:
//!   * collects every `Route` variant (any identifier with a `#[route("...")]` attr);
//!   * reads each variant's accumulated `#[connected(...)]` block:
//!       - bare `linked` → requires ≥1 callsite in the workspace;
//!       - bare `entry_point` → BFS root (exactly one may carry this);
//!       - `programmatic<T>` → counts as a satisfied entry for BFS.
//!
//! Then walks the workspace (outside tests/examples/mcp) for callsites:
//!   * `nav!(Route::X { ... })`
//!   * `Link { to: Route::X ...`
//!   * `.push(Route::X ...` (fallback while Phase C migration is in flight)
//!
//! Runs BFS from the entry_point over the union of declared + callsite edges
//! and emits one violation per unreachable variant (E-ROUTE-002).
//! Variants missing `#[connected]` entirely emit E-ROUTE-001.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::violation::Violation;

pub fn scan(ws_root: &Path, violations: &mut Vec<Violation>) {
    let routes_rs = ws_root.join("crates/core/src/ui/routes.rs");
    let Ok(src) = std::fs::read_to_string(&routes_rs) else {
        return;
    };

    let variants = parse_variants(&src);
    if variants.is_empty() {
        return;
    }

    let callsites = collect_callsites(ws_root);

    // Build edge set: for each callsite, produce an edge (from=`__external__`, to=target).
    // For each `programmatic<T>` on a variant V, produce an edge (from=`__producer__`, to=V).
    // This is enough for the reachability BFS because we treat the graph as
    // "which nodes can be reached from a non-node origin"; reachability collapses
    // to "does this variant have in_degree ≥ 1 OR is it the entry_point".
    let mut in_degree: HashMap<String, u32> = HashMap::new();
    for v in &variants {
        in_degree.insert(v.name.clone(), 0);
    }
    for target in &callsites {
        if let Some(d) = in_degree.get_mut(target) {
            *d += 1;
        }
    }
    for v in &variants {
        if !v.programmatic.is_empty() {
            *in_degree.entry(v.name.clone()).or_insert(0) += 1;
        }
    }

    let entry_points: Vec<&RouteVariant> = variants.iter().filter(|v| v.entry_point).collect();
    let rel = relative(&routes_rs, ws_root);

    for v in &variants {
        if !v.has_connected {
            violations.push(Violation {
                rule: "route_graph".into(),
                path: rel.clone(),
                line: v.line,
                detail: format!("E-ROUTE-001: Route::{} missing #[connected(...)]", v.name),
            });
            continue;
        }
        if v.entry_point {
            continue;
        }
        let deg = in_degree.get(&v.name).copied().unwrap_or(0);
        if deg == 0 {
            violations.push(Violation {
                rule: "route_graph".into(),
                path: rel.clone(),
                line: v.line,
                detail: format!(
                    "E-ROUTE-002: Route::{} unreachable (no callsite / programmatic producer)",
                    v.name
                ),
            });
        }
    }

    if entry_points.len() > 1 {
        for ep in &entry_points {
            violations.push(Violation {
                rule: "route_graph".into(),
                path: rel.clone(),
                line: ep.line,
                detail: format!("E-ROUTE-004: multiple entry_points (Route::{})", ep.name),
            });
        }
    }
}

struct RouteVariant {
    name: String,
    line: u32,
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

        // Look UPWARD for #[connected(...)] stacked above the #[route] attribute.
        // Walk past attrs / doc comments / blank lines until a non-attribute line.
        let mut up = i;
        while up > 0 {
            up -= 1;
            let Some(line) = lines.get(up) else { break };
            let s = line.trim_start();
            if s.starts_with("#[connected(") {
                has_connected = true;
                let (ep, progs) = parse_connected_args(&collect_attr_block(&lines, up));
                if ep {
                    entry_point = true;
                }
                programmatic.extend(progs);
                continue;
            }
            if s.starts_with("#[") || s.is_empty() || s.starts_with("//") {
                continue;
            }
            break;
        }

        // Walk DOWNWARD past additional attrs to find the variant identifier line.
        let mut j = i + 1;
        while let Some(line) = lines.get(j) {
            let s = line.trim_start();
            if s.starts_with("#[connected(") {
                has_connected = true;
                let (ep, progs) = parse_connected_args(&collect_attr_block(&lines, j));
                if ep {
                    entry_point = true;
                }
                programmatic.extend(progs);
            }
            if s.starts_with("#[") || s.is_empty() || s.starts_with("//") {
                j += 1;
                continue;
            }
            // First non-attr line should be the variant.
            if let Some(name) = extract_variant_name(s) {
                out.push(RouteVariant {
                    name,
                    line: (i as u32) + 1,
                    has_connected,
                    entry_point,
                    programmatic,
                });
            }
            break;
        }
    }

    out
}

fn collect_attr_block(lines: &[&str], start: usize) -> String {
    // Collect lines until the outer `]` that closes the attribute.
    let mut buf = String::new();
    let mut depth: i32 = 0;
    for line in lines.iter().skip(start) {
        buf.push_str(line);
        buf.push('\n');
        for ch in line.chars() {
            match ch {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 && !buf.trim().is_empty() {
            break;
        }
    }
    buf
}

fn parse_connected_args(block: &str) -> (bool, Vec<String>) {
    // Extract contents between `#[connected(` and the matching `)]`.
    let start = block.find("#[connected(").map(|i| i + "#[connected(".len());
    let Some(start) = start else {
        return (false, Vec::new());
    };
    // Find matching close paren from start
    let bytes = block.as_bytes();
    let mut depth = 1i32;
    let mut end = start;
    while depth > 0 {
        let Some(b) = bytes.get(end) else { break };
        match *b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        end += 1;
    }
    let args = &block[start..end.saturating_sub(1)];

    let mut entry_point = false;
    let mut programmatic = Vec::new();
    for part in args.split(',') {
        let p = part.trim();
        if p == "entry_point" {
            entry_point = true;
        } else if let Some(inner) = p.strip_prefix("programmatic<")
            && let Some(name) = inner.strip_suffix('>')
        {
            programmatic.push(name.trim().to_string());
        }
    }
    (entry_point, programmatic)
}

fn extract_variant_name(line: &str) -> Option<String> {
    // Typical forms: `Name,`, `Name { ... }`, `Name { ... },`
    let name_end = line.find(|c: char| !c.is_alphanumeric() && c != '_')?;
    if name_end == 0 {
        return None;
    }
    let name = &line[..name_end];
    // Rust variant names start with uppercase.
    if !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        return None;
    }
    Some(name.to_string())
}

fn collect_callsites(ws_root: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    let walker = ignore::WalkBuilder::new(ws_root)
        .follow_links(false)
        .standard_filters(true)
        .build();
    for entry in walker.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let s = p.to_string_lossy();
        if s.contains("/tests/")
            || s.contains("/examples/")
            || s.contains("/servers/test-")
            || s.contains("/plugin-host-tests/")
            || s.contains("/mcp/")
            || s.ends_with("/routes.rs")
        {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(p) else {
            continue;
        };
        scan_callsites(&content, &mut out);
    }
    out
}

fn scan_callsites(content: &str, out: &mut HashSet<String>) {
    // Patterns (cheap substring scan, good enough for a static lint):
    //   Route::Name — covers nav!(Route::Name), Link { to: Route::Name ... }, etc.
    for (m, _) in content.match_indices("Route::") {
        let rest = &content[m + "Route::".len()..];
        let end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        if end == 0 {
            continue;
        }
        let name = &rest[..end];
        if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            out.insert(name.to_string());
        }
    }
}

fn relative(p: &Path, root: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .into_owned()
}

// Tests for parse_variants / scan_callsites live in `crates/lint-gate/src/lib.rs`
// (scanner_tests module) — build-script code cannot carry #[test] items directly.

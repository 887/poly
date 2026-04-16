#!/usr/bin/env python3
"""
Bulk-insert #[context_menu(inherit)] above every #[component] line that
is listed as a context_menu_coverage violation in baseline.json.

Also adds poly-ui-macros to Cargo.toml [dependencies] for crates that
need it, and adds `use poly_ui_macros::context_menu;` to each file.
"""

import json
import re
import sys
from collections import defaultdict
from pathlib import Path

WORKSPACE = Path(__file__).parent.parent
BASELINE = WORKSPACE / "crates/lint-gate/baseline.json"

# Crates that already have poly-ui-macros in their Cargo.toml
ALREADY_HAS_MACROS = {"crates/core"}

# All non-core crates that appear in violations (need Cargo.toml edit)
NEEDS_CARGO_EDIT_PREFIXES = [
    "apps/desktop-devtools",
    "clients/discord",
    "clients/forgejo",
    "clients/github",
    "clients/hackernews",
    "clients/lemmy",
    "clients/server-client",
    "clients/stoat",
    "clients/teams",
]


def crate_prefix(rel_path: str) -> str:
    """Return e.g. 'crates/core' or 'clients/discord' from a relative path."""
    parts = rel_path.split("/")
    return "/".join(parts[:2])


def add_cargo_dep(cargo_toml: Path):
    text = cargo_toml.read_text()
    if "poly-ui-macros" in text:
        return  # already present
    # Insert after [dependencies]
    new_text = re.sub(
        r'(\[dependencies\])',
        r'\1\npoly-ui-macros = { workspace = true }',
        text,
        count=1,
    )
    if new_text == text:
        print(f"  WARNING: could not insert dep in {cargo_toml}", file=sys.stderr)
        return
    cargo_toml.write_text(new_text)
    print(f"  Added poly-ui-macros dep to {cargo_toml.relative_to(WORKSPACE)}")


def has_context_menu_import(lines: list[str]) -> bool:
    for line in lines:
        if "use poly_ui_macros" in line and "context_menu" in line:
            return True
        if re.match(r'\s*use poly_ui_macros\s*;', line):
            return True
    return False


def add_import(lines: list[str]) -> list[str]:
    """Insert `use poly_ui_macros::context_menu;` near the top use-block."""
    # Find the last 'use ' line in the preamble (before first fn/struct/impl/mod/pub)
    insert_after = -1
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("use "):
            insert_after = i
        # Stop looking once we hit code
        if re.match(r'\s*(pub\s+)?(fn|struct|impl|mod|enum|trait|type|const|static)', line):
            break
    if insert_after == -1:
        # No use statements found; insert at top (after any #! attrs)
        for i, line in enumerate(lines):
            if not (line.strip().startswith("#!") or line.strip() == "" or line.strip().startswith("//")):
                insert_after = i - 1
                break
        if insert_after < 0:
            insert_after = 0
    indent = ""
    result = lines[:]
    result.insert(insert_after + 1, f"{indent}use poly_ui_macros::context_menu;\n")
    return result


def already_has_context_menu_above(lines: list[str], component_idx: int) -> bool:
    """Mirror the scanner's has_context_menu_above logic."""
    i = component_idx
    while i > 0:
        i -= 1
        t = lines[i].strip()
        if t == "" or t.startswith("//") or t.startswith("///"):
            continue
        if t.startswith("#["):
            if t.startswith("#[context_menu"):
                return True
            continue
        break
    return False


def insert_context_menu(lines: list[str], line_no: int) -> list[str]:
    """
    line_no is 1-based (from violation). Insert #[context_menu(inherit)] on
    the line immediately before lines[line_no - 1], matching its indentation.
    """
    idx = line_no - 1  # 0-based
    if idx >= len(lines):
        print(f"  WARNING: line {line_no} out of range (file has {len(lines)} lines)", file=sys.stderr)
        return lines

    target = lines[idx]
    # Compute indentation of the #[component] line
    indent = len(target) - len(target.lstrip())
    indentation = target[:indent]

    if already_has_context_menu_above(lines, idx):
        return lines  # skip — already annotated

    attr_line = f"{indentation}#[context_menu(inherit)]\n"
    result = lines[:idx] + [attr_line] + lines[idx:]
    return result


def process_file(abs_path: Path, line_numbers: list[int], need_import: bool) -> int:
    """Returns number of attributes actually inserted."""
    lines = abs_path.read_text().splitlines(keepends=True)

    # Add import if needed
    if need_import and not has_context_menu_import(lines):
        lines = add_import(lines)

    inserted = 0
    # Process line numbers in descending order so earlier insertions don't shift later ones
    for ln in sorted(line_numbers, reverse=True):
        before = len(lines)
        lines = insert_context_menu(lines, ln)
        if len(lines) > before:
            inserted += 1

    abs_path.write_text("".join(lines))
    return inserted


def main():
    with open(BASELINE) as f:
        data = json.load(f)

    violations = [v for v in data["violations"] if v["rule"] == "context_menu_coverage"]
    print(f"Found {len(violations)} context_menu_coverage violations")

    by_file: dict[str, list[int]] = defaultdict(list)
    for v in violations:
        by_file[v["path"]].append(v["line"])

    # Determine which crates need Cargo.toml edits
    crates_needing_cargo = set()
    for rel_path in by_file:
        prefix = crate_prefix(rel_path)
        if not any(rel_path.startswith(p) for p in ["crates/core", "crates/ui-macros"]):
            crates_needing_cargo.add(prefix)

    # Add Cargo.toml deps
    for crate in sorted(crates_needing_cargo):
        cargo_toml = WORKSPACE / crate / "Cargo.toml"
        if cargo_toml.exists():
            add_cargo_dep(cargo_toml)
        else:
            print(f"  WARNING: {cargo_toml} not found", file=sys.stderr)

    total_inserted = 0
    files_modified = 0

    for rel_path, line_nos in sorted(by_file.items()):
        abs_path = WORKSPACE / rel_path
        if not abs_path.exists():
            print(f"  WARNING: {abs_path} not found", file=sys.stderr)
            continue

        # Determine if this file needs an import
        prefix = crate_prefix(rel_path)
        need_import = True  # always add unless already present (checked inside)

        n = process_file(abs_path, line_nos, need_import)
        total_inserted += n
        files_modified += 1
        print(f"  {rel_path}: inserted {n}/{len(line_nos)} attrs")

    print(f"\nDone: {files_modified} files modified, {total_inserted} attributes inserted")


if __name__ == "__main__":
    main()

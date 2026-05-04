# lint-gate baseline.json

`baseline.json` is the grandfather list for lint violations that existed before
a lint rule was introduced. The `lint-gate` build script reads it at compile time
and suppresses failures for any violation whose file + message appears in the list.

The file is currently `{"violations": []}` — empty by design. Every lint rule in
this codebase was introduced with a clean slate (zero pre-existing violations to
grandfather) or with per-file inline `// poly-lint: allow …` suppressions rather
than a bulk baseline entry.

**Do not add entries here speculatively.** The baseline is a one-time emergency
escape hatch for "we need to land a lint today and there are 200 pre-existing
sites we'll clean up next sprint." If you're doing a normal migration, inline the
allow comments at the call sites instead — they stay co-located with the code and
are removed when the migration is complete.

**When the baseline IS the right tool:** a giant automated migration that
temporarily breaks 50+ lint sites across 20 files, where inline comments would
create unreadable noise during the transition. Add the entries, land the migration,
remove the entries. Net delta: zero baseline entries after the migration.

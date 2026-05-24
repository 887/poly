# Plan: SOLID + missing-impl audit — `clients/github/`

## Status: ✅ DONE — Phase A shipped (A.1+A.2 in change `zwmsmpmk` / commit `59b100cc`; A.3 skipped — cosmetic). Phase B fully shipped (B.1 in `6e2cd0a1`, B.2/B.4 via `common-forge` crate, B.3+B.5 in earlier waves). Phase C: C.3 shipped (mapping_tests.rs split); C.1 (octocrab migration) and C.2 (trait fan-out via `as_xxx()`) honestly deferred — both require multi-crate / multi-backend coordination outside this audit's scope.

Audit pass over `clients/github/src/{api.rs,lib.rs,mapping.rs,signup.rs,types.rs}`
(2921 LoC). Identifies SOLID violations and missing implementations.

Scope: only `clients/github/`. Do NOT touch other client crates.

---

## Phase A — Ship-now wins (≤50 LoC each, max 3)

- [x] **A.1** Drop "TODO: migrate to" boilerplate from in-memory
      settings storage doc comment (`lib.rs:68`). — shipped in change `zwmsmpmk` / `59b100cc`.
- [x] **A.2** Dedup `NotSupported` allocation strings (`lib.rs:1025-1133`,
      ~24 sites all "GitHub has no X") into module-level `const` slices.
      — shipped in change `zwmsmpmk` / `59b100cc`; 7 `NS_NO_*` consts replace 20 inline allocations.
- [~] **A.3** ~~`mapping.rs:421,552` (test `meta_text.unwrap()`) — cosmetic
      tighten.~~ **SKIPPED** — test module already has `#![allow(clippy::unwrap_used)]`; no change needed.

## Phase B — Medium refactors (50-300 LoC, max 5)

- [x] **B.1** Split `lib.rs` (1220 LoC, 89 fns) — `IsBackend` impl
      (line 190) + `CodeRepoBackend` (line 793) + `ModerationBackend`
      (line 849) + `SocialGraphBackend` (line 1009) +
      `DmsAndGroupsBackend` (line 1078) into sibling modules. SRP/ISP.
      — shipped in commit `6e2cd0a1`; 9 new files, lib.rs reduced from
      1195 LoC to 183 LoC (85% reduction). New modules: `impl_is_backend`,
      `impl_code_repo`, `impl_moderation`, `impl_social_graph`,
      `impl_dms_and_groups`, `impl_settings`, `impl_view_descriptor`,
      `impl_context_action`, `forum` (parse_forum_channel helper).
- [x] **B.2** Extract `decode_b64` / `decode_b64_simple` (`lib.rs:1170-1220`)
      into a small `base64` helper module. **Resolved via `clients/common-forge`
      crate** — both github and forgejo now import from the shared crate;
      local copies deleted.
- [x] **B.3** `lib.rs:280` `IsBackend::get_messages` for forum channels
      returns `NotSupported(...)` — re-route to `CodeRepoBackend`'s
      issue/PR/discussion handlers instead of refusing.
      — shipped in this change: added `gh-discussions-*` branch to
      `get_messages` (maps via new `mapping::discussion_to_message`);
      `gh-issues-*`/`gh-pulls-*`/`gh-issue-*` were already handled.
      GitHub Discussions (GraphQL-only) map as read-only Messages that link
      to the web URL; no REST endpoint for comment listing exists.
- [x] **B.4** `kind_from_string` / `split_owner_repo` (`lib.rs:1137-1168`)
      extracted to `clients/common-forge`. `parse_forum_channel` (github-specific
      `gh-` prefixes) remains in github lib.rs.
- [x] **B.5** `IsBackend::send_message` (~line 695) returns
      `NotSupported` for several channel kinds. Each branch should
      delegate to a `ChannelKind` handler trait — current shape is a
      growing match arm (Open/Closed violation).
      — shipped in this change: `send_message` now dispatches on channel
      prefix. `gh-issue-*` threads post via new `GhCli::create_issue_comment`
      (backed by `api_post` native+WASM+HTTP-test). Forum-index channels
      (`gh-issues-*`, `gh-pulls-*`, `gh-discussions-*`, `gh-code-*`) return
      honest per-kind `NotSupported` messages explaining the gap. Also added
      `GhCli::api_post` / `api_post_raw` native/WASM/HTTP transports to
      `api.rs` mirroring the existing `api_delete` pattern.

## Phase C — Architectural rewrites (>300 LoC, max 3)

- [~] **C.1** **Deferred — design-level.** GitHub API surface is an
      octocrab-equivalent hand-rolled module (`api.rs`, 817 LoC). A real
      migration to `octocrab` would:
      (a) introduce a new heavy dependency tree (reqwest + jsonwebtoken
      + secrecy) that ALL three shells (web/desktop/electron) pull in
      via `poly-github`, including the WASM build;
      (b) duplicate the existing native↔WASM transport split because
      `octocrab` is native-only — WASM still needs the `/gh` HTTP-bridge
      fallback already present in `GhCli`;
      (c) lose the `gh CLI` authentication path, which is the user-
      facing value prop of this backend (no token extraction).
      Net trade-off: ~400 LoC saved in `api.rs` at the cost of a much
      larger dep graph, a duplicate WASM transport, and a worse auth
      story. Rate limiting and pagination are already handled by the
      `gh` CLI itself. **Verdict: not worth it under the current
      "CLI as transport" design.** Revisit if the project drops the
      `gh` CLI requirement.

- [~] **C.2** Partial — `send_message` migrated via
      `plan-trait-split-readable-vs-writable.md` Phase D.8. The
      per-channel-kind routing (issue threads writable; forum indexes
      / discussions / code channels NotSupported) moved into a
      dedicated `impl_writable_messaging.rs` implementing
      `WritableMessagingBackend`; `IsBackend::send_message` is gone
      from `impl_is_backend.rs` and the parent shim consults
      `as_writable_messaging()`. Remaining all-`NotSupported`
      `impl_moderation.rs` / `impl_social_graph.rs` /
      `impl_dms_and_groups.rs` surfaces stay queued as Tier 2 on the
      same plan.

- [x] **C.3** `mapping.rs` test fixtures split into sibling
      `mapping_tests.rs` (commit on worktree-agent branch). Production
      mapping logic stays in `mapping.rs` (~480 LoC); fixture builders
      `make_issue` / `make_discussion` and all 15 unit tests live in
      `mapping_tests.rs` (210 LoC), declared via
      `#[cfg(test)] mod mapping_tests;` in `lib.rs`. Tests still reach
      `crate::types::*` (legitimate same-crate access — they're testing
      the mapping between internal API types and public Poly types).
      `cargo test -p poly-github --lib` → 15 passed.

---

## Findings index (file:line)

- Pack C P18 stub: `lib.rs:68`.
- Repeated `NotSupported` allocs: `lib.rs:904,914,918,928,932,936,981,989,997,1001,1025,1029,1033,1041,1045,1049,1053,1057,1061,1069,1088,1092,1096,1100,1104,1108,1116,1120,1124,1133` (30 sites).
- Capability gap (not by-design): `lib.rs:280` `get_messages` returning
  `NotSupported` for forum channels.
- Cross-crate duplication: b64 + owner/repo helpers identical to
  `clients/forgejo/src/lib.rs:1089-1153`.
- SRP violations: `lib.rs` 1220 LoC / 89 fns; `mapping.rs` 612 LoC;
  `api.rs` 651 LoC.

# Plan: SOLID + missing-impl audit — `clients/github/`

## Status: IN PROGRESS — Phase A shipped (A.1+A.2 in change `zwmsmpmk` / commit `59b100cc`; A.3 skipped — cosmetic). Phase B + C queued.

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

- [ ] **B.1** Split `lib.rs` (1220 LoC, 89 fns) — `IsBackend` impl
      (line 190) + `CodeRepoBackend` (line 793) + `ModerationBackend`
      (line 849) + `SocialGraphBackend` (line 1009) +
      `DmsAndGroupsBackend` (line 1078) into sibling modules. SRP/ISP.
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

- [ ] **C.1** GitHub API surface is octocrab-equivalent hand-rolled.
      Consider migrating to `octocrab` to get rate-limit + pagination
      + secondary-rate-limit handling for free. Would replace much of
      `api.rs` (651 LoC). _Large; needs design._
- [ ] **C.2** Trait fan-out — `GitHubClient` impls 5 poly_client traits
      where 3 are nearly all-`NotSupported`. Same Interface Segregation
      argument as Lemmy C.2; resolve at the poly_client trait level
      via per-capability `as_xxx() -> Option<&dyn _>`.
- [ ] **C.3** `mapping.rs` (612 LoC) intermixes test fixtures with
      production mapping. Split. (Smaller than HN, still architectural
      because tests use `pub(crate)` to reach internal types.)

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

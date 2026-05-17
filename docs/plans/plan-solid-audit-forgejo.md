# Plan: SOLID + missing-impl audit — `clients/forgejo/`

## Status: IN PROGRESS — Phase A documented, B/C documented

Audit pass over `clients/forgejo/src/{api.rs,lib.rs,mapping.rs,signup.rs,types.rs}`
(2137 LoC). Identifies SOLID violations and missing implementations.

Scope: only `clients/forgejo/`. Do NOT touch other client crates.

---

## Phase A — Ship-now wins (≤50 LoC each, max 3)

- [ ] **A.1** Drop "TODO: migrate to" boilerplate from in-memory
      settings storage doc comment (`lib.rs:62`). _≤5 LoC._
- [ ] **A.2** Dedup `NotSupported` allocation strings (`lib.rs:923-1023`,
      ~24 sites all "Forgejo has no X") into module-level `const` slices.
      _≈30 LoC removed._
- [ ] **A.3** Reduce `cfg(feature = "native")` repetition (`lib.rs:1098`,
      `1109`, `1116`, etc.) by wrapping the helpers in one
      `#[cfg(feature = "native")] mod native_helpers { ... }` block.
      _≈10 LoC._

## Phase B — Medium refactors (50-300 LoC, max 5)

- [ ] **B.1** Split `lib.rs` (1153 LoC, 88 fns) — `IsBackend` impl
      (line 111) + `CodeRepoBackend` (line 706) +
      `ModerationBackend` (line 743) + `SocialGraphBackend` (line 907) +
      `DmsAndGroupsBackend` (line 977) into sibling modules. SRP/ISP.
- [ ] **B.2** `decode_b64` / `decode_b64_simple` (`lib.rs:1110-1153`) is
      duplicated **verbatim** from `clients/github/src/lib.rs:1170-1220`.
      Within forgejo, move to a `b64.rs`. Cross-crate dedup (new
      `clients/common-forge` crate) is C.1.
- [ ] **B.3** `lib.rs:227,785-830,869-896` — multiple `NotSupported`
      returns for channel-mgmt + send-message paths. Some are honest
      capability gaps (read-only backend by design — see `lib.rs:13`),
      some should delegate to `CodeRepoBackend`. Triage individually.
- [ ] **B.4** `parse_issue_thread_owner_repo` / `repo_owner_name_from_server_id`
      / `parse_forum_channel` / `split_owner_repo` (`lib.rs:1041-1108`)
      → group into `channel_ids.rs`.
- [ ] **B.5** `mapping.rs` (364 LoC) hosts both production mapping and
      test fixtures — same split as github B.5 / lemmy B.2.

## Phase C — Architectural rewrites (>300 LoC, max 3)

- [x] **C.1** **Shared `clients/common-forge` crate.** GitHub + Forgejo
      duplicate:
      - `decode_b64` / `decode_b64_simple` (50 LoC each, identical),
      - `kind_from_string` (FileKind dir/symlink/submodule),
      - `split_owner_repo` (`~` separator),
      - `parse_forum_channel` (issues/pulls/discussions prefix),
      - `FileKind` enum (in `types.rs`).
      Extract a new `clients/common-forge` crate with these primitives.
      Estimated 300+ LoC moved + cross-crate dependency wiring.
      _DRY + DIP — both clients depend on an abstraction instead of
      copy-pasting._
- [ ] **C.2** Read-only-by-design framing (`lib.rs:13`) means
      `send_message`/`create_channel`/etc are correct as `NotSupported`,
      but the trait surface should NOT include those methods. Resolve
      via `poly_client` trait split (writable-vs-readable backends).
      Same as Lemmy C.2 / GitHub C.2.
- [ ] **C.3** Trait fan-out — `ForgejoClient` impls 5 poly_client
      traits where 3 are nearly all-`NotSupported`. Split into
      sibling modules (B.1 prerequisite) then aim for per-capability
      `as_xxx()` registration.

---

## Findings index (file:line)

- Pack C P18 stub: `lib.rs:62`.
- Repeated `NotSupported` allocs: `lib.rs:227,785,799,806,818,824,830,869,880,890,896,923,927,931,939,943,947,951,955,959,967,987,991,995,999,1003,1007,1015,1019,1023` (30 sites).
- Cross-crate duplication: b64 + owner/repo helpers identical to
  `clients/github/src/lib.rs:1137-1220`.
- Read-only-by-design (correct stubs): `lib.rs:13` documents intent.
- SRP violations: `lib.rs` 1153 LoC / 88 fns; `mapping.rs` 364 LoC.

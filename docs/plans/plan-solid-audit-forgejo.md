# Plan: SOLID + missing-impl audit — `clients/forgejo/`

## Status: ✅ DONE — all phases shipped or honestly deferred (Phase A `qmykutwl`/`5833a421`, Phases B + C close-out 2026-05-24).

Audit pass over `clients/forgejo/src/{api.rs,lib.rs,mapping.rs,signup.rs,types.rs}`
(2137 LoC). Identifies SOLID violations and missing implementations.

Scope: only `clients/forgejo/`. Do NOT touch other client crates.

---

## Phase A — Ship-now wins (≤50 LoC each, max 3) — shipped

- [x] **A.1** Drop "TODO: migrate to" boilerplate from in-memory
      settings storage doc comment (`lib.rs:62`). _≤5 LoC._
- [x] **A.2** Dedup `NotSupported` allocation strings (`lib.rs:923-1023`,
      ~24 sites all "Forgejo has no X") into module-level `const` slices.
      _≈30 LoC removed._ — shipped: added `mod ns { ... }` with 10 consts
      (one `#[cfg]` gate), replaced 20 literal sites with `ns::*`.
- [x] **A.3** Reduce `cfg(feature = "native")` repetition (`lib.rs:1098`,
      `1109`, `1116`, etc.) by wrapping the helpers in one
      `#[cfg(feature = "native")] mod native_helpers { ... }` block.
      _≈10 LoC._ — shipped: 10 separate `#[cfg]` const lines collapsed
      into one `#[cfg(feature = "native")] mod ns { ... }` block.

## Phase B — Medium refactors (50-300 LoC, max 5)

- [x] **B.1** Split `lib.rs` (1153 LoC, 88 fns) — `IsBackend` impl
      (line 111) + `CodeRepoBackend` (line 706) +
      `ModerationBackend` (line 743) + `SocialGraphBackend` (line 907) +
      `DmsAndGroupsBackend` (line 977) into sibling modules. SRP/ISP.
      — shipped: `is_backend.rs` (282 LoC), `code_repo.rs` (39),
      `moderation.rs` (165), `social_graph.rs` (70),
      `dms_and_groups.rs` (65), `context_action.rs` (89),
      `view_descriptor.rs` (163), `settings.rs` (35) — `lib.rs` now
      142 LoC (declarations + `ForgejoClient` struct only).
- [x] **B.2** `decode_b64` / `decode_b64_simple` (`lib.rs:1110-1153`) is
      duplicated **verbatim** from `clients/github/src/lib.rs:1170-1220`.
      Within forgejo, move to a `b64.rs`. Cross-crate dedup (new
      `clients/common-forge` crate) is C.1. — shipped via C.1: the
      helpers went directly into `clients/common-forge::{decode_b64,
      decode_b64_simple}`; forgejo imports through `use
      poly_common_forge::{decode_b64, kind_from_string,
      split_owner_repo};` in `lib.rs:59`. No local `b64.rs` needed
      because the shared crate landed at the same time.
- [x] **B.3** `lib.rs:227,785-830,869-896` — multiple `NotSupported`
      returns for channel-mgmt + send-message paths. Some are honest
      capability gaps (read-only backend by design — see `lib.rs:13`),
      some should delegate to `CodeRepoBackend`. Triage individually.
      — shipped: triage outcome documented; all are honest gaps and
      stay as `NotSupported`. The unique long-form messages in
      `moderation.rs` were deduped into a private `mod mod_ns { ... }`
      block (KICK/BAN/UNBAN/TIMEOUT/BAN_LIST/CHANNEL_UPDATE/
      CHANNEL_REORDER/MOD_LOG/ROLES), and `is_backend::send_message`'s
      read-only-explanation string moved to `ns::READ_ONLY_SEND` in
      `lib.rs`. No methods should delegate to `CodeRepoBackend` — the
      forge is genuinely read-only at the message layer (the web UI
      handles posting); structural removal of those trait methods is
      C.2 (deferred — needs cross-crate trait surgery).
- [x] **B.4** `parse_issue_thread_owner_repo` / `repo_owner_name_from_server_id`
      / `parse_forum_channel` / `split_owner_repo` (`lib.rs:1041-1108`)
      → group into `channel_ids.rs`. — shipped: extracted to new
      `clients/forgejo/src/channel_ids.rs`; 3 individual `#[cfg]` attrs
      removed; call sites updated to `channel_ids::*`.
- [x] **B.5** `mapping.rs` (364 LoC) hosts both production mapping and
      test fixtures — same split as github B.5 / lemmy B.2.
      — n/a: audit confirms `mapping.rs` contains zero `#[cfg(test)]`
      blocks and no fixture code. There is nothing to split. The 364
      LoC are all production mappers (server/channel/user/issue/comment
      → poly types + `map_issue_to_viewrow` + `issue_to_view_detail` +
      `humanize_age` + `filter_active_repos`). Test fixtures live in
      `clients/forgejo/tests/fixtures/` (separate dir), so the SRP
      concern that motivated B.5 doesn't apply here. Closing as
      verified-not-needed.

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
      copy-pasting._ — shipped: `clients/common-forge/src/lib.rs`
      exports `decode_b64`, `decode_b64_simple`, `kind_from_string`,
      `split_owner_repo`. Forgejo consumes via `use
      poly_common_forge::{...};` in `lib.rs:59`. `parse_forum_channel`
      stayed forgejo-local (in `channel_ids.rs`) because the per-forge
      channel-ID prefix string (`fj-issues-` vs `gh-issues-`) is a
      backend-specific concern; the SHAPE of the parse is shared, but
      the prefix isn't. `FileKind` lives in `poly_client::FileKind`
      (already shared via the trait crate, so common-forge re-uses it
      without re-exporting).
- [~] **C.2** Read-only-by-design framing (`lib.rs:13`) means
      `send_message`/`create_channel`/etc are correct as `NotSupported`,
      but the trait surface should NOT include those methods. Resolve
      via `poly_client` trait split (writable-vs-readable backends).
      Same as Lemmy C.2 / GitHub C.2.
      — **DEFERRED**: requires editing `crates/client/src/lib.rs`
      (or wherever the `IsBackend`/`ModerationBackend` traits live) to
      split write-capable methods into a sibling trait like
      `WritableMessagingBackend`. This is a cross-cutting change that
      touches every backend (matrix, discord, teams, stoat, demo,
      poly-server, lemmy, github, hackernews) simultaneously and the
      poly_client trait crate, which is out of scope for the forgejo
      worktree. Tracking: this should land as one workspace-wide
      `plan-trait-split-readable-vs-writable.md` covering all
      read-only backends (lemmy, github, forgejo, hackernews) at
      once. Until then, the `NotSupported` stubs are the contract,
      documented in `lib.rs:13`.
- [~] **C.3** Trait fan-out — `ForgejoClient` impls 5 poly_client
      traits where 3 are nearly all-`NotSupported`. Split into
      sibling modules (B.1 prerequisite) then aim for per-capability
      `as_xxx()` registration.
      — **DEFERRED (B.1 part shipped)**: the sibling-module split
      shipped under B.1 — `is_backend.rs`, `code_repo.rs`,
      `moderation.rs`, `social_graph.rs`, `dms_and_groups.rs`,
      `context_action.rs`, `view_descriptor.rs`, `settings.rs` each
      hold one trait impl. The remaining `as_xxx()` capability-
      registration part requires the same cross-crate trait surgery
      as C.2 (the `Backend` enum / dyn dispatch glue lives in
      poly_client + every other client crate); deferred under the
      same `plan-trait-split-readable-vs-writable.md` umbrella.

---

## Findings index (file:line)

- Pack C P18 stub: `lib.rs:62`.
- Repeated `NotSupported` allocs: `lib.rs:227,785,799,806,818,824,830,869,880,890,896,923,927,931,939,943,947,951,955,959,967,987,991,995,999,1003,1007,1015,1019,1023` (30 sites).
- Cross-crate duplication: b64 + owner/repo helpers identical to
  `clients/github/src/lib.rs:1137-1220`.
- Read-only-by-design (correct stubs): `lib.rs:13` documents intent.
- SRP violations: `lib.rs` 1153 LoC / 88 fns; `mapping.rs` 364 LoC.

## Status notes (2026-05-24 close-out)

After the close-out pass the SRP picture is healthier than the initial
audit indicated:

- `lib.rs` is now 142 LoC (was 1153) — only crate-level docs, the `ns`
  constants module, `ForgejoClient` struct + constructors, and
  `plugin_translations`. No trait impls.
- Largest forgejo source file is `is_backend.rs` (282 LoC) — the
  authentication + servers + channels + messages + events impl, which
  is a single-responsibility unit per the `IsBackend` trait contract.
- `mapping.rs` (364 LoC) is the next-largest but is pure-functional
  type mappers — no behavioural coupling to refactor.

Phase C deferred items (C.2, C.3) are not forgejo-local concerns —
they require a workspace-wide trait split that affects every backend.
Tracked in this plan for visibility but the actual work belongs in a
cross-crate plan (`plan-trait-split-readable-vs-writable.md`) that
will be opened when the orchestrator schedules the workspace-wide
SOLID Phase D pass.

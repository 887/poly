# Plan — `BatchedSignal<T>` Newtype (Cascade-Hang Prevention)

> Status: **pending** — ready to start after current Teams Sheep cascade debug (task #237) concludes.
> Authors: orchestrator + audit subagent (`/tmp/poly-signal-write-audit.md`) + API design subagent (`/tmp/poly-batched-signal-design.md`).
> Last updated: 2026-04-24.

---

## 1. Why this plan exists

Over the last month, Poly has been hit by the same WASM scheduler hang **six separate times**. Root cause every single time: CLAUDE.md hang class #1 — multiple `Signal::write()` guards dropped in sequence, each drop scheduling a full Dioxus reactive pass, cumulatively starving the single-threaded WASM scheduler and wedging the tab.

Known recent incidents (all the same bug class, different locations):

- commit `a761fe01` — `favorites_sidebar.rs::AccountIcon::onclick` (5 `chat_data.write()` calls batched).
- commit HEAD − 1 — `chat_view.rs:759-763` (5 `chat_data.write()` in `open_message_hit` batched).
- commit HEAD − 1 — `favorites_sidebar.rs::restore_server_channel` (7 writes accumulated into locals, 1 terminal batch).
- Teams Sheep click freeze (task #237, still open at plan-authoring time) — another render cascade, suspected in `ChatView` `use_effect` chain.

**Auditing the whole UI crate** (subagent report at `/tmp/poly-signal-write-audit.md`) found **17 HIGH-severity** (3+ sequential `.write()` calls on one signal in one function) and **104 MEDIUM-severity** (2 sequential `.write()` calls) cascade-prone sites still in tree. Each one is a future hang waiting for the right subscriber density to tip it over.

Hand-batching every site one-at-a-time has not worked — the pattern keeps re-landing because nothing in the type system says "don't". This plan makes cascading a **compile-time error** for the hot-path signals (`ChatData`, `AppState`, later others) via a newtype whose only mutation path is a closure-scoped single-guard `batch(|v| ...)`.

---

## 2. Solution summary

Introduce `BatchedSignal<T>` — a newtype around `Signal<T>` that:

1. **Keeps Dioxus subscriber-graph compatibility** via `Deref<Target=Signal<T>>`. Every `.read()`, `.peek()`, `use_effect(move || { bs(); })`, and `rsx!` formatting keeps working unchanged. ~150 read sites need zero changes.
2. **Bans multi-guard cascading** by shadowing `Signal::write` with a `#[deprecated]` inherent method that returns `!` and panics. Callers must use `.batch(|v| ...)` — one closure, one guard, one cascade.
3. **Supports async-interleaved mutations** via a `PendingUpdate<T>` builder (`#[must_use]`, debug-panics on drop without `.apply()`). This lets `load_server_data_internal`-style functions accumulate mutations across `.await` points and commit them with exactly ONE terminal write.

Full design (including trait-interop matrix, failure-mode analysis, code snippets): [`/tmp/poly-batched-signal-design.md`](file:///tmp/poly-batched-signal-design.md).

---

## 3. Phases

Each phase lands independently, behind its own commit. Phase N blocks on phase N−1.

### Phase 1 — Introduce the type, no call-site changes

**Deliverable:** `crates/core/src/state/batched_signal.rs` (~200 lines incl. tests).

Tasks:
- [ ] Create `crates/core/src/state/batched_signal.rs` with:
  - `BatchedSignal<T>` newtype (manual `Copy`, `Clone`, `PartialEq`, `Debug`, `Deref`).
  - `impl BatchedSignal<T>` with `from_signal`, `use_batched`, `batch`, `with`, `map`, `pending_update`.
  - Shadow `fn write(&self) -> !` marked `#[deprecated]` with a CLAUDE.md pointer.
  - `PendingUpdate<T>` with `set`, `apply`, `discard`; `Drop` impl that debug-panics / release-warns when dropped without `.apply()`.
  - `use_batched_context<T>()` hook.
- [ ] Add `pub use batched_signal::*;` in `crates/core/src/state/mod.rs`.
- [ ] Unit tests covering: `batch` runs closure exactly once, `PendingUpdate::apply` = one write, drop without apply panics in debug, `Copy`/`Clone`/`Deref` work.

Verification: `cargo test -p poly-core` + `cargo check -p poly-core --target wasm32-unknown-unknown` green. No call-site diff, lands no-op.

### Phase 2 — Migrate `Signal<ChatData>` (highest-frequency signal, 48 mutation sites)

**Deliverable:** all `use_context::<Signal<ChatData>>()` flipped to `use_context::<BatchedSignal<ChatData>>()` and every `chat_data.write()` migrated to `chat_data.batch(|cd| ...)` or `chat_data.pending_update()`.

Tasks:
- [ ] Flip the context provider (one line, grep `use_context_provider.*ChatData`).
- [ ] Sed-replace `Signal<ChatData>` → `BatchedSignal<ChatData>` in function signatures (hundred-ish hits, mechanical).
- [ ] Fix every compile error — each is a `chat_data.write()` that needs a `.batch()` or `.pending_update()` wrapper.

**HIGH-severity cascade sites to batch during migration** (from audit, filtered to ChatData):

- [ ] `crates/core/src/ui/favorites_sidebar.rs` `load_server_data_internal` L1111-1124 (3 cascades)
- [ ] `crates/core/src/ui/account/common/channel_list.rs` `load_channel_data` L76-94 (3 cascades)
- [ ] `crates/core/src/ui/account/common/channel_list.rs` `DMChannelItem` L1116-1130 (4 cascades, **mixed with `app_state`** — phase 2 fixes only the chat_data part; `app_state` half waits for phase 3)
- [ ] `crates/core/src/ui/account/common/channel_list.rs` `GroupChannelItem` L1206-1219 (3 cascades)
- [ ] `crates/core/src/ui/account/settings/content_social.rs` `SensitiveMediaSection` L130-147 (3 cascades)
- [ ] `crates/core/src/ui/account/settings/content_social.rs` `FriendRequestsSection` L324-338 (3 cascades)

**MEDIUM-severity ChatData sites to also fix opportunistically** (from audit top-35 extract):

- [ ] `favorites_sidebar.rs` `FavoriteServerIcon` L937-948
- [ ] `favorites_sidebar.rs` `load_server_data_internal` L1157-1171, L1179-1184
- [ ] `favorites_sidebar.rs` `restore_server_channel` L1305-1309 (already partially batched, double-check)
- [ ] `voice_banner.rs` `VoiceBannerControls` L201-215
- [ ] `account_bar.rs` `AccountBarControls` L314-328
- [ ] `account_server_bar.rs` `AccountServerIcon` L287-293
- [ ] `notifications.rs` L503-513, L528-538, L559-577
- [ ] `voice_bar.rs` L269-276, L292-299
- [ ] `voice_view.rs` L635-650, L667-674, L690-697
- [ ] `server/settings/general.rs` `LeaveServerConfirm` L123-124
- [ ] `settings/content_social.rs` `AgeRestrictedSection` L235-242, `SocialPermissionsSection` L280-287

All other `chat_data.write()` sites auto-flip when the compiler errors out — the checklist above is just the multi-cascade hotspots to ALSO batch-group.

Verification: full build green on WASM + native, `cargo test --workspace` green, manual smoke-test every backend's account click, channel click, message send, and context menu flow.

### Phase 3 — Migrate `Signal<AppState>` (55 mutation sites)

**Deliverable:** all `Signal<AppState>` → `BatchedSignal<AppState>`, same playbook as phase 2.

Tasks:
- [ ] Flip the context provider.
- [ ] Sed-replace `Signal<AppState>` → `BatchedSignal<AppState>`.
- [ ] Fix every compile error.

**HIGH-severity AppState cascade sites** (from audit):

- [ ] `crates/core/src/ui/mod.rs` `init_storage` L1093-1109 — **8-write cascade at boot** (mixed with `chat_data`; worst offender)
- [ ] `crates/core/src/ui/mod.rs` `init_storage` L1122-1140 — 6-write `nav.*` cascade
- [ ] `crates/core/src/ui/settings/general.rs` `LayoutModeSelector` L184-203 — 6-write alternating cascade (mixed with `settings_sig`)
- [ ] `crates/core/src/ui/main_layout.rs` `MainLayout` L305-314 — 4-write context-menu-clear cascade
- [ ] `crates/core/src/ui/account/common/chat_view.rs` `close_chat_side_column_state` L2391-2394 — 3 cascades
- [ ] `crates/core/src/ui/account/common/chat_view.rs` `render_mobile_chat_header_right_toggle` L2515-2518 — 3 cascades
- [ ] `crates/core/src/ui/account/common/chat_view.rs` (unknown fn) L2827-2830 — 3 cascades
- [ ] `crates/core/src/ui/account/common/chat_view.rs` `render_chat_tools_panel` L4693-4697 — 3 cascades

**MEDIUM-severity AppState sites** (audit top-35 extract):

- [ ] `account/common/attachment_context_menu.rs` L42-53
- [ ] `account/common/avatar_context_menu.rs` L51-59
- [ ] `account/common/channel_context_menu.rs` L46-54
- [ ] `account/common/channel_list.rs` `ChannelItemRow` L1393-1413
- [ ] `account/common/chat_view.rs` L2725-2727, L2882-2884, L2904-2906, L2946-2948, L3005-3022, L4633-4653, L5107-5123
- [ ] `account/common/direct_call_overlay.rs` L146-160
- [ ] `account/common/reaction_context_menu.rs` L42-50
- [ ] `account/common/user_profile_modal.rs` `open_user_profile` L68-87
- [ ] `account/server/context_menu.rs` `ServerContextMenu` L77-88
- [ ] `settings/mod.rs` `install_settings_scroll_spy` L286-292

Watch out: `app_state.write().nav.selected_channel.unsafe_presync_override(...)` patterns need the `batch` closure to hold the guard long enough for the nested method to complete — single-arg `batch` handles it:

```rust
app_state.batch(|st| {
    st.nav.selected_channel.unsafe_presync_override(Some(id), "reason");
});
```

### Phase 4 — Remaining hot-path signals

Opportunistic — only for signals with 3+ cascade hotspots:

- [ ] `Signal<ChatHistoryUiState>` (`history_state`) — 9 mutation sites; 2 HIGH-severity in `chat_view.rs::load_older_messages` (L3383-3393 and L3456-3476).
- [ ] `Signal<AppSettings>` (`settings_sig`) — 2 mutation sites in `LayoutModeSelector` (already covered mixed with `app_state` in phase 3, re-verify).
- [ ] `Signal<ClientManager>` — mostly `.read()`, skip unless a cascade shows up in practice.
- [ ] `Signal<ThemeConfig>` — settings-only, not a hang vector, skip.

### Phase 5 — Custom lint banning raw `Signal::write` in the UI crate

Two tracks, ship whichever is ready first:

**Track A (regex CI check, fast):**
- [ ] Add `tools/scripts/forbid-signal-write.sh` (grep-based scan of `crates/core/src/ui/**/*.rs`).
- [ ] Allowlist file `tools/scripts/signal-write-allowlist.txt` for intentional cases (local component-scoped signals, tests).
- [ ] Wire into CI (`.github/workflows/ci.yml` or whatever's there).

**Track B (dylint-based custom lint, proper, can follow A):**
- [ ] Add `tools/lints/poly-lints/src/forbid_signal_write.rs` matching `Signal::write` via HIR (ignores `RwLock::write`, `std::io::Write::write`, etc.).
- [ ] Package under `cargo dylint`.
- [ ] Wire into `cargo cranky`.

Exception annotation: `#[allow(poly::raw_signal_write)]` must be accompanied by a rationale comment explaining why (grep-auditable).

### Phase 6 — Documentation + cleanup

- [ ] Update `CLAUDE.md` "Common WASM-hang causes" section #1 to point at `BatchedSignal` as the prescribed prevention.
- [ ] Add short dev-doc at `docs/dev/reactive-state.md` with 3-4 canonical mutation patterns (sync single-field, sync multi-field, async-interleaved, early-return with `.discard()`).
- [ ] Delete any remaining `#[allow(poly::raw_signal_write)]` annotations that phase 5 flagged, where the migration caught up.

---

## 4. Verification

After each phase:
- `cargo check --workspace --target wasm32-unknown-unknown` — clean.
- `cargo check --workspace` (native) — clean.
- `cargo test --workspace` — green.

After phase 3 (end of ChatData + AppState migration):
- Manual smoke-test per backend: Demo, Stoat, Matrix, Discord, Teams, Lemmy, Forgejo, GitHub, poly-server. For each: click account → click server → click channel → type message → send → switch server → open context menu → change settings tab. No hangs, no blank renders, no visual regression.
- **Teams Sheep click** no longer wedges (task #237 reproducer passes).
- Synthetic 18s tight-loop test against the SW watchdog still produces the overlay + auto-reload (regression check for phase-238 work).

After phase 5:
- `cargo cranky` (or CI grep check) fails if someone reintroduces raw `Signal::write` on a migrated signal without `#[allow]`.

---

## 5. Risks / failure modes (honest list)

From the design doc §8 — things the type system **cannot** catch:

1. **Reentrant `.batch()` inside another `.batch()`** — runtime panic ("already borrowed"), not compile-time. Mitigation: documented, rare in practice.
2. **Cross-signal helpers** — if `helper(other_signal)` internally does 5 `.write()`s, hang reappears on that OTHER signal. Phase-5 lint is what catches this, not the type.
3. **`PendingUpdate` leaked into a spawned future that cancels** — debug-panic on drop catches it in dev, release-warn in prod. Not preventable without linear types.
4. **Explicit `&*bs` deref to reach raw `Signal::write`** — typechecks via Deref but the phase-5 lint catches it.
5. **Raw `Signal<T>` locals outside the context provider** — component-local signals can still have cascades. Lint should flag the pattern.

None of these void the design — each is a narrower escape hatch than the current "anyone can `.write()` anywhere" baseline.

---

## 6. Timeline estimate

| Phase | Budget | Agent tier |
|-------|--------|-----------|
| 1 — introduce type | 1 session (~2h) | sonnet-coding |
| 2 — migrate ChatData | 1-2 sessions | sonnet-coding (sed + manual review) |
| 3 — migrate AppState | 1 session | sonnet-coding |
| 4 — remaining signals | 0.5 session | sonnet-coding |
| 5a — regex CI check | 0.3 session | sonnet-coding |
| 5b — dylint custom | 2 sessions | opus-coding |
| 6 — docs + cleanup | 0.3 session | sonnet-coding |

Total: ~1 focused week. Can run phases 2 + 3 in parallel on worktrees if conflicts on `use_context` signatures are resolved carefully — sonnet agents in isolation mode each touch a non-overlapping subset of files.

---

## 7. Reference artifacts (generated during planning)

- [`/tmp/poly-signal-write-audit.md`](file:///tmp/poly-signal-write-audit.md) — 17 HIGH + 104 MEDIUM cascade sites with line-ranges, signal names, severity.
- [`/tmp/poly-batched-signal-design.md`](file:///tmp/poly-batched-signal-design.md) — 430-line API spec with code snippets, trait-interop matrix, failure-mode analysis, migration playbook.
- `CLAUDE.md` § "Common WASM-hang causes" #1 — the hang class being eliminated.
- Commit `a761fe01` — prior batch-fix for `AccountIcon::onclick`, canonical sync-batch example.
- Commit `HEAD` (at plan time) — `restore_server_channel` + `open_message_hit` batch fixes, canonical async-interleaved example (to be supplanted by `PendingUpdate` in phase 2).

---

## 8. Out of scope for this plan

- Fixing the specific **Teams Sheep click** cascade (task #237) — that's an in-flight debug; this plan prevents the CLASS of bug, not that specific incident.
- Refactoring the `ChatData` or `AppState` field shapes — orthogonal; `BatchedSignal<T>` wraps whatever type `T` happens to be today.
- Replacing Dioxus or its signal API — this is a thin wrapper, not a framework swap.
- Splitting `ChatData` into smaller signals — worth exploring separately as a signal-scope reduction pass, but complementary to (not replaced by) this plan.

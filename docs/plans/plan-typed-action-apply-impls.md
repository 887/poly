# Plan: Implement the 69 `todo!()` typed-action `apply()` stubs in `crates/core/src/ui/`

## Status: 📋 PLANNED — scoped only, not started

> Opened 2026-07-29 from the `--all-targets` clippy sweep. **Nothing here is
> implemented.** This document exists so the surface is tracked as real phases
> with sub-steps rather than sitting behind a crate-wide lint allow.

---

## Why this plan exists

`crates/core/src/lib.rs` carries a crate-wide allow:

```rust
// `todo!()` stubs are intentional phase placeholders awaiting feature work;
// clippy::todo only flags their existence, which is by design here.
clippy::todo,
```

Behind it sit **69 `todo!("phase-E: …")` calls** in `crates/core/src/ui/`. None
of them appeared in the 295-error `--all-targets` sweep, because the allow hides
them from clippy entirely. They are not lint debt — they are **unbuilt features
that panic at runtime when the user clicks the control**.

Every one is inside a `UiAction::apply()` match arm. The canonical shape:

```rust
impl UiAction for MediaSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::ToggleProvider(_kind, _enabled) => todo!("phase-E: toggle GIF provider"),
            Self::SetProviderApiKey(_kind, _key)  => todo!("phase-E: persist provider API key"),
            Self::SetActiveProvider(_kind)        => todo!("phase-E: set active GIF provider"),
        }
    }
}
```

The UI renders the control, the typed action is dispatched, and `apply()`
panics. On WASM a panic has no unwinding, so this is not a caught error — it is
the wedged-tab failure mode described in `CLAUDE.md`.

**Failure scenario:** a user opens Settings → Theme and clicks "Dark mode".
`ThemeSettingsAction` dispatches, `apply()` hits
`todo!("phase-E: persist color mode")`, and the WASM main thread aborts. The tab
must be reloaded and the preference is never persisted.

---

## Inventory (verified 2026-07-29, `grep -rn 'todo!(' crates/core/src/ui/` → 69)

| File | count | theme |
|---|---|---|
| `account/settings/content_social.rs` | 11 | content + social preferences |
| `settings/theme.rs` | 9 | preset, colour mode, overrides, custom CSS, import/export |
| `settings/voice_video.rs` | 5 | input/output volume, device selection |
| `settings/general.rs` | 5 | general preferences |
| `settings/plugins.rs` | 4 | plugin enable/disable/install |
| `settings/identity.rs` | 4 | keypair generation, delete identity, recovery phrase |
| `settings/backup.rs` | 4 | add/remove/re-auth backup server, sync now |
| `account/settings/voice_settings.rs` | 4 | per-account voice preferences |
| `settings/media.rs` | 3 | GIF provider toggle, API key, active provider |
| `account/settings/profile.rs` | 3 | presence, avatar upload, banner upload |
| `settings/plugin_settings.rs` | 2 | per-plugin settings |
| `create_forum_post.rs` | 2 | **mislabelled — see Phase A** |
| `account/settings/notifications.rs` | 2 | notification preferences |
| `account/server/settings/profile.rs` | 2 | server profile |
| `account/server/settings/overview.rs` | 2 | server overview |
| `settings/accounts.rs` | 1 | navigate to account settings |
| `favorites_sidebar.rs` | 1 | **needs Signal + async handles — see Phase F** |
| `actions.rs` | 1 | doc example, not a live stub |
| `account/server/settings.rs` | 1 | server settings root |
| `account/server/settings/notifications.rs` | 1 | server notifications |
| remainder | ~2 | |

At least three are **not** unbuilt work and must be handled differently — Phase A.

---

## Phase A — Triage: separate "not needed" from "unbuilt"

Do this first. It shrinks every later phase and prevents implementing things
that should simply be deleted.

- [ ] **A.1** `create_forum_post.rs` — both stubs say
  `"apply not needed — state is local"`. These are not unbuilt work; either the
  action type should not implement `UiAction`, or `apply()` should be
  `unreachable!()` carrying that reason. Decide which and apply it.
- [ ] **A.2** `actions.rs` — the single `todo!("phase-X: …")` sits in a doc
  comment / example, not a live match arm. Confirm and exclude it from the count.
- [ ] **A.3** Walk all 69 sites and label each: **implement** (real feature),
  **unreachable** (cannot be dispatched — prove it by finding no dispatch site),
  or **delete** (the control should not exist yet). Record the label inline.
- [ ] **A.4** Publish the corrected counts in this table before starting Phase B.
  **Do not** assume all 69 need implementing.

## Phase B — Make the failure survivable before making it correct

The crate-wide allow must not come off until the panics are gone, but the panics
should stop being panics *first* — a small change with an outsized safety win.

- [ ] **B.1** Add a `UiAction::apply` fallback that surfaces a "not implemented
  yet" toast via the existing `push_save_outcome_toast` / `action_outcome` path
  instead of panicking.
- [ ] **B.2** Convert every **implement**-labelled stub to that fallback.
  Behaviour goes from "wedge the tab" to "tell the user".
- [ ] **B.3** Convert every **unreachable**-labelled stub to
  `unreachable!("<why it cannot be dispatched>")`.
- [ ] **B.4** Remove the **delete**-labelled controls from the rendering path so
  no dispatch can occur.
- [ ] **B.5** Add a test asserting no `apply()` in `crates/core/src/ui/` contains
  `todo!` — a lint-gate rule is the natural home (Phase G).

## Phase C — Settings persistence substrate

Most remaining stubs are "persist a preference". They share one missing seam, so
build it once.

- [ ] **C.1** Identify the existing settings-persistence path
  (`client.config.<backend_id>.*` in `poly_kv`, per `docs/client-settings.md`)
  and determine whether app-level (non-backend) preferences have an equivalent.
- [ ] **C.2** If not, add one: trait + in-memory impl + KV-backed impl, per SOLID
  item 7. Do **not** let each `apply()` reach for storage directly.
- [ ] **C.3** Wire it through `ActionCx` so `apply()` receives it by injection
  (DIP — `apply()` must not name the concrete store).
- [ ] **C.4** Round-trip test: set → reload → read back, against the in-memory impl.

## Phase D — Theme, media and general preferences (~17)

- [ ] **D.1** `settings/theme.rs` ×9 — preset, colour mode, overrides toggle,
  individual override, reset, custom-CSS toggle, custom CSS, import, export.
- [ ] **D.2** `settings/media.rs` ×3 — provider toggle, API key, active provider.
  The API key is a **secret**: route it through the sealing path, not plain KV.
- [ ] **D.3** `settings/general.rs` ×5.
- [ ] **D.4** Tests per group against the Phase C in-memory impl.

## Phase E — Account, profile and social (~25)

- [ ] **E.1** `account/settings/content_social.rs` ×11.
- [ ] **E.2** `account/settings/profile.rs` ×3 — presence, avatar upload, banner
  upload. The uploads need a blob path; check whether one exists before inventing
  one, and raise it as a finding if not.
- [ ] **E.3** `account/settings/notifications.rs` ×2 and
  `account/settings/voice_settings.rs` ×4.
- [ ] **E.4** `account/server/settings/{profile,overview,notifications}.rs` plus
  `settings.rs` ×6.
- [ ] **E.5** Tests per group.

## Phase F — The ones with real architectural blockers

Called out separately because they are **not** "write the body".

- [ ] **F.1** `favorites_sidebar.rs` — the stub says
  `"FavoritesBarAction requires Signal + async handles"`. `ActionCx` does not
  carry them. Decide: widen `ActionCx`, or move this action out of the typed-action
  system. Write the decision down before coding.
- [ ] **F.2** `settings/identity.rs` ×4 — keypair generation, identity deletion,
  recovery-phrase display. **Security-sensitive and destructive.** Deletion needs
  the confirm-and-separate treatment; the recovery phrase must never be logged,
  toasted, or persisted. Do not fold into a bulk phase.
- [ ] **F.3** `settings/backup.rs` ×4 — add/remove/re-auth a backup server, sync
  now. Needs the backup-server client; confirm it exists and is wired.
- [ ] **F.4** `settings/plugins.rs` ×4 + `plugin_settings.rs` ×2 — enable, disable,
  install. Routes through `/host/plugins/*`, which the capability-gating work
  changes; land after that.
- [ ] **F.5** `settings/accounts.rs` ×1 — navigation only; likely trivial.

## Phase G — Remove the crate-wide allow and gate the regression

- [ ] **G.1** Delete `clippy::todo` from the `crates/core/src/lib.rs` allow list.
- [ ] **G.2** `cargo clippy -p poly-core --all-targets -- -D warnings` clean.
- [ ] **G.3** Add a lint-gate rule forbidding `todo!()` inside an `impl UiAction`
  block, so a new stub fails the build instead of hiding behind an allow.
- [ ] **G.4** Confirm B.5 and G.3 are not redundant; delete whichever is.

## Phase H — Verify (QA gate — iterate until a clean round)

- [ ] **H.1** `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] **H.2** `cargo test --workspace` — no regression in counts.
- [ ] **H.3** `cargo check -p poly-lint-gate` clean, baseline `(path, rule, detail)`
  multiset unchanged (`command diff` — `diff` is shadowed by a zsh function here).
- [ ] **H.4** Manual UI walk: every settings page, every control, no panic, and the
  preference survives a reload. This **cannot** be done without a live stack — if
  no shell is available, record it as gated with a re-open trigger.
- [ ] **H.5** Only tick this phase off a round that surfaced nothing new.

---

## Notes for whoever picks this up

- **Do not start with Phase D.** Phase A changes the numbers, and Phase B converts
  a tab-wedging panic into a visible message across the whole surface at once —
  the highest value-per-line change here.
- `crates/core/src/ui/` is covered by the **line-keyed** `baseline.json`. Prefer
  line-count-neutral edits, run `cargo check -p poly-lint-gate` after each step,
  and never regenerate the baseline to make a new violation go away.
- The 69 is a `grep` count including at least 3 non-stubs. A.4 exists precisely so
  nobody quotes 69 as a work estimate.

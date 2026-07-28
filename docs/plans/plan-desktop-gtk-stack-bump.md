# Plan: Move the desktop shell off the EOL gtk-rs 0.18 generation

## Status: ⛔ BLOCKED (upstream-gated) — Phase A shipped in this PR; A.2's stop condition fired, so Phases B/C/D are NOT executable

> Opened 2026-07-28 from the multi-agent review fan-out. Explicitly out of scope
> for the dependency-upgrade agent ("invasive"), so it is tracked here rather
> than absorbed.
>
> **2026-07-28 — Phase A executed. The migration is not possible today and the
> reason is upstream, not in this repo.** `tao` 0.35.3, `wry` 0.55.1 and
> `webkit2gtk` 2.0.2 — the newest published versions of all three targets — still
> require `gtk ^0.18`, and `gtk` (the GTK3 Rust bindings) is permanently EOL at
> **0.18.2** with no 0.19+ ever published. Executing every phase of this plan as
> written would therefore have moved **zero** of the twelve advisories. See
> "Phase A — findings" below for the measured evidence and the re-open triggers.
>
> Sibling plan: `plan-dependency-upgrade.md` (the routine bump cadence) and
> `plan-host-substrate-capability-gating.md` Phase E (the CI advisory gate that
> will surface this until it is fixed).

---

## Why this plan exists

Three **direct** dependencies hold the whole gtk-rs 0.18 generation in the lock:

| Pin | Location | Current | Original target | Verdict (Phase A) |
|---|---|---|---|---|
| `cairo-rs = { version = "0.18", features = ["png"] }` | `Cargo.toml` (`cairo-rs` pin) | 0.18.5 | 0.22 | ❌ breaks the build, fixes nothing |
| `wry = "0.53"` | `Cargo.toml` (`wry` pin) | 0.53.5 | 0.55 | ❌ hard resolver conflict |
| `tao = "0.34"` | `Cargo.toml` (`tao` pin) | 0.34.6 | 0.35 | ❌ duplicates tao in the lock |
| `webkit2gtk = { version = "2.0", features = [] }` | `Cargo.toml` (`webkit2gtk` pin) | 2.0.1 | 2.0.2 | ❌ conflicts with wry's `=2.0.1` |

The OSV sweep flags **eleven** crates in that closure, all unmaintained:
atk 0.18.2 (RUSTSEC-2024-0413), atk-sys (RUSTSEC-2024-0416), gdk
(RUSTSEC-2024-0412), gdk-sys (RUSTSEC-2024-0418), gdkwayland-sys
(RUSTSEC-2024-0411), gdkx11 (RUSTSEC-2024-0417), gdkx11-sys
(RUSTSEC-2024-0414), gtk (RUSTSEC-2024-0415), gtk-sys (RUSTSEC-2024-0420),
gtk3-macros (RUSTSEC-2024-0419) — plus **glib 0.18.5 carrying
RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g, which is a soundness bug, not merely
an end-of-life notice**. glib-macros and gtk3-macros additionally drag in
proc-macro-error 1.0.4 (RUSTSEC-2024-0370, unmaintained).

`cargo tree -i` confirms these are load-bearing, not vestigial:

```
gtk 0.18.2      <- tao 0.34.6 <- { poly-desktop-web, poly-host-sandbox, dioxus-desktop }
cairo-rs 0.18.5 <- { poly-desktop-devtools, poly-desktop-web, gdk, gtk, webkit2gtk }
```

`crates/host-sandbox` (the `wry-sandbox` feature) is a **security boundary** per
`CLAUDE.md` SOLID item 7 — it is the browser-sandbox seam. Running that boundary
on an EOL toolkit generation with a known soundness advisory is the specific
reason this is medium and not low.

**Failure scenario:** a memory-safety bug lands in gdk/gtk 0.18 or as a
follow-on to the glib 0.18.5 unsoundness. The 0.18 generation is EOL upstream,
so no patch release is ever issued and the fix *is* this migration — executed
under time pressure instead of on a planned schedule. **Phase A's finding does
not remove that risk; it establishes that this repo cannot currently retire it,
and names the upstream events that would unblock it.**

---

## Phase A — Establish the constraint graph before touching any pin — ✅ DONE, shipped in this PR

The single thing that can make this migration impossible is `dioxus-desktop`'s
own `tao` pin. Determine it first; the rest of the plan branches on the answer.

- [x] **A.1** Record the exact tao/wry requirements of the pinned
  `dioxus-desktop` 0.7.9. — done, see "A.1 findings" below.
- [x] **A.2** If dioxus-desktop 0.7.9 requires tao 0.34, check whether a later
  Dioxus 0.7.x moved to 0.35. **It does, and none has** — the stop condition is
  hit. See "A.2 findings".
- [x] **A.3** Enumerate every in-repo call site that touches the gtk-rs API
  surface directly. — done, see "A.3 findings".
- [x] **A.4** Note the two known Linux footguns from `CLAUDE.md`. — done, see
  "A.4 findings"; both call sites recorded with file:line so any future attempt
  can diff against them.

### A.1 findings — the pinned requirements

From `~/.cargo/registry/src/*/dioxus-desktop-0.7.9/Cargo.toml`:

| Requirer | Requirement | Kind |
|---|---|---|
| `dioxus-desktop` 0.7.9 | `tao = "0.34.0"` (features `rwh_05`) | caret → `>=0.34.0, <0.35.0` |
| `dioxus-desktop` 0.7.9 | `wry = "0.53.5"` (default-features off) | caret → `>=0.53.5, <0.54.0` |
| `wry` 0.53.5 (linux cfg) | `webkit2gtk = "=2.0.1"`, `webkit2gtk-sys = "=2.0.1"`, `javascriptcore-rs = "=1.1.2"`, `gtk = "0.18"` | **exact** pins |
| `tao` 0.34.6 (linux cfg) | `gtk = "0.18"` | caret |
| `webkit2gtk` 2.0.1 | `cairo-rs ^0.18`, `gdk ^0.18`, `glib ^0.18`, `gtk ^0.18` | caret |

`dioxus-desktop` is in the graph because `apps/desktop` (and `apps/android`)
depend on `dioxus` 0.7.9 with the `desktop` feature; `apps/desktop-web`,
`apps/desktop-devtools` and `crates/host-sandbox` depend on `wry`/`tao`/
`webkit2gtk`/`cairo-rs` directly. They all resolve in one workspace lock.

### A.2 findings — **STOP CONDITION HIT**

Every published `dioxus-desktop` release requires `tao ^0.34.0` from **0.7.0
through 0.7.9** (the current pin). The first release that moves is
**`0.8.0-alpha.0`** → `tao ^0.35.2`, `wry ^0.55.1`. There is no 0.7.x on tao
0.35. Per this plan's own instruction, that is a stop-and-re-plan.

Measured, not inferred. Setting `wry = "0.55"` / `tao = "0.35"` /
`webkit2gtk = "2.0.2"` and running `cargo metadata` gives a hard resolver
failure (rc=101):

```
error: failed to select a version for `webkit2gtk`.
    ... required by package `wry v0.53.5`
    ... which satisfies dependency `wry = "^0.53.5"` of package `dioxus-desktop v0.7.9`
    ... which satisfies dependency `dioxus-desktop = "^0.7.9"` of package `dioxus v0.7.9`
    ... which satisfies dependency `dioxus = "^0.7.9"` of package `poly-android v0.1.0`
versions that meet the requirements `=2.0.1` are: 2.0.1

all possible versions conflict with previously selected packages

  previously selected package `webkit2gtk v2.0.2`
    ... which satisfies dependency `webkit2gtk = "^2.0.2"` of package `poly-desktop-devtools v0.1.0`

failed to select a version for `webkit2gtk` which could resolve this conflict
```

Bumping `tao` **alone** (leaving wry at 0.53) does resolve — by adding a second
tao major, exactly the outcome this plan forbids:

```
$ cargo tree -d -e normal --target x86_64-unknown-linux-gnu | grep -E '^(tao|wry|gtk|glib|cairo-rs|webkit2gtk) '
tao v0.34.6 (*)
tao v0.35.3 (*)
```

### A.2b findings — the bump would fix nothing even if it resolved

This is the finding that turns "blocked on Dioxus 0.8" into "blocked on the
whole Linux webview ecosystem". Queried from the crates.io sparse index:

| Crate | Newest published | gtk requirement |
|---|---|---|
| `tao` | 0.35.3 | `gtk ^0.18` |
| `wry` | 0.55.1 | `gtk ^0.18`, `gdkx11 ^0.18`, `webkit2gtk =2.0.2` |
| `webkit2gtk` | 2.0.2 | `gtk ^0.18`, `gdk ^0.18`, `glib ^0.18`, `cairo-rs ^0.18` |
| `gtk` (GTK3 bindings) | **0.18.2 — final** | n/a |

The `gtk` crate's published version list ends at `0.18.2`. gtk-rs stopped
shipping GTK3 bindings; the successor is the separate `gtk4` crate, which
`tao`/`wry` do not use. So **there is no target generation to migrate to**, and
all twelve advisories survive any combination of these bumps.

### A.3 findings — in-repo gtk-rs API call sites

| File:line | Surface | Owner crate |
|---|---|---|
| `apps/desktop-web/src/main.rs:484-505` | `wry::WebViewBuilderExtUnix::build_gtk` + `tao ... default_vbox()` | `poly-desktop-web` |
| `apps/desktop-web/src/main.rs:591-599` | `webkit2gtk::WebViewExt::snapshot`, `webkit2gtk::{SnapshotRegion, SnapshotOptions}`, `webkit2gtk::gio`, `webkit2gtk::glib::Error`, `cairo::Surface` | `poly-desktop-web` |
| `apps/desktop-web/src/main.rs:602` | `cairo::Surface::write_to_png` (needs cairo-rs feature `png`) | `poly-desktop-web` |
| `apps/desktop-devtools/src/main.rs:307-317` | same `webkit2gtk` snapshot + `cairo::Surface::write_to_png` shape | `poly-desktop-devtools` |
| `crates/host-sandbox/src/wry_sandbox.rs:138-157` | `tao::platform::unix::WindowExtUnix::default_vbox` + `wry::WebViewBuilderExtUnix::build_gtk` | `poly-host-sandbox` (feature `wry-sandbox`) |

No in-repo code names `gtk::`, `gdk::` or `glib::` directly — the only direct
gtk-generation types we touch are `cairo::Surface` and the `webkit2gtk` façade.

**Why the `cairo-rs` pin exists at all (this was not obvious and is the reason
Phase B is dead):** `webkit2gtk` 2.0.1 depends on `cairo-rs` 0.18 *without* the
`png` feature, and does not re-export `cairo`. Our workspace `cairo-rs` pin is
a feature-unification lever — it exists to switch `png` **on for the same
cairo-rs 0.18 instance** that `WebViewExt::snapshot` hands back, so that
`surface.write_to_png(..)` (cairo-rs `src/surface_png.rs`, gated by
`#[cfg(feature = "png")]`) is in scope. Move the pin to 0.22 and you get a
second, unrelated cairo crate while the one that matters loses `png`.

### A.4 findings — footguns this migration must not regress

1. **`build_gtk()` must receive `window.default_vbox()`, never
   `window.gtk_window()`** — `gtk_window()` yields a **0x0 viewport**. Two live
   call sites: `apps/desktop-web/src/main.rs:490` (`let Some(vbox) =
   window.default_vbox() else { … exit(1) }`, then `builder.build_gtk(vbox)` at
   :498) and `crates/host-sandbox/src/wry_sandbox.rs:152-155`
   (`window.default_vbox().ok_or_else(…)?` then `.build_gtk(vbox)`). Both are
   correct today and must be diffed byte-for-byte after any future bump.
2. **Electron frameless windows use `frame: false` alone** — never combined
   with `titleBarStyle: 'hidden'` / `titleBarOverlay: false`. Unrelated to the
   gtk stack, recorded because `CLAUDE.md` groups the two Linux window
   footguns together.
3. The desktop shell is `apps/desktop-web` (Wry) serving `apps/desktop` on
   port 3002; the screenshot path in both `apps/desktop-web` and
   `apps/desktop-devtools` runs on the GTK main thread inside the tao event
   loop, so any cairo/webkit2gtk change is a threading change too.

---

## Phase B — Bump `cairo-rs` independently — ❌ NOT EXECUTABLE (evaluated and rejected in this PR)

- [x] **B.1** Evaluated. `cairo-rs = "0.22"` *does* resolve, but additively: it
  adds `cairo-rs 0.22.0`, `cairo-sys-rs 0.22.0`, `glib 0.22.8`, `glib-sys
  0.22.8`, `gobject-sys 0.22.6`, `glib-macros 0.22.6` **on top of** the 0.18
  set, which stays because `webkit2gtk` 2.0.1 requires it. `cargo tree -d`
  after the change: `cairo-rs v0.18.5` + `cairo-rs v0.22.0`,
  `glib v0.18.5` + `glib v0.22.8`. Every advisory survives, and the tree grows.
- [x] **B.2** Evaluated. The call sites cannot be "fixed": there is no path to
  name the cairo-rs 0.18 `Surface` type once the direct pin moves (webkit2gtk
  2.0.1 re-exports `gio` and `glib` but **not** `cairo`), and the 0.18 instance
  loses its `png` feature so `write_to_png` disappears. Measured —
  `cargo check -p poly-desktop-web --all-targets` with `cairo-rs = "0.22"`:

  ```
  error[E0631]: type mismatch in closure arguments
      --> apps/desktop-web/src/main.rs:595:16
       |
   595 |             wk.snapshot(
       |                ^^^^^^^^ expected due to this
  ...
   599 |                 move |result: Result<cairo::Surface, webkit2gtk::glib::Error>| match result {
       |                 -------------------------------------------------------------- found signature defined here
       |
       = note: expected closure signature `fn(Result<cairo::surface::Surface, _>) -> _`
                  found closure signature `fn(Result<cairo::Surface, _>) -> _`
  note: required by a bound in `webkit2gtk::WebViewExt::snapshot`
      --> …/webkit2gtk-2.0.1/src/auto/web_view.rs:1125:18
  ```
- [x] **B.3** N/A — B.2 is a hard compile error, not a migration cost.
- [x] **B.4** N/A — never reached a runnable binary.

**Conclusion:** `cairo-rs` is not an independent slice. It is welded to
whatever generation `webkit2gtk` uses, and `webkit2gtk` 2.0.2 (newest) is still
on 0.18. Re-open when webkit2gtk ships a release on cairo-rs 0.20+.

## Phase C — Bump `tao` + `wry` + `webkit2gtk` together — ❌ NOT EXECUTABLE (blocked by A.2)

- [x] **C.1** Evaluated — hard resolver failure, verbatim output in "A.2
  findings". `tao` alone resolves but duplicates the major, which C.1 itself
  defines as the late-detection signal for A.2's stop condition.
- [ ] **C.2** Not attempted — no buildable lock to migrate against. The
  `default_vbox()` invariant to preserve is recorded verbatim in A.4.
- [ ] **C.3** Not attempted. `crates/host-sandbox` remains on the 0.18
  generation; its `HostSandbox` trait + stub impl are unchanged by this PR, so
  SOLID item 7 is neither improved nor regressed.
- [ ] **C.4** Not attempted.
- [ ] **C.5** Pre-answered by A.2b: `glib` 0.18 would survive regardless,
  because `gtk ^0.18` is required by tao 0.35.3, wry 0.55.1 *and*
  webkit2gtk 2.0.2. This is the "partial move fixes none of the advisories"
  case, and it is the *only* available outcome, not a risk.

## Phase D — Retire the advisory ignores — ❌ NOT EXECUTABLE (nothing to retire, and nothing fixed)

- [ ] **D.1** Two reasons this cannot run. (a) The `[advisories.ignore]` block
  it refers to **does not exist in the repo yet** — there is no `deny.toml` or
  `audit.toml` anywhere in the tree, and the only files mentioning `RUSTSEC` are
  this plan and `plan-host-substrate-capability-gating.md`; that plan's Phase
  E.1 has not shipped. (b) Even if it had, the advisories are all still live,
  so deleting the ignores would just red the gate.
- [ ] **D.2** Not applicable. Documented reason + re-open triggers below.

## Phase E — Verify — partially executed (see "Verification" below)

- [x] **E.1** `cargo check -p poly-lint-gate` and
  `cargo clippy -p poly-desktop-web --all-targets -- -D warnings` clean on the
  restored baseline. Workspace-wide runs were deliberately not run in this
  isolated workspace (cold `target/`, shared disk budget).
- [ ] **E.2** **Not possible in this environment** — no GUI/display. Recorded as
  a manual check below.
- [ ] **E.3** **Not possible in this environment** — same reason.
- [ ] **E.4** N/A — nothing shipped that could regress the shell.

---

## Documented reason + re-open triggers (replaces the deleted "target" columns)

**Reason the twelve advisories stay ignored/accepted:** every one of them is
pulled in by `gtk 0.18.2`, and `gtk 0.18.2` is the final published release of
the GTK3 Rust bindings. `tao` 0.35.3, `wry` 0.55.1 and `webkit2gtk` 2.0.2 — the
newest releases of all three — still require `gtk ^0.18`. No combination of
in-repo pin changes moves the generation, so the risk is accepted rather than
deferred.

**Re-open this plan when ANY of these becomes true** (check with the same
sparse-index query used in A.2b — `curl https://index.crates.io/3/t/tao`, `.../3/w/wry`,
`.../we/bk/webkit2gtk`):

- **R1** — `wry` publishes a release whose Linux deps are `gtk4`-based (or
  `gtk ^0.19+` if gtk-rs ever resumes GTK3). This is the real unblock.
- **R2** — `webkit2gtk` publishes a release on `cairo-rs ^0.20+` / `glib ^0.20+`.
  That alone would retire the `glib` soundness advisory and make Phase B a real
  slice again.
- **R3** — `dioxus-desktop` 0.8.0 goes stable (it already targets tao 0.35.2 /
  wry 0.55.1 in `0.8.0-alpha.0`). Necessary but **not** sufficient — R3 without
  R1/R2 only changes which EOL versions we pin.
- **R4** — a *new* advisory lands that is exploitable rather than
  unmaintained-EOL (RUSTSEC-2024-0429 is currently the only soundness one). At
  that point the correct response is not this plan but replacing the Wry desktop
  shell path, e.g. routing the desktop shell through the Electron shell
  (`apps/desktop-electron-web`), which has no gtk-rs closure at all.

---

## What this PR actually changed

1. `Cargo.toml` — added the constraint-graph comment block above the
   `cairo-rs` / `wry` / `tao` pins so the next person to attempt this bump hits
   the reasoning before the resolver. **No pin values changed.**
2. `apps/desktop-web/Cargo.toml` — `tao = "0.34"` → `tao = { workspace = true }`.
   This was a real latent defect found while doing A.1: `poly-desktop-web`
   carried its own literal copy of the `tao` requirement instead of the
   workspace pin, so Phase C.1 (which edits only the workspace pin) would have
   left `apps/desktop-web` on the old requirement and silently produced the
   two-tao-majors lock that A.2 forbids. Resolves to the same `tao 0.34.6`;
   `Cargo.lock` is byte-identical.
3. This plan file — Phase A executed and recorded; B/C/D marked not executable
   with measured evidence and re-open triggers.

## Verification

Run from `.claude/worktrees/pr-desktop-gtk`, each redirected to a log:

- `cargo metadata --format-version 1` — rc=0 on the restored baseline.
- `cargo check -p poly-desktop-web --all-targets` — `Finished`, 0 `^error`,
  0 `could not compile`.
- `cargo clippy -p poly-desktop-web --all-targets -- -D warnings` — `Finished`,
  0 `^error`, 0 `could not compile`, 0 warnings.
- `cargo check -p poly-lint-gate` — `Finished`, 0 `^error`, 0 baseline entries
  added.

**No runtime or visual verification was possible** — this workspace has no
display and cannot launch the Wry shell. A human must still confirm, before
trusting the desktop shell: launch `apps/desktop` on port 3002 via the
poly-desktop MCP, confirm the window opens at a **non-zero** size (a 0x0
viewport is the `default_vbox()` regression from A.4), `connect_cdp` succeeds,
and `take_screenshot` returns a real PNG rather than the sub-100-byte guard
error from `mcp/devtools-protocol/src/mcp.rs`. Nothing in this PR touches that
code path, so a failure there is pre-existing, not introduced here.

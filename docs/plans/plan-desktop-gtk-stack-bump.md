# Plan: Move the desktop shell off the EOL gtk-rs 0.18 generation

## Status: 📋 PLANNED

> Opened 2026-07-28 from the multi-agent review fan-out. Explicitly out of scope
> for the dependency-upgrade agent ("invasive"), so it is tracked here rather
> than absorbed.
>
> Sibling plan: `plan-dependency-upgrade.md` (the routine bump cadence) and
> `plan-host-substrate-capability-gating.md` Phase E (the CI advisory gate that
> will surface this until it is fixed).

---

## Why this plan exists

Three **direct** dependencies hold the whole gtk-rs 0.18 generation in the lock:

| Pin | Location | Current | Target |
|---|---|---|---|
| `cairo-rs = { version = "0.18", features = ["png"] }` | `Cargo.toml:282` | 0.18.5 | 0.22 |
| `wry = "0.53"` | `Cargo.toml:283` | 0.53 | 0.55 |
| `tao = "0.34"` | `Cargo.toml:284` | 0.34.6 | 0.35 |
| `webkit2gtk = { version = "2.0", features = [] }` | `Cargo.toml:233` | 2.0 | 2.0.2 |

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
under time pressure instead of on a planned schedule.

---

## Phase A — Establish the constraint graph before touching any pin

The single thing that can make this migration impossible is `dioxus-desktop`'s
own `tao` pin. Determine it first; the rest of the plan branches on the answer.

- [ ] **A.1** Record the exact tao/wry requirements of the pinned
  `dioxus-desktop` 0.7.9 (`cargo tree -i tao`, `cargo tree -i wry`, and the
  crate's own `Cargo.toml` from the registry source). Write the versions into
  this file.
- [ ] **A.2** If dioxus-desktop 0.7.9 requires tao 0.34, check whether a later
  Dioxus 0.7.x moved to 0.35. If none has, **stop and re-plan**: forcing two tao
  majors into the lock is worse than the advisories. Record the finding here and
  raise it rather than proceeding.
- [ ] **A.3** Enumerate every in-repo call site that touches the gtk-rs API
  surface directly (not through wry/tao): grep `apps/desktop-web/src/main.rs`,
  `apps/desktop-devtools/`, `crates/host-sandbox/src/wry_sandbox.rs` for
  `gtk::`, `gdk::`, `cairo::`, `webkit2gtk::`. List each in this file with
  file:line — these are the migration's actual work items.
- [ ] **A.4** Note the two known Linux footguns from `CLAUDE.md` that this
  migration must not regress: `build_gtk()` must receive
  `window.default_vbox()` (not `gtk_window()`, which yields a 0x0 viewport),
  and Electron frameless windows must use `frame: false` alone.

## Phase B — Bump `cairo-rs` independently (lowest-risk slice)

cairo-rs is used directly by `poly-desktop-devtools` for PNG encoding, which is
a narrower surface than the windowing stack.

- [ ] **B.1** `Cargo.toml:282` `cairo-rs = "0.18"` → `"0.22"`; regenerate the
  lock.
- [ ] **B.2** Fix the `apps/desktop-devtools` call sites surfaced by A.3.
  cairo-rs 0.18 → 0.22 crosses four majors; expect `ImageSurface` /
  `Format` / error-type churn.
- [ ] **B.3** `cargo check -p poly-desktop-devtools -p poly-desktop-web` green.
- [ ] **B.4** Screenshot smoke: take a screenshot through the desktop MCP and
  confirm the PNG is not 0x0 and not sub-100-bytes (the existing guard in
  `mcp/devtools-protocol/src/mcp.rs` will otherwise mask a regression as a text
  error).

## Phase C — Bump `tao` + `wry` + `webkit2gtk` together

These three move as one unit; splitting them just produces an unbuildable
intermediate.

- [ ] **C.1** `Cargo.toml:283-284` `wry = "0.55"`, `tao = "0.35"`;
  `Cargo.toml:233` `webkit2gtk = "2.0.2"`. Regenerate the lock and confirm
  `cargo tree -d | grep -E '^(tao|wry|gtk|glib) '` shows **no duplicate
  majors** — a duplicate here means A.2's stop condition was hit late.
- [ ] **C.2** Migrate `apps/desktop-web/src/main.rs` — the Wry shell. Preserve
  the `default_vbox()` invariant from A.4 verbatim.
- [ ] **C.3** Migrate `crates/host-sandbox/src/wry_sandbox.rs`. Because this is
  the sandbox seam, re-state SOLID item 7 for it after the migration: the
  `HostSandbox` trait + stub impl must still let tests run without a real
  webview.
- [ ] **C.4** `cargo check -p poly-desktop-web -p poly-host-sandbox --all-targets`
  green.
- [ ] **C.5** Confirm the gtk-rs generation actually moved:
  `cargo tree -i glib` shows 0.20+ and `cargo tree -i proc-macro-error` returns
  nothing. If glib 0.18 survives, find the remaining 0.18-pinning parent and
  record it here — a partial move fixes none of the advisories.

## Phase D — Retire the advisory ignores

- [ ] **D.1** Delete the eleven gtk-rs RUSTSEC ids + RUSTSEC-2024-0370 from the
  `[advisories.ignore]` block added in
  `plan-host-substrate-capability-gating.md` Phase E.1.
- [ ] **D.2** `cargo deny check advisories` green with no ignores for this
  cluster. Any advisory that survives gets a one-line documented reason and a
  re-open trigger in this file — never a silent re-add to the ignore list.

## Phase E — Verify (QA gate — iterate until a clean round)

- [ ] **E.1** `cargo clippy --workspace -- -D warnings` clean;
  `cargo test --workspace` green; `cargo check -p poly-lint-gate` rc=0 with zero
  baseline entries added.
- [ ] **E.2** Launch the Wry desktop shell (`apps/desktop`, port 3002) via the
  poly-desktop MCP: window opens at a non-zero size, WASM bundle loads,
  `connect_cdp` succeeds, a screenshot returns a real image. A 0x0 viewport here
  is the A.4 `default_vbox()` regression.
- [ ] **E.3** Exercise `crates/host-sandbox` under the `wry-sandbox` feature —
  the sandbox must still isolate; run its own test suite plus one live
  navigation.
- [ ] **E.4** Re-run E.1–E.3 after the final fix; tick DONE only off a round that
  surfaced nothing new.

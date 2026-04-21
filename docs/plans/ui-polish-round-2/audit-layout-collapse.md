# Account-Bar / Voice-Bar Layout Collapse

> Generated: 2026-04-21

## 0. Summary

- **Symptom:** On test-animal pages (Discord/Koala+Kangaroo, Stoat/Stoat+Raccoon,
  Lemmy/Beaver+Hedgehog, etc.) the `account-bar` and `voice-bar` are pushed up to
  just below the last channel item instead of pinning to the viewport bottom-left.
  The demo page (which has many channels) does not exhibit the bug visually.
- **Root cause:** `.channel-list-wrapper` uses `height: 100%` to fill its parent,
  but its parent (`.poly-split-shell.account-view-main`) carries `overflow: visible`
  (to allow the `-72px` bleed of `.voice-account-footer`). With `overflow: visible`,
  the flex container sizes to its *content* height rather than to the height allocated
  by the outer flex algorithm. When the channel list is short (few channels, as on
  test-animal pages), `height: 100%` resolves to that short content height — so the
  wrapper only reaches as far down as the last channel item, and `voice-account-footer`
  sits flush below it instead of at the viewport bottom.
- **Fix scope:** 1 CSS rule change in
  `crates/core/assets/styling/account-shell.css` (`.channel-list-wrapper`).

---

## 1. Reproduction

**Routes that exhibit it:**
- Any Discord, Stoat, Teams, or poly-native server route with a small channel list.
  Specifically confirmed by the bug report on:
  - `/discord/<instance>/<account>/server/<id>/<channel>` (Koala+Kangaroo)
  - `/stoat/<instance>/<account>/server/<id>/<channel>` (Stoat+Raccoon)
  - `/lemmy/<instance>/<account>/server/<id>/<channel>` (Beaver+Hedgehog)

**Routes that do not exhibit it (visually):**
- `/demo/demo/demo-cat/…` — demo has many channels so the intrinsic content height
  approximately fills the viewport, masking the same underlying bug.

**Trigger condition:** a short channel list (intrinsic height < viewport height).

---

## 2. Root Cause Analysis

### DOM structure (all backends, unified layout)

```
div.poly-split-shell.account-view-main           ← overflow: visible (A)
  div.poly-split-sidebar.poly-left-drawer-panel.channel-list-wrapper  ← (B)
    aside.channel-list                           ← flex: 1; min-height: 0 (C)
      div.server-banner-sidebar
      div.channel-entries                        ← flex: 1; overflow-y: auto (D)
    div.voice-account-footer                     ← flex-shrink: 0 (E)
      div.voice-bar / div.voice-preview-panel
      div.account-bar
```

### CSS chain

**`crates/core/assets/styling/account-shell.css:205-220`**

```css
/* (A) */
.account-view-main {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: visible;   /* <-- intentional: lets voice-account-footer bleed -72px left */
}
.poly-split-shell.account-view-main {
    overflow: visible;   /* <-- reinforces: same intent */
}
```

**`crates/core/assets/styling/layout.css:319-325`**

```css
.poly-split-shell {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;    /* <-- default, overridden for account-view-main */
}
```

**`crates/core/assets/styling/layout.css:327-333`**

```css
/* (B's flex rules) */
.poly-split-sidebar {
    display: flex;
    flex-direction: column;
    flex: 0 0 auto;   /* does not stretch — sizes to content or explicit dimension */
    min-width: 0;
    min-height: 0;
}
```

**`crates/core/assets/styling/account-shell.css:227-239`**

```css
/* (B's size rules) */
.channel-list-wrapper {
    display: flex;
    flex-direction: column;
    width: 240px;
    …
    height: 100%;   /* <-- THIS IS THE PROBLEM */
    …
}
```

**`crates/core/assets/styling/account-shell.css:246-253`**

```css
/* (C) */
.channel-list-wrapper .channel-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    …
}
```

### Why `height: 100%` fails when `overflow: visible` is set

In CSS, `height: 100%` on a child resolves to 100% of the *definite* height of its
containing block. When a flex container's `overflow` is `visible`, the browser's
flex algorithm allows the container's size on the cross axis (height for a row flex)
to be determined by the intrinsic sizes of its children — i.e. it becomes content-
sized. Once the parent has a content-sized height, the child's `height: 100%` simply
inherits that same short height rather than the viewport-allocated height from the
outer flex algorithm.

With a long channel list (demo), the content height fills the viewport incidentally,
hiding the bug. With a short list (test-animal, ≤ 10 channels), the content height
may be only 300–400 px, so `.channel-list-wrapper` collapses to that height and
`voice-account-footer` sits immediately below the last channel item.

### Why the fix is `align-self: stretch`, not a height value

`align-self: stretch` on a flex child instructs the flex algorithm itself to size the
child to the cross-axis extent of the flex container — this is computed *before*
`overflow: visible` is considered, using the container's height as established by the
outer flex context (`main-layout-body → poly-split-shell`). It does not depend on the
`overflow` value of the parent, so the `-72px` bleed mechanism continues to work
unaffected.

Additionally, removing `height: 100%` eliminates the percentage-resolution ambiguity
entirely. `flex: 0 0 auto` on `.poly-split-sidebar` is left unchanged — the sidebar
must not grow horizontally.

---

## 3. Proposed Fix (checkbox each step)

- [ ] **Edit `crates/core/assets/styling/account-shell.css`**
  In the `.channel-list-wrapper` block (approx. line 227), replace

  ```css
  height: 100%;
  ```

  with

  ```css
  align-self: stretch;
  ```

  Full context of the changed block:

  ```css
  /* BEFORE */
  .channel-list-wrapper {
      display: flex;
      flex-direction: column;
      width: 240px;
      min-width: 200px;
      max-width: 280px;
      background: var(--bg-secondary);
      border-right: 1px solid var(--border-color);
      height: 100%;          /* ← remove */
      position: relative;
      z-index: 3;
      overflow: visible;
  }

  /* AFTER */
  .channel-list-wrapper {
      display: flex;
      flex-direction: column;
      width: 240px;
      min-width: 200px;
      max-width: 280px;
      background: var(--bg-secondary);
      border-right: 1px solid var(--border-color);
      align-self: stretch;   /* ← replaces height: 100% */
      position: relative;
      z-index: 3;
      overflow: visible;
  }
  ```

- [ ] **Smoke-test on demo page** — account bar should still pin to bottom.
- [ ] **Smoke-test on Discord/Koala+Kangaroo** — account bar should pin to
  viewport bottom even with ≤ 5 channels visible.
- [ ] **Smoke-test on mobile** — open mobile drawer on a test-animal account;
  confirm the footer is not clipped.
- [ ] **Smoke-test mirrored layout** (`poly-menu-mirrored`) — the
  `.poly-app.poly-menu-mirrored .channel-list-wrapper` rule only changes border
  directions and does not re-set `height:`, so no change needed there.

---

## 4. Alternative Hypotheses Considered

### H2: Missing `flex: 1` on `.client-sidebar` wrapper (loading/error state)

When `ClientSidebar` (`crates/core/src/ui/client_ui/sidebar/mod.rs`) renders the
loading or error fallback it wraps `ChannelListLayout` in `<aside class="client-sidebar ...">`.
There is no CSS rule giving `.client-sidebar` `flex: 1` or `height: 100%`, so during
the brief loading window the inner `.channel-list` is not a direct flex child of
`.channel-list-wrapper` and the `flex: 1` rule on `.channel-list-wrapper .channel-list`
becomes the relevant rule — which uses a *descendant* selector and therefore still
matches. This is a *secondary* cosmetic flicker during the async resolve, not the
primary bug.

**Recommendation:** after the primary fix is applied, optionally add

```css
.channel-list-wrapper > .client-sidebar {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
}
```

to `account-shell.css` to prevent the flicker on slow plugin resolves.

### H3: Per-backend sidebar layout (CommunitiesLayout / FeedLayout / RepoTreeLayout)

Lemmy uses `CommunitiesLayout`, HN uses `FeedLayout`, GitHub uses `RepoTreeLayout`.
All three render an `<aside class="client-sidebar <layout>-layout">` that has no
`flex: 1` rule — meaning their sidebar fills content height, not available height.
These layouts do not render `ChannelList` at all, so the `.channel-list-wrapper .channel-list`
`flex: 1` rule never fires. The primary fix (`align-self: stretch` on
`.channel-list-wrapper`) resolves the footer positioning for these backends too,
since the wrapper itself will now fill the viewport height. Separately, H2's
secondary fix ensures the inner `<aside>` stretches within the wrapper.

### H4: New `utility-rail` tab system (recent commit `74bd47a`)

The utility rail (`ChatUtilityRail`, `chat-utility-rail` class) is rendered in the
*right wing* of `ChatView` — inside `RightWingShell`, which is entirely separate from
the left `channel-list-wrapper` column. The commit does not touch `SplitMenuShell`,
`VoiceAccountFooter`, or any left-column layout. Ruled out.

### H5: `overflow: visible` on `.account-view-main` removed as the fix

Removing `overflow: visible` on `.account-view-main` would restore correct `height: 100%`
resolution, but it would also clip the `.voice-account-footer`'s `-72px` left bleed
(the panel that covers the Bar 2 / favorites-bar column). This would break the
visual continuity of the account bar across all backends. Not recommended.

---

## 5. File References

| File | Relevant lines |
|------|---------------|
| `crates/core/assets/styling/account-shell.css` | `.channel-list-wrapper` block (~line 227) — `height: 100%` → `align-self: stretch` |
| `crates/core/assets/styling/account-shell.css` | `.account-view-main` block (~line 205) — `overflow: visible`, preserved as-is |
| `crates/core/assets/styling/layout.css` | `.poly-split-sidebar` (~line 327) — `flex: 0 0 auto`, preserved |
| `crates/core/assets/styling/account-shell.css` | `.channel-list-wrapper .channel-list` (~line 246) — `flex: 1; min-height: 0`, preserved |
| `crates/core/src/ui/split_shell.rs` | `SplitMenuShell` — applies `channel-list-wrapper` class to the sidebar div |
| `crates/core/src/ui/routes.rs` | `DmsLayout`, `ServerLayout` (~line 1084, 1115) — pass `VoiceAccountFooter` as sidebar child |
| `crates/core/assets/styling/voice-settings.css` | `.voice-account-footer` (~line 438) — `width: calc(100% + 72px); margin-left: -72px` (the bleed, preserved) |

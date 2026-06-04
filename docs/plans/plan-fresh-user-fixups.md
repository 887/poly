# Fresh-user walkthrough — fixups & things to look into

> Captured 2026-05-25 during a `reset_app` + fresh-user walkthrough of
> `apps/web` (chromium @ :3000). The walkthrough barely got past the
> first paint before turning up issues — the reset itself doesn't land
> at the setup wizard, so every subsequent screen is contaminated by
> demo-seed state. Phase A unblocks the rest.

## Status: 🚧 IN PROGRESS — all real bugs + objective polish shipped (2026-06-04); remainder is judgment-calls / mobile / aspirational

**Shipped & live-verified 2026-06-04** (on `main`): A, A2, A3, E, J, I, K,
M.1/M.3, N.1/N.3/N.4, O.1–O.5, P.1–P.5, Q.1–Q.3, R.1/R.3, B, V.1, W.1/W.2/W.4,
X, plus D.1/G.1 (verified already-labeled) and L (verified already-interactive).
All real/functional bugs are closed. ~11 commits; every lint-gate baseline
desync handled with NEW=0.

### Remaining — triaged (needs your call, not blocked-objective)

**Needs a product / design decision** (recommendation noted; UX-shape change):
- **D.2 / D.3 / S.1** — chat header has 8 controls; demote some to overflow
  and/or add cluster dividers. *Which controls demote?*
- **F.1–F.3** — the "NEW" pill semantic (unread divider vs since-last-visit).
- **C.1–C.3** — true empty-state design after a nuke (depends on autoseed flow).
- **H.1–H.3** — account-bar density taste pass against a 25+ account list.
- **R.2** — collapse servers-by-default in global search.
- **M.2** — clickable stat cards; needs target mapping for DMs/Groups/Mentions.
- **N.2** — permanent "Add friend" CTA — deferred until the add-friend feature
  itself lands (currently a coming-soon affordance).
- **Q.4** — only one persona seeded; verify intended vs seed gap.

**Quick-objective backlog** (ready on your word — no decision needed):
- **T.1** consolidate agent-panel disabled copy · **R.4** channel-type icon
  legend/labels in search · **G.2** identify + label the refresh-arrow.

**Mobile — needs a device/viewport pass:** T.2, U.1–U.6.

**Aspirational / larger:** M.4 (stat sparklines), J.3 (hover server card),
K.3 (card metrics as chips — plugin grid), A.5 (fresh-user smoke test).

---

## Phase A — `reset_app` MCP doesn't reach the setup wizard

`mcp__poly-web__reset_app` returns `"Cleared all web storage and
reloaded page. App should restart at setup wizard."` — but the actual
page that loads is `/demo/demo/demo-cat/dms/dm-user-bob` with the
full demo seed (~25 account icons in the far-left bar, populated DM
list, full chat history). The reset is clearing the browser side
(localStorage) without clearing the host-bridge SQLite (`~/.local/share/poly/storage.sqlite3`),
so the next render rehydrates from disk and skips the wizard entirely.

The in-app `☢️ NUKE App State` button (settings → General) DOES
actually wipe state (Welcome wizard appears) — but see Phase A2
below for its own problems.

- [x] **A.1** Contract chosen: option (b) — wipe `poly_kv` rows
  via the existing `/host/kv/clear` host-bridge route. No new route
  needed; `Storage::nuke_all_data` already maps to `clear_all` which
  the route handler exposes. Documented inline in the MCP `reset_app`
  fn comment. Shipped with A.3.
- [x] **A.2** Implementation per A.1: MCP POSTs to
  `http://127.0.0.1:3000/host/kv/clear` (no new host-bridge code
  needed). Then writes the `dev.autoseed_disabled` marker (A3.2),
  clears browser-side storage as belt-and-suspenders, and reloads.
  Shipped with A.3.
- [x] **A.3** ✅ MCP `reset_app` now calls `/host/kv/clear` (same path
  the in-app Nuke uses via `Storage::nuke_all_data` → `clear_all`).
  Both surfaces share behavior. Browser-side localStorage / IndexedDB
  clears kept as a belt-and-suspenders. Page reloads at the Welcome
  wizard.
- [x] **A.4** ✅ Tool description in `mcp/devtools-protocol/src/mcp.rs`
  now reads "Wipe the host-bridge SQLite poly_kv table (the canonical
  app store), clear browser localStorage/sessionStorage/IndexedDB,
  then reload the page. Restarts the app at the Welcome wizard.
  Equivalent to the in-app ☢️ NUKE App State button."
- [ ] **A.5** Add a smoke test (haiku-tier subagent + `TEST_HARNESS.md`
  pattern): `reset_app` → page reload → assert the setup-wizard
  marker text is visible, NOT a populated DM list.

---

## Phase A2 — `☢️ NUKE App State` is a one-click destructive action with no confirm

The nuke button in settings → General fires on a single click. No
"are you sure" modal, no "type DELETE to confirm" gate, no undo.
One stray click destroys every account, every cached message, every
KV setting. **This violates the destructive-actions rule** I have in
memory (`feedback_destructive_actions.md`: "Remove/delete buttons
must require confirm and live away from primary actions").

Code site: `crates/core/src/ui/settings/general.rs:317` calls
`client_manager.nuke_all_data()` directly inside the button's onclick.

- [x] **A2.1** ✅ shipped in change `svsqwpsl` — Add a confirmation modal. Minimum bar: a dialog that
  says "This will delete N accounts, M conversations, and all local
  settings. Type DELETE to confirm." with a primary danger button
  that stays disabled until the input matches. (Reset button also
  gets a soft Yes/Cancel confirm — no typing required.)
- [x] **A2.2** ✅ Nuke button now lives inside a collapsed `▸ Danger
  Zone` disclosure in `ResetSection`. Soft Reset stays in the everyday
  area. Toggle button + body styled in `theme-utils.css` with a
  danger-red left border on the expanded body. Confirm modal (A2.1)
  still gates the actual destructive action — the disclosure is a
  second visual barrier.
- [x] **A2.3** ✅ Soft Reset now shows an inline 10-second countdown
  row instead of firing immediately on confirm. Click "Undo" to abort
  during the window. Implemented inline in `ResetButton` (no toast
  system extension needed). Nuke unchanged — still immediate after
  confirm, no undo (irreversible by definition). Tick loop uses
  `setTimeout` via `dioxus::document::eval` on WASM, `tokio::sleep`
  on native.

---

## Phase A3 — Nuke + dev-plugins seed = can never reach true empty state in dev

After `nuke_all_data()` fires, the Welcome wizard renders correctly
(good!), but clicking "Get Started" navigates the user back to the
previous URL (`/settings/general`) and the `dev-plugins` feature
flag re-seeds all 25+ demo accounts on the next plugin-init pass.
So a dev user using the in-app nuke immediately gets the same
populated bar back — defeating the point.

Two issues compounded:

1. "Get Started" should not navigate to whatever URL was active
   before the nuke. It should land on the home/add-account flow.
2. `dev-plugins` re-seeding after a nuke should respect a
   "user-nuked" marker — if the user explicitly wiped, don't re-seed
   without explicit consent.

- [x] **A3.1** ✅ ResetKind::Nuke now navigates to `/` via
  `window.location.href = '/'` instead of `reload()` — the no-account
  branch routes to Welcome. Soft User reset keeps `reload()` so the
  user stays in URL context after the lighter wipe.
- [x] **A3.2** ✅ Added `keys::DEV_AUTOSEED_DISABLED` KV marker. On
  Nuke (in-app AND MCP `reset_app`) we write `dev.autoseed_disabled =
  true` AFTER the KV clear. `auto_signin_test_accounts` awaits
  STORAGE init then reads the marker — if set, logs and returns
  without seeding. Persists across reloads.
- [x] **A3.3** ✅ Added `LoadDemoButton` (cfg(debug_assertions) only)
  inside the soft-reset block in `ResetSection`. Click clears
  `DEV_AUTOSEED_DISABLED` and reloads; next boot re-runs the
  test-account seed loop. Non-debug builds get an empty no-op stub
  via `#[cfg(not(debug_assertions))]` so the call site stays clean.

---

## Phase B — Date format inconsistency in chat view

Same screen shows two date conventions side-by-side:

- **Date separator** (`.date-separator-text`): `May 23, 2026` /
  `May 25, 2026` — English long form, US-style.
- **Per-message timestamp** (`.message-timestamp`): `23/05/2026, 11:44`
  — DD/MM/YYYY 24h local (the format we just fixed in Phase 3 of
  last session, commit `kkuzvplr` / `9746424d`).

Either both should be DD/MM/YYYY-coded or both should be long-form;
mixing them looks like a half-finished i18n pass.

- [x] **B.1** ✅ Chose `%a %d/%m/%Y` ("Wed 03/06/2026") to match the per-message DD/MM/YYYY stamps with a day-name for scanability. Live-confirmed. Pick one canonical format. Recommendation: match the
  per-message convention (`%d/%m/%Y`) on the separator too —
  `25/05/2026` reads consistent with the row-level stamps. If the
  separator needs the day-name for scanability, use `Mon 25/05/2026`
  (chrono `%a %d/%m/%Y`).
- [x] **B.2** ✅ Patched the separator in `message_row.rs:51` — now uses `.with_timezone(&chrono::Local).format("%a %d/%m/%Y")` like `format_timestamp`. Patch `format_date_separator` (probably in
  `crates/core/src/ui/account/common/chat_view/message_row.rs` near
  the existing `format_timestamp` we already migrated). Match the
  same `with_timezone(&chrono::Local)` pattern so it tracks the
  user's locale.
- [x] **B.3** ✅ Chat separator + per-message stamp now consistent (the side-by-side mismatch on the screen the user flagged). Media-viewer / forum keep their own contextual formats by design. Audit other date sites: history scroll markers, the
  unread divider's `data-date`, message-tooltip on hover. Bring them
  all into the same format.

---

## Phase C — Fresh-user state still ships ~25 demo accounts

The far-left account bar on a "fresh" load shows roughly 25 stacked
account avatars (cat, dog, fox, bunny, panda, …). That's the entire
demo seed, not what a new user would see on first launch. Even if
this is intentional for the demo backend, it confuses the
fresh-user UX test: a brand new poly user should see an empty bar
plus a prominent "Add an account" affordance.

Likely entangled with Phase A — the demo accounts persist because
SQLite isn't being cleared on reset.

- [ ] **C.1** After Phase A lands, confirm: what does the true empty
  state of the account bar look like? Take a screenshot.
- [ ] **C.2** If the empty state is "nothing but a + button at the
  bottom", verify the + button is obvious to a first-timer (size,
  label, hover hint). If it isn't, file a follow-up.
- [ ] **C.3** Separate concern: should the `dev-plugins` build (the
  one used by `apps/web`) auto-seed the demo accounts on every fresh
  SQLite, or only when an explicit "load demo" affordance is clicked?
  Auto-seed is convenient for me but actively breaks UX testing.
  Recommend a `POLY_AUTOSEED_DEMO=0` env knob.

---

## Phase D — Top-bar icon parade (no labels, hard to parse)

Top-right of the chat view stacks 8+ small icons in a row with no
visible labels: phone, microphone-with-slash, gear, target/crosshair,
paperclip(?), monitor, pirate-flag(?), person. On first sight there
is no way to know what most of them do without hovering each one.

- [x] **D.1** ✅ Inventoried every top-bar/header icon (subagent map): all already carry `title`/`aria-label`; the only unlabeled icons were the server-rail ones, fixed in J. Snapshot the actual DOM (we have `take_snapshot` for
  this) and document each icon's `title` / `aria-label`. Anything
  missing one is the immediate fix.
- [ ] **D.2** Triage: which of these belong in the *header* vs which
  belong in a sub-menu / collapsed-by-default overflow? A header
  with 8 controls is over budget for a fresh user. Likely candidates
  to demote: pirate-flag (whatever it is), monitor (screen-share?),
  target/crosshair.
- [ ] **D.3** Group related controls visually (voice cluster vs
  chat-meta cluster vs notifications cluster) with subtle dividers
  so the eye can chunk the row.

---

## Phase E — Unread badge stays on the open DM ✅ DONE (change `yryqkmll`)

The DM list shows Bob with a red `(1)` unread badge even while the
Bob conversation is *currently open* on the right pane. Opening a
conversation should clear its own unread counter, not just the
new-message-divider.

> **2026-06-03 re-check (code):** claimed ALREADY FIXED — **wrong**. Live MCP
> repro on 2026-06-04 confirmed the badge persists on open. The clear-on-open
> wiring was fine; the bug was in `mark_channel_as_read` itself.
> **Real root cause:** the demo backend reuses dm ids across accounts (Dog's
> `dm_channels()` in `flavour.rs:577` takes the first 3 of `demo_dm_channels()`
> and only swaps `account_id`, keeping `id = dm-user-alice/bob/charlie`). So
> `chat_lists.dm_channels` holds two entries with the same id, and
> `mark_channel_as_read` (a) `.find()`-ed the FIRST match — early-returning if
> that sibling's unread was 0 — and (b) `break`-ed after zeroing one entry, so
> the active account's displayed entry was never cleared.

- [x] **E.1** Repro reliably — live MCP: Iris/Alice/Bob each showed `(1)`;
  opening kept the badge. Shipped in change `yryqkmll`.
- [x] **E.2** Found the hook: `chat_view/mod.rs::mark_channel_as_read`
  (called via `mark_channel_as_read_with_backend` from the list-click). Shipped in `yryqkmll`.
- [x] **E.3** Fixed in the existing batch (no new effect): take MAX unread
  across all id matches (so a 0-unread sibling can't trigger the early-return),
  and drop the `break` so EVERY entry sharing the id is zeroed. Live-verified:
  opening Iris cleared her badge. Shipped in `yryqkmll`.
- [x] **E.4** Demo backend read-write already honored via
  `data::apply_local_read_state_dms` (process-local read set); the fix is
  UI-layer so it's backend-agnostic. Shipped in `yryqkmll`.

---

## Phase F — "NEW" pill placement on date separator

There is a `NEW` pill rendered to the *right* of the `May 25, 2026`
date separator on its own line. It's ambiguous — does it mean "every
message below this is new", "the date itself is new", or "the next
message is new"? Standard convention is a horizontal divider with
"NEW" centered or left-aligned, not a free-floating pill at the
right edge.

- [ ] **F.1** Decide the desired semantic: is this the unread
  divider, the date separator, or a third thing? Currently it
  visually overlaps the date separator and that's confusing.
- [ ] **F.2** If it's the unread divider, merge with the existing
  `.message-unread-divider` styling (line + label) and drop the
  free-floating pill.
- [ ] **F.3** If it's a "new since you last visited" marker
  *distinct* from the unread divider, it needs a tooltip or label
  explaining the difference.

---

## Phase G — Bottom-left status bar icons (no labels)

The bottom-left cluster shows `Cat (demo) / Online` then four tiny
icons: power-plug(?), microphone, gear, a small refresh-arrow. None
labeled. Same problem as Phase D but in a different surface.

- [x] **G.1** ✅ Inventoried the bottom-left voice/account bar icons: all already have `title` (mute/deafen/settings/disconnect/etc.). Snapshot the DOM, list each icon's `title` /
  `aria-label`. Anything missing gets one.
- [x] **G.2** ✅ The ↻ button (forum/feed toolbar) re-fetches the current view's rows; relabeled its title/aria-label from generic "Refresh" to "Reload posts" so its purpose is clear. The refresh-arrow specifically is suspicious — what
  does it refresh? If it's "reconnect this account", the icon should
  read as a reconnect (not a generic reload). If it's "switch
  account", it shouldn't be an arrow at all.

---

## Phase H — Far-left account bar density / hit-targets

The ~25 demo accounts (see Phase C) are stacked vertically in a
narrow column with what looks like ~36px hit-targets and minimal
gap between them. Even once Phase A trims this to a real fresh-user
state, the chosen sizing matters for the worst case (a real poly
user with many accounts).

- [ ] **H.1** Measure: what's the current avatar size / gap / total
  column width? Document baseline.
- [ ] **H.2** Try 40px avatars with a 4–6px gap and a hover-grow
  affordance — should make the column scannable without doubling
  its width.
- [ ] **H.3** Test with a deliberately-stuffed account list (25+
  accounts) — does the column scroll cleanly, or does it overflow
  the viewport?

---

## Phase I — Welcome wizard layout & content

Reached the real Welcome wizard via the in-app nuke. Observations:

- Whole wizard is vertically centered on a tall viewport — the
  "Welcome to Poly" h1 sits at roughly 30% from the top with empty
  space above and below. On a 1080p screen it reads as half-empty.
- Three feature cards (🌐 / 🤖 / 🔑) are good shape and copy, but
  the icons could be more on-brand. The 🔑 for "Bring your own AI"
  reads as login/key, not as privacy — 🔒 or a "shield" SVG would
  carry the "your keys stay private" claim better.
- "Get Started" CTA is the only button — small (~120px wide), no
  visual emphasis beyond the background colour. For the only CTA on
  a marketing-style first screen it should be bigger.
- No logo / app mark at the top. Just text. Even a small Poly
  wordmark above the h1 would brand the moment.
- No "I already have an account, log me in" affordance — but this
  is probably fine for a true first-launch since the next step is
  picking a backend to connect anyway.

- [x] **I.1** ✅ `.setup-wizard` now anchors content ~15vh from the top (flex-start column) instead of dead-center. Tighten the vertical layout — push the content up so
  the h1 sits at roughly 15–20% from the top, not 30%. Leave the
  CTA with healthy breathing room but stop the page from looking
  half-empty.
- [x] **I.2** ✅ 🔑 → 🔒 (privacy-coded) in `setup_wizard.rs`. Replace the 🔑 emoji with a privacy-coded glyph
  (🔒 or an inline SVG shield) — and verify the cards' icons all
  share a visual weight (the 🤖 emoji is heavier than the other
  two; consider unifying with a flat-icon set).
- [x] **I.3** ✅ `.setup-start-btn` now min-width 200px, larger/bolder font, subtle lift-on-hover. Make "Get Started" prominent: ~200px min-width, larger
  font, maybe a subtle motion/glow hover.
- [x] **I.4** ✅ Added a `poly` wordmark above the welcome title (`setup_wizard.rs` + `.setup-wordmark`).

---

## Phase J — Server icons in the left rail have no labels

Inspected the `.server-icon` elements in the second-from-left column
(account-server bar). Every entry has `title=""` and no
`aria-label`. Hovering tells you nothing. The icons themselves are
small avatar circles (~48px) without text — first-time users would
have no way to learn which server is which without clicking.

- [x] **J.1** ✅ (change `kytwwqpn`+) `title` added to every server-icon + account-icon render site in `favorites_sidebar.rs`. Live-confirmed: all rail icons expose names (servers + accounts).
  render site. Probably in
  `crates/core/src/ui/account/common/account_server_bar/server_list.rs`
  (saw the `on_context_menu` handler there earlier; the render is
  nearby).
- [x] **J.2** ✅ `aria-label` mirrors the title on every icon. Live-confirmed.
- [ ] **J.3** Consider a hover-tooltip card that shows server name,
  unread count, and "last active" — more useful than the bare name.

---

## Phase K — Overview ("Your Servers") layout

The Home → Overview → General screen shows 4 server cards in a
responsive grid but the 4th card (`Cat ↔ Dog Arena`) sits alone on
its own row, left-aligned, looking lonely. The Search field above
the grid is full-width and very tall — disproportionate for a
search input.

- [x] **K.1** ✅ `.client-view-cards` min column 240→175px so server cards pack 4-across (no lonely 4th-row orphan). Live-confirmed: 4 cards, 1 row. Make the card grid balance the last row: either fill
  the row by stretching cards, or center the orphan card if it's a
  partial row.
- [x] **K.2** ✅ `.overview-page-search-input-fullwidth` capped at max-width 500px. Live-confirmed (was full-width). Constrain the "Search…" input width to ~500px and
  reduce vertical padding — it currently dominates the right pane.
- [ ] **K.3** Each card shows `N members · M unread · @K mentions`
  as inline text — break into a visual chip row so the metrics read
  as scannable badges, not one comma-separated string.

---

## Phase L — "Things you missed" cards are non-interactive ✅ DONE (already fixed; live-confirmed 2026-06-04)

The Things-you-missed panel lists unread DMs + notifications as
cards but none of the cards have action buttons.

> **2026-06-04 live MCP re-check:** RESOLVED since the walkthrough. The cards
> are now `<button class="overview-card-clickable">` with `crate::nav!` onclick
> handlers (`overview_subpages.rs:74-92` DMs, `:114+` notifications). Clicked
> the Alice card live → navigated to `/dms/dm-user-alice`. The design landed as
> **summary-that-links** (L.1 decided): each card opens its source (DM →
> DmChat; notification → FriendsRoute / ServerHome / ServerChat by kind).

- [x] **L.1** Decided: **summary that links into the real screens** (not a
  parallel inbox) — each card navigates to its source. Live-confirmed.
- [x] **L.2** Cards are `<button>`s (cursor/hover via `overview-card-clickable`)
  that nav to the source conversation. Live-confirmed (Alice → DmChat).
- [x] **L.3** N/A — went with the summary design (L.1), so no duplicated
  Accept/Deny buttons and no cross-surface state-sync to maintain.

---

## Phase M — Stats page is barebones

Stats shows 5 cards (Servers, Direct Messages, Groups, Unread,
Mentions) with raw counts. Subtitle "Your activity at a glance"
promises *activity*; the cards show *inventory*.

- [x] **M.1** ✅ Fixed the over-promising subtitle — was "Your activity at a glance" (cards show inventory, not activity); now "A snapshot of your servers, messages, and unread." Either rename the page to "Inventory" / "At a glance"
  or add real activity metrics: messages sent per day, time-in-app,
  most-active channels, etc.
- [ ] **M.2** Make the cards clickable: UNREAD → Things-you-missed,
  SERVERS → Overview General grid, etc.
- [x] **M.3** ✅ `.overview-stats-grid` min 150→130px so all 5 stat cards sit on one row. Live-confirmed (5 cols, 1 row). Fix the wrap: 4 cards on row 1, 1 orphan on row 2.
  Either fit 5 across, or wrap to 3+2.
- [ ] **M.4** Aspirational: add small sparklines / 7-day trends on
  each card. Optional nice-to-have.

---

## Phase N — People panel

Clicked 👥 (People). Sub-nav: Friends / Ignored / Blocked Users.
Right pane is a grid of friend cards (avatar + name + handle +
Message button).

- [x] **N.1** ✅ Fixed (change `kytwwqpn`). **Root cause:** not a tooltip —
  `friends_panel.rs` rendered the section title `friends_management_title`
  ("People") in BOTH the sidebar header (`special-page-sidebar-title`, line 164)
  AND the content-pane header (`special-page-title`, line 189). **Fix:** the
  content header now shows the ACTIVE TAB's title (`Friends` / `Ignored` /
  `Blocked`) via a reactive `active_tab_title` — removes the duplication and
  makes the header informative. Live-confirmed: sidebar "People", content
  "Friends", and switching to Ignored updates the content header to "Ignored".
- [ ] **N.2** No "Add friend" button anywhere. Friends panels in
  Discord / Slack / etc. always have it as a primary CTA. Add one
  near the search.
- [x] **N.3** ✅ `.friend-account` ("demo") now `opacity: 0.6` so the redundant per-friend handle recedes. Every friend card shows the same handle ("demo") since
  they're all on the demo backend. Looks redundant in this view. If
  it must stay, gray it heavily so the eye skips it.
- [x] **N.4** ✅ `friends_panel.rs` returned an empty class for Offline (no dot at all); now renders a grey `presence-dot offline` (matching the member-row helper + existing `.presence-dot.offline` CSS). Invisible stays hidden; Unknown (no presence info) stays dotless. Status dot consistency: some cards show a green dot,
  some don't. If "no dot" means offline, that's a usability fail —
  there should be a visible "offline" indicator (grey dot) so the
  reader knows the state was checked.

---

## Phase O — Notifications panel

Notifications (🔔). Sub-nav: All notifications (7) / Mentions (2) /
Friend requests (2) / Server invites (2) / Voice invites (1) / Other (0).
Right pane lists notification cards with action buttons.

- [x] **O.1** ✅ The sidebar subtitle now renders only when the list is empty (`if total_count == 0`). Live-confirmed: with 7 notifications no contradictory subtitle shows. The middle-column subtitle reads "No new notifications"
  while the list shows 7. Contradicts. Either the subtitle is stale
  (probably) or means "no unread" (then label it "All caught up" or
  hide it when the list is non-empty).
- [x] **O.2** ✅ Unified on "Decline" (set `notifications-deny = Decline`). Live-confirmed: action buttons read Accept / Decline everywhere. Action button wording inconsistent: Friend Request =
  Accept / **Deny**, Server Invite = Accept / **Decline**. Pick one
  word. "Decline" feels more polite for both.
- [x] **O.3** ✅ Notification type pills are now colour-coded by kind (mention=blue, friend=green, server=purple, voice=orange, reauth=red) via a `kind-<slug>` class + small pill background. Live-confirmed. Type pills (Mention / Friend Request / Server Invite /
  Voice Invite) are all neutral grey. Color-code by type so the eye
  can chunk them.
- [x] **O.4** ✅ Notifications now sorted newest-first by timestamp (was type-grouped, mixing a 1h voice invite below 3h/6h invites). Live-confirmed: 5m→20m→45m→1h→2h→3h→6h. Ordering: items are grouped by type then time, not
  pure-time. Voice Invite (1 hour ago) sits below Server Invites (3
  hours, 6 hours). Either commit to time-sort or to type-grouping;
  the current half-way is confusing.
- [x] **O.5** ✅ Not a redundancy bug: per-card "Mark as Read" appears ONLY on Mention/Other cards; Friend-Request/Invite cards have Accept/Deny/Join instead. The header "Mark all read" is a distinct bulk action. "Mark as Read" appears both on each card AND as a
  free-floating button at the bottom-left of the middle column. The
  free-floating one's scope is unclear (all? this filter? selected?).
  Label it explicitly, e.g. "Mark all as read".

---

## Phase P — Agent / Integrations panel

Agent → Integrations. MCP server config + integration feature list.

- [x] **P.1** ✅ Root cause: the i18n keys `settings-mcp-transport-label` + `-desc` were MISSING, so `t()` fell back to the title-cased key ("Settings Mcp Transport"). Added both keys to `locales/en/main.ftl` ("MCP transport"). Label "Settings Mcp Transport" uses inconsistent case
  ("Mcp" vs "MCP" used elsewhere on the same page). Pick one (MCP
  is the canonical acronym) and apply everywhere.
- [x] **P.2** ✅ Resolved by P.1 — the secondary line was the missing `settings-mcp-transport-desc` key showing as a fallback; it now has real copy. The secondary line under "Settings Mcp Transport"
  appears to be a stale i18n key or duplicate label. Inspect and
  either give it real copy or remove.
- [x] **P.3** ✅ `.settings-text-input { width:100% }` (loaded later) was overriding `.settings-input-short`; added a higher-specificity rule so the port field is 120px. Live-confirmed (827px → 120px). The Port input (containing "3010") spans the full
  pane width. A 4-digit port number doesn't need that much real
  estate. Constrain to ~120px.
- [x] **P.4** ✅ The feature-rows sub-section heading reused the page's `agent-section-integrations` key; pointed it at a new `agent-section-features` ("Features") key. No more duplicate "Integrations". The page is titled "Integrations" AND there's a sub-
  section headed "Integrations" with the feature rows (Suggested
  responses / Conversation summaries / etc.). Rename one. The lower
  list is really "Features" or "Capabilities".
- [x] **P.5** ✅ The rows are informational (no per-feature toggle); the section is now clearly headed "Features" (P.4) so it reads as a capability list, not a settings group with hidden toggles. Feature rows (Suggested responses, etc.) have no
  on/off toggle visible — looks read-only. If they're settable, add
  the toggle on the right. If they're informational, label the
  section "What MCP unlocks" or similar.

---

## Phase Q — Agent / Personas tab

Agent → Personas. Loaded once `poly-chat-mcp` was running on :3010.

- [x] **Q.1** ✅ shipped in change `svsqwpsl` — added CSS for `.persona-row-info` and friends so the name and exposure subtitle stack vertically instead of running together. Also fixed Q.2 in the same change (the badge text was the i18n Title-Cased fallback — missing `persona-exposure-*` keys are now in `locales/en/main.ftl`, so the rendered label reads `No sources selected` instead of `Persona Exposure No Sources`).
- [x] **Q.2** ✅ shipped in change `svsqwpsl` — see Q.1 above.
- [x] **Q.3** ✅ shipped in change `svsqwpsl` — Navigating to Personas hides the sub-nav middle column
  entirely (no longer shows Integrations / Agent Profile / Personas
  rows). On the other Agent sub-pages the column is present.
  **Root cause:** `crates/core/src/ui/agent.rs:200-207` — the
  Personas item in `AgentNavigation` calls
  `nav_for_personas.push(Route::PersonasRoute)`, routing the user
  to `/agent/personas` → `PersonasRoute` →
  `PersonaManagementRouteComponent` (a standalone full-page
  component, see `crates/core/src/ui/agent/persona/route.rs:42`).
  The other nav items (`Integrations`, `Profile`) live in
  `NAV_SECTIONS` and render inline within `AgentPage`'s
  `SplitMenuShell` (which provides the sidebar). Personas escapes
  the shell entirely.
  **Two viable fixes:**
  - (a) Promote `Personas` to a first-class `AgentSection` variant
    (alongside `Integrations`, `Profile`), add it to `NAV_SECTIONS`
    and `AGENT_NODES`, render the persona list inline in
    `AgentAllSections`. Then `/agent/personas` either redirects to
    `/agent#personas` or is deleted. **Recommended** — matches the
    existing pattern.
  - (b) Wrap `PersonaManagementRouteComponent` in the agent shell
    (extract a `AgentShell { content: rsx!{…} }` wrapper that both
    `AgentPage` and `PersonaManagementRouteComponent` consume).
    Heavier refactor, lets the persona page keep its richer layout
    (TalkToOverlay, etc.).
- [ ] **Q.4** Only one persona is seeded (Koala the Broker) — but
  the demo has many backends. Verify whether this is the intended
  per-account behavior or a backend-coverage gap in the seed.

---

## Phase R — Global search (`/search`)

Clicked 🔍 in the global rail. Right pane is "Search servers,
channels, DMs, groups…" with toggles for Servers / DMs / Groups,
plus an ACCOUNTS filter column on the left.

- [x] **R.1** ✅ Not a bug (live-confirmed 2026-06-04). The "~15 accounts"
  premise was stale — the fresh-user demo seed was reduced (Phase A3/C work) to
  exactly 3 accounts (demo-cat, demo-dog, demo-platypus). The ACCOUNTS filter
  correctly shows **"All accounts — 3 of 3"**, listing all three. No accounts
  are silently dropped; nothing to fix.
- [ ] **R.2** Default state shows ALL servers/channels expanded.
  For a real user with many backends this scroll-list could be
  thousands of items. Collapse servers by default; expand on click
  or on a query match.
- [x] **R.3** ✅ `.search-page-input` capped at max-width 600px. Search input is full-pane-wide; constrain to ~600px
  for readability.
- [x] **R.4** ✅ Added per-type hover labels to search channel icons via an `icon_title` NodeRow prop (#→"Text channel", 🔊→"Voice channel", 📋→"Forum channel", etc.). Live-confirmed. Mixed channel icons (`#` text, voice glyph, forum
  glyph) — verify each has a visible legend or hover label so
  first-timers can tell text from voice without trial-and-error.

---

## Phase S — Chat header button order

Inspected `.chat-header-btn` rendering. All buttons have titles
(D.1 was wrong — they ARE labeled). But the order is odd:

Left-to-right (x coords): 📞 Call → 🎥 Video → ⚙️ Settings →
🧵 Threads → 📌 Pinned → 📰🔎 Search → 🤖 Agent → 👤 Members.

Settings (⚙️) sits *between* Video and Threads. Settings is a
preferences action; everything around it is a per-chat mode toggle.
The mental model breaks.

- [ ] **S.1** Reorder. Suggested grouping (with small dividers):
  voice-cluster (Call / Video) | chat-mode (Threads / Pinned /
  Search) | side-panels (Agent / Members) | overflow (Settings,
  preferably demoted into the ⚙️ icon at the far right or into a
  "more" menu).
- [x] **S.2** ✅ Replaced the stacked 📰+🔎 composite with a single 🔍 in the chat-header search button. The Search button uses two emojis stacked
  (`📰🔎` — newspaper + magnifying glass). Pick one. 🔎 alone is
  conventional and unambiguous. Newspaper reads as "feed/articles",
  not "search messages".

---

## Phase T — Chat agent side-panel (🤖 from chat header)

Opens a narrow (~240px) right-side panel with three subsections:
Memory, Pending Drafts, Reply Style — each shows the same string
"Agent is disabled for this chat" repeated 3×, plus a "Catch me up
→ Copy last 20 messages" section that DOES work without the agent.

- [x] **T.1** ✅ Agent panel now shows ONE disabled banner (gated at the panel level) instead of repeating the message in Memory/Drafts/Style; copy made actionable ("Agent is off for this chat. Turn it on with the toggle above…"). Live-confirmed: 1 banner. Consolidate the disabled-state copy. Instead of
  printing "Agent is disabled for this chat" three times, show ONE
  empty state at the top of the panel: "Agent is off for this chat
  · Turn on to see memory, drafts, and reply style. [Enable]".
- [ ] **T.2** [deferred — mobile pass] The 240px panel width forces every label to wrap. On
  desktop, give it min-width 320px. On mobile (Phase U) the panel
  should be full-screen overlay, not an inline column.

---

## Phase U — Mobile responsive layout

Set viewport to 390×844 (iPhone-class) and reloaded.

**What works:**
- Chat header reduces from 8 to 3 buttons (Call, Video, Members).
  Other actions presumably live behind a menu.
- Composer + message list look great at narrow width.
- A hamburger (☰) appears in the chat header.

**What's broken:**
- [ ] **U.1** Hamburger drawer opens a *three-column* layout (the
  far-left account-server bar + the middle nav column + the DM
  list) on a 390px viewport. Cumulative width exceeds the screen,
  causing partial-overlap with the chat behind it. The drawer
  should collapse to a single column (DM list) with an account-
  switcher header, not the full desktop sidebar stack.
- [ ] **U.2** Big blank band above the first message ("May 23,
  2026" is centered vertically in the empty third of the viewport).
  The list should auto-scroll to the bottom on open.
- [ ] **U.3** Composer Send button is missing on mobile — only +,
  emoji 😀, and bell 🔕 visible. Either Enter-to-send is the
  intended pattern (then label it via placeholder hint), or the
  Send arrow is being cropped off-screen.
- [ ] **U.4** The "NEW" pill on the date separator is on the right
  edge, even tighter on mobile than on desktop (Phase F still
  applies, more visible here).
- [ ] **U.5** A "Cat (demo) demo" tooltip persists in the top-left
  after the drawer opens — looks like a stale hover popover.
- [ ] **U.6** The chat header buttons drop from 8 to 3 — verify
  the dropped 5 (Settings, Threads, Pinned, Search, Agent) are
  available somewhere on mobile (overflow menu, swipe gesture).
  If not, mobile users lose access to half the chat functionality.

---

## Phase V — Voice channel view (Dev Voice)

Clicked into Poly Development → Dev Voice. View is clean:
- Header: "Dev Voice • demo" with member-count badge top-right
- Three participant tiles: Alice / Charlie / Grace (Grace highlighted with
  purple border + "Watching screen share" label)
- Bottom: "Join Voice" CTA (prominent, full-width)
- Sidebar shows the channel + members nested

Mostly good. One thing to note:

- [x] **V.1** ✅ Verified: member rows already use the shared `dm_user_sidebar::presence_dot_class` which renders Offline/Invisible as a grey dot (Unknown suppressed). With N.4 the friends panel now matches the same convention. Member-row indicators are inconsistent: Alice has a green
  presence dot, Charlie has a mic icon (mute?), Grace has a screen icon.
  Three different visual languages for what should be parallel status
  indicators. Pick a consistent grammar — e.g. always show a presence
  dot, then a stack of capability icons (mic-muted, video-on,
  screen-sharing) in a fixed order.

---

## Phase W — "+ New Conversation" composer panel

Sidebar "+ New Conversation" opens a friends-picker pane.

- [x] **W.1** ✅ Duplicates fixed (change `yryqkmll`). **Root cause:** the
  picker did `chat_lists.friends.values().flatten()` — merging EVERY active
  account's friend list. With demo-cat + demo-dog both active and sharing 6
  contacts (Charlie…Henry), each shared friend rendered twice. **Fix:** scope
  to the active account only (`friends.get(&active_account_id)`) — this is "new
  DM from the active account context" — plus a defensive `seen` HashSet dedup
  by id. Live-verified: picker now shows exactly 9 unique rows (Alice…Henry +
  Dog), no repeats.
- [x] **W.2** ✅ Filter the active account's own user id out of the
  `NewConversationView` friend picker. `new_conversation_view.rs` now
  computes `active_user_id` (via `account_sessions` + active account)
  and `.filter(|f| active_user_id != Some(f.id))`. Snapshot reads use
  `.peek()` (hang-class #7). **Live MCP confirmed 2026-06-04** (change
  `yryqkmll`): no "Cat (demo)" self entry in the picker. The self-filter is now
  belt-and-braces behind the W.1 active-account scoping (Cat's own list never
  contains Cat anyway; the earlier self-appearance was the cross-account
  `.values().flatten()` pulling Dog's Cat-as-friend entry).
- [x] **W.3** ✅ Trimmed the half-finished-feature sentence from `new-conversation-description`; it now reads simply "Choose one friend to start a direct conversation." The description ends with "Multi-person conversations
  will use this composer once shared group creation is wired." That's
  a half-finished-feature note shown to users. Either ship the feature
  or hide the copy until it lands.
- [x] **W.4** ✅ **Live MCP confirmed 2026-06-04**: clicking "Saved Messages"
  navigates to `/demo/demo/demo-cat/saved` and renders the "Saved Messages"
  view. The `dm_view.rs:396` onclick (`nav!(Route::SavedItemsRoute { … })`)
  works; this was fixed since the walkthrough — now verified live, not just code.

---

## Phase X — "Add Account" / Signup flow

Navigated to `/signup`. Left column lists backends, right pane shows
selected backend's form.

- [x] **X.1** **Matrix is missing from the picker.** Root cause: Matrix
  had FTL keys, an `authenticate()` helper, and a feature gate, but no
  `signup_render_fn` and no `register_signup_entry` call — so it was
  silently excluded from the picker. Added a `MatrixSignupPage` component
  (~80 lines, mirroring Stoat's URL+username+password form, defaults to
  `https://matrix.org`) and registered it in
  `register_native_signup_entries`. Added `poly-ui-macros` to
  `clients/matrix/Cargo.toml`. Shipped in change `klqpyxuk` (with X.3+X.4).
- [x] **X.2** Stale i18n key visible: the bottom of the Stoat signup
  form reads `Don't have an account? **Signup Register Link Action**
  →`. **Root cause:** `t("signup-register-link-action")` called with
  no args — but the FTL value references `{$service}`. fluent's
  `format_pattern` emits a "missing argument" error, `t()`'s
  `errors.is_empty()` guard fails, and on the default locale (`en`)
  the function drops through to the title-case fallback. The
  `.replace("{$service}", &host)` chained after never sees the
  placeholder. **Fix shipped** in `register_link.rs:65-66`: use
  `t_args("signup-register-link-action", &[("service", host)])`.
  **Sibling sites (NOT fixed — error-path only, file as X.2b):** the
  same `t("…").replace("{$…}", …)` smell exists in 6 other call sites
  (`bans.rs:127`, `ban_member.rs:118`, `kick_member.rs:106`,
  `timeout_member.rs:138`, `edit_channel.rs:155`,
  `overlays.rs:785-786`). Only `overlays.rs` is visible without
  triggering an error; the rest only render on failure. Sweep when
  next touching dialogs/.
- [x] **X.2b** Sweep `t("…").replace("{$…}", …)` siblings. Same root
  cause as X.2: on the `en` default locale, fluent's missing-`$arg`
  error makes `t()` drop through to the title-case fallback before
  `.replace()` can substitute, so the user would see e.g. "Dialog Ban
  Error" instead of `Failed to ban: …`. Swapped all 10 occurrences
  across 6 files to `t_args(key, &[(name, val)])`:
  `ban_member.rs` (title L35 + error L118), `kick_member.rs` (title
  L33 + error L106), `timeout_member.rs` (title L44 + error L138),
  `edit_channel.rs` (error L155), `bans.rs` (error L127),
  `overlays.rs` (chat-typing L785 + chat-typing-multiple L786 — the
  one visible everyday). The three dialog titles
  (`Kick/Ban/Timeout {$user}?`) were also buggy — they would have
  rendered "Dialog Ban Title" instead of "Ban Alice?" — those are
  visible the moment a moderator opens the dialog, not just on error.
  Verified with `cargo check -p poly-core --target wasm32-unknown-unknown`
  (EXIT=0); lint-gate baseline regen'd for line-number shifts.
- [x] **X.3** Backend descriptions are truncated with `…` in the
  left column. Fix: `.signup-nav-item-desc` was `white-space: nowrap;
  text-overflow: ellipsis` (single-line). Swapped to a 2-line clamp via
  `display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient:
  vertical; word-break: break-word`. Descriptions now wrap to a second
  line before being truncated. Shipped in change `klqpyxuk` (with X.1+X.4).
- [x] **X.4** Backend ordering between Add Account picker and the
  Plugins settings list differs (Stoat first here, Demo first there).
  Fix: reordered `register_native_signup_entries` to match the canonical
  order in `BUILTIN_BACKENDS` (`client_manager/mod.rs`) — Stoat → Matrix →
  Lemmy → GitHub → Forgejo → Hacker News → Poly Server. Demo isn't a
  signup entry (the picker doesn't include it; it's enabled-by-default).
  Bundled plugins (Discord/Teams/Reddit) still come after natives via
  `sync_bundled_signup_entries`. Shipped in change `klqpyxuk` (with X.1+X.3).

---

## Notes for future me

- The first screenshot was taken before knowing the in-app nuke
  exists. After using the nuke, the real Welcome wizard rendered —
  see Phase I for the observations there. The MCP `reset_app` issue
  (Phase A) is still real and worth fixing.
- The most important finding from this walkthrough is **Phase A2**:
  the in-app nuke is a one-click destructive action with no confirm.
  That's a UX safety bug, not a polish bug — it can destroy real
  user data with one stray click. Ship A2 before any of the
  cosmetic phases.
- poly-chat-mcp must be running on :3010 for the Personas tab to
  load. Start with `cargo run -p poly-chat-mcp`. Not a UI bug —
  filed here only as a reminder that the agent panel depends on a
  separate daemon.
- Screenshots used:
  - `/tmp/fresh-01-landing.png` — populated demo state (after MCP `reset_app`)
  - `/tmp/fresh-02-settings.png` — Accounts settings (post-click on bottom-left gear)
  - `/tmp/fresh-03-general.png` — General settings showing Reset / Nuke buttons
  - `/tmp/fresh-04-wizard.png` — actual Welcome wizard after nuke
  - `/tmp/fresh-05-after-getstarted.png` — back on General settings (URL preserved across nuke)
  - `/tmp/fresh-06-home.png` — default route lands back on DM (post-reseed)
  - `/tmp/fresh-07-home-icon.png` — Home → Overview → "Your Servers" grid
  - `/tmp/fresh-08-things-missed.png` — Overview → Things you missed
  - `/tmp/fresh-09-stats.png` — Overview → Stats (5-card grid)
  - `/tmp/fresh-10-agents.png` — Overview → Agents (empty-state copy)
  - `/tmp/fresh-11-friends.png` — People panel (Friends grid)
  - `/tmp/fresh-12-notifications.png` — Notifications panel
  - `/tmp/fresh-13-server.png` / `13b` — Poly Development server, #general
  - `/tmp/fresh-14-agent-panel.png` — Agent → Integrations
  - `/tmp/fresh-15-personas.png` — Agent → Personas (BEFORE Q.3 fix — missing sub-nav)
  - `/tmp/fresh-16-personas-fixed.png` — Agent → Personas (AFTER Q.3 fix, top of page)
  - `/tmp/fresh-17-personas-scrolled.png` — Agent → Personas (AFTER Q.3 fix, scrolled to anchor)
  - `/tmp/fresh-18-search.png` — Global search default state (all servers expanded)
  - `/tmp/fresh-19-search-filtered.png` — Search filtered by "rust"
  - `/tmp/fresh-20-agent-side.png` — Chat agent side-panel (🤖 from chat header)
  - `/tmp/fresh-21-pinned.png` — Pinned messages side-panel (📌 from chat header)
  - `/tmp/fresh-22-mobile.png` — Mobile viewport (390×844) chat view
  - `/tmp/fresh-23-mobile-menu.png` — Mobile viewport hamburger drawer (three columns stacked)
  - `/tmp/fresh-24-personas-q1-fixed.png` — After Q.1 fix: persona row reads cleanly ("No sources selected" on its own line)
  - `/tmp/fresh-26-nuke-modal.png` — Nuke confirm modal (before overlay CSS)
  - `/tmp/fresh-27-nuke-modal-styled.png` — Nuke confirm modal (after overlay CSS, full backdrop + centered card)
  - `/tmp/fresh-28-msg-ctx.png` — Message context menu (verified still working)
  - `/tmp/fresh-29-voice.png` — Voice channel view (Dev Voice in Poly Development)
  - `/tmp/fresh-30-new-convo.png` — "+ New Conversation" friends picker (note duplicate rows)
  - `/tmp/fresh-32-signup.png` — Add Account picker (note missing Matrix entry)
  - `/tmp/fresh-33-stoat-signup.png` — Stoat signup form (note stale "Signup Register Link Action" key)
  (These live in `/tmp` and won't survive reboot. Recapture if needed.)

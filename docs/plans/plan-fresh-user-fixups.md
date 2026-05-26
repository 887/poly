# Fresh-user walkthrough — fixups & things to look into

> Captured 2026-05-25 during a `reset_app` + fresh-user walkthrough of
> `apps/web` (chromium @ :3000). The walkthrough barely got past the
> first paint before turning up issues — the reset itself doesn't land
> at the setup wizard, so every subsequent screen is contaminated by
> demo-seed state. Phase A unblocks the rest.

## Status: 🚧 IN PROGRESS — observations only, no fixes shipped yet

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

- [ ] **A.1** Decide the contract: should `reset_app` (a) wipe the
  SQLite file too, (b) only wipe `poly_kv` rows but keep the schema,
  or (c) flip a "wizard-pending" flag that the boot path honors
  regardless of stored accounts? Document the choice in
  `mcp/web-devtools-mcp/src/main.rs`.
- [ ] **A.2** Implement the chosen contract. If (a), the MCP needs to
  resolve `POLY_DATA_DIR` / the platform default and unlink the
  sqlite3 file before reload. If (b)/(c), the host bridge needs a
  `/host/reset` route the MCP can POST to.
- [ ] **A.3** Simplest implementation: have the MCP hit the same
  `client_manager.nuke_all_data()` path the in-app button uses
  (`crates/core/src/ui/settings/general.rs:317`). Then both surfaces
  share behavior.
- [ ] **A.4** Update the MCP tool description so it no longer
  promises the wizard if it doesn't actually deliver it.
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
- [ ] **A2.2** Move the button visually further from the everyday
  controls. Currently sits right next to "Reset App Data" with no
  visual separation — they share a row. Use a collapsed "Danger
  Zone" section that the user has to expand.
- [ ] **A2.3** Wire an "undo within 10s" toast for the gentler
  "Reset App Data" path (the one that just logs out backends — not
  the nuke). Nuke itself can't reasonably be undone.

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

- [ ] **A3.1** After-nuke navigation: clear the in-memory router
  state on nuke success and push the user to `/` (which the
  no-account branch routes to Welcome). Don't preserve the URL the
  user was on when they pressed the button.
- [ ] **A3.2** dev-plugins auto-seed: gate behind an env knob
  (`POLY_AUTOSEED_DEMO=1`, default `0` after a confirmed nuke). Or
  store a `wiped_at` timestamp in KV and skip seed if it exists.
- [ ] **A3.3** Add a "Load demo accounts" button somewhere in
  settings (visible only when `dev-plugins` is on) so the developer
  can opt back into the seed when they want it.

---

## Phase B — Date format inconsistency in chat view

Same screen shows two date conventions side-by-side:

- **Date separator** (`.date-separator-text`): `May 23, 2026` /
  `May 25, 2026` — English long form, US-style.
- **Per-message timestamp** (`.message-timestamp`): `23/05/2026, 11:44`
  — DD/MM/YYYY 24h local (the format we just fixed in Phase 3 of
  last session, commit `kkuzvplr` / `ec441761`).

Either both should be DD/MM/YYYY-coded or both should be long-form;
mixing them looks like a half-finished i18n pass.

- [ ] **B.1** Pick one canonical format. Recommendation: match the
  per-message convention (`%d/%m/%Y`) on the separator too —
  `25/05/2026` reads consistent with the row-level stamps. If the
  separator needs the day-name for scanability, use `Mon 25/05/2026`
  (chrono `%a %d/%m/%Y`).
- [ ] **B.2** Patch `format_date_separator` (probably in
  `crates/core/src/ui/account/common/chat_view/message_row.rs` near
  the existing `format_timestamp` we already migrated). Match the
  same `with_timezone(&chrono::Local)` pattern so it tracks the
  user's locale.
- [ ] **B.3** Audit other date sites: history scroll markers, the
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

- [ ] **D.1** Snapshot the actual DOM (we have `take_snapshot` for
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

## Phase E — Unread badge stays on the open DM

The DM list shows Bob with a red `(1)` unread badge even while the
Bob conversation is *currently open* on the right pane. Opening a
conversation should clear its own unread counter, not just the
new-message-divider.

- [ ] **E.1** Repro reliably (open closed DM → confirm badge present
  → confirm badge persists after open).
- [ ] **E.2** Find the unread-clear hook — probably in the
  `open_message_hit` / channel-select path in
  `crates/core/src/ui/account/common/chat_view/` or in the per-channel
  read-state writer.
- [ ] **E.3** Wire the clear-on-select. Don't add a new effect for
  this — the existing channel-select path already touches state, the
  unread-clear belongs in that batch.
- [ ] **E.4** Confirm the demo backend supports an unread-write at
  all (if it's a backend-NotSupported, the badge will reappear on
  next sync; gate the badge on backend capability).

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

- [ ] **G.1** Snapshot the DOM, list each icon's `title` /
  `aria-label`. Anything missing gets one.
- [ ] **G.2** The refresh-arrow specifically is suspicious — what
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

- [ ] **I.1** Tighten the vertical layout — push the content up so
  the h1 sits at roughly 15–20% from the top, not 30%. Leave the
  CTA with healthy breathing room but stop the page from looking
  half-empty.
- [ ] **I.2** Replace the 🔑 emoji with a privacy-coded glyph
  (🔒 or an inline SVG shield) — and verify the cards' icons all
  share a visual weight (the 🤖 emoji is heavier than the other
  two; consider unifying with a flat-icon set).
- [ ] **I.3** Make "Get Started" prominent: ~200px min-width, larger
  font, maybe a subtle motion/glow hover.
- [ ] **I.4** Add a Poly wordmark or logo above "Welcome to Poly".

---

## Phase J — Server icons in the left rail have no labels

Inspected the `.server-icon` elements in the second-from-left column
(account-server bar). Every entry has `title=""` and no
`aria-label`. Hovering tells you nothing. The icons themselves are
small avatar circles (~48px) without text — first-time users would
have no way to learn which server is which without clicking.

- [ ] **J.1** Add `title="<server-name>"` to every `.server-icon`
  render site. Probably in
  `crates/core/src/ui/account/common/account_server_bar/server_list.rs`
  (saw the `on_context_menu` handler there earlier; the render is
  nearby).
- [ ] **J.2** Add `aria-label` matching the title for screen
  readers.
- [ ] **J.3** Consider a hover-tooltip card that shows server name,
  unread count, and "last active" — more useful than the bare name.

---

## Phase K — Overview ("Your Servers") layout

The Home → Overview → General screen shows 4 server cards in a
responsive grid but the 4th card (`Cat ↔ Dog Arena`) sits alone on
its own row, left-aligned, looking lonely. The Search field above
the grid is full-width and very tall — disproportionate for a
search input.

- [ ] **K.1** Make the card grid balance the last row: either fill
  the row by stretching cards, or center the orphan card if it's a
  partial row.
- [ ] **K.2** Constrain the "Search…" input width to ~500px and
  reduce vertical padding — it currently dominates the right pane.
- [ ] **K.3** Each card shows `N members · M unread · @K mentions`
  as inline text — break into a visual chip row so the metrics read
  as scannable badges, not one comma-separated string.

---

## Phase L — "Things you missed" cards are non-interactive

The Things-you-missed panel lists unread DMs + notifications as
cards but none of the cards have action buttons. The Notifications
panel (🔔) shows the SAME items with Accept/Deny/Join/Dismiss
buttons. So the user gets two surfaces showing the same data, one
useful and one not. Either the cards here should be actionable, or
the panel should be removed in favor of just linking to
Notifications.

- [ ] **L.1** Decide whether "Things you missed" is a *summary* (no
  actions, just counts and links into the real screens) or a
  *parallel inbox* (full actions). Pick one — don't be the half-way
  thing it is now.
- [ ] **L.2** If summary: each card should be a link that opens the
  source conversation. Add `cursor: pointer` and a hover state.
- [ ] **L.3** If parallel inbox: copy the Accept/Deny/Join buttons
  from the Notifications panel onto these cards. But then we have
  state-sync questions — accepting a friend request in Things-you-
  missed needs to update the Notifications count too.

---

## Phase M — Stats page is barebones

Stats shows 5 cards (Servers, Direct Messages, Groups, Unread,
Mentions) with raw counts. Subtitle "Your activity at a glance"
promises *activity*; the cards show *inventory*.

- [ ] **M.1** Either rename the page to "Inventory" / "At a glance"
  or add real activity metrics: messages sent per day, time-in-app,
  most-active channels, etc.
- [ ] **M.2** Make the cards clickable: UNREAD → Things-you-missed,
  SERVERS → Overview General grid, etc.
- [ ] **M.3** Fix the wrap: 4 cards on row 1, 1 orphan on row 2.
  Either fit 5 across, or wrap to 3+2.
- [ ] **M.4** Aspirational: add small sparklines / 7-day trends on
  each card. Optional nice-to-have.

---

## Phase N — People panel

Clicked 👥 (People). Sub-nav: Friends / Ignored / Blocked Users.
Right pane is a grid of friend cards (avatar + name + handle +
Message button).

- [ ] **N.1** The "Friends" sub-nav label appears to render twice
  visually — once as a label/tooltip floating near the top of the
  middle column, once as the highlighted nav item. Inspect the DOM
  and either kill the tooltip on this page or fix the stacking.
- [ ] **N.2** No "Add friend" button anywhere. Friends panels in
  Discord / Slack / etc. always have it as a primary CTA. Add one
  near the search.
- [ ] **N.3** Every friend card shows the same handle ("demo") since
  they're all on the demo backend. Looks redundant in this view. If
  it must stay, gray it heavily so the eye skips it.
- [ ] **N.4** Status dot consistency: some cards show a green dot,
  some don't. If "no dot" means offline, that's a usability fail —
  there should be a visible "offline" indicator (grey dot) so the
  reader knows the state was checked.

---

## Phase O — Notifications panel

Notifications (🔔). Sub-nav: All notifications (7) / Mentions (2) /
Friend requests (2) / Server invites (2) / Voice invites (1) / Other (0).
Right pane lists notification cards with action buttons.

- [ ] **O.1** The middle-column subtitle reads "No new notifications"
  while the list shows 7. Contradicts. Either the subtitle is stale
  (probably) or means "no unread" (then label it "All caught up" or
  hide it when the list is non-empty).
- [ ] **O.2** Action button wording inconsistent: Friend Request =
  Accept / **Deny**, Server Invite = Accept / **Decline**. Pick one
  word. "Decline" feels more polite for both.
- [ ] **O.3** Type pills (Mention / Friend Request / Server Invite /
  Voice Invite) are all neutral grey. Color-code by type so the eye
  can chunk them.
- [ ] **O.4** Ordering: items are grouped by type then time, not
  pure-time. Voice Invite (1 hour ago) sits below Server Invites (3
  hours, 6 hours). Either commit to time-sort or to type-grouping;
  the current half-way is confusing.
- [ ] **O.5** "Mark as Read" appears both on each card AND as a
  free-floating button at the bottom-left of the middle column. The
  free-floating one's scope is unclear (all? this filter? selected?).
  Label it explicitly, e.g. "Mark all as read".

---

## Phase P — Agent / Integrations panel

Agent → Integrations. MCP server config + integration feature list.

- [ ] **P.1** Label "Settings Mcp Transport" uses inconsistent case
  ("Mcp" vs "MCP" used elsewhere on the same page). Pick one (MCP
  is the canonical acronym) and apply everywhere.
- [ ] **P.2** The secondary line under "Settings Mcp Transport"
  appears to be a stale i18n key or duplicate label. Inspect and
  either give it real copy or remove.
- [ ] **P.3** The Port input (containing "3010") spans the full
  pane width. A 4-digit port number doesn't need that much real
  estate. Constrain to ~120px.
- [ ] **P.4** The page is titled "Integrations" AND there's a sub-
  section headed "Integrations" with the feature rows (Suggested
  responses / Conversation summaries / etc.). Rename one. The lower
  list is really "Features" or "Capabilities".
- [ ] **P.5** Feature rows (Suggested responses, etc.) have no
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

- [ ] **R.1** ACCOUNTS filter shows only 3 entries (Cat (demo), Dog
  (demo), Platypus (demo_forum)) even though the user has ~15
  connected accounts. Verify: is the search index per-backend
  capability-gated (e.g. only `dev-plugins` backends are indexed)
  or is this silently dropping accounts? If the latter, surface the
  reason — "this backend doesn't support search" — instead of
  hiding the account.
- [ ] **R.2** Default state shows ALL servers/channels expanded.
  For a real user with many backends this scroll-list could be
  thousands of items. Collapse servers by default; expand on click
  or on a query match.
- [ ] **R.3** Search input is full-pane-wide; constrain to ~600px
  for readability.
- [ ] **R.4** Mixed channel icons (`#` text, voice glyph, forum
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
- [ ] **S.2** The Search button uses two emojis stacked
  (`📰🔎` — newspaper + magnifying glass). Pick one. 🔎 alone is
  conventional and unambiguous. Newspaper reads as "feed/articles",
  not "search messages".

---

## Phase T — Chat agent side-panel (🤖 from chat header)

Opens a narrow (~240px) right-side panel with three subsections:
Memory, Pending Drafts, Reply Style — each shows the same string
"Agent is disabled for this chat" repeated 3×, plus a "Catch me up
→ Copy last 20 messages" section that DOES work without the agent.

- [ ] **T.1** Consolidate the disabled-state copy. Instead of
  printing "Agent is disabled for this chat" three times, show ONE
  empty state at the top of the panel: "Agent is off for this chat
  · Turn on to see memory, drafts, and reply style. [Enable]".
- [ ] **T.2** The 240px panel width forces every label to wrap. On
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

- [ ] **V.1** Member-row indicators are inconsistent: Alice has a green
  presence dot, Charlie has a mic icon (mute?), Grace has a screen icon.
  Three different visual languages for what should be parallel status
  indicators. Pick a consistent grammar — e.g. always show a presence
  dot, then a stack of capability icons (mic-muted, video-on,
  screen-sharing) in a fixed order.

---

## Phase W — "+ New Conversation" composer panel

Sidebar "+ New Conversation" opens a friends-picker pane.

- [ ] **W.1** The friend list shows duplicates: Charlie, Diana, Eve,
  Frank, Grace, Henry appear once with no checkbox / no avatar, then
  Alice, Bob, Charlie, Diana, Eve, Frank, Grace, Henry, Dog (demo) appear
  again with checkboxes AND avatars. Likely two render-passes overlap, or
  there are two intentionally-distinct sections (recent? all?) that
  aren't visually labeled.
- [ ] **W.2** "Cat (demo)" — the *current account* — appears mid-list
  as a selectable contact. You shouldn't be able to start a DM with
  yourself; filter self-account out of the friend picker.
- [ ] **W.3** The description ends with "Multi-person conversations
  will use this composer once shared group creation is wired." That's
  a half-finished-feature note shown to users. Either ship the feature
  or hide the copy until it lands.
- [ ] **W.4** Clicking "Saved Messages" from the same sidebar did
  nothing (no view change). Either the route is broken or the click
  target is misplaced.

---

## Phase X — "Add Account" / Signup flow

Navigated to `/signup`. Left column lists backends, right pane shows
selected backend's form.

- [ ] **X.1** **Matrix is missing from the picker.** The Accounts
  settings page shows Owl + Axolotl on the Matrix backend, and the
  Plugins page confirms Matrix is enabled — but the Add Account list
  has Stoat, Poly Server, Lemmy, Hacker News, GitHub, Forgejo, Discord,
  Microsoft Teams, Reddit, Test Accounts. No Matrix. Either Matrix
  signup is gated behind a feature flag (then say so) or it's silently
  excluded (then bug).
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
- [ ] **X.3** Backend descriptions are truncated with `…` in the
  left column. Either the column should be wider, or hover should
  show the full text in a tooltip.
- [ ] **X.4** Backend ordering between Add Account picker and the
  Plugins settings list differs (Stoat first here, Demo first there).
  Pick one canonical order.

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

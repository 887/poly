# Phase 2.15 — Chat Search, Member Rail, and Markdown Composer

> **Created:** 2026-03-07  
> **Status:** In Progress  
> **Dependencies:** Phase 2.10 (layout polish), Phase 2.14 (backend search/pin abstraction)  
> **Related Docs:** [message-interactions-plan.md](message-interactions-plan.md)

---

## Goal

Finish the Discord-like chat-shell pass by fixing the right-side chat layout and completing the
first end-to-end markdown/search experience:

- inline contextual search in the chat header
- popup filter discovery for `from:` / `has:` / `mentions:`
- integrated member list as part of the chat window instead of a detached drawer
- remembered member-list open state
- markdown rendering in messages
- richer multiline composer with attachment previews
- demo data that visibly exercises markdown rendering

---

## 2.15.1 — Integrated Right Column

- [x] Move server/group member lists into the chat shell instead of route-level detached sidebars
- [x] Keep the four top-right action icons (threads, pins, mute, members)
- [x] Remove redundant header metadata that competes with search/actions
- [x] Preserve per-channel member switching behavior
- [x] Persist remembered open/closed member-list state in app settings

## 2.15.2 — Search UX Completion

- [x] Keep search inline in the header
- [x] Restore filter/tag overview as an anchored popup for empty/focused search
- [x] Use contextual placeholder text (`Search #channel`, `Search user`)
- [x] Highlight matched text in search result previews
- [x] Keep backend-abstract search and jump-to-message flow

## 2.15.3 — Markdown Messages

- [x] Add workspace markdown parsing/rendering dependencies
- [x] Render common markdown in message bodies (headings, emphasis, code, links, lists, quotes, tables)
- [x] Sanitize generated HTML before rendering
- [x] Style markdown blocks to match the existing chat theme
- [x] Support demo messages that showcase markdown tables/checklists/code blocks

## 2.15.4 — Composer Upgrade

- [x] Move upload affordance to the left side of the compose row
- [x] Keep inline utility buttons inside the compose shell
- [x] Auto-grow composer up to roughly half the chat height
- [x] Use contextual faded placeholder text
- [x] Show pre-send attachment previews for selected images/files
- [x] Clear previews after send

## 2.15.5 — Validation

- [x] `cargo fmt --all`
- [x] `cargo check --workspace`
- [x] `cargo cranky --workspace`
- [x] `cargo check -p poly-web --target wasm32-unknown-unknown`
- [x] `dx build --platform desktop` in `apps/desktop-devtools`
- [x] Desktop DevTools visual verification of search, right column, markdown, and composer previews

---

## Session Notes

- 2026-03-07: Follow-up pass created after backend-abstract search/pins landed. Scope expanded to
  include popup search filters, integrated member list, contextual placeholders, markdown
  rendering, and richer compose previews.
- 2026-03-07: Follow-up fixes verified the contextual Fluent placeholders with `t_args`, extended
  demo search/pinned coverage to DM and group conversations, and added pre-send attachment preview
  cards in the upgraded composer. Desktop DevTools verification confirmed the inline search popup,
  DM search hits with marked matches, DM pinned messages, and markdown rendering in demo chats.

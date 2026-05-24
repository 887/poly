# Plan: SOLID + missing-impl audit — `clients/lemmy/`

## Status: IN PROGRESS — Phase A shipped, B/C documented

Audit pass over `clients/lemmy/src/{api.rs,guest.rs,lib.rs,signup.rs,wit_bindings.rs}`
(4080 LoC). Identifies SOLID violations and missing implementations; tracks
ship-now wins through to landed commits.

Scope: only `clients/lemmy/`. Do NOT touch other client crates.

---

## Phase A — Ship-now wins (≤50 LoC each, max 3)

Small, low-risk cleanups landed during the audit pass itself.

- [x] **A.1** Drop `// TODO: migrate to` boilerplate from the in-memory
      settings storage doc-comment (`clients/lemmy/src/lib.rs:61`). The
      "Pack C P18" stub note is repeated identically in 4 plugins and
      now obscures live API docs; either reference the parent plan or
      delete. _≤5 LoC._ — shipped in change `sqlpzqyv`
- [x] **A.2** Tighten `Err(ClientError::NotSupported(...))` strings into
      one constant per category (`FRIEND_SYS_UNSUPPORTED`, `GROUP_DM_UNSUPPORTED`)
      at the top of the impl block. `lib.rs` currently re-allocates the
      same string in 12+ sites (`:1427-1568`). Pure dedup, no behaviour
      change. _≈30 LoC removed._ — shipped in change `sqlpzqyv`
      (4 consts: FRIEND_SYS, GROUP_DM, CONVO_MUTE, IGNORE; 13 call sites replaced)
- [x] **A.3** Move the literal selector `"a.title"`-style parser
      helpers out of `lib.rs::get_messages` handler bodies into top-of-
      file factory `fn`s. (N/A for lemmy — no scraper; placeholder for
      audit symmetry.) **Drop this checkbox if no parser usage exists.**
      — N/A confirmed: no scraper/parser in lemmy client

## Phase B — Medium refactors (50-300 LoC, max 5)

- [ ] **B.1** Split `clients/lemmy/src/lib.rs` (1729 LoC, 108 fns).
      Single Responsibility violation — one file impls
      `IsBackend`, `ForumBackend`, `ModerationBackend`, `SocialGraphBackend`,
      `DmsAndGroupsBackend`, `MessagingBackend`, `ServerAdminBackend`,
      `DiscoverBackend`. Move each trait impl to its own sibling module
      (`mod forum;`, `mod moderation;` …). Mechanical, ~200 LoC of
      module headers + reorg. _Interface Segregation gain: the
      compile-error blast radius of changing one trait shrinks._
- [ ] **B.2** Split `clients/lemmy/src/api.rs` (1664 LoC). Single file
      hosts HTTP shim, request types, response types, mapping logic,
      AND fixture tests. Suggest: `api/mod.rs`, `api/http.rs`,
      `api/types.rs`, `api/mapping.rs`, keep tests beside the unit they
      cover. _SRP — currently any DTO change recompiles the HTTP layer._
- [ ] **B.3** `IsBackend::authenticate` (`lib.rs:185-300+`) handles
      three credential variants inline with deeply nested matches and
      duplicated `LemmySession` construction. Extract one private
      `fn finalize_session(person, jwt) -> LemmySession` helper to
      collapse the three arms. _DIP — handler stops knowing how
      `Person` becomes `LemmySession`._
- [x] **B.4** `guest.rs` (494 LoC) duplicates ~30 `NotSupported`/`Ok(vec![])`
      stubs across 6 trait impls. Once Phase C.1 lands (real shared
      logic), these become one-line delegates. Until then, dedup the
      stub strings via shared `const`s in `guest.rs` top.
      — shipped in change `vpypsowlyrqz` (4 consts: NS_GROUP_DMS, NS_CODE_CHANNELS,
      NS_WASM_NOT_IMPL, NS_FORUM_NOT_IMPL; 9 call sites replaced with const refs)
- [x] **B.5** ForumBackend::get_forum_posts (`lib.rs:1002`) returns
      `NotSupported` — this is a **capability gap, not a NotSupported-
      by-design**. Lemmy is fundamentally a forum; this method should
      delegate to the existing post-listing code in `api.rs`. Implement.
      — shipped in change `vpypsowlyrqz` (delegates to `fetch_posts_paged`;
      maps ForumSortOrder::LatestActivity→"Active", CreationDate→"New";
      populates starter_message_id + message_count from PostCounts)

## Phase C — Architectural rewrites (>300 LoC, max 3)

- [ ] **C.1** WASM guest plugin (`guest.rs`, 494 LoC) is a wholesale
      stub — every method returns `NotSupported`/`Ok(vec![])`/`NotFound`.
      Native `LemmyClient` (lib.rs) holds all real logic. Either:
      (a) compile the native client to wasm32-wasip2 and wire `guest.rs`
      to delegate (preferred — DRY), or
      (b) re-implement the API surface in the guest via host-api HTTP
      calls. Both are >300 LoC and need a separate design doc.
      _Liskov violation: `LemmyPlugin as MessengerClientGuest` claims to
      be a backend but obeys none of the documented contract._
- [ ] **C.2** Trait-fan-out in `lib.rs`. `LemmyClient` implements 8
      poly_client traits — half return `NotSupported` (`SocialGraphBackend`
      14 methods, all err; `DmsAndGroupsBackend` 12 methods, all err).
      Interface Segregation: the host should request only the traits a
      backend actually implements via `as_social_graph() -> Option<&dyn _>`
      (already exists for some) consistently — and Lemmy should return
      `None` for unsupported capabilities rather than impl-then-err.
      Plan-level: requires a sweep of `poly_client` trait surface.
- [ ] **C.3** `api.rs` HTTP layer holds raw `serde_json::Value`-shaped
      response types alongside typed `LemmyPost`. Open/Closed violation:
      adding a new endpoint requires editing the central
      `LemmyHttp::get` match. Replace with a `trait LemmyEndpoint`
      pattern (associated `RESP` type + URL builder).

---

## Findings index (file:line)

- Dead docs: `lib.rs:61` (TODO Pack C P18 stub).
- Repetitive `NotSupported` allocs: `lib.rs:1427,1431,1435,1443,1447,1451,1455,1459,1463,1471,1517,1523,1529,1533,1537,1541,1551,1555,1559,1568,1578,1601,1610,1614,1623,1645,1654,1670,1674,1678`.
- WASM stub-everything: `guest.rs:51-469`.
- Real capability gap (not by-design): `lib.rs:1002` `get_forum_posts`,
  `lib.rs:1610` `search_messages`.
- SRP violations: `lib.rs` 1729 LoC / 108 fns; `api.rs` 1664 LoC.

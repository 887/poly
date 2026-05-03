# SOLID Survey Shard B — Open/Closed + Liskov Substitution

> Investigation only — no refactors performed. File:line evidence below.

---

## B.1 — Top 5 OCP wins (ranked by ROI)

### B.1.1 — Eliminate the `capabilities_for_slug` table; route through the trait

**The burden today.** Backend capabilities live in **two synchronized
declarations** that drift independently:

1. The **slug-keyed match arms** in
   `clients/client/src/types.rs:81-146` (`capabilities_for_slug`) — every
   new backend adds an arm here.
2. The **trait method override** `ClientBackend::backend_capabilities()`
   on every `impl ClientBackend for *` — 11 impl sites:
   - `clients/client/src/lib.rs:705` (default)
   - `clients/hackernews/src/lib.rs:430`
   - `clients/forgejo/src/lib.rs:487`
   - `clients/demo/src/lib.rs:486`, `:980`, `:1427` (×3 — see B.1.5)
   - `clients/stoat/src/lib.rs:1206`
   - `clients/github/src/lib.rs:406`
   - `clients/lemmy/src/lib.rs:1359`
   - `clients/discord/src/lib.rs:1338`
   - `clients/teams/src/lib.rs:755`
   - `clients/matrix/src/lib.rs:1522`

A **parity test** per backend exists precisely to catch the inevitable
drift — `clients/discord/tests/capabilities.rs:32-41` says it best:

> "If someone tweaks Discord's declaration without syncing
> `capabilities_for_slug("discord")` the UI gating layer will drift
> silently. This test catches that drift."

That test is itself the smell — the safety net only exists because the
abstraction has a parallel-data leak.

**Consumer call sites (31 grep hits, see below) all read via slug**
because they have only a `BackendType` (slug newtype) in hand, not a
live backend. Sample:
- `crates/core/src/ui/routes.rs:1221`, `:1839`, `:1972`, `:1995`, `:2311`
- `crates/core/src/ui/account/common/chat_view.rs:4251`
- `crates/core/src/ui/favorites_sidebar.rs:697`, `:756`
- `crates/core/src/ui/context_menu/menus.rs:136`
- `crates/core/src/ui/account/common/account_bar.rs:296`
- `crates/core/src/ui/signup.rs:275`, `:385`
- `crates/core/src/ui/account/server/settings.rs:226`, `:398`
- `crates/core/src/ui/account/settings/notifications.rs:131`
- `crates/core/src/ui/account/settings/content_social.rs:390`
- `crates/core/src/ui/account/common/forum_view.rs:238`, `:637`
- `crates/core/src/ui/account/common/discover_communities.rs:94`
- `crates/core/src/ui/account/common/notifications.rs:139`, `:150`

Plus the `Pack-F` test at `clients/client/src/types.rs:1798-1808` which
hard-codes a SECOND identical match-arm table just for regression. THREE
declarations of the same data.

**The abstraction.** Either:
- **(a)** Make `BackendCapabilities` a `&'static` keyed by slug owned by
  the plugin crate itself, exposed via a slug→caps registry the host
  builds at startup from each enabled feature (no match arms, just
  registration).
- **(b)** Add `BackendType::capabilities()` that delegates to the
  registry, deleting `capabilities_for_slug` and the per-test parity
  drill.

**Effort.** M (medium). Trait method already exists; the host doesn't
yet ship a slug→caps registry, so `ClientManager` would need to learn
to expose one. But the deletion is mechanical: the slug match arms in
both `capabilities_for_slug` AND the `expected()` test in
`pack_f_capability_gates` go away.

**Sites simplified.** ≈31 consumer call sites unchanged in spelling but
now reading from one source. Two source-of-truth declarations + the
test become one declaration. Per-backend parity test deletes outright.

---

### B.1.2 — Demo backend triplication: extract `DemoFlavour` parametric impl

**The burden today.** `clients/demo/src/lib.rs` declares three structs
and three full `ClientBackend` impls:

| Struct | Decl | impl span | impl LOC |
|--------|------|-----------|----------|
| `DemoClient` | `:77` | `:111-694` | 579 |
| `DemoClient2` | `:697` | `:729-1188` | 455 |
| `DemoClient3` | `:1191` | `:1223-1822` | 600 |

≈1,634 lines of near-identical method bodies. Confirmed by
`diff` of the first ~400-line slices: arm-by-arm, the only differences
are the data source name (`data::demo_session` vs `data::demo2_session`
vs `data::demo3_session`) and a couple of friend-list slice indices.

**The abstraction.** A single
`pub struct DemoClient<F: DemoFlavour>` (or even a runtime
`DemoClient { flavour: DemoFlavour }`) where `DemoFlavour` is a tiny
trait or struct that supplies the data-source bindings. Each "flavour"
shrinks from ~500 lines to ~30 (just the data-source pointers).

**Effort.** M. Mostly mechanical sed; the friend-list slice differences
become a `friends_for(&self) -> Vec<User>` flavour method. Beware: the
existing tests probably instantiate the concrete `DemoClient2` /
`DemoClient3` types directly — those test sites need
`DemoClient::flavour(2)` style construction.

**Sites simplified.** 1 file, ~1,000 lines deleted net. Future demo
flavour adds 30 lines, not 500.

---

### B.1.3 — `BackendType::from("<slug>")` literal-spam in plugin internals

**The burden today.** Each backend hard-codes its own slug as a string
literal at every `Message`/`User`/`Session` construction site.

| File | `BackendType::from("<slug>")` count |
|------|-------------------------------------|
| `clients/matrix/src/lib.rs` | 11 |
| `clients/discord/src/lib.rs` | 9 |
| `clients/stoat/src/lib.rs` | 9 |
| `clients/discord/src/lib.rs` (other) | many in tests |
| Total across `clients/` | 199 |

If we ever rename Discord's slug from `"discord"` to e.g. `"discord-v2"`
during a plugin migration, that's 9-11 string literals to hand-update
*per backend*, with no compile-time safety net.

**The abstraction.** Each plugin crate exports a `pub const SLUG:
&'static str = "matrix";` at the crate root, and every internal
construction uses `BackendType::from(crate::SLUG)`. Or better: define
a `const fn slug() -> BackendType` and let `from(SLUG)` collapse to
`crate::backend_type()`.

**Effort.** S (small). Mechanical sed inside each plugin's own crate.
No public-API change, no cross-crate coordination. Per-backend parallel
worktrees safe (disjoint files).

**Sites simplified.** 199 string literals → 1 `pub const` per backend
crate (≈10 backends = 10 declarations total).

---

### B.1.4 — `Route` enum parallel match-arm constellations

**The burden today.** Adding a new route variant requires touching the
enum *and* every parallel match in `crates/core/src/ui/routes.rs`. The
matches are:

| Function | Line | Arms |
|----------|------|------|
| `route_account_id` | `:105-152` | 32 arms (one per Route variant) |
| `sync_route_to_app_state` | `:524-…` (≈900 lines through ~`:1500`) | ≈40 arms |
| `route_variant_name` | `:2470-2515` | 41 arms |

`Route::` is referenced 133 times in routes.rs alone (`grep -c
'Route::' routes.rs`). Six other files match on `Route::*`
(`overview_sidebar.rs`, `action_outcome.rs`, `channel_list.rs`,
`routes.rs` itself). The macro `#[derive(poly_ui_macros::Connected)]`
already handles the *static* connectedness check, but does NOT generate
these per-variant dispatch tables.

**The abstraction.** The three (and any new) per-variant projections
share a structural pattern: project `(account_id, server_id,
channel_id, …)` from a Route. A trait impl per variant — something
like:

```rust
impl RouteScope for Route::ServerChat { ... }
```

— is not idiomatic Rust (no per-variant impls without newtype
wrappers), so the realistic approach is: a `proc_macro_derive(RouteScope)`
on the enum that synthesises the projections from `#[connected(...)]`
metadata already on each variant.

**Effort.** L (large). New macro infra + retrofit ~3 functions. Higher
risk than B.1.1 because we touch `sync_route_to_app_state`'s ≈40 arms
which DO have intentionally different bodies (DM vs server-chat vs
forum) — the macro must preserve per-variant behaviour. The safe
target is `route_account_id` and `route_variant_name` first; leave
`sync_route_to_app_state` for a follow-up that defines a
`RouteAppStateBinding` trait.

**Sites simplified.** Per-variant editing burden when adding a new
Route variant drops from N≈3 hand-edited match-arm tables to 1 enum
edit + 1 `#[connected(…)]` metadata line.

---

### B.1.5 — `ClientError::NotSupported` policy: `OK(default)` vs `Err`

**The burden today.** Trait defaults are inconsistent in their fallback
strategy. Some methods that "the backend doesn't have" return
`Ok(default)` (silently no-op the feature); others return
`Err(NotSupported)` (the UI must explicitly handle the error). This is
a Liskov violation as well — see B.2.3 — but it's also an OCP problem:
each new backend has to **read 90 trait methods' doc comments and
decide arm-by-arm** whether to override or accept the default. There
are 90 `NotSupported` references in `clients/client/src/lib.rs` alone
(the trait file).

`get_presence` (`clients/hackernews/src/lib.rs:402` returns
`Ok(Offline)`) vs `set_presence` (`:406` returns `Err(NotSupported)`)
is the prototypical asymmetry.

**The abstraction.** Capability-gated sub-traits (Interface
Segregation). E.g.:

```rust
trait ClientBackend { /* core: must impl */ }
trait Pinning { /* opt-in pin/unpin/list-pinned */ }
trait Presence { /* opt-in presence */ }
trait VoiceCalls { /* opt-in voice */ }
```

The host queries `if let Some(p) = backend.as_pinning() { … }` instead
of `match err { NotSupported => hide }`.

**Effort.** L (very large — the trait is 100 methods, ≈90 with
`NotSupported` defaults). Out of scope for an initial OCP sweep, but
flagging it for the eventual Phase 2 of "shrink the kitchen-sink
trait". Touching this disturbs every plugin and every consumer.

**Sites simplified (potential).** 90 default arms in trait + 31 slug
capability lookups in the UI all collapse to "ask the optional sub-
trait". This is the highest theoretical ROI but the longest fuse.

---

## B.2 — LSP violations worth fixing

### B.2.1 — `Teams::get_user` returns `NotFound` instead of `NotSupported`

`clients/teams/src/lib.rs:469-471`:
```rust
async fn get_user(&self, _id: &str) -> ClientResult<User> {
    Err(ClientError::NotFound("Teams user lookup not supported".into()))
}
```

**Contract broken.** Trait `get_user` (`clients/client/src/lib.rs:198`)
makes no `NotSupported` carve-out — it expects `Ok(User) | Err(NotFound
| NetworkError | Auth)`. Callers that branch on `NotFound` to mean
"that user really isn't on the server" will incorrectly conclude the
user doesn't exist when in fact the backend doesn't even support the
operation. A caller falling back to a "user might still exist, retry
elsewhere" path will retry forever.

**Safe fix.** Either: (a) change to `Err(NotSupported(...))` to match
the other "this backend can't" pattern; or (b) add `get_user_supported(
&self) -> bool` as a default-`true` capability hook on the trait and
have callers pre-check. Option (a) is mechanical and matches every
other backend's idiom.

---

### B.2.2 — `get_presence` `Ok(Offline)` masks "not supported"

`clients/hackernews/src/lib.rs:402-404`:
```rust
async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
    Ok(PresenceStatus::Offline)
}
```

**Contract broken.** Caller cannot distinguish "user is offline" from
"this backend has no presence concept". A presence widget will dimly
render every user as Offline forever, instead of hiding the widget
entirely. The companion `set_presence` (`:406`) returns
`Err(NotSupported)` for the *same* feature — the asymmetry IS the
contract violation: callers that probe by reading then writing get
different signals.

**Safe fix.** Replace `Ok(Offline)` with `Err(NotSupported)`, and add a
`PresenceStatus::Unknown` variant for the genuine "we don't know yet"
case. UI checks `caps.has_presence` (already exists in
`BackendCapabilities`) and skips the read entirely when unsupported.
Same fix shape applies to `get_friends → Ok(vec![])` (lines `:365`,
`:473` Teams) where empty-list silently encodes "feature missing".

---

### B.2.3 — `get_pinned_messages` default `Ok(vec![])` vs
`set_message_pinned` default `Err(NotSupported)`

`clients/client/src/lib.rs:152-155` and `:185-193`:
```rust
async fn get_pinned_messages(&self, channel_id: &str) -> ClientResult<Vec<Message>> {
    let _ = channel_id;
    Ok(Vec::new())  // ← read says "no pins"
}
async fn set_message_pinned(&self, ...) -> ClientResult<()> {
    let _ = (channel_id, message_id, pinned);
    Err(ClientError::NotSupported("set_message_pinned".to_string()))  // ← write says "can't"
}
```

**Contract broken.** A backend that supports pins but has 0 pinned
messages is indistinguishable from a backend that doesn't support
pins. A UI that decides "show pin button if `get_pinned_messages` is
non-empty" will hide pinning entirely on empty channels.

**Safe fix.** Make both default to `Err(NotSupported)`, and gate the
read at the UI layer on `caps.has_pinning` (add the bool to
`BackendCapabilities`). Or add a tri-state return: `Ok(Some(vec))` /
`Ok(None)` (=unsupported) / `Err(transient)`.

---

### B.2.4 — `send_reply_message` default silently downgrades to
`send_message`

`clients/client/src/lib.rs:119-127`:
```rust
async fn send_reply_message(
    &self, channel_id: &str, reply_to_message_id: &str, content: MessageContent,
) -> ClientResult<Message> {
    let _ = reply_to_message_id;       // ← discarded silently
    self.send_message(channel_id, content).await
}
```

**Contract broken.** The caller asked for a *reply*. The default
returns a message whose `reply_to` field on the wire is `None`. The
trait docstring says "falls back to `send_message` for backends that
do not yet expose reply semantics natively" — but no caller can detect
this fallback happened. The returned `Message`'s identity claims to be
a reply (caller expectation) when in fact it's a top-level post.

**Safe fix.** Either return `Err(NotSupported("send_reply_message"))`
in the default (forcing every backend to opt in), or add a
`caps.has_replies` capability bit and make the UI hide the reply
button on backends without reply support. The current behaviour is
worse than either alternative because it silently corrupts user
intent.

---

### B.2.5 — No panics found inside `impl ClientBackend for *` bodies

Searched for `panic!`, `unwrap()`, `expect(`, `unimplemented!`, `todo!`
inside `impl ClientBackend for *` blocks across `clients/*/src/`:
zero hits in production paths (all matches are in `#[cfg(test)]` mod
blocks: `matrix:2035`, `matrix:2114`, `stoat:2066`). No LSP "may not
panic" violations to fix here. ✅

---

## B.3 — False positives (don't abstract these)

### B.3.1 — `View` enum (`crates/core/src/state.rs:135`) — keep as enum

11 variants, ~59 match-arm sites in `crates/core/src/ui/`. Every match
is either rendering (which arm to show) or routing (which URL to
emit). These are intentionally exhaustive; if a new `View` lands the
compile error guides the implementer to every place that needs a
decision. A trait dispatch here would obscure the closed-set-of-views
intent and replace a compile error with a runtime "unhandled variant".
**Reject any plan to OCP-ify `View`.**

### B.3.2 — `ConnectionStatus` (`clients/client/src/types.rs:240`) and
`ContainerLabelForm` (`:197`) — keep as enums

5 variants and 3 variants respectively, with single-purpose
dispatches (`css_class`, `emoji`, `needs_reauth`). These satisfy the
"tiny known-finite set" exception — adding a variant is rare AND each
variant genuinely needs custom display logic. A trait would just be a
worse-spelled enum here.

### B.3.3 — `SidebarLayoutKind` (`clients/client/src/ui_surface.rs:346`)
— **borderline keep**

7 variants, dispatched once at `crates/core/src/ui/client_ui/sidebar.rs:145-164`.
Currently *the* point of the enum is to be a closed set of
host-supplied layouts (the comment at the top of the file:
"D5 — stock layout kinds the host renders natively"). Adding a layout
is supposed to be a host-side change, not a plugin-side change —
plugins that want a custom layout pick `Custom` and supply
`sections`. So the closed-set-ness is part of the design.

If we ever want plugin-supplied first-class layouts (not just
`Custom`), THEN visit this — but until then, this is a rare-edit closed
set, exactly where enum-match wins over trait-dispatch.

### B.3.4 — `ActionOutcome` (`clients/client/src/ui_surface.rs:181`)
— keep as enum

9 variants, dispatched in
`crates/core/src/ui/client_ui/action_outcome.rs:90-101`. The host owns
the outcome semantics ("Toast → push to toast queue", "Navigate → use
router") — plugins can only emit instances. A trait-per-variant would
spread host-policy across plugin crates, which is the opposite of
what we want. **Closed set by design.**

### B.3.5 — Slug-keyed `match` in `container_label_key`
(`clients/client/src/types.rs:166-192`)

Looks like an OCP smell on first glance, but it's a localization
fallback table — generic FTL key by default, override per-backend that
has a special term ("space" for matrix, "team" for teams, "repo" for
github). Moving this to the trait would require every plugin to ship
its own FTL fallback chain, which is what the locales/ system is
already supposed to handle if/when plugins get FTL bundles. Until
plugin-FTL ships, the centralized table is correct. **Reject any
near-term refactor.**

### B.3.6 — Per-Route literal slug check in
`routes.rs:2314` (`if backend == "lemmy" { … }`) and similar

Six total slug-equality checks in `crates/core/src/ui/`:

```
crates/core/src/ui/routes.rs:2314           if backend == "lemmy"
crates/core/src/ui/account/common/channel_list.rs:861   slug == "hackernews"
crates/core/src/ui/account/common/forum_view.rs:637     slug != "hackernews"
crates/core/src/ui/dialogs/edit_channel.rs:39           slug != "teams"
crates/core/src/ui/settings/plugins.rs:192-193          "discord" / "teams" emoji
```

Each is a single-line, single-backend special-case. Promoting these
to capability bits is fine, but each addition would be tiny — flag
as "do this opportunistically" rather than batch-refactor. Adding a
`caps.is_top_level_only_lemmy_quirk_X` per quirk is worse than the
literal. **Resist over-abstraction here.**

---

## Summary

| Win | Effort | Impact | Priority |
|-----|--------|--------|----------|
| B.1.3 — plugin slug constants | S | 199 literals → 10 consts | Do first |
| B.1.2 — Demo triplication | M | -1000 LOC | Do second |
| B.1.1 — kill `capabilities_for_slug` table | M | -2 sources of truth, -10 parity tests | Do third |
| B.1.4 — Route projection macro | L | partial — start with `route_account_id` only | Defer |
| B.1.5 — sub-trait Interface Segregation | XL | 90 trait methods → core+optional | Long horizon |

| LSP fix | Severity | Effort |
|---------|----------|--------|
| B.2.4 — `send_reply_message` silent downgrade | HIGH (silent data loss) | S |
| B.2.3 — pin read/write asymmetry | MEDIUM | S |
| B.2.2 — presence Ok(Offline) vs NotSupported | MEDIUM | S |
| B.2.1 — Teams `get_user` `NotFound` | LOW (Teams-only) | S |

End of shard B.

# poly-discord — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

`poly-discord` implements the `ClientBackend` trait for **Discord**. 

**⚠️ HIGH RISK**: Discord's Terms of Service explicitly prohibit unofficial clients and self-botting. Users should be warned clearly.

## Implementation Phase

**Phase 3.3** — Implementation approach to be decided at that time. See [Phase 3 Plan](../../docs/phase-3-plan.md) section 3.3.

## Research Notes (Phase 1)

### Current Landscape (as of 2026-02-28)

**Rust crates available** (all early-stage):
- `discord_client_gateway = "0.2.0"` — "Undetected discord client gateway reimplementation" by UwUDev (~800 downloads)
- `discord_client_rest = "0.1.1"` — REST API companion to the gateway crate (~400 downloads)
- These are pre-alpha, very low adoption, but may work as starting points

**JavaScript ecosystem**:
- `discord.js-selfbot-v13` — ARCHIVED (October 2025). The main JS selfbot library is dead.
- Official `discord.js` is bot-only (requires bot token, not user token)

**Terminal clients** (reference):
- `cordless` (Go) — archived
- `discordo` (Go) — was a thing, status uncertain
- No mature Rust terminal Discord client found

### Discord API Structure
- **Gateway**: WebSocket connection for real-time events (wss://gateway.discord.gg)
- **REST API**: HTTPS endpoints for CRUD operations (https://discord.com/api/v10)
- **Voice**: Separate voice gateway (WebRTC + custom signaling)
- **Auth**: User token (obtained via login) or OAuth2

### TOS Implications
- Discord TOS Section 4: "you agree not to [...] use any unauthorized third-party software that accesses, intercepts, 'mines', or otherwise collects information from or through Discord"
- Using user tokens in unofficial clients is explicitly against TOS
- Risk: Account suspension/termination
- **MUST show clear warning to users before they add a Discord account**

### Possible Implementation Approaches (Decision Deferred)

1. **Direct API Client**: Reverse-engineer gateway/REST, handle anti-bot challenges. Cleanest UX but highest detection risk.
2. **Hidden Webview Bridge**: Run Discord web client in a hidden webview, intercept data via JS injection. Lower detection risk but heavier resource usage.
3. **Matrix Bridge**: Use `mautrix-discord` bridge — route Discord through Matrix SDK. Requires running a bridge server.
4. **Background Official Client**: Run official Discord client in background, communicate via IPC or scraping. Requires Discord installed.
5. **Minimal JS Runtime**: Embed a small JS engine (deno_core, boa) to execute Discord's client-side challenge code.

### Discord → Poly Mapping

| Discord Concept | Poly Type |
|---|---|
| Guild (Server) | `Server` |
| Category | `Category` |
| Text Channel | `Channel` (Text) |
| Voice Channel | `Channel` (Voice) |
| Stage Channel | `Channel` (Voice) |
| DM | `DmChannel` |
| Group DM (up to ~10 users) | `Group` |
| User | `User` |

### Self-Hosted Discord
- "Self-hosted Discord" shouldn't exist per Discord's terms
- But clone APIs exist (e.g., Fosscord/Spacebar)
- Support custom base URL for these instances
- Lower TOS risk for self-hosted clones

## Dependencies (TBD based on approach)

- `poly-client` — trait to implement
- Approach-dependent: `reqwest`, `tokio-tungstenite`, `webrtc`, or webview-based deps

## Module Structure (Preliminary)

```
src/
├── lib.rs              # DiscordClient struct + ClientBackend impl
├── auth.rs             # Authentication (approach-dependent)
├── gateway.rs          # Gateway WebSocket (if direct approach)
├── rest.rs             # REST API client (if direct approach)
├── types/              # Discord-specific types
├── voice.rs            # Voice gateway + WebRTC
└── warnings.rs         # TOS warning display logic
```

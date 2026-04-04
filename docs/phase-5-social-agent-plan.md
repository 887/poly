# Phase 5 Plan — Social Agent (MCP Chat Backend)

> **Created:** 2026-04-03
> **Status:** 🔴 Not Started
> **Goal:** Turn all connected messenger accounts into an MCP-accessible surface that an AI agent can read, respond to, learn from, and autonomously manage — with personality, memory, mood, scheduling, and approval flows.
> **Depends on:** Phase 3.x (all client backends functional), Phase 4 (test servers for safe dev/testing)

---

## Core Concept

Every logged-in account across all backends (Matrix, Stoat, Discord, Teams, Poly Server) becomes a set of MCP tools. An AI agent (Claude, or any MCP-compatible model) can:

1. **See** all incoming messages across all accounts in real-time
2. **Draft** responses with per-chat personality, mood, and memory context
3. **Send** responses after human approval (or auto-send if configured)
4. **Learn** from the user's actual responses to improve future drafts
5. **Schedule** proactive outreach ("message X about Y every N days")
6. **Simulate** realistic typing behavior (delays, typing indicators, variable speed)

The agent shares the same live connections as the Poly UI — no duplicate logins, no separate sessions. If the user responds via the UI, the agent sees it and learns from it.

```
┌─────────────────────────────────────────────────────────┐
│  Poly App (UI)                                          │
│  ├── Matrix accounts ──┐                                │
│  ├── Stoat accounts  ──┤                                │
│  ├── Discord accounts ─┼──► Shared ClientBackend Pool   │
│  ├── Teams accounts  ──┤    (single set of connections) │
│  └── Poly accounts   ──┘         │                      │
│                                  │                      │
│  Social Agent MCP Server ◄───────┘                      │
│  ├── /tools/messages/*     (read, search, history)      │
│  ├── /tools/send/*         (draft, approve, send)       │
│  ├── /tools/contacts/*     (list, profile, memory)      │
│  ├── /tools/schedule/*     (outreach plans, triggers)   │
│  ├── /tools/personality/*  (get/set per-chat persona)   │
│  ├── /tools/mood/*         (current mood, adjust)       │
│  └── /events/incoming      (message stream → AI)        │
│                                                         │
│  Modes:                                                 │
│  ├── Embedded: runs inside Poly app (same process)      │
│  ├── Standalone: runs as separate binary, connects to   │
│  │   shared backend pool via IPC/socket                 │
│  └── Headless: no UI at all, pure MCP + REST            │
└─────────────────────────────────────────────────────────┘
```

---

## 5.0 Architecture Decisions

- [ ] **5.0.1** Decide process model: embedded in Poly app vs. separate binary vs. both (feature-gated). The agent must share connections with the UI — if separate, needs IPC to the backend pool.
- [ ] **5.0.2** Decide MCP transport: stdio (for Claude Code / IDE), SSE (for browser), or both.
- [ ] **5.0.3** Decide AI provider abstraction: pluggable provider trait with multiple backends. User pastes their API key in settings, agent calls the API directly — no MCP middleman needed for inference.
- [ ] **5.0.4** Decide approval UX: in-app notification with approve/edit/reject? Desktop notification? Terminal prompt? All three?
- [ ] **5.0.5** Decide storage: per-chat memories, personality configs, mood state, outreach schedules — SQLite? TOML config + SQLite data? Reuse `crates/core` storage?
- [ ] **5.0.6** Decide event architecture: push (agent subscribes to message stream) vs. pull (agent polls). Push is better for real-time but needs the agent to be always-on.
- [ ] **5.0.7** Decide typing simulation strategy: per-backend typing indicator API + randomized delay before send. Some backends (Discord, Matrix) support "user is typing" events; others don't.

---

## 5.1 MCP Server — Tool Surface

> The MCP server exposes all chat functionality as tools. Any MCP client (Claude Code, Claude Desktop, custom agent) can call these.

### 5.1.1 Account & Connection Tools

- [ ] **5.1.1.1** `list_accounts` — return all logged-in accounts across all backends (id, backend, display_name, status)
- [ ] **5.1.1.2** `get_account_status(account_id)` — connection state, unread counts, active chats
- [ ] **5.1.1.3** `list_backends` — available backend types and their capabilities

### 5.1.2 Message Tools

- [ ] **5.1.2.1** `list_conversations(account_id?, backend?, unread_only?, limit?)` — all active DMs, channels, groups across accounts. Filterable.
- [ ] **5.1.2.2** `get_messages(conversation_id, limit?, before?, after?)` — paginated message history
- [ ] **5.1.2.3** `get_unread_messages(account_id?, limit?)` — all unread messages across all accounts, newest first
- [ ] **5.1.2.4** `search_messages(query, conversation_id?, account_id?, backend?)` — full-text search across backends
- [ ] **5.1.2.5** `get_conversation_context(conversation_id, depth?)` — recent messages + participant profiles + chat memory/summary — everything the AI needs to draft a response

### 5.1.3 Response Tools

- [ ] **5.1.3.1** `draft_response(conversation_id, message)` — queue a draft for human approval. Returns draft_id.
- [ ] **5.1.3.2** `send_message(conversation_id, message)` — send immediately (for auto-approved chats). Triggers typing simulation first.
- [ ] **5.1.3.3** `approve_draft(draft_id)` — human approves a queued draft → triggers typing sim → sends
- [ ] **5.1.3.4** `edit_draft(draft_id, new_message)` — human edits before approving
- [ ] **5.1.3.5** `reject_draft(draft_id, reason?)` — human rejects; reason feeds back into learning
- [ ] **5.1.3.6** `list_pending_drafts` — all drafts awaiting approval
- [ ] **5.1.3.7** `send_reaction(conversation_id, message_id, emoji)` — react to a message

### 5.1.4 Contact & Relationship Tools

- [ ] **5.1.4.1** `list_contacts(account_id?, backend?)` — all known contacts/friends across backends
- [ ] **5.1.4.2** `get_contact_profile(user_id, backend)` — profile, presence, shared servers/channels
- [ ] **5.1.4.3** `get_contact_memory(user_id)` — stored memories, conversation summaries, notes about this person
- [ ] **5.1.4.4** `set_contact_memory(user_id, memory)` — add/update memory for a contact
- [ ] **5.1.4.5** `get_relationship_summary(user_id)` — last interaction, frequency, topics discussed, sentiment trend

### 5.1.5 Personality & Mood Tools

- [ ] **5.1.5.1** `get_base_personality` — the default personality/tone config
- [ ] **5.1.5.2** `set_base_personality(config)` — update base personality
- [ ] **5.1.5.3** `get_chat_personality(conversation_id)` — per-chat overrides (more casual with friends, more formal with work)
- [ ] **5.1.5.4** `set_chat_personality(conversation_id, config)` — set per-chat personality override
- [ ] **5.1.5.5** `list_personality_presets` — named presets ("casual", "professional", "witty", "supportive", etc.)
- [ ] **5.1.5.6** `get_mood` — current mood setting (affects tone across all chats)
- [ ] **5.1.5.7** `set_mood(mood)` — change mood ("energetic", "chill", "busy", "thoughtful")

### 5.1.6 Schedule & Outreach Tools

- [ ] **5.1.6.1** `create_outreach_plan(user_id, frequency, topics, source?)` — e.g. "message @alice 1-2x/week, talk about latest HN posts or bring up something from 3mo memory"
- [ ] **5.1.6.2** `list_outreach_plans` — all active scheduled outreach
- [ ] **5.1.6.3** `update_outreach_plan(plan_id, ...)` — modify frequency, topics, pause/resume
- [ ] **5.1.6.4** `delete_outreach_plan(plan_id)` — remove scheduled outreach
- [ ] **5.1.6.5** `trigger_outreach(plan_id)` — manually trigger a scheduled outreach now
- [ ] **5.1.6.6** `get_outreach_history(plan_id)` — when did outreach happen, what was sent, how did they respond

---

## 5.2 Event System

> The agent needs to know when messages arrive so it can react. Two modes: push (real-time subscription) and pull (poll for new events).

- [ ] **5.2.1** Define `AgentEvent` enum: `MessageReceived`, `MessageEdited`, `MessageDeleted`, `PresenceChanged`, `TypingStarted`, `ReactionAdded`, `DraftApproved`, `DraftRejected`, `OutreachDue`, `UserResponded` (user sent a message via UI — learning signal)
- [ ] **5.2.2** Event bus: fan-out from `ClientBackend::event_stream()` to both the UI and the agent
- [ ] **5.2.3** MCP resource subscription: `events://incoming` — SSE stream of `AgentEvent` for MCP clients that support resources
- [ ] **5.2.4** Polling fallback: `get_new_events(since_id?)` tool for MCP clients that don't support streaming
- [ ] **5.2.5** Event filtering: per-account, per-conversation, per-event-type filters so the agent isn't overwhelmed
- [ ] **5.2.6** `UserResponded` event: when the human sends a message via the Poly UI (not via the agent), emit this event with the full context so the agent can learn from it

---

## 5.3 Memory & Learning System

> The agent builds up knowledge about each contact and conversation over time. Memories persist across sessions.

### 5.3.1 Conversation Memory

- [ ] **5.3.1.1** Auto-summarize conversations: after N messages or time threshold, generate a summary and store it
- [ ] **5.3.1.2** Rolling summary: keep last 3 summaries per conversation + current unsummarized tail
- [ ] **5.3.1.3** Topic extraction: automatically tag conversations with topics discussed
- [ ] **5.3.1.4** Sentiment tracking: per-conversation sentiment trend (are things going well? tense? fading?)

### 5.3.2 Contact Memory

- [ ] **5.3.2.1** Per-contact fact store: things learned about this person (interests, job, timezone, preferences, birthdays, etc.)
- [ ] **5.3.2.2** Relationship context: how do I know this person? work colleague, friend, family, acquaintance
- [ ] **5.3.2.3** Communication preferences: do they prefer short messages? long? emoji-heavy? formal?
- [ ] **5.3.2.4** Last interaction tracking: when did we last talk? what about? is a follow-up due?

### 5.3.3 Response Learning

- [ ] **5.3.3.1** When user sends a message via UI, store (context → response) as a training example
- [ ] **5.3.3.2** When user approves a draft without edits, mark as positive example
- [ ] **5.3.3.3** When user edits a draft before approving, store both original and edited as correction pair
- [ ] **5.3.3.4** When user rejects a draft, store as negative example with optional reason
- [ ] **5.3.3.5** Periodic style analysis: extract patterns from approved/manual responses (length, tone, emoji usage, response time)
- [ ] **5.3.3.6** Feed learned style into personality config as "learned preferences" that augment the base personality

### 5.3.4 Storage

- [ ] **5.3.4.1** SQLite database for memories, summaries, learning examples, outreach history
- [ ] **5.3.4.2** TOML config for personality presets, mood defaults, outreach templates
- [ ] **5.3.4.3** Export/import: backup all memories and configs as a portable archive

---

## 5.4 Personality Engine

> Controls how the AI responds. Layered system: base personality → mood modifier → per-chat override.

### Config Structure (TOML)

```toml
[personality.base]
name = "default"
tone = "casual-friendly"
humor = "dry-wit"          # dry-wit, punny, deadpan, wholesome, none
verbosity = "concise"      # concise, moderate, detailed
emoji_usage = "occasional" # none, occasional, frequent, chaotic
formality = "low"          # low, medium, high
response_speed = "natural" # instant, fast, natural, slow, very-slow
system_prompt = """
You are responding as me in casual conversation. Keep it natural,
don't be overly enthusiastic. Match the energy of the conversation.
"""

[personality.mood]
current = "chill"          # energetic, chill, busy, thoughtful, tired
influence = 0.3            # how much mood affects base personality (0.0-1.0)

[personality.presets.professional]
tone = "polite-professional"
humor = "none"
formality = "high"
emoji_usage = "none"
system_prompt = "You are responding as me in a professional context..."

[personality.presets.close-friend]
tone = "very-casual"
humor = "chaotic"
verbosity = "concise"
emoji_usage = "frequent"
system_prompt = "You are responding as me to a close friend. Be real..."

# Per-chat overrides
[personality.overrides."dm:@alice:matrix.org"]
preset = "close-friend"
custom_notes = "Alice loves puns. Reference our running joke about sourdough."

[personality.overrides."dm:bob#1234"]
preset = "professional"
custom_notes = "Bob is my manager. Keep it brief and action-oriented."
```

- [ ] **5.4.1** Implement personality config parser (TOML → typed config)
- [ ] **5.4.2** Implement personality resolver: merge base → mood → preset → per-chat override
- [ ] **5.4.3** Implement system prompt builder: combine personality config + contact memory + conversation summary into a single system prompt for the AI
- [ ] **5.4.4** Personality preset CRUD via MCP tools
- [ ] **5.4.5** Per-chat personality assignment via MCP tools
- [ ] **5.4.6** Mood state management (get/set, optional time-based auto-reset)

---

## 5.5 Typing Simulation

> Make AI responses look human. Don't just instantly send — simulate the "is typing..." experience.

- [ ] **5.5.1** Typing indicator API per backend: Matrix (`m.typing`), Discord (POST `/channels/{id}/typing`), Stoat (WebSocket typing event), Teams (Graph typing indicator)
- [ ] **5.5.2** Delay calculation: based on message length, personality `response_speed`, and randomization. Short message (< 20 chars) = 2-5s delay. Medium = 5-15s. Long = 15-45s. Add gaussian noise.
- [ ] **5.5.3** Typing indicator pulsing: send typing indicator every 5s during the delay period (backends expire them after ~10s)
- [ ] **5.5.4** False starts: occasionally (configurable probability) start typing, stop, wait 2-5s, start again. Adds realism.
- [ ] **5.5.5** Read delay: don't start typing immediately after receiving a message. Wait 1-10s (configurable) to simulate reading time.
- [ ] **5.5.6** Multi-message splitting: for long responses, optionally split into 2-3 shorter messages with typing gaps between them
- [ ] **5.5.7** Online hours: don't respond outside configured hours (or delay until morning). Configurable per-chat.

---

## 5.6 Outreach Scheduler

> Proactive messaging: "message this person about X every N days."

- [ ] **5.6.1** Outreach plan model: `{ contact, frequency_range (min_days, max_days), topics: [source], last_triggered, enabled }`
- [ ] **5.6.2** Topic sources:
  - `"memory"` — bring up something from conversation memory (last 3 months)
  - `"hackernews"` — fetch top HN stories, pick one relevant to the contact's interests
  - `"rss:url"` — fetch from RSS feed
  - `"random"` — pick from a configured list of conversation starters
  - `"custom:prompt"` — custom AI prompt to generate a topic
- [ ] **5.6.3** Scheduler loop: check outreach plans every hour, trigger any that are due (randomized within the frequency range)
- [ ] **5.6.4** When triggered: generate a message using personality + contact memory + topic source → queue as draft (or auto-send if configured)
- [ ] **5.6.5** Cooldown: don't trigger outreach if there was a recent conversation (< N days) with this contact
- [ ] **5.6.6** Outreach history: log every triggered outreach with timestamp, topic, message sent, response received

---

## 5.7 Auto-Response Pipeline

> The full flow from incoming message to AI response.

```
Message arrives (any backend)
    │
    ▼
Event bus → AgentEvent::MessageReceived
    │
    ▼
Filter: is this chat configured for agent response?
    │ no → ignore (or just store for memory)
    │ yes ↓
    ▼
Build context:
    ├── Recent messages (last N or since last summary)
    ├── Contact memory (facts, preferences, relationship)
    ├── Conversation summary (rolling)
    ├── Personality config (resolved: base → mood → chat override)
    └── Learned style examples (from user's past responses)
    │
    ▼
Generate response via AI provider
    │
    ▼
Typing simulation (read delay → typing indicator → send delay)
    │
    ▼
Approval mode?
    ├── auto-approve → send immediately (after typing sim)
    ├── suggest → show draft in UI / notification → wait for approve/edit/reject
    └── notify-only → just notify user of the message, no draft
    │
    ▼
Send via ClientBackend::send_message()
    │
    ▼
Store in memory (context → response → outcome)
```

- [ ] **5.7.1** Implement response pipeline orchestrator
- [ ] **5.7.2** Implement per-chat response mode config: `auto`, `suggest`, `notify`, `ignore`
- [ ] **5.7.3** Implement context builder (assembles all context for AI prompt)
- [ ] **5.7.4** Implement AI provider adapter (see §5.12)
- [ ] **5.7.5** Implement draft queue with approval UI hooks
- [ ] **5.7.6** Implement feedback loop: approved/edited/rejected drafts feed back into learning
- [ ] **5.7.7** Rate limiting: max responses per hour per chat (prevent runaway conversations)
- [ ] **5.7.8** Conversation threading: if multiple messages arrive before response, batch them into one context
- [ ] **5.7.9** Ignore list: never auto-respond to certain contacts, channels, or message patterns (e.g. bot messages)

---

## 5.8 Deployment Modes

### 5.8.1 Embedded (in Poly app)

- [ ] **5.8.1.1** Feature-gated module in the Poly app binary (`--features social-agent`)
- [ ] **5.8.1.2** MCP server starts on a configurable port alongside the UI
- [ ] **5.8.1.3** Shares the same `ClientBackend` pool — zero connection duplication
- [ ] **5.8.1.4** Agent drafts appear in the Poly UI as pending notifications
- [ ] **5.8.1.5** REST API on same port for non-MCP integrations (webhooks, dashboards)

### 5.8.2 Standalone (headless)

- [ ] **5.8.2.1** Separate binary: `poly-agent` — starts MCP server + connects all configured backends
- [ ] **5.8.2.2** Config file: accounts, personalities, schedules, AI provider settings
- [ ] **5.8.2.3** No UI — all interaction via MCP tools or REST API
- [ ] **5.8.2.4** Can run on a server/VPS as always-on agent
- [ ] **5.8.2.5** Systemd service file / Docker container for deployment

### 5.8.3 MCP Configuration

- [ ] **5.8.3.1** Generate `claude_desktop_config.json` snippet for Claude Desktop integration
- [ ] **5.8.3.2** Generate `mcp.json` for Claude Code integration
- [ ] **5.8.3.3** SSE transport for browser-based MCP clients
- [ ] **5.8.3.4** stdio transport for CLI-based MCP clients

---

## 5.9 Safety & Privacy

- [ ] **5.9.1** All auto-responses are logged and reviewable
- [ ] **5.9.2** Kill switch: instantly disable all auto-responses across all chats
- [ ] **5.9.3** Per-chat consent: never auto-respond in a chat unless explicitly configured
- [ ] **5.9.4** Message content never leaves the local machine except to the configured AI provider
- [ ] **5.9.5** Sensitive message detection: flag messages that might need careful human response (emotional, financial, legal)
- [ ] **5.9.6** Disclosure option: configurable footer or indicator that a response was AI-assisted (for transparency with contacts)
- [ ] **5.9.7** Conversation memory encryption at rest

---

## 5.10 Digest & Briefing System

> "What happened while I was away?" — the agent produces summaries of everything you need to know, prioritized by importance.

### 5.10.1 Briefing Tools

- [ ] **5.10.1.1** `get_briefing(since?, priority?)` — generate a structured summary of all activity since last check-in. Sections: urgent (mentions, DMs from key contacts), important (active conversations, pending drafts), informational (group chats, low-priority channels), outreach due.
- [ ] **5.10.1.2** `get_conversation_digest(conversation_id, since?)` — deep summary of a specific conversation: what was discussed, decisions made, action items, links/files shared, tone/sentiment.
- [ ] **5.10.1.3** `get_contact_digest(user_id, period?)` — everything a specific person said across all backends/channels in a time period, summarized with topics and sentiment.
- [ ] **5.10.1.4** `get_channel_digest(channel_id, since?)` — "catch up on this channel" — key messages, threads worth reading, decisions, links shared. Flags which messages actually need your attention vs. skippable.
- [ ] **5.10.1.5** `get_action_items(since?)` — extract things people asked you to do, questions awaiting your response, deadlines mentioned across all chats.
- [ ] **5.10.1.6** `get_reading_list(since?)` — all links, articles, files, and media shared across all chats, grouped by source and tagged by topic. "Alice shared 3 articles about Rust async. Bob sent a design doc. #general had a thread about the new API."
- [ ] **5.10.1.7** `catch_me_up(conversation_id?, scope?)` — the "where is chat at" shortcut. For a single conversation: "they're debating X, Alice proposed Y, Bob disagrees, no decision yet, 3 messages need your input." For a scope like "all" or "unread": ranked summary across everything, skipping noise. Returns what matters, not 90 messages.

### 5.10.2 Scheduled Briefings

- [ ] **5.10.2.1** Morning briefing: auto-generate at configured time, surface via MCP resource or push notification
- [ ] **5.10.2.2** End-of-day wrap-up: summarize what happened today, outstanding items, upcoming outreach
- [ ] **5.10.2.3** Weekly digest: longer-term trends, relationship health, contacts going cold, conversations that need follow-up
- [ ] **5.10.2.4** Configurable delivery: MCP resource, email, in-app notification, markdown file, or webhook

### 5.10.3 Priority Intelligence

- [ ] **5.10.3.1** Contact priority tiers: VIP (always surface immediately), normal, low (batch into digest)
- [ ] **5.10.3.2** Message importance scoring: mentions, questions, time-sensitive keywords, emotional content, from VIP contacts
- [ ] **5.10.3.3** "You should read this" flags: messages the agent thinks you'd want to see even in channels you normally skim
- [ ] **5.10.3.4** "Safe to skip" markers: messages in active channels that are noise for you (bot messages, reactions-only, off-topic tangents)

---

## 5.11 AI Provider System

> The agent needs an LLM to generate responses, summaries, and briefings. User provides their own API key in the Poly settings UI — the agent calls the provider directly.

### Provider Trait

```rust
#[async_trait]
trait AiProvider: Send + Sync {
    async fn complete(&self, messages: Vec<ChatMessage>, config: &GenerationConfig) -> Result<String>;
    fn name(&self) -> &str;
    fn supports_streaming(&self) -> bool;
}
```

### Supported Providers

- [ ] **5.11.1** Claude (Anthropic API) — `api_key` in settings, models: claude-sonnet-4-6, claude-opus-4-6, claude-haiku-4-5
- [ ] **5.11.2** OpenAI — `api_key` in settings, models: gpt-4o, gpt-4o-mini, o3, o4-mini
- [ ] **5.11.3** Google Gemini — `api_key` in settings, models: gemini-2.5-pro, gemini-2.5-flash
- [ ] **5.11.4** OpenAI-compatible / local (Ollama, LM Studio, vLLM) — custom `base_url` + optional `api_key`, any model string
- [ ] **5.11.5** Provider selection per use case: expensive model for important DMs, cheap/fast model for channel digests, local model for privacy-sensitive chats

### Settings UI

- [ ] **5.11.6** "AI Provider" section in Poly settings: select provider, paste API key, pick default model, test connection
- [ ] **5.11.7** Per-chat model override: use opus for your boss, haiku for meme channels
- [ ] **5.11.8** Usage tracking: show token count / estimated cost per day/week so users don't get bill shock
- [ ] **5.11.9** Fallback chain: if primary provider fails (rate limit, outage), try secondary provider

### Config (TOML)

```toml
[ai.provider]
default = "claude"

[ai.providers.claude]
api_key = "sk-ant-..."        # or read from env: ANTHROPIC_API_KEY
default_model = "claude-sonnet-4-6"
max_tokens = 1024

[ai.providers.openai]
api_key = "sk-..."
default_model = "gpt-4o-mini"

[ai.providers.local]
base_url = "http://localhost:11434/v1"
default_model = "llama3"
api_key = ""                   # optional for local

[ai.routing]
dm_vip = "claude"              # important DMs → best model
dm_normal = "claude"
channel_digest = "openai"      # summaries → cheaper model
outreach_draft = "claude"
briefing = "openai"
```

---

## 5.12 Live Translation

> Translate incoming and outgoing messages on the fly using the configured AI provider. Works across all backends.

- [ ] **5.12.1** Per-chat translation toggle: enable/disable translation per conversation
- [ ] **5.12.2** Target language setting: global default + per-chat override (e.g. "translate #deutsch to English, translate DM with Pierre to English")
- [ ] **5.12.3** Incoming message translation: show original + translated text inline (expandable)
- [ ] **5.12.4** Outgoing message translation: optionally translate your message before sending (e.g. type in English, send in German)
- [ ] **5.12.5** Language detection: auto-detect source language, skip translation if already in target language
- [ ] **5.12.6** Bulk translation: "translate last 50 messages" for catching up on a foreign-language channel
- [ ] **5.12.7** Translation caching: don't re-translate the same message twice (store translated text alongside original)
- [ ] **5.12.8** Cost-aware: use cheap/fast model for translation (haiku/flash/mini), not the expensive response model

---

## 5.13 Image Generation

> Generate images from text prompts directly in chat using AI provider APIs.

- [ ] **5.13.1** Image generation provider config: OpenAI DALL-E, Anthropic (if available), Stability AI, local (Stable Diffusion API)
- [ ] **5.13.2** Chat command: `/imagine <prompt>` or button in chat input to generate an image
- [ ] **5.13.3** Preview before send: show generated image, option to regenerate, edit prompt, or send
- [ ] **5.13.4** Image variations: generate multiple options, pick the best one
- [ ] **5.13.5** Inline display: generated images render as normal chat attachments
- [ ] **5.13.6** Generation history: track prompts and generated images for reuse
- [ ] **5.13.7** Provider API key config in Settings → AI Provider section (alongside chat AI keys)
- [ ] **5.13.8** Cost tracking: show estimated cost per generation

---

## 5.14 Agent UI

> The agent needs dedicated UI surfaces beyond the settings page. These live inside the existing Poly app — not a separate window.

### 5.14.1 Agent Sidebar Panel

A collapsible panel (slide-over or docked) accessible from the main sidebar, like a "command center" for the agent.

- [ ] **5.14.1.1** Agent panel toggle button in the sidebar (brain/robot icon)
- [ ] **5.14.1.2** Pending drafts section — list of responses waiting for approval, newest first. Each shows: contact, preview, timestamp, approve/edit/reject buttons.
- [ ] **5.14.1.3** Incoming alerts — messages the agent flagged as important but isn't auto-responding to. "Alice asked you a question 10 min ago."
- [ ] **5.14.1.4** Outreach due — contacts scheduled for outreach today. "Message Bob about HN — due in 2h." Quick trigger or snooze.
- [ ] **5.14.1.5** Agent status indicator — active/paused/disabled, current mood, connected providers
- [ ] **5.14.1.6** Quick kill switch — one-click disable all auto-responses

### 5.14.2 Per-Chat Agent Controls

Accessible from the chat header (gear icon or agent avatar) when viewing any conversation.

- [ ] **5.14.2.1** Response mode toggle: `auto` / `suggest` / `notify` / `off` — per-chat
- [ ] **5.14.2.2** Personality picker — dropdown of presets + "custom" option. Shows current personality name.
- [ ] **5.14.2.3** Translation toggle + target language selector — enable/disable per-chat, pick language
- [ ] **5.14.2.4** "Catch me up" button — one-click summary of this conversation since last visit
- [ ] **5.14.2.5** Memory viewer — expandable panel showing what the agent remembers about this contact/conversation. Editable.
- [ ] **5.14.2.6** Conversation summary — rolling summary, last updated timestamp, topic tags
- [ ] **5.14.2.7** Priority tier — set contact as VIP / normal / low priority
- [ ] **5.14.2.8** Outreach config — "message this person every N days about X" inline setup

### 5.14.3 Briefing View

A dedicated page (route: `/agent/briefing`) for the daily/weekly digest.

- [ ] **5.14.3.1** Morning briefing page — structured summary: urgent, important, informational sections
- [ ] **5.14.3.2** Action items list — things people asked you to do, questions awaiting response
- [ ] **5.14.3.3** Reading list — links/articles/files shared across all chats, grouped by source
- [ ] **5.14.3.4** Relationship health — contacts going cold, follow-ups due, sentiment trends
- [ ] **5.14.3.5** "Catch me up on everything" button — full cross-platform summary since last check-in
- [ ] **5.14.3.6** Weekly digest view — longer-term trends, conversation highlights, outreach history
- [ ] **5.14.3.7** Configurable delivery time — set when the briefing generates (e.g. 8am)

### 5.14.4 Memory Browser

A dedicated page (route: `/agent/memory`) for searching and managing agent memory.

- [ ] **5.14.4.1** Contact list with memory summary — all known contacts, facts remembered, last interaction
- [ ] **5.14.4.2** Contact detail view — all facts, conversation summaries, topics, sentiment, relationship context
- [ ] **5.14.4.3** Edit/delete memories — correct wrong facts, remove stale info
- [ ] **5.14.4.4** Add manual memory — "remember that Alice is moving to Berlin in June"
- [ ] **5.14.4.5** Search across all memories — "who mentioned Rust async?" finds it across all contacts
- [ ] **5.14.4.6** Conversation timeline — per-contact view of all summarized conversations over time
- [ ] **5.14.4.7** Export memories — backup all agent memory as JSON/markdown

### 5.14.5 Draft Queue

Integrated into the agent panel and also available as its own view.

- [ ] **5.14.5.1** Draft list — all pending drafts across all chats, sortable by time/priority
- [ ] **5.14.5.2** Draft preview — show the full draft with conversation context (what they said → what the agent wants to reply)
- [ ] **5.14.5.3** Inline edit — modify the draft text before approving
- [ ] **5.14.5.4** Approve with typing sim — approve sends after realistic typing delay
- [ ] **5.14.5.5** Batch approve — approve multiple low-risk drafts at once
- [ ] **5.14.5.6** Reject with feedback — "too formal" / "wrong tone" / custom reason → feeds learning
- [ ] **5.14.5.7** Draft history — past approved/rejected drafts with outcomes (did they reply? how?)

### 5.14.6 Outreach Planner

Integrated into briefing view and also standalone (route: `/agent/outreach`).

- [ ] **5.14.6.1** Visual schedule — calendar/timeline view of upcoming outreach triggers
- [ ] **5.14.6.2** Plan list — all active outreach plans with contact, frequency, topics, last triggered
- [ ] **5.14.6.3** Create plan wizard — pick contact → set frequency range → pick topic sources → preview sample message
- [ ] **5.14.6.4** Edit/pause/delete plans
- [ ] **5.14.6.5** Outreach history — log of past triggered outreach with message sent and response received
- [ ] **5.14.6.6** "Trigger now" button — manually fire an outreach plan ahead of schedule
- [ ] **5.14.6.7** Cooldown indicator — shows if a plan is in cooldown because of recent conversation

### 5.14.7 Mobile Considerations

- [ ] **5.14.7.1** Agent panel as bottom sheet (slide up) on mobile instead of sidebar panel
- [ ] **5.14.7.2** Per-chat controls in a swipe-up drawer from chat header
- [ ] **5.14.7.3** Push notifications for pending drafts — "Agent wants to reply to Alice. Approve?"
- [ ] **5.14.7.4** Briefing as a notification card — morning summary appears as a rich notification
- [ ] **5.14.7.5** Approve/reject from notification — quick actions without opening the app
- [ ] **5.14.7.6** Compact memory browser — searchable list optimized for small screens
- [ ] **5.14.7.7** Background agent service — keeps running when app is backgrounded (Android service / iOS background task)

---

## 5.15 External Integrations

- [ ] **5.15.1** Hacker News feed: fetch top/best stories for outreach topic generation
- [ ] **5.15.2** RSS/Atom feeds: configurable feed list for topic sources
- [ ] **5.15.3** Calendar integration: know when user is busy, adjust response times and mood
- [ ] **5.15.4** Webhook endpoint: trigger actions from external systems (e.g. "send this message to Alice")
- [ ] **5.15.5** Analytics dashboard: response times, auto-response accuracy, conversation health metrics

---

## Completion Criteria

- [ ] MCP server exposes all chat backends as tools (accounts, messages, contacts, send)
- [ ] AI can draft responses with per-chat personality and mood
- [ ] Human approval flow works (suggest → approve/edit/reject → send)
- [ ] Auto-response mode works for configured chats with typing simulation
- [ ] Memory system stores and retrieves per-contact facts and conversation summaries
- [ ] Learning system improves drafts based on user's actual responses and corrections
- [ ] Outreach scheduler triggers proactive messages on configured intervals
- [ ] Typing simulation is realistic (delays, indicators, false starts, read time)
- [ ] Runs embedded in Poly app or as standalone headless binary
- [ ] MCP config works with Claude Desktop and Claude Code
- [ ] Kill switch instantly disables all auto-responses
- [ ] No message content stored unencrypted

---

## Crate Structure (Proposed)

```
crates/
├── social-agent/           # Core agent logic (personality, memory, pipeline, scheduler)
│   ├── src/
│   │   ├── lib.rs
│   │   ├── config.rs       # TOML personality/mood/schedule config
│   │   ├── memory.rs       # Contact memory, conversation summaries, fact store
│   │   ├── personality.rs  # Personality resolver (base → mood → override)
│   │   ├── pipeline.rs     # Message → context → AI → draft → approve → send
│   │   ├── scheduler.rs    # Outreach plan scheduler
│   │   ├── typing.rs       # Typing simulation (delays, indicators, false starts)
│   │   ├── learning.rs     # Response learning from user behavior
│   │   └── provider.rs     # AI provider adapter (Claude API, generic)
│   └── Cargo.toml
│
mcp/
├── social-agent-mcp/       # MCP server binary (tool definitions, transport)
│   ├── src/
│   │   ├── main.rs
│   │   ├── tools.rs        # MCP tool implementations
│   │   └── events.rs       # Event subscription / SSE
│   └── Cargo.toml
│
apps/
├── agent/                  # Standalone headless binary
│   ├── src/main.rs
│   ├── config.toml         # Default config template
│   └── Cargo.toml
```

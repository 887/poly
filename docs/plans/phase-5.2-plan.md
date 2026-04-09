# Phase 5.2 Plan — Social Agent Intelligence + UI

> **Created:** 2026-04-04
> **Status:** Not Started
> **Goal:** Build the AI-powered social agent: personality engine, memory/learning system, auto-response pipeline, typing simulation, outreach scheduler, agent UI panels, and AI provider integration.
> **Depends on:** Phase 5.1 (MCP server + shared backend pool)

---

## Core Concept

Phase 5.2 is the **intelligence layer** on top of the MCP backend infrastructure built in Phase 5.1. It turns the raw backend access into an autonomous social agent that can:

1. **Draft** responses with per-chat personality, mood, and memory context
2. **Learn** from the user's actual responses to improve future drafts
3. **Schedule** proactive outreach to maintain relationships
4. **Simulate** realistic typing behavior (delays, indicators, false starts)
5. **Brief** the user on what happened while they were away
6. **Translate** messages across languages using AI providers

The agent UI lives inside the existing Poly app as new panels and controls.

---

## 5.2.1 Personality Engine

Layered system: base personality -> mood modifier -> per-chat override.

### Config Structure (TOML)

```toml
[personality.base]
name = "default"
tone = "casual-friendly"
humor = "dry-wit"
verbosity = "concise"
emoji_usage = "occasional"
formality = "low"
response_speed = "natural"
system_prompt = "You are responding as me in casual conversation..."

[personality.mood]
current = "chill"
influence = 0.3

[personality.presets.professional]
tone = "polite-professional"
humor = "none"
formality = "high"

[personality.overrides."dm:@alice:matrix.org"]
preset = "close-friend"
custom_notes = "Alice loves puns."
```

### Checklist

- [ ] **5.2.1.1** Implement personality config parser (TOML -> typed config)
- [ ] **5.2.1.2** Implement personality resolver: merge base -> mood -> preset -> per-chat override
- [ ] **5.2.1.3** Implement system prompt builder: personality config + contact memory + conversation summary -> system prompt
- [ ] **5.2.1.4** Personality preset CRUD via MCP tools
- [ ] **5.2.1.5** Per-chat personality assignment via MCP tools
- [ ] **5.2.1.6** Mood state management (get/set, optional auto-reset)

---

## 5.2.2 Memory & Learning System

### Conversation Memory

- [ ] **5.2.2.1** Auto-summarize conversations after N messages or time threshold
- [ ] **5.2.2.2** Rolling summary: keep last 3 summaries + current unsummarized tail
- [ ] **5.2.2.3** Topic extraction: auto-tag conversations with topics
- [ ] **5.2.2.4** Sentiment tracking: per-conversation sentiment trend

### Contact Memory

- [ ] **5.2.2.5** Per-contact fact store: interests, job, timezone, preferences, birthdays
- [ ] **5.2.2.6** Relationship context: how you know this person
- [ ] **5.2.2.7** Communication preferences: short vs long, formal vs casual, emoji usage
- [ ] **5.2.2.8** Last interaction tracking and follow-up detection

### Response Learning

- [ ] **5.2.2.9** Store (context -> response) pairs when user sends messages via UI
- [ ] **5.2.2.10** Positive examples: drafts approved without edits
- [ ] **5.2.2.11** Correction pairs: draft vs edited version
- [ ] **5.2.2.12** Negative examples: rejected drafts with reason
- [ ] **5.2.2.13** Periodic style analysis: extract patterns from user's responses
- [ ] **5.2.2.14** Feed learned preferences into personality config

### Storage

- [ ] **5.2.2.15** SQLite database for memories, summaries, learning examples, outreach history
- [ ] **5.2.2.16** TOML config for personality presets, mood defaults, outreach templates
- [ ] **5.2.2.17** Export/import: backup all memories as portable archive

---

## 5.2.3 Auto-Response Pipeline

```
Message arrives -> Event bus -> Filter (is chat configured?) 
-> Build context (messages + memory + personality + learned style)
-> Generate response via AI provider -> Typing simulation
-> Approval mode? (auto/suggest/notify) -> Send via ClientBackend
-> Store in memory (context -> response -> outcome)
```

- [ ] **5.2.3.1** Implement response pipeline orchestrator
- [ ] **5.2.3.2** Per-chat response mode config: auto, suggest, notify, ignore
- [ ] **5.2.3.3** Context builder: assembles all context for AI prompt
- [ ] **5.2.3.4** AI provider adapter (see 5.2.7)
- [ ] **5.2.3.5** Draft queue with approval UI hooks
- [ ] **5.2.3.6** Feedback loop: approved/edited/rejected drafts feed into learning
- [ ] **5.2.3.7** Rate limiting: max responses per hour per chat
- [ ] **5.2.3.8** Conversation threading: batch multiple incoming messages
- [ ] **5.2.3.9** Ignore list: never auto-respond to certain contacts/channels/patterns

---

## 5.2.4 Typing Simulation

- [ ] **5.2.4.1** Typing indicator API per backend (Matrix m.typing, Discord POST, Stoat WS, Teams Graph)
- [ ] **5.2.4.2** Delay calculation: message length x personality response_speed + noise
- [ ] **5.2.4.3** Typing indicator pulsing: send every 5s during delay
- [ ] **5.2.4.4** False starts: occasionally start/stop/restart typing
- [ ] **5.2.4.5** Read delay: wait 1-10s before starting to "type"
- [ ] **5.2.4.6** Multi-message splitting: break long responses into 2-3 messages with gaps
- [ ] **5.2.4.7** Online hours: don't respond outside configured hours

---

## 5.2.5 Outreach Scheduler

- [ ] **5.2.5.1** Outreach plan model: contact, frequency range, topics, last triggered, enabled
- [ ] **5.2.5.2** Topic sources: memory, HN, RSS, random, custom prompt
- [ ] **5.2.5.3** Scheduler loop: check plans hourly, trigger when due
- [ ] **5.2.5.4** Generate outreach message using personality + memory + topic -> draft
- [ ] **5.2.5.5** Cooldown: skip outreach if recent conversation exists
- [ ] **5.2.5.6** Outreach history logging

---

## 5.2.6 Digest & Briefing System

### Briefing Tools (MCP)

- [ ] **5.2.6.1** `get_briefing(since?, priority?)` — structured summary of all activity
- [ ] **5.2.6.2** `get_conversation_digest(conversation_id, since?)` — deep conversation summary
- [ ] **5.2.6.3** `get_contact_digest(user_id, period?)` — everything a person said, summarized
- [ ] **5.2.6.4** `catch_me_up(conversation_id?, scope?)` — "where is chat at" shortcut
- [ ] **5.2.6.5** `get_action_items(since?)` — things people asked you to do
- [ ] **5.2.6.6** `get_reading_list(since?)` — links/articles/files shared

### Scheduled Briefings

- [ ] **5.2.6.7** Morning briefing: auto-generate at configured time
- [ ] **5.2.6.8** End-of-day wrap-up
- [ ] **5.2.6.9** Weekly digest: trends, relationship health, follow-ups
- [ ] **5.2.6.10** Configurable delivery: MCP resource, in-app notification, markdown, webhook

### Priority Intelligence

- [ ] **5.2.6.11** Contact priority tiers: VIP, normal, low
- [ ] **5.2.6.12** Message importance scoring
- [ ] **5.2.6.13** "You should read this" flags
- [ ] **5.2.6.14** "Safe to skip" markers

---

## 5.2.7 AI Provider System

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

- [ ] **5.2.7.1** Claude (Anthropic API) — sonnet, opus, haiku
- [ ] **5.2.7.2** OpenAI — gpt-4o, gpt-4o-mini, o3, o4-mini
- [ ] **5.2.7.3** Google Gemini — gemini-2.5-pro, gemini-2.5-flash
- [ ] **5.2.7.4** OpenAI-compatible / local (Ollama, LM Studio, vLLM)
- [ ] **5.2.7.5** Per-use-case provider routing: expensive model for VIP DMs, cheap for digests

### Settings UI

- [ ] **5.2.7.6** AI Provider section in Settings: select provider, API key, test connection
- [ ] **5.2.7.7** Per-chat model override
- [ ] **5.2.7.8** Usage tracking: token count / estimated cost
- [ ] **5.2.7.9** Fallback chain: try secondary provider on failure

---

## 5.2.8 Agent UI

### Agent Sidebar Panel

- [ ] **5.2.8.1** Agent panel toggle button in sidebar (brain/robot icon)
- [ ] **5.2.8.2** Pending drafts section with approve/edit/reject
- [ ] **5.2.8.3** Incoming alerts — flagged important messages
- [ ] **5.2.8.4** Outreach due — contacts scheduled for today
- [ ] **5.2.8.5** Agent status indicator — active/paused, mood, providers
- [ ] **5.2.8.6** Quick kill switch — disable all auto-responses

### Per-Chat Agent Controls

- [ ] **5.2.8.7** Response mode toggle: auto/suggest/notify/off
- [ ] **5.2.8.8** Personality picker dropdown
- [ ] **5.2.8.9** Translation toggle + language selector
- [ ] **5.2.8.10** "Catch me up" button
- [ ] **5.2.8.11** Memory viewer/editor
- [ ] **5.2.8.12** Priority tier selector

### Briefing View (`/agent/briefing`)

- [ ] **5.2.8.13** Morning briefing page: urgent, important, informational sections
- [ ] **5.2.8.14** Action items list
- [ ] **5.2.8.15** Reading list
- [ ] **5.2.8.16** Relationship health dashboard
- [ ] **5.2.8.17** Weekly digest view

### Memory Browser (`/agent/memory`)

- [ ] **5.2.8.18** Contact list with memory summary
- [ ] **5.2.8.19** Contact detail: facts, summaries, topics, sentiment
- [ ] **5.2.8.20** Edit/delete/add memories
- [ ] **5.2.8.21** Search across all memories
- [ ] **5.2.8.22** Export memories

### Draft Queue

- [ ] **5.2.8.23** Draft list across all chats
- [ ] **5.2.8.24** Draft preview with context
- [ ] **5.2.8.25** Inline edit before approve
- [ ] **5.2.8.26** Batch approve
- [ ] **5.2.8.27** Reject with feedback

### Outreach Planner (`/agent/outreach`)

- [ ] **5.2.8.28** Visual schedule / timeline
- [ ] **5.2.8.29** Plan list with status
- [ ] **5.2.8.30** Create plan wizard
- [ ] **5.2.8.31** Outreach history

---

## 5.2.9 Live Translation

- [ ] **5.2.9.1** Per-chat translation toggle
- [ ] **5.2.9.2** Target language setting (global + per-chat)
- [ ] **5.2.9.3** Incoming: show original + translated inline
- [ ] **5.2.9.4** Outgoing: translate before sending
- [ ] **5.2.9.5** Auto language detection
- [ ] **5.2.9.6** Translation caching

---

## 5.2.10 Image Generation

- [ ] **5.2.10.1** Provider config: DALL-E, Stability, local
- [ ] **5.2.10.2** `/imagine` chat command
- [ ] **5.2.10.3** Preview before send
- [ ] **5.2.10.4** Image variations
- [ ] **5.2.10.5** Cost tracking

---

## 5.2.11 Safety & Privacy

- [ ] **5.2.11.1** All auto-responses logged and reviewable
- [ ] **5.2.11.2** Kill switch for all auto-responses
- [ ] **5.2.11.3** Per-chat consent required
- [ ] **5.2.11.4** Message content stays local (only to AI provider)
- [ ] **5.2.11.5** Sensitive message detection
- [ ] **5.2.11.6** Optional AI-assisted disclosure
- [ ] **5.2.11.7** Memory encryption at rest

---

## 5.2.12 Deployment Modes

### Embedded (in Poly app)

- [ ] **5.2.12.1** Feature-gated module (`--features social-agent`)
- [ ] **5.2.12.2** MCP server on configurable port alongside UI
- [ ] **5.2.12.3** Shares `ClientBackend` pool (zero connection duplication)

### Standalone (headless)

- [ ] **5.2.12.4** Separate binary: `poly-agent`
- [ ] **5.2.12.5** Config file: accounts, personalities, schedules, AI provider
- [ ] **5.2.12.6** Systemd service / Docker container

---

## Completion Criteria

- [ ] Personality engine resolves base -> mood -> per-chat personality
- [ ] Memory system stores and retrieves per-contact facts and conversation summaries
- [ ] Learning system improves drafts from user corrections
- [ ] Auto-response pipeline: message -> context -> AI -> draft -> approve -> send
- [ ] Typing simulation is realistic (delays, indicators, false starts)
- [ ] Outreach scheduler triggers proactive messages
- [ ] Agent UI panels: sidebar panel, per-chat controls, briefing, memory, draft queue, outreach
- [ ] AI provider system supports Claude, OpenAI, Gemini, local models
- [ ] Live translation works per-chat
- [ ] Kill switch instantly disables all auto-responses
- [ ] Runs embedded or standalone

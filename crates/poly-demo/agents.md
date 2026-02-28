# poly-demo — Agent Instructions

> **Read root `agents.md` FIRST**, then this file.  
> **Last Updated:** 2026-02-28

---

## Purpose

`poly-demo` is a **mock/demo client** implementing the `ClientBackend` trait. It generates fake data for testing the UI without requiring real messenger accounts.

## What It Provides

- **Demo users**: Randomly generated names, avatars, online/offline status
- **Demo servers**: Multiple servers with categories and channels (text, voice, video) — mimics Discord/Stoat server structure
- **Demo messages**: Various message types (text, images, links, reactions) with realistic timestamps
- **Demo friends**: Friend list with status, last message preview
- **Demo groups**: Multi-user group chats (like Discord group DMs)
- **Demo notifications**: Friend requests, mentions, DM notifications
- **Fake event stream**: Periodic new messages, presence changes, typing indicators

## How To Use

Add the demo client in settings like any other backend. It creates a "Demo Account" that populates the UI with realistic mock data.

## Dependencies

- `poly-client` — the trait to implement
- `rand` — random data generation
- `lipsum` or similar — random text generation
- `chrono` — timestamps
- `tokio` — async runtime for fake event stream

## Implementation Notes

- Use hardcoded seed for reproducible demo data (but randomize on first init)
- Demo servers should have 3-5 categories each with 3-8 channels
- Demo friend list: 20-50 users with varying online status
- Demo messages: 50-200 per channel with realistic time distribution
- Demo groups: 3-5 group chats with 3-8 members each
- Typing indicator simulation: random users "type" periodically in active channels

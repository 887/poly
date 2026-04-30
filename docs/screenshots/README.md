# docs/screenshots — Screenshot Inventory

UI screenshots captured from running test backends via Playwright.
Organised by feature area; each subdirectory has its own notes.

## Subdirectories

| Directory | Feature | Status |
|-----------|---------|--------|
| `forum-composer/` | Lemmy forum composer + preview-image UX | TODO — see below |

---

## TODO — forum-composer screenshots (Phase E.2 deferred)

These 4 screenshots are deferred from
`docs/plans/plan-test-avatars-and-lemmy-forum-ux.md` Phase E.2 because
they require a running poly-web instance + active Playwright session.
Capture them in a follow-up "screenshots refresh" pass.

Target path: `docs/screenshots/forum-composer/`

| Filename | Description | How to capture |
|----------|-------------|----------------|
| `forum-list-previews-on.png` | Lemmy community post list with "Render previews" toggle ON — seeded koala/axolotl thumbnail visible in the first post row | Navigate to test-lemmy → select a community → ensure toggle is on → screenshot |
| `forum-list-previews-off.png` | Same view with toggle OFF — thumbnail column absent | Toggle the setting off → screenshot |
| `composer-markdown-preview.png` | New-post composer with the "Preview" tab active — rendered HTML visible below the tab bar | Click "New Post" in a forum channel → type some `**bold**` text → click Preview tab → screenshot |
| `inline-reply-expanded.png` | Inline reply composer expanded under the first comment of a forum post — nested `ForumComposer` visible | Open a forum post → click "Reply" on the first comment → screenshot |

### Playwright recipe outline

```js
// Requires poly-test-runner running (all backends on 9100-9107)
// and poly-web at localhost:3000 with test-lemmy account signed in.

// 1. forum-list-previews-on.png
await page.goto('http://localhost:3000/...');   // lemmy community route
await page.screenshot({ path: 'forum-list-previews-on.png' });

// 2. forum-list-previews-off.png
await page.click('[data-mechanism="render-previews"] input');
await page.screenshot({ path: 'forum-list-previews-off.png' });

// 3. composer-markdown-preview.png
await page.click('button:has-text("New Post")');
await page.fill('textarea', '**Hello** *world*');
await page.click('button:has-text("Preview")');
await page.screenshot({ path: 'composer-markdown-preview.png' });

// 4. inline-reply-expanded.png
await page.goto('...');   // open a specific forum post
await page.click('button:has-text("Reply")');
await page.screenshot({ path: 'inline-reply-expanded.png' });
```

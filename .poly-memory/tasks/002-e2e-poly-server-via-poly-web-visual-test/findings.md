# Research Findings — Task: E2E: Poly Server via poly-web visual test

*Auto-updated by poly-memory-mcp. Add findings via CLI or MCP tool.*

---


## Finding 2026-03-10T12:21:39Z

Add Account layout plan: Reuse settings-page/settings-nav/settings-content CSS for the two-panel signup layout. AddAccountShell component wraps both /signup and /signup/:client routes. Left sidebar lists backends with nav items (same CSS as settings), right panel shows backend form or placeholder. CSS to add: `.add-account-page { display:flex; height:100vh; overflow:hidden; background:var(--bg-primary); }`. SignupPickerPage and ClientSignupPage both delegate to AddAccountShell.

---

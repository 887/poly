# Makefile — convenience targets for Poly development.
# Phase F.4 of plan-persona-e2e-multi-agent.md.

.PHONY: e2e-personas e2e-personas-noop e2e-personas-real help

# Run all persona E2E scenarios in mock-claude mode (CI default).
# Usage: make e2e-personas
#        make e2e-personas SCENARIO=two-personas-handoff
SCENARIO ?= noop
MODE     ?= mock-claude

e2e-personas:
	@echo "Running persona E2E: scenario=$(SCENARIO) mode=$(MODE)"
	bash tests/e2e/persona-multi-agent.sh \
		--scenario $(SCENARIO) \
		--mode $(MODE)

# Quick smoke-test (no backends, no claude needed).
e2e-personas-noop:
	bash tests/e2e/persona-multi-agent.sh --scenario noop

# Real-claude mode (requires ANTHROPIC_API_KEY + --budget-tokens).
# Usage: make e2e-personas-real SCENARIO=mcp-to-ui-live-update BUDGET=50000
BUDGET ?= 100000
e2e-personas-real:
	@if [ -z "$$ANTHROPIC_API_KEY" ]; then \
		echo "ERROR: ANTHROPIC_API_KEY must be set for real-claude mode"; \
		exit 1; \
	fi
	bash tests/e2e/persona-multi-agent.sh \
		--scenario $(SCENARIO) \
		--mode real-claude \
		--budget-tokens $(BUDGET)

help:
	@echo "Poly Makefile targets:"
	@echo "  make e2e-personas                   — run noop scenario (smoke test)"
	@echo "  make e2e-personas SCENARIO=<name>   — run a specific scenario (mock-claude)"
	@echo "  make e2e-personas-noop              — alias for the noop smoke test"
	@echo "  make e2e-personas-real SCENARIO=<n> — real-claude mode (needs ANTHROPIC_API_KEY)"
	@echo ""
	@echo "All 6 Phase E scenarios:"
	@echo "  two-personas-handoff          two-personas-shared-channel"
	@echo "  fact-handoff                  mcp-to-ui-live-update"
	@echo "  deny-wins-source-resolution   heartbeat-tick-via-mcp"
	@echo "  rate-limit-respected"

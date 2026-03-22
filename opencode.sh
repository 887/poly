#!/bin/bash
export SHELL=/bin/bash

# Disables automatic token compaction when usage exceeds context window.
# This allows you to manually run /compact when needed.
export OPENCODE_DISABLE_AUTOCOMPACT=true

# Disables automatic pruning of old tool call results to save context window.
# This preserves everything in your context until you manually run /compact.
export OPENCODE_DISABLE_PRUNE=true

exec opencode web "$@"

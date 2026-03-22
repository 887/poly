#!/bin/bash
# Check the supported storage backend feature combinations.

set -euo pipefail

echo "Checking poly-core sqlite backend..."
cargo check -p poly-core

echo "Checking poly-core surreal backend..."
cargo check -p poly-core --no-default-features --features storage-surreal

echo "Checking poly-web wasm backend..."
cargo check -p poly-web --target wasm32-unknown-unknown

echo "All storage backend checks passed."

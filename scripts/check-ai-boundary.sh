#!/usr/bin/env bash
# TA-AI-01: no direct OR transitive normal dependency can expose World to AI.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo tree -p ai --edges normal --prefix none --format '{p}' | python3 -c '
import sys
names = {line.split()[0] for line in sys.stdin if line.strip()}
assert names == {"ai", "ai-api"}, f"AI boundary dependencies changed: {sorted(names)}"
print("ok: ai can depend only on the data-only ai-api crate")
'

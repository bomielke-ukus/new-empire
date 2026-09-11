#!/usr/bin/env bash
# Checks that every committed generated file still matches its generator.
#
# A stale generated file is one of the nastiest determinism bugs available:
# the source of truth and the compiled artefact disagree, nothing warns, and
# the symptom is a wrong number deep in the maths months later. Regenerating
# into a scratch copy and diffing costs a second.
set -euo pipefail
cd "$(dirname "$0")/.."

status=0
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

check() {
  local name="$1" committed="$2" regenerate="$3"
  echo "== $name =="
  if [ ! -f "$committed" ]; then
    echo "FAIL: $committed does not exist"
    status=1
    return
  fi
  cp "$committed" "$scratch/backup"
  if ! eval "$regenerate" > "$scratch/log" 2>&1; then
    echo "FAIL: the generator did not run:"
    sed 's/^/    /' "$scratch/log"
    cp "$scratch/backup" "$committed"
    status=1
    return
  fi
  # Format the regenerated file the same way CI will, so the comparison is
  # against what a contributor would actually commit. Without this the check
  # fails on line wrapping alone, which trains people to ignore it.
  if [ "${committed##*.}" = "rs" ]; then
    rustfmt --edition 2021 "$committed" 2>/dev/null || true
  fi
  if diff -u "$scratch/backup" "$committed" > "$scratch/diff"; then
    echo "ok"
  else
    echo "FAIL: $committed does not match its generator. Regenerate and commit:"
    echo "    $regenerate"
    sed 's/^/    /' "$scratch/diff" | head -40
    status=1
  fi
  # Leave the tree as we found it either way; CI should report, not mutate.
  cp "$scratch/backup" "$committed"
}

check "trig table" \
  "crates/sim/src/trig_table.rs" \
  "python3 tools/gen/gen_trig_table.py"

# The renderer's palette is baked from the art pipeline's source of truth.
# If this drifts, sprites validated against one palette are drawn with another
# (docs/07 Q10).
check "palette table" \
  "crates/view/src/palette_table.rs" \
  "cargo run --quiet -p atlas -- export --out \"$scratch\" --rust crates/view/src/palette_table.rs"

exit $status

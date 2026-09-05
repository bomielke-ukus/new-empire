#!/usr/bin/env bash
# The art conformance gate, per docs/05 §6: "Every sprite goes through
# tools/atlas, which validates size, anchor, facing count and palette
# conformance and fails the build on violation."
#
# Four steps, each of which can fail the build on its own:
#
#   0. The render rig still describes the projection docs/05 specifies. Every
#      sprite in the game is rendered through it, so a drifted rig invalidates
#      every frame already made.
#   1. The palette bakes, every index is claimed, and the eight player colours
#      stay tellable apart under simulated protanopia and deuteranopia.
#   2. The placeholder catalogue regenerates. It is not committed — it is
#      derived from the palette — so this also proves a fresh clone can produce
#      the art the game loads.
#   3. Every sprite set on disk conforms.
#
# Run from the repository root. Exit status is non-zero on any violation.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== rig: camera, facings and lights still match the spec =="
cargo run --quiet -p atlas -- rig

echo
echo "== palette: bakes, and player colours survive colour blindness =="
cargo run --quiet -p atlas -- palette

echo
echo "== placeholders: regenerate from the palette =="
cargo run --quiet -p atlas -- placeholder

echo
echo "== atlas: every sprite set conforms =="
cargo run --quiet -p atlas -- validate

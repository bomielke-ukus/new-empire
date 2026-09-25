#!/usr/bin/env bash
# Rebuilds the slice's sprite sets from their models: for each subject in
# tools/render/slice.py (or the ones named), build the Blender file, render
# every frame through the frozen rig, and compose the frames into
# assets/sprites/<name> with `atlas compose`, which validates the result.
#
#   BLENDER=/path/to/blender scripts/render-sprites.sh [NAME...]
#
# Needs Blender 4.x (`BLENDER`, default `blender` on the PATH); the rig
# renders on the CPU with Cycles, so no GPU is needed. Work files go to
# target/renders/.
set -euo pipefail
cd "$(dirname "$0")/.."

blender="${BLENDER:-blender}"
work="target/renders"
mkdir -p "$work"

subjects=("$@")
if [ ${#subjects[@]} -eq 0 ]; then
  mapfile -t subjects < <("$blender" --background --python tools/render/slice.py -- --list 2>/dev/null \
    | awk 'NF == 3 && ($3 == "unit" || $3 == "building") { print $1 }')
fi

for name in "${subjects[@]}"; do
  line="$("$blender" --background --python tools/render/slice.py -- --list 2>/dev/null \
    | awk -v n="$name" '$1 == n { print $2, $3 }')"
  if [ -z "$line" ]; then
    echo "no subject called $name in tools/render/slice.py" >&2
    exit 1
  fi
  read -r class what <<<"$line"
  echo "== $name ($class $what) =="
  "$blender" --background --python tools/render/slice.py -- \
    --subject "$name" --save "$work/$name.blend" >/dev/null
  root="$("$blender" "$work/$name.blend" --background --python-expr \
    'import bpy; print("ROOT", [o.name for o in bpy.data.objects if o.parent is None and o.type == "EMPTY"][0])' \
    2>/dev/null | awk '$1 == "ROOT" { print $2 }')"
  rm -rf "$work/$name"
  if [ "$what" = unit ]; then
    anims=(--anim idle=1-4 --anim walk=5-12 --anim attack=13-18 --anim death=19-26 --anim decay=27-30)
    facings=()
  else
    anims=(--anim construction=1-3 --anim idle=4-4 --anim rubble=5-5)
    facings=(--still 1)
  fi
  "$blender" "$work/$name.blend" --background --python tools/render/render_sheet.py -- \
    --subject "$root" --class "$class" --out "$work/$name" "${anims[@]}" "${facings[@]}" \
    | grep -E "frames x|wrote"
  cargo run --quiet -p atlas -- compose --renders "$work/$name" --set "$name" \
    --class "$class" --out "assets/sprites/$name"
done
cargo run --quiet -p atlas -- validate | tail -1

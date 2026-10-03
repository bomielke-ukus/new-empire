#!/usr/bin/env bash
# Rebuilds the slice's sprite sets from their models: for each subject in
# tools/render/slice.py (or the ones named), build the Blender file, render
# every frame through the frozen rig, and compose the frames into
# assets/sprites/<name> with `atlas compose`, which validates the result.
# The ground_* subjects are rendered and then made, all eight together, into
# the ground's grain, assets/terrain/detail.png, by `atlas detail`.
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
    | awk 'NF == 3 && ($3 == "unit" || $3 == "villager" || $3 == "building" \
                       || $3 == "wall" || $3 == "gate" || $3 == "node" || $3 == "ground") \
                       { print $1 }')
fi

grounds=0
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
  elif [ "$what" = villager ]; then
    # Every unit's five, then the tasks and the carry walks
    # (tools/render/kit.py, VILLAGER_SPANS).
    anims=(--anim idle=1-4 --anim walk=5-12 --anim attack=13-18 --anim death=19-26 --anim decay=27-30
      --anim chop=31-36 --anim mine=37-42 --anim forage=43-48 --anim farm=49-54 --anim build=55-60
      --anim carry_wood=61-68 --anim carry_food=69-76 --anim carry_gold=77-84
      --anim carry_stone=85-92)
    facings=()
  elif [ "$what" = building ]; then
    anims=(--anim construction=1-3 --anim idle=4-4 --anim rubble=5-5)
    facings=(--still 1)
  elif [ "$what" = wall ]; then
    # The post alone as the finished frame, then an arm toward each of the
    # eight neighbours (tools/render/kit.py, WALL_SPANS).
    anims=(--anim construction=1-3 --anim idle=4-4 --anim rubble=5-5 --anim arm=6-13)
    facings=(--still 1)
  elif [ "$what" = gate ]; then
    # Shut and open in each of four orientations (kit.py, GATE_SPANS).
    anims=(--anim construction=1-3 --anim idle=4-4 --anim rubble=5-5 \
      --anim shut=6-9 --anim open=10-13)
    facings=(--still 1)
  elif [ "$what" = ground ]; then
    # One tile of ground, for its grain; composed with the others below.
    anims=(--anim ground=1-1)
    facings=(--still 1)
  else
    # A node of the map: one standing frame.
    anims=(--anim idle=1-1)
    facings=(--still 1)
  fi
  "$blender" "$work/$name.blend" --background --python tools/render/render_sheet.py -- \
    --subject "$root" --class "$class" --out "$work/$name" "${anims[@]}" "${facings[@]}" \
    | grep -E "frames x|wrote"
  if [ "$what" = ground ]; then
    grounds=1
    continue
  fi
  cargo run --quiet -p atlas -- compose --renders "$work/$name" --set "$name" \
    --class "$class" --out "assets/sprites/$name"
done
if [ "$grounds" = 1 ]; then
  # Needs every ground_* render under target/renders, this run's or an
  # earlier one's; it says which is missing.
  cargo run --quiet -p atlas -- detail --renders "$work"
fi
cargo run --quiet -p atlas -- validate | tail -1

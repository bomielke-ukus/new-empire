# The render rig

Every sprite in the game is rendered through one camera and three lights.
`assets/render/rig.json` is that setup, and it is **frozen**: changing a number
in it invalidates every frame already produced, because a unit lit from a
slightly different angle does not cut together with the ones beside it.

The rig is verified rather than trusted. `cargo run -p atlas -- rig` re-derives
the camera basis from its Euler angles, checks that a tile projects exactly 2:1
as `docs/05` §1 requires, checks each facing rotation actually points the
subject the declared way on screen, checks each light aims where it says and
sits above the horizon, and checks every size class against the table in
`docs/05` §2.3. CI runs it via `scripts/check-art.sh`.

## What the numbers mean

**Camera: orthographic, 30° above the horizon, azimuth 45°.** The 30° is not a
taste decision — it is forced. The on-screen height of a tile over its width is
`sin(elevation)`, and `docs/05` §1 says that ratio is 1:2, so the elevation is
exactly 30°. (The 26.565° figure often quoted for 2:1 pixel art is a different
measurement: the slope of a tile *edge* on screen. It is not the camera angle,
and using it gives a tile that is not 2:1.)

**The camera never moves and never orbits.** Facings come from turning the
subject. If you orbit the camera instead, the key light swings around the
subject with it and every facing ends up lit differently — the fastest way to
ruin a sprite set. `render_sheet.py` parents the subject to a turntable empty
and rotates the empty, so an animation that keys the subject's own rotation
cannot fight the facing.

**Three lights, placed by where they sit on screen** rather than in the world,
because screen position is the thing that has to stay constant across every
sprite in the game:

| Light | Screen position | Energy | Job |
|---|---|---|---|
| key | 45° up and to the left | 4.0, warm | Defines form. The only light that does. |
| fill | up and to the right | 1.0, cool | Keeps shadows sky-lit rather than black. The 4:1 ratio is what gives the palette's ramps somewhere to go. |
| rim | directly above, behind | 2.0, neutral | Catches the top edge, so a dark unit stays legible against dark terrain. |

**The camera slides up per size class.** A subject modelled standing on the
origin would otherwise render at the frame centre, wasting the bottom half of
every frame. `camera_up_shift` moves the camera along its own up axis so the
origin lands on the class's anchor — the ground contact point. Terrain is the
exception: a tile *is* the ground, so it is placed by its centre and its shift
is zero.

**`view_transform` must be `Standard`.** This is the trap. Blender ships AgX (4.x)
or Filmic, both of which desaturate and roll off highlights, so the colours you
texture are not the colours that reach `atlas quantize`, and the palette match
lands somewhere you did not intend.

## Modelling conventions

- **One Blender unit is one tile.** A standing human is **0.9 units** — about
  35 px at 1× in a 48 px frame. That is deliberately oversized against real
  proportion, as every isometric RTS does it, because a unit has to be readable
  at a glance.
- **The subject stands on the world origin**, feet at `z = 0`, facing **+Y**.
- **No asymmetric detail.** SE, E and NE are horizontal mirrors of SW, W and NW
  (`docs/05` §2.1), so a shield on one arm only will swap arms halfway round.
- **Player-coloured surfaces are textured pure magenta** (`#ff00ff`), which
  appears nowhere in the palette. `atlas quantize` maps pixels near that hue
  into the reserved ramp at indices 240–247 *by lightness*, so the shading the
  renderer produced survives into the indexed sprite. Every unit and building
  needs some, or `atlas validate` rejects it — a sprite with no player colour
  has an invisible owner.
- **No ground plane in the render.** The film is transparent and the game draws
  its own shadow.

## Using it

Inspect the rig, or set up Blender by hand from its numbers:

```sh
cargo run -p atlas -- rig
```

Build the rig into a Blender file, and check Blender agrees with it:

```sh
blender --background --python tools/render/rig.py -- --check
blender --background --python tools/render/rig.py -- --save tools/render/rig.blend
```

`--check` is worth running after a Blender upgrade. `atlas rig` proves the
numbers describe the right projection; `--check` proves Blender still agrees
about what those numbers *mean*, which is a different claim and the one that
catches a Euler-order or handedness change.

Apply the rig to a file you are working in:

```sh
blender villager.blend --python tools/render/rig.py
```

Render a subject, then compose the frames into a validated sprite set:

```sh
blender villager.blend --background --python tools/render/render_sheet.py -- \
    --subject Villager --class Foot --out build/renders/villager \
    --anim idle=1-4 --anim walk=5-12 --anim attack=13-18 \
    --anim death=19-26 --anim decay=27-30

cargo run -p atlas -- compose --renders build/renders/villager \
    --set villager --class Foot --out assets/sprites/villager
```

`render_sheet.py` writes one RGBA PNG per frame, named
`<animation>_<facing>_<index>.png`, and knows nothing about palettes, sheet
layout or anchors. `atlas compose` owns all of that, so the layout is defined in
exactly one place rather than in a Rust file and a Python file that drift apart.
It quantises each frame, lays them out one row per (animation, facing) and one
column per frame, writes the manifest with the anchor the rig aimed at, and runs
the same validation any other art goes through.

## Blender version

Written against Blender 4.x. The engine is the one thing the script falls back
on rather than failing: EEVEE Next, then EEVEE, then Cycles. Cycles would be
more accurate and is not worth the render time at 192 px.

# Units in the new look (phase 4)

Binding for every unit colour set: villagers, infantry, riders, engines and
ships. `docs/look/colour-sets.md` holds the format; this file says what a
unit set contains, how it is made, and what it costs. Proven with
`villager` and `hoplite`; the other unit sets are made the same way.

## Which look

As for buildings (`docs/look/buildings.md`, "Which look"): with the new art
on, a kind with any colour set is drawn from its colour sets only, in every
age and architecture; the old `villager_tool`, `hoplite_iron`, ... are left
out (tested: `the_new_units_are_drawn_from_their_colour_sets_in_every_facing`).
An age set added later (`hoplite_iron`) wins from that age on. Units have no
architecture suffix, so every match loads them. With the new art off nothing
here is read.

## The sheet

- **Rows**: one per (animation, facing), the animations in manifest order,
  the five authored facings **S, SW, W, NW, N** within each (`facing8` 1 to
  5). The game mirrors the other three: SE is SW flipped, E is W, NE is NW
  (`sprites::source_facing`). An asymmetric unit (a shield on the left arm)
  carries it on the right in a mirrored facing, as the old art did.
- **Columns**: one per frame, as many as the longest animation; a shorter
  one's other cells are transparent, and every frame of every animation
  and facing draws something (`check_set.py` fails an empty one).
- **One cell size and one anchor** for the whole set: the union of every
  frame in every facing, its contact shadow and the tile's diamond, trimmed
  to what is drawn (on even pixels). The anchor is the unit's ground point:
  the ground under its hips, between its feet (the figure's origin). The
  atlas trims each frame again, so the union costs nothing in memory.
- **`scale: 1`**: a unit is authored at half the buildings' resolution, one
  authored pixel to one screen pixel at zoom 1 (a tile is 64 x 32): it is
  small on screen, and at 2 it costs four times the memory ("Memory").
  At zoom 2 and 3 it is drawn pixel-doubled. `scale: 2` is allowed (a
  `SETS` entry without `scale`); check_set allows 1 only for a turning class.
- **Size**: tiles, as the buildings are. A person is about 1.0 tile tall
  (46 px at zoom 1 for the villager; the hoplite's crest and spear rise
  above that), a horse about 1.6 long; `figures.py`'s scale.

## Light, shadow, colour

- The sun and sky are `lib.light()`'s, as the buildings have, but the sun
  lights the figure only (Cycles light linking): a unit casts no long
  shadow, which would double its cell and read as a second figure in a
  crowd. A second sun straight overhead, 30 degrees wide, lights the ground
  only, so the figure leaves a soft **contact shadow** under its feet (and
  under its body when it lies), translucent black as a building's shadow.
  Picking ignores it (`Atlas::solid_at` skips shadow pixels).
- **Player colour**: large, clean areas of it, so sides read at a glance at
  zoom 1: the tunic (villager), the skirt, sleeves, cape, crest and shield
  face (hoplite). Player materials are `units.pm(player, shade)` with shade
  one of `PLAYER_SHADES` (1.0, 0.82, 0.64), or `lib.mat(player)` itself; the
  exporter finds them by those colours. The mask comes from the same render
  as the picture (an AOV, "Exporter"), antialiased as the picture is.
- Style: the buildings' palette and light; skin, hair, bronze, linen and
  wood from `lib.PAL` and `figures.py`.

## Animations

The names, frame counts, `frame_ms`, `loops` and `impact` frames are
exactly the old set's of the same name (`assets/sprites/<name>/<name>.ron`),
so everything timed by them is unchanged; `check_set.py` and
`sheets::the_new_units_keep_the_old_units_timing` fail a set that differs.
Every foot unit:

| animation | frames | ms   | loops | impact | what |
|-----------|--------|------|-------|--------|------|
| `idle`    | 4      | 160  | yes   |        | breathing and a slow weight shift, small enough not to shimmer |
| `walk`    | 8      | 100  | yes   |        | left foot strikes on frame 0, right on 4 (`docs/05` §2.2); arms swing against the legs |
| `attack`  | 6      | 90   | no    | 3      | wind-up 0 to 2, the blow lands on 3, recovery 4 and 5 |
| `death`   | 8      | 110  | no    |        | hit, stagger back, knees go, falls on its back; the last frame lies still |
| `decay`   | 4      | 7500 | no    |        | the body lying as death left it, sinking into the ground and fading to 55% |

The villager adds, after them in this order: `build` (6 x 140, loops: a
crouched hammer blow), `carry_food`, `carry_gold`, `carry_stone`,
`carry_wood` (8 x 100, loops: the walk with the load held: a basket of
berries in both hands, a basket of gold nuggets on the left shoulder, a
dressed block hugged to the chest, three logs on the right shoulder),
`chop` (axe, a two-handed diagonal chop at waist height), `farm` (hoe, a
short chop into the ground in front), `forage` (bend, pick, lift to the
basket on the hip) and `mine` (pick, overhead down to the ground), each
6 x 140, looping, the stroke landing at the middle of the loop. The
villager's `attack` swings the axe.

How the view plays them (unchanged, `scene.rs`): the attack plays from the
moment the blow is struck in the simulation for 6 ticks (300 ms), so frames
0 to 3 show and the impact frame is the last seen; death plays while the
corpse is younger than its 880 ms, then decay over the 30 s the corpse
lies (`DECAY_TICKS`); a working villager shows its task, a walking one
with a load its carry; idle and work loops are offset per unit.

**Walk and speed.** The feet are IK targets: a planted foot moves back
under the hips at exactly the unit's speed (`kinds.rs`, tiles a second:
villager 0.9, hoplite 0.8) for 58% of the 0.8 s cycle, so it stands still
on the ground while the unit moves at that speed; the hips sit as low as
the planted legs need (lowest with both feet down). A unit sped up or
slowed by its civilisation slides a little, as the old art did.

**Death.** The body falls on its back roughly where the unit stood
(`anim.FALL_X` carries it forward so its middle lies over the ground point,
not a body's length behind it); what it held falls flat beside it and a
shield face up (`units.drop_tool`).

## Figure rules

Readability at game size first: a unit is 40 to 50 px tall at zoom 1.

- **Proportions** (`units.Build`, tiles): slightly heroic. Head radius
  0.135, hands 0.052 (big), thigh 0.205 and shin 0.195 with sturdy radii
  (0.068 to 0.044), torso 0.30, shoulders 0.158 either side.
- **Silhouette**: weapon and shield visible in every facing: tools and
  weapons drawn thick (a handle 0.025 across) with big heads; the shield
  0.25 in radius on the left forearm; the hoplite's crest tall but not
  towering (the hoplite stands about 1.15 to the crest's top, 1.45 to the
  spear's point, against the villager's 1.07).
- **Facing the camera** (S) a shield is carried at the left side, turned
  half outward, so the body shows beside it; a swung tool is aimed (`aimw`,
  `aimup`) so that wound up it points up and back over the right shoulder,
  never straight behind the head, and lands a little right of the body.
- **Tell the kinds apart by shape first** (`units_foot.py`'s docstring):
  weapon, shield shape, headgear and cloak differ between every two
  infantry kinds, and each has large player-colour areas.
- One set is one person (a fixed seed): no per-unit variety in a set.
- The figure faces +X; the exporter turns it, never the camera or the sun.

## How a unit is made (`tools/look`)

- **`anim.py`**: poses and motion, no Blender. A pose is a flat dict of
  joint values (hips, torso, head, both arms by flex, abduction, twist,
  elbow and wrist, IK hand targets, feet as IK targets, a held tool's aim,
  a shield's place); `keyed()` interpolates keyframes with easing;
  `walk()`, `swing_arms()`, `breathe()`, `death()`, `fallen()`, `decay()`
  are shared by every foot unit; `FOOT` and `VILLAGER_WORK` hold the timing.
- **`units.py`**: `Rig` turns a pose into joint positions and frames
  (two-bone IK for legs and IK'd arms); `draw_body`, `draw_torso`,
  `draw_skirt`, `draw_head`, `tool_mesh` (hammer, axe, pick, hoe, spear),
  `shield_mesh`, `draw_cape`, `carry_load` draw on it, one mesh per frame
  (figures.py's builder); `tool_mesh` also has the soldiers' `club`,
  `battleaxe`, `sword`, `gladius`, `spear_plain` and the priest's `staff`.
  A figure with its own shield or tool overrides `Figure.draw_shield` /
  `draw_tool`. A `Figure` subclass is a unit: its `Build`, `speed`,
  `animations` (from `anim.FOOT`), `gear(anim)` (the tool in the right
  hand, a shield on the left), `outfit()`, `load()`, and a `pose_<anim>(t)`
  for each animation, most of them one line over the shared ones.
- **`unitexport.py`**, export.py's path for a figure (a `units.py` factory
  marked `unit`): one scene set up once; per (animation, frame) the figure's
  mesh is rebuilt, then turned to each facing and rendered (Cycles, 20
  samples, denoised with the accurate prefilter, a 1.0 px pixel filter, only
  the render border the pose can reach). Each render
  is written as a multilayer EXR (uncompressed) and read back by `exr.py`:
  the beauty and two AOVs, the subject's coverage (to split the contact
  shadow off, as a building's) and its player-coloured share (the mask).
  The compositor, which would do this in Blender, needs a GPU context
  headless Blender lacks. The decay's frames fade (alpha to 55%).
  **Crispness**: a unit is drawn pixel-doubled at zoom 2 beside buildings
  authored at 2, so each render is sharpened (`unitexport.crisp`): an
  unsharp mask against the figure's own local colour (no halo against the
  ground; `SHARPEN`) and the rim one pixel in from the silhouette darkened
  as a painted outline (`OUTLINE`).
  Resumable: every finished cell is kept under
  `target/look-units/<set>/<hash>/` until the set is written, and a rerun
  with the same hash renders only what is missing.
  `/usr/bin/python3.13 tools/look/unitexport.py villager OUT.png chop,mine`
  previews some animations without exporting.

### `SETS`

`units.py` declares its sets like the architecture modules:

```python
SETS = {
    "villager": dict(func="units.villager", cls="Foot", scale=1),
    "hoplite": dict(func="units.hoplite", cls="Foot", scale=1),
}
```

The foot soldiers and the priest are in `units_foot.py` (its own `SETS`,
listed in `export_all.MODULES`); riders and ships in their own modules the
same way. `func` is a factory made by `units._unit(FigureClass)`; `cls` the size
class (`Foot`, `Mounted`, `Heavy`, `Ship`); `scale` 1 (or omitted: 2);
`samples` as for buildings. `export_all.py` exports, hash-skips and checks
them as it does buildings; a unit's hash also covers `unitexport.py` and
`exr.py` (and, through its imports, `anim.py`, `figures.py`, `lib.py`).
`--points-only` passes units by.

Cost, measured (4 cores, release bpy 5.2): **villager 460 cells, 0.09 s a
cell, 55 s the set; hoplite 150 cells, 0.14 s a cell, 27 s** at scale 1
(at scale 2: 0.27 and 0.47 s a cell, 155 s and 85 s).

### Adding a unit

One `Figure` subclass and one `SETS` line: its proportions if not the
default, an `outfit()` from the shared body parts and its own armour or
headgear, `gear()`, a `base()` stance, and `pose_attack` (the one
animation that is the unit's own); idle, walk, death and decay come from
`anim.py` through `Figure`. A new weapon is a branch of `tool_mesh`. The
hoplite is about 120 lines; another spearman or swordsman reuses most of
them. Riders and ships need their own rigs (a horse's legs and a rider on
it; a hull and oars), built on the same `Rig`, `keyed()` and exporter.

## Memory

Measured with the two sets in, scale 1 (colour pages, RGBA plus mask):

| atlas | before | with villager and hoplite |
|-------|--------|---------------------------|
| Greek only (a Greek match) | 4096 x 2624, 51.3 MiB | 4096 x 3072, 60.0 MiB |
| all four architectures (editor, mapview) | 8192 x 4160, 162.5 MiB | 8192 x 4416, 172.5 MiB |

The two add 9 to 10 MiB packed; the sums of their trimmed frames are 7.7
MiB for the villager's 460 and 4.3 for the hoplite's 150 (its spear and
its lying body widen them). At scale 2 the same sums were 22.7 and 13.3
MiB, three times as much.

Projection for every unit at scale 1 (about 26 mobile kinds, 45 sets with
the old art's infantry and villager age costumes): a foot unit about 3 MiB,
the villager 6, a rider or chariot about 8, an elephant or engine about 9,
a ship about 10, so roughly 230 MiB with age costumes, 180 MiB without; at
scale 2 about 700 MiB. Over the 150 MiB wanted for units, so:

1. **Half resolution** (`scale: 1`) for every unit. Done: the two sets
   are authored at 1.
2. **One look per unit kind**: no age costumes in the new art unless a
   kind needs one to be told apart (the rule already makes one set enough
   for every age). Saves about 50 MiB.
3. **Decay from the death's last frame**: the view sinking and fading the
   corpse as it sinks a fallen building (`scene::sink`), so a set has no
   decay frames: 4 of a foot unit's 30 frames a facing, and the widest
   (lying) ones, about 15%. Needs a view change and a manifest without
   `decay` (the timing test then compares the rest).
4. **Only the units in play**: like the buildings' architectures, load only
   the kinds the match's civilisations can train. Civilisations deny one
   to three units each, so this saves little now.
5. **Compressed colour pages** (BC7 on the GPU, a quarter of RGBA8) if the
   whole new look must fit far below the current 170 MiB.

With 1 to 3, about 150 MiB for every unit.

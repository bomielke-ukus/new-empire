# Buildings in the new look (phase 3)

Binding for every building colour set (`docs/look/colour-sets.md` holds
the format; this file says what a building set contains, how the game
picks one, and how the art kit makes it).

## Which look

Set names are as for the old art: `<kind>` is the first architecture's
(Greek), `<kind>_egyptian`, `<kind>_mesopotamian`, `<kind>_asian` the
others; an age suffix (`_tool`, `_bronze`, `_iron`) after the
architecture's makes a look for that age and later.

With the new art on (`Settings::new_art`, `mapview --look 1`):

1. A kind with **any** colour set is drawn from colour sets only. Every
   indexed set of that kind, of every age and architecture, is left out
   (`Atlas::with_sheets`). There is no placeholder for it either.
2. For an owner of architecture A in age G the look is, in order: A's
   colour set of the latest age up to G; else the first architecture's
   colour set of the latest age up to G; else the kind's first colour set
   (by architecture, then age) as a stand-in, so a kind whose only colour
   set is `temple_egyptian` is that temple for everyone.
3. A kind with no colour set at all is drawn as before, from its indexed
   sets, ages and architectures included.

Phase 3 makes one look per kind per architecture and no age sets, so a
building looks the same in every age. An age set added later (`temple_iron`)
wins from that age on, exactly as the old art's do. With the new art off no
colour set is loaded and none of this changes anything (tested:
`with_the_new_art_a_kind_with_a_colour_set_never_falls_back_to_old_art`).

## What a building set holds

One cell size and one anchor for every frame; the class is not turning, so
one row per animation, one column per frame, in this order:

| animation      | frames | ms   | loops | what                                        |
|----------------|--------|------|-------|---------------------------------------------|
| `construction` | 3      | 200  | no    | the site, the building at ~35%, at ~70%     |
| `idle`         | 1      | 160  | yes   | the building                                |
| `rubble`       | 1      | 200  | no    | what is left when it falls                  |
| (walls) `arm`  | 8      | 1000 | no    | one arm per neighbour, see Walls            |
| (gate) `shut`, `open` | 4 each | 1000 | no | one per line, see Walls                 |

Unused cells (the idle and rubble rows' columns 2 and 3) are transparent.
The view uses these exactly as an indexed set's: construction stage
`done / (work / 3)` picks the frame (`Atlas::stage_frame`), uncropped;
`Atlas::own_rubble` while the building lies fallen; for its first
`COLLAPSE_TICKS` the idle frame sinks into the rubble. A building set
without them falls back to the generic pegs, the idle frame cropped, and
the footprint's indexed rubble, so a set must have all three.

Player colour: the mask is made per frame from the model, so whatever is
player-coloured in a stage keeps its mask. Flags and awnings may be absent
from the stages and rubble.

## How the kit makes them (`tools/look`)

The art agent writes only the idle model function. `export.py --stages`
renders, from that one model, with no per-model work (`stages.py`):

- **construction 0**: the model removed; the footprint pegged out with
  string at knee height, a levelled earth patch, a trench along the walls'
  line (the model's ground rectangle), a pile of squared blocks in the
  wall's colour and a stack of timber.
- **construction 1, 2**: the model cut by a horizontal plane at 35% and 70%
  of its height (of its mass: thin poles and flags do not count). The walls
  are left open-topped, never capped flat (a flat cap reads as a finished
  roof): a cut through a mass as wide as a room (twice its area over its
  perimeter at least 0.2 tile) becomes a ring of wall, its thickness
  showing, round a dark interior sunk to the floor (or to whatever stands
  inside); a very wide mass (over 1.0: a ziggurat's stage, a pyramid) a
  ring round a shallow sunken core; a post or a thin wall keeps a flat cap.
  Blocks of the next courses lie along the ring's outer edge one or two
  high here and there, so the top steps. A wall mass whose own top is under
  the cut once its roof is gone is opened the same way just below its top.
  A roof is never half there: any part wholly above 40% of the
  height that mostly faces up, or is the roof's colour, goes; small things
  (plants, pots) crossing the cut go rather than being sliced. Timber
  scaffolding round the ground rectangle up to the cut: posts, ledgers,
  braces on the two sides the camera sees, planks along the top; a few
  blocks waiting in front.
- **rubble**: the model removed; on its ground rectangle a dust stain, a
  low mound, broken blocks in the wall colour, charred beams, broken tiles
  in the roof colour and two stubs of wall.
- Colours: the idle frame is rendered first and its colours clustered
  (`stages.palette`): the wall is the commonest colour in the picture's
  lower half, the roof the commonest clearly different one in its upper
  part. A Greek ruin comes out white and terracotta, an Egyptian one
  sandstone.
- After rendering, every cell is trimmed to the union of all frames' drawn
  pixels (on even pixels) and the anchor moved with it.

`export.py --anims '{"arm": [{"arm": 0}, ...]}'` adds animations whose
frames are the same builder called with more keyword arguments.

### `SETS` and the batch export

Each architecture module (`greek.py`, `egyptian.py`, `mesopotamian.py`,
`asian.py`), and `nature.py` for the resources (`gold_mine`, `stone_mine`,
`berry_bush`: one `idle` frame each, `stages=False`, as their old sets),
declares its sets:

```python
SETS = {
    "barracks_egyptian": dict(
        func="egyptian.barracks",   # required: MODULE.FUNC
        cls="MediumBuilding",       # required: size class (docs/05 2.3)
        tiles=2,                    # the game's footprint (default: the class's)
        kwargs={},                  # builder keyword arguments
        fit=None,                   # scale the model to this many tiles
        model_tiles=None,           # the model's own footprint, for fit
        stages=True,                # True: building stages; "flat": farm; False: idle only
        anims={},                   # {"arm": [{"arm": k} for k in range(8)]}
        sun_angle=None, samples=None,
    ),
}
```

`/usr/bin/python3.13 tools/look/export_all.py [--only NAME ...] [--jobs N]
[--force] [--list] [--debug DIR]` exports every declared set and checks it
with `check_set.py`. A set is skipped when its manifest's
`// export-hash:` line matches the hash of: the entry, the source of the
module that draws it and every `tools/look` module it imports, `lib.py`,
`stages.py`, and `export.py`'s `EXPORT_VERSION`.

## Walls and gates

Today (`crates/view/src/walls.rs`, `scene.rs::push_arms`) a wall tile is
its **post** (the `idle` frame) plus one **arm** frame per neighbour it
joins, drawn on the post's anchor: `arm` frame k reaches toward
`WALL_DIRECTIONS[k]` = (1,0), (1,1), (0,1), (-1,1), (-1,0), (-1,-1), (0,-1),
(1,-1) in game tiles, ending at the shared edge (orthogonal) or corner
(diagonal), where the neighbour's arm meets it. Arms are depth-sorted
against the post by `arm_depth`. A gate is not a post: it is `shut` or
`open` frame l (0 to 3), standing across line l, the line of
`WALL_DIRECTIONS[l]`; its `idle` frame is its look in memory and ghosts.
There is one frame per direction, not per neighbour mask.

Game axes to the kit's: game +x is the model's +X, game +y the model's -Y
(the camera looks from (+X, -Y)). So arm k points along
`(dx, -dy)` of `WALL_DIRECTIONS[k]` in the model.

A colour wall supplies: `func(arm=None)` drawing the post alone when `arm`
is None and the arm k alone (no post) otherwise, both centred on the tile;
declared with `anims={"arm": [{"arm": k} for k in range(8)]}`. A gate:
`func(line=0, open=False)`, declared with `anims={"shut": [{"line": l,
"open": False} ...], "open": [... True]}`, its idle being line 0 shut. The
exporter renders these now; the stages and rubble are made from the idle
(the post), which is what a wall site shows.

**Shadows apart.** Each piece is rendered alone, so its shadow on the
ground is drawn where its neighbours' pieces stand: the afternoon sun casts
along game -y, so along a wall running in y a post's shadow lies under the
arms of the tile behind it, which are drawn before the post, and the shadow
painted over them in dark bars. The atlas therefore cuts every colour wall's
and gate's frame in two (`sprites.rs::split_shadow`): the shadow (its
translucent near-black pixels, `is_shadow`) and the rest, each trimmed, one
anchor; `Atlas::shadow_of(frame)` gives the shadow. The scene draws it at
the piece's depth less `WALL_SHADOW_DEPTH` (1.4), under the post and arms
of the tile behind, so it falls on the ground and not on the wall (tested:
`a_colour_wall_casts_its_shadow_apart`). Other buildings keep their shadows
in their frames. It costs some page area: the two trimmed rectangles
overlap (about 2.5 MiB an architecture).

## Farms

A farm is one `idle` frame today: the view has no sown, growing or
exhausted states (a farm is reseeded in place, and is not drawn smaller as
it is worked). A colour farm declares `stages="flat"`: construction 0 is the
pegged site, 1 and 2 the field cut at 35% and 70% of its crop height with
no scaffolding, the rubble a trampled stain with a few broken fence rails.
Field states, if wanted later, would be more `idle`-like animations chosen
by `resource / base`; not built.

## Docks

The dock stands on the shore with water under it. The new water is drawn by
the ground (`docs/look/ground.md`), so the dock's sprite must be
transparent wherever water should show: the model has no water, sea bed or
ground of its own, its piles end at z = 0 (the water's surface), and the
shadow catcher at z = 0 gives its shadow on the water as translucent black.
Its rubble should be floating planks rather than a heap: a dock declares
`stages="flat"` until stages.py has a water rubble.

## Smoke, flags and fire

Built for smoke and fire; flags are built through the pipeline but no
building uses them yet (see "Flags" below).

### Attachment points

A building's model places named empties, `lib.attach(kind, loc, parent)`
(`attach_smoke`, `attach_smoke_dark`, `attach_flag`, `attach_fire`), in
its own frame. `export.py` projects them into the cell and writes them
into the manifest, in authored pixels like the anchor, for the standing
(idle) look only:

```ron
    attach: [
        (kind: Smoke, at: (148, 60)),        // a hearth's or chimney's mouth
        (kind: Fire, at: (100, 150)),        // where it burns, in the order fires catch
    ],
```

- **Smoke** (`SmokeDark` for a forge) is lifted, if the model hides the
  point from the camera, until it is seen: smoke from a courtyard hearth
  rises from behind the wall in front of it.
- **Fire**: unless the model places its own `attach_fire` empties, the
  exporter finds them: the camera's rays every 4 px across the cell, kept
  where they land on the building's own mass (above 0.4 of a unit, facing
  the camera, not a pole, a rod, a sheet such as a cloth or door, a plant
  or anything in the player's colour) with neighbours 12 px round that do
  too; the first nearest the upper middle of the mass (the roof), each next
  the farthest from those chosen. How many: 2 for a 1 x 1 building, 3 for
  2 x 2, 4 for 3 x 3, 5 for a Wonder, 1 for a wall post or gate, none for a
  farm or a resource; `fires=N` in a `SETS` entry overrides (docks: 3).
- **Flag** points are written only for a model built without its own cloth
  (kwargs `"flags": False`); none is, so none is written yet.

Who smokes now (placed by hand in the four architecture modules):

| | house | town centre | temple | siege workshop (dark) |
|-|-|-|-|-|
| Greek | the ridge (smoke out through the tiles) | the hall's ridge | the altar's fire | a smith's hearth under the frame |
| Egyptian | the yard's bread oven | a hearth on the upper roof | incense, from the sanctuary's roof | the shed |
| Mesopotamian | the roof terrace | the gate's brazier | the terrace's brazier | under the reeds |
| Asian | the kitchen's chimney | the incense in the ding | the incense in the ding | the shed |

No store, market, stable, range, academy, government centre, dock, tower
or wall smokes; every building with stages can burn.

### Points without re-rendering

`export.py --points-only` (and `export_all.py --points-only`, every set,
stale or not, a few seconds each) rebuilds the model, checks it still
renders what was rendered, and rewrites only the manifest's `attach` and
its export hash. The check is `// export-geometry: F` in the manifest's
header, written by every export: a hash of every mesh's evaluated vertices
(world space, to 1e-4), face and loop counts, shading, render and shadow
flags and materials, and every material's node values and links, over the
idle and the extra animations (the stages are made from the idle by
`stages.py`, which the export hash covers). An empty draws nothing and is
not in it, so adding `lib.attach` calls changes the module's source (every
set it draws goes stale) but not the fingerprint; any other change does,
and `--points-only` then fails (exit 3): that set needs a full export.
`--adopt` took the fingerprints of the sets exported before there were any,
allowed only while their hash was up to date.

### The effects (`tools/look/fx.py`)

Colour sets of their own, filed under `sprites::FX + i` as the scenery is
(`FX_SETS`), loaded with every architecture. Each is a looping `idle`
(declared as `anims={"idle": [{"frame": i} ...]}` with `timing`), class
`SmallBuilding`, its anchor the attachment point; its builders set
`no_ground` (no shadow catcher: an effect casts no shadow and the flag hangs
below its origin). Smoke and fire are billboards: discs in the camera's
plane, Principled with a black base and their colour as emission, alpha a
soft rim broken by noise, so the sheet's alpha is soft and straight.

| set | frames | ms | what |
|-|-|-|-|
| `fx_smoke` | 16 | 125 | eleven grey-white puffs, each born every 1/11 of the loop, rising 2 units, swelling, drifting right, thinning; shaded as balls lit from the sun's side |
| `fx_smoke_dark` | 16 | 125 | the same, sooty |
| `fx_fire_small` | 8 | 100 | six flame tongues (three stacked discs each: yellow core, orange, red rim) under half a tile across, a breathing glow, a wisp of dark smoke |
| `fx_fire_large` | 8 | 100 | three clusters of taller tongues, a wider glow, dark smoke on the fire's loop |
| `fx_flag` | 8 | 125 | a flag's cloth (player colour, gold stripe) waving from a pole's top; masked |

Every loop divides the renderers' clock's period (512 s), so it wraps
unseen.

### In the view

`Scene::build_full` (`SceneOptions::time`: the renderers' clock, seconds,
the water's; 0 in mapview unless `--time`, and in tests) draws, for every
standing finished building (not a site, not falling, not rubble, not a
building remembered in the fog), an effect sprite per attachment point, at
the building's ground point plus the point's offset from its anchor, at
the building's depth plus `FX_DEPTH` (0.03, a thousandth more per point),
`slot = u32::MAX` (never picked), the frame at `time` plus a phase by a
hash of the building's tile and the point, so no two chimneys puff in
step. Smoke always; the flag always, in the owner's colour; fire by health
(`scene::fires`, `BURNS_BELOW`): under 66% one small fire at the first
point, under 50% two, under 33% every point with the first two large, under
15% every one large. The old art draws no fire, and has no attachment
points, so with the new art off nothing changes. The app passes its GPU
clock in a match, the editor and the title's diorama.
`mapview --scenario burning` is the town with five buildings hurt to each
step by player 1's axemen, spawned beside them and taken away again.

### Flags (future work)

The models keep their static flags: animating them would draw the cloth
twice. To animate them: give the flag helpers (`greek.flag`,
`egyptian.flagpole`, `asian.flag`) an `attach_flag` empty at the pole's top
and a `flags` keyword that leaves the cloth out, set `"flags": False` in the
`SETS` kwargs, and re-export every set in full (about 2.5 hours at `--jobs
2`); the exporter then writes the flag points and the view draws `fx_flag`
there with no further change. Chimney pots for the Greek houses and town
centre (which smoke through their roofs) want the same re-export.

## Memory

The effects add five small sets: 4.9 MiB of colour pages (RGBA and mask,
frames trimmed one by one): each smoke 1.7, the large fire 1.0, the small
0.4, the flag 0.06.

Colour frames are trimmed one by one in the atlas, so a set costs the sum
of its frames' drawn rectangles at 5 bytes a pixel (RGBA and mask).
Measured on the five Greek sets with their stages (phase 3):

| set          | class  | cell (trimmed) | 5 frames |
|--------------|--------|----------------|----------|
| house        | 2 x 2  | 326 x 284      | 1.42 MiB |
| barracks     | 2 x 2  | 340 x 272      | 1.49 MiB |
| temple       | 2 x 2  | 326 x 264      | 1.39 MiB |
| watch_tower  | 1 x 1  | 276 x 308      | 0.99 MiB |
| town_center  | 3 x 3  | 530 x 434      | 3.30 MiB |

The colour pages for everything in `assets/look` went from 2048 x 768
(7.5 MiB, the five buildings with one frame each, the trees and the
scenery) to 2048 x 1408 (13.8 MiB).

Projection for 18 kinds in one architecture: eleven 2 x 2 kinds at about
1.45 MiB, two 3 x 3 (Town Center, Dock) at 3.3, the Watch Tower at 1.0, the
two walls (8 arm frames more) and the gate (8 gate frames more) at about
1.8 each, the 5 x 5 Wonder at about 9: some 38 MiB. Measured with all four
architectures exported: one 8192 x 3840 page, 150 MiB with masks (8192 x
4096, 160 MiB, once the walls' shadows are cut apart), before the units'
colour sets.

### Only the architectures in play

So the atlas holds the colour building sets of the architectures of the
civilisations in play and no others, plus the first architecture's where
one of theirs lacks a look of a kind (the fallback of "Which look") or a
side has no civilisation (it builds in the first), plus everything with no
architecture: trees, scenery, resources, and the units the sides can have
(`docs/look/units.md`, "Memory"). With every architecture exported, the
Egyptians against the Shang hold no Greek building.

- `view::sprites::Architectures` is the set (`of_match(sim)`, `of_civs`,
  `of_sides`, `FIRST`, `ALL`); `keeps(name)` says whether a colour set is
  of an architecture in it. `view::sprites::Loadout` is it with the
  match's units (`sprites::Units`): what the atlas holds.
- `sheets::load_art_for(sprites, look, new_art, loadout)` reads every
  manifest first and decodes only the images of the sets
  `sprites::worth_decoding` picks (`load_art` is it with `Loadout::ALL`):
  the colour sets the loadout keeps, the first architecture's where it is
  a fallback, and an indexed set only if no kept colour set replaces it,
  which with the new art on is none of `assets/sprites` (with it off,
  every one, as before). The images are decoded on
  every core (`sheets::par_map`, std threads, the biggest first), and
  `Atlas::with_sheets` cuts the sets' frames out in parallel too, in the
  sets' order, so the atlas is the same whatever the threads do.
- The game (`crates/app`): `App::art` is the loadout the atlas holds with
  the new art on (as the settings had it at start; `None` with it off, and
  then nothing here happens). The game starts with an empty atlas and
  `load_settings` loads the art the settings ask for, so with the new art
  on the old art is never decoded or packed at all.
  `App::ensure_art(want)` rebuilds the atlas when the loadout differs and
  re-uploads it (`render::Renderer::upload_atlas`, new
  textures and bind group), and rebuilds the diorama's scenery, whose
  sprites point into the pages. Called from `enter_match` (a skirmish, a
  campaign scenario, a loaded save, a replay, a playtest) with the match's
  loadout, before the world's scenery is made, in the moment the game
  already takes to set the match up; after every tick,
  `App::ensure_world_art` loads the set of a unit that turns up without
  one. The editor asks for everything (a side's civilisation, and any
  kind, is a click away); the title's diorama has no civilisations, so it
  needs the first architecture and the units standing in its world
  (`Loadout::of_world`), which a match's atlas usually covers: then it
  does not reload on the way back to the title. At start, with the new art
  on, the first architecture and every unit are loaded.
- `mapview` loads everything, as the goldens always have, unless `--civ`
  or `--enemy-civ` names the civilisations: then only theirs.

Measured (release build, 4 cores shared with an export running; median of
three; the decode includes the indexed sets, which every load reads):

| match | colour pages | decode | atlas | total | peak RSS |
|-------|--------------|--------|-------|-------|----------|
| before: everything, one thread | 8192 x 3840, 150 MiB | 1030 ms | 490 ms | 1.5 s | 857 MB |
| Greek only (start, diorama) | 4096 x 2368, 46 MiB | 335 ms | 196 ms | 0.53 s | 445 MB |
| Greek v Egyptian | 8192 x 2240, 87 MiB | 415 ms | 260 ms | 0.68 s | 584 MB |
| Egyptian v Shang (+ Greek) | 8192 x 3136, 122 MiB | 529 ms | 334 ms | 0.86 s | 734 MB |
| all four (editor) | 8192 x 4096, 160 MiB | 636 ms | 396 ms | 1.03 s | 874 MB |

On one thread the same loads take 0.82, 0.96, 1.17 and 1.47 s: the decode
is bound by allocating and faulting in the decoded pages as much as by
inflating them, so four threads give about 1.5 times, not 4. The
decoded sheets are dropped once the atlas is built; the peak is theirs and
the atlas's together. Tested: `a_match_holds_only_the_buildings_of_its_architectures`,
`switching_matches_swaps_the_architectures_loaded`, and the GPU's
`a_colour_frame_draws_as_the_rasteriser_draws_it` (with the atlas uploaded
in place of another).

### Everything in colour

With every kind in colour (130 sets: four architectures' buildings, every
unit, trees, resources, scenery, effects), measured by
`crates/view/tests/atlas_memory.rs` (release, median of three, 4 shared
cores; run it with `cargo test --release -p view --test atlas_memory --
--ignored --nocapture`). Sprite textures in MiB, indexed pages plus colour
pages (RGBA and mask); "before" is the atlas as it was when every kind first
had a colour set:

| load | before | now | after the units' re-export |
|------|--------|-----|----------------------------|
| new art off | 128.0 | 128.0 (unchanged) | 128.0 |
| title, diorama (Greek, every unit) | 161.0 | 148.0 | 138.0 |
| Greeks only (two Greek sides) | 161.0 | 138.0 | 128.0 |
| Greeks v Egyptians | 193.5 | 185.5 | 175.5 |
| Egyptians v Shang | 228.5 | 183.0 | 175.5 |
| four architectures (editor) | 263.5 | 258.0 | 248.0 |

Where it went: the indexed pages with the new art on were already only the
UI's frames (8192 x 128, 1 MiB), but every indexed set was still decoded
and the game built the whole old atlas at start before the settings
switched to the new; now neither happens, and the indexed page is 1024 x
512. The skyline packer fills the colour pages to 92-93% (shelves, 86-88%).
A match of two Greek civilisations loads no chariot or horse archers; a
match with no Greek side loads no Greek building its architectures do not
fall back on. Loading (decode and pack, median of three runs of three, on four shared cores) went
from 0.42 to 0.36 s for the title, 0.52 to 0.47 s for Greeks against
Egyptians, 0.65 to 0.39 s for Egyptians against Shang and 0.68 to 0.55 s
for all four: the decode is a third shorter, the skyline packs in about
the time the shelves did. The start with the new art on also no longer
builds the old atlas first (another 0.21 s). The last column projects the
units exported without decay frames (`decay: Synth`, `docs/look/units.md`, "Memory";
`sprites::tests::projected_memory_without_decay_frames`). On a GPU
limited to 4096 the same frames take 2 to 4 pages of 4096 wide, 1 to 3 MiB
more.

What is left is pixels: for Greek against Egyptian, 171 MiB of trimmed
frames (165 after the re-export), 96.5 MiB of them units', of which 23.3
are the death's lying frames. Below about 170 MiB for two architectures,
with nothing given up, takes compressed textures:

- **BC7** for the RGBA pages (wgpu's `TEXTURE_COMPRESSION_BC`: desktop
  GPUs under Vulkan, D3D12 and Metal on macOS; not most mobile GPUs, nor
  WebGL) is a quarter of RGBA8, and the mask as **BC4**
  half of R8: Greek against Egyptian would take about 50 MiB, not 175.
  BC7 handles soft alpha well (mode 6, separate per-block endpoints) but
  not perfectly: a 4 x 4 block's colours lie on one or two lines, so a
  block where a unit's cloth meets its skin or the shadow meets a bright
  edge shifts by a few levels, and the player-colour path reads the
  sheet's grey through `luma * 7` to pick a ramp step, which magnifies a
  luma error of 1/255 into 7/255 of a ramp step: likely a visible
  shimmer between frames on the masked cloth. Doing it needs: an encoder
  (`intel_tex_2`, ISPC kernels with prebuilt binaries, at load time costs
  CPU seconds for 170 MiB even at its fast setting (not measured here); or
  a build step writing compressed sheets beside each set, which then have
  to be packed in 4 x 4 blocks with frames on 4-pixel boundaries); a fallback to RGBA8 where the feature is
  missing; the rasteriser decoding the same blocks (or the look goldens
  comparing across a tolerance) so `a_colour_frame_draws_as_the_rasteriser_draws_it`
  still holds; and picking (`Atlas::solid_at`) reading alpha from the
  compressed or a separate copy. Recommended only if the budget must
  drop well below 170 MiB: then the mask uncompressed (it is the player
  colour) and BC7 at the slow setting for RGBA, built offline per sheet
  so the game does not encode, with a check that the masked cloth's luma
  error stays under 1/255.
- Smaller, lossless: packing the frames that use the mask first and
  giving the mask texture only their rows would save the mask under the
  rest, 7 MiB of 175 (80% of the frames' area has some player colour).
  Not done.

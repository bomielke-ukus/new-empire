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

## Smoke, flags and fire (designed, not built)

A building declares attachment points in its manifest, in authored pixels
in the cell like the anchor, one list per animation frame they apply to:

```ron
    attach: [
        (kind: Smoke, at: (148, 60)),        // a chimney's mouth
        (kind: Flag, at: (120, 12)),         // a pole's top: the cloth hangs from it
        (kind: Fire, at: (100, 150)),        // where it burns when damaged
    ],
```

`export.py` would find them from empties named `attach_smoke`,
`attach_flag`, `attach_fire_<n>` in the model (projected with `project()`,
then moved by the trim). Effects are small colour sets of their own,
`fx_smoke`, `fx_flag`, `fx_fire` (a looping `idle` of 8 to 12 frames, the
flag's cloth masked for player colour), filed under ids of their own as the
scenery is. The scene pushes one sprite per attachment of a standing,
finished building, anchored at the building's ground point plus the
attachment's offset from the building's anchor, at the building's depth
plus a small epsilon, its clock offset by the slot so they do not breathe
in step: smoke always, the flag always (its frames replace a modelled
flag, so the model draws only the pole), fire at the first `n` points as
health falls below 75%, 50%, 25%. Nothing of this is built.

## Memory

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
civilisations in play and no others, plus the first architecture's (the
fallback of "Which look", and the look of a side with no civilisation),
plus everything with no architecture: trees, scenery, units, resources.

- `view::sprites::Architectures` is the set (`of_match(sim)`,
  `of_civs`, `FIRST`, `ALL`); `keeps(name)` says whether a colour set is
  loaded with it: one with no architecture suffix, or whose suffix is in
  the set.
- `sheets::load_art_for(sprites, look, new_art, archs)` reads every
  manifest but decodes only the images of the sets kept (`load_art` is it
  with `ALL`); indexed sets are always loaded. The images are decoded on
  every core (`sheets::par_map`, std threads, the biggest first), and
  `Atlas::with_sheets` cuts the sets' frames out in parallel too, in the
  sets' order, so the atlas is the same whatever the threads do.
- The game (`crates/app`): `App::art` is the set the atlas holds with the
  new art on (as the settings had it at start; `None` with it off, and then
  nothing here happens). `App::ensure_art(want)` rebuilds the atlas when the
  set differs and re-uploads it (`render::Renderer::upload_atlas`, new
  textures and bind group), and rebuilds the diorama's scenery, whose
  sprites point into the pages. Called from `enter_match` (a skirmish, a
  campaign scenario, a loaded save, a replay, a playtest) with the match's
  architectures, before the world's scenery is made, in the moment the game
  already takes to set the match up. The editor asks for all four (a
  side's civilisation changes there at a click); the title's diorama has no
  civilisations, so it needs only the first, which every atlas holds: it
  never reloads on the way back to the title. At start, with the new art
  on, only the first is loaded.
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

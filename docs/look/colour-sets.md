# Colour sprite sets (the revamp, phase 1)

The new look's sprites are full colour with soft edges and shadows. They
live beside the old indexed sets and replace them one name at a time.

## Where

`assets/look/<name>/<name>.ron`, with its images beside it. `<name>` is a
set name exactly as in `assets/sprites` (`house`, `town_center_bronze`,
`tree`, `hoplite`, ...). When the new art is switched on, a colour set
replaces the indexed set of the same name, and a kind with any colour set
loses all its indexed sets, of every age and architecture: it is drawn from
its colour sets only (`docs/look/buildings.md`, "Which look"). Every other
kind is drawn as before. Switched off (the default until the art is
complete), `assets/look` is not read at all.

## The manifest

The same `SpriteSet` as an indexed set, with three more fields:

```ron
SpriteSet(
    name: "house",
    class: MediumBuilding,
    scale: 2,
    palette: "rgba",                 // not checked for a colour set
    sheet: "house.png",
    anchor: (150, 210),
    animations: [
        (name: "idle", frames: 1, frame_ms: 160, loops: true, impact: None),
    ],
    anchor_overrides: [],
    colour: true,                    // default false: an indexed set
    mask: Some("house.mask.png"),    // default None: nothing takes player colour
    frame: Some((300, 260)),         // default None: the class's size x scale
)
```

- **`sheet`**: an 8-bit RGBA PNG, straight (not premultiplied) alpha. Laid
  out as an indexed sheet is: one row per (animation, facing) with the
  animations in manifest order and, for a class that turns, the five facings
  S, SW, W, NW, N within each; one column per frame, as many columns as the
  longest animation (a shorter one's other cells transparent); every cell
  `frame` pixels, one anchor for all. A unit's are its old set's
  (`docs/look/units.md`). A building's animations are
  `construction` (3), `idle`, `rubble` (1), and a wall's `arm` (8) or a
  gate's `shut` and `open` (4 each): `docs/look/buildings.md`.
- A comment line `// export-hash: H` in the header is `export_all.py`'s
  record of what the set was exported from; the game ignores it.
- **`frame`**: the cell size in authored pixels, when the class's size
  (`docs/05` §2.3) times `scale` is too small (a building with a flag).
  `anchor` is in the cell, in authored pixels, as before.
- **`scale`**: 2. A tile is 128 x 64 authored pixels. A unit's set may be
  authored at 1, half the resolution (`docs/look/units.md`).
- **`attach`** (default none): a building's attachment points for smoke,
  flags and fire, `[(kind: Smoke | SmokeDark | Flag | Fire, at: (x, y))]`
  in authored pixels in the cell (`docs/look/buildings.md`, "Smoke, flags
  and fire"). A comment `// export-geometry: F` in the header fingerprints
  the model it was rendered from, for `export.py --points-only`.
- **`mask`**: an 8-bit greyscale PNG the size of the sheet. 0 is ordinary
  colour; 255 takes the owner's colour; between, a blend (antialiased
  edges).
- **`decay`** (default `Frames`): a unit's `decay: Synth` says the sheet has
  no rows (nor columns) for the `decay` animation, which stays in
  `animations` so its timing is the old set's; the view draws the death's
  last frame instead, fading and sinking (`docs/look/units.md`, "Memory").
  The other animations' rows close up in order. `check_set.py` and the
  loader accept both forms.

## Trees

The forest tree has several looks: the set `tree` and the sets `tree_1`,
`tree_2`, ... (any of 1 to 99 present), each one idle frame. With more than
one, each tree wears one picked by a hash of its tile, the same every frame
and every run, and is drawn up to 0.16 tile off its tile's centre each way
(by another hash of the tile), its depth moving with it, so a forest is a
mix and its rows do not line up. The tree in the simulation does not move:
only its picture. A tree remembered out of sight, and a tree falling, are
drawn in the same look at the same place. With only `tree` (the old art),
every tree is it, at its tile's centre.

The looks now (`tools/look/decor.py`, `TREES`): 0 a round broadleaf, 1 a
tall oval, about 20% taller and cooler green, 2 a low spreading oak, warmer,
3 a two-headed tree, deep green, 4 an olive, silvery grey-green.

## Player colour

Where the mask is set, the sheet's RGB is a grey: how brightly that cloth,
shield or flag is lit, with no hue of its own. The game colours it from the
owner's eight-step ramp (palette indices 240 to 247 of the owner's row):

```text
l    = (0.299 R + 0.587 G + 0.114 B) / 255          // 0..1
t    = clamp(l, 0, 1) * 7
tint = lerp(ramp[floor(t)], ramp[min(floor(t) + 1, 7)], fract(t))
out  = lerp(RGB, tint, mask / 255)
```

computed in sRGB bytes as the palette stores them, the same in the GPU
shader and the software rasteriser. Nature's things (row 0) use row 0's ramp.

## Alpha, shadows and fog

Alpha is coverage: soft edges, and the object's own shadow on the ground as
translucent black. A colour frame is blended over what is behind it with
ordinary "source over". The fog's light (`SpriteInstance::light`) scales the
colour as it does an indexed frame's. Frames are trimmed to their
non-transparent bounds when packed, the anchor moving with the trim.

## In the atlas

Colour frames are packed into their own pages: RGBA8 and an R8 mask, sized
by the colour frames alone and not by the indexed pages. Frames go on
tallest first, each where its top comes lowest on the first page it fits
(a skyline: a short frame fills the gap beside a tall one rather than
starting a shelf of its own), a pixel apart. A page is as wide as the
widest frame needs, rounded up to a power of two, and wider (up to the
largest side) before there is a second page; as tall as its content, to a
multiple of 64. When one page of the largest side is not enough, all pages
(a texture array's layers are alike) are that wide and as short as still
fits the frames on as few pages. `Frame::page` and `SpriteInstance::page`
carry bit 7 (`0x80`) for a colour page, the page number in the low bits, so
nothing that builds a sprite instance from a frame changes.

The largest side is 8192, or what the GPU allows if less
(`max_texture_dimension_2d`): the game asks the device for the adapter's
own limit, and on a GPU that allows less than the atlas measures builds it
again for that limit before uploading it (`Atlas::with_sheets_max`, down
to 2048); the frames are the same, on more and smaller pages
(`a_smaller_texture_limit_packs_the_same_frames_onto_smaller_pages`).

With the new art on, the indexed pages hold only what no colour set
replaces: the UI's procedural frames (solid fills, the font's glyphs in
three inks, the player symbols, selection rings, placement footprints and
the age-up glow), the arrow and the stone in flight, the generic
construction pegs and rubble (fallbacks no colour building uses), and a
placeholder for a kind with no colour set (none now, and a unit left out of
the match, `docs/look/units.md`, "Memory"). No indexed set under
`assets/sprites` is decoded at all: every one is of a kind with colour sets
(`sprites::worth_decoding`). They are sized as the colour pages are, about
square (1024 x 512, half a MiB), not a page 8192 wide. With the new art off
the indexed pages are exactly as before.

What the buildings cost there, and what is loaded for a match, is in
`docs/look/buildings.md`, "Memory".

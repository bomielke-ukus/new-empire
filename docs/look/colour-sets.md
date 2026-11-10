# Colour sprite sets (the revamp, phase 1)

The new look's sprites are full colour with soft edges and shadows. They
live beside the old indexed sets and replace them one name at a time.

## Where

`assets/look/<name>/<name>.ron`, with its images beside it. `<name>` is a
set name exactly as in `assets/sprites` (`house`, `town_center_bronze`,
`tree`, `hoplite`, ...). When the new art is switched on, a colour set
replaces the indexed set of the same name; every other set is drawn as
before. Switched off (the default until the art is complete), `assets/look`
is not read at all.

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
  S, SW, W, NW, N within each; one column per frame; every cell `frame`
  pixels.
- **`frame`**: the cell size in authored pixels, when the class's size
  (`docs/05` §2.3) times `scale` is too small (a building with a flag).
  `anchor` is in the cell, in authored pixels, as before.
- **`scale`**: 2. A tile is 128 x 64 authored pixels.
- **`mask`**: an 8-bit greyscale PNG the size of the sheet. 0 is ordinary
  colour; 255 takes the owner's colour; between, a blend (antialiased
  edges).

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

Colour frames are packed into their own pages: RGBA8 and an R8 mask, the
same size as the indexed pages. `Frame::page` and `SpriteInstance::page`
carry bit 7 (`0x80`) for a colour page, the page number in the low bits, so
nothing that builds a sprite instance from a frame changes.

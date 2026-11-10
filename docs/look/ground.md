# The ground (the revamp, phase 2)

With the new art on, the ground is painted from textures instead of flat
blended colours: one tiling texture per ground type, blended where types
meet along a wobbling edge, water that moves and foams at the shore, and
small scenery scattered over it. Off, the ground is drawn as before.

## Textures: `assets/look/terrain/`

One 8-bit RGB PNG per ground type, named for `sim::Terrain` in its order:
`grass.png`, `dirt.png`, `desert.png`, `sand.png`, `shallow.png`,
`deep.png`, `forest.png`, `snow.png`.

- **512 x 512, covering 4 x 4 tiles**, tiling seamlessly on both axes. The
  texture lies on the ground plane: texel `(u, v)` is world tile position
  `(x, y)` with `u = 128 x mod 512`, `v = 128 y mod 512`. On screen it is
  seen through the game's projection, so a round pebble in the texture is
  drawn twice as wide as tall; paint for that.
- **Lit as flat ground in the sun**, in the colours of the target picture
  (`docs/look/target-picture.png`). The renderer multiplies by the slope's
  shade and the fog's light and adds nothing else.
- `noise.png`: 256 x 256, 8-bit grey, tiling, covering 8 x 8 tiles; smooth
  cloudy noise spanning the full range. It wobbles the edges between types
  and breaks up the foam.

## Blending

Each pixel of ground knows its world position `(x, y)` in tiles. The types
of the four tiles whose centres surround it are blended bilinearly, after
the position is pushed about by the noise (up to 0.35 of a tile), so the
edge between grass and sand wanders instead of following the tile grid.
Each type's colour is its texture at the pixel's own (unwobbled) position.

## Water

`shallow` and `deep` are ground types like the others, and blend the same
way, so the shallows shade into the deep. Over them:

- **Movement**: the water textures are sampled twice, drifting slowly in
  two directions with time, and averaged.
- **Foam**: where water meets land (the blended weight of the two water
  types between about 0.35 and 0.6), a white line, broken by the noise and
  breathing in and out with time.

Time is the frame's, in seconds; pictures for tests are drawn at time 0.

## Scenery

Colour sets named `decor_<kind>_<n>` (`flowers`, `tuft`, `rock`, `bush`,
`reeds`), one idle frame each, are scattered by the view over ground of the
right type from the map's seed. They are pictures only: not in the
simulation, not selectable, drawn in depth order with everything else, and
left out where a building, a tree or a resource stands.

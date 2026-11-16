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
  shade, the fog's light and, on land, the broad tint (below).
- **Even at the scale of its repeat**: a land texture has little variation
  over more than about a tile (`tools/look/ground.py` damps what varies
  over more than a tile to 30%), so its 4-tile repeat does not show as a
  checkerboard from afar. The broad tint gives the land that variation
  instead, without a period.
- `noise.png`: 256 x 256, 8-bit grey, tiling, covering 8 x 8 tiles; smooth
  cloudy noise spanning the full range. It wobbles the edges between types,
  tints the land broadly, and sways and breaks up the foam. Read at other
  scales, it is the same texture stretched (0.13 x is 61 tiles across).

## Blending

Each pixel of ground knows its world position `(x, y)` in tiles. The types
of the four tiles whose centres surround it are blended bilinearly, after
the position is pushed about by the noise (up to 0.35 of a tile), so the
edge between grass and sand wanders instead of following the tile grid.
Each type's colour is its texture at the pixel's own (unwobbled) position.

## Broad tint

Every land type's colour (not the water's) is multiplied by a tint that
wanders over tens of tiles: `m`, from -1 to 1, is the noise read at 0.45 x
(18 tiles across) weighted 0.6 plus the noise at 0.13 x (61 tiles) weighted
0.4, centred; the tint is `1 + 0.07 m` on green, `+ 0.02 m` more on red and
`0.03 m` less on blue, so the brighter stretches are a little warmer. At
most about 7% either way: enough to break the textures' repeat from afar,
too little to read as patches up close.

## Water

`shallow` and `deep` are ground types like the others, and blend the same
way, so the shallows shade into the deep. Over them:

- **Movement**: the water textures are sampled twice, drifting slowly in
  two directions with time, and averaged.
- **Foam**: surf on the water side of the shore, up to 85% opaque at its
  core (a slightly blue white, 0.90 0.96 0.99), with a thin brighter line
  of white along its leading edge, nearest the shore, broken into lace by
  fine noise. It follows the water *smoothed*: the mean of the water's
  blended weight at four points 0.45 tile off diagonally, around the point
  the edge's wobble pushes this one to, swayed a further 0.3 tile at most
  by the noise read at 0.27 x (30 tiles across). So a coast's corners
  round off and the foam does not trace the tile grid. With `calm` that
  smoothed water: the body rises over 0.44 to 0.52 and fades out from 0.70
  to 1.00 (seaward); the edge line rises over 0.40 to 0.46 and is gone by
  0.52 to 0.62. The body is kept off dry ground (it fades in as the pixel's
  own water goes from 0.2 to 0.55), is broken into patches by the noise at
  two scales (3.1 x and 0.9 x, mixed 0.6 to 0.4, kept from 0.22 to 0.46)
  and thinned a third by lace (the noise at 7 x, 0.25 to 0.6); the edge
  line is kept where lace (0.7) and the broad noise (0.3) mix above 0.12
  to 0.36. Alpha is the larger of body x 0.85 and edge x 0.95; the colour
  runs from the core's to white as the edge line takes over. The band
  breathes in and out by 0.04 of water weight every 4 seconds. Ramps are
  `smoothstep`.

Time is the frame's, in seconds; pictures for tests are drawn at time 0.

## Scenery

Colour sets named `decor_<kind>_<n>` (`flowers`, `tuft`, `rock`, `bush`,
`reeds`), one idle frame each, are scattered by the view over ground of the
right type from the map's seed. They are pictures only: not in the
simulation, not selectable, drawn in depth order with everything else, and
left out where a building, a tree or a resource stands.

- **Quiet**: the flowers are dusty, muted colours (faded brick, straw,
  cream, lilac, slate blue), a step darker than pure, so they sit in the
  grass and do not compete with the units' player colours.
- **Not food**: the `bush` sets are low, flat, dull scrub (grey-green sage,
  dry maquis, dark juniper) with bare twigs and no flowers or dots, unlike
  the berry bush, a round bright green ball dotted red, which is a resource.
- **Not confetti**: below zoom level 0.75 (the 0.5 x zoom, display scale
  aside) the smallest things, flowers, tufts, pebbles and small rocks, are
  left out; the boulders, scrub and reeds stay. The view decides this when
  it adds the scenery to a frame (`Scenery::decorate`, from the camera).

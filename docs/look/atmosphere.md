# The atmosphere (the revamp, phase 5)

With the new art on (`Settings::new_art`) and its atmosphere on
(`Settings::atmosphere`, on by default, the settings screen's ATMOSPHERE
row, shown only with the new art), the picture is drawn as one sunlit
place: never-seen ground is a soft ink-blue haze, remembered ground a cool
faded memory, the ground fades into the haze past the map's edge, cloud
shadows drift over everything, deep water glints, and a gentle grade warms
the world. Off (either setting), every frame is drawn exactly as before:
the non-look golden images and the look ones drawn with `--atmo 0`
(`look-battle-flat`) are unchanged by it.

The maths is written once, in `crates/view/src/atmosphere.rs`; the
software rasteriser calls it (`raster::draw_backdrop`,
`raster::draw_ground_with`, `raster::draw_sprites_with`), and the GPU does
the same sums in `crates/render/src/shaders/` (`common.wgsl`: memory and
grade; `ground.wgsl`: haze, feather, clouds, glints, backdrop;
`sprites.wgsl`). A unit test checks the shaders' constants against the
module's; `crates/render/tests/headless.rs`
(`the_atmosphere_draws_as_the_rasteriser_draws_it`) checks the GPU's
frame against the rasteriser's (mean channel delta about 0.4, under 1% of
pixels more than 6 apart).

Everything is a function of world position, the window position (the
vignette) and the renderers' clock (`ground::wrap_time`, `--time` in
mapview, 0 in tests), every drift a whole number of cycles in its
512-second period. Nothing is random and nothing reads the wall clock.

All noise is `assets/look/terrain/noise.png` (`docs/look/ground.md`),
read at other scales.

## Fog of war

The fog's light per tile corner (`view::fog`) is interpolated over the
ground as before: 0 never seen, 0.5 seen once, 1 in sight.

- **Haze** where the light is 0: a mix of ink blue (0.095, 0.105, 0.15)
  and dusky grey (0.145, 0.14, 0.165) by the noise at about 20 and 6 tiles
  across, in world space, so it pans with the map. Exactly the haze,
  whatever the ground under it.
- **Feather**: known ground rises out of the haze as the light goes from 0
  to 0.5, after the noise (about 3 tiles and 1 across) takes up to 0.4 off
  the light. So the edge wobbles instead of following the tile corners,
  and only ever onto ground the player knows: where the light is 0 the
  haze is all there is. Nothing is revealed that the fog rules did not
  already show (tested over random fog states:
  `raster::tests::the_atmospheres_fog_leaks_nothing_of_ground_never_seen`).
- **Memory** at light 0.5: the colour kept 40% saturated toward its luma,
  times (0.55, 0.59, 0.71): dimmer, greyer and cool, so it reads as
  remembered rather than as merely dark. Between 0.5 and 1 it blends to the
  colour itself. Buildings and nodes drawn from memory (sprite light 128)
  get the same tone.

## The map's edge

A backdrop pass draws first, under the ground: a triangle over the whole
window, each pixel's point on the ground plane (elevation 0). On the map,
the haze (the ground covers it). Past the edge, the ground goes on as the
map ends (types clamp), in the fog's light at the nearest point of the
edge (so ground beside a never-seen edge tile is haze), faded out over 1.6
tiles, the distance stretched by the noise so the fade is ragged.

## Cloud shadows

Two reads of the noise, 32 and 40 tiles across, drifting toward the
world's -x (up and left on screen) at 0.06 and 0.08 tiles a second, mixed
0.55 to 0.45; the shadow rises over 0.54 to 0.68 of it, darkening by 15%
at most, on about a quarter of the ground. Applied to ground in sight
(scaled down to none on memories, and not on the haze). A world sprite
takes the shadow at its foot (the middle of its box's bottom edge,
unprojected), computed on the CPU once per sprite
(`atmosphere::sprite_cloud`) and handed to the GPU in the sprite's
instance data, so a building is never brighter than the grass under it.

## Grade

On the world only (the ground, the backdrop and world sprites: never a
screen-space sprite, the HUD or the minimap): saturation up to 14% more at
mid-grey (none at black or white); highlights warmed by factors up to
(1.035, 1.01, 0.955) and shadows cooled by (0.96, 1, 1.06), by factors so
black stays black and white marble is not pushed past white; a vignette
darkening the corners 6% at most. Applied per layer before blending,
which differs from grading the finished frame only at translucent edges.

## Glints

On deep water only, weighted by its share of the blend: two fine reads of
the noise (0.6 and 0.85 tiles across, drifting with the water two ways)
must both be high, and a broad one too, so the glints are sparse, small
and never line up; up to 55% white.

## Combat feedback and life

Colour sets from `tools/look/fx_combat.py` (a module of its own, so the
effects of `fx.py` keep their export hashes), exported by `export_all.py`,
filed under `sprites::FX_COMBAT` and drawn by number
(`Atlas::combat_fx`):

- `fx_arrow`: an arrow in flight, frame `k` toward facing `k`, its
  fletching in the owner's colour, in place of the old pixel arrow.
- `fx_stone`: the siege stone, tumbling.
- `fx_spark`: the burst where a blow lands, in place of the cross of
  squares.
- `fx_dust`: the puff of a kill, a hammer (smaller) and a building's fall
  (a ring of them, larger), in place of the squares; and **hoof dust**
  behind mounted units on the move, a puff every 0.56 s.
- `fx_birds`: a small flock crossing now and then, one flock per 48 x 48
  tiles, 40 seconds of every 128, placed from the map's seed and the
  clock; drawn only where the ground under it and under where it shows on
  screen is in sight.

Without the new art these sets are not loaded and the old art is drawn.
They are drawn with the new art whether or not the atmosphere is on.

## Not done

- No ambient-occlusion blobs: every unit, tree and resource set already
  bakes its shadow.
- No darker band or reflection under ships and docks; no butterflies or
  leaves.

## Cost

Software path (`mapview`, 1920 x 1080, whole process including loading):
the Greek town 1.19 s to 1.52 s; a fogged start 0.96 s to 1.67 s (the
haze and backdrop cover most of the window). The golden-image test went
from 14.3 s to 18.5 s (with one more scene).

GPU: one more full-window triangle (the backdrop: 2 noise samples a pixel,
the full ground shader only in the 1.6-tile band past the edge); on ground
in sight, 2 more noise samples (the clouds), 2 + 2 more (feather, haze)
only near the fog's edge, 3 more on deep water (glints); sprites, no
texture samples, a few dozen ALU operations for the tone and the grade.
Turn the setting off on a slow machine.

# Art and Audio Spec

We cannot use Age of Empires' assets — they are Microsoft's. We can absolutely
use the *techniques*, which are documented and unencumbered, and the visual
language of late-90s isometric pixel art, which is not owned by anyone.

This document is the contract that any artist (human, generative, or
commissioned) must produce against, so that art from different sources still
cuts together into one game.

---

## 1. Projection and grid

- **Isometric 2:1 dimetric projection.** A tile is twice as wide as it is tall.
- **Base tile: 64 × 32 pixels** at 1× zoom.
- **Art is authored at 2× (128 × 64 per tile)** and downsampled for 1×, so the
  game is sharp on HiDPI displays and we are not re-drawing everything in a year.
- World-to-screen:

```
screen_x = (world_x − world_y) × 32
screen_y = (world_x + world_y) × 16 − elevation × 16
```

- **Elevation step: 16 px** (half a tile height) per level, 4 levels maximum.
  Cliffs are drawn as dedicated edge tiles, not as stretched terrain.

---

## 2. Sprites

### 2.1 Facings

- **8 facings**, authored as **5** (S, SW, W, NW, N) and **mirrored** for the
  remaining 3 (SE, E, NE). The renderer flips horizontally via an instance flag.
  This is what the Genie engine did and it cuts frame count by ~37%.
- Facing is derived from the unit's movement or attack vector, quantised to 8.

### 2.2 Animation set

Every mobile unit ships with:

| Animation | Frames | Notes |
|---|---|---|
| Idle | 4 | Loops; subtle breathing/shifting |
| Walk | 8 | Loops; foot-plant on frames 1 and 5 |
| Attack | 6 | Hit lands on frame 4 (the "impact frame", tagged in the manifest) |
| Death | 8 | Plays once, holds on the final frame |
| Decay | 4 | Corpse fade over ~30 s |

Villagers add per-task animations: **chop, mine, forage, farm, fish, build,
repair, carry** (carry variants show the resource being carried — this is a small
detail that does an enormous amount of work for readability).

### 2.3 Sizes

| Subject | Sprite size at 1× |
|---|---|
| Villager, infantry | 40 × 48 px |
| Cavalry, chariot | 56 × 56 px |
| Elephant, siege | 72 × 72 px |
| Small building (House, Farm) | 64 × 64 px (1×1 or 2×2 tiles) |
| Medium building (Barracks, Storehouse) | 128 × 96 px (2×2 tiles) |
| Large building (Town Center, Temple) | 192 × 144 px (3×3 tiles) |
| Wonder | 384 × 320 px (5×5 tiles) |

Anchor point is the sprite's ground contact point, declared per frame in the
atlas manifest. Getting anchors wrong is the most common source of "sprites
sliding around" bugs, so they are authored data, not a guess.

### 2.4 Palette and player colours

- **Indexed 256-colour palette**, one shared palette per architecture set.
  Restricting the palette is what makes independently produced art look like one
  game, and it is why the 1997 aesthetic is coherent.
- **Palette indices 240–247 are reserved for player colour** and remapped in the
  fragment shader to the owning player's ramp. Art is drawn once, in the
  reserved indices, and appears in all eight player colours.
- Player colours: blue, red, green, yellow, cyan, magenta, grey, orange —
  checked against deuteranopia and protanopia simulation before we commit.
- Index 0 is transparent.

### 2.5 Age variants

Pillar 1 ("you can see your empire advance") lives here. Each building needs
**one sprite per age it exists in** — not a recolour, a visibly different
structure: thatch and timber in the Stone Age, mudbrick in the Tool Age, dressed
stone and columns in the Bronze Age, monumental in the Iron Age.

Infantry get costume variants per age too (hides → cloth → bronze → iron).

This is the single largest art cost in the project, and it is where the money
should go.

---

## 3. Terrain

- **Terrain types:** grass, dirt, desert, sand/beach, shallow water, deep water,
  forest floor, snow. Each with **4 tile variants** to avoid visible tiling.
- **Edge blending**: transitions between types via alpha masks, 8 directional
  mask tiles per pair, blended in the shader.
- **Trees** are entities, not terrain — they occupy a tile, have hit points and
  are removed when felled, so the forest line visibly recedes over a match.
- **Cliffs**: 12 dedicated tiles (4 straights, 4 outer corners, 4 inner corners)
  per elevation transition.
- Decorative clutter (rocks, shrubs, bones) scattered by the map generator,
  purely visual, no collision.

---

## 4. UI art

- **Panel style:** carved stone and timber frames, warm neutrals, matching the
  era. High contrast against the world so the HUD reads instantly.
- **Icons:** 48 × 48 px at 1×, one per unit, building, technology and command.
  Silhouette-first: an icon must be identifiable at 24 px in greyscale.
- **Fonts:** one serif display face for headings and age names, one clean
  sans-serif for numbers and body. Both must render crisply at small sizes; all
  numerals tabular so resource counters do not jitter as they tick.
- **Cursor set:** default, move, attack, gather (one per resource), repair,
  garrison, convert, invalid.

---

## 5. Audio

### 5.1 Inventory

| Category | Count | Notes |
|---|---|---|
| Unit acknowledgments | 3–5 per unit type | Randomised, never repeating consecutively |
| Unit selection | 1–2 per unit type | Distinct from acknowledgment |
| Attack / impact | 4 per damage type | Melee, pierce, siege |
| Death | 2–3 per unit class | |
| Work loops | 1 per gather task | Chopping, mining, foraging, farming, fishing, building |
| Building | Complete, destroyed, under-construction loop | Per size class |
| Ambient beds | 1 per terrain type | Low, looping, positional |
| UI | Click, invalid, notification, research complete | |
| Age fanfare | 1 per age | Short, distinct, memorable |
| Music | 1 stem per age + 1 combat stem | Cross-faded |

### 5.2 Rules

- Every player action produces a sound within **50 ms**. This is the single most
  important audio requirement in the document.
- Randomised pitch (±5%) and round-robin selection on all repeated sounds.
- Voice limiting: at most 4 concurrent instances of any one sound.
- World sounds are positional (pan + distance attenuation from camera centre)
  and audible but quiet off-screen, so you hear your economy running.
- Nothing plays for events in fog you cannot see — audio must not leak
  information the fog is hiding.

### 5.3 Musical direction

Ancient-world instrumentation — frame drums, lyre, oud, bone flute, low male
chorus — with each age's stem adding instrumentation over the last, so the score
"ages up" with the player. Sparse rather than constant: silence between cues is
what makes the world feel large.

---

## 6. Production plan

Art is not on the critical path for gameplay. The order is:

1. **Placeholder pass (immediately).** Flat coloured diamonds and boxes,
   procedurally generated at runtime, with correct sizes, anchors and facings.
   Everything is playable and testable before a single sprite exists.
2. **Greybox pass.** One real sprite set for one civ, one age — proving the
   pipeline end to end (source PNG → atlas → manifest → in-game, with player
   colour remapping and mirroring working).
3. **Vertical slice art.** 2 civs × 2 ages, ~12 units, ~10 buildings.
4. **Full art.** Remaining ages, civs and architecture sets.

Every sprite goes through `tools/atlas`, which validates size, anchor, facing
count and palette conformance and **fails the build** on violation. That
validation is what keeps art from four sources looking like one game.

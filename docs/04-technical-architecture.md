# Technical Architecture

Target: **native desktop (Windows, macOS, Linux), written in Rust.**

Rust over C++ because the two hardest problems in this project — a bit-exact
deterministic simulation and a data-oriented entity store touched by many
systems — are exactly where Rust's guarantees pay for themselves, and because
the toolchain (`cargo`, `cargo test`, one command to build) keeps our iteration
loop short.

**One tradeoff to be aware of up front:** a native binary means you need a Rust
toolchain to run a build locally. The stack below (`winit` + `wgpu`) also
compiles to WebAssembly/WebGL2 with no changes to the simulation, so we can
produce a browser build for quick playtesting whenever that is useful. Native is
the product; the web build is a convenience.

---

## 1. Workspace layout

```
new-empire/
├── crates/
│   ├── sim/          Deterministic simulation. No rendering, no I/O, no floats.
│   ├── data/         Game data types + RON loading (units, buildings, techs, civs)
│   ├── mapgen/       Seeded random map generation
│   ├── ai/           Computer opponents. Emits Commands, reads only fogged state.
│   ├── render/       wgpu renderer: sprite batching, terrain, fog, effects
│   ├── audio/        Positional audio, buses, voice limiting
│   ├── ui/           HUD, panels, minimap, menus
│   ├── net/          Lockstep transport (stubbed in v1, interface defined now)
│   └── app/          Binary: window, input, scene stack, wiring
├── tools/
│   ├── atlas/        Sprite sheet packer → atlas + manifest
│   ├── simrunner/    Headless sim: replays, determinism checks, AI benchmarking
│   └── mapview/      Map generator visualiser
├── assets/
│   ├── data/         *.ron game data
│   ├── sprites/      Source PNG sequences (packed by tools/atlas)
│   ├── audio/
│   └── maps/
└── docs/
```

**The dependency rule that matters:** `sim` depends on `data` and nothing else.
It cannot see `render`, `audio`, `ui`, or the clock. Enforced in CI by a check
on `cargo tree`. If a renderer type ever ends up in the simulation, determinism
is gone and we will not find out for weeks.

---

## 2. Determinism

Every machine running the same seed and the same command log must produce
bit-identical state, forever. This is what makes replays, saves, desync
detection, and future multiplayer possible, and it is far cheaper to build in
now than to retrofit.

**Hard rules inside `crates/sim`:**

1. **No `f32` / `f64`.** Positions, velocities, health, gather rates and combat
   maths use fixed-point `Fx` = signed Q16.16 (`i32` with 16 fractional bits),
   with `i64` intermediates for multiply/divide. A `#![deny]` lint plus a CI
   grep enforces the ban.
2. **One RNG, explicitly threaded.** `xoshiro256**` seeded from the match seed,
   stored in the sim state, advanced only by the sim. No `rand::thread_rng()`,
   no `SystemTime`.
3. **No hash-map iteration.** Iteration order over `HashMap` is not stable across
   runs. Entity storage is dense `Vec`s indexed by generational IDs; anywhere a
   map is genuinely needed, `BTreeMap` or a sorted `Vec`.
4. **No wall-clock or frame-time input.** The sim advances only by whole ticks.
5. **All external input arrives as `Command`s** through one queue. There is no
   other way to affect the simulation — not from UI, not from AI, not from
   cheats.

**Verification:** after every tick the sim can produce a **state hash** (FNV-1a
over entity positions, health, resources and the RNG counter). `tools/simrunner`
replays a command log twice and asserts equal hashes at every tick. A corpus of
recorded matches runs in CI on every commit. When multiplayer arrives, clients
exchange this hash periodically and a mismatch names the exact tick.

---

## 3. Time model

```
Real time  ──► fixed accumulator ──► SIM TICKS at 20 Hz (50 ms)
                                        │
                                        ├─► state snapshot (prev, curr)
                                        ▼
Render at display rate (60–240 Hz), interpolating positions between snapshots
```

- **Simulation: 20 Hz fixed.** Matches the granularity of the era's RTS games and
  keeps the sim budget generous.
- **Rendering: uncapped**, interpolating between the previous and current tick so
  motion is smooth at any refresh rate. Interpolation happens in the renderer,
  in floats, and never feeds back into the sim.
- **Game speed** multiplies how many sim ticks a real second produces, not the
  tick length. Tick length is constant, always.

### Command turns (the lockstep model, single-player from day one)

Commands are not applied when issued. They are stamped with an **execution tick**
and applied at the start of that tick:

```
execute_tick = current_tick + COMMAND_DELAY   (COMMAND_DELAY = 2 ticks = 100 ms)
```

In single-player this delay is invisible (the unit still *animates and plays its
acknowledgment sound immediately* — the feedback is decoupled from the sim). In
multiplayer, it becomes the window in which every peer's commands for that tick
arrive. Building it now means adding networking later is a transport job, not a
rewrite. This is precisely the architecture from *"1500 Archers on a 28.8"*.

---

## 4. Entity model

Hand-rolled struct-of-arrays rather than a third-party ECS, because we need
guaranteed iteration order and a serialisable snapshot, and general ECS crates
give us neither for free.

```rust
pub struct EntityId { index: u32, generation: u32 }

pub struct World {
    // Dense parallel arrays, indexed by slot. Iteration order == slot order.
    alive:      Vec<bool>,
    generation: Vec<u32>,
    kind:       Vec<UnitKindId>,     // index into static data
    owner:      Vec<PlayerId>,
    pos:        Vec<Vec2Fx>,
    health:     Vec<Fx>,
    order:      Vec<Order>,          // current order state machine
    path:       Vec<Option<PathId>>,
    // ... one array per component
    free_slots: Vec<u32>,            // reused in deterministic (sorted) order
}
```

- **Static data** (a unit's cost, base HP, animations) lives in `data` and is
  never mutated. Entities hold an ID into it. Per-player modifiers (civ bonuses,
  researched techs) are applied through a `PlayerModifiers` lookup at the point
  of use, not baked into entities — so a tech completing updates every existing
  unit for free.
- **Orders are explicit state machines** (`Idle`, `Move`, `Gather`, `Build`,
  `Attack`, `Repair`, `Garrison`, `Convert`), each with a `tick()` and clear
  transitions. This is what makes unit behaviour debuggable.

### Systems, run in fixed order each tick

```
1. apply_commands       9.  combat_resolution
2. order_state_machines 10. death_and_removal
3. pathfinding_requests 11. construction
4. movement             12. production_queues
5. collision_resolve    13. research
6. gathering            14. population_recount
7. building_effects     15. victory_check
8. projectiles          16. fog_of_war_update
```

Fixed order, every tick, no parallelism inside a tick unless a system is proven
order-independent. Determinism first; we have 50 ms and we will not need it.

---

## 5. Pathfinding

Pathfinding cost ~30% of the original game's frame — as much as rendering the
whole game — and its failures are the number one complaint about it. It gets
first-class treatment.

**Three layers:**

1. **Sector graph (coarse).** The map is divided into 16×16-tile sectors with
   precomputed connectivity between sector edges. A path request first solves at
   sector level — cheap, and it answers "is this even reachable?" immediately
   instead of after an exhaustive failed search.
2. **Flow field (per group, per destination).** For a group order, one flow field
   is computed over the relevant sectors and shared by every unit in the group.
   Cost is per *order*, not per *unit* — this is what lets 60 units move without
   60 A* searches, and it produces natural group movement.
3. **Local avoidance (per unit, per tick).** Steering against a coarse unit
   occupancy grid: separation from neighbours, and a "push through" rule that
   lets a moving unit displace an *idle* friendly unit rather than jamming.

**Behaviour requirements, tested explicitly:**

- A unit whose path is blocked repaths within 3 ticks; it never stops silently.
- A unit ordered to an unreachable tile moves to the nearest reachable tile and
  reports arrival — it does not stand still, and it does not run laps.
- Faster units overtake slower ones on a shared route.
- Units never occupy the same tile centre; overlap is resolved deterministically
  by entity ID order.
- Path requests are **budgeted**: a fixed number of full searches per tick, with
  a priority queue (player-issued orders before AI-issued ones). Over-budget
  requests wait a tick rather than blowing the frame.

---

## 6. Fog of war

- Three grids per player, one byte per tile: `visibility` (count of units seeing
  it), `explored` (bitset), and `remembered` (last-seen building ID per tile).
- Vision updates incrementally: when a unit moves between tiles, decrement the
  circle it left and increment the one it entered. Circles are precomputed
  stamps per line-of-sight radius. No full-map recompute, ever.
- Elevation grants +1 line of sight and lets a unit see over one cliff level.
- The renderer reads the visibility grid into a low-resolution texture and
  smooths it in the shader, so the fog edge is soft while the simulation stays
  tile-exact.
- **The AI queries the same fogged view a player sees.** No exceptions, enforced
  by the `ai` crate having no access to raw `World` state — only to a
  `FoggedView<'_>` wrapper.

---

## 7. Rendering

`wgpu` (Vulkan / Metal / DX12 / WebGL2). One renderer, all platforms.

**Passes, in order:**

1. **Terrain** — chunked static mesh, one vertex buffer per 32×32 tile chunk,
   rebuilt only when terrain changes. Blended tile-edge transitions via a mask
   texture and a per-vertex blend weight.
2. **Ground decals** — building foundations, rubble, farm plots, selection
   ellipses.
3. **Sprites** — a single instanced draw per atlas. One instance = position,
   atlas rect, player-colour index, tint, flip flag.
4. **Projectiles and effects** — same pipeline, later depth.
5. **Fog of war** — full-screen multiply using the smoothed visibility texture.
6. **UI** — separate orthographic pass, no depth.

**Sprite specifics:**

- **Depth sort** by `(tile_y, world_y, entity_id)` — the last term guarantees a
  stable, deterministic order for co-located sprites, so nothing flickers.
- **Palette-indexed textures with a player-colour ramp**, exactly as the Genie
  engine did: sprite pixels store a palette index; indices in a reserved range
  are remapped in the fragment shader to the owning player's colour. One set of
  art serves eight players.
- **Horizontal mirroring** for facings: art is authored for 5 of 8 facings and
  the other 3 are the mirror, set by a flag on the instance. ~37% less art.
- Everything visible is one draw call per atlas per frame; a 400-unit battle is
  a handful of draw calls.

---

## 8. Audio

`kira` for mixing and buses.

- Four buses (UI, acknowledgments, world, music) with independent volume.
- **Voice limiting**: at most N concurrent instances of any one sound (N≈4), with
  ±5% random pitch variation, so twelve villagers chopping is a texture.
- 2D positional panning and distance attenuation relative to camera centre.
- Music: one stem per age, cross-faded over 4 seconds on age-up; a combat stem
  that ducks in when ≥6 units are fighting within the camera's view.
- Audio is driven from **sim events**, not from sim state polling. The sim emits
  an event stream (`UnitDied`, `BuildingCompleted`, `ResourceDeposited`) that the
  presentation layer consumes; it never reads sim internals.

---

## 9. Data and content pipeline

- All balance data in **RON** files under `assets/data/` — units, buildings,
  technologies, civilizations, map templates. Strongly typed on load, validated
  at startup (every referenced ID must exist; every unit must be trainable
  somewhere; every tech must be reachable).
- **Hot reload in debug builds**: edit a RON file, see the change without a
  restart. Balancing without recompiling is worth the plumbing.
- `tools/atlas` packs PNG animation sequences into atlases plus a JSON manifest
  (frame rects, anchor points, facing count, frame durations). Source art stays
  in the repo; atlases are build artefacts.
- Data files are content-hashed into the replay header so a replay recorded
  against different balance data is detected rather than silently desyncing.

---

## 10. Saves and replays

- **Replay** = match setup + seed + the full command log. Kilobytes. Replays are
  the primary debugging tool: a bug report is a replay file.
- **Save** = a full serialised `World` snapshot plus the command log since the
  last snapshot, so a save is also a resumable replay.
- Both are versioned; loading an incompatible version fails loudly with the
  version numbers rather than corrupting.

---

## 11. Testing

| Layer | Approach |
|---|---|
| Determinism | Replay corpus run twice per commit, per-tick hash equality |
| Simulation units | Standard `cargo test` on order state machines, combat maths, gathering |
| Pathfinding | Property tests: every request terminates; no unit is permanently stuck; on adversarial maps (mazes, single-tile gaps, full enclosure) |
| Map generation | Seeded generation asserts balanced starts and full reachability |
| AI | `simrunner` plays AI vs AI headless at 100× speed; asserts no crashes, no stuck villagers, and that Hard beats Easy over 20 matches |
| Performance | Benchmarked scenarios (400 units fighting, 200-pop economy) with a CI regression threshold |
| Rendering | Golden-image tests on a fixed scene, tolerance-compared |

`tools/simrunner` is the workhorse: it runs the whole game with no window, which
makes the AI, balance and determinism all testable in CI.

---

## 12. Performance budget

Per 50 ms simulation tick, at 200 population and 400 total entities:

| System | Budget |
|---|---|
| Pathfinding (amortised, budgeted) | 6 ms |
| Movement + collision | 3 ms |
| Combat, projectiles, orders | 3 ms |
| Economy, production, research | 2 ms |
| Fog of war (incremental) | 1 ms |
| AI (time-sliced across ticks) | 4 ms |
| **Total sim** | **≤ 19 ms of the 50 ms tick** |

Rendering is independent and targets ≤ 8 ms/frame for 60 fps with headroom.

The original's ~30/30/30 split between rendering, pathing/AI and simulation is
the sanity check: if our numbers drift far from that shape, something is wrong.

---

## 13. Dependencies

Kept deliberately small; every one is justified.

| Crate | Why |
|---|---|
| `wgpu` | Cross-platform GPU abstraction, one renderer everywhere |
| `winit` | Windowing and input |
| `kira` | Audio mixing with buses and tweening |
| `serde` + `ron` | Data files and save serialisation |
| `rand_xoshiro` | Deterministic, reproducible RNG |
| `glam` | Vector maths — **presentation layer only**, never in `sim` |
| `image` | Texture loading, atlas tooling |
| `thiserror` | Error types |
| `tracing` | Structured logging and profiling spans |

Nothing else without a conversation. Fixed-point maths, the entity store, and
pathfinding are ours — they are the parts where correctness is the product.

*Amendment (M0):* the RNG is hand-written too (`xoshiro256**` is twenty
lines), so `rand_xoshiro` is not used. That leaves `serde` as the simulation
crate's only dependency, which `scripts/check-sim-purity.sh` enforces.

---

## 14. Implementation notes from M0

Decisions made while building the foundation that the sections above did not
anticipate:

- **All `Fx` divisions round to nearest, halves away from zero** — including
  `from_ratio`, `/`, and `mul_div`. Truncation gave a systematic short bias:
  a unit walking 60 ticks at "1 tile/s" arrived at 2.9992, not 3. Rounding is
  unbiased, so repeated scaling lands where the integers say it should.
  Multiplication also rounds to nearest. Addition and subtraction saturate.
- **Angles are 16-bit BAM** (65536 per turn), not fixed-point radians. Wrap
  is free, there is no π to approximate, and `sin`/`cos` come from a
  committed 257-entry quarter-wave table (`tools/gen/gen_trig_table.py`) with
  linear interpolation, accurate to about 1/1000. `Vec2Fx::angle` is an
  integer `atan2` approximation good to ~0.3°, sufficient for facings.
- **Squared distances are 64-bit raw (Q32.32)**, never `Fx`, because 240² does
  not fit in Q16.16. `Vec2Fx::length` goes through `isqrt_u64` directly.
- **Despawn scrubs the slot.** Component data in dead slots is zeroed so the
  `World`'s derived equality agrees with its state hash — two worlds with the
  same live state compare equal regardless of history.
- **Commands are logged on issue**, so `Simulation::replay()` is always
  available and a replay is exactly "seed + config + `(issue_tick, command)`
  list". `Replay::verify` runs it twice and names the first divergent tick.
- **The state hash is FNV-1a** over tick, config, RNG state and draw count,
  every live entity's components in slot order, and the pending command
  count. Cheap enough to compute every tick; the simrunner does exactly that.
- **The presentation clock** (`crates/app/src/clock.rs`) caps catch-up at 8
  ticks per frame and drops the backlog beyond that, so a stalled window
  resumes rather than fast-forwarding.

## 15. Implementation notes from M1

- **Map generation is inside `sim`**, not a separate `mapgen` crate as §1
  planned. It needs only the sim's RNG and fixed-point noise, and keeping it
  there means a replay carries a seed and a `MapSpec` rather than a map. The
  generator retries derived seeds until a map passes its own checks (every
  start has the same kit, every start can walk to every other, elevation
  steps are at most one level) and falls back to a flat map after twelve.
- **Elevation lives on tile corners** (`(w+1)×(h+1)`), so terrain deforms
  smoothly as in the second game; a tile's gameplay elevation is the rounded
  mean of its corners. Start zones are flattened.
- **A `view` crate sits between `sim` and `render`.** It owns every piece of
  presentation maths — projection, camera, palette, terrain mesh, sprite
  sorting, the minimap — and a software rasteriser that consumes the same
  buffers the GPU does. `tools/mapview` uses it to render PNGs; the render
  crate's only unit test is naga validation of its WGSL. Any divergence
  between the two paths is a bug in `render`, by definition.
- **Terrain blending is per-vertex colour** for now: each corner averages the
  tiles that share it. Mask-texture blending (§7) waits for real terrain art.
- **Depth sorting is CPU-side** in `view::Scene` by `(x + y + footprint
  offset, slot)`; the GPU draws the instance buffer in that order with no
  depth buffer.
- **A `facing` component** was added to `World` and is set from the movement
  vector, so sprites turn as they walk. It is part of the state hash.
- **Placeholder art is drawn procedurally at 1×**, not authored at 2× as §2.1
  of the art spec asks; the atlas format is unchanged, so 2× art drops in
  when it exists.
- **Palette texture is `256 × 9`**: row 0 neutral, rows 1–8 players. The
  fragment shader looks up `(index, row)`; shadow (index 3) always reads row 0.

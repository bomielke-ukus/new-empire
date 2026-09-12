# Technical Architecture

Target: **native macOS desktop, written in Rust.**

Platform clarification (2026-09-11): development, live testing and release
acceptance target macOS. The existing Windows/Linux CI jobs remain additional
portability and determinism checks; they do not define supported products.

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

**[TA-DEP-01] The dependency rule that matters:** `sim` depends on `data` and nothing else.
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

1. **[TA-DET-08] No `f32` / `f64`.** Positions, velocities, health, gather rates and combat
   maths use fixed-point `Fx` = signed Q16.16 (`i32` with 16 fractional bits),
   with `i64` intermediates for multiply/divide. A `#![deny]` lint plus a CI
   grep enforces the ban.
2. **[TA-RNG-01] One RNG, explicitly threaded.** `xoshiro256**` seeded from the match seed,
   stored in the sim state, advanced only by the sim. No `rand::thread_rng()`,
   no `SystemTime`.
3. **[TA-ENT-04] No hash-map iteration.** Iteration order over `HashMap` is not stable across
   runs. Entity storage is dense `Vec`s indexed by generational IDs; anywhere a
   map is genuinely needed, `BTreeMap` or a sorted `Vec`.
4. **[TA-DET-09] No wall-clock or frame-time input.** The sim advances only by whole ticks.
5. **[TA-CMD-03] All external input arrives as `Command`s** through one queue. There is no
   other way to affect the simulation — not from UI, not from AI, not from
   cheats.

**[TA-DET-01] [TA-DET-02] Verification:** after every tick the sim can produce a **state hash** (FNV-1a
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

- **[TA-PATH-02]** A unit whose path is blocked repaths within 3 ticks; it never stops silently.
- **[TA-PATH-03]** A unit ordered to an unreachable tile moves to the nearest reachable tile and
  reports arrival — it does not stand still, and it does not run laps.
- **[TA-PATH-04]** Faster units overtake slower ones on a shared route.
- **[TA-PATH-05]** Units never occupy the same tile centre; overlap is resolved deterministically
  by entity ID order.
- **[TA-PATH-06]** Path requests are **budgeted**: a fixed number of full searches per tick, with
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
- **[TA-AI-01] The AI queries the same fogged view a player sees.** No exceptions, enforced
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

- **[TA-RENDER-01] Depth sort** by `(tile_y, world_y, entity_id)` — the last term guarantees a
  stable, deterministic order for co-located sprites, so nothing flickers.
- **[TA-RENDER-02] Palette-indexed textures with a player-colour ramp**, exactly as the Genie
  engine did: sprite pixels store a palette index; indices in a reserved range
  are remapped in the fragment shader to the owning player's colour. One set of
  art serves eight players.
- **[TA-RENDER-03] Horizontal mirroring** for facings: art is authored for 5 of 8 facings and
  the other 3 are the mirror, set by a flag on the instance. ~37% less art.
- Everything visible is one draw call per atlas per frame; a 400-unit battle is
  a handful of draw calls.

---

## 8. Audio

`kira` for mixing and buses.

- Four buses (UI, acknowledgments, world, music) with independent volume.
- **[TA-AUDIO-01] Voice limiting**: at most N concurrent instances of any one sound (N≈4), with
  ±5% random pitch variation, so twelve villagers chopping is a texture.
- 2D positional panning and distance attenuation relative to camera centre.
- Music: one stem per age, cross-faded over 4 seconds on age-up; a combat stem
  that ducks in when ≥6 units are fighting within the camera's view.
- **[TA-AUDIO-02]** Audio is driven from **sim events**, not from sim state polling. The sim emits
  an event stream (`UnitDied`, `BuildingCompleted`, `ResourceDeposited`) that the
  presentation layer consumes; it never reads sim internals.

---

## 9. Data and content pipeline

- **[TA-DATA-01]** All balance data in **RON** files under `assets/data/` — units, buildings,
  technologies, civilizations, map templates. Strongly typed on load, validated
  at startup (every referenced ID must exist; every unit must be trainable
  somewhere; every tech must be reachable).
- **Hot reload in debug builds**: edit a RON file, see the change without a
  restart. Balancing without recompiling is worth the plumbing.
- `tools/atlas` packs PNG animation sequences into atlases plus a JSON manifest
  (frame rects, anchor points, facing count, frame durations). Source art stays
  in the repo; atlases are build artefacts.
- **[TA-DATA-02]** Data files are content-hashed into the replay header so a replay recorded
  against different balance data is detected rather than silently desyncing.

---

## 10. Saves and replays

- **[TA-DET-05] Replay** = match setup + seed + the full command log. Kilobytes. Replays are
  the primary debugging tool: a bug report is a replay file.
- **[TA-SAVE-01] Save** = a full serialised `World` snapshot plus the command log since the
  last snapshot, so a save is also a resumable replay.
- **[TA-DET-06]** Both are versioned; loading an incompatible version fails loudly with the
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

## 13. Invariants the arithmetic must hold

The fixed-point types are not a utility library; they are the substrate the
whole simulation is defined in, and a rounding difference in any of them is a
desync rather than a wrong pixel. These are the properties they are required
to have, stated so they can be tested rather than assumed. Property tests in
`crates/sim/tests/` check each against a reference computed in `i128`.

### Fixed point (`Fx`)

| ID | Invariant |
|---|---|
| **TA-FX-01** | `floor + frac == self`, `frac` is in `[0, 1)`, `trunc` rounds toward zero, and none of them overflow at `Fx::MIN` or `Fx::MAX` |
| **TA-FX-02** | Ordering agrees with the raw `i32` ordering, and addition is monotone |
| **TA-FX-03** | Addition and subtraction **saturate**; they never wrap. A saturated value is a bug, but a deterministic one the hash will catch — a wrap is a silent teleport |
| **TA-FX-04** | Multiplication is commutative and within one ulp of the exact product |
| **TA-FX-05** | Division rounds to nearest, halves away from zero (`docs/07` D10) |
| **TA-FX-06** | `mul_div` and `from_ratio` match the exact rational result; the intermediate product never overflows |
| **TA-FX-07** | `sqrt` returns the floor of the true root, and zero for non-positive input |
| **TA-FX-08** | `min`, `max` and `clamp` agree with each other and with the ordering |
| **TA-FX-09** | Serialisation round-trips bit-exactly |

### Vectors (`Vec2Fx`)

| ID | Invariant |
|---|---|
| **TA-VEC-01** | `length` is the floor of the true length, and exact for axis-aligned vectors |
| **TA-VEC-02** | `normalized_or_zero` has unit length within an ulp, or is exactly zero |
| **TA-VEC-03** | `distance` is symmetric, and zero only between equal points |
| **TA-VEC-04** | `angle` is accurate to half a degree everywhere, including the octant seams |
| **TA-VEC-05** | `dot` and `length_sq_raw` are exact and never overflow, at any representable input |
| **TA-PATH-01** | `move_toward` makes strict progress every call, never overshoots, lands **exactly** on the target when it is within reach, and arrives in a bounded number of steps. Every "unit stuck forever" bug reduces to this property failing |

### Angles

| ID | Invariant |
|---|---|
| **TA-ANG-01** | `sin² + cos² == 1` within the table's tolerance, and `cos(a) == sin(a + 90°)`, for every one of the 65,536 representable angles |
| **TA-ANG-02** | Sine and cosine have the correct sign in every quadrant |
| **TA-ANG-03** | Both are continuous across the wrap at 65535 → 0 |

### Random numbers

| ID | Invariant |
|---|---|
| **TA-RNG-02** | Every range-limited draw is inside its range, including the empty and maximal ranges |
| **TA-RNG-03** | `chance(0, n)` never fires and `chance(n, n)` always does |
| **TA-RNG-04** | Exactly one draw per call, and none for a call that short-circuits. Desync diagnosis compares draw counts, which only localises anything if the count is a function of the code path |
| **TA-RNG-05** | Serialisation round-trips: a restored generator produces the same stream |
| **TA-RNG-06** | Adjacent seeds produce uncorrelated streams |
| **TA-RNG-07** | Range reduction is not biased toward either end |

Beyond these, **the output stream itself is frozen** (TA-RNG-01). Changing the
generator invalidates every replay and save file ever recorded, so a
known-answer vector is committed as a tripwire.

### Commands, entities and determinism

| ID | Invariant |
|---|---|
| **TA-CMD-01** | `drain_due` returns everything at or before the tick, in canonical `(tick, player, seq)` order, including ticks that were skipped |
| **TA-CMD-02** | Scheduling and draining are safe at the tick counter's ceiling |
| **TA-DET-03** | The queue's state is independent of the order commands *arrived* in. Two peers whose packets interleaved differently hold byte-identical queues, so network jitter alone can never desync them |
| **TA-DET-04** | Every invariant holds after every tick, across the config space |
| **TA-DET-07** | The simulation still reproduces the recorded corpus. Determinism says two runs agree; this says the behaviour has not silently changed since the corpus was recorded |
| **TA-ENT-01** | The live count, the free list and the component columns agree with each other after any sequence of operations |
| **TA-ENT-02** | Slot reuse is lowest-index-first. The identity a new entity receives is part of the state, so two machines that allocate differently have already diverged |
| **TA-ENT-03** | A handle to a despawned entity never resolves, even after its slot is reused |
| **TA-ENT-05** | `despawn` scrubs every component column, so a world's identity is its live state and not the history that produced it. Slot generations and the slot count are deliberately *not* scrubbed: an outstanding `EntityId` resolves in one world and is stale in the other, so those worlds are genuinely different |
| **TA-ENT-06** | Every invariant in `World::check` and `Simulation::check` holds after every tick, in every config |

---

## 14. Dependencies

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

## 15. Implementation notes from M0

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

## 16. Implementation notes from M1

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

## 17. Implementation notes from M2

- **Navigation is three layers, but not the three §5 planned.** Connected
  components over passable tiles (relabelled lazily when a static blocker
  changes) answer reachability in O(1) and redirect unreachable goals to the
  nearest reachable tile *before* any search. A line-of-sight test on the
  tile grid short-circuits the common short trip. A\* (8-connected, no
  corner cutting, octile heuristic) handles the rest, followed by
  string-pulling so paths take diagonals. Flow fields were not needed to
  pass the 60-villager test and were deferred to M4, where group combat
  movement wants them (§20 records their arrival).
- **Budgets:** 12,000 nodes per search, 48,000 per tick. Over budget, a
  walker waits a tick; three failures in a row and it gives up. `TickStats`
  reports searches, nodes, failures and deferrals for the profiler.
- **Stall detection measures progress toward the next waypoint**, not the
  goal. Measuring against the goal made every detour look like a stall — the
  first version gave up halfway round a wall. After 40 ticks without
  progress a walker replans (up to three times), then arrives if within
  2.5 tiles, else fails.
- **Separation is a linked-list tile grid**, resolved in slot order.
  Walkers displace idle units three to one; units anchored at a job (working
  a node or a site) are never displaced, so a column of walkers flows around
  a woodline instead of scattering it. Approach tiles prefer ones no other
  worker is anchored on, so a crowd spreads round a node.
- **Orders are the state machines §4 promised**, in `orders.rs`:
  `Idle`, `Move`, `Gather { node, resource, phase }`, `Build { site, working }`.
  A gatherer whose node vanishes delivers what it carries, then looks for
  the nearest node of the same resource within ten tiles that it can
  actually stand beside; a walled-in node counts as gone.
- **Buildings are entities with `construction: Option<ticks>`.** A site blocks
  its footprint the moment it is placed, is paid for on placement, refunds
  the unbuilt fraction if cancelled, and only counts toward population or
  drop-off once complete. Up to five builders add a tick of work each.
- **Production queues live on the building** (`Production { queue, rally }`).
  A finished unit waits at the door while the player is housed. Rally points
  may be a point, a node (new villagers gather) or a site (they build).
- **Positions are tile centres now.** M1 spawned everything on tile corners;
  buildings sit on the geometric centre of their footprint
  (`nav::building_centre`) and `anchor_tile` inverts it.
- **The HUD is sprites.** A 5×7 pixel font and solid fills live in the atlas
  under reserved kind ids; screen-space sprites carry a flag the vertex
  shader honours. No text library, no second pipeline, and the software
  rasteriser draws it identically.

## 18. Implementation notes from Q10

- **The palette has one source of truth**, `assets/palette/ancient.ron`. The
  art tool bakes it (`atlas export --rust`) into
  `crates/view/src/palette_table.rs`, which is committed so the renderer has
  no runtime I/O, and diffed against its generator in CI. Named colours in
  `view::palette` resolve at compile time through a `const fn` ramp lookup.
- **Index 239 is `shadow`**, the one index whose alpha is not 255. The
  palette texture carries the alpha; the sprite shader only redirects it to
  row 0 so a shadow is never player-coloured.
- **Sprite sets load at startup** from `assets/sprites` (found by walking up
  from the working directory). Each set's PNG palette chunk must match the
  baked palette entry for entry, or the set is refused with the `atlas
  repalette` fix named in the error. A refused set costs that sprite, not
  the game: the kind keeps its placeholder.
- **The atlas is 2048 wide and grows to fit**; the app requests default
  (8192) limits rather than downlevel ones. Frames carry their authored
  `scale`; the scene draws `w / scale` so 2× art occupies 1× space and is
  sampled at full resolution when zoomed in.
- **Animation is chosen from simulation state** (walking, working, idle)
  and timed from game ticks plus a per-slot phase. It never feeds back.

## 19. Implementation notes from M3

- **Technology is a queue item.** A building's production queue holds
  `Item::Unit` and `Item::Tech` alike; research pays on queueing, refunds on
  cancel, and completes through the same `production` pass as a villager.
  Effects fold into `Player::modifiers`, which the gather, movement and
  construction systems read; an age advance is an effect like any other.
- **The age gate is a query, `Simulation::can_research`**, and the command
  runs the same query before paying, so the panel greys a button for the
  reason the simulation would refuse it. `age_buildings` counts finished,
  owned buildings of the player's current age, less Houses, the Town Center
  and Farms.
- **Construction progress is in hundredths of a builder-tick**
  (`KindInfo::build_work`), so a percentage build-speed bonus applies
  exactly. Anything that reads `World::construction` divides by
  `build_work`, not `build_ticks`.
- **Farms are nodes that belong to someone.** `gatherable_by` is the one
  place that rule lives: a farm is worked only by its owner, a site holds
  nothing, and an exhausted farm stays standing at zero for the `farms` pass
  to reseed before orders run, so a villager mid-harvest never notices.
- **`SimConfig::validate` is the setup screen's check, not the engine's.**
  The population-cap range from `docs/02` is enforced there; the engine and
  the replay path stay permissive because tests and the soak deliberately
  run caps of 0, 6 and 12.
- **Age variants are atlas lookups, not sprite state.** `Atlas::variant`
  maps `(kind, age)` to the id the age-styled frames are filed under;
  placeholders draw four material sets (timber, mudbrick, limestone,
  granite), and rendered sets answer with themselves until their manifests
  carry variants. The sweep and banner are app-side timers passed into
  `Scene::build_full` and `HudInput`, so a frame is still a pure function of
  its inputs and `mapview --sweep` can render any moment of it.
- **The HUD owns the hotkey table.** Each button carries its key; the app
  looks the pressed letter up in the buttons it last drew, so the panel and
  the keyboard cannot disagree.

## 20. Implementation notes from M4, chunk 1: flow fields

- **The three layers of §5 now exist, in `flow.rs`.** A sector graph
  (16×16-tile sectors, one portal per open run along each edge, walking
  costs between a sector's portals and from each portal to every tile of
  its sector, built lazily and rebuilt per sector when its tiles change) is
  searched once per group to find the *corridor* of sectors a route passes
  through; a flow field is flooded over that corridor only, from the
  destination outward; and every unit of the group reads a heading off the
  field. The local layer (separation, push-through) is M2's, unchanged.
- **A field is keyed by destination, not by order.** The key is the goal
  tile, or the footprint of the building or node being walked to, so every
  villager bound for the same woodline shares one field, and so does every
  unit of a group order. Fields live in a cache that is *not* simulation
  state: their contents are a pure function of the grid, so two machines
  with different caches steer identically. The budget of `TA-PATH-06` is
  therefore counted in distinct *destinations* served per tick
  (`DESTINATIONS_PER_TICK`), never in fields built, so a warm cache can
  change what a tick costs but never what a unit does.
- **Fields extend rather than rebuild.** A straggler outside a field's
  corridor adds the sectors of its own route and the flood carries on from
  the old edge; a flood stops as soon as every tile that asked for it is
  settled, and records the cost it is exact up to, so the next asker can
  resume it. A whole-field re-flood happens only when a covered sector's
  tiles change. Fields nobody has read for 20 seconds are evicted.
- **Steering is a lookahead along the field, not a waypoint list.** A unit
  walks toward the furthest of the next twelve tiles downhill that it can
  see, re-reads the field when it gets there, when it stalls for 3 ticks
  (`TA-PATH-02`), or when the grid has changed under a straight-line
  heading. Giving up is a separate measure: no improvement in distance to
  the goal for 20 seconds *while staying within two tiles of where it last
  improved* means jammed; moving without getting closer is a detour and
  continues. The per-order replan allowance that tied `STALL_TICKS` to 40
  is gone.
- **The inner loops are arrays, not maps.** The flood reads one byte per
  neighbour from a per-box mask (passable; passable and covered) and a
  dense per-sector cover table, and pops from a bucket queue keyed by
  integer cost; the corridor search indexes portals as
  `sector × MAX_PORTALS + portal` into stamped scratch tables. Each of
  those replaced a `BTreeMap` or a binary heap that profiling found in the
  hot path; `docs/09` §8 has the numbers at each step.
- **What A\* is still for.** `nav::find_path` remains for tests and for
  anything that wants one explicit path (the debug overlay); the
  simulation no longer calls it.

## 21. Implementation notes from M4, chunk 2: the damage model and the roster

- **Combat numbers live in the kinds table, not on entities.** `KindInfo`
  gained a `class` (villager, infantry, archers, cavalry, siege, buildings,
  animals) and a `combat` block: attack, damage type, range, reload, the two
  armours, class bonuses and line of sight. Technology never edits the
  table; it accumulates in `Modifiers` as per-class attack, armour and range
  bonuses, and `combat::attack_of` / `armour_of` / `range_of` add the two
  together at the moment of asking. A unit's numbers are therefore always
  the table plus its owner's research, with nothing to keep in sync.
- **`combat::damage` is the whole rule.** `max(1, elevation(attack) −
  armour_of_matching_type + bonus)`, in integers, with elevation rounded to
  nearest and halves up; siege meets no armour and `hits_friends`. The
  simulation's `damage_between(a, b)` wraps it with both owners' modifiers
  and the map's elevation under each unit, and is what the attack order of
  chunk 3 will call. Nothing in chunk 2 deals a hit.
- **The matrix is generated, committed and diffed.** `simrunner matrix`
  writes `docs/damage-matrix.md` from the table; `check-generated.sh`
  regenerates and compares it, so a stat change shows up as a reviewable
  diff of hits-to-kill rather than as a surprise in a playtest.
- **Rosters are derived, not declared per building.** Each unit names the
  building that trains it (`trained_at`); `kinds::trained_at(building)` is
  the roster and `Simulation::roster(player, building)` drops the kinds a
  line upgrade has moved past. `can_train` gives the reason a button is
  grey in the simulation's words (age, unresearched line, superseded,
  queue full, unaffordable), and the `Train` command runs the same check
  before paying.
- **A line upgrade is an effect.** `Effect::UpgradeLine(from, to)` swaps
  the kind of every live unit the player owns and every queued item, adding
  the hit-point difference so a full Clubman is a full Axeman. The Axe is
  the slice's one line upgrade; Toolworking, Leather Armour and Fletching
  are the per-class bonuses.
- **Placeholders share one body.** The six soldiers draw the villager's
  body with a weapon over it (`foot_body` and `shaft`), so they read as
  the same people with different jobs, and the infantry take the
  villager's age costume. The atlas tool already catalogued their sprite
  sets; `kind_for_set` now maps the names.

## 22. Implementation notes from M4, chunk 3: fighting

- **Combat is four passes after movement,** in `battle.rs`: `acquire`
  (units whose stance allows it pick a target they can see), `strike`
  (units on an attack and in reach swing when their reload allows; melee
  lands at once, ranged loosens a projectile), `fly` (projectiles home on
  their target and land), `deaths` (zero health becomes a corpse for
  thirty seconds, or removes a building outright). The order machines for
  attack, attack-move, patrol and flee live in the same file and run in
  the `orders` pass with the rest. §4's system list had projectiles,
  combat resolution and death in that order; this is that.
- **An attack remembers what to do afterwards.** `Order::Attack` carries a
  `Then`: stand, return to where the unit stood, carry on the attack-move,
  or resume the patrol leg. Target acquisition fills it in from what the
  unit was doing, so a defensive unit that steps out to meet a raider
  walks back to its post and an attack-move column that stops to fight
  goes on to its destination. A `leash` (origin and radius) is what
  separates the stances: aggressive chases to twice its sight, defensive
  to its sight, stand-ground not at all, and an ordered attack has none.
- **Passive means run.** A passive unit hit by an enemy takes a `Flee`
  order to its nearest finished Town Center, or eight tiles the other way
  if it has none, and its side gets one `Event::Alarm` per ten seconds.
  Events are a per-tick list the presentation reads and the state hash
  ignores; the app turns the alarm into a banner.
- **A corpse is the same entity, marked dying.** `World::dying` counts the
  corpse down; every system skips a dying slot (orders, movement,
  separation, population, picking, targeting) and the renderer plays the
  death animation then the decay frame from the tick of death. Buildings
  have no corpse yet: chunk 4's rubble.
- **Formations are offsets in quarter tiles** (`formation.rs`), rotated to
  face the way the group walks and assigned so the left of the group
  forms the left of the line. A group order with a formation hands every
  unit its slot and the group's slowest speed as the trip's `pace`;
  `Formation::None` is the old spread, each at its own pace, which is how
  the pace matching is switched off (`UX-CMD-08`'s "toggleable").
- **Keys.** `docs/03` asks for `A` then click for attack-move. `A` is
  camera panning, which nothing may share (`docs/09` §5), so attack-move
  is `M` and patrol `P`; stances are `Q E I K` and `Z` cycles the
  formation. Soldier keys appear only when no villager is selected, so a
  villager's building keys never clash with them.

## 23. Implementation notes from M4, chunk 4: buildings in combat

- **Rubble is the building, marked dying,** the way a corpse is the unit.
  `deaths` hands a building at zero health to `demolish`: the garrison
  steps out onto the ring around the footprint, the footprint is
  unblocked at once (a breach, if it was a wall), the production queue is
  lost, builders on it stop, a site refunds nothing, and `World::dying`
  counts down `RUBBLE_TICKS` (sixty seconds, `docs/03` §6.2). Every check
  that asked "is this building finished?" now also asks "is it standing?":
  drop-offs, farms, the age count, training, research, gates, shelter.
  `remove` skips the unblock for rubble, since it already happened. A new
  site placed over rubble clears it.
- **Buildings fight through the same passes as units.** A kind's `Combat`
  block gained `arrows`, the projectiles per volley: one for every unit,
  one for the Watch Tower, none for the Town Center; a building adds one
  per unit garrisoned inside it, so an empty Town Center is silent and a
  full one is a bulwark. `can_fight` is "attack above zero and a volley
  above zero", which is what keeps the empty Town Center out of `acquire`.
  Buildings default to the stand-ground stance and hold an `Attack` order
  like anything else; `orders` runs the attack machine for a non-mobile
  entity when it holds one. A volley's arrows start a quarter tile apart
  so a full tower visibly fires more than one.
- **Building armour** is on the kinds table through `fortified(melee,
  pierce)`: five pierce for an ordinary building, so arrows do the
  minimum, and more for walls (palisade 2/8, stone 3/10). The matrix shows
  it: a bowman does one to any building, an axeman three to a palisade
  and two to stone. Siege is still the answer the design intends, and is
  not in the slice.
- **A gate is a wall segment whose tile is a blocker only while an enemy
  is near.** One grid, one set of flow fields, no per-player passability:
  the `gates` pass (before `orders`) blocks a finished gate's tile when an
  enemy unit is within two tiles and unblocks it when none is within three.
  Nothing walks the gap in a tick, so no enemy is ever standing on a gate
  as it shuts; an owner's unit caught on the tile is nudged off by
  `keep_off_blocked` like anything else. The fields re-flood for the sector
  as the grid changes, so an enemy column finds the way shut and, on an
  attack-move, sets about the wall. A gate under construction is solid
  like any site; a finished one opens; a spawned one starts open. The
  view reads the tile's passability for the open frame.
- **Attack-move takes buildings only where the walk ends.** Chunk 3's
  acquisition took units in sight; a first version of this chunk took
  buildings in sight too, and a column chewed every segment it passed
  instead of advancing. Now `tick_attack_move` looks for the nearest enemy
  building in sight when its trip arrives or fails: walled out, it breaks
  in; arrived, it razes what stands there. The order carries the point
  the player asked for while the trip goes to the formation slot, so a
  column that stopped for a wall goes on to the right place afterwards.
- **Garrison is a column, `World::inside`,** the building's id. A unit
  inside stands at the building's centre, is drawn by nothing, picked by
  nothing, hit by nothing and moved by nothing, holds no order, and still
  counts toward population. `Order::Garrison` walks to an approach tile
  beside the footprint exactly as a builder does, and enters within
  `REACH_SLACK`. `eject` (the `Ungarrison` command, a fallen or deleted
  building) spreads them over the nearest open tiles. A fleeing villager
  carries the Town Center it runs for in `Order::Flee::into` and steps
  inside on arrival, which is the town bell of later games without the
  bell; they come out when told to (`ALL OUT`). `Simulation::check` holds
  every `inside` to a standing, same-owner building built to hold units.
- **Walls are one-tile buildings placed in runs.** `nav::line_tiles` is
  the straight eight-connected run between two tiles; the app drags it,
  the ghost draws every tile hatched on its own, the release issues one
  `Build` per tile the simulation accepts with the villagers named on the
  first, and a builder whose site finishes looks for the next site within
  six tiles (`next_site`), which is how one villager builds a run. A gate
  is placed onto one of the player's finished wall segments, which it
  replaces and refunds, or onto clear ground. Walls and gates do not count
  toward an age.
- **The defences page.** The Watch Tower, both walls and the gate share
  one DEFENCES button (`J`) that swaps the build grid for a page of four
  plus BACK, because the grid has fifteen slots and the Tool Age already
  fills them, and because every letter is spoken for in the mixed
  villager-plus-building panel the hotkey test guards. On the page the
  keys are `J P N G`; the uniqueness rule is now "distinct within any panel
  that can be shown at once", and the test says so. `T` on a building alone
  is ALL OUT; with units also selected it is STOP, which comes first.


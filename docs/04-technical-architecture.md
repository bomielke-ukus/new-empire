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
│   ├── fogged/       FoggedView: one player's view of a match, and nothing else
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

- A `Fog` per player (`sim::fog`): `visibility` (count of the player's
  things seeing each tile now), `explored` (a bitset), and the static things
  last seen, by anchor tile, with the kind, owner, the owner's age and
  whether it was a construction site when seen. The explored bitset and the
  memories are history and are hashed; the visibility counts are derived
  each tick and are not.
- Vision is recomputed every tick, not incrementally (`docs/07` D23, §24):
  every standing, living, un-garrisoned thing stamps a precomputed disc for
  its line of sight, and every static thing on a seen tile is remembered.
- Elevation grants +1 line of sight. Cliffs neither block nor extend sight
  yet.
- The renderers draw it as a light per tile *corner* (`view::fog`, §25):
  black where any tile at the corner was never seen, the mean of the
  explored and visible tiles around it otherwise. Terrain vertices carry
  their corner and the light shades across each tile, so the edge is soft
  over a tile, the simulation stays tile-exact, and elevation is exact
  because the vertex already has its height. Sprites carry one light each.
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
5. **Fog of war** — not a pass of its own: the terrain vertex shader reads
   the corner-light texture and the sprite shader its instance's light
   (§6, §25).
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
  the primary debugging tool: a bug report is a replay file. *As built
  (M6):* every match played in the app is recorded as one under the
  recordings directory, named like a save, and the WATCH REPLAY screen
  plays it back (§32).
- **[TA-SAVE-01] Save** = a full serialised `World` snapshot plus the command log since the
  last snapshot, so a save is also a resumable replay.
- **[TA-DET-06]** Both are versioned; loading an incompatible version fails loudly with the
  version numbers rather than corrupting.

*As built (M6):* a save (`crates/save`) is the whole `Simulation`, which
carries its command log from tick 0, plus the opponents mid-thought and
the camera, so the "log since the last snapshot" is the whole log and
`Save::replay` is the match so far; `Save::verify` replays it and requires
the snapshot's hash. Three version numbers are checked before the world
is parsed: the save layout, `sim::STATE_VERSION` and `Replay::VERSION`.
See §31.

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

## 24. Implementation notes from M5, chunk 1: fog of war and the AI boundary

- **A `Fog` per player** (`fog.rs`), sized to the map: a `visibility`
  count per tile, an `explored` bitset, and the static things last seen,
  by anchor tile. `visibility` is recomputed every tick and is not hashed;
  `explored` and the memories are history and are. A memory is kept until
  the tile is seen again, and a tile in sight has no memory at all: what
  is there is there. `Fog::remembered` and `memories` answer only for tiles
  out of sight.
- **Recomputed, not incremental** (`docs/07` D23). §6 planned to
  increment and decrement circles as units move between tiles. The pass
  `fog_of_war_update` instead clears every count and stamps every standing,
  living, un-garrisoned entity's sight disc afresh, then remembers every
  static thing on a seen tile. The cost is the sum of the discs, a few
  hundred thousand byte operations a tick at full population, and no
  per-entity bookkeeping to get wrong. The discs are cached by radius in
  `Scratch`. A building sees its line of sight past its edge; high ground
  sees a tile further; nothing yet sees over or is stopped by a cliff.
- **`FoggedView` lives in its own crate,** `fogged`, over `sim`'s public
  API: the tile's state, terrain and elevation where explored, passability
  as last seen, the player's own state and modifiers, every own entity and
  every other on a tile in sight, the memories, and the placement,
  training and research queries for the player's own side. It re-exports
  the command and data types an opponent needs and nothing that reaches
  the world.
- **The `ai` crate depends on `fogged` and not on `sim`.** That is the
  whole enforcement of `TA-AI-01`: `sim::World` is unnameable there
  because `sim` is not a dependency, and `fogged` re-exports no path to
  it. Three compile-fail cases pin it, so a re-export or a new dependency
  that opened the world would fail the build. Each is built as a crate of
  its own on `fogged` alone and must be rejected with an unresolved-path or
  private-item error code; the message's wording is not compared, because
  it changes between compiler releases (it did, between the toolchain here
  and CI's, and `trybuild`'s exact-text comparison went red). The crate holds
  `Difficulty`, and an `Opponent` seeded from the match and its player
  number whose `think` returns the commands to issue; for now, none.
- **Who issued a command travels with it.** `Source::{Player, Ai}` is
  recorded in the queue and the replay log (`Replay::sources`, parallel
  to the commands, defaulting to the player for files recorded before
  there were opponents, so the format is unchanged). A command names
  units; the last source to name a unit is written to `World::priority`,
  and `plan_paths` sorts its requests by that before slot order, so when
  the destination budget binds the player's requests are served first
  (`TA-PATH-06`, now whole). `simrunner ai` runs opponents on every side
  headless, invariants on, and verifies the recording.

## 25. Implementation notes from M5, chunk 2: fog in the presentation

- **Light per corner, not a full-screen pass.** §7 planned a full-screen
  multiply over a smoothed visibility texture. A full-screen pass has to
  find the ground under each pixel, and with elevation it cannot without a
  height buffer, so the fog edge would drift up a hillside. Instead
  `view::fog::FogLights` turns a player's `Fog` into one byte per tile
  corner, `(width + 1) × (height + 1)`: black if any tile at the corner was
  never seen, the mean of the tiles' lights (half for seen once, full for
  in sight) otherwise. `TerrainVertex` gained its corner; the terrain
  shader reads the corner's light from an `R8Unorm` texture in the vertex
  stage and scales the colour, and the rasteriser scales the vertex colour
  before its Gouraud fill, so the two agree to rounding. The rule that a
  corner beside unseen ground is black means the soft edge eats into the
  seen side and never shows a sliver of ground the player has not seen.
- **What is drawn.** With a viewer, `Scene::build_full` (its options now
  a struct, `SceneOptions`) draws the viewer's own things anywhere, anyone
  else's only with a tile of it in sight, projectiles only on a tile in
  sight, and every memory as the building or node it was: the idle frame
  of the kind in the owner's age when seen, or the site pegs, at the
  explored light, with no slot, so it is not pickable. A remembered
  building outlives its destruction until the tile is seen again, which is
  the asymmetry `GD-FOG-01` asks for. A placement ghost is refused on
  ground never seen, and the app's `placeable` checks the same before it
  issues a `Build`. Impact sparks are not shown in the fog.
- **The minimap** (`Minimap::render_for`) is black where never seen, the
  terrain at half light where seen once with the remembered things on it in
  their owner's colour, and live where in sight. The rasteriser now draws
  it in the panel's corner as the UI pass does (`raster::draw_minimap`),
  so a `mapview --hud 1` frame shows the whole window the game shows and
  the golden images pin the minimap too.
- **The fog changes only with the tick,** so the app uploads the lights
  once per tick, not per frame; the sprite's light rides in the instance
  word that was spare.
- **Tools see everything by default only when asked.** `mapview` renders
  player 0's view; `--fog 0` shows the whole map for looking at a
  generated map, and `inland-start` is the one golden image that uses it.

## 26. Implementation notes from M5, chunk 3: the economy manager

- **A thought every few ticks, from the view alone.** `Opponent::think`
  runs on every tick of its build order's cadence (40 for Easy, 20 for
  Standard, 10 for Hard), offset by its player number so two opponents do
  not think together, and answers with nothing in between. A thought reads
  one `FoggedView` and returns `CommandKind`s that the caller issues as
  `Source::Ai`; the manager keeps almost nothing between thoughts (the
  rally it last set, whether auto-reseed is on, the buildings it ordered
  and has not yet seen as sites). Everything else is re-derived from the
  view, so a lost villager or a refused order costs one thought, not a
  stale plan.
- **The build order is a table** (`BuildOrder::for_difficulty`): villager
  targets and gather shares by age, the cadence, the population headroom a
  house is started at, the last age aimed for, and whether one gatherer a
  thought is moved from the resource most over its share to the one most
  under it. Until food and wood are stocked, the stone and gold shares go
  to them. Farms come before the next age's buildings when the food in
  sight is short, because a settlement with no food coming in buys
  nothing. Without hunting (owed), the Stone Age's food is the berries,
  so the Stone Age villager target is small enough to leave 400 food for
  the Tool Age, and farms feed the growth after it.
- **The view grew what a player sees by looking.** A `Sighting` says
  what one of the player's own villagers is doing (`Job`: idle, gathering
  which resource, building which site) and what a node has left; a
  building's queue and the match's population limit are readable. A
  `Memory` carries the entity's handle (`docs/07` D24), so a remembered
  tree or building can be ordered at as a player clicks on one; the
  manager sends a villager to *walk* to a remembered node rather than
  gather from it, since a memory may be of something gone, and the walk
  settles it either way.
- **Head-on walkers now step aside.** The first opponents' first two
  villagers, sent past each other along one row, stood where they met
  for twenty seconds until they gave up: separation pushed them apart
  along the line they walked, undoing each step. `separation` now detects
  a pair walking toward each other and pushes them to opposite sides of
  their line instead, and `behaviour_pathfinding` pins it. This changed
  every corpus digest.
- **`simrunner ai --stats --save FILE`** prints each side's jobs,
  buildings, queue and food in sight, and keeps the recording for
  `mapview --replay`, which is how the stall above was found.

## 27. Implementation notes from M5, chunk 4: the military manager and scouting

- **The military spends what the economy leaves.** One thought runs the
  economy manager first and hands the military the stock it did not
  spend, less what the economy is saving for the Tool Age (the farms
  behind it are the food engine; later ages compete with the army for
  what comes in). A reserve of food and wood stays untouched for the
  next villager and house. The first version let soldiers eat the Tool
  Age's 400 food and one Hard opponent sat in the Stone Age for twenty
  minutes with fifteen hundred wood; the saving rule is the fix.
- **The scout rides rings** of 14, 22, 30, 40 and 50 tiles round the
  Town Center, eight compass points each, to the first point on them the
  view has not yet seen, clamped to the map, and starts over when every
  point is seen. Passive stance, so it runs when hit. Hard and Standard
  scout; Easy does not (`docs/02` §12's table). A Hard opponent sees
  70–80% of a 96-tile map in twenty minutes; Easy sees 5%.
- **The bell.** `FoggedView::events` passes on the player's own alarms and
  losses and nothing of anyone else's. `Opponent::think` listens every
  tick, so an alarm between thoughts is not missed, and answers one at
  home (within thirty tiles of the Town Center) with every soldier at
  home, once per alarm.
- **Composition and targets.** Soldiers are trained to the army target of
  the age, the kind furthest below its share first, among what a
  finished building of the side can train now (`roster`), so the Stone
  Age is clubmen and the Tool Age axemen, bowmen and slingers as their
  buildings stand. A raid goes out with `attack_size` soldiers idle at
  home once the order's hour has come, at the nearest enemy building
  that does not shoot back, in sight or remembered; the army walks into
  the Town Center's arrows only at twice that. Soldiers idle away from
  home carry on to the next such target or come home. The first raids
  went out four at a time at the Town Center and died to it; the sizes
  above are what stopped that.
- **A Watch Tower** by the Town Center for Hard, in the Tool Age, when the
  stone is there; stone is gathered only once food and wood are stocked,
  so it is late. Walls are not built yet (owed).
- **Sheltering villagers count.** The economy counted only villagers
  outside, so a raid that sent them into the Town Center made it train a
  second workforce (one Easy side reached eighteen villagers against a
  target of ten). They count wherever they are.

## 28. Implementation notes from M5, chunk 5: victory, defeat and the acceptance run

- **Standing, winner, score.** `Simulation::standing(p)` is `docs/02` §10's
  conquest rule: not resigned, and a living unit or a finished building
  that trains (the Town Center included). `winner()` is the one side
  standing once every other is out, `over()` whether the match is
  decided, and `score(p)` what a side gathered plus the cost of everything
  it has standing, which decides a match at a time limit (`docs/07` D25).
  `CommandKind::Resign` takes a side out at once and its later commands
  are ignored. None of this is new state beyond the `resigned` flag; the
  rest is derived from the world every time it is asked.
- **The declared bonus.** `SimConfig::gather_bonus_pct`, one entry per
  player, bounded by the setup screen's `validate`, is folded into
  `modifiers(p)` and hashed with the config. A Hardest opponent is set up
  with 25%. The opponent itself is the same code as Hard: the bonus is
  the match's, not the AI's, which is what "declared honestly" means.
- **`simrunner versus`** is the `RM-M5-01` harness: matches between the
  sides of `--difficulty`, each to elimination or the time limit, the
  invariants checked every tick, and a villager idle for sixty seconds
  with a resource in sight counted as stuck. One line per match (seed,
  ticks, winner, how, scores, hash) goes to the record at
  `tools/simrunner/tests/versus-hard-easy.golden`; CI runs the twenty and
  fails on a changed line or fewer than eighteen Hard wins. The first
  recorded match is also played in full by a test, so the record moves in
  a test before it moves in the job.
- **Two pockets the harness found.** The stuck-villager rule caught, in
  one seed, a villager sent again and again to a tree inside a forest
  with no ground beside it (the manager now sends nobody to a node it
  cannot stand next to, and stops sending anyone to a node a villager
  came back idle from), and then villagers shut into pockets of one and
  three tiles between the farms packed round the Town Center. Two fixes:
  `NavGrid::nearest_open` re-seats a unit displaced by a site into the
  roomiest ground within reach rather than the nearest tile, which can be
  the pocket; and the manager's `place` floods the open ground beside a
  proposed footprint four ways and refuses a spot that would leave fewer
  than 48 tiles connected. The first changed every corpus digest.
- **What the twenty matches say.** Hard beats Easy 20 of 20, every one
  on score at thirty minutes; none by elimination. Armies stay small and
  raids trade soldiers for houses, so a Town Center is never taken. That
  is within the acceptance as written and short of the opponent we want;
  the next step in `docs/10` says what to do about it.

## 29. Implementation notes from the M5 tuning pass: an army that ends a match

The first acceptance record was twenty wins on score and none by
elimination. Six changes, each found by watching one seed, turned that
into eighteen eliminations in twenty:

- **All out.** Every villager of the losing side was sitting inside its
  Town Center. Villagers shelter when hit (`GD-STANCE-02`) and nothing
  but their own side tells them the danger has passed; a player presses
  ALL OUT and the opponent never did. So the economy stopped (the
  manager gives no orders to anyone inside) and the Town Center, which
  shoots one arrow per unit inside, became a fortress that killed every
  assault. The military manager now empties every building with anyone
  inside once no alarm has sounded for thirty seconds. This one change
  did more than the other five together.
- **The army masses.** Raids in ones and twos, and assaults of eight,
  fed the Town Center's arrows and took nothing. The army now waits at
  home for the attack size (twenty for Hard) or three quarters of it
  once the order's hour has come, then attack-moves at the enemy Town
  Center as one, fighting what meets it on the way, in Aggressive stance
  so it chases what runs. A plain `Attack` on the Town Center was tried
  and dropped: the soldiers ignored the defenders killing them.
- **Wood, not food, was the army's limit.** Food piled up to two
  thousand while soldiers waited on twenty wood each. The gather shares
  moved from gold to wood, and when nothing in the composition can be
  paid for, the cheapest soldier a building trains for food alone is
  trained instead. Hard builds a second Barracks.
- **A side without a Town Center keeps working.** The economy manager
  used to do nothing without one; a side that lost its Town Center stood
  idle with farms in sight, which the stuck-villager rule rightly
  refused. Home is now the Town Center, else any finished building,
  else where the villagers are, and the first thing built is a new Town
  Center.
- **The scout keeps riding.** Once every point on its rings was seen it
  stopped; now it rides them again regardless, for what has changed and
  for whoever is hiding, which is how the army finds the last villagers.
- **The limit is forty minutes**, because assaults leave at twenty and
  the hunt for the last villager takes a while. The record shows the
  earliest elimination at twenty minutes and the latest at thirty-nine.

What still ends on score: two seeds in twenty where the last villagers
are never found in time. Counters to what the enemy fields, walls, and
siege are still owed, and a human will find this opponent predictable.

## 30. Implementation notes from M6, chunk 1: the shell

- **The shell is three states around the match the app had.**
  `Shell::{Title, Setup, Match}`: the match state is the app as it was
  before M6, and the two others draw a screen and take only its buttons
  and Enter and Escape. The match code path checks the state in one
  place, the first line of each input handler.
- **A screen is built like the HUD.** `view::shell` produces sprites and
  hit rectangles in HUD pixels and scales them on the way out, as
  `Hud::build` does, so `F2` and the display scale apply and the same
  rasteriser previews it (`mapview --screen`). The app keeps the last
  built screen for hit-testing, as it keeps the last HUD.
- **`Setup::config` is the one place a choice becomes a parameter.** The
  declared bonus (`GD-AI-01`): `Setup::declared_bonus` gives
  `HARDEST_GATHER_BONUS_PCT` to a Hardest opponent and zero to everyone
  else, the human included, and the screen prints it beside that
  opponent. The `ai` crate never sees it; `simrunner versus` sets the
  same thing its own way, and both are tested.
- **The preview is a match.** The setup screen's minimap is
  `Simulation::new` on the setup, regenerated on every change (a Giant
  eight-player map in under 0.2 s). START generates once more so the
  match begins at tick 0 from the config rather than from a preview.
- **The opponents think in the app's tick loop**, each on
  `FoggedView::new(&sim, player)` and issued through
  `issue_from(.., Source::Ai)`, exactly as `simrunner versus` does. The
  player's own commands are as they were; the replay records both.
- **Menus pause** (`docs/03` §1): opening the menu remembers whether the
  clock was paused and restores that on close, so a Space pause
  survives a look at the menu.
- **Ending a live match takes two clicks.** RESIGN and QUIT TO TITLE
  arm on the first click (the label becomes CONFIRM ...) and act on the
  second; Escape or RESUME disarms. Once the match is decided QUIT
  needs no second click and RESIGN is greyed.
- **The results come up once.** `ResultsState::{Pending, Shown,
  Dismissed}`: shown when `Simulation::over` holds or the player is not
  standing, put away by KEEP WATCHING or Escape and never back; the
  pause menu's QUIT is the way out afterwards. The world keeps running
  behind the panel.
- **An empty world is a decided match.** A world with no standing side
  is over by `docs/02` §10, which the app's tests met at once: their
  worlds are empty until a test spawns into them. The test helper puts
  the results away; a real match starts with every side standing.
- **`view` depends on `ai`** for `Difficulty` and nothing else. The
  dependency runs from the presentation to the opponent, never back;
  `ai` still depends on `fogged` alone.
- **`Renderer::upload_terrain` replaces the map.** It used to add
  chunks; a Small map after a Giant one would have kept the Giant's
  edges. `Gpu::render` takes the minimap rectangle as an option, since
  the title has none and the setup's preview sits inside its panel.

## 31. Implementation notes from M6, chunk 2: save and load

- **A save is the simulation, whole.** `Simulation` already derived
  serde for the soak; the snapshot is that, with the command log it has
  carried since M0 inside it, so a save is its own replay (`TA-SAVE-01`)
  with nothing to reconcile. `crates/save` adds the opponents' minds
  (`ai` now derives serde on `Opponent` and its managers) and the camera,
  and a header repeating the seed, tick and player count.
- **Three versions, checked before the world is read** (`TA-DET-06`).
  `save::VERSION` for the file's layout, `sim::STATE_VERSION` for the
  serialised shape of `Simulation`, `Replay::VERSION` for the commands.
  A `Header` with defaulted fields is parsed first (serde ignores the
  rest), so a save from another build is refused as "simulation state
  version 9 but this build reads version 1" rather than as a parse error
  deep inside the world. A replay file parses as a header with no state
  version and is reported as not a save, which is how `simrunner verify`
  tells the two apart.
- **The alarm rate-limit moved out of scratch.** `last_alarm` steered
  when the next UNDER ATTACK could sound and lived in the unserialised
  `Scratch`, so a loaded match would have raised its next alarm at once
  and the opponents, who hear alarms, would have thought differently
  from the unsaved match. It is a `#[serde(default)]` field of
  `Simulation` now, and the round-trip test plays 300 ticks of opponents
  on both and requires the same commands and hashes every tick. It is
  still not hashed: it steers an event, never a replay's outcome.
- **`Save::verify` is the tool's check.** Replaying the log inside a save
  from tick 0 must reach the snapshot's hash; `simrunner verify` runs
  that on a save and then the usual twice-over replay, so a save from a
  bug report is checked the way a replay is.
- **Files are named for listing.** `20260919-190512-seed3-tick4321-p2.ron`
  (UTC) carries everything the load screen shows, so a directory of
  multi-megabyte saves is listed without reading one. A file with
  another name is not listed; a listed file from another build is
  refused on loading, on the screen, with its numbers. Saves live under
  `NEW_EMPIRE_SAVES`, else the platform's data directory
  (`~/.local/share/new-empire/saves`, `~/Library/Application
  Support/new-empire/saves`, `%APPDATA%\new-empire\saves`), else
  `saves` under the working directory; no crate was added for that.
- **Loading is entering a match.** `start_match` and `resume` share
  `enter_match`: a fresh clock and selection, the camera over the
  player's start, the age remembered as the one the player is in (a
  loaded Bronze Age match does not celebrate the Bronze Age), the results
  pending. `resume` then puts the camera back where the save left it.
- **Dates without a crate.** The civil-date arithmetic for the file
  names and the list is twenty lines (Hinnant's algorithms), in UTC and
  saying so; a local time zone would need a dependency and is not worth
  one for a file name.

## 32. Implementation notes from M6, chunk 3: replay playback

- **Playback is the match state with a different source of commands.**
  `Playback { replay, next }` issues the log's commands at their ticks
  through `issue_from` with their recorded source, exactly as
  `Replay::run` does, and the opponents list is empty. Everything else
  (the scene, the HUD, the fog, the minimap, the camera, the clock) is
  the match's own code; the state machine did not grow a state for it.
  The clock stops at the recording's last tick and the results come up.
- **Nothing the watcher does reaches the log.** `issue` returns at once
  in playback, the HUD's buttons and letter hotkeys are ignored,
  right-click and Delete return early, F5 does not save. Selection
  still works, so a unit or building can be looked at. The test issues
  every one of those and requires the command count unchanged.
- **The viewer is a field, not a constant.** `viewer: Option<u8>` is
  `Some(ME)` in a match; in a replay Tab cycles it through every player
  and then `None`, which draws with `FogLights::lit` and an unfogged
  minimap and scene. The HUD's panel, the age banner, the VICTORY and
  DEFEAT banner and the alarm follow `hud_player()`, the viewer or the
  human. The match code paths that name `ME` for orders are untouched;
  they cannot issue in playback anyway.
- **One recording per match.** The recording is written at three
  moments: when the match is decided (so a window closed on the results
  loses nothing), when the match is left, and when the window is closed
  on a live match; each write replaces the match's earlier file, whose
  name differs only by the tick. A match that never ticked is not
  recorded, and a replay being watched is never recorded again.
- **A recording is a replay file, nothing more.** `save::replays` writes
  `Simulation::replay()` as compact RON under a name shaped like a
  save's, and `read` validates it as `Replay::validate` does, version
  included (`TA-DET-06`), then runs the engine's config check. The
  difficulties are not in a replay (`Replay` has no field for them and
  adding one would change the format), so the results name sides by
  number.
- **Sixteen times.** `FixedClock` allows eight ticks per frame, which at
  sixty frames is twenty-four times real time; the replay's speed cap is
  sixteen and a match's stays at eight.

## 33. Implementation notes from M6, chunk 4: settings

- **Keys are names.** A binding is the key's name as `winit` prints it
  (`KeyW`, `F5`, `BracketRight`), so the settings model lives in `view`
  with no dependency on the windowing crate, the file is readable, and
  the app compares names on a key press. Only the four pan keys need
  turning back into keys, since the camera reads them while held;
  `app::keys::code` knows every bindable key and refuses the rest.
- **What a general key may not take.** `hud::command_letters` is every
  letter a command button may carry, gathered from the same tables the
  buttons are built from, so `Settings::bind` refuses a letter the
  panels use and stays right when a table changes. Escape is the menu
  and the digits are the control groups, so those are refused too, and
  a key already bound to another control is refused with that control's
  name. A refusal changes nothing.
- **The overlay reads the bindings.** `hud::controls` takes the settings
  and names the bound key on every general row; the pan row reads
  `WASD` when the four keys are single letters and lists them otherwise.
  The overlay and the settings screen cannot disagree.
- **Applied and kept at once.** Every change on the screen, and the
  HUD-size and edge-scroll keys in a match, go through
  `apply_and_save_settings`: into effect (the HUD scale, edge scrolling,
  the pan keys, the window mode) and onto disk. There is no OK button to
  forget. A file that cannot be written reports on the screen and the
  change still holds for the session.
- **A file never fails for being old.** `#[serde(default)]` on
  `Settings` gives a missing field its default, so a build that adds a
  setting reads last week's file; a file that does not parse is left
  alone, reported, and the defaults stand in. This is the other side of
  the save format's strictness: a save read wrong corrupts a match, a
  setting read wrong is a key.
- **Shift+E became F3.** A binding is one key without modifiers, and E
  is a panel letter; the edge-scroll toggle needed a key of its own.
  Home took over the Town Center jump from H for the same reason.
- **The window mode is winit's borderless fullscreen**, set at creation
  and on the change; nothing else about video is a setting yet, since
  the renderer has no options to expose.

## 34. Implementation notes from M6, chunk 5: notifications

- **A notice is a line, a kind, a tick and maybe a tile.** `Notices`
  lives in `view` with no dependency on the app or the clock: time is
  match ticks, so the stack stands still while the match is paused and
  runs at the match's speed in a replay. The app raises notices from
  what it already watched for the banner: the simulation's alarm and
  death events for the viewer's side, an age change, and a longer
  `researched` list (ages excepted, since the age is its own notice).
- **One per area per twenty seconds** (`UX-NOTIFY-01`). The simulation
  raises an alarm at most every ten seconds per player; the stack drops
  an attack within twelve tiles and twenty seconds of an attack already
  on it, and does not refresh the older one, so a siege is a notice
  every twenty seconds rather than none at all once it starts. Other
  kinds are not limited: two houses falling are two notices.
- **A notice with a place is a HUD button.** `Action::Jump(row)` joins
  the HUD's actions; the app checks the stack's buttons before the
  world gets a click, since the stack sits in the world area rather
  than on a panel, and `hotkey` never matches them, their key being a
  space. The row index is into the shown slice, which the app passes
  and reads back.
- **The acceptance run is a test.** `RM-M6-01` asks for a skirmish to
  a victory screen, a save, a reload and a replay without a terminal.
  The test goes through the same buttons and key handlers the window
  calls, with two liberties a test takes: the army is spawned rather
  than trained, and the hunt for the last of the enemy reads the world
  where a player would read the minimap. Everything the shell does is
  exercised as the player does it; the sprites and the window are not,
  which is what the Mac pass is for.


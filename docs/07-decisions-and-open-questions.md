# Decisions and Open Questions

A running log. Decisions are recorded with their reasoning so we can revisit them
knowing *why*, not just *what*.

---

## Decisions taken

### D1 — Native desktop, written in Rust
**Date:** 2026-09-05 · **Decided by:** Bo

Alternatives considered: browser/TypeScript, Godot 4, Unity.

Rust gives us the control needed for a bit-exact deterministic simulation
(fixed-point maths, explicit memory layout, no hidden allocation) and a fast
build/test loop. The cost is that running a build needs a local toolchain. The
same `winit` + `wgpu` stack compiles to WebAssembly, so a browser build for
playtesting stays available without changing the simulation.

### D2 — Vertical slice first
**Date:** 2026-09-05 · **Decided by:** Bo

One map type, 2 civilizations, 3 ages, ~12 unit types, skirmish against AI —
playable end to end, then widened. The full design is specified so we build
toward it, but M7 is the first date anything is judged.

### D3 — Single-player first, on a multiplayer-ready architecture
**Date:** 2026-09-05 · **Decided by:** Bo

Deterministic lockstep from day one: fixed-point maths, seeded RNG, commands
scheduled two ticks ahead, per-tick state hashes. Multiplayer later becomes a
transport and lobby problem rather than a rewrite. This is the architecture from
*"1500 Archers on a 28.8"*, and it also buys us replays and reliable saves
immediately.

### D4 — Original pixel art to a strict spec
**Date:** 2026-09-05 · **Decided by:** Bo

We cannot ship Microsoft's assets. We author our own to the constraints in
`docs/05-art-and-audio-spec.md` — isometric 2:1, 64×32 tiles, 8 facings from 5
mirrored, indexed palette with reserved player-colour ramp. Placeholder shapes
render from M1 so gameplay is never blocked on art.

### D5 — One drop-off building instead of Granary + Storage Pit
The original's split was bookkeeping, not depth. The Storehouse accepts
everything. Placement distance remains the real economic skill.

### D6 — Full modern command vocabulary
Attack-move, patrol, waypoints, formations, unit queueing, stances, uncapped
selection, rally-onto-resource. All absent in 1997, all now assumed. Pillar 3.

### D7 — The AI does not cheat below Hardest
It issues the same commands a player can and reads the same fogged view,
enforced architecturally by the `ai` crate only receiving a `FoggedView`. An
opponent that cheats teaches you nothing.

### D8 — Population cap defaults to 75
Low caps keep individual units meaningful, battles readable at our sprite scale,
and the simulation cheap. Configurable 50–200.

### D9 — Hand-written RNG and fixed-point; `serde` is the sim's only dependency
**Date:** 2026-09-05

`xoshiro256**` and Q16.16 arithmetic are small enough to own outright, and
owning them means no dependency can change the simulation's output under a
version bump. `scripts/check-sim-purity.sh` fails CI if anything else appears
in `crates/sim`'s dependency tree, if a float type or literal appears in its
source, or if it touches the clock or a `HashMap`.

### D10 — Fixed-point division rounds to nearest, not toward zero
**Date:** 2026-09-05

Found during M0: truncating division made every movement step fractionally
short, so a 60-tick walk at 1 tile/s ended at 2.9992 tiles. Rounding to nearest
(halves away from zero) is unbiased and lands on the integer the design says.
All `Fx` divisions and `mul_div` share one rounding routine.

### D11 — Rust over C++
**Date:** 2026-09-05 · **Decided by:** Claude, within Bo's "Rust or C++" choice

The bit-exact simulation and the data-oriented entity store are where Rust's
guarantees (no hidden allocation, no implicit float promotion, `#![deny]`-able
lints, `cargo test` in CI on three OSes) pay off most. `wgpu`/`winit` also give
one renderer for Windows, macOS, Linux and — as a secondary target — the web.

### D12 — Map generation lives in the simulation crate
**Date:** 2026-09-05

Planned as its own crate; moved into `sim::mapgen` because it depends only on
the sim's RNG and fixed-point maths, and because it lets a replay carry a seed
and a spec instead of a map. The `sim` dependency allowlist is unchanged.

### D13 — A software rasteriser is the renderer's reference
**Date:** 2026-09-05

The GPU cannot be exercised in CI or in the environment this is being built
in, so `view::raster` draws the same vertex buffers and sprite instances the
GPU receives, and `tools/mapview` writes the result to PNG. Every frame the
GPU shows should match a `mapview` render of the same camera; when it does
not, the renderer is wrong.

### D14 — Flow fields deferred to M4
**Date:** 2026-09-05

A\* with line-of-sight shortcuts, string-pulling and per-tick budgets moved
60 villagers across a forested map with none stuck. Flow fields are a
throughput optimisation for many units sharing a destination; armies are
where that matters, so they arrive with combat rather than adding surface
now.

### D15 — Hunting waits for combat
**Date:** 2026-09-05

Animals need to be killed before they are food, and killing is M4. Berries,
trees, stone and gold cover "gather all four resources" for the slice.

---

## Open questions

### Q1 — Naval in the vertical slice, or after?
Water doubles the pathfinding surface (separate navigation domain, transports,
shore-landing edge cases) for one map type. **Recommendation:** hold until M8.

### Q2 — Relics: static (AoE1 ruins) or carryable (AoE2)?
Carryable relics create better fights over specific objects; static ruins are
simpler and match the original. **Recommendation:** carryable, held in the
Temple, generating gold — it gives priests a second job and creates map tension.

### Q3 — Does the Government Centre earn its own building?
Its upgrades could fold into the Town Center, saving a building and a data
table. Counter-argument: a separate building is a real strategic investment and
a target. **Undecided.**

### Q4 — Campaign fiction: written by us, or straight history?
Straight history is free, accurate and evocative. Original fiction gives us
narrative control. **Recommendation:** history, told through a single narrator,
in the style of the original's campaign intros.

### Q5 — What is the game actually called?
"New Empire" is the repository name and a placeholder. Worth deciding before
there is a main menu (M6).

### Q6 — Where does the art come from in practice?
Generated, commissioned, or drawn by hand? The spec is source-agnostic and the
atlas tool validates conformance, but the answer changes cost and schedule for
M7 substantially. **Needs an answer before M3.**

### Q7 — Music: licensed, commissioned, or generated?
Five stems plus four fanfares. Smaller than the sprite problem but on the same
critical path for M7.

### Q8 — Do we want a hard 4-age structure, or a 5th age?
The original's four ages map cleanly onto ancient history and end at a natural
place. A fifth (Classical/Imperial) would extend matches past 40 minutes.
**Recommendation:** stay at four. Long matches were not the appeal.

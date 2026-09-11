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

### D16 — Art is modelled and rendered, not drawn or prompted
**Date:** 2026-09-05 · **Decided by:** Bo

Closes Q6. Alternatives considered: direct AI sprite generation via the
purpose-built APIs, permissively licensed asset packs, and commissioning the
whole inventory. Reasoning in full in `docs/08-art-production.md`.

The short version is that the slice needs ~4,400 individual images and the full
game ~8,100, and the hard part is not producing any one of them — it is that
every one has to agree with every other about light, palette, anchor,
proportion, costume and the identity of the subject across five angles and
thirty frames. Modelling and rendering makes all six consistent by
construction: you author ~24 rigged models and the frames fall out of a render
script. Per-age costume variants, which `docs/05` §2.5 calls the largest art
cost in the project, become a mesh swap.

Two supporting reasons that are easy to overlook. Age of Empires' own sprites
were modelled in 3D Studio Max and converted to 2D, so this route reproduces the
method that produced the look we are after — which is how you get the feel
without copying anything, since there is nothing to copy from. And purely
prompt-generated output is not copyrightable in the US, so sprites made that way
could be lifted out of our data files by anyone.

Generative AI keeps a real job upstream of the frames: concept and costume
exploration, model textures, and the icons, which are single static images with
no coherence problem.

### D17 — Placeholders are generated as files, not at runtime
**Date:** 2026-09-05

`docs/05` §6 said placeholder shapes would be generated at runtime. They are
generated as indexed PNGs with manifests instead, by `atlas placeholder`.

Runtime shapes prove the renderer can draw a diamond. Files prove the pipeline —
manifest, sheet layout, indexed palette, player-colour remapping, anchors, five
authored facings — which is step 2 of the same production plan, and they go
through `atlas validate` exactly as real art will. The gate is therefore
load-bearing from M1 rather than from the first drawn sprite. The cost is a
build step; the generated art is not committed, because it is derived from the
palette and a committed copy could only go stale.

### D18 — Player colours are searched, not chosen
**Date:** 2026-09-05

`docs/05` §2.4 required the eight player colours to be checked against
deuteranopia and protanopia simulation. Doing that to hand-picked colours failed
badly — pairs collapsed to a tenth of the separation they had in normal vision.

Hue alone cannot separate eight owners for a dichromat. So the ramps are
generated: hue pinned near each colour's name so it still answers to it, then
lightness and chroma optimised to maximise the worst pair across normal vision
and both simulated dichromacies. The measured worst separations (0.086 normal,
0.076 protanopia, 0.072 deuteranopia) are asserted in `cargo test`, so an edit
that closes a gap fails the build. Owners are also assigned in palette order and
the first four held to roughly twice the bar, because most matches never reach
the fifth colour.

### D19 — One palette: the renderer bakes `assets/palette/ancient.ron`
**Date:** 2026-09-11 · Resolves Q10

`atlas export --rust` writes the baked palette into
`crates/view/src/palette_table.rs`, committed and policed by
`scripts/check-generated.sh`. `view::palette`'s named constants are now
indices into the real ramps ("timber, step 4") rather than colours of their
own, and terrain colours come from the terrain ramps. The renderer's
hand-picked player colours, which failed dichromacy simulation, are gone;
the searched ramps from D18 are what draws.

Two additions to the palette contract came with it. Index 239 is a new
`shadow` special, drawn translucent (a 40% black multiply) by both renderers
and opaque black by any other tool; the reserve shrinks to 233–238. And
`atlas repalette` rewrites a committed sheet's PNG palette chunk against the
current palette without touching its indices, for the case where a colour
moves but the layout does not — which is exactly what adding `shadow` did to
the committed villager sheet.

The same change loads rendered sprite sets from `assets/sprites` into the
game: `view::sheets` reads a manifest and its indexed PNG, checks the sheet
was exported against this build's palette, and the atlas packs its frames
in place of the procedural placeholder for that kind, with the set's
animations (idle, walk, work, death, decay) driven by what the unit is
doing. Art authored at 2× draws at 1× size and is crisp at 2× zoom, as
`docs/05` §2.1 intended.

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

### Q6 — Where does the art come from in practice? — **answered, see D16**
Modelled and rendered, with generative AI upstream of the frames and
commissioning for icons and UI. Full reasoning and the researched alternatives
are in `docs/08-art-production.md`.

### Q7 — Music: licensed, commissioned, or generated?
Five stems plus four fanfares, plus the sound inventory in `docs/05` §5.1.

D16's reasoning does not transfer. Audio has no equivalent of the coherence
problem that decided the sprite question — a fanfare does not have to agree with
the next fanfare about anything but key and instrumentation — and the volume is
two orders of magnitude smaller. The copyright and Steam disclosure positions in
`docs/08` §5 *do* transfer unchanged, and they cut against generation for
anything shipped.

**Recommendation:** commission the nine music cues, and treat the sound-effect
inventory separately — it is large, repetitive and mostly foley, which is what
licensed libraries are good at. **Still needs a decision before M6.**

### Q8 — Do we want a hard 4-age structure, or a 5th age?
The original's four ages map cleanly onto ancient history and end at a natural
place. A fifth (Classical/Imperial) would extend matches past 40 minutes.
**Recommendation:** stay at four. Long matches were not the appeal.

### Q9 — What is the second ownership cue, besides colour?
Player colour is currently the only way to tell whose unit is whose, and
`docs/08` §6 shows that eight colours cannot be separated comfortably for a
dichromatic player — the worst pair sits at 0.072 Oklab, against 0.15 for the
first four owners. Ordering owners so small games use only the well-separated
colours helps but does not solve an eight-player game.

Options: a per-owner shape badge on the selection ring; a hatch or outline
pattern keyed to the owner; a dedicated high-contrast palette selectable in
options, as most modern RTS games ship. The first is cheapest and does not touch
the art. **Needs an answer before the HUD work in M6.**

### Q10 — There are two palettes, and they disagree — **answered, see D19**
**Filed:** 2026-09-05 · Resolved 2026-09-11 by baking the art palette into the renderer.

M1 and the art pipeline were built in parallel and each grew a 256-colour
palette. They agree on the two things that matter structurally and on nothing
else:

| | `crates/view/src/palette.rs` | `assets/palette/ancient.ron` |
|---|---|---|
| Index 0 transparent | yes | yes |
| Player ramp | 240–247 | 240–247 |
| Everything else | hand-written constants at sparse indices (10–13 browns, 20–22 greens, 30–32 greys…) | 27 material ramps of 8 steps, Oklab-interpolated, densely packed 17–232 |

So a sprite that passes `atlas validate` and is then drawn by the renderer comes
out with the wrong colours at every index except transparency and player colour.
Nothing is visibly broken today only because no art has gone through the
pipeline into the renderer yet — the placeholder atlas in `view` is drawn from
`view`'s own constants.

The accessibility half is measured rather than argued. `view`'s
`PLAYER_COLOURS` carries the comment "Chosen to stay distinct under
deuteranopia and protanopia simulation; verify again when art lands". Verified:

| Palette | Worst of all eight | Worst of the first four |
|---|---|---|
| `crates/view` | **0.024** (green/orange, protanopia) | 0.054 (red/green, deuteranopia) |
| `ancient.ron` | 0.075 (magenta/grey, deuteranopia) | 0.163 (green/yellow, protanopia) |

0.024 Oklab is below the threshold at which two colours are tellable apart at
all, so under protanopia `view`'s green and orange players are the same colour.
The instinct in that comment was right; the colours were picked by eye.

**Recommended fix:** `crates/view` stops holding its own table and takes its
colours from `assets/palette/ancient.ron`, baked into a generated Rust file so
there is no runtime I/O in the renderer. `view`'s named constants stay — they
become named indices into the real ramps rather than colours in their own
right. That keeps one palette, keeps the searched player colours, and keeps the
per-age material progression `docs/05` §2.5 depends on. The cost is remapping
M1's placeholder atlas and terrain colours onto the new indices, which is
mechanical.

Rejected: making `ancient.ron` adopt `view`'s colours. It would drop the player
separation back to 0.024 and throw away the ramp structure the art spec is
built on.

**Deferred by Bo, to its own PR.** Nothing renders through the art pipeline
yet, so this is not urgent — but it becomes urgent the moment the first real
sprite reaches the renderer, which is the greybox unit in `docs/08` §9 step 2.
Whoever does that work should expect to do this first.

# Roadmap

Ordered so that **something is playable as early as possible** and every
milestone after that makes it more playable. Each milestone has acceptance
criteria that are demonstrable, not subjective — if we cannot show it, it is not
done.

Effort estimates are relative sizes, not calendar dates.

---

## M0 — Foundation

*Nothing to play. Proves the stack.*

- Cargo workspace with the crate layout from `docs/04-technical-architecture.md`
- Window, event loop, `wgpu` device, clear to a colour
- Fixed-point `Fx` type (Q16.16) with full test coverage: add, mul, div, sqrt,
  distance, trig lookup tables
- Sim tick loop at 20 Hz with a render-side interpolation accumulator
- `Command` queue with the 2-tick execution delay
- Entity store (dense arrays, generational IDs, deterministic slot reuse)
- State hashing + `tools/simrunner` replaying a log twice and asserting equality
- CI: build, test, clippy, the `sim` no-float check, the `sim` dependency check

**Done when:** `simrunner` replays a synthetic 10,000-tick command log twice and
every per-tick hash matches, on all three platforms.

**Size:** Medium.

**Status:** landed. `simrunner determinism --ticks 10000` passes locally
(4 players, 400 units, ~4,500 commands, ~28 µs/tick); CI runs it on Linux,
Windows and macOS and asserts the three final hashes are identical. The
window opens and clears to a colour, and drives the sim at 20 Hz from a
fixed-timestep clock (Space pauses, `+`/`-` change speed). Not yet verified
on a real GPU from this environment — first thing to check on a desktop.

---

## M1 — A world you can look at

*First screenshot.*

- Tile map with terrain types and elevation
- Chunked terrain renderer with edge blending
- Isometric camera: edge scroll, keyboard, drag, discrete zoom, minimap jump
- Instanced sprite pipeline with palette remapping and horizontal mirroring
- Procedural placeholder sprites (coloured diamonds, correct sizes and anchors)
- `mapgen` producing a seeded Inland map with resources and balanced starts
- `tools/mapview` to eyeball generated maps

**Done when:** we can generate a map from a seed, scroll and zoom around it
smoothly at 60 fps, and the same seed always produces the same map.

**Size:** Medium.

**Status:** landed, with one caveat. Map generation lives in `sim::mapgen`
(same seed, same map, verified by test and by the determinism run, which now
executes over a populated Inland map). The renderer draws Gouraud terrain
chunks, palette-remapped instanced sprites and a diamond minimap; the camera
edge-scrolls, drags, zooms in steps and jumps from the minimap. Placeholder
sprites for every kind go through the real atlas pipeline (indices, anchors,
five facings mirrored to eight). `tools/mapview` renders frames to PNG via a
software rasteriser that shares all the maths, and those frames look right.
The caveat: the GPU path has only been validated by compiling and by naga
shader validation — the "60 fps in a window" half of the acceptance test is
still waiting on a machine with a display.

---

## M2 — Villagers, movement, economy

*First time it feels like a game.*

- Selection: click, band-box, control groups, stable ordering
- Right-click contextual commands with cursor feedback
- Pathfinding: sector graph + flow fields + local avoidance
- Gathering all four resources, carry capacity, drop-off routing
- Building placement, construction, multi-villager builds
- Houses and the population cap
- Resource bar and the selection panel

**Done when:** a player can select villagers, gather all four resources, build a
house and a Storehouse, and 60 villagers can be ordered across the map without
one getting permanently stuck. Pathfinding property tests pass on adversarial
maps.

**Size:** Large. *This is the milestone that decides whether the game feels good.*

---

## M3 — Ages, production and technology

- Town Center producing villagers, with a queue and rally points (including onto
  resources)
- Age advancement with the resource-and-buildings gate
- Technology research at buildings, applied through `PlayerModifiers`
- Farms with auto-reseed
- Age-up presentation: fanfare, light sweep, building and unit sprite swaps
- Full command panel with the context-sensitive command grid

**Done when:** a player can go Stone → Tool → Bronze in a live match, and the
settlement visibly changes at each transition.

**Size:** Medium.

---

## M4 — Combat

- Military units, training buildings, unit stats from data
- Attack orders, projectiles, the damage model with armour, bonuses and elevation
- Stances, attack-move, patrol, formations
- Death, corpses, building destruction, rubble
- Towers, walls, gates
- Villagers fleeing and raising an alarm

**Done when:** two forces of 40 units fight, the result is readable, counters
work as designed, and no unit gets stuck during combat.

**Size:** Large.

---

## M5 — An opponent

- AI: build-order planner, economy manager, military manager, scouting
- `FoggedView` enforcement — the AI physically cannot read hidden state
- Four difficulty levels
- Victory and defeat conditions, elimination, resign

**Done when:** `simrunner` runs 20 headless AI-vs-AI matches with no crashes, no
stuck units, and Hard beats Easy at least 18 times out of 20.

**Size:** Large.

---

## M6 — Game shell

- Main menu, skirmish setup (map, size, civ, difficulty, pop cap, victory
  conditions), loading, results screen
- Save and load
- Replay recording and playback with speed controls
- Settings: audio, video, hotkey rebinding, UI scale
- Notification system with click-to-jump

**Done when:** a player can launch the game, configure and play a full skirmish
to a victory screen, save mid-match, reload, and watch the replay — without ever
touching a terminal.

**Size:** Medium.

---

## M7 — The feel pass ← **vertical slice complete**

*The milestone that decides whether this is the game you remember.*

- Full audio: acknowledgments, work loops, positional world SFX, ambience,
  age fanfares, music stems, combat ducking
- Real sprite art for 2 civs across Stone/Tool/Bronze
- All animations including villager task and carry variants
- Hit reactions, death animations, construction stages, dust, resource depletion
- Notification polish, tooltips, first-time hints
- Performance pass against the budgets in `docs/04-technical-architecture.md`

**Done when:** someone who loved the original plays a full match and does not
want to stop. That is a real acceptance test and we should run it on actual
people.

**Size:** Large.

---

## M8 — Breadth

- Iron Age and its full unit and tech roster
- All 8 civilizations with bonuses and tech-tree denials
- Remaining map types: Coastal, Continental, Highland, Islands, Narrows, Oasis
- Naval: docks, fishing, transports, warships, water maps
- Priests and conversion
- Wonder and Relic victory conditions
- Cheat codes

**Size:** Large.

---

## M9 — Content and community

- Campaign system: scenarios, objectives, triggers, narration
- The learning campaign (4 scenarios, one concept each)
- Two historical campaigns
- Scenario editor with save, load and playtest
- Multiplayer: lockstep transport over the existing command-turn architecture,
  lobby, adaptive turn length, desync detection using the state hashes we
  already compute

**Size:** Very large. This is a second project's worth of work, and it is where
the game gets its long tail.

---

## Sequencing notes

- **M2 is the risk.** Movement and pathfinding are where RTS games live or die,
  and where the original's reputation suffered most. If it takes twice as long
  as estimated, that is money well spent.
- **M7 is not optional polish.** Pillar 4 says the feel *is* the product. A
  functionally complete game that skips M7 is not the game we set out to build.
- **M5 before M6** deliberately: an opponent to lose to teaches us more about the
  design than a menu does.
- Art can proceed in parallel from M1 onward, against
  `docs/05-art-and-audio-spec.md`, because placeholders unblock all gameplay work.

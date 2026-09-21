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

**Done when: [RM-M0-01]** `simrunner` replays a synthetic 10,000-tick command log twice and
every per-tick hash matches, on all three platforms.

**Size:** Medium.

**Status:** landed. `simrunner determinism --ticks 10000` passes locally
(4 players, 400 units, ~4,500 commands, ~28 µs/tick); CI runs it on Linux
and macOS and asserts the final hashes are identical (Windows was a third
leg until 2026-09-21). The
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
  — **done ahead of M1**: `atlas placeholder` generates 26 conformant sets
- `mapgen` producing a seeded Inland map with resources and balanced starts
- `tools/mapview` to eyeball generated maps

**Done when: [RM-M1-01]** we can generate a map from a seed, scroll and zoom around it
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

**Done when: [RM-M2-01]** a player can select villagers, gather all four resources, build a
house and a Storehouse, and **[RM-M2-02]** 60 villagers can be ordered across the map without
one getting permanently stuck. **[RM-M2-03]** Pathfinding property tests pass on adversarial
maps.

**Size:** Large. *This is the milestone that decides whether the game feels good.*

**Status:** landed, with the same caveat as M1 (the GPU window is verified by
compilation and software renders, not by eyes on a screen). Simulation tests
cover every acceptance item: 60 villagers cross an Inland map and all arrive
within the spread radius with none stacked; villagers gather all four
resources and deliver them; a house raises the cap and a storehouse becomes
the nearest drop-off; a wall of trees is detoured around; a sealed pocket
resolves to the nearest reachable tile; a placed building blocks its tiles
at once and refunds on cancel; training respects the population cap and
rally points send new villagers straight to work. Two things the roadmap
listed are deferred: group flow fields (A\* with line-of-sight shortcuts and
a per-tick node budget met the acceptance test without them; they return
with M4's armies) and hunting (animals need combat to die first). Selection
is click, drag, double-click, shift, and ten control groups; the `.` key
cycles idle villagers.

---

## M3 — Ages, production and technology

- Town Center producing villagers, with a queue and rally points (including onto
  resources)
- Age advancement with the resource-and-buildings gate
- Technology research at buildings, applied through `PlayerModifiers`
- Farms with auto-reseed
- Age-up presentation: fanfare, light sweep, building and unit sprite swaps
- Full command panel with the context-sensitive command grid

**Done when: [RM-M3-01]** a player can go Stone → Tool → Bronze in a live match, and the
settlement visibly changes at each transition.

**Size:** Medium.

**Status:** landed, with the same caveat as M1 and M2 (the GPU window is
verified by compilation and the software rasteriser). Ages advance through a
`Research` command at the Town Center, gated on resources and on two finished
buildings of the current age — Houses, the Town Center and Farms do not
count. Technologies queue at the Storehouse and Market alongside villagers
and apply through `Player::modifiers` (gather rate per resource, carry
capacity, farm yield, villager speed, build speed). Farms hold 250 food, are
seeded for free on completion, and reseed for 60 wood when they run dry while
the owner's auto-reseed is on; the resource bar warns when the wood is not
there. Ten buildings join the roster (Barracks, Farm, Archery Range, Stable,
Market, Watch Tower, Temple, Academy, Siege Workshop, Government Centre),
placeable from the age that unlocks them. The command panel is a five-by-three
grid that lists what the selection can do, greys what it cannot with the
reason, and carries a queue strip; an age-up swaps every building and
villager to its new-age variant, sweeps a light across the settlement and
raises a banner. Deferred: the fanfare (there is no audio yet), a per-farm
reseed toggle (the toggle is per player), the Town Center as a buildable
(it waits on Q3), and rendered-art age variants (the placeholder buildings
have them; the villager sheet does not).

---

## M4 — Combat

**Status: landed 2026-09-13.** PRs #9 and #10 completed the bounded 40-versus-40
acceptance, equal-budget counter trials and combat readability pass. The owner
approved the native Apple M4 Mac playtest; its replay reproduced identically.
Results, CI evidence and retained scope limits are recorded in `docs/10`.

- Military units, training buildings, unit stats from data
- Attack orders, projectiles, the damage model with armour, bonuses and elevation
- Stances, attack-move, patrol, formations
- Death, corpses, building destruction, rubble
- Towers, walls, gates
- Villagers fleeing and raising an alarm

**Done when: [RM-M4-01]** two forces of 40 units fight, the result is readable, counters
work as designed, and no unit gets stuck during combat.

**Size:** Large.

---

## M5 — An opponent

- AI: build-order planner, economy manager, military manager, scouting
- `FoggedView` enforcement — the AI physically cannot read hidden state
- Four difficulty levels
- Victory and defeat conditions, elimination, resign

**Done when: [RM-M5-01]** `simrunner` runs 20 headless AI-vs-AI matches with no crashes, no
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

**Done when: [RM-M6-01]** a player can launch the game, configure and play a full skirmish
to a victory screen, save mid-match, reload, and watch the replay — without ever
touching a terminal.

**Size:** Medium.

**Status: landed 2026-09-19**, with the same caveat as the milestones before
it: the shell is verified by the app's own handlers and the software
rasteriser. The owner played the downloadable build on the Mac on
2026-09-20 (the Mac build workflow, `docs/10`) and reported that it plays
well; the save, load and replay steps by hand there are not separately
confirmed. The game opens on
a title screen; a skirmish is set up (map size, one to seven opponents each
at a difficulty with the Hardest bonus declared beside it, population cap,
seed, the map previewed) and played against opponents thinking in the
app's tick loop, with a pause menu, resign, and a results screen; F5 or
the menu saves the match and LOAD GAME resumes it, with the opponents'
minds and the camera, refused by version from another build; every match
is recorded and WATCH REPLAY plays it back with pause, speed and whose
eyes; SETTINGS holds the HUD size, edge scrolling, the window mode and
every general key; a notification stack in the lower left names attacks,
losses, research and ages and jumps the camera on a click. The acceptance
run is `crates/app/src/tests.rs`: title to setup to a skirmish played to
VICTORY, a save in the middle, the save reloaded, the recording watched to
its last tick, every step through the screens' buttons and the window's
handlers. Deferred: civilisation, teams, victory conditions and starting
age on the setup screen (the content they need is M8's); the panels'
command letters are not rebindable (`GD-A11Y-02` is met for the general
keys); no audio settings, there being no audio; no seeking in a replay;
the game's name (`docs/07` Q5).

---

## M7 — The feel pass ← **vertical slice complete**

*The milestone that decides whether this is the game you remember.*

**Status: in progress since 2026-09-20.** Chunk 1, the audio engine,
landed: four buses, the units answering orders and selection, the world
heard where it is through the fog, buttons and refusals, the bell and the
fanfares, every sound a synthesised placeholder until the recordings
exist (`docs/07` Q7), the volumes on the settings screen. Chunk 2, the
score and the beds, landed 2026-09-21: a stem per age cross-fading on
age-up, the combat stem over a fight in view, an ambient bed per kind of
ground under the camera, placeholders all. Chunk 3, the visual
feedback, landed 2026-09-21: the flinch and the spark, the kill puff,
the collapse cloud, the hammer's dust, three construction stages, nodes
thinning and trees falling, the chevron at the screen's edge for an
attack out of view. Chunk 4, tooltips, hints and the rest of the
notifications, landed the same day: every unit, building and
technology tooltip with cost, time, counters and key (`UX-TIP-01`),
five first-time hints each shown at most twice, the idle chime, the
resource flash for a refused click, the minimap's flash and ping. Chunk
5, the performance pass, landed the same day: the tick measured by phase
against the budgets of `docs/04` §12 on two new benchmark scenarios (400
soldiers fighting; eight Hard opponents on a full world, their thinking
timed), the fog of war made incremental and the target search bucketed,
which halved the tick on the eight-player maps, the ceilings lowered to
match, an `F4` readout in the app for the measurement on a real Mac, and
the `RM-M7-01` observation sheet (`docs/09` §9.1). The plan and the
record are in `docs/10` §4d. What remains of M7 is the owner's: the Mac
measurement, the six players, and the real sprite art and its animations,
which wait on the art pipeline (`docs/08` §9 step 3) and need a modeller
and Blender.

- Full audio: acknowledgments, work loops, positional world SFX, ambience,
  age fanfares, music stems, combat ducking
- Real sprite art for 2 civs across Stone/Tool/Bronze
- All animations including villager task and carry variants
- Hit reactions, death animations, construction stages, dust, resource depletion
- Notification polish, tooltips, first-time hints
- Performance pass against the budgets in `docs/04-technical-architecture.md`

**Done when: [RM-M7-01]** someone who loved the original plays a full match and does not
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
  `docs/05-art-and-audio-spec.md` and `docs/08-art-production.md`, because
  placeholders unblock all gameplay work. The first art task is not a sprite: it
  is freezing the render camera and light rig, which everything after it depends
  on (`docs/08` §9).

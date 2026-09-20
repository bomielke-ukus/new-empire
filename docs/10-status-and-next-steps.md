# 10 — Status and next steps

**As of 2026-09-13.** The living summary of where the project is and what
comes next, for anyone joining or checking in. The roadmap (`docs/06`) holds
the milestone definitions and their acceptance tests; this document says
which of them are done, what was learned, and what the next steps are. Update
it whenever a milestone lands or the plan changes.

**Target platform: macOS.** Development, live testing and release acceptance
focus on the Mac. Existing Windows/Linux CI jobs remain additional
portability and determinism checks, without a shipping commitment.

---

## 1. Where we are

Six milestones landed; the vertical slice wants only the feel pass, M7.

| Milestone | Status | Acceptance |
|---|---|---|
| M0 — Foundation | Landed | `simrunner determinism --ticks 10000`: a synthetic match replays bit-identically |
| M1 — A world you can look at | Landed | A map from a seed, scrolled and zoomed, rendered by the GPU path and the software rasteriser alike |
| M2 — Villagers, movement, economy | Landed | Gathering all four resources, building, training with rally points, on a pathfinder that does not get stuck |
| M3 — Ages, production and technology | **Landed 2026-09-11** | Stone → Tool → Bronze in a live match, with the settlement visibly changing at each transition |
| M4 — Combat | **Landed 2026-09-13** | Two forces of 40 fight; counters work; no unit stalls in the acceptance arena; native readability approved |
| M5 — An opponent | **Landed 2026-09-18** | 20 headless AI-vs-AI matches, Hard beats Easy 18 of 20: 20 of 20, 18 by elimination, in CI |
| M6 — Game shell | **Landed 2026-09-19** | Configure, play, save, reload and watch a replay without a terminal: the run in `crates/app/src/tests.rs`; the owner played the Mac build 2026-09-20 |
| M7 — The feel pass | **In progress since 2026-09-20** | Chunk 1, audio, landed; music, visual feedback, tooltips and hints, the performance pass and the playtest follow (§4d) |
| M7 — The feel pass | Not started | Someone who loved the original plays a match and does not want to stop |
| M8 — Breadth, M9 — Content | Not started | Beyond the vertical slice |

The Mac checks of 2026-09-12 exercised the economy and age progression
(§3, row 4), then the native combat/siege window (work record below).
The second pass confirmed garrison/ungarrison, dragged walls, gate replacement
and passage, attacks, rubble and a completed 40-versus-40 fight. The owner
initially reported that arrows looked random. After the readability changes,
the owner approved the 2026-09-13 native rerun as clear enough to proceed.
PRs #9 and #10 are merged; M4 is landed with the native readability gate passed.
The software rasteriser (`docs/07` D13) remains the source of the repository's
golden images; native observations are recorded separately below.

### What you can do in the game today

Open the game on its title screen, set up a skirmish (map size, one to
seven computer opponents each at a difficulty, the population cap and the
seed, with the map previewed) and play it: select and command villagers,
gather food, wood, stone and gold, build Houses and Storehouses, train
villagers from the Town Center with rally points onto resources, put up the
ten M3 buildings as your age allows, research technologies at the Storehouse
and Market, advance Stone → Tool → Bronze → Iron behind the resource-and-
buildings gate, and farm with auto-reseed. Train the six Tool Age soldiers,
attack, attack-move and patrol with stances and formations; drag a palisade
or stone wall, set a gate into it, put up a Watch Tower and garrison it or
the Town Center; knock the other side's buildings down to rubble. The
opponents play from the first tick, scouting, building, advancing and
coming for the Town Center; Escape opens the pause menu (resume, resign,
quit, save) and the results come up when the match is decided. F5 or
the menu saves the match; LOAD GAME on the title lists the saves and
resumes one where it was. Every match played is recorded; WATCH REPLAY
plays one back with pause and speed, through any side's eyes or
everyone's. SETTINGS holds the HUD size, edge scrolling, the window mode
and every general key, kept in a file. A stack in the lower left names
attacks, losses, research and ages, and a click on one looks there.
The game is heard: units answer an order and a click, the woodline and
the fight sound where they are and only where the player can see, a
button clicks and a greyed one buzzes, the bell tolls for an attack and
a fanfare for an age, every sound a synthesised placeholder for now.
SETTINGS holds the four volumes.

```sh
cargo run --release -p new-empire           # the game window
cargo run -p mapview -- --scenario ages --stockpile 5000 --ticks 100 \
    --select-tc 1 --hud 1 --out frame.png   # the same frame, headless
```

### The three streams

Work has run as three streams that merge into this branch:

- **Core** (simulation, view, app, tools): M0–M3 above, and M4 to step 5.
- **Art pipeline** (`docs/08`): the render-to-sprite pipeline is built and
  proven end to end. The camera and light rig are frozen, `atlas` validates
  and composes sheets against the one palette (`docs/07` D19), and the
  greybox villager renders, loads and animates in-game in place of its
  placeholder. Everything else still draws as a procedural placeholder.
- **Test and automation** (`docs/09`): CI runs the unit and behaviour tests,
  the replay corpus, the golden images, requirement traceability, the perf
  budget, generated-file drift, CLI-caller and workflow checks, and the art
  gate. Property tests and a soak runner exist; the nightly job that would
  run them at depth does not yet.

---

## 2. What M3 established

Details are in `docs/06` (status) and `docs/04` §19 (implementation notes).
The points that affect what comes next:

- **Technology is a production-queue item**, and effects fold into
  per-player modifiers that the gather, movement and construction systems
  read. Combat modifiers (attack, armour, range, speed) slot into the same
  struct.
- **The age gate is one query, `Simulation::can_research`**, and the
  command panel greys buttons with the reason the simulation would give.
  Every later gate (unit availability, upgrades) should follow the pattern:
  a public query the HUD asks and the command re-runs before paying.
- **Static tables stay in Rust** (`docs/07` D20) until the roster stops
  moving. The `data` crate and its validator are deferred, not dropped.
- **The HUD owns the hotkey table.** Buttons carry their keys; the app looks
  the pressed letter up in the buttons it last drew. New commands get a
  hotkey by getting a button.
- **The perf budget was spent early, and has been bought back.**
  `marching-8p` (roughly 320 units pathing at once, no combat, no AI) sat
  at about 6.5 ms p99 on per-unit A\* against the 6 ms `docs/04` §12 allows
  for pathfinding at full population. M4 chunk 1's flow fields brought it
  to about 2.4 ms (record below), which is the headroom the armies of the
  rest of M4 and the opponent of M5 will spend.

---

## 3. Review follow-up before M4

The 2026-09-11 code review identified a short stabilisation pass before the
combat work below. Keep the work in these independently reviewable chunks;
all four chunks are done.

| Chunk | Scope | Status / completion check |
|---|---|---|
| 1 — Restore CI | Fix the stable Clippy failure in PNG palette validation | Merged as `dac80ad` after final CI passed on PR #6 |
| 2 — Input routing | Handle minimap clicks before the general HUD hit test; allow Storehouse and Market research cancellation | Merged by the owner in PR #7 as `384ae28` |
| 3 — Commands and production | Resolve WASD/command hotkey conflicts and update controls documentation; retain a paid training item when the entity cap prevents spawning | Merged in PR #8 as `9e0b3e6` after final CI passed; 295 local macOS tests passed |
| 4 — Real-window check | Run the Mac app through selection, gathering, building, training, cancellation and age advancement | Done 2026-09-12 on a MacBook Air (Mac16,13, Apple M4, macOS 27.0). First launch aborted (winit on macOS 26, fixed as `cf5e310`); with the fix, everything in the list worked. Feedback below. |

### Work record: chunk 1

- Reviewed the default branch at `ead7002`, including this status document.
- Confirmed CI run `34645363689` failed at the palette iterator in
  `crates/view/src/sheets.rs`, with downstream jobs skipped.
- Changed the iterator to `as_chunks::<3>().0.iter()` and compared the
  dereferenced RGB array with the palette entry. This preserves complete
  three-byte groups, their order, the 256-entry limit, and the transparent
  index-zero exception. The renderer already uses this array-chunk pattern.
- Local whitespace and requirement-traceability checks passed. Rust was not
  installed during chunk 1, so GitHub Actions supplied the Rust and platform
  verification; the game window was not exercised.
- The first CI attempt caught an array-reference comparison mismatch in
  the iterator edit; it was corrected in `332c444` before the full rerun.
- Full CI passed for code commit `332c444` in
  [run 34646217196](https://github.com/bomielke-ukus/new-empire/actions/runs/34646217196):
  lint and purity (including art, generated-file, traceability, CLI and
  workflow gates), tests on Linux/macOS/Windows, replay and determinism
  checks, software-rendered frames, performance budgets, the 300-match
  soak, and agreement of simulation hashes across platforms.
- The subsequent documentation commit also passed full CI in
  [run 34646894917](https://github.com/bomielke-ukus/new-empire/actions/runs/34646894917).
  [PR #6](https://github.com/bomielke-ukus/new-empire/pull/6) was merged as
  `dac80ad` before starting chunk 2.

### Work record: chunk 2

- Read `docs/09-test-plan.md` and used app-level input injection to cover
  the gap between HUD/selection and simulation commands.
- Minimap left-clicks now take priority over HUD and placement handling;
  right-click movement and rally orders bypass the HUD only inside the
  minimap diamond. Right-click still cancels active building placement.
- The UNQUEUE action carries the ID of the building whose queue is shown.
  Storehouse/Market research can now be cancelled, including with a Town
  Center in the selection, without accidentally cancelling that TC's queue.
- Added five headless app regressions covering navigation at three viewport
  sizes, scrubbing, selection/placement preservation, movement/rallies,
  outside-diamond HUD clicks, both research cancellation input methods,
  exact refunds and mixed selection. Four tests failed against the old
  handlers; all 12 app tests pass after the fixes.
- Installed a task-local Rust toolchain without modifying the shell profile
  or system toolchain, enabling local macOS tests. The full workspace suite
  passed (290 tests, zero failures), as did Clippy with warnings denied,
  formatting and requirement traceability.
- Full GitHub CI passed for code commit `662e086` in
  [run 34648843607](https://github.com/bomielke-ukus/new-empire/actions/runs/34648843607):
  lint/purity and all preliminary gates, macOS tests, the additional
  Linux/Windows jobs, performance, the 300-match soak and cross-platform
  hash agreement. This result entry was added after that run, with no
  executable changes. [PR #7](https://github.com/bomielke-ukus/new-empire/pull/7)
  was subsequently merged by the owner as `384ae28` before chunk 3.
- Clarified macOS as the target in the README, architecture, testing plan
  and this status file. Windows/Linux CI remains additional verification.
- No simulation rules or golden fixtures changed. The real-window Mac
  check remains chunk 4; chunk 2's tests do not complete it.

### Work record: chunk 3

- Confirmed the PR #7 merge and branched from `384ae28`. Followed the
  testing plan's distinction between behaviour regressions, deterministic
  replay stability, reference images and live hardware testing.
- Reserved WASD for camera movement in the physical-key handler. Build
  shortcuts now use O (Storehouse), N (Archery Range), J (Watch Tower);
  research uses Q, E, I, K, Z in displayed order. Updated the README and
  UX controls, including the existing Shift+E edge-scroll toggle.
- Extracted keyboard dispatch from window exit handling so the same
  press/release/repeat path can run in headless app tests. Added two tests
  for camera-only keys, release, replacement commands, repeats and Escape,
  plus a HUD test for shortcut uniqueness across mixed selections.
- Completed training now leaves the queue only after a unit spawns.
  Two new simulation regressions cover waiting at the entity cap, preserving
  queue order, spawning once with the rally after capacity opens, no second
  charge, and full cancellation refunds. Both also pass with debug invariant
  checks enabled; the recovery test verifies its replay.
- Both app regressions and both capacity regressions failed on the old
  behaviour before the fixes. Focused checks now pass.
- The full corpus identified exactly one intended digest change:
  `entity-cap-pressed`, whose paid units now remain queued. Updated only
  its expected digest/final hash; all replay input files remain unchanged.
  Existing expected hashes for affected older replays are not compatible
  with the corrected behaviour; the replay file format is unchanged.
- Refreshed reference renders: only `gather-hud.png` changed. Inspected the
  before/after images; Storehouse, Archery Range and Tower shortcut labels
  change, with the scene and layout preserved.
- Full local macOS verification passed: 295 tests, zero failures; Clippy
  with warnings denied, formatting and requirement traceability also pass.
- Full CI passed for code commit `ad37ec5` in
  [run 34650485707](https://github.com/bomielke-ukus/new-empire/actions/runs/34650485707):
  macOS tests, all preliminary gates, additional Linux/Windows tests,
  performance, the 300-match soak and cross-platform hash agreement.
  The final documentation commit `43a3473` also passed full CI in
  [PR run 34651001791](https://github.com/bomielke-ukus/new-empire/actions/runs/34651001791)
  and [push run 34650998730](https://github.com/bomielke-ukus/new-empire/actions/runs/34650998730).
- At the owner's request, marked
  [PR #8](https://github.com/bomielke-ukus/new-empire/pull/8) ready and
  squash-merged it as `9e0b3e6`, with the verified final head protected
  against intervening changes.

### Work record: chunk 4

- **First launch on the Mac (2026-09-12) aborted before a window
  appeared.** MacBook Air, Mac16,13, Apple M4, macOS 27.0 (26A428). The
  crash report shows a Rust panic inside winit's `applicationDidFinishLaunching`
  callback, which cannot unwind, so the process aborted with SIGABRT.
- **Diagnosis.** macOS 26 changed the Objective-C type encoding of
  `countByEnumeratingWithState:objects:count:` from unsigned to signed.
  winit 0.30 enumerates `NSScreen` through it at launch, and objc2 0.5's
  signature check (active in debug builds) panics there. Upstream fixed it
  on winit's main branch by enabling objc2's `relax-sign-encoding` feature
  (rust-windowing/winit#4302); the 0.30 line does not carry the fix.
- **Fix.** `crates/app/Cargo.toml` enables `relax-sign-encoding` on objc2
  for macOS. The app also installs a panic hook that copies every panic
  message to `new-empire-panic.log` in the working directory, so a crash in
  a native callback leaves the cause on disk beside the crash report.
- **The renderer is now exercised on a real device in CI.**
  `crates/render/tests/headless.rs` creates a wgpu device on whatever
  adapter exists (a software Vulkan driver on the Linux job, Metal on the
  macOS runner where available), builds every pipeline, renders a frame and
  reads it back. It passed here on lavapipe. It skips only when no adapter
  exists at all.
- **Rerun with the fix: passed.** The window, the Metal path and every item
  on the chunk 4 list (selection, gathering, building, training,
  cancellation, age advancement, minimap orders, WASD camera) worked on the
  MacBook Air. The owner's feedback from the session is recorded in the
  next section, with what was done about each point.

### Work record: Mac feedback, round 1 (2026-09-12)

The live check passed once the launch abort was fixed. Feedback from it, and
what was done:

- **"Zoom barely does anything" and "the panel is too small to read"** had
  one cause: the app ignored the display's scale factor. On the Retina
  MacBook the window is 2560×1440 device pixels, so the world and the HUD
  drew at half their intended size, and the zoom range of 1× to 2× only got
  back to the intended look. Now `Camera::dpi` folds the scale factor into
  the zoom, the HUD lays out in its own pixels and scales on the way out,
  the minimap, the panel band and drag thresholds scale with it, and `F2`
  cycles a further 1×/1.5×/2× HUD magnification. Zoom runs 0.5× to 3× in
  six levels, steps about the cursor, and accumulates wheel travel so a
  trackpad steps once per unit of travel instead of once per event.
  Pinned by the `retina-hud` golden and by camera, input, HUD and app tests.
- **Colours are boring.** Done as a palette pass, shown as before/after
  renders: the light ends of the terrain, foliage and building-material
  ramps in `assets/palette/ancient.ron` carry more chroma and lightness
  while the dark ends stay cool and deep, so shadows keep their weight;
  grass tiles lean toward dry grass in patches a few tiles across so a
  meadow is not one flat green; per-tile brightness variation widened.
  The baked table and the villager sheet's palette chunk were regenerated
  (`atlas export --rust`, `atlas repalette`), the player-colour separation
  test still passes, and every golden image was re-baselined.
- **The Town Center and House look boring.** Decided 2026-09-12: stay with
  the plan. Real buildings come from the render pipeline (`docs/08` §9
  step 3), textured in M7, and the placeholders stand until then. No
  placeholder polish and no early greybox of the two buildings.
- **A reminder of the keyboard commands.** Done: `F1` or `?` opens a
  controls overlay generated from the same hotkey tables as the command
  grid, the resource bar shows "F1 CONTROLS" for the first minute, and
  Escape closes the overlay first. Pinned by the `controls-overlay` golden
  and HUD and app tests.

### Work record: M4 chunk 1 — flow fields and the sector graph (2026-09-12)

- **What landed.** `crates/sim/src/flow.rs`: the sector graph (16×16-tile
  sectors, portals per open edge run, intra-sector cost tables), corridor
  search per group, flow fields flooded over the corridor and shared by
  every unit bound for the same destination, incremental extension for
  stragglers, stop-early floods, eviction after 20 seconds unread. The
  simulation's `plan_paths` groups planning units by destination and
  serves up to 16 destinations a tick; `movement` steers along the field
  with a twelve-tile lookahead. `docs/04` §20 has the design notes.
- **`TA-PATH-02` is closed.** `STALL_TICKS` is 3; the give-up rule is a
  separate measure (no progress toward the goal for 20 seconds while
  staying put), so the replan allowance that pinned the timer at 40 is
  gone. The claiming test walls a walker's corridor mid-trip and asserts a
  new heading within 3 ticks. `DEFERRED` in the traceability script is
  empty.
- **Measured.** `marching-8p` p99 6.5 ms → 2.4 ms, p50 0.9 → 0.6 ms;
  `crowded` p99 2.9 → 1.9 ms; `economy-2p` unchanged. The first version was
  slower than A\* (7.2 ms); `docs/09` §8 records each step from there,
  because the lesson — the flood and corridor loops were paying for
  `BTreeMap` lookups and binary heaps, not for the algorithm — will
  recur. `simrunner bench --stats` now prints the field diagnostics that
  found it. Ceilings in `perf/budgets.ron` lowered to three times the new
  numbers.
- **What changed for the player.** Nothing visible on purpose: gathering,
  building and marching behave as before, with units taking straighter
  lines through open ground and re-steering rather than stopping when a
  house goes up in front of them. The corpus and five golden images were
  re-recorded for the changed unit positions.
- **Not done, deliberately.** The priority half of `TA-PATH-06`
  (player-issued before AI-issued orders) still waits for M5, when there
  is an AI to issue anything. Formations are chunk 4's.

### Work record: M4 chunk 2 — the damage model and the roster (2026-09-12)

- **What landed.** `crates/sim/src/combat.rs`: the `docs/02` §8 rule as a
  pure function, elevation from the map, per-class technology bonuses,
  siege friendly fire as a property of the damage type. The kinds table
  carries a class and a combat block per kind and the six slice soldiers:
  Clubman, Axeman, Spearman (Barracks), Slinger, Bowman (Archery Range),
  Light Cavalry and the Scout (Stable). Four military technologies: the
  Axe line upgrade, Toolworking, Leather Armour, Fletching. `docs/04` §21
  has the design notes.
- **The matrix.** `docs/damage-matrix.md`, generated by `simrunner matrix`
  and held to the table by `check-generated.sh`: kinds, damage per hit,
  hits to kill, and the elevation rows.
- **What changed for the player.** The Barracks, Archery Range and Stable
  panels list their rosters with hotkeys (C, P, G, B, L), greyed with the
  reason until the age or the Axe arrives; unit tooltips carry cost,
  time, hit points, attack, counters and countered-by (`UX-TIP-01`); a
  selected fighter shows its attack and armour after research; the
  selection panel counts soldiers by kind; F1 lists the training keys.
  The six soldiers have placeholders built on the villager's body with a
  weapon each. The corpus bot now trains soldiers, so every corpus digest
  was re-recorded.
- **Not done, deliberately.** Nothing deals a hit yet: attack orders,
  target acquisition, projectiles, stances and death are step 4. Buildings
  have no armour of their own until step 5. The Stone Thrower and the
  other non-slice units stay out of the table; the siege rule is tested
  on the damage type, not on a unit.

### Work record: M4 chunk 3 — fighting (2026-09-12)

- **What landed.** `crates/sim/src/battle.rs` and `formation.rs`: attack,
  attack-move and patrol orders; target acquisition by stance
  (aggressive, defensive, stand ground, passive) with a leash that decides
  how far a unit chases and a memory of what it goes back to; melee hits
  and homing projectiles on the reload; death into a corpse that lies for
  thirty seconds and takes no part in anything; villagers running for
  the Town Center when hit, with one alarm per ten seconds for their
  side; formations (line, box, staggered, flank, or none) laid out facing
  the way the group walks, at the slowest member's pace. `docs/04` §22
  has the design notes.
- **What changed for the player.** Right-click an enemy to attack. With
  soldiers selected the panel offers attack-move (`M`) and patrol (`P`),
  which arm the cursor for one click; the four stances (`Q E I K`, the
  current one starred); and the formation (`Z` cycles it). A selected
  soldier's panel shows its stance and formation and what it is doing.
  Arrows fly, bodies fall and lie, and "UNDER ATTACK" goes up when your
  villagers are hit. The bot now raids in the corpus scenarios, so every
  digest was re-recorded and a `battle-hud` golden image was added.
- **Measured.** The acquisition scan is every fourth tick per unit over
  every entity; `marching-8p` and `crowded` stay inside their ceilings.
- **Not done, deliberately.** Hunting (`docs/07` D15) still waits: animals
  cannot be attacked and there is no carcass. Buildings die without
  rubble and without armour of their own, and towers do not shoot yet:
  step 5. Waypoints (`UX-CMD-04`) and garrison (`UX-CMD-09`) are
  untouched. Attack-move is `M`, not the `A` `docs/03` names, because
  `A` pans the camera.

### Work record: M4 chunk 4 — buildings in combat (2026-09-12)

- **What landed.** Rubble: a building at zero health is the same entity
  marked dying for sixty seconds, its footprint open from the first tick,
  its garrison out, its queue lost, a site refunding nothing. Building
  armour on the kinds table (arrows do the minimum to any building).
  Towers and the Town Center shoot through the same passes as units: the
  Watch Tower one arrow of its own, plus one per unit garrisoned inside
  either. Palisade Wall (Tool), Stone Wall and Gate (Bronze) as one-tile
  buildings placed in runs; a gate goes onto a wall segment of yours and
  shuts while an enemy is within two tiles. Garrison (`UX-CMD-09`): units
  inside cannot be hit or picked, still count toward population, and come
  out on ALL OUT or when the building falls; a villager who runs home
  under attack goes inside. A column on attack-move walled out breaks in.
  `docs/04` §23 has the design notes; `docs/07` D21 the decisions.
- **What changed for the player.** J opens a DEFENCES page in place of the
  build grid: tower, palisade, stone wall, gate, back. A wall is dragged as
  a run with a live count and cost in the top bar; the ghost hatches every
  tile of it. Right-click a tower or Town Center of yours with units
  selected to garrison them; the building's panel shows INSIDE n/cap and
  offers ALL OUT (`T`). A selected tower or Town Center shows its attack
  and armour. Rubble is drawn where a building stood; an open gate shows
  its doors swung back. `mapview --scenario siege` and the `siege-hud`
  golden image show a siege; the corpus bot builds wall runs, gates and
  towers and garrisons, so every corpus digest was re-recorded.
- **Measured.** The gates pass and building acquisition add nothing
  measurable; `marching-8p` and `crowded` stay inside their ceilings.
- **Verified.** Full CI passed on the first attempt for both M4 commits of
  the day: chunk 3 (`6753c84`, run `34696906353`) and chunk 4 (`a978d73`,
  run `34699090008`), on every platform job. The status-document commit
  that followed (`6135fda`, run `34701101338`) failed on macOS in the
  config sweep (`properties_sim`), on a latent bug older than this chunk:
  a building placed and a move ordered in the same tick left the grid
  dirty for the move's connectivity query. Fixed the same day by
  relabelling the grid between commands, with the seed kept in the
  regression file and a unit test for the pair. The subsequent native Mac
  combat/siege checks and remaining readability issue are recorded below.
- **Not done, deliberately.** Repair (`docs/03` §3's cursor table) is not
  in the plan's step and waits; a damaged building stays damaged. The
  Guard Tower is not a slice item. "Automatic gate suggestion at road
  crossings" (`UX-PLACE-03`) has no roads to suggest at. Collapse and dust
  are art (M7); rubble appears at once. The Town Center stays off the
  build panel until Q3 is answered (§6). Hunting still waits.

### Work record: M4 acceptance automation (2026-09-12)

- Added a command-driven 40-versus-40 mixed-army fixture, with a 6,000-tick
  deadline and a per-unit 400-tick inactivity check. Invariants run every
  tick. Both armies converge on the centre of a flat arena; the fixture
  covers a controlled fight, not all terrain or formation combinations.
- The reference battle ends at tick 699 (about 35 seconds), with 17 units
  surviving on side 1 and a longest individual inactivity span of 148 ticks.
  Its test checks 80 spawns, a decisive outcome, recipe/corpus agreement and
  per-tick replay determinism. The `RM-M4-01` marker moved from the old
  one-on-one test to this automated acceptance test; readability stays manual.
- Added only `battle-40v40.ron`, its digest and its golden image. The new
  `mapview --replay` option renders the actual corpus at tick 200. Inspected
  the frame: both armies, ranged fire and the selected unit's HUD are visible.
  Existing replay inputs, digests and reference images remain unchanged.
- Added `simrunner battle` and `simrunner balance`. Both specified counters
  win 10/10 trials on each side: 12 Spearmen versus 9 Light Cavalry, and
  14 Slingers versus 10 Axemen, at equal total resource budgets. Seeds vary
  deployment and each is repeated with owners/sides exchanged. The gate
  requires at least 90% wins separately on each side and no stalled trials.
  CI also runs balance in release with invariant checks on every platform.
- Proved the inactivity detector rejects passive, stopped armies. Temporarily
  removing the Spearman's cavalry bonus made its balance check fail with
  zero wins. Removing the Slinger bonus did not change its win rate in this
  arena; its existing damage-model test remains the check for that exact
  bonus. Restored the original data afterward. No gameplay stats changed.
- Cleaned up the testing plan's stale corpus/image/coverage counts, closed
  M1/M2 backlog, GPU claims and old command-count wording, and documented
  what the new acceptance checks establish and what remains manual.
- Incorporated the concurrent navigation fix `83c9f24` from the default
  branch before completing review; its regression adds one workspace test.
- Local macOS verification: all 355 workspace tests pass, plus all four
  acceptance/CLI tests in release with debug invariant checks. Formatting,
  Clippy with warnings denied, purity, traceability, generated-file freshness
  and art conformance pass.
- Full GitHub CI passed for combined code commit `ecf6cd6` in
  [run 34702500527](https://github.com/bomielke-ukus/new-empire/actions/runs/34702500527):
  lint/purity and preliminary gates (including CLI callers and workflows),
  macOS and additional Linux/Windows tests, release combat/balance trials
  with invariants, performance, the 300-match soak and cross-platform hash
  agreement. This result entry was added afterward with no executable changes.
  [PR #9](https://github.com/bomielke-ukus/new-empire/pull/9) awaited review
  and merge at this checkpoint; its merge is recorded below. No live Mac combat playtest has been performed in this chunk.

### Work record: live Mac combat check (2026-09-12)

- Tested PR #9 head `70df19f`, still awaiting merge. Hardware: MacBook Air
  (Mac16,13), Apple M4, 16 GB, macOS 27.0 (26A428). The native Metal window
  visibly rendered, with the title reporting roughly 60 fps during these
  observations; this is a smoke check, not a performance benchmark.
- **Normal-start checks passed:** launch, readable HUD at the displayed
  window size, Space pause/resume, speed adjustment to 8x, F1 controls and
  Escape dismissal, villager selection, barracks placement/construction,
  and Clubman training. The visible food/population changes confirmed the
  unit was produced.
- **Controlled siege setup:** a local-only bootstrap prepared a Bronze-age
  arena through Spawn/Research commands, with units, buildings and an enemy
  wall/gate. It starts paused and records a replay on normal exit; game
  input, simulation and rendering handlers are unchanged. The temporary
  bootstrap edits were restored afterward, and the production binary rebuilt.
  This distinguishes controlled-fixture evidence from the normal-start pass.
- **Garrison passed:** drag-selected three Bowmen, right-clicked the Watch
  Tower and resumed. At tick 3315, the tower's live panel showed INSIDE 3/5,
  HP 250/250 and population unchanged at 9/35.
- **Resumed after manual unlock. Ungarrison passed:** ALL OUT (`T`)
  returned all three Bowmen to the world; the panel changed to INSIDE 0/5
  and population remained 9/35.
- **Defences passed:** one villager completed a six-segment dragged palisade
  and a four-segment dragged stone wall. Replacing an own palisade segment
  with a gate refunded 5 wood and charged 30 stone. Its doors visibly opened
  during friendly villager passage. The final gate had 350/350 HP and each
  stone wall 400/400 HP.
- **Combat and destruction passed:** three Axemen right-click attacked the
  enemy gate and reduced it to visible rubble. After switching them to
  Aggressive, attack-move sent them through that breach, killed the enemy
  Clubman and destroyed the house; its rubble was visible at tick 12494.
  All three Axemen survived and finished their orders. This native sequence
  used an explicit gate attack before attack-move; automatic breach selection
  against a fully closed wall retains its separate automated test coverage.
- **Native 40-versus-40 run passed functionally:** loaded the committed
  `battle-40v40.ron` at tick 6 and ran at 1x with no tactical intervention.
  The live view showed both armies approaching, projectiles, casualties and
  the surviving orange army. At the final pause, tick 1178, player 0 had no
  living soldiers and player 1 had 17, matching the automated arena outcome;
  the 25 entities still counted by the title included eight remaining corpses.
  The window reported about 60 fps during combat. The captured game content
  was 1280×720 logical pixels; physical display resolution was not independently
  re-measured in this pass. GPU: Apple M4, 10 cores, Metal supported.
- **Replay evidence passed:** the local siege recording contains 94 commands
  and 15275 ticks; production `simrunner verify` reproduced every tick twice
  (final hash `350ffd7c33a118ba`). The native battle recording contains 82
  commands and 1178 ticks and likewise verified (`3780a6da33f68800`). Recordings
  and final-world summaries are retained locally as `mac-siege-played.ron`,
  `mac-battle-played.ron` and matching `mac-*-result.txt` files in the task's
  `work` directory. They are local evidence, not new committed golden fixtures.
- **Readability did not pass.** The owner selected “It needs clearer visuals”
  and added: “I could see arrows flying, but it was quite random.” The observer
  also found infantry roles difficult to distinguish in the clustered fight.
  This is a presentation issue to investigate, not evidence that simulation
  targeting is random. The UNDER ATTACK banner incorrectly displayed the
  age-up subtitle “NEW BUILDINGS AND TECHNOLOGIES AVAILABLE”; confirmed in
  `crates/view/src/hud.rs`, which applies that subtitle to every banner.
  Both findings remain open; no gameplay or presentation fix is claimed here.
- Both native sessions were closed normally and their recordings verified.
  Production sources remain unchanged. PR #9 checkpoint `79ca7e4` passed both
  CI runs (`34704682171`, `34704679626`); the documentation update below will
  trigger its normal rerun. At that checkpoint PR #9 was draft/unmerged; the later closure is recorded below.
- The initial app-control timeouts were traced to a temporary shell launcher;
  a fresh native app bundle was controllable. A process sample showed active
  Metal rendering and no startup panic was found. This is not recorded as a
  demonstrated game startup defect.

### Work record: combat readability follow-up (2026-09-12–13)

- **Problem:** the owner could see arrows but found the fighting random-looking.
  Infantry shared nearly identical bodies with tiny weapons; every projectile
  was a short horizontal mark irrespective of its flight direction.
- **Presentation changes on `codex/combat-readability`, stacked on PR #9:**
  procedural infantry now carry larger, outlined clubs, axe heads, long spears
  with shields, raised slings and curved bows with quivers. A short strike or
  release pose follows the actual reload counter rather than a free-running
  animation. Rendered sprite sheets still take precedence over placeholders.
- Arrows point along their current aim in all eight facings and have team-colour
  fletching. A 200 ms impact spark marks the position of actual damage events;
  repeated hits on one target coalesce, and pause/speed controls apply to the
  effect. Both the native app and replay image renderer collect the same
  per-tick presentation history. Combat rules, costs, targeting, replay inputs
  and simulation hashes are unchanged. Projectiles remain the shared arrow
  placeholder for now; separate sling-stone artwork is not part of this chunk.
- **Attack warning fixed:** typed age-up and attack notifications replace the
  shared text-only banner. Only an age-up displays the buildings/technologies
  subtitle. A HUD regression covers the distinction.
- **Validated locally:** 357 workspace tests passed, including all replay
  digests. New checks cover all eight arrowhead directions and team-colour
  tails, impact expiry/idempotence and replay equivalence. Formatting, Clippy
  with warnings denied, art conformance, generated files, purity and
  traceability passed. The normal release app and an isolated native test app
  built successfully. Four intentionally changed golden images were inspected
  and updated: `army-hud`, `battle-hud`, `siege-hud`, `battle-40v40`; the other
  nine are unchanged. Full code CI passed for `0179152` in run
  `34727546332`: all seven jobs, including macOS, supplemental Windows/Linux
  checks, performance, the 300-match soak and cross-platform hash agreement.
  This acceptance-record update changes documentation only and triggers its
  normal CI rerun.
- **Native recheck passed (2026-09-13):** after the earlier app-control
  timeout, selecting `uk.newempire.codex.readability` resumed the native window
  at tick 6. On the same Apple M4 MacBook Air/macOS 27.0, the same 40-versus-40
  replay ran at 1x with no tactical intervention. The window showed distinct
  weapons, direction-facing projectiles, attack poses and impact cues at about
  60 fps. The owner answered: **“Yes, this is clear enough to proceed.”**
- The battle was paused at tick 835 after player 0's last soldier died.
  The saved world held 17 living player-1 soldiers and 53 corpses (70 entities
  in the title); it matches the automated outcome. Production `simrunner verify`
  reproduced all 835 ticks twice, final hash `9875adee3ccba0d9`, 82 commands.
  The accepted recording is retained locally as
  `work/mac-readability-battle-accepted-played.ron`, with its matching
  `mac-readability-battle-accepted-result.txt` world summary.
- A second short native run specifically verified the **UNDER ATTACK banner
  without the age-up subtitle**, visible near tick 176. It was closed at tick
  366 and its 82-command replay also verified (`91d2af9d6414d07e`). Both windows
  were closed normally. No production code changed during this acceptance pass.
- The isolated app's setup bootstrap remains local in `work/mac-readability-app`
  and `work/mac-playtest/New Empire Readability.app`; production startup and
  Cargo manifests were never replaced. The earlier control timeout is resolved.
  The owner's readability approval completed the native gate. The following
  merge record closes M4; M5 has not started.

### Work record: M4 merged and closed (2026-09-13)

- Merged [PR #9](https://github.com/bomielke-ukus/new-empire/pull/9) as
  `155c98391ba9cacd3526a8d8b7432692783b4dc9`, preserving its ancestry so the
  stacked readability PR could follow cleanly. Its final head `38aba74`
  passed the full PR workflow (`34706310971`).
- Retargeted [PR #10](https://github.com/bomielke-ukus/new-empire/pull/10)
  to the default branch. Verified the new base tree exactly matched its prior
  tested base and the change remained the expected 13 files. Its final head
  `b303d5d` passed the full PR workflow (`34727917977`), then merged as
  `78620a549af067ab4d7669f95641c801a3639f53`. Both merges were guarded by the
  expected head SHA. The merged tree matches the accepted local tree.
- Updated the roadmap and status to mark **M4 landed**, and enabled
  `RM-M4`, `GD-COMBAT` and `GD-STANCE` enforcement in the traceability gate.
  The gate passes with no missing landed requirements. Existing automated
  wall-breach coverage and the documented native smoke scope remain unchanged.
- All 357 local workspace tests and full code CI had passed before merging;
  the owner approved the native battle and both native recordings verified.
  This closure commit changes documentation and traceability enforcement only;
  normal post-merge/closure CI runs remain separate from those completed checks.

### Work record: the Town Center on the build panel (2026-09-18)

- Q3 answered (`docs/07` D22): the Government Centre stays its own
  building. The Town Center is on the villager's build panel, and a second
  one needs a finished Government Centre standing, as in the original;
  `Simulation::can_build` is the non-positional half of `can_place` and
  the panel greys the button with its words ("NEEDS A GOVERNMENT CENTRE").
  The Town Center's button has no key: every letter is taken (`docs/04`
  §23), so it is placed by clicking, and the controls overlay says so.
- The plan below is rewritten for M5, and the three `docs/09` §11 items
  are a parallel track for a second agent.

### Work record: M5 chunk 1 — fog of war and the AI boundary (2026-09-18)

- **What landed.** `crates/sim/src/fog.rs`: a `Fog` per player with a
  visibility count per tile, an explored bitset and the static things last
  seen by anchor tile, recomputed every tick from every standing, living,
  un-garrisoned entity's sight (`docs/07` D23 for why not incrementally);
  the explored bitset and the memories are in the state hash. Two new
  crates: `fogged`, whose `FoggedView` is one player's view of a match
  over `sim`'s public API and re-exports the command and data types an
  opponent needs and nothing that reaches the world; and `ai`, which
  depends on `fogged` and not on `sim`, holds `Difficulty` and an
  `Opponent` seeded from the match whose `think` returns commands (none
  yet), and whose three compile-fail cases prove the world is unnameable
  from there (`TA-AI-01`); they check the error code, not the message,
  after CI's newer compiler reworded it and the first version went red. Who issued a command
  (`Source::{Player, Ai}`) is recorded in the queue and the replay log
  without changing the file format; the last source to name a unit is its
  path priority, and `plan_paths` serves the player's requests first
  (`TA-PATH-06`, whole now). The Town Center gate on a Government Centre
  from the same day is recorded above. `docs/04` §24 has the notes.
- **What changed for the player.** Nothing visible: fog is computed and
  not yet drawn (step 2). `simrunner ai` runs opponents on every side
  headless with invariants on and verifies the recording; they explore
  what their start kit sees and issue nothing.
- **Measured.** The corpus test in debug went from 17 s to about 45 s
  before the stamp was cheapened with a bitset test per tile, and every
  corpus digest was re-recorded for the fog history now in the hash. The
  per-tick fog pass, measured by running the release bench with and
  without it: `marching-8p` p50 0.7 → 1.5 ms and p99 2.5 → 3.5 ms,
  `crowded-map` p50 0.9 → 1.7 ms and p99 2.2 → 3.5 ms, `economy-2p` p50
  0.15 → 0.18 ms and p99 0.18 → 0.23 ms. The gate stays inside its
  ceilings (7.5 ms and 6 ms), so the cost is recorded and not yet paid
  down; the candidates are the memory loop over every static entity per
  player, the full visibility clear each tick and the disc stamp itself.
  A recording with only player-issued commands serialises exactly as it
  did before: `sources` is written only when an opponent issued
  something, so `battle-40v40.ron` and the corpus files did not change
  shape.
- **Not done, deliberately.** Cliffs neither block nor extend sight beyond
  the one-tile bonus for high ground. Rendering the three states, the
  minimap and remembered buildings is step 2. `GD-AI-01` is claimed only
  when an opponent issues a command (step 3).

### Work record: M5 chunk 2 — fog in the presentation (2026-09-18)

- **What landed.** `GD-FOG-01` on screen. `crates/view/src/fog.rs` turns a
  player's fog into a light per tile corner, black beside ground never
  seen and the mean of seen-once and in-sight tiles otherwise; terrain
  vertices carry their corner and both renderers shade each tile from it,
  the GPU reading the corner texture in the terrain vertex shader. The
  scene is built for a viewer: their own things anywhere, others' only in
  sight, projectiles only in sight, and every memory drawn as the building
  or node it was, in the age it was seen, dimmed, not pickable; `Memory`
  gained the owner's age and whether it was a site, so a remembered
  building does not give away an advance made out of sight. The minimap is
  fogged the same way and is now drawn by the rasteriser in the panel's
  corner, so `mapview --hud 1` shows the whole window the game shows. The
  app sees through the player's eyes, uploads the lights once per tick, and
  refuses to place on ground never seen. `mapview --fog 0` shows the whole
  map. `docs/04` §6 and §7 are rewritten to what was built, §25 has the
  notes.
- **What changed for the player.** The map starts black beyond the
  settlement's sight, ground seen once stays dimmed with what was there
  when it was seen, and enemy units are seen only where someone of yours
  is looking. Impact sparks in the fog are not shown.
- **Measured.** The lights are a pass over the map's tiles, once per tick,
  a fraction of a millisecond at Giant size, and not in the simulation's
  budget at all. The corpus digests are re-recorded for the two new memory
  fields in the hash. Every golden image is re-baselined for the fog and
  the minimap; `fog-scout` is new and pins the three states in one frame.
- **Not done, deliberately.** A remembered building cannot be right-clicked
  as a target: it has no entity behind it. Sound in the fog is `docs/05`'s
  rule for when there is sound. Cliffs still neither block nor extend
  sight.

### Work record: M5 chunk 3 — the economy manager (2026-09-18)

- **What landed.** The opponent issues commands (`GD-AI-01` claimed):
  `crates/ai/src/economy.rs` is the build-order planner and the economy
  manager. A thought every few ticks (by difficulty) reads the view and
  orders villagers to the resource furthest below its share, a house ahead
  of the cap, a villager while under the target, the Storehouse by the
  wood and the Barracks for the age gate, the Tool Age when the
  simulation allows it, farms once the food in sight is short, and adopts
  any site nobody is building. `BuildOrder::for_difficulty` is the table
  behind it. The view grew a villager's job, a node's remaining amount, a
  building's queue and the population limit; a memory carries its handle
  (`docs/07` D24). `simrunner ai --stats --save` shows each side and
  keeps the recording. `docs/04` §26 has the notes.
- **What changed for the player.** Two units walking toward each other
  along one line no longer stand where they meet until they give up: they
  step aside and pass (`separation`, `TA-PATH-05` test). Every corpus
  digest is re-recorded for it, and the 40v40 battle and the balance
  trials still pass.
- **Measured.** A Standard opponent on Inland 96: 8 villagers and two
  houses by four minutes, the Tool Age between six and eight minutes, and
  by twelve minutes 16 villagers, four houses, three to five farms, the
  Storehouse, Barracks, Archery Range and Market, on every seed tried.
  Slower than a good player of the original by about a third, mostly for
  want of hunting. Stone and gold stay untouched until food and wood are
  stocked, so the Bronze Age is not reached in twelve minutes.
- **Not done, deliberately.** No scouting: the opponent explores 3–4% of
  the map, what its start kit sees. No soldiers, no reaction to attack;
  step 4. Hunting is still owed, and would be the biggest single gain to
  the opening.

### Work record: M5 chunk 4 — the military manager and scouting (2026-09-18)

- **What landed.** `crates/ai/src/military.rs`: the scout rides widening
  rings to ground not yet seen; soldiers are trained to a composition by
  age from what the side's buildings can train; an alarm at home is
  answered by every soldier at home; a raid goes out at the order's hour
  with `attack_size` soldiers to the nearest enemy building that does not
  shoot back, in sight or remembered, and the army walks into the Town
  Center's arrows only at twice that; Hard builds a Watch Tower. The
  military spends what the economy leaves, less what is saved for the
  Tool Age. `FoggedView::events` passes on the player's own alarms and
  losses. `simrunner ai --difficulty` runs mixed matches. `docs/04` §27
  has the notes, including the two things the first version got wrong.
- **What changed for the player.** Nothing on screen: no opponent is in
  the app yet (M6's setup screen). Under `simrunner ai`, a Hard opponent
  now scouts, raids and defends.
- **Measured.** Hard against Easy on Inland 96 for twenty minutes: Hard
  sees 70–80% of the map, Easy 5%; Hard ends with 20 villagers, 7–8
  soldiers standing and the Tool Age; Easy with 10 villagers and 1–2
  soldiers, having lost houses and villagers to raids. Neither eliminates
  the other in twenty minutes: the armies are small and raids trade
  soldiers for houses. Four-player mixed matches run and replay.
- **Not done, deliberately.** No walls. No counters to what the enemy
  fields (the composition is fixed by age). No victory or defeat: step 5,
  which also decides what "beats" means at a time limit, since twenty
  minutes ends with both Town Centers standing.

### Work record: M5 chunk 5 — victory, defeat and the acceptance run (2026-09-18)

- **What landed.** Conquest (`GD-WIN-01`): `Simulation::standing`,
  `winner`, `over` and `score`, `CommandKind::Resign`, the victory and
  defeat banners in the app. The fourth difficulty: Hardest is Hard with
  a declared 25% gather bonus set on the match (`docs/07` D25). The
  acceptance run: `simrunner versus`, its record
  `tools/simrunner/tests/versus-hard-easy.golden`, the `acceptance` CI
  job (`RM-M5-01`), and the test that plays the first recorded match.
  Two stalls the run found, fixed: a villager sent to a tree with no
  ground beside it, and villagers shut into pockets between farms (the
  simulation re-seats a displaced unit into open ground and the manager
  refuses a placement that seals a pocket). `docs/04` §28 has the notes.
- **What changed for the player.** The match ends: VICTORY when every
  other side is out, DEFEAT when yours is. Two units walking through a
  site going up are no longer dropped into a pocket between buildings.
- **Measured.** Twenty matches, Hard against Easy, thirty minutes each,
  in 86 s of release time: Hard wins 20 of 20, every one on score, none
  by elimination. Scores run 9,500–14,300 to 2,700–7,100. No invariant
  broke and no villager stood idle with work in sight.
- **Not done, deliberately.** No match is won by taking the Town Center:
  the armies are too small and raids trade soldiers for houses. The
  opponent we want ends a match; the one we have wins on points. No
  counters to what the enemy fields, no walls, no hunting, no resign
  key in the app (M6's menu). The nightly job could run more seeds and
  longer limits than CI's twenty.

### Work record: M5 tuning — an army that ends a match (2026-09-19)

- **What landed.** The opponent empties its buildings once an alarm has
  passed (its whole workforce used to sit in the Town Center for the rest
  of the match); the army masses to the attack size and goes at the enemy
  Town Center as one, aggressive; soldiers are trained for food alone
  when wood is short and Hard builds a second Barracks; a side without a
  Town Center keeps gathering and builds one; the scout keeps riding once
  the map is seen. The acceptance limit is forty minutes. `docs/04` §29
  has the notes, one per thing found.
- **What changed for the player.** Nothing in the app. Under `simrunner
  versus`, Hard now ends a match.
- **Measured.** Twenty matches, Hard against Easy: 20 wins, 18 by
  elimination between twenty and thirty-nine minutes, 2 on score at the
  limit. No invariant broke, no villager stuck. 123 s of release time.
- **Not done, deliberately.** No counters to what the enemy fields, no
  walls, no siege. Standard against Standard is not in the record. A
  human will find the assault predictable: it comes from the nearest
  side, at the Town Center, at about twenty minutes.

### Work record: M6 chunk 1 — the shell, the setup screen and the opponent in the app (2026-09-19)

- **What landed.** `crates/view/src/shell.rs`: the title, the setup
  screen, the pause menu and the results, built like the HUD (sprites
  and hit rectangles in HUD pixels, scaled on the way out), and the
  `Setup` model whose `config()` is the one place the screen's choices
  become match parameters. The app has a shell state (title, setup,
  match) around the match it had; the opponents think in its tick loop
  on their own `FoggedView` and issue as the AI; Escape opens the pause
  menu instead of closing the window. `mapview --screen title|setup`
  renders the screens for the golden images. `docs/04` §30 has the
  notes.
- **What changed for the player.** The game opens on a title screen.
  NEW GAME (or Enter) opens the setup: the map (Inland; the rest with
  M8), the size from Tiny 96 to Giant 240, one to seven opponents each
  at Easy, Standard, Hard or Hardest, the population cap 50 to 200, the
  seed with a SHUFFLE, and the seed's map previewed as the minimap will
  show it. A Hardest opponent has "+25% GATHER RATE" beside it. START
  (or Enter) begins the match against opponents that play from the
  first tick. In the match, Escape with nothing to cancel opens the
  pause menu, which pauses: RESUME, RESIGN and QUIT TO TITLE, the last
  two taking a second click. When the match is decided the results come
  up: VICTORY or DEFEAT, why, and each side's score; KEEP WATCHING puts
  the panel away, BACK TO TITLE leaves. `new-empire [SEED]` pre-fills
  the seed; without one the clock picks.
- **Measured.** A Giant map with eight players generates and previews in
  under 0.2 s, so every arrow press regenerates the preview. Four app
  tests drive the flow through the window's own handlers, five unit
  tests pin the screens and the setup model, two golden images pin the
  title and the setup screen.
- **Not done, deliberately.** LOAD GAME, WATCH REPLAY and SETTINGS are
  on the title greyed NOT YET: chunks 2 to 4. The title shows the
  placeholder name (`docs/07` Q5). Civilisation, teams, victory
  conditions and starting age (`docs/02` §13) wait for the content they
  need. A seed cannot be typed: the arrows and SHUFFLE are it. Nothing
  has been clicked on the Mac yet; the screens are verified by the
  handlers and the software rasteriser, as every milestone's first
  chunk has been.

### Work record: M6 chunk 2 — save and load (2026-09-19)

- **What landed.** `crates/save`: a save is the whole `Simulation`
  (which carries its command log, so a save is its own replay,
  `TA-SAVE-01`), the opponents mid-thought and the camera, under three
  version numbers checked before the world is parsed (`TA-DET-06`);
  `Save::verify` replays the log inside and requires the snapshot's
  hash. `ai` derives serde on the opponent; `sim` gains
  `STATE_VERSION` and moves the alarm rate-limit out of scratch so a
  resumed match raises its next alarm when the unsaved one would. The
  app has SAVE GAME on the pause menu and F5, a LOAD GAME screen on the
  title listing the saves newest first from their file names, and a
  `resume` that shares `enter_match` with a new game. `simrunner verify`
  takes a save. `docs/04` §31 has the notes.
- **What changed for the player.** F5, or SAVE GAME on the pause menu,
  writes the match to the saves directory and says so. LOAD GAME on the
  title lists the saves (seed, match clock, when, players); clicking one
  or pressing Enter resumes it at its tick with the opponents where they
  were in their thinking and the camera where it was. A save from
  another build is refused on that screen with both version numbers.
  Saves live under `NEW_EMPIRE_SAVES` or the platform's data directory.
- **Measured.** A save of a Tiny map after 600 ticks of two opponents is
  241 KB of RON; a loaded save continues identically to
  the unsaved match for 300 ticks of opponent play. Three unit tests in
  the save crate, one app test through the handlers, one on the built
  `simrunner`, one golden image.
- **Not done, deliberately.** No save names: the time, seed and tick
  are the name. No delete on the load screen. Saves are plain RON, not
  compressed. The list shows at most ten, newest first, and says how
  many older there are. Nothing has been clicked on the Mac.

### Work record: M6 chunk 3 — replay playback (2026-09-19)

- **What landed.** `save::replays`: a recording is the simulation's
  replay written under a name for when the match started, its seed,
  how far it got and its players, like a save. The app records the
  match when it is decided, when it is left and when the window is
  closed on it, replacing the match's earlier file each time, so a
  match leaves one recording. WATCH REPLAY on the title lists the
  recordings; watching one is the match state with a `Playback` that
  issues the log's commands at their ticks, as `Replay::run` does, and
  no opponents thinking. Tab cycles the viewer through each player's
  fog and then no fog at all (`FogLights::lit`); the HUD, the minimap,
  the banner and the alarm follow the viewer. Nothing the watcher does
  issues a command: `issue` is a no-op in playback and the panels,
  hotkeys, right-click and Delete are looked at, not used. The status
  line shows `REPLAY 12:34/40:00 P2`; the results screen says REPLAY
  OVER where the recording ends. `docs/04` §32 has the notes.
- **What changed for the player.** WATCH REPLAY on the title. In a
  replay: Space pauses, `[` `]` set the speed up to 16×, Tab changes
  whose eyes, Escape's menu has SAVE and RESIGN greyed and QUIT needs
  no second click. Recordings live under `NEW_EMPIRE_REPLAYS` or the
  platform's data directory beside the saves.
- **Measured.** A recorded match watched back to its end reaches the
  recorded match's state hash and command count, in the app test, with
  the viewer changed twice on the way. One unit test in the save crate,
  one app test through the handlers, one more case in the shell tests.
- **Not done, deliberately.** No seeking: a replay runs forward from
  tick 0, and the way back is to watch it again. No recording is
  written for a match that never ticked. A recording says nothing of
  the difficulties; the sides are PLAYER 1 to n. A save's history can
  be watched only by way of `simrunner verify`, not from the title.

### Work record: M6 chunk 4 — settings (2026-09-19)

- **What landed.** `crates/view/src/settings.rs`: `Settings` (the HUD
  size, edge scrolling, fullscreen, and a key per `Control`, seventeen
  general controls named by `winit`'s key names) with `bind` refusing a
  key another control holds, a letter the panels use
  (`hud::command_letters`), Escape and the digits, each with its reason;
  pretty RON with every field defaulted so an older file loads. The
  settings screen: three settings with step buttons, a CHANGE button per
  control that waits for the next key. The app keeps `settings.ron` in
  the data directory, applies every change at once and writes it at
  once, routes its general keys through the bindings (arrows, the keypad
  and `?` stay fixed aliases), and the controls overlay names the bound
  keys. `crates/app/src/keys.rs` turns key names back into keys for the
  four pan keys the camera reads while held. The font gained `[ ] = ;`
  so those keys can be shown. `docs/04` §33 has the notes.
- **What changed for the player.** SETTINGS on the title. Edge
  scrolling's toggle moved from Shift+E to F3 (E is a panel letter, and a
  binding is one key); Home jumps to the Town Center where H did with
  nothing selected. Fullscreen is borderless and takes effect at once.
- **Measured.** Nothing timed; the file is a few hundred bytes. Two unit
  tests in the settings module, one in `keys`, one shell test, one app
  test through the handlers; the overlay and settings screen goldens.
- **Not done, deliberately.** The panels' command letters (build, train,
  research, stance, formation, stop and the rest) are the HUD's tables
  and are not rebindable; `GD-A11Y-02` is met for the general keys only,
  and full rebinding is owed to M7 or M8 with a per-command capture
  flow. No audio settings (there is no audio). No edge-scroll dead zone
  setting, no key for a second binding per control, no mouse settings.
  Fullscreen has not been tried on the Mac.

### Work record: M6 chunk 5 — notifications and the acceptance run (2026-09-19)

- **What landed.** `crates/view/src/notify.rs`: `Notices`, a stack of
  what happened to the side (an attack, a loss, a technology, an age),
  each with a tile to look at where there is one, timed in match ticks
  so it stands still with the match, leaving after thirty seconds, the
  newest five shown; an attack within twelve tiles and twenty seconds of
  an attack on the stack is not added (`UX-NOTIFY-01`). The HUD draws
  the stack above the panel on the left with a mark per kind, and a
  notice with a place is a `Jump` button; the app puts the camera there
  on a click, before the world gets the click. The app raises notices
  from the simulation's alarm and death events for the viewer's side,
  from an age reached (at the Town Center) and from a technology
  finishing (ages excepted). `docs/04` §34 has the notes. The
  `RM-M6-01` run is an app test: title, setup, a skirmish played to
  VICTORY, a save in the middle, the save reloaded, the recording
  watched to its end, every step a button or a key through the window's
  handlers.
- **What changed for the player.** A stack of notices in the lower
  left: UNDER ATTACK, VILLAGER LOST, HOUSE DESTROYED, STONE MINING
  RESEARCHED, TOOL AGE; clicking one with a place looks there. The
  UNDER ATTACK banner stays as it was.
- **Measured.** The acceptance run, headless: a Tiny map, an Easy
  opponent, forty clubmen take the town inside ten minutes of match
  time; the test runs in under ten seconds of wall time with the rest
  of the app's tests.
- **Not done, deliberately.** No sound cues (M7), no minimap flash or
  ping, no "cannot afford" or "population capped" notice (the panels
  grey the button and say why), no notice for an idle unit at a rally.
  The M6 run has not been done by hand on the Mac; `docs/06` says so.

### Work record: a downloadable Mac build (2026-09-20)

- The owner asked for a build to test on the Mac. Nothing in the
  development environment can produce one (Linux, no Apple SDK), so CI
  does: `.github/workflows/mac-build.yml` builds `New Empire.app` on an
  Apple Silicon runner on every push to `main` and on demand, and attaches
  it to the run as `New-Empire-macOS-<commit>` for thirty days.
  `scripts/bundle-mac.sh` lays the bundle out (the release binary,
  `assets/sprites`, `packaging/macos/Info.plist` with the version and run
  number filled in, `PkgInfo`), ad-hoc signs it and zips it with `ditto`,
  which keeps the bundle's permissions. Without those tools it still
  assembles the layout, which is how it was dry-run on Linux.
- A double-clicked bundle runs with `/` as its working directory, where
  the old lookup (`assets/sprites` in the working directory or an
  ancestor) finds nothing, and the game would have fallen back to
  placeholders without a word. `view::sheets::candidates` now looks beside
  the binary and in `Contents/Resources` first, then the working directory
  and its ancestors. A unit test pins the order; `mapview` copied into a
  bundle layout and run from an empty directory loaded the villager set.
- The first run of the workflow was by hand on the development branch;
  `main` builds on its next push.
- Not done: the bundle is not notarised, so the first launch of a
  downloaded copy goes through Privacy & Security, Open Anyway (the
  README says how); it is Apple Silicon only; it has no icon.

### Work record: M7 chunk 1 — the audio engine (2026-09-20)

- The owner played the downloadable Mac build and reported that it plays
  well, which closes the Mac pass owed on M6 as far as playing goes; the
  save, load and replay steps by hand on the Mac are not separately
  confirmed. `docs/06` M6 says so.
- `crates/audio`, pure: `Bus` (UI, voices, world, music), `Cue` (the
  bark and the selection call per class, the swing per task, hits on
  units and buildings, deaths per class, completed, trained, deposited,
  research, a fanfare per age, click, invalid, alarm, loss), `Clip`,
  `Library`, `Listener`, `Play` and `Mixer`: four voices per cue on the
  wall clock, ±5% pitch from the mixer's own generator, round-robin
  never twice running, gain and pan from the listener with an
  off-screen floor of 0.2 (`TA-AUDIO-01`, `UX-AUDIO-02`).
  `placeholder.rs` synthesises every cue's clips at 22,050 Hz.
  `events.rs` maps a tick's events to cues through one player's fog
  (`TA-AUDIO-02`).
- `crates/sim`: `Task` and `WORK_PERIOD`; events `Completed`,
  `Trained`, `Researched`, `Deposited` and `Work`, the last raised by a
  villager gathering within reach or building once every sixteen ticks,
  staggered by slot. Events are not state: no hash, corpus or replay
  moved; `check-sim-purity` passes.
- `crates/app/src/sound.rs`: `Speaker` (silent, a recorder for tests,
  the device), `Device` on `kira` with a sub-track per bus and every
  clip decoded once, `recordings` loading `assets/sounds/<cue>/*.wav`
  over the placeholders. `main.rs`: the bark raised in `issue` before
  the command is queued (`UX-AUDIO-01`), the selection call at every
  place the selection is set, a click in `do_action` and on shell
  buttons, a buzz on a greyed panel button, the tick's cues in
  `tick_once` through the viewer's fog, the listener from the camera
  each frame, the device opened after the settings in `main`.
  `kira` is `default-features = false` with `cpal` and `wav`.
- Settings: `volumes` (four percentages, music at 70 by default) with
  `volume` and `step_volume`; `ShellAction::Volume`; the settings screen
  widened to 800 with a VOLUME column beside the keys. The
  settings-screen golden image rebaked; the battle goldens moved within
  tolerance and were restored.
- CI: the Linux lint and test jobs install `libasound2-dev`, which
  `cpal` builds against. The bundle script copies `assets/sounds` when
  it exists.
- Tests as `docs/09` records them; `TRACEABILITY_LANDED` gained
  `UX-AUDIO` and `TA-AUDIO`. Nothing has been heard on the Mac: the
  device is opened there for the first time by the next build.

### Resume here next session

**M7 is in progress; chunk 2, music and ambience, is next** (§4d): a
stem per age cross-fading on age-up, the combat stem ducking in, an
ambient bed per terrain, placeholders synthesised on the music bus.
Then the visual feedback, tooltips and hints, the performance pass and
the playtest handoff. The real sprite art is the owner's decision
(§4d, `docs/08` §9 step 3), and the name (`docs/07` Q5) still bites.
The parallel track (§4b) runs on its own branch.

## 4. What M4 completed — Combat

The roadmap's list, in the order we intend to build it. Each step is
shippable on its own and has a headless test before it has a sprite.

1. **Flow fields and the sector graph** (`docs/04` §5). **Done
   2026-09-12**, record above: group movement of 40 units on one field
   instead of 40 A\* searches, the `TA-PATH` tests green, `TA-PATH-02`
   closed, `marching-8p`'s p99 cut from 6.5 ms to 2.4 ms against the aim
   of halving it.
2. **The damage model.** **Done 2026-09-12**, record above: attack,
   armour classes, class bonuses, elevation, minimum damage 1, siege
   friendly fire (`GD-COMBAT-01`–`05`) as a pure function, with the
   damage matrix generated, committed and diffed.
3. **Military units and their buildings.** **Done 2026-09-12**, record
   above: the six slice soldiers in the `kinds` table with their rosters,
   the four military technologies in the `tech` table, the panels and
   placeholders to match. The Siege Workshop still trains nothing: no
   siege unit is in the slice.
4. **Attack orders.** **Done 2026-09-12**, record above: attack,
   attack-move and patrol (`UX-CMD-02`, `-03`), projectiles, target
   acquisition, stances (`UX-CMD-07`), formations (`UX-CMD-08`), and
   villagers fleeing and raising an alarm (`GD-STANCE-02`).
5. **Buildings in combat.** **Done 2026-09-12**, record above: rubble,
   building armour, towers and the Town Center shooting, walls in runs,
   gates that shut on an enemy, garrison (`UX-CMD-09`). The Town Center as
   a buildable followed on 2026-09-18 once Q3 was answered.
6. **The 40-versus-40 acceptance match** (`RM-M4-01`). Automated coverage
   added in the current review chunk: bounded fight, replay corpus, golden
   image and equal-budget counter trials. See the record below and
   `docs/09`. The native functional smoke pass and owner readability approval
   are recorded above; the acceptance and readability PRs are now merged.

## 4b. What M5 completed — An opponent

`docs/06` M5: an AI that plays through the same command interface a human
uses and sees only what a human sees (`GD-AI-01`, `TA-AI-01`, `docs/07`
D7), four difficulties, and victory and defeat. `RM-M5-01` holds in CI:
twenty headless AI-versus-AI matches, no crashes, no stuck units, Hard
beats Easy 20 of 20, 18 by elimination. In the order it was built, each
step shipped and tested headless before it had a sprite:

1. **Fog of war in the simulation and the AI boundary.** **Done
   2026-09-18**, record above: per-player visibility, explored and
   memories from a `fog_of_war_update` pass; `FoggedView` in its own
   crate; the `ai` crate that cannot name the world (`TA-AI-01`), with an
   opponent that issues nothing yet, driven by `simrunner ai`; the
   priority half of `TA-PATH-06`.
2. **Fog in the presentation** (`GD-FOG-01`). **Done 2026-09-18**, record
   above: a light per tile corner shading the terrain in both renderers,
   buildings and nodes drawn from memory where seen once, units only in
   sight, the minimap fogged and now in every golden image, `fog-scout`
   pinning the three states in one frame.
3. **The economy manager and build orders.** **Done 2026-09-18**, record
   above: villagers to resources by share, houses ahead of the cap, farms
   once the food in sight is short, the age gate met and taken, a build
   order per difficulty; and the head-on walking stall it found, fixed.
4. **The military manager and scouting.** **Done 2026-09-18**, record
   above: the scout rides rings to unseen ground; soldiers to a
   composition by age; the alarm answered at home; raids at soft targets
   and the army at the Town Center by size; a tower for Hard. Walls are
   owed.
5. **Difficulty, victory and defeat.** **Done 2026-09-18**, record
   above: conquest, resign, the score at a time limit, Hardest's declared
   bonus, `simrunner versus` with its record and the `acceptance` CI job;
   Hard 20 of 20 on score.

### The parallel track

Three items from `docs/09` §11 are independent of M5 and are being done
alongside it by a second agent (Codex), on its own branch, merged by pull
request. The fence that keeps the two from colliding:

| Item | Touches | Order and rule |
|---|---|---|
| **The nightly job.** A workflow on a schedule: the soak runner at depth, the property tests with a case count in the thousands, a mapgen sweep over 1,000 seeds (`docs/09` §5), the fuzz targets for a bounded time. | `.github/workflows/nightly.yml`, `scripts/check-workflows.sh` | First: nothing in M5 touches these files. |
| **The fuzz targets.** `cargo-fuzz` targets for the replay reader and the command interface, rebuilt for the nineteen command variants of today (`Command::validate` and `Simulation::issue` must survive any bytes). | A `fuzz/` crate, `Cargo.toml` workspace excludes, the README | Second: also disjoint from M5. |
| **The resource-conservation invariant** (`docs/09` §5): map remaining + carried + stockpiled + spent is constant per tick, with a reseed converting 60 wood into a farm's food at seeding and a destroyed site's unbuilt cost gone for good. | `Simulation::check` and its `Violation`, the corpus and soak runners that call it, a behaviour test | Last, and as a check and a test only. If it fails on an existing corpus match, the pull request reports the failure; it does not change simulation behaviour or re-record digests, since M5's fog pass will be re-recording the corpus and two sets of digest edits do not merge. |

Neither track edits the other's files. The parallel track does not edit
the status rows in §1, the plans in §4 and §4b or the landed list in
`scripts/check-traceability.sh`; it appends its own work record to §3 and
the merge reconciles. M5 stays out of the three files above.

---

## 4c. What M6 completed — The game shell

`docs/06` M6: a player launches the game, configures and plays a full
skirmish to a victory screen, saves mid-match, reloads, and watches the
replay, without touching a terminal (`RM-M6-01`). In the order we intend
to build it, each step shippable and driven headless through the app's
own handlers before anyone clicks it:

1. **The shell, the setup screen and the opponent in the app.** **Done
   2026-09-19**, record above: a title screen; a setup screen with the
   map size, one to seven opponents each at a difficulty, the population
   cap and the seed, the map previewed, the Hardest bonus declared beside
   the opponent that gets it; the opponents thinking in the app's tick
   loop; the pause menu with resign and quit; the results screen.
2. **Save and load** (`TA-SAVE-01`, `TA-DET-06`). **Done 2026-09-19**,
   record above: `crates/save`, the whole simulation with its log and the
   opponents' minds, three versions checked before the world is read;
   SAVE GAME and F5; the load screen; `simrunner verify` on a save.
3. **Replay playback with speed controls.** **Done 2026-09-19**, record
   above: every match played is recorded when decided or left, one file
   per match; WATCH REPLAY lists the recordings; playback from the log
   with pause, speed to 16× and Tab for whose eyes, nothing issued.
4. **Settings.** **Done 2026-09-19**, record above: `settings.ron` in
   the data directory; the HUD size, edge scrolling and the window mode;
   every general key rebound on the settings screen with the panels'
   letters, Escape and the digits refused (`GD-A11Y-02` for the general
   keys; the panels' command letters are owed).
5. **Notifications with click-to-jump and the acceptance run.** **Done
   2026-09-19**, record above: the stack in the lower left, attacks
   rate-limited by area (`UX-NOTIFY-01`), a click looking where a
   notice points; and the `RM-M6-01` run through the app's handlers,
   title to victory to save to load to replay. The Mac pass is owed.

The name (`docs/07` Q5) now bites: the title screen shows the
placeholder.

---

## 4d. What comes next: M7 — The feel pass

`docs/06` M7: the milestone that decides whether this is the game you
remember. In the order we intend to build it, each chunk shippable and
verified headless before anyone hears or sees it on the Mac:

1. **The audio engine.** **Done 2026-09-20**, record above: `crates/audio`
   with the four buses, cues, voice limiting, pitch variation, round-robin
   and positional gain (`TA-AUDIO-01`, `UX-AUDIO-02`); the simulation's
   new events and their mapping through the fog (`TA-AUDIO-02`); the
   units answering the moment they are told (`UX-AUDIO-01`); clicks,
   refusals, the bell and the fanfares; a synthesised placeholder for
   every cue with recordings replacing them by name; `kira` in the app;
   the volumes on the settings screen.
2. **Music and ambience.** A stem per age cross-fading over four seconds
   on age-up, the combat stem ducking in when six or more units fight in
   view, an ambient bed per terrain under the camera; placeholders
   synthesised, on the music bus.
3. **Visual feedback** (`docs/03` §6.2), with placeholder art: the hit
   flinch and spark, the kill puff, three visible construction stages,
   bushes thinning and veins shrinking, trees falling, the screen-edge
   indicator for an attack off-screen. Golden images.
4. **Tooltips, hints and notification polish.** Every unit and building
   tooltip with cost, build time, counters and hotkey (`UX-TIP-01`);
   contextual hints shown at most twice and disableable; the rest of
   `docs/03` §6.3: the idle-at-rally chime, cannot-afford with the
   resource flashing, the minimap flash and ping.
5. **The performance pass and the playtest handoff.** The scenarios
   against `docs/04` §12 on known hardware, cliffs fixed and ceilings
   lowered; the `RM-M7-01` observation sheet in `docs/09`; the Mac
   measurement and the six players are the owner's.

**Not in these chunks:** the real sprite art for two civilisations across
three ages and its animations (`docs/06` M7 bullets 2 and 3). They are
the art pipeline's step 3 (`docs/08` §9), which needs a modeller and a
Blender install; nothing here can produce them. The vertical slice is not
complete without them, and the owner decides when and by whom.

---

## 5. Owed items and known debt

Stated so they are not rediscovered.

- **Hunting** (`docs/07` D15) waits for a carcass: animals cannot be
  attacked yet.
- **Repair** (`docs/03` §3) is unimplemented: a damaged building stays
  damaged until it falls. Villagers "repair" in `docs/02` §5.1.
- **Waypoints** (`UX-CMD-04`) are untouched; Shift only keeps placement
  and targeting armed.
- **Attack-move is `M`**, where `docs/03` says `A`; `A` pans the camera.
- **The panels' command letters are not rebindable** (`GD-A11Y-02`):
  the settings screen rebinds the seventeen general keys only. Full
  rebinding needs a per-command capture flow and the HUD's tables read
  through the bindings.
- **Every sound is a placeholder** (`docs/07` Q7): synthesised tones and
  noise. Recordings under `assets/sounds/<cue>/` replace them by name.
- **No music, no ambient beds** yet: M7 chunk 2.
- **The notification cues are partial**: the bell, the loss and the
  research note play; the idle-at-rally chime, cannot-afford and the
  minimap flash and ping are chunk 4.
- **The Mac build is not notarised, Apple Silicon only, and has no icon.**
  Notarising needs an Apple Developer account and a signing identity in
  the workflow's secrets; an Intel slice needs a second target and `lipo`;
  the icon waits on the name (`docs/07` Q5).
- The age-up **fanfare** waits for audio (M7). The sweep and banner exist.
- **Auto-reseed is per player**, not per farm as `docs/02` [GD-ECON-05]
  asks. A per-farm flag needs a per-entity toggle in the world store.
- **Age variants exist for placeholders only.** Rendered sets carry no
  variants yet; `Atlas::variant` answers with the base kind for them. The
  sprite manifest needs a per-age entry when the art stream models the
  slice (`docs/08` §9 step 3).
- **No resource-conservation invariant, no fuzzing, no nightly job**
  (`docs/09` §11): the parallel track in §4b.

---

## 6. Decisions needed

Open questions in `docs/07` that will block or shape the next milestones,
in the order they bite:

| Question | Blocks | Recommendation |
|---|---|---|
| Q9 — A second ownership cue besides colour | M4 (readability of a fight), M7 | Decide before combat art is commissioned; a banner glyph per player is the cheapest candidate |
| Q1 — Naval in the vertical slice? | M4 scope | Leave it out of the slice; the map generator has water but nothing sails |
| Q8 — Four ages or five? | Content tables | Four, as `docs/02` stands; M3 shipped the four-age structure |
| Q5 — The game's name | M6 (menus), M9 | Biting now: the title screen shows the placeholder, one constant (`view::shell::TITLE`) to change |

---

## 7. How to check the state yourself

```sh
cargo test --workspace                       # everything, headless
scripts/check-traceability.sh                # every landed requirement has a test
scripts/check-perf.sh                        # bench against perf/budgets.ron
cargo run -p simrunner -- golden             # the replay corpus still verifies
cargo test -p mapview --test golden_images   # the pinned frames still render
scripts/bundle-mac.sh                        # New Empire.app into target/bundle (on a Mac)
```

The golden images under `tools/mapview/tests/golden-images/` are the
quickest way to see what the game looks like without building it.

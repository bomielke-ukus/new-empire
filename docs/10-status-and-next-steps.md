# 10 — Status and next steps

**As of 2026-09-12.** The living summary of where the project is and what
comes next, for anyone joining or checking in. The roadmap (`docs/06`) holds
the milestone definitions and their acceptance tests; this document says
which of them are done, what was learned, and what the next steps are. Update
it whenever a milestone lands or the plan changes.

**Target platform: macOS.** Development, live testing and release acceptance
focus on the Mac. Existing Windows/Linux CI jobs remain additional
portability and determinism checks, without a shipping commitment.

---

## 1. Where we are

Four milestones landed; the vertical slice is at its halfway point.

| Milestone | Status | Acceptance |
|---|---|---|
| M0 — Foundation | Landed | `simrunner determinism --ticks 10000`: a synthetic match replays bit-identically |
| M1 — A world you can look at | Landed | A map from a seed, scrolled and zoomed, rendered by the GPU path and the software rasteriser alike |
| M2 — Villagers, movement, economy | Landed | Gathering all four resources, building, training with rally points, on a pathfinder that does not get stuck |
| M3 — Ages, production and technology | **Landed 2026-09-11** | Stone → Tool → Bronze in a live match, with the settlement visibly changing at each transition |
| M4 — Combat | **In progress** (steps 1–5 of 6 landed 2026-09-12) | Two forces of 40 fight; counters work; nothing gets stuck |
| M5 — An opponent | Not started | 20 headless AI-vs-AI matches, Hard beats Easy 18 of 20 |
| M6 — Game shell | Not started | Configure, play, save, reload and watch a replay without a terminal |
| M7 — The feel pass | Not started | Someone who loved the original plays a match and does not want to stop |
| M8 — Breadth, M9 — Content | Not started | Beyond the vertical slice |

One caveat applies to every landed milestone: the GPU window has been
verified by compilation and by the shader validator, not by running it on a
display, because development has been headless. The software rasteriser
(`docs/07` D13) is the reference renderer and is what the golden images and
every screenshot in this repository come from. Running the app on a real
machine is worth doing before M4 goes far.

### What you can do in the game today

Start a skirmish on a generated Inland map, select and command villagers,
gather food, wood, stone and gold, build Houses and Storehouses, train
villagers from the Town Center with rally points onto resources, put up the
ten M3 buildings as your age allows, research technologies at the Storehouse
and Market, advance Stone → Tool → Bronze → Iron behind the resource-and-
buildings gate, and farm with auto-reseed. Train the six Tool Age soldiers,
attack, attack-move and patrol with stances and formations; drag a palisade
or stone wall, set a gate into it, put up a Watch Tower and garrison it or
the Town Center; knock the other side's buildings down to rubble. There is
no opponent yet: the other players sit still unless a scenario moves them.

```sh
cargo run --release -p new-empire           # the game window
cargo run -p mapview -- --scenario ages --stockpile 5000 --ticks 100 \
    --select-tc 1 --hud 1 --out frame.png   # the same frame, headless
```

### The three streams

Work has run as three streams that merge into this branch:

- **Core** (simulation, view, app, tools): M0–M3, above.
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
- **Not done, deliberately.** Repair (`docs/03` §3's cursor table) is not
  in the plan's step and waits; a damaged building stays damaged. The
  Guard Tower is not a slice item. "Automatic gate suggestion at road
  crossings" (`UX-PLACE-03`) has no roads to suggest at. Collapse and dust
  are art (M7); rubble appears at once. The Town Center stays off the
  build panel until Q3 is answered (§6). Hunting still waits.

### Resume here next session

M4 chunks 1 to 4 are landed. Next is step 6 below: the 40-versus-40
acceptance match and the balance harness. Art continues on the `docs/08`
schedule.

## 4. What comes next: M4 — Combat

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
   a buildable still waits on Q3.
6. **The 40-versus-40 acceptance match** (`RM-M4-01`) as a corpus entry and
   a golden image, plus the balance harness `docs/09` describes: counters
   win as designed over N trials, headless.

Alongside, not blocking: the resource-conservation invariant (`docs/09`
§11), rebuilding the fuzz targets for the current command set, and the
nightly job.

---

## 5. Owed items and known debt

Stated so they are not rediscovered.

- The **priority** half of `TA-PATH-06` (player-issued orders before
  AI-issued ones) is unimplemented until M5 supplies an AI.
- **Hunting** (`docs/07` D15) waits for a carcass: animals cannot be
  attacked yet.
- **Repair** (`docs/03` §3) is unimplemented: a damaged building stays
  damaged until it falls. Villagers "repair" in `docs/02` §5.1.
- **Waypoints** (`UX-CMD-04`) are untouched; Shift only keeps placement
  and targeting armed.
- **Attack-move is `M`**, where `docs/03` says `A`; `A` pans the camera.
- The age-up **fanfare** waits for audio (M7). The sweep and banner exist.
- **Auto-reseed is per player**, not per farm as `docs/02` [GD-ECON-05]
  asks. A per-farm flag needs a per-entity toggle in the world store.
- **Age variants exist for placeholders only.** Rendered sets carry no
  variants yet; `Atlas::variant` answers with the base kind for them. The
  sprite manifest needs a per-age entry when the art stream models the
  slice (`docs/08` §9 step 3).
- **The Town Center is hidden from the build panel** until Q3 (Government
  Centre) is decided; the simulation still accepts placing one.
- **No resource-conservation invariant, no fuzzing, no nightly job**
  (`docs/09` §11).

---

## 6. Decisions needed

Open questions in `docs/07` that will block or shape the next milestones,
in the order they bite:

| Question | Blocks | Recommendation |
|---|---|---|
| Q3 — Does the Government Centre earn its own building? | The buildable Town Center (M4 step 5 landed without it), the Bronze roster | Keep it as its own building; it is already placed and priced, and folding its upgrades into the Town Center saves less than it costs in legibility |
| Q9 — A second ownership cue besides colour | M4 (readability of a fight), M7 | Decide before combat art is commissioned; a banner glyph per player is the cheapest candidate |
| Q1 — Naval in the vertical slice? | M4 scope | Leave it out of the slice; the map generator has water but nothing sails |
| Q8 — Four ages or five? | Content tables | Four, as `docs/02` stands; M3 shipped the four-age structure |
| Q5 — The game's name | M6 (menus), M9 | Not urgent; needed before the shell has a title screen |

---

## 7. How to check the state yourself

```sh
cargo test --workspace                       # everything, headless
scripts/check-traceability.sh                # every landed requirement has a test
scripts/check-perf.sh                        # bench against perf/budgets.ron
cargo run -p simrunner -- golden             # the replay corpus still verifies
cargo test -p mapview --test golden_images   # the pinned frames still render
```

The golden images under `tools/mapview/tests/golden-images/` are the
quickest way to see what the game looks like without building it.

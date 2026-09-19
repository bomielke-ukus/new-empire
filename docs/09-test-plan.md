# Test Plan

How we find out whether *New Empire* is any good before a player does.

`docs/04-technical-architecture.md` §11 is the one-paragraph version this grew
out of. Everything described here as landed exists and runs; everything marked
**M*n*** is designed but waiting on the system it tests.

---

## 1. What can go wrong, and what catches it

Three failure modes, three different kinds of machinery. Conflating them is how
a project ends up with a thousand unit tests and a game that crashes.

| Failure mode | What actually catches it |
|---|---|
| **Crashes and hangs** | Soak runs under `catch_unwind` with a watchdog, per-tick invariant checks, validation of everything read off disk |
| **Wrong behaviour, and desync** | Golden per-tick trace digests, property tests against an `i128` reference, a build/OS/architecture matrix |
| **Missing features** | Requirement IDs traced from the specs into tests and enforced in CI, golden images, data validation |

The lever that makes almost all of it automatable is determinism. Because the
simulation is a pure function of `(seed, command log)`, a bug is a file, a file
is a regression test, and the whole game runs headless at a thousand times real
speed. Very little of this plan would be affordable otherwise.

---

## 2. Six layers

Cheapest and fastest first. Only the last has a human in it.

| Layer | What | Cost |
|---|---|---|
| **1. Static gates** | `fmt`, `clippy -D warnings`, sim purity, generated-file freshness, requirement traceability, CLI-caller and workflow validation | seconds |
| **2. Unit and property tests** | `cargo test`, `proptest` over the maths, the queue and the entity store | seconds |
| **3. Invariant checking** | `World::check` and `Simulation::check`, after every tick under `--features debug-checks` | free in shipping builds |
| **4. Golden traces and images** | A committed replay corpus with a digest over every tick, and committed PNGs of fixed scenes | ~15 s |
| **5. Soak and benchmarks** | Randomised matches across the config space; per-tick timings against ceilings | minutes |
| **6. Human playtest** | The feel, the audio texture, the M7 acceptance criterion | scheduled, out of CI's way |

### The distinction layer 4 exists for

**Determinism** is "the same replay produces the same result twice".
**Stability** is "the simulation still produces the result it produced when the
corpus was recorded". They are not the same property, and only the first was
tested before this plan.

A refactor can be perfectly deterministic and quietly change what a unit does.
It passes every determinism test and invalidates every replay and save file in
existence without saying so. The `.golden` files are the tripwire: when one
changes, either it was intended — `simrunner golden --update` records it and
the diff goes through review — or the diff just caught a bug. Verified by
raising a tree's wood yield from 75 to 76, which fails four entries by name.

---

## 3. What runs, and when

| Job | Contents |
|---|---|
| **Lint and purity** | fmt, clippy, sim purity, generated files, traceability, CLI callers, workflow validation |
| **Test (×3 OS)** | Every test including the corpus and golden images; release counter-balance trials with invariant checks; determinism runs; a software-rendered frame |
| **Hashes agree** | The final state hash from all three platforms must be identical |
| **Performance** | Benchmark scenarios against `perf/budgets.ron`, with the numbers posted to the run summary |
| **Soak** | 300 randomised matches with invariant checking; failure replays uploaded |

### Why the platform matrix matters

**macOS is the product target.** Its test job and a live Mac hardware pass
are the target-platform evidence. Windows/Linux jobs remain extra
portability and determinism checks, not shipping commitments.

`docs/04` §2 promises bit-identical state on every machine, and nothing but
running it proves that. The corpus digests are recorded on x86_64 Linux and
checked on Windows and on aarch64 macOS — a different architecture, not just a
different OS. They match, which is the strongest evidence so far that the
fixed-point simulation is genuinely portable.

Debug and release builds also differ in a way that matters: debug panics on
integer overflow, release wraps. Several of the defects found while writing
this plan were exactly that — the same input, two answers, depending on how the
binary was built.

---

## 4. What is tested today

### 4.1 Arithmetic — the substrate

`docs/04` §13 states the invariants; `TA-FX-*`, `TA-VEC-*` and `TA-ANG-*` name
them. Every operation is checked against a reference computed in `i128`.

The ones that matter most: addition and subtraction **saturate** rather than
wrapping (a wrap is a silent teleport); division rounds to nearest with halves
away from zero, which is `docs/07` D10 and the reason units arrive at 3 tiles
rather than 2.9992; and `mul_div` matches the exact rational, since its whole
purpose is an intermediate that cannot overflow.

Trig is checked **exhaustively** over all 65,536 angles rather than sampled.
The domain is small enough that exhaustive is cheap, and it leaves no table
boundary for a sampler to miss.

> **TA-PATH-01** — `move_toward` makes strict progress on every call, never
> overshoots, lands *exactly* on the target when it is within reach, and
> arrives in a bounded number of steps.

Every "unit stands still forever" bug reduces to that property failing.

### 4.2 Random numbers

The output stream is **frozen** by a committed known-answer vector
(`TA-RNG-01`). Changing the generator invalidates every replay and save ever
recorded; the vector makes that happen deliberately. Alongside: every
range-limited draw is in range including the empty and maximal cases;
serialisation round-trips the stream; and **exactly one draw per call**
(`TA-RNG-04`), because desync diagnosis compares draw counts to localise where
two machines parted company, which only helps if the count is a function of the
code path taken.

### 4.3 Commands and the entity store

> **TA-DET-03** — the queue's state is independent of the order commands
> *arrived* in.

Two peers whose packets interleaved differently must hold byte-identical
queues, or network jitter alone desyncs them and the bug report is
unreproducible. Tested as a property over random command sets and random
*interleavings* of the per-player streams — not arbitrary shuffles, because a
player's own commands keep their relative order at the transport level and
reordering them is a genuine state change.

The state hash covers the queue's full contents, not just its length. Two
simulations holding different pending orders used to agree for two ticks and
then diverge with no attributable cause.

The entity store is checked over random operation sequences: slot reuse is
lowest-index-first (`TA-ENT-02` — the identity a new entity receives is part of
the state, so machines that allocate differently have already diverged); stale
handles never resolve; iteration is slot order; and `despawn` scrubs every
component column (`TA-ENT-05`).

That last one had no coverage at all before: M2's suite never despawns a
resource-bearing entity, because an exhausted node is already zero. Removing a
column's scrub failed nothing until a test was written that dirties every
column first.

### 4.4 The corpus

Twelve recorded matches with a digest over every tick's hash, driving
gathering and drop-off, construction, training, research, ages, rally points,
group pathing, combat and building defences. `battle-40v40` adds a bounded
mixed-army fight to the economy and edge-case scenarios. A corpus that only
moved units around would not notice a change to the economy, which is most of what the simulation now does.

They span the *edges* as well as the middle, because the default config is the
one everything is developed against and therefore the one a bug is least likely
to hide in: the smallest and largest map sizes, one and eight players, an
entity cap low enough that spawns are refused constantly, wander off, a bare
`Flat` map with no start kit at all, an idle match that issues nothing, and a
20,000-tick run so drift that needs time to accumulate has time.

`simrunner record` reports what each scenario *achieved* — live entities,
buildings standing, resources gathered — not just that it ran. That readout
caught three scenarios doing nothing, including one where marching dragged
villagers off mid-gather so 2,000 ticks gathered exactly zero.

**Every crash found anywhere becomes a corpus entry.** That is the corpus's
main job over time, and what turns the soak from a one-off run into a ratchet.

### 4.5 Rendering

Thirteen scenes rendered through `tools/mapview` and compared against committed
PNGs with a tolerance. The test drives the binary rather than the rendering
library, because the command line is what CI invokes and what a developer
types.

`mapview` renders through the software rasteriser in `crates/view`, which is an
advantage over a GPU or even a software Vulkan driver: there is no driver to
vary between runners, so a pixel difference means the renderer changed rather
than that the machine did. A separate test asserts two renders of the same
scene are byte-identical, because a golden test on a non-deterministic renderer
is a coin toss.

### 4.6 Robustness

A replay is a file: it arrives from a bug report, a save directory, or
eventually a download. `Replay::validate` bounds `ticks`, requires
non-decreasing issue ticks, and range-checks every command; `run` and `verify`
call it, so nothing executes unvalidated.

`tests/robustness.rs` pins that every command variant from a player the match
does not have is an inert no-op, since a replay can name any player index
inside `MAX_PLAYERS` while a match may have one.

---

## 5. What is tested when it lands

### Pathfinding — M2 shipped; original test backlog closed

See §7. Flow fields closed `TA-PATH-02`. Player-versus-AI order priority
remains deferred until M5 supplies competing AI orders.

### Map generation — M1 shipped

Same seed produces a bit-identical map (`GD-MAP-01`, `RM-M1-01`), covered.
Still owed: a nightly sweep over 1,000 seeds, because a generator that fails
one seed in five hundred will meet that seed in front of a player.

### Economy and ages — M2 and M3 shipped

Chunk 3 adds `crates/sim/tests/production_capacity.rs`: a completed, paid
unit stays queued at the entity cap, its tail cannot overtake it, freeing
one slot produces exactly one unit with its rally order and no second charge,
and cancellation while capped refunds the full cost. The recovery scenario
also verifies its replay, and these tests run with debug invariant checks.
The existing `entity-cap-pressed` replay's expected digest changes with this
fix; its input commands remain unchanged. Older expected hashes for a replay
that loses production at the entity cap will no longer match.

M3's tests live in `crates/sim/tests/behaviour_ages.rs`: the building gate
(`GD-AGE-01`, three ways), technology queued at its building in its age after
its prerequisites and applied through modifiers, cancel refunds, farms
reseeding for sixty wood until the wood runs out and the toggle off and on
(`GD-ECON-05`), farms worked only by their owner, a farm seeded on
completion, and the milestone walk Stone → Tool → Bronze by commands alone
(`RM-M3-01`). The presentation half (`GD-AGE-02`) is pinned in `view`'s
tests and by the `ages-tool-hud` and `ages-bronze-sweep` golden images. The
corpus gained `ages-2p`, a rich two-player match whose bot researches,
advances and farms.

The strong one, still not written: **resource conservation** as a per-tick
invariant — map remaining + carried + stockpiled + spent is constant. Every
duplication and every leak violates it and it costs nothing to check. Farms
make it slightly more interesting: a reseed converts 60 wood into 250 food
at the moment of seeding.

### Combat — M4

**Landed in chunk 2:** the damage model as a pure function
(`crates/sim/src/combat.rs`), with one unit test per rule claiming
`GD-COMBAT-01` to `05`; the damage matrix generated from the kinds table by
`simrunner matrix`, committed at `docs/damage-matrix.md` and diffed by
`scripts/check-generated.sh`; and `behaviour_military.rs`, which trains at
the Barracks, Archery Range and Stable, checks the age and line gates with
the words the panel shows, walks the Axe upgrade through live and queued
Clubmen, and reads the effect of Toolworking, Leather Armour and Fletching
off `Simulation::damage_between`. The `army-hud` golden image pins the
roster panel and the six placeholders.

**Landed in chunk 3:** `behaviour_combat.rs`, on a flat empty map so no
start-kit scout wanders into a test: an attack order closes, hits on the
reload and kills, with the corpse lingering and then going; a bowman
shoots from range and the arrow takes time to land; the four stances
decide who engages and how far they chase (`GD-STANCE-01`); a villager hit
runs for the Town Center and the side is told once (`GD-STANCE-02`);
attack-move engages on the way and carries on (`UX-CMD-02`); a patrol
turns round at each end (`UX-CMD-03`); a formation forms a line across the
way at the slowest member's pace, and no formation is a clump
(`UX-CMD-08`); and a twelve-a-side fight replays identically. The app
tests drive the panel's attack-move, patrol, stance and formation
buttons and the right-click attack, and check that a corpse is not
picked (`UX-CMD-07`). The `battle-hud` golden image pins two lines
fighting: arrows in the air, the first bodies down.

**Landed in chunk 4:** `behaviour_siege.rs`, on the same flat map with a
deep stockpile: a palisade hit to nothing becomes rubble, the breach opens
at once, the rubble goes after a minute; a site takes damage and its
destruction refunds nothing (`GD-BUILD-01`); a Watch Tower shoots a passer-by
unordered and two bowmen inside make it three arrows a volley; units garrison
in a Town Center, stop counting as idle, keep counting toward population,
take no orders, come out round the footprint when let out, and step out
unhurt when a tower falls (`UX-CMD-09`); a gate lets its owner through, shuts
on an enemy who cannot pass and gives up, and opens once they leave; a gate
set onto an own wall segment replaces and refunds it, and one villager builds
a three-segment run alone (`UX-PLACE-03`); a column on attack-move walled out
of its destination breaks the wall where it stands and goes on through the
breach (`UX-CMD-02`); a villager hit near home shelters in the Town Center
and it shoots back (`GD-STANCE-02`); and a siege replays identically from its
command log. The app test drives the right-click garrison, the ALL OUT
button, the DEFENCES page and a dragged wall run. The `siege-hud` golden
image pins their column at the shut gate, the tower's volley and the
tower's panel with its garrison.

**M4 acceptance automation:** `tools/simrunner/src/arena.rs` supplies a
command-driven, flat-map fight with exactly 40 units per side: 20 Spearmen,
10 Slingers and 10 Bowmen versus 20 Axemen, 10 Bowmen and 10 Light Cavalry.
Both armies attack-move to the centre. The run must finish decisively within
6,000 ticks. Every tick checks simulation invariants; every living soldier
must move at least half a tile, attack, or take damage within 400 ticks.
This catches an individually inactive unit even while others keep fighting.
It is an inactivity detector for this arena, not proof that every possible
combat path is free of jams. A deliberately stopped, passive army confirms
that the detector fails when nobody acts.

`tools/simrunner/tests/combat_acceptance.rs` claims the automated portion of
`RM-M4-01`, checks exactly 80 spawns, completion and per-tick replay equality,
and asserts that the recipe still equals the committed `battle-40v40.ron`.
The corpus pins its digest, and `mapview --replay` renders that same input at
tick 200 for `battle-40v40.png`. No existing corpus input, digest or image
needs to change for this addition. These debug-spawned armies deliberately
bypass economy and age gates; training is covered by `behaviour_military.rs`.

**Counter balance:** `simrunner balance` runs the two explicit slice counters:
12 Spearmen versus 9 Light Cavalry (720 total resources each), and 14 Slingers
versus 10 Axemen (700 each). Ten seeds vary spacing, offset and deployment
axis; every trial swaps owners and starting sides, for 40 fights total.
Each counter must win at least 90% on each side separately; any inactivity,
invariant failure or timeout fails the run. The CLI can dump losing or
failed replays with `--dump`. These equal-budget trials value each resource
unit equally and use base stats, level ground and no micro. They detect drift
in these matchups, not competitive balance across terrain, technologies,
production times or every roster combination. Workspace tests run them in
debug; each platform's CI job also runs the CLI in release with invariants.

**Manual acceptance status (2026-09-13):** the native Mac functional smoke
pass is recorded in `docs/10`: training, garrison/ungarrison, wall construction,
gate replacement/passage, explicit gate attack, attack-move through the breach,
building rubble and a completed 40-versus-40 fight. Automatic selection of a
breach in a fully closed wall was not separately established in the native
pass; its automated coverage remains above. The owner found the combat visuals
unclear on the first pass: arrows were visible but looked quite random.
After PR #10's presentation changes, the same native battle ran at 1x and the
owner approved it: “Yes, this is clear enough to proceed.” The readability
half of `RM-M4-01` has passed; PRs #9 and #10 merged and M4 landed on 2026-09-13.
Automated acceptance alone does not close the milestone.

The readability follow-up adds regression checks for all eight arrowhead
orientations, owner-colour fletching, impact cue expiry and replay equivalence,
and the distinction between age-up and attack banner subtitles. Four combat
reference images are intentionally updated; replay inputs and digests remain
unchanged. The initial app-control timeout was resolved on retry. Native checks
confirmed the corrected attack warning, the expected 17 battle survivors and
identical replay results through tick 835 (`docs/10`, 2026-09-13).

### The AI — M5

**Landed in chunk 1:** fog of war in the simulation and the AI boundary.
`crates/sim/tests/behaviour_fog.rs`: a tile is unexplored, then visible,
then explored, a building out of sight is remembered until the tile is
seen again and the memory outlives the building (`GD-FOG-01`); garrisoned
units and corpses do not see; and the player's path requests are planned
before a computer opponent's when the destination budget binds, nobody is
dropped, and the replay records who issued what (`TA-PATH-06`, whose
priority half this closes; §7). `crates/fogged`'s test reads a view: own
things anywhere, others only in sight, memories, placement refused on
ground never seen. `crates/ai/tests/compile_fail.rs` is the boundary:
three programs that try to name the world from the `ai` crate, each built
as its own crate on `fogged` alone, and each must be rejected with an
unresolved-path or private-item error code (`TA-AI-01`), which they are
because `sim` is not a dependency of that crate and `fogged` re-exports no
path to it. The error's wording is not compared: it changed between the
local toolchain and CI's, so an exact-text check was a toolchain check. `docs/07`
D7 calls this architectural, and review is not an architecture.
`tools/simrunner/tests/ai_cli.rs` runs `simrunner ai` for two short
matches with three opponents each and checks the recordings verify.

**Landed in chunk 2:** fog in the presentation (`GD-FOG-01`). In
`crates/view`: the corner lights are black beside ground never seen and
the mean between ground seen once and ground in sight (`fog.rs`); a scene
built for a viewer leaves out someone else's unit out of sight, draws
their building in sight live, and once it is out of sight draws it from
memory at the explored light, in the same frame where it stood, with no
slot to pick, while the other side's view holds none of it (`scene.rs`);
the minimap is black, dimmed with the remembered house marked, or live
tile by tile (`minimap.rs`); and the rasteriser darkens the ground by
state, scales a sprite by its light, and draws the minimap in its diamond
and nowhere outside it (`raster.rs`). Every golden image is re-baselined
for the player's fog and the minimap in the panel; `fog-scout` pins the
three states in one frame: the settlement in sight, the scout's trail seen
once with their house remembered on it, the scout in its own circle of
sight, black beyond, and the minimap fogged the same. The headless render
test uploads the fog lights and draws a fogged scene on a real device
where one exists.

**Landed in chunk 3:** the economy manager, the first commands an opponent
issues (`GD-AI-01`). `tools/simrunner/tests/ai_economy.rs` (there and not
in `crates/ai`, which must not depend on `sim` even for tests): a Standard
opponent left alone for eight minutes trains villagers to its target,
houses them ahead of the cap, gathers food and wood, puts up the
Storehouse and Barracks the Tool Age needs and takes the age, with no
villager idle longer than twenty seconds, every command validated and
issued as the AI, the other side untouched, and the match replaying
identically; the same seed and difficulty think the same thoughts twice;
Easy ends with fewer villagers than Standard and never leaves the Tool
Age; two opponents share one match and it replays. `crates/ai`'s unit
tests pin the build orders' shape. `behaviour_pathfinding` gained the
head-on case the opponent's first builders found: two walkers ordered
past each other along one row must step aside and both arrive
(`TA-PATH-05`); it fails without the separation change.

**Landed in chunk 4:** the military manager and scouting.
`tools/simrunner/tests/ai_military.rs`: Hard against Easy for twenty
minutes, Hard's scout has seen at least 40% of the map and Easy's under
15%, Hard has raised an army and its raids have cost Easy something, Hard
is ahead in age or villagers, and the match replays; and a Standard
opponent with three clubmen by its Town Center, raided by two enemy
clubmen sent at a house, answers the alarm and the raiders die with a
defender left standing. `crates/ai`'s unit tests pin the compositions and
the scout's compass. `simrunner ai --difficulty hard,easy` runs mixed
matches and `--stats` shows each side's soldiers.

**Landed in chunk 5:** victory, defeat and the acceptance run.
`crates/sim/tests/behaviour_victory.rs` (`GD-WIN-01`): a side with a house
and nothing else is out and the other has won; a lone villager or a lone
Town Center keeps a side in; resigning is defeat, silences the side and
replays; the score counts what was gathered and what stands; the gather
bonus is a match setting the setup screen bounds. `simrunner versus` is
the `RM-M5-01` run: twenty matches, Hard against Easy, thirty minutes
each, invariants every tick, a villager idle for sixty seconds with a
resource in sight counted as stuck, one line per match recorded in
`tools/simrunner/tests/versus-hard-easy.golden`; the `acceptance` CI job
plays all twenty against the record and needs eighteen Hard wins.
`tools/simrunner/tests/ai_versus.rs` plays the first recorded match in
full and requires the recorded line, and shows a short match decided on
score and the same setup playing the same match. The first record was
Hard 20 of 20, all on score; after the tuning pass (`docs/04` §29) it is
20 of 20 with 18 by elimination, at a forty-minute limit.

### Interface — M2–M6

The chunk-2 review fixes add five app-level regression tests in
`crates/app/src/tests.rs`, run by `cargo test -p new-empire`. These invoke
the same click/hotkey handlers as the window, build the actual HUD, and
advance the simulation command queue before checking outcomes:

- Minimap clicks at three viewport sizes, including during building
  placement; drag scrubbing preserves selection and does not place buildings.
- Minimap right-click movement and Town Center rally orders.
- HUD space outside the minimap diamond does not issue world commands.
- Storehouse and Market research cancellation via both mouse and hotkey,
  with exact refunds, no applied technology, and the empty queue reflected
  in the HUD.
- Mixed selection cancels the displayed building's queue while preserving
  another selected building's training; Town Center cancellation still works.

Four of these tests failed against the pre-fix input handlers, establishing
that they catch the reviewed bugs. They run headlessly with a paused frame
clock and explicit simulation ticks. They cover input routing and cancellation;
they do not establish native event delivery, GPU correctness, every binding,
or the complete production-queue UX. Those checks remain below and in §9.

Still planned for the complete interface: every binding in `docs/03` §2–3
exists and is unique. Selection ordering stable
across repeated band-boxes (`UX-SEL-01`) — a pure function, testable with no
rendering. Click-to-response latency under 100 ms by input injection
(`UX-PERF-02`). The eight player colours passing a deuteranopia and protanopia
simulation (`GD-A11Y-01`).

Chunk 3 adds native-key-handler regressions for WASD/arrow panning across
villager, research-building and mixed selections, release stopping movement,
replacement build/research shortcuts, key-repeat suppression and Escape.
A HUD table test checks that build/research shortcuts are unique even when
combined and never occupy WASD. The reference HUD PNGs are refreshed only
where the displayed shortcut letters change; this does not replace a live
Mac keyboard/GPU pass.

**M6 chunk 1, the shell.** `crates/view/src/shell.rs` unit tests: the
setup's choices become the match's parameters and only a Hardest opponent
gets the declared bonus (`GD-AI-01`); every setup the arrows can reach
passes the engine's check and the arrows stop at its bounds; the title
offers NEW GAME and QUIT and greys the rest saying so; the setup screen
has a step button either side of every setting, a preview at 1280 wide
and none at 640, START greyed when refused, and the bonus line beside the
Hardest opponent; the overlays scale with the HUD and the results list
every side. `crates/app/src/tests.rs`: the game opens on the title,
letters and clicks there reach no match, Enter opens the setup, the
arrows add a Hardest opponent and raise the cap, START gives a match
whose config carries exactly the declared bonus, and the opponents issue
as the AI within two hundred ticks with nothing in the human's name; a
refused setup greys START and Enter will not start it; Escape opens the
pause menu, which pauses, takes every key and click, resumes to the pause
the player had, arms RESIGN on one click and resigns on the second, and
the results say YOU RESIGNED with every side's standing; the last side
falling shows VICTORY, and quitting a live match takes two clicks where a
decided one takes one. Two golden images pin the title and the setup
screen. Six existing app tests met the shell at once: their empty worlds
are decided matches by `docs/02` §10, which the helper now puts away.

### Data files — M3 onward

The startup validator from `docs/04` §9 run as a test: every referenced ID
exists, every unit is trainable somewhere, every tech is reachable, every
tooltip carries cost, build time, counters, countered-by and hotkey. **This is
the main automated defence against missing content**, and it is worth building
the day the first RON file lands rather than the day the first one is wrong.

---

## 6. Requirement traceability

The half of testing that catches *missing features*. A requirement written in a
spec and never implemented produces no failing test, because there is no test —
which is how a milestone gets declared done with a system silently absent.

Every acceptance-bearing statement in `docs/02`, `03`, `04` and `06` carries a
stable ID, written next to the requirement so it is diffed with it. Tests claim
one with a `REQ: <id>` marker. `scripts/check-traceability.sh` pairs them up.

As of this acceptance chunk: **127 declared, 83 claimed by tests, 0 gaps in
landed work.** M4 closure enables `RM-M4`, `GD-COMBAT` and `GD-STANCE`
enforcement. Run the script for current counts. A claim can cover only part
of a requirement: `RM-M4-01` has a separately recorded native readability approval, and
`TA-PATH-06` still owes player-versus-AI priority in M5.

The check fails on three things, each verified by breaking it deliberately: a
landed requirement with no test; a test claiming an ID no document declares;
and a *document* citing an ID no specification declares, since this file alone
cites dozens and they rot silently as requirements are renamed.

Requirements belonging to milestones that have not landed are reported as
**planned**, not failed. A check that is red from day one until M7 gets
disabled in week two. Extending `TRACEABILITY_LANDED` is the moment a
milestone's requirements start being enforced, and that edit belongs in the
milestone's own commit — it is the mechanical definition of "this milestone is
done".

---

## 7. The M1/M2 test backlog

`TRACEABILITY_LANDED` now names M1 and M2 as well as M0, so `TA-PATH`,
`GD-ECON`, `GD-POP` and `RM-M2` are enforced. That pass increased claimed
requirements from 51 to 61; later milestones extended coverage further. `MISSING (landed)` remains zero.

None of the original twelve remain. Requirements inside a landed prefix that
are knowingly not covered go in `DEFERRED` in `scripts/check-traceability.sh`,
each with its blocker; the script fails if one is still listed once a test
starts claiming it, so a closed deferral cannot stay on the list and hide the
next one. The list has been empty since M4 chunk 1 closed `TA-PATH-02`.

### TA-PATH-02: closed by the flow fields, as predicted

The deferral said the number in the spec — a repath "within 3 ticks" — could
not be shipped on M2's pathfinder, because `STALL_TICKS` and the per-order
replan allowance `Nav::replans` were one tuning constant pretending to be two:
the allowance ran out in proportion to journey-time ÷ `STALL_TICKS`, and every
value below 40 stranded villagers in
`sixty_villagers_cross_the_map_without_getting_stuck`.

Flow fields separate the two. Asking the field for a new heading costs a
lookup, not a search, so a walker can re-steer after **3** ticks without
progress toward its heading (`STALL_TICKS` is now 3) and as often as it
needs. Giving up is decided by a different measure: `Nav::no_progress`
counts ticks since the unit last got closer to its *goal*, and only a unit
that has not improved for `GIVE_UP_TICKS` (20 seconds) **and** is still
within two tiles of where it last improved is declared jammed — near enough
counts as arrived, otherwise the trip fails and the order machine re-tasks
the unit. A unit that is moving but not getting closer is on a detour and
keeps going. `Nav::replans` is kept as a diagnostic and no longer limits
anything.

The claiming test, `a_walker_whose_path_is_blocked_repaths_within_three_ticks`
in `behaviour_pathfinding.rs`, walls a villager's corridor while it walks and
asserts a new heading within 3 ticks and arrival after. It needed one more
thing than the timer: a walker on a straight line-of-sight heading never
stalls against a wall that appears in front of it, it just fails to advance,
so `movement` re-checks line of sight whenever the grid's generation has
changed since the heading was chosen.

### What TA-PATH-06 got, and did not

The budget half is tested: a 300-villager crowd ordered across a 200-tile map
on one tick drives `path_deferred` above zero, and every unit is then served
rather than dropped — which is the requirement's actual content. The
**priority** half ("player-issued orders before AI-issued ones") landed with
M5 chunk 1: who issued a command is recorded, the last source to name a unit
is its priority, and `plan_paths` serves the player's requests first.
`behaviour_fog.rs` walls a map so no goal is in a straight line, sends twelve
player-issued and twenty-four AI-issued trips in one tick against a budget of
sixteen, and checks that every player trip has a heading that tick while some
AI trips wait, and that all are served a few ticks on.

---

## 8. Performance

`simrunner bench --json` reports p50/p99/max per tick; `scripts/check-perf.sh`
fails against ceilings in `perf/budgets.ron`, set at three times the observed
p99. Measured spread on a developer machine was 1.1×–1.2×; CI is worse, which
is what the headroom is for.

**The number worth knowing.** `marching-8p` — eight players keeping about 320
mobile units under way, with **no combat and no AI** — spent roughly 6.5 ms
at p99 on M2's per-unit A\*, nearly all of it planning paths, against the
6 ms `docs/04` §12 budgets for pathfinding at 200 population and 400
entities. M4 chunk 1's sector graph and flow fields brought it to about
2.4 ms p99 (p50 from 0.9 ms to 0.6 ms), and `crowded` from 2.9 ms to 1.9 ms,
on the same machine; the ceilings were lowered to match.

Where the time went, and goes, is worth recording because the first flow
field was *slower* than the A\* it replaced (7.2 ms p99):

| Change | `marching-8p` p99 |
|---|---|
| M2 A\* (baseline) | 6.5 ms |
| First flow fields: one field per destination, full re-flood per straggler | 50 ms |
| Incremental flood into added sectors only; one corridor search per group | 8.7 ms |
| Bucket queue instead of a binary heap; stop the flood once every asker is settled | 7.2 ms |
| Dense per-sector cover table instead of a `BTreeMap` in the inner loop | 4.7 ms |
| Per-tile mask (passable, covered) for the box; bucket queue inside sectors | 3.8 ms |
| Portal→tile distance tables built once per sector, so a start costs lookups, not a Dijkstra | **2.4 ms** |

`simrunner bench --stats` prints the diagnostics that found each of these:
fields built and tiles flooded (sum and peak per tick), corridor searches,
whole-map fallbacks, steers, live fields and own-goal fallbacks.

The ceilings are cliff detectors, not precision instruments. Verify §12
properly on known hardware at milestone review; a shared runner cannot answer
that question and should not pretend to.

---

## 9. What stays manual

Automation cannot answer "does this feel like the game you remember", and a
test plan that implies otherwise is lying about the hardest part.

**Per-milestone smoke pass.** A scripted 30-minute run-through with explicit
pass/fail steps drawn from that milestone's acceptance criteria in `docs/06`.

**Feel review.** Against `docs/03` §6 at each milestone, by a person:
acknowledgment latency, camera smoothness, audio texture under twelve
simultaneous workers, the age-up presentation. `docs/02` pillar 4 says the feel
*is* the product; nothing in CI has an opinion about it.

**M7 playtest** (`RM-M7-01`). At least six people who played the original, a
structured observation sheet, and the criterion operationalised: unprompted
session length, and whether they start a second match.

**Real hardware.** The golden images run on a software rasteriser, which proves
the renderer and proves nothing about a GPU driver. `crates/render/tests/headless.rs`
closes part of that gap: it builds every wgpu pipeline on a real device and
renders a frame, on a software Vulkan driver in the Linux job and on Metal on
the macOS runner. What it cannot prove is the window, the swapchain and the
input path, so one pass on real Mac hardware per milestone stays, recording
the macOS version, hardware and display/GPU setup.

---

## 10. Running it locally

```sh
# Everything the lint job runs.
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
scripts/check-sim-purity.sh
scripts/check-art.sh
scripts/check-generated.sh
scripts/check-traceability.sh
scripts/check-cli-callers.sh
scripts/check-workflows.sh

# Everything else.
cargo test --workspace
cargo run --release -p simrunner -- golden
cargo run --release -p simrunner -- battle
cargo run --release -p simrunner --features sim/debug-checks -- balance --matches 10 --dump balance-failures
scripts/check-perf.sh

# Deeper, when changing the simulation.
PROPTEST_CASES=20000 cargo test --release -p sim
cargo run --release -p simrunner --features sim/debug-checks -- soak --matches 1000
cargo test -p sim --features debug-checks

# Deliberate updates, which must be reviewed as diffs.
cargo run -p simrunner -- record          # re-record the corpus inputs
cargo run -p simrunner -- golden --update # re-record the expected digests
cargo run -p simrunner -- matrix --out docs/damage-matrix.md # after a stat change
UPDATE_GOLDEN=1 cargo test -p mapview --test golden_images
```

`record` and `golden --update` are deliberately separate commands. Rewriting
the corpus *inputs* is a much larger claim than rewriting the expected
*outputs*, and the two should never happen in the same commit by accident.

---

## 11. Known gaps

Stated rather than left to be discovered.

- **M4 native acceptance passed for the recorded scenarios.** The 2026-09-13
  owner readability approval and combat/siege results are in `docs/10`; PRs
  #9 and #10 are merged and M4 is landed.
- The original twelve M1/M2 test gaps are closed (§7), and `TA-PATH-06`'s
  priority half with M5 chunk 1.
- **No resource-conservation invariant.** The strongest economy check
  available and it is not written.
- ~~**The HUD overlaps below ~960px.**~~ Fixed in M3: the resource bar
  drops worker counts, then shrinks, then drops the status, then wraps to two
  lines, and `view`'s `the_resource_bar_reflows_instead_of_overlapping` pins
  it at four widths. The `narrow-hud-overlap` golden keeps its name and now
  shows the reflow.
- **No fuzzing.** `cargo-fuzz` targets for the replay reader and the command
  interface were written against M0 and need rebuilding for the current command
  variants, including combat and garrison.
- **No nightly job.** The long soak, deep property runs, Miri over the
  hand-rolled entity store, and the mapgen seed sweep all belong there.
- **GPU/window coverage remains bounded.** The real-device render test may
  skip if no adapter exists. The recorded Mac economy, siege and 40-versus-40
  checks cover those scenarios on one hardware setup; they do not replace
  broader feel review or future hardware checks.
- **The perf gate is coarse**, on purpose — see §8. It will not catch a 20%
  regression.
- **`clippy::indexing_slicing` is not denied** in `crates/sim`. The entity
  store is struct-of-arrays indexed by `Slot`, and `Slot` is only obtainable
  from a live lookup, so the indexing is safe by construction; denying the lint
  would mean replacing that design with bounds-checked accessors everywhere for
  no gain. `World::check` and the soak are the empirical check.

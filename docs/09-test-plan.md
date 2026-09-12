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
| **Test (×3 OS)** | Every test including the corpus and the golden images; determinism runs; a software-rendered frame |
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
handles never resolve; iteration is slot order; and `despawn` scrubs every one
of the thirteen component columns (`TA-ENT-05`).

That last one had no coverage at all before: M2's suite never despawns a
resource-bearing entity, because an exhausted node is already zero. Removing a
column's scrub failed nothing until a test was written that dirties every
column first.

### 4.4 The corpus

Ten recorded matches with a digest over every tick's hash, driving M2's real
systems — gathering and drop-off, construction, training, rally points, group
pathing and separation. A corpus that only moved units around would not notice
a change to the economy, which is most of what the simulation now does.

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

Five scenes rendered through `tools/mapview` and compared against committed
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

### Pathfinding — **M2 shipped, tests owed**

See §7. The system exists; four of its stated behaviours have no test.

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

A damage matrix generated from `docs/02` §8 and committed, covering armour
types, class bonuses, elevation, minimum damage 1 and siege friendly fire
(`GD-COMBAT-01`–`05`). A deterministic 40v40 that terminates. Counters win as
designed over N trials — balance drift is a real regression and headless is the
cheapest place to catch it.

### The AI — M5

Twenty headless AI-vs-AI matches: no panics, no unit idle over 60 s with work
available, Hard beats Easy at least 18 times in 20. `FoggedView` enforced
**mechanically** by a `trybuild` compile-fail test proving the `ai` crate
cannot name `World` (`TA-AI-01`). `docs/07` D7 calls this architectural, and
review is not an architecture.

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

Every binding in `docs/03` §2–3 exists and is unique. Selection ordering stable
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

Today: **127 declared, 51 covered, 0 gaps in landed work.**

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
`GD-ECON`, `GD-POP` and `RM-M2` are enforced. Coverage went from 51 declared
requirements to 61, and `MISSING (landed)` is zero.

**Two** of the original twelve remain, and neither is waiting on a test being
written. They are listed by ID in `DEFERRED` in
`scripts/check-traceability.sh`, each with its blocker, and the script fails if
one is still listed once a test starts claiming it — a closed deferral that
stays on the list hides the next one.

| ID | Blocker |
|---|---|
| `TA-PATH-02` | See below. Needs a design decision, not a test. |

### TA-PATH-02: the number in the spec cannot be shipped as it stands

`docs/04` asks for a repath "within 3 ticks". `STALL_TICKS` is 40. Every value
below 40 strands villagers in M2's own
`sixty_villagers_cross_the_map_without_getting_stuck`:

| `STALL_TICKS` | 40 | 20 | 15 | 10 | 8 | 6 | 5 | 4 | 3 |
|---|---|---|---|---|---|---|---|---|---|
| villagers failing to arrive (of 60) | 0 | 1 | 5 | 5 | 6 | 5 | 5 | 5 | 9 |

The cause is that `Nav::replans` is a per-order allowance of **3**, never
reset. How often a unit exhausts it is journey-time ÷ `STALL_TICKS`, so the
allowance is implicitly calibrated to 40: at 40 a three-minute crossing
affords about 90 stall windows and using 3 is rare; at 3 it affords 1,200 and
using 3 is routine. They are one tuning constant pretending to be two.

The economics argue for tightening it. Measured across the corpus at 3:
`long-run` gathers 3,950 against 3,555 with the same fourteen buildings, and
`economy-8p` gathers 875 against 715, for 0.24% more path searches on
`marching-8p` and no movement in p99 outside run-to-run noise.

Resetting `replans` on progress is **not** the fix, and was tried: it drives
`path_failures` to zero and costs about a third of `long-run`'s gathering,
because the allowance running out is the *give-up* mechanism — `nav_failed`
returns the unit to `Order::Idle`, where the idle counter surfaces it and it
can be re-tasked. A unit that never gives up churns against a contested node
forever. Making both halves of [TA-PATH-02] true needs a replan allowance
scaled to the journey rather than a flat 3.

### What TA-PATH-06 got, and did not

The budget half is tested: a 300-villager crowd ordered across a 200-tile map
on one tick drives `path_deferred` above zero, and every unit is then served
rather than dropped — which is the requirement's actual content. The
**priority** half ("player-issued orders before AI-issued ones") is not
implemented: `plan_paths` iterates slots in index order with no priority
queue, and there is no `ai` crate to issue a competing order. It is owed to
M5, and is recorded here rather than in `DEFERRED` because the requirement as
a whole is now partly covered.

---

## 8. Performance

`simrunner bench --json` reports p50/p99/max per tick; `scripts/check-perf.sh`
fails against ceilings in `perf/budgets.ron`, set at three times the observed
p99. Measured spread on a developer machine was 1.1×–1.2×; CI is worse, which
is what the headroom is for.

**The number worth knowing.** `marching-8p` — eight players keeping about 320
mobile units under way, with **no combat and no AI** — already spends roughly
5 ms at p99, nearly all of it planning paths. `docs/04` §12 budgets 6 ms for
pathfinding at 200 population and 400 entities.

That is not a regression; it is the shape of the work. But the pathfinding
budget is substantially spent before M4 and M5 add combat and an opponent, and
the sector-graph and flow-field layers in `docs/04` §5 are what has to buy it
back. Finding that out at M7 would be much more expensive.

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
scripts/check-perf.sh

# Deeper, when changing the simulation.
PROPTEST_CASES=20000 cargo test --release -p sim
cargo run --release -p simrunner --features sim/debug-checks -- soak --matches 1000
cargo test -p sim --features debug-checks

# Deliberate updates, which must be reviewed as diffs.
cargo run -p simrunner -- record          # re-record the corpus inputs
cargo run -p simrunner -- golden --update # re-record the expected digests
UPDATE_GOLDEN=1 cargo test -p mapview --test golden_images
```

`record` and `golden --update` are deliberately separate commands. Rewriting
the corpus *inputs* is a much larger claim than rewriting the expected
*outputs*, and the two should never happen in the same commit by accident.

---

## 11. Known gaps

Stated rather than left to be discovered.

- **The twelve M1/M2 requirements in §7**, four of them pathfinding behaviours
  on the milestone the roadmap calls the risk.
- **No resource-conservation invariant.** The strongest economy check
  available and it is not written.
- ~~**The HUD overlaps below ~960px.**~~ Fixed in M3: the resource bar
  drops worker counts, then shrinks, then drops the status, then wraps to two
  lines, and `view`'s `the_resource_bar_reflows_instead_of_overlapping` pins
  it at four widths. The `narrow-hud-overlap` golden keeps its name and now
  shows the reflow.
- **No fuzzing.** `cargo-fuzz` targets for the replay reader and the command
  interface were written against M0 and need rebuilding for M2's ten command
  variants.
- **No nightly job.** The long soak, deep property runs, Miri over the
  hand-rolled entity store, and the mapgen seed sweep all belong there.
- **No real-hardware GPU pass.** A software rasteriser is not a driver
  compatibility test.
- **The perf gate is coarse**, on purpose — see §8. It will not catch a 20%
  regression.
- **`clippy::indexing_slicing` is not denied** in `crates/sim`. The entity
  store is struct-of-arrays indexed by `Slot`, and `Slot` is only obtainable
  from a live lookup, so the indexing is safe by construction; denying the lint
  would mean replacing that design with bounds-checked accessors everywhere for
  no gain. `World::check` and the soak are the empirical check.

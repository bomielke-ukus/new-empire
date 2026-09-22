# New Empire

A real-time strategy game about taking a civilization from hand-axes to iron in
about half an hour — built to recapture what made *Age of Empires* (1997)
engaging, without its 1997 frustrations.

**Status: M6 (the game shell) in progress — the game opens on a title
screen; a skirmish is set up against one to seven computer opponents at
four difficulties, played to a results screen, saved and loaded. The
opponents scout, build, advance and attack, seeing only what they have
scouted; every match is recorded and can be watched back; the general
keys, HUD size, edge scrolling and window mode are settings; a stack of
notices in the corner jumps the camera to what happened. M7, the feel
pass, is next. See the roadmap.**

---

## The specification

Read in order:

| Document | What it covers |
|---|---|
| [01 — Research: Age of Empires (1997)](docs/01-research-age-of-empires.md) | What the original did, how it ran, what it got wrong, and the actual mechanisms behind why it was engaging |
| [02 — Game design spec](docs/02-game-design-spec.md) | Pillars, resources, ages, units, buildings, tech, combat, maps, victory, civilizations, AI |
| [03 — UX and feel spec](docs/03-ux-and-feel-spec.md) | Screen layout, selection, commands, camera, feedback, audio design, onboarding |
| [04 — Technical architecture](docs/04-technical-architecture.md) | Rust workspace, deterministic simulation, tick model, entity store, pathfinding, rendering, testing |
| [05 — Art and audio spec](docs/05-art-and-audio-spec.md) | Isometric projection, sprite and animation standards, palette, terrain, UI art, audio inventory |
| [06 — Roadmap](docs/06-roadmap.md) | M0–M9 milestones with demonstrable acceptance criteria |
| [07 — Decisions and open questions](docs/07-decisions-and-open-questions.md) | Decision log with reasoning, and what still needs answering |
| [08 — Art production](docs/08-art-production.md) | Where the art comes from: the inventory cost, the options researched, the legal position, the render-to-sprite pipeline |
| [09 — Test plan](docs/09-test-plan.md) | How we find out whether it is any good before a player does: what each layer catches, what is tested today, and what is still owed |
| [10 — Status and next steps](docs/10-status-and-next-steps.md) | Where the project is, what each milestone established, what M4 builds first, the debt owed, and the decisions waiting |

## Building and running

Requires a stable Rust toolchain (`rustup` installs it; `rust-toolchain.toml`
pins the channel). On Linux the audio device builds against ALSA
(`libasound2-dev`); macOS needs nothing extra.

```sh
cargo test --workspace                       # unit tests for every crate
cargo run --release -p simrunner -- determinism --ticks 10000
                                             # M0 acceptance: run a synthetic
                                             # match twice, compare every tick
cargo run --release -p simrunner -- bench            # per-tick timings
cargo run --release -p simrunner -- bench --stats    # and where each tick went, by phase (docs/04 §12)
cargo run --release -p simrunner -- golden           # replay the corpus, compare digests
cargo run --release -p simrunner -- verify FILE      # a replay or a save: it must replay identically
cargo run --release -p simrunner -- ai --matches 3 --difficulty hard,easy --stats   # computer opponents, headless
cargo run --release -p simrunner -- versus --matches 20 --expect tools/simrunner/tests/versus-hard-easy.golden   # the M5 acceptance
cargo run -p simrunner -- matrix                     # the damage matrix, from the kinds table
cargo run --release -p new-empire [SEED]     # open the game window on a generated map
cargo run --release -p mapview -- --seed 1 --out frame.png --minimap mini.png
                                             # render a frame to PNG with no GPU
scripts/check-sim-purity.sh                  # no floats, no clock, no stray deps in sim
scripts/check-art.sh                         # palette, placeholder regen, atlas gate

cargo run -p atlas -- palette                # player-colour separation report
cargo run -p atlas -- export                 # swatch + .gpl for Aseprite/GIMP
cargo run -p atlas -- placeholder            # generate the placeholder sprite sets
cargo run -p atlas -- validate               # art conformance gate
cargo run -p atlas -- rig                    # render rig, checked against the specs
```

### Testing a build on a Mac

Every push to `main`, and a manual run of the **Mac build** workflow under
the repository's Actions tab, builds `New Empire.app` for Apple Silicon and
attaches it to the run as the artifact `New-Empire-macOS-<commit>`, kept
for thirty days. On the Mac: open the run, download the artifact, and
unzip the `New Empire.zip` inside it: a `New Empire` folder with
`New Empire.app` and the player's `READ ME FIRST.txt`. The bundle
is ad-hoc signed and not notarised, so the first launch is refused. On
macOS 15 and later: open it once, click Done, then in System Settings,
Privacy & Security, scroll to the note that "New Empire" was blocked and
click Open Anyway. On older macOS, Control-click the app and choose Open.
After that it opens normally. Saves, recordings and settings go to
`~/Library/Application Support/new-empire/` as with any other build.
`scripts/bundle-mac.sh` builds the same bundle from a checkout on a Mac,
into `target/bundle/`.

### Playtest builds

A build to hand to playtesters is a GitHub Release, which anyone can
download without an account. Under the Actions tab run **Playtest build**
with a name such as `playtest-1`, or push a tag of that name. The
workflow builds `New Empire.app` on an Apple Silicon runner, zips it in a
folder with `packaging/macos/READ ME FIRST.txt`, and publishes the zip as
`New-Empire-macOS-playtest-1.zip` on the pre-release
`github.com/bomielke-ukus/new-empire/releases/tag/playtest-1`, with the
note as the release text. Send the player that link. The note says how
to get past Gatekeeper, where the recordings are, that nothing leaves
the Mac, and what to send back; `docs/09` §9.1 is the observer's sheet.
Running the workflow again with the same name replaces the zip.

The game opens on a title screen: `Enter` or NEW GAME opens the skirmish
setup (map size, opponents and their difficulties, population cap, seed,
with the map previewed); `Enter` or START begins the match. LOAD GAME
lists the saves and resumes one. In a match, `F5` or SAVE GAME on the
pause menu saves it. Every match played is recorded when it is decided
or left; WATCH REPLAY lists the recordings and plays one back from the
start, with `Space` to pause, `[` `]` for speed up to 16× and `Tab` to
switch whose eyes it is seen through, each player's in turn and then
everyone's. Saves go under `NEW_EMPIRE_SAVES` and recordings under
`NEW_EMPIRE_REPLAYS` if set, else the platform's data directory
(`~/.local/share/new-empire/{saves,replays}`,
`~/Library/Application Support/new-empire/{saves,replays}`,
`%APPDATA%\new-empire\{saves,replays}`).

In the match: edge-scroll, `WASD`/arrows or middle-drag to pan; wheel or
`+`/`-` to zoom, from 0.5× to 3×, about the cursor; click the minimap to
jump; `Space` pause; `[` `]` speed; `F3` toggles edge scrolling; `F2`
cycles the HUD size (1×, 1.5×, 2×); `Home` jumps to your Town Center;
`F4` opens a performance readout: the frame and the tick, the tick's
phases and the budgets they are held to (`docs/04` §12), for measuring on
real hardware. `F1` or `?` opens a controls overlay listing all of this, and the resource
bar points at it for the first minute of a match. WASD is reserved for
camera movement. Every one of these general keys can be rebound on the
title's SETTINGS screen, which also holds the HUD size, edge scrolling, the
window mode and the four sound volumes, kept in `settings.ron` in the game's data directory (or
`NEW_EMPIRE_SETTINGS`); the command letters on the panels are fixed. The
game honours the display's scale factor, so 1× is the same apparent size
on a Retina screen as on any other.

Rendered sprite sets under `assets/sprites` replace the procedural placeholders
for their kinds at startup (today: the greybox villager). `cargo run -p atlas
-- repalette` refreshes their palette chunks after a palette colour changes.

The game is heard from the first click: units answer an order and a
selection, the woodline and the fight sound where they are and only where
you can see, a button clicks, a greyed one buzzes, the bell tolls for an
attack and a fanfare for an age. Twelve villagers chopping are four voices
at once, each at its own pitch. Every sound today is a synthesised
placeholder; a recording under `assets/sounds/<cue>/*.wav` (`ack-villager`,
`work-chop`, `alarm`: the names are in `crates/audio/src/lib.rs`) replaces
it with no code change. A stem plays under the match and cross-fades to
the next age's; drums come in while six or more units fight in view; surf,
wind or birds sit under the camera by the ground it is over. Those are
placeholders too (`stem-stone`, `stem-combat`, `bed-surf`, the names in
`crates/audio/src/score.rs`).

And it is seen: a blow moves what it hits and sparks; a kill throws dust
the way the blow went; a building coming down raises a cloud over its
rubble; a site rises in three stages under the hammers; a bush thins and
a vein shrinks as they are used; a tree falls toward whoever felled it;
and an attack on your own out of view is a red chevron at the screen's
edge and a flash on the minimap.

Hover any unit, building or technology button for its tooltip: cost,
time, what it counters and what counters it, and its key. Five first-time
hints come in context, each at most twice, and SETTINGS turns them off. A
click you cannot afford flashes the resource you are short of.

Play: left-click or drag to select, double-click for all of a kind on screen,
`Shift` adds, `Ctrl`+`0-9` saves a control group and `0-9` recalls it, `.`
cycles idle villagers. Right-click moves, or gathers when over a tree, bush
or vein, or helps build when over your own site. With villagers selected,
`H` places a house, `O` a storehouse, `B` a barracks, `N` an archery range
and `J` a watch tower (`Shift` keeps placing; age/resource gates apply); with the
Town Center selected, `V` trains a villager, `X` unqueues, and right-click
sets its rally point. `T` stops, `Delete` dismisses, `Esc` cancels, or with nothing to cancel
opens the pause menu (resume, save, resign, quit to title).
Research buttons use `Q`, `E`, `I`, `K`, then `Z` in displayed order;
the command grid shows the current key for each available technology.
Completed training waits in its paid queue slot if the entity cap blocks
spawning, and can still be cancelled for a refund.
Commands take effect two ticks (100 ms) after you give them — that delay is
the lockstep window, and it is why multiplayer will be a transport job.

Workspace layout:

| Path | What |
|---|---|
| `crates/sim` | Deterministic simulation: fixed-point maths, RNG, entity store, command queue, replay, tile map, map generation |
| `crates/fogged` | One player's view of a match, and nothing else: the interface a computer opponent gets |
| `crates/ai` | Computer opponents: they issue the same commands a player can and read only a `FoggedView`; depends on `fogged`, never on `sim` |
| `crates/audio` | Sound as the game asks for it: buses, cues, voice limiting, positional gain, the events that drive them; no device |
| `crates/view` | Presentation maths: projection, camera, palette, placeholder atlas, terrain mesh, scene, minimap, software rasteriser |
| `crates/render` | The wgpu renderer: terrain, palette-indexed sprites, minimap |
| `crates/app` | The game binary: window, GPU surface, input, fixed-timestep clock |
| `tools/simrunner` | Headless runner for determinism checks, replay verification and benchmarks |
| `tools/mapview` | Renders generated maps to PNG through the software rasteriser |
| `tools/atlas` | Art gate: bakes the palette, validates sprite sets, generates placeholders, quantises and composes renders |
| `tools/render` | Blender scripts for the frozen camera and light rig, and the render driver |
| `tools/gen` | Generators for committed tables (trig) |
| `scripts` | CI checks, and the Mac bundle |
| `packaging/macos` | The `Info.plist` template `scripts/bundle-mac.sh` fills in, and the player's `READ ME FIRST.txt` |
| `assets/palette` | The 256-colour indexed palette, with the reserved player-colour ramp |
| `assets/render` | The frozen render rig every sprite is rendered through |
| `assets/sprites` | Rendered art (needs Blender, so committed rather than regenerated) |

## Design pillars

1. **You can see your empire advance** — progress is expressed in the world, not in a progress bar.
2. **The map is a finite, shared board** — resources deplete and never return; economic pressure creates conflict.
3. **Commands are cheap; micromanagement is optional** — a relaxed player and a fast player both get a good match.
4. **Everything is legible, and everything makes a sound.**
5. **The simulation is deterministic** — replays, saves and future multiplayer all follow from this.

## Shape of the build

- **Rust**, native **macOS** desktop, `wgpu` + `winit` + `kira`.
  macOS is the product target; the Linux CI jobs are the fast gate and
  the second leg of the determinism check, without a shipping commitment.
  Windows is not built (dropped from CI 2026-09-21).
  The same stack compiles to WebAssembly for quick playtest builds.
- **Deterministic lockstep simulation** at 20 Hz, fixed-point maths, seeded RNG,
  commands scheduled two ticks ahead — the architecture from *"1500 Archers on a
  28.8"*, which is what let the original run 1,500 units over a modem.
- **2D isometric sprites**, 64×32 tiles, 8 facings authored as 5 and mirrored,
  palette-indexed with a reserved player-colour ramp — Genie engine technique,
  our own art. Modelled and rendered rather than drawn, which is how the
  original's sprites were made too (`docs/08`).

## First playable target (M7)

One random map type, two civilizations, Stone → Tool → Bronze, ~12 unit types,
skirmish against an AI opponent, with the full audio and animation pass. A
complete 25-minute match, start to victory screen.

## Assets and legal position

No Age of Empires assets, data files or code are used anywhere in this project.
All art, audio and balance data are original. Shipped art is rendered from
geometry we build; generative tools are used upstream of that, for concept and
texture work, and never prompted with the name of a game, studio or franchise
(`docs/08` §5). The techniques documented in
`docs/01` (isometric tiling, palette-indexed player colours, sprite mirroring,
deterministic lockstep networking) are published engineering practice and are
what we are building on.

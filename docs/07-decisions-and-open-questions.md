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

**Closed 2026-09-22.** Hunting landed with the playtest-2 fixes
(`docs/02` `GD-ECON-06`): the gazelle runs when hit, its carcass lies
three minutes, the hunter gathers it unasked. The same day the clock was
made to run only while nobody gathers the carcass ("decays if left"), and
the opponent was taught to hunt and to repair (`GD-AI-02`).

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

### D20 — Static tables stay in Rust until the roster stops moving
**Date:** 2026-09-11

`docs/04` §9 plans a `data` crate that loads unit, building and technology
tables from RON at startup, behind a validator. M3 added the technology table
and ten buildings as Rust constants in `sim::tech` and `sim::kinds` instead.
While the roster is still being invented, a table the compiler checks and
`cargo test` exercises is worth more than one a validator checks at startup;
files pay off when someone who does not build the game needs to edit them,
which is the balance pass at the earliest. The shapes are the ones the RON
will take (`KindInfo`, `TechInfo`, `Effect`), so the move is mechanical when
it comes.

### D21 — Gates shut on proximity; garrison arms towers and the Town Center
**Date:** 2026-09-12

`docs/02` §6 says a gate lets allies pass and enemies not. The cheap way to
honour that with one navigation grid and one set of flow fields is to make
the gate's tile a blocker only while an enemy unit is within two tiles
(open again once none is within three). Per-player passability would have
multiplied the fields by the players and doubled the sector graph; this
costs a distance check per gate per tick. The owner's units are also shut
out while an enemy stands at the gate, which is how later games behave.

Garrison (`UX-CMD-09`) does two things: units inside cannot be hit, and
each adds an arrow to the building's volley. The Watch Tower fires one of
its own; the Town Center fires nothing until someone is inside, so a
villager who runs home under attack makes the Town Center shoot back. Units
come out when told to or when the building falls, unhurt. Capacities: Town
Center fifteen, Watch Tower five. Rubble lies for sixty seconds with the
footprint open from the first tick, so a breach is a breach.

### D22 — The Government Centre stays; a second Town Center needs one
**Date:** 2026-09-18

Q3 is answered: the Government Centre keeps its own building, placeholder and
Bronze Age price. It is a real investment and a real target, and folding its
upgrades into the Town Center would save less than it costs in legibility.
With that settled the Town Center goes onto the villager's build panel, and
as in the original a second one needs a finished Government Centre standing,
which gives that building a purpose before its civic technologies arrive.

### D23 — Fog is recomputed every tick, not kept incrementally
**Date:** 2026-09-18

`docs/04` §6 planned incremental vision: decrement the circle a unit
leaves, increment the one it enters. That needs per-entity bookkeeping of
the last stamped tile and radius, scrubbed on death, garrison and removal,
and it goes wrong quietly. Clearing every count and stamping every seeing
entity afresh costs the sum of the sight discs, well under the 1 ms §8
budgeted, and has no state to corrupt. The explored bitset and the
memories are the only fog that is hashed. If a profile ever shows the
stamp pass, the incremental scheme is the optimisation, behind the same
`Fog` interface.

### D24 — A memory carries the handle of what was seen
**Date:** 2026-09-18

A player can right-click a tree or a building drawn from memory and the
order goes out; the simulation acts on it if the thing is still there and
ignores it if not. `Fog`'s memories therefore keep the `EntityId` of what
was seen, alongside its kind, owner, age and whether it was a site, and
`FoggedView::remembered` hands it to the opponent. This is not a leak: the
handle was learned by looking, and a stale one does nothing. What the
handle cannot tell is whether the thing is still there, so the opponent
walks to a memory before gathering from it.

### D25 — At a time limit the higher score wins, and the score is what was gathered plus what stands
**Date:** 2026-09-18

`docs/02` §10 says a match with a time limit goes to the highest score
and does not say what the score is. It is now: everything a side has
gathered, plus the cost of every unit and finished building it has
standing. Gathering counts once; what was built with it counts again for
as long as it stands, so a side that lost its houses to a raid is behind
one that kept them. Kills are not counted, because the simulation does
not record who dealt a blow and the presentation should not have to. The
M5 acceptance run decides its matches this way when thirty minutes end
with both Town Centers up, which in the first twenty is every match; the
same rule will decide a skirmish with a time limit in M6. A draw is
possible and is recorded as one.

The Hardest difficulty's declared advantage is a 25% gather bonus, set on
the match by whoever sets it up (`SimConfig::gather_bonus_pct`) and shown
in M6's setup screen, never taken by the opponent's code.

### D26 — Attack-move is `A`; the arrows pan by default
**Date:** 2026-09-22 · **Decided by:** Bo

`docs/03` gave `A` to attack-move (`UX-CMD-02`) and to WASD panning at
once, and the build had settled it by putting attack-move on `M`. The
owner chose the spec's key: the arrow keys, edge-scroll and middle-drag
pan out of the box, as the original did; `A` is attack-move; WASD stays a
binding away on the settings screen, and a player who binds it gives up
`A` for attack-move, since the general keys win over the panel's
letters. The alternative, `A` meaning attack-move only with soldiers
selected and panning otherwise, was a modal key and was turned down.

### D27 — The game is called *Brenden's Empires*
**Date:** 2026-09-25 · **Decided by:** the owner

Answers Q5. The owner's first choice put *Age of Empires* in the name; that
is Microsoft's trademark, the playtest builds are public downloads, and the
project's position (`docs/08` §5.3) is an original game in that spirit, so
the owner kept their name and dropped the trademarked words. Everything a
player reads says *Brenden's Empires*: the title screen, the window, the
app and its menu bar, the zip, the release and its note. The repository,
the crates, the binary, the environment variables and the data folder keep
the code name `new-empire`, so a player's settings, saves and recordings
stay where they are. The pixel font gained an apostrophe for it.

### D28 — The ground is blended colour with a rendered grain, not tile sets
**Date:** 2026-10-03 · **Decided by:** the owner

`docs/05` §3 asks for four tile variants per ground type and alpha-mask
transitions between types. Of a grain over today's ground, full tile sets
with transitions, or leaving the ground for later, the owner chose the
grain. The ground keeps its per-vertex colours, which blend between types
at every corner and shade the slopes; over them each tile carries its
type's grain, a greyscale layer rendered from a modelled patch (blades,
pebbles, cracked hardpan, ripples, leaf litter, drifts) and multiplied in
by the terrain shader and the rasteriser alike (`view::detail`). One
layer per type repeats on every tile, softened by the per-tile colour
variation; there are no variants and no transition tiles. Tile sets can
still replace it: the sheet is one file, and the colours under it stay.

### D29 — The ages restyle the buildings and dress the figures; they do not remodel them
**Date:** 2026-10-04 · **Decided by:** the owner

`docs/05` §2.5 asks for "a visibly different structure" in each age, not
a recolour, and walks the materials from thatch and timber to monumental
stone. The rendered Stone Age buildings were already mudbrick under
thatch. The owner approved the proposal to age them by material and trim
rather than by new models: each building keeps its shape through the four
ages; its roofs go from thatch to shingle, terracotta and slate, its walls
from mudbrick (with a timber frame in the Tool Age) to plaster and to
dressed stone, and it gains a stone base course, then cornices and
pilasters (`kit.style_building`). The villager and the infantry keep their
bodies and change their dress, mostly on the head (`kit.age_dress`). It
costs one re-render per age, not a model per age, and a modeller can
still replace any age's set by name (`house_bronze`). The roof colour is
what reads at the camera's height, so it is the one change every
building makes.

### D30 — The Legionary ends the Hoplite line; a siege stone lands where it was aimed
**Date:** 2026-10-04 · **Decided by:** Claude, on the owner's instruction to build the rest of M8

Two things `docs/02` left open or said two ways. §7 gives the line
upgrades as Clubman → Axeman → Swordsman → Legionary, while §5.2's table,
which carries the numbers, makes the Legionary the Hoplite line's last
tier and gives the Swordsman a cost of its own. The table wins: the Axe
upgrades the Clubman, the Legion upgrade the Hoplite (at the Academy, in
the Iron Age), and Torsion the Stone Thrower into the Catapult (at the
Siege Workshop); the Swordsman is the Barracks' Bronze Age unit, trained
beside the Axeman. Second, `GD-COMBAT-04` turns friendly fire on but does
not say how a stone flies. It flies to where its target stood when it was
thrown and comes down there, so a moving target can step out of the way,
and hurts everything within its blast, friend or foe: half a tile for the
Stone Thrower, a tile for the Catapult. The engine that threw it is never
hit by its own stone; there is no minimum range. The Ballista's bolt is an
arrow: it follows its target and has no blast.

### D31 — Relics are carried by priests, held in Temples, and earn gold
**Date:** 2026-10-04 · **Decided by:** Claude, on the owner's instruction to build the rest of M8; Q2's recommendation

Q2 asked whether relics stand still where they lie, as the original's
ruins did, or are carried. They are carried: five on a generated map, in
the open ground between the starts and apart from one another. Only a
priest can take one up; carried to a Temple of its side's, it earns that
side a gold every two seconds for as long as the Temple stands. A priest
that falls drops its relic where it fell, a Temple that falls drops all it
held round its rubble, and a priest converted with a relic in hand brings
it over. Holding every relic on the map in one side's Temples for ten
minutes wins (`GD-WIN-03`). It gives priests a second job and puts five
places on the map worth fighting over. A relic on the ground is nature's
and blocks its tile like a bush; it cannot be attacked.

### D32 — The civilizations' bonuses and denials, on the game as it is
**Date:** 2026-10-04 · **Decided by:** Claude, on the owner's instruction to build the rest of M8

`docs/02` §11's table names things the game does not have: ships, guard
towers, a priest's second tier, cavalry upgrades. A bonus or denial that
names one of them waits for it; the rest are read onto what exists:

| Civ | Bonuses as built | Denied as built |
|---|---|---|
| Egyptians | Gold +20%; Chariot Archers +33% HP; priests convert from 2 tiles further | the Academy, Hoplite and Legionary (the Legion), Heavy Cavalry |
| Greeks | Hoplites and Legionaries +25% speed; the Legion upgrade from the Bronze Age (the spec's "hoplites in Bronze" is every side's already); ships +30% speed (since D33) | Chariot Archer, Horse Archer |
| Assyrians | Villagers +10% speed; Slingers, Bowmen, Chariot and Horse Archers strike 20% more often | the Legion upgrade ("heavy infantry upgrades") |
| Babylonians | Walls, the gate and towers +60% HP; stone +20% | Heavy Cavalry ("cavalry upgrades"); Torsion and the Catapult ("siege workshop tier 2") |
| Persians | Hunting +30%; War Elephants +50% speed | Ballista; guard towers wait for them |
| Phoenicians | Wood +30%; War Elephants cost 25% less | Stone Wall; priests' tier 2 waits for it |
| Shang | Villagers cost 30% less; walls and the gate +100% HP | War Elephant; Torsion and the Catapult ("siege upgrades") |
| Sumerians | Farms hold twice the food; siege engines strike 50% more often | War Elephant and Horse Archer ("cavalry beyond Bronze") |

Every bonus is a number on what a side already has; nothing about how a
unit plays changes. A match names a civilization per side in its
configuration; one that names none (every test and corpus match before
this) plays as before and hashes as before. On the setup screen the
player picks theirs and the opponents' are dealt from the seed. The four
architecture sets are named per civilization; D34 draws them.

### D33 — Boats move on a grid of their own; the Dock stands in the water at the shore
**Date:** 2026-10-04 · **Decided by:** Claude, on the owner's instruction to build naval next; Q1's recommendation (hold naval until M8)

Q1 named the cost: water doubles the pathfinding surface. It is paid
once, as a second `NavGrid` where the water is open and the land
blocked, with its own sector graph and flow fields; every place a unit
asks the grid (planning, walking, standing, approaching, spreading a
group, stepping out of a building) asks the grid of its element. A group
of boats and walkers ordered somewhere parts and goes as two. Shallow
water stays closed to walkers, as it was: fords are land, and a boat
can sail anywhere wet. The water grid is derived from the map and the
Docks and fish standing in it, so it is not hashed and a match without
boats hashes as before; a save from before boats reads it empty and it
is rebuilt.

The Dock is three tiles square, every tile of it open water, with open
land beside it to be built from and open water beside it for its boats;
anywhere else it is refused ("goes in the water by the shore"). It is
the water's drop-off: a fishing boat's catch goes there and nothing from
the land does, so the land keeps its one drop-off type (D5). Every
letter is a key already, so the Dock is placed by its button, like the
Town Center and the Wonder.

Ships are a class of their own for the counter system. No technology
names them yet, so the per-class technology tables keep their eight
entries and a ship's is none; old saves and hashes keep their shape.

The warships the spec leaves unnamed are three, one an age from the
Tool Age (the owner agreed): the Archer Ship, the War Galley and the
Catapult Ship, trained at the Dock. They fight across the shore with
what reaches over the water: a melee unit never takes a unit of the
other element as a target, given the order or not, and a unit sent
after one across the shore that has gone as near as its element allows
and is still out of reach gives up (`GD-NAVAL-02`). A building is hit
from beside it, so hand weapons can still burn a Dock. The Greeks'
ships are 30% faster.

A transport is a garrison that moves. A unit told to board walks to the
land nearest the boat and waits there, the boat comes in to the water
nearest the party, and each steps aboard in reach; aboard, it rides
where the boat goes. Told to unload somewhere, the boat sails to the
water nearest the place and puts everyone ashore on the land beside it.
At sea, nobody steps off; a transport that sinks, or is scuttled, takes
everyone aboard with it; one converted brings them over. A trade boat
loads 20 wood at a Dock of its side's, sells it at another side's Dock
(an enemy's will do: there are no alliances yet) for 10 gold and
three quarters of a gold a tile from its side's nearest Dock, brings the
gold home and goes again, waiting at home while there is no wood.

Islands puts the starts on a ring two fifths of the map out with
little jitter and gives each the land within half the distance to its
nearest neighbour, then cuts a channel of deep water along the line
halfway between every two, no nearer a start than twelve tiles; a map
where any start can still walk to another is thrown away and the next
seed tried. Relics go on any island.

### D34 — The first architecture set is the Greek; the other three restyle it
**Date:** 2026-10-05 · **Decided by:** Claude, on the owner's instruction to make the other three building styles

`docs/02` §11 gives eight civilizations four architecture sets, and D32
named each civilization's. The set drawn first, mudbrick and thatch
turning to plaster, terracotta, dressed stone and slate, with porticoes
and a podium Wonder under a columned shrine, is the **Greek**: the
plainly named sets (`house`, `house_bronze`) are the Greek, and a
civilization without a set of its own (and every match that names no
civilization) draws them. The other three are the same buildings, the
same footprints and the same stages, restyled after they are built, as
the ages restyle the Greek (D29), so that a building is the same shape
to the eye in every architecture and a new kind costs one model, not
four:

- **Egyptian**: mudbrick, then whitewash, sandstone, and white
  limestone in the Iron Age; flat roofs behind a low parapet under a
  flared cavetto cornice, painted bands of blue and red ochre from the
  Tool Age, battered wall feet from the Bronze, a wind-catcher on a roof
  big enough to live on, papyrus capitals on columns. Its Wonder is a
  pyramid cased in white limestone with a gilt capstone and two
  obelisks.
- **Mesopotamian**: mudbrick, then baked brick from the Bronze Age;
  flat roofs crenellated, the merlons stepped from the Bronze; buttressed
  walls; a band of blue glaze from the Bronze and walls glazed to the
  top with gold rosettes in the Iron. Its Wonder is a three-tiered
  ziggurat with a blue-glazed shrine.
- **East Asian**: rammed earth on an earthen podium, then white plaster
  on a stone one from the Bronze Age; hipped roofs with turned-up
  corners, thatch and then dark tile with a ridge and, in the Iron Age,
  gilt horns; posts of dark timber, lacquered red from the Bronze, with
  brackets under the eaves. Its Wonder is a hall under two tiers of roof
  on two terraces, with gate towers by its stair.

What stood on a roof (a flag) is set on the new roof. The Dock, the
farm, the walls and the gate are everyone's. The sets are named for
their architecture before their age (`house_egyptian_bronze`); a
building draws its owner's architecture in the latest age that
architecture has a set for, and the Greek set where it has none.

The four sets do not fit one 8192 × 8192 atlas, the widest texture the
GPU's default limits allow: the atlas is a texture array, a page a
layer, and each frame says which page it is on.

### D35 — M9 is campaigns and the editor; multiplayer becomes M10
**Date:** 2026-10-05 · **Decided by:** the owner

M9 (`docs/06`) as planned held five things: the campaign system, the
learning campaign, two historical campaigns, the scenario editor and
multiplayer. The owner will play against the computer first, so
**multiplayer leaves M9** for a milestone of its own after it (M10); the
architecture it needs (D3: command turns, per-tick state hashes) is in
place and waits. M9 is built in this order: the campaign system, the
learning campaign, the scenario editor, then the two historical
campaigns.

- **The fiction is history** (Q4 answered as recommended), told by one
  narrator, **in text**: briefings, objectives and messages on screen,
  no recorded or generated voice (which also keeps Steam's disclosure
  rule for generated voice out of it, `docs/08` §5.2).
- **The learning campaign** follows early Egypt, one idea a scenario as
  `docs/03` §7 sets out: gathering and building, advancing an age,
  combat, counters.
- **The historical campaigns** are **the Persian Wars**, played as the
  Greeks, and **Sargon of Akkad**, played as the Mesopotamian kingdom he
  founded. Neither borrows a shipped game's campaign: the history is the
  source (D27, `docs/08` §5.3).

### D36 — Soldiers answer what hits them; a first Town Center is free; the Spearman's bonus is +12
**Date:** 2026-10-05 · **Decided by:** the owner

Three questions M9's campaigns raised (`docs/10` §6), answered:

- **A soldier answers a unit that hits it, seen or not** (`GD-STANCE-03`).
  Until now an Aggressive or Defensive unit only picked fights in its own
  sight, so hoplites (sight 4) stood and were shot by bowmen (range 5).
  Now the unit hit turns on its attacker if it is free to (idle,
  attack-moving or patrolling), follows no further than its stance's
  leash or two tiles past where the shooter stood, and walks back. Stand
  ground and Passive units do not answer. Towers are not answered: walking
  under one to hack at stone is an order for a player to give, as
  `GD-STANCE-01`'s acquisition takes only units on its own.
- **A first Town Center needs no Government Centre**, as D22 always said:
  only a second one does. A foundation counts as the first, so two cannot
  be laid at once to get round it. A player whose town is razed, or a
  scenario that starts without one, may raise one again; the computer
  opponents, which already rebuild a lost Town Center when they may, now
  always may.
- **The Spearman's bonus against cavalry is +12**, up from +6: one
  Spearman beats one Light Cavalry, six hits to the rider's seven, where
  it lost nine to seven. The counter now holds one on one, not only in
  numbers (`GD-COMBAT-02`).

All three change how fights and games go, so the replay corpus, the
Hard-against-Easy record (RM-M5-01) and the golden images are recorded
again in the same change, and the campaigns are played through again
by their tests.

### D37 — Each player has a symbol as well as a colour
**Date:** 2026-10-08 · **Decided by:** the owner, as recommended

Q9 is answered with its cheapest option, a per-owner shape (`GD-A11Y-03`):
a circle, a square, a triangle, a diamond, a plus, an X, a triangle
upside down and a star, in player order, so a 1v1 is a blue circle
against a red square. Each is 9 pixels square, in its player's colour
with a lit top edge and a black outline, and needs no change to the art.

- **Where**: over what is selected, by default; over every unit and
  building of every side with the settings screen's SYMBOLS at ALWAYS
  (walls and gates aside, which would bury a wall in symbols), or
  nowhere with OFF; and in place of the plain colour square on the setup
  and results screens. Over the world it draws above everything, so a
  nearer unit never hides it.
- **Not on the minimap**, where a unit is a dot a pixel or two wide.
- The high-contrast palette, the third option, stays open if a
  colour-blind player finds the symbols not enough.

## Open questions

### Q1 — Naval in the vertical slice, or after? — **answered, see D33**
Water doubles the pathfinding surface (separate navigation domain, transports,
shore-landing edge cases) for one map type. **Recommendation:** hold until M8.

### Q2 — Relics: static (AoE1 ruins) or carryable (AoE2)? — **answered, see D31**
Carryable relics create better fights over specific objects; static ruins are
simpler and match the original. **Recommendation:** carryable, held in the
Temple, generating gold — it gives priests a second job and creates map tension.

### Q3 — Does the Government Centre earn its own building? — **answered, see D22**
Its upgrades could fold into the Town Center, saving a building and a data
table. Counter-argument: a separate building is a real strategic investment and
a target. **Decided 2026-09-18: it stays its own building.**

### Q4 — Campaign fiction: written by us, or straight history? — **answered, see D35**
Straight history is free, accurate and evocative. Original fiction gives us
narrative control. **Recommendation:** history, told through a single narrator,
in the style of the original's campaign intros.

### Q5 — What is the game actually called? — **answered, see D27**
"New Empire" is the repository name and a placeholder. Worth deciding before
there is a main menu (M6). **Biting as of M6 chunk 1 (2026-09-19):** the
title screen shows the placeholder, `view::shell::TITLE`, one constant to
change.

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

**As of M7 chunk 1 (2026-09-20):** the game is heard before the decision
is taken. Every cue in the `docs/05` §5.1 inventory that the game can
raise today has a synthesised placeholder (`crates/audio/src/placeholder.rs`):
tones and noise bursts of the right shape and length, honest about what
they are, the way the placeholder sprites stand in for art. A recording
under `assets/sounds/<cue>/*.wav` replaces one by name with no code
change, so the commissioned cues and the licensed foley drop in when they
exist. The music stems and the ambient beds followed in chunk 2
(2026-09-21), placeholders too: a pentatonic sketch per age, drums for a
fight, noise for the surf, the wind and the birds. The recommendation
above stands and still needs taking; the nine cues it names are exactly
the loops under `stem-<age>/`, `stem-combat/` and the fanfares.

### Q8 — Do we want a hard 4-age structure, or a 5th age?
The original's four ages map cleanly onto ancient history and end at a natural
place. A fifth (Classical/Imperial) would extend matches past 40 minutes.
**Recommendation:** stay at four. Long matches were not the appeal.

### Q9 — What is the second ownership cue, besides colour? — **answered, see D37**
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

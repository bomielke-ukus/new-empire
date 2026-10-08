# Game Design Spec — *Brenden's Empires*

A real-time strategy game about taking a civilization from hand-axes to iron, in
about half an hour, with the pacing and texture of *Age of Empires* (1997) and
none of its 1997 frustrations.

Read `docs/01-research-age-of-empires.md` first — this document assumes it.

---

## 1. Design pillars

Five statements. Every feature decision gets checked against them, and anything
that serves none of them is cut.

1. **You can see your empire advance.** Progress is expressed in the world —
   buildings and units visibly change with each age — not in a progress bar.
2. **The map is a finite, shared board.** Resources deplete and never return.
   Economic pressure produces geography, geography produces conflict.
3. **Commands are cheap; micromanagement is optional.** A relaxed player and a
   fast player should both be able to enjoy a full match.
4. **Everything is legible, and everything makes a sound.** If a thing happens,
   you can see it, hear it, and tell what it was without reading text.
5. **The simulation is deterministic.** Replays, saves, and future multiplayer
   all fall out of this, and it keeps us honest about state.

**Explicit non-goals for v1:** competitive esports balance, a 3D engine,
procedurally generated civilizations, base-building on water, mod-script support.

---

## 2. The session we are trying to produce

A 1v1 skirmish, normal speed, should feel like this:

| Time | What the player is doing | What they're feeling |
|---|---|---|
| 0:00–2:00 | Three villagers on berries, one scouting, house down | Small, fragile, curious |
| 2:00–6:00 | 8–12 villagers, splitting onto wood and food, finding the enemy | Getting ahead, planning |
| 6:00–9:00 | Age up to Tool. Barracks. First raid or first tower | First real tension |
| 9:00–15:00 | Bronze Age. Real army, tech choices, resource lines running dry near home | Committed, stretched |
| 15:00–25:00 | Expansion fights over remaining gold. Siege, priests, a Wonder going up | Decisive, spectacular |
| 25:00+ | Conquest or Wonder timer | Resolution |

If a build does not produce roughly this arc, the tuning is wrong regardless of
what the spreadsheets say.

---

## 3. Resources and the economy

### 3.1 The four resources

| Resource | Sources | Role |
|---|---|---|
| **Food** | Foraging, hunting, farming, fishing | Population growth and age-ups |
| **Wood** | Trees | Buildings, boats, archers |
| **Stone** | Stone veins | Fortification, towers, Wonder |
| **Gold** | Gold veins, trade, tribute | Elite units and late technology |

**[GD-ECON-01] Nothing regenerates.** Trees are removed when felled. Veins mine out. Hunted
animals do not respawn. Farms are the only renewable food, and they cost wood
each time they are re-seeded — a deliberate wood-to-food conversion that keeps
late-game economies dependent on a shrinking forest.

### 3.2 Yields (initial tuning values)

| Node | Yield | Notes |
|---|---|---|
| Tree | 75 wood | Removed from map when exhausted |
| Berry bush | 150 food | Cluster of 5–7 near most starts |
| Gazelle / deer | 200 food | Must be killed first; decays if left; four within about seven tiles of every start |
| Boar / elephant | 400 food | Fights back; a genuine early decision |
| Fish (shallow) | 200 food | Reachable by villagers on shore |
| Fish (deep) | 350 food | Requires a Fishing Boat |
| Gold vein | 400 gold | 4–7 tiles per deposit |
| Stone vein | 350 stone | 3–5 tiles per deposit |
| Farm | 250 food | Re-seed costs 60 wood; auto-reseed toggle on by default |

### 3.3 Gathering

- **[GD-ECON-02]** Base gather rate **0.45 resources/second**, carry capacity **10**, then walk to
  the nearest valid drop-off and deposit. Meat off a carcass comes at **0.75/second**, so the
  herd near home is the fastest food of the first minutes; food bonuses apply to it too.
- **[GD-ECON-03] One drop-off building type — the Storehouse** — accepting all resources. The
  original's Granary/Storage Pit split was bookkeeping, not depth. The Town
  Center also accepts everything.
- **[GD-ECON-04]** Distance to drop-off is the real economic skill: placement matters, walking
  time is the cost.
- **[GD-ECON-05] Farms auto-reseed by default** (toggle per-farm and globally), and a
  notification fires when wood is too low to reseed.
- **[GD-ECON-06] Hunting.** An animal is food only once killed: villagers or soldiers attack
  it, it bolts two tiles when hit, and the carcass lies where it fell with its yield on it, gatherable
  by any villager until it is taken. It decays if left: after three minutes with nobody
  gathering it, it is gone, and the clock stands still while someone is. A villager that
  makes the kill gathers the carcass without being told. Villagers hunt: +2 attack against
  animals, so two hits take a gazelle.

### 3.4 Population

- **[GD-POP-01] House: +5 population, 30 wood.** Town Center provides 5.
- **[GD-POP-02] Default cap 75**, configurable 50–200 in skirmish setup.
- **[GD-POP-03]** Villager costs **50 food**. Military costs vary; every unit costs 1 pop except
  siege (2 pop) and elephants (2 pop).

A low cap is intentional. It keeps individual units meaningful, keeps battles
readable at our sprite scale, and keeps the sim cheap.

---

## 4. Ages

| Age | Cost | Also requires | Research time |
|---|---|---|---|
| **Stone** | — | Starting age | — |
| **Tool** | 400 food | 2 Stone Age buildings (excl. Town Center, House) | 60 s |
| **Bronze** | 800 food, 200 wood | 2 Tool Age buildings | 90 s |
| **Iron** | 1200 food, 500 gold | 2 Bronze Age buildings | 120 s |

**[GD-AGE-01]** The building requirement is doing real work: it stops a hoarding player from
skipping development, and it forces you to commit to a direction before you
advance.

**[GD-AGE-02] Age-up presentation** (pillar 1) — when an age completes:

- A short fanfare, distinct per age, ducking other audio.
- A sweep of light across the settlement, building by building, as each
  structure's sprite swaps to its new-age variant.
- The command panel visibly gains its new buttons.
- Villager and infantry sprites swap to their new-age costume.

This moment is the emotional payload of the game. It gets a dedicated
implementation task, not "swap the texture".

---

## 5. Units

Slice units marked **[V1]**. Stats are opening values for tuning, not gospel.

### 5.1 Economic and support

| Unit | Age | Cost | HP | Notes |
|---|---|---|---|---|
| **Villager** [V1] | Stone | 50F | 25 | Gathers, builds, repairs, fights badly (3 dmg) |
| **Scout** [V1] | Stone | 60F | 45 | Fast, wide line of sight, weak attack |
| **Fishing Boat** | Stone | 50W | 45 | Gathers deep fish |
| **Trade Boat** | Bronze | 100W | 100 | Converts wood to gold at a foreign dock |
| **Transport Boat** | Tool | 75W | 150 | Carries 10 pop |
| **Priest** | Bronze | 125G | 25 | Heals; converts enemy units; no attack |

### 5.2 Infantry

| Unit | Age | Cost | HP | Attack | Armour | Notes |
|---|---|---|---|---|---|---|
| **Clubman** [V1] | Stone | 50F | 40 | 3 melee | 0/0 | The first thing you can build |
| **Axeman** [V1] | Tool | 50F 20W | 50 | 5 melee | 0/0 | Clubman upgrade |
| **Spearman** [V1] | Tool | 40F 20W | 45 | 4 melee | 0/1 | +12 vs cavalry & elephants |
| **Swordsman** | Bronze | 45F 25G | 70 | 8 melee | 1/1 | Line infantry |
| **Hoplite** | Bronze | 60F 40G | 120 | 12 melee | 4/2 | Slow, brutal, from the Academy |
| **Legionary** | Iron | 60F 40G | 160 | 16 melee | 5/3 | Hoplite line, final tier |

### 5.3 Ranged

| Unit | Age | Cost | HP | Attack | Range | Notes |
|---|---|---|---|---|---|---|
| **Slinger** [V1] | Tool | 40F 10S | 40 | 4 pierce | 4 | +4 vs infantry; cheap counter |
| **Bowman** [V1] | Tool | 40F 20W | 40 | 5 pierce | 5 | Backbone ranged unit |
| **Chariot Archer** | Bronze | 40W 60G | 70 | 6 pierce | 5 | Fast, high HP, no armour |
| **Horse Archer** | Iron | 50W 70G | 60 | 7 pierce | 6 | Raiding unit |

### 5.4 Mounted and siege

| Unit | Age | Cost | HP | Attack | Notes |
|---|---|---|---|---|---|
| **Light Cavalry** [V1] | Tool | 60F 20G | 90 | 7 melee | Fast; raids villagers |
| **Heavy Cavalry** | Bronze | 70F 40G | 150 | 12 melee | Shock unit |
| **War Elephant** | Iron | 170F 40G | 450 | 20 melee | 2 pop, slow, terrifying |
| **Stone Thrower** | Bronze | 180W 80G | 75 | 40 siege, range 8 | 2 pop; friendly fire on |
| **Catapult** | Iron | 180W 100G | 90 | 55 siege, range 9 | Splash damage |
| **Ballista** | Iron | 100W 80G | 80 | 30 pierce, range 9 | Anti-unit siege |

### 5.5 Priests and conversion

Kept close to the original because it is the series' signature:

- **[GD-PRIEST-01]** Priest walks into range (7 tiles), begins a chant, and after a variable
  interval the target unit changes ownership permanently.
- **[GD-PRIEST-02]** Conversion consumes **faith**, which recharges over ~40 seconds. A priest is
  a spent resource, not a spam unit.
- **[GD-PRIEST-03]** Buildings cannot be converted. Siege can (and it is devastating, on purpose).
- **[GD-PRIEST-04]** Priests heal friendly units at 3 HP/s when not converting.
- The chant is audible to both players. Hearing it near your army should make
  you react.

**As built** (2026-10-04): faith is spent whole and comes back over 40
seconds; the chant lasts four to ten seconds, and a priest without faith
walks into reach and waits there. The healing is 3 hit points once a
second to the most wounded unit of the priest's side within 4 tiles, not
to a siege engine. A priest at its chant does not run when hit; idle, it
runs home like a villager. A priest can convert a priest.

### 5.6 The water

- **[GD-NAVAL-01]** Boats move on the water and nothing else does: a boat
  never stands on land and a walker never stands in the water, shallow or
  deep. The Dock is built in the water against the shore, from the land
  beside it, and trains the boats onto the water beside it. Fish lie in
  the water; only a fishing boat gathers them, and it brings its catch to
  a Dock, which takes nothing from the land.
- **[GD-NAVAL-02]** Warships fight across the shore with what reaches over
  the water: a warship shoots at boats and at anything on land in its
  range, and archers, towers and siege on land shoot back. Hand weapons
  cannot fight a ship, and nothing waits at the water's edge for a target
  it cannot reach.
- **[GD-NAVAL-03]** A transport carries units over the water: they board
  it from the shore and it comes in to meet them; sent to land, it sails
  to the water nearest and puts them ashore there. At sea nobody steps
  off, and a transport that sinks takes everyone aboard with it. On the
  Islands map every start has an island of its own, and only boats cross.
- **[GD-NAVAL-04]** A trade boat takes wood from a Dock of its side's to
  another side's Dock and brings gold home, the more the further it
  sails, over and over until told otherwise.

**As built** (2026-10-04, `docs/07` D33): the water is its own grid with
its own flow fields; a group of boats and walkers ordered somewhere goes
as two. A Dock is three tiles square, every tile of it water, with land
beside it and water beside it. A fishing boat holds 15 (a villager 10),
gathers at the villager's rate and is trained in 30 seconds for 50 wood.
Fish hold 350 food: about one for every seventy tiles of water on the
wet map types, a tile or more from the shore and within six of it.
The warships: the Archer Ship (Tool Age, 100W 20G, 110 HP, 5 pierce at
range 5), the War Galley (Bronze, 130W 50G, 200 HP, 9 pierce at 6) and
the Catapult Ship (Iron, 160W 100G, 180 HP, 45 siege at 9 with a blast,
two population). The Transport (Tool Age, 75W, 150 HP) holds ten. The
Trade Boat (Bronze, 100W, 100 HP) takes 20 wood a trip and brings home
10 gold and three quarters of a gold for each tile between the market
and its side's nearest Dock. A right-click on land with a loaded
transport sails it there to unload; ALL ASHORE unloads where it lies.

---

## 6. Buildings

| Building | Age | Cost | Purpose |
|---|---|---|---|
| **Town Center** [V1] | Stone | 200W | Trains villagers, age research, drop-off, +5 pop |
| **House** [V1] | Stone | 30W | +5 pop |
| **Storehouse** [V1] | Stone | 100W | Universal drop-off; economy upgrades |
| **Barracks** [V1] | Stone | 125W | Infantry |
| **Dock** | Stone | 100W | Boats; water drop-off |
| **Farm** [V1] | Tool | 75W | Renewable food |
| **Archery Range** [V1] | Tool | 150W | Ranged units |
| **Stable** [V1] | Tool | 150W | Mounted units |
| **Market** | Tool | 150W | Economy tech; resource trading |
| **Watch Tower** [V1] | Tool | 120S | Static defence, vision |
| **Palisade Wall** [V1] | Tool | 5W/segment | Cheap early wall |
| **Temple** | Bronze | 200W | Priests, religious tech |
| **Academy** | Bronze | 200W | Heavy infantry |
| **Siege Workshop** | Bronze | 200W | Siege engines |
| **Government Centre** | Bronze | 175W | Civ-defining upgrades |
| **Stone Wall** | Bronze | 5S/segment | Real fortification |
| **Gate** | Bronze | 30S | Allies pass, enemies do not |
| **Guard Tower** | Bronze | 150S | Upgraded tower |
| **Wonder** | Iron | 1000W 1000S 1000G | Victory condition; enormous, visible, attackable |

**[GD-BUILD-01]** Buildings under construction show a build progress silhouette, take damage
normally, and can be finished by any villager.

**[GD-BUILD-02]** Villagers repair a finished, damaged building of their own at build speed,
as many of them counting as of builders. A repair is paid when it starts, half the
building's cost in proportion to the health missing at that moment, and does not start
if the side cannot pay; a repair interrupted and taken up again pays again for what
remains.

---

## 7. Technology

Research lives at the building it belongs to (armour at the Storehouse, faith at
the Temple), matching the original's mental model.

Three families:

1. **Economy** — gather rate bonuses, carry capacity, farm yield, villager HP.
2. **Military** — attack, armour, range and per-line upgrades (Clubman → Axeman,
   Hoplite → Legionary, Stone Thrower → Catapult), unlocked age by age. The
   Swordsman is the Barracks' own Bronze Age unit, and the Legionary the
   Hoplite line's last tier, as §5.2 has it (`docs/07` D30).
3. **Civic** — population efficiency, building HP, tower range, priest faith,
   conversion resistance, trade rates.

**Tech-tree denial is a design tool.** Each civ is missing real things. Being
denied the Iron Age cavalry upgrade should hurt and should change how you play.

Approximate counts for the full game: ~40 technologies, of which ~14 are in the
vertical slice.

---

## 8. Combat model

```
damage = max(1, (attack × elevation_modifier) − armour_of_matching_type + bonus_vs_class)
```

- **[GD-COMBAT-01] Damage types:** melee, pierce, siege. Units carry separate melee and pierce
  armour values.
- **[GD-COMBAT-02] Bonus damage vs class** (spearman vs cavalry, slinger vs infantry) is the
  counter system. It is deliberately shallow — three or four real counters, all
  discoverable from the unit tooltip.
- **[GD-COMBAT-03] Elevation:** ×1.25 attacking downhill, ×0.75 attacking uphill. Unchanged in
  spirit from the original.
- **[GD-COMBAT-04] Siege friendly fire is on.** It makes siege a decision rather than a free
  upgrade.
- **[GD-COMBAT-05] Minimum damage 1**, so nothing is literally invulnerable.

### 8.1 Unit behaviour (fixing the original's worst flaw)

**[GD-STANCE-01]** Every unit has a **stance**:

| Stance | Behaviour |
|---|---|
| **Aggressive** | Pursues enemies within ~6 tiles, returns to origin afterwards |
| **Defensive** (default for military) | Attacks enemies in range, does not chase far |
| **Stand ground** | Attacks in range, never moves |
| **Passive** (default for villagers) | Never attacks, flees toward the nearest Town Center when hit |

**[GD-STANCE-02]** Villagers being attacked **run and raise an alarm** rather than standing there
being killed. This one change removes most of the original's cruelty.

**[GD-STANCE-03]** A soldier on Aggressive or Defensive that is **hit by an enemy unit answers
it**, even one standing beyond its sight: an archer cannot stand off and shoot soldiers that
wait to see it. It answers only when free to (idle, attack-moving or patrolling), follows no
further than its stance's leash or two tiles past where the shooter stood, whichever is
further, and then walks back. Stand ground and Passive units do not answer, and nobody
answers a tower's arrows on their own. (Decided 2026-10-05, `docs/10` §6.)

---

## 9. Map, exploration and vision

- **Tile grid**, isometric 2:1 presentation, discrete elevation levels with
  cliffs. Elevation affects combat and vision, not movement cost (movement
  penalties on hills make pathing feel bad; we skip them).
- **[GD-FOG-01] Three visibility states** per player:
  1. **Unexplored** — black. Nothing known.
  2. **Explored** — terrain and last-known buildings visible, dimmed. Units are
     *not* shown. Buildings you saw stay drawn even after they are destroyed,
     until you look again — an intentional information asymmetry.
  3. **Visible** — live.
- **Map sizes:** Tiny 96², Small 128², Medium 168², Large 200², Giant 240².
- **Random map types for the full game:** Inland, Coastal, Continental,
  Highland, Islands, Narrows, Oasis. **Slice ships Inland only.**

  **As built** (2026-10-04): all seven, chosen on the setup screen.
  Highland is hillier, its mines richer and its forests thinner; Oasis is
  desert round a lake in the middle with six groves of palms on its shore;
  Coastal has a sea down one side, the starts moved away from it; Continental
  is a round land in a sea; Narrows is a river through the middle, between
  the starts, crossed at three fords that are kept clear of forest. No
  water lies within 14 tiles of a start, and every start's kit is the
  same. Islands puts every start on an island of its own on a ring two
  fifths of the map out, a channel of sea cut between every two however
  many players there are; only boats cross (`docs/07` D33). Boats sail
  every wet map (§5.6).
- **[GD-MAP-01]** Map generation is seeded and deterministic: the same seed always produces the
  same map, and starting positions are balanced (equal resources within a
  tolerance, verified by the generator before it returns).

---

## 10. Victory conditions

| Condition | Rule |
|---|---|
| **[GD-WIN-01] Conquest** (default) | Last player or team standing. A player is eliminated when they have no units and no buildings capable of producing them. |
| **[GD-WIN-02] Wonder** | Build a Wonder and hold it for **10 minutes**. Global announcement and a permanent minimap marker the moment it completes. |
| **[GD-WIN-03] Relics** | Control all relics on the map for **10 minutes**. |
| **Score** | Highest score at the time limit, if one is set. |
| **Resign** | Always available. |

Wonder and Relic victories exist to force endgames. Without them, two turtling
players produce a stalemate, which is the worst outcome an RTS can have.

**As built** (2026-10-04): every match has all three. A Wonder's clock
starts when it stands finished, and stops for good if it falls; a second
Wonder has its own. The relic clock runs while every relic on the map is
in the Temples of one side, and starts over when that stops (`docs/07`
D31). Both clocks are shown to every side under the top bar, a finished
Wonder is announced to everyone and marked on everyone's minimap in its
owner's colour, and so is a side coming to hold every relic. The Wonder
takes 1,500 builder-seconds and has 4,000 hit points.

---

## 11. Civilizations

Same design pattern as the original: **two or three flat numeric bonuses plus
tech-tree holes.** No unique mechanics, no unique UI. A civ should be readable
in ten seconds and still change your plan.

Eight civilizations for the full game, sharing four architecture sets:

| Civ | Bonuses | Denied |
|---|---|---|
| **Egyptians** [V1] | +20% gold gathering; chariots +33% HP; priests +2 range | Academy line, heavy cavalry |
| **Greeks** [V1] | Academy units +25% speed; ships +30% speed; hoplites available in Bronze | Chariots, horse archers |
| **Assyrians** | Villagers +10% move speed; archers fire 20% faster | Heavy infantry upgrades |
| **Babylonians** | Walls and towers +60% HP; stone miners +20% rate | Cavalry upgrades, siege workshop tier 2 |
| **Persians** | Hunting +30% food; elephants +50% speed | Ballista, guard towers |
| **Phoenicians** | +30% woodcutting; elephants −25% cost | Stone walls, priests tier 2 |
| **Shang** | Villagers −30% cost; walls +100% HP | Elephants, siege upgrades |
| **Sumerians** | Farms +100% output; siege +50% fire rate | Cavalry line beyond Bronze |

Vertical slice ships **Egyptians and Greeks** — an economic civ and a military
civ, enough to prove asymmetry is working.

**As built** (2026-10-04): all eight, read onto the units, buildings and
technologies the game has (`docs/07` D32, which gives the table as built);
what names something not yet in the game waits for it. The player picks a
civilization on the setup screen; the opponents' are dealt from the seed.
Each builds in its architecture (`docs/07` D34): the Greeks and
the Phoenicians in the Greek set, the Egyptians and the Sumerians in the
Egyptian, the Assyrians, the Babylonians and the Persians in the
Mesopotamian, and the Shang in the East Asian.

---

## 12. The computer opponent

**[GD-AI-01]** The AI plays through **exactly the same command interface a human uses**. It
cannot see through fog, and at Standard difficulty and below it does not receive
resource bonuses. This is a hard architectural rule, not a preference — an AI
that cheats produces an opponent you cannot learn from.

| Difficulty | Behaviour |
|---|---|
| **Easy** | Simple build order, small attacks, slow reactions, does not raid villagers |
| **Standard** | Solid build order, scouts, expands, counters unit composition, raids |
| **Hard** | Faster decisions, multi-pronged attacks, targets economy, walls chokes |
| **Hardest** | Hard, plus explicit resource bonuses — declared honestly in the UI |

**[GD-AI-02]** The AI's villagers do what a player's do, with the same orders: they
gather, build, farm, **hunt the animals near their drop-offs** from the opening, and
**repair damaged buildings** once no enemy is near them, paying
for the repair as a player does.

The AI is built as: a **build-order planner** (age goals, ratios), an **economy
manager** (villager assignment, drop-off placement), a **military manager**
(composition, grouping, attack timing), and a **scouting/threat model** driven by
its own fog state.

**As built** (2026-10-04): Standard and Hard research the economy's and the
army's technologies as each becomes worth having (Woodworking first; armour and
arrows for the soldiers they field; the farming technologies once they farm);
Easy researches none. Until the army has gone out once, research gets only what
the army leaves, so the first attack is not late. The economy builds a new
Storehouse by the trees or the mine once a third of their gatherers walk more
than eight tiles to drop off, moves gatherers from a resource piled past 1000 to
one running short, and saves for the next age after ten minutes in an age
whatever its army. Hard saves for a Wonder after ten minutes in the Iron Age.
At sea (`crate::navy`): with fish or an enemy over the water near home, it
builds a Dock on open sea where its villagers can walk to build it, keeps
fishing boats (Easy 2, Standard 4, Hard 6), keeps warships (Standard 2, Hard 4)
when the enemy is at sea or over the water, and looks for an enemy it has not
found with a ship along the edge of the water it has seen. Standard and Hard
carry the army over in transports once it has gathered, when the enemy cannot
be walked to, and land it on the shore nearest the enemy's Town Center.

---

## 13. Game modes

- **Skirmish** — 1–8 players, any mix of humans (later) and AI, teams, chosen map
  type, size, resources, population cap, victory conditions, starting age.
- **Campaign** — scripted scenarios with objectives, triggers and narration. Post-slice.
  First campaign is a *learning campaign*, in the model of *Ascent of Egypt*:
  each scenario teaches exactly one system.
  - **[GD-CAMP-01]** A scenario sets the match up as written: its map
    (generated as a skirmish map is, or drawn tile by tile), its sides
    (civilization, starting age, stockpile, technologies, who plays them),
    and what stands where.
  - **[GD-CAMP-02]** The player's objectives decide a scenario: every one
    that is not optional done wins it; one failed, or nothing of the
    player's left standing, loses it. The skirmish victories hold only when
    the scenario says so.
  - **[GD-CAMP-03]** Triggers, checked once a second in order, wait on
    time, objectives, other triggers, counts, stockpiles, ages,
    technologies, units in an area or a named unit gone, so long after
    another trigger, or any one of several such things, or one not
    holding; they narrate,
    show, complete or fail objectives, set units down, give resources,
    reveal ground, send a side to attack, and win or lose the scenario.
  - **[GD-CAMP-04]** A scenario is checked when it is loaded, and refused
    with what is wrong: a name that names nothing, an id defined twice or
    never, a map whose rows disagree, a place off the map.
  - **[GD-CAMP-05]** The campaigns are listed from the title, each with
    its scenarios in order; a scenario opens once the one before it is won,
    and a win is kept between sessions. A briefing gives the story and the
    objectives before play. In the match the objectives stand at the top
    right and the narrator's lines at the top, and the results offer the
    scenario again, the campaigns, or the next scenario once it is won.

  **As built** (2026-10-05, `sim::scenario`): a scenario is a RON file —
  the map, the sides, the placements, the objectives and the triggers —
  carried in the match's configuration, so a save and a replay carry it.
  A drawn map is rows of letters, one a tile (`g` grass, `d` dirt, `a`
  desert, `s` sand, `w` and `W` shallow and deep water, `f` forest, `n`
  snow), with optional corner heights; kinds and technologies are named as
  they are shown (`"Town Center"` or `"town_center"`). The campaigns are
  directories under `assets/campaigns`, each a `campaign.ron` naming its
  scenarios in order (`save::campaigns`); the scenarios won are kept in
  `campaigns.ron` beside the settings. Narration is text only (D35): a
  line stays up at least eight seconds, longer the longer it is.

  **The learning campaign** (2026-10-05, `assets/campaigns/learning`),
  *The Gift of the River*, played as the Egyptians on drawn maps of the
  Nile, one idea a scenario, each objective shown when the one before it
  is done:
  1. *Hunters on the Bank* (Naqada, about 3500 BC): gather wood, build a
     house, grow to seven villagers, build a storehouse by a far forest,
     gather food.
  2. *The Black Land* (Nekhen, about 3300 BC): a storehouse and a
     barracks, 400 food, the advance to the Tool Age, then farms (a
     market is optional).
  3. *Raiders from the West* (Thinis, about 3150 BC): train clubmen; the
     Tjehenu raid when they are ready or at five minutes; then destroy
     their camp, with a second raid at ten minutes for a slow player. The
     Town Center lost loses it.
  4. *Spears Against Horses* (Thebes, about 1550 BC): spearmen against
     Hyksos horsemen, slingers against their axemen, then both against
     Avaris. **This one leaves early Egypt on purpose:** the horse came to
     Egypt with the Hyksos, so a lesson in spears against riders set
     before them would be false history. Its raids are smaller than the
     army asked for, so the counter is seen to win.

  Each scenario is played to a win by a plain bot in the tests
  (`crates/sim/tests/campaign_learning.rs`), in about nine to twelve
  minutes for the first, second and fourth.

  **The historical campaigns** (2026-10-05, `assets/campaigns/persian-wars`
  and `assets/campaigns/sargon`), told from the history (D35):

  *The Persian Wars*, as the Greeks, 490 to 479 BC:
  1. *Marathon*: beat the Persian army on the plain, then march 8 hoplites
     home to Athens before the fleet, which sails twenty seconds after the
     battle, rounds the cape to Phaleron.
  2. *Thermopylae*: hold the pass for ten minutes against six waves; the
     archers shoot from beyond a hoplite's sight, so they must be charged,
     and at 6:30 Ephialtes' path brings the Immortals down behind, onto the
     road south, forty seconds after the warning. Leonidas living is
     optional.
  3. *Salamis*: three Persian squadrons come up the straits, the last round
     the island from the west; sink them all. The ships of Aegina join.
  4. *Plataea*: a full match against Mardonius, played by the computer;
     burn his tent (a Government Centre) and keep the Greek camp.

  *Sargon of Akkad*, as the Akkadians (the Assyrians' civilization, for
  its Mesopotamian look), about 2334 to 2280 BC; Sargon is a named unit
  who must live, and may shelter in a Town Center or a tower:
  1. *The Cupbearer*: grow Agade to 15 villagers, build a barracks and
     reach the Tool Age while Kish raids at four and eight minutes.
  2. *Lugal-zage-si*: a full match against Uruk, walled, played by the
     computer; take its Town Center. Bringing five soldiers to Nippur is
     optional.
  3. *Washing Weapons in the Sea*: take Ur (its old walls breached) and
     Lagash, then bring five soldiers to the shore of the Lower Sea; Umma
     raids the camp at five minutes.
  4. *King of the Four Quarters*: hold walled Agade for ten minutes against
     seven waves from Kish, Uruk, Ur and Elam, then destroy their camps.

  The bot wins every battle by playing it as asked
  (`crates/sim/tests/campaign_history.rs`); standing still loses at
  Thermopylae, and dawdling after Marathon loses the race. The app's
  tests have the computer play Mardonius and Uruk.

  A side may name where it starts, which the camera opens on (`start`).
  There are no alliances: every side other than the player's is at war
  with every other, so the shipped scenarios set no two of them within
  twelve tiles of each other, and the shipped-campaign check holds them to
  it. The check also holds every building to land and every ship to water.
- **Scenario editor** — terrain painting, unit placement, triggers, save/load,
  playtest. Post-slice, but the data formats are designed for it from the start.
  - **[GD-CAMP-06]** The editor opens a new map of a chosen size, or a
    scenario (the player's own, or a campaign's as a copy). It paints the
    ground, raises and lowers it, sets units, buildings and nature down
    for any side and takes them up, and edits the title, the briefing,
    the sides, the objectives and the triggers, every field of them. It
    shows what the check says is wrong, saves the scenario as the file a
    campaign reads, and plays it from where it stands and back. The
    player's saved scenarios are a campaign of their own, all open.

  **As built** (2026-10-05, `app::editor`, `app::editing`,
  `view::editor`): SCENARIO EDITOR on the title. The world on screen is
  a match made from the scenario and never stepped, so what is drawn is
  what will be played, without fog. A strip of tools along the top
  (terrain, height, units, scenario, objectives, triggers, check) and
  SAVE, PLAYTEST, EXIT; a palette beside the minimap; for the scenario,
  objectives and triggers, a list whose every field is a chip: a left
  click steps it on, a right click back, shift for big steps, a quoted
  line opens a line to type, and an area or a tile is dragged or clicked
  on the map. On the map a left click paints (right picks the letter
  up), raises (right lowers), or sets down, takes up or names a unit for
  a trigger to wait on (right takes up). A unit or building goes only
  where the ground is clear for it. A scenario on a generated map opens
  baked onto a drawn one. Saves go to `scenarios/` beside the settings,
  named after the title; PLAYTEST refuses while anything is wrong and
  shows the check; a playtest is not recorded and does not count as a
  win. Leaving with changes unsaved takes a second EXIT or Escape.
  Conditions made of others (`Any`, `Not`) are shown and deleted in the
  editor but edited in the file.
- **Cheat codes** — anachronistic joke units and resource grants, disabled in
  multiplayer and flagged in the replay. Non-negotiable; they are part of the
  memory of this game.
  **[GD-CHEAT-01]** Resource grants: in a match, Enter opens a line, a code
  and Enter again gives the player 1000 of a resource. Each code is an order
  like any other, so the replay holds it and who gave it; the simulation
  ignores one a computer opponent issues, and a replay being watched takes
  none. The codes: `BOUNTIFUL HARVEST` (food), `MIGHTY OAK` (wood),
  `SOLID ROCK` (stone), `MIDAS TOUCH` (gold); case and spacing do not
  matter. The joke units are still to come.

---

## 14. Difficulty, speed and accessibility

- **[GD-SPEED-01] Game speed** ×0.5 / ×1.0 / ×1.5 / ×2.0, changeable mid-match in single-player.
- **[GD-SPEED-02] Pause** in single-player, with commands issuable while paused.
- **[GD-A11Y-01]** Colourblind-safe player palette, verified against deuteranopia and protanopia
  simulations.
- **[GD-A11Y-02]** Full key rebinding, UI scale from 100% to 200%, subtitles for all narration.
- **[GD-A11Y-03]** A second ownership cue besides colour: each player has a symbol (a circle,
  a square, a triangle, a diamond, a plus, an X, a triangle upside down, a star, in player order),
  drawn in the player's colour over the selection, over every unit and building with the
  setting SYMBOLS at ALWAYS (none with OFF), and beside the colour wherever a side is listed.
- No timed input requirements anywhere in the interface.

---

## 15. Open design questions

Tracked in `docs/07-decisions-and-open-questions.md`. The significant ones:

1. Do we ship water/naval in v1.0, or hold it for the first content update?
2. Should relics be static objects (AoE1 ruins) or carryable by priests (AoE2)?
3. Is the Government Centre worth its own building, or should its upgrades fold
   into the Town Center?
4. How much of the campaign fiction do we write ourselves versus lean on real
   history? *(Answered: history, one narrator, in text; `docs/07` D35.)*

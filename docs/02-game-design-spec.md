# Game Design Spec — *New Empire* (working title)

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
| Gazelle / deer | 140 food | Must be killed first; decays if left |
| Boar / elephant | 400 food | Fights back; a genuine early decision |
| Fish (shallow) | 200 food | Reachable by villagers on shore |
| Fish (deep) | 350 food | Requires a Fishing Boat |
| Gold vein | 400 gold | 4–7 tiles per deposit |
| Stone vein | 350 stone | 3–5 tiles per deposit |
| Farm | 250 food | Re-seed costs 60 wood; auto-reseed toggle on by default |

### 3.3 Gathering

- **[GD-ECON-02]** Base gather rate **0.45 resources/second**, carry capacity **10**, then walk to
  the nearest valid drop-off and deposit.
- **[GD-ECON-03] One drop-off building type — the Storehouse** — accepting all resources. The
  original's Granary/Storage Pit split was bookkeeping, not depth. The Town
  Center also accepts everything.
- **[GD-ECON-04]** Distance to drop-off is the real economic skill: placement matters, walking
  time is the cost.
- **[GD-ECON-05] Farms auto-reseed by default** (toggle per-farm and globally), and a
  notification fires when wood is too low to reseed.

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
| **Spearman** [V1] | Tool | 40F 20W | 45 | 4 melee | 0/1 | +6 vs cavalry & elephants |
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

---

## 7. Technology

Research lives at the building it belongs to (armour at the Storehouse, faith at
the Temple), matching the original's mental model.

Three families:

1. **Economy** — gather rate bonuses, carry capacity, farm yield, villager HP.
2. **Military** — attack, armour, range and per-line upgrades (Clubman → Axeman →
   Swordsman → Legionary), unlocked age by age.
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

The AI is built as: a **build-order planner** (age goals, ratios), an **economy
manager** (villager assignment, drop-off placement), a **military manager**
(composition, grouping, attack timing), and a **scouting/threat model** driven by
its own fog state.

---

## 13. Game modes

- **Skirmish** — 1–8 players, any mix of humans (later) and AI, teams, chosen map
  type, size, resources, population cap, victory conditions, starting age.
- **Campaign** — scripted scenarios with objectives, triggers and narration. Post-slice.
  First campaign is a *learning campaign*, in the model of *Ascent of Egypt*:
  each scenario teaches exactly one system.
- **Scenario editor** — terrain painting, unit placement, triggers, save/load,
  playtest. Post-slice, but the data formats are designed for it from the start.
- **Cheat codes** — anachronistic joke units and resource grants, disabled in
  multiplayer and flagged in the replay. Non-negotiable; they are part of the
  memory of this game.

---

## 14. Difficulty, speed and accessibility

- **[GD-SPEED-01] Game speed** ×0.5 / ×1.0 / ×1.5 / ×2.0, changeable mid-match in single-player.
- **[GD-SPEED-02] Pause** in single-player, with commands issuable while paused.
- **[GD-A11Y-01]** Colourblind-safe player palette, verified against deuteranopia and protanopia
  simulations.
- **[GD-A11Y-02]** Full key rebinding, UI scale from 100% to 200%, subtitles for all narration.
- No timed input requirements anywhere in the interface.

---

## 15. Open design questions

Tracked in `docs/07-decisions-and-open-questions.md`. The significant ones:

1. Do we ship water/naval in v1.0, or hold it for the first content update?
2. Should relics be static objects (AoE1 ruins) or carryable by priests (AoE2)?
3. Is the Government Centre worth its own building, or should its upgrades fold
   into the Town Center?
4. How much of the campaign fiction do we write ourselves versus lean on real
   history?

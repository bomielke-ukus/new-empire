# Research: Age of Empires (1997) — what it was, and why it worked

This document is the evidence base for our design. It records what the original
game actually did, how it ran on 1997 hardware, what players loved, and what it
got wrong. Everything downstream in `docs/` should be traceable to something here.

Sources are listed at the bottom. Where a number could not be confirmed from a
reachable source it is marked **[unverified]** and must not be treated as fact.

---

## 1. The shape of the thing

| | |
|---|---|
| Developer / publisher | Ensemble Studios / Microsoft |
| Release | 26 October 1997 (North America), February 1998 (rest of world) |
| Engine | Genie — 2D, isometric, tile-based, single-threaded game loop |
| Default resolution | 640×480, 256-colour palette |
| Players | Up to 8, LAN or internet (MSN Gaming Zone) |
| Expansion | *The Rise of Rome* (1998) — 4 more civilizations, 4 more map types |

The pitch was a genre collision: take the real-time base-building of *Warcraft*
and *Command & Conquer*, and put it on the historical-progression subject matter
of *Civilization*. Nobody had done ancient history in real time at that scale.

---

## 2. Core systems

### 2.1 Resources

Four resources: **food, wood, stone, gold**.

Food came from four distinct activities — hunting, foraging, farming, fishing —
each with its own rhythm and risk. Wood from trees, stone and gold from mines.

The defining rule: **resources are finite and do not regenerate**. A felled tree
is gone. A mined-out gold vein is gone forever. This single decision is
load-bearing for the whole game, and is explored in §5.

Drop-off buildings mattered, and were deliberately split:

- **Granary** — food from foraging and farming only
- **Storage Pit** — wood, stone, gold, and food from hunting and fishing
- **Town Center** — everything

Reference values (confirmed): a tree yields **75 wood** (forest trees 40); a
forage bush yields **150 food**; a house supports **+4 population**; a Granary
costs **120 wood**; a Farm costs **75 wood**.

### 2.2 Ages

Four ages: **Stone → Tool → Bronze → Iron**.

Advancing was gated on resources *and* on having built into the current age —
e.g. reaching the Tool Age required **500 food and two Stone Age buildings**
(Town Center and Houses excluded). So you could not rush the age button by
hoarding; you had to actually develop.

Each age-up visibly transformed your settlement — buildings and unit sprites
changed appearance — and unlocked a new tier of buildings, units and research.

### 2.3 Technology

Research happened at thematically appropriate buildings: armour upgrades at the
Storage Pit, religious research at the Temple, and so on. Each civilization had
its **own tech tree with deliberate holes** — a civ was defined as much by what
it was *denied* as by what it was given.

### 2.4 Civilizations

Twelve at launch — Assyrians, Babylonians, Choson, Egyptians, Greeks, Hittites,
Minoans, Persians, Phoenicians, Shang, Sumerians, Yamato — drawn in four
architecture sets (East Asian, Egyptian, Greek, Mesopotamian), so four civs
shared a visual identity.

Bonuses were small, numeric and legible. Examples confirmed in research:

- **Phoenicians** — +30% woodcutting, cheaper elephants, faster siege ships
- **Sumerians** — +50% catapult fire rate, +30% villager HP, double farm output
- **Shang** — −30% villager cost, +100% wall HP
- **Hittites** — double siege HP, +1 archer attack, +4 warship range
- **Babylonians** — +60% wall/tower HP, faster stone mining, faster priest recharge

Note the pattern: **two to four percentage modifiers plus tech-tree denials.** No
civ had a unique mechanic. You could learn a civ in one game.

*(Some per-civ bonus lists in secondary sources are transcription-corrupted —
Choson and Yamato in particular. We are designing our own bonuses anyway, so
the pattern matters more than the exact figures.)*

### 2.5 Combat

- Damage was **attack minus the matching armour class**, with separate armour
  types per damage type, floored at a small minimum.
- **Elevation mattered**: roughly ±25% attack for high/low ground. Holding a
  hill was a real tactical decision.
- **Priests** could not attack. They healed allies and **converted enemy units**
  — permanently stealing them, complete with a chant that became the series'
  most recognisable sound. Conversion had a faith recharge, so priests were a
  resource you spent and waited on, not a weapon you spammed.

### 2.6 Population and scale

Default population cap was **50**. Patch 1.0a and *Rise of Rome* let multiplayer
games set it anywhere from **25 to 200**. Fifty units is small by modern
standards, and it made every unit feel individually consequential.

### 2.7 Victory

- **Conquest** — destroy everything. The default, and by far the most common.
- **Wonder** — build a Wonder and hold it for 2,000 in-game years (**16m40s** at
  normal speed). A giant, expensive "come and stop me" declaration.
- **Ruins / Artifacts** — control all of them on the map for the same duration.
- **Score / time limit** — points-based fallback.

Wonder victory is a genuinely great piece of design: it converts a wealth lead
into a visible, attackable object and forces the endgame to happen.

### 2.8 Content beyond skirmish

- **Campaigns**: *Ascent of Egypt* (a learning campaign that taught the game
  scenario by scenario), *Glory of Greece*, *Voices of Babylon*, *Yamato: Empire
  of the Rising Sun*; *Rise of Rome* added *Reign of the Hittites* and the
  Roman campaigns.
- **Random maps** with selectable types, so skirmish had endless variety.
- **A scenario editor**, shipped in the box, with community sharing. A large part
  of the game's long tail.
- **Cheat codes** (`BIGDADDY`, `E=MC2 TROOPER`, `PHOTON MAN`…) that spawned
  anachronistic units. Pure joy, zero competitive relevance, and a big part of
  how the game was remembered.

---

## 3. How it actually ran

### 3.1 The renderer

Genie was a **2D, single-threaded, tile-based isometric engine** drawing
**256-colour sprites** stored in a custom `SLP` format — essentially a bundle of
bitmaps with run-length compression, special palette indices, and multiple
viewing angles stored as consecutive frames.

Two consequences worth stealing:

1. **Palette-indexed sprites with reserved index ranges** gave free per-player
   recolouring — the same sprite drawn blue, red or yellow with no extra art.
2. **Mirroring**: sprites were authored for roughly half the facings and
   mirrored horizontally for the rest, cutting frame count substantially.

Age of Empires II later added 3D-rendered *terrain* with elevation while keeping
2D sprites for units and buildings — the aesthetic and the performance profile
both survived.

### 3.2 The simulation and the network

This is the most instructive part of the whole game, from Mark Terrano and Paul
Bettner's GDC 2001 talk *"1500 Archers on a 28.8"*:

The obvious approach — send each unit's position, state, facing and damage —
would have capped the game at roughly **250 moving units** on a 28.8 kbps modem.
Unacceptable for the game they wanted.

So they did the opposite: **send only player commands, and run the identical
simulation on every machine.** Bandwidth then scales with *how fast players
click*, not with how many units exist. 1,500 units cost the same as 15.

The price is strict determinism: every machine must produce bit-identical
results, which means controlling random number generation and every arithmetic
operation that feeds the simulation. Any divergence is an out-of-sync, and it is
unrecoverable — which is why AoE-era RTS games famously dropped everyone when
one machine drifted.

Commands were scheduled to execute a couple of turns in the future, so each
machine always had every player's input for the turn it was about to run. Turn
length adapted to measured latency.

### 3.3 Performance budget

After optimisation the frame breakdown was roughly:

- **~30% graphics rendering**
- **~30% AI and pathfinding**
- **~30% simulation and maintenance**

Pathfinding costing as much as *drawing the entire game* is the headline number.
It should calibrate our expectations: pathing is not a detail to bolt on.

---

## 4. What it got wrong

Named plainly, because our design must answer each one:

1. **Pathfinding was poor.** Units took bizarre routes, got stuck on each other
   and on terrain, and faster units would not overtake slower ones. This is the
   single most cited criticism of the game.
2. **Everything needed micromanagement.** Units did not defend themselves
   sensibly; battles felt like herding rather than commanding.
3. **Selection was capped** — band-box selection was limited to a small number
   of units (reported around 25), forcing repetitive input.
4. **No unit queueing.** You clicked a building once per unit.
5. **Farms did not re-seed.** They expired silently and you lost economy without
   noticing.
6. **No rally points onto resources, no waypoints, no formations, no
   attack-move.** All added later in the series, or in the 2018 Definitive
   Edition, and all now feel like they were always there.
7. **The AI was weak and predictable**, leaning on resource cheats rather than
   good play.

Age of Empires II and the Definitive Editions fixed most of this: unit queueing,
auto-reseeding farms, garrisoning, waypoints, formations, the town bell, idle
villager cycling (`.`), villager rally points onto resources, and attack-move
(`A` + right-click).

---

## 5. Why it was engaging — the actual design analysis

Nostalgia is not a feature. These are the mechanisms that produced the feeling,
and they are what we have to reproduce:

**5.1 Compressed, visible history.** You go from a handful of hunter-gatherers to
an armoured empire in half an hour, and *you can see it happen*. Buildings and
units visibly change at each age-up. Progress is not a number on a bar; it is
your town looking different.

**5.2 The map is a finite, shared board.** Because nothing regenerates, the map
is a slowly depleting resource that everyone is drawing from. That converts
economy into geography: you expand because the wood near home ran out, and
expansion puts you in contact with the enemy. Conflict emerges from the economy
rather than being scheduled by the designer.

**5.3 Every villager is a decision.** With a 50-population cap, a villager on
gold is a villager not in your army. The economy/military tension is felt at the
level of individual units.

**5.4 Fog of war makes the map a puzzle.** Exploration has real information
value: where is the enemy, where is the gold, where is the choke point. Scouting
is a genuine strategic activity, not a chore.

**5.5 Everything speaks.** Chopping, mining, sheep, the villager acknowledgment
grunt, the priest's chant, the age-up fanfare. Dense, distinct, positional audio
is why the world felt alive rather than like a spreadsheet in motion. This is
routinely underestimated and it is at least a third of the "feel".

**5.6 A low skill floor with a high ceiling.** You could play slowly, badly, at
your own pace, and still have a good time and finish a game. It did not demand
actions-per-minute to be enjoyable. That is why so many people who are not RTS
players loved this specific RTS.

**5.7 Legible asymmetry.** Civ bonuses were two or three percentages. You could
read a civ in ten seconds and still find it changed how you played.

**5.8 Creation tools.** The scenario editor turned players into authors and gave
the game a decade-long tail.

**5.9 Pacing with distinct movements.** Stone Age scramble (fragile, exploratory)
→ Tool Age tension (first contact, first raids) → Bronze Age war (real armies,
real tech choices) → Iron Age spectacle (siege, elephants, wonders). Each age
feels like a different game.

---

## 6. What we take, what we fix

| Original behaviour | Our decision |
|---|---|
| 4 resources, finite, no regrowth | **Keep.** Load-bearing for the whole design. |
| Split Granary / Storage Pit drop-offs | **Simplify.** One drop-off type; the split was busywork, not depth. |
| 4 ages with visible transformation | **Keep**, and invest more in the visual change. |
| Small numeric civ bonuses + tech-tree holes | **Keep** exactly this pattern. |
| Elevation combat modifier | **Keep.** Cheap to implement, real tactical texture. |
| Priest conversion | **Keep.** It is the series' signature and it is genuinely fun. |
| Population cap ~50 | **Keep low by default** (75), configurable. Unit value > unit count. |
| Wonder / relic hold-timer victories | **Keep.** They force endgames. |
| Deterministic lockstep simulation | **Keep**, from day one, with fixed-point maths instead of floats. |
| Palette-indexed sprites, mirrored facings | **Keep.** Free player colours, half the art. |
| Bad pathfinding | **Fix.** Hierarchical pathing + flow fields + local avoidance. |
| Selection cap, no queueing, no waypoints | **Fix.** No caps; full modern command vocabulary. |
| Farms silently expiring | **Fix.** Auto-reseed, toggleable, with clear notification. |
| Weak, cheating AI | **Fix.** Honest AI issuing the same commands a player can. |
| Cheat codes | **Keep.** They cost nothing and they are part of the memory. |
| Scenario editor | **Keep** — but later; it is post-slice work. |

---

## Sources

- [Age of Empires (video game) — Wikipedia](https://en.wikipedia.org/wiki/Age_of_Empires_(video_game))
- [Age of Empires — Age of Empires Series Wiki](https://ageofempires.fandom.com/wiki/Age_of_Empires)
- [Civilization (Age of Empires) — Series Wiki](https://ageofempires.fandom.com/wiki/Civilization_(Age_of_Empires))
- [Civilization bonus — Series Wiki](https://ageofempires.fandom.com/wiki/Civilization_bonus)
- [Genie Engine — Wikipedia](https://en.wikipedia.org/wiki/Genie_Engine)
- [Genie Engine — ModDB](https://www.moddb.com/engines/genie-engine)
- [An overview of the SLP asset format used by the Genie Engine](https://gist.github.com/prototypicalpro/f551d4612c3dac1e545464dec63efc67)
- [1500 Archers on a 28.8: Network Programming in Age of Empires and Beyond — Terrano & Bettner, GDC 2001](https://www.gamedeveloper.com/programming/1500-archers-on-a-28-8-network-programming-in-age-of-empires-and-beyond)
- [openage discussion on the 1500 Archers architecture](https://github.com/SFTtech/openage/discussions/1493)
- [Victory — Series Wiki](https://ageofempires.fandom.com/wiki/Victory) and [Liquipedia: Victory](https://liquipedia.net/ageofempires/Victory)
- [Population — Series Wiki](https://ageofempires.fandom.com/wiki/Population)
- [Elevation — Series Wiki](https://ageofempires.fandom.com/wiki/Elevation) and [Combat Basics: Elevation](https://steamcommunity.com/sharedfiles/filedetails/?id=637133974)
- [Attack](https://ageofempires.fandom.com/wiki/Attack) / [Armor](https://ageofempires.fandom.com/wiki/Armor) — Series Wiki
- [Priest (Age of Empires) — Series Wiki](https://ageofempires.fandom.com/wiki/Priest_(Age_of_Empires))
- [Storage Pit](https://ageofempires.fandom.com/wiki/Storage_Pit_(Age_of_Empires)) / [Granary](https://ageofempires.fandom.com/wiki/Granary_(Age_of_Empires)) — Series Wiki
- [Tree](https://ageofempires.fandom.com/wiki/Tree) / [Berry Bush](https://ageofempires.fandom.com/wiki/Berry_Bush) — Series Wiki
- [Stone Age](https://ageofempires.fandom.com/wiki/Stone_Age) / [Tool Age](https://ageofempires.fandom.com/wiki/Tool_Age) / [Bronze Age](https://ageofempires.fandom.com/wiki/Bronze_Age) — Series Wiki
- [What's new in Age of Empires: Definitive Edition](https://www.ageofempires.com/news/whats-new-age-empires-definitive-edition-2/)
- [Age of Empires: Definitive Edition review — PC Gamer](https://www.pcgamer.com/age-of-empires-definitive-edition-review/)
- [On Age DE's pathing/movement — Richard Geldreich](http://richg42.blogspot.com/2018/02/on-age-des-pathingmovement.html)
- [Age of Empires (1997) — Lilura1 retrospective](https://lilura1.blogspot.com/2026/01/Age-of-Empires-Windows-PC-Ensemble-Studios-1997.html)
- [Once Upon a Time… an Age of Empires Retrospective — Xbox Wire](https://news.xbox.com/en-us/2018/02/24/once-upon-a-time-age-of-empires-feature/)

# Damage matrix

Generated from the kinds table by the command below; `scripts/check-generated.sh` fails if this file drifts from it. Do not edit by hand.

```sh
cargo run -p simrunner -- matrix --out docs/damage-matrix.md
```

The rule (`docs/02` §8, `GD-COMBAT-01` to `GD-COMBAT-05`):

```text
damage = max(1, (attack × elevation) − armour_of_matching_type + bonus_vs_class)
```

Elevation is five quarters of the attack downhill and three quarters uphill, rounded to nearest with halves up. Melee hits meet melee armour, pierce hits meet pierce armour, siege hits meet no armour and land on friends in the way too. Nothing does less than 1.

## Kinds

| Kind | Class | HP | Attack | Range | Armour (melee/pierce) | Bonus |
|---|---|---|---|---|---|---|
| Villager | villagers | 25 | 3 melee | hand | 0/0 | +2 vs animals |
| Scout | cavalry | 45 | 2 melee | hand | 0/0 | — |
| Clubman | infantry | 40 | 3 melee | hand | 0/0 | — |
| Axeman | infantry | 50 | 5 melee | hand | 0/0 | — |
| Spearman | infantry | 45 | 4 melee | hand | 0/1 | +6 vs cavalry |
| Slinger | archers | 40 | 4 pierce | 4 | 0/0 | +4 vs infantry |
| Bowman | archers | 40 | 5 pierce | 5 | 0/0 | — |
| Light Cavalry | cavalry | 90 | 7 melee | hand | 0/0 | — |
| Swordsman | infantry | 70 | 8 melee | hand | 1/1 | — |
| Hoplite | infantry | 120 | 12 melee | hand | 4/2 | — |
| Legionary | infantry | 160 | 16 melee | hand | 5/3 | — |
| Chariot Archer | archers | 70 | 6 pierce | 5 | 0/0 | — |
| Horse Archer | archers | 60 | 7 pierce | 6 | 0/1 | — |
| Heavy Cavalry | cavalry | 150 | 12 melee | hand | 1/1 | — |
| War Elephant | cavalry | 450 | 20 melee | hand | 1/2 | — |
| Stone Thrower | siege | 75 | 40 siege | 8 | 0/2 | — |
| Catapult | siege | 90 | 55 siege | 9 | 0/2 | — |
| Ballista | siege | 80 | 30 pierce | 9 | 0/2 | — |
| Town Center | buildings | 600 | 5 pierce | 5 | 0/6 | — |
| House | buildings | 75 | — | — | 0/5 | — |
| Storehouse | buildings | 200 | — | — | 0/5 | — |
| Barracks | buildings | 350 | — | — | 0/5 | — |
| Farm | buildings | 60 | — | — | 0/5 | — |
| Archery Range | buildings | 350 | — | — | 0/5 | — |
| Stable | buildings | 350 | — | — | 0/5 | — |
| Market | buildings | 300 | — | — | 0/5 | — |
| Watch Tower | buildings | 250 | 4 pierce | 5 | 1/6 | — |
| Palisade Wall | buildings | 150 | — | — | 2/8 | — |
| Stone Wall | buildings | 400 | — | — | 3/10 | — |
| Gate | buildings | 350 | — | — | 3/10 | — |
| Temple | buildings | 400 | — | — | 0/5 | — |
| Academy | buildings | 400 | — | — | 0/5 | — |
| Siege Workshop | buildings | 400 | — | — | 0/5 | — |
| Government Centre | buildings | 400 | — | — | 0/5 | — |

## Damage per hit, level ground, no technology

Rows attack columns.

| Attacker \ Target | Villager | Scout | Clubman | Axeman | Spearman | Slinger | Bowman | Light Cavalry | Swordsman | Hoplite | Legionary | Chariot Archer | Horse Archer | Heavy Cavalry | War Elephant | Stone Thrower | Catapult | Ballista | Town Center | House | Storehouse | Barracks | Farm | Archery Range | Stable | Market | Watch Tower | Palisade Wall | Stone Wall | Gate | Temple | Academy | Siege Workshop | Government Centre |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Villager** | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 2 | 1 | 1 | 3 | 3 | 2 | 2 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 2 | 1 | 1 | 1 | 3 | 3 | 3 | 3 |
| **Scout** | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 1 | 1 | 1 | 2 | 2 | 1 | 1 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 1 | 1 | 1 | 1 | 2 | 2 | 2 | 2 |
| **Clubman** | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 2 | 1 | 1 | 3 | 3 | 2 | 2 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 2 | 1 | 1 | 1 | 3 | 3 | 3 | 3 |
| **Axeman** | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 4 | 1 | 1 | 5 | 5 | 4 | 4 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 4 | 3 | 2 | 2 | 5 | 5 | 5 | 5 |
| **Spearman** | 4 | 10 | 4 | 4 | 4 | 4 | 4 | 10 | 3 | 1 | 1 | 4 | 4 | 9 | 9 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 3 | 2 | 1 | 1 | 4 | 4 | 4 | 4 |
| **Slinger** | 4 | 4 | 8 | 8 | 7 | 4 | 4 | 4 | 7 | 6 | 5 | 4 | 3 | 3 | 2 | 2 | 2 | 2 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| **Bowman** | 5 | 5 | 5 | 5 | 4 | 5 | 5 | 5 | 4 | 3 | 2 | 5 | 4 | 4 | 3 | 3 | 3 | 3 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| **Light Cavalry** | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 6 | 3 | 2 | 7 | 7 | 6 | 6 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 6 | 5 | 4 | 4 | 7 | 7 | 7 | 7 |
| **Swordsman** | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 7 | 4 | 3 | 8 | 8 | 7 | 7 | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 8 | 7 | 6 | 5 | 5 | 8 | 8 | 8 | 8 |
| **Hoplite** | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 11 | 8 | 7 | 12 | 12 | 11 | 11 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 11 | 10 | 9 | 9 | 12 | 12 | 12 | 12 |
| **Legionary** | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 15 | 12 | 11 | 16 | 16 | 15 | 15 | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 16 | 15 | 14 | 13 | 13 | 16 | 16 | 16 | 16 |
| **Chariot Archer** | 6 | 6 | 6 | 6 | 5 | 6 | 6 | 6 | 5 | 4 | 3 | 6 | 5 | 5 | 4 | 4 | 4 | 4 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| **Horse Archer** | 7 | 7 | 7 | 7 | 6 | 7 | 7 | 7 | 6 | 5 | 4 | 7 | 6 | 6 | 5 | 5 | 5 | 5 | 1 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 1 | 1 | 1 | 1 | 2 | 2 | 2 | 2 |
| **Heavy Cavalry** | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 11 | 8 | 7 | 12 | 12 | 11 | 11 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 11 | 10 | 9 | 9 | 12 | 12 | 12 | 12 |
| **War Elephant** | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 19 | 16 | 15 | 20 | 20 | 19 | 19 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 20 | 19 | 18 | 17 | 17 | 20 | 20 | 20 | 20 |
| **Stone Thrower** | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 | 40 |
| **Catapult** | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 | 55 |
| **Ballista** | 30 | 30 | 30 | 30 | 29 | 30 | 30 | 30 | 29 | 28 | 27 | 30 | 29 | 29 | 28 | 28 | 28 | 28 | 24 | 25 | 25 | 25 | 25 | 25 | 25 | 25 | 24 | 22 | 20 | 20 | 25 | 25 | 25 | 25 |

## Hits to kill, level ground, no technology

| Attacker \ Target | Villager | Scout | Clubman | Axeman | Spearman | Slinger | Bowman | Light Cavalry | Swordsman | Hoplite | Legionary | Chariot Archer | Horse Archer | Heavy Cavalry | War Elephant | Stone Thrower | Catapult | Ballista | Town Center | House | Storehouse | Barracks | Farm | Archery Range | Stable | Market | Watch Tower | Palisade Wall | Stone Wall | Gate | Temple | Academy | Siege Workshop | Government Centre |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Villager** | 9 | 15 | 14 | 17 | 15 | 14 | 14 | 30 | 35 | 120 | 160 | 24 | 20 | 75 | 225 | 25 | 30 | 27 | 200 | 25 | 67 | 117 | 20 | 117 | 117 | 100 | 125 | 150 | 400 | 350 | 134 | 134 | 134 | 134 |
| **Scout** | 13 | 23 | 20 | 25 | 23 | 20 | 20 | 45 | 70 | 120 | 160 | 35 | 30 | 150 | 450 | 38 | 45 | 40 | 300 | 38 | 100 | 175 | 30 | 175 | 175 | 150 | 250 | 150 | 400 | 350 | 200 | 200 | 200 | 200 |
| **Clubman** | 9 | 15 | 14 | 17 | 15 | 14 | 14 | 30 | 35 | 120 | 160 | 24 | 20 | 75 | 225 | 25 | 30 | 27 | 200 | 25 | 67 | 117 | 20 | 117 | 117 | 100 | 125 | 150 | 400 | 350 | 134 | 134 | 134 | 134 |
| **Axeman** | 5 | 9 | 8 | 10 | 9 | 8 | 8 | 18 | 18 | 120 | 160 | 14 | 12 | 38 | 113 | 15 | 18 | 16 | 120 | 15 | 40 | 70 | 12 | 70 | 70 | 60 | 63 | 50 | 200 | 175 | 80 | 80 | 80 | 80 |
| **Spearman** | 7 | 5 | 10 | 13 | 12 | 10 | 10 | 9 | 24 | 120 | 160 | 18 | 15 | 17 | 50 | 19 | 23 | 20 | 150 | 19 | 50 | 88 | 15 | 88 | 88 | 75 | 84 | 75 | 400 | 350 | 100 | 100 | 100 | 100 |
| **Slinger** | 7 | 12 | 5 | 7 | 7 | 10 | 10 | 23 | 10 | 20 | 32 | 18 | 20 | 50 | 225 | 38 | 45 | 40 | 600 | 75 | 200 | 350 | 60 | 350 | 350 | 300 | 250 | 150 | 400 | 350 | 400 | 400 | 400 | 400 |
| **Bowman** | 5 | 9 | 8 | 10 | 12 | 8 | 8 | 18 | 18 | 40 | 80 | 14 | 15 | 38 | 150 | 25 | 30 | 27 | 600 | 75 | 200 | 350 | 60 | 350 | 350 | 300 | 250 | 150 | 400 | 350 | 400 | 400 | 400 | 400 |
| **Light Cavalry** | 4 | 7 | 6 | 8 | 7 | 6 | 6 | 13 | 12 | 40 | 80 | 10 | 9 | 25 | 75 | 11 | 13 | 12 | 86 | 11 | 29 | 50 | 9 | 50 | 50 | 43 | 42 | 30 | 100 | 88 | 58 | 58 | 58 | 58 |
| **Swordsman** | 4 | 6 | 5 | 7 | 6 | 5 | 5 | 12 | 10 | 30 | 54 | 9 | 8 | 22 | 65 | 10 | 12 | 10 | 75 | 10 | 25 | 44 | 8 | 44 | 44 | 38 | 36 | 25 | 80 | 70 | 50 | 50 | 50 | 50 |
| **Hoplite** | 3 | 4 | 4 | 5 | 4 | 4 | 4 | 8 | 7 | 15 | 23 | 6 | 5 | 14 | 41 | 7 | 8 | 7 | 50 | 7 | 17 | 30 | 5 | 30 | 30 | 25 | 23 | 15 | 45 | 39 | 34 | 34 | 34 | 34 |
| **Legionary** | 2 | 3 | 3 | 4 | 3 | 3 | 3 | 6 | 5 | 10 | 15 | 5 | 4 | 10 | 30 | 5 | 6 | 5 | 38 | 5 | 13 | 22 | 4 | 22 | 22 | 19 | 17 | 11 | 31 | 27 | 25 | 25 | 25 | 25 |
| **Chariot Archer** | 5 | 8 | 7 | 9 | 9 | 7 | 7 | 15 | 14 | 30 | 54 | 12 | 12 | 30 | 113 | 19 | 23 | 20 | 600 | 75 | 200 | 350 | 60 | 350 | 350 | 300 | 250 | 150 | 400 | 350 | 400 | 400 | 400 | 400 |
| **Horse Archer** | 4 | 7 | 6 | 8 | 8 | 6 | 6 | 13 | 12 | 24 | 40 | 10 | 10 | 25 | 90 | 15 | 18 | 16 | 600 | 38 | 100 | 175 | 30 | 175 | 175 | 150 | 250 | 150 | 400 | 350 | 200 | 200 | 200 | 200 |
| **Heavy Cavalry** | 3 | 4 | 4 | 5 | 4 | 4 | 4 | 8 | 7 | 15 | 23 | 6 | 5 | 14 | 41 | 7 | 8 | 7 | 50 | 7 | 17 | 30 | 5 | 30 | 30 | 25 | 23 | 15 | 45 | 39 | 34 | 34 | 34 | 34 |
| **War Elephant** | 2 | 3 | 2 | 3 | 3 | 2 | 2 | 5 | 4 | 8 | 11 | 4 | 3 | 8 | 24 | 4 | 5 | 4 | 30 | 4 | 10 | 18 | 3 | 18 | 18 | 15 | 14 | 9 | 24 | 21 | 20 | 20 | 20 | 20 |
| **Stone Thrower** | 1 | 2 | 1 | 2 | 2 | 1 | 1 | 3 | 2 | 3 | 4 | 2 | 2 | 4 | 12 | 2 | 3 | 2 | 15 | 2 | 5 | 9 | 2 | 9 | 9 | 8 | 7 | 4 | 10 | 9 | 10 | 10 | 10 | 10 |
| **Catapult** | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 2 | 2 | 3 | 3 | 2 | 2 | 3 | 9 | 2 | 2 | 2 | 11 | 2 | 4 | 7 | 2 | 7 | 7 | 6 | 5 | 3 | 8 | 7 | 8 | 8 | 8 | 8 |
| **Ballista** | 1 | 2 | 2 | 2 | 2 | 2 | 2 | 3 | 3 | 5 | 6 | 3 | 3 | 6 | 17 | 3 | 4 | 3 | 25 | 3 | 8 | 14 | 3 | 14 | 14 | 12 | 11 | 7 | 20 | 18 | 16 | 16 | 16 | 16 |

## Elevation

Each fighter's attack against no armour, by ground.

| Attacker | Uphill | Level | Downhill |
|---|---|---|---|
| Villager | 2 | 3 | 4 |
| Scout | 2 | 2 | 3 |
| Clubman | 2 | 3 | 4 |
| Axeman | 4 | 5 | 6 |
| Spearman | 3 | 4 | 5 |
| Slinger | 3 | 4 | 5 |
| Bowman | 4 | 5 | 6 |
| Light Cavalry | 5 | 7 | 9 |
| Swordsman | 6 | 8 | 10 |
| Hoplite | 9 | 12 | 15 |
| Legionary | 12 | 16 | 20 |
| Chariot Archer | 5 | 6 | 8 |
| Horse Archer | 5 | 7 | 9 |
| Heavy Cavalry | 9 | 12 | 15 |
| War Elephant | 15 | 20 | 25 |
| Stone Thrower | 30 | 40 | 50 |
| Catapult | 41 | 55 | 69 |
| Ballista | 23 | 30 | 38 |

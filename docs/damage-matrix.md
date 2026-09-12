# Damage matrix

Generated from the kinds table by the command below; `scripts/check-generated.sh` fails if this file drifts from it. Do not edit by hand.

```sh
cargo run -p simrunner -- matrix --out docs/damage-matrix.md
```

The rule (`docs/02` §8, `GD-COMBAT-01` to `GD-COMBAT-05`):

```text
damage = max(1, (attack × elevation) − armour_of_matching_type + bonus_vs_class)
```

Elevation is ×1.25 attacking downhill and ×0.75 uphill, rounded to nearest with halves up. Melee hits meet melee armour, pierce hits meet pierce armour, siege hits meet no armour and land on friends in the way too. Nothing does less than 1.

## Kinds

| Kind | Class | HP | Attack | Range | Armour (melee/pierce) | Bonus |
|---|---|---|---|---|---|---|
| Villager | villagers | 25 | 3 melee | hand | 0/0 | — |
| Scout | cavalry | 45 | 2 melee | hand | 0/0 | — |
| Clubman | infantry | 40 | 3 melee | hand | 0/0 | — |
| Axeman | infantry | 50 | 5 melee | hand | 0/0 | — |
| Spearman | infantry | 45 | 4 melee | hand | 0/1 | +6 vs cavalry |
| Slinger | archers | 40 | 4 pierce | 4 | 0/0 | +4 vs infantry |
| Bowman | archers | 40 | 5 pierce | 5 | 0/0 | — |
| Light Cavalry | cavalry | 90 | 7 melee | hand | 0/0 | — |
| Town Center | buildings | 600 | — | — | 0/0 | — |
| House | buildings | 75 | — | — | 0/0 | — |
| Storehouse | buildings | 200 | — | — | 0/0 | — |
| Barracks | buildings | 350 | — | — | 0/0 | — |
| Farm | buildings | 60 | — | — | 0/0 | — |
| Archery Range | buildings | 350 | — | — | 0/0 | — |
| Stable | buildings | 350 | — | — | 0/0 | — |
| Market | buildings | 300 | — | — | 0/0 | — |
| Watch Tower | buildings | 250 | — | — | 0/0 | — |
| Temple | buildings | 400 | — | — | 0/0 | — |
| Academy | buildings | 400 | — | — | 0/0 | — |
| Siege Workshop | buildings | 400 | — | — | 0/0 | — |
| Government Centre | buildings | 400 | — | — | 0/0 | — |

## Damage per hit, level ground, no technology

Rows attack columns.

| Attacker \ Target | Villager | Scout | Clubman | Axeman | Spearman | Slinger | Bowman | Light Cavalry | Town Center | House | Storehouse | Barracks | Farm | Archery Range | Stable | Market | Watch Tower | Temple | Academy | Siege Workshop | Government Centre |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Villager** | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 |
| **Scout** | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 2 |
| **Clubman** | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 3 |
| **Axeman** | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 |
| **Spearman** | 4 | 10 | 4 | 4 | 4 | 4 | 4 | 10 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| **Slinger** | 4 | 4 | 8 | 8 | 7 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| **Bowman** | 5 | 5 | 5 | 5 | 4 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 |
| **Light Cavalry** | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 | 7 |

## Hits to kill, level ground, no technology

| Attacker \ Target | Villager | Scout | Clubman | Axeman | Spearman | Slinger | Bowman | Light Cavalry | Town Center | House | Storehouse | Barracks | Farm | Archery Range | Stable | Market | Watch Tower | Temple | Academy | Siege Workshop | Government Centre |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Villager** | 9 | 15 | 14 | 17 | 15 | 14 | 14 | 30 | 200 | 25 | 67 | 117 | 20 | 117 | 117 | 100 | 84 | 134 | 134 | 134 | 134 |
| **Scout** | 13 | 23 | 20 | 25 | 23 | 20 | 20 | 45 | 300 | 38 | 100 | 175 | 30 | 175 | 175 | 150 | 125 | 200 | 200 | 200 | 200 |
| **Clubman** | 9 | 15 | 14 | 17 | 15 | 14 | 14 | 30 | 200 | 25 | 67 | 117 | 20 | 117 | 117 | 100 | 84 | 134 | 134 | 134 | 134 |
| **Axeman** | 5 | 9 | 8 | 10 | 9 | 8 | 8 | 18 | 120 | 15 | 40 | 70 | 12 | 70 | 70 | 60 | 50 | 80 | 80 | 80 | 80 |
| **Spearman** | 7 | 5 | 10 | 13 | 12 | 10 | 10 | 9 | 150 | 19 | 50 | 88 | 15 | 88 | 88 | 75 | 63 | 100 | 100 | 100 | 100 |
| **Slinger** | 7 | 12 | 5 | 7 | 7 | 10 | 10 | 23 | 150 | 19 | 50 | 88 | 15 | 88 | 88 | 75 | 63 | 100 | 100 | 100 | 100 |
| **Bowman** | 5 | 9 | 8 | 10 | 12 | 8 | 8 | 18 | 120 | 15 | 40 | 70 | 12 | 70 | 70 | 60 | 50 | 80 | 80 | 80 | 80 |
| **Light Cavalry** | 4 | 7 | 6 | 8 | 7 | 6 | 6 | 13 | 86 | 11 | 29 | 50 | 9 | 50 | 50 | 43 | 36 | 58 | 58 | 58 | 58 |

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

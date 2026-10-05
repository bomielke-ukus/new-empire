#!/usr/bin/env python3
"""Writes the learning campaign's scenario files (assets/campaigns/learning).

Each map is drawn on a 64-tile square: the Nile along the high-x edge, the
black land beside it, the red desert on the low-x side.
"""
import math
import os
import sys

N = 64
OUT = sys.argv[1] if len(sys.argv) > 1 else "assets/campaigns/learning"


def h(x, y, s=0):
    """A deterministic value in [0, 1)."""
    v = (x * 374761393 + y * 668265263 + s * 2147483647) & 0xFFFFFFFF
    v = ((v ^ (v >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((v ^ (v >> 16)) & 0xFFFF) / 65536.0


def smooth(x, y, scale, s):
    """Value noise, smooth at `scale` tiles."""
    gx, gy = x / scale, y / scale
    x0, y0 = int(math.floor(gx)), int(math.floor(gy))
    fx, fy = gx - x0, gy - y0
    fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    a, b = h(x0, y0, s), h(x0 + 1, y0, s)
    c, d = h(x0, y0 + 1, s), h(x0 + 1, y0 + 1, s)
    return (a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy


def nile(seed, river=52, desert=16, sweep=3.0):
    """Rows of letters: desert, dirt, grass, the bank, the river."""
    grid = [["g"] * N for _ in range(N)]
    for y in range(N):
        bend = sweep * math.sin(y / 11.0 + seed)
        bank = int(round(river + bend))
        edge = int(round(desert + 4 * (smooth(0, y, 7, seed) - 0.5) * 2))
        for x in range(N):
            if x >= bank + 4:
                t = "W"
            elif x >= bank + 2:
                t = "w"
            elif x >= bank:
                t = "s"
            elif x < edge:
                t = "a"
            elif x < edge + 2:
                t = "d"
            else:
                t = "g"
                if smooth(x, y, 5, seed + 3) > 0.72:
                    t = "d"
            grid[y][x] = t
    return grid


def forest(grid, cells):
    for x, y in cells:
        if 0 <= x < N and 0 <= y < N and grid[y][x] in "gd":
            grid[y][x] = "f"


def blob(cx, cy, rx, ry, seed, fill=0.85):
    out = []
    for y in range(cy - ry, cy + ry + 1):
        for x in range(cx - rx, cx + rx + 1):
            d = ((x - cx) / max(rx, 1)) ** 2 + ((y - cy) / max(ry, 1)) ** 2
            if d <= 1.0 and h(x, y, seed) < fill + 0.3 * (1 - d) - 0.3:
                out.append((x, y))
    return out


def bank_palms(grid, seed, ys, density=0.35):
    """Palms along the river's sand edge."""
    out = []
    for y in ys:
        row = grid[y]
        sand = row.index("s") if "s" in row else None
        if sand is None:
            continue
        for x in (sand - 1, sand - 2):
            if h(x, y, seed) < density:
                out.append((x, y))
    return out


def heights(grid, seed):
    """Corner heights: the land by the river flat at 0, the desert rising
    in low dunes; neighbours never more than one apart."""
    H = [[0] * (N + 1) for _ in range(N + 1)]
    for cy in range(N + 1):
        for cx in range(N + 1):
            near = [
                grid[ty][tx]
                for ty in (cy - 1, cy)
                for tx in (cx - 1, cx)
                if 0 <= tx < N and 0 <= ty < N
            ]
            if any(t not in "a" for t in near):
                H[cy][cx] = 0
            else:
                H[cy][cx] = int(smooth(cx, cy, 6, seed + 9) * 3.2)
    # Relax until neighbours are at most one apart.
    changed = True
    while changed:
        changed = False
        for cy in range(N + 1):
            for cx in range(N + 1):
                for dx, dy in ((1, 0), (0, 1), (1, 1), (1, -1)):
                    nx, ny = cx + dx, cy + dy
                    if 0 <= nx <= N and 0 <= ny <= N:
                        a, b = H[cy][cx], H[ny][nx]
                        if a > b + 1:
                            H[cy][cx] = b + 1
                            changed = True
                        elif b > a + 1:
                            H[ny][nx] = a + 1
                            changed = True
    return ["".join(str(v) for v in row) for row in H]


def q(s):
    assert '"' not in s and "\\" not in s, s
    return '"' + s + '"'


def ron_list(items, indent="    "):
    if not items:
        return "[]"
    return "[\n" + "".join(f"{indent}    {i},\n" for i in items) + f"{indent}]"


def place(kind, owner, at, count=1, tag=None):
    s = f"(kind: {q(kind)}, owner: {owner}, at: ({at[0]}, {at[1]})"
    if count != 1:
        s += f", count: {count}"
    if tag:
        s += f", tag: Some({q(tag)})"
    return s + ")"


def objective(oid, text, goal, hidden=False, optional=False):
    s = f"(id: {q(oid)}, text: {q(text)}, goal: {goal}"
    if hidden:
        s += ", hidden: true"
    if optional:
        s += ", optional: true"
    return s + ")"


def trigger(then, when=(), tid=None, repeat=False):
    parts = []
    if tid:
        parts.append(f"id: Some({q(tid)})")
    if when:
        parts.append("when: [" + ", ".join(when) + "]")
    parts.append("then: [" + ", ".join(then) + "]")
    if repeat:
        parts.append("repeat: true")
    return "(" + ", ".join(parts) + ")"


def say(text):
    return f"Say({q(text)})"


def side(name, control, civ="Egyptians", age="Stone", stockpile=None, techs=()):
    s = f"(name: {q(name)}, control: {control}, civ: Some({civ}), age: {age}"
    if stockpile is not None:
        s += f", stockpile: ({', '.join(str(v) for v in stockpile)})"
    if techs:
        s += ", techs: [" + ", ".join(q(t) for t in techs) + "]"
    return s + ")"


def scenario(name, title, briefing, seed, grid, hs, sides, placements, objectives, triggers):
    text = "// The learning campaign (docs/03 §7, docs/07 D35). Drawn with the Nile\n"
    text += "// along the right-hand edge; edit freely, it is checked when read.\n"
    text += "(\n"
    text += f"    title: {q(title)},\n"
    text += "    briefing: [\n" + "".join(f"        {q(p)},\n" for p in briefing) + "    ],\n"
    text += f"    seed: Some({seed}),\n"
    text += "    map: Drawn((\n        terrain: [\n"
    text += "".join(f'            "{"".join(r)}",\n' for r in grid)
    text += "        ],\n        heights: [\n"
    text += "".join(f'            "{r}",\n' for r in hs)
    text += "        ],\n    )),\n"
    text += "    sides: " + ron_list(sides) + ",\n"
    text += "    placements: " + ron_list(placements) + ",\n"
    text += "    objectives: " + ron_list(objectives) + ",\n"
    text += "    triggers: " + ron_list(triggers) + ",\n"
    text += ")\n"
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, name + ".ron"), "w") as f:
        f.write(text)
    return grid


def clear(grid, x0, y0, x1, y1, to="g"):
    """Clears trees from a rectangle so a placement has room."""
    for y in range(y0, y1 + 1):
        for x in range(x0, x1 + 1):
            if grid[y][x] == "f":
                grid[y][x] = to


def check_land(grid, at, size=1):
    for y in range(at[1], at[1] + size):
        for x in range(at[0], at[0] + size):
            assert grid[y][x] in "gda", (at, x, y, grid[y][x])


# ---------------------------------------------------------------------------
# 1. Hunters on the Bank: gathering and building.

def hunters():
    seed = 11
    g = nile(seed, river=52, desert=14)
    near = [(36, 25), (37, 25), (38, 25), (36, 26), (37, 26), (38, 26), (37, 24), (39, 25)]
    forest(g, blob(45, 8, 4, 5, seed))
    forest(g, blob(42, 50, 4, 4, seed + 1))
    forest(g, bank_palms(g, seed, range(18, 46)))
    clear(g, 27, 27, 37, 36)
    clear(g, 24, 38, 30, 42)
    clear(g, 41, 14, 47, 18)
    forest(g, near)
    tc = (31, 29)
    check_land(g, tc, 3)
    p = [
        place("Town Center", 0, tc, tag="tc"),
        place("Villager", 0, (34, 33), 3),
        place("Berry Bush", 255, (25, 39), 4),
        place("Berry Bush", 255, (25, 40), 3),
        place("Gazelle", 255, (19, 27), 4),
        place("Gazelle", 255, (20, 46), 3),
    ]
    objs = [
        objective("wood", "Gather 100 wood", "Stockpile(resource: Wood, amount: 100)"),
        objective("house", "Build a house", 'Have(kind: "House", count: 1)', hidden=True),
        objective("people", "Have 7 villagers", 'Have(kind: "Villager", count: 7)', hidden=True),
        objective(
            "store",
            "Build a storehouse by the forest",
            'Have(kind: "Storehouse", count: 1)',
            hidden=True,
        ),
        objective("food", "Gather 300 food", "Stockpile(resource: Food, amount: 300)", hidden=True),
    ]
    trig = [
        trigger([say(
            "Your village has three villagers. Left click one to choose her, then right click a "
            "tree by the Town Center: she will cut wood and carry it home."
        )]),
        trigger([say(
            "Drag a box around villagers to choose several at once. Every villager you set to "
            "work makes the village richer."
        )], when=["After(25)"]),
        trigger(["Show(\"house\")", say(
            "A Town Center shelters five people. Each house shelters five more. Choose a "
            "villager, pick House on her panel, and click open ground to lay it out."
        )], when=['Done("wood")']),
        trigger(["Show(\"people\")", say(
            "Now the village can grow. Choose the Town Center and train villagers: each costs "
            "50 food. Set them to work as they come out."
        )], when=['Done("house")']),
        trigger(["Show(\"store\")", say(
            "The small grove will not last. A forest stands upriver, far from the Town Center. "
            "Build a storehouse beside it: villagers leave their loads at the nearest one."
        ), 'Reveal((from: (40, 3), to: (50, 15)))'], when=['Done("people")']),
        trigger(["Show(\"food\")", say(
            "Berry bushes grow toward the desert, and gazelles graze at its edge. Villagers "
            "pick berries, and hunt a gazelle before they carry its meat home."
        ), 'Reveal((from: (17, 24), to: (30, 44)))'], when=['Done("store")']),
        trigger([say(
            "When a villager has nothing to do, the idle count at the top of the screen shows "
            "it, and the next idle villager key finds her. F1 lists the keys."
        )], when=["After(240)"]),
    ]
    return scenario(
        "hunters",
        "Hunters on the Bank",
        [
            "Naqada, about 3500 BC. Long before there were kings, the people of the Nile lived "
            "in villages of reed and mud along the river, hunting in the desert and gathering "
            "what grew by the water.",
            "One such village stands on the west bank. Gather wood and food, build where your "
            "people live, and the village will grow.",
        ],
        3501,
        g,
        heights(g, seed),
        [side("Naqada", "Player", stockpile=(200, 0, 0, 0))],
        p,
        objs,
        trig,
    )


# ---------------------------------------------------------------------------
# 2. The Black Land: advancing an age.

def black_land():
    seed = 23
    g = nile(seed, river=50, desert=15)
    forest(g, blob(44, 14, 3, 6, seed))
    forest(g, blob(43, 44, 3, 5, seed + 1))
    forest(g, blob(22, 12, 3, 3, seed + 2))
    forest(g, bank_palms(g, seed, range(4, 60), 0.3))
    clear(g, 26, 26, 38, 38)
    tc = (30, 30)
    check_land(g, tc, 3)
    p = [
        place("Town Center", 0, tc, tag="tc"),
        place("House", 0, (26, 27)),
        place("Villager", 0, (34, 34), 5),
        place("Berry Bush", 255, (23, 36), 4),
        place("Berry Bush", 255, (37, 22), 4),
        place("Gazelle", 255, (19, 24), 4),
        place("Gazelle", 255, (21, 44), 4),
        place("Gazelle", 255, (12, 34), 3),
    ]
    objs = [
        objective("store", "Build a storehouse", 'Have(kind: "Storehouse", count: 1)'),
        objective("barracks", "Build a barracks", 'Have(kind: "Barracks", count: 1)'),
        objective("tool", "Advance to the Tool Age", "Age(Tool)"),
        objective("farms", "Build 3 farms", 'Have(kind: "Farm", count: 3)', hidden=True),
        objective("market", "Build a market", 'Have(kind: "Market", count: 1)', optional=True, hidden=True),
    ]
    trig = [
        trigger([say(
            "A town advances to a new age at its Town Center. It needs 400 food, and two "
            "buildings of the age it is in, besides its houses. A storehouse and a barracks "
            "will do."
        )]),
        trigger([say(
            "Both stand. Gather 400 food, choose the Town Center, and begin the advance to the "
            "Tool Age. Your people can keep working while it is under way."
        )], when=['Done("store")', 'Done("barracks")']),
        trigger(["Show(\"farms\")", 'Show("market")', say(
            "The Tool Age. New buildings are on the villagers' panel. Farms sow the black land: "
            "they cost wood, and are sown again for more wood when they run out."
        )], when=['Done("tool")']),
    ]
    return scenario(
        "black-land",
        "The Black Land",
        [
            "Nekhen, about 3300 BC. The Egyptians called their country Kemet, the black land, "
            "after the dark mud the flood left on the fields each year. The desert beyond was "
            "the red land.",
            "At Nekhen the smiths have learned to work copper. For the town to make tools of it, "
            "it must grow into a new age.",
        ],
        3302,
        g,
        heights(g, seed),
        [side("Nekhen", "Player", stockpile=(150, 250, 0, 0))],
        p,
        objs,
        trig,
    )


# ---------------------------------------------------------------------------
# 3. Raiders from the West: combat.

def raiders():
    seed = 37
    g = nile(seed, river=54, desert=24, sweep=2.0)
    forest(g, blob(48, 12, 3, 5, seed))
    forest(g, blob(47, 52, 3, 5, seed + 1))
    forest(g, bank_palms(g, seed, range(20, 44), 0.3))
    clear(g, 34, 24, 48, 42)
    tc = (40, 31)
    camp = (6, 8)
    check_land(g, tc, 3)
    p = [
        place("Town Center", 0, tc, tag="tc"),
        place("House", 0, (36, 27), 2),
        place("Barracks", 0, (36, 36)),
        place("Storehouse", 0, (45, 16)),
        place("Villager", 0, (44, 30), 4),
        place("Villager", 0, (45, 19), 3),
        place("Clubman", 0, (39, 35), 2),
        place("Berry Bush", 255, (42, 39), 4),
        place("Gazelle", 255, (30, 46), 4),
        # The camp in the desert.
        place("Barracks", 2, camp),
        place("House", 2, (10, 6), 2),
        place("House", 2, (5, 12)),
        place("Clubman", 2, (10, 11), 4),
    ]
    raid_from = (2, 38)
    objs = [
        objective("army", "Have 5 clubmen", 'Have(kind: "Clubman", count: 5)'),
        objective("raid", "Drive off the raiders", "Scripted", hidden=True),
        objective("camp", "Destroy the Tjehenu camp", "Destroy(owner: 2)", hidden=True),
        objective("tc", "Keep the Town Center standing", "Scripted"),
    ]
    trig = [
        trigger([say(
            "Choose the barracks and train clubmen. They cost 50 food each, so keep villagers "
            "on the berries and the hunt."
        )]),
        trigger(
            [
                'Show("raid")',
                say(
                    "Raiders come out of the desert! Drag a box around your soldiers, then "
                    "right click an enemy to attack it. Villagers run to the Town Center on "
                    "their own."
                ),
                place("Clubman", 1, raid_from, 4).replace("(kind", "Place((kind", 1) + ")",
                f"Attack(owner: 1, to: ({tc[0] + 1}, {tc[1] + 1}))",
            ],
            # When the army is ready, or at five minutes.
            when=['Any([Done("army"), After(300)])'],
            tid="wave1",
        ),
        trigger(
            [
                'Complete("raid")',
                'Show("camp")',
                say(
                    "They are beaten. Their camp lies in the desert beyond the dunes. Gather "
                    "your soldiers and choose ATTACK MOVE on their panel, then click near the "
                    "camp: they will fight whatever they meet on the way."
                ),
                f'Reveal((from: ({camp[0] - 2}, {camp[1] - 3}), to: ({camp[0] + 8}, {camp[1] + 7})))',
            ],
            when=['Fired("wave1")', "HasAtMost(owner: 1, count: 0)"],
            tid="beaten",
        ),
        trigger(
            [
                say("More raiders! They are making for the Town Center."),
                place("Clubman", 1, raid_from, 3).replace("(kind", "Place((kind", 1) + ")",
                place("Scout", 1, (4, 44), 2).replace("(kind", "Place((kind", 1) + ")",
                f"Attack(owner: 1, to: ({tc[0] + 1}, {tc[1] + 1}))",
            ],
            when=['Fired("beaten")', "After(600)"],
        ),
        trigger(['Complete("tc")'], when=['Done("camp")']),
        trigger(
            ['Fail("tc")', 'Lose("The Town Center has fallen")'],
            when=['HasAtMost(owner: 0, kind: Some("Town Center"), count: 0)'],
        ),
    ]
    return scenario(
        "raiders",
        "Raiders from the West",
        [
            "Thinis, about 3150 BC. West of the Nile lies the desert, and in it the Tjehenu "
            "herd their flocks between the oases. In lean years they come down to the river to "
            "take what the farmers have grown.",
            "Our scouts have seen their fires. Train soldiers at the barracks, meet the "
            "raiders, then carry the fight to their camp.",
        ],
        3150,
        g,
        heights(g, seed),
        [
            side("Thinis", "Player", stockpile=(300, 150, 0, 0)),
            side("Tjehenu raiders", "Scripted"),
            side("Tjehenu camp", "Scripted"),
        ],
        p,
        objs,
        trig,
    )


# ---------------------------------------------------------------------------
# 4. Spears Against Horses: counters.

def horses():
    seed = 41
    g = nile(seed, river=52, desert=12, sweep=2.5)
    forest(g, blob(44, 40, 3, 5, seed))
    forest(g, blob(20, 52, 3, 3, seed + 1))
    forest(g, blob(46, 22, 3, 4, seed + 2))
    forest(g, bank_palms(g, seed, range(26, 62), 0.3))
    clear(g, 26, 44, 44, 60)
    clear(g, 24, 2, 44, 14)
    tc = (34, 51)
    fort = (30, 6)
    check_land(g, tc, 3)
    p = [
        place("Town Center", 0, tc, tag="tc"),
        place("House", 0, (30, 47), 3),
        place("House", 0, (36, 57), 3),
        place("Barracks", 0, (38, 46)),
        place("Archery Range", 0, (29, 55)),
        place("Storehouse", 0, (40, 41)),
        place("Villager", 0, (38, 52), 6),
        place("Villager", 0, (41, 44), 4),
        place("Spearman", 0, (36, 48), 2),
        place("Berry Bush", 255, (24, 52), 4),
        place("Gazelle", 255, (16, 44), 4),
        place("Stone Vein", 255, (18, 57), 2),
        place("Gold Vein", 255, (22, 40), 2),
        # Avaris, downriver.
        place("Barracks", 2, fort),
        place("Stable", 2, (36, 6)),
        place("House", 2, (30, 10), 3),
        place("Watch Tower", 2, (34, 12)),
        place("Axeman", 2, (32, 14), 3),
        place("Light Cavalry", 2, (37, 9), 2),
        place("Bowman", 2, (34, 4), 2),
    ]
    objs = [
        objective("spears", "Have 6 spearmen", 'Have(kind: "Spearman", count: 6)'),
        objective("horses", "Beat the Hyksos horsemen", "Scripted", hidden=True),
        objective("slings", "Have 6 slingers", 'Have(kind: "Slinger", count: 6)', hidden=True),
        objective("foot", "Beat the Hyksos foot soldiers", "Scripted", hidden=True),
        objective("avaris", "Destroy the Hyksos at Avaris", "Destroy(owner: 2)", hidden=True),
        objective("tc", "Keep the Town Center standing", "Scripted"),
    ]
    tc_mid = f"({tc[0] + 1}, {tc[1] + 1})"
    # Out of sight of Avaris, whose tower and guards are another side.
    from_north = (26, 28)
    trig = [
        trigger([say(
            "The Hyksos fight from horseback, and a horseman rides down a clubman. A spearman "
            "is the answer: a long spear does six more damage to anything on a horse. Train "
            "spearmen at the barracks."
        )]),
        trigger(
            [
                'Show("horses")',
                say(
                    "Horsemen! Keep your spearmen together and set them on the riders. A "
                    "soldier's panel, and the tip on its training button, say what it beats "
                    "and what beats it."
                ),
                place("Light Cavalry", 1, from_north, 3).replace("(kind", "Place((kind", 1) + ")",
                f"Attack(owner: 1, to: {tc_mid})",
            ],
            when=['Any([Done("spears"), After(300)])'],
            tid="riders",
        ),
        trigger(
            [
                'Complete("horses")',
                'Show("slings")',
                say(
                    "The riders are beaten. Now their foot soldiers march, axemen who would cut "
                    "your spearmen down. A slinger does four more damage to a man on foot, and "
                    "strikes from a distance. Train slingers at the archery range: they cost "
                    "stone."
                ),
            ],
            when=['Fired("riders")', "HasAtMost(owner: 1, count: 0)"],
            tid="riders_beaten",
        ),
        trigger(
            [
                'Show("foot")',
                say(
                    "Axemen! Let them come to your slingers, and keep your spearmen in front "
                    "to hold them."
                ),
                place("Axeman", 1, from_north, 5).replace("(kind", "Place((kind", 1) + ")",
                f"Attack(owner: 1, to: {tc_mid})",
            ],
            when=['Fired("riders_beaten")', 'Any([Done("slings"), After(720)])'],
            tid="march",
        ),
        trigger(
            [
                'Complete("foot")',
                'Show("avaris")',
                say(
                    "Their army is broken. Avaris lies downriver: horsemen, axemen and archers "
                    "behind a watch tower. Send each of your soldiers against what it beats, "
                    "and drive the Hyksos out of Egypt."
                ),
                f'Reveal((from: ({fort[0] - 3}, {fort[1] - 4}), to: ({fort[0] + 12}, {fort[1] + 10})))',
            ],
            when=['Fired("march")', "HasAtMost(owner: 1, count: 0)"],
        ),
        trigger(['Complete("tc")'], when=['Done("avaris")']),
        trigger(
            ['Fail("tc")', 'Lose("The Town Center has fallen")'],
            when=['HasAtMost(owner: 0, kind: Some("Town Center"), count: 0)'],
        ),
    ]
    return scenario(
        "horses",
        "Spears Against Horses",
        [
            "Thebes, about 1550 BC. Two thousand years after the first villages, Egypt is old, "
            "and divided. The Hyksos, kings from Canaan, rule the north from Avaris. They "
            "brought the horse into Egypt, and the Egyptians have learned to fear it.",
            "King Ahmose of Thebes means to drive them out. Every soldier is strong against "
            "some and weak against others: choose the right one for each enemy.",
        ],
        1550,
        g,
        heights(g, seed),
        [
            side("Thebes", "Player", age="Tool", stockpile=(400, 300, 150, 50)),
            side("Hyksos riders", "Scripted", civ="Phoenicians", age="Tool"),
            side("Avaris", "Scripted", civ="Phoenicians", age="Tool"),
        ],
        p,
        objs,
        trig,
    )


hunters()
black_land()
raiders()
horses()
with open(os.path.join(OUT, "campaign.ron"), "w") as f:
    f.write(
        "// The learning campaign: one idea a scenario (docs/03 §7, docs/07 D35).\n"
        "(\n"
        '    title: "The Gift of the River",\n'
        '    about: "Learn to gather, build, advance, fight and choose your soldiers.",\n'
        "    order: 0,\n"
        '    scenarios: ["hunters", "black-land", "raiders", "horses"],\n'
        ")\n"
    )
print("wrote", OUT)

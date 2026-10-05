#!/usr/bin/env python3
"""Writes the Sargon of Akkad campaign (assets/campaigns/sargon)."""
import math
import sys

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from scenlib import (Grid, area, campaign, heights, lose, objective, place, placed,
                     say, side, smooth, trigger, write)

OUT = sys.argv[1] if len(sys.argv) > 1 else "assets/campaigns/sargon"
NOTE = ("Sargon of Akkad, played as the Akkadians (docs/07 D35). First drawn by a "
        "script; edit freely, it is checked when read.")
AKKAD = "Assyrians"
SUMER = "Sumerians"


def river(g, pts, width, seed):
    """A river: deep in the middle, shallow at the edges, sand banks."""
    g.line(pts, width + 2, "w")
    g.line(pts, max(1, width - 1), "W")


def walls_round(cx, cy, r, gates, owner, g):
    """A square ring of stone wall about (cx, cy), with gates."""
    out = []
    for i in range(-r, r + 1):
        for (x, y) in [(cx + i, cy - r), (cx + i, cy + r), (cx - r, cy + i), (cx + r, cy + i)]:
            if g.get(x, y) not in ("g", "d", "a", "s"):
                continue
            if (x, y) in gates:
                out.append(place("Gate", owner, (x, y)))
            else:
                out.append(place("Stone Wall", owner, (x, y)))
    # Corners come twice; keep one of each.
    seen, uniq = set(), []
    for p in out:
        if p not in seen:
            seen.add(p)
            uniq.append(p)
    return uniq


def sargon_lives():
    return [
        objective("sargon", "Sargon must live", "Scripted"),
    ]


def sargon_triggers(done_when):
    return [
        trigger([say("Keep Sargon out of the fighting: if the king falls, Akkad falls. He "
                     "can shelter in a Town Center or a tower.")], when=["After(10)"]),
        trigger(['Fail("sargon")', lose("Sargon has fallen, and Akkad with him")],
                when=['Gone("sargon")']),
        trigger(['Complete("sargon")'], when=done_when),
    ]


# ---------------------------------------------------------------------------
# 1. The Cupbearer: found Agade and grow it while Kish sends men.

def cupbearer():
    n, seed = 80, 2334
    g = Grid(n, "d")
    river(g, [(28, 0), (24, 20), (18, 40), (10, 60), (6, 80)], 6, seed)
    # The black land along the river, the desert beyond.
    g.where(lambda x, y: g.get(x, y) == "d" and x > 52 + (smooth(x, y, 8, seed) - 0.5) * 12, "a")
    g.where(lambda x, y: g.get(x, y) == "d" and smooth(x, y, 6, seed + 1) > 0.5, "g")
    g.shores()
    for cx, cy in [(30, 30), (24, 52), (34, 12), (20, 66)]:
        g.blob(cx, cy, 4, 5, "f", seed + cx, fill=0.8)
    g.clear(40, 34, 60, 56)
    g.clear(46, 2, 64, 16)
    hs = heights(g, lambda cx, cy: 1.8 * max(0.0, (cx - 60) / 20) * smooth(cx, cy, 6, seed + 2) * 2)
    for x, y, w, h_ in [(52, 6, 3, 3), (60, 50, 1, 1)]:
        g.land(x, y, w, h_)
    p = [
        # Agade: a town on the river, Sargon's now.
        place("Town Center", 0, (49, 43), tag="agade"),
        place("House", 0, (44, 48), 2),
        place("Swordsman", 0, (54, 48), tag="sargon"),
        place("Villager", 0, (53, 44), 7),
        place("Clubman", 0, (55, 50), 3),
        place("Berry Bush", 255, (44, 44), 4),
        place("Berry Bush", 255, (44, 45), 3),
        place("Gazelle", 255, (66, 40), 4),
        place("Gazelle", 255, (68, 60), 4),
        place("Stone Vein", 255, (64, 30), 2),
        place("Gold Vein", 255, (70, 48), 2),
        # Kish, Ur-Zababa's city, up the river.
        place("Town Center", 1, (52, 6)),
        place("House", 1, (46, 4), 2),
        place("House", 1, (58, 12), 2),
        place("Barracks", 1, (56, 4)),
    ]
    objs = [
        objective("barracks", "Build a barracks for Agade's own army", 'Have(kind: "Barracks", count: 1)'),
        objective("people", "Grow Agade to 15 villagers", 'Have(kind: "Villager", count: 15)'),
        objective("tool", "Advance to the Tool Age", "Age(Tool)"),
    ] + sargon_lives()
    trig = [
        trigger([say(
            "Agade is a small town on the river, and it is Sargon's now. Ur-Zababa will hear "
            "of it soon, and he wanted you dead. Grow the town, and give it an army."
        )], tid="founded"),
        trigger(
            [say("Men of Kish are coming down the river road to burn Agade!"),
             placed("Axeman", 1, (54, 18), 4),
             placed("Slinger", 1, (56, 18), 2),
             "Attack(owner: 1, to: (50, 44))"],
            when=['Since(trigger: "founded", seconds: 240)']),
        trigger(
            [say("Ur-Zababa sends more of his men, and horsemen with them."),
             placed("Axeman", 1, (54, 18), 5),
             placed("Slinger", 1, (56, 18), 3),
             placed("Scout", 1, (58, 18), 2),
             "Attack(owner: 1, to: (50, 44))"],
            when=['Since(trigger: "founded", seconds: 480)']),
        trigger(['Fail("people")', lose("Kish has burned Agade")], when=['Gone("agade")']),
    ] + sargon_triggers(['Done("barracks")', 'Done("people")', 'Done("tool")'])
    write(OUT, "cupbearer", "The Cupbearer", [
        "Mesopotamia, about 2334 BC. The story says that Sargon's mother set him in a basket "
        "of rushes on the Euphrates, and that Aqqi the water-drawer lifted him out and raised "
        "him as a gardener. He rose to be cupbearer to Ur-Zababa, king of Kish.",
        "The king came to fear him, and sent him to Uruk with a tablet asking its king to "
        "kill him. Sargon did not deliver it. Now he and the men who follow him will make a "
        "city of their own.",
    ], 2334, g, hs, [
        side("Akkad", "Player", AKKAD, "Stone", (250, 300, 0, 0)),
        side("Kish", "Scripted", SUMER, "Tool"),
    ], p, objs, trig, NOTE)


# ---------------------------------------------------------------------------
# 2. Lugal-zage-si: Uruk, a city the computer plays.

def uruk():
    n, seed = 96, 2316
    g = Grid(n, "d")
    river(g, [(0, 26), (24, 36), (46, 50), (70, 58), (96, 72)], 6, seed)
    for fx in (22, 62):
        g.rect(fx, 20, fx + 2, 80, "s", over="wW")
    g.where(lambda x, y: g.get(x, y) == "d" and smooth(x, y, 6, seed + 1) > 0.45, "g")
    g.where(lambda x, y: g.get(x, y) == "d" and (x + y) < 30 and smooth(x, y, 5, seed) > 0.5, "a")
    g.shores()
    for cx, cy in [(30, 22), (12, 40), (52, 40), (40, 66), (84, 50), (60, 82)]:
        g.blob(cx, cy, 5, 4, "f", seed + cx, fill=0.8)
    g.clear(6, 4, 30, 22)
    g.clear(64, 70, 92, 94)
    hs = heights(g, lambda cx, cy: 0.0)
    uruk_c = (78, 82)
    walls = walls_round(uruk_c[0], uruk_c[1], 10, {(uruk_c[0], uruk_c[1] - 10), (uruk_c[0] - 10, uruk_c[1])}, 1, g)
    p = [
        place("Town Center", 0, (14, 10), tag="agade"),
        place("House", 0, (8, 6), 4),
        place("House", 0, (20, 16), 3),
        place("Barracks", 0, (22, 6)),
        place("Archery Range", 0, (8, 16)),
        place("Storehouse", 0, (24, 12)),
        place("Villager", 0, (16, 14), 10),
        place("Swordsman", 0, (18, 20), tag="sargon"),
        place("Axeman", 0, (20, 20), 6),
        place("Bowman", 0, (16, 22), 4),
        place("Berry Bush", 255, (4, 22), 4),
        place("Gold Vein", 255, (30, 8), 2),
        place("Stone Vein", 255, (4, 2), 2),
        # Uruk, inside its walls.
        place("Town Center", 1, (77, 81), tag="uruk"),
        place("House", 1, (71, 76), 3),
        place("House", 1, (72, 88), 3),
        place("Barracks", 1, (83, 76)),
        place("Archery Range", 1, (83, 86)),
        place("Watch Tower", 1, (75, 70)),
        place("Watch Tower", 1, (66, 80)),
        place("Villager", 1, (80, 84), 10),
        place("Axeman", 1, (74, 84), 6),
        place("Berry Bush", 255, (88, 92), 4),
        place("Gold Vein", 255, (90, 78), 2),
    ] + walls
    nippur = (40, 36, 46, 42)
    objs = [
        objective("uruk", "Take Uruk: destroy Lugal-zage-si's Town Center", "Scripted"),
        objective("nippur", "Bring 5 soldiers to the gate of Enlil at Nippur", f"Reach(area: {area(*nippur)}, count: 5)",
                  optional=True),
    ] + sargon_lives()
    trig = [
        trigger([say(
            "Lugal-zage-si of Uruk calls himself king of the land: fifty governors serve him. "
            "Break his walls and take his city."
        ), f"Reveal({area(*nippur)})"]),
        trigger([say(
            "Nippur, Enlil's holy city, is between you and Uruk. Whoever holds the god's gate "
            "is seen to rule by his will."
        )], when=["After(60)"]),
        trigger(['Complete("uruk")'], when=['Gone("uruk")']),
    ] + sargon_triggers(['Done("uruk")'])
    write(OUT, "uruk", "Lugal-zage-si", [
        "Uruk, about 2316 BC. Lugal-zage-si has made himself master of Sumer, from Uruk to "
        "Ur and Lagash and Umma. Kish has fallen to him, and Ur-Zababa is gone.",
        "Agade is young, but its army is Sargon's own. It is time to see which of the two "
        "kings the land will follow.",
    ], 2316, g, hs, [
        side("Akkad", "Player", AKKAD, "Tool", (500, 500, 200, 200)),
        side("Uruk", "Computer(1)", SUMER, "Tool", (400, 400, 200, 200)),
    ], p, objs, trig, NOTE)


# ---------------------------------------------------------------------------
# 3. Washing weapons in the sea: Ur, Lagash, and the gulf.

SHORE = (64, 68, 74, 78)


def gulf():
    n, seed = 96, 2310
    g = Grid(n, "d")
    # The Lower Sea at the south-east.
    g.where(lambda x, y: x + y > 152 + (smooth(x, y, 5, seed) - 0.5) * 8, "W")
    river(g, [(0, 20), (20, 34), (36, 56), (52, 74), (70, 86), (80, 90)], 5, seed)
    river(g, [(60, 0), (66, 20), (78, 40), (84, 60), (90, 70)], 4, seed + 1)
    for fx, fy in [(14, 30), (40, 60), (70, 18), (82, 50)]:
        g.rect(fx - 1, fy - 4, fx + 1, fy + 4, "s", over="wW")
    # Marshes by the river mouth.
    g.where(lambda x, y: g.get(x, y) == "d" and x + y > 130 and smooth(x, y, 3, seed + 2) > 0.62, "w")
    g.where(lambda x, y: g.get(x, y) == "d" and smooth(x, y, 6, seed + 3) > 0.48, "g")
    # The shore where the weapons are washed: firm ground to the water.
    g.rect(SHORE[0], SHORE[1], SHORE[2], SHORE[3], "g", over="w")
    g.shores()
    for cx, cy in [(24, 12), (8, 48), (48, 30), (58, 50), (30, 84)]:
        g.blob(cx, cy, 4, 4, "f", seed + cx, fill=0.8)
    g.clear(2, 2, 22, 18)
    g.clear(24, 66, 46, 88)
    g.clear(58, 28, 80, 46)
    hs = heights(g, lambda cx, cy: 0.0)
    ur_c, lagash_c = (34, 78), (68, 36)
    p = [
        place("Town Center", 0, (8, 6)),
        place("House", 0, (4, 2), 4),
        place("House", 0, (16, 1), 4),
        place("Barracks", 0, (14, 4)),
        place("Archery Range", 0, (4, 12)),
        place("Siege Workshop", 0, (14, 12)),
        place("Villager", 0, (10, 10), 8),
        place("Swordsman", 0, (18, 16), tag="sargon"),
        place("Swordsman", 0, (20, 16), 10),
        place("Bowman", 0, (16, 18), 10),
        place("Spearman", 0, (20, 20), 6),
        place("Stone Thrower", 0, (14, 20), 3),
        place("Berry Bush", 255, (2, 20), 4),
        place("Gold Vein", 255, (24, 4), 2),
        # Ur.
        place("Town Center", 1, (ur_c[0] - 1, ur_c[1] - 1), tag="ur"),
        place("House", 1, (28, 72), 3),
        place("Temple", 1, (38, 82)),
        place("Watch Tower", 1, (29, 75)),
        place("Watch Tower", 1, (39, 73)),
        place("Spearman", 1, (34, 72), 10),
        place("Bowman", 1, (36, 74), 6),
        place("Swordsman", 1, (32, 80), 4),
        # Lagash.
        place("Town Center", 1, (lagash_c[0] - 1, lagash_c[1] - 1), tag="lagash"),
        place("House", 1, (62, 32), 3),
        place("Barracks", 1, (72, 40)),
        place("Watch Tower", 1, (64, 30)),
        place("Watch Tower", 1, (62, 42)),
        place("Axeman", 1, (66, 40), 8),
        place("Bowman", 1, (70, 32), 6),
        place("Swordsman", 1, (72, 36), 4),
    ]
    # Ur's walls are old, and breached on the north-east.
    ring = walls_round(ur_c[0], ur_c[1], 8, {(ur_c[0] - 8, ur_c[1])}, 1, g)
    breach = {f"at: ({x}, {ur_c[1] - 8})" for x in range(ur_c[0] + 2, ur_c[0] + 6)}
    p += [w for w in ring if not any(b in w for b in breach)]
    shore = SHORE
    objs = [
        objective("ur", "Take Ur: destroy its Town Center", "Scripted"),
        objective("lagash", "Take Lagash: destroy its Town Center", "Scripted"),
        objective("sea", "Wash your weapons in the sea: bring 5 soldiers to the shore",
                  f"Reach(area: {area(*shore)}, count: 5)", hidden=True),
    ] + sargon_lives()
    trig = [
        trigger([say(
            "Uruk is yours, but Ur and Lagash still close their gates to you. The stone "
            "throwers from the workshop will break walls and towers that arrows cannot."
        ), f"Reveal({area(ur_c[0] - 9, ur_c[1] - 9, ur_c[0] + 9, ur_c[1] + 9)})",
            f"Reveal({area(lagash_c[0] - 8, lagash_c[1] - 8, lagash_c[0] + 8, lagash_c[1] + 8)})"]),
        trigger(['Complete("ur")'], when=['Gone("ur")']),
        trigger(['Complete("lagash")'], when=['Gone("lagash")']),
        trigger(['Show("sea")', say(
            "Ur and Lagash have fallen. March on to the Lower Sea, and wash your weapons in "
            "it, as a sign that all the land from the river to the sea is Sargon's."
        ), f"Reveal({area(*shore)})"], when=['Done("ur")', 'Done("lagash")']),
        trigger(
            [say("Men of Umma come to help their neighbours, and fall on your camp."),
             placed("Spearman", 1, (48, 6), 6),
             placed("Bowman", 1, (50, 8), 4),
             "Attack(owner: 1, to: (12, 10))"],
            when=["After(300)"]),
    ] + sargon_triggers(['Done("sea")'])
    write(OUT, "gulf", "Washing Weapons in the Sea", [
        "The south, about 2310 BC. Lugal-zage-si has been brought to Nippur in a neck-stock "
        "and shown at the gate of Enlil. But the cities of the south are not yet Akkad's.",
        "Sargon's inscription says he conquered Ur and Lagash and washed his weapons in the "
        "sea. Make it true.",
    ], 2310, g, hs, [
        side("Akkad", "Player", AKKAD, "Bronze", (400, 400, 300, 300), start=(18, 18)),
        side("The cities of Sumer", "Scripted", SUMER, "Bronze"),
    ], p, objs, trig, NOTE)


# ---------------------------------------------------------------------------
# 4. King of the Four Quarters: the great revolt.

def four_quarters():
    n, seed = 80, 2280
    g = Grid(n, "d")
    river(g, [(16, 0), (14, 30), (18, 50), (12, 80)], 5, seed)
    for fy in (12, 62):
        g.rect(10, fy, 22, fy + 2, "s", over="wW")
    g.where(lambda x, y: g.get(x, y) == "d" and smooth(x, y, 6, seed + 1) > 0.45, "g")
    g.shores()
    for cx, cy in [(30, 14), (60, 30), (30, 66), (56, 56), (70, 70)]:
        g.blob(cx, cy, 4, 4, "f", seed + cx, fill=0.8)
    c = (44, 40)
    g.clear(c[0] - 12, c[1] - 12, c[0] + 12, c[1] + 12)
    hs = heights(g, lambda cx, cy: 0.0)
    gates = {(c[0], c[1] - 9), (c[0], c[1] + 9), (c[0] - 9, c[1]), (c[0] + 9, c[1])}
    p = walls_round(c[0], c[1], 9, gates, 0, g)
    p += [
        place("Town Center", 0, (c[0] - 1, c[1] - 1), tag="agade"),
        place("House", 0, (c[0] - 7, c[1] - 7), 4),
        place("House", 0, (c[0] - 7, c[1] + 5), 4),
        place("Barracks", 0, (c[0] + 4, c[1] - 7)),
        place("Archery Range", 0, (c[0] + 4, c[1] + 4)),
        place("Temple", 0, (c[0] + 4, c[1] - 3)),
        place("Watch Tower", 0, (c[0] - 4, c[1] - 4)),
        place("Watch Tower", 0, (c[0] + 3, c[1] + 2)),
        place("Villager", 0, (c[0] - 2, c[1] + 3), 10),
        place("Swordsman", 0, (c[0], c[1] - 4), tag="sargon"),
        place("Swordsman", 0, (c[0] + 2, c[1] - 4), 8),
        place("Bowman", 0, (c[0] - 3, c[1] + 1), 10),
        place("Chariot Archer", 0, (c[0] + 2, c[1] + 6), 4),
        place("Berry Bush", 255, (c[0] - 6, c[1] - 1), 4),
        place("Gold Vein", 255, (60, 44), 2),
        place("Stone Vein", 255, (44, 24), 2),
    ]
    camps = {"Kish": (4, 4), "Uruk": (70, 70), "Ur": (28, 72), "Elam": (70, 4)}
    for name, (x, y) in camps.items():
        p += [place("Barracks", 1, (x, y)), place("House", 1, (x + 3, y), 2)]
    waves = [
        (30, "Kish", [("Axeman", 6), ("Bowman", 3)], "The men of Kish come first, from the north."),
        (120, "Uruk", [("Spearman", 6), ("Bowman", 4)], "Uruk's army comes up from the south-east."),
        (210, "Elam", [("Light Cavalry", 4), ("Bowman", 4)], "Elam's horsemen ride in from the east."),
        (300, "Ur", [("Swordsman", 6), ("Bowman", 4)], "Ur's soldiers come from the south."),
        (390, "Kish", [("Swordsman", 6), ("Spearman", 4)], "Kish again, with more."),
        (480, "Uruk", [("Chariot Archer", 4), ("Swordsman", 4)], "Uruk sends its chariots."),
        (540, "Elam", [("Swordsman", 6), ("Bowman", 4)], "The last of them: Elam's best."),
    ]
    trig = [trigger([say(
        "In his old age all the lands rose against Sargon and shut him up in Agade. Hold the "
        "walls while the rebels wear themselves out on them."
    )])]
    for i, (t, camp, units, line) in enumerate(waves):
        x, y = camps[camp]
        acts = [say(line)] + [placed(k, 1, (x + 2 + 2 * j, y + 4), cnt) for j, (k, cnt) in enumerate(units)]
        acts.append(f"Attack(owner: 1, to: ({c[0]}, {c[1]}))")
        trig.append(trigger(acts, when=[f"After({t})"], tid="last" if i == len(waves) - 1 else None))
    trig += [
        trigger(['Show("camps")', say(
            "The rebels are spent. Sargon went out and beat them, the inscriptions say, and "
            "made their lands his again. Destroy the rebels' camps."
        )] + [f"Reveal({area(x - 2, y - 2, x + 8, y + 6)})" for (x, y) in camps.values()],
            when=['Done("hold")']),
        trigger(['Complete("camps")'], when=['Done("hold")', 'Fired("last")', "HasAtMost(owner: 1, count: 0)"]),
        trigger(['Fail("hold")', lose("Agade has fallen")], when=['Gone("agade")']),
    ] + sargon_triggers(['Done("camps")'])
    objs = [
        objective("hold", "Hold Agade for ten minutes", "Survive(seconds: 600)"),
        objective("camps", "Destroy the rebels' camps", "Scripted", hidden=True),
    ] + sargon_lives()
    write(OUT, "four-quarters", "King of the Four Quarters", [
        "Agade, about 2280 BC. Sargon has ruled for more than fifty years, from the Lower Sea "
        "to the Cedar Forest. Five thousand four hundred men eat bread before him every day.",
        "Now he is old, and the cities he conquered have risen together against him. Kish, "
        "Uruk, Ur and Elam march on Agade from every quarter.",
    ], 2280, g, hs, [
        side("Akkad", "Player", AKKAD, "Bronze", (400, 400, 300, 300)),
        side("The rebels", "Scripted", SUMER, "Bronze"),
    ], p, objs, trig, NOTE)


cupbearer()
uruk()
gulf()
four_quarters()
campaign(OUT, "Sargon of Akkad", "The first empire: Akkad, from a basket on the river to the four quarters.",
         2, ["cupbearer", "uruk", "gulf", "four-quarters"])
print("wrote", OUT)

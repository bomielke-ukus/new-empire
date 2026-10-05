#!/usr/bin/env python3
"""Writes the Persian Wars campaign (assets/campaigns/persian-wars)."""
import math
import sys

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from scenlib import (Grid, area, campaign, heights, lose, objective, place, placed,
                     say, side, smooth, trigger, write)

OUT = sys.argv[1] if len(sys.argv) > 1 else "assets/campaigns/persian-wars"
NOTE = ("The Persian Wars, played as the Greeks (docs/07 D35). First drawn by a "
        "script; edit freely, it is checked when read.")
GREEKS = "Greeks"
PERSIANS = "Persians"


# ---------------------------------------------------------------------------
# 1. Marathon, 490 BC: beat the army on the plain, then race the fleet home.

MARATHON_ATHENS = (6, 72, 20, 88)
MARATHON_PHALERON = (24, 80, 34, 90)


def marathon():
    n, seed = 96, 490
    g = Grid(n, "g")

    def coast_x(y):
        return 84 - 7 * math.exp(-(((y - 34) / 14) ** 2)) + (smooth(0, y, 6, seed) - 0.5) * 3

    def coast_y(x):
        return 91 + (smooth(x, 0, 6, seed + 1) - 0.5) * 3

    g.where(lambda x, y: x >= coast_x(y), "W")
    g.where(lambda x, y: x >= 18 and y >= coast_y(x), "W")
    # Phaleron, Athens' harbour, bites north out of the gulf.
    g.poly([(23, 96), (24, 84), (28, 79), (33, 80), (36, 86), (37, 96)], "W", wobble=1.2, seed=seed + 2)
    # The Great Marsh at the plain's north end.
    g.where(lambda x, y: 62 <= x <= 80 and 3 <= y <= 18 and smooth(x, y, 3, seed + 3) > 0.5, "w", over="g")
    # Dirt and dry ground about the plain and the hills.
    g.where(lambda x, y: smooth(x, y, 7, seed + 4) > 0.62, "d", over="g")
    # Pentelikon and Parnes: forested mountains between the plain and Athens.
    for cx, cy, rx, ry in [(36, 50, 13, 13), (24, 28, 9, 10), (50, 62, 9, 8), (60, 80, 8, 6)]:
        g.blob(cx, cy, rx, ry, "f", seed + cx, fill=0.8)
    # The road from the plain round the mountains to Athens.
    road = [(50, 34), (40, 31), (30, 38), (20, 50), (16, 64), (14, 76)]
    g.line(road, 3, "d", over="gf")
    # The Athenian camp's clearing and the Persian beach.
    g.clear(42, 26, 54, 40, to="g")
    g.clear(64, 26, 80, 46, to="s")
    g.shores()
    hs = heights(g, lambda cx, cy: 3.4 * max(
        0.0,
        1 - math.hypot((cx - 36) / 15, (cy - 50) / 15),
    ) + 2.5 * max(0.0, 1 - math.hypot((cx - 24) / 10, (cy - 28) / 11))
        + 2.0 * max(0.0, 1 - math.hypot((cx - 12) / 5, (cy - 78) / 5))
        + 1.0 * max(0.0, 1 - math.hypot((cx - 44) / 5, (cy - 30) / 5)))
    for x, y, w, h_ in [(10, 76, 3, 3), (46, 30, 1, 1)]:
        g.land(x, y, w, h_)
    athens = MARATHON_ATHENS
    phaleron = MARATHON_PHALERON
    p = [
        # Athens itself, and its people: the city the army must reach.
        place("Town Center", 0, (10, 76), tag="athens"),
        place("House", 0, (4, 72), 5),
        place("House", 0, (15, 82), 2),
        place("Temple", 0, (7, 82)),
        place("Villager", 0, (12, 72), 4),
        # The army at the sanctuary of Heracles.
        place("Hoplite", 0, (47, 30), 14),
        place("Slinger", 0, (45, 36), 4),
        # The Persians on the beach.
        place("Bowman", 1, (72, 32), 20),
        place("Spearman", 1, (68, 37), 12),
        place("Swordsman", 1, (70, 42), 10),
        # Their fleet, moored in the bay.
        place("War Galley", 2, (92, 36), 6),
    ]
    objs = [
        objective("plain", "Defeat the Persian army on the plain", "Scripted"),
        objective(
            "athens",
            "Bring 8 hoplites to Athens before the Persian fleet reaches Phaleron",
            f"Reach(area: {area(*athens)}, kind: Some(\"Hoplite\"), count: 8)",
            hidden=True,
        ),
    ]
    trig = [
        trigger([say(
            "Datis and Artaphernes have landed the King's army at Marathon. Ten thousand "
            "Athenians face them from the hills. Miltiades says: attack."
        )]),
        trigger(
            [say("The Plataeans have come, every man of fighting age, to stand with Athens."),
             placed("Hoplite", 0, (30, 30), 4)],
            when=["After(25)"],
        ),
        trigger(
            [say("The Persians tire of waiting and march on your camp."),
             "Attack(owner: 1, to: (47, 33))"],
            when=["After(300)", 'Not(Fired("won"))'],
        ),
        trigger(
            ['Complete("plain")', 'Show("athens")', say(
                "The Persians break and run for their ships. But their fleet will sail round "
                "Cape Sounion for Athens, and the city has no army. March home, now!"
            )],
            when=["HasAtMost(owner: 1, count: 0)"],
            tid="won",
        ),
        trigger(
            [say("The Persian ships put out from Marathon, making for Phaleron."),
             f"Attack(owner: 2, to: ({(phaleron[0] + phaleron[2]) // 2}, {(phaleron[1] + phaleron[3]) // 2}))"],
            when=['Since(trigger: "won", seconds: 20)'],
        ),
        trigger(
            ['Fail("athens")', lose("The Persian fleet reached Phaleron before the army, and Athens fell")],
            when=[f"Inside(owner: 2, area: {area(*phaleron)}, count: 1)"],
        ),
    ]
    write(OUT, "marathon", "Marathon", [
        "Athens, 490 BC. Ten years ago the Greek cities of Ionia rose against Darius, King "
        "of Persia, and Athens sent ships to help them. The revolt is crushed, and the King "
        "has not forgotten.",
        "His army has burned Eretria and landed on the plain of Marathon, a day's march from "
        "Athens. Sparta will come, but not before the full moon. Athens must fight alone.",
    ], 490, g, hs, [
        side("Athens", "Player", GREEKS, "Bronze", (0, 0, 0, 0), start=(47, 31)),
        side("The King's army", "Scripted", PERSIANS, "Bronze"),
        side("The King's fleet", "Scripted", PERSIANS, "Bronze"),
    ], p, objs, trig, NOTE)


# ---------------------------------------------------------------------------
# 2. Thermopylae, 480 BC: hold the pass, and turn when the Immortals come.

THERMO_REAR = (26, 60, 42, 63)


def thermopylae():
    n, seed = 64, 480
    g = Grid(n, "g")

    def coast(y):
        # The pass is narrowest at the Middle Gate.
        narrow = 3.5 * math.exp(-(((y - 30) / 6) ** 2))
        return 40 - narrow + (smooth(0, y, 5, seed) - 0.5) * 2

    g.where(lambda x, y: x >= coast(y), "W")

    def cliff(y):
        narrow = 3 * math.exp(-(((y - 30) / 6) ** 2))
        return 30 + narrow + (smooth(1, y, 4, seed + 1) - 0.5) * 2

    # Mount Kallidromos: steep and wooded, no way over but the hidden path.
    g.where(lambda x, y: x < cliff(y) and 4 <= y <= 52, "f")
    g.where(lambda x, y: x < cliff(y) - 4 and 8 <= y <= 48 and smooth(x, y, 3, seed + 2) > 0.62, "a", over="f")
    # The open ground north (the Persian side) and south (the Greek rear).
    g.where(lambda x, y: y < 6 and x < 40, "d", over="gf")
    g.where(lambda x, y: y > 52 and smooth(x, y, 5, seed + 3) > 0.55, "d", over="g")
    g.shores()
    hs = heights(g, lambda cx, cy: 3.5 if cx < cliff(cy) - 2 and 6 <= cy <= 50 else 0.0)
    wall_y = 31
    gap = (35, 36)
    p = [place("House", 0, (26, 56), 4), place("House", 0, (28, 59), 3)]
    for x in range(30, 40):
        if x not in gap and g.get(x, wall_y) in "gds":
            p.append(place("Stone Wall", 0, (x, wall_y)))
    g.land(30, 58, 2, 2)
    p += [
        place("Hoplite", 0, (35, 34), 15),
        place("Hoplite", 0, (34, 36), 1, tag="leonidas"),
        place("Spearman", 0, (33, 39), 6),
        place("Bowman", 0, (36, 41), 6),
        place("Slinger", 0, (34, 43), 4),
    ]
    north = (34, 2)
    behind = (4, 58)
    waves = [
        (20, [("Spearman", 10), ("Bowman", 2)], "The Medes come first, in their thousands."),
        (100, [("Spearman", 12), ("Bowman", 3)], "The Cissians come on behind the Medes."),
        (190, [("Swordsman", 6), ("Bowman", 3)],
         "Xerxes sends his own guard, the Immortals, at the wall."),
        (290, [("Spearman", 14), ("Bowman", 4)], "Another wave. The Persians do not run short of men."),
        (330, [("Swordsman", 8), ("Bowman", 3)], "The Immortals again."),
        (490, [("Spearman", 12), ("Bowman", 3)], "The last of the day's assaults."),
    ]
    trig = [trigger([say(
        "Hold the narrow way between the mountain and the sea. In the pass, numbers count "
        "for less. Keep your men at the wall's gap."
    )])]
    for t, units, line in waves:
        acts = [say(line)]
        for i, (kind, count) in enumerate(units):
            acts.append(placed(kind, 1, (north[0] - 2 + 2 * i, north[1]), count))
        acts.append("Attack(owner: 1, to: (34, 62))")
        trig.append(trigger(acts, when=[f"After({t})"]))
    trig.append(trigger(
        [say("A man of Trachis, Ephialtes, has shown the Persians a path over the "
             "mountain. The Immortals will come down behind you, from the west, onto the "
             "road south. Send men to meet them, and keep the wall.")],
        when=["After(390)"], tid="ephialtes",
    ))
    trig.append(trigger(
        [say("The Immortals are coming down off the mountain!"),
         placed("Swordsman", 1, behind, 6),
         "Attack(owner: 1, to: (34, 62))"],
        when=['Since(trigger: "ephialtes", seconds: 40)'],
    ))
    trig.append(trigger(
        [lose("The Persians are through the pass, and the road to Greece is open")],
        when=[f"Inside(owner: 1, area: {area(*THERMO_REAR)}, count: 5)"],
    ))
    trig.append(trigger(
        ['Fail("leonidas")'],
        when=['Gone("leonidas")'],
    ))
    objs = [
        objective("hold", "Hold the pass for ten minutes", "Survive(seconds: 600)"),
        objective("leonidas", "Keep King Leonidas alive", "Scripted", optional=True),
    ]
    trig.append(trigger(['Complete("leonidas")'], when=['Done("hold")', 'Not(Failed("leonidas"))']))
    write(OUT, "thermopylae", "Thermopylae", [
        "Thermopylae, 480 BC. Darius is dead, and his son Xerxes has come himself, with the "
        "greatest army anyone has seen, and a fleet beside it along the coast.",
        "King Leonidas of Sparta holds the Hot Gates, a road a cart wide between the mountain "
        "and the sea, with three hundred Spartans and a few thousand allies. Every day he "
        "holds is a day for Greece to gather.",
    ], 480, g, hs, [
        side("Sparta and her allies", "Player", GREEKS, "Bronze", (0, 0, 0, 0), start=(34, 34)),
        side("Xerxes", "Scripted", PERSIANS, "Bronze"),
    ], p, objs, trig, NOTE)


# ---------------------------------------------------------------------------
# 3. Salamis, 480 BC: sink the Persian fleet in the straits.

def salamis():
    n, seed = 80, 4801
    g = Grid(n, "W")
    # Salamis island, west, with its Cynosura spit reaching east.
    g.poly([(0, 8), (14, 6), (22, 14), (24, 30), (31, 37), (30, 42), (22, 44), (20, 60),
            (12, 74), (0, 76)], "g", wobble=1.5, seed=seed)
    # Attica, east, with Mount Aigaleos above the straits.
    g.poly([(80, 0), (54, 0), (50, 16), (56, 28), (47, 34), (50, 40), (60, 46), (62, 62),
            (70, 80), (80, 80)], "g", wobble=1.5, seed=seed + 1)
    # Psyttaleia, the islet in the straits' mouth.
    g.poly([(40, 50), (44, 49), (45, 53), (41, 54)], "g", wobble=0.5, seed=seed + 2)
    g.where(lambda x, y: g.get(x, y) == "g" and smooth(x, y, 5, seed + 3) > 0.6, "d")
    g.blob(8, 24, 5, 7, "f", seed + 4, fill=0.8)
    g.blob(66, 20, 6, 7, "f", seed + 5, fill=0.8)
    g.shores()
    hs = heights(g, lambda cx, cy: 3.0 * max(0.0, 1 - math.hypot((cx - 68) / 10, (cy - 18) / 12))
                 + 2.0 * max(0.0, 1 - math.hypot((cx - 8) / 7, (cy - 40) / 14)))
    # The Athenians who left their city, on the island.
    for x, y in [(10, 50), (13, 54), (8, 56)]:
        g.land(x, y, 2, 2)
    p = [
        place("House", 0, (10, 50)),
        place("House", 0, (13, 54)),
        place("House", 0, (8, 56)),
        place("House", 0, (4, 48), 3),
        place("Villager", 0, (12, 46), 6),
        # The Greek fleet in the bay of Eleusis, out of sight of the straits.
        place("War Galley", 0, (36, 18), 12),
        place("Archer Ship", 0, (32, 22), 4),
    ]
    south_east = (48, 76)
    north_west = (4, 2)
    trig = [
        trigger([say(
            "Themistocles has drawn them in: the King's fleet is coming up the narrow straits, "
            "too many ships for the space. Meet them where they cannot spread out."
        )]),
        trigger(
            [say("The Phoenician squadron leads the way into the straits."),
             placed("War Galley", 1, south_east, 8),
             "Attack(owner: 1, to: (38, 24))"],
            when=["After(20)"], tid="phoenicians"),
        trigger(
            [say("The Ionian squadron follows them in."),
             placed("War Galley", 1, south_east, 6),
             placed("Archer Ship", 1, (54, 77), 4),
             "Attack(owner: 1, to: (38, 24))"],
            when=['Since(trigger: "phoenicians", seconds: 100)'], tid="ionians"),
        trigger(
            [say("The ships of Aegina come round the island to join the fight."),
             placed("War Galley", 0, (10, 78), 4)],
            when=['Since(trigger: "ionians", seconds: 30)']),
        trigger(
            [say("The Egyptian squadron has come round Salamis from the west, to close the "
                 "back door. Turn and meet them."),
             placed("War Galley", 1, north_west, 6),
             "Attack(owner: 1, to: (36, 18))"],
            when=['Since(trigger: "ionians", seconds: 90)'], tid="egyptians"),
        trigger(
            ['Complete("fleet")'],
            when=['Fired("egyptians")', 'HasAtMost(owner: 1, count: 0)']),
        trigger(
            [lose("The Greek fleet is lost, and Greece with it")],
            when=['HasAtMost(owner: 0, kind: Some("War Galley"), count: 0)',
                  'HasAtMost(owner: 0, kind: Some("Archer Ship"), count: 0)']),
    ]
    objs = [
        objective("fleet", "Sink the Persian fleet", "Scripted"),
    ]
    write(OUT, "salamis", "Salamis", [
        "Salamis, 480 BC. Thermopylae has fallen and Athens is burning. Its people have "
        "crossed to the island of Salamis; its men are on the ships.",
        "Themistocles sent a slave to Xerxes with a message: the Greeks will run tonight. "
        "The King believed it, and has sent his fleet into the straits to catch them.",
    ], 4801, g, hs, [
        side("Athens and her allies", "Player", GREEKS, "Bronze", (0, 0, 0, 0), start=(34, 20)),
        side("The King's fleet", "Scripted", PERSIANS, "Bronze"),
    ], p, objs, trig, NOTE)


# ---------------------------------------------------------------------------
# 4. Plataea, 479 BC: a full battle against Mardonius' army.

def plataea():
    n, seed = 96, 479
    g = Grid(n, "g")
    # The Asopus, west to east, with fords.
    river = [(0, 44), (20, 46), (40, 42), (60, 47), (80, 44), (96, 46)]
    g.line(river, 3, "w")
    for fx in (14, 46, 78):
        g.rect(fx, 38, fx + 2, 52, "s", over="w")
    # Mount Cithaeron in the south, wooded.
    g.where(lambda x, y: y > 78 and smooth(x, y, 6, seed) > 0.42, "f")
    g.where(lambda x, y: smooth(x, y, 8, seed + 1) > 0.63, "d", over="g")
    for cx, cy in [(10, 20), (86, 14), (30, 64), (70, 66), (52, 24)]:
        g.blob(cx, cy, 5, 4, "f", seed + cx, fill=0.8)
    # Clearings for the towns.
    g.clear(30, 6, 66, 32)
    g.clear(28, 56, 68, 76)
    hs = heights(g, lambda cx, cy: 3.2 * max(0.0, (cy - 74) / 18))
    p = [
        # The Greek camp at the foot of Cithaeron.
        place("Town Center", 0, (46, 66), tag="camp"),
        place("House", 0, (36, 62), 4),
        place("House", 0, (54, 70), 2),
        place("Academy", 0, (40, 70)),
        place("Barracks", 0, (54, 62)),
        place("Storehouse", 0, (62, 66)),
        place("Villager", 0, (48, 72), 8),
        place("Hoplite", 0, (46, 58), 8),
        place("Bowman", 0, (42, 58), 4),
        place("Berry Bush", 255, (34, 74), 4),
        place("Gold Vein", 255, (64, 74), 2),
        place("Stone Vein", 255, (30, 70), 2),
        # Mardonius' camp beyond the river, with its stockade.
        place("Town Center", 1, (46, 14)),
        place("Government Centre", 1, (40, 8), tag="mardonius"),
        place("House", 1, (36, 18), 3),
        place("Barracks", 1, (54, 10)),
        place("Archery Range", 1, (56, 16)),
        place("Stable", 1, (52, 22)),
        place("Watch Tower", 1, (44, 26)),
        place("Watch Tower", 1, (52, 28)),
        place("Villager", 1, (48, 18), 10),
        place("Light Cavalry", 1, (46, 30), 6),
        place("Bowman", 1, (40, 24), 8),
        place("Berry Bush", 255, (60, 6), 4),
        place("Gold Vein", 255, (32, 10), 2),
        place("Stone Vein", 255, (62, 26), 2),
    ]
    for x in range(32, 66, 1):
        if x in (47, 48):
            continue
        p.append(place("Palisade Wall", 1, (x, 33)))
    for x, y, w, h_ in [(46, 66, 3, 3), (46, 14, 3, 3), (40, 8, 3, 3)]:
        g.land(x, y, w, h_)
    objs = [
        objective("tent", "Destroy Mardonius' tent, his Government Centre", "Scripted"),
        objective("camp", "Keep your Town Center standing", "Scripted"),
    ]
    trig = [
        trigger([say(
            "Mardonius waits behind the Asopus with the King's best troops and his cavalry. "
            "Build up your army, then cross the river and burn his camp."
        )]),
        trigger([say(
            "The Persian horse will raid your camp: hoplites and spearmen in the town keep "
            "the villagers safe."
        )], when=["After(120)"]),
        trigger(['Complete("tent")', 'Complete("camp")'], when=['Gone("mardonius")']),
        trigger(['Fail("camp")', lose("The Persians have burned the Greek camp")],
                when=['Gone("camp")']),
    ]
    write(OUT, "plataea", "Plataea", [
        "Plataea, 479 BC. Xerxes has gone home after Salamis, but he left his general "
        "Mardonius in Greece with the best of the army.",
        "Now Sparta has marched at last, and the Greeks stand together on the slopes of "
        "Cithaeron: the largest Greek army ever gathered. End the war.",
    ], 479, g, hs, [
        side("The Greek alliance", "Player", GREEKS, "Bronze", (600, 600, 300, 300)),
        side("Mardonius", "Computer(1)", PERSIANS, "Bronze", (400, 400, 200, 200)),
    ], p, objs, trig, NOTE)


marathon()
thermopylae()
salamis()
plataea()
campaign(OUT, "The Persian Wars", "Athens and Sparta against the King of Kings, 490 to 479 BC.",
         1, ["marathon", "thermopylae", "salamis", "plataea"])
print("wrote", OUT)

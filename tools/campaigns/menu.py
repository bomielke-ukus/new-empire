"""The diorama behind the campaign screens: a Greek town and an Egyptian
one, each raided from its far side, two pairs of galleys at sea.

    python3 tools/campaigns/menu.py assets/menu

The app plays it live under the CAMPAIGNS and briefing screens and starts
it again when the fight is over (`crates/app/src/diorama.rs`). It is a
scenario like any other, so the editor opens it.
"""
import sys

from scenlib import (Grid, heights, place, side, smooth, trigger, objective,
                     write)

OUT = sys.argv[1] if len(sys.argv) > 1 else "assets/menu"
N = 56
SEED = 776
NOTE = ("The menu diorama (docs/10). First drawn by tools/campaigns/menu.py; "
        "edit freely, it is checked when read.")


def diorama():
    g = Grid(N, "g")
    # A sea in the top corner, where the screen's top is.
    g.where(lambda x, y: x + y + (smooth(x, y, 5, SEED) - 0.5) * 6 < 15, "W")
    g.shores()
    # A road between the towns, worn into dirt.
    g.line([(13, 31), (21, 21), (31, 13)], 2, "d", over="g")
    for cx, cy in ((12, 31), (31, 12)):
        g.blob(cx, cy, 6, 6, "d", SEED + cx, fill=0.55, over="g")
    # Forest along the far edges, framing the view.
    for cx, cy, r in ((38, 40, 4), (41, 35, 3), (30, 45, 3), (46, 29, 3),
                      (23, 49, 3), (50, 22, 3), (48, 46, 5), (2, 22, 2), (22, 2, 2)):
        g.blob(cx, cy, r, r, "f", SEED + cx * 3 + cy, fill=0.9)
    # Each town, and the ways the raids come in, are open ground.
    for x0, y0, x1, y1 in ((0, 24, 22, 44), (24, 0, 44, 22)):
        g.clear(x0, y0, x1, y1)
    hs = heights(g, lambda cx, cy: 2 * smooth(cx, cy, 9, SEED + 5)
                 + (1 if cx + cy > 70 else 0))

    blue, red = 0, 1
    p = [
        # The Greeks, on the left of the screen.
        place("Town Center", blue, (12, 30)),
        place("House", blue, (7, 26), 3),
        place("Barracks", blue, (16, 26)),
        place("Archery Range", blue, (16, 30)),
        place("Temple", blue, (15, 34)),
        place("Academy", blue, (11, 35)),
        place("Watch Tower", blue, (19, 28)),
        place("Farm", blue, (6, 32), 2),
        place("Storehouse", blue, (7, 36)),
        place("Villager", blue, (10, 34), 8),
        place("Bowman", blue, (14, 28), 4),
        place("Hoplite", blue, (18, 32), 4),
        # The Egyptians, on the right, under their pyramid.
        place("Wonder", red, (27, 5)),
        place("Town Center", red, (30, 12)),
        place("House", red, (35, 18), 3),
        place("Barracks", red, (26, 16)),
        place("Stable", red, (30, 16)),
        place("Temple", red, (34, 15)),
        place("Watch Tower", red, (28, 19)),
        place("Farm", red, (35, 10), 2),
        place("Villager", red, (34, 13), 8),
        place("Priest", red, (36, 18), 2),
        place("Chariot Archer", red, (27, 13), 3),
        place("Spearman", red, (32, 19), 4),
        # The raids, each coming in from the far side of the town it
        # falls on (`diorama.rs` sends them in).
        place("Legionary", red, (3, 44), 7),
        place("War Elephant", red, (6, 47), 2),
        place("Chariot Archer", red, (2, 40), 4),
        place("Hoplite", blue, (44, 3), 7),
        place("Heavy Cavalry", blue, (47, 6), 3),
        place("Bowman", blue, (40, 2), 4),
        place("Catapult", blue, (46, 9), 1),
        # Galleys at sea.
        place("War Galley", blue, (5, 7), 2),
        place("War Galley", red, (9, 4), 2),
        # Nature's.
        place("Berry Bush", 255, (4, 30), 3),
        place("Gold Vein", 255, (40, 13), 2),
        place("Gazelle", 255, (33, 36), 5),
        place("Berry Bush", 255, (27, 38), 3),
    ]
    trig = []
    objs = [objective("stand", "Stand", "Survive(seconds: 3600)")]
    write(OUT, "diorama", "The Plain Between", [
        "Behind the campaign screens.",
    ], SEED, g, hs, [
        side("Greeks", "Player", "Greeks", "Iron", start=(21, 21)),
        side("Egyptians", "Scripted", "Egyptians", "Iron"),
    ], p, objs, trig, NOTE)


diorama()

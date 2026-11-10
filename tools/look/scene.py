"""The target picture (docs/10, the revamp, phase 0): a Greek town and an
Egyptian one on a painted landscape, a skirmish between them, galleys at sea.

    /usr/bin/python3.13 tools/look/scene.py OUT.png [width_px] [samples]
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib  # noqa: E402
import egyptian as eg  # noqa: E402
import figures as fg  # noqa: E402
import greek as gr  # noqa: E402
import terrain  # noqa: E402
from lib import BLUE, RED  # noqa: E402

E = math.pi / 4          # facing screen-right
W = E + math.pi          # facing screen-left
V = fg.VIEW              # facing the camera


def _prune():
    """Opens the view: the landscape is dressed for its own sake, and a
    picture of a game needs its fight, its sea and its plazas in sight."""
    import bpy
    r2 = math.sqrt(2)
    gone = []
    for ob in bpy.data.objects:
        kind = ob.name.split(".")[0]
        if ob.type != "EMPTY" or kind not in ("olive", "cypress", "palm", "bush", "rock"):
            continue
        x, y = ob.location.x, ob.location.y
        u, v = (x + y) / r2, (y - x) / r2      # screen-right, away
        tree = kind in ("olive", "cypress", "palm")
        near_fight = v < -3.0 and abs(u - 0.0) < 6.5
        on_beach = y - x > 6.6 and abs(u) < 7.5
        in_town = (-9 < x < 0.6 and -6.5 < y < 1.6) or (-3 < x < 7.2 and 0 < y < 8.6)
        n = sum(map(ord, ob.name)) + int(abs(x * 7 + y * 13))
        if (tree and (near_fight or in_town)) or (tree and on_beach and n % 4 != 0) \
                or (kind == "bush" and (in_town or (near_fight and n % 3 != 0))) \
                or (kind == "rock" and in_town):
            gone.append(ob)
    for ob in gone:
        for c in list(ob.children):
            bpy.data.objects.remove(c)
        bpy.data.objects.remove(ob)


def build():
    terrain.build()
    _prune()

    # The Greek town, on its plaza round (-4, -2).
    gr.temple((-5.9, -0.3, 0))
    gr.town_center((-2.5, -0.2, 0))
    gr.house((-6.3, -3.3, 0), variant=0)
    gr.house((-4.0, -4.6, 0), variant=1)
    gr.barracks((-1.4, -3.3, 0))
    gr.watch_tower((-0.2, -5.0, 0))
    gr.market_stall((-4.1, -2.4, 0))
    gr.cart((-2.9, -2.6, 0), rot_z=0.6)
    fg.villager((-3.4, -2.3, 0), V + 0.4, BLUE, "stand", 1, job="carry")
    fg.villager((-4.9, -2.7, 0), E, BLUE, "walk", 2)
    fg.villager((-2.2, -4.4, 0), W, BLUE, "walk", 5, job="hammer")
    fg.priest((-5.4, -2.0, 0), V, BLUE, "attack", 0, style="greek")
    fg.donkey((-5.6, -5.4, 0), E - 0.3, BLUE, "walk", 1)
    fg.villager((-6.2, -5.6, 0), E - 0.3, BLUE, "walk", 3)
    fg.villager((-7.2, -3.9, 0), V, BLUE, "attack", 4, job="farm")
    fg.villager((-8.6, -5.0, 0), E, BLUE, "attack", 7, job="farm")
    fg.hoplite((-0.3, -4.1, 0), V, BLUE, "stand", 2)
    fg.horseman((-1.2, -5.7, 0), E - 0.5, BLUE, "walk", 0)

    # The Egyptian town, on its plaza round (2, 4.5).
    eg.pyramid((0.5, 6.4, 0))
    eg.temple((4.3, 4.6, 0))
    eg.town_center((1.3, 2.5, 0))
    eg.house((-1.7, 3.4, 0), variant=0)
    eg.barracks((5.7, 2.4, 0))
    eg.sphinx((3.3, 2.3, 0))
    eg.granary((3.4, 0.9, 0))
    fg.priest((4.0, 2.6, 0), V, RED, "attack", 1)
    fg.villager((2.9, 1.8, 0), W, RED, "walk", 2, job="carry")
    fg.villager((-0.4, 2.2, 0), V, RED, "stand", 6)
    fg.ox((4.6, 1.2, 0), W + 0.3, RED, "walk", 0)
    fg.villager((5.3, 1.0, 0), W + 0.3, RED, "walk", 4)
    fg.villager((5.0, 6.9, 0), V, RED, "attack", 3, job="farm")
    fg.egyptian_spearman((5.2, 3.6, 0), V, RED, "stand", 3)

    # The fight on the trampled ground between them.
    fight = fg.skirmish()
    fight.location = (3.0, -3.0, 0.0)
    fg.hoplite((0.6, -2.6, 0), E - 0.6, BLUE, "walk", 8, cloak=True)
    fg.hoplite((0.9, -3.6, 0), E - 0.6, BLUE, "walk", 9)

    # Galleys on the sea.
    fg.galley((-7.6, 4.4, terrain.WATER_Z), E - 0.5, BLUE, "walk", 0)
    fg.galley((-3.4, 9.4, terrain.WATER_Z), W + 0.4, RED, "walk", 1)


if __name__ == "__main__":
    out = sys.argv[1]
    width = int(sys.argv[2]) if len(sys.argv) > 2 else 1600
    samples = int(sys.argv[3]) if len(sys.argv) > 3 else 64
    lib.reset()
    build()
    lib.camera((0.0, 0.3, 0.4), 13.8, (width, width * 5 // 8))
    lib.light()
    lib.render(out, samples=samples)
    print("wrote", out)

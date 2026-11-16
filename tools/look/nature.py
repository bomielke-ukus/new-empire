"""Nature's resources in the new look: the gold vein, the stone vein and the
berry bush, each on one tile (`crates/sim/src/kinds.rs`: footprint 1). Like
the old sets (`assets/sprites/<name>`) each is one `idle` frame, class
SmallBuilding; nothing on them takes player colour (they belong to nobody).

    gold_mine()   glinting gold ore in a dark rock outcrop, nuggets at its foot
    stone_mine()  pale raw boulders with cut, squared blocks quarried out of them
    berry_bush()  a round, lush bush dotted all over with clearly red berries:
                  rounder, brighter and bigger than the dull, flat scenery scrub
                  of decor.bush, so a forager's target never reads as scenery

They are seen small (a tile is 128 x 64 px), so they are bold: the gold is
lit a little from within so it glints even on the shadow side.

    /usr/bin/python3.13 tools/look/export_all.py --only gold_mine stone_mine berry_bush
"""
import math
import random

import bpy  # noqa: F401  (before bmesh)

import lib
from lib import blob, box, empty, mat, sphere

ROCK_DK = (0.30, 0.27, 0.25)      # the gold's host rock: dark, so the ore shows
ROCK_DK2 = (0.24, 0.22, 0.21)
ORE = (1.00, 0.78, 0.20)
QUARTZ = (0.92, 0.90, 0.84)
STONE = (0.84, 0.82, 0.76)        # pale limestone, raw
STONE_CUT = (0.92, 0.89, 0.80)    # fresh-cut faces
LEAF = (0.22, 0.52, 0.18)
LEAF_LT = (0.34, 0.64, 0.22)
LEAF_DK = (0.12, 0.34, 0.12)
BERRY = (0.86, 0.08, 0.10)


def _gold():
    return mat(ORE, rough=0.22, metal=1.0, emit=0.35)


def gold_mine(loc=(0, 0, 0), rot_z=0.0, parent=None):
    """A dark rock outcrop about 0.8 across and 0.55 tall, veined with quartz
    and studded with gold ore; loose nuggets and a few at its foot."""
    r = empty("gold_mine", loc, rot_z, parent)
    rnd = random.Random(11)
    rock = [mat(ROCK_DK, rough=0.85), mat(ROCK_DK2, rough=0.9), mat(tuple(v * 1.15 for v in ROCK_DK), rough=0.85)]
    lumps = [((0.02, 0.08, 0.16), 0.3, (1.15, 1.0, 1.15)), ((-0.2, -0.08, 0.1), 0.2, (1.0, 1.1, 0.95)),
             ((0.22, -0.1, 0.08), 0.2, (1.1, 1.0, 0.85)), ((-0.05, -0.22, 0.05), 0.15, (1.2, 1.0, 0.8)),
             ((0.18, 0.22, 0.1), 0.18, (1.0, 1.0, 0.9))]
    for i, (p, rad, sc) in enumerate(lumps):
        blob(rad, p, rock[i % 3], sc, r, seed=40 + i, rough=0.22, subdiv=2)
    gm = _gold()
    qm = mat(QUARTZ, rough=0.5)
    # ore on the sunlit and camera faces (-Y, +X, the top): clusters of nuggets
    spots = [(-0.12, -0.2, 0.3), (0.12, -0.18, 0.26), (0.28, -0.02, 0.2), (0.02, -0.05, 0.44),
             (-0.28, -0.12, 0.18), (0.2, 0.12, 0.3), (-0.08, 0.12, 0.42), (0.06, -0.3, 0.12),
             (0.34, -0.2, 0.08)]
    for (x, y, z) in spots:
        for k in range(3):
            sphere(0.035 + 0.018 * rnd.random(), (x + rnd.uniform(-0.04, 0.04), y + rnd.uniform(-0.03, 0.03),
                                                  z + rnd.uniform(-0.03, 0.03)), gm, parent=r, segs=6)
    for i in range(4):                      # quartz veins across the face
        a = rnd.uniform(-0.6, 0.6)
        box((0.22, 0.02, 0.03), (rnd.uniform(-0.15, 0.2), -0.24 + 0.05 * i, 0.12 + 0.06 * i), qm,
            rot=(0, a, 0.3), parent=r)
    for i in range(7):                      # loose nuggets on the ground in front
        a = rnd.uniform(-2.6, -0.4)
        d = rnd.uniform(0.3, 0.42)
        sphere(0.03 + 0.015 * rnd.random(), (math.cos(a) * d, math.sin(a) * d, 0.02), gm, parent=r, segs=6,
               scale=(1, 1, 0.7))
    return r


def stone_mine(loc=(0, 0, 0), rot_z=0.0, parent=None):
    """A pale outcrop: raw rounded boulders at the back, squared blocks cut
    from it stacked and scattered in front, bright cut faces."""
    r = empty("stone_mine", loc, rot_z, parent)
    rnd = random.Random(23)
    raw = [mat(STONE, rough=0.8), mat(tuple(v * 0.9 for v in STONE), rough=0.85),
           mat(tuple(min(1, v * 1.05) for v in STONE), rough=0.8)]
    for i, (p, rad, sc) in enumerate((((0.0, 0.14, 0.14), 0.28, (1.2, 1.0, 1.1)),
                                      ((-0.24, 0.0, 0.1), 0.2, (1.0, 1.1, 1.0)),
                                      ((0.24, 0.12, 0.08), 0.2, (1.1, 1.0, 0.9)))):
        blob(rad, p, raw[i], sc, r, seed=60 + i, rough=0.18, subdiv=2)
    cut = [mat(STONE_CUT, rough=0.75), mat(tuple(v * 0.94 for v in STONE_CUT), rough=0.75)]
    # a stepped quarry face cut into the big boulder's front
    box((0.34, 0.18, 0.24), (0.02, -0.06, 0.12), cut[0], parent=r)
    box((0.22, 0.14, 0.14), (0.02, -0.17, 0.07), cut[1], parent=r)
    # squared blocks: a little stack and a few lying about
    blocks = [((-0.22, -0.26, 0.06), (0.2, 0.13, 0.12), 0.1), ((0.02, -0.33, 0.06), (0.2, 0.13, 0.12), -0.15),
              ((-0.12, -0.3, 0.18), (0.18, 0.12, 0.11), 0.3), ((0.27, -0.2, 0.055), (0.16, 0.12, 0.11), 0.6),
              ((0.3, -0.02, 0.05), (0.13, 0.1, 0.1), 0.2)]
    for i, (p, s, a) in enumerate(blocks):
        box(s, p, cut[i % 2], rot=(0, 0, a), parent=r, bevel=0.01)
    for i in range(6):                       # chips and rubble
        ang = rnd.uniform(-2.8, -0.3)
        d = rnd.uniform(0.32, 0.44)
        blob(0.035, (math.cos(ang) * d, math.sin(ang) * d, 0.02), raw[i % 3], (1, 1, 0.6), r, seed=80 + i,
             rough=0.3, subdiv=1)
    return r


def berry_bush(loc=(0, 0, 0), rot_z=0.0, parent=None):
    """A round lush bush about 0.7 across and 0.55 tall, fresh green, dotted
    all over (sunlit side most) with clusters of red berries."""
    r = empty("berry_bush", loc, rot_z, parent)
    rnd = random.Random(31)
    leaves = [mat(LEAF, rough=0.8), mat(LEAF_LT, rough=0.75), mat(LEAF_DK, rough=0.85)]
    clumps = [((0.0, 0.0, 0.26), 0.27), ((-0.17, -0.06, 0.2), 0.19), ((0.17, -0.08, 0.2), 0.19),
              ((0.0, 0.17, 0.22), 0.2), ((-0.04, -0.17, 0.16), 0.17), ((0.06, 0.0, 0.42), 0.17)]
    for i, (p, rad) in enumerate(clumps):
        sphere(rad, p, leaves[2 if i == 3 else i % 2], (1.05, 1.0, 0.92), r, segs=14)
        for k in range(5):                     # leaf knobs so the outline is leafy, not a ball
            a, b = rnd.uniform(0, 2 * math.pi), rnd.uniform(-0.2, 1.2)
            q = (p[0] + math.cos(a) * math.cos(b) * rad * 0.85, p[1] + math.sin(a) * math.cos(b) * rad * 0.85,
                 p[2] + math.sin(b) * rad * 0.8)
            blob(rad * 0.38, q, leaves[(i + k) % 3], (1, 1, 0.8), r, seed=100 + i * 7 + k, rough=0.2, subdiv=1)
    bm = mat(BERRY, rough=0.35, emit=0.12)
    hl = mat((1.0, 0.75, 0.72), rough=0.2)
    n = 0
    tries = 0
    while n < 30 and tries < 400:
        tries += 1
        a = rnd.uniform(-math.pi, math.pi)
        b = rnd.uniform(-0.1, 1.1)
        # more on the faces the camera sees (-Y, +X)
        if math.sin(a) > 0.2 and math.cos(a) < 0.2 and rnd.random() < 0.7:
            continue
        cx, cy, cz, R = 0.0, 0.0, 0.24, 0.36
        p = (cx + math.cos(a) * math.cos(b) * R, cy + math.sin(a) * math.cos(b) * R * 0.95,
             cz + math.sin(b) * R * 0.75)
        for k in range(3):
            q = (p[0] + rnd.uniform(-0.03, 0.03), p[1] + rnd.uniform(-0.03, 0.03), p[2] + rnd.uniform(-0.03, 0.02))
            sphere(0.032, q, bm, parent=r, segs=8)
            if k == 0:
                sphere(0.009, (q[0] + 0.012, q[1] - 0.02, q[2] + 0.02), hl, parent=r, segs=4)
        n += 1
    return r


SETS = {
    name: dict(func="nature." + name, cls="SmallBuilding", tiles=1, stages=False, sun_angle=8)
    for name in ("gold_mine", "stone_mine", "berry_bush")
}

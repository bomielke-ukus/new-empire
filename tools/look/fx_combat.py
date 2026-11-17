"""Combat feedback and small life in the new look (docs/look/atmosphere.md,
"Combat feedback" and "Life"): small colour sets, exported by
export_all.py like the other effects (fx.py), each an `idle` whose frames
the game picks as it needs.

    fx_arrow    an arrow in flight: frame k flies toward the game's facing
                k (0 = +x, 1 = +x+y, ... 7 = +x-y), its fletching in the
                owner's colour
    fx_stone    a siege engine's stone in flight, tumbling over 4 frames
    fx_spark    where a blow lands: a bright burst, 4 frames, gone by the
                last
    fx_dust     a puff of dust thrown up: a kill, a hammer, hooves; 6
                frames swelling and thinning to nothing
    fx_birds    a few birds far above, wings beating, a 6-frame loop; the
                game flies them across the map (no shadow: they are high)

A module of its own rather than more of fx.py, so the sets fx.py already
exported keep their hashes. The game's (gx, gy) is Blender's (gx, -gy)
(export.py's RIGHT and UP). Nothing stands on the ground (`no_ground`).
"""
import math
import random

import bpy  # noqa: F401  (before bmesh)
from mathutils import Matrix, Vector

import lib
from lib import BLUE, mat
from fx import RIGHT, UP, TOWARD, _disc_material, _no_ground, disc

ARROW_FRAMES = 8
STONE_FRAMES, STONE_MS = 4, 90
SPARK_FRAMES, SPARK_MS = 4, 50
DUST_FRAMES, DUST_MS = 6, 70
BIRD_FRAMES, BIRD_MS = 6, 90


def _facing(k):
    """The game's facing k as a unit direction in Blender's ground plane."""
    a = math.radians(45 * k)
    return Vector((math.cos(a), -math.sin(a), 0.0))


def _along(ob, d, up=0.0):
    """Turn `ob`, built along local +Z, to point along `d` tilted `up`."""
    v = (d + Vector((0, 0, up))).normalized()
    ob.rotation_euler = v.to_track_quat("Z", "Y").to_euler()


@_no_ground
def arrow(frame=0, player=BLUE, length=0.62):
    """An arrow flying toward facing `frame`, nose a little down: a pale
    shaft, a dark iron head, the fletching in the owner's colour."""
    r = lib.empty("fx_arrow")
    d = _facing(frame)
    pivot = lib.empty("arrow", (0, 0, 0.0), parent=r)
    lib.cyl(0.018, length, (0, 0, 0), mat((0.82, 0.70, 0.50), rough=0.7), parent=pivot,
            segs=8, name="shaft")
    lib.cyl(0.045, 0.13, (0, 0, length / 2 + 0.06), mat((0.22, 0.22, 0.24), metal=0.6,
            rough=0.4), parent=pivot, r_top=0.0, segs=8, name="head")
    for i in range(3):
        a = 2 * math.pi * i / 3
        vs = [(0, 0, -length / 2 + 0.02), (0, 0, -length / 2 + 0.17),
              (0.07, 0, -length / 2 - 0.01)]
        ob = lib.mesh(vs, [(0, 1, 2)], mat(player, rough=0.8), parent=pivot, name="fletch")
        ob.rotation_euler = (0, 0, a)
    _along(pivot, d, up=-0.12)
    for ob in r.children_recursive:
        ob.visible_shadow = False
    return r


@_no_ground
def stone(frame=0):
    """A rough stone ball, a little over a tenth of a tile, tumbling."""
    r = lib.empty("fx_stone")
    ob = lib.blob(0.13, (0, 0, 0), mat((0.56, 0.53, 0.49), rough=0.9), parent=r, seed=7,
                  rough=0.22, subdiv=2, name="stone")
    ob.rotation_euler = (frame * math.pi / 2 / STONE_FRAMES * 2, 0.4, frame * 0.7)
    ob.visible_shadow = False
    return r


def _ring(frame_t, parent, n, r0, r1, colour, opacity, size, seed):
    """`n` soft discs thrown out from the origin, `frame_t` 0 to 1 of the way."""
    for k in range(n):
        rnd = random.Random(seed * 31 + k)
        a = 2 * math.pi * (k + rnd.uniform(-0.3, 0.3)) / n
        dist = r0 + (r1 - r0) * frame_t * rnd.uniform(0.7, 1.1)
        pos = RIGHT * math.cos(a) * dist + UP * math.sin(a) * dist * 0.8
        alpha = opacity * (1.0 - frame_t) ** 1.2
        if alpha < 0.02:
            continue
        m = _disc_material("spark%d" % k, colour, colour, alpha, False, 0.1, seed + k, soft=0.8)
        disc(pos, size * (1.0 - 0.5 * frame_t), m, parent, toward=0.1, name="fx_mote")


@_no_ground
def spark(frame=0):
    """Where a blow lands: a white-gold flash, and motes flying out of it,
    gone by the last frame."""
    r = lib.empty("fx_spark")
    t = frame / SPARK_FRAMES
    core = 0.95 * (1.0 - t) ** 1.5
    if core > 0.02:
        m = _disc_material("flash", (1.0, 0.98, 0.86), (1.0, 0.78, 0.32), core, False, 0.15, 3,
                           soft=0.9)
        disc((0, 0, 0), 0.11 + 0.08 * t, m, r, name="fx_flash")
    _ring(t, r, 6, 0.04, 0.26, (1.0, 0.86, 0.45), 0.95, 0.035, 11)
    return r


@_no_ground
def dust(frame=0):
    """A puff of dust off the ground: a few soft tan balls swelling and
    rising a little, thinning to nothing by the last frame."""
    r = lib.empty("fx_dust")
    t = (frame + 0.5) / DUST_FRAMES
    for k in range(7):
        rnd = random.Random(70 + k)
        a = 2 * math.pi * (k + rnd.uniform(-0.3, 0.3)) / 7
        dist = 0.05 + 0.22 * t ** 0.7 * rnd.uniform(0.6, 1.0)
        pos = (RIGHT * math.cos(a) * dist + Vector((0, 0, 0.04 + 0.16 * t * rnd.uniform(0.5, 1.0)))
               + UP * math.sin(a) * dist * 0.35)
        alpha = 0.75 * min(1.0, t * 4.0) * (1.0 - t) ** 1.3
        if alpha < 0.02:
            continue
        m = _disc_material("dust", (0.86, 0.78, 0.62), (0.62, 0.54, 0.42), alpha, True, 0.35,
                           20 + k)
        disc(pos, 0.07 + 0.12 * t ** 0.8, m, r, rnd.uniform(0.8, 1.0), toward=-0.1 * k,
             name="fx_puff")
    return r


@_no_ground
def birds(frame=0):
    """Five small birds in a loose V, wings beating out of step a little:
    dark, seen from far above."""
    r = lib.empty("fx_birds")
    body = mat((0.16, 0.15, 0.17), rough=0.9)
    spots = [(0.0, 0.0), (-0.22, 0.16), (-0.22, -0.16), (-0.44, 0.30), (-0.42, -0.33)]
    for k, (x, y) in enumerate(spots):
        phase = 2 * math.pi * (frame / BIRD_FRAMES + k * 0.17)
        lift = 0.07 * math.sin(phase)
        at = Vector((x, y, 0.0))
        lib.sphere(0.022, at, body, scale=(1.8, 0.8, 0.8), parent=r, segs=8, name="bird")
        for side in (-1, 1):
            vs = [(at.x + 0.025, at.y, at.z), (at.x - 0.03, at.y, at.z),
                  (at.x - 0.005, at.y + side * 0.11, at.z + lift)]
            lib.mesh(vs, [(0, 1, 2)], body, parent=r, name="wing")
    # Flying toward the game's facing 7 (+x-y): up and to the right on screen.
    r.rotation_euler = (0, 0, math.radians(45))
    for ob in r.children_recursive:
        ob.visible_shadow = False
    return r


def _frames(n):
    return {"idle": [{"frame": i} for i in range(n)]}


def _set(func, n, ms, loops, samples=64):
    return dict(func=func, cls="SmallBuilding", tiles=1, stages=False, anims=_frames(n),
                timing={"idle": [ms, loops]}, samples=samples)


SETS = {
    "fx_arrow": _set("fx_combat.arrow", ARROW_FRAMES, 100, True),
    "fx_stone": _set("fx_combat.stone", STONE_FRAMES, STONE_MS, True),
    "fx_spark": _set("fx_combat.spark", SPARK_FRAMES, SPARK_MS, False),
    "fx_dust": _set("fx_combat.dust", DUST_FRAMES, DUST_MS, False),
    "fx_birds": _set("fx_combat.birds", BIRD_FRAMES, BIRD_MS, True),
}

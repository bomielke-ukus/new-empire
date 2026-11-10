"""Small scenery for the game's ground (docs/look/ground.md, "Scenery") and
the forest tree. Each builder makes one prop at the origin, inside one tile,
and returns its root; export with tools/look/export.py:

    /usr/bin/python3.13 tools/look/export.py decor_flowers_0 decor.flowers \
        --class SmallBuilding --kwargs '{"variant": 0}' --sun-angle 12

    flowers(variant)  0 red poppies, 1 yellow, 2 white, 3 mauve, 4 blue
    tuft(variant)     0 lush, 1 tall light, 2 dry
    rock(variant)     0 pebble cluster, 1 small rock, 2 mossy boulder
    bush(variant)     0 plain, 1 pink oleander, 2 yellow broom, 3 white blossom
    reeds(variant)    0 reeds and cattails, 1 papyrus
    tree(variant)     a broadleaf with a full, layered crown, ~1.3 x 2 tiles

They are seen small (a tile is 128 x 64 px), so they are drawn bigger and
bolder than life: few, large, saturated blooms; chunky leaf clumps lit on
top and dark beneath (vertex colours, as tools/look/terrain.py's props).
Everything is seeded: the same variant always builds the same mesh.
"""
import math
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import empty  # noqa: E402
from terrain import _B, _c  # noqa: E402

GRASS_B, GRASS_T = (0.18, 0.42, 0.12), (0.56, 0.78, 0.24)


def _finish(b, name):
    root = empty(name)
    b.finish(name, root, smooth=True)
    return root


def _blades(b, x, y, n, h, cb, ct, spread=0.06, w=0.035, lean=(0.15, 0.55), z=0.0):
    """A tuft of n blades at (x, y), fanned outward."""
    r = b.rnd
    for _ in range(n):
        a = r.uniform(0, 2 * math.pi)
        o = r.uniform(0, spread)
        hh = h * r.uniform(0.6, 1.15)
        ln = r.uniform(*lean) * hh
        p = (x + math.cos(a) * o * 0.5, y + math.sin(a) * o * 0.5, z - 0.01)
        tip = (x + math.cos(a) * (o + ln), y + math.sin(a) * (o + ln), z + hh)
        k = r.uniform(0.85, 1.15)
        b.blade(p, tip, w * r.uniform(0.8, 1.3), _c(cb, k), _c(ct, k))


# --------------------------------------------------------------- flowers

BLOOMS = [  # petal, petal shade, eye
    ((0.96, 0.12, 0.08), (0.62, 0.04, 0.04), (0.10, 0.06, 0.06)),     # poppy
    ((1.00, 0.84, 0.10), (0.86, 0.56, 0.04), (0.92, 0.46, 0.06)),     # crown daisy
    ((1.00, 0.99, 0.96), (0.80, 0.80, 0.84), (1.00, 0.78, 0.12)),     # chamomile
    ((0.78, 0.46, 0.92), (0.50, 0.24, 0.66), (0.98, 0.92, 0.60)),     # mallow
    ((0.24, 0.46, 1.00), (0.12, 0.24, 0.70), (0.98, 0.96, 0.80)),     # cornflower, flax
]


def _bloom(b, cen, rad, kind, k=1.0, cup=0.35):
    """A five-petalled cup facing up (and a little toward the camera)."""
    petal, shade, eye = BLOOMS[kind]
    r = b.rnd
    a0 = r.uniform(0, 2 * math.pi)
    cen = np.asarray(cen, float)
    verts = [cen + (0, 0, rad * 0.1)]
    cols = [_c(eye, k)]
    for i in range(10):
        a = a0 + i * math.pi / 5
        rr = rad if i % 2 == 0 else rad * 0.62
        verts.append(cen + (math.cos(a) * rr, math.sin(a) * rr, rr * cup))
        lit = 0.5 + 0.5 * math.cos(a - math.atan2(-0.8, 0.3))
        cols.append(_c(np.asarray(shade) * (1 - lit * 0.6) + np.asarray(petal) * lit * 0.6
                       + np.asarray(petal) * 0.4, k))
    faces = [[0, i + 1, (i + 1) % 10 + 1] for i in range(10)]
    b.add(verts, cols, faces)
    if kind != 0:          # an eye that shows
        b.blob(cen + (0, 0, rad * 0.22), rad * 0.3, _c(eye, k * 1.05), _c(eye, k * 0.7), sub=1,
               rough=0.05)
    else:                  # the poppy's dark heart
        b.blob(cen + (0, 0, rad * 0.18), rad * 0.26, (0.16, 0.12, 0.10), (0.04, 0.03, 0.03), sub=1,
               rough=0.05)


def flowers(variant=0):
    """A small drift of wildflowers, ~0.7 x 0.5 tile: one kind, a few
    blooms of a second, a little foliage."""
    kind = variant % 5
    b = _B(900 + variant)
    r = b.rnd
    n = 20
    for i in range(n):
        a = r.uniform(0, 2 * math.pi)
        o = abs(r.gauss(0, 0.15)) + 0.02
        x, y = math.cos(a) * o * 1.25, math.sin(a) * o * 0.85
        x, y = max(-0.36, min(0.36, x)), max(-0.3, min(0.3, y))
        h = r.uniform(0.10, 0.24)
        ox, oy = r.uniform(-0.03, 0.03), r.uniform(-0.03, 0.03)
        b.blade((x, y, -0.01), (x + ox, y + oy, h), 0.022, (0.16, 0.40, 0.12), (0.34, 0.60, 0.18))
        kk = kind if r.random() < 0.85 else (kind + 2) % 5
        _bloom(b, (x + ox, y + oy, h), r.uniform(0.075, 0.095), kk, r.uniform(0.92, 1.06))
    for i in range(7):                     # leaves and grass at the feet
        a = r.uniform(0, 2 * math.pi)
        o = r.uniform(0.0, 0.22)
        _blades(b, math.cos(a) * o * 1.2, math.sin(a) * o * 0.8, 4, 0.13, GRASS_B, GRASS_T,
                spread=0.05, w=0.04)
    return _finish(b, "flowers")


# --------------------------------------------------------------- tufts

def tuft(variant=0):
    """A clump of grass ~0.4 across: 0 lush, 1 tall and light, 2 dry."""
    b = _B(910 + variant)
    r = b.rnd
    cb, ct, h, n = [((0.10, 0.36, 0.10), (0.60, 0.86, 0.24), 0.40, 8),
                    ((0.24, 0.50, 0.12), (0.84, 0.94, 0.36), 0.52, 7),
                    ((0.50, 0.44, 0.18), (0.98, 0.88, 0.50), 0.42, 7)][variant % 3]
    for i in range(n):
        a = i * 2.4 + r.uniform(-0.3, 0.3)
        o = r.uniform(0.0, 0.13) if i else 0.0
        _blades(b, math.cos(a) * o, math.sin(a) * o, 8, h * r.uniform(0.75, 1.05), cb, ct,
                spread=0.06, w=0.05, lean=(0.2, 0.6))
    if variant % 3 == 2:                   # a few seed heads on the dry one
        for i in range(4):
            a = r.uniform(0, 2 * math.pi)
            p = np.array([math.cos(a) * 0.06, math.sin(a) * 0.06, 0.0])
            tip = p + (math.cos(a) * 0.12, math.sin(a) * 0.12, h * 1.25)
            b.blade(p, tip, 0.012, (0.66, 0.58, 0.28), (0.86, 0.76, 0.42))
            b.blob(tip, 0.035, (0.96, 0.86, 0.52), (0.66, 0.54, 0.28), scale=(0.7, 0.7, 1.6), sub=1)
    return _finish(b, "tuft")


# --------------------------------------------------------------- rocks

STONE_T, STONE_B = (0.62, 0.60, 0.57), (0.22, 0.21, 0.25)


def _stone(b, cen, r, sc, warm=0.0, shade=1.0):
    top = np.asarray(STONE_T) * (1 - warm) + np.asarray((0.72, 0.60, 0.46)) * warm
    k = b.rnd.uniform(0.92, 1.05)
    b.blob(cen, r, _c(top, k), _c(STONE_B, k), scale=sc, rough=0.22, sub=2, shade=shade)


def rock(variant=0):
    """0 a cluster of pebbles, 1 a small rock, 2 a mossy boulder (< 1 tile)."""
    b = _B(920 + variant)
    r = b.rnd
    v = variant % 3
    if v == 0:
        for i in range(7):
            a = i * 2.3 + r.uniform(-0.4, 0.4)
            o = r.uniform(0.10, 0.26) if i else 0.0
            rad = r.uniform(0.07, 0.12) if i else 0.15
            _stone(b, (math.cos(a) * o, math.sin(a) * o, rad * 0.3), rad,
                   (r.uniform(1.0, 1.4), r.uniform(0.8, 1.1), 0.6), warm=r.uniform(0, 0.6))
        for i in range(3):
            a = r.uniform(0, 2 * math.pi)
            _blades(b, math.cos(a) * 0.2, math.sin(a) * 0.15, 4, 0.12, GRASS_B, GRASS_T)
    elif v == 1:
        _stone(b, (0, 0, 0.08), 0.24, (1.25, 1.0, 0.8), warm=0.15)
        _stone(b, (0.20, -0.12, 0.03), 0.08, (1.2, 1.0, 0.6), warm=0.5)
        _stone(b, (-0.17, 0.14, 0.02), 0.06, (1.2, 1.0, 0.6), warm=0.1)
        for i in range(4):
            a = r.uniform(0, 2 * math.pi)
            _blades(b, math.cos(a) * 0.24, math.sin(a) * 0.2, 5, 0.14, GRASS_B, GRASS_T)
    else:
        _stone(b, (0, 0, 0.14), 0.36, (1.15, 0.95, 0.8), warm=0.15)
        _stone(b, (0.26, 0.18, 0.08), 0.18, (1.1, 1.0, 0.8), warm=0.1)
        # moss on the sunny crown
        b.blob((-0.03, -0.05, 0.38), 0.24, (0.50, 0.70, 0.22), (0.26, 0.44, 0.16),
               scale=(1.25, 1.0, 0.32), sub=2, rough=0.15)
        b.clumps((-0.03, -0.05, 0.38), 0.22, 9, 0.06, (0.56, 0.76, 0.26), (0.28, 0.46, 0.16),
                 scale=(1.2, 1.0, 0.3), stretch=(1, 1, 0.6))
        _stone(b, (-0.30, -0.14, 0.03), 0.07, (1.2, 1.0, 0.6), warm=0.3)
        for i in range(6):
            a = r.uniform(0, 2 * math.pi)
            _blades(b, math.cos(a) * 0.36, math.sin(a) * 0.30, 6, 0.18, GRASS_B, GRASS_T)
    return _finish(b, "rock")


# --------------------------------------------------------------- bushes

BUSHES = [  # leaf top, leaf bottom, bloom (or None)
    ((0.42, 0.68, 0.22), (0.08, 0.28, 0.12), None),
    ((0.36, 0.62, 0.26), (0.08, 0.28, 0.14), ((1.00, 0.46, 0.66), (0.80, 0.22, 0.44))),
    ((0.52, 0.70, 0.20), (0.14, 0.34, 0.10), ((1.00, 0.86, 0.10), (0.88, 0.58, 0.04))),
    ((0.34, 0.60, 0.24), (0.07, 0.26, 0.14), ((1.00, 1.00, 0.97), (0.78, 0.80, 0.86))),
]


def bush(variant=0):
    """A round shrub ~0.8 across and ~0.55 tall: 0 plain myrtle, 1 pink
    oleander, 2 yellow broom, 3 white blossom."""
    v = variant % 4
    b = _B(930 + variant)
    r = b.rnd
    top, bot, bloom = BUSHES[v]
    lobes = [(0.0, 0.0, 0.26, 0.27)]
    for i in range(5):
        a = i * 2 * math.pi / 5 + r.uniform(-0.3, 0.3)
        o = r.uniform(0.17, 0.24)
        lobes.append((math.cos(a) * o, math.sin(a) * o * 0.85, r.uniform(0.15, 0.22), r.uniform(0.17, 0.21)))
    for x, y, z, rad in lobes:
        kk = r.uniform(0.92, 1.08)
        sc = (1, 1, 0.9)
        b.blob((x, y, z), rad, _c(top, kk), _c(bot, kk), scale=sc, rough=0.18)
        b.clumps((x, y, z), rad, 9, rad * 0.42, _c(top, kk * 1.06), _c(bot, kk * 1.2), scale=sc,
                 stretch=(1, 1, 0.85))
        b.leaves((x, y, z), rad, 10, 0.08, _c(top, 1.12), scale=sc, width=0.6)
        if bloom:
            for _ in range(6):
                d = b.np.randn(3)
                d[2] = abs(d[2]) + 0.4
                d /= np.linalg.norm(d)
                p = np.array((x, y, z)) + d * rad * np.array(sc) * 1.02
                br = 0.042 if v != 2 else 0.036
                b.blob(p, br, _c(bloom[0], r.uniform(0.95, 1.05)), _c(bloom[1]), sub=1, rough=0.12,
                       scale=(1, 1, 0.8))
    for i in range(6):
        a = r.uniform(0, 2 * math.pi)
        _blades(b, math.cos(a) * 0.36, math.sin(a) * 0.30, 4, 0.15, GRASS_B, GRASS_T)
    return _finish(b, "bush")


# --------------------------------------------------------------- reeds

def reeds(variant=0):
    """0 a clump of reeds with cattails, 1 papyrus. ~0.6 across, ~0.9 tall."""
    b = _B(940 + variant)
    r = b.rnd
    if variant % 2 == 0:
        for k in range(40):
            a = r.uniform(0, 2 * math.pi)
            o = abs(r.gauss(0, 0.11))
            x, y = math.cos(a) * o, math.sin(a) * o
            hh = r.uniform(0.40, 0.85)
            la = r.uniform(0, 2 * math.pi)
            ln = r.uniform(0.05, 0.3) * hh
            kk = r.uniform(0.85, 1.15)
            b.blade((x, y, -0.02), (x + math.cos(la) * ln, y + math.sin(la) * ln, hh), 0.045,
                    _c((0.18, 0.44, 0.14), kk), _c((0.64, 0.80, 0.28), kk))
        for k in range(5):                               # cattails
            a, o = r.uniform(0, 2 * math.pi), r.uniform(0, 0.13)
            x, y = math.cos(a) * o, math.sin(a) * o
            hh = r.uniform(0.75, 0.95)
            b.tube([(x, y, 0), (x + 0.02, y, hh)], [0.013, 0.011], (0.56, 0.70, 0.28),
                   (0.30, 0.46, 0.18), segs=4)
            b.tube([(x + 0.02, y, hh - 0.2), (x + 0.02, y, hh - 0.03)], [0.035, 0.035],
                   (0.62, 0.36, 0.18), (0.30, 0.17, 0.10), segs=6)
    else:
        for k in range(18):                              # low leaves
            a = r.uniform(0, 2 * math.pi)
            o = abs(r.gauss(0, 0.08))
            x, y = math.cos(a) * o, math.sin(a) * o
            b.blade((x, y, -0.02), (x + math.cos(a) * 0.12, y + math.sin(a) * 0.12, r.uniform(0.2, 0.35)),
                    0.05, (0.18, 0.44, 0.14), (0.52, 0.74, 0.24))
        for k in range(7):                               # stems with umbels
            a, o = k * 0.9 + r.uniform(0, 0.5), r.uniform(0.02, 0.14)
            x, y = math.cos(a) * o, math.sin(a) * o
            hh = r.uniform(0.6, 0.95)
            tx, ty = x + math.cos(a) * 0.12, y + math.sin(a) * 0.12
            b.tube([(x, y, 0), ((x + tx) / 2, (y + ty) / 2, hh * 0.6), (tx, ty, hh)],
                   [0.018, 0.015, 0.012], (0.50, 0.72, 0.26), (0.24, 0.46, 0.18), segs=4)
            for j in range(18):
                d = b.np.randn(3)
                d[2] = abs(d[2]) * 0.6 - 0.05
                d /= np.linalg.norm(d)
                tip = np.array([tx, ty, hh]) + d * r.uniform(0.13, 0.19)
                b.blade((tx, ty, hh), tip, 0.03, (0.40, 0.64, 0.22), (0.84, 0.88, 0.40))
    for i in range(5):
        a = r.uniform(0, 2 * math.pi)
        _blades(b, math.cos(a) * 0.22, math.sin(a) * 0.2, 4, 0.14, GRASS_B, GRASS_T)
    return _finish(b, "reeds")


# --------------------------------------------------------------- the tree

def tree(variant=0):
    """A broadleaf for the forests (one per tile, in dense stands): a short
    trunk, a full crown of layered leaf masses ~1.3 tiles wide, ~2 tall,
    lit gold-green on top and deep green underneath, slightly irregular."""
    b = _B(950 + variant)
    r = b.rnd
    bark_l, bark_d = (0.58, 0.44, 0.32), (0.26, 0.18, 0.13)
    lean = r.uniform(0, 2 * math.pi)
    top = np.array([math.cos(lean) * 0.05, math.sin(lean) * 0.05, 0.70])
    b.tube([(0, 0, -0.05), (0.01, -0.01, 0.30), top], [0.13, 0.10, 0.085], bark_l, bark_d, segs=8,
           lump=0.08)
    for k in range(4):                                   # root flare
        a = k * 1.57 + r.uniform(0, 0.5)
        b.tube([(math.cos(a) * 0.22, math.sin(a) * 0.22, -0.03),
                (math.cos(a) * 0.10, math.sin(a) * 0.10, 0.06), (0, 0, 0.22)],
               [0.025, 0.05, 0.04], bark_l, bark_d, segs=5)
    for k in range(3):                                   # limbs into the crown
        a = lean + k * 2.1 + r.uniform(-0.3, 0.3)
        end = top + (math.cos(a) * 0.32, math.sin(a) * 0.32, 0.38)
        b.tube([top, (top + end) / 2 + (0, 0, -0.03), end], [0.07, 0.05, 0.035], bark_l, bark_d,
               segs=5)
    # the crown: a lower skirt of masses, a middle ring, a top, a little
    # lopsided; each mass a lumpy ball studded with smaller clumps and a
    # fringe of leaf cards, lit gold-green on top and deep green beneath
    cz = 1.30
    sk = np.array([math.cos(lean + 1.0), math.sin(lean + 1.0)]) * 0.06      # lopsided
    masses = []
    for i in range(7):                                   # lower ring
        a = i * 2 * math.pi / 7 + r.uniform(-0.25, 0.25)
        o = r.uniform(0.50, 0.60)
        masses.append((math.cos(a) * o + sk[0], math.sin(a) * o + sk[1], cz - 0.30 + r.uniform(-0.08, 0.06),
                       r.uniform(0.28, 0.34)))
    for i in range(5):                                   # upper ring
        a = i * 2 * math.pi / 5 + 0.6 + r.uniform(-0.3, 0.3)
        o = r.uniform(0.30, 0.40)
        masses.append((math.cos(a) * o, math.sin(a) * o, cz + 0.16 + r.uniform(-0.05, 0.08),
                       r.uniform(0.28, 0.34)))
    masses.append((r.uniform(-0.08, 0.08), r.uniform(-0.08, 0.08), cz + 0.46, 0.30))   # top
    masses.append((0, 0, cz - 0.02, 0.56))               # the core, filling gaps
    for x, y, z, rad in masses:
        h = min(1.0, max(0.0, (z - (cz - 0.40)) / 0.9))  # 0 low .. 1 top
        kk = r.uniform(0.86, 1.1)
        hue = r.random()                                 # some masses yellower, some bluer
        tc = (0.26, 0.48, 0.10) if hue < 0.5 else (0.20, 0.44, 0.14)
        topc = _c(tc, (0.75 + 0.45 * h) * kk)
        botc = _c((0.02, 0.10, 0.05), (0.80 + 0.4 * h) * kk)
        sc = (1.0, 1.0, 0.80)
        b.blob((x, y, z), rad, topc, botc, scale=sc, rough=0.18, shade=0.95)
        b.clumps((x, y, z), rad, 14, rad * 0.36, _c(topc, 1.08), _c(botc, 1.0), scale=sc,
                 stretch=(1, 1, 0.8))
        b.leaves((x, y, z), rad, 14, 0.10, _c((0.46, 0.68, 0.20), 0.7 + 0.35 * h), scale=sc,
                 width=0.55)
    return _finish(b, "tree")


# --------------------------------------------------------------- previews

def pv_all():
    """Every decor prop in a row, and a tree, for preview.py."""
    root = empty("decor")
    items = [(flowers, 5), (tuft, 3), (rock, 3), (bush, 4), (reeds, 2)]
    x = -5.0
    for fn, n in items:
        for v in range(n):
            ob = fn(v)
            ob.parent = root
            ob.location = (x, -x * 0.0, 0)
            x += 0.6
    return root


# --------------------------------------------------------------- export all

SETS = ([("decor_flowers_%d" % v, "flowers", v) for v in range(5)]
        + [("decor_tuft_%d" % v, "tuft", v) for v in range(3)]
        + [("decor_rock_%d" % v, "rock", v) for v in range(3)]
        + [("decor_bush_%d" % v, "bush", v) for v in range(4)]
        + [("decor_reeds_%d" % v, "reeds", v) for v in range(2)]
        + [("tree", "tree", 0)])
SUN_ANGLE = 12        # degrees: a soft shadow edge for the small things


def export_all(names=None, debug=None):
    """Export every set (or those named) one after another, then check each."""
    import subprocess
    here = os.path.dirname(os.path.abspath(__file__))
    for name, fn, v in SETS:
        if names and name not in names:
            continue
        cmd = [sys.executable, os.path.join(here, "export.py"), name, "decor." + fn,
               "--class", "SmallBuilding", "--kwargs", '{"variant": %d}' % v,
               "--sun-angle", str(SUN_ANGLE if fn != "tree" else 6)]
        if debug:
            cmd += ["--debug", debug]
        out = subprocess.run(cmd, capture_output=True, text=True)
        lines = [ln for ln in out.stdout.splitlines() if ln.startswith(("frame", "ground", "  **", "wrote"))]
        print("\n".join(lines))
        if out.returncode:
            print(out.stdout[-2000:], out.stderr[-2000:])
            raise SystemExit("export of %s failed" % name)
        root = os.path.dirname(os.path.dirname(here))
        chk = [sys.executable, os.path.join(here, "check_set.py"),
               os.path.join(root, "assets", "look", name)] + (["--out", debug] if debug else [])
        c = subprocess.run(chk, capture_output=True, text=True)
        print(c.stdout.strip().splitlines()[-1] if c.stdout.strip() else "", c.stderr.strip())
        if c.returncode:
            raise SystemExit("check of %s failed" % name)


if __name__ == "__main__":
    import argparse
    ap = argparse.ArgumentParser(description="export the decor sets and the tree")
    ap.add_argument("names", nargs="*")
    ap.add_argument("--debug")
    a = ap.parse_args()
    export_all(a.names, a.debug)

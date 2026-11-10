"""The landscape for the new look: painted ground, sea, fields, vegetation.

    /usr/bin/python3.13 tools/look/preview.py terrain.build out.png 22 1000

`build()` makes the whole ground for the square x, y in [-16, 16] (it owns the
ground: OWN_GROUND). The props (`olive`, `cypress`, `palm`, `bush`, `rock`,
`reeds`, `flowers`, `wheat_patch`) each build alone at the origin and take
`loc=(x, y, 0), seed=0, scale=1.0, parent=None`.

The ground colour is painted into one image with numpy (zones, noise, foam);
props are single meshes with vertex colours (light on top, dark below), so a
tree is one object and a repeated seed shares its mesh.
"""
import math
import random

import bpy
import bmesh
import numpy as np

from lib import empty

OWN_GROUND = True

HALF = 16.0          # the ground covers [-HALF, HALF] squared
TEX = 2048           # ground painting resolution (pixels across the square)
WATER_Z = -0.05

GREEK = (-4.0, -2.0, 3.6)
EGYPT = (2.0, 4.5, 3.6)
BATTLE = (3.0, -3.0, 2.6)
WHEAT = (-8.2, -4.6, 3.0, 2.5)      # centre x, y, size x, y
PLOT = (5.2, 7.2, 2.5, 2.0)
SHORE = 9.2                          # sea where y - x > SHORE (+ wobble)

_F = np.float32
_L = np.array([0.30, -0.80, 0.80])
_L = _L / np.linalg.norm(_L)         # toward the sun


# --- small maths -----------------------------------------------------------

def _ss(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


_tables = {}


def _vnoise(x, y, seed):
    if seed not in _tables:
        _tables[seed] = np.random.RandomState(1000 + seed).rand(256, 256).astype(_F)
    p = _tables[seed]
    xf, yf = np.floor(x), np.floor(y)
    xi, yi = xf.astype(np.int32), yf.astype(np.int32)
    fx, fy = (x - xf).astype(_F), (y - yf).astype(_F)
    fx = fx * fx * (3 - 2 * fx)
    fy = fy * fy * (3 - 2 * fy)
    x0, x1, y0, y1 = xi & 255, (xi + 1) & 255, yi & 255, (yi + 1) & 255
    a = p[y0, x0]
    a = a + (p[y0, x1] - a) * fx
    c = p[y1, x0]
    c = c + (p[y1, x1] - c) * fx
    return a + (c - a) * fy


def _fbm(x, y, scale, seed, octaves=3):
    """Value-noise fbm, about 0..1 and stretched to use the range."""
    x = np.asarray(x, dtype=_F)
    y = np.asarray(y, dtype=_F)
    tot, amp, norm, f = 0.0, 1.0, 0.0, 1.0 / scale
    for o in range(octaves):
        tot = tot + amp * _vnoise(x * f + 17.3 * o, y * f - 9.1 * o, seed + o)
        norm += amp
        amp *= 0.5
        f *= 2.03
    return np.clip((tot / norm - 0.5) * 2.0 + 0.5, 0.0, 1.0)


def _spline(ctrl, step=0.35):
    """Catmull-Rom through the control points, resampled."""
    p = [np.array(c, dtype=float) for c in ctrl]
    p = [p[0] * 2 - p[1]] + p + [p[-1] * 2 - p[-2]]
    out = []
    for i in range(1, len(p) - 2):
        p0, p1, p2, p3 = p[i - 1], p[i], p[i + 1], p[i + 2]
        n = max(2, int(np.linalg.norm(p2 - p1) / step))
        for k in range(n):
            t = k / n
            out.append(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t
                              + (-p0 + 3 * p1 - 3 * p2 + p3) * t ** 3))
    out.append(p[-2])
    return np.array(out)


ROAD_MAIN = _spline([(-4, -2), (-2.9, 0.3), (-1.0, 1.0), (-0.2, 2.6), (2, 4.5)])
ROAD_WEST = _spline([(-4, -2), (-5.3, -4.6), (-6.0, -6.7), (-8.4, -7.6), (-10.4, -7.8),
                     (-12, -9), (-14, -11.2), (-16.5, -12.5)])
ROAD_EAST = _spline([(2, 4.5), (5.2, 3.5), (7.6, 4.2), (10, 3.5), (13, 4.4), (16.5, 4.0)])
CHANNEL = _spline([(-0.2, 12.6), (0.9, 11.6), (2.3, 10.1), (3.3, 8.9), (3.8, 7.8), (4.3, 7.25),
                   (5.2, 7.2), (6.45, 7.2)], step=0.2)


def _seg_d(x, y, a, b):
    ab = b - a
    t = np.clip(((x - a[0]) * ab[0] + (y - a[1]) * ab[1]) / max(1e-9, ab @ ab), 0, 1)
    return np.hypot(x - (a[0] + t * ab[0]), y - (a[1] + t * ab[1]))


def _poly_dist(x, y, pts, pad=2.6):
    """Distance to a polyline. For a big painting grid only a band around the
    line is computed (the rest is 'far': 1e3)."""
    x = np.asarray(x, dtype=_F)
    y = np.asarray(y, dtype=_F)
    if x.ndim == 2 and x.size > 200000:
        t = x.shape[0]
        d = np.full(x.shape, 1e3, dtype=_F)
        for a, b in zip(pts[:-1], pts[1:]):
            j0 = int((min(a[0], b[0]) - pad + HALF) / (2 * HALF) * t)
            j1 = int((max(a[0], b[0]) + pad + HALF) / (2 * HALF) * t) + 1
            i0 = int((min(a[1], b[1]) - pad + HALF) / (2 * HALF) * t)
            i1 = int((max(a[1], b[1]) + pad + HALF) / (2 * HALF) * t) + 1
            j0, j1, i0, i1 = max(j0, 0), min(j1, t), max(i0, 0), min(i1, t)
            if j1 <= j0 or i1 <= i0:
                continue
            sl = (slice(i0, i1), slice(j0, j1))
            d[sl] = np.minimum(d[sl], _seg_d(x[sl], y[sl], a, b))
        return d
    a, b = pts[:-1], pts[1:]
    ab = b - a
    den = np.maximum(1e-9, (ab * ab).sum(1))
    xf, yf = x.ravel(), y.ravel()
    out = np.empty(xf.shape, dtype=_F)
    step = max(1, 4000000 // len(a))
    for i in range(0, len(xf), step):
        px, py = xf[i:i + step, None], yf[i:i + step, None]
        t = np.clip(((px - a[:, 0]) * ab[:, 0] + (py - a[:, 1]) * ab[:, 1]) / den, 0, 1)
        out[i:i + step] = np.hypot(px - (a[:, 0] + t * ab[:, 0]), py - (a[:, 1] + t * ab[:, 1])).min(1)
    return out.reshape(x.shape)


def _rect_d(x, y, r, grow=0.0):
    """Distance outside a rectangle (0 inside)."""
    dx = np.abs(x - r[0]) - r[2] / 2 - grow
    dy = np.abs(y - r[1]) - r[3] / 2 - grow
    return np.hypot(np.maximum(dx, 0), np.maximum(dy, 0)) + np.minimum(np.maximum(dx, dy), 0)


def _shore(x, y):
    """Signed across-shore distance in (y - x) units: > 0 is sea."""
    u = x + y
    wav = (0.42 * np.sin(u * 0.47 + 0.6) + 0.22 * np.sin(u * 1.21 + 2.0)
           + 0.10 * np.sin(u * 2.9 + 1.0) + 0.45 * np.exp(-((u + 5.5) / 1.6) ** 2)
           - 0.5 * np.exp(-((u - 4.0) / 2.2) ** 2))
    return (y - x) - SHORE - wav


def _zones(x, y):
    """Everything the painter and the scatterer ask about a place."""
    x = np.asarray(x, dtype=_F)
    y = np.asarray(y, dtype=_F)
    z = {}
    z["s"] = _shore(x, y)
    z["dg"] = np.hypot(x - GREEK[0], y - GREEK[1]) - GREEK[2]
    z["de"] = np.hypot(x - EGYPT[0], y - EGYPT[1]) - EGYPT[2]
    z["db"] = np.hypot(x - BATTLE[0], y - BATTLE[1]) - BATTLE[2]
    z["road"] = np.minimum(_poly_dist(x, y, ROAD_MAIN), _poly_dist(x, y, ROAD_WEST))
    z["road2"] = _poly_dist(x, y, ROAD_EAST)
    z["chan"] = _poly_dist(x, y, CHANNEL)
    z["wheat"] = _rect_d(x, y, WHEAT)
    z["plot"] = _rect_d(x, y, PLOT)
    return z


def ground_z(x, y):
    """Ground height at (x, y): 0 on every flat zone, a low roll elsewhere,
    dipping under the sea past the shore. Scalars or arrays."""
    scalar = np.isscalar(x)
    x = np.atleast_1d(np.asarray(x, dtype=_F))
    y = np.atleast_1d(np.asarray(y, dtype=_F))
    z = _zones(x, y)
    flat = (_ss(0.3, 2.6, z["dg"]) * _ss(0.3, 2.6, z["de"]) * _ss(0.3, 2.4, z["db"])
            * _ss(0.6, 2.0, z["road"]) * _ss(0.5, 2.0, z["road2"]) * _ss(0.3, 1.6, z["chan"])
            * _ss(0.3, 1.8, z["wheat"]) * _ss(0.3, 1.8, z["plot"]) * _ss(-1.6, -4.0, z["s"]))
    h = 0.42 * (_fbm(x, y, 5.5, 40, 2) - 0.42) * flat
    h = h - 0.05 * _ss(-1.3, 0.0, z["s"]) - 0.40 * _ss(0.0, 1.6, z["s"])
    return float(h[0]) if scalar else h


def is_clear(x, y, margin=0.0):
    """True where nothing may be planted: plazas, battle ground, fields,
    roads, channel, sea. Scalars or arrays."""
    scalar = np.isscalar(x)
    x = np.atleast_1d(np.asarray(x, dtype=_F))
    y = np.atleast_1d(np.asarray(y, dtype=_F))
    z = _zones(x, y)
    c = ((z["dg"] < margin) | (z["de"] < margin) | (z["db"] < margin)
         | (z["road"] < 0.48 + margin) | (z["road2"] < 0.4 + margin)
         | (z["chan"] < 0.24 + margin) | (z["wheat"] < 0.12 + margin)
         | (z["plot"] < 0.12 + margin) | (z["s"] > -0.05))
    return bool(c[0]) if scalar else c


# --- the painting ----------------------------------------------------------

def _mix(c, col, m):
    c += (np.array(col, dtype=_F) - c) * m[..., None]


def _paint(t=TEX):
    """The ground as a (t, t, 3) sRGB array; row 0 is y = -HALF."""
    ax = (np.arange(t, dtype=_F) + 0.5) / t * 2 * HALF - HALF
    x, y = np.meshgrid(ax, ax)
    u, far = x + y, y - x
    # broad noise is made at quarter size and stretched up: four times cheaper
    q = t // 4
    ax4 = (np.arange(q, dtype=_F) + 0.5) / q * 2 * HALF - HALF
    x4, y4 = np.meshgrid(ax4, ax4)
    u4, far4 = x4 + y4, y4 - x4
    pos = np.clip((np.arange(t) + 0.5) / 4 - 0.5, 0, q - 1)
    i0 = np.floor(pos).astype(np.int32)
    i1 = np.minimum(i0 + 1, q - 1)
    w = (pos - i0).astype(_F)

    def up(a):
        a = a[i0] * (1 - w[:, None]) + a[i1] * w[:, None]
        return a[:, i0] * (1 - w[None, :]) + a[:, i1] * w[None, :]
    z = _zones(x, y)
    s = z["s"]

    # grass: big patches of dark / mid / light / dry, then mottling and strokes
    n1 = up(_fbm(x4, y4, 5.0, 1, 4))
    n2 = up(_fbm(x4, y4, 1.7, 5, 3))
    n3 = up(_fbm(x4, y4, 6.5, 9, 3))
    n4 = up(_fbm(x4, y4, 0.45, 13, 3))
    stroke = _fbm(u / 0.05, far / 0.32, 1.0, 17, 2)
    grain = _fbm(x, y, 0.05, 21, 2)
    c = np.empty((t, t, 3), dtype=_F)
    c[:] = (0.34, 0.62, 0.18)
    _mix(c, (0.17, 0.44, 0.14), _ss(0.50, 0.22, n1) * 0.9)
    _mix(c, (0.56, 0.76, 0.24), _ss(0.52, 0.80, n1) * 0.9)
    _mix(c, (0.24, 0.54, 0.20), _ss(0.55, 0.85, n2) * 0.6)
    _mix(c, (0.50, 0.72, 0.22), _ss(0.45, 0.15, n2) * 0.5)
    dry = _ss(0.60, 0.80, n3) * (0.55 + 0.45 * n4)
    _mix(c, (0.76, 0.72, 0.32), dry * 0.85)
    _mix(c, (0.20, 0.48, 0.20), _ss(0.62, 0.80, n4) * 0.45)      # clover-dark clumps
    _mix(c, (0.66, 0.80, 0.30), _ss(0.36, 0.20, n4) * 0.40)      # sun-bleached dabs
    c *= (0.86 + 0.28 * stroke)[..., None]
    c *= (0.93 + 0.14 * grain)[..., None]

    def edge(d, soft=0.28, wob=0.55, seed=30):
        """A soft, clumpy edge: 1 inside (d < 0), grass creeping in."""
        w = (up(_fbm(x4, y4, 1.3, seed, 3)) - 0.5) * wob + (n4 - 0.5) * 0.5 + (grain - 0.5) * 0.25
        return _ss(soft, -soft, d + w)

    def earth(m, light, mid, dark, seed):
        e = np.empty((t, t, 3), dtype=_F)
        e[:] = mid
        _mix(e, light, _ss(0.45, 0.80, up(_fbm(x4, y4, 2.2, seed, 3))) * 0.8)
        _mix(e, dark, _ss(0.50, 0.22, up(_fbm(x4, y4, 1.1, seed + 4, 3))) * 0.6)
        e *= (0.90 + 0.20 * n4)[..., None]
        e *= (0.92 + 0.16 * grain)[..., None]
        peb = _fbm(x, y, 0.07, seed + 8, 1)
        _mix(e, (0.93, 0.88, 0.74), _ss(0.90, 0.96, peb) * 0.4)   # pale pebbles
        _mix(e, (0.46, 0.34, 0.24), _ss(0.10, 0.05, peb) * 0.4)   # dark flecks
        # a thin dark lip where the turf stops
        c[:] = c * (1 - 0.30 * (_ss(0.05, 0.4, m) * _ss(0.75, 0.4, m)))[..., None]
        c[:] = c + (e - c) * m[..., None]

    # sand beach, blending into the grass
    sand_m = _ss(-2.05, -1.25, s + (up(_fbm(x4, y4, 1.6, 50, 3)) - 0.5) * 0.9 + (n4 - 0.5) * 0.5)
    sand = np.empty((t, t, 3), dtype=_F)
    sand[:] = (0.95, 0.85, 0.60)
    _mix(sand, (0.99, 0.92, 0.72), _ss(0.5, 0.85, up(_fbm(x4, y4, 1.2, 54, 3))) * 0.8)
    _mix(sand, (0.86, 0.72, 0.48), _ss(0.5, 0.2, up(_fbm(x4, y4, 0.8, 57, 3))) * 0.6)
    rip = np.sin((s + 0.25 * up(_fbm(x4, y4, 0.9, 58, 2))) * 34.0)
    sand *= (0.97 + 0.035 * rip * _ss(-1.2, -0.3, s))[..., None]   # wind ripples
    sand *= (0.93 + 0.14 * grain)[..., None]
    speck = _fbm(x, y, 0.06, 59, 1)
    _mix(sand, (0.62, 0.52, 0.42), _ss(0.90, 0.94, speck) * 0.8)   # pebbles, shells
    _mix(sand, (1.0, 0.98, 0.92), _ss(0.08, 0.05, speck) * 0.9)
    _mix(sand, (0.74, 0.62, 0.42), _ss(-0.42, -0.02, s) * 0.75)    # wet sand
    c[:] = c + (sand - c) * sand_m[..., None]

    # the Greek plaza, the Egyptian plaza (sandier), the roads, the battle ground
    earth(edge(z["dg"], seed=30), (0.90, 0.78, 0.56), (0.80, 0.63, 0.41), (0.62, 0.45, 0.29), 60)
    earth(edge(z["de"], seed=33), (0.95, 0.84, 0.58), (0.86, 0.70, 0.44), (0.68, 0.50, 0.30), 70)
    rm = edge(z["road"] - 0.40, soft=0.14, wob=0.30, seed=36)
    earth(rm, (0.88, 0.75, 0.52), (0.76, 0.59, 0.38), (0.58, 0.42, 0.27), 80)
    rm2 = edge(z["road2"] - 0.30, soft=0.14, wob=0.30, seed=38)
    earth(rm2, (0.90, 0.78, 0.54), (0.78, 0.62, 0.40), (0.60, 0.44, 0.28), 84)
    for d, hw in ((z["road"], 0.40), (z["road2"], 0.30)):          # ruts, grassy crown
        rut = np.exp(-((d - hw * 0.52) / 0.045) ** 2) * (0.5 + 0.5 * n4)
        c *= (1 - 0.22 * rut * _ss(0.3, -0.3, z["dg"] * 0 + d - hw))[..., None]
    bm = edge(z["db"], soft=0.45, wob=0.9, seed=41)
    earth(bm, (0.74, 0.62, 0.36), (0.56, 0.41, 0.25), (0.36, 0.25, 0.17), 90)
    churn = _fbm(x, y, 0.35, 95, 3)
    _mix(c, (0.72, 0.66, 0.30), bm * _ss(0.55, 0.75, up(_fbm(x4, y4, 0.9, 96, 3))) * 0.75)  # dry grass
    _mix(c, (0.30, 0.21, 0.15), bm * _ss(0.62, 0.80, churn) * 0.75)                   # gouges
    _mix(c, (0.70, 0.56, 0.36), bm * _ss(0.36, 0.22, churn) * 0.5)                    # scuffs

    # wheat field: stubble-gold ground with rows
    wm = _ss(0.10, -0.06, z["wheat"] + (grain - 0.5) * 0.1)
    row = 0.5 + 0.5 * np.sin((y - WHEAT[1]) * (2 * math.pi / 0.2))
    wg = np.empty((t, t, 3), dtype=_F)
    wg[:] = (0.86, 0.66, 0.24)
    _mix(wg, (0.60, 0.42, 0.20), row * 0.7)
    wg *= (0.9 + 0.2 * n4)[..., None]
    c[:] = c + (wg - c) * wm[..., None]

    # irrigated plot: dark wet soil in beds, little ditches between them
    pm = _ss(0.10, -0.06, z["plot"] + (grain - 0.5) * 0.1)
    pg = np.empty((t, t, 3), dtype=_F)
    pg[:] = (0.40, 0.29, 0.19)
    _mix(pg, (0.30, 0.22, 0.16), _ss(0.4, 0.7, n4) * 0.7)
    bed = np.abs(((x - PLOT[0] + 0.21) % 0.42) - 0.21)             # ditches every 0.42
    _mix(pg, (0.22, 0.50, 0.58), _ss(0.045, 0.02, bed))
    _mix(pg, (0.26, 0.19, 0.14), _ss(0.085, 0.05, bed) * _ss(0.02, 0.045, bed) * 0.8)
    c[:] = c + (pg - c) * pm[..., None]

    # the channel: lush banks, wet mud, water
    ch = z["chan"]
    land = _ss(0.1, -0.2, s)
    _mix(c, (0.22, 0.56, 0.16), _ss(0.95, 0.30, ch + (n4 - 0.5) * 0.4) * land * (1 - pm) * 0.8)
    _mix(c, (0.36, 0.27, 0.18), _ss(0.27, 0.19, ch + (grain - 0.5) * 0.06) * land)
    _mix(c, (0.20, 0.58, 0.66), _ss(0.15, 0.12, ch) * land)
    _mix(c, (0.42, 0.78, 0.80), _ss(0.07, 0.0, ch) * land * 0.6)

    # the sea: sandy shallows -> turquoise -> blue; seabed, caustics, foam, glints
    sw = s + (up(_fbm(x4, y4, 0.5, 100, 2)) - 0.5) * 0.10
    knots = [-0.05, 0.12, 0.6, 1.6, 3.2, 6.5, 12.0]
    cols = [(0.86, 0.84, 0.62), (0.62, 0.88, 0.74), (0.24, 0.82, 0.78), (0.10, 0.66, 0.80),
            (0.06, 0.50, 0.78), (0.05, 0.38, 0.70), (0.04, 0.30, 0.62)]
    sea = np.stack([np.interp(sw, knots, [k[i] for k in cols]).astype(_F) for i in range(3)], -1)
    bedn = up(_fbm(x4, y4, 1.0, 104, 3))
    shallow = _ss(3.0, 0.2, sw)
    _mix(sea, (0.06, 0.42, 0.50), _ss(0.58, 0.80, bedn) * shallow * 0.55)       # weed and rock
    _mix(sea, (0.60, 0.92, 0.82), _ss(0.40, 0.20, bedn) * shallow * 0.45)       # bright sand
    cau = np.abs(up(_fbm(x4, y4, 0.42, 108, 2)) - 0.5)
    _mix(sea, (0.80, 0.98, 0.92), _ss(0.035, 0.0, cau) * _ss(3.5, 0.3, sw) * 0.55)
    swell = up(_fbm(u4 / 2.2, far4 / 0.5, 1.0, 112, 3))                               # long swells
    sea *= (0.90 + 0.20 * swell)[..., None]
    gl = _fbm(u / 0.5, far / 0.07, 1.0, 116, 2)
    _mix(sea, (1.0, 1.0, 1.0), _ss(0.88, 0.96, gl) * _ss(0.3, 1.5, sw) * _ss(0.35, 0.7, swell) * 0.6)  # glints
    fo = _fbm(x, y, 0.16, 120, 2)
    foam = _ss(0.11, 0.03, np.abs(sw - 0.04)) * (0.65 + 0.35 * fo)
    foam = np.maximum(foam, _ss(0.07, 0.02, np.abs(sw - 0.42 - 0.2 * (bedn - 0.5)))
                      * _ss(0.42, 0.62, up(_fbm(u4 / 1.5, far4 / 0.4, 1.0, 124, 2))))
    foam = np.maximum(foam, 0.6 * _ss(0.05, 0.015, np.abs(sw - 0.95 - 0.3 * (bedn - 0.5)))
                      * _ss(0.50, 0.66, up(_fbm(u4 / 1.2, far4 / 0.4, 1.0, 128, 2))))
    foam = np.maximum(foam, _ss(0.30, 0.0, sw) * _ss(0.55, 0.75, fo) * 0.8)     # lacy edge
    _mix(sea, (0.98, 1.0, 1.0), np.clip(foam, 0, 1))
    c[:] = c + (sea - c) * _ss(-0.02, 0.02, sw)[..., None]

    # painted speckle on the turf: tiny flowers in drifts, far cheaper than geometry
    turf = ((1 - sand_m) * (z["dg"] > 0.3) * (z["de"] > 0.3) * (z["db"] > 0.4) * (1 - rm) * (1 - rm2)
            * (z["wheat"] > 0.2) * (z["plot"] > 0.2) * (s < -1.2))
    rng = np.random.RandomState(7)
    dots = rng.rand(t, t).astype(_F)
    for col, seed, th in (((0.98, 0.90, 0.30), 140, 0.9965), ((0.98, 0.97, 0.92), 144, 0.9965),
                          ((0.92, 0.22, 0.16), 148, 0.997), ((0.60, 0.46, 0.86), 152, 0.998)):
        drift = _ss(0.55, 0.75, up(_fbm(x4, y4, 2.4, seed, 3)))
        hit = (np.roll(dots, seed, 0) > th - 0.012 * drift) & (turf > 0.5)
        hit = hit | np.roll(hit, 1, 0)
        c[hit] = col
    return np.clip(c, 0.0, 1.0)


# --- materials -------------------------------------------------------------

_cache = {}


def _alive(key):
    ob = _cache.get(key)
    if ob is None:
        return None
    try:
        ob.name
    except ReferenceError:
        del _cache[key]
        return None
    return ob


def _vmat(rough=0.9, name="vcol"):
    """Vertex-colour material (attribute 'Col')."""
    key = ("vmat", rough)
    m = _alive(key)
    if m:
        return m
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    b = nt.nodes["Principled BSDF"]
    a = nt.nodes.new("ShaderNodeAttribute")
    a.attribute_name = "Col"
    nt.links.new(a.outputs["Color"], b.inputs["Base Color"])
    b.inputs["Roughness"].default_value = rough
    _cache[key] = m
    return m


def _ground_image():
    img = _alive("img")
    if img:
        return img
    px = _paint(TEX)
    img = bpy.data.images.new("ground_paint", TEX, TEX, alpha=False)
    rgba = np.ones((TEX, TEX, 4), dtype=_F)
    rgba[..., :3] = px
    img.pixels.foreach_set(rgba.ravel())
    img.pack()
    _cache["img"] = img
    return img


def _ground_mat(water=False):
    key = ("gmat", water)
    m = _alive(key)
    if m:
        return m
    m = bpy.data.materials.new("water" if water else "ground")
    m.use_nodes = True
    nt = m.node_tree
    b = nt.nodes["Principled BSDF"]
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = _ground_image()
    tex.interpolation = "Cubic"
    tex.extension = "EXTEND"
    co = nt.nodes.new("ShaderNodeTexCoord")
    nt.links.new(co.outputs["UV"], tex.inputs["Vector"])
    noise = nt.nodes.new("ShaderNodeTexNoise")
    bump = nt.nodes.new("ShaderNodeBump")
    nt.links.new(co.outputs["Object"], noise.inputs["Vector"])
    nt.links.new(noise.outputs["Fac"], bump.inputs["Height"])
    nt.links.new(bump.outputs["Normal"], b.inputs["Normal"])
    if water:
        noise.inputs["Scale"].default_value = 3.2
        noise.inputs["Detail"].default_value = 3.0
        bump.inputs["Strength"].default_value = 0.22
        bump.inputs["Distance"].default_value = 0.3
        b.inputs["Roughness"].default_value = 0.10
        nt.links.new(tex.outputs["Color"], b.inputs["Base Color"])
    else:
        noise.inputs["Scale"].default_value = 26.0
        noise.inputs["Detail"].default_value = 4.0
        bump.inputs["Strength"].default_value = 0.12
        bump.inputs["Distance"].default_value = 0.04
        b.inputs["Roughness"].default_value = 1.0
        # fine grain so close-ups are not just blurred pixels
        ramp = nt.nodes.new("ShaderNodeMapRange")
        ramp.inputs["To Min"].default_value = 0.93
        ramp.inputs["To Max"].default_value = 1.07
        mul = nt.nodes.new("ShaderNodeMix")
        mul.data_type = "RGBA"
        mul.blend_type = "MULTIPLY"
        mul.inputs["Factor"].default_value = 1.0
        nt.links.new(noise.outputs["Fac"], ramp.inputs["Value"])
        nt.links.new(tex.outputs["Color"], mul.inputs["A"])
        nt.links.new(ramp.outputs["Result"], mul.inputs["B"])
        nt.links.new(mul.outputs["Result"], b.inputs["Base Color"])
    _cache[key] = m
    return m


# --- a mesh builder with vertex colours ------------------------------------

_ico = {}


def _icosphere(sub):
    if sub not in _ico:
        bm = bmesh.new()
        bmesh.ops.create_icosphere(bm, subdivisions=sub, radius=1.0)
        bm.verts.index_update()
        v = np.array([w.co[:] for w in bm.verts])
        f = [[w.index for w in face.verts] for face in bm.faces]
        bm.free()
        _ico[sub] = (v, f)
    return _ico[sub]


def _lin(c):
    c = np.asarray(c, dtype=float)
    return np.where(c > 0.04045, ((c + 0.055) / 1.055) ** 2.4, c / 12.92)


def _c(col, k=1.0):
    return np.clip(np.array(col, dtype=float) * k, 0, 1)


class _B:
    """Collects coloured geometry, then makes one object of it."""

    def __init__(self, seed=0):
        self.v, self.c, self.f = [], [], []
        self.n = 0
        self.rnd = random.Random(seed)
        self.np = np.random.RandomState(seed + 5)

    def add(self, verts, cols, faces):
        base = self.n
        self.v.append(np.asarray(verts, dtype=float).reshape(-1, 3))
        self.c.append(np.asarray(cols, dtype=float).reshape(-1, 3))
        self.n += len(self.v[-1])
        self.f.extend([[i + base for i in f] for f in faces])

    def blob(self, cen, r, top, bot, scale=(1, 1, 1), rough=0.22, sub=2, shade=0.75):
        """A lumpy ball, lit side `top` colour, shadow side `bot`."""
        v, f = _icosphere(sub)
        q = self.np.randn(4)
        q /= np.linalg.norm(q)
        a, b, c, d = q
        rot = np.array([[a*a+b*b-c*c-d*d, 2*(b*c-a*d), 2*(b*d+a*c)],
                        [2*(b*c+a*d), a*a-b*b+c*c-d*d, 2*(c*d-a*b)],
                        [2*(b*d-a*c), 2*(c*d+a*b), a*a-b*b-c*c+d*d]])
        n = v @ rot.T
        p = n * (1.0 + self.np.uniform(-rough, rough, (len(v), 1)))
        t = np.clip(0.5 + 0.5 * (n @ _L) * shade / 0.75 + self.np.uniform(-0.12, 0.12, len(v)), 0, 1)
        cols = np.outer(1 - t, bot) + np.outer(t, top)
        self.add(p * r * np.array(scale) + np.array(cen), cols, f)

    def leaves(self, cen, r, n, size, col, scale=(1, 1, 1), up=0.3, width=0.5, jit=0.18):
        """Little leaf cards sticking out of a ball: a broken, leafy outline."""
        for _ in range(n):
            d = self.np.randn(3)
            d[2] = abs(d[2]) * 0.8 + 0.15 if self.rnd.random() < 0.8 else d[2]
            d /= np.linalg.norm(d)
            p = np.array(cen) + d * r * np.array(scale) * self.rnd.uniform(0.82, 1.0)
            a = d + self.np.randn(3) * 0.55 + np.array([0, 0, up])
            a /= np.linalg.norm(a)
            w = np.cross(a, self.np.randn(3))
            w /= max(1e-6, np.linalg.norm(w))
            sz = size * self.rnd.uniform(0.7, 1.3)
            k = self.rnd.uniform(1 - jit, 1 + jit) * (0.8 + 0.3 * max(0.0, float(d @ _L)))
            cc = _c(col, k)
            self.add([p, p + a * sz * 0.5 + w * sz * width * 0.5, p + a * sz,
                      p + a * sz * 0.5 - w * sz * width * 0.5], [_c(cc, 0.8), cc, _c(cc, 1.12), cc],
                     [[0, 1, 2, 3]])

    def clumps(self, cen, r, n, size, top, bot, scale=(1, 1, 1), stretch=(1, 1, 0.8)):
        """Small leafy lumps studding a ball: the painterly cauliflower."""
        for _ in range(n):
            d = self.np.randn(3)
            d[2] = abs(d[2]) * 0.9 + 0.1 if self.rnd.random() < 0.85 else d[2]
            d /= np.linalg.norm(d)
            p = np.array(cen) + d * r * np.array(scale) * self.rnd.uniform(0.85, 1.02)
            k = self.rnd.uniform(0.88, 1.1) * (0.82 + 0.25 * max(0.0, float(d @ _L) + 0.3))
            self.blob(p, size * self.rnd.uniform(0.7, 1.25), _c(top, k), _c(bot, k), scale=stretch,
                      sub=1, rough=0.18)

    def tube(self, pts, radii, light, dark, segs=6, cap=True, lump=0.0):
        """A tube along points; colour by which way each side faces."""
        pts = [np.array(p, dtype=float) for p in pts]
        rings, cols = [], []
        prev = None
        for i, p in enumerate(pts):
            t = pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]
            t /= max(1e-9, np.linalg.norm(t))
            ref = np.array([0, 0, 1.0]) if abs(t[2]) < 0.9 else np.array([1.0, 0, 0])
            if prev is not None:
                ref = prev
            n1 = np.cross(t, ref)
            n1 /= max(1e-9, np.linalg.norm(n1))
            n2 = np.cross(t, n1)
            prev = np.cross(n1, t)
            for k in range(segs):
                a = 2 * math.pi * k / segs
                n = math.cos(a) * n1 + math.sin(a) * n2
                rr = radii[i] * (1 + (self.rnd.uniform(-lump, lump) if lump else 0))
                rings.append(p + n * rr)
                tt = min(1.0, max(0.0, 0.5 + 0.6 * float(n @ _L)))
                cols.append(np.array(dark) * (1 - tt) + np.array(light) * tt)
        faces = []
        for i in range(len(pts) - 1):
            for k in range(segs):
                a, b = i * segs + k, i * segs + (k + 1) % segs
                faces.append([a, b, b + segs, a + segs])
        if cap:
            faces.append(list(range((len(pts) - 1) * segs, len(pts) * segs)))
        self.add(rings, cols, faces)

    def blade(self, p, tip, w, cb, ct, side=None):
        """A thin triangle from a base of width w at p up to tip."""
        p, tip = np.array(p, dtype=float), np.array(tip, dtype=float)
        if side is None:
            a = self.rnd.uniform(0, math.pi)
            side = np.array([math.cos(a), math.sin(a), 0.0])
        self.add([p - side * w / 2, p + side * w / 2, tip], [cb, cb, ct], [[0, 1, 2]])

    def finish(self, name, parent=None, smooth=False, rough=0.9, key=None):
        me = _alive(key) if key else None
        if me is None:
            v = np.concatenate(self.v) if self.v else np.zeros((0, 3))
            col = np.concatenate(self.c) if self.c else np.zeros((0, 3))
            me = bpy.data.meshes.new(name)
            me.from_pydata(v.tolist(), [], self.f)
            ca = me.color_attributes.new("Col", "FLOAT_COLOR", "POINT")
            rgba = np.ones((len(v), 4), dtype=_F)
            rgba[:, :3] = _lin(np.clip(col, 0, 1))
            ca.data.foreach_set("color", rgba.ravel())
            if smooth:
                me.polygons.foreach_set("use_smooth", [True] * len(me.polygons))
            me.materials.append(_vmat(rough))
            me.update()
            if key:
                _cache[key] = me
        ob = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(ob)
        if parent is not None:
            ob.parent = parent
        return ob


def _prop(kind, builder, loc, seed, scale, parent, rot=None):
    """Root empty + one mesh object; the mesh is shared per (kind, seed)."""
    r = empty(kind, loc, parent=parent)
    r.scale = (scale, scale, scale)
    if rot is not None:
        r.rotation_euler = (0, 0, rot)
    key = ("mesh", kind, seed)
    me = _alive(key)
    if me is None:
        b = _B(seed * 7919 + hash(kind) % 1000)
        smooth = builder(b)
        b.finish(kind, r, smooth=bool(smooth), key=key)
    else:
        ob = bpy.data.objects.new(kind, me)
        bpy.context.scene.collection.objects.link(ob)
        ob.parent = r
    return r


def _tuft(b, x, y, z, n, h, cb, ct, spread=0.07, w=0.035):
    for _ in range(n):
        a = b.rnd.uniform(0, 2 * math.pi)
        o = b.rnd.uniform(0, spread)
        hh = h * b.rnd.uniform(0.6, 1.15)
        lean = b.rnd.uniform(0.15, 0.6) * hh
        p = (x + math.cos(a) * o * 0.5, y + math.sin(a) * o * 0.5, z - 0.01)
        tip = (x + math.cos(a) * (o + lean), y + math.sin(a) * (o + lean), z + hh)
        k = b.rnd.uniform(0.85, 1.15)
        b.blade(p, tip, w * b.rnd.uniform(0.8, 1.3), _c(cb, k), _c(ct, k))


# --- the props -------------------------------------------------------------

def olive(loc=(0, 0, 0), seed=0, scale=1.0, parent=None):
    """An old olive: twisted grey trunk, a wide silvery-green crown. ~2.5 tall."""
    def make(b):
        r = b.rnd
        bark_l, bark_d = (0.66, 0.56, 0.44), (0.34, 0.26, 0.20)
        lean = r.uniform(0, 6.28)
        fork = np.array([math.cos(lean) * 0.12, math.sin(lean) * 0.12, r.uniform(0.75, 0.95)])
        b.tube([(0, 0, -0.05), (0.03, -0.02, 0.2), fork * (0.6, 0.6, 0.55) + (0.05, 0.03, 0),
                fork], [0.26, 0.19, 0.17, 0.15], bark_l, bark_d, segs=7, lump=0.16)
        for k in range(5):                                   # root flare
            a = k * 1.26 + r.uniform(0, 0.6)
            b.tube([(math.cos(a) * 0.34, math.sin(a) * 0.34, -0.03),
                    (math.cos(a) * 0.17, math.sin(a) * 0.17, 0.10), (0, 0, 0.32)],
                   [0.03, 0.07, 0.05], bark_l, bark_d, segs=5)
        nl = r.choice((3, 4, 4))
        a0 = r.uniform(0, 6.28)
        for k in range(nl):                                  # limbs, each with its clumps
            a = a0 + k * 2 * math.pi / nl + r.uniform(-0.35, 0.35)
            reach = r.uniform(0.55, 0.85)
            end = fork + np.array([math.cos(a) * reach, math.sin(a) * reach, r.uniform(0.55, 0.9)])
            mid = (fork + end) / 2 + np.array([math.cos(a + 1.2) * 0.12, math.sin(a + 1.2) * 0.12, -0.08])
            b.tube([fork, mid, end], [0.12, 0.085, 0.05], bark_l, bark_d, segs=5, lump=0.1)
            for j in range(r.choice((5, 6))):
                c = end + np.array([r.uniform(-0.55, 0.55), r.uniform(-0.55, 0.55), r.uniform(-0.2, 0.45)])
                rad = r.uniform(0.26, 0.42)
                hgt = (c[2] - 1.2) / 1.2
                top = _c((0.50, 0.64, 0.32), 0.90 + 0.16 * hgt + r.uniform(-0.07, 0.07))
                bot = _c((0.13, 0.28, 0.17), 0.9 + 0.2 * hgt)
                sc = (1.0, 1.0, r.uniform(0.62, 0.8))
                b.blob(c, rad, top, bot, scale=sc, rough=0.2)
                b.clumps(c, rad, 9, 0.13, (0.70, 0.80, 0.46), (0.26, 0.42, 0.24), scale=sc)
                b.leaves(c, rad, 8, 0.13, (0.82, 0.88, 0.62), scale=sc, width=0.4)
        c = fork + np.array([0, 0, 1.05])                    # the crown's top
        b.blob(c, 0.5, (0.56, 0.70, 0.36), (0.20, 0.36, 0.22), scale=(1.1, 1.1, 0.7))
        b.clumps(c, 0.5, 14, 0.14, (0.76, 0.84, 0.50), (0.30, 0.46, 0.26), scale=(1.1, 1.1, 0.7))
        b.leaves(c, 0.5, 10, 0.13, (0.84, 0.90, 0.64), scale=(1.1, 1.1, 0.7), width=0.4)
        return True
    return _prop("olive", make, loc, seed, scale, parent)


def cypress(loc=(0, 0, 0), seed=0, scale=1.0, parent=None):
    """A tall dark Mediterranean cypress, a green flame. ~4 tall."""
    def make(b):
        r = b.rnd
        h = r.uniform(3.6, 4.4)
        wmax = r.uniform(0.40, 0.50)
        b.tube([(0, 0, -0.05), (0, 0, 0.5)], [0.09, 0.07], (0.50, 0.36, 0.24), (0.28, 0.19, 0.13),
               segs=6)
        n = 24
        for i in range(n):
            t = i / (n - 1)
            zc = 0.42 + t * (h - 0.75)
            rad = wmax * (math.sin(math.pi * min(1.0, t * 0.9 + 0.16)) ** 0.7) * (1 - 0.55 * t) + 0.07
            a = i * 2.4 + r.uniform(-0.3, 0.3)
            off = rad * 0.45 * (1 - t * 0.7)
            rad *= r.uniform(0.7, 0.95)
            cen = (math.cos(a) * off, math.sin(a) * off, zc)
            k = 0.85 + 0.25 * t + r.uniform(-0.05, 0.05)
            sc = (1, 1, r.uniform(1.5, 1.9))
            b.blob(cen, rad, _c((0.17, 0.42, 0.19), k), _c((0.03, 0.16, 0.11), k), scale=sc, rough=0.16)
            b.clumps(cen, rad, 7, 0.11, _c((0.26, 0.54, 0.22), k), _c((0.05, 0.22, 0.13), k), scale=sc,
                     stretch=(1, 1, 2.0))
            b.leaves(cen, rad, 5, 0.24, (0.24, 0.50, 0.22), scale=sc, up=2.0, width=0.28)
        b.tube([(0, 0, h - 0.5), (0.02, 0, h + 0.05)], [0.07, 0.0], (0.28, 0.56, 0.22),
               (0.08, 0.28, 0.15), segs=5, cap=False)
        return True
    return _prop("cypress", make, loc, seed, scale, parent)


def palm(loc=(0, 0, 0), seed=0, scale=1.0, parent=None):
    """A date palm: ringed leaning trunk, arching fronds, orange dates. ~3.6 tall."""
    def make(b):
        r = b.rnd
        h = r.uniform(2.7, 3.5)
        la = r.uniform(0, 6.28)
        lean = r.uniform(0.15, 0.6)
        pts, rad = [], []
        n = 26
        for i in range(n):
            t = i / (n - 1)
            bend = lean * t * t
            pts.append((math.cos(la) * bend, math.sin(la) * bend, -0.05 + t * h))
            base = 0.19 - 0.08 * min(1.0, t * 3) - 0.02 * t
            rad.append(base * (1.16 if i % 2 == 0 else 0.90))
        b.tube(pts, rad, (0.72, 0.56, 0.36), (0.36, 0.25, 0.16), segs=8)
        top = np.array(pts[-1])
        b.blob(top + (0, 0, -0.02), 0.20, (0.74, 0.54, 0.26), (0.40, 0.26, 0.14), scale=(1, 1, 1.25),
               sub=1, rough=0.1)
        nf = 17
        for k in range(nf):
            a = k * 2.399 + r.uniform(-0.2, 0.2)
            age = k / (nf - 1)                       # 0 young and upright, 1 old and hanging
            e = 1.25 - 1.5 * age + r.uniform(-0.1, 0.1)
            length = r.uniform(1.25, 1.6) * (0.8 + 0.25 * math.sin(math.pi * age))
            bend = r.uniform(1.2, 1.7) * (0.75 + 0.5 * (1 - age))
            d = np.array([math.cos(a), math.sin(a), 0.0])
            sd = np.array([-math.sin(a), math.cos(a), 0.0])
            m = 11
            p = top + d * 0.06
            path = [p]
            for i in range(m):
                t = (i + 0.5) / m
                ee = e - bend * t ** 1.3
                p = p + (d * math.cos(ee) + np.array([0, 0, math.sin(ee)])) * length / m
                path.append(p)
            g = 1.0 - 0.25 * age + r.uniform(-0.06, 0.06)
            cb = _c((0.16, 0.42, 0.14), g)
            ct = _c((0.50, 0.74, 0.22), g) if age < 0.8 else _c((0.66, 0.70, 0.28), g)
            cr = _c((0.62, 0.78, 0.30), g)
            for i in range(m):
                t = (i + 0.5) / m
                w = 0.36 * math.sin(math.pi * min(1.0, t * 0.9 + 0.12)) ** 0.6
                mid = (path[i] + path[i + 1]) / 2 + (path[i + 1] - path[i]) * 0.9
                for sgn in (-1, 1):
                    tip = mid + sd * sgn * w + np.array([0, 0, -w * 0.55])
                    b.add([path[i], path[i + 1], tip], [cr, cr, ct if sgn > 0 else _c(ct, 0.82)],
                          [[0, 1, 2]])
                    b.add([path[i], tip, path[i] * 0.5 + path[i + 1] * 0.5 + sd * sgn * w * 0.5
                           + np.array([0, 0, -w * 0.5])], [cb, cb, cb], [[0, 1, 2]])
        for k in range(5):                           # dry skirt under the crown
            a = k * 1.3 + r.uniform(0, 0.5)
            d = np.array([math.cos(a), math.sin(a), 0])
            b.blade(top + d * 0.1 + (0, 0, -0.05), top + d * 0.42 + (0, 0, -0.75), 0.16,
                    (0.70, 0.56, 0.30), (0.82, 0.70, 0.40))
        for k in range(4):                           # date bunches
            a = k * 1.7 + r.uniform(0, 0.8)
            d = np.array([math.cos(a), math.sin(a), 0])
            c0 = top + d * 0.30 + (0, 0, -0.30)
            b.tube([top + d * 0.1, top + d * 0.26 + (0, 0, -0.05), c0], [0.02, 0.02, 0.02],
                   (0.90, 0.66, 0.24), (0.66, 0.44, 0.16), segs=4)
            b.blob(c0 + (0, 0, -0.08), 0.12, (1.0, 0.62, 0.14), (0.70, 0.30, 0.08), scale=(1, 1, 1.5),
                   sub=1, rough=0.12)
        return False
    return _prop("palm", make, loc, seed, scale, parent)


def bush(loc=(0, 0, 0), seed=0, scale=1.0, parent=None):
    """A low shrub; by seed plain myrtle, pink oleander, or yellow broom. ~0.7 tall."""
    def make(b):
        r = b.rnd
        kind = seed % 4
        top, bot = [((0.50, 0.78, 0.24), (0.16, 0.40, 0.14)), ((0.42, 0.70, 0.30), (0.14, 0.36, 0.18)),
                    ((0.60, 0.78, 0.26), (0.22, 0.44, 0.14)), ((0.38, 0.66, 0.24), (0.12, 0.34, 0.16))][kind]
        bloom = [None, (0.98, 0.50, 0.66), (1.0, 0.86, 0.18), (0.98, 0.97, 0.92)][kind]
        for k in range(r.choice((5, 6, 7))):
            a = r.uniform(0, 6.28)
            o = r.uniform(0.0, 0.36)
            rad = r.uniform(0.22, 0.36)
            cen = (math.cos(a) * o, math.sin(a) * o, rad * 0.75 + r.uniform(0, 0.22))
            kk = r.uniform(0.9, 1.1)
            b.blob(cen, rad, _c(top, kk), _c(bot, kk), scale=(1, 1, 0.85))
            b.leaves(cen, rad, 16, 0.17, _c(top, 1.2), scale=(1, 1, 0.85), width=0.6)
            if bloom:
                for _ in range(7):
                    d = b.np.randn(3)
                    d[2] = abs(d[2]) + 0.3
                    d /= np.linalg.norm(d)
                    p = np.array(cen) + d * rad * (1, 1, 0.85)
                    b.blob(p, 0.05, _c(bloom, 1.0), _c(bloom, 0.8), sub=1, rough=0.1)
        for k in range(8):
            a = r.uniform(0, 6.28)
            _tuft(b, math.cos(a) * 0.45, math.sin(a) * 0.45, 0, 4, 0.22, (0.20, 0.44, 0.14),
                  (0.58, 0.78, 0.26))
        return True
    return _prop("bush", make, loc, seed, scale, parent)


def rock(loc=(0, 0, 0), seed=0, scale=1.0, parent=None):
    """A weathered limestone boulder with a couple of chips and moss. ~0.45 tall."""
    def make(b):
        r = b.rnd
        warm = r.uniform(0, 1)
        top = _c((0.76, 0.73, 0.66), 1.0) * (1 - warm * 0.5) + _c((0.80, 0.68, 0.50)) * warm * 0.5
        bot = (0.34, 0.30, 0.33)
        hgt = r.uniform(0.5, 0.8)
        b.blob((0, 0, 0.14), 0.36, top, bot, scale=(r.uniform(1.0, 1.4), r.uniform(0.8, 1.1), hgt),
               rough=0.24, shade=1.0)
        b.blob((r.uniform(-0.1, 0.1), r.uniform(-0.1, 0.1), 0.30 * hgt + 0.1), 0.2,
               _c((0.50, 0.66, 0.24)), _c((0.30, 0.46, 0.20)), scale=(1.2, 1.0, 0.35), sub=1)  # moss
        for k in range(r.choice((2, 3, 4))):
            a = r.uniform(0, 6.28)
            o = r.uniform(0.38, 0.62)
            rad = r.uniform(0.09, 0.19)
            b.blob((math.cos(a) * o, math.sin(a) * o, rad * 0.4), rad, _c(top, r.uniform(0.9, 1.05)),
                   bot, scale=(1.2, 1, 0.7), rough=0.28, sub=1, shade=1.0)
        for k in range(6):
            a = r.uniform(0, 6.28)
            _tuft(b, math.cos(a) * 0.5, math.sin(a) * 0.45, 0, 4, 0.2, (0.20, 0.44, 0.14),
                  (0.60, 0.78, 0.26))
        return False
    return _prop("rock", make, loc, seed, scale, parent)


def reeds(loc=(0, 0, 0), seed=0, scale=1.0, parent=None):
    """A clump of reeds with cattails and papyrus umbels. ~1.2 tall."""
    def make(b):
        r = b.rnd
        for k in range(34):
            a = r.uniform(0, 6.28)
            o = abs(r.gauss(0, 0.2))
            x, y = math.cos(a) * o, math.sin(a) * o
            hh = r.uniform(0.5, 1.15)
            la = r.uniform(0, 6.28)
            ln = r.uniform(0.05, 0.3) * hh
            kk = r.uniform(0.85, 1.15)
            b.blade((x, y, -0.02), (x + math.cos(la) * ln, y + math.sin(la) * ln, hh), 0.05,
                    _c((0.20, 0.46, 0.16), kk), _c((0.66, 0.80, 0.28), kk))
        for k in range(4):                               # cattails
            a, o = r.uniform(0, 6.28), r.uniform(0, 0.22)
            x, y = math.cos(a) * o, math.sin(a) * o
            hh = r.uniform(0.9, 1.2)
            b.tube([(x, y, 0), (x + 0.02, y, hh)], [0.012, 0.01], (0.56, 0.70, 0.28),
                   (0.30, 0.46, 0.18), segs=4)
            b.tube([(x + 0.02, y, hh - 0.2), (x + 0.02, y, hh - 0.02)], [0.035, 0.035],
                   (0.52, 0.30, 0.16), (0.30, 0.17, 0.10), segs=5)
        for k in range(5):                               # papyrus
            a, o = r.uniform(0, 6.28), r.uniform(0.05, 0.32)
            x, y = math.cos(a) * o, math.sin(a) * o
            hh = r.uniform(0.95, 1.4)
            tx, ty = x + math.cos(a) * 0.14, y + math.sin(a) * 0.14
            b.tube([(x, y, 0), ((x + tx) / 2, (y + ty) / 2, hh * 0.6), (tx, ty, hh)],
                   [0.018, 0.015, 0.012], (0.50, 0.72, 0.26), (0.24, 0.46, 0.18), segs=4)
            for j in range(16):
                d = b.np.randn(3)
                d[2] = abs(d[2]) * 0.6 - 0.1
                d /= np.linalg.norm(d)
                tip = np.array([tx, ty, hh]) + d * r.uniform(0.16, 0.24)
                b.blade((tx, ty, hh), tip, 0.035, (0.44, 0.66, 0.24), (0.82, 0.86, 0.40))
        return False
    return _prop("reeds", make, loc, seed, scale, parent)


_BLOOMS = [((0.94, 0.14, 0.10), (0.12, 0.08, 0.08)),     # poppy
           ((1.0, 0.86, 0.14), (0.96, 0.60, 0.10)),      # crown daisy
           ((0.99, 0.98, 0.95), (1.0, 0.82, 0.16)),      # chamomile
           ((0.62, 0.44, 0.90), (0.98, 0.92, 0.60)),     # mallow / anemone
           ((0.26, 0.48, 0.96), (0.98, 0.96, 0.80))]     # flax, cornflower


def _flower(b, x, y, z, kind, size=1.0):
    r = b.rnd
    petal, eye = _BLOOMS[kind]
    k = r.uniform(0.9, 1.08)
    petal = _c(petal, k)
    h = r.uniform(0.14, 0.26) * size
    ox, oy = r.uniform(-0.03, 0.03), r.uniform(-0.03, 0.03)
    b.blade((x, y, z - 0.01), (x + ox, y + oy, z + h), 0.022, (0.20, 0.44, 0.14), (0.36, 0.60, 0.20))
    rad = r.uniform(0.050, 0.068) * size
    cen = np.array([x + ox, y + oy, z + h])
    a0 = r.uniform(0, 6.28)
    ring = [cen + np.array([math.cos(a0 + i * 1.2566) * rad, math.sin(a0 + i * 1.2566) * rad,
                            rad * 0.35 + (0.012 if i % 2 else -0.012)]) for i in range(5)]
    faces = [[0, i + 1, (i + 1) % 5 + 1] for i in range(5)]
    b.add([cen] + ring, [eye] + [petal] * 5, faces)


def flowers(loc=(0, 0, 0), seed=0, scale=1.0, parent=None):
    """A drift of wildflowers ~1.6 across; the seed picks the mix (poppies,
    yellow daisies, white chamomile, mauve, blue)."""
    def make(b):
        r = b.rnd
        main = seed % 5
        other = (main + 1 + seed // 5 % 3) % 5
        for k in range(60):
            a = r.uniform(0, 6.28)
            o = abs(r.gauss(0, 0.42))
            x, y = math.cos(a) * o * 1.3, math.sin(a) * o * 0.8
            if k % 4 == 0:
                _tuft(b, x, y, 0, 5, 0.20, (0.20, 0.46, 0.14), (0.60, 0.80, 0.26))
            else:
                _flower(b, x, y, 0, main if r.random() < 0.8 else other)
        return False
    return _prop("flowers", make, loc, seed, scale, parent, rot=(seed * 1.7) % 6.28)


def _wheat(b, sx, sy, row=0.2, step=0.04):
    r = b.rnd
    ny = max(1, int(round(sy / row)))
    for j in range(ny):
        y0 = -sy / 2 + (j + 0.5) * sy / ny
        x = -sx / 2 + 0.03
        while x < sx / 2 - 0.02:
            for q in range(3):
                px, py = x + r.uniform(-0.02, 0.02), y0 + r.uniform(-0.06, 0.06)
                h = r.uniform(0.46, 0.62)
                la = r.uniform(0, 6.28)
                ln = r.uniform(0.02, 0.10)
                tip = np.array([px + math.cos(la) * ln, py + math.sin(la) * ln, h])
                k = r.uniform(0.9, 1.1)
                b.blade((px, py, -0.01), tip, 0.045, _c((0.66, 0.56, 0.18), k), _c((0.92, 0.74, 0.26), k))
                d = np.array([math.cos(la), math.sin(la), 0]) * 0.08 + np.array([0, 0, 0.17])
                w = np.array([-math.sin(la), math.cos(la), 0]) * 0.04
                e0 = tip - d * 0.25
                b.add([e0, e0 + d * 0.5 + w, e0 + d, e0 + d * 0.5 - w],
                      [_c((0.90, 0.70, 0.22), k), _c((1.0, 0.84, 0.34), k), _c((1.0, 0.90, 0.46), k),
                       _c((0.84, 0.62, 0.18), k)], [[0, 1, 2, 3]])
                w2 = np.array([0, 0, 0.03]) + w * 0.4
                b.add([e0, e0 + d * 0.5 + w2, e0 + d, e0 + d * 0.5 - w2],
                      [_c((0.86, 0.66, 0.20), k), _c((1.0, 0.86, 0.38), k), _c((1.0, 0.90, 0.46), k),
                       _c((0.80, 0.58, 0.16), k)], [[0, 1, 2, 3]])
            x += step * r.uniform(0.8, 1.2)
    for k in range(int(sx * sy * 5)):                    # poppies and cornflowers in the corn
        px, py = r.uniform(-sx / 2, sx / 2), r.uniform(-sy / 2, sy / 2)
        if r.random() < 0.5 or abs(px) > sx / 2 - 0.2 or abs(py) > sy / 2 - 0.2:
            _flower(b, px, py, 0.34, 0 if r.random() < 0.7 else 4, size=1.2)


def wheat_patch(loc=(0, 0, 0), seed=0, scale=1.0, parent=None, size=(1.0, 1.0)):
    """Ripe wheat in rows along X, `size` = (x, y) tiles, ~0.6 tall. (The
    golden soil under it is painted by build(); alone it stands on the grass.)"""
    def make(b):
        _wheat(b, size[0], size[1])
        return False
    return _prop("wheat%gx%g" % tuple(size), make, loc, seed, scale, parent)


def wattle_fence(pts, parent=None, seed=0, h=0.42):
    """A low woven fence along a polyline of (x, y) points."""
    b = _B(seed + 300)
    r = b.rnd
    posts = []
    for a, c in zip(pts[:-1], pts[1:]):
        a, c = np.array(a, dtype=float), np.array(c, dtype=float)
        n = max(1, int(round(np.linalg.norm(c - a) / 0.3)))
        seg = [a + (c - a) * i / n for i in range(n + 1)]
        nrm = np.array([-(c - a)[1], (c - a)[0]]) / max(1e-9, np.linalg.norm(c - a))
        for i, p in enumerate(seg[:-1] if posts else seg[:-1]):
            posts.append(p)
        posts.append(seg[-1])
        for lv in range(4):
            z = 0.07 + lv * (h - 0.12) / 3
            path, rad = [], []
            for i, p in enumerate(seg):
                sg = 1 if (i + lv) % 2 else -1
                q = p + nrm * sg * 0.028
                path.append((q[0], q[1], z + r.uniform(-0.012, 0.012)))
                rad.append(0.024)
            k = r.uniform(0.85, 1.1)
            b.tube(path, rad, _c((0.78, 0.60, 0.36), k), _c((0.40, 0.27, 0.16), k), segs=4, cap=False)
    for p in posts:
        hh = h + r.uniform(0.0, 0.1)
        b.tube([(p[0], p[1], -0.03), (p[0] + r.uniform(-0.015, 0.015), p[1], hh)], [0.03, 0.024],
               (0.66, 0.48, 0.28), (0.34, 0.22, 0.13), segs=5)
    return b.finish("wattle", parent)


# --- the ground ------------------------------------------------------------

def _ground_mesh(parent):
    n = 177                                   # vertices per side: ~0.18 tile cells
    ax = np.linspace(-HALF, HALF, n)
    x, y = np.meshgrid(ax, ax)
    z = ground_z(x.ravel(), y.ravel())
    v = np.stack([x.ravel(), y.ravel(), z], -1)
    i = np.arange(n - 1)
    a = (i[None, :] + i[:, None] * n).ravel()
    faces = np.stack([a, a + 1, a + n + 1, a + n], -1)
    me = bpy.data.meshes.new("ground")
    me.from_pydata(v.tolist(), [], faces.tolist())
    uv = me.uv_layers.new(name="uv")
    li = np.empty(len(me.loops), dtype=np.int32)
    me.loops.foreach_get("vertex_index", li)
    uvs = (v[li, :2] + HALF) / (2 * HALF)
    uv.data.foreach_set("uv", uvs.astype(_F).ravel())
    me.polygons.foreach_set("use_smooth", [True] * len(me.polygons))
    me.materials.append(_ground_mat())
    ob = bpy.data.objects.new("ground", me)
    bpy.context.scene.collection.objects.link(ob)
    ob.parent = parent
    # a skirt so the slab has a side instead of a paper edge
    sk = bpy.data.meshes.new("skirt")
    h = HALF
    sk.from_pydata([(-h, -h, 0), (h, -h, 0), (h, h, 0), (-h, h, 0), (-h, -h, -1.2), (h, -h, -1.2),
                    (h, h, -1.2), (-h, h, -1.2)], [],
                   [(0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)])
    from lib import mat
    sk.materials.append(mat((0.42, 0.30, 0.20), rough=1.0))
    so = bpy.data.objects.new("skirt", sk)
    bpy.context.scene.collection.objects.link(so)
    so.parent = parent
    so.location = (0, 0, -0.06)
    return ob


def _flat_uv_mesh(name, verts, faces, material, parent):
    me = bpy.data.meshes.new(name)
    me.from_pydata([tuple(p) for p in verts], [], faces)
    uv = me.uv_layers.new(name="uv")
    v = np.array(verts)
    li = np.empty(len(me.loops), dtype=np.int32)
    me.loops.foreach_get("vertex_index", li)
    uv.data.foreach_set("uv", ((v[li, :2] + HALF) / (2 * HALF)).astype(_F).ravel())
    me.materials.append(material)
    ob = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(ob)
    ob.parent = parent
    return ob


def _water(parent):
    h, k = HALF, SHORE - 1.6
    _flat_uv_mesh("sea", [(-h, -h + k, WATER_Z), (h - k, h, WATER_Z), (-h, h, WATER_Z)], [(0, 1, 2)],
                  _ground_mat(True), parent)
    # the channel: a glossy ribbon just above the painted water
    verts, faces = [], []
    pts = CHANNEL
    for i, p in enumerate(pts):
        if _shore(p[0], p[1]) > -0.1:
            continue
        t = pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]
        nrm = np.array([-t[1], t[0]]) / np.linalg.norm(t)
        verts += [(p[0] + nrm[0] * 0.13, p[1] + nrm[1] * 0.13, 0.004),
                  (p[0] - nrm[0] * 0.13, p[1] - nrm[1] * 0.13, 0.004)]
    for i in range(len(verts) // 2 - 1):
        faces.append((2 * i, 2 * i + 1, 2 * i + 3, 2 * i + 2))
    _flat_uv_mesh("channel", verts, faces, _ground_mat(True), parent)


def _pick(n, rng, region, min_d, taken, margin, tall=0.0, tries=4000):
    """Up to n points from region() (-> x, y arrays) that are on open ground,
    apart from each other, and (for tall things) not standing in front of a
    plaza or the battle from the camera's side."""
    out = []
    for _ in range(tries):
        if len(out) >= n:
            break
        x, y = region(rng)
        if is_clear(x, y, margin):
            continue
        if tall:
            bad = abs(x + y - BATTLE[0] - BATTLE[1]) < 4.3 and -14.0 < y - x < -2.0
            for t in (0.8, 1.6, 2.4, 3.2):
                if t <= tall and is_clear(x - 0.707 * t, y + 0.707 * t, -0.4) and _shore(
                        x - 0.707 * t, y + 0.707 * t) < -0.3:
                    bad = True
            if bad:
                continue
        if any((x - p[0]) ** 2 + (y - p[1]) ** 2 < max(min_d, p[2]) ** 2 for p in taken):
            continue
        taken.append((x, y, min_d))
        out.append((x, y))
    return out


def _uf(u, far):
    """Screen coordinates (u = x + y, far = y - x) to x, y."""
    return (u - far) / 2, (u + far) / 2


def tree_spots():
    """Where build() plants: [(kind, x, y, seed, scale)]; deterministic."""
    rng = random.Random(11)
    taken, out = [], []

    def put(kind, pts, variants, smin=0.85, smax=1.15):
        for x, y in pts:
            out.append((kind, x, y, rng.randrange(variants), rng.uniform(smin, smax)))

    def near(cx, cy, rad):
        return lambda r: (cx + r.gauss(0, rad), cy + r.gauss(0, rad))

    def band(u0, u1, f0, f1):
        return lambda r: _uf(r.uniform(u0, u1), r.uniform(f0, f1))

    # the two foreground clumps that frame the bottom of the picture
    put("olive", _pick(2, rng, near(8.3, -6.6, 0.7), 1.6, taken, 0.6, 2.4), 6, 0.85, 0.95)
    put("cypress", _pick(1, rng, near(9.6, -5.6, 0.4), 0.8, taken, 0.6, 3.2), 5, 0.7, 0.8)
    put("olive", _pick(2, rng, near(-1.0, -9.8, 0.8), 1.6, taken, 0.6, 2.4), 6, 0.9, 1.05)
    put("cypress", _pick(2, rng, near(-2.4, -10.6, 0.6), 0.8, taken, 0.6, 3.2), 5, 0.8, 1.0)
    # cypresses: a row behind the Greek plaza, a few on the left
    put("cypress", _pick(5, rng, lambda r: (lambda a: (GREEK[0] + math.cos(a) * 4.4,
                                                         GREEK[1] + math.sin(a) * 4.4))(
        r.uniform(1.75, 3.1)), 0.9, taken, 0.5, 0), 5)
    put("cypress", _pick(3, rng, band(-13.5, -9.0, -9, -3), 0.9, taken, 0.6, 3.2), 5)
    put("cypress", _pick(2, rng, band(-12, -5, -15.5, -12), 0.9, taken, 0.6, 3.2), 5)
    # olives: between the towns, on the left, along the near edge
    put("olive", _pick(1, rng, band(-1.0, 1.5, 5.0, 6.8), 2.0, taken, 0.8, 0), 6, 0.8, 0.9)
    put("olive", _pick(2, rng, band(-13, -6, -9.5, -3.5), 2.0, taken, 0.8, 2.4), 6, 0.8, 1.0)
    put("olive", _pick(3, rng, band(-12.5, -5, -16, -11.5), 2.4, taken, 0.8, 2.4), 6, 0.8, 1.0)
    put("olive", _pick(2, rng, band(5.5, 12.5, -16, -11.5), 2.4, taken, 0.8, 2.4), 6, 0.8, 1.0)
    put("olive", _pick(1, rng, band(5.5, 12, -10, -5), 2.2, taken, 0.8, 2.4), 6, 0.8, 1.0)
    # palms: round the Egyptian plaza (behind it), the beach, the right edge
    put("palm", _pick(6, rng, lambda r: (lambda a, d: (EGYPT[0] + math.cos(a) * d,
                                                         EGYPT[1] + math.sin(a) * d))(
        r.uniform(0.9, 3.4), r.uniform(4.1, 4.9)), 1.1, taken, 0.4, 0), 6)
    put("palm", _pick(9, rng, band(-12.5, 13, 6.6, 8.7), 1.6, taken, 0.5, 3.2), 6)
    put("palm", _pick(3, rng, band(10, 13.5, -9, -3), 1.4, taken, 0.6, 3.2), 6)
    put("palm", _pick(2, rng, band(11.5, 13.5, 4.5, 6.5), 1.2, taken, 0.6, 0), 6)
    return out, taken


def _scatter(root):
    rng = random.Random(23)
    nr = np.random.RandomState(23)
    spots, taken = tree_spots()
    fn = {"olive": olive, "cypress": cypress, "palm": palm}
    for kind, x, y, seed, sc in spots:
        r = fn[kind]((x, y, ground_z(x, y) - 0.02), seed, sc, root)
        r.rotation_euler = (0, 0, rng.uniform(0, 6.28))

    def window(n):
        u = nr.uniform(-15.0, 15.0, n)
        far = nr.uniform(-17.0, 9.5, n)
        return _uf(u, far)

    # bushes, rocks, boulders
    def region(r):
        return _uf(r.uniform(-14, 14), r.uniform(-16.5, 8.5))
    for x, y in _pick(46, rng, region, 0.9, taken, 0.35):
        r = bush((x, y, ground_z(x, y) - 0.02), rng.randrange(8), rng.uniform(0.7, 1.25), root)
        r.rotation_euler = (0, 0, rng.uniform(0, 6.28))
    for x, y in _pick(46, rng, region, 0.7, taken, 0.3):
        r = rock((x, y, ground_z(x, y) - 0.02), rng.randrange(8), rng.uniform(0.45, 1.0), root)
        r.rotation_euler = (0, 0, rng.uniform(0, 6.28))
    for x, y in _pick(6, rng, region, 2.0, taken, 0.9):
        r = rock((x, y, ground_z(x, y) - 0.05), rng.randrange(8), rng.uniform(1.5, 2.4), root)
        r.rotation_euler = (0, 0, rng.uniform(0, 6.28))
    for x, y in _pick(14, rng, lambda r: _uf(r.uniform(-13, 13), SHORE + r.uniform(-1.4, 0.5)),
                      0.8, [], -9):                       # rocks on the beach and in the surf
        if abs(_shore(x, y)) < 1.5 and _poly_dist(x, y, CHANNEL) > 0.6:
            r = rock((x, y, ground_z(x, y) - 0.03), rng.randrange(8), rng.uniform(0.5, 1.3), root)
            r.rotation_euler = (0, 0, rng.uniform(0, 6.28))

    # reeds: along the channel, round the plot, and in stretches of the shore
    for i in range(3, len(CHANNEL) - 14, 2):
        p = CHANNEL[i]
        if _shore(p[0], p[1]) > -0.25 or rng.random() < 0.3:
            continue
        t = CHANNEL[i + 1] - CHANNEL[i - 1]
        nrm = np.array([-t[1], t[0]]) / np.linalg.norm(t) * rng.choice((-1, 1))
        q = p + nrm * rng.uniform(0.3, 0.42)
        if np.hypot(q[0] - EGYPT[0], q[1] - EGYPT[1]) < EGYPT[2] + 0.2:
            continue
        reeds((q[0], q[1], 0), rng.randrange(4), rng.uniform(0.6, 0.95), root)
    for k in range(60):
        u = rng.uniform(-13, 13)
        if math.sin(u * 0.9 + 1.0) < 0.1:
            continue
        x, y = _uf(u, SHORE)
        far = SHORE - float(_shore(x, y)) + rng.uniform(-0.28, 0.12)
        x, y = _uf(u, far)
        if _poly_dist(x, y, CHANNEL) < 0.35:
            continue
        reeds((x, y, ground_z(x, y) - 0.02), rng.randrange(4), rng.uniform(0.6, 1.0), root)

    # wildflower drifts (geometry), densest near the camera
    for x, y in _pick(90, rng, lambda r: _uf(r.uniform(-14, 14), -17 + 25 * r.random() ** 1.5),
                      1.0, [], 0.3):
        if any((x - p[0]) ** 2 + (y - p[1]) ** 2 < 0.25 for p in taken):
            continue
        r = flowers((x, y, ground_z(x, y)), rng.randrange(15), rng.uniform(0.8, 1.5), root)

    # grass tufts everywhere on the turf, as one mesh
    b = _B(77)
    x, y = window(30000)
    zz = _zones(x, y)
    ok = ~is_clear(x, y, -0.25) & (zz["s"] < -1.0 - 0.8 * nr.rand(len(x)))
    ok &= (zz["dg"] > -0.25) & (zz["de"] > -0.25) & (zz["db"] > -0.6)
    dens = 0.25 + 0.75 * _fbm(x, y, 1.4, 61, 3)
    ok &= nr.rand(len(x)) < dens
    x, y = x[ok], y[ok]
    z = ground_z(x, y)
    tone = _fbm(x, y, 5.0, 1, 4)
    dryn = _fbm(x, y, 6.5, 9, 3)
    for i in range(len(x)):
        if dryn[i] > 0.66 and b.rnd.random() < 0.7:
            cb, ct = (0.56, 0.56, 0.22), (0.92, 0.84, 0.44)
        elif tone[i] > 0.55:
            cb, ct = (0.30, 0.56, 0.16), (0.74, 0.88, 0.32)
        else:
            cb, ct = (0.14, 0.40, 0.14), (0.46, 0.72, 0.24)
        big = b.rnd.random() < 0.12
        _tuft(b, x[i], y[i], z[i], 7 if big else 5, 0.34 if big else 0.2, cb, ct,
              spread=0.1 if big else 0.06, w=0.045 if big else 0.035)
        if b.rnd.random() < 0.06:
            _flower(b, x[i] + 0.05, y[i], z[i], b.rnd.randrange(5))
    # dune grass on the back of the beach, dry tufts on the battle ground and plaza rims
    x, y = window(9000)
    zz = _zones(x, y)
    dune = (zz["s"] > -2.2) & (zz["s"] < -0.9) & (nr.rand(len(x)) < 0.5) & (zz["chan"] > 0.4)
    rim = ((np.abs(zz["dg"] + 0.15) < 0.3) | (np.abs(zz["de"] + 0.15) < 0.3)) & (nr.rand(len(x)) < 0.4)
    bat = (zz["db"] < 0.2) & (nr.rand(len(x)) < 0.35)
    for m, cb, ct, h in ((dune, (0.52, 0.60, 0.26), (0.86, 0.86, 0.50), 0.3),
                         (rim, (0.36, 0.52, 0.18), (0.74, 0.80, 0.34), 0.14),
                         (bat, (0.48, 0.46, 0.20), (0.84, 0.76, 0.40), 0.12)):
        for px, py, pz in zip(x[m], y[m], ground_z(x[m], y[m])):
            _tuft(b, px, py, pz, 5, h, cb, ct)
    b.finish("tufts", root)


def _fields(root):
    wx, wy, sx, sy = WHEAT
    wheat_patch((wx, wy, 0), 3, 1.0, root, size=(sx - 0.3, sy - 0.25))
    hx, hy = sx / 2 + 0.05, sy / 2 + 0.05
    gate = 0.45
    wattle_fence([(wx + hx, wy - hy + gate + 0.5), (wx + hx, wy + hy), (wx - hx, wy + hy),
                  (wx - hx, wy - hy), (wx + hx, wy - hy), (wx + hx, wy - hy + gate - 0.2)], root)
    # the irrigated plot: beds of greens, a mud bank round it
    px, py, sx, sy = PLOT
    b = _B(55)
    r = b.rnd
    bed = 0
    x = px - sx / 2 + 0.21
    while x < px + sx / 2 - 0.1:
        kind = bed % 3
        for dx in (-0.09, 0.09):
            y = py - sy / 2 + 0.12
            while y < py + sy / 2 - 0.08:
                if abs(y - py) > 0.24:
                    cx, cy = x + dx + r.uniform(-0.015, 0.015), y
                    if kind == 0:                       # lettuce: pale rosettes
                        b.blob((cx, cy, 0.05), 0.075, (0.66, 0.90, 0.30), (0.26, 0.56, 0.18),
                               scale=(1, 1, 0.8), sub=1, rough=0.2)
                    elif kind == 1:                     # onions / leeks: blue-green spikes
                        _tuft(b, cx, cy, 0, 5, 0.26, (0.16, 0.46, 0.30), (0.50, 0.80, 0.56),
                              spread=0.03, w=0.03)
                    else:                               # flax in flower
                        _tuft(b, cx, cy, 0, 4, 0.24, (0.22, 0.50, 0.18), (0.56, 0.80, 0.30),
                              spread=0.03, w=0.03)
                        if r.random() < 0.7:
                            _flower(b, cx, cy, 0.06, 4, size=0.9)
                y += 0.14
        x += 0.42
        bed += 1
    hx, hy = sx / 2 + 0.06, sy / 2 + 0.06
    ring = [(px - hx, py - hy), (px + hx, py - hy), (px + hx, py + hy), (px - hx, py + hy)]
    for a, c in zip(ring, ring[1:] + ring[:1]):
        n = 12
        pts = [(a[0] + (c[0] - a[0]) * i / n + r.uniform(-0.02, 0.02),
                a[1] + (c[1] - a[1]) * i / n + r.uniform(-0.02, 0.02), 0.01) for i in range(n + 1)]
        pts = [p for p in pts if _poly_dist(p[0], p[1], CHANNEL) > 0.22]
        if len(pts) > 1:
            b.tube(pts, [0.08 + r.uniform(-0.015, 0.015) for _ in pts], (0.66, 0.50, 0.32),
                   (0.34, 0.24, 0.16), segs=5)
    b.finish("plot", root)
    for q in ((px - sx / 2 - 0.3, py + sy / 2 + 0.25), (px + sx / 2 + 0.3, py + 0.3),
              (px + sx / 2 + 0.25, py - 0.35), (px - 0.4, py + sy / 2 + 0.35)):
        reeds((q[0], q[1], 0), r.randrange(4), r.uniform(0.7, 0.9), root)


def build(parent=None):
    """The whole landscape. Returns its root empty (at the origin)."""
    root = empty("terrain", parent=parent)
    _ground_mesh(root)
    _water(root)
    _fields(root)
    _scatter(root)
    return root


# --- previews --------------------------------------------------------------

def pv_props():
    from lib import box, mat
    r = empty("props")
    box((40, 40, 0.2), (0, 0, -0.1), mat("grass", rough=1.0), parent=r)
    olive((-2.6, 2.2, 0), 0, parent=r)
    olive((-0.2, 3.2, 0), 1, parent=r)
    cypress((-2.6, -0.2, 0), 0, parent=r)
    cypress((-1.9, -1.2, 0), 1, parent=r)
    palm((1.6, 2.4, 0), 0, parent=r)
    palm((3.2, 3.2, 0), 1, parent=r)
    for i in range(4):
        bush((-1.2 + i * 1.0, -1.2 + i * 1.0, 0), i, parent=r)
    rock((-1.2, -3.0, 0), 0, parent=r)
    rock((0.0, -2.2, 0), 1, 1.8, parent=r)
    reeds((1.4, -1.6, 0), 0, parent=r)
    flowers((2.6, -0.2, 0), 0, parent=r)
    flowers((3.8, 1.0, 0), 1, parent=r)
    flowers((1.2, -3.4, 0), 2, parent=r)
    wheat_patch((-0.6, 0.6, 0), 0, parent=r)
    return r


def _pv(cx, cy):
    r = build()
    r.location = (-cx, -cy, 0)
    return r


def pv_greek():
    return _pv(-6.0, -3.5)


def pv_egypt():
    return _pv(3.5, 6.5)


def pv_shore():
    return _pv(-3.0, 6.0)


def pv_near():
    return _pv(4.0, -7.0)

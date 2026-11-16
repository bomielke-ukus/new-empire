"""Mesopotamian buildings for the new look: sun-dried mudbrick in warm tan
and ochre, buttressed and niched walls under stepped crenellations, glazed
lapis-blue brick bands with white-and-gold rosettes, bitumen-dark dados,
palm-log beams and reed roofs, stepped ziggurats, winged lamassu.

Every builder is centred on the origin on its footprint (in tiles), takes
`loc=(x, y, 0), rot_z=0.0, player=BLUE, parent=None` and returns its root
empty. The camera sees the -Y (sunlit) and +X faces, so the doors and the
best detail are there. Units are tiles; a person is 1.0 tall.

    town_center()        3 x 3   a palace compound with a glazed gate
    house(variant)       2 x 2   flat-roofed mudbrick house, roof shade
    barracks()           2 x 2   crenellated drill yard, gate tower
    temple()             2 x 2   a small ziggurat with a glazed shrine
    watch_tower()        1 x 1
    academy()            2 x 2   tablet house: a court of tablets and a stele
    archery_range()      2 x 2
    dock()               3 x 3   palm-log quays round a reed boathouse
    farm(ripe)           2 x 2   barley by a canal, a shaduf, date palms
    government_centre()  2 x 2   audience hall on a podium, lamassu
    market()             2 x 2
    siege_workshop()     2 x 2   a battering ram half built
    stable()             2 x 2
    storehouse()         2 x 2
    wonder()             5 x 5   the great ziggurat
    stone_wall(arm, joins, post)  palisade_wall(...)  gate(line, open)
    render_showcase_all(path)

SETS at the bottom declares the colour sets (docs/look/buildings.md).
"""
import math
import random
from math import atan2, cos, hypot, pi, sin, sqrt

import bpy
import bmesh  # noqa: F401  (after bpy)
from mathutils import Vector

import lib
from lib import BLUE, PAL, RED, blob, box, cyl, empty, mat, mesh, sphere
import greek
from greek import (arc_rod, banner, basket, bow, cloth, courses, crate, face, fence, flag,
                   gold, horse_head, lathe, noisy, quiver, rod, sack, sack_pile, shield, target,
                   tint)

# ---------------------------------------------------------------- colours

MUD = (0.82, 0.60, 0.37)          # sun-dried brick, warm ochre-tan
MUD_DK = (0.66, 0.44, 0.26)
MUD_LT = (0.88, 0.70, 0.47)       # mud plaster on roofs and terraces
BITUMEN = (0.11, 0.09, 0.08)
LAPIS = (0.07, 0.24, 0.66)        # glazed brick
TURQ = (0.10, 0.52, 0.62)
GLAZE_Y = (0.97, 0.76, 0.22)
GLAZE_W = (0.97, 0.94, 0.84)
ALABASTER = (0.91, 0.86, 0.74)
REED = (0.82, 0.70, 0.40)
REED_DK = (0.66, 0.54, 0.28)
PALMWOOD = (0.46, 0.34, 0.22)
PALMWOOD_DK = (0.33, 0.24, 0.16)
DIORITE = (0.16, 0.16, 0.18)
CLAY = (0.74, 0.60, 0.44)         # tablets
DATE = (0.66, 0.26, 0.10)
FROND = [(0.30, 0.56, 0.20), (0.22, 0.46, 0.16), (0.38, 0.60, 0.24)]
RUG = [(0.72, 0.16, 0.12), (0.12, 0.26, 0.60), (0.90, 0.66, 0.20), (0.50, 0.14, 0.30)]


def brick(c=MUD, scale=11.0):
    return courses(c, scale, 0.84, 0.07)


def glaze(c=LAPIS):
    return mat(c, rough=0.28)


def plaster(c=MUD_LT):
    return noisy(c, 0.08, 3.0, 0.95)


# ---------------------------------------------------- one mesh, many parts


class Geo:
    """Accumulates many small solids into one mesh (one object, one material):
    ribs, merlons, rosettes. Normals are fixed per island on emit."""

    def __init__(self):
        self.v, self.f = [], []

    def prism(self, prof, origin, u, n, b0, b1):
        """Extrude the (a, z) outline `prof` from b0 to b1 along n; a runs along u."""
        k = len(prof)
        base = len(self.v)
        for b in (b0, b1):
            for a, z in prof:
                self.v.append((origin[0] + u[0] * a + n[0] * b, origin[1] + u[1] * a + n[1] * b, origin[2] + z))
        self.f.append(tuple(base + i for i in range(k)))
        self.f.append(tuple(base + k + i for i in range(k)))
        for i in range(k):
            j = (i + 1) % k
            self.f.append((base + i, base + j, base + k + j, base + k + i))

    def box(self, size, c, rz=0.0):
        hx, hy, hz = size[0] / 2, size[1] / 2, size[2] / 2
        u = (cos(rz), sin(rz))
        n = (-sin(rz), cos(rz))
        self.prism([(-hx, -hz), (hx, -hz), (hx, hz), (-hx, hz)], c, u, n, -hy, hy)

    def merlon(self, p, u, z0, w=0.13, h=0.15, t=0.08):
        """A stepped (Assyrian) merlon standing at p on z0, its face across u."""
        h1, h2 = h * 0.4, h * 0.72
        prof = [(-w / 2, 0), (w / 2, 0), (w / 2, h1), (w / 3, h1), (w / 3, h2), (w / 6, h2), (w / 6, h),
                (-w / 6, h), (-w / 6, h2), (-w / 3, h2), (-w / 3, h1), (-w / 2, h1)]
        n = (-u[1], u[0])
        self.prism(prof, (p[0], p[1], z0), u, n, -t / 2, t / 2)

    def emit(self, m, parent, name="geo"):
        if self.f:
            return mesh(self.v, self.f, m, parent=parent, name=name)


def _edges(x0, x1, y0, y1):
    """The rectangle's four sides as (start, end, outward normal)."""
    return (((x0, y0), (x1, y0), (0, -1)), ((x1, y0), (x1, y1), (1, 0)),
            ((x1, y1), (x0, y1), (0, 1)), ((x0, y1), (x0, y0), (-1, 0)))


def crenels(parent, x0, x1, y0, y1, z, m=None, w=0.11, h=0.13, t=0.075, pitch=0.21, sides="nesw"):
    """Stepped merlons right round a flat roof's edge at height z."""
    g = Geo()
    for idx, ((ax, ay), (bx, by), (nx, ny)) in enumerate(_edges(x0, x1, y0, y1)):
        if "senw"[idx] not in sides:
            continue
        L = hypot(bx - ax, by - ay)
        ux, uy = (bx - ax) / L, (by - ay) / L
        trim = t if idx % 2 else 0.0          # the y sides stop short of the corners
        nm = max(1, round((L - 2 * trim) / pitch))
        for i in range(nm):
            s = trim + (L - 2 * trim) * (i + 0.5) / nm
            p = (ax + ux * s - nx * (t / 2 + 0.004), ay + uy * s - ny * (t / 2 + 0.004))
            g.merlon(p, (ux, uy), z, w, h, t)
    return g.emit(m or brick(MUD), parent, "merlons")


def mud_block(parent, x0, x1, y0, y1, h, z=0.0, c=MUD, ribs=True, pitch=0.3, rib_w=0.075, rib_d=0.035,
              dado=0.12, band=0.82, band_h=0.085, rosettes=True, merlons=True, mh=0.13, roof=MUD_LT,
              beams=False, band_c=LAPIS, sides="nesw", wall_m=None, merlon_m=None):
    """A mudbrick block from (x0, y0) to (x1, y1), h tall from z: buttress
    ribs and corner buttresses, a bitumen dado, a glazed band at `band` of
    the height with rosettes, a plastered flat roof, stepped merlons.
    Returns the roof's height."""
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    sx, sy = x1 - x0, y1 - y0
    wm = wall_m or brick(c)
    box((sx, sy, h), (cx, cy, z + h / 2), wm, parent=parent, name="wall")
    co = rib_d + 0.03 if ribs else 0.0
    if ribs:
        g = Geo()
        for (ax, ay), (bx, by), (nx, ny) in _edges(x0, x1, y0, y1):
            L = hypot(bx - ax, by - ay)
            ux, uy = (bx - ax) / L, (by - ay) / L
            n = max(1, round((L - 0.2) / pitch))
            for i in range(1, n):
                s = 0.1 + (L - 0.2) * i / n
                px, py = ax + ux * s + nx * rib_d / 2, ay + uy * s + ny * rib_d / 2
                g.box((rib_w, rib_d, h - 0.03), (px, py, z + (h - 0.03) / 2), atan2(uy, ux))
            cw = co * 2
            g.box((cw, cw, h - 0.02), (ax, ay, z + (h - 0.02) / 2))
        g.emit(wall_m or brick(tint(c, 1.04)), parent, "ribs")
    if dado:
        box((sx + 2 * co + 0.025, sy + 2 * co + 0.025, dado), (cx, cy, z + dado / 2),
            noisy(BITUMEN, 0.25, 6.0, 0.6), parent=parent, name="dado")
    if band:
        zb = z + h * band
        box((sx + 2 * co + 0.03, sy + 2 * co + 0.03, band_h), (cx, cy, zb), glaze(band_c), parent=parent,
            name="band")
        if rosettes:
            g, gc = Geo(), Geo()
            r_ = band_h * 0.3
            for (ax, ay), (bx, by), (nx, ny) in _edges(x0, x1, y0, y1)[:2]:
                L = hypot(bx - ax, by - ay)
                ux, uy = (bx - ax) / L, (by - ay) / L
                n = max(1, round(L / 0.16))
                for i in range(n):
                    s = L * (i + 0.5) / n
                    p = (ax + ux * s + nx * (co + 0.015), ay + uy * s + ny * (co + 0.015), zb)
                    # an eight-petalled rosette: a square and a diamond, a gold boss
                    g.prism([(-r_, -r_), (r_, -r_), (r_, r_), (-r_, r_)], p, (ux, uy), (nx, ny), 0.0, 0.008)
                    q = r_ * 1.3
                    g.prism([(0, -q), (q, 0), (0, q), (-q, 0)], p, (ux, uy), (nx, ny), 0.0, 0.007)
                    k = r_ * 0.45
                    gc.prism([(-k, -k), (k, -k), (k, k), (-k, k)], p, (ux, uy), (nx, ny), 0.0, 0.014)
            g.emit(mat(GLAZE_W, rough=0.35), parent, "rosettes")
            gc.emit(gold(), parent, "rosette_boss")
    zt = z + h
    if roof:
        box((sx - 0.1, sy - 0.1, 0.02), (cx, cy, zt + 0.01), plaster(roof), parent=parent, name="roof")
    if beams:
        g = Geo()
        for (ax, ay), (bx, by), (nx, ny) in _edges(x0, x1, y0, y1)[:2]:
            L = hypot(bx - ax, by - ay)
            ux, uy = (bx - ax) / L, (by - ay) / L
            n = max(1, round(L / 0.2))
            for i in range(n):
                s = L * (i + 0.5) / n
                g.box((0.05, 0.11, 0.05), (ax + ux * s + nx * 0.05, ay + uy * s + ny * 0.05, zt - 0.13),
                      atan2(uy, ux))
        g.emit(mat(PALMWOOD_DK), parent, "beam_ends")
    if merlons:
        crenels(parent, x0, x1, y0, y1, zt, merlon_m or wall_m or brick(tint(c, 1.04)), h=mh, sides=sides)
    return zt


def extrude_x(parent, prof, x0, x1, m, name="ext"):
    """A solid from the (y, z) outline `prof`, between x0 and x1."""
    g = Geo()
    k = len(prof)
    for x in (x0, x1):
        for y, z in prof:
            g.v.append((x, y, z))
    g.f.append(tuple(range(k)))
    g.f.append(tuple(range(k, 2 * k)))
    for i in range(k):
        j = (i + 1) % k
        g.f.append((i, j, k + j, k + i))
    return g.emit(m, parent, name)


def stair(parent, x, y0, y1, z0, z1, w, n=None, m=None, rail=True, rot_z=0.0, pivot=None):
    """A solid stair climbing along +Y from (x, y0, z0) to (x, y1, z1), w wide,
    with mudbrick side walls. rot_z turns it about `pivot` (default (x, y0))."""
    px, py = pivot if pivot else (x, y0)
    e = empty("stair", (px, py, 0), rot_z, parent)
    lx, ly0, ly1 = x - px, y0 - py, y1 - py
    n = n or max(2, round((z1 - z0) / 0.1))
    dy, dz = (ly1 - ly0) / n, (z1 - z0) / n
    prof = [(ly0, z0)]
    for i in range(n):
        prof += [(ly0 + i * dy, z0 + (i + 1) * dz), (ly0 + (i + 1) * dy, z0 + (i + 1) * dz)]
    prof += [(ly1, z0)]
    extrude_x(e, prof, lx - w / 2, lx + w / 2, m or plaster(MUD_LT), "steps")
    if rail:
        for sx in (-1, 1):
            a, b = (lx + sx * (w / 2 - 0.004), lx + sx * (w / 2 + 0.07))
            extrude_x(e, [(ly0, z0), (ly0, z0 + dz + 0.09), (ly1, z1 + 0.09), (ly1, z0)], min(a, b), max(a, b),
                      brick(MUD_DK), "stair_rail")
    return e


def tomb(w, h, n=8):
    """A round-topped (arched) outline in (u, z): w wide, h tall."""
    r = w / 2
    pts = [(-r, 0), (r, 0)]
    for i in range(n + 1):
        a = pi * i / n
        pts.append((r * cos(a), h - r + r * sin(a)))
    return pts


def slab(parent, prof, b0, b1, m, name="slab"):
    """A flat shape in a face frame (local -Y outward): outline in (x, z),
    from y = -b0 out to y = -b1."""
    g = Geo()
    g.prism(prof, (0, 0, 0), (1, 0), (0, -1), b0, b1)
    return g.emit(m, parent, name)


def arch_door(loc, rz=0.0, parent=None, w=0.4, h=0.85, frame=LAPIS, inner=GLAZE_Y, door=(0.56, 0.36, 0.2),
              studs=True, open_=False):
    """An arched doorway on a wall plane: a glazed frame, a yellow ring,
    cedar doors with bronze studs (or a dark opening). loc: its bottom centre."""
    e = face(loc, rz, parent)
    slab(e, tomb(w + 0.17, h + 0.09), -0.002, 0.03, glaze(frame), "door_frame")
    slab(e, tomb(w + 0.07, h + 0.035), 0.0, 0.038, mat(inner, rough=0.35), "door_ring")
    if open_ or door is None:
        slab(e, tomb(w, h), 0.0, 0.044, mat((0.08, 0.06, 0.05)), "door_dark")
        return e
    slab(e, tomb(w, h), 0.0, 0.044, mat(door, rough=0.7), "door")
    box((0.012, 0.012, h - 0.05), (0, -0.048, (h - 0.05) / 2), mat(tint(door, 0.55)), parent=e)
    if studs:
        g = Geo()
        for zf in (0.2, 0.45, 0.7):
            for k in (-2, -1, 1, 2):
                g.box((0.02, 0.012, 0.02), (k * w * 0.19, -0.05, h * zf))
        g.emit(greek.bronze(), e, "studs")
    box((w + 0.22, 0.14, 0.04), (0, -0.07, 0.02), plaster(MUD_LT), parent=e)
    return e


def slit(loc, rz=0.0, parent=None, w=0.07, h=0.22):
    """A narrow dark window with a glazed sill."""
    e = face(loc, rz, parent)
    slab(e, [(-w / 2, 0), (w / 2, 0), (w / 2, h), (-w / 2, h)], -0.002, 0.012, mat((0.1, 0.07, 0.05)))
    box((w + 0.06, 0.05, 0.025), (0, -0.02, -0.01), glaze(TURQ), parent=e)
    return e


# ------------------------------------------------------------ props, plants


def palm(parent, x, y, z=0.0, h=1.6, seed=0, lean=(0.08, 0.04), fronds=10, dates=True, s=1.0):
    """A date palm: a ringed, slightly leaning trunk, a crown of arching
    fronds, clusters of orange dates."""
    rnd = random.Random(seed)
    h *= s
    top = Vector((x + lean[0] * h, y + lean[1] * h, z + h))
    n = 7
    pts = [Vector((x + lean[0] * h * (i / n) ** 1.6, y + lean[1] * h * (i / n) ** 1.6, z + h * i / n))
           for i in range(n + 1)]
    for i, (a, b) in enumerate(zip(pts, pts[1:])):
        r0 = 0.065 * s * (1.0 - 0.25 * i / n)
        rod(a, b, r0, (PALMWOOD, PALMWOOD_DK)[i % 2], parent, 7, r0 * 0.9)
        cyl(r0 * 1.12, 0.03 * s, tuple(b), PALMWOOD_DK, parent=parent, segs=7)
    sphere(0.1 * s, tuple(top), (0.36, 0.42, 0.18), parent=parent, segs=8)
    fm = [mat(c, rough=0.8) for c in FROND]
    for k in range(fronds):
        a = 2 * pi * k / fronds + rnd.uniform(-0.2, 0.2)
        L = rnd.uniform(0.75, 0.95) * s
        up = rnd.uniform(0.15, 0.45)
        d = Vector((cos(a), sin(a), 0))
        side = Vector((-sin(a), cos(a), 0))
        vs, fs = [], []
        m_ = 8
        for i in range(m_ + 1):
            t = i / m_
            c = top + d * (L * t) + Vector((0, 0, L * (up * sin(pi * t * 0.9) - 0.55 * t * t)))
            wdt = 0.13 * s * sin(pi * min(1.0, t * 1.15)) ** 0.6 + 0.005
            vs += [tuple(c - side * wdt + Vector((0, 0, -0.03 * s))), tuple(c + Vector((0, 0, 0.012 * s))),
                   tuple(c + side * wdt + Vector((0, 0, -0.03 * s)))]
        for i in range(m_):
            b = i * 3
            fs += [(b, b + 3, b + 4, b + 1), (b + 1, b + 4, b + 5, b + 2)]
        mesh(vs, fs, fm[k % 3], parent=parent, smooth=True, name="frond")
    if dates:
        for k in range(3):
            a = k * 2.1 + 0.4
            p = top + Vector((0.09 * cos(a) * s, 0.09 * sin(a) * s, -0.13 * s))
            blob(0.06 * s, tuple(p), mat(DATE, rough=0.5), (1, 1, 1.4), parent, seed + k, 0.25, 1)
    return top


def jar(parent, x, y, z=0.0, s=0.4, c=(0.80, 0.58, 0.38), band=LAPIS):
    """A big storage jar with a glazed band."""
    prof = [(0.1, 0), (0.24, 0.12), (0.3, 0.42), (0.26, 0.72), (0.14, 0.88), (0.15, 0.95), (0.11, 0.96)]
    lathe(prof, mat(c, rough=0.7), (x, y, z), parent=parent, s=s, segs=12)
    if band:
        lathe([(0.296, 0.47), (0.305, 0.52), (0.29, 0.57)], glaze(band), (x, y, z), parent=parent, s=s, segs=12)


def tablet_stack(parent, x, y, z=0.0, n=5, rz=0.0, seed=0):
    rnd = random.Random(seed)
    e = empty("tablets", (x, y, z), rz, parent)
    for i in range(n):
        box((0.1, 0.07, 0.025), (rnd.uniform(-0.01, 0.01), rnd.uniform(-0.01, 0.01), 0.0125 + i * 0.026),
            mat(tint(CLAY, rnd.uniform(0.9, 1.08)), rough=0.9), rot=(0, 0, rnd.uniform(-0.2, 0.2)), parent=e,
            bevel=0.006)
    return e


def tablet_shelf(loc, rz=0.0, parent=None, w=0.6, h=0.7, seed=0):
    """Shelves of clay tablets standing on edge, against a wall (front -Y)."""
    e = face(loc, rz, parent)
    rnd = random.Random(seed)
    box((w, 0.15, h), (0, 0.0, h / 2), mat(PALMWOOD), parent=e)
    nz = max(2, round(h / 0.18))
    g = Geo()
    for k in range(nz):
        zz = (k + 0.5) * h / nz
        box((w - 0.04, 0.02, h / nz - 0.03), (0, -0.07, zz), mat((0.2, 0.14, 0.09)), parent=e)
        nx = int((w - 0.06) / 0.045)
        for i in range(nx):
            if rnd.random() < 0.15:
                continue
            g.box((0.03, 0.06, 0.09 + rnd.uniform(-0.02, 0.01)), (-w / 2 + 0.05 + i * 0.045, -0.085,
                                                                  zz - h / nz / 2 + 0.06), rnd.uniform(-0.15, 0.15))
    g.emit(mat(CLAY, rough=0.9), e, "tablets")
    return e


def stele(loc, rz=0.0, parent=None, s=1.0):
    """A round-topped black diorite law stele: a carved scene at the top,
    columns of signs below (Hammurabi's)."""
    e = face(loc, rz, parent)
    e.scale = (s, s, s)
    box((0.36, 0.26, 0.08), (0, 0, 0.04), plaster(ALABASTER), parent=e, bevel=0.01)
    slab(e, tomb(0.26, 0.86, 10), -0.08, 0.08, mat(DIORITE, rough=0.35), "stele")
    g = Geo()
    for row in range(9):
        for k in range(5):
            g.box((0.03, 0.008, 0.012), (-0.08 + k * 0.04, -0.082, 0.12 + row * 0.04))
    g.emit(mat((0.36, 0.36, 0.4), rough=0.4), e, "signs")
    # the king before the seated god, in gold
    for x, hh in ((-0.05, 0.14), (0.05, 0.11)):
        box((0.04, 0.01, hh), (x, -0.084, 0.6 + hh / 2), gold(), parent=e)
        sphere(0.018, (x, -0.086, 0.62 + hh), gold(), parent=e, segs=6)
    sphere(0.025, (0.0, -0.085, 0.82), gold(), parent=e, segs=8, scale=(1, 0.3, 1))
    return e


def standard(loc, h=1.6, player=BLUE, parent=None, w=0.3, bh=0.5, rot_z=0.0, seed=0):
    """A Mesopotamian standard: a pole crowned by a gold sun-disc and star,
    a player-colour banner with gold fringe hanging from a crossbar."""
    e = empty("standard", loc, rot_z, parent)
    cyl(0.022, h, (0, 0, h / 2), PALMWOOD_DK, parent=e, segs=8)
    zt = h - 0.12
    rod((-w / 2 - 0.04, -0.03, zt), (w / 2 + 0.04, -0.03, zt), 0.016, PALMWOOD_DK, e)
    cloth(w, bh, mat(player, rough=0.7), (0, -0.035, zt), 0, e, hang=True, wave=0.018, seed=seed)
    cloth(w, 0.05, gold(), (0, -0.05, zt - bh * 0.85), 0, e, hang=True, wave=0.012, seed=seed)
    # an eight-pointed star of Ishtar on the cloth
    for k, a in enumerate((0.0, pi / 4)):
        box((0.09, 0.01, 0.09), (0, -0.056 - 0.004 * k, zt - bh * 0.42), gold(), rot=(0, a, 0), parent=e)
    cyl(0.07, 0.02, (0, 0, h + 0.07), gold(), rot=(pi / 2, 0, 0), parent=e, segs=14)
    for k in range(8):
        a = k * pi / 4
        rod((0.07 * cos(a), 0, h + 0.07 + 0.07 * sin(a)), (0.115 * cos(a), 0, h + 0.07 + 0.115 * sin(a)), 0.011,
            gold(), e, 4, 0.002)
    return e


def canopy(parent, x0, x1, y0, y1, z, player=BLUE, posts=True, sag=0.04, fringe=True, stripes=True):
    """A sagging cloth shade on palm-wood poles, the owner's colour striped
    with linen, a gold fringe on the front and right edges."""
    ms = [mat(player, rough=0.75), mat("linen", rough=0.8)]
    n = max(2, round((x1 - x0) / 0.16)) if stripes else 1
    for i in range(n):
        a, b = x0 + (x1 - x0) * i / n, x0 + (x1 - x0) * (i + 1) / n
        vs, fs = [], []
        ny = 4
        for j in range(ny + 1):
            yy = y0 + (y1 - y0) * j / ny
            dz = -sag * sin(pi * j / ny)
            vs += [(a, yy, z + dz), (b, yy, z + dz)]
        for j in range(ny):
            fs.append((2 * j, 2 * j + 1, 2 * j + 3, 2 * j + 2))
        mesh(vs, fs, ms[i % 2] if stripes else ms[0], parent=parent, smooth=True, name="canopy")
    if fringe:
        g = Geo()
        for i in range(int((x1 - x0) / 0.05)):
            g.box((0.018, 0.012, 0.07), (x0 + 0.025 + i * 0.05, y0 - 0.006, z - 0.035))
        for i in range(int((y1 - y0) / 0.05)):
            g.box((0.012, 0.018, 0.07), (x1 + 0.006, y0 + 0.025 + i * 0.05, z - 0.035))
        g.emit(mat(GLAZE_Y, rough=0.6), parent, "fringe")
    if posts:
        for x in (x0 + 0.02, x1 - 0.02):
            for y in (y0 + 0.02, y1 - 0.02):
                rod((x, y, 0.0 if posts is True else posts), (x, y, z + 0.02), 0.022, PALMWOOD_DK, parent, 6)


def reed_roof(parent, x0, x1, y0, y1, z, rise=0.0, posts=True, z_post=0.0, valance=None, beams=True):
    """A flat (or gently pitched toward -Y by `rise`) roof of reed matting on
    palm-log beams; `valance` hangs a player-colour cloth along the front."""
    g = Geo()
    L = y1 - y0
    a = atan2(rise, L)
    n = max(3, int((x1 - x0) / 0.055))
    for i in range(n):
        xx = x0 + (i + 0.5) * (x1 - x0) / n
        g.box(((x1 - x0) / n * 0.92, hypot(L, rise) + 0.08, 0.035), (xx, (y0 + y1) / 2, z + rise / 2))
    ob = g.emit(noisy(REED, 0.14, 12.0, 0.95), parent, "reeds")
    if ob is not None and rise:
        # tilt the mat about the X axis through its centre
        import mathutils
        mx = mathutils.Matrix.Translation((0, (y0 + y1) / 2, z + rise / 2)) @ mathutils.Matrix.Rotation(a, 4, "X") \
            @ mathutils.Matrix.Translation((0, -(y0 + y1) / 2, -(z + rise / 2)))
        ob.data.transform(mx)
    if beams:
        for y, zz in ((y0 + 0.03, z - 0.035), (y1 - 0.03, z + rise - 0.035)):
            rod((x0 - 0.06, y, zz), (x1 + 0.06, y, zz), 0.035, PALMWOOD, parent, 7)
        if L > 0.8:
            ym = (y0 + y1) / 2
            rod((x0 - 0.06, ym, z + rise / 2 - 0.035), (x1 + 0.06, ym, z + rise / 2 - 0.035), 0.032, PALMWOOD, parent, 7)
    if posts:
        for x in (x0 + 0.04, x1 - 0.04):
            for y, zz in ((y0 + 0.03, z), (y1 - 0.03, z + rise)):
                if posts == "front" and y > y0 + 0.1:
                    continue
                rod((x, y, z_post), (x, y, zz - 0.04), 0.035, PALMWOOD_DK, parent, 7)
    if valance is not None:
        cloth(x1 - x0 + 0.06, 0.13, mat(valance, rough=0.7), ((x0 + x1) / 2, y0 - 0.0, z - 0.02), 0, parent,
              hang=True, wave=0.01, nx=8)


def tannur(parent, x, y, z=0.0, s=1.0):
    """A domed clay bread oven."""
    lathe([(0.17, 0), (0.18, 0.12), (0.13, 0.26), (0.06, 0.32), (0.05, 0.34), (0.03, 0.34)], plaster(MUD_DK),
          (x, y, z), parent=parent, s=s, segs=12)
    box((0.08 * s, 0.02, 0.07 * s), (x, y - 0.165 * s, z + 0.06 * s), mat((0.12, 0.07, 0.05)), parent=parent)


def lamassu(loc, rz=0.0, parent=None, s=1.0):
    """A winged human-headed bull of pale alabaster on a plinth, its body
    along local Y, head toward -Y: horned cap, black curled beard, gold.
    About 1.0 tall at s=1."""
    e = face(loc, rz, parent)
    e.scale = (s, s, s)
    st = plaster(ALABASTER)
    stm = mat(ALABASTER, rough=0.6)
    box((0.36, 0.96, 0.08), (0, 0, 0.04), brick(MUD_DK, 22.0), parent=e, bevel=0.01)
    box((0.38, 0.98, 0.025), (0, 0, 0.085), glaze(), parent=e)
    z0 = 0.1
    box((0.26, 0.62, 0.3), (0, 0.04, z0 + 0.5), st, parent=e, bevel=0.05)          # body
    box((0.25, 0.2, 0.34), (0, -0.2, z0 + 0.52), st, parent=e, bevel=0.05)         # chest
    for y in (-0.26, -0.14, 0.18, 0.3):                                          # legs (one more seen in front)
        for sx in (-1, 1):
            box((0.075, 0.085, 0.38), (sx * 0.08, y, z0 + 0.19), stm, parent=e, bevel=0.015)
            box((0.085, 0.1, 0.05), (sx * 0.08, y - 0.005, z0 + 0.025), mat(DIORITE, rough=0.5), parent=e)
    rod((0, 0.35, z0 + 0.6), (0, 0.42, z0 + 0.3), 0.02, stm, e, 6)
    sphere(0.04, (0, 0.43, z0 + 0.28), mat(DIORITE), parent=e, segs=6, scale=(1, 1, 1.4))
    # wings, sweeping up and back from the shoulders
    for sx in (-1, 1):
        x = sx * 0.145
        prof = [(-0.2, 0.62), (0.36, 0.66), (0.4, 0.98), (0.1, 0.96), (-0.22, 0.82)]
        g = Geo()
        g.prism([(y, z0 + zz) for y, zz in prof], (x, 0, 0), (0, 1), (1, 0), -0.012, 0.012)
        g.emit(st, e, "wing")
        g = Geo()
        for row, (zz, y0, y1) in enumerate(((0.7, -0.15, 0.37), (0.8, -0.18, 0.39), (0.9, -0.06, 0.4))):
            nf = 7
            for k in range(nf):
                yy = y0 + (y1 - y0) * (k + 0.5) / nf
                g.box((0.01, (y1 - y0) / nf * 0.7, 0.05), (x + sx * 0.014, yy, z0 + zz + 0.04 * (yy - y0)))
        g.emit(gold(), e, "feathers")
    # head: face, curled beard, horned cylindrical cap
    hy, hz = -0.32, z0 + 0.82
    box((0.15, 0.13, 0.17), (0, hy, hz), stm, parent=e, bevel=0.03)
    box((0.15, 0.08, 0.2), (0, hy - 0.04, hz - 0.15), mat((0.08, 0.08, 0.12), rough=0.5), parent=e, bevel=0.02)
    for k in range(3):
        box((0.155, 0.085, 0.012), (0, hy - 0.042, hz - 0.08 - k * 0.06), mat((0.2, 0.2, 0.3)), parent=e)
    box((0.17, 0.1, 0.1), (0, hy + 0.06, hz - 0.03), mat((0.08, 0.08, 0.12), rough=0.5), parent=e, bevel=0.02)
    for sx in (-1, 1):
        sphere(0.014, (sx * 0.035, hy - 0.068, hz + 0.02), "black", parent=e, segs=6)
    cyl(0.08, 0.15, (0, hy + 0.01, hz + 0.16), stm, parent=e, segs=12, r_top=0.085)
    for zz in (0.1, 0.16, 0.22):
        cyl(0.086, 0.015, (0, hy + 0.01, hz + zz), gold(), parent=e, segs=12)
    lathe([(0.085, 0), (0.095, 0.02), (0.06, 0.04)], gold(), (0, hy + 0.01, hz + 0.235), parent=e, segs=12)
    return e


def lion(loc, rz=0.0, parent=None, s=1.0, c=GLAZE_Y):
    """A striding lion in yellow glaze, side-on along local X (a relief or a
    small statue on a plinth)."""
    e = face(loc, rz, parent)
    e.scale = (s, s, s)
    m = mat(c, rough=0.4)
    box((0.34, 0.1, 0.11), (0, 0, 0.17), m, parent=e, bevel=0.03)
    sphere(0.085, (-0.17, 0, 0.21), mat(tint(c, 0.8), rough=0.5), parent=e, segs=10)   # mane
    box((0.07, 0.07, 0.06), (-0.24, 0, 0.21), m, parent=e, bevel=0.015)
    for x in (-0.12, 0.13):
        for sy in (-1, 1):
            box((0.04, 0.035, 0.13), (x + sy * 0.02, sy * 0.03, 0.065), m, parent=e)
    rod((0.17, 0, 0.2), (0.26, 0, 0.27), 0.012, m, e, 5)
    return e


def battering_ram(loc, rz=0.0, parent=None, player=BLUE, done=False):
    """An Assyrian six-wheeled ram, long axis local X, ram toward -X: a long
    bed, a wicker-and-hide body with a turret. Half built (done=False): a
    bare timber frame with a few hides on, a wheel off, the ram beam on
    trestles beside it."""
    e = empty("ram", loc, rz, parent)
    wr = 0.15
    zb = wr + 0.05
    for sy in (-1, 1):
        box((1.1, 0.07, 0.07), (0, sy * 0.2, zb), mat(PALMWOOD), parent=e, bevel=0.008)
    for x in (-0.45, -0.15, 0.15, 0.45):
        box((0.06, 0.46, 0.05), (x, 0, zb + 0.05), mat("wood"), parent=e)
    for x in (-0.38, 0.0, 0.38):
        for sy in (-1, 1):
            if not done and (x, sy) == (0.38, -1):
                continue
            greek.wheel((x, sy * 0.27, wr), e, wr, spokes=4)
        rod((x, -0.3, wr), (x, 0.3, wr), 0.018, "wood_dark", e)
    # the frame: uprights, a sloped roof line, the turret at the front
    hide = mat((0.62, 0.46, 0.3), rough=0.9)
    for x in (-0.48, -0.2, 0.1, 0.42):
        hh = 0.62 if x > -0.3 else 0.95
        for sy in (-1, 1):
            box((0.045, 0.045, hh), (x, sy * 0.2, zb + hh / 2), mat(PALMWOOD), parent=e)
        rod((x, -0.2, zb + hh), (x, 0.2, zb + hh), 0.02, mat(PALMWOOD), e)
    for sy in (-1, 1):
        rod((-0.48, sy * 0.2, zb + 0.62), (0.42, sy * 0.2, zb + 0.62), 0.02, mat(PALMWOOD), e)
        rod((-0.48, sy * 0.2, zb + 0.95), (-0.2, sy * 0.2, zb + 0.95), 0.02, mat(PALMWOOD), e)
    if done:
        box((0.94, 0.44, 0.6), (-0.03, 0, zb + 0.32), hide, parent=e)
    else:
        # a few hides lashed on the near side and the turret, the rest bare
        box((0.32, 0.015, 0.5), (-0.34, -0.215, zb + 0.32), hide, parent=e)
        box((0.32, 0.015, 0.5), (0.26, -0.215, zb + 0.32), hide, parent=e)
        box((0.29, 0.015, 0.25), (-0.34, -0.215, zb + 0.79), hide, parent=e)
        cloth(0.28, 0.24, mat(player, rough=0.7), (-0.34, -0.23, zb + 0.9), 0, e, hang=True, wave=0.006, nx=4)
        # the ram beam with its bronze head, on trestles
        for x in (0.0, 0.5):
            for sx in (-1, 1):
                rod((x + sx * 0.08, -0.56, 0.0), (x, -0.56, 0.26), 0.014, "wood_dark", e)
        rod((-0.32, -0.56, 0.3), (0.72, -0.56, 0.3), 0.045, mat(PALMWOOD), e, 8)
        cyl(0.06, 0.16, (-0.38, -0.56, 0.3), greek.bronze(), rot=(0, pi / 2, 0), parent=e, segs=8, r_top=0.02)
        greek.wheel((0.62, -0.32, wr), e, wr, rot=(pi / 2, 0.35, 0), spokes=4)
    return e


# ----------------------------------------------------------------- temple


def _horns(parent, x0, x1, y0, y1, z, s=1.0):
    """Gilded horns of divinity on a shrine's four corners."""
    for x in (x0, x1):
        for y in (y0, y1):
            for k in (-1, 1):
                a = atan2(y, x) + k * 0.6
                rod((x, y, z), (x + 0.07 * s * cos(a), y + 0.07 * s * sin(a), z + 0.16 * s), 0.022 * s, gold(),
                    parent, 6, 0.004)


def _shrine(r, x0, x1, y0, y1, z, h, player=BLUE, door_w=0.24):
    """A glazed-blue cella on a ziggurat's top, gold band and horns, its
    arched gold door to -Y."""
    mud_block(r, x0, x1, y0, y1, h, z, ribs=True, pitch=0.2, dado=0.0, band=0.8, band_c=GLAZE_Y,
              rosettes=True, merlons=False, roof=MUD_LT, wall_m=glaze())
    box((x1 - x0 + 0.14, y1 - y0 + 0.14, 0.06), ((x0 + x1) / 2, (y0 + y1) / 2, z + h + 0.03), gold(), parent=r)
    arch_door(((x0 + x1) / 2, y0 - 0.07, z), 0, r, door_w, h * 0.68, frame=GLAZE_Y, inner=GLAZE_W,
              door=(0.95, 0.72, 0.25))
    _horns(r, x0 - 0.05, x1 + 0.05, y0 - 0.05, y1 + 0.05, z + h + 0.05)


def temple(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A small ziggurat: two buttressed mudbrick stages climbed by a
    front stair, a glazed-blue shrine with gilded horns on top, braziers
    and the owner's standards at the stair's foot."""
    r = empty("temple", loc, rot_z, parent)
    box((1.92, 1.92, 0.03), (0, 0, 0.015), brick(MUD_LT, 4.0), parent=r, bevel=0.01)
    z = 0.03
    t1 = mud_block(r, -0.88, 0.88, -0.5, 0.92, 0.58, z, band=0.78, mh=0.12)
    t2 = mud_block(r, -0.6, 0.6, -0.12, 0.84, 0.46, t1, c=MUD_DK, band=0.72, mh=0.11, pitch=0.24)
    _shrine(r, -0.32, 0.32, 0.18, 0.7, t2, 0.5, player)
    stair(r, 0.0, -0.93, -0.5, z, t1, 0.36, 7)
    stair(r, 0.0, -0.5 + 0.004, -0.12, t1, t2, 0.3, 6)
    for sx in (-1, 1):
        standard((sx * 0.36, -0.82, z), 1.35, player, r, 0.22, 0.4, seed=sx + 2)
        greek.brazier((sx * 0.62, -0.38, t1), r, 0.75)
    jar(r, -0.72, -0.78, z, 0.34)
    jar(r, 0.7, -0.8, z, 0.3, band=TURQ)
    palm(r, 0.78, -0.62, z, 1.3, seed=3, lean=(0.06, -0.02), s=0.8)
    return r


# ----------------------------------------------------------------- houses


def yard_wall(r, pts, h=0.34, t=0.08, m=None):
    """A low mudbrick yard wall along an axis-aligned polyline, plaster coped.
    Each later segment starts at the joint's face, its coping a little
    further, so no two faces lie on one another."""
    m = m or brick(MUD, 16.0)
    cap = plaster()
    for i, (a, b) in enumerate(zip(pts, pts[1:])):
        ax_ = 0 if abs(b[0] - a[0]) > abs(b[1] - a[1]) else 1
        sg = 1 if b[ax_] > a[ax_] else -1
        w0 = a[ax_] - sg * t / 2 if i == 0 else a[ax_] + sg * t / 2
        c0 = a[ax_] - sg * (t / 2 + 0.015) if i == 0 else a[ax_] + sg * (t / 2 + 0.015)
        w1 = b[ax_] + sg * t / 2
        c1 = b[ax_] + sg * (t / 2 + 0.015)
        for lo, hi, zz, hh, th, mm in ((w0, w1, h / 2, h, t, m), (c0, c1, h + 0.02, 0.04, t + 0.03, cap)):
            L, c = abs(hi - lo), (lo + hi) / 2
            if ax_ == 0:
                box((L, th, hh), (c, a[1], zz), mm, parent=r)
            else:
                box((th, L, hh), (a[0], c, zz), mm, parent=r)


def gate_pier(r, x, y, h=0.5, m=None):
    box((0.13, 0.13, h), (x, y, h / 2), m or brick(MUD, 16.0), parent=r)
    box((0.16, 0.16, 0.045), (x, y, h + 0.02), glaze(), parent=r)


def _parapet(r, x0, x1, y0, y1, z, h=0.1, t=0.06):
    """A low rounded mud parapet round a flat roof (houses: no merlons)."""
    m = plaster(tint(MUD_LT, 0.95))
    for (ax, ay), (bx, by), (nx, ny) in _edges(x0, x1, y0, y1):
        cx_, cy_ = (ax + bx) / 2 - nx * t / 2, (ay + by) / 2 - ny * t / 2
        L = hypot(bx - ax, by - ay) - (2 * t if nx else 0)
        sx_, sy_ = (t, L) if nx else (L, t)
        box((sx_, sy_, h), (cx_, cy_, z + h / 2), m, parent=r, bevel=0.02, name="parapet")


def house(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, variant=0):
    """2 x 2. A flat-roofed mudbrick house round a yard: palm-log beam ends,
    an upper room, a stair to the roof and a striped shade on it in the
    owner's colour; an oven, jars and a date palm in the yard."""
    r = empty("house%d" % variant, loc, rot_z, parent)
    box((1.92, 1.92, 0.03), (0, 0, 0.015), noisy("sand", 0.1, 3.0, 1.0), parent=r, bevel=0.01)
    z = 0.03
    x0, x1, y0, y1, h = -0.94, 0.4, -0.05, 0.94, 1.0
    zt = mud_block(r, x0, x1, y0, y1, h, z, ribs=False, band=0.86, band_h=0.06, rosettes=False, beams=True,
                   merlons=False)
    _parapet(r, x0, x1, y0, y1, zt)
    # the upper room at the back
    ux0, ux1, uy0 = -0.94, -0.2, 0.36
    mud_block(r, ux0, ux1, uy0, y1, 0.55, zt, ribs=False, band=0.8, band_h=0.05, rosettes=False, beams=True,
              merlons=False)
    _parapet(r, ux0, ux1, uy0, y1, zt + 0.55)
    arch_door(((ux0 + ux1) / 2 + 0.1, uy0, zt), 0, r, 0.2, 0.42, frame=TURQ, inner=GLAZE_W)
    slit((ux1, (uy0 + y1) / 2, zt + 0.18), pi / 2, r, 0.08, 0.16)
    arch_door((-0.42, y0, z), 0, r, 0.3, 0.72, frame=LAPIS)
    slit((-0.78, y0, z + 0.5), 0, r, 0.09, 0.2)
    slit((0.1, y0, z + 0.5), 0, r, 0.09, 0.2)
    slit((x1, 0.45, z + 0.5), pi / 2, r, 0.09, 0.2)
    # the shade on the roof
    canopy(r, 0.0, 0.32, 0.02, 0.86, zt + 0.5, player, posts=zt, sag=0.03)
    # stair up the +X side, climbing toward +Y
    stair(r, x1 + 0.16, -0.02, 0.86, z, zt, 0.28, 8)
    # the yard: low wall, oven, jars, palm, a goat's trough
    yard_wall(r, [(-0.25, -0.92), (-0.92, -0.92)])
    yard_wall(r, [(0.2, -0.92), (0.92, -0.92), (0.92, -0.1)])
    for x in (-0.25, 0.2):
        gate_pier(r, x, -0.92)
    tannur(r, 0.55, -0.55, z)
    jar(r, -0.75, -0.6, z, 0.36)
    jar(r, -0.55, -0.72, z, 0.3, c=(0.7, 0.5, 0.34), band=TURQ)
    palm(r, -0.74, -0.3, z, 1.5, seed=1, lean=(-0.03, -0.06), s=0.85)
    basket(( -0.05, -0.55, z), r, 0.11, 4, 3)
    sack((0.25, -0.35, z), r, 0.11, 2)
    box((0.42, 0.62, 0.012), (0.24, 0.42, zt + 0.032), mat(RUG[variant % 4], rough=0.9), parent=r)
    jar(r, -0.1, 0.25, zt + 0.02, 0.26)
    return r


# ------------------------------------------------------------ watch tower


def watch_tower(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """1 x 1. A tall buttressed mudbrick tower, glazed bands, stepped
    merlons, a reed sun-shade and the owner's pennant on top, a ladder."""
    r = empty("watch_tower", loc, rot_z, parent)
    box((0.94, 0.94, 0.05), (0, 0, 0.025), brick(MUD_DK, 9.0), parent=r, bevel=0.01)
    z = 0.05
    z1 = mud_block(r, -0.36, 0.36, -0.36, 0.36, 1.0, z, pitch=0.24, band=0.0, merlons=False, roof=None)
    zt = mud_block(r, -0.31, 0.31, -0.31, 0.31, 0.72, z1, pitch=0.21, band=0.72, band_h=0.08, mh=0.13,
                   c=tint(MUD, 1.02))
    box((0.8, 0.8, 0.06), (0, 0, z1 + 0.0), glaze(), parent=r)
    arch_door((0, -0.401, z), 0, r, 0.26, 0.62)
    slit((0, -0.31 - 0.065, z1 + 0.2), 0, r)
    slit((0.31 + 0.065, 0, z1 + 0.2), pi / 2, r)
    slit((0.401, 0.0, z + 0.55), pi / 2, r)
    shield((0.43, -0.2, z1 - 0.32), pi / 2, r, 0.12, player, 0)
    # reed shade on four poles, the pennant above
    reed_roof(r, -0.24, 0.24, -0.24, 0.24, zt + 0.5, posts=True, z_post=zt, valance=player, beams=True)
    flag((0.2, 0.2, zt + 0.52), 0.6, player, r, w=0.45, fh=0.22, pennant=True)
    greek.brazier((-0.1, 0.05, zt), r, 0.6)
    # ladder up the +X face
    for t in (-0.09, 0.09):
        rod((0.56, 0.12 + t, z), (0.44, 0.12 + t, z1), 0.014, PALMWOOD, r)
    for i in range(8):
        f = (i + 0.6) / 8.5
        rod((0.56 - 0.12 * f, 0.03, z + f * (z1 - z)), (0.56 - 0.12 * f, 0.21, z + f * (z1 - z)), 0.01, PALMWOOD, r)
    jar(r, -0.3, -0.44, z, 0.24)
    return r


# ------------------------------------------------------- glazed gateways


def _bull(e, x, z, s=0.1, m=None, d=0.0):
    """A striding bull or dragon in raised glazed brick on a face frame
    (local -Y outward), facing -X, `s` tall at the back."""
    g = Geo()
    g.box((s * 1.5, 0.012, s * 0.55), (x, d - 0.006, z + s * 0.62))           # body
    g.box((s * 0.42, 0.012, s * 0.42), (x - s * 0.8, d - 0.006, z + s * 0.82))  # head
    g.box((s * 0.1, 0.012, s * 0.4), (x - s * 0.84, d - 0.006, z + s * 1.12))  # horn
    for k in (-0.55, -0.25, 0.3, 0.6):
        g.box((s * 0.13, 0.012, s * 0.4), (x + k * s, d - 0.006, z + s * 0.2))
    g.box((s * 0.5, 0.012, s * 0.08), (x + s * 0.95, d - 0.006, z + s * 0.7), 0.0)
    g.emit(m or mat(GLAZE_Y, rough=0.35), e, "bull")


def glazed_gate(r, x0, x1, y0, y1, h, z=0.0, door_w=0.36, door_h=0.95, beasts=True, player=None):
    """An Ishtar-gate-like portal block of lapis glaze: yellow band with white
    rosettes, white stepped merlons, rows of yellow bulls on its -Y face,
    an arched gold-framed door."""
    zt = mud_block(r, x0, x1, y0, y1, h, z, wall_m=glaze(), pitch=0.22, band=0.9, band_c=GLAZE_Y, dado=0.1,
                   merlon_m=mat(GLAZE_W, rough=0.35), mh=0.17)
    xc = (x0 + x1) / 2
    co = 0.035 + 0.03
    arch_door((xc, y0 - co, z), 0, r, door_w, door_h, frame=GLAZE_Y, inner=GLAZE_W)
    if beasts:
        e = face((0, y0, 0), 0, r)
        w = x1 - x0
        for row, zz in enumerate((z + door_h + 0.12, z + 0.32)):
            n = max(1, int((w - 0.1) / 0.3))
            for i in range(n):
                bx = x0 + 0.08 + (i + 0.5) * (w - 0.16) / n
                if row == 1 and abs(bx - xc) < door_w / 2 + 0.16:
                    continue
                if zz + 0.13 > z + h * 0.9 - 0.05:
                    continue
                _bull(e, bx, zz, 0.11, mat(GLAZE_Y if row == 0 else GLAZE_W, rough=0.35), -co + 0.0)
    return zt


# ------------------------------------------------------------ town centre


def town_center(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """3 x 3. A palace compound: a buttressed hall with a tall keep, a glazed
    gate flanked by lamassu, a lower wing with a stair to the roofs, and a
    walled court with a palm, an oven, a shaded stall and a cart."""
    r = empty("town_center", loc, rot_z, parent)
    box((2.94, 2.94, 0.04), (0, 0, 0.02), brick(MUD_LT, 4.5), parent=r, bevel=0.012)
    z = 0.04
    zt = mud_block(r, -1.42, 0.98, 0.14, 1.42, 1.38, z, band=0.8)
    zk = mud_block(r, -1.42, -0.48, 0.52, 1.42, 0.78, zt, c=MUD_DK, dado=0.0, band=0.72, pitch=0.26)
    slit((-0.95, 0.52 - 0.065, zt + 0.28), 0, r, 0.08, 0.24)
    slit((-0.48 + 0.065, 0.97, zt + 0.28), pi / 2, r, 0.08, 0.24)
    for x in (-1.15, 0.6):
        slit((x, 0.14 - 0.065, z + 0.62), 0, r)
    glazed_gate(r, -0.66, 0.16, -0.12, 0.2, 1.72, z, door_w=0.38, door_h=0.98)
    for sx in (-1, 1):
        lamassu((-0.25 + sx * 0.6, -0.5, z), 0, r, 0.62)
    # the east wing, lower, with a door and a stair up its court side
    zw = mud_block(r, 0.9, 1.42, -0.8, 1.42, 1.02, z, band=0.78, pitch=0.28)
    arch_door((1.16, -0.8 - 0.065, z), 0, r, 0.26, 0.66)
    for y in (-0.35, 0.3, 0.95):
        slit((1.42 + 0.065, y, z + 0.5), pi / 2, r)
    stair(r, 0.72, -0.78, 0.1, z, zw, 0.26, 9)
    canopy(r, 1.0, 1.34, -0.72, -0.05, zw + 0.5, player, posts=zw, sag=0.03)
    # the court wall and its gate piers
    yard_wall(r, [(-0.55, -1.4), (-1.4, -1.4), (-1.4, 0.1)], h=0.42)
    yard_wall(r, [(0.05, -1.4), (0.86, -1.4)], h=0.42)
    for x in (-0.55, 0.05):
        gate_pier(r, x, -1.4, 0.62)
        greek.brazier((x, -1.4, 0.645), r, 0.5)
    # the court
    palm(r, -1.18, -1.12, z, 1.6, seed=4, lean=(0.03, 0.03), s=0.9)
    tannur(r, -1.15, -0.15, z)
    jar(r, 0.55, -0.15, z, 0.36)
    jar(r, 0.38, -0.08, z, 0.3, band=TURQ)
    jar(r, 0.5, 0.02, z, 0.28, c=(0.68, 0.48, 0.32))
    # a shaded stall in the court
    canopy(r, 0.12, 0.78, -1.22, -0.72, 0.82, player, sag=0.035)
    box((0.56, 0.24, 0.04), (0.45, -1.02, 0.36), mat(PALMWOOD), parent=r)
    for x in (0.2, 0.7):
        box((0.04, 0.2, 0.34), (x, -1.02, 0.19), mat(PALMWOOD_DK), parent=r)
    for i, x in enumerate((0.27, 0.45, 0.63)):
        basket((x, -1.02, 0.38), r, 0.08, i + 1, i)
    greek.cart((-0.1, -0.85, z), 0.5, player, r)
    sack_pile((0.62, -0.5, z), r, seed=4, n=(3, 1), s=0.1)
    standard((-0.82, -0.12, z), 1.7, player, r, 0.26, 0.5, seed=1)
    standard((0.32, -0.12, z), 1.7, player, r, 0.26, 0.5, seed=2)
    flag((-0.95, 0.97, zk), 1.15, player, r, w=0.95, fh=0.55, pole_r=0.03, seed=2)
    return r


# --------------------------------------------------------------- barracks


def barracks(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A crenellated drill yard: a buttressed barrack hall at the back
    with a reed portico, a glazed gate between two towers, shields in the
    owner's colour on the walls, spears, a dummy and a target."""
    r = empty("barracks", loc, rot_z, parent)
    box((1.92, 1.92, 0.03), (0, 0, 0.015), noisy("sand", 0.1, 3.0, 1.0), parent=r, bevel=0.01)
    z = 0.03
    zt = mud_block(r, -0.94, 0.94, 0.44, 0.94, 1.12, z, band=0.8)
    for x in (-0.55, 0.55):
        arch_door((x, 0.44 - 0.065, z), 0, r, 0.26, 0.66, open_=True)
    reed_roof(r, -0.9, 0.9, 0.02, 0.36, 0.86, rise=0.05, posts="front", valance=player)
    # yard walls with merlons, the gate towers
    wh = 0.6
    mud_block(r, -0.94, -0.3, -0.94, -0.82, wh, z, band=0.0, pitch=0.3, rosettes=False, mh=0.12, roof=None)
    mud_block(r, 0.3, 0.94, -0.94, -0.82, wh, z, band=0.0, pitch=0.3, rosettes=False, mh=0.12, roof=None)
    mud_block(r, 0.81, 0.93, -0.81, 0.44, wh, z, band=0.0, pitch=0.3, rosettes=False, mh=0.12, roof=None)
    for sx in (-1, 1):
        mud_block(r, sx * 0.3 - 0.13, sx * 0.3 + 0.13, -0.98, -0.74, 1.08, z, band=0.82, rosettes=False, pitch=0.2,
                  mh=0.12)
    box((0.36, 0.2, 0.22), (0, -0.86, z + 0.86), glaze(), parent=r)
    box((0.38, 0.22, 0.04), (0, -0.86, z + 0.99), mat(GLAZE_Y, rough=0.35), parent=r)
    cloth(0.26, 0.2, mat(player, rough=0.7), (0, -0.975, z + 0.84), 0, r, hang=True, wave=0.008, nx=4)
    for x in (-0.68, 0.62):
        shield((x, -1.0, 0.32), 0, r, 0.13, player, 0)
    for y in (-0.45, 0.05):
        shield((0.995, y, 0.32), pi / 2, r, 0.13, player, 0)
    # spear rack against the hall, a dummy, a target, a standard
    rod((-0.3, 0.38, z + 0.6), (0.4, 0.38, z + 0.6), 0.016, PALMWOOD_DK, r)
    for i in range(5):
        x = -0.24 + i * 0.15
        greek.spear((x, 0.32, z + 0.02), (x + 0.02, 0.38, z + 0.95), r)
    for i in range(2):
        shield((-0.75 + i * 0.3, 0.38, z + 0.36), 0, r, 0.12, player, 0)
    dx, dy = 0.42, -0.36
    cyl(0.03, 0.88, (dx, dy, z + 0.44), "wood_dark", parent=r, segs=6)
    cyl(0.1, 0.34, (dx, dy, z + 0.6), REED, parent=r, segs=8, r_top=0.085)
    rod((dx - 0.25, dy, z + 0.7), (dx + 0.25, dy, z + 0.7), 0.02, PALMWOOD, r)
    sphere(0.08, (dx, dy, z + 0.9), REED, parent=r, segs=8)
    lathe([(0.0, 0.0), (0.09, 0.0), (0.06, 0.1), (0.0, 0.16)], greek.bronze(), (dx, dy, z + 0.9), parent=r, segs=10)
    target((-0.42, -0.4, z + 0.42), 0.4, r, 0.2, 3, seed=2)
    standard((-0.62, 0.1, z), 1.6, player, r, 0.26, 0.48, seed=3)
    greek.bench((-0.2, -0.55, z), 0.0, r, 0.4, PALMWOOD)
    jar(r, 0.62, 0.2, z, 0.3)
    return r


# ---------------------------------------------------------------- academy


def academy(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A tablet house (edubba): a buttressed hall of tablet shelves, a
    shaded scholars' court with benches of clay tablets, tablets drying on
    reed mats, and a black diorite law stele."""
    r = empty("academy", loc, rot_z, parent)
    box((1.92, 1.92, 0.035), (0, 0, 0.015), brick(MUD_LT, 4.0), parent=r, bevel=0.01)
    z = 0.035
    zt = mud_block(r, -0.94, 0.94, 0.42, 0.94, 1.22, z, band=0.8)
    arch_door((0.0, 0.42 - 0.065, z), 0, r, 0.32, 0.8)
    tablet_shelf((-0.5, 0.42 - 0.07, z), 0, r, 0.6, 0.72, seed=1)
    tablet_shelf((0.52, 0.42 - 0.07, z), 0, r, 0.56, 0.72, seed=2)
    slit((0.94 + 0.065, 0.68, z + 0.7), pi / 2, r)
    zw = mud_block(r, -0.94, -0.52, -0.94, 0.5, 0.92, z, band=0.78, pitch=0.28)
    for y in (-0.6, -0.05):
        slit((-0.52 + 0.065, y, z + 0.5), pi / 2, r)
    # the shaded court: benches of tablets, the teacher's seat
    canopy(r, -0.36, 0.42, -0.42, 0.28, 0.98, player, sag=0.04)
    for i, y in enumerate((-0.2, 0.08)):
        greek.bench((0.03, y, z), 0.0, r, 0.58, PALMWOOD)
        for k in range(3):
            tablet_stack(r, -0.17 + k * 0.2, y, z + 0.215, 2 + (k + i) % 2, 0.2 * k, seed=k + 3 * i)
    box((0.2, 0.2, 0.3), (0.0, -0.38, z + 0.15), plaster(ALABASTER), parent=r, bevel=0.015)
    box((0.22, 0.22, 0.04), (0.0, -0.38, z + 0.31), glaze(), parent=r)
    tablet_stack(r, 0.0, -0.38, z + 0.33, 3, 0.4, seed=9)
    # tablets drying in the sun on reed mats
    for k, (x, y) in enumerate(((-0.28, -0.75), (0.12, -0.78))):
        box((0.36, 0.26, 0.012), (x, y, z + 0.006), noisy(REED, 0.14, 12.0), parent=r)
        for i in range(4):
            for j in range(3):
                box((0.06, 0.045, 0.018), (x - 0.12 + i * 0.08, y - 0.07 + j * 0.07, z + 0.02),
                    mat(tint(CLAY, 0.96 + 0.04 * ((i + j + k) % 3)), rough=0.9), parent=r)
    stele((0.66, -0.35, z), 0.15, r, 0.9)
    jar(r, 0.58, 0.18, z, 0.3)
    jar(r, 0.75, 0.05, z, 0.26, band=TURQ)
    tablet_stack(r, 0.72, 0.26, z, 4, 0.3)
    palm(r, 0.8, 0.3, z, 1.35, seed=5, lean=(0.03, -0.04), s=0.75)
    standard((-0.42, -0.86, z), 1.5, player, r, 0.24, 0.44, seed=4)
    return r


# ---------------------------------------------------------- archery range


def archery_range(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. Three painted targets before a crenellated mudbrick butt, lane
    pennants, a shooting rail with quivers and bows, a reed shelter."""
    r = empty("archery_range", loc, rot_z, parent)
    box((1.92, 1.92, 0.03), (0, 0, 0.015), noisy("sand", 0.1, 3.0, 1.0), parent=r, bevel=0.01)
    z = 0.03
    mud_block(r, -0.94, 0.36, 0.72, 0.92, 0.66, z, band=0.72, pitch=0.3, mh=0.13)
    for i in range(5):
        blob(0.15, (-0.82 + i * 0.28, 0.6, z + 0.04), ("earth", "grass_dry", "sand")[i % 3], (1.2, 0.9, 0.6), r, i,
             0.25, 1)
    for i, x in enumerate((-0.7, -0.24, 0.2)):
        target((x, 0.5, z + 0.55), 0.0, r, 0.24, 3, seed=i)
        box((0.025, 1.2, 0.008), (x - 0.23, 0.0, z + 0.006), "whitewash", parent=r)
    box((0.025, 1.2, 0.008), (0.43, 0.0, z + 0.006), "whitewash", parent=r)
    box((1.36, 0.04, 0.008), (-0.25, -0.6, z + 0.006), "whitewash", parent=r)
    for i, x in enumerate((-0.9, -0.47, -0.01, 0.34)):
        flag((x, 0.62, z), 1.05 if i % 3 == 0 else 0.9, player, r, w=0.32, fh=0.17, pennant=True, seed=i)
    for x in (-0.9, -0.3, 0.3, 0.9):
        box((0.05, 0.05, 0.4), (x, -0.76, z + 0.2), mat(PALMWOOD_DK), parent=r)
    rod((-0.92, -0.76, z + 0.36), (0.92, -0.76, z + 0.36), 0.02, PALMWOOD, r)
    quiver((-0.62, -0.81, z), r, player, 0.2)
    quiver((-0.02, -0.81, z), r, player, -0.15)
    quiver((0.55, -0.81, z), r, player, 0.1)
    bow((-0.45, -0.805, z + 0.28), 0.0, r, 0.42)
    bow((0.72, -0.805, z + 0.28), 0.0, r, 0.42, "leather")
    # reed shelter in the +X back corner, a bow rack under it
    sx0, sx1, sy0, sy1 = 0.5, 0.94, -0.3, 0.92
    reed_roof(r, sx0, sx1, sy0, sy1, 0.95, rise=0.0, valance=None)
    cloth(sy1 - sy0, 0.16, mat(player, rough=0.7), (sx1 + 0.0, (sy0 + sy1) / 2, 0.92), pi / 2, r, hang=True,
          wave=0.01, nx=8)
    rx, ry = (sx0 + sx1) / 2, (sy0 + sy1) / 2
    for y in (ry - 0.36, ry + 0.36):
        for sx in (-1, 1):
            rod((rx + sx * 0.12, y, z), (rx, y, z + 0.66), 0.014, "wood_dark", r)
    rod((rx, ry - 0.4, z + 0.64), (rx, ry + 0.4, z + 0.64), 0.016, PALMWOOD, r)
    for i in range(5):
        bow((rx - 0.03, ry - 0.3 + i * 0.15, z + 0.4), pi / 2, r, 0.46, ("wood_dark", "leather")[i % 2])
    jar(r, 0.68, -0.52, z, 0.3)
    greek.hay_bale((-0.62, -0.38, z), 0.3, r)
    return r


# ------------------------------------------------------ government centre


def government_centre(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. An audience hall on a brick podium up a broad stair: a glazed
    lapis portal with yellow bulls, a pair of lamassu guarding it, the
    owner's standards and flag."""
    r = empty("government_centre", loc, rot_z, parent)
    box((1.92, 1.92, 0.03), (0, 0, 0.015), noisy("sand", 0.1, 3.0, 1.0), parent=r, bevel=0.01)
    z = 0.03
    ph = 0.26
    mud_block(r, -0.92, 0.92, -0.44, 0.94, ph, z, c=MUD_DK, ribs=False, dado=0.0, band=0.82, band_h=0.06,
              rosettes=False, merlons=False, roof=MUD_LT)
    zp = z + ph + 0.02
    stair(r, 0.0, -0.94, -0.44 + 0.004, z, zp, 0.62, 4)
    zt = mud_block(r, -0.86, 0.86, 0.24, 0.9, 1.28, zp, band=0.78, dado=0.1)
    for x in (-0.62, 0.62):
        slit((x, 0.24 - 0.065, zp + 0.66), 0, r, 0.08, 0.26)
    for y in (0.42, 0.72):
        slit((0.86 + 0.065, y, zp + 0.66), pi / 2, r, 0.08, 0.26)
    glazed_gate(r, -0.34, 0.34, -0.02, 0.3, 1.6, zp, door_w=0.3, door_h=0.82)
    for sx in (-1, 1):
        lamassu((sx * 0.62, -0.1, zp), 0, r, 0.54)
        standard((sx * 0.84, -0.7, z), 1.75, player, r, 0.26, 0.56, seed=sx + 5)
        greek.brazier((sx * 0.4, -0.35, zp), r, 0.6)
    flag((0.55, 0.62, zt), 0.8, player, r, w=0.62, fh=0.36, pole_r=0.025, seed=6)
    return r


# ----------------------------------------------------------------- market


def _stall(r, x, y, z, rz=0.0, player=BLUE, w=0.66, d=0.46, goods=0, seed=0):
    """A counter under a striped shade on four palm poles, front to local -Y."""
    e = empty("stall", (x, y, z), rz, r)
    canopy(e, -w / 2, w / 2, -d / 2, d / 2, 0.86, player, sag=0.03)
    box((w - 0.06, d * 0.55, 0.035), (0, -d * 0.1, 0.38), mat(PALMWOOD), parent=e)
    box((w - 0.04, 0.025, 0.15), (0, -d * 0.38, 0.3), mat(player, rough=0.75), parent=e)
    for xx in (-w / 2 + 0.06, w / 2 - 0.06):
        box((0.04, 0.04, 0.36), (xx, -d * 0.1, 0.18), mat(PALMWOOD_DK), parent=e)
    rnd = random.Random(seed)
    if goods == 0:
        for i in range(3):
            basket(((i - 1) * w * 0.28, -d * 0.1, 0.4), e, 0.08, rnd.randrange(6), seed + i)
    elif goods == 1:
        for i in range(4):
            lathe([(0.03, 0), (0.06, 0.05), (0.05, 0.1), (0.025, 0.13), (0.03, 0.15), (0.02, 0.15)],
                  mat([LAPIS, TURQ, (0.8, 0.58, 0.38), GLAZE_Y][i], rough=0.35), ((i - 1.5) * w * 0.2, -d * 0.1, 0.4),
                  parent=e, segs=8)
    else:
        for i in range(3):
            cyl(0.045, d * 0.5, ((i - 1) * w * 0.26, -d * 0.1, 0.44), mat(RUG[i], rough=0.85), rot=(pi / 2, 0, 0),
                parent=e, segs=8)
    return e


def market(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A bazaar: a row of mudbrick shops with rugs hung at their
    doors under a reed portico, striped stalls of dates, glazed pots and
    cloth, a loaded donkey, a balance, jars and baskets."""
    r = empty("market", loc, rot_z, parent)
    box((1.92, 1.92, 0.035), (0, 0, 0.015), brick(MUD_LT, 4.0), parent=r, bevel=0.01)
    z = 0.035
    zt = mud_block(r, -0.94, 0.94, 0.6, 0.94, 0.98, z, ribs=False, band=0.84, band_h=0.06, rosettes=False,
                   beams=True, merlons=False)
    _parapet(r, -0.94, 0.94, 0.6, 0.94, zt)
    for i, x in enumerate((-0.6, 0.0, 0.6)):
        arch_door((x, 0.6, z), 0, r, 0.26, 0.62, frame=(LAPIS, TURQ, LAPIS)[i], open_=True)
        e = face((x + 0.22, 0.6 - 0.02, z + 0.74), 0, r)
        cloth(0.16, 0.5, mat(RUG[i], rough=0.9), (0, 0, 0), 0, e, hang=True, wave=0.01, nx=3)
        cloth(0.16, 0.06, mat(RUG[(i + 2) % 4], rough=0.9), (0, -0.006, -0.2), 0, e, hang=True, wave=0.01, nx=3)
    reed_roof(r, -0.92, 0.92, 0.26, 0.58, 0.8, rise=0.06, posts="front", valance=player)
    for k in range(4):
        jar(r, 0.35 + k * 0.13, 0.42 - (k % 2) * 0.05, z, 0.26, band=(LAPIS, TURQ)[k % 2])
    for k in range(3):
        cyl(0.045, 0.24, (-0.8 + k * 0.1, 0.44, z + 0.045), mat(RUG[k], rough=0.85), rot=(pi / 2, 0, 0), parent=r,
            segs=8)
    _stall(r, -0.48, -0.18, z, 0.0, player, goods=0, seed=1)
    _stall(r, 0.58, -0.34, z, pi / 2, player, 0.62, 0.42, goods=1, seed=2)
    _stall(r, -0.5, -0.72, z, 0.0, player, 0.6, 0.38, goods=2, seed=3)
    import figures
    figures.donkey((0.12, -0.72, z), -0.4, player, parent=r)
    # a balance on a post
    bxp, byp = 0.05, 0.02
    box((0.05, 0.05, 0.62), (bxp, byp, z + 0.31), mat(PALMWOOD_DK), parent=r)
    rod((bxp - 0.18, byp, z + 0.6), (bxp + 0.18, byp, z + 0.6), 0.01, greek.bronze(), r)
    for sx in (-1, 1):
        rod((bxp + sx * 0.18, byp, z + 0.6), (bxp + sx * 0.18, byp, z + 0.45), 0.003, "wool", r, 4)
        cyl(0.06, 0.012, (bxp + sx * 0.18, byp, z + 0.44), greek.bronze(), parent=r, segs=10)
    box((0.5, 0.36, 0.01), (0.55, 0.12, z + 0.005), mat(RUG[3], rough=0.9), parent=r)
    box((0.42, 0.28, 0.012), (0.55, 0.12, z + 0.008), mat(RUG[2], rough=0.9), parent=r)
    jar(r, 0.48, 0.12, z, 0.28)
    jar(r, 0.64, 0.1, z, 0.24, band=TURQ)
    sack_pile((0.82, -0.82, z), r, seed=3, n=(2, 1), s=0.1)
    basket((-0.86, -0.4, z), r, 0.1, 2, 4)
    flag((0.0, 0.8, zt), 0.7, player, r, w=0.5, fh=0.3, seed=4)
    return r


# --------------------------------------------------------- siege workshop


def siege_workshop(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A reed-roofed timber shed against mudbrick walls, and in the
    yard an Assyrian battering ram half built: bare frame, a few hides on,
    its bronze-headed beam on trestles; a shear-legs crane, timber, hides."""
    r = empty("siege_workshop", loc, rot_z, parent)
    box((1.92, 1.92, 0.03), (0, 0, 0.015), noisy("earth", 0.12, 3.0, 1.0), parent=r, bevel=0.01)
    z = 0.03
    mud_block(r, -0.94, 0.5, 0.78, 0.94, 1.18, z, band=0.8, pitch=0.3, mh=0.12)
    mud_block(r, -0.94, -0.78, -0.12, 0.77, 1.18, z, band=0.8, pitch=0.3, mh=0.12)
    reed_roof(r, -0.76, 0.5, -0.12, 0.76, 1.1, rise=0.08, posts="front", valance=player)
    for y in (0.3,):
        rod((0.46, y, 0.0), (0.46, y, 1.12), 0.035, PALMWOOD_DK, r, 7)
    # tools on the back wall, a bench, a big wheel, hides
    yb = 0.78 - 0.065
    box((0.7, 0.22, 0.04), (-0.32, yb - 0.12, z + 0.42), mat(PALMWOOD), parent=r)
    for x in (-0.62, -0.02):
        box((0.05, 0.2, 0.4), (x, yb - 0.12, z + 0.2), mat(PALMWOOD_DK), parent=r)
    rod((-0.62, yb - 0.01, z + 0.85), (-0.1, yb - 0.01, z + 0.85), 0.012, "wood_dark", r)
    for i, x in enumerate((-0.56, -0.44, -0.3, -0.18)):
        rod((x, yb - 0.02, z + 0.85), (x, yb - 0.02, z + 0.58 + 0.06 * (i % 2)), 0.01, "wood", r, 4)
        box((0.07 if i % 2 else 0.04, 0.012, 0.06), (x, yb - 0.03, z + 0.58 + 0.06 * (i % 2)), "iron", parent=r)
    greek.wheel((0.22, yb - 0.06, z + 0.3), r, 0.26, rot=(pi / 2 - 0.15, 0, 0), spokes=4)
    for i in range(4):
        box((0.9, 0.12, 0.025), (-0.32, 0.18 + i * 0.002, z + 0.013 + i * 0.026), (PALMWOOD, "wood")[i % 2],
            rot=(0, 0, 0.03 * i), parent=r)
    # a frame of hides drying, the owner's colour on one
    hx, hy = -0.62, -0.62
    for x in (hx - 0.26, hx + 0.26):
        rod((x, hy, 0), (x, hy, 0.78), 0.018, PALMWOOD_DK, r)
    rod((hx - 0.3, hy, 0.76), (hx + 0.3, hy, 0.76), 0.016, PALMWOOD, r)
    for k, x in enumerate((hx - 0.13, hx + 0.13)):
        cloth(0.22, 0.42, mat(((0.62, 0.46, 0.3), (0.5, 0.36, 0.22))[k], rough=0.9), (x, hy - 0.01, 0.75), 0, r,
              hang=True, wave=0.012, nx=4, seed=k)
    # the ram, a shear-legs crane over it
    battering_ram((0.28, -0.42, z), pi + 0.12, r, player, done=False)
    cx, cy = 0.3, -0.3
    for px in (cx - 0.55, cx + 0.55):
        rod((px, cy + 0.45, z), (cx, cy + 0.02, z + 1.6), 0.026, PALMWOOD_DK, r)
    rod((cx, cy + 0.02, z + 1.6), (cx, cy + 0.02, z + 0.95), 0.005, "wool", r, 4)
    cyl(0.03, 0.06, (cx, cy + 0.02, z + 0.93), "wood", parent=r, segs=8)
    cloth(0.4, 0.2, mat(player, rough=0.7), (cx, cy + 0.02, z + 1.6), pi / 4, r, taper=0.92, wave=0.04, seed=3)
    greek.timber_stack((-0.2, -0.82, z), pi / 2, r, (3, 2, 1), 0.7, 0.05)
    greek.sawhorse((-0.3, -0.25, z), 0.3, r)
    for k in range(3):
        cyl(0.1 - k * 0.022, 0.03, (0.78, 0.55, z + 0.015 + k * 0.03), "wool", parent=r, segs=12)
    return r


# ----------------------------------------------------------------- stable


def stable(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A row of mudbrick stalls with horses looking out over half
    doors under arched openings, a reed lean-to of hay, a palm-rail paddock
    with a horse in a saddle cloth, a trough."""
    r = empty("stable", loc, rot_z, parent)
    box((1.92, 1.92, 0.03), (0, 0, 0.015), noisy("earth", 0.12, 3.0, 1.0), parent=r, bevel=0.01)
    z = 0.03
    x0, x1, y0, y1, h = -0.94, 0.56, 0.32, 0.94, 1.0
    zt = mud_block(r, x0, x1, y0, y1, h, z, band=0.86, band_h=0.07, rosettes=True, merlons=False, beams=False)
    _parapet(r, x0, x1, y0, y1, zt)
    coats = [("cedar", "hair_black", True), ((0.93, 0.9, 0.84), (0.7, 0.66, 0.6), False),
             ((0.24, 0.17, 0.13), "hair_black", True), ((0.70, 0.48, 0.26), (0.36, 0.22, 0.13), False)]
    for i, x in enumerate((-0.74, -0.38, -0.02, 0.34)):
        arch_door((x, y0 - 0.065, z), 0, r, 0.26, 0.74, frame=(LAPIS, TURQ)[i % 2], open_=True)
        box((0.27, 0.035, 0.4), (x, y0 - 0.12, z + 0.2), mat(PALMWOOD, rough=0.7), parent=r)
        box((0.29, 0.045, 0.035), (x, y0 - 0.12, z + 0.41), mat(PALMWOOD_DK), parent=r)
        cm, mn, bl = coats[i]
        horse_head((x, y0 - 0.14, z + 0.42), 0, r, cm, mn, bl)
    # hay under a reed lean-to on the +X end
    reed_roof(r, 0.58, 0.94, 0.3, 0.94, 0.78, rise=0.0, valance=None)
    for i, (x, y, zz) in enumerate(((0.76, 0.45, z), (0.76, 0.77, z), (0.76, 0.61, z + 0.16))):
        greek.hay_bale((x, y, zz), pi / 2, r, (0.3, 0.2, 0.16))
    # paddock
    rail = mat(PALMWOOD)
    fence([(-0.92, 0.22), (-0.92, -0.92), (-0.1, -0.92)], r, 0.42, rail, PALMWOOD_DK)
    fence([(0.3, -0.92), (0.92, -0.92), (0.92, 0.22)], r, 0.42, rail, PALMWOOD_DK)
    for sx in (-0.1, 0.3):
        box((0.08, 0.08, 0.62), (sx, -0.92, 0.31), brick(MUD, 16.0), parent=r)
        box((0.11, 0.11, 0.04), (sx, -0.92, 0.64), glaze(), parent=r)
    for (x, y, rz) in ((-0.92, -0.45, pi / 2), (0.6, -0.92, 0.0)):
        e = face((x, y, 0.42), rz, r)
        cloth(0.3, 0.2, mat(player, rough=0.7), (0, -0.02, 0.0), 0, e, hang=True, wave=0.01, nx=4)
        cloth(0.3, 0.2, mat(player, rough=0.7), (0, 0.02, 0.0), pi, e, hang=True, wave=0.01, nx=4)
        box((0.3, 0.05, 0.02), (0, 0, 0.0), gold(), parent=e)
    greek._horse((0.36, -0.48, z), -1.1, r, coat=(0.62, 0.38, 0.2), cloth=[gold(), mat(player, rough=0.7), gold()])
    greek.trough((-0.45, -0.6, z), 0, r, 0.55)
    greek.hay_bale((-0.62, -0.2, z), 0.4, r)
    blob(0.12, (-0.28, -0.22, z + 0.02), "straw", (1.4, 1, 0.5), r, 5, 0.3, 1)
    standard((-0.92, 0.28, z), 1.5, player, r, 0.24, 0.44, seed=7)
    return r


# ------------------------------------------------------------- storehouse


def storehouse(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A buttressed mudbrick storeroom, a reed lean-to over palm logs
    and stone, sacks of grain, a chest of gold and rows of great jars."""
    r = empty("storehouse", loc, rot_z, parent)
    box((1.92, 1.92, 0.03), (0, 0, 0.015), noisy("sand", 0.1, 3.0, 1.0), parent=r, bevel=0.01)
    z = 0.03
    x0, x1, y0, y1, h = -0.94, 0.2, 0.24, 0.94, 1.08
    zt = mud_block(r, x0, x1, y0, y1, h, z, band=0.8, pitch=0.28)
    arch_door((-0.4, y0 - 0.065, z), 0, r, 0.36, 0.78)
    box((0.56, 0.2, 0.04), (-0.4, y0 - 0.16, z + 0.02), mat(PALMWOOD), parent=r)
    slit((x1 + 0.065, 0.6, z + 0.6), pi / 2, r)
    # a hoist beam over the door with a sack on the rope
    rod((-0.4, y0, zt - 0.1), (-0.4, y0 - 0.36, zt - 0.1), 0.026, PALMWOOD_DK, r)
    rod((-0.4, y0 - 0.33, zt - 0.12), (-0.4, y0 - 0.33, z + 0.62), 0.005, "wool", r, 4)
    sack((-0.4, y0 - 0.33, z + 0.42), r, 0.09, 11)
    # lean-to on posts down the +X side, logs and stone under it
    reed_roof(r, 0.32, 0.94, -0.16, 0.92, 0.8, rise=0.0, valance=None)
    cloth(1.08, 0.14, mat(player, rough=0.7), (0.94, 0.38, 0.78), pi / 2, r, hang=True, wave=0.01, nx=8)
    greek.timber_stack((0.62, 0.42, z), 0, r, (4, 3, 2), 0.86, 0.06, tarp=None)
    greek.stone_pile((0.6, -0.48, z), r, seed=3)
    # the yard
    sack_pile((-0.62, -0.1, z), r, seed=2)
    sack_pile((-0.7, -0.52, z), r, seed=7, n=(3, 2), s=0.11, colours=("linen", "wool"))
    greek.gold_pile((-0.05, -0.6, z), r, seed=4)
    for i, (x, y) in enumerate(((-0.12, -0.06), (0.08, -0.12), (-0.02, 0.08), (0.2, 0.02))):
        jar(r, x, y, z, 0.34, band=(LAPIS, TURQ)[i % 2])
    crate((-0.32, -0.84, z), 0.2, r, 0.22)
    standard((-0.9, -0.88, z), 1.45, player, r, 0.24, 0.44, seed=5)
    return r


# ------------------------------------------------------------------- farm


def farm(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, ripe=True):
    """2 x 2, low and flat. Barley in rows on tilled earth between a canal
    along the front and the right, a shaduf dipping a bucket into it with
    the owner's pennant on its post, two young date palms and a reed hut on
    the far corner, sheaves on the stubble."""
    r = empty("farm", loc, rot_z, parent)
    box((1.94, 1.94, 0.04), (0, 0, 0.01), noisy("earth", 0.14, 4.0, 1.0), parent=r, bevel=0.015)
    # the canal: banks of mud and a water strip, front (-Y) and right (+X)
    water = mat("water", rough=0.15)
    bank = noisy(MUD_DK, 0.12, 6.0)
    box((1.9, 0.12, 0.02), (0.0, -0.84, 0.035), water, parent=r)
    box((0.12, 1.66, 0.02), (0.84, 0.05, 0.035), water, parent=r)
    box((1.9, 0.05, 0.07), (0.0, -0.93, 0.045), bank, parent=r, bevel=0.015)
    box((1.6, 0.05, 0.07), (-0.13, -0.755, 0.045), bank, parent=r, bevel=0.015)
    box((0.05, 1.74, 0.07), (0.93, 0.07, 0.045), bank, parent=r, bevel=0.015)
    box((0.05, 1.62, 0.07), (0.755, 0.13, 0.045), bank, parent=r, bevel=0.015)
    rnd = random.Random(7)
    dk = mat(tint("earth", 0.72))
    rows = [-0.66 + i * 0.12 for i in range(13)]
    wheat = [mat(tint("wheat", f), rough=0.9) for f in (0.92, 1.0, 1.06)]
    pts = [[], [], []]
    for j, y in enumerate(rows):
        box((1.4, 0.05, 0.03), (-0.02, y, 0.04), dk if j % 2 else mat(tint("earth", 0.85)), parent=r)
        if y < -0.4:
            for i in range(12):
                x = -0.68 + i * 0.12 + rnd.uniform(-0.02, 0.02)
                cyl(0.012, 0.06, (x, y, 0.08), "straw", parent=r, segs=4)
            continue
        for i in range(18):
            x = -0.72 + i * 0.08 + rnd.uniform(-0.015, 0.015)
            if x < -0.3 and y > 0.32:
                continue
            hh = rnd.uniform(0.18, 0.24) if ripe else rnd.uniform(0.08, 0.12)
            pts[rnd.randrange(3)].append(((x, y + rnd.uniform(-0.012, 0.012), 0.05 + hh * 0.6), (0.8, 0.75, hh / 0.09)))
    for k in range(3):
        greek.clumps(pts[k], 0.045, wheat[k] if ripe else mat(("leaf", "grass", "leaf_olive")[k]), r, seed=k)
    # the reed hut and the palms in the far corner
    hx, hy = -0.68, 0.68
    for i in range(6):
        y = hy - 0.2 + i * 0.08
        arc_rod((hx, y, 0.03), 0.2, 0.0, pi, r, 0.03, mat(REED_DK if i % 5 == 0 else REED), "xz", 8)
    vs, fs = [], []
    n = 10
    for i in range(n + 1):
        a = pi * i / n
        for y in (hy - 0.2, hy + 0.2):
            vs.append((hx + 0.19 * cos(a), y, 0.03 + 0.19 * sin(a)))
    for i in range(n):
        fs.append((2 * i, 2 * i + 1, 2 * i + 3, 2 * i + 2))
    mesh(vs, fs, noisy(REED, 0.14, 12.0), parent=r, smooth=True, name="hut")
    box((0.12, 0.012, 0.16), (hx, hy - 0.23, 0.11), mat((0.1, 0.07, 0.05)), parent=r)
    palm(r, -0.84, 0.28, 0.03, 1.2, seed=11, lean=(0.03, -0.03), s=0.62)
    # the shaduf on the canal bank
    qx, qy = 0.62, -0.62
    rod((qx, qy, 0.03), (qx, qy, 0.68), 0.025, PALMWOOD_DK, r)
    rod((qx - 0.38, qy - 0.04, 0.5), (qx + 0.34, qy + 0.28, 0.8), 0.016, PALMWOOD, r)
    blob(0.07, (qx - 0.38, qy - 0.04, 0.47), "rock", (1, 1, 0.8), r, 3, 0.2, 1)
    rod((qx + 0.34, qy + 0.28, 0.8), (qx + 0.34, qy + 0.28, 0.2), 0.004, "wool", r, 4)
    lathe([(0.04, 0), (0.06, 0.08), (0.055, 0.08)], plaster(MUD_DK), (qx + 0.34, qy + 0.28, 0.12), parent=r, segs=8)
    flag((qx, qy, 0.68), 0.12, player, r, w=0.34, fh=0.18, pennant=True, pole_r=0.02)
    # sheaves on the stubble, a basket, a sickle
    stalk = mat(tint("grass_dry", 0.9))
    for i, (x, y) in enumerate(((-0.5, -0.56), (-0.2, -0.6), (0.15, -0.54))):
        e = empty("sheaf", (x, y, 0.04), i * 0.7, r)
        cyl(0.06, 0.24, (0, 0, 0.12), stalk, parent=e, segs=8, r_top=0.045)
        blob(0.07, (0, 0, 0.27), wheat[i % 3], (1, 1, 1.3), e, i, 0.3, 1)
        cyl(0.055, 0.03, (0, 0, 0.15), mat(player, rough=0.75), parent=e, segs=8)
    basket((0.42, -0.5, 0.04), r, 0.1, 1, 3)
    return r


# ------------------------------------------------------------------- dock


def _quay(r, x0, x1, y0, y1, z, along="x", seed=0):
    """Palm-log decking on piles, top at z, open air beneath."""
    rnd = random.Random(seed)
    th = 0.05
    pl = [mat(PALMWOOD), mat(tint(PALMWOOD, 0.88)), mat(tint(PALMWOOD, 1.12)), mat((0.56, 0.42, 0.28))]
    if along == "x":
        n = max(1, round((y1 - y0) / 0.11))
        for i in range(n):
            yy = y0 + (i + 0.5) * (y1 - y0) / n
            box((x1 - x0, (y1 - y0) / n - 0.012, th), ((x0 + x1) / 2, yy, z - th / 2), pl[rnd.randrange(4)], parent=r)
    else:
        n = max(1, round((x1 - x0) / 0.11))
        for i in range(n):
            xx = x0 + (i + 0.5) * (x1 - x0) / n
            box(((x1 - x0) / n - 0.012, y1 - y0, th), (xx, (y0 + y1) / 2, z - th / 2), pl[rnd.randrange(4)], parent=r)
    box((x1 - x0 - 0.01, y1 - y0 - 0.01, 0.02), ((x0 + x1) / 2, (y0 + y1) / 2, z - th - 0.01), mat((0.2, 0.13, 0.08)),
        parent=r)
    for (a, b) in (((x0, y0), (x1, y0)), ((x1, y0), (x1, y1)), ((x0, y1), (x1, y1)), ((x0, y0), (x0, y1))):
        rod((a[0], a[1], z - th - 0.03), (b[0], b[1], z - th - 0.03), 0.04, PALMWOOD_DK, r, 8)
    nxp, nyp = max(1, round((x1 - x0) / 0.5)), max(1, round((y1 - y0) / 0.5))
    for i in range(nxp + 1):
        for k in range(nyp + 1):
            px, py = x0 + (x1 - x0) * i / nxp, y0 + (y1 - y0) * k / nyp
            cyl(0.05, z - 0.04, (px, py, (z - 0.04) / 2), PALMWOOD_DK, parent=r, segs=8)
            cyl(0.054, 0.07, (px, py, 0.035), mat((0.08, 0.07, 0.06), rough=0.5), parent=r, segs=8)


def reed_boat(loc, rz=0.0, parent=None, L=1.0, player=BLUE):
    """A boat of bound reed bundles, its ends curled up, along local X, the
    waterline at loc.z; a pole and a player-colour cloth on a little mast."""
    e = empty("reed_boat", loc, rz, parent)
    rm = noisy(REED, 0.12, 14.0)
    n = 10
    pts = []
    for i in range(n + 1):
        t = i / n
        x = (t - 0.5) * L
        zz = 0.05 + 0.25 * abs(2 * t - 1) ** 3
        rr = 0.1 * (1 - 0.65 * abs(2 * t - 1) ** 2) + 0.02
        pts.append((x, zz, rr))
    for sy in (-0.07, 0.0, 0.07):
        for (x0_, z0_, r0_), (x1_, z1_, r1_) in zip(pts, pts[1:]):
            rod((x0_, sy * (r0_ / 0.12), z0_), (x1_, sy * (r1_ / 0.12), z1_), r0_ * 0.6, rm, e, 8, r1_ * 0.6)
    for k in range(5):
        x = (k - 2) * L * 0.16
        cyl(0.075, 0.03, (x, 0, 0.07), mat(BITUMEN), rot=(0, pi / 2, 0), parent=e, segs=8)
    rod((0.0, 0, 0.1), (0.0, 0, 0.75), 0.014, PALMWOOD_DK, e)
    cloth(0.3, 0.32, mat(player, rough=0.7), (0.0, 0.0, 0.74), pi / 2, e, hang=False, wave=0.03, taper=0.0)
    return e


def quffa(loc, parent=None, s=1.0):
    """A round coracle of bitumen-coated basketwork."""
    lathe([(0.1, -0.03), (0.28, 0.02), (0.33, 0.12), (0.34, 0.16), (0.31, 0.16), (0.3, 0.12)], mat(BITUMEN, rough=0.4),
          loc, parent=parent, s=s, segs=18)
    cyl(0.29 * s, 0.01, (loc[0], loc[1], loc[2] + 0.06 * s), noisy(REED_DK, 0.1, 10.0), parent=parent, segs=16)


def dock(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """3 x 3, all over water (none modelled, open air under the decks): a
    palm-log landing stage and two piers round a slip roofed by an arched
    reed boathouse (mudhif) sheltering a reed boat; a coracle, nets, jars,
    sacks, a crane, the owner's standards at the pier heads."""
    r = empty("dock", loc, rot_z, parent)
    zd = 0.26
    E = 1.44
    _quay(r, -E, E, 0.5, E, zd, "x", 1)
    _quay(r, -E, -0.62, -E, 0.5, zd, "y", 2)
    _quay(r, 0.58, E, -E, 0.5, zd, "y", 3)
    # the reed boathouse: bundle arches across the slip, matting over them
    ax0, ax1, ay0, ay1 = -0.66, 0.62, -0.62, 1.16
    cxm, span, H = (ax0 + ax1) / 2, (ax1 - ax0) / 2, 1.12
    bund = noisy(REED_DK, 0.1, 14.0)
    na = 7
    for i in range(na + 1):
        y = ay0 + (ay1 - ay0) * i / na
        prev = None
        for k in range(13):
            a = pi * k / 12
            p = (cxm + span * cos(a), y, zd + H * sin(a) ** 0.8)
            if prev:
                rod(prev, p, 0.055 if i in (0, na) else 0.04, bund, r, 7)
            prev = p
    vs, fs = [], []
    n = 16
    for i in range(n + 1):
        a = pi * i / n
        for y in (ay0 + 0.08, ay1 - 0.02):
            vs.append((cxm + (span - 0.02) * cos(a), y, zd + (H - 0.02) * sin(a) ** 0.8))
    for i in range(n):
        fs.append((2 * i, 2 * i + 1, 2 * i + 3, 2 * i + 2))
    mesh(vs, fs, noisy(REED, 0.12, 9.0), parent=r, smooth=False, name="mat")
    # a lattice in the front arch, the owner's colour over the opening
    for k in (-2, -1, 0, 1, 2):
        x = cxm + k * span * 0.3
        hgt = H * (1 - (k * 0.3) ** 2) ** 0.4
        if abs(k) > 0:
            rod((x, ay0, zd + hgt * 0.62), (x, ay0, zd + hgt * 0.98), 0.012, REED_DK, r, 5)
    cloth(0.8, 0.18, mat(player, rough=0.7), (cxm, ay0 - 0.06, zd + H * 0.6), 0, r, hang=True, wave=0.012, nx=8)
    standard((cxm, ay1 + 0.12, zd), 1.55, player, r, 0.28, 0.44, seed=6)
    reed_boat((-0.02, 0.1, 0.0), pi / 2, r, 1.3, player)
    quffa((0.05, -1.12, 0.0), r, 0.9)
    rod((0.25, -1.1, 0.05), (0.35, -1.25, 0.55), 0.01, PALMWOOD, r)
    # landing stage: jars, sacks, crates, a crane
    z = zd
    for i in range(4):
        jar(r, -1.25 + i * 0.14, 0.75 + (i % 2) * 0.07, z, 0.34, band=(LAPIS, TURQ)[i % 2])
    crate((-1.15, 1.15, z), 0.2, r, 0.26)
    crate((-0.86, 1.2, z), -0.1, r, 0.24, fruit=5, seed=2)
    sack_pile((1.0, 1.12, z), r, seed=3, n=(3, 2), s=0.1)
    jar(r, 1.25, 0.72, z, 0.36)
    kx, ky = 1.2, 0.2
    box((0.18, 0.18, 0.08), (kx, ky, z + 0.04), mat(PALMWOOD_DK), parent=r)
    rod((kx, ky, z), (kx, ky, z + 1.2), 0.035, PALMWOOD_DK, r)
    rod((kx, ky, z + 1.15), (kx - 0.5, ky - 0.4, z + 1.3), 0.025, PALMWOOD, r)
    rod((kx, ky, z + 0.7), (kx - 0.28, ky - 0.22, z + 1.2), 0.018, PALMWOOD, r)
    rod((kx - 0.5, ky - 0.4, z + 1.3), (kx - 0.5, ky - 0.4, z + 0.62), 0.004, "wool", r, 4)
    sack((kx - 0.5, ky - 0.4, z + 0.45), r, 0.1, 6)
    greek.net_rack((-1.03, -0.45, z), pi / 2, r, 0.75, 0.82)
    for i, (x, y) in enumerate(((-1.25, 0.15), (-0.85, 0.2))):
        basket((x, y, z), r, 0.1, 0, i)
    for (x, y) in ((-0.68, -1.36), (0.64, -1.36), (-0.68, 0.0), (0.64, 0.0)):
        cyl(0.05, 0.2, (x, y, z + 0.1), PALMWOOD_DK, parent=r, segs=8)
        sphere(0.05, (x, y, z + 0.2), PALMWOOD_DK, parent=r, segs=8)
    for (x, y) in ((-1.36, -1.36), (1.36, -1.36)):
        standard((x, y, z), 1.25, player, r, 0.22, 0.38, seed=int(x * 3))
    jar(r, 1.2, -0.95, z, 0.3)
    greek.brazier((0.95, -0.3, z), r, 0.7)
    return r


# ----------------------------------------------------------------- wonder


def _garden(r, pts, seed=0, flowers=True):
    """Hanging-garden greenery: bushes along a terrace edge, some in flower."""
    rnd = random.Random(seed)
    greek.clumps([((x, y, z + 0.08), (1.0, 1.0, 0.8)) for x, y, z in pts], 0.12,
                 mat((0.22, 0.5, 0.18), rough=0.9), r, seed=seed)
    if flowers:
        g = Geo()
        for x, y, z in pts:
            if rnd.random() < 0.6:
                g.box((0.05, 0.05, 0.05), (x + rnd.uniform(-0.06, 0.06), y + rnd.uniform(-0.06, 0.06), z + 0.17))
        g.emit(mat((0.92, 0.3, 0.42), emit=0.1), r, "flowers")


def wonder(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """5 x 5. The great ziggurat: three buttressed stages, the lowest
    bitumen-dark below, climbed by a grand central stair and two side stairs
    meeting on a landing; a glazed-blue temple with gilded horns on
    the summit; gardens spilling from the terraces, palms, lamassu at the
    stair's foot, the owner's standards up the way and a flag on top."""
    r = empty("wonder", loc, rot_z, parent)
    box((4.92, 4.92, 0.05), (0, 0, 0.025), brick(MUD_LT, 3.0), parent=r, bevel=0.012)
    z = 0.05
    t1 = mud_block(r, -2.0, 2.0, -1.0, 2.3, 1.1, z, c=MUD_DK, pitch=0.34, band=0.82, dado=0.3, mh=0.15,
                   rib_w=0.1, rib_d=0.05)
    t2 = mud_block(r, -1.42, 1.42, -0.3, 2.0, 0.92, t1, pitch=0.32, band=0.8, dado=0.0, mh=0.14, rib_w=0.09)
    t3 = mud_block(r, -0.88, 0.88, 0.32, 1.72, 0.72, t2, c=tint(MUD, 1.05), pitch=0.3, band=0.78, dado=0.0, mh=0.13)
    _shrine(r, -0.56, 0.56, 0.62, 1.42, t3, 0.78, player, door_w=0.32)
    # the stairs: a landing before the first stage, the grand central stair
    # up to it, two side flights along the front meeting on it, and the
    # stair on up through the stages
    mud_block(r, -0.58, 0.58, -1.5, -1.0, t1 - z, z, c=MUD_DK, pitch=0.3, band=0.82, dado=0.3, rosettes=True,
              merlons=False, roof=None, rib_w=0.1, rib_d=0.05)
    box((1.1, 0.46, 0.02), (0, -1.25, t1 + 0.01), plaster(), parent=r)
    stair(r, 0.0, -2.36, -1.5 - 0.004, z, t1, 0.64, 12)
    stair(r, 0.0, -1.0 + 0.004, -0.3 + 0.006, t1, t2, 0.5, 9)
    stair(r, 0.0, -0.3 + 0.004, 0.32 + 0.006, t2, t3, 0.42, 7)
    for sx in (-1, 1):
        e = empty("side", (sx * 1.92, -1.3, 0), sx * pi / 2, r)
        stair(e, 0.0, 0.0, 1.32, z, t1, 0.36, 10)
    # lamassu and braziers at the foot, standards up the stair
    for sx in (-1, 1):
        lamassu((sx * 0.62, -1.95, z), 0, r, 0.8)
        greek.brazier((sx * 0.38, -2.38, z), r, 0.8)
        standard((sx * 0.48, -1.42, t1), 1.5, player, r, 0.26, 0.5, seed=sx + 4)
        standard((sx * 0.4, -0.38, t2), 1.3, player, r, 0.24, 0.44, seed=sx + 8)
        standard((sx * 2.3, -2.3, z), 2.0, player, r, 0.3, 0.6, seed=sx + 12)
        # gardens on the terraces and palms on the corners
        _garden(r, [(sx * (0.62 + i * 0.22), -0.86 + (i % 2) * 0.1, t1) for i in range(5)], seed=sx + 20)
        _garden(r, [(sx * 1.85, -0.6 + i * 0.25, t1) for i in range(10)], seed=sx + 22)
        _garden(r, [(sx * (0.62 + i * 0.2), -0.18, t2) for i in range(4)], seed=sx + 24)
        _garden(r, [(sx * 1.3, 0.0 + i * 0.25, t2) for i in range(7)], seed=sx + 26)
        palm(r, sx * 1.75, -0.78, t1, 1.5, seed=sx + 30, lean=(sx * 0.06, -0.04), s=0.9)
        palm(r, sx * 1.22, -0.12, t2, 1.4, seed=sx + 32, lean=(sx * 0.05, -0.03), s=0.8)
        palm(r, sx * 2.25, 0.2 + (sx + 1) * 0.6, z, 1.7, seed=sx + 34, lean=(0.03, -0.04))
        _garden(r, [(sx * (1.0 + i * 0.25), -2.3, z) for i in range(5)], seed=sx + 40)
    flag((0.0, 1.02, t3 + 0.78 + 0.06), 1.0, player, r, w=1.0, fh=0.56, pole_r=0.035, seed=7)
    return r


# ------------------------------------------------------- walls and a gate
#
# As greek.py's (docs/look/buildings.md, "Walls and gates"): the post is the
# idle frame, arm k the arm toward WALL_DIRECTIONS[k] alone; a gate stands
# across line `line` (0-3), shut or open.

wall_dir = greek.wall_dir
_arm_outline = greek._arm_outline
_along = greek._along
plan_prism = greek.plan_prism
WALL_DIRECTIONS = greek.WALL_DIRECTIONS

MUD_PIER, MUD_W, MUD_H, MUD_POST_H = 0.21, 0.32, 1.04, 1.36


def stone_wall(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, arm=None, joins=0, post=True):
    """1 x 1. Mudbrick curtain wall: a square buttress tower with a bitumen
    foot, a lapis band and an owner's-colour band under stepped merlons;
    arms of wall with buttresses, a glazed band and stepped merlons.
    `arm` = k draws arm k alone (no post); `joins` a post with its arms."""
    if arm is not None:
        joins, post = 1 << arm, False
    r = empty("stone_wall", loc, rot_z, parent)
    P, W, H = MUD_PIER, MUD_W, MUD_H
    wm = brick(MUD, 12.0)
    if post:
        PH = MUD_POST_H
        box((P * 2 + 0.06, P * 2 + 0.06, 0.16), (0, 0, 0.08), noisy(BITUMEN, 0.25, 6.0, 0.6), parent=r)
        box((P * 2, P * 2, PH), (0, 0, PH / 2), wm, parent=r)
        box((P * 2 + 0.03, P * 2 + 0.03, 0.09), (0, 0, PH - 0.36), glaze(), parent=r)
        box((P * 2 + 0.025, P * 2 + 0.025, 0.11), (0, 0, PH - 0.22), mat(player, rough=0.7), parent=r)
        box((P * 2 + 0.035, P * 2 + 0.035, 0.02), (0, 0, PH - 0.155), gold(), parent=r)
        box((P * 2 + 0.035, P * 2 + 0.035, 0.02), (0, 0, PH - 0.285), gold(), parent=r)
        box((P * 2 - 0.08, P * 2 - 0.08, 0.02), (0, 0, PH + 0.01), plaster(), parent=r)
        crenels(r, -P, P, -P, P, PH, brick(tint(MUD, 1.04), 12.0), w=0.11, h=0.15, t=0.075, pitch=0.14)
        for (x, y, rz) in ((0, -P, 0), (P, 0, pi / 2)):
            slit((x * 1.0 + (0.0 if rz == 0 else 0.0), y, 0.62), rz, r, 0.06, 0.2)
    for k in range(8):
        if not joins & (1 << k):
            continue
        (ux, uy), ang, reach = wall_dir(k)
        dx, dy = WALL_DIRECTIONS[k]
        plan_prism(_arm_outline(k, P, W, 0.03, 0.003), 0.0, 0.13, noisy(BITUMEN, 0.25, 6.0, 0.6), r, "arm_dado")
        plan_prism(_arm_outline(k, P, W), 0.0, H, wm, r, "arm")
        plan_prism(_arm_outline(k, P, W, 0.022, 0.003), H - 0.24, H - 0.16, glaze(), r, "arm_band")
        plan_prism(_arm_outline(k, P, W, 0.01, 0.003), H, H + 0.025, plaster(), r, "arm_cap")
        s0 = P * (math.sqrt(2) if dx and dy else 1.0) + 0.02
        g = Geo()
        n = max(1, int((reach - s0) / 0.15 + 0.5))
        step = (reach - s0) / n
        for i in range(n):
            s = s0 + (i + 0.5) * step
            for sv in (-1, 1):
                x, y = _along(k, s, sv * (W / 2 - 0.04))
                g.merlon((x, y), (ux, uy), H + 0.025, 0.1, 0.14, 0.07)
        # a buttress rib half way along each face
        sm = (s0 + reach) / 2
        for sv in (-1, 1):
            x, y = _along(k, sm, sv * (W / 2 + 0.017))
            g.box((0.08, 0.034, H - 0.26), (x, y, (H - 0.26) / 2), ang)
        g.emit(brick(tint(MUD, 1.04), 12.0), r, "arm_merlons")
    return r


def palisade_wall(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, arm=None, joins=0, post=True):
    """1 x 1. A palisade of palm trunks and bound reed bundles: a post of
    three trunks with owner's-colour lashings and a tuft of fronds; arms of
    sharpened palm stakes alternating with reed bundles on two rope rails.
    `arm`, `joins`, `post` as for stone_wall."""
    if arm is not None:
        joins, post = 1 << arm, False
    r = empty("palisade_wall", loc, rot_z, parent)
    bark = [mat((0.48, 0.38, 0.27), rough=0.95), mat((0.42, 0.32, 0.22), rough=0.95)]
    tip = mat((0.8, 0.66, 0.46), rough=0.9)
    reed = noisy(REED, 0.12, 14.0)
    rnd = random.Random(11)
    if post:
        blob(0.2, (0, 0, 0.0), "earth", (1.1, 1.1, 0.35), r, 3, 0.2, 1)
        for i, a in enumerate((0.5, 0.5 + 2.1, 0.5 + 4.2)):
            h = 1.28 - i * 0.06
            p = (0.075 * cos(a), 0.075 * sin(a))
            for j in range(4):
                cyl(0.085, h / 4, (p[0], p[1], h / 8 + j * h / 4), bark[(i + j) % 2], parent=r, segs=7)
            cyl(0.085, 0.24, (p[0], p[1], h + 0.12), tip, parent=r, segs=7, r_top=0.004)
        for zz in (0.42, 0.9):
            cyl(0.2, 0.08, (0, 0, zz), mat(player, rough=0.75), parent=r, segs=12)
            cyl(0.205, 0.015, (0, 0, zz - 0.045), "wool", parent=r, segs=12)
    for k in range(8):
        if not joins & (1 << k):
            continue
        _, ang, reach = wall_dir(k)
        dx, dy = WALL_DIRECTIONS[k]
        s0 = 0.17 if not (dx and dy) else 0.19
        n = max(2, int((reach - s0) / 0.11 + 0.5))
        step = (reach - s0) / n
        for i in range(n):
            s = s0 + (i + 0.5) * step
            p = _along(k, s, rnd.uniform(-0.012, 0.012))
            if i % 2 == 0:
                hh = rnd.uniform(0.98, 1.08)
                rr = 0.055 * rnd.uniform(0.94, 1.06)
                cyl(rr, hh, (p[0], p[1], hh / 2), bark[i % 4 // 2], parent=r, segs=7)
                cyl(rr, rr * 3.2, (p[0], p[1], hh + rr * 1.6), tip, parent=r, segs=7, r_top=0.004)
            else:
                hh = rnd.uniform(0.86, 0.94)
                cyl(0.052, hh, (p[0], p[1], hh / 2), reed, parent=r, segs=8)
                sphere(0.06, (p[0], p[1], hh), reed, parent=r, segs=8, scale=(0.9, 0.9, 0.6))
                for zz in (0.25, 0.6):
                    cyl(0.056, 0.03, (p[0], p[1], zz), mat(REED_DK), parent=r, segs=8)
        for zz in (0.34, 0.76):
            for side in (1, -1):
                a, b = _along(k, s0 - 0.06, side * 0.062), _along(k, reach - 0.002, side * 0.062)
                rod((a[0], a[1], zz), (b[0], b[1], zz), 0.016, "wool", r, 5)
        blob(0.1, (*_along(k, (s0 + reach) / 2, 0.0), -0.02), "earth", (reach * 3.2, 1.6, 0.6), r, k, 0.15,
             1).rotation_euler = (0, 0, ang)
    return r


def _arch_frame(ow, zs, zt, n=10):
    """The face of a wall over an arched opening: a rectangle from -ow..ow,
    zs..zt with a semicircle (radius ow, centre (0, zs)) cut from below."""
    pts = [(-ow, zs), (-ow, zt), (ow, zt), (ow, zs)]
    for i in range(1, n):
        a = pi * i / n
        pts.append((ow * cos(a), zs + ow * sin(a)))
    return pts


def gate(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, line=0, open=False):
    """1 x 1. An Ishtar gate: two lapis-glazed towers with yellow bulls and
    white rosettes, stepped white merlons, the owner's banners, and between
    them an arched passage with cedar doors studded in bronze. `line` (0-3)
    is the direction its walls run; `open` swings the doors back."""
    r = empty("gate", loc, rot_z, parent)
    (ux, uy), ang, reach = wall_dir(line)
    e = empty("gate_line", (0, 0, 0), ang, r)
    T, D, TH = 0.15, 0.22, 1.7
    tx = reach - T
    gm = glaze()
    for sx in (-1, 1):
        x = sx * tx
        box((T * 2 + 0.05, D * 2 + 0.05, 0.14), (x, 0, 0.07), noisy(BITUMEN, 0.25, 6.0, 0.6), parent=e)
        box((T * 2, D * 2, TH), (x, 0, TH / 2), gm, parent=e)
        box((T * 2 + 0.03, D * 2 + 0.03, 0.08), (x, 0, TH - 0.2), mat(GLAZE_Y, rough=0.35), parent=e)
        box((T * 2 - 0.06, D * 2 - 0.06, 0.02), (x, 0, TH + 0.01), plaster(), parent=e)
        crenels(e, x - T, x + T, -D, D, TH, mat(GLAZE_W, rough=0.35), w=0.1, h=0.15, t=0.07, pitch=0.15)
        for sy in (-1, 1):
            f = face((x, sy * D, 0), 0 if sy < 0 else pi, e)
            _bull(f, 0.0, 0.5, 0.1, mat(GLAZE_Y, rough=0.35))
            _bull(f, 0.0, 0.95, 0.1, mat(GLAZE_W, rough=0.35))
            for zz in (0.36, 0.82, TH - 0.2):
                if zz < TH - 0.3:
                    g = Geo()
                    for kx in (-1, 1):
                        g.box((0.05, 0.008, 0.05), (kx * 0.09, -0.004, zz), pi / 4 * 0)
                    g.emit(mat(GLAZE_W, rough=0.35), f, "rosette")
            cloth(0.16, 0.42, mat(player, rough=0.7), (0, -0.075 if False else -0.03, TH - 0.28), 0, f, hang=True,
                  wave=0.008, nx=4)
            box((0.22, 0.04, 0.035), (0, -0.02, TH - 0.27), mat(PALMWOOD_DK), parent=f)
    # the arched passage between the towers
    ow = tx - T
    zs, zt = 0.82, 1.32
    g = Geo()
    g.prism(_arch_frame(ow + 0.002, zs, zt), (0, 0, 0), (1, 0), (0, 1), -D + 0.03, D - 0.03)
    g.emit(gm, e, "arch")
    for sy in (-1, 1):
        g = Geo()   # a yellow archivolt ring on each face
        ring = [(ow * cos(pi * i / 10), zs + ow * sin(pi * i / 10)) for i in range(11)]
        ring += [((ow + 0.05) * cos(pi * i / 10), zs + (ow + 0.05) * sin(pi * i / 10)) for i in range(10, -1, -1)]
        for i in range(10):
            g.prism([ring[i], ring[i + 1], ring[20 - i], ring[21 - i - 1]], (0, 0, 0), (1, 0), (0, 1),
                    sy * (D - 0.03), sy * (D - 0.012))
        g.emit(mat(GLAZE_Y, rough=0.35), e, "archivolt")
    box((ow * 2 + 0.01, D * 2 - 0.04, 0.05), (0, 0, zt + 0.025), mat(GLAZE_Y, rough=0.35), parent=e)
    crenels(e, -ow, ow, -D + 0.02, D - 0.02, zt + 0.05, mat(GLAZE_W, rough=0.35), w=0.09, h=0.13, t=0.065,
            pitch=0.14, sides="sn")
    box((ow * 2 - 0.004, D * 2 - 0.06, 0.02), (0, 0, zt + 0.06), plaster(), parent=e)
    box((ow * 2, D * 2, 0.02), (0, 0, 0.01), brick(MUD_LT, 9.0), parent=e)
    # the doors, hinged at the towers, their tops following the arch
    dm = mat((0.56, 0.36, 0.2), rough=0.7)
    for sx in (-1, 1):
        f = empty("leaf", (sx * ow, 0.0, 0), 0, e)
        f.rotation_euler = (0, 0, (sx * 1.45) if open else 0.0)
        lw = ow - 0.008
        prof = [(0.0, 0.02), (lw, 0.02)]
        for i in range(7):
            xx = lw - lw * i / 6           # from the middle back to the hinge
            ax = ow - xx                   # distance from the arch's centre line
            prof.append((xx, zs + sqrt(max(0.0, ow * ow - ax * ax)) - 0.012))
        pts = [(-sx * a_, z_) for a_, z_ in prof]
        if sx < 0:
            pts = pts[::-1]
        gg = Geo()
        gg.prism([(a_, z_) for a_, z_ in pts], (0, 0, 0), (1, 0), (0, 1), -0.025, 0.025)
        gg.emit(dm, f, "door")
        gs = Geo()
        for zz in (0.2, 0.48, 0.76):
            gs.box((lw, 0.065, 0.035), (-sx * lw / 2, 0, zz))
        gs.emit(mat(PALMWOOD_DK), f, "door_bands")
        gb = Geo()
        for zz in (0.2, 0.48, 0.76):
            for k in range(3):
                for sy in (-1, 1):
                    gb.box((0.022, 0.012, 0.022), (-sx * (0.035 + k * (lw - 0.07) / 2), sy * 0.038, zz))
        gb.emit(greek.bronze(), f, "studs")
    return r

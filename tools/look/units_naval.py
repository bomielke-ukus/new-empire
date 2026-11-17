"""The new look's ships, the fishing spot and the relic (docs/look/units.md):
everything on the water, exported as turning animated colour sets by
unitexport.py (the ships) or as one-frame sets by export.py (fish, relic).

A ship is a `Ship`: a hull lofted from its sections (beam, sheer, keel),
its rig (masts, yards, sails in the owner's colour), oars, a crew of small
figures and its deck load, posed per frame by `state()` (the bob, the oar
stroke, the sail's fill, the death's break and sinking). It sits IN the
water: the exporter's ground for a figure with `water = True` is a holdout
at the waterline (unitexport.water_plane), so nothing under it is drawn;
the ship draws its own soft water shadow (a dark translucent sheet tagged
`look_ground`, split off as a shadow is) and its foam (white translucent
sheets with a per-vertex alpha, part of the sprite): a lapping line and
slow ripple rings at idle, a bow wave, a V wake, a stern wash and oar
splashes when moving, a churn and flotsam when it sinks.

    fishing_boat   the smallest: one triangular sail, two fishermen, a net
                   (its `attack` is the cast and haul the game loops while
                   it fishes)
    trade_boat     round-bellied and high-sided, no oars, one big square
                   sail, amphorae and bales heaped fore and aft
    transport      broad and deep, two masts with two sails, a crowd of
                   passengers on a wide deck, a few sweeps
    archer_ship    long, low and light, one sail, a bank of oars, three
                   archers on deck (looses on the impact frame)
    war_galley     the longest: a bronze ram, eyes, shields along the rail,
                   two dozen oars, a tall striped sail, four archers
    catapult_ship  heavy and broad: a torsion engine on a raised bow deck
                   (throws on the impact frame), a small sail aft

    /usr/bin/python3.13 tools/look/units_naval.py war_galley OUT.png [walk,death] [S,W]

previews some animations through the exporter's own scene (no assets
written), over the water, blue and red rows alternating.
"""
import math
import os
import random
import sys

import bpy  # noqa: F401  (before bmesh)
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import anim  # noqa: E402
import lib  # noqa: E402
import units  # noqa: E402
from figures import bronze, gold  # noqa: E402
from figures import B  # noqa: E402
from lib import mat  # noqa: E402
from units import R, S, T, pm  # noqa: E402

V = Vector
FOAM = (0.93, 0.97, 1.0)          # the ground's foam (docs/look/ground.md), a little blue
FOAM_Z, SHADOW_Z = 0.008, 0.003   # the sheets just over the waterline
WALK_S = 0.8                      # the walk loop, seconds (8 x 100 ms)


# ------------------------------------------------------------ soft sheets

def _soft_material(name, colour, emit, ground=False):
    """Unlit translucent sheet: black base, `colour` emitted, alpha from
    the mesh's float attribute `fa` (so the beauty and the mask pass of
    export.py both read it). `ground`: a shadow, split off by the exporter."""
    m = bpy.data.materials.get(name)
    if m is not None:
        return m
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    b = nt.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (0, 0, 0, 1)
    b.inputs["Roughness"].default_value = 1.0
    b.inputs["Specular IOR Level"].default_value = 0.0
    b.inputs["Emission Color"].default_value = (*lib._lin(colour), 1.0)
    b.inputs["Emission Strength"].default_value = emit
    at = nt.nodes.new("ShaderNodeAttribute")
    at.attribute_name = "fa"
    nt.links.new(at.outputs["Fac"], b.inputs["Alpha"])
    if ground:
        m["look_ground"] = True
    return m


def foam_mat():
    return _soft_material("naval_foam", FOAM, 1.0)


def shadow_mat():
    return _soft_material("naval_water_shadow", (0, 0, 0), 0.0, ground=True)


class Sheet:
    """A flat translucent mesh on the water: vertices with an alpha each."""

    def __init__(self):
        self.v, self.a, self.f = [], [], []

    def grid(self, rows, closed=False):
        """`rows`: lists of ((x, y), alpha), all the same length, joined
        row to row (and the last column to the first if `closed`)."""
        base = len(self.v)
        n = len(rows[0])
        for r in rows:
            for (x, y), a in r:
                self.v.append((x, y))
                self.a.append(max(0.0, min(1.0, a)))
        cols = n if closed else n - 1
        for j in range(len(rows) - 1):
            for i in range(cols):
                i1 = (i + 1) % n
                k0, k1 = base + j * n, base + (j + 1) * n
                self.f.append((k0 + i, k0 + i1, k1 + i1, k1 + i))

    def blob(self, c, rx, ry, a, n=10, rot=0.0, soft=0.6):
        """A soft elliptical spot: full `a` inside, fading over the outer `soft`."""
        rows = []
        for r, al in ((0.0, a), (1 - soft, a), (1 - soft * 0.5, a * 0.4), (1.0, 0.0)):
            row = []
            for k in range(n):
                t = 2 * math.pi * k / n
                x, y = rx * r * math.cos(t), ry * r * math.sin(t)
                cr, sr = math.cos(rot), math.sin(rot)
                row.append(((c[0] + x * cr - y * sr, c[1] + x * sr + y * cr), al))
            rows.append(row)
        self.grid(rows, closed=True)

    def obj(self, name, m, z, parent):
        if not self.f:
            return None
        me = bpy.data.meshes.new(name)
        me.from_pydata([(x, y, z) for x, y in self.v], [], self.f)
        me.update()
        # a plain float attribute, not a colour one: export.py's mask pass
        # rewrites colour attributes (it takes them for player colour)
        at = me.attributes.new("fa", "FLOAT", "POINT")
        at.data.foreach_set("value", self.a)
        me.materials.append(m)
        ob = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(ob)
        ob.parent = parent
        return ob


def _waves(x, y, seed, period):
    """Smooth noise 0..1 along x (periodic over `period`) and y."""
    rnd = random.Random(seed)
    s = 0.0
    for j in (1, 2, 3, 5):
        p1, p2 = rnd.uniform(0, 6.3), rnd.uniform(0, 6.3)
        s += (1.0 / j ** 0.7) * math.sin(2 * math.pi * j * x / period + p1) * \
            math.cos(9.0 * y / j + p2)
    return 0.5 + 0.5 * s / 1.9


def offset_loop(loop, normals, d):
    return [(p[0] + n[0] * d, p[1] + n[1] * d) for p, n in zip(loop, normals)]


def loop_normals(loop):
    n = len(loop)
    out = []
    for i in range(n):
        a, b = loop[i - 1], loop[(i + 1) % n]
        tx, ty = b[0] - a[0], b[1] - a[1]
        ln = math.hypot(tx, ty) or 1.0
        out.append((ty / ln, -tx / ln))
    # outward: the loop runs counter-clockwise or clockwise; make it outward
    cx = sum(p[0] for p in loop) / n
    cy = sum(p[1] for p in loop) / n
    k = sum((p[0] - cx) * q[0] + (p[1] - cy) * q[1] for p, q in zip(loop, out))
    return out if k > 0 else [(-x, -y) for x, y in out]


def ring(sheet, loop, normals, d0, d1, alpha, steps=3):
    """A band round a loop from offset d0 to d1, `alpha(i)` per point at its
    middle, fading to 0 at both edges."""
    rows = []
    for k in range(steps + 1):
        u = k / steps
        f = math.sin(math.pi * u) ** 1.2
        d = d0 + (d1 - d0) * u
        pts = offset_loop(loop, normals, d)
        rows.append([(p, alpha(i) * f) for i, p in enumerate(pts)])
    sheet.grid(rows, closed=True)


def strip(sheet, centre, half, alpha, profile=(0.0, 0.8, 1.0, 0.8, 0.0)):
    """A ribbon along an open polyline `centre` [(x, y)], half widths
    `half[i]`, alphas `alpha[i]` at its middle and a soft profile across."""
    n = len(centre)
    rows = []
    m = len(profile)
    for k in range(m):
        s = -1 + 2 * k / (m - 1)
        row = []
        for i in range(n):
            a, b = centre[max(0, i - 1)], centre[min(n - 1, i + 1)]
            tx, ty = b[0] - a[0], b[1] - a[1]
            ln = math.hypot(tx, ty) or 1.0
            nx, ny = -ty / ln, tx / ln
            row.append(((centre[i][0] + nx * half[i] * s, centre[i][1] + ny * half[i] * s),
                        alpha[i] * profile[k]))
        rows.append(row)
    sheet.grid(rows)


# ------------------------------------------------------------ the crew

def person(b, M, skin, hair, cloth, hands=None, helmet=None, lean=0.0, k=0.36, bow=None):
    """A small crewman, feet at M's origin looking along its +X, about `k`
    tall (a big head and hands: he is seen at a few pixels). `hands`: the
    left and right hand in his frame (units of his height); `bow`: (grip,
    nock or None, aim) to draw a bow in his left hand; `helmet`: a crest
    material, or "cap"."""
    hands = hands or ((0.05, 0.22, 0.42), (0.05, -0.22, 0.42))
    with b.at(M @ S(k, k, k) @ R("Y", lean), True):
        for s in (1, -1):
            b.seg((0, s * 0.075, 0.0), (0, s * 0.08, 0.40), 0.065, 0.06, skin, n=6)
        b.loft([(0, 0, 0.30, .14, .16), (0, 0, 0.46, .13, .15), (0.01, 0, 0.62, .14, .18),
                (0, 0, 0.74, .10, .15)], cloth, n=8)
        b.ball((0.015, 0, 0.88), 0.135, skin, n=8)
        if helmet is None:
            b.ball((-0.02, 0, 0.915), 0.13, hair, n=8, scale=(1, 1, 0.8))
        elif helmet == "cap":
            b.ball((0.0, 0, 0.93), 0.142, mat("wool", rough=0.9), n=8, scale=(1, 1, 0.7))
        else:
            b.ball((0.0, 0, 0.915), 0.148, bronze(), n=8, scale=(1.02, 1, 0.85))
            b.plate([(-0.17, 0.0), (-0.12, 0.13), (0.05, 0.17), (0.17, 0.08), (0.16, 0.0)],
                    0.06, helmet, frame=T(0, 0, 1.0) @ R("X", 90))
        for side, h in zip((1, -1), hands):
            sh = V((0, side * 0.17, 0.70))
            h = V(h)
            mid = (sh + h) / 2 + V((-0.06, side * 0.05, -0.04))
            b.seg(sh, mid, 0.055, 0.05, cloth if side else skin, n=6)
            b.seg(mid, h, 0.05, 0.045, skin, n=6)
            b.ball(h, 0.062, skin, n=6)
        if bow is not None:
            grip, nock, aim = (None if v is None else V(v) for v in bow)
            aim.normalize()
            up = V((0, 0, 1)) - aim * aim.z
            up.normalize()
            c = 0.2 if nock is not None else 0.1
            pts = [grip + up * (t * 0.4) - aim * (c * t * t) for t in (-1, -0.5, 0, 0.5, 1)]
            b.chain(pts, 0.035, 0.035, "wood_dark", n=5)
            st = mat((0.95, 0.92, 0.82))
            if nock is not None:
                b.seg(pts[0], nock, 0.012, 0.012, st, n=4)
                b.seg(pts[-1], nock, 0.012, 0.012, st, n=4)
            else:
                b.seg(pts[0], pts[-1], 0.012, 0.012, st, n=4)


def _people(seed):
    rnd = random.Random(seed)
    skin = mat(rnd.choice([lib.PAL["skin_light"], lib.PAL["skin_tan"], lib.PAL["skin_tan"],
                           lib.PAL["skin_dark"]]), rough=0.75)
    hair = mat(rnd.choice(["hair_black", "hair_brown", "hair_black"]), rough=0.9)
    return skin, hair


# ------------------------------------------------------------ the ships

DEATH = [  # (t, dict): how the ship breaks and goes under, stern last (8 frames)
    (0.0, dict(z=-0.005, pitch=1.0, roll=7.0, brk=0.0, mast=4.0)),
    (1 / 7, dict(z=-0.015, pitch=3.0, roll=13.0, brk=2.0, mast=14.0)),
    (2 / 7, dict(z=-0.03, pitch=6.0, roll=11.0, brk=6.0, mast=40.0)),
    (3 / 7, dict(z=-0.05, pitch=11.0, roll=8.0, brk=11.0, mast=72.0)),
    (4 / 7, dict(z=-0.09, pitch=18.0, roll=6.0, brk=15.0, mast=84.0)),
    (5 / 7, dict(z=-0.16, pitch=27.0, roll=5.0, brk=18.0, mast=88.0)),
    (6 / 7, dict(z=-0.30, pitch=38.0, roll=4.0, brk=20.0, mast=90.0)),
    (1.0, dict(z=-1.40, pitch=40.0, roll=4.0, brk=20.0, mast=90.0)),
]


class Ship(units.Figure):
    """A hull with its rig, oars, crew and load. Bow toward +X, z = 0 the
    waterline; a subclass sets the numbers and draws its own deck."""

    cls = "Ship"
    water = True
    animations = [(n,) + anim.FOOT[n] for n in ("idle", "walk", "attack", "death", "decay")]
    speed = 1.6            # tiles a second (kinds.rs): the wake's flow
    HL, W, H = 0.6, 0.22, 0.15      # half length, half beam, gunwale over the water
    rise_b, rise_s = 0.10, 0.16     # the sheer's rise at the bow and the stern
    keel = 0.12
    full = 2.4             # the hull's fullness (bigger: fuller ends)
    stripe = 0.065         # the player band's height under the rail
    oars = 0               # a side
    oar_x = (-0.5, 0.5)    # the first and last oar, as u
    oar_reach = 0.3
    eye = True
    seed = 0
    stem = "post"          # "post", "ram", "scroll"
    stern = "curl"         # "curl", "post", "fan"

    def __init__(self):
        super().__init__()
        self.rnd = random.Random(self.seed)

    # ---- the hull's numbers
    def beam(self, u):
        u = max(-1.0, min(1.0, u))
        return max(0.012, self.W * (1 - abs(u) ** self.full) ** 0.55)

    def sheer(self, u):
        u = max(-1.0, min(1.0, u))
        return self.H + (self.rise_b * u ** 3 if u > 0 else self.rise_s * abs(u) ** 3)

    def at_x(self, x):
        u = x / self.HL
        return self.beam(u), self.sheer(u)

    def deck(self, x):
        return self.at_x(x)[1] - 0.045

    # ---- motion
    def state(self, name, t, i, n):
        st = dict(z=0.0, pitch=0.0, roll=0.0, brk=0.0, mast=0.0, oar=None, fill=0.35,
                  crew=True, sink=0.0)
        if name == "idle":
            st["z"] = 0.010 * math.sin(2 * math.pi * t)
            st["roll"] = 1.6 * math.sin(2 * math.pi * t + 1.1)
            st["pitch"] = 0.7 * math.cos(2 * math.pi * t)
            st["fill"] = 0.25 + 0.05 * math.sin(2 * math.pi * t)
        elif name == "walk":
            st["z"] = 0.008 * math.sin(4 * math.pi * t)
            st["pitch"] = -0.8 + 0.8 * math.sin(4 * math.pi * t + 0.7)
            st["roll"] = 1.0 * math.sin(2 * math.pi * t)
            st["oar"] = t
            st["fill"] = 0.8
        elif name == "attack":
            st.update(self.attack_state(t))
        elif name == "death":
            st.update(anim.keyed(DEATH, t, loop=False))
            st["crew"] = t < 0.4
            st["sink"] = t
        elif name == "decay":
            st.update(DEATH[-1][1])
            st["crew"] = False
            st["sink"] = 1.0 + t
        return st

    def attack_state(self, t):
        """The hull's motion while it attacks (the deck's own is attack())."""
        kick = math.exp(-((t - 0.62) / 0.2) ** 2)
        return dict(z=0.004 * math.sin(2 * math.pi * t), roll=1.4 * kick - 0.4, pitch=0.3)

    # ---- the build
    def build(self, name, i, player, parent=None):
        n, loops = next((nn, lp) for a, nn, _, lp, _ in self.animations if a == name)
        t = anim.frame_t(i, n, loops)
        st = self.state(name, t, i, n)
        self.st, self.anim, self.t, self.player = st, name, t, player
        root = lib.empty("unit", (0, 0, 0), 0.0, parent)
        base = (T(0, 0, st["z"]) @ T(-0.25 * self.HL, 0, 0) @ R("Y", st["pitch"])
                @ T(0.25 * self.HL, 0, 0) @ R("X", st["roll"]))
        xb = 0.08 * self.HL                 # where it breaks
        self.xb = xb
        self.Mb = base @ T(xb, 0, 0) @ R("Y", st["brk"]) @ T(-xb, 0, 0)
        self.Ms = base @ T(xb, 0, 0) @ R("Y", -st["brk"] * 0.8) @ T(-xb, 0, 0)
        b = B("ship")
        if st["sink"] < 1.0:
            self.draw_hull(b, player)
            self.draw_oars(b)
            self.draw_rig(b, player)
            self.draw_deck(b, player)
        b.finish(root)
        self.draw_water(root, name, t)
        if name in ("death", "decay"):
            self.draw_flotsam(root, player, st["sink"])
        return root

    def M(self, x):
        """The matrix a part at x rides on: the bow's or the stern's half."""
        return self.Mb if x >= self.xb else self.Ms

    # ---- hull
    def hull_mats(self, player):
        return dict(low=mat((0.20, 0.12, 0.08), rough=0.8), wet=mat((0.16, 0.10, 0.07), rough=0.5),
                    hull=mat((0.40, 0.23, 0.12), rough=0.7), stripe=pm(player),
                    rail=mat((0.94, 0.90, 0.80), rough=0.8), inner=mat((0.30, 0.18, 0.10), rough=0.8),
                    deck=mat((0.74, 0.56, 0.36), rough=0.9), broken=mat((0.12, 0.08, 0.05), rough=0.9))

    def section(self, u):
        w, sh = self.beam(u), self.sheer(u)
        keel = -self.keel * (1 - abs(u) ** 4)
        sl = max(0.03, sh - 0.03 - self.stripe)
        half = [(0.0, keel), (0.62 * w, keel * 0.6), (0.96 * w, -0.03), (w, 0.012),
                (1.01 * w, sl), (1.02 * w, sh - 0.028), (1.035 * w, sh), (0.90 * w, sh - 0.004),
                (0.88 * w, sh - 0.045)]
        return half, sh

    def draw_hull(self, b, player):
        m = self.hull_mats(player)
        bands = [m["low"], m["low"], m["wet"], m["hull"], m["stripe"], m["rail"], m["inner"],
                 m["inner"]]
        N = 22
        us = [-1 + 2 * k / N for k in range(N + 1)]
        ub = self.xb / self.HL
        cut = min(range(len(us)), key=lambda k: abs(us[k] - ub))
        us[cut] = ub
        for lo, hi in ((0, cut), (cut, N)):
            self._hull_piece(b, us[lo:hi + 1], bands, m)
        self.draw_ends(b, player, m)

    def _hull_piece(self, b, us, bands, m):
        M = self.M(0.5 * (us[0] + us[-1]) * self.HL + 1e-4)
        bm = b.bm
        rows = []
        for u in us:
            half, sh = self.section(u)
            x = u * self.HL
            row = {}
            for s in (1, -1):
                row[s] = [bm.verts.new(M @ V((x, s * y, z))) for y, z in half]
            row["c"] = bm.verts.new(M @ V((x, 0, sh - 0.045)))
            rows.append(row)
        for r0, r1 in zip(rows, rows[1:]):
            for s in (1, -1):
                for j, bmat in enumerate(bands):
                    f = (r0[s][j], r1[s][j], r1[s][j + 1], r0[s][j + 1])
                    try:
                        fc = bm.faces.new(f if s > 0 else f[::-1])
                    except ValueError:
                        continue
                    fc.material_index = b.mi(bmat)
                    fc.smooth = bmat is not m["rail"]
                try:
                    fc = bm.faces.new((r0[s][-1], r1[s][-1], r1["c"], r0["c"])
                                      if s > 0 else (r0["c"], r1["c"], r1[s][-1], r0[s][-1]))
                    fc.material_index = b.mi(m["deck"])
                except ValueError:
                    pass
        for row in (rows[0], rows[-1]):       # the ends: closed (a broken end shows dark)
            ring_ = row[1] + list(reversed(row[-1]))
            try:
                fc = bm.faces.new(ring_)
                fc.material_index = b.mi(m["broken"])
            except ValueError:
                pass
        # deck planks' seams, a few dark lines across
        dk = mat((0.45, 0.31, 0.18), rough=0.9)
        for u in us[1:-1:2]:
            if abs(u) > 0.85:
                continue
            w, sh = self.beam(u), self.sheer(u)
            x = u * self.HL
            with b.at(M, True):
                b.cube((x, 0, sh - 0.043), (0.012, 1.76 * w, 0.004), dk)

    def draw_ends(self, b, player, m):
        HL = self.HL
        hullm = m["hull"]
        Mb, Ms = self.Mb, self.Ms
        _, shb = self.at_x(HL)
        _, shs = self.at_x(-HL)
        with b.at(Mb, True):
            if self.stem == "ram":
                b.seg((HL - 0.10, 0, 0.0), (HL + 0.06, 0, 0.0), 0.06, 0.05, bronze(), n=8, sy=0.6)
                b.seg((HL + 0.06, 0, 0.0), (HL + 0.20, 0, 0.012), 0.05, 0.01, bronze(), n=8, sy=0.6)
                b.chain([V((HL - 0.02, 0, 0.0)), V((HL + 0.03, 0, shb * 0.55)),
                         V((HL + 0.05, 0, shb + 0.06)), V((HL - 0.02, 0, shb + 0.13))],
                        0.04, 0.025, hullm, n=6, sy=0.6)
                b.ball((HL - 0.03, 0, shb + 0.14), 0.035, pm(player), n=6)
            elif self.stem == "scroll":
                pts = [V((HL - 0.02, 0, 0.0)), V((HL + 0.04, 0, shb * 0.6)), V((HL + 0.06, 0, shb + 0.08)),
                       V((HL + 0.02, 0, shb + 0.16)), V((HL - 0.05, 0, shb + 0.15))]
                b.chain(pts, 0.035, 0.022, hullm, n=6, sy=0.6)
            else:
                b.chain([V((HL - 0.02, 0, 0.0)), V((HL + 0.03, 0, shb * 0.7)),
                         V((HL + 0.04, 0, shb + 0.07))], 0.035, 0.025, hullm, n=6, sy=0.6)
            if self.eye:
                ex = HL * 0.80
                ew, es = self.at_x(ex)
                for s in (1, -1):
                    c = V((ex, s * (ew * 1.02 + 0.004), es - 0.03 - self.stripe * 0.5))
                    n_ = V((0.3, s, 0.1)).normalized()
                    F = T(c) @ units._frame(n_, (0, 0, 1)).to_4x4()
                    with b.at(Mb @ F, True):
                        r = min(0.055, 0.25 * self.stripe + 0.03)
                        b.seg((0, 0, 0), (0, 0, 0.01), r, r, mat((0.98, 0.96, 0.9), rough=0.7), n=10,
                              sy=0.65)
                        b.seg((0, 0, 0.01), (0, 0, 0.017), r * 0.5, r * 0.5, mat((0.05, 0.05, 0.08)),
                              n=8)
        with b.at(Ms, True):
            if self.stern == "curl":
                sp = [V((-HL + 0.02, 0, 0.0)), V((-HL - 0.06, 0, shs * 0.6)),
                      V((-HL - 0.08, 0, shs + 0.08)), V((-HL - 0.03, 0, shs + 0.17)),
                      V((-HL + 0.06, 0, shs + 0.20)), V((-HL + 0.14, 0, shs + 0.17))]
                b.chain(sp, 0.04, 0.02, hullm, n=6, sy=0.6)
            elif self.stern == "fan":
                sp = [V((-HL + 0.02, 0, 0.0)), V((-HL - 0.06, 0, shs * 0.6)),
                      V((-HL - 0.09, 0, shs + 0.12))]
                b.chain(sp, 0.04, 0.03, hullm, n=6, sy=0.6)
                for a in (-35, 0, 35):
                    d = V((-math.cos(math.radians(70 + a * 0.2)) * 0.3, math.sin(math.radians(a)),
                           math.cos(math.radians(a)) * 0.9)).normalized()
                    b.seg(sp[-1], sp[-1] + d * 0.11, 0.016, 0.026, pm(player) if a else gold(), n=5,
                          sy=0.5)
            else:
                b.chain([V((-HL + 0.02, 0, 0.0)), V((-HL - 0.04, 0, shs * 0.7)),
                         V((-HL - 0.05, 0, shs + 0.07))], 0.035, 0.025, hullm, n=6, sy=0.6)
            # the steering oars
            for s in (1, -1):
                w, sh = self.at_x(-HL * 0.82)
                o0 = V((-HL * 0.82, s * w * 0.95, sh + 0.02))
                o1 = V((-HL * 1.02, s * (w + 0.07), -0.04))
                b.seg(o0, o1, 0.012, 0.012, "wood", n=5)
                b.seg(o0 + (o1 - o0) * 0.6, o1, 0.035, 0.04, "wood", n=6, sy=0.25)

    # ---- oars
    def oar_pose(self, k, s):
        """(thole, blade) of oar k on side s, in the ship's frame."""
        st = self.st
        u0, u1 = self.oar_x
        u = u0 + (u1 - u0) * k / max(1, self.oars - 1)
        x = u * self.HL
        w, sh = self.at_x(x)
        thole = V((x, s * w * 0.99, sh - 0.03))
        reach = self.oar_reach
        if st["oar"] is None:
            if self.anim == "death":
                # dropped: trailing in the water, askew
                return thole, V((x - reach * 0.7, s * (w + reach * 0.75), -0.02 - 0.03 * (k % 2)))
            return thole, V((x - 0.04, s * (w + reach), 0.015))
        ph = (st["oar"] - 0.04 * k / max(1, self.oars)) % 1.0
        c, sn = math.cos(2 * math.pi * ph), math.sin(2 * math.pi * ph)
        dx = 0.14 * c * reach / 0.3
        z = -0.035 if sn >= 0 else -0.035 + 0.11 * (-sn) ** 0.8
        return thole, V((x + dx, s * (w + reach * (0.97 - 0.05 * abs(c))), z))

    def draw_oars(self, b):
        if not self.oars or not self.st["crew"] and self.anim != "death":
            return
        oarm = mat((0.80, 0.64, 0.40), rough=0.8)
        for k in range(self.oars):
            for s in (1, -1):
                th, bl = self.oar_pose(k, s)
                if self.anim == "death" and self.st["brk"] > 8 and (k + (s > 0)) % 3 == 0:
                    continue          # a few broken off
                M = self.M(th.x)
                with b.at(M, True):
                    d = (bl - th).normalized()
                    b.seg(th - d * 0.04, bl, 0.011, 0.011, oarm, n=5)
                    b.seg(bl - d * 0.09, bl + d * 0.01, 0.03, 0.034, oarm, n=6, sy=0.3,
                          hint=(0, 0, 1))

    # ---- rig
    def sail(self, b, M, x, z_top, half_w, drop, mats, brace=0.0, belly=0.12, taper=0.06,
             nu=8, nv=4):
        """A square sail on its yard at (x, z_top) across the ship, bellied
        forward by `belly` (scaled by the state's fill), turned by `brace`
        degrees; `mats(i, j)` its material per patch (columns i, rows j)."""
        fill = self.st["fill"]
        Mx = M @ T(x, 0, 0) @ R("Z", brace)
        with b.at(Mx, True):
            b.seg((0.02, -half_w - 0.04, z_top), (0.02, half_w + 0.04, z_top), 0.017, 0.017, "wood",
                  n=6)
            vs = []
            for j in range(nv + 1):
                v = j / nv
                for i in range(nu + 1):
                    u = i / nu
                    y = (u - 0.5) * 2 * half_w * (1 - taper * v)
                    bl = belly * fill * math.sin(math.pi * u) ** 0.8 * math.sin(math.pi * (0.1 + 0.75 * v))
                    vs.append((0.035 + bl, y, z_top - 0.015 - v * drop))
            bm = b.bm
            vv = [bm.verts.new(b.M @ V(p)) for p in vs]
            for j in range(nv):
                for i in range(nu):
                    k = j * (nu + 1) + i
                    fc = bm.faces.new((vv[k], vv[k + 1], vv[k + nu + 2], vv[k + nu + 1]))
                    fc.material_index = b.mi(mats(i, j))
                    fc.smooth = True
            rope = mat((0.85, 0.78, 0.6))
            for s in (1, -1):    # sheets to the rail
                b.seg(vs[-1 if s > 0 else -(nu + 1)], (-0.25, s * 0.18, 0.15), 0.005, 0.005, rope, n=4)

    def mast(self, b, M, x, top, player, pennant=True, r=0.024):
        _, sh = self.at_x(x)
        with b.at(M, True):
            b.seg((x, 0, sh - 0.1), (x, 0, top), r, r * 0.7, "wood", n=6)
            b.ball((x, 0, top + 0.01), r * 1.1, gold(), n=6)
            if pennant:
                fl = math.sin(2 * math.pi * self.t) * 0.03
                b.plate([(0, 0), (-0.06, 0.012 + fl * 0.3), (-0.18, 0.03 + fl), (-0.06, 0.05 + fl * 0.3),
                         (0, 0.06)], 0.008, pm(player), frame=T(x, 0, top - 0.07) @ R("X", 90))

    def draw_rig(self, b, player):
        pass

    def mast_matrix(self, x):
        """A mast's frame: falling to port about its foot as the ship dies."""
        d = self.deck(x)
        return self.M(x) @ T(x, 0, d) @ R("X", -self.st["mast"]) @ T(-x, 0, -d)

    def draw_deck(self, b, player):
        pass

    # ---- the water
    def waterline(self, n=28, grow=0.0):
        """The hull's outline at the waterline, a closed loop (port side
        bow to stern, then starboard), in the ship's own frame on z = 0."""
        pts = []
        for k in range(n + 1):
            u = 1 - 2 * k / n
            pts.append((u * self.HL * (1 + grow * 0.15) + (0.03 if u > 0.97 else 0),
                        self.beam(u) * 0.98 + grow))
        loop = pts + [(x, -y) for x, y in reversed(pts[1:-1])]
        return loop

    def draw_water(self, root, name, t):
        st = self.st
        sink = st["sink"]
        z = st["z"]
        foam, shade = Sheet(), Sheet()
        seed = 7
        if sink < 1.0:
            loop = self.waterline()
            # the hull's outline moves with its pitch: shift the waterline
            # forward a little as the bow goes down
            if name == "death":
                k = 1 + 0.6 * t
                loop = [(x * k - 0.15 * self.HL * t, y * (1 + 0.3 * t)) for x, y in loop]
            nr = loop_normals(loop)
            # the dark water round the hull, soft
            rows = []
            for d, a in ((-0.03, 0.55), (0.02, 0.48), (0.06, 0.28), (0.11, 0.10), (0.17, 0.0)):
                rows.append([(p, a * (1 - sink * 0.7)) for p in offset_loop(loop, nr, d * (1 + self.W * 2))])
            shade.grid(rows, closed=True)
            # the lapping line at the hull
            per = 2.0 * self.HL
            lap = 0.55 if name != "walk" else 0.75
            ring(foam, loop, nr, -0.01, 0.035 + 0.01 * math.sin(2 * math.pi * t),
                 lambda i: lap * (0.35 + 0.9 * _waves(loop[i][0] + (loop[i][1] > 0) * 0.37,
                                                      0.0, seed, per)))
        if name == "idle" or name == "attack":
            loop = self.waterline(grow=0.0)
            nr = loop_normals(loop)
            for ph in (0.0, 0.5):
                f = (t + ph) % 1.0
                d = 0.04 + 0.17 * f
                a = 0.36 * (1 - f) ** 1.4 * min(1.0, f * 5 + 0.2)
                ring(foam, loop, nr, d, d + 0.05 + 0.04 * f,
                     lambda i, a=a: a * (0.4 + 0.8 * _waves(loop[i][0], loop[i][1], seed + 3,
                                                             2 * self.HL)))
        if name == "walk":
            self.wake(foam, t)
        if name == "attack":
            self.attack_water(foam, t)
        if name == "death":
            self.death_water(foam, t)
        if name in ("death", "decay"):
            self.flotsam_water(foam, shade, st["sink"])
        foam.obj("foam", foam_mat(), FOAM_Z, root)
        shade.obj("water_shadow", shadow_mat(), SHADOW_Z, root)

    def wake(self, foam, t):
        """The bow wave, the V wake, the stern wash and the oar splashes,
        flowing back at the ship's speed (the loop is seamless: the noise's
        period divides the distance one loop covers)."""
        HL, W = self.HL, self.W
        flow = self.speed * WALK_S          # tiles a loop
        per = flow / 2
        shift = flow * t
        seed = 11
        # bow wave hugging the hull from the stem back
        n = 14
        for s in (1, -1):
            c, hw, al = [], [], []
            for k in range(n + 1):
                u = 1.02 - 0.85 * k / n
                x = u * HL
                w = self.beam(min(1, u)) * 0.98
                c.append((x, s * (w + 0.02 + 0.03 * (k / n))))
                hw.append(0.035 + 0.04 * (1 - k / n))
                al.append(1.0 * (1 - k / n) ** 0.7 * (0.65 + 0.5 * _waves(x + shift, s, seed, per)))
            strip(foam, c, hw, al)
        # the V: two arms from the bow, opening at about 20 degrees, past the stern
        trail = 0.9 * 2 * HL
        n = 22
        ang = math.tan(math.radians(21))
        for s in (1, -1):
            c, hw, al = [], [], []
            for k in range(n + 1):
                f = k / n
                x = HL * 0.9 - f * (2 * HL + trail)
                spread = max(self.beam(x / HL) + 0.05 if abs(x) < HL else 0.0, (HL * 0.9 - x) * ang)
                c.append((x, s * spread))
                hw.append(0.04 + 0.08 * f)
                wv = _waves(x + shift, s * 2.0, seed + 1, per)
                al.append(0.95 * (1 - f) ** 1.2 * (0.3 + 0.9 * wv) * min(1.0, 0.4 + f * 4))
            strip(foam, c, hw, al)
        # the wash: churned water behind the stern
        n = 14
        c, hw, al = [], [], []
        for k in range(n + 1):
            f = k / n
            x = -HL * 0.95 - f * trail * 0.85
            c.append((x, 0.02 * math.sin(6 * f)))
            hw.append(W * (0.75 + 1.0 * f))
            wv = _waves(x + shift, 0.5, seed + 2, per)
            al.append(1.0 * (1 - f) ** 1.2 * (0.45 + 0.8 * wv))
        strip(foam, c, hw, al, profile=(0.0, 0.5, 0.9, 1.0, 0.9, 0.5, 0.0))
        # oar splashes: where a blade bites (the catch) and swirls along the stroke
        if self.oars:
            for k in range(self.oars):
                for s in (1, -1):
                    th, bl = self.oar_pose(k, s)
                    ph = (self.st["oar"] - 0.04 * k / max(1, self.oars)) % 1.0
                    if ph < 0.5:
                        a = 0.75 * (1 - ph * 1.6) if ph < 0.25 else 0.35
                        r = 0.035 + 0.05 * ph
                        foam.blob((bl.x + 0.02, bl.y), r * 1.3, r, a, n=8)
                    # the swirls left behind by the last stroke
                    back = (ph - 0.5) % 1.0
                    foam.blob((bl.x - 0.08 - back * flow * 0.5, bl.y + s * 0.02), 0.03 + 0.03 * back,
                              0.022 + 0.02 * back, 0.3 * (1 - back), n=8)

    def attack_water(self, foam, t):
        pass

    def death_water(self, foam, t):
        """The churn as it lists and goes under: a burst at its side, then
        a broad white boil where it sinks, spreading and thinning."""
        HL, W = self.HL, self.W
        if t < 0.35:
            a = 0.8 * (1 - t / 0.35) ** 0.7
            for k in range(5):
                x = HL * (0.3 - 0.25 * k)
                foam.blob((x, -(W + 0.03 + 0.06 * t)), 0.08 + 0.1 * t, 0.05 + 0.06 * t, a, n=10)
        boil = max(0.0, (t - 0.2) / 0.8)
        if boil > 0:
            cx = HL * (0.4 - 0.6 * boil)
            loop = [(cx + (HL * 0.9 + 0.1 * boil) * math.cos(2 * math.pi * k / 24),
                     (W + 0.06 + 0.12 * boil) * math.sin(2 * math.pi * k / 24)) for k in range(24)]
            nr = loop_normals(loop)
            ring(foam, loop, nr, -0.25 - 0.1 * boil, 0.05 + 0.1 * boil,
                 lambda i: 0.7 * (1 - 0.4 * boil) * (0.35 + 0.8 * _waves(loop[i][0], loop[i][1], 5, 0.7)),
                 steps=4)
            for k in range(7):
                rr = random.Random(30 + k)
                foam.blob((cx + rr.uniform(-0.6, 0.6) * HL, rr.uniform(-1, 1) * W * 0.8),
                          0.06 + 0.05 * boil, 0.04 + 0.04 * boil, 0.6 * (1 - 0.5 * boil), n=8)

    # ---- flotsam
    def pieces(self):
        """[(kind, x, y, drift x, drift y, turn)] what floats when it sinks."""
        rr = random.Random(self.seed + 50)
        HL, W = self.HL, self.W
        out = []
        kinds = ["plank", "plank", "plank", "oar", "rag", "barrel", "plank", "spar", "plank", "rag"]
        n = 6 + int(HL * 8)
        for k in range(n):
            a = rr.uniform(0, 2 * math.pi)
            x = rr.uniform(-0.8, 0.8) * HL
            y = rr.uniform(-1, 1) * W
            out.append((kinds[k % len(kinds)], x, y, math.cos(a) * rr.uniform(0.1, 0.22),
                        math.sin(a) * rr.uniform(0.06, 0.16), rr.uniform(-40, 40)))
        return out

    def flotsam_at(self, sink):
        """Where the pieces are: they rise from 0.4 into the death and drift
        apart through the decay."""
        if sink < 0.4:
            return []
        f = (sink - 0.4) / 0.6       # 0..1 to the death's end, 1..2.67 through the decay
        out = []
        for kind, x, y, dx, dy, turn in self.pieces():
            sp = f if f < 1 else 1 + (f - 1) * 0.45
            out.append((kind, x * 0.7 + dx * sp - 0.15 * self.HL, y + dy * sp, turn * (0.3 + sp)))
        return out

    def draw_flotsam(self, root, player, sink):
        items = self.flotsam_at(sink)
        if not items:
            return
        b = B("flotsam")
        wood, dark = mat((0.50, 0.32, 0.18), rough=0.8), mat((0.30, 0.19, 0.11), rough=0.8)
        for k, (kind, x, y, turn) in enumerate(items):
            M = T(x, y, 0.0) @ R("Z", turn) @ R("X", 6 * math.sin(k + sink * 3))
            with b.at(M, True):
                if kind == "plank":
                    b.cube((0, 0, 0.004), (0.20, 0.05, 0.03), wood if k % 2 else dark)
                elif kind == "spar":
                    b.seg((-0.22, 0, 0.0), (0.22, 0, 0.0), 0.022, 0.018, "wood", n=6)
                elif kind == "oar":
                    b.seg((-0.2, 0, 0.0), (0.12, 0, 0.0), 0.012, 0.012, mat((0.80, 0.64, 0.40)), n=5)
                    b.seg((0.10, 0, 0.0), (0.2, 0, 0.0), 0.03, 0.034, mat((0.80, 0.64, 0.40)), n=6,
                          sy=0.3, hint=(0, 0, 1))
                elif kind == "barrel":
                    b.seg((-0.05, 0, 0.0), (0.05, 0, 0.0), 0.04, 0.04, mat("terracotta"), n=8)
                elif kind == "rag":       # a torn piece of the sail, in the owner's colour
                    b.plate([(-0.11, -0.06), (0.03, -0.09), (0.12, -0.02), (0.08, 0.07), (-0.07, 0.08)],
                            0.012, pm(player), frame=T(0, 0, 0.002))
        b.finish(root)

    def flotsam_water(self, foam, shade, sink):
        fade = 1.0 if sink <= 1 else max(0.0, 1 - (sink - 1) * 0.8)
        for k, (kind, x, y, turn) in enumerate(self.flotsam_at(sink)):
            r = 0.12 if kind in ("plank", "spar", "oar") else 0.07
            foam.blob((x, y), r * 1.2, r * 0.6, 0.28 * fade, n=10, rot=math.radians(turn))
            shade.blob((x, y), r * 1.1, r * 0.5, 0.25, n=10, rot=math.radians(turn))
        if sink > 0.85:           # the last bubbles where it went down
            f = min(1.0, (sink - 0.85) / 1.2)
            loop = [((0.3 + 0.5 * f) * self.HL * math.cos(2 * math.pi * k / 20) - 0.2 * self.HL,
                     (self.W + 0.1 + 0.2 * f) * math.sin(2 * math.pi * k / 20)) for k in range(20)]
            nr = loop_normals(loop)
            ring(foam, loop, nr, 0.0, 0.05 + 0.04 * f,
                 lambda i: 0.45 * (1 - f) * (0.4 + 0.8 * _waves(loop[i][0], loop[i][1], 9, 0.9)))


# ----------------------------------------------------------- the six ships

def _stripes(a, b_, every=2):
    return lambda i, j: a if (i // every) % 2 == 0 else b_


class FishingBoat(Ship):
    """The smallest: one triangular (lateen) sail, two fishermen, a net
    heaped amidships and a basket of fish; its attack casts the net and
    hauls it back (the game loops it while it fishes)."""
    speed = 1.3
    HL, W, H = 0.46, 0.17, 0.10
    rise_b, rise_s = 0.07, 0.09
    keel = 0.07
    full = 2.2
    stripe = 0.045
    stem, stern = "post", "post"
    seed = 3

    def draw_rig(self, b, player):
        x = 0.06 * self.HL
        M = self.mast_matrix(x)
        self.mast(b, M, x, 0.74, player, pennant=False, r=0.018)
        M = M @ T(x, 0, 0) @ R("Z", 28) @ T(-x, 0, 0)      # the yard swung out a little
        fill = self.st["fill"]
        # the lateen: a long yard slanting from low forward to high aft, the
        # sail hung under it, sheeted to the stern
        with b.at(M, True):
            y0, y1 = V((0.40, 0, 0.18)), V((-0.36, 0, 0.92))
            b.seg(y0, y1, 0.012, 0.010, "wood", n=5)
            clew = V((-0.30, 0.0, 0.16))
            nu, nv = 6, 5
            vs = []
            for j in range(nv + 1):        # rows from the yard down to the foot
                v = j / nv
                for i in range(nu + 1):
                    u = i / nu
                    top = y0.lerp(y1, u)
                    bot = y0.lerp(clew, u)
                    p = top.lerp(bot, v)
                    bl = 0.07 * fill * math.sin(math.pi * u) * math.sin(math.pi * v)
                    vs.append((p.x, p.y + bl, p.z))
            bm = b.bm
            vv = [bm.verts.new(b.M @ V(p)) for p in vs]
            pl = pm(player)
            pd = pm(player, 0.82)
            for j in range(nv):
                for i in range(nu):
                    k = j * (nu + 1) + i
                    try:
                        fc = bm.faces.new((vv[k], vv[k + 1], vv[k + nu + 2], vv[k + nu + 1]))
                    except ValueError:
                        continue
                    fc.material_index = b.mi(pl if j < nv - 1 else pd)
                    fc.smooth = True

    def draw_deck(self, b, player):
        st = self.st
        if not st["crew"]:
            return
        skin, hair = _people(4)
        skin2, hair2 = _people(9)
        cloth = pm(player)
        lin = mat((0.93, 0.86, 0.70), rough=0.9)
        HL = self.HL
        # the helmsman at the stern
        xs = -0.62 * HL
        person(b, self.M(xs) @ T(xs, 0.0, self.deck(xs)), skin2, hair2, lin,
               hands=((0.1, 0.2, 0.45), (0.0, -0.25, 0.5)), helmet="cap")
        # the net heap and a basket of fish
        with b.at(self.M(0.0), True):
            net = mat((0.62, 0.55, 0.42), rough=0.95)
            if self.anim != "attack":
                b.ball((-0.12 * HL, 0.02, self.deck(0) + 0.015), 0.07, net, scale=(1.3, 1, 0.45), n=8)
            b.seg((-0.35 * HL, -0.06, self.deck(0)), (-0.35 * HL, -0.06, self.deck(0) + 0.05), 0.04,
                  0.05, mat("straw", rough=0.9), n=8)
            for k in range(3):
                b.ball((-0.35 * HL + 0.012 * k, -0.06 + 0.01 * k, self.deck(0) + 0.055), 0.016,
                       mat((0.80, 0.84, 0.88), rough=0.3, metal=0.5), n=5, scale=(1.6, 0.8, 0.8))
        # the fisherman: casts on the attack, otherwise minds the net
        xf = 0.3 * HL
        if self.anim == "attack":
            self.cast(b, player, xf, skin, hair, cloth)
        else:
            sway = 4 * math.sin(2 * math.pi * self.t)
            person(b, self.M(xf) @ T(xf, 0, self.deck(xf)) @ R("Z", 70), skin, hair, cloth,
                   hands=((0.18, 0.18, 0.42), (0.2, -0.15, 0.40)), lean=10 + sway)

    def cast(self, b, player, xf, skin, hair, cloth):
        """The cast and haul, 6 frames that loop: lift the net (0, 1), throw
        it wide (2), it lands spread on the water (3, the impact), haul it
        back with the catch (4, 5)."""
        t = self.t
        i = round(t * 5)
        net = mat((0.62, 0.55, 0.42), rough=0.95)
        side = 1.0           # to port
        M0 = self.M(xf) @ T(xf, 0, self.deck(xf)) @ R("Z", 90)   # he faces the port side
        poses = [((0.05, 0.25, 0.45), (0.05, -0.2, 0.45), 18), ((-0.05, 0.2, 0.95), (-0.1, -0.2, 0.95), -12),
                 ((0.35, 0.15, 0.8), (0.35, -0.15, 0.8), 14), ((0.3, 0.15, 0.55), (0.3, -0.15, 0.55), 20),
                 ((0.0, 0.2, 0.6), (0.25, -0.15, 0.45), 4), ((0.05, 0.22, 0.5), (0.2, -0.2, 0.42), 14)]
        hl, hr, lean = poses[i]
        person(b, M0, skin, hair, cloth, hands=(hl, hr), lean=lean)
        k = 0.36
        hand = (M0 @ S(k, k, k) @ R("Y", lean)) @ V(hl)
        w, sh = self.at_x(xf)
        if i in (0, 1):            # the net bunched in his hands
            with b.at(Matrix.Identity(4), True):
                b.ball(hand + V((0, 0, -0.03)), 0.06, net, scale=(1, 1, 1.4), n=8)
        elif i == 2:               # flying out, opening
            c = V((xf, side * (w + 0.22), 0.16))
            self._net(b, c, 0.13, 0.09, net, hand, lift=0.04)
        elif i == 3:               # spread on the water
            c = V((xf - 0.02, side * (w + 0.30), 0.004))
            self._net(b, c, 0.20, 0.14, net, hand, lift=0.0)
        elif i == 4:               # hauled in, the catch glinting
            c = V((xf, side * (w + 0.16), 0.03))
            self._net(b, c, 0.10, 0.07, net, hand, lift=0.04, fish=True)
        else:
            with b.at(self.M(xf), True):
                b.ball((xf - 0.03, 0.05, self.deck(xf) + 0.03), 0.07, net, scale=(1.3, 1, 0.6), n=8)
                for q in range(4):
                    b.ball((xf - 0.05 + 0.025 * q, 0.06, self.deck(xf) + 0.07), 0.018,
                           mat((0.85, 0.88, 0.92), rough=0.25, metal=0.6), n=5, scale=(1.6, 0.8, 0.8))

    def _net(self, b, c, rx, ry, net, hand, lift, fish=False):
        """A round net: rim cords and spokes, weighted rim, a line to the hand."""
        with b.at(Matrix.Identity(4), True):
            pts = [c + V((rx * math.cos(2 * math.pi * k / 12), ry * math.sin(2 * math.pi * k / 12),
                          lift * (0.5 + 0.5 * math.cos(2 * math.pi * k / 12)))) for k in range(12)]
            top = c + V((0, 0, lift + 0.03))
            for k in range(12):
                b.seg(pts[k], pts[(k + 1) % 12], 0.006, 0.006, net, n=4)
                if k % 2 == 0:
                    b.seg(top, pts[k], 0.004, 0.004, net, n=4)
                b.ball(pts[k], 0.01, mat((0.3, 0.3, 0.32)), n=4)
            b.seg(top, hand, 0.004, 0.004, net, n=4)
            if fish:
                for q in range(5):
                    a = q * 1.3
                    b.ball(c + V((rx * 0.4 * math.cos(a), ry * 0.4 * math.sin(a), lift * 0.5)), 0.02,
                           mat((0.88, 0.90, 0.94), rough=0.2, metal=0.6), n=5, scale=(1.6, 0.8, 0.8))

    def attack_state(self, t):
        return dict(z=0.004 * math.sin(2 * math.pi * t), roll=-2.0 * math.sin(math.pi * t), pitch=0.3)

    def attack_water(self, foam, t):
        i = round(t * 5)
        w, _ = self.at_x(0.3 * self.HL)
        c = (0.3 * self.HL - 0.02, w + 0.30)
        if i == 3:                 # the net lands: a ring of splashes
            for k in range(12):
                a = 2 * math.pi * k / 12
                foam.blob((c[0] + 0.21 * math.cos(a), c[1] + 0.15 * math.sin(a)), 0.035, 0.025, 0.7, n=8)
            foam.blob(c, 0.24, 0.17, 0.18, n=14)
        elif i == 4:
            foam.blob((c[0], c[1] - 0.12), 0.16, 0.11, 0.35, n=12)
            loop = [(c[0] + 0.26 * math.cos(2 * math.pi * k / 18), c[1] + 0.18 * math.sin(2 * math.pi * k / 18))
                    for k in range(18)]
            ring(foam, loop, loop_normals(loop), 0.0, 0.03, lambda q: 0.35)
        elif i == 5:
            loop = [(c[0] + 0.32 * math.cos(2 * math.pi * k / 18), c[1] + 0.22 * math.sin(2 * math.pi * k / 18))
                    for k in range(18)]
            ring(foam, loop, loop_normals(loop), 0.0, 0.03, lambda q: 0.18)


class TradeBoat(Ship):
    """A round-bellied merchantman: high sides, no oars, one big square
    sail with a white border, amphorae forward and bales aft of the mast,
    a small deckhouse at the stern."""
    speed = 1.6
    HL, W, H = 0.60, 0.27, 0.19
    rise_b, rise_s = 0.08, 0.13
    keel = 0.12
    full = 1.9
    stripe = 0.08
    stem, stern = "scroll", "curl"
    eye = True
    seed = 5

    def draw_rig(self, b, player):
        x = 0.05 * self.HL
        M = self.mast_matrix(x)
        self.mast(b, M, x, 1.02, player)
        pl, pd, wh = pm(player), pm(player, 0.82), mat((0.95, 0.92, 0.84), rough=0.85)

        def mats(i, j):
            if i in (0, 7) or j == 0:
                return wh
            return pl if j < 3 else pd
        self.sail(b, M, x, 0.95, 0.40, 0.62, mats, brace=18, belly=0.16, nu=8, nv=4)
        rope = mat((0.85, 0.78, 0.6))
        with b.at(M, True):
            b.seg((x, 0, 0.99), (self.HL + 0.05, 0, self.sheer(1) + 0.12), 0.005, 0.005, rope, n=4)
            b.seg((x, 0, 0.99), (-self.HL, 0, self.sheer(-1) + 0.15), 0.005, 0.005, rope, n=4)

    def draw_deck(self, b, player):
        st = self.st
        HL = self.HL
        rr = random.Random(12)
        terr = [mat("terracotta", rough=0.7), mat((0.70, 0.40, 0.24), rough=0.7)]
        bale = [mat((0.86, 0.78, 0.58), rough=0.95), mat((0.76, 0.64, 0.44), rough=0.95)]
        rope = mat((0.55, 0.40, 0.24), rough=0.9)
        # amphorae, three rows forward of the mast, standing in their racks
        for a in range(3):
            x = HL * (0.20 + 0.17 * a)
            w, sh = self.at_x(x)
            d = self.deck(x)
            ny = 3 if a < 2 else 2
            for k in range(ny):
                y = (k - (ny - 1) / 2) * w * 0.55
                with b.at(self.M(x) @ T(x, y, d) @ R("Y", rr.uniform(-6, 6)), True):
                    m = terr[(a + k) % 2]
                    b.loft([(0, 0, 0.0, .012, .012), (0, 0, 0.05, .042, .042), (0, 0, 0.12, .05, .05),
                            (0, 0, 0.17, .03, .03), (0, 0, 0.22, .016, .016), (0, 0, 0.25, .02, .02)], m, n=8)
                    b.seg((0, -0.03, 0.2), (0, 0.03, 0.2), 0.008, 0.008, m, n=4)
        # bales aft of the mast: wrapped, roped, stacked two high
        for k, (x, y, z) in enumerate(((-0.12, 0.07, 0), (-0.12, -0.08, 0), (-0.30, 0.0, 0),
                                        (-0.12, 0.0, 1), (-0.30, 0.09, 1))):
            xx = x * HL / 0.6
            d = self.deck(xx)
            with b.at(self.M(xx) @ T(xx, y, d + 0.045 + z * 0.085) @ R("Z", rr.uniform(-12, 12)), True):
                b.cube((0, 0, 0), (0.12, 0.10, 0.085), bale[k % 2])
                b.cube((0, 0, 0), (0.125, 0.018, 0.09), rope)
                b.cube((0, 0, 0), (0.02, 0.105, 0.09), rope)
        # the stern deckhouse with a player-coloured awning
        xs = -0.62 * HL
        d = self.deck(xs)
        with b.at(self.M(xs), True):
            for sx in (-0.07, 0.07):
                for sy in (-0.1, 0.1):
                    b.seg((xs + sx, sy, d), (xs + sx, sy, d + 0.2), 0.01, 0.01, "wood", n=4)
            b.plate([(-0.10, -0.14), (0.10, -0.14), (0.10, 0.14), (-0.10, 0.14)], 0.015, pm(player, 0.82),
                    frame=T(xs, 0, d + 0.21))
        if st["crew"]:
            skin, hair = _people(21)
            lin = mat((0.93, 0.86, 0.70), rough=0.9)
            xh = -0.80 * HL
            person(b, self.M(xh) @ T(xh, 0.0, self.deck(xh)), skin, hair, lin,
                   hands=((0.05, 0.25, 0.5), (0.05, -0.25, 0.5)), helmet="cap")
            skin, hair = _people(22)
            xc = 0.05 * HL - 0.05
            if self.anim == "attack":       # unused by the game: a sailor heaves a bale
                u = math.sin(math.pi * self.t)
                hands = ((0.25, 0.12, 0.45 + 0.4 * u), (0.25, -0.12, 0.45 + 0.4 * u))
                person(b, self.M(xc) @ T(xc, -0.12, self.deck(xc)), skin, hair, pm(player), hands=hands,
                       lean=20 - 25 * u)
            else:
                person(b, self.M(xc) @ T(xc, -0.12, self.deck(xc)) @ R("Z", -60), skin, hair,
                       pm(player), hands=((0.1, 0.22, 0.45), (0.1, -0.22, 0.45)))


class Transport(Ship):
    """Broad and deep, a wide deck crowded with passengers (helmets and
    spears in the owner's colours), two masts with two sails, a few sweeps."""
    speed = 1.4
    HL, W, H = 0.64, 0.33, 0.16
    rise_b, rise_s = 0.07, 0.12
    keel = 0.13
    full = 1.7
    stripe = 0.075
    oars = 4
    oar_x = (-0.45, 0.35)
    oar_reach = 0.22
    stem, stern = "post", "fan"
    seed = 7

    def draw_rig(self, b, player):
        pl, pd, wh = pm(player), pm(player, 0.82), mat((0.95, 0.92, 0.84), rough=0.85)
        for x, top, hw, dr, br in ((0.42 * self.HL, 0.78, 0.26, 0.42, 14),
                                   (-0.22 * self.HL, 0.90, 0.32, 0.52, 14)):
            M = self.mast_matrix(x)
            self.mast(b, M, x, top, player, pennant=x < 0)

            def mats(i, j):
                return wh if j == 1 else (pl if j < 3 else pd)
            self.sail(b, M, x, top - 0.06, hw, dr, mats, brace=br, belly=0.12, nu=6, nv=4)

    def draw_deck(self, b, player):
        if not self.st["crew"]:
            return
        HL = self.HL
        pl, pd = pm(player), pm(player, 0.82)
        crests = [pl, pd]
        spots = [(0.12, 0.13), (0.12, -0.13), (-0.02, 0.0), (0.25, 0.0), (-0.40, 0.12), (-0.40, -0.12),
                 (0.0, 0.17), (-0.05, -0.17), (0.32, 0.12), (-0.55, 0.0)]
        for k, (u, y) in enumerate(spots):
            x = u * HL
            skin, hair = _people(40 + k)
            d = self.deck(x)
            yaw = 60 * math.sin(k * 2.1)
            if self.anim == "attack":     # unused by the game: they cheer, spears up
                up = 0.35 * math.sin(math.pi * self.t + k)
                hands = ((0.1, 0.2, 0.75 + up), (0.12, -0.2, 0.5))
            else:
                hands = ((0.1, 0.22, 0.42), (0.15, -0.2, 0.5))
            M = self.M(x) @ T(x, y * self.W / 0.2, d) @ R("Z", yaw)
            person(b, M, skin, hair, pl if k % 2 else pd, hands=hands,
                   helmet=crests[k % 2] if k % 3 != 2 else None)
            if k % 2 == 0:              # a spear upright
                k_ = 0.36
                hnd = M @ S(k_, k_, k_) @ V(hands[1])
                with b.at(Matrix.Identity(4), True):
                    b.seg(hnd - V((0, 0, 0.12)), hnd + V((0.01, 0, 0.30)), 0.007, 0.007, "wood", n=4)
                    b.seg(hnd + V((0.01, 0, 0.30)), hnd + V((0.012, 0, 0.36)), 0.012, 0.002, bronze(), n=4)
        skin, hair = _people(60)
        xh = -0.84 * HL
        person(b, self.M(xh) @ T(xh, 0.0, self.deck(xh)), skin, hair, mat("linen", rough=0.9),
               hands=((0.05, 0.25, 0.5), (0.05, -0.25, 0.5)), helmet="cap")


class Warship(Ship):
    """A warship's archers: each draws on frames 0 to 2 and looses on the
    impact frame (3), the bowstring snapping back; recovering on 4, 5."""
    archers = ()           # (u, y, yaw) on deck
    shoot_aim = (1.0, 0.0, 0.18)

    def archer(self, b, player, k, u, y, yaw, helmet):
        x = u * self.HL
        skin, hair = _people(70 + k + self.seed)
        M = self.M(x) @ T(x, y, self.deck(x)) @ R("Z", yaw)
        t = self.t
        cloth = pm(player) if k % 2 == 0 else pm(player, 0.82)
        if self.anim == "attack":
            i = round(t * 5) + (k % 2) * 0      # all together: a volley
            # (bow hand, draw hand, nocked?)
            seq = [((0.30, 0.10, 0.55), (0.18, -0.02, 0.58), True),
                   ((0.42, 0.06, 0.78), (0.22, -0.04, 0.80), True),
                   ((0.45, 0.04, 0.82), (0.00, -0.10, 0.86), True),
                   ((0.45, 0.04, 0.82), (-0.10, -0.22, 0.88), False),
                   ((0.42, 0.06, 0.76), (-0.02, -0.22, 0.70), False),
                   ((0.32, 0.10, 0.60), (0.10, -0.15, 0.55), False)][i]
            bh, dh, nocked = seq
            aim = V(self.shoot_aim)
            person(b, M, skin, hair, cloth, hands=(bh, dh), helmet=helmet, lean=-4 if i == 2 else 2,
                   bow=(bh, dh if nocked else None, aim))
            if nocked:
                kk = 0.36
                P = M @ S(kk, kk, kk)
                tail, tip = P @ V(dh), P @ V(bh) + (P.to_3x3() @ aim).normalized() * 0.06
                with b.at(Matrix.Identity(4), True):
                    b.seg(tail, tip, 0.006, 0.006, "wood", n=4)
        else:
            sway = 3 * math.sin(2 * math.pi * (t + k * 0.3))
            bh = (0.22, 0.20, 0.48)
            person(b, M, skin, hair, cloth, hands=(bh, (0.08, -0.2, 0.45)), helmet=helmet, lean=sway,
                   bow=(bh, None, (0.2, 1.0, 0.0)))


class ArcherShip(Warship):
    """Long, low and light: one sail with a white band, a single bank of
    oars, three archers standing on deck."""
    speed = 1.6
    HL, W, H = 0.62, 0.19, 0.12
    rise_b, rise_s = 0.09, 0.15
    keel = 0.09
    full = 2.6
    stripe = 0.055
    oars = 6
    oar_x = (-0.55, 0.45)
    oar_reach = 0.26
    stem, stern = "post", "curl"
    seed = 9
    archers = ((0.55, 0.0, 0), (0.05, 0.04, 20), (-0.40, -0.03, -10))

    def draw_rig(self, b, player):
        x = -0.12 * self.HL
        M = self.mast_matrix(x)
        self.mast(b, M, x, 0.88, player)
        pl, pd, wh = pm(player), pm(player, 0.82), mat((0.95, 0.92, 0.84), rough=0.85)
        self.sail(b, M, x, 0.82, 0.30, 0.50, lambda i, j: wh if j == 2 else (pl if j < 2 else pd),
                  brace=22, belly=0.12, nu=6, nv=4)

    def draw_deck(self, b, player):
        if not self.st["crew"]:
            return
        for k, (u, y, yaw) in enumerate(self.archers):
            self.archer(b, player, k, u, y, yaw, helmet=None)
        skin, hair = _people(80)
        xh = -0.85 * self.HL
        person(b, self.M(xh) @ T(xh, 0.0, self.deck(xh)), skin, hair, mat("linen", rough=0.9),
               hands=((0.05, 0.25, 0.5), (0.05, -0.25, 0.5)), helmet="cap")


class WarGalley(Warship):
    """The longest: a bronze ram, big eyes, shields along the rail, a long
    bank of oars, a tall sail striped in the owner's colour and white, four
    crested archers."""
    speed = 1.6
    HL, W, H = 0.82, 0.235, 0.15
    rise_b, rise_s = 0.10, 0.20
    keel = 0.12
    full = 2.7
    stripe = 0.07
    oars = 9
    oar_x = (-0.62, 0.55)
    oar_reach = 0.34
    stem, stern = "ram", "fan"
    seed = 11
    archers = ((0.62, 0.0, 0), (0.30, 0.06, 15), (-0.10, -0.06, -15), (-0.45, 0.04, 10))

    def draw_rig(self, b, player):
        x = 0.08 * self.HL
        M = self.mast_matrix(x)
        self.mast(b, M, x, 1.12, player)
        pl, wh = pm(player), mat((0.96, 0.93, 0.86), rough=0.85)
        self.sail(b, M, x, 1.05, 0.40, 0.70, _stripes(pl, wh, 2), brace=20, belly=0.15, nu=8, nv=5)
        rope = mat((0.85, 0.78, 0.6))
        with b.at(M, True):
            b.seg((x, 0, 1.08), (self.HL + 0.0, 0, self.sheer(1) + 0.12), 0.005, 0.005, rope, n=4)
            b.seg((x, 0, 1.08), (-self.HL - 0.05, 0, self.sheer(-1) + 0.12), 0.005, 0.005, rope, n=4)

    def draw_deck(self, b, player):
        # shields along both rails: bronze bosses rimmed in the owner's colour
        for k in range(7):
            u = -0.52 + 1.0 * k / 6
            x = u * self.HL
            w, sh = self.at_x(x)
            for s in (1, -1):
                with b.at(self.M(x), True):
                    c = V((x, s * (w * 1.04 + 0.01), sh + 0.0))
                    b.seg(c, c + V((0, s * 0.012, 0)), 0.052, 0.05, pm(player, 0.82), n=10)
                    b.seg(c + V((0, s * 0.012, 0)), c + V((0, s * 0.02, 0)), 0.03, 0.015, bronze(), n=8)
        if not self.st["crew"]:
            return
        for k, (u, y, yaw) in enumerate(self.archers):
            self.archer(b, player, k, u, y, yaw, helmet=pm(player))
        skin, hair = _people(90)
        xh = -0.80 * self.HL
        person(b, self.M(xh) @ T(xh, 0.0, self.deck(xh) + 0.02), skin, hair, pm(player, 0.82),
               hands=((0.05, 0.25, 0.5), (0.05, -0.25, 0.5)), helmet=pm(player))


class CatapultShip(Ship):
    """Heavy and broad: a raised deck at the bow carrying a torsion engine
    (the arm cocked back, thrown forward on the impact frame, a stone
    leaving its cup), a small sail aft, big sweeps."""
    speed = 1.2
    HL, W, H = 0.74, 0.31, 0.18
    rise_b, rise_s = 0.06, 0.14
    keel = 0.14
    full = 2.0
    stripe = 0.085
    oars = 5
    oar_x = (-0.50, 0.15)
    oar_reach = 0.30
    stem, stern = "post", "curl"
    seed = 13

    def draw_rig(self, b, player):
        x = -0.40 * self.HL
        M = self.mast_matrix(x)
        self.mast(b, M, x, 0.86, player)
        pl, pd = pm(player), pm(player, 0.64)
        self.sail(b, M, x, 0.80, 0.27, 0.42, lambda i, j: pd if j == 0 else pl, brace=15, belly=0.10,
                  nu=6, nv=3)

    def engine_angle(self):
        """The throwing arm's angle from upright, degrees (+ forward)."""
        if self.anim != "attack":
            return -72.0
        return [-72, -76, -80, 22, 12, -35][round(self.t * 5)]

    def draw_deck(self, b, player):
        HL = self.HL
        x0, x1 = 0.12 * HL, 0.70 * HL
        dk = mat((0.62, 0.44, 0.26), rough=0.9)
        wood = mat((0.46, 0.30, 0.17), rough=0.8)
        dark = mat((0.28, 0.18, 0.10), rough=0.8)
        pl = pm(player)
        xm = 0.5 * (x0 + x1)
        dz = self.deck(xm) + 0.08
        with b.at(self.M(xm), True):
            # the raised fighting deck, its front faced with the owner's colour
            w = self.at_x(xm)[0]
            b.cube((xm, 0, dz - 0.04), (x1 - x0, 2 * w * 0.92, 0.08), dk)
            b.cube((x1 - 0.01, 0, dz - 0.03), (0.02, 2 * w * 0.94, 0.10), pl)
            for s in (1, -1):
                b.cube((xm, s * w * 0.93, dz + 0.03), (x1 - x0, 0.02, 0.06), pl)
            # the engine: a frame, the torsion bundle, the arm and its cup
            fx = xm - 0.02
            for s in (1, -1):
                b.cube((fx, s * 0.09, dz + 0.035), (0.34, 0.035, 0.05), wood)
                b.seg((fx + 0.12, s * 0.09, dz + 0.04), (fx + 0.05, s * 0.09, dz + 0.36), 0.022, 0.022, wood,
                      n=5)
                b.seg((fx - 0.05, s * 0.09, dz + 0.04), (fx + 0.05, s * 0.09, dz + 0.36), 0.018, 0.018, wood,
                      n=5)
            b.seg((fx + 0.05, -0.11, dz + 0.36), (fx + 0.05, 0.11, dz + 0.36), 0.024, 0.024, wood, n=6)
            b.cube((fx + 0.06, 0, dz + 0.385), (0.06, 0.24, 0.04), pl)       # the padded crossbar
            piv = V((fx - 0.06, 0, dz + 0.06))
            b.seg((fx - 0.06, -0.11, dz + 0.06), (fx - 0.06, 0.11, dz + 0.06), 0.055, 0.055,
                  mat((0.70, 0.60, 0.42), rough=0.9), n=8)
            a = math.radians(self.engine_angle())
            d = V((math.sin(a), 0, math.cos(a)))
            tip = piv + d * 0.50
            b.seg(piv, tip, 0.028, 0.022, dark, n=6)
            b.ball(tip, 0.055, dark, scale=(1, 1, 0.6), n=8)
            i = round(self.t * 5) if self.anim == "attack" else -1
            if i in (0, 1, 2) or self.anim != "attack":
                b.ball(tip + V((0, 0, 0.02)) - d * 0.0, 0.035, mat("rock", rough=0.9), n=6)
            if i == 3:                      # the stone leaving, high and forward
                b.ball(tip + V((0.16, 0, 0.14)), 0.045, mat("rock", rough=0.9), n=6)
            # a winch at the back of the frame
            b.seg((fx - 0.17, -0.09, dz + 0.06), (fx - 0.17, 0.09, dz + 0.06), 0.03, 0.03, wood, n=6)
        if not self.st["crew"]:
            return
        # the engine's crew: one at the winch, one at the trigger; helmsman
        skin, hair = _people(101)
        i = round(self.t * 5) if self.anim == "attack" else -1
        xw = fx - 0.24
        crank = 0.1 * math.sin(2 * math.pi * self.t) if i in (0, 1, 2) else 0
        person(b, self.M(xw) @ T(xw, 0.12, self.deck(xw)), skin, hair, pl,
               hands=((0.22, 0.05, 0.5 + crank), (0.22, -0.15, 0.5 - crank)), lean=18, helmet=pm(player, 0.82))
        skin, hair = _people(102)
        xt = fx - 0.14
        pull = 0.1 if i == 3 else 0.0
        person(b, self.M(xt) @ T(xt, -0.18, self.deck(xt)) @ R("Z", 40), skin, hair, pm(player, 0.82),
               hands=((0.2, 0.1, 0.5), (0.25 - pull, -0.1, 0.45)), lean=12, helmet=pm(player, 0.82))
        skin, hair = _people(103)
        xh = -0.84 * HL
        person(b, self.M(xh) @ T(xh, 0.0, self.deck(xh)), skin, hair, mat("linen", rough=0.9),
               hands=((0.05, 0.25, 0.5), (0.05, -0.25, 0.5)), helmet="cap")

    def attack_state(self, t):
        # the throw jars the hull: it rocks back as the arm hits the crossbar
        kick = math.exp(-((t - 0.65) / 0.15) ** 2)
        return dict(z=0.004 * math.sin(2 * math.pi * t), pitch=-2.5 * kick + 0.2, roll=0.6 * kick)

    def attack_water(self, foam, t):
        i = round(t * 5)
        if i in (3, 4):           # the bow slaps down: a spray round it
            a = 0.6 if i == 3 else 0.3
            for s in (1, -1):
                foam.blob((self.HL * 0.8, s * (self.W * 0.6 + 0.05)), 0.12, 0.06, a, n=10)


# ------------------------------------------------------ the fish, the relic

def _soft_obj(sheet, m, z, parent):
    return sheet.obj("soft", m, z, parent)


def fish(loc=(0, 0, 0), rot_z=0.0, parent=None, player=None):
    """A fishing spot about 0.9 across: dark backs of a shoal just under
    the surface, rings of ripples, silver fish leaping and glints of
    spray; soft alpha, so the drawn water shows through."""
    top = lib.empty("fish", loc, rot_z, parent)
    r = lib.empty("fish_spot", (0, 0, 0), 0.0, top)
    r.scale = (1.55, 1.55, 1.3)          # most of a tile: easy to find from afar
    rnd = random.Random(17)
    shade, foam = Sheet(), Sheet()
    # the shoal under the surface: dark, then silver backs
    shade.blob((0.0, 0.0), 0.42, 0.34, 0.20, n=16, soft=0.8)
    backs = Sheet()
    for k in range(9):
        a = rnd.uniform(0, 2 * math.pi)
        d = rnd.uniform(0.05, 0.3)
        c = (d * math.cos(a), d * math.sin(a) * 0.85)
        ang = rnd.uniform(0, math.pi)
        backs.blob(c, 0.075, 0.022, 0.55, n=10, rot=ang, soft=0.5)
    # rings where fish broke the surface
    for (cx, cy, rr, a) in ((0.12, -0.10, 0.13, 0.55), (-0.18, 0.08, 0.10, 0.5), (0.02, 0.18, 0.18, 0.35),
                            (-0.05, -0.22, 0.08, 0.5), (0.25, 0.12, 0.07, 0.45)):
        loop = [(cx + rr * math.cos(2 * math.pi * k / 18), cy + rr * 0.8 * math.sin(2 * math.pi * k / 18))
                for k in range(18)]
        ring(foam, loop, loop_normals(loop), 0.0, 0.025,
             lambda i, a=a, cx=cx: a * (0.5 + 0.6 * _waves(i * 0.3, cx, 21, 5.4)))
        foam.blob((cx, cy), rr * 0.4, rr * 0.3, a * 0.5, n=8)
    shade.obj("fish_shade", _soft_material("fish_shade", (0.02, 0.08, 0.14), 0.0), 0.002, r)
    backs.obj("fish_backs", _soft_material("fish_backs", (0.70, 0.78, 0.84), 1.0), 0.004, r)
    foam.obj("fish_foam", foam_mat(), 0.006, r)
    # leaping fish: silver bodies, a tail, arched over the water
    silver = mat((0.84, 0.88, 0.92), rough=0.22, metal=0.55)
    dark = mat((0.30, 0.40, 0.48), rough=0.4, metal=0.3)
    b = B("fish")
    for (cx, cy, h, yaw, pitch) in ((0.12, -0.10, 0.12, 30, -25), (-0.18, 0.08, 0.07, 150, 30),
                                    (0.25, 0.12, 0.04, -60, 50)):
        with b.at(T(cx, cy, h) @ R("Z", yaw) @ R("Y", pitch), True):
            b.ball((0, 0, 0), 0.05, silver, scale=(1.6, 0.45, 0.6), n=8)
            b.ball((0.01, 0, 0.012), 0.045, dark, scale=(1.45, 0.3, 0.4), n=8)
            b.plate([(-0.06, 0.0), (-0.11, 0.04), (-0.10, 0.0), (-0.11, -0.04)], 0.008, silver,
                    frame=R("X", 90))
            b.ball((0.06, -0.013, 0.008), 0.008, mat((0.05, 0.05, 0.06)), n=4)
    # droplets of spray
    for k in range(10):
        a = rnd.uniform(0, 2 * math.pi)
        c = (0.12 + 0.07 * math.cos(a), -0.10 + 0.05 * math.sin(a))
        b.ball((c[0], c[1], rnd.uniform(0.02, 0.09)), 0.008, mat(FOAM, rough=0.2, emit=0.5), n=4)
    b.finish(r)
    return top


fish.no_ground = True


def relic(loc=(0, 0, 0), rot_z=0.0, parent=None, player=None):
    """A golden reliquary on a small stone plinth: a gabled gold chest,
    jewelled, with winged finials and carrying poles, glinting."""
    r = lib.empty("relic", loc, rot_z, parent)
    stone = mat((0.86, 0.82, 0.74), rough=0.85)
    stone_dk = mat((0.70, 0.66, 0.58), rough=0.9)
    g = mat((1.0, 0.76, 0.22), rough=0.22, metal=1.0, emit=0.25)
    g2 = mat((0.95, 0.66, 0.16), rough=0.3, metal=1.0, emit=0.15)
    red = mat((0.80, 0.08, 0.12), rough=0.2, emit=0.3)
    blue = mat((0.10, 0.30, 0.85), rough=0.2, emit=0.3)
    wood = mat("wood_dark", rough=0.8)
    lib.box((0.56, 0.48, 0.08), (0, 0, 0.04), stone_dk, parent=r, bevel=0.01)
    lib.box((0.48, 0.40, 0.10), (0, 0, 0.13), stone, parent=r, bevel=0.01)
    # carrying poles
    for y in (-0.15, 0.15):
        lib.cyl(0.018, 0.78, (0, y, 0.21), wood, rot=(0, math.pi / 2, 0), parent=r, segs=8)
        for x in (-0.39, 0.39):
            lib.sphere(0.024, (x, y, 0.21), g, parent=r, segs=8)
    # the chest: body, a band of jewels, a gabled lid with a ridge, finials
    lib.box((0.36, 0.24, 0.20), (0, 0, 0.29), g, parent=r, bevel=0.012)
    lib.box((0.37, 0.25, 0.035), (0, 0, 0.24), g2, parent=r)
    lib.box((0.37, 0.25, 0.03), (0, 0, 0.375), g2, parent=r)
    for k in range(5):
        x = -0.14 + 0.07 * k
        lib.sphere(0.018, (x, -0.125, 0.31), red if k % 2 == 0 else blue, parent=r, segs=8)
        lib.sphere(0.018, (0.18 + 0.0 * k, -0.08 + 0.04 * k, 0.31), blue if k % 2 == 0 else red, parent=r,
                   segs=8) if k < 5 else None
    lib.prism((0.36, 0.24, 0.12), (0, 0, 0.39), g, parent=r, overhang=0.015)
    lib.cyl(0.016, 0.38, (0, 0, 0.51), g2, rot=(0, math.pi / 2, 0), parent=r, segs=8)
    for x in (-0.19, 0.19):
        lib.sphere(0.035, (x, 0, 0.53), g, parent=r, segs=10)
    # the gold cross/ankh-like finial: a sunburst disc on top
    lib.cyl(0.07, 0.015, (0, 0, 0.60), g, rot=(math.pi / 2, 0, -math.pi / 4), parent=r, segs=16)
    lib.sphere(0.03, (0, 0, 0.60), red, parent=r, segs=8)
    lib.cyl(0.012, 0.06, (0, 0, 0.54), g, parent=r, segs=6)
    # the glint: small bright stars on the lit edges
    star = mat((1.0, 0.98, 0.85), rough=0.1, emit=4.0)
    for (x, y, z, s) in ((-0.18, -0.12, 0.40, 0.05), (0.0, 0.0, 0.68, 0.06), (0.19, -0.12, 0.22, 0.04)):
        for ang in (0, 90):
            lib.box((s, 0.006, 0.006), (x, y, z), star, rot=(0, math.radians(ang), math.radians(-45)),
                    parent=r)
    return r


# ------------------------------------------------------------ the sets

def _ship(cls):
    return units._unit(cls)


fishing_boat = _ship(FishingBoat)
trade_boat = _ship(TradeBoat)
transport = _ship(Transport)
archer_ship = _ship(ArcherShip)
war_galley = _ship(WarGalley)
catapult_ship = _ship(CatapultShip)

SETS = {
    name: dict(func="units_naval." + name, cls="Ship", scale=1)
    for name in ("fishing_boat", "trade_boat", "transport", "archer_ship", "war_galley",
                 "catapult_ship")
}
SETS.update({
    "fish": dict(func="units_naval.fish", cls="SmallBuilding", tiles=1, stages=False, sun_angle=8),
    "relic": dict(func="units_naval.relic", cls="SmallBuilding", tiles=1, stages=False, sun_angle=8),
})


# ------------------------------------------------------------ preview

def water_bg(w, h, seed=0):
    """A piece of the game's deep-water texture, as the ground draws it at 1x."""
    import numpy as np
    from PIL import Image
    if os.environ.get("NAVAL_BG"):      # a picture of the game's water (mapview --look 1)
        bg = np.asarray(Image.open(os.environ["NAVAL_BG"]).convert("RGB")).astype(float)
        ys, xs = np.mgrid[0:h, 0:w]
        return bg[(ys + seed * 37) % 400 + 50, (xs + seed * 53) % 700 + 50]
    tex = np.asarray(Image.open(os.path.join(os.path.dirname(os.path.dirname(HERE)), "assets", "look",
                                             "terrain", "deep.png")).convert("RGB")).astype(float)
    # the texture is 128 px a tile at 2x: at 1x half that, and seen squashed 2:1
    small = np.asarray(Image.fromarray(tex.astype("uint8")).resize((256, 128), Image.BILINEAR)).astype(float)
    ys, xs = np.mgrid[0:h, 0:w]
    return small[(ys + seed * 37) % 128, (xs + seed * 53) % 256]


def tint_cell(rgba, m, base):
    import numpy as np
    rgb = rgba[..., :3].astype(float)
    l = (rgb @ np.array([0.299, 0.587, 0.114])) / 255.0
    t = np.clip(l, 0, 1) * 7
    rmp = np.array([np.clip(np.array(base, float) * (0.30 + 0.95 * i / 7), 0, 255) for i in range(8)])
    i0 = np.floor(t).astype(int).clip(0, 7)
    i1 = np.minimum(i0 + 1, 7)
    f = (t - i0)[..., None]
    tn = rmp[i0] * (1 - f) + rmp[i1] * f
    mm = (m / 255.0)[..., None]
    return rgb * (1 - mm) + tn * mm


BLUE_BASE, RED_BASE = (40, 90, 230), (220, 40, 30)


def over_water(rgba, m, base, seed=0):
    import numpy as np
    h, w = rgba.shape[:2]
    bg = water_bg(w, h, seed)
    al = rgba[..., 3:4] / 255.0
    return (tint_cell(rgba, m, base) * al + bg * (1 - al)).clip(0, 255).astype(np.uint8)


def sheet_over_water(path, grid, names, frame, anchor, zoom=2):
    """A contact sheet: each row a (animation, facing), blue and red rows
    alternating, over the deep water, at `zoom`."""
    from PIL import Image, ImageDraw
    w, h = frame
    ncols = max(len(r) for r in grid)
    lw = 90
    out = Image.new("RGB", (lw + w * ncols * zoom, h * len(grid) * zoom), (30, 30, 30))
    d = ImageDraw.Draw(out)
    for j, row in enumerate(grid):
        base = BLUE_BASE if j % 2 == 0 else RED_BASE
        for i, (rgba, m) in enumerate(row):
            cell = over_water(rgba, m, base, seed=i + j)
            im = Image.fromarray(cell).resize((w * zoom, h * zoom), Image.NEAREST)
            out.paste(im, (lw + i * w * zoom, j * h * zoom))
        d.text((4, j * h * zoom + 4), names[j], fill=(255, 255, 255))
    out.save(path)


def preview(name, out, only=None, facings=None, samples=12):
    """Some animations of a ship through the exporter's scene, no assets
    written: /usr/bin/python3.13 tools/look/units_naval.py war_galley OUT.png [walk] [S,W]"""
    import shutil
    import tempfile
    import export
    import unitexport
    unitexport.set_scale(1)
    tmp = tempfile.mkdtemp(prefix="look-ship-")
    stage = unitexport.Stage(samples, tmp, water=True)
    fig = globals()[name]()
    fig.animations = [a for a in fig.animations if not only or a[0] in only]
    frame, anchor = unitexport.bounds(stage, fig)
    export.camera(frame, anchor)
    grid, names = [], []
    for a, n, *_ in fig.animations:
        for f in (facings or unitexport.FACING_ORDER):
            row = []
            for i in range(n):
                stage.place(fig, a, i)
                stage.turn(export.FACINGS[f])
                stage.border(frame, anchor)
                beauty, mask = stage.render("p")
                row.append(export.finish(beauty, mask))
            grid.append(row)
            names.append("%s %s" % (a, f))
    sheet_over_water(out, grid, names, frame, anchor)
    shutil.rmtree(tmp, ignore_errors=True)
    print("wrote", out, "frame", frame, "anchor", anchor)


if __name__ == "__main__":
    preview(sys.argv[1], sys.argv[2], sys.argv[3].split(",") if len(sys.argv) > 3 and sys.argv[3] else None,
            sys.argv[4].split(",") if len(sys.argv) > 4 else None)

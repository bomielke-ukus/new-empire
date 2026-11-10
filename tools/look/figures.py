"""People, animals, vehicles and a ship for the new look (painted miniatures).

Every public function takes loc=(x, y, 0), facing (radians about Z, 0 = looking
along +X), player (sRGB tuple), usually pose="stand"|"walk"|"attack", seed and
parent, builds ONE mesh object (many material slots) under a root empty and
returns the root. Scale the root to resize a figure.

    /usr/bin/python3.13 tools/look/preview.py figures.showcase out.png 14 1400
"""
import math
import random

import bpy
import bmesh
from mathutils import Matrix, Vector

from lib import BLUE, PAL, RED, empty, mat

V = Vector
I4 = Matrix.Identity(4)
COUNTS = {}  # name -> faces of the last one built (for reporting)
VIEW = -math.pi / 4  # facing that looks at the camera


def R(axis, deg):
    return Matrix.Rotation(math.radians(deg), 4, axis)


def T(*p):
    return Matrix.Translation(p[0] if len(p) == 1 else p)


def S(x, y, z):
    return Matrix.Diagonal((x, y, z, 1.0))


def nv(v):
    return V(v).normalized()


def _frame(z, hint=(0, 1, 0)):
    """A rotation whose Z is `z` and whose Y is as near `hint` as it can be."""
    z = V(z).normalized()
    h = V(hint)
    y = h - z * h.dot(z)
    if y.length < 1e-4:
        h = V((1, 0, 0))
        y = h - z * h.dot(z)
    y.normalize()
    x = y.cross(z)
    return Matrix(((x.x, y.x, z.x, 0), (x.y, y.y, z.y, 0), (x.z, y.z, z.z, 0), (0, 0, 0, 1)))


def _mix(a, b, t):
    a = PAL[a] if isinstance(a, str) else a
    b = PAL[b] if isinstance(b, str) else b
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def _shade(c, f):
    c = PAL[c] if isinstance(c, str) else c
    return tuple(max(0.0, min(1.0, v * f)) for v in c)


def bronze():
    return mat((0.90, 0.64, 0.24), rough=0.38, metal=0.45)


def gold():
    return mat((1.0, 0.78, 0.22), rough=0.35, metal=0.35)


# --- the builder: one mesh, many materials -------------------------------

class _At:
    def __init__(self, b, M):
        self.b, self.M = b, M

    def __enter__(self):
        self.old = self.b.M
        self.b.M = self.M

    def __exit__(self, *a):
        self.b.M = self.old


class B:
    def __init__(self, name):
        self.name = name
        self.bm = bmesh.new()
        self.mats = []
        self.M = I4.copy()

    def at(self, M, absolute=False):
        return _At(self, M if absolute else self.M @ M)

    def mi(self, m):
        m = m if isinstance(m, bpy.types.Material) else mat(m)
        if m not in self.mats:
            self.mats.append(m)
        return self.mats.index(m)

    def _fin(self, verts, m, smooth, L=None):
        M = self.M @ L if L is not None else self.M
        idx = self.mi(m)
        faces = set()
        for v in verts:
            v.co = M @ v.co
            faces.update(v.link_faces)
        for f in faces:
            f.material_index = idx
            f.smooth = smooth

    def seg(self, a, b, r0, r1=None, m="wood", n=8, sy=1.0, hint=(0, 1, 0)):
        """A (tapered, maybe flattened) rod from a to b."""
        a, b = V(a), V(b)
        d = b - a
        if d.length < 1e-6:
            return
        r1 = r0 if r1 is None else r1
        res = bmesh.ops.create_cone(self.bm, cap_ends=True, segments=n, radius1=r0,
                                    radius2=max(r1, 1e-4), depth=d.length)
        self._fin(res["verts"], m, True, T((a + b) / 2) @ _frame(d, hint) @ S(1, sy, 1))

    def chain(self, pts, r0, r1=None, m="wood", n=6, sy=1.0, hint=(0, 1, 0)):
        r1 = r0 if r1 is None else r1
        k = len(pts) - 1
        for i in range(k):
            self.seg(pts[i], pts[i + 1], r0 + (r1 - r0) * i / k, r0 + (r1 - r0) * (i + 1) / k,
                     m, n, sy, hint)

    def ball(self, c, r, m="rock", scale=(1, 1, 1), n=10, rot=None):
        res = bmesh.ops.create_uvsphere(self.bm, u_segments=n, v_segments=max(5, (n * 2) // 3),
                                        radius=r)
        self._fin(res["verts"], m, True, T(V(c)) @ (rot or I4) @ S(*scale))

    def cube(self, c, size, m="wood", rot=None):
        res = bmesh.ops.create_cube(self.bm, size=1.0)
        self._fin(res["verts"], m, False, T(V(c)) @ (rot or I4) @ S(*size))

    def loft(self, rings, m, n=10, frame=None, caps=True, smooth=True):
        """Stacked ellipses (cx, cy, cz, rx, ry) joined into a skin. `m` is one
        material or one per band."""
        M = self.M @ frame if frame is not None else self.M
        ms = m if isinstance(m, (list, tuple)) and not (len(m) == 3 and isinstance(m[0], float)) else [m]
        if isinstance(m, tuple) and len(m) == 3 and isinstance(m[0], float):
            ms = [m]
        rows = []
        for (cx, cy, cz, rx, ry) in rings:
            rows.append([self.bm.verts.new(M @ V((cx + rx * math.cos(2 * math.pi * k / n),
                                                  cy + ry * math.sin(2 * math.pi * k / n), cz)))
                         for k in range(n)])
        for i in range(len(rows) - 1):
            idx = self.mi(ms[min(i, len(ms) - 1)])
            for k in range(n):
                f = self.bm.faces.new((rows[i][k], rows[i][(k + 1) % n],
                                       rows[i + 1][(k + 1) % n], rows[i + 1][k]))
                f.material_index = idx
                f.smooth = smooth
        if caps:
            f = self.bm.faces.new(list(reversed(rows[0])))
            f.material_index = self.mi(ms[0])
            f = self.bm.faces.new(rows[-1])
            f.material_index = self.mi(ms[min(len(rows) - 2, len(ms) - 1)])

    def plate(self, pts, thick, m, frame=None):
        """A flat outline (2D points in the frame's XY) given thickness along Z."""
        M = self.M @ frame if frame is not None else self.M
        h = thick / 2
        top = [self.bm.verts.new(M @ V((x, y, h))) for x, y in pts]
        bot = [self.bm.verts.new(M @ V((x, y, -h))) for x, y in pts]
        idx = self.mi(m)
        n = len(pts)
        fs = [self.bm.faces.new(top), self.bm.faces.new(list(reversed(bot)))]
        for k in range(n):
            fs.append(self.bm.faces.new((top[(k + 1) % n], top[k], bot[k], bot[(k + 1) % n])))
        for f in fs:
            f.material_index = idx

    def poly(self, verts, faces, m, smooth=False):
        vs = [self.bm.verts.new(self.M @ V(p)) for p in verts]
        idx = self.mi(m)
        for f in faces:
            fc = self.bm.faces.new([vs[i] for i in f])
            fc.material_index = idx
            fc.smooth = smooth

    def finish(self, root):
        bmesh.ops.recalc_face_normals(self.bm, faces=self.bm.faces)
        me = bpy.data.meshes.new(self.name)
        COUNTS[self.name] = len(self.bm.faces)
        self.bm.to_mesh(me)
        self.bm.free()
        me.set_sharp_from_angle(angle=math.radians(52))
        for m in self.mats:
            me.materials.append(m)
        ob = bpy.data.objects.new(self.name, me)
        bpy.context.scene.collection.objects.link(ob)
        ob.parent = root
        return ob


def _start(name, loc, facing, parent):
    return empty(name, loc, facing, parent), B(name)


# --- the body ------------------------------------------------------------

LT, LS, FH = 0.20, 0.19, 0.04   # thigh, shin, ankle height
UA, FA = 0.16, 0.15             # upper arm, forearm
STAND = dict(legL=(7, 3, 3), legR=(-6, 3, 3), lean=0.0, twist=0.0, head=(0, 0), hipz=None,
             armL=((0.10, 0.16, -1), (0.35, 0.02, -1)),
             armR=((0.02, -0.16, -1), (0.30, -0.02, -1)))
WALK = dict(legL=(30, 8, 3), legR=(-24, 28, 3), lean=5,
            armL=((-0.35, 0.14, -1), (-0.05, 0.05, -1)),
            armR=((0.40, -0.14, -1), (0.9, 0.0, -0.6)))


def _pose(*ds, **kw):
    p = dict(STAND)
    for d in ds:
        p.update(d)
    p.update(kw)
    return p


class Body:
    """Joint positions from a pose; then draw the parts wanted. Frames:
    base (feet on the ground, X forward), P (pelvis), U (upper body, leaned
    and twisted, origin at the hips), H (head centre)."""

    def __init__(self, b, pose, skin, M=None):
        self.b, self.skin, self.p = b, skin, pose
        p = pose
        self.base = b.M @ (M if M is not None else I4)
        self.leg = {}
        for s, k in ((1, "L"), (-1, "R")):
            th, kn, sp = p["leg" + k]
            a, a2, spr = math.radians(th), math.radians(th - kn), math.radians(sp) * s
            hip = V((0, s * 0.062, 0))
            knee = hip + V((math.sin(a) * math.cos(spr), math.sin(spr), -math.cos(a) * math.cos(spr))) * LT
            ank = knee + V((math.sin(a2), math.sin(spr) * 0.5, -math.cos(a2))).normalized() * LS
            self.leg[k] = (hip, knee, ank, th - kn)
        hz = p["hipz"]
        if hz is None:
            hz = max(-l[2].z for l in self.leg.values()) + FH
        self.hipz = hz
        self.P = self.base @ T(0, 0, hz)
        self.U = self.P @ R("Y", p["lean"]) @ R("Z", p["twist"])
        yaw, pitch = p["head"]
        self.H = self.U @ T(0.012, 0, 0.455) @ R("Z", yaw) @ R("Y", pitch)
        self.sh, self.el, self.hand, self.fad = {}, {}, {}, {}
        for s, k in ((1, "L"), (-1, "R")):
            ua, fa = p["arm" + k]
            self.sh[k] = V((0, s * 0.15, 0.262))
            self.el[k] = self.sh[k] + nv(ua) * UA
            self.fad[k] = nv(fa)
            self.hand[k] = self.el[k] + self.fad[k] * FA
        self.toF = self.base.inverted() @ self.U  # U coords -> figure coords

    def pt(self, k):
        """Hand position in the figure frame."""
        return self.toF @ self.hand[k]

    def fpt(self, p):
        return self.toF @ V(p)

    def fdir(self, d):
        return (self.toF.to_3x3() @ V(d)).normalized()

    def fig(self):
        return self.b.at(self.base, True)

    # -- parts --
    def legs(self, thigh=None, shin=None, shin_r=1.0, foot=None):
        b, sk = self.b, self.skin
        with b.at(self.P, True):
            for k in "LR":
                hip, knee, ank, a2 = self.leg[k]
                b.seg(hip, knee, 0.062, 0.049, thigh or sk)
                b.ball(knee, 0.049, thigh or sk, n=8)
                b.seg(knee, ank, 0.047 * shin_r, 0.035 * shin_r, shin or sk)
                pit = -a2 * 0.6
                b.ball(ank + V((0.03, 0, -0.008)), 0.04, foot or sk, scale=(1.9, 1.05, 0.8), n=8,
                       rot=R("Y", pit))

    def torso(self, m=None, scale=1.0):
        m = m or self.skin
        s = scale
        rings = [(0, 0, 0.0, .078 * s, .108 * s), (0, 0, 0.09, .072 * s, .098 * s),
                 (0.005, 0, 0.19, .088 * s, .128 * s), (0.0, 0, 0.262, .082 * s, .145 * s),
                 (0, 0, 0.30, .05 * s, .095 * s)]
        with self.b.at(self.U, True):
            self.b.loft(rings, m, n=10)
            self.b.seg((0, 0, 0.28), (0.006, 0, 0.36), 0.042, 0.04, self.skin)

    def arms(self, upper=None, fore=None, sleeve=None, sleeve_f=0.5, hand=None):
        b, sk = self.b, self.skin
        with b.at(self.U, True):
            for k in "LR":
                sh, el, ha = self.sh[k], self.el[k], self.hand[k]
                b.ball(sh, 0.05, sleeve or upper or sk, n=8)
                b.seg(sh, el, 0.044, 0.037, upper or sk)
                if sleeve:
                    b.seg(sh, sh + (el - sh) * sleeve_f, 0.052, 0.05, sleeve)
                b.ball(el, 0.037, upper or sk, n=8)
                b.seg(el, ha, 0.036, 0.03, fore or sk)
                b.ball(ha, 0.043, hand or sk, n=8)

    def skirt(self, mats, top=0.075, hem=-0.13, flare=1.25, w=(.084, .114), zs=None):
        if not isinstance(mats, list):
            mats = [mats]
        kx = [self.leg[k][1].x for k in "LR"]
        spread, cx = max(abs(kx[0]), abs(kx[1])), (kx[0] + kx[1]) / 2
        f = min(1.3, (top - hem) / LT)
        zs = zs or [i / len(mats) for i in range(len(mats) + 1)]
        rings = []
        for t in zs:
            rings.append((cx * t * f * 0.8, 0, top + (hem - top) * t,
                          w[0] * (1 + (flare - 1) * t) + spread * f * t * 0.75,
                          w[1] * (1 + (flare - 1) * t * 0.6)))
        with self.b.at(self.P, True):
            self.b.loft(rings, mats, n=10)

    def head(self, hair="hair_black", style="short", beard=False, band=None, face=True):
        b, sk = self.b, self.skin
        with b.at(self.H, True):
            b.ball((0, 0, 0), 0.115, sk, scale=(1, 0.94, 1.03), n=12)
            if face:
                b.ball((0.113, 0, -0.018), 0.026, sk, n=6)
                dark = mat((0.07, 0.05, 0.05))
                for s in (1, -1):
                    b.ball((0.096, s * 0.045, 0.012), 0.025, dark, scale=(0.55, 1, 0.9), n=6)
                b.cube((0.094, 0, 0.05), (0.035, 0.14, 0.012), hair if style != "bald" else _shade(sk.diffuse_color[:3] if hasattr(sk, "diffuse_color") else (0.4, 0.3, 0.2), 0.5))
            if style == "short":
                b.ball((-0.026, 0, 0.026), 0.124, hair, n=10)
            elif style == "wig":
                b.ball((-0.02, 0, 0.03), 0.132, hair, scale=(1, 1.04, 1), n=10)
                b.loft([(-0.05, 0, 0.02, .105, .14), (-0.055, 0, -0.12, .10, .145)], hair, n=10)
            elif style == "long":
                b.ball((-0.026, 0, 0.026), 0.126, hair, n=10)
                b.loft([(-0.06, 0, 0.0, .09, .12), (-0.075, 0, -0.2, .07, .11)], hair, n=8)
            if beard:
                b.ball((0.05, 0, -0.075), 0.072, hair, scale=(1, 0.95, 0.9), n=8)
            if band:
                b.loft([(-0.016, 0, 0.058, .130, .127), (-0.018, 0, 0.09, .122, .12)], band, n=12,
                       caps=False)


def _people(seed, egypt=False):
    """(rnd, skin material, hair colour) for a seed."""
    rnd = random.Random(seed * 7919 + (13 if egypt else 0))
    if egypt:
        skin = _mix("skin_tan", "skin_dark", rnd.uniform(0.0, 0.45))
        hair = "hair_black"
    else:
        skin = _mix("skin_light", "skin_tan", rnd.uniform(0.0, 0.7))
        hair = rnd.choice(["hair_black", "hair_brown", "hair_brown", (0.45, 0.30, 0.14)])
    return rnd, mat(skin, rough=0.75), hair


def _vary(p, rnd, amt=1.0):
    p = dict(p)
    p["lean"] = p["lean"] + rnd.uniform(-3, 3) * amt
    y, pt = p["head"]
    p["head"] = (y + rnd.uniform(-14, 14) * amt, pt + rnd.uniform(-4, 4) * amt)
    return p


# --- props ---------------------------------------------------------------

def _spear(b, grip, d, L=1.5, back=0.5, m="wood", r=0.018, head=0.17, butt=True):
    d = nv(d)
    grip = V(grip)
    a, e = grip - d * back, grip + d * (L - back)
    b.seg(a, e, r, r, m, n=6)
    b.seg(e - d * 0.01, e + d * head * 0.45, 0.014, 0.04, bronze(), n=6, sy=0.45)
    b.seg(e + d * head * 0.45, e + d * head, 0.04, 0.002, bronze(), n=6, sy=0.45)
    if butt:
        b.seg(a, a - d * 0.06, 0.016, 0.004, bronze(), n=6)
    return e + d * head


def _arrow(b, tail, d, L=0.42):
    d = nv(d)
    tail = V(tail)
    b.seg(tail, tail + d * L, 0.008, 0.008, "straw", n=5)
    b.seg(tail + d * L, tail + d * (L + 0.05), 0.016, 0.002, "iron", n=5)
    b.seg(tail, tail + d * 0.07, 0.02, 0.014, "white", n=5, sy=0.3)


def _bow(b, grip, aim, nock=None, h=0.31, composite=False, m="wood", arrow=True):
    """A bow held at `grip`, its belly toward -aim; drawn if `nock` is given."""
    aim = nv(aim)
    grip = V(grip)
    up = V((0, 0, 1)) - aim * aim.z
    up.normalize()
    c = 0.17 if nock is not None else 0.10
    pts = []
    for i in range(9):
        t = -1 + i / 4
        off = -c * t * t
        if composite:
            off += 0.9 * max(0.0, abs(t) - 0.6) ** 2 + 0.03 * (1 - abs(t)) * (1 if abs(t) < 0.3 else 0)
        pts.append(grip + up * (t * h) + aim * off)
    b.chain(pts[:5], 0.010, 0.019, m, n=6)
    b.chain(pts[4:], 0.019, 0.010, m, n=6)
    tipm = gold() if composite else "wood_dark"
    b.ball(pts[0], 0.016, tipm, n=6)
    b.ball(pts[-1], 0.016, tipm, n=6)
    sm = mat((0.95, 0.92, 0.82))
    if nock is not None:
        nock = V(nock)
        b.seg(pts[0], nock, 0.0045, 0.0045, sm, n=4)
        b.seg(pts[-1], nock, 0.0045, 0.0045, sm, n=4)
        if arrow:
            _arrow(b, nock, (grip + aim * 0.02) - nock, L=(grip - nock).length + 0.06)
    else:
        b.seg(pts[0], pts[-1], 0.0045, 0.0045, sm, n=4)


def _quiver(b, top, bottom, m="leather", band=None):
    top, bottom = V(top), V(bottom)
    d = nv(top - bottom)
    b.seg(bottom, top, 0.036, 0.046, m, n=8)
    if band:
        b.seg(bottom + (top - bottom) * 0.62, bottom + (top - bottom) * 0.8, 0.049, 0.05, band, n=8)
    for i, c in enumerate(("white", "flower_red", "white")):
        o = V(((i - 1) * 0.02, (i % 2) * 0.02 - 0.01, 0))
        b.seg(top + o, top + o + d * 0.07, 0.014, 0.016, c, n=5, sy=0.4)


def _aspis(b, c, n, player, blazon="lambda", Rr=0.235, back="wood_dark"):
    """The hoplite's round shield, centred at c facing along n."""
    F = T(V(c)) @ _frame(n, (0, 0, 1))
    h = 0.075
    rim = Rr + 0.032
    with b.at(F):
        rings = [(0, 0, -0.022, rim * 0.97, rim * 0.97), (0, 0, 0.0, rim, rim), (0, 0, 0.006, Rr, Rr)]
        for ph in (24, 46, 66, 82):
            a = math.radians(ph)
            rings.append((0, 0, 0.006 + h * math.sin(a), Rr * math.cos(a), Rr * math.cos(a)))
        pm = mat(player, rough=0.6)
        b.loft(rings, [bronze(), bronze(), pm, pm, pm, pm], n=14)
        b.seg((0, 0, -0.024), (0, 0, -0.022), rim * 0.9, rim * 0.9, back, n=14)

        def zf(r):
            return 0.006 + h * math.sqrt(max(0.0, 1 - (r / Rr) ** 2)) + 0.006

        def stroke(p0, p1, w, m):
            p0, p1 = V(p0), V(p1)
            d = (p1 - p0).normalized()
            q = V((-d.y, d.x)) * w / 2
            vs, fs = [], []
            for i in range(6):
                p = p0 + (p1 - p0) * i / 5
                for sgn in (1, -1):
                    pp = p + q * sgn
                    vs.append((pp.x, pp.y, zf(pp.length)))
                if i:
                    k = len(vs)
                    fs.append((k - 4, k - 3, k - 1, k - 2))
            b.poly(vs, fs, m)

        wm = mat("white", rough=0.6)
        if blazon == "lambda":
            for s in (1, -1):
                stroke((0, 0.60 * Rr), (s * 0.5 * Rr, -0.55 * Rr), 0.20 * Rr, wm)
        elif blazon == "star":
            gm = mat((1.0, 0.86, 0.3), rough=0.5)
            for k in range(4):
                a = math.radians(45 * k)
                d = V((math.cos(a), math.sin(a)))
                stroke(d * -0.72 * Rr, d * 0.72 * Rr, 0.15 * Rr, gm)
        else:  # the gorgon dot: a white boss and ring
            b.ball((0, 0, h * 0.9), Rr * 0.36, wm, scale=(1, 1, 0.3), n=10)
            for s in (1, -1):
                b.ball((s * Rr * 0.13, Rr * 0.06, h + 0.012), 0.022, "black", scale=(1, 1, 0.4), n=6)


def _eshield(b, c, n, player, seed=0, w=0.31, hgt=0.58):
    """The Egyptian shield: cowhide on a board, round-topped."""
    F = T(V(c)) @ _frame(n, (0, 0, 1))
    rnd = random.Random(seed + 5)
    out = [(-w / 2, -hgt / 2), (w / 2, -hgt / 2)]
    cy = hgt / 2 - w / 2
    for i in range(9):
        a = math.pi * i / 8
        out.append((w / 2 * math.cos(a), cy + w / 2 * math.sin(a)))
    with b.at(F):
        b.plate(out, 0.03, mat((0.97, 0.94, 0.86), rough=0.8))
        b.plate([(x * 1.0, y) for x, y in out], 0.012, "leather", frame=T(0, 0, -0.02))
        dk = mat((0.22, 0.13, 0.08))
        for (bx, by) in ((-0.06, 0.17), (0.07, 0.05), (-0.05, -0.2), (0.08, -0.16), (0.05, 0.22)):
            bx += rnd.uniform(-0.02, 0.02)
            by += rnd.uniform(-0.02, 0.02)
            r0 = rnd.uniform(0.04, 0.062)
            pts = []
            for k in range(7):
                a = 2 * math.pi * k / 7
                rr = r0 * rnd.uniform(0.6, 1.2)
                pts.append((max(-w / 2 + 0.01, min(w / 2 - 0.01, bx + rr * math.cos(a))), by + rr * math.sin(a)))
            b.plate(pts, 0.004, dk, frame=T(0, 0, 0.016))
        yb = -0.045
        b.plate([(-w / 2 - 0.004, yb - 0.045), (w / 2 + 0.004, yb - 0.045), (w / 2 + 0.004, yb + 0.045),
                 (-w / 2 - 0.004, yb + 0.045)], 0.04, mat(player, rough=0.6))
        b.ball((0, yb, 0.02), 0.03, gold(), scale=(1, 1, 0.5), n=8)


# --- outfits --------------------------------------------------------------

def _dress_villager(bd, player, hair, rnd, job="idle"):
    b = bd.b
    lin = mat(_mix("linen", "wool", rnd.uniform(0, 0.6)), rough=0.9)
    pm = mat(player, rough=0.7)
    bd.legs()
    woman = rnd.random() < 0.34
    if woman:
        lin = mat(rnd.choice([(0.93, 0.80, 0.50), (0.90, 0.66, 0.52), (0.82, 0.84, 0.70)]), rough=0.9)
        bd.skirt([pm, lin, lin, pm], top=0.085, hem=-0.33, zs=[0, 0.12, 0.5, 0.9, 1.0], flare=1.45)
    else:
        bd.skirt([pm, lin, pm], top=0.085, hem=-0.15, zs=[0, 0.22, 0.82, 1.0], flare=1.3)
    bd.torso(lin, 1.04)
    bd.arms(sleeve=lin, sleeve_f=0.45)
    bd.head(hair, "long" if woman else "short", beard=(not woman) and rnd.random() < 0.4, band=pm)
    with b.at(bd.P, True):  # the sash end
        b.cube((0.03, -0.11, -0.03), (0.03, 0.05, 0.13), pm, rot=R("X", -8))


def _dress_hoplite(bd, player, hair, rnd, cloak=False, crest=True):
    b = bd.b
    br, pm = bronze(), mat(player, rough=0.7)
    lin = mat("linen", rough=0.9)
    bd.legs(shin=br, shin_r=1.22, foot="leather")
    bd.skirt([lin, lin, pm], top=0.08, hem=-0.125, zs=[0, 0.3, 0.62, 1.0], flare=1.4)
    bd.torso(br, 1.07)
    bd.arms(sleeve=pm, sleeve_f=0.4)
    bd.head(hair, "none", face=False)
    with b.at(bd.H, True):  # the Corinthian helmet
        b.ball((0.0, 0, 0.012), 0.131, br, scale=(1.03, 0.96, 1.0), n=12)
        b.seg((0.004, 0, 0.02), (0.012, 0, -0.125), 0.131, 0.112, br, n=12, sy=0.94)
        dk = mat((0.04, 0.03, 0.03))
        for s in (1, -1):
            b.ball((0.112, s * 0.05, 0.004), 0.034, dk, scale=(0.6, 1.1, 0.8), n=6)
        b.cube((0.118, 0, -0.075), (0.03, 0.026, 0.10), dk)
        b.cube((0.132, 0, -0.01), (0.02, 0.022, 0.07), br)
        if crest:
            inn, out = [], []
            for i in range(9):
                a = math.radians(-48 + 172 * i / 8)  # from the brow over to the nape
                hh = 0.11 if i < 7 else 0.11 + 0.02 * (i - 6)
                inn.append((0.126 * math.sin(-a) * -1, 0.012 + 0.126 * math.cos(a)))
                out.append(((0.128 + hh) * math.sin(-a) * -1, 0.012 + (0.128 + hh) * math.cos(a)))
            # sin(-a)*-1 = sin(a): a<0 is the front (+X)... flip so the front is +X
            inn = [(-x, z) for x, z in inn]
            out = [(-x, z) for x, z in out]
            b.plate(inn + list(reversed(out)), 0.06, pm, frame=R("X", 90))
            b.plate(inn + [(x * 1.1, 0.012 + (z - 0.012) * 1.1) for x, z in reversed(inn)], 0.068,
                    mat("white", rough=0.7), frame=R("X", 90))
    if cloak:
        _cloak(bd, mat(_shade(player, 0.8), rough=0.8))


def _cloak(bd, m, fly=0.0, length=0.42):
    vs, fs = [], []
    nr, nc = 5, 5
    for i in range(nr):
        t = i / (nr - 1)
        for j in range(nc):
            u = j / (nc - 1) - 0.5
            w = 0.31 + 0.16 * t
            x = -0.085 - 0.05 * t - fly * t * 0.5 - 0.05 * (1 - (2 * u) ** 2) * (0.4 + t) + 0.07 * (2 * u) ** 2
            vs.append((x, u * w, 0.30 - length * t * (1 - fly * 0.45)))
    for i in range(nr - 1):
        for j in range(nc - 1):
            k = i * nc + j
            fs.append((k, k + 1, k + nc + 1, k + nc))
    with bd.b.at(bd.U, True):
        bd.b.poly(vs, fs, m, smooth=True)
        bd.b.ball((0.07, 0.0, 0.275), 0.025, gold(), n=6)
        bd.b.seg((-0.06, -0.15, 0.30), (0.075, 0, 0.275), 0.03, 0.022, m, n=6, sy=0.5)
        bd.b.seg((-0.06, 0.15, 0.30), (0.075, 0, 0.275), 0.03, 0.022, m, n=6, sy=0.5)


def _dress_archer(bd, player, hair, rnd):
    b = bd.b
    pm = mat(player, rough=0.75)
    lin = mat("linen", rough=0.9)
    bd.legs(foot="leather", shin=mat("leather"), shin_r=1.08)
    bd.skirt([mat("leather"), pm, lin], top=0.085, hem=-0.14, zs=[0, 0.2, 0.75, 1.0], flare=1.3)
    bd.torso(pm, 1.04)
    bd.arms(sleeve=pm, sleeve_f=0.45)
    bd.head(hair, "short", beard=rnd.random() < 0.3)
    with b.at(bd.H, True):  # a soft pointed cap
        cm = mat((0.90, 0.80, 0.58), rough=0.9)
        b.ball((-0.02, 0, 0.035), 0.127, cm, n=10)
        b.seg((-0.03, 0, 0.11), (0.05, 0, 0.21), 0.07, 0.03, cm, n=8)
        b.ball((0.055, 0, 0.21), 0.034, cm, n=6)
    with b.at(bd.U, True):
        _quiver(b, (-0.13, -0.12, 0.36), (-0.10, 0.07, 0.02), band=pm)
        b.seg((0.09, 0.1, 0.03), (0.0, -0.13, 0.29), 0.016, 0.016, "leather", n=5, sy=0.4)


def _dress_egyptian(bd, player, hair, rnd, head="nemes"):
    b = bd.b
    pm = mat(player, rough=0.7)
    wh = mat((0.98, 0.96, 0.9), rough=0.85)
    bd.legs()
    bd.skirt([pm, wh, wh], top=0.075, hem=-0.135, zs=[0, 0.2, 0.6, 1.0], flare=1.22)
    with b.at(bd.P, True):  # the stiff front apron
        b.plate([(-0.045, 0.05), (0.045, 0.05), (0.075, -0.14), (-0.075, -0.14)], 0.02, pm,
                frame=T(0.10 + bd.leg["L"][1].x * 0.25, 0, 0) @ R("Z", 90) @ R("X", 90))
    bd.torso()
    bd.arms()
    with b.at(bd.U, True):  # the broad collar
        b.loft([(0, 0, 0.24, .093, .15), (0, 0, 0.275, .086, .15), (0.004, 0, 0.305, .055, .075)],
               [gold(), pm], n=10, caps=False)
        for k in "LR":  # armlets
            p0 = bd.sh[k] + (bd.el[k] - bd.sh[k]) * 0.45
            b.seg(p0, p0 + (bd.el[k] - bd.sh[k]) * 0.2, 0.047, 0.046, gold(), n=8)
    if head == "nemes":
        bd.head(hair, "none")
        rings = [(-0.02, 0, 0.134, .035, .04), (-0.015, 0, 0.105, .095, .095), (-0.03, 0, 0.065, .128, .128),
                 (-0.05, 0, 0.02, .118, .142), (-0.06, 0, -0.03, .10, .155), (-0.055, 0, -0.085, .095, .168),
                 (-0.055, 0, -0.15, .085, .175)]
        with b.at(bd.H, True):
            b.loft(rings, [pm, wh, pm, wh, pm, wh], n=12)
            b.ball((0.115, 0, 0.065), 0.02, gold(), n=6)
    else:
        bd.head(hair, "wig", band=pm)
        with b.at(bd.H, True):
            b.seg((-0.14, 0, 0.06), (-0.2, 0, -0.05), 0.022, 0.014, pm, n=5, sy=0.4)


def _dress_priest(bd, player, hair, rnd, style="egypt"):
    b = bd.b
    pm = mat(player, rough=0.7)
    wh = mat((0.99, 0.98, 0.94), rough=0.85)
    bd.legs(foot="leather")
    bd.skirt([pm, wh, wh, wh], top=0.085, hem=-0.385, zs=[0, 0.12, 0.4, 0.75, 1.0], flare=1.5, w=(.086, .116))
    if style == "egypt":
        leo = mat((0.93, 0.70, 0.22), rough=0.8)
        bd.torso([wh, leo, leo, leo], 1.06)
        bd.arms(sleeve=leo, sleeve_f=0.5)
        bd.head(hair, "bald")
        dk = mat((0.2, 0.1, 0.05))
        with b.at(bd.U, True):
            for i in range(14):
                a = rnd.uniform(0, 6.28)
                z = rnd.uniform(0.11, 0.27)
                rx, ry = 0.095, 0.13 + (0.02 if z > 0.2 else 0)
                b.ball((rx * math.cos(a), ry * math.sin(a), z), 0.02, dk, scale=(1, 1, 1), n=5)
            b.loft([(0, 0, 0.26, .09, .15), (0.004, 0, 0.305, .055, .075)], gold(), n=10, caps=False)
        with b.at(bd.P, True):
            b.cube((0.105, 0.0, -0.13), (0.03, 0.07, 0.3), pm)
            b.cube((0.108, 0.0, -0.27), (0.034, 0.075, 0.03), gold())
    else:
        bd.torso(wh, 1.06)
        bd.arms(sleeve=wh, sleeve_f=0.8)
        bd.head(hair, "short", beard=True)
        with b.at(bd.U, True):  # a stole over the left shoulder
            b.seg((0.10, -0.09, 0.0), (0.03, 0.14, 0.31), 0.045, 0.05, pm, n=6, sy=0.35, hint=(1, 0, 0))
            b.seg((-0.09, -0.09, 0.0), (-0.04, 0.14, 0.31), 0.045, 0.05, pm, n=6, sy=0.35, hint=(1, 0, 0))
        with b.at(bd.H, True):  # an olive wreath
            b.loft([(-0.014, 0, 0.045, .132, .129), (-0.014, 0, 0.075, .13, .127)], "leaf", n=12, caps=False)
        with b.at(bd.P, True):
            b.cube((0.10, 0.07, -0.12), (0.03, 0.06, 0.26), pm)


# --- the people ------------------------------------------------------------

def villager(loc=(0, 0, 0), facing=0.0, player=BLUE, pose="stand", seed=0, parent=None, job="idle"):
    root, b = _start("villager", loc, facing, parent)
    rnd, skin, hair = _people(seed)
    base = WALK if pose == "walk" else {}
    if job == "farm":
        p = _pose(legL=(22, 12, 4), legR=(-16, 8, 4), lean=15, head=(0, -4),
                  armL=((0.75, -0.1, -0.65), (0.75, -0.25, -0.6)),
                  armR=((0.25, 0.05, -0.95), (0.95, 0.3, -0.25)))
    elif job == "carry":
        p = _pose(WALK if pose != "stand" else {}, lean=3,
                  armL=((0.15, 0.55, 0.8), (0.0, -0.75, 0.65)))
    elif job == "hammer":
        p = _pose(legL=(20, 10, 4), legR=(-14, 6, 4), lean=10, head=(0, 6),
                  armR=((0.25, -0.35, 0.9), (-0.45, 0.1, 0.9)),
                  armL=((0.55, 0.1, -0.8), (1, -0.15, -0.15)))
    else:
        p = _pose(base, armR=((-0.1, -0.5, -1), (0.35, 0.75, -0.25))) if pose != "walk" else _pose(base)
    bd = Body(b, _vary(p, rnd), skin)
    _dress_villager(bd, player, hair, rnd, job)
    with bd.fig():
        if job == "farm":
            a, c = bd.pt("R"), bd.pt("L")
            d = nv(c - a)
            e = c + d * 0.42
            b.seg(a - d * 0.14, e, 0.02, 0.02, "wood", n=6)
            dn = nv(V((-0.5, 0, -1)) - d * d.dot(V((-0.5, 0, -1))))
            b.seg(e - d * 0.02, e + dn * 0.2, 0.06, 0.08, "iron", n=6, sy=0.25, hint=(0, 1, 0))
        elif job == "hammer":
            a = bd.pt("R")
            d = bd.fdir((-0.35, 0.05, 1))
            b.seg(a - d * 0.06, a + d * 0.24, 0.015, 0.015, "wood", n=6)
            x = bd.fdir((1, 0, 0.35))
            b.seg(a + d * 0.24 - x * 0.075, a + d * 0.24 + x * 0.075, 0.048, 0.048, "wood_dark", n=8)
            c = bd.pt("L")
            b.seg(c + V((0, 0, 0.06)), c - V((-0.02, 0, 0.1)), 0.012, 0.008, "iron", n=5)
    if job == "carry":
        with b.at(bd.U, True):
            c = V((-0.01, 0.20, 0.375))
            b.seg(c, c + V((0, 0, 0.15)), 0.075, 0.115, "straw", n=10)
            b.seg(c + V((0, 0, 0.15)), c + V((0, 0, 0.165)), 0.122, 0.122, "thatch", n=10)
            b.ball(c + V((0, 0, 0.16)), 0.10, rnd.choice(["wheat", "flower_red", "leaf_olive"]),
                   scale=(1, 1, 0.5), n=8)
    b.finish(root)
    return root


def hoplite(loc=(0, 0, 0), facing=0.0, player=BLUE, pose="stand", seed=0, parent=None,
            blazon=None, cloak=False):
    root, b = _start("hoplite", loc, facing, parent)
    rnd, skin, hair = _people(seed)
    blazon = blazon or ("lambda", "star", "dot")[seed % 3]
    if pose == "attack":
        p = _pose(legL=(38, 30, 5), legR=(-32, 8, 5), lean=11, head=(0, 2),
                  armL=((0.7, 0.3, -0.6), (1, -0.05, 0.25)),
                  armR=((-0.35, -0.6, 0.5), (0.5, 0.3, 0.8)))
        sn, sdir, back = (1, 0.22, 0.08), (1, 0.04, 0.02), 0.55
    elif pose == "walk":
        p = _pose(WALK, armL=((0.25, 0.3, -1), (1, 0.1, 0.1)), armR=((0.1, -0.2, -1), (0.75, -0.05, -0.3)))
        sn, sdir, back = (0.8, 0.6, 0.05), (0.22, 0, 1), 0.42
    else:
        p = _pose(armL=((0.25, 0.3, -1), (1, 0.1, 0.1)), armR=((0.12, -0.3, -1), (0.6, -0.1, -0.45)))
        sn, sdir, back = (0.75, 0.66, 0.05), (0.03, -0.02, 1), None
    bd = Body(b, _vary(p, rnd, 0.6), skin)
    _dress_hoplite(bd, player, hair, rnd, cloak=cloak)
    with bd.fig():
        g = bd.pt("R")
        _spear(b, g, bd.fdir(sdir) if pose == "attack" else sdir, L=1.55,
               back=back if back is not None else g.z / nv(sdir).z - 0.03)
        sn = nv(sn)
        c = bd.fpt(bd.el["L"] + bd.fad["L"] * 0.09) + sn * 0.06
        _aspis(b, c, sn, player, blazon)
    b.finish(root)
    return root


def _archer_pose(pose, rnd, legs=True):
    if pose == "attack":
        p = _pose(twist=-68, head=(66, -2), lean=0,
                  armL=((0.37, 0.93, 0.06), (0.37, 0.93, 0.06)),
                  armR=((-0.15, -0.75, 0.65), (0.1, 0.22, -0.05)))
        if legs:
            p.update(legL=(24, 6, 6), legR=(-20, 4, 8))
    elif pose == "walk":
        p = _pose(WALK, armL=((0.3, 0.15, -1), (1, 0.0, -0.2)))
    else:
        p = _pose(armL=((0.2, 0.2, -1), (0.9, 0.05, -0.35)), armR=((0.0, -0.2, -1), (0.3, 0.3, -0.8)))
    return p


def _hold_bow(bd, pose, composite, m="wood"):
    b = bd.b
    with bd.fig():
        g = bd.pt("L")
        if pose == "attack":
            aim = nv((1, 0, 0.06))
            g = g + aim * 0.03
            _bow(b, g, aim, nock=bd.pt("R") + V((0.0, 0, 0.0)), composite=composite, m=m)
        else:
            _bow(b, g + V((0.02, 0.03, 0.05)), nv((0.75, 0.6, 0.15)), composite=composite, m=m)


def archer(loc=(0, 0, 0), facing=0.0, player=BLUE, pose="stand", seed=0, parent=None):
    root, b = _start("archer", loc, facing, parent)
    rnd, skin, hair = _people(seed)
    bd = Body(b, _vary(_archer_pose(pose, rnd), rnd, 0.4), skin)
    _dress_archer(bd, player, hair, rnd)
    _hold_bow(bd, pose, False)
    b.finish(root)
    return root


def egyptian_spearman(loc=(0, 0, 0), facing=0.0, player=RED, pose="stand", seed=0, parent=None,
                      weapon="spear"):
    root, b = _start("egyptian_spearman", loc, facing, parent)
    rnd, skin, hair = _people(seed, True)
    if pose == "attack":
        p = _pose(legL=(36, 26, 5), legR=(-30, 8, 5), lean=10,
                  armL=((0.7, 0.3, -0.6), (1, -0.05, 0.2)),
                  armR=((-0.35, -0.6, 0.5), (0.5, 0.3, 0.8)) if weapon == "spear"
                  else ((0.1, -0.5, 0.85), (-0.25, 0.1, 1)))
        sn, sdir, back = (1, 0.2, 0.05), (1, 0.04, 0.0), 0.5
    elif pose == "walk":
        p = _pose(WALK, armL=((0.25, 0.3, -1), (1, 0.1, 0.1)), armR=((0.1, -0.2, -1), (0.75, -0.05, -0.3)))
        sn, sdir, back = (0.8, 0.6, 0.0), (0.22, 0, 1), 0.42
    else:
        p = _pose(armL=((0.25, 0.3, -1), (1, 0.1, 0.1)), armR=((0.12, -0.3, -1), (0.6, -0.1, -0.45)))
        sn, sdir, back = (0.75, 0.66, 0.0), (0.03, -0.02, 1), None
    bd = Body(b, _vary(p, rnd, 0.6), skin)
    _dress_egyptian(bd, player, hair, rnd, "nemes")
    with bd.fig():
        g = bd.pt("R")
        if weapon == "spear":
            _spear(b, g, bd.fdir(sdir) if pose == "attack" else sdir, L=1.35,
                   back=back if back is not None else g.z / nv(sdir).z - 0.03, butt=False, head=0.2)
        else:
            d = bd.fdir((-0.3, 0, 1)) if pose == "attack" else nv((0.55, 0, 0.85))
            f = bd.fdir((1, 0, 0.3)) if pose == "attack" else nv((0.85, 0, -0.55))
            b.seg(g - d * 0.05, g + d * 0.12, 0.016, 0.016, "wood_dark", n=6)
            pts = [g + d * 0.12]
            for i in range(1, 7):
                a = math.radians(-20 + 150 * i / 6)
                pts.append(g + d * (0.20 + 0.15 * math.sin(a)) + f * (0.13 - 0.13 * math.cos(a) + 0.0))
            b.chain(pts, 0.022, 0.016, bronze(), n=6, sy=0.4, hint=(0, 1, 0))
        sn = nv(sn)
        c = bd.fpt(bd.el["L"] + bd.fad["L"] * 0.09) + sn * 0.05 + V((0, 0, 0.03))
        _eshield(b, c, sn, player, seed)
    b.finish(root)
    return root


def egyptian_archer(loc=(0, 0, 0), facing=0.0, player=RED, pose="stand", seed=0, parent=None):
    root, b = _start("egyptian_archer", loc, facing, parent)
    rnd, skin, hair = _people(seed, True)
    bd = Body(b, _vary(_archer_pose(pose, rnd), rnd, 0.4), skin)
    _dress_egyptian(bd, player, hair, rnd, "wig")
    with b.at(bd.U, True):
        _quiver(b, (-0.13, -0.12, 0.36), (-0.10, 0.07, 0.02), m=mat(player, rough=0.7), band=gold())
        b.seg((0.09, 0.1, 0.03), (0.0, -0.13, 0.29), 0.016, 0.016, "leather", n=5, sy=0.4)
    _hold_bow(bd, pose, True, m=mat((0.3, 0.18, 0.1)))
    b.finish(root)
    return root


def priest(loc=(0, 0, 0), facing=0.0, player=RED, pose="stand", seed=0, parent=None, style="egypt"):
    root, b = _start("priest", loc, facing, parent)
    rnd, skin, hair = _people(seed, style == "egypt")
    arms = dict(armR=((0.3, -0.25, -0.9), (1, 0.0, 0.3)), armL=((0.2, 0.2, -1), (0.8, -0.3, 0.3)))
    if pose == "attack":  # arms raised in blessing
        arms["armL"] = ((0.4, 0.4, 0.6), (0.3, 0.1, 1))
    p = _pose(dict(legL=(16, 4, 3), legR=(-12, 12, 3)) if pose == "walk" else {}, arms)
    bd = Body(b, _vary(p, rnd, 0.5), skin)
    _dress_priest(bd, player, hair, rnd, style)
    with bd.fig():
        g = bd.pt("R")
        d = nv((0.04, 0, 1))
        top = g + d * (1.22 - g.z)
        b.seg(g - d * (g.z - 0.0), top, 0.016, 0.016, "wood_dark", n=6)
        if style == "egypt":  # an ankh-ish gold head
            b.seg(top, top + d * 0.03, 0.03, 0.03, gold(), n=6)
            b.seg(top + V((0, -0.07, 0.03)), top + V((0, 0.07, 0.03)), 0.016, 0.016, gold(), n=6)
            b.ball(top + d * 0.11, 0.06, gold(), scale=(0.45, 0.9, 1.2), n=8)
        else:
            b.ball(top, 0.04, gold(), n=8)
            b.seg(top - d * 0.1, top - d * 0.04, 0.025, 0.025, mat(player), n=6)
    b.finish(root)
    return root


# --- animals ---------------------------------------------------------------

SPEC = {
    "horse": dict(L=0.72, rz=0.185, ry=0.16, leg=0.50, neck=(0.33, 54, 0.14, 0.09),
                  head=(0.29, -40, 0.094, 0.062), ear=0.075),
    "donkey": dict(L=0.50, rz=0.135, ry=0.12, leg=0.34, neck=(0.19, 42, 0.085, 0.062),
                   head=(0.23, -30, 0.07, 0.048), ear=0.14),
    "ox": dict(L=0.74, rz=0.195, ry=0.175, leg=0.33, neck=(0.15, 8, 0.14, 0.11),
               head=(0.21, -42, 0.10, 0.07), ear=0.05),
}
GAITS = {
    "stand": [((3, 0), (-3, 0), (-5, 3), (4, 5))],
    "walk": [((24, 6), (-18, -34), (-22, -8), (18, -4))],
    "gallop": [((64, 50), (48, 30), (-52, -66), (-38, -52)),
               ((40, -50), (20, -68), (44, 18), (28, 6))],
}


def _quad(b, kind="horse", pose="stand", coat="cedar", dark="hair_black", phase=0, pitch=0.0,
          plume=None, cloth=None, sock=None):
    """A four-legged beast facing +X in the current frame, hooves on z=0.
    Returns points in the current frame: back (seat), withers, mouth, and Q,
    the matrix (relative to the current frame) the beast was built in."""
    sp = SPEC[kind]
    L, rz, ry, leg = sp["L"], sp["rz"], sp["ry"], sp["leg"]
    gait = GAITS["gallop" if pose in ("attack", "gallop") else pose]
    gait = gait[phase % len(gait)]
    bz = leg + rz * 0.8
    lu, ll = leg * 0.52, leg * 0.48
    tops, pts = [], []
    for i, (au, al) in enumerate(gait):
        x = 0.34 * L if i < 2 else -0.40 * L
        y = ry * 0.62 * (1 if i % 2 == 0 else -1)
        top = V((x, y, leg + 0.03))
        k = top + V((math.sin(math.radians(au)), 0, -math.cos(math.radians(au)))) * lu
        h = k + V((math.sin(math.radians(al)), 0, -math.cos(math.radians(al)))) * (ll + 0.03)
        pts.append((top, k, h))
    Q = T(-0.25 * L, 0, 0) @ R("Y", -pitch) @ T(0.25 * L, 0, 0)
    low = min((Q @ p[2]).z for p in pts)
    Q = T(0, 0, -low + 0.012) @ Q
    cm = mat(coat, rough=0.7)
    dm = mat(dark, rough=0.8)
    with b.at(Q):
        fr = R("Y", 90)
        rings = [(-0.02, 0, -0.55 * L, rz * 0.5, ry * 0.5), (-0.02, 0, -0.45 * L, rz * 0.98, ry * 0.97),
                 (0.0, 0, -0.1 * L, rz * 0.93, ry * 0.98), (0, 0, 0.27 * L, rz * 1.04, ry * 1.0),
                 (-0.02, 0, 0.45 * L, rz * 0.85, ry * 0.8), (-0.05, 0, 0.53 * L, rz * 0.5, ry * 0.45)]
        b.loft(rings, cm, n=10, frame=T(0, 0, bz) @ fr)
        if cloth:
            cl = cloth if isinstance(cloth, list) else [cloth]
            xs = [-0.2 * L, -0.12 * L, 0.12 * L, 0.2 * L] if len(cl) == 3 else [-0.16 * L, 0.16 * L]
            b.loft([(0.0, 0, x, rz * 1.06, ry * 1.09) for x in xs], cl, n=10,
                   frame=T(0, 0, bz) @ fr, caps=False)
        for i, (top, k, h) in enumerate(pts):
            b.seg(top, k, rz * 0.46, rz * 0.3, cm, n=8)
            b.seg(k, h, rz * 0.28, rz * 0.21, mat(sock) if sock and i % 3 == 0 else cm, n=8)
            d = nv(h - k)
            b.seg(h - d * 0.035, h + d * 0.012, rz * 0.27, rz * 0.33, dm, n=6)
        nl, na, nr0, nr1 = sp["neck"]
        n0 = V((0.38 * L, 0, bz + rz * 0.42))
        nd = V((math.cos(math.radians(na)), 0, math.sin(math.radians(na))))
        n1 = n0 + nd * nl
        b.seg(n0 - nd * 0.04, n1, nr0, nr1, cm, n=8, sy=0.68)
        hl, ha, hr0, hr1 = sp["head"]
        hd = V((math.cos(math.radians(ha)), 0, math.sin(math.radians(ha))))
        h0 = n1 - hd * 0.03 + V((0, 0, 0.012))
        h1 = h0 + hd * hl
        b.seg(h0, h1, hr0, hr1, cm, n=8, sy=0.82)
        b.ball(h0, hr0 * 1.02, cm, scale=(1, 0.85, 1), n=8)
        b.ball(h1, hr1 * 1.05, mat(_shade(coat, 0.55)) if kind != "horse" else cm, scale=(1, 0.85, 1), n=6)
        up = V((-hd.z, 0, hd.x))
        for s in (1, -1):
            b.ball(h0 + hd * hl * 0.28 + up * hr0 * 0.35 + V((0, s * hr0 * 0.72, 0)), 0.02, "black", n=5)
            e0 = h0 + up * hr0 * 0.7 + V((0, s * hr0 * 0.55, 0)) - hd * 0.02
            ed = nv(up + V((0, s * (0.9 if kind != "horse" else 0.25), 0)) - hd * 0.3)
            b.seg(e0, e0 + ed * sp["ear"], 0.024, 0.006, cm, n=5, sy=0.6)
        if kind == "ox":
            for s in (1, -1):
                c0 = h0 + up * hr0 * 0.75 + V((0, s * hr0 * 0.5, 0))
                b.chain([c0, c0 + V((0.0, s * 0.11, 0.05)), c0 + V((0.03, s * 0.15, 0.15))], 0.024, 0.006,
                        mat((0.95, 0.92, 0.82)), n=6)
        else:
            mb = V((-nd.z, 0, nd.x))  # the back of the neck
            b.seg(n0 + mb * nr0 * 0.95, n1 + mb * nr1 * 1.0 + nd * 0.03, 0.04, 0.03, dm, n=6, sy=0.4)
            if kind == "horse":
                b.ball(h0 + up * hr0 * 0.8 + hd * 0.03, 0.035, dm, n=6)
        if plume:
            c0 = h0 + up * hr0 * 0.7
            b.seg(c0, c0 + up * 0.07 + V((0, 0, 0.0)), 0.022, 0.03, gold(), n=6)
            b.chain([c0 + up * 0.07, c0 + up * 0.17 - hd * 0.02, c0 + up * 0.25 - hd * 0.08], 0.05, 0.02,
                    mat(plume, rough=0.7), n=6, sy=0.6)
            # headstall and breast band
            b.seg(h0 + hd * hl * 0.55, h0 + hd * hl * 0.68, hr0 * 0.86, hr0 * 0.8, mat(plume, rough=0.7), n=8, sy=0.85)
            b.seg(n0 + nd * 0.02, n0 + nd * 0.085, nr0 * 1.1, nr0 * 1.04, mat(plume, rough=0.7), n=8, sy=0.7)
        t0 = V((-0.54 * L, 0, bz + rz * 0.45))
        if kind == "horse":
            if pose in ("attack", "gallop", "walk"):
                tp = [t0, t0 + V((-0.1, 0, 0.07)), t0 + V((-0.24, 0, 0.03)), t0 + V((-0.36, 0, -0.1))]
            else:
                tp = [t0, t0 + V((-0.07, 0, 0.0)), t0 + V((-0.11, 0, -0.16)), t0 + V((-0.1, 0, -0.36))]
            b.chain(tp, 0.03, 0.045, dm, n=6, sy=0.7)
        else:
            tp = [t0, t0 + V((-0.05, 0, -0.05)), t0 + V((-0.06, 0, -0.26))]
            b.chain(tp, 0.016, 0.012, cm, n=5)
            b.ball(tp[-1], 0.03, dm, scale=(1, 1, 1.5), n=6)
    return dict(Q=Q, back=Q @ V((-0.02, 0, bz + rz * 0.95)), withers=Q @ V((0.27 * L, 0, bz + rz * 1.0)),
                mouth=Q @ (h1 - V((0, 0, 0.02))), bz=bz, rz=rz, ry=ry, L=L)


COATS = ["cedar", (0.36, 0.22, 0.13), (0.93, 0.90, 0.84), (0.70, 0.48, 0.26), (0.20, 0.15, 0.13)]


def ox(loc=(0, 0, 0), facing=0.0, player=BLUE, pose="stand", seed=0, parent=None):
    root, b = _start("ox", loc, facing, parent)
    rnd = random.Random(seed)
    coat = [(0.93, 0.88, 0.76), (0.72, 0.46, 0.26), (0.86, 0.74, 0.56)][seed % 3]
    q = _quad(b, "ox", "walk" if pose == "walk" else "stand", coat=coat, dark="wood_dark")
    with b.at(q["Q"]):
        bz, rz, ry, L = q["bz"], q["rz"], q["ry"], q["L"]
        pm = mat(player, rough=0.7)
        if seed % 3 == 0:  # patches
            pc = mat((0.5, 0.3, 0.18))
            b.ball((-0.12, 0.05, bz + rz * 0.75), 0.11, pc, scale=(1.2, 1, 0.45), n=8)
            b.ball((0.14, -ry * 0.8, bz + 0.02), 0.09, pc, scale=(1.2, 0.4, 1), n=8)
        # a neck collar in the owner's colour with a bell
        b.seg((0.30 * L, 0, bz + rz * 0.25), (0.37 * L, 0, bz + rz * 0.3), rz * 0.98, rz * 0.9, pm, n=10, sy=0.9)
        b.ball((0.43 * L, 0, bz - rz * 0.62), 0.03, gold(), n=6)
    b.finish(root)
    return root


def donkey(loc=(0, 0, 0), facing=0.0, player=BLUE, pose="stand", seed=0, parent=None, load=True):
    root, b = _start("donkey", loc, facing, parent)
    coat = [(0.62, 0.58, 0.54), (0.55, 0.45, 0.36)][seed % 2]
    q = _quad(b, "donkey", "walk" if pose == "walk" else "stand", coat=coat, dark=(0.2, 0.17, 0.15))
    with b.at(q["Q"]):
        bz, rz, ry, L = q["bz"], q["rz"], q["ry"], q["L"]
        b.ball((0.5 * L + 0.21, 0, bz + 0.19), 0.001, "white", n=4)
        if load:
            pm = mat(player, rough=0.7)
            b.loft([(0.0, 0, x, rz * 1.07, ry * 1.1) for x in (-0.17 * L, 0.2 * L)], pm, n=10,
                   frame=T(0, 0, bz) @ R("Y", 90), caps=False)
            for s in (1, -1):
                c = V((0.0, s * (ry + 0.085), bz - 0.03))
                b.seg(c, c + V((0, s * 0.02, 0.19)), 0.07, 0.10, "straw", n=8)
                b.ball(c + V((0, s * 0.02, 0.19)), 0.09, ("wheat", "terracotta")[seed % 2], scale=(1, 1, 0.55), n=8)
                b.seg(c + V((0, -s * 0.05, 0.16)), V((0, 0, bz + rz * 1.05)), 0.014, 0.014, "leather", n=5)
    b.finish(root)
    return root


def _rider_pose(pose):
    legs = dict(legL=(62, 62, 24), legR=(62, 62, 24))
    if pose == "attack":
        return _pose(legs, lean=8, armL=((0.5, 0.15, -0.8), (1, -0.1, 0.1)),
                     armR=((-0.3, -0.6, 0.55), (0.45, 0.25, 0.85)))
    return _pose(legs, lean=2, armL=((0.4, 0.15, -0.9), (1, -0.1, 0.1)),
                 armR=((0.15, -0.3, -1), (0.8, -0.05, 0.05)))


def horseman(loc=(0, 0, 0), facing=0.0, player=BLUE, pose="stand", seed=0, parent=None):
    root, b = _start("horseman", loc, facing, parent)
    rnd, skin, hair = _people(seed)
    pm = mat(player, rough=0.7)
    q = _quad(b, "horse", pose, coat=COATS[seed % len(COATS)], phase=seed, pitch=10 if pose == "attack" else 0,
              cloth=[gold(), pm, gold()], plume=None, sock="white" if seed % 2 else None)
    seat = q["back"]
    p = _rider_pose(pose)
    p["hipz"] = 0.0
    with b.at(T(seat + V((0.02, 0, 0.055))) @ R("Y", -(10 if pose == "attack" else 0) * 0.6)):
        bd = Body(b, _vary(p, rnd, 0.4), skin)
        lin = mat("linen", rough=0.9)
        bd.legs(foot="leather", shin=mat("leather"), shin_r=1.1)
        bd.skirt([pm, lin, lin], top=0.085, hem=-0.07, flare=1.25, zs=[0, 0.35, 0.7, 1])
        bd.torso(lin, 1.05)
        bd.arms(sleeve=lin, sleeve_f=0.4)
        bd.head(hair, "short", beard=rnd.random() < 0.4)
        with b.at(bd.H, True):  # the petasos, a traveller's sun hat
            hm = mat("straw", rough=0.9)
            b.seg((-0.01, 0, 0.065), (-0.01, 0, 0.085), 0.2, 0.17, hm, n=12)
            b.ball((-0.01, 0, 0.085), 0.10, hm, scale=(1, 1, 0.75), n=10)
            b.seg((-0.01, 0, 0.088), (-0.01, 0, 0.105), 0.102, 0.098, pm, n=10)
        _cloak(bd, pm, fly=0.9 if pose in ("attack", "walk") else 0.15, length=0.46)
        with bd.fig():
            g = bd.pt("R")
            if pose == "attack":
                _spear(b, g, bd.fdir((1, 0.04, -0.1)), L=1.15, back=0.5, r=0.012, head=0.13, butt=False)
            else:
                _spear(b, g, nv((0.25, -0.05, 1)), L=1.15, back=0.4, r=0.012, head=0.13, butt=False)
            l = bd.pt("L")
    with b.at(I4):
        lw = bd.base @ l
        lw = b.M.inverted() @ lw
        for s in (1, -1):
            b.seg(lw, q["mouth"] + V((0, s * 0.04, 0)), 0.006, 0.006, "leather", n=4)
    b.finish(root)
    return root


def chariot(loc=(0, 0, 0), facing=0.0, player=RED, pose="stand", seed=0, parent=None):
    root, b = _start("chariot", loc, facing, parent)
    rnd, skin, hair = _people(seed, True)
    pm = mat(player, rough=0.7)
    gd = gold()
    wood = mat("cedar", rough=0.7)
    rw, ax, hy = 0.27, -0.66, 0.31
    for s in (1, -1):  # six-spoked wheels
        with b.at(T(ax, s * hy, rw) @ R("X", 90)):
            rim = [V((rw * math.cos(2 * math.pi * k / 12), rw * math.sin(2 * math.pi * k / 12), 0)) for k in range(13)]
            b.chain(rim, 0.024, 0.024, mat("wood_dark"), n=6)
            a0 = rnd.uniform(0, 1)
            for k in range(6):
                a = a0 + math.pi * k / 3
                b.seg((0, 0, 0), (rw * math.cos(a), rw * math.sin(a), 0), 0.014, 0.014, mat("yellow_ochre"), n=5)
            b.seg((0, 0, -0.05), (0, 0, 0.05), 0.045, 0.045, gd, n=8)
    b.seg((ax, -hy, rw), (ax, hy, rw), 0.02, 0.02, "wood_dark", n=6)
    # the cab: a D-shaped floor and a curved breastwork
    fz = 0.30
    cx, rx, ry = -0.68, 0.36, 0.215
    arc = [(-90 + 180 * k / 8) for k in range(9)]
    out = [(cx + rx * math.cos(math.radians(a)), ry * math.sin(math.radians(a))) for a in arc]
    b.plate(out, 0.035, wood, frame=T(0, 0, fz))
    vs, fs = [], []
    for k, (x, y) in enumerate(out):
        tz = fz + 0.24 + 0.12 * math.cos(math.radians(arc[k]))
        vs += [(x, y, fz), (x, y, tz)]
        if k:
            fs.append((2 * k - 2, 2 * k, 2 * k + 1, 2 * k - 1))
    b.poly(vs, fs, mat((0.93, 0.80, 0.50), rough=0.7), smooth=True)
    top = [V(vs[2 * k + 1]) for k in range(9)]
    b.chain(top, 0.02, 0.02, gd, n=6)
    b.chain([V((x, y, fz + 0.13)) * 1.0 + V((0.004 * math.cos(math.radians(a)), 0.004 * math.sin(math.radians(a)), 0))
             for (x, y), a in zip(out, arc)], 0.03, 0.03, pm, n=6, sy=0.25, hint=(0, 0, 1))
    for s in (1, -1):  # bow and javelin cases
        b.seg((ax + 0.02, s * (ry + 0.03), fz + 0.03), (ax + 0.30, s * (ry + 0.02), fz + 0.30), 0.05, 0.055, pm, n=6, sy=0.45,
              hint=(0, 1, 0))
        b.seg((ax + 0.27, s * (ry + 0.02), fz + 0.27), (ax + 0.31, s * (ry + 0.02), fz + 0.31), 0.058, 0.058, gd, n=6, sy=0.45)
        for j in range(2):
            b.seg((ax + 0.31, s * (ry + 0.02 + j * 0.02), fz + 0.31), (ax + 0.36 + j * 0.02, s * (ry + 0.02), fz + 0.37),
                  0.012, 0.016, "white", n=5, sy=0.4)
    # the horses
    hx = 0.46
    qs = []
    cols = [(0.95, 0.93, 0.88), (0.62, 0.33, 0.16)]
    if seed % 2:
        cols.reverse()
    for i, s in enumerate((1, -1)):
        with b.at(T(hx, s * 0.185, 0)):
            q = _quad(b, "horse", pose, coat=cols[i], dark=("rock", "hair_black")[i] if cols[i][0] > 0.9 else "hair_black",
                      phase=i + seed, pitch=9 if pose == "attack" else 0,
                      plume=player, cloth=[gd, pm, gd])
            qs.append({k: (T(hx, s * 0.185, 0) @ v if isinstance(v, (Vector, Matrix)) else v) for k, v in q.items()})
    wy = (qs[0]["withers"] + qs[1]["withers"]) / 2
    b.chain([V((ax, 0, rw)), V((-0.25, 0, rw + 0.02)), V((0.2, 0, wy.z - 0.22)), wy + V((0, 0, 0.02))], 0.022, 0.018,
            "wood_dark", n=6)
    b.seg(qs[0]["withers"] + V((0, 0.05, 0.03)), qs[1]["withers"] + V((0, -0.05, 0.03)), 0.022, 0.022, gd, n=6)
    # the crew
    with b.at(T(-0.50, 0.085, fz + 0.018)):
        p = _pose(lean=6, legL=(10, 6, 3), legR=(-6, 4, 3), armL=((0.6, 0.1, -0.7), (1, -0.1, 0.15)),
                  armR=((0.6, -0.1, -0.7), (1, 0.1, 0.15)))
        d = Body(b, _vary(p, rnd, 0.3), skin)
        _dress_egyptian(d, player, hair, rnd, "nemes")
        hands = [b.M.inverted() @ d.base @ d.pt(k) for k in "LR"]
    for i in (0, 1):
        for s in (1, -1):
            b.seg(hands[i], qs[i]["mouth"] + V((0, s * 0.035, 0)), 0.006, 0.006, "leather", n=4)
    rnd2, skin2, hair2 = _people(seed + 3, True)
    with b.at(T(-0.60, -0.10, fz + 0.018)):
        ap = "attack" if pose == "attack" else "stand"
        p = _archer_pose(ap, rnd2, legs=False)
        a = Body(b, _vary(p, rnd2, 0.3), skin2)
        _dress_egyptian(a, player, hair2, rnd2, "wig")
        _hold_bow(a, ap, True, m=mat((0.3, 0.18, 0.1)))
    b.finish(root)
    return root


# --- the galley -------------------------------------------------------------

def galley(loc=(0, 0, 0), facing=0.0, player=BLUE, pose="stand", seed=0, parent=None, sail=True):
    """A war galley about 3.2 long, bow toward +X; z=0 is the waterline."""
    root, b = _start("galley", loc, facing, parent)
    pm = mat(player, rough=0.6)
    wh = mat((0.98, 0.96, 0.90), rough=0.8)
    hullm = mat((0.36, 0.20, 0.11), rough=0.7)
    lowm = mat((0.22, 0.12, 0.08), rough=0.8)
    deckm = mat((0.74, 0.56, 0.36), rough=0.9)
    N, HL = 16, 1.40

    def beam(u):
        return max(0.014, 0.31 * (1 - abs(u) ** 2.6) ** 0.55)

    def sheer(u):
        return 0.21 + (0.15 * u ** 4 if u > 0 else 0.22 * abs(u) ** 3)

    def at_x(x):
        u = max(-1, min(1, x / HL))
        return beam(u), sheer(u)

    vs, rows = [], []
    for i in range(N + 1):
        u = -1 + 2 * i / N
        w, sh = beam(u), sheer(u)
        keel = -0.12 * (1 - abs(u) ** 4) + 0.05 * abs(u) ** 6
        x = u * HL
        half = [(0.0, keel), (0.6 * w, keel * 0.7), (0.93 * w, -0.015), (w, sh - 0.105), (1.03 * w, sh - 0.035),
                (1.04 * w, sh), (0.88 * w, sh - 0.01), (0.86 * w, sh - 0.05)]
        row = {}
        for s in (1, -1):
            row[s] = []
            for (y, z) in half:
                row[s].append(len(vs))
                vs.append((x, s * y, z))
        row["c"] = len(vs)
        vs.append((x, 0, sh - 0.05))
        rows.append(row)
    bandm = [lowm, lowm, hullm, pm, wh, hullm, hullm]
    groups = {}
    for i in range(N):
        r0, r1 = rows[i], rows[i + 1]
        for s in (1, -1):
            for j in range(7):
                groups.setdefault(id(bandm[j]), (bandm[j], []))[1].append(
                    (r0[s][j], r1[s][j], r1[s][j + 1], r0[s][j + 1]))
            groups.setdefault(id(deckm), (deckm, []))[1].append((r0[s][7], r1[s][7], r1["c"], r0["c"]))
    base = len(b.bm.verts)
    allv = [b.bm.verts.new(b.M @ V(p)) for p in vs]
    for m_, fl in groups.values():
        idx = b.mi(m_)
        for f in fl:
            try:
                fc = b.bm.faces.new([allv[k] for k in f])
                fc.material_index = idx
                fc.smooth = m_ in (lowm, hullm, pm, wh)
            except ValueError:
                pass
    # stem, ram, eye
    b.chain([V((HL - 0.03, 0, -0.03)), V((HL + 0.05, 0, 0.2)), V((HL + 0.09, 0, 0.44)), V((HL + 0.04, 0, 0.64)),
             V((HL - 0.08, 0, 0.72))], 0.05, 0.03, hullm, n=6, sy=0.6)
    b.ball((HL - 0.1, 0, 0.72), 0.045, pm, n=8)
    b.seg((HL - 0.12, 0, 0.0), (HL + 0.16, 0, 0.0), 0.085, 0.06, bronze(), n=8, sy=0.6)
    b.seg((HL + 0.16, 0, 0.0), (HL + 0.34, 0, 0.01), 0.06, 0.012, bronze(), n=8, sy=0.6)
    ex = 1.03
    ew, es = at_x(ex)
    for s in (1, -1):
        c = V((ex, s * (ew * 1.0 + 0.004), es - 0.105))
        n_ = nv((0.28, s, 0.12))
        F = T(c) @ _frame(n_, (0, 0, 1))
        with b.at(F):
            b.seg((0, 0, 0), (0, 0, 0.012), 0.1, 0.1, wh, n=12, sy=0.62, hint=(0, 1, 0))
            b.seg((0, 0, 0.012), (0, 0, 0.02), 0.047, 0.047, mat((0.05, 0.05, 0.08)), n=10)
    # stern: the upswept tail and steering oars
    sp = [V((-HL + 0.02, 0, 0.1)), V((-HL - 0.1, 0, 0.36)), V((-HL - 0.13, 0, 0.62)), V((-HL - 0.05, 0, 0.84)),
          V((-HL + 0.12, 0, 0.95)), V((-HL + 0.3, 0, 0.93))]
    b.chain(sp, 0.055, 0.028, hullm, n=6, sy=0.6)
    for a in (-28, 0, 28):
        d = V((math.cos(math.radians(a - 12)), 0, math.sin(math.radians(a - 12))))
        b.seg(sp[-1], sp[-1] + d * 0.17, 0.02, 0.03, pm if a else gold(), n=6, sy=0.5)
    for s in (1, -1):
        o0, o1 = V((-HL + 0.42, s * 0.2, 0.42)), V((-HL + 0.10, s * 0.3, -0.1))
        b.seg(o0, o1, 0.014, 0.014, "wood", n=5)
        b.seg(o0 + (o1 - o0) * 0.6, o1, 0.05, 0.06, "wood", n=6, sy=0.25, hint=(0, 1, 0))
    # oars
    oarm = mat((0.80, 0.64, 0.40), rough=0.8)
    no = 10
    sw = 0.0 if pose == "stand" else 0.12
    for k in range(no):
        x = -0.95 + 1.8 * k / (no - 1)
        w, sh = at_x(x)
        for s in (1, -1):
            o0 = V((x, s * w * 0.98, sh - 0.045))
            o1 = V((x - 0.14 + sw, s * (w + 0.44), -0.03))
            b.seg(o0, o1, 0.012, 0.012, oarm, n=5)
            d = nv(o1 - o0)
            b.seg(o1 - d * 0.13, o1 + d * 0.02, 0.035, 0.04, oarm, n=6, sy=0.25, hint=(0, 0, 1))
    # crew: rowers' heads along both sides, a helmsman, a few marines' shields
    rnd = random.Random(seed)
    for k in range(no - 1):
        x = -0.85 + 1.8 * k / (no - 1)
        w, sh = at_x(x)
        for s in (1, -1):
            _, skin, hair = _people(seed * 31 + k * 2 + (s > 0))
            c = V((x, s * (w - 0.1), sh + 0.055))
            b.ball(c, 0.06, skin, n=8)
            b.ball(c + V((-0.012, 0, 0.014)), 0.064, hair, n=8)
            b.ball(c + V((0, 0, -0.085)), 0.075, pm if (k + (s > 0)) % 2 else "linen", scale=(0.9, 1.2, 0.8), n=8)
    for k in (1, 3, 5, 7):
        x = -0.85 + 1.8 * (k + 0.5) / (no - 1)
        w, sh = at_x(x)
        for s in (1, -1):
            c = V((x, s * (w * 1.05 + 0.012), sh - 0.03))
            b.seg(c, c + V((0, s * 0.015, 0)), 0.075, 0.07, bronze(), n=10)
            b.seg(c + V((0, s * 0.015, 0)), c + V((0, s * 0.03, 0)), 0.058, 0.03, pm, n=10)
    _, skin, hair = _people(seed + 99)
    with b.at(T(-HL + 0.42, 0, sheer(-0.7) - 0.05) @ S(0.8, 0.8, 0.8)):
        hb = Body(b, _pose(armL=((0.2, 0.6, -0.6), (0.3, 0.6, -0.3)), armR=((0.2, -0.6, -0.6), (0.3, -0.6, -0.3))), skin)
        hb.legs()
        hb.skirt([pm, pm], hem=-0.13)
        hb.torso(pm, 1.04)
        hb.arms()
        hb.head(hair, "short", beard=True)
    # mast, yard, sail, pennant
    mx = 0.12
    mtop = 1.42
    b.seg((mx, 0, 0.12), (mx, 0, mtop), 0.034, 0.024, "wood", n=8)
    if sail:
        yz, yw = 1.30, 1.12
        b.seg((mx + 0.03, -yw / 2 - 0.05, yz), (mx + 0.03, yw / 2 + 0.05, yz), 0.02, 0.02, "wood", n=6)
        nu, nvv = 10, 5
        sv = []
        for j in range(nvv + 1):
            v = j / nvv
            for i in range(nu + 1):
                u = i / nu
                y = (u - 0.5) * yw * (1 - 0.06 * v)
                bulge = 0.2 * math.sin(math.pi * u) ** 0.8 * math.sin(math.pi * (0.12 + 0.7 * v))
                sv.append((mx + 0.05 + bulge, y, yz - 0.02 - v * 0.82))
        fa, fb = [], []
        for j in range(nvv):
            for i in range(nu):
                k = j * (nu + 1) + i
                (fa if (i // 2) % 2 == 0 else fb).append((k, k + 1, k + nu + 2, k + nu + 1))
        base_n = len(b.bm.verts)
        svs = [b.bm.verts.new(b.M @ V(p)) for p in sv]
        for fl, m_ in ((fa, pm), (fb, wh)):
            idx = b.mi(m_)
            for f in fl:
                fc = b.bm.faces.new([svs[k] for k in f])
                fc.material_index = idx
                fc.smooth = True
        rope = mat((0.85, 0.78, 0.6))
        for s in (1, -1):
            b.seg(sv[-1 if s > 0 else -(nu + 1)], (-0.75, s * 0.24, 0.33), 0.006, 0.006, rope, n=4)
    else:
        b.seg((mx, -0.4, 1.1), (mx, 0.4, 1.1), 0.03, 0.03, wh, n=6)
    rope = mat((0.85, 0.78, 0.6))
    b.seg((mx, 0, mtop - 0.03), (HL + 0.04, 0, 0.64), 0.006, 0.006, rope, n=4)
    b.seg((mx, 0, mtop - 0.03), sp[3], 0.006, 0.006, rope, n=4)
    b.plate([(0, 0), (-0.3, 0.05), (0, 0.1)], 0.008, pm, frame=T(mx, 0, mtop - 0.06) @ R("X", 90))
    b.ball((mx, 0, mtop + 0.02), 0.03, gold(), n=6)
    b.finish(root)
    return root


# --- the fallen and battlefield litter ---------------------------------------

def fallen(loc=(0, 0, 0), facing=0.0, player=BLUE, pose="back", seed=0, parent=None, kind="hoplite"):
    """A casualty lying on the ground, head toward -X (pose "back") or +X ("front")."""
    root, b = _start("fallen", loc, facing, parent)
    rnd, skin, hair = _people(seed, kind == "egyptian")
    p = _pose(hipz=0.0, legL=(10, 28, 14), legR=(-2, 4, 8), head=(rnd.uniform(-30, 30), 0),
              armL=((0, 1, 0.5), (0.0, 0.3, 1)), armR=((0, -1, -0.6), (0.05, -0.6, -1)))
    M = T(0, 0, 0.085) @ R("Y", -90 if pose != "front" else 90)
    bd = Body(b, p, skin, M)
    if kind == "egyptian":
        _dress_egyptian(bd, player, hair, rnd, "nemes" if seed % 2 == 0 else "wig")
    elif kind == "villager":
        _dress_villager(bd, player, hair, rnd)
    else:
        _dress_hoplite(bd, player, hair, rnd)
    # a spear or arrow beside
    if kind != "villager":
        _spear(b, (0.1, -0.33, 0.02), nv((1, 0.25, 0.0)), L=1.2, back=0.6)
    if seed % 2:
        _arrow(b, (0.02, 0.03, 0.42), (0.1, 0.0, -1), L=0.28)
    b.finish(root)
    return root


def dropped_shield(loc=(0, 0, 0), facing=0.0, player=BLUE, pose="stand", seed=0, parent=None, kind="aspis"):
    root, b = _start("dropped_shield", loc, facing, parent)
    rnd = random.Random(seed)
    n = nv((rnd.uniform(0.15, 0.35), rnd.uniform(-0.3, -0.1), 1))
    if kind == "aspis":
        _aspis(b, (0, 0, 0.07), n, player, ("lambda", "star", "dot")[seed % 3])
    else:
        _eshield(b, (0, 0, 0.05), n, player, seed)
    b.finish(root)
    return root


def stuck_arrows(loc=(0, 0, 0), facing=0.0, player=BLUE, pose="stand", seed=0, parent=None, n=4):
    root, b = _start("stuck_arrows", loc, facing, parent)
    rnd = random.Random(seed)
    for i in range(n):
        p = V((rnd.uniform(-0.3, 0.3), rnd.uniform(-0.3, 0.3), 0))
        d = nv((rnd.uniform(-0.5, -0.2), rnd.uniform(-0.15, 0.15), -1))
        _arrow(b, p - d * 0.3, d, L=0.32)
    if seed % 2 == 0:  # a broken spear
        _spear(b, (0.1, 0.2, 0.02), nv((1, -0.6, 0.02)), L=0.6, back=0.2, butt=False)
    b.finish(root)
    return root


# --- pictures ---------------------------------------------------------------

def _sp(sx, sd):
    """Screen-ish coords (right, away) -> world."""
    k = math.sqrt(0.5)
    return (k * (sx - sd), k * (sx + sd), 0)


def showcase():
    root = empty("showcase")
    f = VIEW
    side = math.pi / 4 - 0.5
    r = -5.6
    for i, job in enumerate(("idle", "farm", "carry", "hammer")):
        villager(_sp(-7 + i * 1.5, r), f, BLUE if i % 2 == 0 else RED, seed=i, job=job, parent=root)
    villager(_sp(-1.0, r), f, RED, pose="walk", seed=7, parent=root)
    priest(_sp(0.6, r), f, RED, seed=1, parent=root)
    priest(_sp(2.1, r), f, BLUE, seed=2, style="greek", parent=root)
    ox(_sp(4.0, r), side, BLUE, seed=0, parent=root)
    donkey(_sp(6.2, r), side, RED, pose="walk", seed=1, parent=root)
    r = -1.9
    hoplite(_sp(-7, r), f, BLUE, "stand", 0, parent=root)
    hoplite(_sp(-5.5, r), f, BLUE, "walk", 1, parent=root, cloak=True)
    hoplite(_sp(-4.0, r), f + 0.5, BLUE, "attack", 2, parent=root)
    hoplite(_sp(-2.4, r), f, RED, "stand", 3, parent=root)
    archer(_sp(-0.9, r), f, BLUE, "stand", 1, parent=root)
    archer(_sp(0.6, r), f + 0.6, BLUE, "attack", 2, parent=root)
    horseman(_sp(2.8, r), side, BLUE, "stand", 0, parent=root)
    horseman(_sp(5.6, r), side, RED, "attack", 1, parent=root)
    r = 1.8
    egyptian_spearman(_sp(-7, r), f, RED, "stand", 0, parent=root)
    egyptian_spearman(_sp(-5.5, r), f + 0.5, RED, "attack", 1, parent=root)
    egyptian_spearman(_sp(-4.0, r), f, BLUE, "walk", 2, weapon="khopesh", parent=root)
    egyptian_spearman(_sp(-2.6, r), f + 0.4, RED, "attack", 3, weapon="khopesh", parent=root)
    egyptian_archer(_sp(-1.1, r), f, RED, "stand", 1, parent=root)
    egyptian_archer(_sp(0.4, r), f + 0.6, RED, "attack", 2, parent=root)
    fallen(_sp(2.2, r), 0.3, BLUE, seed=0, parent=root)
    fallen(_sp(3.9, r), 2.0, RED, seed=1, kind="egyptian", parent=root)
    dropped_shield(_sp(5.3, r), 0, BLUE, seed=0, parent=root)
    dropped_shield(_sp(6.2, r), 0.4, RED, seed=1, kind="egyptian", parent=root)
    stuck_arrows(_sp(7.2, r), 0, parent=root)
    r = 6.0
    chariot(_sp(-5.2, r), side, RED, "attack", 0, parent=root)
    chariot(_sp(-1.6, r), side, BLUE, "stand", 1, parent=root)
    galley(_sp(3.6, r + 0.6), side, BLUE, parent=root)
    return root


def skirmish():
    root = empty("skirmish")
    E = math.pi / 4          # toward screen-right
    W = E + math.pi          # toward screen-left
    blue = [  # (kind, sx, sd, facing offset, pose, seed)
        ("h", -0.55, -0.9, 0.0, "attack", 0), ("h", -0.45, 0.25, -0.25, "attack", 1),
        ("h", -0.75, 1.3, 0.15, "attack", 2), ("h", -1.35, -0.3, -0.1, "walk", 3),
        ("h", -1.5, 0.85, 0.1, "attack", 4), ("h", -2.1, -1.3, 0.0, "walk", 5),
        ("h", -1.2, -1.9, -0.3, "stand", 6),
        ("a", -2.3, 0.2, 0.05, "attack", 1), ("a", -2.5, 1.5, -0.1, "attack", 2),
        ("a", -2.9, -0.7, 0.1, "stand", 3),
    ]
    for kind, sx, sd, fo, pose, seed in blue:
        fn = hoplite if kind == "h" else archer
        fn(_sp(sx * 1.1, sd * 1.25), E + fo * 1.5 - 0.5, BLUE, pose, seed, parent=root)
    red = [
        ("s", 0.5, -0.75, 0.1, "attack", 0, "spear"), ("s", 0.45, 0.45, -0.2, "attack", 1, "khopesh"),
        ("s", 0.65, 1.5, 0.2, "attack", 2, "spear"), ("s", 1.3, -1.6, 0.0, "walk", 3, "spear"),
        ("s", 1.4, 0.0, 0.1, "attack", 4, "spear"), ("s", 1.5, 1.0, -0.1, "walk", 5, "khopesh"),
        ("s", 2.1, -0.8, 0.0, "stand", 6, "spear"),
        ("a", 2.3, 0.6, 0.0, "attack", 1, ""), ("a", 2.6, -0.2, 0.15, "attack", 2, ""),
        ("a", 2.4, 1.7, -0.1, "stand", 3, ""),
    ]
    for kind, sx, sd, fo, pose, seed, wp in red:
        if kind == "s":
            egyptian_spearman(_sp(sx * 1.1, sd * 1.25), W + fo * 1.5 + 0.25, RED, pose, seed, weapon=wp, parent=root)
        else:
            egyptian_archer(_sp(sx * 1.1, sd * 1.25), W + fo * 1.5 + 0.25, RED, pose, seed, parent=root)
    chariot(_sp(2.3, 3.2), W + 0.3, RED, "attack", 0, parent=root)
    fallen(_sp(-0.1, -0.35), 1.0, BLUE, seed=1, parent=root)
    fallen(_sp(0.25, 1.0), 2.6, RED, seed=0, kind="egyptian", parent=root)
    dropped_shield(_sp(0.0, -1.7), 0, RED, seed=2, kind="egyptian", parent=root)
    dropped_shield(_sp(-0.2, 1.95), 0, BLUE, seed=1, parent=root)
    stuck_arrows(_sp(0.9, -0.2), 0.5, seed=1, parent=root)
    stuck_arrows(_sp(-1.0, 0.45), 2.0, seed=2, parent=root)
    return root


# single-figure previews -----------------------------------------------------

def p_hoplites():
    hoplite(_sp(-1.2, 0), VIEW, BLUE, "stand", 0)
    hoplite(_sp(0, 0), VIEW + 0.5, BLUE, "attack", 1)
    hoplite(_sp(1.3, 0), VIEW, RED, "walk", 2, cloak=True)


def p_villagers():
    for i, job in enumerate(("idle", "farm", "carry", "hammer")):
        villager(_sp(-1.6 + i * 1.1, 0), VIEW, BLUE if i % 2 == 0 else RED, seed=i, job=job)


def p_egypt():
    egyptian_spearman(_sp(-1.7, 0), VIEW, RED, "stand", 0)
    egyptian_spearman(_sp(-0.6, 0), VIEW + 0.5, RED, "attack", 1, weapon="khopesh")
    egyptian_archer(_sp(0.5, 0), VIEW + 0.7, RED, "attack", 2)
    egyptian_archer(_sp(1.6, 0), VIEW, BLUE, "stand", 3)


def p_misc():
    archer(_sp(-1.7, 0), VIEW, BLUE, "stand", 0)
    archer(_sp(-0.6, 0), VIEW + 0.7, BLUE, "attack", 1)
    priest(_sp(0.5, 0), VIEW, RED, seed=1)
    priest(_sp(1.6, 0), VIEW, BLUE, seed=2, style="greek")


def p_beasts():
    ox(_sp(-1.5, 0.3), 0.3, BLUE)
    donkey(_sp(0, -0.3), 0.3, RED, seed=1)
    horseman(_sp(1.5, 0.3), 0.3, BLUE, "attack", 0)


def p_chariot():
    chariot((0, 0, 0), 0.3, RED, "attack", 0)


def p_galley():
    galley((0, 0, 0), 0.3, BLUE)


def p_fallen():
    fallen(_sp(-1.2, 0), 0.3, BLUE, seed=0)
    fallen(_sp(0.3, 0), 2.0, RED, seed=1, kind="egyptian")
    dropped_shield(_sp(1.3, -0.4), 0, BLUE, seed=0)
    dropped_shield(_sp(1.4, 0.6), 0.4, RED, seed=1, kind="egyptian")
    stuck_arrows(_sp(-0.3, -0.9), 0)

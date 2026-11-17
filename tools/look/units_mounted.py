"""Mounted and heavy units, animals and siege engines in the new look
(docs/look/units.md): horsemen, a chariot, a war elephant, the gazelle and
the three siege engines, exported as turning animated colour sets through
unitexport.py like units.py's foot figures.

A beast is a `QuadRig`: a body frame (pitched, rolled), four legs with
two-bone IK to hoof targets (the knee or hock bending toward its pole), a
neck and head, a tail. Its motion is a flat dict of numbers, so anim.keyed()
blends it like a person's pose; `gait()` makes the walk: each foot planted
for `duty` of the 0.8 s cycle while it moves back under the body at the
unit's speed (no sliding), swung forward between; the body as high as its
planted legs allow, front and hind apart, which pitches it as a canter does.
A rider is a units.Rig seated on the saddle (its hips there, feet down the
flanks, the left hand at the reins), so every foot unit's body part is
reused. Engines are rigid timber parts posed per frame (wheels turned by
the ground speed; an arm, a beam or a bowstring), collapsing into their
parts when they die.

    /usr/bin/python3.13 tools/look/units_mounted.py NAME OUT.png [anim,anim] [S,W]

renders a contact sheet through the exporter's scene without exporting.
"""
import math
import os
import random
import sys

import bpy  # noqa: F401  (before bmesh)
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import anim  # noqa: E402
import lib  # noqa: E402
import units  # noqa: E402
from figures import B, _arrow, _aspis, _bow, bronze, gold, nv  # noqa: E402
from lib import mat  # noqa: E402
from units import PLAYER_SHADES, R, S, T, pm, two_bone  # noqa: E402,F401

V = Vector
I4 = Matrix.Identity(4)
PERIOD = 0.8                     # the walk's cycle: 8 frames x 100 ms
ANIMS = [(n,) + anim.FOOT[n] for n in ("idle", "walk", "attack", "death", "decay")]
LEGS = ("LF", "RF", "LH", "RH")


def side(k):
    return 1.0 if k[0] == "L" else -1.0


def rot_about(axis, deg, pivot):
    p = V(pivot)
    return T(p) @ R(axis, deg) @ T(-p)


# ------------------------------------------------------------------ beasts

class Spec:
    """A four-legged beast's proportions (tiles; the horse's by default).
    `body` is the barrel's profile, rump to chest: (x, up, half height, half
    width) in the body frame, whose origin is the barrel's centre."""

    def __init__(self, **kw):
        self.fx, self.hx = 0.27, -0.29          # leg tops along the body
        self.ty, self.tz = 0.085, -0.10         # their half spread, height in the body
        self.fleg, self.hleg = (0.29, 0.28), (0.29, 0.28)
        self.hoof = 0.05                        # fetlock to the ground
        self.stand = 0.60                       # leg tops' height standing
        self.rz, self.ry = 0.20, 0.165
        self.body = [(-0.45, 0.04, 0.07, 0.06), (-0.40, 0.03, 0.17, 0.14),
                     (-0.30, 0.02, 0.215, 0.17), (-0.12, 0.0, 0.205, 0.165),
                     (0.08, -0.005, 0.20, 0.162), (0.25, 0.02, 0.205, 0.155),
                     (0.36, 0.05, 0.17, 0.13), (0.43, 0.08, 0.08, 0.07)]
        self.leg_r = (0.056, 0.036, 0.042)      # upper, lower, hoof
        self.neck0 = (0.34, 0.07)               # the neck's root (x, up) in the body
        self.neck = (0.36, 55, 0.13, 0.085)     # length, angle up, radii
        self.head = (0.30, -45, 0.085, 0.06)
        self.tail0 = (-0.44, 0.11)
        self.k = 1.0
        self.__dict__.update(kw)

    def scaled(self, k):
        s = Spec(**dict(self.__dict__))
        for a in ("fx", "hx", "ty", "tz", "hoof", "stand", "rz", "ry"):
            setattr(s, a, getattr(self, a) * k)
        s.fleg = tuple(v * k for v in self.fleg)
        s.hleg = tuple(v * k for v in self.hleg)
        s.body = [tuple(v * k for v in r) for r in self.body]
        s.leg_r = tuple(v * k for v in self.leg_r)
        s.neck0 = tuple(v * k for v in self.neck0)
        s.neck = (self.neck[0] * k, self.neck[1], self.neck[2] * k, self.neck[3] * k)
        s.head = (self.head[0] * k, self.head[1], self.head[2] * k, self.head[3] * k)
        s.tail0 = tuple(v * k for v in self.tail0)
        s.k = self.k * k
        return s

    def top(self, k):
        return V((self.fx if k[1] == "F" else self.hx, side(k) * self.ty, self.tz))

    def leg(self, k):
        return self.fleg if k[1] == "F" else self.hleg


HORSE = Spec()


def qpose(sp, **kw):
    """A beast standing square: a flat dict (anim.keyed blends it)."""
    q = dict(bx=0.0, by=0.0, bz=sp.stand - sp.tz, pitch=0.0, roll=0.0, yaw=0.0,
             nk=0.0, hp=0.0, hy=0.0, ts=0.0, tu=0.0, ear=0.0, jaw=0.0, trunk=0.0, tk2=0.0)
    for k in LEGS:
        t = sp.top(k)
        q[k + "x"], q[k + "y"], q[k + "z"], q[k + "f"], q[k + "w"] = t.x, 0.0, 0.0, 0.0, 0.0
    q.update(kw)
    return q


_TRACKS = {}


def _track(sp, speed, phases, duty, n=96):
    """The front and hind leg tops' heights over the cycle: as high as the
    planted legs reach (standing at most), interpolated across the moments
    no foot of the pair is down, with a little lift there (a flight)."""
    key = (id(sp), speed, tuple(sorted(phases.items())), duty)
    if key in _TRACKS:
        return _TRACKS[key]
    half = speed * PERIOD * duty / 2
    out = []
    for pair in ("F", "H"):
        lu, ll = sp.leg("x" + pair)
        reach = (lu + ll) * 0.985
        vals = []
        for j in range(n):
            t = j / n
            h = None
            for k, ph in phases.items():
                if k[1] != pair:
                    continue
                u = (t - ph) % 1.0
                if u < duty:
                    dx = half - 2 * half * u / duty
                    hh = math.sqrt(max(0.0, reach * reach - dx * dx)) + sp.hoof
                    h = hh if h is None else min(h, hh)
            vals.append(None if h is None else min(h, sp.stand))
        # fill the gaps (no foot down) between their ends, with a lift
        known = [j for j in range(n) if vals[j] is not None]
        filled = list(vals)
        for j in range(n):
            if vals[j] is not None:
                continue
            a = max([i for i in known if i < j], default=known[-1] - n)
            c = min([i for i in known if i > j], default=known[0] + n)
            u = (j - a) / (c - a)
            va, vc = vals[a % n], vals[c % n]
            filled[j] = va + (vc - va) * u + 0.035 * sp.k * math.sin(math.pi * u)
        out.append(filled)
    _TRACKS[key] = out
    return out


def _sample(vals, t):
    n = len(vals)
    x = (t % 1.0) * n
    i = int(x)
    f = x - i
    return vals[i % n] * (1 - f) + vals[(i + 1) % n] * f


def gait(sp, t, speed, phases, duty, lift=(0.20, 0.15), fold=(100, 45), base=None):
    """The beast's walk (or canter, or bound) at t: docstring above."""
    q = dict(base or qpose(sp))
    half = speed * PERIOD * duty / 2
    for k, ph in phases.items():
        i = 0 if k[1] == "F" else 1
        u = (t - ph) % 1.0
        if u < duty:
            x, z, f = half - 2 * half * u / duty, 0.0, 0.0
        else:
            s = (u - duty) / (1 - duty)
            x = -half + 2 * half * anim.smooth(s)
            z = lift[i] * sp.k * math.sin(math.pi * s) ** 0.7
            f = fold[i] * math.sin(math.pi * min(1.0, s * 1.3)) ** 1.2
        q[k + "x"] = sp.top(k).x + x
        q[k + "z"], q[k + "f"] = z, f
    hf, hh = (_sample(v, t) for v in _track(sp, speed, phases, duty))
    q["pitch"] = math.degrees(math.atan2(hf - hh, sp.fx - sp.hx))
    q["bz"] = (hf + hh) / 2 - sp.tz
    return q


class QuadRig:
    """A beast's joints in a pose `q`, in the frame M (the figure's)."""

    def __init__(self, sp, q, M=I4):
        self.sp, self.q, self.M = sp, q, M
        self.pitch = q["pitch"]
        self.Bf = (M @ T(q["bx"], q["by"], q["bz"]) @ R("Z", q["yaw"]) @ R("X", q["roll"])
                   @ R("Y", -q["pitch"]))
        B3 = self.Bf.to_3x3()
        M3 = M.to_3x3()
        self.legs = {}
        for k in LEGS:
            lu, ll = sp.leg(k)
            top = self.Bf @ sp.top(k)
            ank = M @ V((q[k + "x"], side(k) * sp.ty + q[k + "y"], q[k + "z"] + sp.hoof))
            w = q[k + "w"]
            if w > 0:                            # lying: the legs out from the body
                d = V((0.25 if k[1] == "F" else -0.15, 0.0, -1.0)).normalized()
                loc = self.Bf @ (sp.top(k) + d * (lu + ll) * 0.93)
                ank = ank.lerp(loc, w)
            pole = B3 @ V((1.0 if k[1] == "F" else -1.0, 0, 0))
            joint, end = two_bone(top, ank, lu, ll, pole)
            down = (M3 @ V((0, 0, -1))).lerp((end - joint).normalized(), max(w, 0.0)).normalized()
            lat = (B3 @ V((0, 1, 0))).normalized()
            hd = Matrix.Rotation(math.radians(q[k + "f"]), 3, lat) @ down
            self.legs[k] = (top, joint, end, end + hd * sp.hoof)
        n0 = V((sp.neck0[0], 0, sp.neck0[1]))
        self.N = self.Bf @ T(n0) @ R("Z", q["hy"])
        nl, na, _, _ = sp.neck
        a = math.radians(na + q["nk"])
        self.nd = V((math.cos(a), 0, math.sin(a)))
        self.n1 = self.nd * nl                  # in N
        hl, ha, _, _ = sp.head
        h = math.radians(ha + q["hp"] + q["nk"] * 0.4)
        self.hd = V((math.cos(h), 0, math.sin(h)))
        self.h1 = self.n1 + self.hd * hl
        self.mouth = self.N @ (self.h1 - V((0, 0, sp.head[3] * 0.3)))


def draw_barrel(b, qr, m, scale=1.0):
    with b.at(qr.Bf @ R("Y", 90), True):
        b.loft([(-up, 0, x, hz * scale, hy * scale) for x, up, hz, hy in qr.sp.body], m, n=14)


def draw_legs(b, qr, coat, hoof_m, upper=None, sock=None, lower_m=None):
    sp = qr.sp
    r0, r1, rh = sp.leg_r
    for k, (top, joint, end, toe) in qr.legs.items():
        b.seg(top, joint, r0, r0 * 0.75, upper or coat, n=8)
        b.ball(joint, r0 * 0.74, upper or coat, n=8)
        lm = sock if (sock and k in ("LF", "RH")) else (lower_m or coat)
        b.seg(joint, end, r1 * 1.05, r1 * 0.9, lm, n=8)
        b.ball(end, r1 * 1.05, lm, n=6)
        b.seg(end, toe, r1 * 0.95, rh, hoof_m, n=8)


def draw_tail(b, qr, m, r=(0.03, 0.05), length=0.40, hair=True):
    """Hanging, or streaming back with `tu`, swished sideways with `ts`."""
    sp, q = qr.sp, qr.q
    t0 = qr.Bf @ V((sp.tail0[0], 0, sp.tail0[1]))
    pts = [t0]
    up = q["tu"]
    for i in range(1, 5):
        u = i / 4
        ang = math.radians(12 + 62 * up - 25 * u * (1 - up) + 8 * u)
        sw = math.radians(q["ts"] * u)
        seg = length * sp.k / 4
        d = V((-math.sin(ang), math.sin(sw) * 0.8, -math.cos(ang)))
        d = (qr.Bf.to_3x3() @ R("Y", q["pitch"]).to_3x3() @ d).normalized()
        pts.append(pts[-1] + d * seg)
    b.chain(pts, r[0] * sp.k, r[1] * sp.k, m, n=6, sy=0.7)
    if hair:
        b.ball(pts[-1], r[1] * sp.k * 0.9, m, scale=(1, 0.7, 1.4), n=6)


def draw_horse(b, qr, coat, dark, sock=None, blaze=True, mane_m=None, ears=True):
    """The horse: barrel, haunches and shoulders, legs, neck and head, mane
    and tail."""
    sp = qr.sp
    k = sp.k
    cm = mat(coat, rough=0.7)
    dm = mat(dark, rough=0.85)
    hoof = mat((0.16, 0.14, 0.13), rough=0.6)
    draw_barrel(b, qr, cm)
    with b.at(qr.Bf, True):
        for s in (1, -1):
            b.ball((sp.hx - 0.01 * k, s * 0.085 * k, -0.06 * k), 0.12 * k, cm,
                   scale=(1.15, 0.75, 1.45), n=10)
            b.ball((sp.fx, s * 0.08 * k, -0.09 * k), 0.08 * k, cm, scale=(1.05, 0.8, 1.7), n=8)
    draw_legs(b, qr, cm, hoof, sock=mat(sock) if sock else None)
    nl, na, nr0, nr1 = sp.neck
    hl, ha, hr0, hr1 = sp.head
    with b.at(qr.N, True):
        n1, hd, h1 = qr.n1, qr.hd, qr.h1
        b.seg(-qr.nd * 0.05 * k, n1, nr0, nr1, cm, n=10, sy=0.7)
        b.ball(n1, nr1 * 1.05, cm, scale=(1, 0.75, 1), n=8)
        b.seg(n1, h1, hr0, hr1, cm, n=10, sy=0.78)
        b.ball(h1, hr1 * 1.08, cm, scale=(1, 0.8, 1), n=8)
        upv = V((-hd.z, 0, hd.x))
        for s in (1, -1):
            b.ball(n1 + hd * hl * 0.25 + upv * hr0 * 0.45 + V((0, s * hr0 * 0.68, 0)), 0.018 * k,
                   "black", n=5)
            if ears:
                e0 = n1 + upv * hr0 * 0.75 + V((0, s * hr0 * 0.4, 0)) - hd * 0.01 * k
                ed = (upv + V((0, s * 0.25, 0)) - hd * (0.25 + 0.3 * qr.q["ear"])).normalized()
                b.seg(e0, e0 + ed * 0.075 * k, 0.022 * k, 0.005 * k, cm, n=5, sy=0.6)
        if blaze:
            b.seg(n1 + hd * hl * 0.15 + upv * hr0 * 0.75, h1 + upv * hr1 * 0.7, 0.025 * k,
                  0.022 * k, "white", n=5, sy=0.5)
        b.ball(h1 - hd * 0.02 * k, hr1 * 0.95, mat(_dk(coat, 0.55)), scale=(0.9, 0.8, 0.85), n=6)
        mb = V((-qr.nd.z, 0, qr.nd.x))       # the back of the neck: the mane
        mm = mane_m or dm
        b.seg(mb * nr0 * 0.8 - qr.nd * 0.04 * k, n1 + mb * nr1 * 0.95 + qr.nd * 0.02 * k,
              0.045 * k, 0.035 * k, mm, n=6, sy=0.45)
        b.ball(n1 + upv * hr0 * 0.85 + hd * 0.03 * k, 0.03 * k, mm, n=6)
    draw_tail(b, qr, dm)


def _dk(c, f):
    c = lib.PAL[c] if isinstance(c, str) else c
    return tuple(max(0.0, min(1.0, v * f)) for v in c)


def saddle_cloth(b, qr, mats, x0=-0.17, x1=0.16, drop=0.0, flare=1.08):
    """A cloth over the back: loft rings over the barrel, `mats` one per band.
    `drop` (tiles) hangs its edge lower as a skirt."""
    sp = qr.sp
    k = sp.k
    xs = [x0 * k + (x1 - x0) * k * i / (len(mats)) for i in range(len(mats) + 1)]
    rings = []
    for x in xs:
        up, hz, hy = _profile(sp, x)
        rings.append((-up, 0, x, hz * flare + 0.006, hy * flare + 0.008))
    with b.at(qr.Bf @ R("Y", 90), True):
        b.loft(rings, mats, n=14, caps=False)
    if drop > 0:
        drape(b, qr, x0 * k, x1 * k, drop, mats[len(mats) // 2])


def _profile(sp, x):
    """The barrel's (up, half height, half width) at x."""
    prof = sp.body
    for (xa, ua, za, ya), (xb, ub, zb, yb) in zip(prof, prof[1:]):
        if xa <= x <= xb:
            u = (x - xa) / (xb - xa)
            return ua + (ub - ua) * u, za + (zb - za) * u, ya + (yb - ya) * u
    return prof[-1][1:] if x > prof[-1][0] else prof[0][1:]


def drape(b, qr, x0, x1, depth, m, flare=1.12, top_up=0.0, hem=None):
    """A caparison: a cloth tube around the barrel from its middle line down
    to `depth` below it (body frame), its hem a little wider (and banded
    with `hem`)."""
    sp = qr.sp
    _, hz, hy = _profile(sp, (x0 + x1) / 2)
    rx = (x1 - x0) / 2
    rings = [(0.0, 0.0, top_up, rx, hy * flare),
             (0.0, 0.0, top_up - depth * 0.5, rx * 1.02, hy * flare * 1.03),
             (0.0, 0.0, top_up - depth, rx * 1.05, hy * flare * 1.07)]
    with b.at(qr.Bf @ T((x0 + x1) / 2, 0, 0), True):
        b.loft(rings, m, n=16, caps=False)
        if hem is not None:
            hb = 0.045 * sp.k
            b.loft([(0.0, 0.0, top_up - depth + hb, rx * 1.045 + 0.006, hy * flare * 1.065 + 0.006),
                    (0.0, 0.0, top_up - depth - 0.004, rx * 1.055 + 0.006,
                     hy * flare * 1.075 + 0.006)], hem, n=16, caps=False)


# ----------------------------------------------------------------- riders

def seat_pose(rig_q, bd, seat, pitch, base):
    """A rider's pose with the hips on `seat` (world), pitched with the horse."""
    p = dict(base)
    p.update(x=seat.x, y=seat.y, z=seat.z - bd.stand + p.get("drop", 0.0), fallx=0.0,
             fallz=bd.stand - p.get("drop", 0.0), fall=-pitch * 0.55 + base.get("fall", 0.0))
    return p


SEATED = dict(FLx=0.05, FLy=0.15, FLz=0.13, FRx=0.05, FRy=0.15, FRz=0.13, FLp=-10, FRp=-10,
              ikL=1.0, Lx=0.24, Ly=0.07, Lz=0.10, Lf=30, Rf=20, Ra=14, Re=40)


def weapon(b, M, kind, player=None):
    """A rider's weapon in its frame (origin at the grip, X along it)."""
    wood, dark = mat("wood", rough=0.8), mat("wood_dark", rough=0.8)
    iron = mat((0.72, 0.74, 0.78), rough=0.35, metal=0.6)
    with b.at(M, True):
        if kind == "lance":
            b.seg((-0.55, 0, 0), (1.25, 0, 0), 0.020, 0.017, wood, n=6)
            b.seg((1.24, 0, 0), (1.33, 0, 0), 0.016, 0.045, iron, n=6, sy=0.45)
            b.seg((1.33, 0, 0), (1.50, 0, 0), 0.045, 0.002, iron, n=6, sy=0.45)
            b.seg((-0.55, 0, 0), (-0.62, 0, 0), 0.018, 0.004, bronze(), n=6)
            if player is not None:          # a pennon behind the head
                b.plate([(1.05, 0.0), (1.20, 0.0), (1.12, -0.14), (1.02, -0.20)], 0.012,
                        pm(player), frame=R("X", 90))
        elif kind == "spear":
            b.seg((-0.40, 0, 0), (0.78, 0, 0), 0.018, 0.016, wood, n=6)
            b.seg((0.77, 0, 0), (0.85, 0, 0), 0.014, 0.042, bronze(), n=6, sy=0.45)
            b.seg((0.85, 0, 0), (0.98, 0, 0), 0.042, 0.002, bronze(), n=6, sy=0.45)
        elif kind == "sword":
            b.seg((-0.07, 0, 0), (0.02, 0, 0), 0.022, 0.022, mat("leather"), n=6)
            b.cube((0.03, 0, 0), (0.025, 0.11, 0.03), bronze())
            b.plate([(0.04, -0.028), (0.30, -0.035), (0.42, 0.0), (0.36, 0.03), (0.04, 0.026)],
                    0.016, iron)
        elif kind == "goad":
            b.seg((-0.15, 0, 0), (0.50, 0, 0), 0.016, 0.014, dark, n=6)
            b.seg((0.50, 0, 0), (0.58, 0, 0), 0.02, 0.003, bronze(), n=6)
            b.seg((0.48, 0, 0), (0.52, 0, -0.07), 0.012, 0.003, bronze(), n=5)


def cloak(b, r, m, fly=0.5, length=0.40, width=0.32):
    """A cloak from the shoulders, streaming back as the rider gallops."""
    vs, fs = [], []
    nr, nc = 5, 5
    for i in range(nr):
        t = i / (nr - 1)
        for j in range(nc):
            u = j / (nc - 1) - 0.5
            w = width + 0.12 * t
            x = -0.10 - 0.04 * t - fly * t * t * 0.30 - 0.035 * (1 - (2 * u) ** 2) * (0.5 + t)
            z = r.bd.torso - (length - fly * 0.16) * t
            vs.append((x, u * w, z))
    for i in range(nr - 1):
        for j in range(nc - 1):
            k = i * nc + j
            fs.append((k, k + 1, k + nc + 1, k + nc))
    with b.at(r.U, True):
        b.poly(vs, fs, m, smooth=True)
        b.poly([(x - 0.004, y, z) for x, y, z in vs], [tuple(reversed(f)) for f in fs], m,
               smooth=True)


def bigbow(b, grip, aim, nock=None, h=0.34, m=None, arrow=True):
    """figures._bow drawn thick enough to read at game size: a recurved
    composite bow held at `grip`, drawn to `nock` if given."""
    m = m or mat((0.30, 0.18, 0.10), rough=0.6)
    aim = nv(aim)
    grip = V(grip)
    up = V((0, 0, 1)) - aim * aim.z
    up = up.normalized() if up.length > 1e-4 else V((1, 0, 0))
    c = 0.20 if nock is not None else 0.11
    pts = []
    for i in range(9):
        t = -1 + i / 4
        off = -c * t * t + 0.9 * max(0.0, abs(t) - 0.6) ** 2
        pts.append(grip + up * (t * h) + aim * off)
    b.chain(pts[:5], 0.016, 0.03, m, n=6)
    b.chain(pts[4:], 0.03, 0.016, m, n=6)
    for e in (pts[0], pts[-1]):
        b.ball(e, 0.022, gold(), n=6)
    sm = mat((0.95, 0.92, 0.82))
    if nock is not None:
        nock = V(nock)
        b.seg(pts[0], nock, 0.007, 0.007, sm, n=4)
        b.seg(pts[-1], nock, 0.007, 0.007, sm, n=4)
        if arrow:
            d = nv((grip + aim * 0.02) - nock)
            L = (grip - nock).length + 0.10
            b.seg(nock, nock + d * L, 0.012, 0.012, "straw", n=5)
            b.seg(nock + d * L, nock + d * (L + 0.07), 0.026, 0.002, "iron", n=5)
            b.seg(nock, nock + d * 0.09, 0.03, 0.02, "white", n=5, sy=0.3)
    else:
        b.seg(pts[0], pts[-1], 0.007, 0.007, sm, n=4)


def reins(b, r, mouth, M3, hand="L"):
    h = r.hand[hand]
    lat = M3 @ V((0, 1, 0))
    for s in (1, -1):
        b.seg(h, mouth + lat * s * 0.035, 0.007, 0.007, mat("leather"), n=4)


class Beast(units.Figure):
    """A figure built on a QuadRig (and maybe a rider): subclasses supply
    quad(name, t) -> q, draw(b, name, t, q, player)."""

    animations = ANIMS
    cls = "Mounted"
    size = 1.0                  # the whole figure scaled (its gait corrected for it)

    def build(self, name, i, player, parent=None):
        n, loops = next((n, lp) for a, n, _, lp, _ in self.animations if a == name)
        t = anim.frame_t(i, n, loops)
        sink = -0.05 * t if name == "decay" else 0.0
        root = lib.empty("unit", (0, 0, sink), 0.0, parent)
        root.scale = (self.size,) * 3
        b = B("unit")
        self.draw(b, name, t, player)
        b.finish(root)
        return root


# A canter for the horses: the right hind strikes, then the left hind and
# right fore, then the leading left fore; a moment of flight before the
# right hind again. Each foot down 40% of the cycle.
CANTER = {"RH": 0.0, "LH": 0.10, "RF": 0.20, "LF": 0.30}


def horse_death(sp, t, base, roll_side=1.0):
    """Stumbles, the forelegs go, it rolls on its right side; the last
    frame lies still (the decay's first matches it)."""
    s = roll_side
    stumble = dict(base, pitch=-10, bz=base["bz"] - 0.08 * sp.k, bx=0.06 * sp.k, nk=-25, hp=-10,
                   LFf=60, RFf=40, LFz=0.04, ts=10)
    for kk in ("LF", "RF"):
        stumble[kk + "x"] = base[kk + "x"] + 0.10 * sp.k
    kneel = dict(stumble, pitch=-14, bz=base["bz"] - 0.28 * sp.k, roll=18 * s, bx=0.04 * sp.k,
                 nk=-30, hp=-5, LFw=0.5, RFw=0.5, LHw=0.25, RHw=0.25, by=-0.05 * s * sp.k)
    lying = lying_q(sp, base, s)
    return anim.keyed([(0.0, base), (0.2, stumble, anim.ease_out), (0.5, kneel),
                       (0.8, lying, anim.ease_in), (1.0, lying)], t, loop=False)


def lying_q(sp, base, s=1.0):
    q = dict(base, pitch=0.0, roll=84 * s, bz=sp.ry * 1.0 + 0.01 * sp.k, by=-0.10 * s * sp.k,
             bx=0.02 * sp.k, nk=-48, hp=12, hy=0, ts=-20, tu=0.1)
    for k in LEGS:
        q[k + "w"] = 1.0
        q[k + "f"] = 15.0
    return q


class Horseman(Beast):
    """A horse and its rider. Subclasses set the coat, the rider's outfit,
    gear and attack."""

    cls = "Mounted"
    speed = 2.0
    size = 1.12                 # a rider stands out over the foot soldiers
    horse = HORSE
    coat, dark, sock = "cedar", "hair_black", None
    weapon_kind = None
    lean = 4.0                  # the rider's lean at the gallop

    def __init__(self):
        super().__init__()
        self.bd = units.Build()
        self.skin = lib.PAL["skin_tan"]

    # --- the horse's motion
    def quad(self, name, t):
        sp = self.horse
        base = qpose(sp, LFx=sp.fx + 0.02, RFx=sp.fx - 0.02, LHx=sp.hx - 0.01, RHx=sp.hx + 0.03)
        if name == "idle":
            s, c = math.sin(2 * math.pi * t), math.cos(2 * math.pi * t)
            return dict(base, nk=-4 + 5 * s, hp=4 * c, ts=18 * c, tu=0.05 + 0.08 * max(0, s),
                        ear=0.5 * max(0.0, c), RHz=0.01, RHf=25, RHx=sp.hx + 0.06, hy=3 * s)
        if name == "walk":
            q = gait(sp, t, self.speed / self.size, CANTER, 0.40, base=base)
            q["nk"] = -6 + 7 * math.sin(2 * math.pi * (t - 0.30))
            q["hp"] = 6 * math.sin(2 * math.pi * (t - 0.1))
            q["tu"] = 0.5 + 0.1 * math.sin(2 * math.pi * t)
            q["ts"] = 6 * math.sin(2 * math.pi * t)
            q["ear"] = 1.0
            return q
        if name == "attack":
            return self.quad_attack(t, base)
        if name == "death":
            return horse_death(sp, t, base)
        return lying_q(sp, base)

    def quad_attack(self, t, base):
        """A melee rider's horse: half rears on the wind-up, lunges into
        the blow (frame 3), settles."""
        sp = self.horse
        rear = dict(base, pitch=9, bz=base["bz"] + 0.0, bx=-0.03, LFz=0.14, LFf=95,
                    LFx=sp.fx + 0.06, nk=10, hp=-6, tu=0.4, ear=1.0)
        lunge = dict(base, pitch=-4, bx=0.07, bz=base["bz"] - 0.03, nk=-8, hp=4, tu=0.6,
                     LFx=sp.fx + 0.13, ear=1.0, RHx=sp.hx - 0.06)
        return anim.keyed([(0.0, base), (0.4, rear), (0.6, lunge, anim.ease_in),
                           (1.0, anim.lerp(lunge, base, 0.6))], t, loop=False)

    def seat(self, qr):
        sp = self.horse
        return qr.Bf @ V((-0.03 * sp.k, 0, _profile(sp, -0.03)[0] + _profile(sp, -0.03)[1] + 0.03))

    # --- the rider
    def rider_base(self):
        return anim.pose(SEATED)

    def rider(self, name, t, qr):
        base = self.rider_base()
        if name == "idle":
            p = anim.breathe(base, t, 0.7)
            p.update(FLx=base["FLx"], FRx=base["FRx"], FLy=base["FLy"], FRy=base["FRy"])
        elif name == "walk":
            p = dict(base)
            p["lean"] += self.lean + 3 * math.sin(2 * math.pi * (t - 0.35))
            p["drop"] = 0.0
            p["hpitch"] -= self.lean * 0.6
            p = self.rider_walk(p, t)
        elif name == "attack":
            p = self.rider_attack(t, base)
        elif name in ("death", "decay"):
            return self.rider_fall(t if name == "death" else 1.0, base, qr)
        else:
            p = base
        return seat_pose(None, self.bd, self.seat(qr), qr.pitch, p)

    def rider_walk(self, p, t):
        return p

    def rider_fall(self, t, base, qr):
        """Thrown off over the horse's back as it rolls, lands on its back
        beside it."""
        seated = seat_pose(None, self.bd, self.seat(QuadRig(self.horse, self.quad("idle", 0))),
                           0.0, base)
        k = self.horse.k
        lying = anim.fallen(anim.pose())
        lying.update(x=-0.22 * k, y=-0.62 * k, z=0.0, hyaw=-30)
        thrown = dict(anim.lerp(seated, lying, 0.45), z=seated["z"] * 0.7 + 0.05,
                      roll=-25, fall=-35, Lf=120, La=60, Rf=140, Ra=70, ikL=0.0)
        return anim.keyed([(0.0, dict(seated, hpitch=-15, lean=-12)), (0.3, thrown, anim.ease_out),
                           (0.62, lying, anim.ease_in), (1.0, lying)], t, loop=False)

    def rider_attack(self, t, base):
        return base

    def gear(self, name):
        return self.weapon_kind

    def draw(self, b, name, t, player):
        q = self.quad(name, t)
        qr = QuadRig(self.horse, q)
        self.draw_mount(b, qr, player, name)
        p = self.rider(name, t, qr)
        tool = self.gear(name)
        rr = units.Rig(self.bd, p, tool, False)
        self.outfit(b, rr, player, name, t)
        if tool and (name not in ("death", "decay") or (name == "death" and t < 0.3)):
            weapon(b, rr.tool, tool, player)
        elif tool:          # dropped: lying in the grass beside the rider
            k = self.horse.k
            weapon(b, T(-0.05 * k, -0.40 * k, 0.025) @ R("Z", 160) @ R("Y", -2), tool, player)
        if name not in ("death", "decay") and p.get("ikL", 0) > 0.5:
            reins(b, rr, qr.mouth, qr.Bf.to_3x3())

    def draw_mount(self, b, qr, player, name):
        draw_horse(b, qr, self.coat, self.dark, sock=self.sock)
        self.tack(b, qr, player)

    def tack(self, b, qr, player):
        saddle_cloth(b, qr, [gold(), pm(player), pm(player), gold()], x0=-0.20, x1=0.14)


# ------------------------------------------------------------------- scout

class Scout(Horseman):
    """Light and fast: a small pale horse with no armour, the rider in a
    tunic and broad straw hat, cloak streaming, a short spear."""

    speed = 1.6
    horse = HORSE.scaled(0.9)
    coat, dark, sock = (0.88, 0.70, 0.42), (0.96, 0.92, 0.80), "white"
    weapon_kind = "spear"
    lean = 12.0

    def draw_mount(self, b, qr, player, name):
        draw_horse(b, qr, self.coat, self.dark, sock=self.sock, mane_m=mat((0.96, 0.92, 0.80)))
        saddle_cloth(b, qr, [pm(player, 0.82), pm(player)], x0=-0.16, x1=0.10)

    def rider_base(self):
        return anim.pose(SEATED, Rf=30, Ra=20, Re=40, Rt=0, aimw=1, Tx=0.6, Ty=-0.1, Tz=0.8)

    def rider_walk(self, p, t):
        p.update(Rf=40, Re=45, Tx=0.8, Ty=-0.1, Tz=0.65)
        return p

    def rider_attack(self, t, base):
        ready = anim.pose(base, Rf=70, Ra=75, Re=95, Rt=-30, Tx=1.0, Ty=0.0, Tz=-0.25, lean=4)
        drawn = anim.pose(base, Rf=20, Ra=80, Re=115, Rt=-40, Tx=1.0, Ty=0.1, Tz=-0.2, lean=-4,
                          twist=-14)
        thrust = anim.pose(base, Rf=90, Ra=40, Re=10, Rt=-20, Tx=1.0, Ty=-0.1, Tz=-0.45, lean=16,
                           twist=10)
        return anim.keyed([(0.0, ready), (0.4, drawn), (0.6, thrust, anim.ease_in),
                           (1.0, anim.lerp(thrust, ready, 0.6))], t, loop=False)

    def outfit(self, b, r, player, name, t):
        skin = mat(self.skin, rough=0.75)
        cloth = pm(player)
        leather = mat("leather", rough=0.8)
        units.draw_body(b, r, skin, foot=leather, upper=cloth, thigh=pm(player, 0.82))
        units.draw_torso(b, r, [cloth] * 4, 1.0)
        with b.at(r.P, True):
            b.loft([(0, 0, 0.065, .088, .12), (0, 0, 0.105, .086, .117)], leather, n=12, caps=False)
        units.draw_head(b, r, skin, mat("hair_brown", rough=0.9), "short", beard=False)
        k = r.bd.head_r / 0.115
        with b.at(r.H @ S(k, k, k), True):     # the petasos: a broad sun hat
            hm = mat("straw", rough=0.9)
            b.seg((-0.01, 0, 0.065), (-0.01, 0, 0.085), 0.22, 0.19, hm, n=14)
            b.ball((-0.01, 0, 0.09), 0.11, hm, scale=(1, 1, 0.75), n=10)
            b.seg((-0.01, 0, 0.088), (-0.01, 0, 0.11), 0.108, 0.104, cloth, n=12)
        fly = 0.9 if name in ("walk", "attack") else 0.15
        if name not in ("death", "decay"):
            cloak(b, r, pm(player, 0.82), fly=fly, length=0.40, width=0.30)


# ----------------------------------------------------------- light cavalry

class LightCavalry(Horseman):
    """A bay horse under a saddle cloth, the rider helmeted with a small
    round shield and a sword (the raider)."""

    speed = 2.0
    coat, dark, sock = (0.52, 0.27, 0.14), "hair_black", None
    weapon_kind = "sword"
    lean = 8.0

    def draw_mount(self, b, qr, player, name):
        draw_horse(b, qr, self.coat, self.dark, sock=self.sock)
        sp = qr.sp
        saddle_cloth(b, qr, [gold(), pm(player), pm(player), gold()], x0=-0.20, x1=0.14)
        drape(b, qr, -0.20 * sp.k, 0.14 * sp.k, 0.15 * sp.k, pm(player, 0.82), flare=1.1,
              top_up=0.02 * sp.k, hem=gold())

    def rider_base(self):
        return anim.pose(SEATED, Rf=35, Ra=25, Re=60, Rt=0, aimw=1, Tx=0.45, Ty=-0.15, Tz=0.9,
                         ikL=1.0, Lx=0.24, Ly=0.10, Lz=0.12)

    def rider_walk(self, p, t):
        p.update(Rf=40, Ra=30, Re=70, Tx=0.2, Ty=-0.25, Tz=1.0)
        return p

    def rider_attack(self, t, base):
        up = anim.pose(base, Rf=160, Ra=35, Re=50, Tx=-0.5, Ty=-0.2, Tz=0.85, lean=-6, twist=-16,
                       side=4)
        hit = anim.pose(base, Rf=70, Ra=30, Re=8, Tx=0.85, Ty=-0.35, Tz=-0.45, lean=14,
                        twist=14, side=-6)
        return anim.keyed([(0.0, anim.lerp(base, up, 0.5)), (0.4, up), (0.6, hit, anim.ease_in),
                           (1.0, anim.lerp(hit, base, 0.6))], t, loop=False)

    def outfit(self, b, r, player, name, t):
        skin = mat(self.skin, rough=0.75)
        cloth = pm(player)
        leather = mat("leather", rough=0.8)
        br = bronze()
        units.draw_body(b, r, skin, foot=leather, upper=cloth, thigh=cloth)
        units.draw_torso(b, r, [cloth, cloth, leather, leather], 1.0)
        with b.at(r.P, True):
            b.loft([(0, 0, 0.065, .088, .12), (0, 0, 0.105, .086, .117)], br, n=12, caps=False)
        units.draw_head(b, r, skin, mat("hair_brown", rough=0.9), "short", beard=False)
        k = r.bd.head_r / 0.115
        with b.at(r.H @ S(k, k, k), True):     # a Boeotian helmet: a dome and a flaring brim
            b.ball((-0.005, 0, 0.03), 0.128, br, scale=(1.02, 1.0, 0.95), n=12)
            b.seg((-0.01, 0, 0.0), (-0.01, 0, 0.03), 0.165, 0.13, br, n=12)
            b.seg((-0.01, 0, 0.14), (-0.01, 0, 0.17), 0.03, 0.02, cloth, n=6)
        if name not in ("death", "decay"):
            cloak(b, r, pm(player, 0.82), fly=0.7 if name in ("walk", "attack") else 0.1,
                  length=0.30, width=0.26)
        # a small round shield on the left forearm, facing out and forward
        el, ha = r.el["L"], r.hand["L"]
        mid = el + (ha - el) * 0.45
        n = (r.U.to_3x3() @ V((0.35, 1.0, 0.1))).normalized()
        _aspis(b, mid + n * 0.07, n, player, "none", Rr=0.17)


# ----------------------------------------------------------- heavy cavalry

class HeavyCavalry(Horseman):
    """The shock unit: a big dark horse in a player caparison to its knees
    with a bronze chamfron, the rider in a bronze cuirass and a crested
    helmet with a long lance."""

    speed = 1.8
    horse = HORSE.scaled(1.06)
    coat, dark, sock = (0.20, 0.15, 0.13), (0.08, 0.06, 0.05), None
    weapon_kind = "lance"
    lean = 6.0

    def __init__(self):
        super().__init__()
        self.bd.chest = 1.1
        self.skin = lib.PAL["skin_light"]

    def draw_mount(self, b, qr, player, name):
        draw_horse(b, qr, self.coat, self.dark, blaze=False)
        sp = qr.sp
        k = sp.k
        cloth = pm(player)
        saddle_cloth(b, qr, [cloth] * 3, x0=-0.42, x1=0.40, flare=1.07)
        drape(b, qr, -0.40 * k, 0.36 * k, 0.30 * k, pm(player, 0.82), flare=1.16, hem=gold())
        with b.at(qr.N, True):            # chamfron and crinet: bronze over the face and neck
            hd, n1, h1 = qr.hd, qr.n1, qr.h1
            upv = V((-hd.z, 0, hd.x))
            b.seg(n1 + upv * sp.head[2] * 0.3, h1 + upv * sp.head[3] * 0.5 - hd * 0.04 * k,
                  sp.head[2] * 0.9, sp.head[3] * 0.85, bronze(), n=10, sy=0.9)
            b.seg(n1 + upv * 0.11 * k, n1 + upv * 0.20 * k + hd * 0.02 * k, 0.025 * k, 0.04 * k,
                  cloth, n=6)
            b.seg(V((0, 0, 0)), n1 * 0.92, sp.neck[2] * 1.06, sp.neck[3] * 1.08, pm(player, 0.82),
                  n=10, sy=0.74)

    def rider_base(self):
        return anim.pose(SEATED, Rf=20, Ra=15, Re=70, Rt=0, aimw=1, Tx=0.25, Ty=-0.05, Tz=1.0)

    def rider_walk(self, p, t):     # the lance couched, levelled forward
        p.update(Rf=10, Ra=25, Re=85, Tx=1.0, Ty=0.05, Tz=0.12)
        return p

    def rider_attack(self, t, base):
        ready = anim.pose(base, Rf=10, Ra=25, Re=85, Tx=1.0, Ty=0.05, Tz=0.15, lean=2)
        drawn = anim.pose(base, Rf=-20, Ra=35, Re=95, Tx=1.0, Ty=0.12, Tz=0.25, lean=-8,
                          twist=-18)
        thrust = anim.pose(base, Rf=70, Ra=25, Re=15, Tx=1.0, Ty=-0.05, Tz=-0.12, lean=18,
                           twist=12)
        return anim.keyed([(0.0, ready), (0.4, drawn), (0.6, thrust, anim.ease_in),
                           (1.0, anim.lerp(thrust, ready, 0.6))], t, loop=False)

    def outfit(self, b, r, player, name, t):
        skin = mat(self.skin, rough=0.75)
        cloth = pm(player)
        br = bronze()
        leather = mat("leather", rough=0.8)
        units.draw_body(b, r, skin, shin=br, foot=leather, upper=cloth, thigh=cloth)
        units.draw_skirt(b, r, [cloth, cloth], top=0.08, hem=-0.06, zs=[0, 0.6, 1.0], flare=1.4)
        units.draw_torso(b, r, br, 1.0)
        units.draw_head(b, r, skin, None, "none", face=False)
        units.Hoplite.helmet(None, b, r, cloth)
        if name not in ("death", "decay"):
            cloak(b, r, pm(player, 0.82), fly=0.6 if name in ("walk", "attack") else 0.1,
                  length=0.40, width=0.34)


# ------------------------------------------------------------ horse archer

class HorseArcher(Horseman):
    """A small grey steppe horse under a fringed cloth, the rider in
    trousers and a pointed cap with a bow and a full quiver."""

    speed = 2.0
    horse = HORSE.scaled(0.94)
    coat, dark, sock = (0.80, 0.78, 0.74), (0.30, 0.28, 0.27), None
    lean = 6.0

    def gear(self, name):
        return None

    def rider_base(self):
        # the bow held low in the left hand, the right on the thigh
        return anim.pose(SEATED, ikL=1.0, Lx=0.24, Ly=0.12, Lz=0.12, ikR=1.0, Rx=0.10, Ry=-0.17,
                         Rz=0.05)

    def bow_state(self, name, t):
        """(raise 0..1, draw 0..1, arrow on the string)."""
        if name == "attack":
            raise_ = anim.keyed([(0.0, {"v": 0.6}), (0.2, {"v": 1.0}), (1.0, {"v": 1.0})], t,
                                loop=False)["v"]
            draw = anim.keyed([(0.0, {"v": 0.2}), (0.4, {"v": 1.0}), (0.55, {"v": 1.0}),
                               (0.6, {"v": 0.0}, anim.ease_in), (1.0, {"v": 0.0})], t,
                              loop=False)["v"]
            return raise_, draw, t < 0.55
        if name == "walk":
            return 0.25, 0.0, True
        return 0.0, 0.0, True

    def rider(self, name, t, qr):
        if name in ("death", "decay"):
            return super().rider(name, t, qr)
        p = super().rider(name, t, qr)
        rs, dr, _ = self.bow_state(name, t)
        if rs > 0:
            # the left arm out at the target, the right drawing to the cheek
            p.update(ikL=1.0, Lx=0.16 + 0.20 * rs, Ly=0.10 + 0.04 * rs, Lz=0.12 + 0.26 * rs,
                     ikR=1.0, Rx=0.30 * rs - 0.20 * dr * rs + 0.04, Ry=-0.10 + 0.03 * dr,
                     Rz=0.12 + 0.26 * rs + 0.02 * dr, twist=p["twist"] + 18 * rs,
                     hyaw=p["hyaw"] - 14 * rs)
        return p

    def draw(self, b, name, t, player):
        super().draw(b, name, t, player)

    def outfit(self, b, r, player, name, t):
        skin = mat(self.skin, rough=0.75)
        cloth = pm(player)
        dark = pm(player, 0.64)
        leather = mat("leather", rough=0.8)
        units.draw_body(b, r, skin, shin=dark, foot=leather, upper=cloth, thigh=dark)
        units.draw_torso(b, r, [cloth, cloth, cloth, cloth], 1.0)
        with b.at(r.P, True):
            b.loft([(0, 0, 0.065, .09, .122), (0, 0, 0.105, .088, .119)], gold(), n=12, caps=False)
        units.draw_head(b, r, skin, mat("hair_brown", rough=0.9), "none", beard=False)
        k = r.bd.head_r / 0.115
        with b.at(r.H @ S(k, k, k), True):     # a tall pointed felt cap with lappets
            b.loft([(-0.02, 0, 0.0, 0.128, 0.124), (-0.03, 0, 0.09, 0.105, 0.1),
                    (-0.06, 0, 0.20, 0.03, 0.03)], cloth, n=12)
            b.ball((-0.07, 0, 0.215), 0.03, cloth, n=6)
        if name in ("death", "decay"):
            return
        # the gorytos (bow case and quiver) at the left hip
        with b.at(r.P @ T(-0.06, 0.13, 0.02) @ R("Y", -35), True):
            b.cube((0, 0, 0), (0.07, 0.04, 0.24), gold())
            b.cube((0.0, 0.005, 0.0), (0.06, 0.04, 0.22), leather)
            for i in range(3):
                b.seg((-0.01 + 0.012 * i, 0, 0.11), (-0.02 + 0.012 * i, 0, 0.19), 0.01, 0.012,
                      "white", n=4)
        rs, dr, arrow = self.bow_state(name, t)
        grip = r.hand["L"]
        aim = (r.U.to_3x3() @ V((1.0, 0.25 * (1 - rs), 0.05))).normalized()
        if rs > 0.3:
            nock = r.hand["R"] if dr > 0.05 else None
            if nock is not None:
                bigbow(b, grip, aim, nock=nock, arrow=arrow)
            else:
                bigbow(b, grip, aim)
                if arrow:
                    _arrow(b, grip - aim * 0.3, aim, L=0.38)
        else:
            bigbow(b, grip, (r.U.to_3x3() @ V((0.2, 0.9, 0.3))).normalized())

    def draw_mount(self, b, qr, player, name):
        draw_horse(b, qr, self.coat, self.dark, blaze=False)
        sp = qr.sp
        cloth = pm(player)
        saddle_cloth(b, qr, [cloth, cloth], x0=-0.22, x1=0.12)
        drape(b, qr, -0.22 * sp.k, 0.12 * sp.k, 0.14 * sp.k, pm(player, 0.82), flare=1.1,
              top_up=0.02 * sp.k)

    def quad_attack(self, t, base):
        sp = self.horse
        s = math.sin(math.pi * t)
        return dict(base, nk=-3 + 4 * s, hp=3 * s, ts=10 * s, ear=1.0)


# ------------------------------------------------------------------ gazelle

GAZELLE = Spec(
    fx=0.16, hx=-0.17, ty=0.05, tz=-0.05, fleg=(0.20, 0.20), hleg=(0.21, 0.20), hoof=0.03,
    stand=0.42, rz=0.10, ry=0.09,
    body=[(-0.27, 0.03, 0.05, 0.054), (-0.23, 0.02, 0.10, 0.09), (-0.12, 0.0, 0.105, 0.096),
          (0.04, -0.01, 0.095, 0.09), (0.16, 0.01, 0.10, 0.084), (0.23, 0.04, 0.07, 0.066),
          (0.26, 0.06, 0.035, 0.036)],
    leg_r=(0.030, 0.016, 0.016), neck0=(0.20, 0.04), neck=(0.22, 64, 0.05, 0.034),
    head=(0.15, -28, 0.045, 0.026), tail0=(-0.27, 0.06))
BOUND = {"LH": 0.0, "RH": 0.05, "LF": 0.40, "RF": 0.46}


class Gazelle(Beast):
    """Small, light and tan, white below with a dark flank stripe, lyre
    horns, thin legs; grazes, bounds away, falls. The game shows idle, walk
    (fleeing), death and decay (it has no attack: the set's attack is a head
    toss, never seen)."""

    cls = "Foot"
    speed = 1.4
    size = 1.2
    sp = GAZELLE
    mask_always = True          # no player colour, but the view wants a mask of every unit

    def quad(self, name, t):
        sp = self.sp
        base = qpose(sp, LFx=sp.fx + 0.02, RFx=sp.fx - 0.01, LHx=sp.hx - 0.01, RHx=sp.hx + 0.02)
        if name == "idle":       # grazing: the head down at the grass, chewing, the tail flicking
            c = math.cos(2 * math.pi * t)
            s = math.sin(2 * math.pi * t)
            return dict(base, nk=-88 + 4 * s, hp=-22 + 6 * c, ts=40 * max(0.0, s) ** 3, tu=0.5,
                        ear=0.6 * max(0, c), pitch=-3, bz=base["bz"] - 0.01, LFx=sp.fx + 0.06)
        if name == "walk":
            q = gait(sp, t, self.speed / self.size, BOUND, 0.30, lift=(0.14, 0.12), fold=(110, 60),
                     base=base)
            q["nk"] = 4 * math.sin(2 * math.pi * (t - 0.2))
            q["tu"], q["ear"] = 0.9, 1.0
            return q
        if name == "attack":     # a head toss (unused in the game)
            down = dict(base, nk=-40, hp=-30, pitch=-4, ear=1.0)
            up = dict(base, nk=15, hp=10, pitch=4, ear=1.0)
            return anim.keyed([(0.0, base), (0.4, down), (0.6, up, anim.ease_in), (1.0, base)], t,
                              loop=False)
        if name == "death":
            return horse_death(sp, t, base)
        return lying_q(sp, base)

    def draw(self, b, name, t, player):
        q = self.quad(name, t)
        qr = QuadRig(self.sp, q)
        sp = self.sp
        tan = mat((0.80, 0.55, 0.30), rough=0.8)
        white = mat((0.96, 0.93, 0.86), rough=0.85)
        dark = mat((0.26, 0.17, 0.11), rough=0.85)
        hoof = mat((0.12, 0.10, 0.09), rough=0.6)
        draw_barrel(b, qr, tan)
        with b.at(qr.Bf, True):
            # white belly and rump, a dark stripe between the tan and the white
            b.ball((0.0, 0, -0.045), 0.12, white, scale=(1.65, 0.84, 0.55), n=12)
            b.ball((-0.24, 0, 0.0), 0.05, white, scale=(0.8, 1.35, 1.4), n=8)
            b.ball((0.0, 0, -0.008), 0.12, dark, scale=(1.5, 0.86, 0.12), n=12)
            for s in (1, -1):
                b.ball((sp.hx, s * 0.05, -0.03), 0.06, tan, scale=(1.1, 0.8, 1.4), n=8)
                b.ball((sp.fx, s * 0.045, -0.04), 0.04, tan, scale=(1.0, 0.8, 1.6), n=8)
        draw_legs(b, qr, tan, hoof, lower_m=mat((0.88, 0.70, 0.48), rough=0.8))
        with b.at(qr.N, True):
            n1, hd, h1 = qr.n1, qr.hd, qr.h1
            b.seg(-qr.nd * 0.03, n1, sp.neck[2], sp.neck[3], tan, n=8, sy=0.85)
            b.seg(-qr.nd * 0.02 + V((0.012, 0, -0.01)), n1 + V((0.01, 0, -0.012)), sp.neck[2] * 0.7,
                  sp.neck[3] * 0.6, white, n=8, sy=0.8)
            b.ball(n1, sp.head[2] * 1.05, tan, n=8)
            b.seg(n1, h1, sp.head[2], sp.head[3], tan, n=8, sy=0.85)
            b.ball(h1, sp.head[3] * 1.1, dark, n=6)
            upv = V((-hd.z, 0, hd.x))
            for s in (1, -1):
                # white face stripe, eyes, big ears, ringed lyre horns
                b.seg(n1 + upv * 0.03 + V((0, s * 0.03, 0)), h1 + upv * 0.012 + V((0, s * 0.018, 0)),
                      0.012, 0.008, white, n=5)
                b.ball(n1 + hd * 0.03 + upv * 0.02 + V((0, s * 0.04, 0)), 0.013, "black", n=5)
                e0 = n1 + upv * 0.03 + V((0, s * 0.03, 0)) - hd * 0.02
                ed = (upv * 0.5 + V((0, s * 1.0, 0)) - hd * (0.4 + 0.4 * q["ear"])).normalized()
                b.seg(e0, e0 + ed * 0.07, 0.02, 0.006, tan, n=5, sy=0.5)
                h0 = n1 + upv * 0.04 + V((0, s * 0.016, 0))
                pts = [h0, h0 + upv * 0.08 - hd * 0.05 + V((0, s * 0.03, 0)),
                       h0 + upv * 0.16 - hd * 0.06 + V((0, s * 0.02, 0)),
                       h0 + upv * 0.21 - hd * 0.02 + V((0, s * 0.03, 0))]
                b.chain(pts, 0.014, 0.004, dark, n=5)
        draw_tail(b, qr, dark, r=(0.012, 0.016), length=0.12, hair=False)


# ---------------------------------------------------------------- elephant

ELEPHANT = Spec(
    fx=0.42, hx=-0.40, ty=0.17, tz=-0.20, fleg=(0.42, 0.40), hleg=(0.42, 0.40), hoof=0.07,
    stand=0.86, rz=0.44, ry=0.36,
    body=[(-0.66, 0.0, 0.18, 0.16), (-0.60, 0.02, 0.38, 0.31), (-0.42, 0.04, 0.46, 0.37),
          (-0.10, 0.02, 0.46, 0.38), (0.20, 0.02, 0.45, 0.36), (0.45, 0.06, 0.42, 0.32),
          (0.60, 0.10, 0.30, 0.24), (0.66, 0.12, 0.12, 0.10)],
    leg_r=(0.155, 0.125, 0.135), neck0=(0.58, 0.14), neck=(0.12, 20, 0.30, 0.27),
    head=(0.18, -55, 0.27, 0.19), tail0=(-0.66, 0.12))
AMBLE = {"LH": 0.0, "LF": 0.25, "RH": 0.5, "RF": 0.75}


class WarElephant(Beast):
    """The heaviest thing on the field: a grey bull elephant under a player
    caparison and a towered howdah with painted shields, a mahout on its
    neck with a goad, a spearman in the howdah; tusks swing on the impact."""

    cls = "Heavy"
    speed = 0.7
    sp = ELEPHANT

    def __init__(self):
        super().__init__()
        self.bd = units.Build()

    def quad(self, name, t):
        sp = self.sp
        base = qpose(sp, LFx=sp.fx + 0.03, RFx=sp.fx - 0.02, LHx=sp.hx - 0.02, RHx=sp.hx + 0.03)
        if name == "idle":
            s, c = math.sin(2 * math.pi * t), math.cos(2 * math.pi * t)
            return dict(base, hp=3 * s, trunk=0.25 * s, tk2=0.2 * c, ts=20 * c, ear=0.5 + 0.5 * s,
                        hy=2 * c)
        if name == "walk":
            q = gait(sp, t, self.speed, AMBLE, 0.68, lift=(0.14, 0.11), fold=(20, 15), base=base)
            q["trunk"] = 0.3 * math.sin(2 * math.pi * t)
            q["tk2"] = 0.15 * math.cos(2 * math.pi * t)
            q["ts"] = 15 * math.sin(2 * math.pi * t)
            q["hp"] = 3 * math.sin(4 * math.pi * t)
            q["ear"] = 0.5 + 0.5 * math.sin(2 * math.pi * t)
            return q
        if name == "attack":
            rear = dict(base, pitch=8, bx=-0.05, hp=14, hy=18, trunk=-1.0, tk2=0.6, ear=1.0,
                        LFz=0.22, LFf=30, LFx=sp.fx + 0.08)
            swing = dict(base, pitch=-4, bx=0.08, hp=-16, hy=-26, trunk=0.8, tk2=-0.3, ear=1.0,
                         LFx=sp.fx + 0.16, bz=base["bz"] - 0.03)
            return anim.keyed([(0.0, base), (0.4, rear), (0.6, swing, anim.ease_in),
                               (1.0, anim.lerp(swing, base, 0.6))], t, loop=False)
        if name == "death":
            return horse_death(sp, t, base)
        return lying_q(sp, base)

    def draw(self, b, name, t, player):
        q = self.quad(name, t)
        qr = QuadRig(self.sp, q)
        sp = self.sp
        grey = mat((0.56, 0.54, 0.52), rough=0.9)
        dgrey = mat((0.44, 0.42, 0.41), rough=0.9)
        ivory = mat((0.97, 0.94, 0.84), rough=0.5)
        cloth, cloth2 = pm(player), pm(player, 0.82)
        draw_barrel(b, qr, grey)
        with b.at(qr.Bf, True):
            for s in (1, -1):
                b.ball((sp.hx, s * 0.17, -0.10), 0.24, grey, scale=(1.05, 0.7, 1.3), n=10)
                b.ball((sp.fx, s * 0.17, -0.12), 0.21, grey, scale=(1.0, 0.75, 1.4), n=10)
        draw_legs(b, qr, grey, mat((0.80, 0.76, 0.68), rough=0.8))
        # the caparison: a deep player cloth with a gold border, a rug over the back
        saddle_cloth(b, qr, [gold(), cloth, cloth, cloth, gold()], x0=-0.50, x1=0.40, flare=1.05)
        drape(b, qr, -0.48, 0.38, 0.42, cloth2, flare=1.08, top_up=0.10, hem=gold())
        # head, ears, trunk, tusks (in the neck frame)
        with b.at(qr.N, True):
            n1, hd, h1 = qr.n1, qr.hd, qr.h1
            b.seg(-qr.nd * 0.12, n1, sp.neck[2], sp.neck[3], grey, n=12, sy=0.9)
            hc = n1 + hd * 0.05
            b.ball(hc + V((0, 0, 0.06)), 0.27, grey, scale=(1.0, 0.88, 1.0), n=14)
            b.ball(hc + V((0.10, 0, -0.02)), 0.20, grey, scale=(1.0, 0.85, 1.1), n=12)
            for s in (1, -1):        # ears: big flapping fans
                e = math.radians(20 + 35 * q["ear"])
                ec = hc + V((-0.08, s * 0.24, 0.04))
                with b.at(qr.N @ T(ec) @ R("Z", -s * math.degrees(e)) @ R("X", 90) @ S(1.3, 1.3, 1),
                          True):
                    b.plate([(0, 0.15), (-0.20, 0.17), (-0.30, 0.02), (-0.24, -0.20), (-0.06, -0.28),
                             (0.02, -0.08)], 0.035, dgrey)
                b.ball(hc + V((0.17, s * 0.13, 0.10)), 0.025, "black", n=5)
                # tusks curving forward and up from the mouth
                t0 = hc + V((0.20, s * 0.09, -0.16))
                tk = [t0, t0 + V((0.12, s * 0.02, -0.08)), t0 + V((0.26, s * 0.03, -0.06)),
                      t0 + V((0.36, s * 0.02, 0.04))]
                b.chain(tk, 0.045, 0.012, ivory, n=6)
            # the trunk: hanging and curling, swung by trunk/tk2
            pts = [hc + V((0.20, 0, -0.05))]
            d = V((0.45, 0, -1.0)).normalized()
            seg = 0.15
            for i in range(6):
                a = math.radians(8 * q["trunk"] * (i + 1) * 2 + (i * i) * 2.5 * (1 + q["trunk"]))
                ay = math.radians(10 * q["tk2"] * i)
                dd = (Matrix.Rotation(-a, 3, "Y") @ Matrix.Rotation(ay, 3, "Z") @ d).normalized()
                pts.append(pts[-1] + dd * seg)
                d = dd
            b.chain(pts, 0.10, 0.045, grey, n=10)
            b.ball(pts[-1], 0.048, dgrey, n=6)
            # a player headdress over the brow, a gold boss
            b.ball(hc + V((0.06, 0, 0.20)), 0.17, cloth, scale=(1.05, 1.0, 0.45), n=12)
            b.ball(hc + V((0.21, 0, 0.12)), 0.05, gold(), n=8)
        draw_tail(b, qr, dgrey, r=(0.03, 0.025), length=0.45, hair=True)
        if name == "decay" or (name == "death" and t > 0.1):
            self.crew_fallen(b, t if name == "death" else 1.0, player, qr)
            return
        self.howdah(b, qr, player)
        self.mahout(b, name, t, qr, player)

    def howdah(self, b, qr, player):
        """A box tower on the back: wooden posts and rails, a player shield
        on each side and at the back, gold trim."""
        top = _profile(self.sp, -0.08)
        wood = mat("cedar", rough=0.75)
        with b.at(qr.Bf @ T(-0.10, 0, top[0] + top[1] - 0.02), True):
            b.cube((0, 0, 0.03), (0.52, 0.48, 0.06), wood)
            hx, hy, hz = 0.24, 0.22, 0.30
            for sx in (1, -1):
                for sy in (1, -1):
                    b.seg((sx * hx, sy * hy, 0.0), (sx * hx, sy * hy, hz + 0.05), 0.025, 0.025,
                          wood, n=6)
            b.cube((0, hy, 0.15), (0.50, 0.03, 0.22), pm(player))
            b.cube((0, -hy, 0.15), (0.50, 0.03, 0.22), pm(player))
            b.cube((-hx, 0, 0.15), (0.03, 0.46, 0.22), pm(player, 0.82))
            b.cube((hx, 0, 0.15), (0.03, 0.46, 0.22), pm(player, 0.82))
            for sy in (1, -1):
                b.seg((-hx, sy * hy, hz), (hx, sy * hy, hz), 0.022, 0.022, gold(), n=6)
                b.seg((-hx, sy * hy, 0.04), (hx, sy * hy, 0.04), 0.022, 0.022, gold(), n=6)
                _aspis(b, V((0.0, sy * (hy + 0.03), 0.16)), V((0, sy, 0.15)), player, "none",
                       Rr=0.12)
            for sx in (1, -1):
                b.seg((sx * hx, -hy, hz), (sx * hx, hy, hz), 0.022, 0.022, gold(), n=6)
        self.howdah_top = qr.Bf @ V((-0.10, 0, top[0] + top[1] - 0.02))

    def mahout(self, b, name, t, qr, player):
        sp = self.sp
        seat = qr.Bf @ V((0.48, 0, _profile(sp, 0.48)[0] + _profile(sp, 0.48)[1] + 0.02))
        base = anim.pose(SEATED, FLy=0.20, FRy=0.20, FLx=0.10, FRx=0.10, FLz=0.12, FRz=0.12,
                         ikL=0.0, Lf=30, La=25, Le=50, Rf=35, Ra=15, Re=60, aimw=1, Tx=0.6,
                         Ty=-0.2, Tz=0.5)
        if name == "attack":
            up = anim.pose(base, Rf=150, Re=40, Tx=0.3, Tz=1.0, lean=-6)
            hit = anim.pose(base, Rf=60, Re=10, Tx=0.6, Tz=-0.8, lean=14)
            p = anim.keyed([(0.0, base), (0.4, up), (0.6, hit, anim.ease_in),
                            (1.0, anim.lerp(hit, base, 0.6))], t, loop=False)
        elif name == "idle":
            p = anim.breathe(base, t, 0.6)
        else:
            p = dict(base, lean=4 + 3 * math.sin(2 * math.pi * t))
        p = seat_pose(None, self.bd, seat, qr.pitch, p)
        r = units.Rig(self.bd, p, "goad", False)
        skin = mat(lib.PAL["skin_dark"], rough=0.75)
        units.draw_body(b, r, skin, foot=skin, upper=skin, thigh=pm(player))
        units.draw_skirt(b, r, [pm(player)], top=0.08, hem=-0.04, flare=1.3)
        units.draw_torso(b, r, [skin, skin, skin, skin], 0.95)
        units.draw_head(b, r, skin, mat("hair_black"), "short", beard=True)
        k = r.bd.head_r / 0.115
        with b.at(r.H @ S(k, k, k), True):     # a turban
            b.ball((-0.01, 0, 0.05), 0.13, mat("white", rough=0.9), scale=(1.05, 1.05, 0.8), n=10)
        weapon(b, r.tool, "goad")
        # a spearman standing in the howdah (his legs hidden by its sides)
        sp_pose = anim.pose(FLx=0.04, FLy=0.02, FRx=-0.03, FRy=0.02, Rf=8, Ra=14, Re=70, Rt=6,
                            aimw=1, Tx=0.15, Ty=0.0, Tz=1.0, Lf=30, La=20, Le=60)
        if name == "attack":
            sp_pose = anim.keyed([(0.0, sp_pose), (0.4, anim.pose(sp_pose, Rf=60, Ra=80, Re=110,
                                                                   Tx=1.0, Tz=-0.3)),
                                  (0.6, anim.pose(sp_pose, Rf=95, Ra=40, Re=10, Tx=1.0, Tz=-0.6,
                                                  lean=10), anim.ease_in), (1.0, sp_pose)], t,
                                 loop=False)
        h = self.howdah_top
        sp_pose.update(x=h.x - 0.02, y=h.y, z=h.z + 0.06 - 0.25, fallx=0, fallz=self.bd.stand,
                       fall=-qr.pitch * 0.8)
        r2 = units.Rig(self.bd, sp_pose, "spear", False)
        skin2 = mat(lib.PAL["skin_tan"], rough=0.75)
        units.draw_body(b, r2, skin2, upper=pm(player), foot=skin2)
        units.draw_torso(b, r2, bronze(), 1.0)
        units.draw_head(b, r2, skin2, None, "none", face=False)
        units.Hoplite.helmet(None, b, r2, pm(player))
        units.tool_mesh(b, r2.tool, "spear")

    def crew_fallen(self, b, t, player, qr):
        """The howdah breaks away and the crew fall beside the beast."""
        skin = mat(lib.PAL["skin_dark"], rough=0.75)
        lying = anim.fallen(anim.pose())
        u = anim.smooth(min(1.0, t / 0.7))
        lying.update(x=0.35, y=-0.95 * u - 0.1, z=1.4 * (1 - u) ** 2, hyaw=-20,
                     fall=-90 * u, roll=-40 * (1 - u))
        r = units.Rig(self.bd, lying)
        units.draw_body(b, r, skin, foot=skin, upper=skin, thigh=pm(player))
        units.draw_torso(b, r, [skin] * 4, 0.95)
        units.draw_head(b, r, skin, mat("hair_black"), "short", beard=True)
        # the howdah's wreck: boards in the player colour on the ground
        wood = mat("cedar", rough=0.75)
        with b.at(T(-0.25 - 0.1 * u, -0.70 * u - 0.05, 1.3 * (1 - u) ** 2 + 0.03)
                  @ R("X", -80 * u) @ R("Z", 20 * u), True):
            b.cube((0, 0, 0), (0.50, 0.30, 0.04), pm(player))
            b.cube((0, 0.0, 0.03), (0.52, 0.04, 0.05), gold())
            b.seg((-0.25, 0.1, 0.02), (0.25, 0.14, 0.02), 0.025, 0.025, wood, n=6)


# ----------------------------------------------------------------- chariot

CHARIOT_HORSE = HORSE.scaled(0.86)


def spoked_wheel(b, c, r, angle, n, rim_m, spoke_m, hub_m, w=0.045):
    """A wheel at c turning about Y: `angle` degrees forward roll."""
    with b.at(T(V(c)) @ R("Y", angle) @ R("X", 90)):
        rim = [V((r * math.cos(2 * math.pi * k / 16), r * math.sin(2 * math.pi * k / 16), 0))
               for k in range(17)]
        b.chain(rim, w * 0.55, w * 0.55, rim_m, n=6)
        for k in range(n):
            a = 2 * math.pi * k / n
            b.seg((0, 0, 0), (r * math.cos(a), r * math.sin(a), 0), w * 0.32, w * 0.28, spoke_m,
                  n=5)
        b.seg((0, 0, -w * 1.4), (0, 0, w * 1.4), w * 0.9, w * 0.9, hub_m, n=8)


def roll_angle(t, speed, r):
    """Degrees a wheel of radius r has turned at t of the walk cycle."""
    return math.degrees(speed * PERIOD * t / r)


class ChariotArcher(Beast):
    """Two horses abreast under player plumes and cloths pulling a light
    two-wheeled car with player-painted sides; a driver at the reins and an
    archer who draws and looses on the impact frame. The six-spoked wheels
    turn with the ground: five spokes' turn a cycle."""

    cls = "Heavy"
    speed = 1.7
    sp = CHARIOT_HORSE
    WR = 1.7 * PERIOD * 6 / (2 * math.pi * 5)      # wheel radius: 5/6 turn a cycle
    HX, HY = 0.40, 0.19                            # the horses' place
    CX = -0.62                                     # the car's axle

    def __init__(self):
        super().__init__()
        self.bd = units.Build()

    def horse_q(self, name, t, i):
        sp = self.sp
        base = qpose(sp, LFx=sp.fx + 0.02, RFx=sp.fx - 0.02, LHx=sp.hx - 0.01, RHx=sp.hx + 0.03)
        if name == "idle":
            s, c = math.sin(2 * math.pi * (t + 0.3 * i)), math.cos(2 * math.pi * (t + 0.3 * i))
            return dict(base, nk=-4 + 5 * s, hp=4 * c, ts=15 * c, tu=0.05, ear=0.5 * max(0, c))
        if name == "walk":
            q = gait(sp, (t + 0.06 * i) % 1.0, self.speed, CANTER, 0.40, base=base)
            q["nk"] = -6 + 7 * math.sin(2 * math.pi * (t + 0.06 * i - 0.30))
            q["tu"], q["ear"] = 0.5, 1.0
            return q
        if name == "attack":
            s = math.sin(math.pi * t)
            return dict(base, nk=-3 + 6 * s, hp=4 * s, ts=8 * s, ear=1.0)
        if name == "death":
            return horse_death(sp, t, base, 1.0 if i == 0 else -1.0)
        return lying_q(sp, base, 1.0 if i == 0 else -1.0)

    def car_M(self, name, t):
        """The car's frame: on its wheels, or tipping over as it dies."""
        if name in ("death", "decay"):
            u = anim.ease_in(min(1.0, (t if name == "death" else 1.0) / 0.75))
            return (T(self.CX - 0.1 * u, -0.10 * u, 0) @ rot_about("X", -62 * u, (0, -0.30, 0))
                    @ rot_about("Y", -14 * u, (0.35, 0, 0)))
        bob = 0.012 * math.sin(4 * math.pi * t) if name == "walk" else 0.0
        return T(self.CX, 0, bob)

    def draw(self, b, name, t, player):
        sp = self.sp
        qrs = []
        for i, s in enumerate((1, -1)):
            M = T(self.HX, s * self.HY, 0)
            if name in ("death", "decay"):
                M = T(self.HX + 0.05, s * (self.HY + 0.10), 0)
            q = self.horse_q(name, t, i)
            qr = QuadRig(sp, q, M)
            coat = [(0.95, 0.93, 0.88), (0.55, 0.30, 0.15)][i]
            draw_horse(b, qr, coat, ["rock", "hair_black"][i], blaze=i == 1)
            saddle_cloth(b, qr, [gold(), pm(player), pm(player), gold()], x0=-0.30, x1=0.22)
            with b.at(qr.N, True):           # a tall player plume on the brow
                hd, n1 = qr.hd, qr.n1
                upv = V((-hd.z, 0, hd.x))
                c0 = n1 + upv * sp.head[2] * 0.8
                b.seg(c0, c0 + upv * 0.05, 0.02, 0.026, gold(), n=6)
                b.chain([c0 + upv * 0.05, c0 + upv * 0.14 - hd * 0.03, c0 + upv * 0.22 - hd * 0.09],
                        0.05, 0.025, pm(player), n=6, sy=0.6)
            qrs.append(qr)
        C = self.car_M(name, t)
        wr = self.WR
        ang = roll_angle(t, self.speed, wr) if name == "walk" else 0.0
        rim, spk, hub = mat("wood_dark", rough=0.8), mat("yellow_ochre", rough=0.7), gold()
        wy = 0.33
        for s in (1, -1):
            c = C @ V((0, s * wy, wr))
            if name in ("death", "decay") and s == -1:
                u = anim.smooth(min(1.0, (t if name == "death" else 1.0) / 0.6))
                c = V((self.CX - 0.35 * u, -0.85 * u, wr * (1 - u) + 0.03 * u))
                with b.at(T(c) @ R("X", 90 * u) @ T(-c), True):
                    spoked_wheel(b, c, wr, ang, 6, rim, spk, hub)
                continue
            with b.at(C, True):
                spoked_wheel(b, (0, s * wy, wr), wr, ang, 6, rim, spk, hub)
        with b.at(C, True):
            b.seg((0, -wy, wr), (0, wy, wr), 0.02, 0.02, rim, n=6)
            fz = wr + 0.03
            rx, ry = 0.30, 0.22
            arc = [(-90 + 180 * k / 8) for k in range(9)]
            out = [(0.06 + rx * math.cos(math.radians(a)), ry * math.sin(math.radians(a)))
                   for a in arc]
            b.plate(out, 0.035, mat("cedar", rough=0.7), frame=T(0, 0, fz))
            b.cube((-0.03, 0, fz), (0.18, ry * 2, 0.035), mat("cedar", rough=0.7))
            vs, fs = [], []
            for k, (x, y) in enumerate(out):
                tz = fz + 0.30 + 0.08 * math.cos(math.radians(arc[k]))
                vs += [(x, y, fz), (x, y, tz)]
                if k:
                    fs.append((2 * k - 2, 2 * k, 2 * k + 1, 2 * k - 1))
            b.poly(vs, fs, pm(player), smooth=True)
            b.poly([(x * 0.985 + 0.0, y * 0.97, z) for x, y, z in vs],
                   [tuple(reversed(f)) for f in fs], pm(player, 0.64), smooth=True)
            top = [V(vs[2 * k + 1]) for k in range(9)]
            b.chain(top, 0.022, 0.022, gold(), n=6)
            b.chain([V(vs[2 * k]) + V((0, 0, 0.15)) for k in range(9)], 0.016, 0.016, gold(), n=5)
            # bow case and quiver on the side, crossed
            for s in (1, -1):
                b.seg((0.0, s * (ry + 0.03), fz + 0.03), (0.28, s * (ry + 0.02), fz + 0.28), 0.04,
                      0.05, gold(), n=6, sy=0.45)
        # the pole to the yoke between the horses
        if name not in ("death", "decay"):
            wyk = (qrs[0].Bf @ V((0.22 * sp.k, 0, sp.rz)) + qrs[1].Bf @ V((0.22 * sp.k, 0, sp.rz))) / 2
            p0 = C @ V((0.10, 0, wr + 0.02))
            b.chain([p0, p0 + V((0.25, 0, 0.02)), wyk - V((0.12, 0, 0.12)), wyk], 0.022, 0.018,
                    mat("wood_dark"), n=6)
            l = qrs[0].Bf @ V((0.20 * sp.k, 0, sp.rz * 1.05))
            r = qrs[1].Bf @ V((0.20 * sp.k, 0, sp.rz * 1.05))
            b.seg(l, r, 0.025, 0.025, gold(), n=6)
        self.crew(b, name, t, player, C, qrs)

    def crew(self, b, name, t, player, C, qrs):
        skin = mat(lib.PAL["skin_tan"], rough=0.75)
        fz = self.WR + 0.03 + 0.018
        lin = mat("linen", rough=0.9)
        if name in ("death", "decay"):
            u = anim.smooth(min(1.0, (t if name == "death" else 1.0) / 0.7))
            for j, (x, y) in enumerate(((-0.55, -0.62), (-0.85, 0.30))):
                lying = anim.fallen(anim.pose())
                lying.update(x=self.CX + 0.1 + (x - self.CX - 0.1) * u, y=y * u,
                             z=fz * (1 - u) ** 2, fall=-90 * u, hyaw=-20 + 40 * j)
                r = units.Rig(self.bd, lying)
                self.dress(b, r, player, skin, lin, j)
            return
        # the driver, the reins in both hands
        stand = dict(FLx=0.06, FLy=0.02, FRx=-0.06, FRy=0.02)
        dp = anim.pose(stand, lean=8, ikL=1, ikR=1, Lx=0.26, Ly=0.08, Lz=0.10, Rx=0.26, Ry=-0.08,
                       Rz=0.10)
        if name == "walk":
            dp["lean"] = 14
            dp["drop"] = 0.03 + 0.012 * math.sin(4 * math.pi * t)
        elif name == "idle":
            dp = anim.breathe(dp, t, 0.6)
            dp.update(stand)
        Cm = C @ T(0.12, -0.09, fz)
        dp.update(x=Cm.translation.x, y=Cm.translation.y, z=Cm.translation.z)
        r = units.Rig(self.bd, dp)
        self.dress(b, r, player, skin, lin, 0)
        for i in (0, 1):
            for s in (1, -1):
                b.seg(r.hand["L" if i == 0 else "R"], qrs[i].mouth + V((0, s * 0.03, 0)), 0.006,
                      0.006, mat("leather"), n=4)
        # the archer
        rs, dr, arrow = HorseArcher.bow_state(None, name, t)
        if name == "walk":
            rs = 0.5
        ap = anim.pose(stand, lean=4, pyaw=20)
        if rs > 0:
            ap.update(ikL=1.0, Lx=0.18 + 0.18 * rs, Ly=0.12, Lz=0.14 + 0.22 * rs, ikR=1.0,
                      Rx=0.28 * rs - 0.22 * dr * rs + 0.04, Ry=-0.10 + 0.03 * dr,
                      Rz=0.14 + 0.24 * rs + 0.02 * dr, twist=22 * rs, hyaw=-10 * rs,
                      FLx=0.12, FRx=-0.10)
        else:
            ap = anim.breathe(dict(ap, ikL=1.0, Lx=0.16, Ly=0.18, Lz=0.10, Rf=10, Re=20), t, 0.6)
            ap.update(stand)
        Am = C @ T(-0.12, 0.11, fz)
        ap.update(x=Am.translation.x, y=Am.translation.y, z=Am.translation.z)
        ra = units.Rig(self.bd, ap)
        self.dress(b, ra, player, skin, lin, 1)
        bm = mat((0.28, 0.17, 0.10))
        grip = ra.hand["L"]
        if rs > 0.3:
            aim = (ra.U.to_3x3() @ V((1.0, 0.1, 0.05))).normalized()
            bigbow(b, grip, aim, nock=ra.hand["R"] if dr > 0.05 else None, m=bm,
                   arrow=arrow and dr > 0.05)
            if dr <= 0.05 and arrow:
                _arrow(b, grip - aim * 0.3, aim, L=0.38)
        else:
            bigbow(b, grip, (ra.U.to_3x3() @ V((0.6, 0.6, 0.0))).normalized(), m=bm)

    def dress(self, b, r, player, skin, lin, j):
        cloth = pm(player)
        units.draw_body(b, r, skin, foot=skin, upper=skin)
        units.draw_skirt(b, r, [lin, lin, cloth], top=0.09, hem=-0.15, zs=[0, 0.6, 0.9, 1.0],
                         flare=1.3)
        units.draw_torso(b, r, [cloth, cloth, lin, lin], 1.0)
        units.draw_head(b, r, skin, mat("hair_black", rough=0.9), "short", face=True)
        k = r.bd.head_r / 0.115
        with b.at(r.H @ S(k, k, k), True):
            if j == 0:      # the driver's blue-crowned helmet is the player's
                b.loft([(-0.01, 0, 0.0, 0.128, 0.124), (-0.02, 0, 0.09, 0.12, 0.115),
                        (-0.03, 0, 0.17, 0.06, 0.06)], cloth, n=12)
            else:           # the archer's striped headcloth
                b.ball((-0.03, 0, 0.03), 0.128, lin, n=12)
                b.loft([(-0.05, 0, 0.0, 0.10, 0.13), (-0.08, 0, -0.16, 0.08, 0.14)], cloth, n=10)


# ------------------------------------------------------------ siege engines

def pennant(b, base, h, player, t, length=0.30, width=0.14):
    """A pole with a waving player pennant."""
    b.seg(base, base + V((0, 0, h)), 0.016, 0.014, mat("wood_dark"), n=6)
    b.ball(base + V((0, 0, h + 0.01)), 0.022, gold(), n=6)
    vs, fs = [], []
    nc, nr = 6, 3
    for i in range(nc):
        u = i / (nc - 1)
        for j in range(nr):
            v = j / (nr - 1)
            w = width * (1 - 0.6 * u)
            y = 0.04 * u * math.sin(2 * math.pi * (t - u * 0.8))
            vs.append(base + V((-u * length, y, h - 0.02 - v * w + w * (1 - 0.6 * u) * 0)))
    for i in range(nc - 1):
        for j in range(nr - 1):
            k = i * nr + j
            fs.append((k, k + nr, k + nr + 1, k + 1))
    b.poly(vs, fs, pm(player), smooth=True)
    b.poly([v + V((0, -0.006, 0)) for v in vs], [tuple(reversed(f)) for f in fs], pm(player),
           smooth=True)


class Part:
    """A rigid part of an engine and where it goes when the engine
    collapses: moved by (dx, dy, dz), turned (rx, ry, rz) degrees about
    `pivot`, between t0 and t1 of the death."""

    def __init__(self, pivot=(0, 0, 0), d=(0, 0, 0), rot=(0, 0, 0), t0=0.0, t1=0.7):
        self.pivot, self.d, self.rot, self.t0, self.t1 = V(pivot), V(d), rot, t0, t1

    def M(self, u):
        u = anim.ease_in(min(1.0, max(0.0, (u - self.t0) / (self.t1 - self.t0))))
        bounce = 0.0
        p = self.pivot
        return (T(p + self.d * u + V((0, 0, bounce))) @ R("X", self.rot[0] * u)
                @ R("Y", self.rot[1] * u) @ R("Z", self.rot[2] * u) @ T(-p))


class Engine(Beast):
    """A wheeled timber engine: `parts()` the collapse, `draw_engine(b, P,
    s)` with P the parts' matrices and s the state (wheel angle, arm,
    loaded, pennant phase)."""

    cls = "Heavy"
    speed = 0.7
    wheel_r, spokes = 0.3, 8

    def state(self, name, t):
        s = dict(roll=0.0, arm=0.0, loaded=1.0, flag=t, sling=0.0, dt=0.0)
        if name == "walk":
            s["roll"] = roll_angle(t, self.speed, self.wheel_r)
        elif name == "attack":
            s.update(self.attack_state(t))
        elif name in ("death", "decay"):
            s["dt"] = t if name == "death" else 1.0
            s.update(self.dead_state())
        return s

    def dead_state(self):
        return {}

    def draw(self, b, name, t, player):
        s = self.state(name, t)
        P = {k: (p.M(s["dt"]) if s["dt"] > 0 else I4) for k, p in self.parts().items()}
        self.draw_engine(b, P, s, player)


def timber(b, a, c, w, h, m):
    """A squared beam from a to c, w wide and h deep."""
    a, c = V(a), V(c)
    d = c - a
    L = d.length
    if L < 1e-6:
        return
    fr = units._frame(d.normalized(), (0, 0, 1) if abs(d.normalized().z) < 0.9 else (1, 0, 0))
    with b.at(T((a + c) / 2) @ fr.to_4x4()):
        b.cube((0, 0, 0), (w, h, L), m)


class StoneThrower(Engine):
    """The early, light engine: a narrow four-wheeled cart with a tall
    trestle and a see-saw beam; a player-painted counterweight box drops
    and the long arm whips its sling over the top on the impact frame."""

    speed = 0.7
    wheel_r = 0.7 * PERIOD * 6 / (2 * math.pi * 3)        # half a turn a cycle (6 spokes)
    spokes = 6
    PIV = V((0.02, 0, 0.98))
    LONG, SHORT = 0.86, 0.26

    def parts(self):
        return {
            "cart": Part((0, 0, 0), (0, 0, -self.wheel_r + 0.06), (10, -4, 0), 0.25, 0.65),
            "tower": Part((0.0, 0.2, 0.25), (0.05, 0.10, -0.12), (72, 0, 8), 0.05, 0.75),
            "beam": Part((0.0, 0.0, 0.9), (-0.10, 0.62, -0.78), (82, 0, 25), 0.0, 0.70),
            "w0": Part((0.28, -0.30, 0.0), (0.12, -0.30, 0.0), (-90, 0, 20), 0.15, 0.55),
            "w1": Part((-0.28, -0.30, 0.0), (-0.05, -0.12, 0.0), (-28, 0, 0), 0.30, 0.65),
            "w2": Part((0.28, 0.30, 0.0), (0.0, 0.0, 0.0), (0, 0, 0), 0.30, 0.65),
            "w3": Part((-0.28, 0.30, 0.0), (-0.25, 0.25, 0.0), (90, 0, -30), 0.10, 0.55),
        }

    def attack_state(self, t):
        # cocked (the long arm down behind) through frame 2 with a last
        # creak; on frame 3 the counterweight has fallen and the arm stands
        # high over the front, the sling whipped out and empty; then it
        # swings back a little
        a = anim.keyed([(0.0, {"a": 0.0}), (0.4, {"a": -0.04}), (0.6, {"a": 1.0}, anim.ease_in),
                        (0.8, {"a": 0.86}), (1.0, {"a": 0.9})], t, loop=False)["a"]
        return dict(arm=a, loaded=1.0 if t < 0.5 else 0.0, sling=1.0 if t >= 0.5 else 0.0)

    def dead_state(self):
        return dict(arm=0.3, loaded=0.0)

    def beam_angle(self, s):
        # the long arm's direction: 214 deg (back and down) cocked, 78 (up,
        # leaning forward) thrown
        return 214 + (78 - 214) * s["arm"]

    def draw_engine(self, b, P, s, player):
        wr = self.wheel_r
        wood = mat((0.66, 0.46, 0.27), rough=0.8)
        dark = mat("wood_dark", rough=0.8)
        rope = mat((0.80, 0.70, 0.48), rough=0.9)
        iron = mat("iron", rough=0.5, metal=0.4)
        cz = wr + 0.03
        with b.at(P["cart"]):
            for y in (0.20, -0.20):
                timber(b, (-0.46, y, cz), (0.46, y, cz), 0.07, 0.08, wood)
            for x in (-0.40, 0.0, 0.40):
                timber(b, (x, -0.26, cz + 0.06), (x, 0.26, cz + 0.06), 0.07, 0.06, wood)
            b.cube((0.0, 0, cz + 0.10), (0.30, 0.36, 0.03), dark)
            for x in (0.28, -0.28):
                timber(b, (x, -0.31, wr), (x, 0.31, wr), 0.04, 0.04, dark)
            # player side boards
            for y in (0.235, -0.235):
                b.cube((0.0, y, cz + 0.02), (0.78, 0.02, 0.12), pm(player, 0.82))
        with b.at(P["cart"] @ P["tower"]):
            top = self.PIV
            for y in (0.17, -0.17):        # the trestle: two A-frames
                timber(b, (-0.30, y, cz + 0.06), top + V((0, y, 0)), 0.06, 0.06, wood)
                timber(b, (0.30, y, cz + 0.06), top + V((0, y, 0)), 0.06, 0.06, wood)
                timber(b, (-0.15, y, cz + 0.42), (0.15, y, cz + 0.42), 0.05, 0.05, wood)
            b.seg(top + V((0, -0.22, 0)), top + V((0, 0.22, 0)), 0.035, 0.035, iron, n=8)
            pennant(b, V((-0.30, 0.20, cz + 0.06)), 0.86, player, s["flag"], 0.26, 0.12)
        with b.at(P["cart"] @ P["tower"] @ P["beam"]):
            a = math.radians(self.beam_angle(s))
            d = V((math.cos(a), 0, math.sin(a)))
            top = self.PIV
            le, se = top + d * self.LONG, top - d * self.SHORT
            b.seg(se, le, 0.05, 0.03, wood, n=8, sy=0.8)
            b.ball(top, 0.05, dark, n=8)
            # the counterweight box hanging from the short end: player painted
            cw = se - V((0, 0, 0.16))
            b.seg(se, cw + V((0, 0, 0.1)), 0.012, 0.012, rope, n=4)
            b.cube(cw, (0.22, 0.30, 0.22), pm(player))
            b.cube(cw + V((0, 0, 0.115)), (0.24, 0.32, 0.03), dark)
            b.cube(cw - V((0, 0, 0.115)), (0.24, 0.32, 0.03), dark)
            for k in range(3):
                b.ball(cw + V((-0.05 + 0.05 * k, 0.03 * (k - 1), 0.14)), 0.05, "rock", n=6)
            # the sling: lying behind on the ground with a stone, or whipped out
            if s["sling"] > 0:
                tip = le + (d + V((0.6, 0, 0.3))).normalized() * 0.32
            else:
                tip = V((le.x - 0.18, 0, max(0.06, le.z - 0.22)))
            b.seg(le, tip, 0.01, 0.01, rope, n=4)
            b.ball(tip, 0.035, mat("leather"), scale=(1, 1, 0.5), n=6)
            if s["loaded"] > 0.5:
                b.ball(tip + V((0, 0, 0.04)), 0.06, "rock", n=8)
        rim, spk, hub = dark, wood, iron
        for i, (x, y) in enumerate(((0.28, -0.31), (-0.28, -0.31), (0.28, 0.31), (-0.28, 0.31))):
            with b.at(P["cart"] @ P["w%d" % i]):
                spoked_wheel(b, (x, y, wr), wr, s["roll"], 6, rim, spk, hub, w=0.05)


class Catapult(Engine):
    """The Iron Age's heavy engine: a long, low frame on four big wheels,
    player boards along its sides, a torsion skein and a great arm that
    slams up against a padded player crossbar on the impact frame."""

    speed = 0.7
    wheel_r = 0.7 * PERIOD * 8 / (2 * math.pi * 3)        # three spokes' turn a cycle
    SKEIN = V((-0.30, 0, 0.0))

    def parts(self):
        return {
            "base": Part((0, 0, 0), (0, 0, -self.wheel_r + 0.08), (-8, 5, 0), 0.30, 0.70),
            "tower": Part((0.12, 0.0, 0.5), (-0.10, -0.20, -0.30), (-70, 0, -10), 0.05, 0.75),
            "arm": Part((-0.30, 0.0, 0.5), (0.20, 0.55, -0.35), (75, 0, 20), 0.0, 0.65),
            "w0": Part((0.48, -0.42, 0.0), (0.10, -0.30, 0.0), (-90, 0, 15), 0.10, 0.55),
            "w1": Part((-0.48, -0.42, 0.0), (0.0, -0.06, 0.0), (-20, 0, 0), 0.30, 0.65),
            "w2": Part((0.48, 0.42, 0.0), (0.0, 0.0, 0.0), (0, 0, 0), 0.30, 0.65),
            "w3": Part((-0.48, 0.42, 0.0), (-0.20, 0.30, 0.0), (90, 0, -20), 0.15, 0.55),
        }

    def attack_state(self, t):
        a = anim.keyed([(0.0, {"a": 0.0}), (0.4, {"a": -0.06}), (0.6, {"a": 1.0}, anim.ease_in),
                        (0.8, {"a": 0.9}), (1.0, {"a": 0.95})], t, loop=False)["a"]
        return dict(arm=a, loaded=1.0 if t < 0.5 else 0.0)

    def dead_state(self):
        return dict(arm=0.2, loaded=0.0)

    def draw_engine(self, b, P, s, player):
        wr = self.wheel_r
        wood = mat((0.58, 0.38, 0.22), rough=0.8)
        dark = mat("wood_dark", rough=0.8)
        rope = mat((0.80, 0.70, 0.48), rough=0.9)
        iron = mat("iron", rough=0.5, metal=0.4)
        bz = wr + 0.05
        with b.at(P["base"]):
            for y in (0.27, -0.27):
                timber(b, (-0.78, y, bz), (0.78, y, bz), 0.11, 0.13, wood)
                b.cube((0.0, y * 1.0 + (0.06 if y > 0 else -0.06), bz + 0.03), (1.30, 0.02, 0.24),
                       pm(player, 0.82))
                b.cube((0.0, y * 1.0 + (0.07 if y > 0 else -0.07), bz + 0.15), (1.32, 0.025, 0.03),
                       gold())
                for x in (-0.55, 0.0, 0.55):
                    b.ball((x, y + (0.075 if y > 0 else -0.075), bz + 0.01), 0.022, iron, n=5)
            for x in (-0.68, -0.30, 0.12, 0.68):
                timber(b, (x, -0.34, bz + 0.07), (x, 0.34, bz + 0.07), 0.09, 0.08, wood)
            for x in (0.48, -0.48):
                timber(b, (x, -0.43, wr), (x, 0.43, wr), 0.06, 0.06, dark)
            # the torsion skein between two heavy side posts
            c = V((self.SKEIN.x, 0, bz + 0.12))
            b.seg(c + V((0, -0.24, 0)), c + V((0, 0.24, 0)), 0.09, 0.09, rope, n=10)
            for y in (0.24, -0.24):
                b.cube(c + V((0, y, 0)), (0.20, 0.06, 0.24), wood)
                b.seg(c + V((0, y * 1.15, 0)), c + V((0, y * 1.35, 0)), 0.08, 0.08, iron, n=10)
            # the winch at the back
            b.seg((-0.70, -0.30, bz + 0.12), (-0.70, 0.30, bz + 0.12), 0.05, 0.05, dark, n=8)
        with b.at(P["base"] @ P["tower"]):
            # the A-frame and its padded crossbar (the player's)
            tz = bz + 0.92
            for y in (0.26, -0.26):
                timber(b, (0.12, y, bz + 0.07), (0.12, y, tz), 0.09, 0.09, wood)
                timber(b, (0.62, y, bz + 0.07), (0.16, y, tz - 0.12), 0.07, 0.07, wood)
            b.seg((0.10, -0.32, tz - 0.02), (0.10, 0.32, tz - 0.02), 0.10, 0.10, pm(player), n=12)
            b.seg((0.10, -0.33, tz - 0.02), (0.10, -0.29, tz - 0.02), 0.105, 0.105, dark, n=12)
            b.seg((0.10, 0.29, tz - 0.02), (0.10, 0.33, tz - 0.02), 0.105, 0.105, dark, n=12)
            pennant(b, V((0.12, 0.26, tz + 0.03)), 0.30, player, s["flag"], 0.28, 0.13)
        with b.at(P["base"] @ P["arm"]):
            # the arm: lying back over the frame cocked, upright thrown
            a = math.radians(176 + (94 - 176) * s["arm"])
            c = V((self.SKEIN.x, 0, bz + 0.12))
            d = V((math.cos(a), 0, math.sin(a)))
            e = c + d * 0.86
            b.seg(c, e, 0.065, 0.05, wood, n=8, sy=0.9)
            b.seg(c + d * 0.3, c + d * 0.36, 0.07, 0.07, iron, n=8)
            up = V((-d.z, 0, d.x))
            if d.x < 0:
                up = -up
            cup = e + up * 0.03
            b.seg(cup - up * 0.02, cup + up * 0.06, 0.09, 0.12, iron, n=10)
            if s["loaded"] > 0.5:
                b.ball(cup + up * 0.09, 0.09, "rock", n=8)
        rim, spk, hub = dark, wood, iron
        for i, (x, y) in enumerate(((0.48, -0.43), (-0.48, -0.43), (0.48, 0.43), (-0.48, 0.43))):
            with b.at(P["base"] @ P["w%d" % i]):
                spoked_wheel(b, (x, y, wr), wr, s["roll"], 8, rim, spk, hub, w=0.07)


class Ballista(Engine):
    """The anti-unit bolt thrower: low and wide, two torsion springs in
    player-painted housings, long bow arms, a slider on a two-wheeled
    carriage with a trail, a player mantlet in front; the string snaps
    forward and the bolt leaves on the impact frame."""

    speed = 0.8
    wheel_r = 0.8 * PERIOD * 8 / (2 * math.pi * 3)        # three spokes' turn a cycle
    TILT = 6.0

    def parts(self):
        return {
            "carriage": Part((0, 0, 0), (0, 0, -0.10), (0, 8, 6), 0.30, 0.70),
            "bow": Part((0.25, 0, 0.62), (0.15, -0.35, -0.50), (-80, -10, 20), 0.0, 0.65),
            "w0": Part((-0.10, -0.36, 0.0), (0.0, -0.30, 0.0), (-90, 0, 20), 0.10, 0.55),
            "w1": Part((-0.10, 0.36, 0.0), (0.0, 0.10, 0.0), (35, 0, 0), 0.25, 0.65),
        }

    def attack_state(self, t):
        d = anim.keyed([(0.0, {"d": 0.85}), (0.4, {"d": 1.0}), (0.6, {"d": 0.0}, anim.ease_in),
                        (0.8, {"d": 0.15}), (1.0, {"d": 0.45})], t, loop=False)["d"]
        return dict(arm=d, loaded=1.0 if t < 0.5 else 0.0, sling=1.0 if 0.5 <= t < 0.7 else 0.0)

    def state(self, name, t):
        s = super().state(name, t)
        if name != "attack":
            s.setdefault("arm", 0.85)
            if name in ("idle", "walk"):
                s["arm"] = 0.85
        return s

    def dead_state(self):
        return dict(arm=0.0, loaded=0.0)

    def draw_engine(self, b, P, s, player):
        wr = self.wheel_r
        wood = mat((0.62, 0.42, 0.24), rough=0.8)
        dark = mat("wood_dark", rough=0.8)
        rope = mat((0.80, 0.70, 0.48), rough=0.9)
        iron = mat("iron", rough=0.5, metal=0.4)
        sz = 0.64                         # the stock's height
        G = T(0.0, 0, sz) @ R("Y", -self.TILT)
        with b.at(P["carriage"]):
            # axle, trail and the post under the stock
            timber(b, (-0.10, -0.38, wr), (-0.10, 0.38, wr), 0.06, 0.06, dark)
            for y in (0.10, -0.10):
                timber(b, (0.10, y, wr + 0.02), (-0.75, y * 1.4, 0.05), 0.07, 0.07, wood)
            timber(b, (0.0, 0, wr), (0.0, 0, sz - 0.04), 0.09, 0.09, wood)
            b.cube((-0.68, 0, 0.05), (0.12, 0.32, 0.05), dark)
            # the mantlet: a broad player shield in front of the post
            with b.at(T(0.18, 0, 0.42) @ R("Y", -14)):
                b.cube((0, 0, 0), (0.035, 0.58, 0.30), pm(player))
                b.cube((0.0, 0, 0.16), (0.045, 0.60, 0.03), gold())
                b.cube((0.0, 0, -0.16), (0.045, 0.60, 0.03), gold())
            with b.at(G):
                # the stock with its slider
                b.cube((0.05, 0, 0.0), (1.10, 0.10, 0.07), wood)
                b.cube((0.05, 0, 0.045), (1.0, 0.05, 0.02), dark)
                b.seg((-0.52, -0.12, -0.0), (-0.52, 0.12, 0.0), 0.04, 0.04, dark, n=8)
                for y in (0.13, -0.13):
                    b.seg((-0.52, y, 0), (-0.52, y * 1.3, 0.06), 0.012, 0.012, iron, n=4)
        with b.at(P["carriage"] @ P["bow"] @ G):
            fx = 0.42
            # the spring frame: two housings painted the player's colour
            b.cube((fx, 0, 0.0), (0.12, 0.36, 0.06), wood)
            b.cube((fx, 0, 0.17), (0.12, 0.36, 0.05), wood)
            for y in (0.15, -0.15):
                b.seg((fx, y, -0.10), (fx, y, 0.27), 0.065, 0.065, pm(player), n=10)
                b.seg((fx, y, 0.27), (fx, y, 0.30), 0.075, 0.075, gold(), n=10)
                b.seg((fx, y, -0.13), (fx, y, -0.10), 0.075, 0.075, gold(), n=10)
            # the arms, swept back by the draw
            draw = s["arm"]
            tips = []
            for sy in (1, -1):
                a = math.radians(14 + 34 * draw)
                root = V((fx, sy * 0.15, 0.08))
                tip = root + V((-math.sin(a), sy * math.cos(a), 0)) * 0.50
                b.seg(root, tip, 0.04, 0.025, dark, n=6)
                b.ball(tip, 0.028, iron, n=6)
                tips.append(tip)
            # the string to the claw on the slider
            cx = fx - 0.05 - 0.75 * draw
            claw = V((cx, 0, 0.08))
            for tip in tips:
                b.seg(tip, claw, 0.008, 0.008, rope, n=4)
            b.cube(claw + V((0, 0, -0.02)), (0.08, 0.08, 0.05), iron)
            if s["loaded"] > 0.5:
                b.seg(claw + V((0.0, 0, 0.03)), claw + V((0.82, 0, 0.03)), 0.017, 0.017, dark, n=6)
                b.seg(claw + V((0.82, 0, 0.03)), claw + V((0.94, 0, 0.03)), 0.03, 0.002, iron,
                      n=6)
                b.plate([(0.0, 0.0), (0.10, 0.0), (0.02, 0.05)], 0.008, "white",
                        frame=T(claw + V((0.02, 0, 0.03))) @ R("X", 90))
            elif s["sling"] > 0:      # the bolt just away: a streak in front
                b.seg((fx + 0.55, 0, 0.11), (fx + 1.0, 0, 0.14), 0.01, 0.017, dark, n=5)
                b.seg((fx + 1.0, 0, 0.14), (fx + 1.10, 0, 0.145), 0.028, 0.002, iron, n=5)
        if True:
            with b.at(P["carriage"]):
                pennant(b, V((-0.72, 0.15, 0.08)), 0.60, player, s["flag"], 0.26, 0.12)
        rim, spk, hub = dark, wood, iron
        for i, y in enumerate((-0.36, 0.36)):
            with b.at(P["carriage"] @ P["w%d" % i]):
                spoked_wheel(b, (-0.10, y, wr), wr, s["roll"], 8, rim, spk, hub, w=0.055)


# ----------------------------------------------------------------- factories

scout = units._unit(Scout)
light_cavalry = units._unit(LightCavalry)
heavy_cavalry = units._unit(HeavyCavalry)
horse_archer = units._unit(HorseArcher)
chariot_archer = units._unit(ChariotArcher)
war_elephant = units._unit(WarElephant)
gazelle = units._unit(Gazelle)
ballista = units._unit(Ballista)
catapult = units._unit(Catapult)
stone_thrower = units._unit(StoneThrower)

SETS = {
    "scout": dict(func="units_mounted.scout", cls="Mounted", scale=1),
    "light_cavalry": dict(func="units_mounted.light_cavalry", cls="Mounted", scale=1),
    "heavy_cavalry": dict(func="units_mounted.heavy_cavalry", cls="Mounted", scale=1),
    "horse_archer": dict(func="units_mounted.horse_archer", cls="Mounted", scale=1),
    "chariot_archer": dict(func="units_mounted.chariot_archer", cls="Heavy", scale=1),
    "war_elephant": dict(func="units_mounted.war_elephant", cls="Heavy", scale=1),
    "gazelle": dict(func="units_mounted.gazelle", cls="Foot", scale=1),
    "ballista": dict(func="units_mounted.ballista", cls="Heavy", scale=1),
    "catapult": dict(func="units_mounted.catapult", cls="Heavy", scale=1),
    "stone_thrower": dict(func="units_mounted.stone_thrower", cls="Heavy", scale=1),
}


def figure(name):
    return globals()[name]()


# ------------------------------------------------------------------ preview

def preview(name, out, only=None, facings=None, samples=10, scale=1):
    """A contact sheet of a figure through the exporter's own scene (its
    camera, light and contact shadow), nothing exported."""
    import shutil
    import tempfile
    import export
    import unitexport
    unitexport.set_scale(scale)
    tmp = tempfile.mkdtemp(prefix="look-mounted-")
    stage = unitexport.Stage(samples, tmp)
    fig = figure(name)
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
    unitexport.contact_sheet(out, grid, frame, anchor, names)
    shutil.rmtree(tmp, ignore_errors=True)
    print("wrote", out, "frame", frame)


if __name__ == "__main__":
    preview(sys.argv[1], sys.argv[2],
            sys.argv[3].split(",") if len(sys.argv) > 3 and sys.argv[3] else None,
            sys.argv[4].split(",") if len(sys.argv) > 4 else None)

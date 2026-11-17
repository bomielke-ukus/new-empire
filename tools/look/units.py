"""The new look's units (docs/look/units.md): jointed figures posed per
frame by anim.py, drawn as one mesh, exported as turning animated colour
sets by export.py (unitexport.py).

A unit is a `Figure`: its proportions, its outfit (a function drawing the
clothes, armour and fixed gear on the rig), what it holds in each
animation, and its animations as functions of t (0..1) returning a pose.
Most of a new unit is reuse: the rig, the body, the walk, the idle, the
death and decay are shared; a figure supplies its outfit, its gear and its
attack.

    /usr/bin/python3.13 tools/look/units.py villager OUT.png [anim]

renders a contact sheet of a figure (every animation's frames in the S
facing, or one animation in all five facings) for looking at, without the
exporter.
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
from figures import B, bronze, gold, _aspis, _frame  # noqa: E402
from lib import mat  # noqa: E402

V = Vector


def R(axis, deg):
    return Matrix.Rotation(math.radians(deg), 4, axis)


def T(x, y=None, z=None):
    return Matrix.Translation(V(x) if y is None else V((x, y, z)))


def S(x, y, z):
    return Matrix.Diagonal((x, y, z, 1.0))


# Shades of the player colour a figure may use; the exporter finds the
# player's materials by these (docs/look/colour-sets.md, "Player colour").
PLAYER_SHADES = (1.0, 0.82, 0.64)


def pm(player, shade=1.0, rough=0.7):
    """A player-coloured material: `player` scaled by one of PLAYER_SHADES."""
    if shade not in PLAYER_SHADES:
        raise ValueError("player shade %r is not one of PLAYER_SHADES" % shade)
    return mat(tuple(c * shade for c in player), rough=rough)


def player_materials(player):
    """The materials made from the player colour (by lib.mat's cache)."""
    want = {tuple(c * s for c in player) for s in PLAYER_SHADES}
    return {m.name for key, m in lib._mats.items() if tuple(key[0]) in want}


# ------------------------------------------------------------------- rig

class Build:
    """A figure's proportions (tiles). Heroic: a big head and hands,
    sturdy limbs, short enough legs to read as chunky miniatures."""

    def __init__(self, **kw):
        self.thigh, self.shin, self.ankle = 0.205, 0.195, 0.045
        self.hip_w = 0.068
        self.torso = 0.30           # hips to the shoulders' line
        self.shoulder_w = 0.158
        self.upper, self.fore = 0.165, 0.150
        self.neck = 0.07
        self.head_r = 0.135
        self.hand_r = 0.052
        self.thigh_r, self.knee_r, self.shin_r = 0.068, 0.054, 0.046
        self.arm_r, self.fore_r = 0.050, 0.044
        self.foot = (0.085, 0.048, 0.034)     # half length, half width, half height
        self.chest = 1.0                      # torso girth
        self.__dict__.update(kw)

    @property
    def leg(self):
        return (self.thigh, self.shin, self.ankle)

    @property
    def stand(self):
        """Hip height standing, knees a hair soft."""
        return self.thigh + self.shin + self.ankle - 0.012


def two_bone(a, t, la, lb, pole):
    """Elbow (or knee) and end for a limb from `a` reaching for `t`, bending
    toward `pole` (a direction)."""
    d = t - a
    dist = max(1e-5, d.length)
    u = d / dist
    dist = min(max(dist, abs(la - lb) + 1e-4), la + lb - 1e-4)
    x = (la * la - lb * lb + dist * dist) / (2 * dist)
    h = math.sqrt(max(0.0, la * la - x * x))
    n = V(pole) - u * u.dot(V(pole))
    if n.length < 1e-6:
        n = u.orthogonal()
    n.normalize()
    return a + u * x + n * h, a + u * dist


class Rig:
    """Joint positions and frames of a figure in a pose (anim.py's keys).
    World frame = the figure's (X forward, origin on the ground)."""

    def __init__(self, bd, p, tool=None, shield=False):
        self.bd, self.p = bd, p
        ff = (T(p["fallx"], 0, p["fallz"]) @ R("Y", p["fall"]) @ T(-p["fallx"], 0, -p["fallz"]))
        self.G = T(p["x"], p["y"], p["z"]) @ R("X", p["roll"]) @ ff
        hipz = bd.stand - p["drop"]
        self.P = self.G @ T(p["px"], p["py"], hipz) @ R("Z", p["pyaw"]) @ R("X", p["proll"])
        self.U = self.P @ R("Y", p["lean"]) @ R("X", p["side"]) @ R("Z", p["twist"])
        self.H = (self.U @ T(0.012, 0, bd.torso + bd.neck + bd.head_r * 0.62)
                  @ R("Z", p["hyaw"]) @ R("Y", p["hpitch"]))
        G3 = self.G.to_3x3()
        self.hip, self.knee, self.ankle, self.footM = {}, {}, {}, {}
        for k, s in (("L", 1), ("R", -1)):
            hip = self.P @ V((0, s * bd.hip_w, 0))
            f = V((p["F%sx" % k], s * (bd.hip_w + p["F%sy" % k]), p["F%sz" % k] + bd.ankle))
            knee, ank = two_bone(hip, self.G @ f, bd.thigh, bd.shin,
                                 G3 @ V((1, s * 0.12, 0)))
            self.hip[k], self.knee[k], self.ankle[k] = hip, knee, ank
            self.footM[k] = (self.G @ T(self.G.inverted() @ ank) @ R("Z", s * 7)
                             @ R("Y", -p["F%sp" % k]))
        # arms, FK first
        self.sh, self.el, self.hand, self.W = {}, {}, {}, {}
        for k, s in (("L", 1), ("R", -1)):
            sh = V((0, s * bd.shoulder_w, bd.torso - 0.035))
            MS = (self.U @ T(sh) @ R("Y", -p[k + "f"]) @ R("X", s * p[k + "a"])
                  @ R("Z", -s * p[k + "t"]))
            ME = MS @ T(0, 0, -bd.upper) @ R("Y", -p[k + "e"])
            self.sh[k] = self.U @ sh
            self.el[k] = MS @ V((0, 0, -bd.upper))
            self.W[k] = ME @ T(0, 0, -bd.fore)
            self.hand[k] = self.W[k].to_translation()
        # the tool in the right hand: its frame (origin at the grip, X along it)
        self.tool = None
        if tool is not None:
            M = self.W["R"] @ R("Y", p["Rw"])
            if p.get("aimw", 0) > 0:
                aim = (self.U.to_3x3() @ V((p["Tx"], p["Ty"], p["Tz"]))).normalized()
                cur = M.to_3x3() @ V((1, 0, 0))
                a = cur.lerp(aim, p["aimw"]).normalized()
                up = M.to_3x3() @ V((0, 0, 1))
                up = (up - a * up.dot(a)).normalized()
                y = up.cross(a)
                M = Matrix(((a.x, y.x, up.x, 0), (a.y, y.y, up.y, 0), (a.z, y.z, up.z, 0),
                            (0, 0, 0, 1)))
                M.translation = self.hand["R"]
            self.tool = M
        # IK hands
        pole = {"L": V((-0.35, 0.8, -0.55)), "R": V((-0.35, -0.8, -0.55))}
        U3 = self.U.to_3x3()
        for k, s in (("L", 1), ("R", -1)):
            w = p["ik" + k]
            if w <= 0:
                continue
            if k == "L" and p["grip"] > 0 and self.tool is not None:
                target = self.tool @ V((p["grip"], 0, 0))
            elif k == "L" and shield:
                target = self.shield_grip()
            else:
                target = self.U @ V((p[k + "x"], p[k + "y"], p[k + "z"]))
            target = self.hand[k].lerp(target, w)
            el, hand = two_bone(self.sh[k], target, bd.upper, bd.fore, U3 @ pole[k])
            self.el[k], self.hand[k] = el, hand
            d = (hand - el).normalized()
            self.W[k] = Matrix.Translation(hand) @ _frame(-d, U3 @ V((1, 0, 0))).to_4x4()

    def shield_grip(self):
        """Where the left hand holds the shield: behind its face, toward its
        right rim, the forearm across its middle."""
        c, n = self.shield_place()
        right = n.cross(V((0, 0, 1)))
        right = right.normalized() if right.length > 1e-4 else V((0, -1, 0))
        return c - n * 0.075 + right * 0.06

    def shield_place(self):
        p = self.p
        c = self.U @ V((p["Sx"], p["Sy"], p["Sz"]))
        n = (self.U.to_3x3() @ V((p["Snx"], p["Sny"], p["Snz"]))).normalized()
        return c, n


# ----------------------------------------------------------- the body

def draw_body(b, r, skin, shin=None, foot=None, thigh=None, upper=None, fore=None, hand=None):
    """Legs, arms, neck and the head's ball (outfits go over them)."""
    bd = r.bd
    for k in "LR":
        hip, knee, ank = r.hip[k], r.knee[k], r.ankle[k]
        b.seg(hip, knee, bd.thigh_r, bd.knee_r, thigh or skin, n=8)
        b.ball(knee, bd.knee_r, thigh or skin, n=8)
        b.seg(knee, ank, bd.shin_r * 1.05, bd.shin_r * 0.8, shin or skin, n=8)
        hx, hy, hz = bd.foot
        with b.at(r.footM[k], True):
            b.ball((hx * 0.55, 0, -bd.ankle + hz), 1.0, foot or skin, scale=(hx, hy, hz), n=10)
    for k in "LR":
        sh, el, ha = r.sh[k], r.el[k], r.hand[k]
        b.ball(sh, bd.arm_r * 1.12, upper or skin, n=8)
        b.seg(sh, el, bd.arm_r, bd.arm_r * 0.85, upper or skin, n=8)
        b.ball(el, bd.arm_r * 0.86, upper or skin, n=8)
        b.seg(el, ha, bd.fore_r, bd.fore_r * 0.8, fore or skin, n=8)
        b.ball(ha, bd.hand_r, hand or skin, scale=(1.0, 0.85, 1.1), n=8)
    with b.at(r.U, True):
        b.seg((0, 0, bd.torso - 0.03), (0.008, 0, bd.torso + bd.neck + 0.02), 0.048, 0.044, skin,
              n=8)


def draw_torso(b, r, m, flare=1.0):
    """The trunk in the torso frame, `m` one material or one per band."""
    c = r.bd.chest
    rings = [(0, 0, -0.02, .082 * c, .112 * c), (0, 0, 0.09, .076 * c, .104 * c),
             (0.006, 0, 0.19, .092 * c, .134 * c * flare), (0.0, 0, 0.265, .086 * c, .150 * c * flare),
             (0, 0, r.bd.torso + 0.005, .052 * c, .10 * c)]
    with b.at(r.U, True):
        b.loft(rings, m, n=12)


def draw_skirt(b, r, mats, top=0.085, hem=-0.15, zs=None, flare=1.3, w=(.088, .118)):
    """A tunic's skirt from the waist, its hem following the knees."""
    if not isinstance(mats, list):
        mats = [mats]
    Pi = r.P.inverted()
    kl, kr = Pi @ r.knee["L"], Pi @ r.knee["R"]
    zs = zs or [i / len(mats) for i in range(len(mats) + 1)]
    f = min(1.0, (top - hem) / r.bd.thigh)
    cx = (kl.x + kr.x) / 2
    spread = abs(kl.x - kr.x) / 2
    rings = []
    for t in zs:
        rings.append((cx * t * f * 0.85, (kl.y + kr.y) / 2 * t * f * 0.5, top + (hem - top) * t,
                      w[0] * (1 + (flare - 1) * t) + spread * f * t * 0.9,
                      w[1] * (1 + (flare - 1) * t * 0.6)))
    with b.at(r.P, True):
        b.loft(rings, mats, n=12)


def draw_head(b, r, skin, hair, style="short", beard=False, face=True, band=None):
    bd = r.bd
    k = bd.head_r / 0.115
    with b.at(r.H @ S(k, k, k), True):
        b.ball((0, 0, 0), 0.115, skin, scale=(1, 0.95, 1.04), n=12)
        if face:
            b.ball((0.113, 0, -0.018), 0.024, skin, n=6)
            dark = mat((0.07, 0.05, 0.05))
            for s in (1, -1):
                b.ball((0.098, s * 0.043, 0.012), 0.022, dark, scale=(0.5, 1, 1), n=6)
            b.ball((0.102, 0, -0.05), 0.026, mat((0.55, 0.25, 0.2)), scale=(0.4, 1.2, 0.4), n=6)
        if style == "short":
            b.ball((-0.028, 0, 0.03), 0.123, hair, n=12)
            b.cube((0.093, 0, 0.062), (0.03, 0.15, 0.03), hair)
        elif style == "long":
            b.ball((-0.028, 0, 0.03), 0.125, hair, n=12)
            b.loft([(-0.06, 0, 0.0, .09, .122), (-0.075, 0, -0.17, .07, .11)], hair, n=8)
        if beard:
            b.ball((0.05, 0, -0.075), 0.07, hair, scale=(1, 0.95, 0.85), n=8)
        if band is not None:
            b.loft([(-0.016, 0, 0.055, .130, .128), (-0.018, 0, 0.085, .123, .121)], band, n=12,
                   caps=False)


# ---------------------------------------------------------------- props

def tool_mesh(b, M, kind):
    """A tool in the frame M (origin at the grip, X along the handle toward
    its head, Z the head's striking side)."""
    wood, dark, iron = mat("wood", rough=0.8), mat("wood_dark", rough=0.8), \
        mat((0.62, 0.64, 0.68), rough=0.4, metal=0.6)
    with b.at(M, True):
        if kind == "hammer":
            b.seg((-0.05, 0, 0), (0.30, 0, 0), 0.026, 0.024, wood, n=6)
            b.cube((0.31, 0, 0.0), (0.10, 0.10, 0.22), dark)
            b.cube((0.31, 0, 0.0), (0.105, 0.105, 0.04), iron)
        elif kind == "axe":
            b.seg((-0.08, 0, 0), (0.48, 0, 0), 0.026, 0.024, wood, n=6)
            b.plate([(0.34, 0.0), (0.52, 0.0), (0.56, 0.21), (0.44, 0.23), (0.38, 0.08)], 0.04, iron,
                    frame=R("X", 90))
            b.cube((0.46, 0, -0.03), (0.08, 0.055, 0.08), dark)
        elif kind == "pick":
            b.seg((-0.08, 0, 0), (0.50, 0, 0), 0.026, 0.024, wood, n=6)
            b.seg((0.49, 0, -0.03), (0.43, 0, 0.30), 0.042, 0.008, iron, n=6)
            b.seg((0.49, 0, -0.03), (0.46, 0, -0.26), 0.042, 0.008, iron, n=6)
            b.ball((0.49, 0, -0.03), 0.048, iron, n=6)
        elif kind == "hoe":
            b.seg((-0.10, 0, 0), (0.62, 0, 0), 0.025, 0.023, wood, n=6)
            b.plate([(0.58, -0.02), (0.65, -0.02), (0.68, 0.22), (0.52, 0.22)], 0.03, iron,
                    frame=R("X", 90))
        elif kind == "spear":
            b.seg((-0.62, 0, 0), (0.95, 0, 0), 0.019, 0.017, wood, n=6)
            b.seg((0.94, 0, 0), (1.03, 0, 0), 0.014, 0.042, bronze(), n=6, sy=0.45)
            b.seg((1.03, 0, 0), (1.17, 0, 0), 0.042, 0.002, bronze(), n=6, sy=0.45)
            b.seg((-0.62, 0, 0), (-0.69, 0, 0), 0.017, 0.004, bronze(), n=6)


def shield_mesh(b, r, player, blazon):
    """The aspis on the left forearm: across its middle, facing its normal."""
    c, n = r.shield_place()
    el, ha = r.el["L"], r.hand["L"]
    d = (ha - el).normalized()
    n = (n - d * n.dot(d)).normalized()
    mid = el + (ha - el) * 0.5
    _aspis(b, mid + n * 0.085, n, player, blazon, Rr=0.25)


# -------------------------------------------------------------- figures

class Figure:
    """A unit's look and motion. Subclasses (or instances with these
    callables) supply `outfit(b, rig, player)`, `gear(anim)` ->
    (tool kind or None, shield bool), `load(anim)` and `poses` {anim:
    fn(t) -> pose}."""

    cls = "Foot"
    speed = 1.0                  # tiles a second (kinds.rs), for the walk
    animations = ()              # [(name, frames, ms, loops, impact)]
    seed = 0

    def __init__(self):
        self.bd = Build()

    def frames(self):
        for name, n, ms, loops, impact in self.animations:
            yield name, n, loops

    def pose_of(self, name, i):
        n, loops = next((n, lp) for a, n, _, lp, _ in self.animations if a == name)
        t = anim.frame_t(i, n, loops)
        fn = getattr(self, "pose_" + name, None)
        if fn is None:
            raise SystemExit("%s has no pose for %s" % (type(self).__name__, name))
        if name in ("death", "decay"):
            return fn(t, n) if name == "decay" else fn(t)
        return fn(t)

    def walk_pose(self, t, base, arms=28, which="LR"):
        p = anim.walk(t, self.speed, 0.8, self.bd.leg, base=base)
        return anim.swing_arms(p, t, arms, which)

    def build(self, name, i, player, parent=None):
        """The figure in frame i of animation `name`: one mesh under a root
        empty (the exporter turns the root to the facing)."""
        p = self.pose_of(name, i)
        tool, shield = self.gear(name)
        rig = Rig(self.bd, p, tool, shield)
        root = lib.empty("unit", (0, 0, 0), 0.0, parent)
        b = B("unit")
        self.outfit(b, rig, player)
        if tool:
            tool_mesh(b, rig.tool, tool)
        if shield:
            shield_mesh(b, rig, player, self.blazon)
        self.load(b, rig, name, player)
        b.finish(root)
        return root

    def load(self, b, rig, name, player):
        pass

    def pose_death(self, t):
        return anim.death(self.base(), t)

    def pose_decay(self, t, n):
        return anim.decay(self.base(), t, n)


# ------------------------------------------------------------- villager

def carry_load(b, r, kind, rnd):
    """What a villager walks home: food in a basket held in front, wood as
    logs on the right shoulder, stone a dressed block in both arms, gold
    nuggets heaped in a basket on the left shoulder."""
    U = r.U
    if kind == "food":
        c = (r.hand["L"] + r.hand["R"]) / 2 + U.to_3x3() @ V((0.05, 0, 0.02))
        with b.at(T(c), True):
            b.seg((0, 0, -0.06), (0, 0, 0.05), 0.10, 0.15, mat("straw", rough=0.9), n=12)
            b.seg((0, 0, 0.05), (0, 0, 0.065), 0.155, 0.155, mat("thatch"), n=12)
            for k in range(7):
                a = k * 2.4
                rr = 0.075 * (k % 3) / 2
                b.ball((rr * math.cos(a), rr * math.sin(a), 0.075 + 0.01 * (k % 2)), 0.045,
                       mat("flower_red", rough=0.5), n=6)
            b.ball((0.05, -0.05, 0.08), 0.04, mat("wheat"), n=6)
    elif kind == "wood":
        with b.at(U, True):
            for j, (y, z) in enumerate(((-0.11, 0.34), (-0.19, 0.36), (-0.15, 0.42))):
                b.seg((0.34, y, z + 0.07), (-0.36, y, z - 0.04), 0.055, 0.055,
                      mat(("wood", "cedar", "wood")[j], rough=0.9), n=8)
                b.ball((0.34, y, z + 0.07), 0.054, mat((0.86, 0.70, 0.46), rough=0.9),
                       scale=(0.3, 1, 1), n=8)
    elif kind == "stone":
        c = (r.hand["L"] + r.hand["R"]) / 2 + U.to_3x3() @ V((0.02, 0, 0.07))
        with b.at(T(c) @ U.to_quaternion().to_matrix().to_4x4(), True):
            b.cube((0.0, 0, 0.0), (0.20, 0.24, 0.16), mat((0.80, 0.78, 0.72), rough=0.9))
            b.cube((0.0, 0, 0.081), (0.17, 0.21, 0.004), mat((0.88, 0.86, 0.80), rough=0.9))
    elif kind == "gold":
        with b.at(U @ T(-0.02, 0.17, 0.36), True):
            b.seg((0, 0, -0.05), (0, 0, 0.06), 0.085, 0.12, mat("wood_dark", rough=0.9), n=10)
            for k in range(8):
                a = k * 2.3 + rnd.uniform(0, 0.3)
                rr = 0.07 * ((k % 3) / 2)
                b.ball((rr * math.cos(a), rr * math.sin(a), 0.07 + 0.012 * (k % 2)), 0.042,
                       gold(), scale=(1, 0.9, 0.8), n=6)


class Villager(Figure):
    speed = 0.9
    animations = [(n,) + anim.FOOT[n] for n in ("idle", "walk", "attack", "death", "decay")] + \
        [(n,) + anim.VILLAGER_WORK[n] for n in ("build", "carry_food", "carry_gold", "carry_stone",
                                                  "carry_wood", "chop", "farm", "forage", "mine")]
    TOOLS = {"attack": "axe", "build": "hammer", "chop": "axe", "mine": "pick", "farm": "hoe"}

    def __init__(self):
        super().__init__()
        self.skin_c, self.hair_c = lib.PAL["skin_tan"], "hair_brown"

    def gear(self, name):
        return self.TOOLS.get(name), False

    def outfit(self, b, r, player):
        self.skin, self.hair = mat(self.skin_c, rough=0.75), mat(self.hair_c, rough=0.9)
        cloth = pm(player)
        dark = pm(player, 0.64)
        lin = mat((0.93, 0.86, 0.70), rough=0.9)
        leather = mat("leather", rough=0.8)
        draw_body(b, r, self.skin, foot=leather, upper=cloth)
        draw_skirt(b, r, [cloth, cloth, dark], top=0.09, hem=-0.17, zs=[0, 0.5, 0.88, 1.0],
                   flare=1.32)
        draw_torso(b, r, [cloth, cloth, cloth, cloth], 1.0)
        with b.at(r.P, True):     # the belt
            b.loft([(0, 0, 0.065, .088, .12), (0, 0, 0.105, .086, .117)], leather, n=12,
                   caps=False)
        draw_head(b, r, self.skin, self.hair, "short", beard=True, band=lin)

    def load(self, b, r, name, player):
        if name.startswith("carry_"):
            carry_load(b, r, name[6:], random.Random(7))
        if name == "forage":
            with b.at(r.P @ T(0.02, 0.15, -0.02) @ R("X", 8), True):     # a basket on the hip
                b.seg((0, 0, -0.08), (0, 0, 0.05), 0.07, 0.10, mat("straw", rough=0.9), n=10)
                for k in range(4):
                    b.ball((0.03 * math.cos(k * 1.6), 0.03 * math.sin(k * 1.6), 0.055), 0.035,
                           mat("flower_red", rough=0.5), n=6)

    def base(self):
        return anim.pose(FLx=0.02, FLy=0.01, FRx=-0.02, FRy=0.01, Lf=6, La=10, Le=14, Rf=6,
                         Ra=10, Re=14)

    def pose_idle(self, t):
        return anim.breathe(self.base(), t)

    def pose_walk(self, t):
        return self.walk_pose(t, self.base())

    # ---- carrying: the walk with the arms holding the load
    def pose_carry_food(self, t):
        p = self.walk_pose(t, self.base(), arms=0)
        return dict(p, ikL=1, ikR=1, Lx=0.20, Ly=0.11, Lz=0.08, Rx=0.20, Ry=-0.11, Rz=0.08,
                    lean=p["lean"] - 5)

    def pose_carry_stone(self, t):
        p = self.walk_pose(t, self.base(), arms=0)
        return dict(p, ikL=1, ikR=1, Lx=0.17, Ly=0.12, Lz=0.05, Rx=0.17, Ry=-0.12, Rz=0.05,
                    lean=p["lean"] - 8, drop=p["drop"] + 0.01)

    def pose_carry_wood(self, t):
        p = self.walk_pose(t, self.base(), arms=26, which="L")
        return dict(p, ikR=1, Rx=0.12, Ry=-0.15, Rz=0.36, side=p["side"] + 4)

    def pose_carry_gold(self, t):
        p = self.walk_pose(t, self.base(), arms=26, which="R")
        return dict(p, ikL=1, Lx=0.02, Ly=0.22, Lz=0.40, side=p["side"] - 5)

    # ---- swings: two hands on the handle
    def swing(self, t, up, hit, grip, rise=0.0):
        """A two-handed swing: up (wound) at t=0, the stroke landing on
        frame 3 of 6 (t = 0.5) as the attack's impact does, the follow
        through and lift back between."""
        down = dict(hit)
        follow = anim.lerp(hit, up, 0.3)
        lift = anim.lerp(hit, up, 0.75)
        keys = [(0.0, up), (0.333, anim.lerp(up, hit, 0.35), anim.ease_in),
                (0.5, down, anim.ease_in), (0.667, follow), (0.833, lift)]
        p = anim.keyed(keys, t)
        p["grip"] = grip
        return p

    def pose_attack(self, t):
        base = anim.pose(self.base(), FLx=0.14, FLy=0.02, FRx=-0.12, FRy=0.02, ikL=1)
        up = anim.pose(base, Rf=165, Ra=20, Re=55, Rw=-30, lean=-6, twist=-14, drop=0.01,
                       hpitch=-6, ikL=1)
        hit = anim.pose(base, Rf=70, Ra=10, Re=8, Rw=65, lean=18, twist=12, drop=0.05,
                        hpitch=10, ikL=1)
        p = anim.keyed([(0.0, anim.lerp(up, hit, 0.5)), (0.4, up), (0.6, hit, anim.ease_in),
                        (1.0, anim.lerp(hit, up, 0.4))], t, loop=False)
        p["grip"] = 0.16
        return p

    def pose_chop(self, t):
        base = anim.pose(self.base(), FLx=0.10, FLy=0.03, FRx=-0.10, FRy=0.03, ikL=1)
        up = anim.pose(base, Rf=150, Ra=40, Re=60, Rw=-40, Rt=20, twist=-28, lean=-4, hyaw=12)
        hit = anim.pose(base, Rf=75, Ra=12, Re=10, Rw=60, Rt=-10, twist=20, lean=12, drop=0.03,
                        hyaw=-6)
        return self.swing(t, up, hit, 0.18)

    def pose_mine(self, t):
        base = anim.pose(self.base(), FLx=0.11, FLy=0.04, FRx=-0.11, FRy=0.04, ikL=1)
        up = anim.pose(base, Rf=175, Ra=18, Re=60, Rw=-35, lean=-8, hpitch=-8)
        hit = anim.pose(base, Rf=55, Ra=8, Re=10, Rw=30, lean=24, drop=0.07, hpitch=14)
        return self.swing(t, up, hit, 0.2)

    def pose_farm(self, t):
        base = anim.pose(self.base(), FLx=0.12, FLy=0.03, FRx=-0.12, FRy=0.03, ikL=1)
        up = anim.pose(base, Rf=110, Ra=12, Re=40, Rw=-10, lean=6, hpitch=6)
        hit = anim.pose(base, Rf=40, Ra=8, Re=10, Rw=35, lean=22, drop=0.05, hpitch=16)
        return self.swing(t, up, hit, 0.24)

    def pose_build(self, t):
        base = anim.pose(self.base(), FLx=0.13, FLy=0.05, FRx=-0.14, FRy=0.04, drop=0.10,
                         lean=24, hpitch=14, Lf=55, La=14, Le=30)
        up = anim.pose(base, Rf=140, Ra=22, Re=80, Rw=-40)
        hit = anim.pose(base, Rf=48, Ra=10, Re=18, Rw=40, drop=0.12)
        return self.swing(t, up, hit, 0.0)

    def pose_forage(self, t):
        base = anim.pose(self.base(), FLx=0.10, FLy=0.04, FRx=-0.08, FRy=0.04)
        reach = anim.pose(base, lean=42, drop=0.13, hpitch=18, Rf=70, Ra=8, Re=5, Lf=20, Le=40)
        pick = anim.pose(reach, Rf=62, Re=25)
        up = anim.pose(base, lean=10, drop=0.03, Rf=20, Ra=20, Re=95, Rt=30, Lf=10, Le=20)
        return anim.keyed([(0.0, reach), (0.2, pick), (0.5, up), (0.7, up), (0.85, reach)], t)


# -------------------------------------------------------------- hoplite

class Hoplite(Figure):
    speed = 0.8
    animations = [(n,) + anim.FOOT[n] for n in ("idle", "walk", "attack", "death", "decay")]
    blazon = "lambda"

    def __init__(self):
        super().__init__()
        self.skin_c = lib.PAL["skin_light"]
        self.bd.chest = 1.08

    def gear(self, name):
        return "spear", True

    def outfit(self, b, r, player):
        self.skin = mat(self.skin_c, rough=0.75)
        cloth = pm(player)
        dark = pm(player, 0.82)
        br = bronze()
        lin = mat("linen", rough=0.9)
        leather = mat("leather", rough=0.8)
        draw_body(b, r, self.skin, shin=br, foot=leather, upper=cloth)
        draw_skirt(b, r, [cloth, cloth, dark], top=0.08, hem=-0.15, zs=[0, 0.4, 0.85, 1.0],
                   flare=1.4)
        with b.at(r.P, True):          # the pteruges' fringe over the skirt's top
            b.loft([(0, 0, 0.02, .094, .126), (0, 0, -0.04, .108, .138)], lin, n=12, caps=False)
        draw_torso(b, r, br, 1.0)
        draw_head(b, r, self.skin, None, "none", face=False)
        self.helmet(b, r, cloth)
        self.cape(b, r, dark)

    def helmet(self, b, r, crest_m):
        br = bronze()
        k = r.bd.head_r / 0.115
        with b.at(r.H @ S(k, k, k), True):   # the Corinthian helmet
            b.ball((0.0, 0, 0.012), 0.131, br, scale=(1.03, 0.96, 1.0), n=12)
            b.seg((0.004, 0, 0.02), (0.012, 0, -0.125), 0.131, 0.112, br, n=12, sy=0.94)
            dk = mat((0.04, 0.03, 0.03))
            for s in (1, -1):
                b.ball((0.112, s * 0.05, 0.004), 0.034, dk, scale=(0.6, 1.1, 0.8), n=6)
            b.cube((0.118, 0, -0.075), (0.03, 0.026, 0.10), dk)
            b.cube((0.132, 0, -0.01), (0.02, 0.022, 0.07), br)
            # a tall crest from the brow over to the nape, in the player's colour
            inn, out = [], []
            for i in range(11):
                a = math.radians(-50 + 175 * i / 10)
                hh = 0.13 if i < 8 else 0.13 + 0.03 * (i - 7)
                inn.append((0.126 * math.sin(a), 0.012 + 0.126 * math.cos(a)))
                out.append(((0.128 + hh) * math.sin(a), 0.012 + (0.128 + hh) * math.cos(a)))
            b.plate(inn + list(reversed(out)), 0.075, crest_m, frame=R("X", 90))
            b.seg((0.0, 0, 0.13), (0.0, 0, 0.15), 0.03, 0.03, br, n=6)

    def cape(self, b, r, m):
        """A short cape from the shoulders, swinging back as it walks."""
        p = r.p
        fly = min(1.0, max(0.0, (p["lean"] - 2) / 10)) * 0.5 + 0.1
        vs, fs = [], []
        nr, nc = 5, 5
        for i in range(nr):
            t = i / (nr - 1)
            for j in range(nc):
                u = j / (nc - 1) - 0.5
                w = 0.30 + 0.12 * t
                x = -0.105 - 0.04 * t - fly * t * 0.12 - 0.035 * (1 - (2 * u) ** 2) * (0.5 + t)
                vs.append((x, u * w, r.bd.torso + 0.0 - 0.36 * t))
        for i in range(nr - 1):
            for j in range(nc - 1):
                k = i * nc + j
                fs.append((k, k + 1, k + nc + 1, k + nc))
        with b.at(r.U, True):
            b.poly(vs, fs, m, smooth=True)
            b.poly([(x - 0.004, y, z) for x, y, z in vs], [tuple(reversed(f)) for f in fs], m,
                   smooth=True)

    def base(self):
        return anim.pose(FLx=0.04, FLy=0.02, FRx=-0.03, FRy=0.02, Rf=8, Ra=14, Re=70, Rt=6,
                         Rw=0, aimw=1, Tx=0.05, Ty=0.0, Tz=1.0,
                         ikL=1, Sx=0.17, Sy=0.13, Sz=0.12, Snx=0.85, Sny=0.55, Snz=0.08)

    def pose_idle(self, t):
        return anim.breathe(self.base(), t)

    def pose_walk(self, t):
        p = self.walk_pose(t, self.base(), arms=14, which="R")
        return dict(p, Tx=0.35, Tz=1.0)

    def pose_attack(self, t):
        # the spear overhand: raised to the shoulder pointing at the enemy,
        # drawn back, thrust on the impact frame (3 of 6), drawn back again
        base = anim.pose(self.base(), FLx=0.13, FLy=0.03, FRx=-0.13, FRy=0.04, drop=0.03,
                         Sx=0.22, Sy=0.08, Sz=0.14, Snx=1.0, Sny=0.3, Snz=0.0, Tx=1.0, Ty=-0.02,
                         Tz=-0.05)
        ready = anim.pose(base, Rf=70, Ra=75, Re=95, Rt=-30, twist=-6)
        drawn = anim.pose(base, Rf=20, Ra=80, Re=110, Rt=-40, twist=-12, lean=-6,
                          hyaw=10, Ty=0.10)
        thrust = anim.pose(base, Rf=95, Ra=40, Re=10, Rt=-20, twist=6, lean=10, drop=0.06,
                           px=0.04, hyaw=-6, FLx=0.17, Ty=-0.25, Tz=-0.02)
        return anim.keyed([(0.0, ready), (0.4, drawn), (0.6, thrust, anim.ease_in),
                           (0.8, anim.lerp(thrust, ready, 0.6)), (1.0, ready)], t, loop=False)

    def pose_death(self, t):
        p = anim.death(self.base(), t)
        p["ikL"] = max(0.0, 1 - t * 2.5)
        p["aimw"] = max(0.0, 1 - t * 1.5)
        return p

    def pose_decay(self, t, n):
        p = anim.decay(self.base(), t, n)
        p["ikL"] = 0.0
        p["aimw"] = 0.0
        return p


def _unit(cls):
    """The factory export.py calls for a figure (marked `unit`)."""
    def make():
        return cls()
    make.unit = True
    make.figure = cls
    return make


villager = _unit(Villager)
hoplite = _unit(Hoplite)

# The unit sets (docs/look/units.md, "SETS"): export_all.py exports each
# with export.py's unit path (unitexport.py).
SETS = {
    "villager": dict(func="units.villager", cls="Foot", scale=1),
    "hoplite": dict(func="units.hoplite", cls="Foot", scale=1),
}


def figure(name):
    return globals()[name]()


# ------------------------------------------------------------ preview

def _preview(name, out, only=None):
    """A contact sheet through the game camera, no exporter: every
    animation's frames facing S, or one animation in five facings."""
    import export
    lib.reset()
    fig = figure(name)
    rows = []
    for a, n, ms, loops, imp in fig.animations:
        if only and a != only:
            continue
        rows += [(a, f) for f in (export.FACINGS if only else ["S"])]
    lib.box((60, 60, 0.2), (0, 0, -0.1), lib.mat("grass", rough=1.0))
    cols = max(n for a, n, *_ in fig.animations if not only or a == only)
    sp = 1.6
    for j, (a, f) in enumerate(rows):
        n = next(n for aa, n, *_ in fig.animations if aa == a)
        for i in range(n):
            # screen right is (+X, +Y), screen down (+X, -Y)
            sx, sy = i * sp, j * sp * 1.2
            loc = (sx * 0.7071 + sy * 0.7071 * 1.0, sx * 0.7071 - sy * 0.7071, 0)
            root = fig.build(a, i, lib.BLUE if j % 2 == 0 else lib.RED)
            root.location = loc
            root.rotation_euler = (0, 0, math.radians(export.FACINGS[f]))
    cx, cy = (cols - 1) * sp / 2, (len(rows) - 1) * sp * 1.2 / 2
    lib.camera((cx * 0.7071 + cy * 0.7071, cx * 0.7071 - cy * 0.7071, 0.5),
               cols * sp + 1.5, (110 * cols, int(110 * 1.2 * len(rows) * 0.62) + 60))
    lib.light()
    lib.render(out, samples=24)


if __name__ == "__main__":
    _preview(sys.argv[1], sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else None)

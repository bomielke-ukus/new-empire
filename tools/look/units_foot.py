"""The new look's foot soldiers and the priest (docs/look/units.md): one
`Figure` each over units.py's rig, body and shared motion, exported as
turning animated colour sets by export_all.py like units.py's villager and
hoplite.

Each kind is told apart at zoom 1 by its silhouette first (weapon, shield
shape, headgear, cloak) and by large areas of the player colour:

    clubman    near-naked, shaggy hair and beard, a player-colour hide kilt
               and pelt across the chest, a knobbed club over the shoulder;
               smashes it down overhead
    axeman     a player tunic under a hide jerkin, a pointed fur cap, a
               long-hafted bearded axe; a two-handed diagonal chop
    swordsman  a pointed bronze helmet, a leather cuirass, a player tunic
               and a long player cape, a bronze leaf sword; a slash
    legionary  the best armoured: banded iron cuirass and shoulder plates,
               an iron helmet with cheek guards and a short player plume,
               the big curved rectangular shield in the player colour; a
               short-sword stab from behind it
    spearman   a plain kit: a leather skullcap, no crest, a player tunic,
               an oval hide shield with a player-colour field, a plain
               spear; an underhand thrust at the waist (the hoplite's is
               overhand, with a crest and a round bronze shield)
    bowman     a player Phrygian cap, a quiver on the back, the bow; draws
               and looses (the arrow leaves on the impact frame: the view
               flies it from there)
    slinger    a wide player headband with flying tails, a stone bag at the
               hip, the sling; whirls it overhead and releases on impact
    priest     a long white robe, a player mantle and stole, a tall linen
               cap, a staff with a gold sun disc; raises staff and hand to
               chant (the attack animation plays while converting; its last
               frame is held, so it ends on the raised pose)

    /usr/bin/python3.13 tools/look/unitexport.py NAME OUT.png [anims]

does not know this module; preview with export_all.py --only NAME, or
units_foot.py NAME OUT.png [anim] (a contact sheet, as units.py's).
"""
import math
import os
import sys

import bpy  # noqa: F401  (before bmesh)
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import anim  # noqa: E402
import lib  # noqa: E402
from figures import bronze, gold, _frame, _quiver  # noqa: E402
from lib import mat  # noqa: E402
from units import (Figure, R, S, T, draw_body, draw_cape, draw_head, draw_skirt,  # noqa: E402
                   draw_torso, drop_tool, pm, _unit)

V = Vector
FOOT = [(n,) + anim.FOOT[n] for n in ("idle", "walk", "attack", "death", "decay")]


def aim(x, y, z, up=1.0):
    """Pose keys pointing the right hand's tool along (x, y, z) in the torso
    frame, its striking side leading the swing (anim.py's aimup)."""
    return dict(aimw=1.0, Tx=x, Ty=y, Tz=z, aimup=up)


def head_frame(r):
    """The head's frame scaled so the skull is units.draw_head's 0.115 ball."""
    k = r.bd.head_r / 0.115
    return r.H @ S(k, k, k)


def iron():
    return mat((0.66, 0.68, 0.72), rough=0.38, metal=0.6)


def iron_dark():
    return mat((0.34, 0.35, 0.38), rough=0.5, metal=0.5)


class Soldier(Figure):
    """A foot soldier: idle and walk from its base stance, death and decay
    shared, with the hands let go of what they held as it falls."""

    animations = FOOT
    walk_arms = (24, "L")             # the swing amount and which arms swing

    def pose_idle(self, t):
        return anim.breathe(self.base(), t)

    def pose_walk(self, t):
        amount, which = self.walk_arms
        return self.walk_pose(t, self.walk_base(), amount, which)

    def walk_base(self):
        return self.base()

    def pose_death(self, t):
        p = anim.death(self.base(), t)
        p["ikL"] = p["ikL"] * max(0.0, 1 - t * 2.5)
        p["ikR"] = p["ikR"] * max(0.0, 1 - t * 2.5)
        p["grip"] = 0.0
        return drop_tool(p, (t - 0.25) / 0.6)

    def pose_decay(self, t, n):
        p = anim.decay(self.base(), t, n)
        p["ikL"] = p["ikR"] = p["grip"] = 0.0
        return drop_tool(p, 1.0)

    def blow(self, ready, up, hit, t):
        """An overhead or swung blow: from the ready stance (frame 0) up
        (frame 2), landing on frame 3 (the impact), recovering after."""
        return anim.keyed([(0.0, ready), (0.4, up), (0.6, hit, anim.ease_in),
                           (1.0, anim.lerp(hit, up, 0.35))], t, loop=False)


# --------------------------------------------------------------- clubman

class Clubman(Soldier):
    speed = 1.1
    walk_arms = (26, "L")

    def __init__(self):
        super().__init__()
        self.bd.chest = 1.12
        self.bd.arm_r, self.bd.fore_r = 0.054, 0.047

    def gear(self, name):
        return "club", False

    def outfit(self, b, r, player):
        skin = mat(lib.PAL["skin_tan"], rough=0.7)
        hide = pm(player)
        hide_d = pm(player, 0.64)
        hair = mat("hair_black", rough=0.95)
        fur = mat((0.42, 0.30, 0.20), rough=0.95)
        draw_body(b, r, skin)
        draw_torso(b, r, skin, 1.05)
        # the hide kilt, ragged and short, and a fur belt
        draw_skirt(b, r, [hide, hide, hide_d], top=0.09, hem=-0.10, zs=[0, 0.5, 0.85, 1.0],
                   flare=1.35)
        with b.at(r.P, True):
            b.loft([(0, 0, 0.065, .094, .126), (0, 0, 0.115, .092, .123)], fur, n=12,
                   caps=False)
            for k in range(6):          # ragged points at the hem
                a = math.radians(30 + k * 60)
                b.ball((0.11 * math.cos(a) * 1.05, 0.15 * math.sin(a), -0.07), 0.04, hide_d,
                       scale=(1, 1, 1.6), n=6)
        # a pelt over the left shoulder, across the chest to the right hip
        c = r.bd.chest
        with b.at(r.U @ T(0.0, 0.0, 0.17) @ R("X", 38), True):
            b.loft([(0.0, 0, -0.045, .100 * c, .175 * c), (0.0, 0, 0.0, .106 * c, .182 * c),
                    (0.0, 0, 0.045, .098 * c, .170 * c)], hide, n=12, caps=False)
        with b.at(r.U, True):            # its knot and tail on the left shoulder
            b.ball((0.0, 0.14, 0.28), 0.06, hide, scale=(1.2, 1, 0.8), n=8)
        # wrist and ankle wraps
        for k in "LR":
            b.ball(r.hand[k] + (r.el[k] - r.hand[k]) * 0.25, r.bd.fore_r * 1.05, fur, n=6)
        draw_head(b, r, skin, hair, "long", beard=True, band=hide_d)
        with b.at(head_frame(r), True):          # shaggy hair, sticking up
            for (x, y, z, rr) in ((-0.02, 0.0, 0.12, 0.07), (-0.07, 0.06, 0.10, 0.06),
                                  (-0.07, -0.06, 0.10, 0.06), (0.03, 0.07, 0.10, 0.05),
                                  (0.03, -0.07, 0.10, 0.05), (-0.11, 0, 0.04, 0.07)):
                b.ball((x, y, z), rr, hair, n=6)
            b.ball((0.06, 0, -0.10), 0.075, hair, scale=(1, 1, 1.15), n=8)

    def base(self):
        # the club resting on the right shoulder
        return anim.pose(FLx=0.06, FLy=0.04, FRx=-0.05, FRy=0.04, Lf=8, La=16, Le=20,
                         Rf=40, Ra=20, Re=100, **aim(-0.50, -0.12, 0.86))

    def pose_attack(self, t):
        st = anim.pose(self.base(), FLx=0.14, FLy=0.04, FRx=-0.12, FRy=0.04, drop=0.02,
                       Lf=40, La=24, Le=30)
        ready = anim.pose(st, Rf=95, Ra=26, Re=60, lean=0, **aim(-0.1, -0.45, 0.89))
        up = anim.pose(st, Rf=165, Ra=28, Re=45, lean=-8, twist=-10, hpitch=-8,
                       **aim(-0.55, -0.35, 0.76))
        hit = anim.pose(st, Rf=78, Ra=20, Re=5, lean=22, twist=10, drop=0.06, hpitch=12,
                        Lf=20, La=40, **aim(0.80, -0.32, -0.50))
        return self.blow(ready, up, hit, t)


# ---------------------------------------------------------------- axeman

class Axeman(Soldier):
    speed = 1.1

    def __init__(self):
        super().__init__()
        self.bd.chest = 1.06

    def gear(self, name):
        return "battleaxe", False

    def outfit(self, b, r, player):
        skin = mat(lib.PAL["skin_tan"], rough=0.75)
        cloth, dark = pm(player), pm(player, 0.64)
        hide = mat((0.50, 0.36, 0.22), rough=0.9)
        fur = mat((0.30, 0.22, 0.15), rough=0.95)
        leather = mat("leather", rough=0.8)
        draw_body(b, r, skin, foot=leather, shin=leather, upper=cloth)
        draw_skirt(b, r, [cloth, cloth, dark], top=0.09, hem=-0.15, zs=[0, 0.5, 0.86, 1.0],
                   flare=1.35)
        # a sleeveless hide jerkin over the tunic, the tunic's colour at the
        # neck, sleeves and skirt
        draw_torso(b, r, [hide, hide, hide, cloth], 1.04)
        with b.at(r.P, True):
            b.loft([(0, 0, 0.06, .092, .124), (0, 0, 0.11, .090, .121)], leather, n=12,
                   caps=False)
        draw_head(b, r, skin, mat("hair_brown", rough=0.9), "short", beard=True)
        with b.at(head_frame(r), True):           # a pointed fur cap
            b.loft([(-0.01, 0, 0.02, .128, .124), (-0.015, 0, 0.07, .124, .12),
                    (-0.03, 0, 0.14, .08, .075), (-0.05, 0, 0.21, .02, .02)], fur, n=12)
            b.loft([(-0.01, 0, 0.0, .135, .131), (-0.01, 0, 0.04, .135, .131)], dark, n=12)

    def base(self):
        # the axe held upright at the right side, its head by the shoulder
        return anim.pose(FLx=0.05, FLy=0.03, FRx=-0.04, FRy=0.03, Lf=6, La=12, Le=18,
                         Rf=18, Ra=14, Re=55, Nx=0.7, Ny=0.7, Nz=0.0,
                         **aim(0.20, -0.12, 0.97))

    def pose_attack(self, t):
        st = anim.pose(self.base(), FLx=0.15, FLy=0.03, FRx=-0.12, FRy=0.04, drop=0.02, ikL=1,
                       Nx=0.0, Ny=0.55, Nz=0.83)
        ready = anim.pose(st, Rf=80, Ra=24, Re=50, twist=-6, **aim(0.35, -0.55, 0.76))
        up = anim.pose(st, Rf=150, Ra=40, Re=45, twist=-26, lean=-6, hyaw=10,
                       **aim(-0.35, -0.62, 0.70))
        hit = anim.pose(st, Rf=80, Ra=14, Re=8, twist=22, lean=16, drop=0.06, hyaw=-8,
                        **aim(0.82, 0.32, -0.47))
        p = self.blow(ready, up, hit, t)
        p["grip"] = 0.20
        return p


# -------------------------------------------------------------- swordsman

class Swordsman(Soldier):
    speed = 1.1

    def __init__(self):
        super().__init__()
        self.bd.chest = 1.06

    def gear(self, name):
        return "sword", False

    def outfit(self, b, r, player):
        skin = mat(lib.PAL["skin_light"], rough=0.75)
        cloth, mid, dark = pm(player), pm(player, 0.82), pm(player, 0.64)
        leather = mat((0.55, 0.36, 0.18), rough=0.6)
        sandal = mat("leather", rough=0.8)
        br = bronze()
        draw_body(b, r, skin, foot=sandal, upper=cloth)
        draw_skirt(b, r, [cloth, cloth, dark], top=0.08, hem=-0.15, zs=[0, 0.45, 0.85, 1.0],
                   flare=1.35)
        draw_torso(b, r, leather, 1.02)          # a leather cuirass
        with b.at(r.U, True):                    # its bronze breast disc
            b.ball((0.085 * r.bd.chest, 0, 0.19), 0.05, br, scale=(0.35, 1, 1), n=10)
        with b.at(r.P, True):
            b.loft([(0, 0, 0.05, .092, .124), (0, 0, 0.10, .090, .121)], br, n=12, caps=False)
        draw_head(b, r, skin, mat("hair_black", rough=0.9), "short", beard=False)
        with b.at(head_frame(r), True):          # a pointed bronze helmet
            b.loft([(-0.005, 0, -0.01, .135, .131), (-0.01, 0, 0.06, .128, .124),
                    (-0.02, 0, 0.13, .08, .077), (-0.03, 0, 0.21, .025, .025),
                    (-0.035, 0, 0.27, .004, .004)], br, n=12)
            b.loft([(0.0, 0, -0.015, .142, .138), (0.0, 0, 0.012, .142, .138)], br, n=12)
            b.cube((0.135, 0, -0.04), (0.02, 0.02, 0.07), br)      # nose guard
        draw_cape(b, r, mid, length=0.48, width=0.30, flare=0.14)

    def base(self):
        # the sword held up before the right shoulder
        return anim.pose(FLx=0.06, FLy=0.03, FRx=-0.05, FRy=0.03, Lf=8, La=14, Le=20,
                         Rf=30, Ra=18, Re=70, **aim(0.35, -0.20, 0.91))

    def pose_attack(self, t):
        st = anim.pose(self.base(), FLx=0.16, FLy=0.03, FRx=-0.12, FRy=0.05, drop=0.03,
                       Lf=30, La=38, Le=25, Nx=0.0, Ny=0.45, Nz=0.89)
        ready = anim.pose(st, Rf=70, Ra=30, Re=60, **aim(0.55, -0.35, 0.76))
        up = anim.pose(st, Rf=140, Ra=60, Re=60, twist=-26, lean=-4, hyaw=12,
                       **aim(-0.30, -0.75, 0.59))
        hit = anim.pose(st, Rf=82, Ra=8, Re=4, twist=26, lean=12, drop=0.05, hyaw=-10,
                        **aim(0.72, 0.58, -0.38))
        return self.blow(ready, up, hit, t)


# -------------------------------------------------------------- legionary

def scutum(b, c, n, player, w=0.17, h=0.29, bow=0.26):
    """The legionary's curved rectangular shield centred at c facing n: the
    player colour's face, a gold rim, boss and bars."""
    F = T(V(c)) @ _frame(n, (0, 0, 1))
    face = mat(player, rough=0.6)
    g = gold()
    back = mat("wood_dark", rough=0.8)
    cols, rows = 6, 2
    phi = w / bow                                 # half the arc's angle

    def pt(u, v, off=0.0):
        a = phi * u
        rr = bow + off
        return (rr * math.sin(a), h * v, rr * math.cos(a) - bow)

    with b.at(F):
        vs, fs, vb = [], [], []
        for j in range(rows + 1):
            v = -1 + 2 * j / rows
            for i in range(cols + 1):
                u = -1 + 2 * i / cols
                vs.append(pt(u, v))
                vb.append(pt(u, v, -0.025))
        for j in range(rows):
            for i in range(cols):
                k = j * (cols + 1) + i
                fs.append((k, k + 1, k + cols + 2, k + cols + 1))
        b.poly(vs, fs, face, smooth=True)
        b.poly(vb, [tuple(reversed(f)) for f in fs], back, smooth=True)
        # the rim: the edges joined front to back in gold
        edge = [(u, -1) for u in [-1 + 2 * i / cols for i in range(cols + 1)]] + \
               [(1, -1 + 2 * j / rows) for j in range(1, rows + 1)] + \
               [(u, 1) for u in [1 - 2 * i / cols for i in range(1, cols + 1)]] + \
               [(-1, 1 - 2 * j / rows) for j in range(1, rows)]
        for (u0, v0), (u1, v1) in zip(edge, edge[1:] + edge[:1]):
            b.seg(V(pt(u0, v0, -0.012)), V(pt(u1, v1, -0.012)), 0.018, 0.018, g, n=5)
        # the boss and the gold bars across and along it
        b.ball((0, 0, 0.012), 0.06, g, scale=(1, 1, 0.55), n=10)
        for v in (-0.55, 0.55):
            for i in range(cols):
                u0, u1 = -1 + 2 * i / cols, -1 + 2 * (i + 1) / cols
                b.seg(V(pt(u0 * 0.92, v, 0.006)), V(pt(u1 * 0.92, v, 0.006)), 0.011, 0.011, g,
                      n=4, sy=0.5)
        b.seg(V(pt(0, -0.92, 0.006)), V(pt(0, 0.92, 0.006)), 0.012, 0.012, g, n=4, sy=0.5)


class Legionary(Soldier):
    speed = 0.9
    walk_arms = (16, "R")

    def __init__(self):
        super().__init__()
        self.bd.chest = 1.12

    def gear(self, name):
        return "gladius", True

    def draw_shield(self, b, r, player):
        c, n = r.shield_place()
        el, ha = r.el["L"], r.hand["L"]
        d = (ha - el).normalized()
        n = (n - d * n.dot(d)).normalized()
        mid = el + (ha - el) * 0.5
        scutum(b, mid + n * 0.07, n, player)

    def outfit(self, b, r, player):
        skin = mat(lib.PAL["skin_light"], rough=0.75)
        cloth, dark = pm(player), pm(player, 0.64)
        ir, ird = iron(), iron_dark()
        leather = mat("leather", rough=0.8)
        draw_body(b, r, skin, foot=leather, upper=cloth, shin=leather)
        draw_skirt(b, r, [cloth, cloth, dark], top=0.07, hem=-0.13, zs=[0, 0.45, 0.85, 1.0],
                   flare=1.3)
        with b.at(r.P, True):           # the belt and its hanging straps
            b.loft([(0, 0, 0.03, .096, .128), (0, 0, 0.08, .094, .125)], leather, n=12,
                   caps=False)
            for y in (-0.05, -0.015, 0.02, 0.055):
                b.cube((0.098, y, -0.03), (0.012, 0.02, 0.12), leather)
        # the banded cuirass: iron hoops with dark seams, shoulder plates
        c = r.bd.chest
        draw_torso(b, r, ir, 1.04)
        with b.at(r.U, True):
            for z, rx, ry in ((0.05, .082, .110), (0.11, .080, .108), (0.17, .094, .134),
                              (0.23, .092, .146)):
                b.loft([(0, 0, z - 0.006, rx * c + 0.004, ry * c + 0.004),
                        (0, 0, z + 0.006, rx * c + 0.004, ry * c + 0.004)], ird, n=12,
                       caps=False)
            for s in (1, -1):
                for k, (dz, rr) in enumerate(((0.0, 0.074), (-0.04, 0.070), (-0.08, 0.066))):
                    b.ball((0.0, s * (r.bd.shoulder_w + 0.006), r.bd.torso - 0.02 + dz), rr,
                           ir if k % 2 == 0 else ird, scale=(1.1, 0.95, 0.55), n=10)
        draw_head(b, r, skin, None, "none", face=True)
        with b.at(head_frame(r), True):           # the iron helmet
            b.ball((-0.005, 0, 0.02), 0.13, ir, scale=(1.04, 1.0, 0.95), n=12)
            b.loft([(-0.02, 0, -0.01, .132, .128), (-0.035, 0, 0.0, .15, .14)], ir, n=12)
            b.loft([(-0.11, 0, -0.03, .05, .12), (-0.15, 0, -0.10, .05, .13)], ir, n=10)
            for s in (1, -1):              # cheek guards
                b.cube((0.05, s * 0.112, -0.07), (0.08, 0.018, 0.11), ir)
            b.loft([(0.06, 0, 0.07, .07, .13), (0.075, 0, 0.09, .06, .12)], gold(), n=10,
                   caps=False)
            # a short upright plume in the player colour, front to back
            b.seg((0.0, 0, 0.13), (0.0, 0, 0.16), 0.022, 0.022, gold(), n=6)
            b.plate([(-0.09, 0.15), (0.08, 0.15), (0.07, 0.25), (-0.10, 0.26)], 0.05, cloth,
                    frame=R("X", 90))
        draw_cape(b, r, pm(player, 0.82), length=0.34, width=0.30, flare=0.10)

    def base(self):
        # the scutum carried at the left, the gladius low and forward
        return anim.pose(FLx=0.05, FLy=0.03, FRx=-0.05, FRy=0.04, Rf=20, Ra=14, Re=70,
                         ikL=1, Sx=0.12, Sy=0.20, Sz=0.04, Snx=0.62, Sny=0.78, Snz=0.0,
                         **aim(0.85, -0.10, 0.50, up=0.0))

    def pose_attack(self, t):
        # the shield brought round in front, the sword drawn back at the
        # hip and stabbed out past the shield's edge on the impact frame
        st = anim.pose(self.base(), FLx=0.15, FLy=0.03, FRx=-0.12, FRy=0.05, drop=0.04,
                       Sx=0.22, Sy=0.10, Sz=0.06, Snx=0.95, Sny=0.30, Snz=0.0)
        ready = anim.pose(st, Rf=15, Ra=32, Re=100, twist=-4, **aim(1.0, -0.05, 0.05, up=0.0))
        drawn = anim.pose(st, Rf=-15, Ra=34, Re=110, twist=-14, lean=-4, hyaw=8,
                          **aim(1.0, 0.0, 0.10, up=0.0))
        thrust = anim.pose(st, Rf=80, Ra=26, Re=6, twist=16, lean=12, drop=0.07, px=0.04,
                           FLx=0.19, hyaw=-6, **aim(1.0, 0.05, -0.08, up=0.0))
        return anim.keyed([(0.0, ready), (0.4, drawn), (0.6, thrust, anim.ease_in),
                           (0.8, anim.lerp(thrust, ready, 0.6)), (1.0, ready)], t, loop=False)


# --------------------------------------------------------------- spearman

def hide_shield(b, c, n, player, rx=0.19, ry=0.29):
    """An oval hide-and-wicker shield: a hide rim, a player-colour field,
    a wooden boss."""
    F = T(V(c)) @ _frame(n, (0, 0, 1))
    hide = mat((0.66, 0.52, 0.34), rough=0.9)
    face = mat(player, rough=0.7)

    def oval(sx, sy, k=16):
        return [(sx * math.cos(2 * math.pi * i / k), sy * math.sin(2 * math.pi * i / k))
                for i in range(k)]
    with b.at(F):
        b.plate(oval(rx, ry), 0.03, hide)
        b.plate(oval(rx * 0.80, ry * 0.86), 0.012, face, frame=T(0, 0, 0.018))
        b.plate(oval(rx * 0.16, ry * 0.88), 0.008, hide, frame=T(0, 0, 0.026))   # the spine
        b.ball((0, 0, 0.03), 0.045, mat("wood_dark", rough=0.8), scale=(1, 1, 0.6), n=8)
        for i in range(12):                       # lacing round the rim
            a = 2 * math.pi * i / 12
            b.ball((rx * 0.98 * math.cos(a), ry * 0.98 * math.sin(a), 0.016), 0.014,
                   mat((0.30, 0.20, 0.12)), n=4)


class Spearman(Soldier):
    speed = 1.1
    walk_arms = (14, "R")

    def gear(self, name):
        return "spear_plain", True

    def draw_shield(self, b, r, player):
        c, n = r.shield_place()
        el, ha = r.el["L"], r.hand["L"]
        d = (ha - el).normalized()
        n = (n - d * n.dot(d)).normalized()
        mid = el + (ha - el) * 0.5
        hide_shield(b, mid + n * 0.06, n, player)

    def outfit(self, b, r, player):
        skin = mat(lib.PAL["skin_tan"], rough=0.75)
        cloth, dark = pm(player), pm(player, 0.64)
        leather = mat("leather", rough=0.8)
        draw_body(b, r, skin, foot=leather, upper=cloth)
        draw_skirt(b, r, [cloth, cloth, dark], top=0.09, hem=-0.16, zs=[0, 0.5, 0.86, 1.0],
                   flare=1.32)
        draw_torso(b, r, [cloth, cloth, cloth, cloth], 1.0)
        with b.at(r.P, True):
            b.loft([(0, 0, 0.065, .088, .12), (0, 0, 0.105, .086, .117)], leather, n=12,
                   caps=False)
        with b.at(r.U, True):            # a baldric
            b.loft([(0, 0, -0.012, .090, .150), (0, 0, 0.012, .090, .150)], leather, n=12,
                   caps=False, frame=T(0.0, 0.0, 0.17) @ R("X", -38))
        draw_head(b, r, skin, mat("hair_black", rough=0.9), "short", beard=True)
        with b.at(head_frame(r), True):           # a leather skullcap, no crest
            cap = mat((0.66, 0.48, 0.28), rough=0.75)
            b.ball((-0.01, 0, 0.03), 0.128, cap, scale=(1.02, 1.0, 0.86), n=12)
            b.loft([(-0.01, 0, -0.005, .14, .136), (-0.01, 0, 0.03, .136, .132)],
                   mat((0.40, 0.28, 0.16), rough=0.8), n=12)

    def base(self):
        return anim.pose(FLx=0.05, FLy=0.03, FRx=-0.04, FRy=0.03, Rf=12, Ra=16, Re=45, Rt=6,
                         ikL=1, Sx=0.10, Sy=0.20, Sz=0.06, Snx=0.55, Sny=0.83, Snz=0.05,
                         **aim(0.12, 0.0, 1.0, up=0.0))

    def walk_base(self):
        return anim.pose(self.base(), Tx=0.45)

    def pose_attack(self, t):
        # an underhand thrust at the waist, the shield before the body
        st = anim.pose(self.base(), FLx=0.15, FLy=0.03, FRx=-0.13, FRy=0.05, drop=0.05,
                       Sx=0.20, Sy=0.13, Sz=0.08, Snx=0.92, Sny=0.40, Snz=0.0)
        ready = anim.pose(st, Rf=20, Ra=26, Re=75, **aim(1.0, -0.05, 0.14, up=0.0))
        drawn = anim.pose(st, Rf=-18, Ra=28, Re=80, twist=-16, lean=-4, hyaw=8,
                          **aim(1.0, 0.0, 0.18, up=0.0))
        thrust = anim.pose(st, Rf=62, Ra=16, Re=4, twist=14, lean=14, drop=0.08, px=0.05,
                           FLx=0.20, hyaw=-6, **aim(1.0, -0.05, 0.02, up=0.0))
        return anim.keyed([(0.0, ready), (0.4, drawn), (0.6, thrust, anim.ease_in),
                           (0.8, anim.lerp(thrust, ready, 0.6)), (1.0, ready)], t, loop=False)


# ----------------------------------------------------------------- bowman

def big_bow(b, grip, aim_d, nock=None, h=0.42, arrow=True):
    """figures._bow drawn thick enough to read at game size: a recurved
    self bow, dark wood with pale tips, its string and (nocked) arrow."""
    aim_d = V(aim_d).normalized()
    grip = V(grip)
    up = V((0, 0, 1)) - aim_d * aim_d.z
    up.normalize()
    c = 0.20 if nock is not None else 0.10
    pts = []
    for i in range(9):
        t = -1 + i / 4
        off = -c * t * t + 0.5 * max(0.0, abs(t) - 0.75) ** 2 * 4
        pts.append(grip + up * (t * h) + aim_d * off)
    wood = mat((0.30, 0.18, 0.10), rough=0.6)
    b.chain(pts[:5], 0.016, 0.027, wood, n=6)
    b.chain(pts[4:], 0.027, 0.016, wood, n=6)
    tip = mat((0.90, 0.84, 0.68), rough=0.6)
    b.ball(pts[0], 0.02, tip, n=6)
    b.ball(pts[-1], 0.02, tip, n=6)
    b.ball(grip, 0.034, mat("leather", rough=0.8), scale=(1, 1, 1.6), n=6)
    sm = mat((0.95, 0.92, 0.82))
    if nock is not None:
        nock = V(nock)
        b.seg(pts[0], nock, 0.0065, 0.0065, sm, n=4)
        b.seg(pts[-1], nock, 0.0065, 0.0065, sm, n=4)
        if arrow:
            d = (grip + aim_d * 0.02) - nock
            L = d.length + 0.08
            d.normalize()
            b.seg(nock, nock + d * L, 0.011, 0.011, "straw", n=5)
            b.seg(nock + d * L, nock + d * (L + 0.07), 0.024, 0.002, "iron", n=5)
            b.seg(nock, nock + d * 0.09, 0.028, 0.02, "white", n=5, sy=0.3)
    else:
        b.seg(pts[0], pts[-1], 0.0065, 0.0065, sm, n=4)


class Bowman(Soldier):
    speed = 1.0
    walk_arms = (24, "R")

    def gear(self, name):
        return None, False

    def outfit(self, b, r, player):
        skin = mat(lib.PAL["skin_tan"], rough=0.75)
        cloth, mid, dark = pm(player), pm(player, 0.82), pm(player, 0.64)
        leather = mat("leather", rough=0.8)
        lin = mat((0.93, 0.86, 0.70), rough=0.9)
        draw_body(b, r, skin, foot=leather, upper=lin)
        draw_skirt(b, r, [cloth, cloth, dark], top=0.09, hem=-0.15, zs=[0, 0.5, 0.86, 1.0],
                   flare=1.3)
        draw_torso(b, r, [cloth, cloth, cloth, cloth], 1.0)
        with b.at(r.P, True):
            b.loft([(0, 0, 0.065, .088, .12), (0, 0, 0.105, .086, .117)], leather, n=12,
                   caps=False)
        with b.at(r.U, True):            # the quiver's strap and the quiver on the back
            b.loft([(0, 0, -0.012, .092, .150), (0, 0, 0.012, .092, .150)], leather, n=12,
                   caps=False, frame=T(0.0, 0.0, 0.17) @ R("X", 40))
            _quiver(b, (-0.13, -0.10, 0.42), (-0.12, 0.09, 0.0), leather, band=dark)
        draw_head(b, r, skin, mat("hair_brown", rough=0.9), "short", beard=False)
        with b.at(head_frame(r), True):           # a Phrygian cap, its tip forward
            b.loft([(-0.01, 0, 0.0, .133, .129), (-0.015, 0, 0.06, .13, .126),
                    (-0.01, 0, 0.13, .09, .088), (0.03, 0, 0.19, .05, .05)], mid, n=12)
            b.ball((0.07, 0, 0.19), 0.045, mid, n=8)
            b.loft([(-0.13, 0, -0.02, .03, .10), (-0.15, 0, -0.12, .03, .11)], mid, n=8)

    def load(self, b, r, name, player):
        p = r.p
        # the bow in the left hand; its aim the arm's line when raised, else
        # hanging forward
        ha = r.hand["L"]
        if p.get("bow_up", 0) > 0.5:
            a = (ha - r.sh["L"])
            a.z *= 0.4
        else:
            # hanging at the left side, turned half outward so its curve
            # shows from the front and the side
            a = r.U.to_3x3() @ V((0.6, 0.8, 0))
            a.z = 0
        a.normalize()
        nock = r.hand["R"] if p.get("nock", 0) > 0.5 else None
        big_bow(b, ha, a, nock, arrow=p.get("arrow", 0) > 0.5)

    def base(self):
        return anim.pose(FLx=0.05, FLy=0.03, FRx=-0.04, FRy=0.03, Lf=12, La=12, Le=24,
                         Rf=6, Ra=10, Re=16)

    def pose_attack(self, t):
        # nock (0), draw (1, 2), loose on the impact (3): the arrow is gone
        # and the hand flies back; lower, reach for the next (4, 5)
        st = anim.pose(self.base(), FLx=0.12, FLy=0.06, FRx=-0.10, FRy=0.0, twist=22, hyaw=-18,
                       ikL=1, ikR=1, Lx=0.40, Ly=0.10, Lz=0.30)
        nock = anim.pose(st, Rx=0.34, Ry=0.02, Rz=0.28)
        half = anim.pose(st, Rx=0.22, Ry=-0.04, Rz=0.31)
        full = anim.pose(st, Rx=0.06, Ry=-0.10, Rz=0.34, lean=-3)
        loose = anim.pose(st, Rx=-0.04, Ry=-0.18, Rz=0.36, lean=-4)
        low = anim.pose(st, Lx=0.32, Ly=0.14, Lz=0.16, Rx=0.0, Ry=-0.16, Rz=0.30, twist=12)
        reach = anim.pose(st, Lx=0.30, Ly=0.14, Lz=0.14, Rx=-0.12, Ry=-0.08, Rz=0.40,
                          twist=8)
        i = round(t * 5)
        p = dict([nock, half, full, loose, low, reach][min(5, max(0, i))])
        p["bow_up"] = 1.0
        p["nock"] = 1.0 if i <= 2 else 0.0
        p["arrow"] = 1.0 if i <= 2 else 0.0
        return p


# ---------------------------------------------------------------- slinger

class Slinger(Soldier):
    speed = 1.0
    walk_arms = (26, "LR")

    def gear(self, name):
        return None, False

    def outfit(self, b, r, player):
        skin = mat(lib.PAL["skin_tan"], rough=0.75)
        cloth, mid, dark = pm(player), pm(player, 0.82), pm(player, 0.64)
        leather = mat("leather", rough=0.8)
        draw_body(b, r, skin, foot=leather)
        draw_skirt(b, r, [cloth, cloth, dark], top=0.09, hem=-0.14, zs=[0, 0.5, 0.86, 1.0],
                   flare=1.3)
        # a tunic off the right shoulder (bare right arm and shoulder)
        draw_torso(b, r, [cloth, cloth, cloth, cloth], 1.0)
        with b.at(r.P, True):
            b.loft([(0, 0, 0.065, .088, .12), (0, 0, 0.105, .086, .117)], leather, n=12,
                   caps=False)
        with b.at(r.U, True):            # the stone bag's strap, the bag on the left hip
            b.loft([(0, 0, -0.012, .092, .150), (0, 0, 0.012, .092, .150)], leather, n=12,
                   caps=False, frame=T(0.0, 0.0, 0.17) @ R("X", -40))
        with b.at(r.P, True):
            b.ball((0.02, 0.16, 0.02), 0.09, leather, scale=(1.0, 0.8, 1.1), n=10)
            b.ball((0.02, 0.15, 0.09), 0.05, mat((0.32, 0.22, 0.14)), scale=(1, 0.8, 0.5),
                   n=8)
        draw_head(b, r, skin, mat("hair_black", rough=0.9), "long", beard=True)
        with b.at(head_frame(r), True):           # a wide-brimmed straw hat
            straw = mat((0.86, 0.72, 0.42), rough=0.85)
            b.loft([(-0.01, 0, 0.03, .22, .22), (-0.01, 0, 0.055, .21, .21)], straw, n=16)
            b.loft([(-0.01, 0, 0.05, .14, .14), (-0.01, 0, 0.13, .11, .11),
                    (-0.01, 0, 0.16, .05, .05)], straw, n=12)
            b.loft([(-0.01, 0, 0.055, .142, .142), (-0.01, 0, 0.095, .132, .132)], cloth,
                   n=12, caps=False)

    def load(self, b, r, name, player):
        """The sling from the right hand: two cords to a pouch. Whirling
        (the pose's `whirl` angle) it circles above the head; let go it
        hangs; released it flies out empty."""
        p = r.p
        h = r.hand["R"]
        U3 = r.U.to_3x3()
        cord = mat((0.75, 0.62, 0.42), rough=0.9)
        L = 0.30
        if p.get("whirl", -1) >= 0:
            a = math.radians(p["whirl"])
            pouch = h + U3 @ V((L * math.cos(a), L * math.sin(a), 0.06))
        elif p.get("flung", 0) > 0.5:
            pouch = h + U3 @ V((0.27, -0.02, 0.12))
        else:
            pouch = h + V((0.03, 0, -L))
        d = (pouch - h).normalized()
        side = d.cross(V((0, 0, 1)))
        side = side.normalized() if side.length > 1e-4 else V((0, 1, 0))
        for s in (1, -1):
            b.seg(h, pouch + side * s * 0.03, 0.010, 0.010, cord, n=4)
        b.ball(pouch, 0.042, mat("leather", rough=0.8), scale=(1, 1, 0.7), n=8)
        if p.get("stone", 1) > 0.5:
            b.ball(pouch + V((0, 0, 0.014)), 0.032, mat((0.72, 0.70, 0.66), rough=0.9), n=6)

    def base(self):
        return anim.pose(FLx=0.05, FLy=0.03, FRx=-0.04, FRy=0.03, Lf=6, La=12, Le=16,
                         Rf=14, Ra=16, Re=30)

    def pose_attack(self, t):
        # whirl overhead (0 to 2), release forward on the impact (3), the
        # cords flying out empty; recover (4, 5)
        st = anim.pose(self.base(), FLx=0.13, FLy=0.05, FRx=-0.11, FRy=0.04, drop=0.02,
                       Lf=40, La=30, Le=20)
        up = anim.pose(st, Rf=165, Ra=28, Re=20, lean=-4, hpitch=-6, twist=-10)
        rel = anim.pose(st, Rf=105, Ra=12, Re=0, lean=14, twist=14, drop=0.05, hpitch=4,
                        Lf=10, La=20)
        rec = anim.pose(st, Rf=50, Ra=14, Re=20, lean=6, twist=6)
        i = round(t * 5)
        p = dict([up, up, up, rel, anim.lerp(rel, rec, 0.5), rec][min(5, max(0, i))])
        p["whirl"] = (-60 + 130 * i) if i <= 2 else -1
        p["flung"] = 1.0 if i == 3 else 0.0
        p["stone"] = 1.0 if i <= 2 or i == 5 else 0.0
        return p


# ----------------------------------------------------------------- priest

class Priest(Soldier):
    speed = 0.8
    walk_arms = (16, "L")

    def gear(self, name):
        return "staff", False

    def outfit(self, b, r, player):
        skin = mat(lib.PAL["skin_light"], rough=0.75)
        cloth, mid = pm(player), pm(player, 0.82)
        robe = mat((0.95, 0.93, 0.86), rough=0.85)
        robe_d = mat((0.82, 0.79, 0.70), rough=0.85)
        draw_body(b, r, skin, foot=mat("leather", rough=0.8), upper=robe, fore=robe)
        # the robe to the ankles, its hem following the feet
        hem = -(r.bd.stand - 0.05)
        draw_skirt(b, r, [robe, robe, robe, robe_d], top=0.10, hem=hem,
                   zs=[0, 0.35, 0.7, 0.92, 1.0], flare=1.55, w=(.094, .124))
        draw_torso(b, r, robe, 1.02)
        with b.at(r.U, True):
            # the stole: two broad player bands down the front
            for s in (1, -1):
                b.seg((0.07, s * 0.06, r.bd.torso - 0.01), (0.105, s * 0.055, -0.08), 0.035,
                      0.04, cloth, n=6, sy=0.35, hint=(0, 0, 1))
            b.loft([(0, 0, r.bd.torso - 0.05, .095, .16), (0, 0, r.bd.torso + 0.0, .07, .13)],
                   cloth, n=12)                    # the mantle's collar
            b.loft([(0, 0, 0.07, .092, .126), (0, 0, 0.11, .09, .123)], gold(), n=12,
                   caps=False)                     # a gold girdle
        draw_cape(b, r, mid, length=0.62, width=0.32, flare=0.18, fly=0.1)
        draw_head(b, r, skin, mat((0.92, 0.92, 0.90), rough=0.9), "none", beard=True)
        with b.at(head_frame(r), True):           # a tall linen cap with a gold band
            b.loft([(-0.01, 0, 0.0, .128, .124), (-0.01, 0, 0.10, .12, .116),
                    (-0.015, 0, 0.20, .10, .096), (-0.02, 0, 0.25, .06, .058)], robe, n=12)
            b.loft([(-0.01, 0, 0.0, .134, .13), (-0.01, 0, 0.035, .132, .128)], gold(), n=12)

    def base(self):
        return anim.pose(FLx=0.04, FLy=0.02, FRx=-0.03, FRy=0.02, Rf=16, Ra=16, Re=50,
                         Lf=10, La=10, Le=30, **aim(0.10, -0.05, 1.0, up=0.0))

    def pose_attack(self, t):
        # the chant: the staff and the left hand raised (0 to 2), held up
        # with a slow sway (3 to 5); the last frame is held while it lasts
        st = anim.pose(self.base(), FLx=0.07, FRx=-0.05)
        lift = anim.pose(st, Rf=70, Ra=22, Re=60, Lf=50, La=20, Le=50,
                         **aim(0.20, -0.10, 0.97, up=0.0))
        up = anim.pose(st, Rf=150, Ra=22, Re=15, Lf=145, La=32, Le=20, lean=-6, hpitch=-14,
                       **aim(0.15, -0.10, 0.98, up=0.0))
        sway = anim.pose(up, side=3, Lf=150, La=38, hpitch=-16, Rf=155)
        return anim.keyed([(0.0, st), (0.2, lift), (0.4, up), (0.6, sway), (0.8, up),
                           (1.0, sway)], t, loop=False)


clubman = _unit(Clubman)
axeman = _unit(Axeman)
swordsman = _unit(Swordsman)
legionary = _unit(Legionary)
spearman = _unit(Spearman)
bowman = _unit(Bowman)
slinger = _unit(Slinger)
priest = _unit(Priest)

# The foot sets (docs/look/units.md, "SETS"): one look per kind at every age.
SETS = {name: dict(func="units_foot." + name, cls="Foot", scale=1)
        for name in ("clubman", "axeman", "swordsman", "legionary", "spearman", "bowman",
                     "slinger", "priest")}


def figure(name):
    return globals()[name]()


if __name__ == "__main__":
    import units
    units.figure = figure
    units._preview(sys.argv[1], sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else None)

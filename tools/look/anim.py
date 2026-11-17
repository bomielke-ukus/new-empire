"""Shared motion for the new look's units (docs/look/units.md): poses as
flat dicts of joint values, keyframed animations with easing, and a walk
cycle whose feet do not slide. Pure Python, no Blender: units.py turns a
pose into joint matrices and draws the figure on them.

A pose is a dict; missing keys take NEUTRAL's value. Angles in degrees,
lengths in tiles (a person is about 1.0 tall). The figure's frame: X
forward, Y to its left, Z up, origin on the ground between its feet.

    root     x y z       the whole figure moved (a stagger, a sink)
             fall        pitch of the whole figure about a pivot on the
                         ground at (fallx, 0, fallz); negative falls backward
             roll        roll of the whole figure about the X axis (to its left +)
    pelvis   drop        how far the hips are lowered from standing (knees bend)
             px py       the hips moved forward / to the left over the feet
             pyaw proll  the hips turned (about Z) and tilted (about X)
    torso    lean side twist   forward pitch, roll to the left, turn to the left
    head     hyaw hpitch       turn to the left, nod down
    arms     Lf La Lt Le Lw    left: flex forward, abduct out, twist in, elbow, wrist
             Rf Ra Rt Re Rw    right (the wrist sets a held tool's angle)
             ikL ikR           0..1: how far the hand goes to its IK target instead
             Lx Ly Lz Rx Ry Rz the IK targets, in the torso's frame
             grip              with ikL > 0 and grip > 0: the left hand's target is
                               on the right hand's tool, this far along it
    legs     FLx FLy FLz FLp   left foot: forward, outward, sole height, toe pitch up
             FRx FRy FRz FRp   right foot (feet are IK targets: the legs bend to them)
    tool     aimw Tx Ty Tz     0..1: how far the right hand's tool points along (T*)
                               in the torso's frame instead of following the wrist
             aimup Nx Ny Nz    with aimw: 0..1, how far the tool's striking side
                               (its Z) turns to the way a swing about the axis N
                               (torso frame; default its left, an overhead swing
                               forward) moves it, instead of the wrist's up
    shield   Sx Sy Sz          its centre, in the torso's frame (the left hand holds it)
             Snx Sny Snz       the way its face looks
"""
import math

NEUTRAL = dict(
    x=0.0, y=0.0, z=0.0, fall=0.0, fallx=0.0, fallz=0.0, roll=0.0,
    drop=0.0, px=0.0, py=0.0, pyaw=0.0, proll=0.0,
    lean=0.0, side=0.0, twist=0.0, hyaw=0.0, hpitch=0.0,
    Lf=4.0, La=8.0, Lt=0.0, Le=12.0, Lw=0.0, Rf=4.0, Ra=8.0, Rt=0.0, Re=12.0, Rw=0.0,
    ikL=0.0, ikR=0.0, Lx=0.0, Ly=0.0, Lz=0.0, Rx=0.0, Ry=0.0, Rz=0.0, grip=0.0,
    FLx=0.0, FLy=0.0, FLz=0.0, FLp=0.0, FRx=0.0, FRy=0.0, FRz=0.0, FRp=0.0,
    aimw=0.0, Tx=1.0, Ty=0.0, Tz=0.0, aimup=0.0, Nx=0.0, Ny=1.0, Nz=0.0,
    Sx=0.17, Sy=0.13, Sz=0.12, Snx=1.0, Sny=0.0, Snz=0.0,
)

# Timing of the foot classes' animations, as the old sets have them
# (assets/sprites/villager/villager.ron): frames, ms per frame, loops, impact.
FOOT = {
    "idle": (4, 160, True, None),
    "walk": (8, 100, True, None),
    "attack": (6, 90, False, 3),
    "death": (8, 110, False, None),
    "decay": (4, 7500, False, None),
}
VILLAGER_WORK = {
    "build": (6, 140, True, None),
    "carry_food": (8, 100, True, None),
    "carry_gold": (8, 100, True, None),
    "carry_stone": (8, 100, True, None),
    "carry_wood": (8, 100, True, None),
    "chop": (6, 140, True, None),
    "farm": (6, 140, True, None),
    "forage": (6, 140, True, None),
    "mine": (6, 140, True, None),
}


def pose(*parts, **kw):
    """NEUTRAL with the parts laid over it in order."""
    p = dict(NEUTRAL)
    for d in parts:
        p.update(d)
    p.update(kw)
    return p


# ----------------------------------------------------------------- easing

def smooth(u):
    """Ease in and out (smoothstep)."""
    u = min(1.0, max(0.0, u))
    return u * u * (3 - 2 * u)


def ease_out(u):
    u = min(1.0, max(0.0, u))
    return 1 - (1 - u) ** 2


def ease_in(u):
    u = min(1.0, max(0.0, u))
    return u * u


def lerp(a, b, u):
    return {k: a[k] + (b[k] - a[k]) * u for k in a}


def keyed(keys, t, loop=True):
    """The pose at `t` (0..1) between keyframes [(t, pose, ease), ...] in
    order of t; `ease` is how the motion INTO that key goes (smooth by
    default). A looping animation wraps from the last key to the first."""
    keys = [k if len(k) == 3 else (k[0], k[1], smooth) for k in keys]
    if loop:
        keys = keys + [(keys[0][0] + 1.0, keys[0][1], keys[0][2])]
        t = t % 1.0
        if t < keys[0][0]:
            t += 1.0
    if t <= keys[0][0]:
        return dict(keys[0][1])
    for (t0, a, _), (t1, b, e) in zip(keys, keys[1:]):
        if t <= t1:
            return lerp(a, b, e((t - t0) / max(1e-9, t1 - t0)))
    return dict(keys[-1][1])


def frame_t(i, n, loop):
    """Where frame i of n falls in its animation, 0..1."""
    return i / n if loop else i / max(1, n - 1)


# ------------------------------------------------------------------- walk

def walk(t, speed, period, leg, stance=0.58, lift=0.12, width=0.035, base=None):
    """The walk cycle at `t` (0..1): the feet planted on the ground while
    they carry the figure (moving back under it at `speed` tiles a second,
    so they do not slide when the unit moves at that speed), lifted and
    swung forward between; the left foot strikes the ground at t = 0, the
    right at 0.5 (docs/05 2.2: "foot-plant on frames 1 and 5"); the hips as
    low as the planted legs need (lowest with both feet down), turning with
    the stride; the arms swinging against the legs. `leg` = (thigh, shin,
    ankle height); `base` is the pose to lay it over."""
    p = dict(base or NEUTRAL)
    reach = (leg[0] + leg[1]) * 0.975
    half = speed * period * stance / 2         # the foot's travel under the hip, each way
    lo = None
    for side, ph in (("L", t), ("R", t + 0.5)):
        ph %= 1.0
        if ph < stance:
            x = half - 2 * half * ph / stance
            z, pitch = 0.0, 0.0
            # toe-off: the heel rises at the end of the stance
            if ph > stance - 0.12:
                pitch = -18 * smooth((ph - stance + 0.12) / 0.12)
            need = math.sqrt(max(0.0, reach * reach - (x - p["px"]) ** 2)) + leg[2]
            lo = need if lo is None else min(lo, need)
        else:
            u = (ph - stance) / (1 - stance)
            x = -half + 2 * half * smooth(u)
            z = lift * math.sin(math.pi * u) ** 1.3
            pitch = -18 * (1 - smooth(u * 2.5)) + 14 * smooth((u - 0.5) * 2) - 0 * u
        p["F%sx" % side] = x
        p["F%sz" % side] = z
        p["F%sp" % side] = pitch
        p["F%sy" % side] = width
    stand = leg[0] + leg[1] + leg[2] - 0.012
    p["drop"] = max(p["drop"], stand - lo + 0.008)
    c = math.cos(2 * math.pi * t)
    p["pyaw"] += -7 * c                    # the left hip forward at t = 0
    p["twist"] += 5 * c                    # the shoulders against it
    # over the planted foot: a clear side to side sway, which is what shows
    # of a walk seen from the front (S)
    p["py"] += 0.02 * math.cos(2 * math.pi * (t - 0.12))
    p["proll"] += 4 * math.cos(2 * math.pi * (t - 0.12))
    p["side"] -= 2.5 * math.cos(2 * math.pi * (t - 0.12))
    p["lean"] += 4
    p["hyaw"] += -4 * c
    return p


def swing_arms(p, t, amount=28, which="LR"):
    """Arms swinging against the legs: the right forward as the left foot
    strikes (t = 0)."""
    c = math.cos(2 * math.pi * t)
    for s, sign in (("R", 1), ("L", -1)):
        if s in which:
            p[s + "f"] += amount * c * sign
            p[s + "e"] += 10 + 12 * max(0.0, c * sign)
    return p


# --------------------------------------------------------- shared actions

def breathe(base, t, amount=1.0):
    """The idle: breathing and a slow weight shift, small enough not to
    shimmer at game size."""
    s = math.sin(2 * math.pi * t)
    c = math.cos(2 * math.pi * t)
    p = dict(base)
    p["drop"] += 0.006 * amount * (1 + s) / 2
    p["py"] += 0.010 * amount * c
    p["proll"] += 1.5 * amount * c
    p["side"] += -1.2 * amount * c
    p["lean"] += 1.0 * amount * s
    p["hyaw"] += 4 * amount * c
    p["Le"] += 4 * amount * s
    p["Re"] += 4 * amount * s
    return p


def death(base, t):
    """Hit, stagger back, the knees go, and the body falls back flat; the
    last frame lies still (the decay's first frame matches it)."""
    lying = fallen(base)
    stagger = pose(base, x=-0.05, lean=-14, hpitch=-18, drop=0.02,
                   Lf=40, La=30, Le=40, Rf=35, Ra=30, Re=30, ikL=0.0, ikR=0.0,
                   FLx=0.06, FRx=-0.12)
    buckle = pose(base, x=-0.02, drop=0.16, lean=-20, hpitch=-10, side=8,
                  Lf=20, La=45, Le=30, Rf=10, Ra=50, Re=25, ikL=0.0, ikR=0.0,
                  FLx=0.10, FRx=-0.05, fall=-18, fallx=-0.08)
    sit = pose(lying, x=FALL_X * 0.5, fall=-62, drop=0.20, lean=-10, Lf=60, Rf=60, La=50, Ra=50,
               hpitch=-20)
    return keyed([(0.0, base), (0.15, stagger, ease_out), (0.42, buckle),
                  (0.70, sit, ease_in), (0.86, lying, ease_in), (1.0, lying)], t, loop=False)


# How far forward the falling body is carried so that it lies roughly where
# the unit stood (its middle over the ground point), not a body's length
# behind it.
FALL_X = 0.42


def fallen(base):
    """Lying on its back, arms flung out, one knee up, its middle about over
    the ground point."""
    return pose(base, x=FALL_X, fall=-90, fallx=-0.05, fallz=0.075, drop=0.04, lean=0, side=0, twist=0,
                hyaw=25, hpitch=-6, Lf=20, La=70, Le=30, Rf=10, Ra=55, Re=15, ikL=0.0, ikR=0.0,
                FLx=0.10, FLz=0.02, FRx=-0.04, FRz=0.0, FRp=20, FLp=10, px=0.0, py=0.0)


def decay(base, t, n=4):
    """The body lying, sinking into the ground frame by frame (the exporter
    also fades it): the old art's corpse sinks."""
    p = fallen(base)
    p["z"] = -0.05 * t * (n - 1) / max(1, n - 1)
    return p

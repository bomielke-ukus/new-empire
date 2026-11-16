"""Effects drawn at a building's attachment points (docs/look/buildings.md,
"Smoke, flags and fire"): small looping colour sets, each an animated
`idle`, exported by export_all.py like the buildings.

    fx_smoke        soft grey-white puffs rising from a hearth, thinning
    fx_smoke_dark   the same, sooty: a forge, a workshop
    fx_fire_small   a few flames and their glow: a building hurt
    fx_fire_large   a blaze, its glow and its smoke: a building badly hurt
    fx_flag         a flag waving from the top of a pole (the cloth only,
                    in the player's colour; for models built without their
                    own cloth, none yet)

Each builder takes `frame` (0 to FRAMES - 1) and draws that moment of a
loop whose last frame runs on into its first. The set's anchor (the world's
origin) is the attachment point: the hearth's mouth, the place the fire
sits on, the pole's top. Nothing stands on the ground (`no_ground`): no
shadow, and the flag hangs below its origin.

Smoke and fire are billboards: discs in the camera's plane with a soft
edge, a little noise and their own unlit colour (Principled, black base,
emission), so the sheet's alpha is soft, straight and as the disc's
coverage. A smoke puff is shaded as a ball lit from the sun's side; a
flame is a stack of discs, white-yellow at the core, orange, red at the rim.
"""
import math
import random

import bpy  # noqa: F401  (before bmesh)
from mathutils import Matrix, Vector

import lib
from lib import BLUE, mat

# The export camera's axes in the world (export.py's RIGHT, UP, TOWARD_CAMERA).
_EL = math.radians(30)
RIGHT = Vector((1.0, 1.0, 0.0)) / math.sqrt(2)
UP = Vector((-math.sin(_EL) / math.sqrt(2), math.sin(_EL) / math.sqrt(2), math.cos(_EL)))
TOWARD = Vector((math.cos(_EL) / math.sqrt(2), -math.cos(_EL) / math.sqrt(2), math.sin(_EL)))
# Rotation taking a disc in local XY (facing local +Z) into the camera's plane.
FACE_CAMERA = Matrix((RIGHT, UP, TOWARD)).transposed().to_4x4()

SMOKE_FRAMES, SMOKE_MS = 16, 125        # a 2 s loop
FIRE_FRAMES, FIRE_MS = 8, 100           # 0.8 s
FLAG_FRAMES, FLAG_MS = 8, 125           # 1 s


def _disc_material(key, inner, outer, opacity, shade, noise, seed, soft=0.55):
    """A soft disc's material: radius-1 disc in object XY. Colour from
    `inner` at the centre to `outer` at the rim (sRGB), shaded as a lit
    ball with `shade`; alpha `opacity` falling to 0 at the rim over the
    outer `soft` of the radius, broken by noise of strength `noise`."""
    k = ("fx_disc", key, tuple(inner), tuple(outer), round(opacity, 4), shade, noise, seed, soft)
    if k in lib._mats:
        return lib._mats[k]
    m = bpy.data.materials.new("fx_%s" % key)
    m.use_nodes = True
    nt = m.node_tree
    N, L = nt.nodes, nt.links
    b = N["Principled BSDF"]
    b.inputs["Base Color"].default_value = (0, 0, 0, 1)
    b.inputs["Roughness"].default_value = 1.0
    b.inputs["Specular IOR Level"].default_value = 0.0
    tc = N.new("ShaderNodeTexCoord")
    # r^2 and the rim falloff
    r2 = N.new("ShaderNodeVectorMath")
    r2.operation = "DOT_PRODUCT"
    L.new(tc.outputs["Object"], r2.inputs[0])
    L.new(tc.outputs["Object"], r2.inputs[1])
    r = N.new("ShaderNodeMath")
    r.operation = "SQRT"
    L.new(r2.outputs["Value"], r.inputs[0])
    edge = N.new("ShaderNodeMapRange")
    edge.interpolation_type = "SMOOTHSTEP"
    edge.inputs["From Min"].default_value = 1.0
    edge.inputs["From Max"].default_value = 1.0 - soft
    L.new(r.outputs["Value"], edge.inputs["Value"])
    # noise breaks the edge and the body
    nz = N.new("ShaderNodeTexNoise")
    nz.noise_dimensions = "4D"
    nz.inputs["W"].default_value = seed * 1.37
    nz.inputs["Scale"].default_value = 1.1
    nz.inputs["Detail"].default_value = 3.0
    L.new(tc.outputs["Object"], nz.inputs["Vector"])
    nmix = N.new("ShaderNodeMapRange")
    nmix.inputs["From Min"].default_value = 0.3
    nmix.inputs["From Max"].default_value = 0.7
    nmix.inputs["To Min"].default_value = 1.0 - noise
    nmix.inputs["To Max"].default_value = 1.0
    L.new(nz.outputs["Fac"], nmix.inputs["Value"])
    a = N.new("ShaderNodeMath")
    a.operation = "MULTIPLY"
    L.new(edge.outputs["Result"], a.inputs[0])
    L.new(nmix.outputs["Result"], a.inputs[1])
    a2 = N.new("ShaderNodeMath")
    a2.operation = "MULTIPLY"
    a2.use_clamp = True
    L.new(a.outputs["Value"], a2.inputs[0])
    a2.inputs[1].default_value = opacity
    L.new(a2.outputs["Value"], b.inputs["Alpha"])
    # colour: a ball lit from the upper left (shade), or radial (a flame)
    if shade:
        lit = N.new("ShaderNodeVectorMath")
        lit.operation = "DOT_PRODUCT"
        L.new(tc.outputs["Object"], lit.inputs[0])
        lit.inputs[1].default_value = (-0.55, 0.55, 0.0)
        cap = N.new("ShaderNodeMath")
        cap.operation = "SUBTRACT"
        cap.inputs[0].default_value = 1.0
        L.new(r2.outputs["Value"], cap.inputs[1])
        cap2 = N.new("ShaderNodeMath")
        cap2.operation = "POWER"
        cap2.use_clamp = True
        L.new(cap.outputs["Value"], cap2.inputs[0])
        cap2.inputs[1].default_value = 0.5
        capz = N.new("ShaderNodeMath")
        capz.operation = "MULTIPLY"
        L.new(cap2.outputs["Value"], capz.inputs[0])
        capz.inputs[1].default_value = 0.62
        s = N.new("ShaderNodeMath")
        s.operation = "ADD"
        L.new(lit.outputs["Value"], s.inputs[0])
        L.new(capz.outputs["Value"], s.inputs[1])
        fac = N.new("ShaderNodeMapRange")
        fac.inputs["From Min"].default_value = -0.4
        fac.inputs["From Max"].default_value = 1.0
        L.new(s.outputs["Value"], fac.inputs["Value"])
    else:
        fac = N.new("ShaderNodeMapRange")
        fac.inputs["From Min"].default_value = 1.0
        fac.inputs["From Max"].default_value = 0.0
        L.new(r.outputs["Value"], fac.inputs["Value"])
    ramp = N.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].color = (*lib._lin(outer), 1)
    ramp.color_ramp.elements[1].color = (*lib._lin(inner), 1)
    L.new(fac.outputs["Result"], ramp.inputs["Fac"])
    L.new(ramp.outputs["Color"], b.inputs["Emission Color"])
    b.inputs["Emission Strength"].default_value = 1.0
    lib._mats[k] = m
    return m


def disc(centre, radius, m, parent, aspect=1.0, toward=0.0, name="fx_disc"):
    """A disc of `radius` world units (x `aspect` tall) facing the camera,
    centred on world point `centre`, moved `toward` the camera (to lie in
    front of the discs with less)."""
    n = 16
    vs = [(0.0, 0.0, 0.0)] + [(math.cos(2 * math.pi * i / n), math.sin(2 * math.pi * i / n), 0.0)
                              for i in range(n)]
    fs = [(0, 1 + i, 1 + (i + 1) % n) for i in range(n)]
    ob = lib.mesh(vs, fs, m, parent=parent, name=name)
    ob.matrix_world = (Matrix.Translation(Vector(centre) + TOWARD * toward) @ FACE_CAMERA
                       @ Matrix.Diagonal((radius, radius * aspect, 1.0, 1.0)))
    ob.visible_shadow = False
    return ob


def _no_ground(f):
    f.no_ground = True
    return f


# ------------------------------------------------------------------ smoke

def _smoke(frame, parent, inner, outer, opacity, puffs=11, rise=2.0, r0=0.12, r1=0.62, drift=0.4,
           seed=1, at=(0, 0, 0)):
    """Puffs born at the origin every 1/puffs of the loop, rising `rise`,
    swelling from r0 to r1, drifting with the wind toward screen-right,
    thinning out. Puff k is the same puff (wobble, noise) every loop."""
    t0 = frame / SMOKE_FRAMES
    for k in range(puffs):
        rnd = random.Random(seed * 100 + k)
        t = (t0 + k / puffs) % 1.0
        wob = rnd.uniform(-0.06, 0.06)
        z = rise * t ** 0.85
        side = drift * t ** 1.6 + wob * math.sin(t * 6.0 + k) + 0.04 * math.sin(t * 9 + k * 2)
        pos = Vector(at) + Vector((0, 0, z)) + RIGHT * side
        rad = r0 + (r1 - r0) * t ** 0.8
        a = opacity * min(1.0, t / 0.1) * (1.0 - t) ** 1.1
        if a < 0.01:
            continue
        m = _disc_material("smoke", inner, outer, a, True, 0.3, k + seed * 10)
        disc(pos, rad, m, parent, rnd.uniform(0.85, 1.05), toward=-t * 0.5, name="fx_puff")


@_no_ground
def smoke(frame=0):
    """Hearth smoke: pale grey-white puffs."""
    r = lib.empty("fx_smoke")
    _smoke(frame, r, (0.92, 0.91, 0.89), (0.52, 0.53, 0.56), 0.85)
    return r


@_no_ground
def smoke_dark(frame=0):
    """Forge smoke: sooty, a little denser."""
    r = lib.empty("fx_smoke_dark")
    _smoke(frame, r, (0.46, 0.44, 0.42), (0.15, 0.14, 0.14), 0.88, seed=2)
    return r


# ------------------------------------------------------------------- fire

def _flames(frame, parent, at, width, height, tongues, seed, toward=0.0):
    """A cluster of flame tongues standing on `at`: each born at the base
    every 1/tongues of the loop, rising `height` and narrowing as it goes;
    a tongue is three discs stacked into a teardrop."""
    t0 = frame / FIRE_FRAMES
    base = Vector(at)
    for k in range(tongues):
        rnd = random.Random(seed * 100 + k)
        t = (t0 + k / tongues) % 1.0
        x = rnd.uniform(-0.5, 0.5) * width * (1 - 0.6 * t)
        z = height * (0.05 + 0.75 * t)
        w = width * 0.32 * (1.0 - 0.75 * t) * rnd.uniform(0.85, 1.15)
        a = min(1.0, 1.3 * (1 - t) ** 0.5)
        cols = (((1.0, 0.86, 0.32), (0.96, 0.36, 0.03)), ((1.0, 0.62, 0.12), (0.86, 0.22, 0.02)),
                ((0.96, 0.42, 0.06), (0.70, 0.12, 0.02)))
        for j, (dz, f) in enumerate(((0.0, 1.0), (1.0, 0.7), (1.8, 0.42))):
            c = base + RIGHT * x + Vector((0, 0, z + dz * w * 1.6))
            m = _disc_material("flame%d" % j, cols[j][0], cols[j][1], a * (1.0 - 0.1 * j),
                               False, 0.2, seed * 10 + k, soft=0.45)
            disc(c, w * f, m, parent, 1.6, toward=toward + 0.02 * (2 - j) - 0.05 * t, name="fx_flame")
    # the bed of the fire: a steady hot core
    m = _disc_material("core", (1.0, 0.93, 0.6), (1.0, 0.55, 0.1), 1.0, False, 0.2, seed, soft=0.6)
    disc(base + Vector((0, 0, height * 0.12)), width * 0.42, m, parent, 0.6, toward=toward + 0.1,
         name="fx_core")


def _glow(frame, parent, at, radius, seed):
    """The warm light round a fire: a big faint disc behind it, breathing."""
    breathe = 0.85 + 0.15 * math.sin(2 * math.pi * frame / FIRE_FRAMES + seed)
    m = _disc_material("glow", (1.0, 0.62, 0.22), (1.0, 0.38, 0.10), 0.30 * breathe, False, 0.15,
                       seed, soft=0.95)
    disc(Vector(at) + Vector((0, 0, radius * 0.35)), radius, m, parent, 0.8, toward=-0.4, name="fx_glow")


@_no_ground
def fire_small(frame=0):
    """A few flames on a hurt building, under half a tile across, and a
    wisp of smoke."""
    r = lib.empty("fx_fire_small")
    _glow(frame, r, (0, 0, 0), 0.5, 1)
    _flames(frame, r, (0, 0, 0), 0.44, 0.65, 6, 1)
    _smoke((frame * 2) % SMOKE_FRAMES, r, (0.45, 0.43, 0.41), (0.18, 0.17, 0.17), 0.45, puffs=3,
           rise=0.8, r0=0.1, r1=0.28, drift=0.2, seed=4, at=(0, 0, 0.5))
    return r


@_no_ground
def fire_large(frame=0):
    """A blaze: three clusters of tall flames, the glow, dark smoke over it."""
    r = lib.empty("fx_fire_large")
    _glow(frame, r, (0, 0, 0), 0.8, 2)
    for i, (dx, s) in enumerate(((-0.24, 0.8), (0.0, 1.0), (0.22, 0.75))):
        _flames(frame, r, RIGHT * dx + TOWARD * (0.02 * i), 0.42 * s, 0.85 * s, 6, 3 + i,
                toward=0.05 * (1 - abs(dx)))
    # smoke from the top of the flames, on the fire's loop: every other of
    # the smoke's 16 frames, so it rises twice as fast as a hearth's
    _smoke((frame * 2) % SMOKE_FRAMES, r, (0.42, 0.40, 0.38), (0.14, 0.13, 0.13), 0.55, puffs=4,
           rise=1.2, r0=0.14, r1=0.42, drift=0.3, seed=3, at=(0, 0, 0.6))
    return r


# ------------------------------------------------------------------- flag

@_no_ground
def flag(frame=0, player=BLUE, w=0.5, fh=0.3, nx=10):
    """A flag's cloth waving from a pole's top at the origin: its hoist
    along the pole from z = 0 down to -fh, flying out toward screen-right
    and a little away; a wave runs along it, one cycle a loop. Player
    colour with a gold stripe."""
    r = lib.empty("fx_flag")
    ph = 2 * math.pi * frame / FLAG_FRAMES
    rot_z = math.pi / 4

    def cloth(z0, z1, m):
        vs, fs = [], []
        nz = 2
        for i in range(nx + 1):
            u = i / nx
            for j in range(nz + 1):
                v = j / nz
                x = u * w * (1 - 0.06 * u)
                y = 0.07 * math.sin(u * 7.0 - ph) * u ** 0.8 + 0.02 * math.sin(u * 13.0 - 2 * ph) * u
                z = z0 + (z1 - z0) * v - 0.12 * w * u * u + 0.025 * math.sin(u * 5.0 - ph + v) * u
                vs.append((x, y, z))
        for i in range(nx):
            for j in range(nz):
                a = i * (nz + 1) + j
                fs.append((a, a + nz + 1, a + nz + 2, a + 1))
        ob = lib.mesh(vs, fs, m, (0, 0, 0), (0, 0, rot_z), r, smooth=True, name="cloth")
        ob.visible_shadow = False
        return ob

    zz = 0.0
    for frac, m in ((0.4, mat(player, rough=0.7)), (0.2, mat("gold", rough=0.6)),
                    (0.4, mat(player, rough=0.7))):
        cloth(zz, zz - fh * frac, m)
        zz -= fh * frac
    return r


SMOKE_ANIM = {"idle": [{"frame": i} for i in range(SMOKE_FRAMES)]}
FIRE_ANIM = {"idle": [{"frame": i} for i in range(FIRE_FRAMES)]}
FLAG_ANIM = {"idle": [{"frame": i} for i in range(FLAG_FRAMES)]}

SETS = {
    "fx_smoke": dict(func="fx.smoke", cls="SmallBuilding", tiles=1, stages=False, anims=SMOKE_ANIM,
                     timing={"idle": [SMOKE_MS, True]}, samples=96),
    "fx_smoke_dark": dict(func="fx.smoke_dark", cls="SmallBuilding", tiles=1, stages=False,
                          anims=SMOKE_ANIM, timing={"idle": [SMOKE_MS, True]}, samples=96),
    "fx_fire_small": dict(func="fx.fire_small", cls="SmallBuilding", tiles=1, stages=False,
                          anims=FIRE_ANIM, timing={"idle": [FIRE_MS, True]}, samples=96),
    "fx_fire_large": dict(func="fx.fire_large", cls="SmallBuilding", tiles=1, stages=False,
                          anims=FIRE_ANIM, timing={"idle": [FIRE_MS, True]}, samples=96),
    "fx_flag": dict(func="fx.flag", cls="SmallBuilding", tiles=1, stages=False, anims=FLAG_ANIM,
                    timing={"idle": [FLAG_MS, True]}, samples=64),
}

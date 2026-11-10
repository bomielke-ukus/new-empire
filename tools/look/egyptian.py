"""Egyptian buildings for the new look: New Kingdom polychrome, as a sunlit
painted diorama. Every builder is centred on the origin on its footprint (in
tiles), takes loc, rot_z, player, parent and returns its root empty.

    /usr/bin/python3.13 tools/look/preview.py egyptian.temple out.png 6

The visible faces are -Y (sunlit, screen left) and +X (screen right), so the
doors and the paint are there.
"""
import math
import random

import bpy
import bmesh
from mathutils import Vector

from lib import PAL, RED, BLUE, _lin, _obj, box, cyl, sphere, blob, empty, mat, mesh

PADS = True  # a thin packed-sand pad under each building (set False to drop it)

# Extra pigments, in the kit's spirit (sRGB).
C = {
    "stone": (0.93, 0.78, 0.52),       # painted/sunlit sandstone
    "stone_lt": (0.97, 0.87, 0.66),
    "stone_dk": (0.74, 0.56, 0.36),
    "mud": (0.78, 0.58, 0.40),
    "mud_dk": (0.60, 0.42, 0.28),
    "white": (0.99, 0.96, 0.88),
    "cream": (0.98, 0.90, 0.70),
    "ochre_wash": (0.96, 0.78, 0.42),
    "granite": (0.72, 0.33, 0.26),
    "granite_dk": (0.52, 0.24, 0.20),
    "skin": (0.72, 0.34, 0.18),
    "blue": PAL["egyptian_blue"], "lapis": PAL["lapis"],
    "red": (0.80, 0.20, 0.12), "yellow": (0.98, 0.76, 0.18),
    "green": (0.10, 0.60, 0.36), "turq": (0.16, 0.70, 0.68),
    "dark": (0.10, 0.07, 0.06), "roof": (0.84, 0.70, 0.50),
}


def col(c):
    if isinstance(c, str):
        return C[c] if c in C else PAL[c]
    return tuple(c)


def M(c, **kw):
    if isinstance(c, bpy.types.Material):
        return c
    return mat(col(c), **kw)


def gold():
    return mat(PAL["gold"], rough=0.32, metal=0.75, emit=0.35)


def _key(prefix, *parts):
    return (prefix + "_" + "%08x" % (hash(repr(parts)) & 0xFFFFFFFF))


def _scale(rgb, k):
    return tuple(min(1.0, v * k) for v in rgb)


def _newmat(name, rough):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    b = nt.nodes["Principled BSDF"]
    b.inputs["Roughness"].default_value = rough
    return m, nt.nodes, nt.links, b


def _mixrgb(N, mode="MIX"):
    n = N.new("ShaderNodeMix")
    n.data_type = "RGBA"
    n.blend_type = mode
    n.clamp_result = False
    return n  # inputs[0] fac, [6] A, [7] B ; outputs[2]


def brickmat(c, bw=0.30, bh=0.10, contrast=0.07, mortar=0.80, rough=0.9, blotch=0.10,
             msize=0.010):
    """Courses of brick/block on vertical faces, mottled; flat on top faces."""
    rgb = col(c)
    name = _key("bk", rgb, bw, bh, contrast, mortar, rough, blotch, msize)
    m = bpy.data.materials.get(name)
    if m:
        return m
    m, N, L, bsdf = _newmat(name, rough)
    tc = N.new("ShaderNodeTexCoord")
    sep = N.new("ShaderNodeSeparateXYZ")
    L.new(tc.outputs["Object"], sep.inputs[0])
    add = N.new("ShaderNodeMath")
    add.operation = "ADD"
    L.new(sep.outputs["X"], add.inputs[0])
    L.new(sep.outputs["Y"], add.inputs[1])
    comb = N.new("ShaderNodeCombineXYZ")
    L.new(add.outputs[0], comb.inputs["X"])
    L.new(sep.outputs["Z"], comb.inputs["Y"])
    bk = N.new("ShaderNodeTexBrick")
    L.new(comb.outputs[0], bk.inputs["Vector"])
    bk.inputs["Color1"].default_value = (*_lin(_scale(rgb, 1 + contrast)), 1)
    bk.inputs["Color2"].default_value = (*_lin(_scale(rgb, 1 - contrast)), 1)
    bk.inputs["Mortar"].default_value = (*_lin(_scale(rgb, mortar)), 1)
    bk.inputs["Scale"].default_value = 1.0
    bk.inputs["Mortar Size"].default_value = msize
    bk.inputs["Mortar Smooth"].default_value = 0.1
    bk.inputs["Bias"].default_value = 0.0
    bk.inputs["Brick Width"].default_value = bw
    bk.inputs["Row Height"].default_value = bh
    noise = N.new("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = 1.7
    noise.inputs["Detail"].default_value = 3.0
    L.new(tc.outputs["Object"], noise.inputs["Vector"])
    mr = N.new("ShaderNodeMapRange")
    mr.inputs["From Min"].default_value = 0.3
    mr.inputs["From Max"].default_value = 0.7
    mr.inputs["To Min"].default_value = 1 - blotch
    mr.inputs["To Max"].default_value = 1 + blotch
    L.new(noise.outputs["Fac"], mr.inputs["Value"])
    mul = _mixrgb(N, "MULTIPLY")
    mul.inputs[0].default_value = 1.0
    L.new(bk.outputs["Color"], mul.inputs[6])
    L.new(mr.outputs["Result"], mul.inputs[7])
    # top faces: no courses
    geo = N.new("ShaderNodeNewGeometry")
    sn = N.new("ShaderNodeSeparateXYZ")
    L.new(geo.outputs["Normal"], sn.inputs[0])
    ab = N.new("ShaderNodeMath")
    ab.operation = "ABSOLUTE"
    L.new(sn.outputs["Z"], ab.inputs[0])
    gt = N.new("ShaderNodeMath")
    gt.operation = "GREATER_THAN"
    gt.inputs[1].default_value = 0.8
    L.new(ab.outputs[0], gt.inputs[0])
    flat = _mixrgb(N, "MULTIPLY")
    flat.inputs[0].default_value = 1.0
    flat.inputs[6].default_value = (*_lin(rgb), 1)
    L.new(mr.outputs["Result"], flat.inputs[7])
    mix = _mixrgb(N, "MIX")
    L.new(gt.outputs[0], mix.inputs[0])
    L.new(mul.outputs[2], mix.inputs[6])
    L.new(flat.outputs[2], mix.inputs[7])
    L.new(mix.outputs[2], bsdf.inputs["Base Color"])
    return m


def mottle(c, amt=0.10, scale=3.0, rough=0.95):
    """A plain colour with soft blotches (sand, plaster, roof mud)."""
    rgb = col(c)
    name = _key("mo", rgb, amt, scale, rough)
    m = bpy.data.materials.get(name)
    if m:
        return m
    m, N, L, bsdf = _newmat(name, rough)
    tc = N.new("ShaderNodeTexCoord")
    noise = N.new("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = scale
    noise.inputs["Detail"].default_value = 4.0
    L.new(tc.outputs["Object"], noise.inputs["Vector"])
    mr = N.new("ShaderNodeMapRange")
    mr.inputs["From Min"].default_value = 0.3
    mr.inputs["From Max"].default_value = 0.7
    mr.inputs["To Min"].default_value = 1 - amt
    mr.inputs["To Max"].default_value = 1 + amt
    L.new(noise.outputs["Fac"], mr.inputs["Value"])
    mul = _mixrgb(N, "MULTIPLY")
    mul.inputs[0].default_value = 1.0
    mul.inputs[6].default_value = (*_lin(rgb), 1)
    L.new(mr.outputs["Result"], mul.inputs[7])
    L.new(mul.outputs[2], bsdf.inputs["Base Color"])
    return m


def stripemat(colors, period=0.1, axis="h", rough=0.8):
    """Repeating painted stripes. axis: 'h' along a wall (x+y), 'z' up,
    'x', 'y', or 'ang' around the object's Z axis (period = 1/count)."""
    cols = [col(c) for c in colors]
    name = _key("st", cols, period, axis, rough)
    m = bpy.data.materials.get(name)
    if m:
        return m
    m, N, L, bsdf = _newmat(name, rough)
    tc = N.new("ShaderNodeTexCoord")
    sep = N.new("ShaderNodeSeparateXYZ")
    L.new(tc.outputs["Object"], sep.inputs[0])
    if axis == "h":
        u = N.new("ShaderNodeMath")
        u.operation = "ADD"
        L.new(sep.outputs["X"], u.inputs[0])
        L.new(sep.outputs["Y"], u.inputs[1])
        uo = u.outputs[0]
    elif axis == "ang":
        a = N.new("ShaderNodeMath")
        a.operation = "ARCTAN2"
        L.new(sep.outputs["Y"], a.inputs[0])
        L.new(sep.outputs["X"], a.inputs[1])
        u = N.new("ShaderNodeMath")
        u.operation = "MULTIPLY_ADD"
        u.inputs[1].default_value = 1 / (2 * math.pi)
        u.inputs[2].default_value = 0.5
        L.new(a.outputs[0], u.inputs[0])
        uo = u.outputs[0]
    else:
        uo = sep.outputs[axis.upper()]
    div = N.new("ShaderNodeMath")
    div.operation = "DIVIDE"
    div.inputs[1].default_value = period
    L.new(uo, div.inputs[0])
    off = N.new("ShaderNodeMath")
    off.operation = "ADD"
    off.inputs[1].default_value = 1000.25
    L.new(div.outputs[0], off.inputs[0])
    fr = N.new("ShaderNodeMath")
    fr.operation = "FRACT"
    L.new(off.outputs[0], fr.inputs[0])
    ramp = N.new("ShaderNodeValToRGB")
    cr = ramp.color_ramp
    cr.interpolation = "CONSTANT"
    n = len(cols)
    for i, c in enumerate(cols):
        if i < 2:
            e = cr.elements[i]
            e.position = i / n
        else:
            e = cr.elements.new(i / n)
        e.color = (*_lin(c), 1)
    L.new(fr.outputs[0], ramp.inputs["Fac"])
    L.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])
    return m


def cowhide(seed=0.0):
    name = _key("cow", seed)
    m = bpy.data.materials.get(name)
    if m:
        return m
    m, N, L, bsdf = _newmat(name, 0.8)
    tc = N.new("ShaderNodeTexCoord")
    mp = N.new("ShaderNodeMapping")
    mp.inputs["Location"].default_value = (seed * 3.1, seed * 1.7, seed)
    L.new(tc.outputs["Object"], mp.inputs["Vector"])
    noise = N.new("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = 7.0
    noise.inputs["Detail"].default_value = 1.0
    L.new(mp.outputs["Vector"], noise.inputs["Vector"])
    ramp = N.new("ShaderNodeValToRGB")
    cr = ramp.color_ramp
    cr.interpolation = "CONSTANT"
    cr.elements[0].position = 0.0
    cr.elements[0].color = (*_lin((0.97, 0.93, 0.84)), 1)
    cr.elements[1].position = 0.54
    cr.elements[1].color = (*_lin((0.42, 0.22, 0.12)), 1)
    e = cr.elements.new(0.66)
    e.color = (*_lin((0.12, 0.08, 0.06)), 1)
    L.new(noise.outputs["Fac"], ramp.inputs["Fac"])
    L.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])
    return m


# --- shapes ---------------------------------------------------------------

def loft(secs, m, loc=(0, 0, 0), parent=None, rot=(0, 0, 0), bevel=0.0, name="loft"):
    """Stacked rectangles: secs = (sx, sy, z[, cx, cy]) from the bottom up."""
    bm = bmesh.new()
    rings = []
    for s in secs:
        sx, sy, z = max(s[0], 1e-4), max(s[1], 1e-4), s[2]
        cx, cy = (s[3], s[4]) if len(s) > 3 else (0, 0)
        rings.append([bm.verts.new((cx + dx * sx / 2, cy + dy * sy / 2, z))
                      for dx, dy in ((-1, -1), (1, -1), (1, 1), (-1, 1))])
    for a, b in zip(rings, rings[1:]):
        for i in range(4):
            j = (i + 1) % 4
            bm.faces.new((a[i], a[j], b[j], b[i]))
    bm.faces.new(rings[0][::-1])
    bm.faces.new(rings[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _obj(bm, name, M(m), loc, rot, False, parent, bevel)


def lathe(prof, m, loc=(0, 0, 0), parent=None, segs=12, rot=(0, 0, 0), closed=False,
          scale=(1, 1, 1), name="lathe"):
    """Revolve (r, z) points about Z. closed=True joins the last ring to the first."""
    bm = bmesh.new()
    rings = []
    for r, z in prof:
        if r <= 1e-6:
            rings.append([bm.verts.new((0, 0, z))])
        else:
            rings.append([bm.verts.new((r * math.cos(2 * math.pi * i / segs) * scale[0],
                                        r * math.sin(2 * math.pi * i / segs) * scale[1], z))
                          for i in range(segs)])
    pairs = list(zip(rings, rings[1:]))
    if closed:
        pairs.append((rings[-1], rings[0]))
    for a, b in pairs:
        for i in range(segs):
            j = (i + 1) % segs
            if len(a) == 1 and len(b) == 1:
                continue
            if len(a) == 1:
                bm.faces.new((a[0], b[j], b[i]))
            elif len(b) == 1:
                bm.faces.new((a[i], a[j], b[0]))
            else:
                bm.faces.new((a[i], a[j], b[j], b[i]))
    if not closed:
        if len(rings[0]) > 1:
            bm.faces.new(rings[0][::-1])
        if len(rings[-1]) > 1:
            bm.faces.new(rings[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _obj(bm, name, M(m), loc, rot, True, parent, 0.0)


def rod(p0, p1, r, m, parent=None, r_top=None, segs=8):
    """A thin cylinder from p0 to p1."""
    p0, p1 = Vector(p0), Vector(p1)
    d = p1 - p0
    rot = d.to_track_quat("Z", "Y").to_euler()
    return cyl(r, d.length, (p0 + p1) / 2, M(m), rot=rot, parent=parent, r_top=r_top,
               segs=segs)


class Paint:
    """Flat painted shapes on a wall plane. o = origin on the wall, u = unit
    vector along the wall (screen-right), v = unit vector up the wall; shapes
    are given in (a, b) tiles from o. Call done() to build."""

    def __init__(self, o, u, v, parent, off=0.006):
        self.o, self.u, self.v = Vector(o), Vector(u).normalized(), Vector(v).normalized()
        self.n = self.u.cross(self.v).normalized()
        self.parent, self.off = parent, off
        self.batches = {}

    def poly(self, pts, c, layer=0):
        m = M(c)
        vs, fs = self.batches.setdefault(m.name, ([], []))
        self.batches[m.name] = (vs, fs)
        self._m = getattr(self, "_m", {})
        self._m[m.name] = m
        k = len(vs)
        for a, b in pts:
            vs.append(self.o + self.u * a + self.v * b + self.n * (self.off * (layer + 1)))
        fs.append(tuple(range(k, k + len(pts))))

    def rect(self, a0, b0, a1, b1, c, layer=0):
        self.poly([(a0, b0), (a1, b0), (a1, b1), (a0, b1)], c, layer)

    def disc(self, a, b, r, c, layer=0, segs=14, sy=1.0):
        self.poly([(a + r * math.cos(2 * math.pi * i / segs),
                    b + r * sy * math.sin(2 * math.pi * i / segs)) for i in range(segs)],
                  c, layer)

    def done(self):
        for name, (vs, fs) in self.batches.items():
            mesh([tuple(v) for v in vs], fs, self._m[name], parent=self.parent, name="paint")
        self.batches = {}


def figure(p, a, b, h, facing=1, crown="blue", kilt="white", layer=1, staff=True):
    """A striding painted figure, feet at (a, b), `h` tall, facing +a or -a."""
    f = facing

    def P(pts, c, lay=0):
        p.poly([(a + f * x * h, b + y * h) for x, y in pts], c, layer + lay)

    sk = "skin"
    P([(-0.13, 0), (-0.03, 0), (0.02, 0.46), (-0.07, 0.46)], sk)
    P([(0.07, 0), (0.17, 0), (0.07, 0.46), (-0.02, 0.46)], sk)
    P([(-0.13, 0), (0.0, 0), (0.0, 0.035), (-0.13, 0.035)], sk)
    P([(0.07, 0), (0.23, 0), (0.23, 0.035), (0.07, 0.035)], sk)
    P([(-0.10, 0.34), (0.15, 0.30), (0.09, 0.56), (-0.08, 0.56)], kilt, 1)
    P([(-0.085, 0.53), (0.095, 0.53), (0.09, 0.57), (-0.08, 0.57)], "yellow", 2)
    P([(0.02, 0.33), (0.07, 0.32), (0.06, 0.53), (0.03, 0.53)], "blue", 2)
    P([(-0.08, 0.56), (0.09, 0.56), (0.17, 0.79), (-0.16, 0.79)], sk)
    P([(-0.15, 0.755), (0.16, 0.755), (0.17, 0.80), (-0.16, 0.80)], "blue", 1)
    P([(-0.13, 0.735), (0.14, 0.735), (0.15, 0.757), (-0.14, 0.757)], "yellow", 1)
    P([(-0.04, 0.80), (0.075, 0.80), (0.085, 0.93), (-0.04, 0.93)], sk)
    if crown == "blue":
        P([(-0.075, 0.87), (0.085, 0.92), (0.07, 1.06), (-0.02, 1.09), (-0.10, 1.0)], "blue", 1)
        P([(-0.06, 0.885), (0.09, 0.925), (0.09, 0.95), (-0.065, 0.91)], "yellow", 2)
    elif crown == "white":
        P([(-0.06, 0.90), (0.085, 0.92), (0.05, 1.10), (0.0, 1.17), (-0.045, 1.10)], "white", 1)
        P([(-0.065, 0.89), (0.09, 0.915), (0.09, 0.945), (-0.065, 0.92)], "yellow", 2)
    elif crown == "red":
        P([(-0.09, 0.90), (0.085, 0.92), (0.07, 1.0), (-0.03, 1.0), (-0.06, 1.14), (-0.10, 1.14)],
          "red", 1)
    else:  # wig + disc (a god)
        P([(-0.085, 0.76), (-0.02, 0.76), (-0.02, 0.90), (0.085, 0.93), (0.07, 0.97),
           (-0.085, 0.97)], "lapis", 1)
        p.disc(a + f * 0.0, b + 1.06 * h, 0.075 * h, "red", layer + 1)
    # arms
    P([(0.13, 0.78), (0.33, 0.63), (0.34, 0.585), (0.12, 0.72)], sk, 1)
    P([(-0.15, 0.79), (-0.31, 0.93), (-0.27, 0.965), (-0.12, 0.83)], sk, 1)
    if staff:
        P([(0.325, 0.03), (0.35, 0.03), (0.35, 0.98), (0.325, 0.98)], "yellow", 1)
        P([(0.30, 0.95), (0.375, 0.95), (0.375, 1.0), (0.30, 1.0)], "blue", 2)
    P([(-0.33, 0.90), (-0.30, 0.90), (-0.30, 1.10), (-0.33, 1.10)], "yellow", 1)
    p.disc(a - f * 0.315 * h, b + 1.11 * h, 0.035 * h, "white", layer + 2, segs=8)


def glyphs(p, a, b0, b1, w, seed=0, layer=1, cols=("blue", "red", "green", "dark", "yellow")):
    """A column of little signs between b0 and b1, centred on a, `w` wide."""
    rnd = random.Random(seed)
    b = b1
    while b - w * 0.5 > b0:
        k = rnd.randrange(6)
        c = rnd.choice(cols)
        hgt = w * rnd.choice((0.35, 0.5, 0.7))
        if b - hgt < b0:
            break
        if k == 0:
            p.rect(a - w / 2, b - hgt, a + w / 2, b, c, layer)
        elif k == 1:
            p.disc(a, b - hgt / 2, hgt / 2, c, layer, segs=8)
        elif k == 2:
            p.rect(a - w / 2, b - hgt, a - w * 0.1, b, c, layer)
            p.rect(a + w * 0.1, b - hgt, a + w / 2, b, rnd.choice(cols), layer)
        elif k == 3:
            p.poly([(a - w / 2, b - hgt), (a + w / 2, b - hgt), (a, b)], c, layer)
        elif k == 4:
            p.rect(a - w * 0.15, b - hgt, a + w * 0.15, b, c, layer)
        else:
            p.poly([(a - w / 2, b - hgt), (a + w / 2, b - hgt * 0.6), (a + w / 2, b),
                    (a - w / 2, b - hgt * 0.4)], c, layer)
        b -= hgt + w * 0.22


def winged_disc(p, a, b, r, span, layer=1):
    for s in (-1, 1):
        for i, c in enumerate(("blue", "green", "red")):
            y0 = b + r * (0.55 - i * 0.5)
            ln = span * (1 - i * 0.17)
            p.poly([(a + s * r * 0.5, y0), (a + s * ln, y0 + r * 0.25),
                    (a + s * ln * 0.97, y0 - r * 0.42), (a + s * r * 0.5, y0 - r * 0.5)], c, layer)
        p.poly([(a + s * r * 0.5, b + r * 0.75), (a + s * span, b + r * 0.98),
                (a + s * span, b + r * 0.80), (a + s * r * 0.5, b + r * 0.55)], "yellow", layer)
    p.disc(a, b, r * 1.18, "yellow", layer + 1)
    p.disc(a, b, r, "red", layer + 2)


def cavetto(root, sx, sy, z, h=0.18, flare=0.07, loc=(0, 0), period=0.11,
            colors=("blue", "white", "red", "white", "green", "white"), roll="yellow",
            top="stone_lt"):
    """The Egyptian cornice: a roll moulding, a flaring painted gorge, a fillet.
    Sits on a wall top of sx x sy at height z; returns the new top z."""
    x, y = loc
    box((sx + 0.04, sy + 0.04, 0.035), (x, y, z + 0.0175), M(roll), parent=root)
    z0 = z + 0.035
    loft([(sx, sy, 0), (sx + flare * 0.5, sy + flare * 0.5, h * 0.55),
          (sx + flare * 2, sy + flare * 2, h)], stripemat(colors, period, "h"),
         (x, y, z0), root)
    box((sx + flare * 2 + 0.02, sy + flare * 2 + 0.02, 0.04), (x, y, z0 + h + 0.02), M(top),
        parent=root)
    return z0 + h + 0.04


def band(root, sx, sy, z, h, c, loc=(0, 0), out=0.008):
    """A painted band right round a rectangular block."""
    return box((sx + 2 * out, sy + 2 * out, h), (loc[0], loc[1], z + h / 2), M(c), parent=root)


def slab(root, side, p, u, z, w, h, d, c, bevel=0.0):
    """Something flat stuck on a wall: side 'y' = the -Y wall at y=p, 'x' = the
    +X wall at x=p; centred at u along the wall, bottom at z, d proud."""
    if side == "y":
        return box((w, d, h), (u, p - d / 2 + 0.004, z + h / 2), M(c), parent=root, bevel=bevel)
    return box((d, w, h), (p + d / 2 - 0.004, u, z + h / 2), M(c), parent=root, bevel=bevel)


def door(root, side, p, u, w=0.34, h=1.05, c="blue", frame="red", lintel="stone_lt", z=0.0):
    slab(root, side, p, u, z, w + 0.14, h + 0.07, 0.03, frame)
    slab(root, side, p, u, z + h + 0.05, w + 0.26, 0.09, 0.06, lintel)
    slab(root, side, p, u, z, w, h, 0.045, c)
    # planks and a bolt
    for k in (-1, 0, 1):
        slab(root, side, p, u + k * w / 3.2, z + 0.02, 0.012, h - 0.04, 0.05, _scale(col(c), 0.6))
    slab(root, side, p, u + w * 0.3, z + h * 0.5, 0.05, 0.05, 0.06, "gold")
    slab(root, side, p, u, z, w + 0.2, 0.05, 0.10, "stone_dk")


def window(root, side, p, u, z, w=0.2, h=0.16, bars=2, frame="wood"):
    slab(root, side, p, u, z - 0.025, w + 0.08, 0.03, 0.035, frame)
    slab(root, side, p, u, z + h, w + 0.08, 0.03, 0.035, frame)
    slab(root, side, p, u, z, w, h, 0.012, "dark")
    for i in range(bars):
        t = (i + 1) / (bars + 1) - 0.5
        slab(root, side, p, u + t * w, z, 0.018, h, 0.022, frame)


def parapet(root, sx, sy, z, h, t, c, loc=(0, 0), roof="roof", cap=None):
    """A flat roof with a low wall round it, on a block sx x sy topped at z."""
    x, y = loc
    m = M(c)
    box((sx - t, sy - t, 0.03), (x, y, z + 0.015), mottle(roof, 0.08, 4.0), parent=root)
    for s in (-1, 1):
        box((sx, t, h), (x, y + s * (sy - t) / 2, z + h / 2), m, parent=root)
        box((t, sy - 2 * t, h), (x + s * (sx - t) / 2, y, z + h / 2), m, parent=root)
    if cap:
        cm = M(cap)
        for s in (-1, 1):
            box((sx + 0.02, t + 0.02, 0.025), (x, y + s * (sy - t) / 2, z + h + 0.012), cm,
                parent=root)
            box((t + 0.02, sy - 2 * t - 0.02, 0.025), (x + s * (sx - t) / 2, y, z + h + 0.012), cm,
                parent=root)
    return z + h


def merlons(root, p0, p1, z, n, w, h, t, c):
    """Round-topped merlons along a wall top from p0 to p1 (xy)."""
    m = M(c)
    along_x = abs(p1[0] - p0[0]) > abs(p1[1] - p0[1])
    for i in range(n):
        f = (i + 0.5) / n
        x, y = p0[0] + (p1[0] - p0[0]) * f, p0[1] + (p1[1] - p0[1]) * f
        size = (w, t, h) if along_x else (t, w, h)
        box(size, (x, y, z + h / 2), m, parent=root)
        rot = (math.pi / 2, 0, 0) if along_x else (0, math.pi / 2, 0)
        cyl(w / 2, t + 0.006, (x, y, z + h), m, rot=rot, parent=root, segs=10)


def column(root, x, y, h, r=0.11, z=0.0, style=0, abacus=True):
    """A papyrus column, painted."""
    sets = (("green", "yellow", "red", "blue"), ("blue", "yellow", "red", "green"),
            ("red", "yellow", "blue", "green"))
    c1, c2, c3, c4 = sets[style % 3]
    L = (x, y, z)
    lathe([(r * 1.55, 0), (r * 1.55, 0.05), (r * 1.2, 0.07)], "white", L, root)
    lathe([(r * 0.92, 0.05), (r * 1.06, 0.12 * h), (r * 1.04, 0.24 * h)],
          stripemat((c3, c2, c3, c1), 1 / 8, "ang"), L, root)
    lathe([(r * 1.03, 0.24 * h), (r * 0.84, 0.66 * h)], "cream", L, root)
    zb = 0.60 * h
    for i, c in enumerate((c4, c2, c3, c2, c4)):
        lathe([(r * 0.90, zb + i * 0.024 * h), (r * 0.90, zb + (i + 1) * 0.024 * h)], c, L, root)
    lathe([(r * 0.80, 0.72 * h), (r * 0.98, 0.80 * h), (r * 1.55, 0.91 * h), (r * 1.85, 0.945 * h)],
          stripemat((c1, c2, c1, c4), 1 / 8, "ang"), L, root)
    lathe([(r * 1.85, 0.945 * h), (r * 1.88, 0.965 * h), (r * 1.0, 0.965 * h)], c2, L, root)
    if abacus:
        box((r * 1.9, r * 1.9, 0.035 * h + 0.005), (x, y, z + 0.9825 * h), M("white"), parent=root)
    return z + h


def pennant(root, top, length, height, c, kind="pennant", dirv=(1, 1), wave=0.035, seed=0,
            bands=None, n=8):
    """Cloth flying from `top` (the hoist's upper corner) toward dirv."""
    d = Vector((dirv[0], dirv[1], 0)).normalized()
    nr = Vector((d.y, -d.x, 0))
    top = Vector(top)
    ph = seed * 1.7
    bands = bands or [(0.0, 1.0, c)]

    def pt(t, s):  # t along, s 0 top..1 bottom
        hh = height * (1 - 0.72 * t) if kind == "pennant" else height * (1 - 0.06 * math.sin(t * 3))
        mid = 0.5 * height if kind == "pennant" else 0.0
        zc = -mid - (0.10 * t * t * length) + (0.5 - s) * hh if kind == "pennant" else \
            -(0.07 * t * t * length) - s * hh
        w = math.sin(t * 2 * math.pi * 1.25 + ph + s * 0.8) * wave * (0.25 + t)
        return tuple(top + d * (t * length) + nr * w + Vector((0, 0, zc)))

    for s0, s1, bc in bands:
        vs, fs = [], []
        for i in range(n + 1):
            t = i / n
            vs += [pt(t, s0), pt(t, s1)]
        for i in range(n):
            fs.append((2 * i, 2 * i + 2, 2 * i + 3, 2 * i + 1))
        mesh(vs, fs, M(bc, rough=0.7), parent=root, smooth=True, name="flag")


def flagpole(root, x, y, h, player, z=0.0, r=0.035, flag=(0.55, 0.17), kind="pennant", seed=0,
             bands=None, c="cedar", flag_drop=0.08):
    rod((x, y, z), (x, y, z + h), r, c, root, r_top=r * 0.55)
    for f in (0.08, 0.5):
        cyl(r * 1.25, 0.03, (x, y, z + h * f), M("bronze", metal=0.5, rough=0.4), parent=root,
            segs=8)
    lathe([(r * 0.7, 0), (r * 1.0, 0.035), (0, 0.11)], gold(), (x, y, z + h - 0.01), root, segs=8)
    pennant(root, (x, y, z + h - flag_drop), flag[0], flag[1], player, kind, seed=seed,
            bands=bands)


def cloth(root, corners, c, sag=0.06, n=6, stripes=None, period=0.14):
    """An awning between four corner points (in order round), sagging."""
    a, b, c2, d = [Vector(p) for p in corners]
    vs, fs = [], []
    for i in range(n + 1):
        for j in range(n + 1):
            u, v = i / n, j / n
            p = (a * (1 - u) + b * u) * (1 - v) + (d * (1 - u) + c2 * u) * v
            p.z -= sag * math.sin(math.pi * u) * math.sin(math.pi * v) * 4 * 0.25 * 4 * 0.25 * 4
            vs.append(tuple(p))
    for i in range(n):
        for j in range(n):
            k = i * (n + 1) + j
            fs.append((k, k + 1, k + n + 2, k + n + 1))
    m = stripemat(stripes, period, "h", rough=0.75) if stripes else M(c, rough=0.75)
    return mesh(vs, fs, m, parent=root, smooth=True, name="awning")


def awning(root, x0, y0, x1, y1, z, h, player, sag=0.05, posts="wood", lean=0.0):
    """A striped awning on four poles over the rectangle, top at z + h."""
    pts = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    for px, py in pts:
        rod((px, py, z), (px, py, z + h + 0.03), 0.018, posts, root, segs=6)
    hz = [z + h, z + h, z + h + lean, z + h + lean]
    cloth(root, [(px, py, hz[i]) for i, (px, py) in enumerate(pts)], player, sag,
          stripes=(player, player, "linen"), period=0.085)


def jar(root, x, y, z=0.0, s=1.0, c="terracotta", bandc=None):
    lathe([(0.045 * s, 0), (0.09 * s, 0.05 * s), (0.115 * s, 0.15 * s), (0.10 * s, 0.24 * s),
           (0.055 * s, 0.30 * s), (0.05 * s, 0.33 * s), (0.075 * s, 0.36 * s),
           (0.045 * s, 0.36 * s)], M(c, rough=0.6), (x, y, z), root, segs=10)
    if bandc:
        lathe([(0.117 * s, 0.13 * s), (0.118 * s, 0.17 * s), (0.112 * s, 0.20 * s)], bandc,
              (x, y, z), root, segs=10)


def basket(root, x, y, z=0.0, s=1.0, fill="wheat"):
    lathe([(0.085 * s, 0), (0.125 * s, 0.12 * s), (0.13 * s, 0.13 * s), (0.10 * s, 0.13 * s)],
          stripemat(("straw", "thatch", "straw", "leather"), 0.035 * s, "z"), (x, y, z), root,
          segs=10)
    sphere(0.105 * s, (x, y, z + 0.115 * s), M(fill), scale=(1, 1, 0.45), parent=root, segs=10)


def palm(root, x, y, h=1.7, seed=0, lean=(0.08, 0.05), z=0.0):
    rnd = random.Random(seed)
    n = 7
    pts = []
    for i in range(n + 1):
        t = i / n
        pts.append(Vector((x + lean[0] * t * t * h, y + lean[1] * t * t * h, z + t * h)))
    for i in range(n):
        r0 = 0.085 - 0.03 * i / n
        rod(pts[i], pts[i + 1] + (pts[i + 1] - pts[i]) * 0.08, r0 * 0.82,
            (0.60, 0.42, 0.24) if i % 2 else (0.50, 0.33, 0.19), root, r_top=r0, segs=8)
    top = pts[-1]
    for k in range(4):
        a = k * 1.6 + 0.4
        sphere(0.05, (top.x + 0.07 * math.cos(a), top.y + 0.07 * math.sin(a), top.z - 0.07),
               M((0.95, 0.50, 0.12)), parent=root, segs=8, scale=(1, 1, 1.4))
    nf = 11
    for k in range(nf):
        az = 2 * math.pi * k / nf + rnd.uniform(-0.15, 0.15)
        ln = h * rnd.uniform(0.40, 0.52)
        rise = rnd.uniform(0.25, 0.75) if k % 2 else rnd.uniform(0.0, 0.3)
        d = Vector((math.cos(az), math.sin(az), 0))
        side = Vector((-d.y, d.x, 0))
        vs, fs = [], []
        m = 6
        for i in range(m + 1):
            t = i / m
            c = top + d * (ln * (t - 0.25 * t * t)) + Vector((0, 0, ln * (rise * t - 0.95 * t * t)))
            w = 0.15 * h / 1.7 * math.sin(math.pi * (0.12 + 0.86 * t))
            vs += [tuple(c + side * w - Vector((0, 0, 0.05))), tuple(c + Vector((0, 0, 0.02))),
                   tuple(c - side * w - Vector((0, 0, 0.05)))]
        for i in range(m):
            b = 3 * i
            fs += [(b, b + 3, b + 4, b + 1), (b + 1, b + 4, b + 5, b + 2)]
        mesh(vs, fs, M(("palm", "leaf", (0.40, 0.66, 0.22))[k % 3]), parent=root, name="frond")


def ladder(root, p0, p1, w=0.16, c="wood", rungs=6):
    p0, p1 = Vector(p0), Vector(p1)
    d = (p1 - p0)
    side = Vector((-d.y, d.x, 0))
    if side.length < 1e-4:
        side = Vector((1, 0, 0))
    side.normalize()
    for s in (-1, 1):
        rod(p0 + side * s * w / 2, p1 + side * s * w / 2, 0.014, c, root, segs=6)
    for i in range(rungs):
        t = (i + 0.7) / (rungs + 0.4)
        q = p0 + d * t
        rod(q - side * w / 2, q + side * w / 2, 0.011, c, root, segs=6)


def stairs(root, p0, p1, w, c, n=7, side="x"):
    """Solid steps climbing from p0 (x, y, z0) to p1 (x, y, z1); `side` is the
    axis the stair is wide along."""
    p0, p1 = Vector(p0), Vector(p1)
    m = M(c)
    for i in range(n):
        t0, t1 = i / n, (i + 1) / n
        a, b = p0.lerp(p1, t0), p0.lerp(p1, t1)
        top = p0.z + (p1.z - p0.z) * t1
        cx, cy = (a.x + b.x) / 2, (a.y + b.y) / 2
        run = (b - a).to_2d().length
        size = (w, run + 0.002, top) if side == "x" else (run + 0.002, w, top)
        box(size, (cx, cy, top / 2), m, parent=root)


def silo(root, x, y, r=0.3, h=0.95, c="white", hatch_side="y", z=0.0, seed=0):
    m = mottle(c, 0.07, 3.0 + seed)
    lathe([(r * 1.04, 0), (r * 1.04, 0.07 * h), (r, 0.09 * h), (r * 0.98, 0.45 * h),
           (r * 0.86, 0.68 * h), (r * 0.60, 0.86 * h), (r * 0.28, 0.96 * h), (0, 0.99 * h)],
          m, (x, y, z), root, segs=14)
    lathe([(r * 1.07, 0), (r * 1.07, 0.085 * h), (r * 1.0, 0.09 * h)], "mud_dk", (x, y, z), root,
          segs=14)
    lathe([(r * 1.0, 0.40 * h), (r * 0.995, 0.445 * h)], "red", (x, y, z), root, segs=14)
    lathe([(r * 0.3, 0.955 * h), (r * 0.3, 1.0 * h), (r * 0.12, 1.03 * h)], "mud", (x, y, z), root,
          segs=10)
    if hatch_side == "y":
        box((0.15, 0.05, 0.15), (x, y - r * 0.80, z + 0.70 * h), M("wood_dark"), parent=root,
            rot=(-0.5, 0, 0))
        box((0.13, 0.04, 0.14), (x, y - r * 1.0, z + 0.17 * h), M("wood"), parent=root)
    else:
        box((0.05, 0.15, 0.15), (x + r * 0.80, y, z + 0.70 * h), M("wood_dark"), parent=root,
            rot=(0, -0.5, 0))
        box((0.04, 0.13, 0.14), (x + r * 1.0, y, z + 0.17 * h), M("wood"), parent=root)


def pad(root, sx, sy, c="sand"):
    if PADS:
        box((sx - 0.06, sy - 0.06, 0.03), (0, 0, 0.012), mottle(c, 0.07, 1.5), parent=root,
            bevel=0.02, name="pad")


def _sub(root, loc, rot_z=0.0, s=1.0, name="part"):
    e = empty(name, loc, rot_z, root)
    e.scale = (s, s, s)
    return e


def nemes_head(e, z, s=1.0):
    """A pharaoh's head in a striped nemes, facing -Y, chin at z (local)."""
    st = stripemat(("blue", "yellow"), 0.045 * s, "z")
    box((0.17 * s, 0.17 * s, 0.20 * s), (0, 0, z + 0.10 * s), M("skin"), parent=e, bevel=0.045 * s)
    box((0.035 * s, 0.03 * s, 0.05 * s), (0, -0.09 * s, z + 0.085 * s), M("skin"), parent=e)
    loft([(0.40 * s, 0.14 * s, 0, 0, 0.05 * s), (0.36 * s, 0.20 * s, 0.14 * s, 0, 0.03 * s),
          (0.24 * s, 0.22 * s, 0.25 * s, 0, 0.02 * s), (0.16 * s, 0.16 * s, 0.29 * s, 0, 0.02 * s)],
         st, (0, 0, z - 0.03 * s), e)
    for sx in (-1, 1):
        box((0.085 * s, 0.05 * s, 0.24 * s), (sx * 0.125 * s, -0.065 * s, z - 0.10 * s), st,
            parent=e)
    box((0.18 * s, 0.03 * s, 0.035 * s), (0, -0.085 * s, z + 0.19 * s), gold(), parent=e)
    box((0.03 * s, 0.04 * s, 0.07 * s), (0, -0.10 * s, z + 0.22 * s), gold(), parent=e)
    box((0.045 * s, 0.04 * s, 0.11 * s), (0, -0.085 * s, z - 0.05 * s), M("blue"), parent=e)
    for sx in (-1, 1):
        box((0.035 * s, 0.01, 0.022 * s), (sx * 0.045 * s, -0.087 * s, z + 0.125 * s), M("dark"),
            parent=e)


def seated_statue(root, loc, s=1.0, rot_z=0.0, stone="stone_lt"):
    """A seated king on a throne, facing -Y; about 1.05*s tall."""
    e = _sub(root, loc, rot_z, s, "statue")
    st = M(stone)
    box((0.34, 0.50, 0.08), (0, 0, 0.04), brickmat("stone", 0.2, 0.08), parent=e)
    box((0.30, 0.46, 0.02), (0, 0, 0.09), M("blue"), parent=e)
    box((0.26, 0.22, 0.40), (0, 0.10, 0.30), st, parent=e, bevel=0.01)       # throne
    box((0.262, 0.10, 0.12), (0, 0.16, 0.24), M("red"), parent=e)
    box((0.264, 0.10, 0.03), (0, 0.16, 0.32), M("yellow"), parent=e)
    box((0.24, 0.30, 0.10), (0, -0.04, 0.43), M("white"), parent=e, bevel=0.01)  # kilt/lap
    for sx in (-1, 1):
        box((0.085, 0.09, 0.30), (sx * 0.065, -0.16, 0.25), M("skin"), parent=e, bevel=0.01)
        box((0.085, 0.15, 0.04), (sx * 0.065, -0.19, 0.12), M("skin"), parent=e)
        box((0.06, 0.24, 0.06), (sx * 0.135, -0.05, 0.50), M("skin"), parent=e, bevel=0.01)
        box((0.065, 0.08, 0.26), (sx * 0.14, 0.07, 0.62), M("skin"), parent=e, bevel=0.01)
    box((0.24, 0.13, 0.30), (0, 0.07, 0.62), M("skin"), parent=e, bevel=0.015)
    box((0.25, 0.135, 0.05), (0, 0.068, 0.74), M("yellow"), parent=e)
    box((0.252, 0.137, 0.02), (0, 0.068, 0.72), M("blue"), parent=e)
    nemes_head(e, 0.80, 0.9)
    return e


# --- props ----------------------------------------------------------------

def _obelisk(root, x, y, h, w, z=0.0, seed=0, base=True):
    g = brickmat("granite", 0.9, 0.9, 0.03, 0.9, rough=0.45, blotch=0.14)
    if base:
        box((w * 2.3, w * 2.3, 0.07), (x, y, z + 0.035), brickmat("stone", 0.2, 0.07), parent=root)
        box((w * 1.75, w * 1.75, 0.08), (x, y, z + 0.11), M("stone_lt"), parent=root)
        box((w * 1.4, w * 1.4, 0.14), (x, y, z + 0.22), M("granite_dk", rough=0.5), parent=root)
        z += 0.29
    hs = h - (z if base else 0) if False else h
    wt = w * 0.68
    loft([(w, w, 0), (wt, wt, hs)], g, (x, y, z), root)
    loft([(wt + 0.006, wt + 0.006, 0), (0, 0, wt * 1.25)], gold(), (x, y, z + hs), root)
    # hieroglyph columns on the two visible faces
    slope = (w - wt) / 2
    for o, u, v in (((x, y - w / 2, z), (1, 0, 0), (0, slope, hs)),
                    ((x + w / 2, y, z), (0, 1, 0), (-slope, 0, hs))):
        p = Paint(o, u, v, root, off=0.004)
        ln = Vector(v).length
        p.rect(-w * 0.26, ln * 0.04, -w * 0.23, ln * 0.97, "yellow")
        p.rect(w * 0.23, ln * 0.04, w * 0.26, ln * 0.97, "yellow")
        glyphs(p, 0, ln * 0.05, ln * 0.96, w * 0.34, seed, 0,
               cols=("yellow", "yellow", (0.35, 0.14, 0.12), "stone_lt", "blue"))
        seed += 1
        p.done()
    return z + hs + wt * 1.25


def obelisk(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """1x1 prop: a red granite obelisk with a gilded tip."""
    r = empty("eg_obelisk", loc, rot_z, parent)
    pad(r, 1, 1)
    box((0.78, 0.78, 0.06), (0, 0, 0.03), brickmat("stone", 0.26, 0.06), parent=r)
    _obelisk(r, 0, 0, 2.5, 0.27, z=0.06, seed=3)
    for sx, sy in ((-1, -1), (1, -1), (1, 1)):
        jar(r, sx * 0.3, sy * 0.3, 0.06, 0.55, "white", "blue")
    return r


def granary(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """1x1 prop: two domed mud silos with ladders."""
    r = empty("eg_granary", loc, rot_z, parent)
    pad(r, 1, 1)
    box((0.92, 0.92, 0.07), (0, 0, 0.035), brickmat("mud", 0.2, 0.07, 0.09), parent=r, bevel=0.01)
    silo(r, -0.17, 0.17, 0.27, 1.05, "white", "y", 0.07, 0)
    silo(r, 0.20, -0.14, 0.22, 0.85, "cream", "y", 0.07, 1)
    ladder(r, (-0.30, -0.36, 0.07), (-0.22, -0.02, 0.86), 0.15)
    ladder(r, (0.42, -0.42, 0.07), (0.30, -0.26, 0.68), 0.13, rungs=5)
    basket(r, -0.02, -0.36, 0.07, 0.8, "wheat")
    jar(r, 0.36, 0.30, 0.07, 0.7, "terracotta", "white")
    box((0.2, 0.14, 0.09), (0.34, 0.12, 0.115), M("linen"), parent=r, bevel=0.03)
    box((0.06, 0.3, 0.02), (-0.4, -0.2, 0.08), M(player), parent=r)
    return r


def sphinx(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """1x2 prop (1 wide in X, 2 long in Y), facing -Y."""
    r = empty("eg_sphinx", loc, rot_z, parent)
    pad(r, 1, 2)
    body = mottle((0.96, 0.78, 0.46), 0.08, 4.0, rough=0.8)
    box((0.80, 1.78, 0.10), (0, 0, 0.05), brickmat("stone", 0.3, 0.10), parent=r)
    box((0.72, 1.70, 0.12), (0, 0, 0.16), M("stone_lt"), parent=r)
    box((0.725, 1.705, 0.035), (0, 0, 0.18), M("blue"), parent=r)
    box((0.727, 1.707, 0.012), (0, 0, 0.145), M("red"), parent=r)
    z = 0.22
    box((0.40, 1.05, 0.34), (0, 0.20, z + 0.19), body, parent=r, bevel=0.07)
    for sx in (-1, 1):
        sphere(0.22, (sx * 0.17, 0.52, z + 0.20), body, scale=(0.7, 1.15, 0.95), parent=r)
        box((0.13, 0.36, 0.10), (sx * 0.24, 0.42, z + 0.05), body, parent=r, bevel=0.03)
        box((0.13, 0.62, 0.12), (sx * 0.15, -0.50, z + 0.06), body, parent=r, bevel=0.035)
        sphere(0.15, (sx * 0.16, -0.20, z + 0.20), body, scale=(0.7, 1.0, 1.0), parent=r)
    box((0.36, 0.32, 0.50), (0, -0.24, z + 0.30), body, parent=r, bevel=0.06)
    lathe([(0.03, 0), (0.03, 0.5)], body, (0.22, 0.60, z + 0.33), r, segs=6, rot=(0.9, 0, 0.5))
    # collar and head
    box((0.34, 0.06, 0.10), (0, -0.39, z + 0.42), stripemat(("blue", "yellow", "red", "yellow"),
                                                             0.05, "h"), parent=r)
    e = _sub(r, (0, -0.30, 0), 0.0, 1.0, "head")
    nemes_head(e, z + 0.60, 1.45)
    # player cloth on the plinth front
    box((0.30, 0.012, 0.16), (0, -0.857, 0.14), M(player), parent=r)
    box((0.30, 0.014, 0.025), (0, -0.858, 0.085), gold(), parent=r)
    return r


# --- the temple -------------------------------------------------------------

def temple(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """3x3: a pylon temple facing -Y."""
    r = empty("eg_temple", loc, rot_z, parent)
    pad(r, 3, 3)
    stone = brickmat("stone", 0.36, 0.15, 0.06, 0.84)
    stone2 = brickmat("stone_lt", 0.36, 0.15, 0.05, 0.86)
    box((2.9, 2.9, 0.06), (0, 0, 0.03), brickmat("stone_lt", 0.3, 0.3, 0.05, 0.85), parent=r)
    z0 = 0.06
    yP, H = -0.60, 2.30
    bw, bd, tw, td = 1.16, 0.60, 1.00, 0.40
    for sx in (-1, 1):
        cx = sx * 0.85
        loft([(bw + 0.05, bd + 0.05, 0), (bw + 0.04, bd + 0.04, 0.12)], brickmat("stone_dk", 0.36, 0.12),
             (cx, yP, z0), r)
        loft([(bw, bd, 0), (tw, td, H)], stone, (cx, yP, z0), r)
        # corner torus rolls
        for ex in (-1, 1):
            for ey in (-1, 1):
                rod((cx + ex * bw / 2, yP + ey * bd / 2, z0), (cx + ex * tw / 2, yP + ey * td / 2, z0 + H),
                    0.022, "yellow", r, segs=6)
        cavetto(r, tw, td, z0 + H, 0.20, 0.075, (cx, yP), 0.10)
        # --- paint on the front (-Y) face
        v = Vector((0, (bd - td) / 2, H))
        ln = v.length
        p = Paint((cx, yP - bd / 2, z0), (1, 0, 0), v, r)
        hw = lambda b: bw / 2 - (bw - tw) / 2 * (b / ln) - 0.03  # noqa: E731
        p.rect(-hw(0.14), 0.14, hw(0.14), 0.22, "blue")
        p.rect(-hw(0.23), 0.23, hw(0.23), 0.26, "yellow")
        p.rect(-hw(0.27), 0.27, hw(0.27), 0.31, "red")
        # frieze under the cornice
        p.rect(-hw(ln), ln - 0.30, hw(ln), ln - 0.03, "white")
        p.rect(-hw(ln), ln - 0.33, hw(ln), ln - 0.30, "red")
        n = 9
        for i in range(n):
            a = (i + 0.5) / n * 2 - 1
            a *= hw(ln) - 0.02
            c = ("blue", "red", "green")[i % 3]
            p.rect(a - 0.032, ln - 0.27, a + 0.032, ln - 0.08, c, 1)
            p.disc(a, ln - 0.075, 0.03, "yellow", 2, segs=8)
        # white panel with the king, facing the gate
        p.rect(-0.36, 0.82, 0.36, 1.88, "white")
        p.rect(-0.36, 0.82, 0.36, 0.86, "blue", 1)
        p.rect(-0.36, 1.84, 0.36, 1.88, "blue", 1)
        figure(p, sx * 0.07, 0.87, 0.80, facing=-sx, crown="blue" if sx < 0 else "red", layer=1)
        glyphs(p, sx * 0.30, 0.90, 1.80, 0.07, 11 + sx, 1)
        glyphs(p, -sx * 0.31, 1.45, 1.80, 0.06, 5 + sx, 1)
        # cartouche
        p.disc(-sx * 0.30, 1.22, 0.05, "yellow", 1, sy=1.9, segs=12)
        p.disc(-sx * 0.30, 1.22, 0.034, "blue", 2, sy=2.2, segs=12)
        p.done()
        # --- the outer end face (+X on the right tower is what shows)
        v2 = Vector((-(bw - tw) / 2 * sx, 0, H))
        p = Paint((cx + sx * bw / 2, yP, z0), (0, sx, 0), v2, r)
        l2 = v2.length
        hd = lambda b: bd / 2 - (bd - td) / 2 * (b / l2) - 0.03  # noqa: E731
        p.rect(-hd(0.14), 0.14, hd(0.14), 0.22, "blue")
        p.rect(-hd(0.23), 0.23, hd(0.23), 0.26, "yellow")
        p.rect(-hd(0.27), 0.27, hd(0.27), 0.31, "red")
        p.rect(-hd(l2), l2 - 0.30, hd(l2), l2 - 0.03, "white")
        p.rect(-hd(l2), l2 - 0.33, hd(l2), l2 - 0.30, "red")
        for i in range(3):
            a = (i - 1) * 0.11
            p.rect(a - 0.032, l2 - 0.27, a + 0.032, l2 - 0.08, ("blue", "red", "green")[i], 1)
        p.rect(-0.14, 0.60, 0.14, 1.80, "white")
        glyphs(p, -0.07, 0.64, 1.76, 0.07, 21, 1)
        glyphs(p, 0.07, 0.64, 1.76, 0.07, 22, 1)
        p.done()
        # flagpoles in front of the tower, standing in stone sockets
        for k, ex in enumerate((-0.40, 0.40)):
            fx, fy = cx + ex, yP - bd / 2 - 0.085
            box((0.14, 0.14, 0.16), (fx, fy, z0 + 0.08), M("stone_lt"), parent=r)
            box((0.10, 0.07, 0.05), (fx, fy + 0.06, z0 + 1.55), M("bronze"), parent=r)
            flagpole(r, fx, fy, 3.25 + 0.0 * k, player, z0 + 0.1, 0.034, (0.62, 0.17),
                     seed=k + sx)
    # the gate between the towers
    gw, gd, gh = 0.62, 0.46, 1.62
    box((gw, gd, gh), (0, yP, z0 + gh / 2), stone2, parent=r)
    yf = yP - gd / 2
    slab(r, "y", yf, 0, z0, 0.40, 1.30, 0.012, "dark")
    for sx in (-1, 1):
        slab(r, "y", yf, sx * 0.245, z0, 0.09, 1.36, 0.05, "white")
        pj = Paint((sx * 0.245, yf - 0.05, z0), (1, 0, 0), (0, 0, 1), r)
        glyphs(pj, 0, 0.1, 1.3, 0.05, 30 + sx, 0)
        pj.done()
    slab(r, "y", yf, 0, z0 + 1.30, 0.62, 0.30, 0.05, "white")
    pl = Paint((0, yf - 0.05, z0 + 1.30), (1, 0, 0), (0, 0, 1), r)
    winged_disc(pl, 0, 0.14, 0.065, 0.28, 0)
    pl.done()
    cavetto(r, gw + 0.02, gd + 0.06, z0 + gh, 0.15, 0.06, (0, yP), 0.08)
    # a door hanging in the player's colour, half drawn
    box((0.40, 0.02, 0.34), (0, yf - 0.01, z0 + 1.13), M(player), parent=r)
    box((0.40, 0.022, 0.035), (0, yf - 0.011, z0 + 0.975), gold(), parent=r)
    box((0.13, 0.02, 0.62), (-0.135, yf - 0.012, z0 + 0.66), M(player), parent=r)
    # steps
    box((0.70, 0.20, 0.03), (0, yf - 0.10, z0 + 0.015), M("stone_lt"), parent=r)

    # statues and obelisks before the gate
    for sx in (-1, 1):
        seated_statue(r, (sx * 0.53, -1.12, z0), 0.80)
        _obelisk(r, sx * 1.20, -1.32, 1.25, 0.125, z0, seed=7 + sx)

    # the court behind: side colonnades and a sanctuary
    y0, y1 = yP + bd / 2 - 0.05, 1.42
    wl = brickmat("stone_lt", 0.3, 0.12, 0.05, 0.86)
    ys = (-0.10, 0.25, 0.60)
    yc, yl = 0.20, 1.02
    for sx in (-1, 1):
        x = sx * 1.25
        box((0.34, y1 - y0, 0.10), (x, (y0 + y1) / 2, z0 + 0.05), wl, parent=r)
        zc = z0 + 0.10
        # screen walls between the columns, painted
        box((0.10, yl, 0.50), (x, yc, zc + 0.25), M("white"), parent=r)
        band(r, 0.10, yl, zc + 0.03, 0.07, "blue", (x, yc))
        band(r, 0.10, yl, zc + 0.11, 0.025, "yellow", (x, yc))
        band(r, 0.10, yl, zc + 0.36, 0.09,
             stripemat(("red", "white", "green", "white", "blue", "white"), 0.10, "h"), (x, yc))
        box((0.14, yl, 0.04), (x, yc, zc + 0.52), M("yellow"), parent=r)
        for i, y in enumerate(ys):
            column(r, x, y, 1.40, 0.12, zc, style=i)
        zt = zc + 1.40
        box((0.28, yl + 0.14, 0.15), (x, yc, zt + 0.075), stone2, parent=r)
        box((0.284, yl + 0.144, 0.05), (x, yc, zt + 0.075), M("red"), parent=r)
        cavetto(r, 0.28, yl + 0.14, zt + 0.15, 0.12, 0.05, (x, yc), 0.09)
    # inner columns across the court front, seen over the walls
    # sanctuary at the back
    sw, sd, sh = 2.2, 0.62, 1.60
    sy = y1 - sd / 2
    loft([(sw, sd, 0), (sw - 0.10, sd - 0.10, sh)], stone, (0, sy, z0), r)
    cavetto(r, sw - 0.10, sd - 0.10, z0 + sh, 0.18, 0.07, (0, sy), 0.10)
    v = Vector((-0.05, 0, sh))
    p = Paint((sw / 2, sy, z0), (0, 1, 0), v, r)
    p.rect(-0.27, 0.12, 0.27, 0.19, "blue")
    p.rect(-0.27, 0.20, 0.27, 0.225, "yellow")
    p.rect(-0.24, 0.42, 0.24, 1.42, "white")
    figure(p, -0.03, 0.46, 0.70, facing=1, crown="god", layer=1)
    glyphs(p, -0.19, 1.0, 1.36, 0.05, 41, 1)
    p.rect(-0.24, 1.38, 0.24, 1.42, "red", 1)
    p.done()
    v = Vector((0, 0.05, sh))
    p = Paint((0, sy - sd / 2, z0), (1, 0, 0), v, r)
    p.rect(-0.9, 0.12, 0.9, 0.19, "blue")
    p.rect(-0.9, 0.20, 0.9, 0.225, "yellow")
    p.rect(-0.85, 0.7, 0.85, 1.40, "white")
    for i in range(5):
        figure(p, -0.62 + i * 0.33, 0.74, 0.50, facing=1 if i < 2 else -1,
               crown=("god", "blue", "white", "god", "red")[i], layer=1, staff=i % 2 == 0)
    p.rect(-0.14, 0.0, 0.14, 0.62, "dark", 1)
    p.done()
    # offerings, a palm at the corner of the court
    jar(r, 1.02, -0.30, z0, 0.8, "white", "blue")
    jar(r, 1.34, -1.05, z0, 0.9, "terracotta", "white")
    basket(r, 1.20, -1.22, z0, 0.9, "flower_red")
    return r


# --- the pyramid ------------------------------------------------------------

def _pyr_faces(root, cx, cy, base, h, z, mats, name="pyr"):
    """A pyramid with one material per face: mats = (-Y, +X, +Y, -X)."""
    b = base / 2
    cs = [(cx - b, cy - b, z), (cx + b, cy - b, z), (cx + b, cy + b, z), (cx - b, cy + b, z)]
    ap = (cx, cy, z + h)
    for i in range(4):
        mesh([cs[i], cs[(i + 1) % 4], ap], [(0, 1, 2)], mats[i], parent=root, name=name)


def pyramid(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """4x4: a cased pyramid with a gilded capstone, its chapel on the -Y side."""
    r = empty("eg_pyramid", loc, rot_z, parent)
    pad(r, 4, 4)
    cx, cy, B, H = -0.30, 0.42, 3.0, 2.15
    z0 = 0.08
    box((3.86, 3.86, 0.05), (0, 0, 0.025), brickmat("sand", 0.5, 0.5, 0.03, 0.9), parent=r)
    box((B + 0.30, B + 0.30, z0), (cx, cy, z0 / 2), brickmat("stone_lt", 0.4, 0.08, 0.05), parent=r)
    warm = brickmat((0.97, 0.925, 0.82), 0.55, 0.17, 0.03, 0.86, rough=0.55, blotch=0.035, msize=0.008)
    cool = brickmat((0.93, 0.94, 1.0), 0.55, 0.17, 0.022, 0.90, rough=0.55, blotch=0.035, msize=0.008)
    back = brickmat((0.97, 0.93, 0.86), 0.55, 0.17, 0.022, 0.90, rough=0.6, blotch=0.035)
    _pyr_faces(r, cx, cy, B, H, z0, (warm, cool, back, back))
    # a course of red granite at the foot
    k = 0.085
    loft([(B + 0.05, B + 0.05, 0), (B * (1 - k) + 0.035, B * (1 - k) + 0.035, H * k)],
         brickmat("granite", 0.5, 0.09, 0.05, 0.85, rough=0.5), (cx, cy, z0), r)
    # the gilded capstone, a hair proud of the casing
    t = 0.80
    cb = B * (1 - t) + 0.03
    loft([(cb, cb, 0), (0, 0, H * (1 - t) + 0.02)],
         mat(PAL["gold"], rough=0.25, metal=0.7, emit=0.55), (cx, cy, z0 + H * t), r)
    loft([(cb + 0.05, cb + 0.05, 0), (cb + 0.02, cb + 0.02, 0.035)], M("blue"),
         (cx, cy, z0 + H * t - 0.03), r)
    # crisp arrises
    b = B / 2
    for ex, ey in ((-1, -1), (1, -1), (1, 1)):
        rod((cx + ex * b, cy + ey * b, z0), (cx, cy, z0 + H), 0.012, (1.0, 0.98, 0.93), r, segs=4)

    # enclosure wall, whitewashed, round-topped
    E, wh, wt = 1.88, 0.26, 0.09
    wm = brickmat("white", 0.3, 0.09, 0.03, 0.9)

    def wall(x0, y0, x1, y1):
        sx, sy = abs(x1 - x0) + wt, abs(y1 - y0) + wt
        if sy > sx:
            sy -= 2 * wt + 0.002
        mx, my = (x0 + x1) / 2, (y0 + y1) / 2
        box((sx, sy, wh), (mx, my, wh / 2), wm, parent=r)
        box((sx + 0.02, sy + 0.02, 0.04), (mx, my, 0.07), M("red"), parent=r)
        if sx > sy:
            cyl(wt / 2, sx - 0.006, (mx, my, wh), M("white"), rot=(0, math.pi / 2, 0), parent=r, segs=10)
        else:
            cyl(wt / 2, sy - 0.006, (mx, my, wh), M("white"), rot=(math.pi / 2, 0, 0), parent=r, segs=10)

    gx = cx  # the causeway gap
    wall(-E, -E, gx - 0.30, -E)
    wall(gx + 0.30, -E, E, -E)
    wall(E, -E, E, E)
    wall(-E, E, E, E)
    wall(-E, -E, -E, E)

    # mortuary chapel against the sunlit face
    cw, cd, ch = 1.10, 0.44, 0.74
    yb = cy - B / 2          # pyramid foot
    ycen = yb - cd / 2 + 0.16
    box((cw + 0.5, cd + 0.62, 0.07), (cx, ycen - 0.18, z0 / 2 + 0.035),
        brickmat("stone_lt", 0.25, 0.07), parent=r)
    zc = 0.105
    loft([(cw, cd, 0), (cw - 0.06, cd - 0.04, ch)], brickmat("white", 0.28, 0.11, 0.035, 0.9),
         (cx, ycen, zc), r)
    band(r, cw - 0.005, cd, zc + 0.02, 0.06, "blue", (cx, ycen))
    band(r, cw - 0.012, cd, zc + 0.085, 0.022, "yellow", (cx, ycen), out=0.004)
    zt = cavetto(r, cw - 0.06, cd - 0.04, zc + ch, 0.13, 0.05, (cx, ycen), 0.09)
    yf = ycen - cd / 2
    p = Paint((cx, yf, zc), (1, 0, 0), (0, 0.02, ch), r)
    p.rect(-0.16, 0, 0.16, 0.60, "red")
    p.rect(-0.12, 0, 0.12, 0.56, "dark", 1)
    winged_disc(p, 0, 0.635, 0.035, 0.2, 1)
    for s in (-1, 1):
        figure(p, s * 0.37, 0.14, 0.40, facing=-s, crown="white" if s < 0 else "god", layer=0)
    p.done()
    p = Paint((cx + cw / 2 - 0.01, ycen, zc), (0, 1, 0), (-0.03, 0, ch), r)
    figure(p, -0.02, 0.14, 0.40, facing=-1, crown="red", layer=0)
    p.done()
    # portico: four painted columns and a slab
    yp = yf - 0.26
    for i, dx in enumerate((-0.42, -0.14, 0.14, 0.42)):
        column(r, cx + dx, yp, 0.70, 0.065, zc, style=i)
    box((cw + 0.02, 0.44, 0.07), (cx, yf - 0.16, zc + 0.735), M("stone_lt"), parent=r)
    box((cw + 0.024, 0.444, 0.03), (cx, yf - 0.16, zc + 0.735), M("red"), parent=r)
    cavetto(r, cw + 0.02, 0.44, zc + 0.77, 0.09, 0.04, (cx, yf - 0.16), 0.08)
    # causeway stub out to the edge
    y_c0, y_c1 = yp - 0.16, -1.97
    box((0.34, y_c0 - y_c1, 0.06), (cx, (y_c0 + y_c1) / 2, 0.06), brickmat("stone_lt", 0.17, 0.17, 0.05),
        parent=r)
    for s in (-1, 1):
        box((0.07, y_c0 - y_c1, 0.22), (cx + s * 0.22, (y_c0 + y_c1) / 2, 0.11), wm, parent=r)
        box((0.075, y_c0 - y_c1, 0.035), (cx + s * 0.22, (y_c0 + y_c1) / 2, 0.14), M("blue"), parent=r)
        # pennant poles at the chapel
        flagpole(r, cx + s * 0.72, yp - 0.02, 1.75, player, zc, 0.024, (0.42, 0.13), seed=s)
        box((0.10, 0.10, 0.10), (cx + s * 0.72, yp - 0.02, zc + 0.03), M("stone_lt"), parent=r)

    # the +X strip: a queen's pyramid, offering tables, a palm
    qx, qy, qb, qh = 1.52, 1.18, 0.62, 0.50
    box((qb + 0.10, qb + 0.10, 0.05), (qx, qy, 0.07), M("stone_lt"), parent=r)
    _pyr_faces(r, qx, qy, qb, qh, 0.095, (warm, cool, back, back))
    loft([(qb * 0.22, qb * 0.22, 0), (0, 0, qh * 0.22)], gold(), (qx, qy, 0.095 + qh * 0.80), r)
    qx, qy, qb, qh = 1.52, 0.42, 0.50, 0.40
    box((qb + 0.10, qb + 0.10, 0.05), (qx, qy, 0.07), M("stone_lt"), parent=r)
    _pyr_faces(r, qx, qy, qb, qh, 0.095, (warm, cool, back, back))
    loft([(qb * 0.22, qb * 0.22, 0), (0, 0, qh * 0.22)], gold(), (qx, qy, 0.095 + qh * 0.80), r)
    palm(r, 1.50, -0.45, 1.25, seed=4)
    palm(r, 1.62, -1.25, 0.95, seed=9, lean=(-0.06, 0.05))
    box((0.26, 0.18, 0.12), (1.30, -1.52, 0.11), M("stone_lt"), parent=r)
    box((0.262, 0.182, 0.03), (1.30, -1.52, 0.15), M("red"), parent=r)
    basket(r, 1.26, -1.52, 0.17, 0.6, "flower_red")
    jar(r, 1.38, -1.52, 0.17, 0.5, "white", "blue")
    jar(r, -1.20, -1.60, 0.05, 0.8, "terracotta", "white")
    jar(r, -1.38, -1.50, 0.05, 0.65, "white", "red")
    # a little sphinx by the causeway
    e = _sub(r, (0.55, -1.52, 0.04), 0.0, 0.42, "sph")
    sphinx(parent=e, player=player)
    return r


# --- town centre ------------------------------------------------------------

def dado(root, sx, sy, z, loc, base="red", line="blue", h=0.16):
    band(root, sx, sy, z, h, base, loc)
    band(root, sx, sy, z + h + 0.012, 0.03, line, loc, out=0.006)


def malqaf(root, x, y, z, w=0.36, d=0.42, h=0.34, c="white", player=None):
    """A wind-catcher: a wedge-lidded scoop on the roof, mouth to -Y."""
    m = M(c)
    vs = [(-w / 2, -d / 2, 0), (w / 2, -d / 2, 0), (w / 2, d / 2, 0), (-w / 2, d / 2, 0),
          (-w / 2, -d / 2, h), (w / 2, -d / 2, h), (w / 2, d / 2, h * 0.25), (-w / 2, d / 2, h * 0.25)]
    mesh(vs, [(0, 3, 7, 4), (1, 5, 6, 2), (2, 6, 7, 3), (4, 7, 6, 5)], m, (x, y, z), parent=root)
    box((w - 0.05, 0.02, h - 0.07), (x, y - d / 2 + 0.03, z + h / 2 - 0.01), M("dark"), parent=root)
    box((w + 0.03, 0.03, 0.035), (x, y - d / 2, z + h), M("red"), parent=root)
    for s in (-1, 1):
        box((0.035, 0.03, h), (x + s * w / 2, y - d / 2, z + h / 2), M("blue"), parent=root)


def town_center(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """3x3: a walled villa / administrative compound."""
    r = empty("eg_town_center", loc, rot_z, parent)
    pad(r, 3, 3)
    white = brickmat("white", 0.26, 0.09, 0.03, 0.90, blotch=0.05)
    mud = brickmat("mud", 0.2, 0.07, 0.08, 0.80)
    # --- compound wall
    E, t = 1.40, 0.11

    def wall(x0, y0, x1, y1, h, m=white, coping="red"):
        sx, sy = abs(x1 - x0) + t, abs(y1 - y0) + t
        if sy > sx:
            sy -= 2 * t + 0.002
        mx, my = (x0 + x1) / 2, (y0 + y1) / 2
        box((sx, sy, h), (mx, my, h / 2), m, parent=r)
        box((sx + 0.015, sy + 0.015, 0.07), (mx, my, 0.035), M("mud_dk"), parent=r)
        box((sx + 0.03, sy + 0.03, 0.04), (mx, my, h + 0.02), M(coping), parent=r)
        box((sx + 0.012, sy + 0.012, 0.035), (mx, my, h - 0.07), M("blue"), parent=r)

    gx = -0.42
    wall(-E, -E, gx - 0.32, -E, 0.40)
    wall(gx + 0.32, -E, E, -E, 0.40)
    wall(E, -E, E, E, 0.40)
    wall(-E, E, E, E, 0.80)
    wall(-E, -E, -E, E, 0.80)
    # gate: two piers, lintel, cornice, a hanging in the player's colour
    for s in (-1, 1):
        loft([(0.24, 0.20, 0), (0.20, 0.17, 1.22)], white, (gx + s * 0.34, -E, 0), r)
        band(r, 0.225, 0.19, 0.07, 0.10, "red", (gx + s * 0.34, -E))
        band(r, 0.215, 0.18, 0.20, 0.03, "blue", (gx + s * 0.34, -E))
        p = Paint((gx + s * 0.34, -E - 0.10, 0), (1, 0, 0), (0, 0.015, 1.22), r)
        glyphs(p, 0, 0.32, 1.12, 0.07, 60 + s)
        p.done()
    box((0.92, 0.19, 0.16), (gx, -E, 1.30), M("white"), parent=r)
    box((0.924, 0.194, 0.05), (gx, -E, 1.30), M("yellow"), parent=r)
    cavetto(r, 0.92, 0.19, 1.38, 0.12, 0.05, (gx, -E), 0.09)
    box((0.44, 0.02, 0.26), (gx, -E - 0.02, 1.09), M(player), parent=r)
    box((0.44, 0.022, 0.03), (gx, -E - 0.021, 0.97), gold(), parent=r)

    # --- the house: ground storey A, upper storey B at the back
    ax, ay, aw, ad, ah = -0.50, 0.42, 1.62, 1.72, 1.12
    box((aw, ad, ah), (ax, ay, ah / 2), white, parent=r)
    dado(r, aw, ad, 0, (ax, ay), "red", "blue", 0.17)
    band(r, aw, ad, ah - 0.13, 0.07, stripemat(("blue", "white", "red", "white", "green", "white"),
                                                0.10, "h"), (ax, ay))
    parapet(r, aw, ad, ah, 0.13, 0.07, "white", (ax, ay), cap="red")
    bw, bd, bh = 1.62, 0.92, 0.98
    by = ay + ad / 2 - bd / 2
    box((bw, bd, bh), (ax, by, ah + bh / 2), white, parent=r)
    band(r, bw, bd, ah + 0.03, 0.09, "yellow", (ax, by))
    band(r, bw, bd, ah + bh - 0.14, 0.07, stripemat(("red", "white", "blue", "white"), 0.10, "h"),
         (ax, by))
    zr = parapet(r, bw, bd, ah + bh, 0.12, 0.07, "white", (ax, by), cap="blue")
    yA, xA = ay - ad / 2, ax + aw / 2          # the two visible wall planes of A
    yB = by - bd / 2
    # doors and windows
    door(r, "y", yA, ax, 0.36, 0.98, "blue", "red")
    for dx in (-0.55, 0.55):
        window(r, "y", yA, ax + dx, 0.68, 0.20, 0.18)
    door(r, "y", yB, ax - 0.35, 0.28, 0.72, "green", "yellow", z=ah + 0.03)
    for dx in (0.15, 0.55):
        window(r, "y", yB, ax + dx, ah + 0.50, 0.18, 0.18)
    for dy in (-0.45, 0.25):
        window(r, "x", xA, ay + dy, 0.68, 0.2, 0.18)
    for dy in (-0.2, 0.2):
        window(r, "x", xA, by + dy, ah + 0.5, 0.18, 0.18)
    # painted panel on the +X wall
    p = Paint((xA, ay - 0.1, 0), (0, 1, 0), (0, 0, 1), r)
    p.rect(-0.14, 0.26, 0.14, 0.60, "yellow")
    p.disc(0, 0.43, 0.10, "red", 1)
    p.disc(0, 0.43, 0.06, "yellow", 2)
    p.done()
    # roof terrace: awning in the player's colour, jars, mats
    ty0, ty1 = yA + 0.12, yB - 0.08
    awning(r, ax - 0.05, ty0, ax + 0.68, ty1 - 0.05, ah + 0.02, 0.62, player, lean=0.10)
    box((0.42, 0.30, 0.012), (ax + 0.30, (ty0 + ty1) / 2, ah + 0.04),
        stripemat(("straw", "red", "straw", "green"), 0.06, "h"), parent=r)
    jar(r, ax - 0.62, ty0 + 0.08, ah + 0.03, 0.75, "terracotta", "white")
    jar(r, ax - 0.44, ty0 + 0.04, ah + 0.03, 0.6, "white", "blue")
    basket(r, ax - 0.62, ty1 - 0.14, ah + 0.03, 0.7, (0.9, 0.3, 0.1))
    # wind-catcher and a small roof pavilion on B
    malqaf(r, ax + 0.45, by + 0.05, zr - 0.10, 0.40, 0.46, 0.44)
    jar(r, ax - 0.55, by - 0.25, ah + bh + 0.03, 0.6, "terracotta")
    # columned porch
    py = yA - 0.36
    box((1.02, 0.50, 0.06), (ax, yA - 0.24, 0.03), brickmat("stone_lt", 0.2, 0.06), parent=r)
    for i, dx in enumerate((-0.40, 0.40)):
        column(r, ax + dx, py, 0.92, 0.075, 0.06, style=i * 2)
    box((1.04, 0.50, 0.07), (ax, yA - 0.24, 1.015), M("white"), parent=r)
    box((1.044, 0.504, 0.03), (ax, yA - 0.24, 1.01), M("blue"), parent=r)
    cavetto(r, 1.04, 0.50, 1.05, 0.10, 0.045, (ax, yA - 0.24), 0.085)
    # outside stair up the +X wall to the terrace
    stairs(r, (xA + 0.11, yA + 0.05, 0), (xA + 0.11, yA + 0.95, ah), 0.22, white, 8)
    # --- the yard
    silo(r, 0.92, 0.95, 0.30, 1.0, "white", "y", 0, 0)
    silo(r, 0.98, 0.33, 0.25, 0.84, "cream", "y", 0, 1)
    ladder(r, (0.70, 0.52, 0), (0.86, 0.75, 0.82), 0.14)
    # garden pool with a palm
    box((0.74, 0.50, 0.07), (0.88, -0.88, 0.035), M("stone_lt"), parent=r)
    box((0.62, 0.38, 0.02), (0.88, -0.88, 0.068), mat(PAL["water"], rough=0.15, emit=0.1), parent=r)
    for k, (lx, ly) in enumerate(((0.75, -0.95), (0.98, -0.80), (1.05, -0.97))):
        cyl(0.045, 0.006, (lx, ly, 0.082), M("leaf"), parent=r, segs=8)
        if k < 2:
            sphere(0.025, (lx, ly, 0.095), M((0.98, 0.6, 0.75)), parent=r, segs=8)
    palm(r, 1.14, -0.40, 1.65, seed=2, lean=(0.04, -0.04))
    palm(r, -1.12, -0.42, 1.15, seed=5, lean=(-0.04, 0.02))
    # flag: a tall pole by the gate with a big banner
    box((0.16, 0.16, 0.14), (-1.12, -1.08, 0.07), M("stone_lt"), parent=r)
    flagpole(r, -1.12, -1.08, 3.3, player, 0.05, 0.04, (0.85, 0.50), kind="banner", seed=2,
             bands=[(0.0, 0.36, player), (0.36, 0.5, "gold"), (0.5, 1.0, player)], flag_drop=0.1)
    # jars and baskets
    jar(r, ax + 0.72, yA - 0.14, 0.0, 0.95, "terracotta", "white")
    jar(r, ax + 0.90, yA - 0.26, 0.0, 0.75, "white", "blue")
    basket(r, ax - 0.68, yA - 0.16, 0.0, 0.9, "wheat")
    basket(r, 0.52, -0.42, 0.0, 0.8, (0.9, 0.3, 0.1))
    jar(r, 0.60, 1.20, 0.0, 0.9, "terracotta")
    return r


# --- houses -----------------------------------------------------------------

def _yard_wall(root, x0, y0, x1, y1, h, m, t=0.08, coping=None):
    sx, sy = abs(x1 - x0) + t, abs(y1 - y0) + t
    if sy > sx:
        sy -= 2 * t + 0.002
    mx, my = (x0 + x1) / 2, (y0 + y1) / 2
    box((sx, sy, h), (mx, my, h / 2), m, parent=root)
    if sx > sy:
        cyl(t / 2, sx - 0.006, (mx, my, h), M(coping) if coping else m, rot=(0, math.pi / 2, 0), parent=root,
            segs=8)
    else:
        cyl(t / 2, sy - 0.006, (mx, my, h), M(coping) if coping else m, rot=(math.pi / 2, 0, 0), parent=root,
            segs=8)


def oven(root, x, y, z=0.0):
    lathe([(0.15, 0), (0.15, 0.10), (0.11, 0.20), (0.05, 0.25), (0.05, 0.27), (0.03, 0.27)],
          mottle("mud", 0.1, 5.0), (x, y, z), root, segs=10)
    box((0.08, 0.03, 0.08), (x, y - 0.145, z + 0.06), M("dark"), parent=root)
    sphere(0.03, (x, y - 0.15, z + 0.03), mat((1.0, 0.5, 0.1), emit=2.0), parent=root, segs=6)


def house(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None, variant=0):
    """2x2: a flat-roofed house in a walled yard. variant 0 whitewashed,
    1 mudbrick L-shape, 2 a two-storey ochre-washed town house."""
    r = empty("eg_house%d" % variant, loc, rot_z, parent)
    pad(r, 2, 2)
    v = variant % 3
    if v == 0:
        wm = brickmat("white", 0.24, 0.085, 0.03, 0.90, blotch=0.05)
        ym = brickmat("mud", 0.2, 0.07, 0.08, 0.80)
        ax, ay, aw, ad, ah = -0.22, 0.26, 1.34, 1.26, 1.18
        box((aw, ad, ah), (ax, ay, ah / 2), wm, parent=r)
        dado(r, aw, ad, 0, (ax, ay), "red", "blue", 0.16)
        band(r, aw, ad, ah - 0.10, 0.05, "yellow", (ax, ay))
        parapet(r, aw, ad, ah, 0.12, 0.065, "white", (ax, ay), cap="blue")
        yA, xA = ay - ad / 2, ax + aw / 2
        door(r, "y", yA, ax - 0.25, 0.32, 0.92, "blue", "red")
        window(r, "y", yA, ax + 0.32, 0.72, 0.2, 0.16)
        window(r, "x", xA, ay + 0.35, 0.72, 0.2, 0.16)
        # roof room at the back corner
        bx, by2, bw2, bd2, bh2 = ax - 0.30, ay + 0.28, 0.66, 0.62, 0.62
        box((bw2, bd2, bh2), (bx, by2, ah + bh2 / 2), wm, parent=r)
        band(r, bw2, bd2, ah + bh2 - 0.10, 0.05, "red", (bx, by2))
        parapet(r, bw2, bd2, ah + bh2, 0.07, 0.05, "white", (bx, by2))
        slab(r, "y", by2 - bd2 / 2, bx, ah + 0.03, 0.22, 0.44, 0.02, "dark")
        slab(r, "y", by2 - bd2 / 2, bx, ah + 0.03, 0.30, 0.50, 0.012, "green")
        window(r, "x", bx + bw2 / 2, by2, ah + 0.32, 0.16, 0.14)
        # awning and roof things
        awning(r, ax + 0.10, ay - 0.50, ax + 0.56, ay - 0.02, ah + 0.02, 0.50, player, lean=0.07)
        box((0.36, 0.26, 0.012), (ax + 0.36, ay + 0.36, ah + 0.04),
            stripemat(("straw", "red", "straw", "blue"), 0.05, "h"), parent=r)
        for k in range(5):
            sphere(0.035, (ax + 0.24 + 0.06 * k, ay + 0.36 + 0.03 * (k % 2), ah + 0.06),
                   M((0.85, 0.35, 0.10)), parent=r, segs=6, scale=(1, 1, 0.6))
        jar(r, ax - 0.50, ay - 0.46, ah + 0.03, 0.7, "terracotta", "white")
        jar(r, ax - 0.34, ay - 0.50, ah + 0.03, 0.55, "white", "blue")
        # outside stair on the +X wall
        stairs(r, (xA + 0.10, yA + 0.02, 0), (xA + 0.10, yA + 0.95, ah), 0.20, wm, 8)
        # yard
        _yard_wall(r, -0.92, -0.92, 0.92, -0.92, 0.30, ym)
        _yard_wall(r, 0.92, -0.92, 0.92, 0.55, 0.30, ym)
        box((0.36, 0.10, 0.34), (ax - 0.25, -0.92, 0.17), M("dark"), parent=r)   # gateway gap
        for s in (-1, 1):
            box((0.10, 0.12, 0.46), (ax - 0.25 + s * 0.22, -0.92, 0.23), wm, parent=r)
            box((0.12, 0.14, 0.04), (ax - 0.25 + s * 0.22, -0.92, 0.47), M("blue"), parent=r)
        oven(r, 0.66, -0.60)
        jar(r, 0.35, -0.70, 0, 0.9, "terracotta", "white")
        jar(r, 0.20, -0.56, 0, 0.7, "terracotta")
        basket(r, -0.78, -0.62, 0, 0.8, "wheat")
        box((0.10, 0.012, 0.22), (ax + 0.02, yA - 0.02, 0.62), M(player), parent=r)
    elif v == 1:
        wm = brickmat("mud", 0.2, 0.07, 0.09, 0.78)
        wm2 = brickmat((0.84, 0.64, 0.44), 0.2, 0.07, 0.08, 0.80)
        # back range
        ax, ay, aw, ad, ah = -0.10, 0.42, 1.60, 0.92, 1.10
        box((aw, ad, ah), (ax, ay, ah / 2), wm, parent=r)
        band(r, aw, ad, 0, 0.14, "mud_dk", (ax, ay))
        band(r, aw, ad, 0.42, 0.07, "white", (ax, ay))
        band(r, aw, ad, 0.50, 0.025, "red", (ax, ay))
        parapet(r, aw, ad, ah, 0.11, 0.065, wm, (ax, ay), cap="white")
        # front wing
        fx, fy, fw, fd, fh = -0.52, -0.38, 0.76, 0.78, 0.88
        box((fw, fd, fh), (fx, fy, fh / 2), wm2, parent=r)
        band(r, fw, fd, 0, 0.14, "mud_dk", (fx, fy))
        band(r, fw, fd, 0.42, 0.07, "white", (fx, fy))
        band(r, fw, fd, 0.50, 0.025, "blue", (fx, fy))
        parapet(r, fw, fd, fh, 0.10, 0.06, wm2, (fx, fy), cap="white")
        door(r, "x", fx + fw / 2, fy + 0.02, 0.30, 0.74, "red", "white", "wood")
        window(r, "y", fy - fd / 2, fx, 0.56, 0.22, 0.14)
        door(r, "y", ay - ad / 2, 0.38, 0.30, 0.86, "red", "white", "wood")
        window(r, "x", ax + aw / 2, ay, 0.70, 0.2, 0.15)
        window(r, "y", ay - ad / 2, ax + 0.62, 0.70, 0.14, 0.14, 1)
        # stair from the wing roof to the main roof, ladder from the yard
        stairs(r, (fx + 0.22, fy + 0.10, fh), (fx + 0.22, fy + fd / 2 + 0.06, ah), 0.2, wm, 3)
        ladder(r, (fx + 0.50, fy - 0.60, 0), (fx + 0.30, fy - fd / 2 - 0.02, fh + 0.2), 0.15)
        # roof life
        box((0.50, 0.40, 0.012), (ax + 0.35, ay, ah + 0.04),
            stripemat(("straw", "thatch"), 0.03, "h"), parent=r)
        for k in range(12):
            sphere(0.03, (ax + 0.16 + 0.075 * (k % 6), ay - 0.08 + 0.16 * (k // 6), ah + 0.055),
                   M(((0.90, 0.25, 0.12), (0.95, 0.60, 0.15))[k // 6]), parent=r, segs=6,
                   scale=(1, 1, 0.6))
        awning(r, ax - 0.70, ay - 0.30, ax - 0.20, ay + 0.28, ah + 0.02, 0.46, player, lean=0.08)
        jar(r, fx - 0.2, fy - 0.2, fh + 0.03, 0.7, "terracotta", "white")
        jar(r, fx - 0.2, fy + 0.02, fh + 0.03, 0.55, "white", "red")
        basket(r, fx + 0.14, fy - 0.22, fh + 0.03, 0.7, "wheat")
        # yard
        _yard_wall(r, -0.10, -0.92, 0.92, -0.92, 0.28, wm)
        _yard_wall(r, 0.92, -0.92, 0.92, -0.22, 0.28, wm)
        palm(r, 0.62, -0.50, 1.35, seed=3, lean=(0.05, -0.05))
        oven(r, 0.10, -0.62)
        jar(r, 0.78, -0.10, 0, 0.9, "terracotta", "white")
        basket(r, 0.36, -0.72, 0, 0.8, (0.9, 0.3, 0.1))
        box((0.012, 0.10, 0.22), (fx + fw / 2 + 0.02, fy + 0.28, 0.50), M(player), parent=r)
    else:
        wm = brickmat("ochre_wash", 0.24, 0.085, 0.035, 0.88, blotch=0.06)
        white = brickmat("white", 0.24, 0.085, 0.03, 0.90)
        ym = brickmat("mud", 0.2, 0.07, 0.08, 0.80)
        ax, ay, aw, ad, ah = -0.26, 0.26, 1.16, 1.20, 1.10
        box((aw, ad, ah), (ax, ay, ah / 2), white, parent=r)
        dado(r, aw, ad, 0, (ax, ay), "blue", "red", 0.16)
        band(r, aw, ad, ah - 0.06, 0.06, "red", (ax, ay))
        bh = 0.92
        box((aw - 0.04, ad - 0.04, bh), (ax, ay, ah + bh / 2), wm, parent=r)
        band(r, aw - 0.04, ad - 0.04, ah + bh - 0.15, 0.07,
             stripemat(("green", "white", "red", "white"), 0.09, "h"), (ax, ay))
        zr = parapet(r, aw - 0.04, ad - 0.04, ah + bh, 0.12, 0.065, wm, (ax, ay), cap="green")
        yA, xA = ay - ad / 2, ax + aw / 2
        door(r, "y", yA, ax - 0.18, 0.32, 0.92, "green", "yellow")
        window(r, "y", yA, ax + 0.32, 0.70, 0.18, 0.16)
        for dx in (-0.3, 0.3):
            window(r, "y", yA + 0.02, ax + dx, ah + 0.42, 0.2, 0.22)
        window(r, "x", xA, ay + 0.30, 0.70, 0.18, 0.16)
        for dy in (-0.3, 0.3):
            window(r, "x", xA - 0.02, ay + dy, ah + 0.42, 0.2, 0.22)
        malqaf(r, ax - 0.22, ay + 0.22, zr - 0.10, 0.36, 0.42, 0.40)
        awning(r, ax + 0.08, ay - 0.46, ax + 0.46, ay + 0.0, ah + bh + 0.02, 0.44, player, lean=0.06)
        jar(r, ax + 0.38, ay + 0.40, ah + bh + 0.03, 0.6, "terracotta", "white")
        # lean-to with an awning shop front on +X
        lx, lw, ld, lh = xA + 0.24, 0.48, 0.80, 0.70
        box((lw, ld, lh), (lx, ay + 0.18, lh / 2), ym, parent=r)
        parapet(r, lw, ld, lh, 0.08, 0.05, ym, (lx, ay + 0.18), cap="white")
        stairs(r, (lx, ay + 0.18 - ld / 2 - 0.42, 0), (lx, ay + 0.18 - ld / 2, lh), 0.3, ym, 5)
        ladder(r, (lx + 0.05, ay + 0.0, lh + 0.02), (xA + 0.04, ay + 0.0, ah + bh + 0.14), 0.14, rungs=7)
        basket(r, lx + 0.05, ay + 0.42, lh + 0.03, 0.7, (0.9, 0.3, 0.1))
        _yard_wall(r, -0.92, -0.92, 0.92, -0.92, 0.28, ym)
        _yard_wall(r, 0.92, -0.92, 0.92, -0.25, 0.28, ym)
        box((0.36, 0.10, 0.32), (ax - 0.18, -0.92, 0.16), M("dark"), parent=r)
        for s in (-1, 1):
            box((0.10, 0.12, 0.44), (ax - 0.18 + s * 0.22, -0.92, 0.22), white, parent=r)
            box((0.12, 0.14, 0.04), (ax - 0.18 + s * 0.22, -0.92, 0.45), M("green"), parent=r)
        jar(r, 0.30, -0.66, 0, 1.0, "terracotta", "white")
        jar(r, 0.48, -0.56, 0, 0.75, "white", "blue")
        basket(r, -0.76, -0.66, 0, 0.8, "wheat")
        palm(r, 0.72, -0.62, 1.0, seed=6, lean=(0.03, -0.05))
        box((0.10, 0.012, 0.22), (ax + 0.10, yA - 0.02, 0.62), M(player), parent=r)
    return r


# --- barracks ---------------------------------------------------------------

def shield(root, x, y, z, player, rot=(0, 0, 0), s=1.0, seed=0):
    """A tall round-topped cowhide shield, face to -Y before `rot`."""
    e = empty("shield", (x, y, z), 0.0, root)
    e.rotation_euler = rot
    e.scale = (s, s, s)
    hide = cowhide(seed)
    box((0.26, 0.03, 0.34), (0, 0, 0.17), hide, parent=e)
    cyl(0.13, 0.03, (0, 0, 0.34), hide, rot=(math.pi / 2, 0, 0), parent=e, segs=12)
    box((0.27, 0.036, 0.07), (0, 0, 0.20), M(player), parent=e)
    box((0.27, 0.036, 0.03), (0, 0, 0.015), M(player), parent=e)
    cyl(0.035, 0.05, (0, 0, 0.20), gold(), rot=(math.pi / 2, 0, 0), parent=e, segs=8)
    return e


def spear(root, p0, p1):
    p0, p1 = Vector(p0), Vector(p1)
    rod(p0, p1, 0.012, "wood", root, segs=6)
    d = (p1 - p0).normalized()
    rod(p1, p1 + d * 0.13, 0.026, mat(PAL["bronze"], metal=0.6, rough=0.35, emit=0.1), root,
        r_top=0.0, segs=6)


def wheel(e, x, r=0.22):
    wd = M("wood_dark")
    lathe([(r - 0.03, -0.014), (r, -0.014), (r, 0.014), (r - 0.03, 0.014)], wd, (x, 0, r), e,
          segs=14, rot=(0, math.pi / 2, 0), closed=True)
    for k in range(3):
        a = k * math.pi / 3
        box((0.016, 0.02, 2 * r - 0.03), (x, 0, r), M("yellow_ochre"), rot=(a, 0, 0), parent=e)
    cyl(0.035, 0.05, (x, 0, r), M("bronze"), rot=(0, math.pi / 2, 0), parent=e, segs=8)


def chariot(root, loc, rot_z=0.0, player=RED, s=1.0):
    """A light two-wheeled chariot, pole toward -Y."""
    e = _sub(root, loc, rot_z, s, "chariot")
    rr = 0.22
    for sx in (-1, 1):
        wheel(e, sx * 0.25, rr)
    rod((-0.27, 0, rr), (0.27, 0, rr), 0.016, "wood_dark", e, segs=6)
    box((0.36, 0.30, 0.03), (0, -0.04, rr + 0.03), M("wood"), parent=e)
    # the cab: a curved front rail
    n = 8
    vs, fs = [], []
    for i in range(n + 1):
        a = math.pi * i / n
        px, py = 0.18 * math.cos(a), -0.02 - 0.19 * math.sin(a)
        vs += [(px, py, rr + 0.04), (px, py, rr + 0.34 - 0.10 * abs(math.cos(a)) ** 2)]
    for i in range(n):
        fs.append((2 * i, 2 * i + 2, 2 * i + 3, 2 * i + 1))
    mesh(vs, fs, M(player), parent=e, name="cab")
    vs2 = [(x * 1.03, (y + 0.02) * 1.03 - 0.02, z if k % 2 else z + 0.0) for k, (x, y, z) in enumerate(vs)]
    vs3 = []
    for k in range(0, len(vs2), 2):
        top = vs2[k + 1]
        vs3 += [(top[0], top[1], top[2] - 0.05), top]
    mesh(vs3, fs, gold(), parent=e, name="cabrail")
    rod((0, -0.16, rr + 0.03), (0, -0.78, rr + 0.20), 0.016, "wood", e, segs=6)
    rod((-0.16, -0.78, rr + 0.20), (0.16, -0.78, rr + 0.20), 0.016, "wood_dark", e, segs=6)
    box((0.07, 0.03, 0.26), (0.19, 0.02, rr + 0.22), M("leather"), parent=e, rot=(0.25, 0, 0))
    return e


def barracks(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """2x2: a mudbrick fort yard: gate, chariot shed, shields and spears."""
    r = empty("eg_barracks", loc, rot_z, parent)
    pad(r, 2, 2)
    mud = brickmat("mud", 0.2, 0.07, 0.09, 0.78)
    mud2 = brickmat((0.86, 0.66, 0.46), 0.2, 0.07, 0.08, 0.80)
    white = brickmat("white", 0.24, 0.085, 0.03, 0.9)
    E = 0.90
    # back block: the barrack rooms, battered, with merlons
    bx, by, bw, bd, bh = 0.0, 0.42, 1.84, 0.96, 1.18
    loft([(bw, bd, 0), (bw - 0.10, bd - 0.08, bh)], mud, (bx, by, 0), r)
    band(r, bw - 0.005, bd - 0.005, 0, 0.13, "mud_dk", (bx, by))
    tw, td = bw - 0.10, bd - 0.08
    band(r, tw, td, bh - 0.24, 0.10, "white", (bx, by), out=0.012)
    band(r, tw, td, bh - 0.215, 0.05, player, (bx, by), out=0.016)
    box((tw + 0.06, td + 0.06, 0.05), (bx, by, bh + 0.025), mud2, parent=r)
    zt = bh + 0.05
    box((tw - 0.1, td - 0.1, 0.02), (bx, by, zt + 0.005), mottle("roof", 0.08, 4), parent=r)
    hx, hy = tw / 2 - 0.02, td / 2 - 0.02
    merlons(r, (bx - hx - 0.04, by - hy), (bx + hx + 0.04, by - hy), zt, 8, 0.15, 0.10, 0.08, mud2)
    merlons(r, (bx + hx, by - hy - 0.04), (bx + hx, by + hy + 0.04), zt, 4, 0.15, 0.10, 0.08, mud2)
    merlons(r, (bx - hx - 0.04, by + hy), (bx + hx + 0.04, by + hy), zt, 8, 0.15, 0.10, 0.08, mud2)
    merlons(r, (bx - hx, by - hy - 0.04), (bx - hx, by + hy + 0.04), zt, 4, 0.15, 0.10, 0.08, mud2)
    yB, xB = by - bd / 2, bx + bw / 2
    door(r, "y", yB - 0.0, -0.20, 0.30, 0.78, "red", "white", "wood")
    window(r, "y", yB + 0.01, 0.45, 0.62, 0.12, 0.2, 0)
    for dy in (-0.25, 0.2):
        window(r, "x", xB - 0.02, by + dy, 0.62, 0.12, 0.2, 0)
    # shields and spears along the +X wall
    for k, dy in enumerate((-0.30, -0.02)):
        shield(r, xB + 0.035, by + dy + 0.01, 0.02, player, (0, -0.12, math.pi / 2), 1.0, k + 3)
    for k, dy in enumerate((0.26, 0.33, 0.40)):
        spear(r, (xB + 0.14, by + dy, 0), (xB - 0.0, by + dy + 0.02, 1.05 + 0.04 * k))
    # standard on the roof
    sx_, sy_ = bx + 0.55, by - 0.10
    rod((sx_, sy_, zt), (sx_, sy_, zt + 1.5), 0.028, "cedar", r, r_top=0.02)
    box((0.30, 0.03, 0.035), (sx_, sy_, zt + 1.36), gold(), parent=r, rot=(0, 0, math.pi / 4))
    lathe([(0.0, 0), (0.12, 0.10), (0.13, 0.13), (0.0, 0.13)], gold(), (sx_, sy_, zt + 1.42), r,
          segs=14, scale=(1, 0.2, 1), rot=(0, 0, math.pi / 4))
    pennant(r, (sx_, sy_, zt + 1.34), 0.50, 0.28, player, "banner", seed=1,
            bands=[(0, 0.4, player), (0.4, 0.6, "gold"), (0.6, 1.0, player)])
    # front yard wall with a gate (left) ...
    fh = 0.72
    gx = -0.45
    for (x0, x1) in ((-E, gx - 0.20), (gx + 0.20, 0.02)):
        w = x1 - x0
        box((w, 0.12, fh), ((x0 + x1) / 2, -E + 0.02, fh / 2), mud, parent=r)
        box((w + 0.01, 0.13, 0.12), ((x0 + x1) / 2, -E + 0.02, 0.06), M("mud_dk"), parent=r)
        merlons(r, (x0, -E + 0.02), (x1, -E + 0.02), fh, max(1, round(w / 0.15)), 0.11, 0.07, 0.12, mud2)
    box((0.12, 1.84 - bd - 0.02, fh), (-E + 0.02, (yB - E) / 2, fh / 2), mud, parent=r)
    merlons(r, (-E + 0.02, -E), (-E + 0.02, yB), fh, 5, 0.11, 0.07, 0.12, mud2)
    for s in (-1, 1):
        loft([(0.20, 0.22, 0), (0.17, 0.19, 1.02)], white, (gx + s * 0.27, -E + 0.02, 0), r)
        band(r, 0.19, 0.21, 0.02, 0.10, "red", (gx + s * 0.27, -E + 0.02))
        box((0.21, 0.23, 0.05), (gx + s * 0.27, -E + 0.02, 1.045), M(player), parent=r)
        lathe([(0.07, 0), (0.05, 0.08), (0, 0.12)], gold(), (gx + s * 0.27, -E + 0.02, 1.07), r, segs=4,
              rot=(0, 0, math.pi / 4))
    box((0.50, 0.14, 0.10), (gx, -E + 0.02, 0.92), M("wood"), parent=r)
    box((0.36, 0.03, 0.20), (gx, -E - 0.03, 0.80), M(player), parent=r)
    # shields hung on the front wall
    shield(r, -0.80, -E - 0.055, 0.14, player, (0.06, 0, 0), 0.95, 1)
    shield(r, -0.10, -E - 0.055, 0.14, player, (0.06, 0, 0), 0.95, 2)
    # ... and the open chariot shed (right)
    sx0, sx1, sy0, sy1, sh = 0.10, E + 0.02, -0.52, yB, 0.92
    for px in (sx0 + 0.04, sx1 - 0.04):
        box((0.07, 0.07, sh), (px, sy0 + 0.04, sh / 2), M("wood"), parent=r)
        box((0.11, 0.11, 0.06), (px, sy0 + 0.04, 0.03), M("stone_lt"), parent=r)
    # reed-and-mud roof, sloping to the front
    vs = [(sx0 - 0.05, sy0 - 0.08, sh), (sx1 + 0.05, sy0 - 0.08, sh), (sx1 + 0.05, sy1, sh + 0.12),
          (sx0 - 0.05, sy1, sh + 0.12)]
    vs += [(x, y, z + 0.06) for x, y, z in vs]
    mesh(vs, [(0, 1, 2, 3), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)],
         stripemat(("thatch", "straw", "thatch", (0.70, 0.55, 0.26)), 0.07, "x"), parent=r)
    box((sx1 - sx0 + 0.12, 0.03, 0.08), ((sx0 + sx1) / 2, sy0 - 0.085, sh + 0.03), M(player),
        parent=r)
    box((0.03, sy1 - sy0 + 0.06, 0.08), (sx1 + 0.055, (sy0 + sy1) / 2 - 0.03, sh + 0.08), M(player),
        parent=r, rot=(0.12 / (sy1 - sy0), 0, 0))
    chariot(r, ((sx0 + sx1) / 2, -0.30, 0), 0.0, player, 0.92)
    # low wall on the right of the yard, paved floor
    box((0.10, 0.36, 0.30), (E - 0.03, -0.64, 0.15), mud, parent=r)
    box((0.11, 0.36, 0.04), (E - 0.03, -0.64, 0.31), M("white"), parent=r)
    box((0.86, 0.12, 0.30), (0.50, -E + 0.02, 0.15), mud, parent=r)
    box((0.87, 0.13, 0.04), (0.50, -E + 0.02, 0.31), M("white"), parent=r)
    shield(r, 0.30, -E - 0.055, 0.03, player, (0.10, 0, 0), 0.9, 5)
    shield(r, 0.62, -E - 0.055, 0.03, player, (0.10, 0, 0), 0.9, 6)
    # spear rack in the yard corner by the gate, visible through it; jars
    for k in range(4):
        spear(r, (-0.78 + 0.07 * k, -0.30, 0), (-0.80 + 0.07 * k, -0.08, 1.12))
    jar(r, 0.97 - 0.1, 0.0, 0, 0.8, "terracotta", "white")
    return r


def showcase():
    """All of them, laid out in screen rows (u = screen-right, d = away)."""
    def at(u, d):
        k = math.sqrt(2)
        return ((u - d) / k, (u + d) / k, 0)

    r = empty("eg_showcase")
    town_center(at(-6.8, 5.6), player=BLUE, parent=r)
    temple(at(-1.4, 6.0), parent=r)
    pyramid(at(5.0, 5.6), player=BLUE, parent=r)
    barracks(at(-6.6, -1.0), parent=r)
    house(at(-2.6, -1.0), variant=0, parent=r)
    house(at(1.4, -1.0), variant=1, player=BLUE, parent=r)
    house(at(5.4, -1.0), variant=2, parent=r)
    obelisk(at(-4.6, -6.0), parent=r)
    granary(at(-1.0, -6.0), player=BLUE, parent=r)
    sphinx(at(2.8, -6.0), parent=r)
    return r

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


_names = {}


def _key(prefix, *parts):
    """A material's name for these parameters: the cached one while it exists,
    else the next in sequence. Names follow build order, not the parameters,
    so two builds with different player colours name their materials alike
    (export.py lines the builds up by material name to find the mask)."""
    k = (prefix, repr(parts))
    name = _names.get(k)
    if name is not None and bpy.data.materials.get(name) is not None:
        return name
    if _names and not any(bpy.data.materials.get(n) is not None for n in list(_names.values())[:8]):
        _names.clear()      # a new scene (lib.reset): start the sequence again
    name = "%s_%04d" % (prefix, len(_names))
    _names[k] = name
    return name


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


def temple2(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """2x2: the pylon temple drawn into the game's two-tile footprint: its
    plan taken in to two thirds, its height to 0.85, so the pylons still
    stand well above the houses round it."""
    r = empty("eg_temple2", loc, rot_z, parent)
    e = empty("eg_temple2_s", (0, 0, 0), 0.0, r)
    e.scale = (0.664, 0.664, 0.85)
    temple(player=player, parent=e)
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


# --- more props -------------------------------------------------------------

def sack(root, x, y, z=0.0, s=1.0, c="linen", tie=None):
    lathe([(0.0, 0), (0.10 * s, 0.0), (0.135 * s, 0.07 * s), (0.125 * s, 0.17 * s),
           (0.075 * s, 0.24 * s), (0.03 * s, 0.26 * s), (0.05 * s, 0.30 * s), (0, 0.31 * s)],
          M(c, rough=0.95), (x, y, z), root, segs=10, scale=(1, 0.85, 1))
    if tie:
        lathe([(0.034 * s, 0.25 * s), (0.034 * s, 0.27 * s)], tie, (x, y, z), root, segs=8)


def crate(root, x, y, z=0.0, s=1.0, rz=0.0, fill=None):
    e = _sub(root, (x, y, z), rz, s, "crate")
    box((0.30, 0.30, 0.22), (0, 0, 0.11), M("cedar"), parent=e)
    for t in (-1, 1):
        box((0.025, 0.31, 0.23), (t * 0.14, 0, 0.115), M("wood_dark"), parent=e)
        box((0.31, 0.025, 0.23), (0, t * 0.14, 0.115), M("wood_dark"), parent=e)
    if fill:
        for i in range(5):
            sphere(0.06, (-0.07 + 0.07 * (i % 3), -0.05 + 0.09 * (i // 3), 0.23), M(fill),
                   parent=e, segs=8, scale=(1, 1, 0.7))
    return e


def logs(root, x, y, z=0.0, rows=(4, 3, 2), L=0.9, r=0.055, rz=0.0, c="wood"):
    """A stack of trimmed logs lying along local X."""
    e = _sub(root, (x, y, z), rz, 1.0, "logs")
    ring = M((0.88, 0.70, 0.46))
    for row, n in enumerate(rows):
        for i in range(n):
            yy = (i - (n - 1) / 2) * 2.02 * r
            zz = r + row * r * 1.72
            rod((-L / 2, yy, zz), (L / 2, yy, zz), r, c if (i + row) % 2 else "wood_dark", e, segs=8)
            for sx in (-1, 1):
                cyl(r * 0.86, 0.006, (sx * (L / 2 + 0.002), yy, zz), ring, rot=(0, math.pi / 2, 0),
                    parent=e, segs=8)
    return e


def planks(root, x, y, z=0.0, n=5, L=0.8, w=0.16, rz=0.0):
    e = _sub(root, (x, y, z), rz, 1.0, "planks")
    for i in range(n):
        box((L - 0.03 * (i % 2), w, 0.035), (0.01 * (i % 3), 0.01 * (i % 2), 0.02 + i * 0.037),
            M(("cedar", "wood", (0.70, 0.50, 0.30))[i % 3]), parent=e)
    for sx in (-1, 1):
        box((0.05, w + 0.04, 0.05), (sx * L * 0.35, 0, 0.0), M("wood_dark"), parent=e)
    return e


def blocks(root, x, y, z=0.0, rz=0.0, n=3):
    """Dressed stone blocks, stacked."""
    e = _sub(root, (x, y, z), rz, 1.0, "blocks")
    st = brickmat("stone_lt", 0.9, 0.9, 0.04, 0.9, blotch=0.12)
    lay = [(-0.17, 0, 0), (0.17, 0, 0), (0, 0.02, 1), (-0.17, 0.30, 0), (0.17, 0.30, 0)][:n + 2]
    for dx, dy, lev in lay:
        box((0.32, 0.27, 0.22), (dx, dy, 0.11 + lev * 0.225), st, parent=e, bevel=0.012)
    return e


def gold_heap(root, x, y, z=0.0, s=1.0):
    """A basket of gold rings and nuggets, ingots beside it."""
    e = _sub(root, (x, y, z), 0.0, s, "gold")
    g = gold()
    lathe([(0.12, 0), (0.17, 0.12), (0.18, 0.13), (0.14, 0.13)],
          stripemat(("straw", "thatch", "straw", "red"), 0.035, "z"), (0, 0, 0), e, segs=12)
    rnd = random.Random(3)
    for i in range(9):
        a = i * 2.4
        rr = 0.03 + 0.08 * ((i * 37) % 10) / 10
        sphere(0.045, (rr * math.cos(a), rr * math.sin(a), 0.13 + rnd.uniform(0, 0.04)), g,
               parent=e, segs=8, scale=(1, 1, 0.7))
    for i in range(3):
        box((0.16, 0.07, 0.04), (0.28, -0.05 + 0.08 * i, 0.02), g, parent=e, bevel=0.01)
    box((0.16, 0.07, 0.04), (0.28, 0.03, 0.06), g, parent=e, bevel=0.01)
    return e


def hay(root, x, y, z=0.0, s=1.0, seed=0):
    m = mottle("straw", 0.16, 9.0, rough=1.0)
    blob(0.30 * s, (x, y, z + 0.08 * s), m, scale=(1.2, 1.0, 0.62), parent=root, seed=seed,
         rough=0.10)
    for i in range(6):
        a = i * 1.1 + seed
        rod((x + 0.25 * s * math.cos(a), y + 0.2 * s * math.sin(a), z + 0.02),
            (x + 0.36 * s * math.cos(a + 0.2), y + 0.30 * s * math.sin(a + 0.2), z + 0.01),
            0.006, "straw", root, segs=4)


def trough(root, x, y, z=0.0, L=0.7, rz=0.0):
    e = _sub(root, (x, y, z), rz, 1.0, "trough")
    box((L, 0.24, 0.20), (0, 0, 0.10), brickmat("stone_lt", 0.3, 0.1, 0.05), parent=e, bevel=0.012)
    box((L - 0.07, 0.17, 0.02), (0, 0, 0.18), mat(PAL["water"], rough=0.12, emit=0.08), parent=e)
    box((L + 0.004, 0.244, 0.03), (0, 0, 0.13), M("blue"), parent=e)
    return e


def mat_reed(root, x, y, z, sx, sy, cols=("straw", "thatch"), rz=0.0, period=0.035):
    return box((sx, sy, 0.012), (x, y, z + 0.006), stripemat(cols, period, "x"), parent=root,
               rot=(0, 0, rz))


def bow(root, p, ang=0.0, h=0.62, c="wood_dark", tilt=0.18):
    """A strung bow standing on its tip, in the plane of `ang` (about Z)."""
    e = _sub(root, p, ang, 1.0, "bow")
    pts = []
    for i in range(9):
        t = i / 8
        pts.append(Vector((0.06 * math.sin(math.pi * t) + tilt * t * h * 0.3, 0, t * h)))
    for a, b in zip(pts, pts[1:]):
        rod(a, b, 0.010, c, e, segs=5)
    rod(pts[0], pts[-1], 0.003, "linen", e, segs=3)
    return e


def quiver(root, x, y, z=0.0, c="red", band=None, lean=(0.0, 0.0)):
    top = (x + lean[0], y + lean[1], z + 0.42)
    rod((x, y, z), top, 0.045, c, root, r_top=0.05, segs=8)
    if band is not None:
        rod((x + lean[0] * 0.55, y + lean[1] * 0.55, z + 0.22),
            (x + lean[0] * 0.65, y + lean[1] * 0.65, z + 0.27), 0.052, band, root, segs=8)
    for i in range(4):
        a = i * 1.7
        q = (top[0] + 0.02 * math.cos(a), top[1] + 0.02 * math.sin(a), top[2])
        rod(q, (q[0], q[1], q[2] + 0.10), 0.005, "wood", root, segs=3)
        box((0.012, 0.03, 0.05), (q[0], q[1], q[2] + 0.09), M(("white", "red")[i % 2]), parent=root)


def scrolls(root, x, y, z=0.0, n=5, rz=0.0, L=0.26):
    """Rolled papyri lying in a little pile."""
    e = _sub(root, (x, y, z), rz, 1.0, "scrolls")
    pap = M((0.96, 0.90, 0.70))
    k = 0
    for row, cnt in enumerate((n - n // 2, n // 2)):
        for i in range(cnt):
            yy = (i - (cnt - 1) / 2) * 0.062
            zz = 0.03 + row * 0.052
            rod((-L / 2, yy, zz), (L / 2, yy, zz), 0.029, pap, e, segs=8)
            for sx in (-1, 1):
                cyl(0.031, 0.008, (sx * L / 2, yy, zz), M(("red", "blue", "yellow")[k % 3]),
                    rot=(0, math.pi / 2, 0), parent=e, segs=8)
            k += 1
    return e


def desk(root, x, y, z=0.0, rz=0.0):
    """A scribe's low table with a palette, a pot of reeds and an open papyrus."""
    e = _sub(root, (x, y, z), rz, 1.0, "desk")
    box((0.38, 0.24, 0.03), (0, 0, 0.20), M("cedar"), parent=e)
    for sx in (-1, 1):
        box((0.03, 0.22, 0.19), (sx * 0.16, 0, 0.095), M("wood_dark"), parent=e)
    box((0.22, 0.15, 0.006), (-0.03, 0, 0.218), M((0.97, 0.92, 0.74)), parent=e)
    for i in range(3):
        box((0.16, 0.006, 0.004), (-0.03, -0.04 + 0.03 * i, 0.222), M("dark"), parent=e)
    box((0.04, 0.16, 0.015), (0.14, 0, 0.222), M("white"), parent=e)
    sphere(0.012, (0.14, -0.03, 0.232), M("red"), parent=e, segs=6)
    sphere(0.012, (0.14, 0.02, 0.232), M("dark"), parent=e, segs=6)
    return e


def horse(root, loc, facing=0.0, coat="cedar", cloth=None, pose="stand"):
    import figures as F
    e, b = F._start("horse", loc, facing, root)
    F._quad(b, "horse", pose, coat=coat, dark="hair_black", cloth=cloth)
    b.finish(e)
    return e


def target(root, x, y, z=0.0, player=RED, rz=0.0, s=1.0, arrows=3, seed=0):
    """A round straw butt on a stand, painted face toward -Y before rz."""
    e = _sub(root, (x, y, z), rz, s, "target")
    zc = 0.58
    for sx in (-1, 1):
        rod((sx * 0.20, -0.02, 0), (sx * 0.12, 0.02, zc + 0.20), 0.018, "wood", e, segs=6)
    rod((0, 0.34, 0), (0, 0.05, zc + 0.12), 0.018, "wood_dark", e, segs=6)
    rod((-0.2, 0.0, 0.18), (0.2, 0.0, 0.18), 0.014, "wood_dark", e, segs=5)
    rot = (math.pi / 2, 0, 0)
    cyl(0.30, 0.12, (0, 0.06, zc), mottle("straw", 0.12, 14.0), rot=rot, parent=e, segs=20)
    for i, (rr, c) in enumerate(((0.27, "white"), (0.215, player), (0.15, "white"),
                                 (0.095, "red"), (0.04, "yellow"))):
        cyl(rr, 0.01, (0, -0.002 - 0.004 * i, zc), M(c), rot=rot, parent=e, segs=20)
    rnd = random.Random(seed)
    for i in range(arrows):
        a = rnd.uniform(0, 6.28)
        rr = rnd.uniform(0.03, 0.2)
        px, pz = rr * math.cos(a), zc + rr * math.sin(a)
        rod((px, -0.02, pz), (px + rnd.uniform(-0.05, 0.05), -0.36, pz - 0.05), 0.006, "wood", e, segs=3)
        box((0.008, 0.06, 0.035), (px, -0.33, pz - 0.045), M(("white", "red")[i % 2]), parent=e)
    return e


def reed_roof(root, x0, y0, x1, y1, z0, z1, axis="y", c=("thatch", "straw"), t=0.06, edge=None):
    """A flat-ish mat roof; z0 at y0 (or x0), z1 at the other side."""
    if axis == "y":
        vs = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z1), (x0, y1, z1)]
    else:
        vs = [(x0, y0, z0), (x1, y0, z1), (x1, y1, z1), (x0, y1, z0)]
    vs += [(x, y, z + t) for x, y, z in vs]
    m = stripemat(tuple(c) + ((0.70, 0.55, 0.26),), 0.06, "x" if axis == "y" else "y")
    mesh(vs, [(0, 1, 2, 3), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)],
         m, parent=root, name="reedroof")
    if edge is not None:
        if axis == "y":
            box((x1 - x0 + 0.04, 0.025, 0.07), ((x0 + x1) / 2, y0 - 0.012, z0 + t / 2), M(edge),
                parent=root)
        else:
            box((0.025, y1 - y0 + 0.04, 0.07), (x1 + 0.012, (y0 + y1) / 2, z1 + t / 2), M(edge),
                parent=root)


def post(root, x, y, h, r=0.05, z=0.0, c="wood"):
    """A palm-log post with a cap."""
    rod((x, y, z), (x, y, z + h), r, c, root, r_top=r * 0.9, segs=8)
    box((r * 2.6, r * 2.6, 0.04), (x, y, z + h - 0.02), M("wood_dark"), parent=root)


def hangings(root, x0, x1, y, z, player, n=3, h=0.30, side="y"):
    """Strips of player cloth hanging from a beam (y wall) with gold tassels."""
    for i in range(n):
        u = x0 + (x1 - x0) * (i + 0.5) / n
        w = (x1 - x0) / n * 0.62
        if side == "y":
            box((w, 0.012, h), (u, y, z - h / 2), M(player), parent=root)
            box((w, 0.014, 0.03), (u, y, z - h + 0.03), gold(), parent=root)
        else:
            box((0.012, w, h), (y, u, z - h / 2), M(player), parent=root)
            box((0.014, w, 0.03), (y, u, z - h + 0.03), gold(), parent=root)


def battered(root, sx, sy, h, loc, m, k=0.10):
    """A block whose walls lean in by k on every side."""
    return loft([(sx, sy, 0), (sx - k, sy - k, h)], m, loc, root)


# --- watch tower ---------------------------------------------------------------

def watch_tower(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """1x1: a battered sandstone tower with a linen-shaded lookout."""
    r = empty("eg_watch_tower", loc, rot_z, parent)
    pad(r, 1, 1)
    stone = brickmat("stone", 0.28, 0.10, 0.06, 0.84)
    box((0.94, 0.94, 0.07), (0, 0, 0.035), brickmat("stone_lt", 0.25, 0.07), parent=r, bevel=0.01)
    z0, H, b0, b1 = 0.07, 1.90, 0.78, 0.62
    loft([(b0 + 0.04, b0 + 0.04, 0), (b0 + 0.03, b0 + 0.03, 0.10)], brickmat("stone_dk", 0.28, 0.10),
         (0, 0, z0), r)
    loft([(b0, b0, 0), (b1, b1, H)], stone, (0, 0, z0), r)
    for ex, ey in ((-1, -1), (1, -1), (1, 1)):
        rod((ex * b0 / 2, ey * b0 / 2, z0), (ex * b1 / 2, ey * b1 / 2, z0 + H), 0.02, "yellow", r, segs=6)
    sl = (b0 - b1) / 2
    for o, u, v in (((0, -b0 / 2, z0), (1, 0, 0), (0, sl, H)), ((b0 / 2, 0, z0), (0, 1, 0), (-sl, 0, H))):
        p = Paint(o, u, v, r)
        ln = Vector(v).length
        hw = lambda b: b0 / 2 - sl * (b / ln) - 0.025  # noqa: E731
        p.rect(-hw(0.13), 0.13, hw(0.13), 0.21, "blue")
        p.rect(-hw(0.22), 0.22, hw(0.22), 0.245, "yellow")
        p.rect(-hw(0.25), 0.25, hw(0.25), 0.28, "red")
        p.rect(-hw(ln), ln - 0.24, hw(ln), ln - 0.03, "white")
        p.rect(-hw(ln), ln - 0.27, hw(ln), ln - 0.24, "red")
        for i in range(5):
            a = (i - 2) * 0.115
            p.rect(a - 0.03, ln - 0.21, a + 0.03, ln - 0.06, ("blue", "red", "green")[i % 3], 1)
        # arrow slits
        for b in (1.02, 1.40):
            p.rect(-0.022, b, 0.022, b + 0.20, "dark")
            p.rect(-0.05, b - 0.03, 0.05, b, "stone_lt")
        p.done()
    door(r, "y", -b0 / 2 + 0.01, -0.08, 0.24, 0.70, "blue", "red", z=z0 + 0.10)
    slab(r, "y", -b0 / 2 + 0.03, 0.22, z0 + 0.36, 0.16, 0.42, 0.016, "white")
    pj = Paint((0.22, -b0 / 2 + 0.012, z0 + 0.36), (1, 0, 0), (0, 0, 1), r)
    glyphs(pj, 0, 0.03, 0.39, 0.06, 77, 0)
    pj.done()
    # a shield hung on the +X face
    shield(r, b0 / 2 - 0.02, 0.0, z0 + 0.42, player, (0, -0.1, math.pi / 2), 0.85, 7)
    zt = cavetto(r, b1, b1, z0 + H, 0.15, 0.06, (0, 0), 0.09)
    # the lookout: round-topped merlons, a linen sunshade, a signal brazier
    hx = (b1 + 0.12) / 2 - 0.04
    for (p0, p1) in (((-hx, -hx), (hx, -hx)), ((hx, -hx), (hx, hx)), ((hx, hx), (-hx, hx)),
                     ((-hx, hx), (-hx, -hx))):
        merlons(r, p0, p1, zt, 3, 0.12, 0.10, 0.07, brickmat("stone_lt", 0.2, 0.06))
    awning(r, -0.26, -0.26, 0.26, 0.26, zt, 0.62, player, sag=0.04, lean=0.07)
    lathe([(0.0, 0), (0.07, 0.02), (0.10, 0.09), (0.09, 0.10)], M("bronze", metal=0.6, rough=0.4),
          (0.16, -0.16, zt + 0.0), r, segs=10)
    sphere(0.06, (0.16, -0.16, zt + 0.12), mat((1.0, 0.55, 0.12), emit=3.0), parent=r, segs=8,
           scale=(1, 1, 1.5))
    flagpole(r, -0.24, 0.24, 1.15, player, zt + 0.62, 0.025, (0.50, 0.16), seed=3)
    jar(r, 0.36, -0.40, z0, 0.6, "terracotta", "white")
    return r


# --- archery range ---------------------------------------------------------------

def archery_range(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """2x2: a shooting yard: straw targets before an earth butt, a reed shelter
    with bow racks and quivers, pennants at the shooting line."""
    r = empty("eg_archery_range", loc, rot_z, parent)
    pad(r, 2, 2)
    mud = brickmat("mud", 0.2, 0.07, 0.09, 0.78)
    white = brickmat("white", 0.24, 0.085, 0.03, 0.9)
    # the butt: an earth bank along the back, faced with mudbrick
    loft([(1.80, 0.34, 0, 0.08, 0.76), (1.70, 0.18, 0.46, 0.08, 0.80), (1.62, 0.06, 0.56, 0.08, 0.82)],
         mottle("earth", 0.12, 4.0), (0, 0, 0.02), r)
    box((1.80, 0.10, 0.62), (0.08, 0.92, 0.31), mud, parent=r)
    box((1.82, 0.12, 0.04), (0.08, 0.92, 0.64), M("white"), parent=r)
    # targets in a row before it
    for i, x in enumerate((-0.42, 0.18, 0.72)):
        target(r, x, 0.42, 0.02, player, rz=0.0 if i != 2 else 0.25, s=1.0, arrows=2 + i, seed=i)
    # a copper oxhide ingot on a post, shot through (Amenhotep's target)
    rod((0.80, -0.20, 0), (0.80, -0.20, 0.72), 0.03, "wood_dark", r, segs=6)
    vs = []
    for a, b in ((-1, -1), (-0.6, -0.8), (0.6, -0.8), (1, -1), (0.8, 0), (1, 1), (0.6, 0.8),
                 (-0.6, 0.8), (-1, 1), (-0.8, 0)):
        vs.append((0.80 + a * 0.14, -0.215, 0.72 + b * 0.10))
    mesh(vs + [(x, y + 0.02, z) for x, y, z in vs], [tuple(range(10)), tuple(range(19, 9, -1))] +
         [(i, (i + 1) % 10, 10 + (i + 1) % 10, 10 + i) for i in range(10)],
         mat((0.86, 0.46, 0.24), rough=0.35, metal=0.7), parent=r)
    for k in range(2):
        rod((0.78 + 0.05 * k, -0.20, 0.70 + 0.04 * k), (0.70 + 0.05 * k, -0.52, 0.66 + 0.04 * k),
            0.006, "wood", r, segs=3)
    # the shelter on the left: palm posts, a reed roof, a whitewashed back wall
    x0, x1, y0, y1 = -0.96, -0.40, -0.70, 0.30
    box((0.10, y1 - y0 + 0.04, 1.02), (x0 + 0.05, (y0 + y1) / 2, 0.51), white, parent=r)
    band(r, 0.10, y1 - y0 + 0.04, 0.0, 0.14, "red", (x0 + 0.05, (y0 + y1) / 2))
    band(r, 0.10, y1 - y0 + 0.04, 0.16, 0.03, "blue", (x0 + 0.05, (y0 + y1) / 2))
    for y in (y0 + 0.05, y1 - 0.05):
        post(r, x1, y, 0.98, 0.045)
    reed_roof(r, x0 - 0.02, y0 - 0.04, x1 + 0.08, y1 + 0.04, 1.08, 0.96, axis="x", edge=player)
    # bow rack against the wall
    for y in (y0 + 0.16, y1 - 0.16):
        rod((x0 + 0.13, y, 0), (x0 + 0.13, y, 0.80), 0.016, "wood_dark", r, segs=6)
    rod((x0 + 0.13, y0 + 0.12, 0.78), (x0 + 0.13, y1 - 0.12, 0.78), 0.014, "wood_dark", r, segs=5)
    for i in range(5):
        y = y0 + 0.24 + i * 0.13
        bow(r, (x0 + 0.17, y, 0.08), math.pi / 2, 0.66, ("wood_dark", "cedar")[i % 2], tilt=-0.15)
    for i, y in enumerate((-0.52, -0.32, 0.10)):
        quiver(r, x0 + 0.34, y, 0.0, ("red", "leather", "red")[i], player if i != 1 else "yellow",
               lean=(-0.08, 0.0))
    jar(r, x0 + 0.38, 0.0 - 0.18, 0.0, 0.8, "white", "blue")
    for k in range(5):
        rod((x0 + 0.38 + 0.01 * k, -0.18 + 0.01 * (k % 2), 0.2), (x0 + 0.36 + 0.03 * k, -0.20, 0.52),
            0.005, "wood", r, segs=3)
    # the shooting line: a reed mat, pennants, spent arrows in the sand
    mat_reed(r, -0.05, -0.70, 0.02, 0.90, 0.20, ("straw", "red", "straw", "straw"))
    for x in (-0.30, 0.55):
        box((0.10, 0.10, 0.10), (x, -0.86, 0.05), M("stone_lt"), parent=r)
        flagpole(r, x, -0.86, 1.25, player, 0.10, 0.022, (0.34, 0.12), seed=int(x * 10))
    for k in range(5):
        x, y = -0.2 + 0.2 * k, 0.05 + 0.04 * (k % 3)
        rod((x, y, 0.02), (x - 0.05, y - 0.22, 0.12), 0.005, "wood", r, segs=3)
    quiver(r, 0.95, -0.85, 0.0, "red", player, lean=(-0.05, 0.05))
    return r


# --- stable ----------------------------------------------------------------------

def stable(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """2x2: stalls under a reed roof against a mudbrick wall, horses in a
    railed yard, hay, a stone trough."""
    r = empty("eg_stable", loc, rot_z, parent)
    pad(r, 2, 2)
    mud = brickmat("mud", 0.2, 0.07, 0.09, 0.78)
    white = brickmat("white", 0.24, 0.085, 0.03, 0.9)
    # stall block: back (+Y) and left (-X) walls, a reed roof on palm posts
    bh = 1.15
    box((1.92, 0.12, bh), (0, 0.90, bh / 2), mud, parent=r)
    box((0.12, 1.30, bh), (-0.90, 0.31, bh / 2), mud, parent=r)
    for (sx, sy, x, y) in ((1.92, 0.12, 0, 0.90), (0.12, 1.30, -0.90, 0.31)):
        band(r, sx, sy, 0.0, 0.12, "mud_dk", (x, y))
        band(r, sx, sy, bh - 0.20, 0.07, "white", (x, y))
        band(r, sx, sy, bh - 0.17, 0.025, "red", (x, y), out=0.012)
        box((sx + 0.03, sy + 0.03, 0.04), (x, y, bh + 0.02), M("white"), parent=r)
    # stall partitions and posts
    yf = 0.30
    for x in (-0.84, -0.30, 0.24, 0.78):
        post(r, x, yf, 1.04, 0.045)
    for x in (-0.30, 0.24):
        box((0.05, 0.56, 0.62), (x, 0.60, 0.31), M("wood"), parent=r)
        box((0.06, 0.58, 0.04), (x, 0.60, 0.63), M("wood_dark"), parent=r)
    reed_roof(r, -0.90, yf - 0.10, 0.92, 0.90, 1.02, 1.14, axis="y", edge=player)
    box((1.82, 0.04, 0.10), (0.01, yf - 0.02, 0.98), M("wood_dark"), parent=r)
    # a horse in a stall, head out; mangers
    horse(r, (-0.03, 0.66, 0), -math.pi / 2, coat=(0.93, 0.90, 0.84))
    horse(r, (-0.57, 0.68, 0), -math.pi / 2 - 0.08, coat=(0.36, 0.22, 0.13))
    box((0.36, 0.14, 0.12), (0.51, 0.80, 0.42), M("wood"), parent=r)
    hay(r, 0.51, 0.80, 0.42, 0.45, seed=5)
    hay(r, 0.50, 0.58, 0.0, 0.8, 2)
    # tack on the left wall: harness pegs, saddle cloths in the owner's colour
    for i, y in enumerate((-0.05, 0.12)):
        box((0.012, 0.16, 0.26), (-0.83, y, 0.68), M(player), parent=r)
        box((0.014, 0.16, 0.035), (-0.83, y, 0.56), gold(), parent=r)
    rod((-0.84, -0.40, 0.75), (-0.80, -0.40, 0.75), 0.012, "wood_dark", r, segs=5)
    lathe([(0.07, -0.01), (0.075, 0.01), (0.06, 0.01), (0.055, -0.01)], "leather", (-0.80, -0.40, 0.66),
          r, segs=12, rot=(0, math.pi / 2, 0), closed=True)
    wheel(_sub(r, (-0.80, -0.20, 0.0), math.pi / 2, 1.0, "wheel"), 0.0, 0.22)
    # the yard: horses, trough, rails
    horse(r, (0.12, -0.42, 0), math.pi * 0.98, coat="cedar", cloth=M(player))
    trough(r, 0.45, 0.02, 0.0, 0.62)
    basket(r, -0.55, -0.62, 0.0, 0.9, "straw")
    jar(r, 0.86, 0.10, 0.0, 0.8, "terracotta", "white")
    # rail fence along the front and the right, a gap at the front
    rail = M("wood")
    for (xa, ya, xb, yb) in ((-0.92, -0.92, -0.10, -0.92), (0.30, -0.92, 0.92, -0.92),
                             (0.92, -0.92, 0.92, 0.25)):
        n = max(2, int(round(math.hypot(xb - xa, yb - ya) / 0.38)) + 1)
        for i in range(n):
            t = i / (n - 1)
            rod((xa + (xb - xa) * t, ya + (yb - ya) * t, 0), (xa + (xb - xa) * t, ya + (yb - ya) * t, 0.62),
                0.025, "wood_dark", r, segs=6)
        for z in (0.30, 0.56):
            rod((xa, ya, z), (xb, yb, z), 0.018, rail, r, segs=5)
    # a standard by the gap
    flagpole(r, -0.10, -0.92, 1.6, player, 0.0, 0.026, (0.42, 0.14), seed=5)
    return r


# --- siege workshop ----------------------------------------------------------------

def stone_thrower(root, loc, rz=0.0, player=RED, s=1.0, done=0.6):
    """A torsion stone-thrower on its timber bed, part built, arm toward -Y."""
    e = _sub(root, loc, rz, s, "engine")
    w, wd = M("cedar"), M("wood_dark")
    for sx in (-1, 1):
        box((0.08, 1.0, 0.10), (sx * 0.20, 0.0, 0.25), w, parent=e)
    for y in (-0.44, 0.0, 0.42):
        box((0.48, 0.08, 0.07), (0, y, 0.27), wd, parent=e)
    rod((-0.30, -0.30, 0.18), (0.30, -0.30, 0.18), 0.022, wd, e, segs=6)
    rod((-0.30, 0.30, 0.18), (0.30, 0.30, 0.18), 0.022, wd, e, segs=6)
    _sub_wheel = _sub(e, (0, -0.30, 0.0), 0.0, 1.0, "wh")
    wheel(_sub_wheel, 0.29, 0.18)
    _sub_wheel = _sub(e, (0, 0.30, 0.0), 0.0, 1.0, "wh")
    wheel(_sub_wheel, 0.29, 0.18)
    # uprights and the padded crossbar
    for sx in (-1, 1):
        box((0.07, 0.07, 0.56), (sx * 0.18, -0.02, 0.58), w, parent=e)
        rod((sx * 0.18, 0.12, 0.30), (sx * 0.18, -0.02, 0.70), 0.02, wd, e, segs=5)
    box((0.46, 0.09, 0.08), (0, -0.02, 0.88), w, parent=e)
    box((0.30, 0.12, 0.08), (0, -0.04, 0.95), M(player), parent=e, bevel=0.03)
    # the twisted skein and the arm, resting on the bar
    rod((-0.22, 0.30, 0.34), (0.22, 0.30, 0.34), 0.07, stripemat(("leather", (0.62, 0.42, 0.24)), 0.025,
                                                              "x"), e, segs=10)
    p0, p1 = Vector((0, 0.32, 0.34)), Vector((0, -0.42, 1.25))
    rod(p0, p1, 0.05, wd, e, r_top=0.036, segs=8)
    sphere(0.07, tuple(p1 + Vector((0, 0, 0.07))), M("stone"), parent=e, segs=10)
    lathe([(0.0, -0.04), (0.10, 0.0), (0.11, 0.03), (0.08, 0.03)], "leather", tuple(p1), e, segs=10)
    # unfinished: a frame of ribs with no planks yet at the back
    for sx in (-1, 1):
        rod((sx * 0.20, 0.42, 0.30), (sx * 0.12, 0.50, 0.62), 0.02, w, e, segs=5)
    return e


def siege_workshop(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """2x2: a timber yard: a half-built stone-thrower under a shear-leg hoist,
    log stacks and planks, a mudbrick workroom and a reed shed."""
    r = empty("eg_siege_workshop", loc, rot_z, parent)
    pad(r, 2, 2)
    mud = brickmat("mud", 0.2, 0.07, 0.09, 0.78)
    # workroom at the back left
    ax, ay, aw, ad, ah = -0.52, 0.50, 0.88, 0.90, 1.12
    box((aw, ad, ah), (ax, ay, ah / 2), mud, parent=r)
    band(r, aw, ad, 0, 0.13, "mud_dk", (ax, ay))
    band(r, aw, ad, ah - 0.20, 0.08, "white", (ax, ay))
    band(r, aw, ad, ah - 0.17, 0.025, player, (ax, ay), out=0.014)
    parapet(r, aw, ad, ah, 0.10, 0.06, mud, (ax, ay), cap="white")
    door(r, "y", ay - ad / 2, ax - 0.12, 0.34, 0.86, "wood", "white", "wood_dark")
    window(r, "x", ax + aw / 2, ay + 0.15, 0.66, 0.18, 0.14)
    # tools on the wall: saw, adze, mallet
    slab(r, "x", ax + aw / 2, ay - 0.22, 0.52, 0.32, 0.06, 0.012, "iron")
    slab(r, "x", ax + aw / 2, ay - 0.22, 0.58, 0.32, 0.03, 0.02, "wood")
    rod((ax + aw / 2 + 0.03, ay + 0.38, 0.40), (ax + aw / 2 + 0.03, ay + 0.38, 0.70), 0.016, "wood", r)
    box((0.03, 0.12, 0.05), (ax + aw / 2 + 0.03, ay + 0.43, 0.70), M("bronze"), parent=r)
    # reed shed at the back right over the timber store
    x0, x1, y0, y1 = 0.02, 0.94, 0.30, 0.94
    for x in (x0 + 0.04, x1 - 0.04):
        for y in (y0 + 0.04, y1 - 0.04):
            post(r, x, y, 1.20, 0.045)
    reed_roof(r, x0 - 0.04, y0 - 0.06, x1 + 0.04, y1 + 0.02, 1.18, 1.30, axis="y", edge=player)
    logs(r, 0.48, 0.62, 0.0, (5, 4, 3, 2), L=0.80, r=0.055)
    planks(r, 0.52, 0.42, 0.0, 6, 0.74, 0.20)
    # the engine, and a timber gantry behind it hoisting a beam
    # the engine is the hero: big, side-on, out in the sun at the front, its
    # arm up (no hoist over it)
    stone_thrower(r, (0.16, -0.32, 0), -math.pi / 4, player, 1.5)
    hangings(r, ax - 0.36, ax + 0.30, ay - ad / 2 - 0.02, ah - 0.06, player, 2, 0.30)
    flagpole(r, ax - aw / 2 + 0.1, ay - ad / 2 + 0.1, 0.7, player, ah + 0.1, 0.022, (0.42, 0.14), seed=2)
    for i in range(2):
        lathe([(0.08, -0.025), (0.12, -0.025), (0.12, 0.025), (0.08, 0.025)], "straw",
              (0.20 + 0.03 * i, -0.85, 0.025 + 0.05 * i), r, segs=12, closed=True)
    whl = _sub(r, (0.9, 0.12, 0.0), 0.0, 1.0, "lean")
    whl.rotation_euler = (0.0, -0.32, 0.0)
    wheel(whl, 0.0, 0.18)
    logs(r, -0.62, -0.62, 0.0, (3, 2), L=0.62, r=0.05, rz=0.2)
    return r


# --- academy -------------------------------------------------------------------------

def scribe_statue(root, loc, s=1.0, rot_z=0.0):
    """A seated scribe, cross-legged, papyrus on his lap, facing -Y."""
    e = _sub(root, loc, rot_z, s, "scribe")
    box((0.46, 0.46, 0.26), (0, 0, 0.13), brickmat("stone", 0.2, 0.08), parent=e)
    box((0.47, 0.47, 0.04), (0, 0, 0.22), M("blue"), parent=e)
    box((0.48, 0.48, 0.03), (0, 0, 0.27), M("stone_lt"), parent=e)
    z = 0.285
    box((0.40, 0.28, 0.11), (0, -0.03, z + 0.055), M("white"), parent=e, bevel=0.04)
    for sx in (-1, 1):
        sphere(0.07, (sx * 0.15, -0.10, z + 0.06), M("skin"), parent=e, segs=10, scale=(1.2, 1, 0.8))
    box((0.24, 0.13, 0.012), (0, -0.10, z + 0.12), M((0.98, 0.93, 0.74)), parent=e)
    box((0.24, 0.14, 0.32), (0, 0.05, z + 0.26), M("skin"), parent=e, bevel=0.05)
    box((0.25, 0.145, 0.04), (0, 0.05, z + 0.38), M("yellow"), parent=e)
    box((0.252, 0.147, 0.016), (0, 0.05, z + 0.36), M("blue"), parent=e)
    for sx in (-1, 1):
        rod((sx * 0.13, 0.05, z + 0.36), (sx * 0.13, -0.04, z + 0.20), 0.032, "skin", e, segs=6)
        rod((sx * 0.13, -0.04, z + 0.20), (sx * 0.05, -0.12, z + 0.15), 0.028, "skin", e, segs=6)
    sphere(0.085, (0, 0.04, z + 0.50), M("skin"), parent=e, segs=12, scale=(0.9, 1, 1.05))
    box((0.19, 0.17, 0.13), (0, 0.075, z + 0.52), M("hair_black"), parent=e, bevel=0.04)
    for sx in (-1, 1):
        box((0.03, 0.01, 0.015), (sx * 0.035, -0.04, z + 0.51), M("dark"), parent=e)
    rod((0.06, -0.12, z + 0.13), (0.10, -0.20, z + 0.16), 0.006, "dark", e, segs=3)
    return e


def scroll_rack(root, x, y, z=0.0, rz=0.0, cols=4, rows=4):
    """Pigeonholes full of rolled papyri, open to -Y before rz."""
    e = _sub(root, (x, y, z), rz, 1.0, "rack")
    w, h, d = cols * 0.13 + 0.04, rows * 0.13 + 0.08, 0.22
    wd = M("cedar")
    box((w, d, 0.04), (0, 0, h - 0.02), wd, parent=e)
    box((w, 0.02, h), (0, d / 2, h / 2), M("wood_dark"), parent=e)
    for i in range(cols + 1):
        box((0.025, d, h), (-w / 2 + 0.02 + i * 0.13, 0, h / 2), wd, parent=e)
    for j in range(rows + 1):
        box((w, d, 0.02), (0, 0, 0.04 + j * 0.13), wd, parent=e)
    pap = M((0.96, 0.90, 0.70))
    k = 0
    for i in range(cols):
        for j in range(rows):
            cx = -w / 2 + 0.02 + 0.065 + i * 0.13
            cz = 0.05 + j * 0.13
            for q in range(2 + (i + j) % 2):
                px = cx - 0.03 + 0.03 * q
                cyl(0.024, d - 0.02, (px, -0.01, cz + 0.026 + 0.04 * (q % 2)), pap, rot=(math.pi / 2, 0, 0),
                    parent=e, segs=8)
                cyl(0.025, 0.006, (px, -d / 2 + 0.006, cz + 0.026 + 0.04 * (q % 2)),
                    M(("red", "blue", "yellow", "green")[k % 4]), rot=(math.pi / 2, 0, 0), parent=e, segs=8)
                k += 1
    return e


def academy(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """2x2: a House of Life: a columned hall, a scribes' court with mats, desks
    and papyri under a linen awning, a seated-scribe statue."""
    r = empty("eg_academy", loc, rot_z, parent)
    pad(r, 2, 2, "stone_lt")
    white = brickmat("white", 0.26, 0.09, 0.03, 0.90, blotch=0.05)
    # the hall
    ax, ay, aw, ad, ah = -0.12, 0.52, 1.66, 0.84, 1.30
    box((aw + 0.06, ad + 0.40, 0.08), (ax, ay - 0.18, 0.04), brickmat("stone_lt", 0.3, 0.08), parent=r)
    z0 = 0.08
    box((aw, ad, ah), (ax, ay, z0 + ah / 2), white, parent=r)
    dado(r, aw, ad, z0, (ax, ay), "blue", "yellow", 0.15)
    band(r, aw, ad, z0 + ah - 0.24, 0.20, stripemat(("blue", "white", "red", "white", "green", "white"),
                                                    0.10, "h"), (ax, ay))
    band(r, aw, ad, z0 + ah - 0.27, 0.03, "red", (ax, ay))
    zt = cavetto(r, aw, ad, z0 + ah, 0.15, 0.06, (ax, ay), 0.09)
    yA, xA = ay - ad / 2, ax + aw / 2
    door(r, "y", yA, ax, 0.36, 0.94, "green", "yellow", z=z0)
    for dx in (-0.52, 0.52):
        window(r, "y", yA, ax + dx, z0 + 0.66, 0.20, 0.20)
    p = Paint((xA, ay, z0), (0, 1, 0), (0, 0, 1), r)
    p.rect(-0.30, 0.30, 0.30, 0.95, "white")
    p.rect(-0.30, 0.30, 0.30, 0.33, "blue", 1)
    p.rect(-0.30, 0.92, 0.30, 0.95, "blue", 1)
    figure(p, -0.10, 0.36, 0.52, facing=1, crown="god", layer=1)
    glyphs(p, 0.10, 0.40, 0.90, 0.06, 81, 1)
    glyphs(p, 0.21, 0.40, 0.90, 0.06, 82, 1)
    p.done()
    # portico of lotus columns, banners hung between them
    py = yA - 0.30
    for i, dx in enumerate((-0.62, -0.21, 0.21, 0.62)):
        column(r, ax + dx, py, 1.02, 0.075, z0, style=i)
    box((aw - 0.12, 0.42, 0.08), (ax, yA - 0.20, z0 + 1.06), M("white"), parent=r)
    box((aw - 0.116, 0.424, 0.035), (ax, yA - 0.20, z0 + 1.05), M("red"), parent=r)
    cavetto(r, aw - 0.12, 0.42, z0 + 1.10, 0.10, 0.045, (ax, yA - 0.20), 0.085)
    hangings(r, ax - 0.62, ax - 0.21, py, z0 + 1.02, player, 1, 0.36)
    hangings(r, ax + 0.21, ax + 0.62, py, z0 + 1.02, player, 1, 0.36)
    # roof: a little observatory kiosk and jars
    for sx in (-1, 1):
        for sy in (-1, 1):
            rod((ax + 0.45 + sx * 0.18, ay + sy * 0.18, zt), (ax + 0.45 + sx * 0.18, ay + sy * 0.18, zt + 0.5),
                0.025, "yellow", r, segs=6)
    box((0.46, 0.46, 0.06), (ax + 0.45, ay, zt + 0.53), M("white"), parent=r)
    box((0.464, 0.464, 0.025), (ax + 0.45, ay, zt + 0.52), M("blue"), parent=r)
    jar(r, ax - 0.55, ay + 0.1, zt, 0.6, "white", "blue")
    # the court: reed mats, desks, papyri, a linen awning in the owner's colours
    mat_reed(r, 0.18, -0.55, 0.0, 0.70, 0.56, ("straw", "thatch", "straw", "green"))
    awning(r, -0.08, -0.86, 0.86, -0.26, 0.0, 0.92, player, sag=0.05, lean=0.06)
    desk(r, 0.08, -0.62, 0.012, 0.0)
    desk(r, 0.50, -0.45, 0.012, 0.2)
    scrolls(r, 0.40, -0.80, 0.012, 5, 0.3)
    basket(r, 0.72, -0.72, 0.012, 0.7, (0.96, 0.90, 0.70))
    jar(r, -0.10, -0.38, 0.0, 0.6, "terracotta", "white")
    scroll_rack(r, 0.86, 0.30, 0.0, math.pi / 2, 3, 5)
    scribe_statue(r, (-0.62, -0.60, 0), 1.0)
    palm(r, 0.80, 0.80, 1.55, seed=8, lean=(0.04, -0.02))
    return r


# --- market ---------------------------------------------------------------------------

def balance(root, x, y, z=0.0, s=1.0):
    """The scales: a post, a beam, two pans."""
    e = _sub(root, (x, y, z), 0.0, s, "scales")
    box((0.24, 0.16, 0.06), (0, 0, 0.03), M("stone_lt"), parent=e)
    rod((0, 0, 0.06), (0, 0, 0.86), 0.025, "cedar", e, segs=8)
    box((0.06, 0.06, 0.08), (0, 0, 0.86), gold(), parent=e)
    rod((-0.30, 0, 0.84), (0.30, 0, 0.80), 0.014, "wood_dark", e, segs=6)
    for sx, zz in ((-1, 0.84), (1, 0.80)):
        for a in range(3):
            q = (sx * 0.30 + 0.05 * math.cos(a * 2.1), 0.05 * math.sin(a * 2.1), zz - 0.30)
            rod((sx * 0.30, 0, zz), q, 0.003, "linen", e, segs=3)
        lathe([(0, -0.02), (0.08, 0.0), (0.085, 0.01)], M("bronze", metal=0.7, rough=0.35),
              (sx * 0.30, 0, zz - 0.32), e, segs=10)
    for i in range(3):
        sphere(0.025, (-0.30 + 0.03 * (i - 1), 0, 0.53), gold(), parent=e, segs=6)
    box((0.06, 0.05, 0.05), (0.30, 0, 0.51), M("iron", metal=0.5), parent=e)
    return e


def linen_bolts(root, x, y, z=0.0, rz=0.0):
    e = _sub(root, (x, y, z), rz, 1.0, "bolts")
    for i, (c, yy, zz) in enumerate((("linen", -0.07, 0.05), ("white", 0.07, 0.05), ("blue", 0.0, 0.14),
                                     ((0.95, 0.80, 0.40), 0.14, 0.14))):
        rod((-0.16, yy, zz), (0.16, yy, zz), 0.05, c, e, segs=10)
    return e


def market(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """2x2: stalls under linen awnings, produce on mats, a weighing scale,
    a mudbrick shop and a laden donkey."""
    r = empty("eg_market", loc, rot_z, parent)
    pad(r, 2, 2)
    box((1.86, 1.86, 0.03), (0, 0, 0.03), brickmat("stone_lt", 0.45, 0.45, 0.05, 0.86, blotch=0.06),
        parent=r)
    z0 = 0.045
    mud = brickmat("mud", 0.2, 0.07, 0.09, 0.78)
    white = brickmat("white", 0.24, 0.085, 0.03, 0.9)
    # the shop at the back left
    ax, ay, aw, ad, ah = -0.52, 0.52, 0.86, 0.84, 1.10
    box((aw, ad, ah), (ax, ay, z0 + ah / 2), white, parent=r)
    dado(r, aw, ad, z0, (ax, ay), "red", "blue", 0.15)
    band(r, aw, ad, z0 + ah - 0.10, 0.05, "yellow", (ax, ay))
    zr = parapet(r, aw, ad, z0 + ah, 0.10, 0.06, "white", (ax, ay), cap="blue")
    yA, xA = ay - ad / 2, ax + aw / 2
    door(r, "y", yA, ax - 0.15, 0.30, 0.84, "blue", "red", z=z0)
    window(r, "y", yA, ax + 0.22, z0 + 0.62, 0.18, 0.15)
    window(r, "x", xA, ay, z0 + 0.62, 0.18, 0.15)
    awning(r, ax - 0.40, yA - 0.34, ax + 0.40, yA, z0, 0.82, player, sag=0.03, lean=0.10)
    for i, (jx, jy) in enumerate(((ax - 0.25, ay + 0.15), (ax + 0.05, ay + 0.22), (ax + 0.22, ay - 0.1))):
        jar(r, jx, jy, zr - 0.10, 0.65, ("terracotta", "white", "terracotta")[i], ("white", "blue", None)[i])
    # a stall at the back right: fish on a rack, bread
    sx0, sx1, sy0, sy1 = 0.18, 0.90, 0.30, 0.86
    awning(r, sx0, sy0, sx1, sy1, z0, 0.98, player, sag=0.04, lean=0.10)
    box((0.62, 0.26, 0.04), ((sx0 + sx1) / 2, sy0 + 0.20, z0 + 0.42), M("cedar"), parent=r)
    for x in (sx0 + 0.06, sx1 - 0.06):
        box((0.04, 0.22, 0.42), (x, sy0 + 0.20, z0 + 0.21), M("wood_dark"), parent=r)
    for i in range(5):
        sphere(0.04, (sx0 + 0.12 + 0.11 * i, sy0 + 0.16, z0 + 0.47), M((0.86, 0.62, 0.30)), parent=r,
               segs=8, scale=(1.3, 1, 0.6))
    rod((sx0 + 0.04, sy1 - 0.10, z0 + 0.80), (sx1 - 0.04, sy1 - 0.10, z0 + 0.80), 0.012, "wood", r)
    for i in range(5):
        x = sx0 + 0.14 + 0.12 * i
        sphere(0.04, (x, sy1 - 0.10, z0 + 0.66), mat((0.62, 0.70, 0.76), rough=0.3), parent=r, segs=8,
               scale=(0.45, 0.35, 2.2))
    basket(r, sx1 - 0.10, sy0 - 0.06, z0, 0.8, (0.86, 0.62, 0.30))
    # produce on a mat at the front left, a second awning
    mat_reed(r, -0.48, -0.42, z0, 0.80, 0.62, ("straw", "red", "straw", "thatch"))
    awning(r, -0.90, -0.78, -0.08, -0.12, z0, 0.88, player, sag=0.05, lean=0.08)
    for i, (bx, by, fill) in enumerate(((-0.72, -0.60, "flower_red"), (-0.48, -0.66, (0.42, 0.20, 0.10)),
                                        (-0.24, -0.58, "wheat"), (-0.66, -0.30, (0.30, 0.55, 0.18)),
                                        (-0.40, -0.30, (0.95, 0.55, 0.10)))):
        basket(r, bx, by, z0 + 0.012, 0.85, fill)
    jar(r, -0.18, -0.26, z0, 0.8, "terracotta", "white")
    linen_bolts(r, -0.80, -0.08, z0, 0.3)
    # the scales and a scribe's mat, a donkey with panniers in the owner's colour
    balance(r, 0.30, -0.18, z0, 1.0)
    mat_reed(r, 0.30, -0.18, z0, 0.36, 0.30, ("straw", "thatch"))
    gold_heap(r, 0.08, -0.30, z0 + 0.012, 0.55)
    import figures as F
    F.donkey((0.58, -0.66, z0), math.pi * 0.98, player, parent=r, seed=1, load=True)
    for i in range(3):
        sack(r, 0.85, -0.40 + 0.12 * i, z0, 0.75, ("linen", "wool", "linen")[i], "red")
    return r


# --- storehouse ------------------------------------------------------------------------

def storehouse(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """2x2: domed silos on a platform, a reed shed of sacks, and the yard's
    stacks of timber, dressed stone and gold."""
    r = empty("eg_storehouse", loc, rot_z, parent)
    pad(r, 2, 2)
    mud = brickmat("mud", 0.2, 0.07, 0.09, 0.78)
    # silo platform at the back left
    px, py, pw, pd, ph = -0.46, 0.46, 1.02, 1.02, 0.28
    box((pw, pd, ph), (px, py, ph / 2), mud, parent=r)
    band(r, pw, pd, ph - 0.07, 0.05, "white", (px, py))
    band(r, pw, pd, ph - 0.05, 0.02, "red", (px, py), out=0.012)
    silo(r, px - 0.18, py + 0.20, 0.27, 1.08, "white", "y", ph, 0)
    silo(r, px + 0.24, py + 0.24, 0.22, 0.92, "cream", "y", ph, 1)
    silo(r, px - 0.22, py - 0.24, 0.20, 0.82, "cream", "y", ph, 2)
    stairs(r, (px + 0.30, py - pd / 2 - 0.30, 0), (px + 0.30, py - pd / 2, ph), 0.24, mud, 3)
    ladder(r, (px + 0.10, py - 0.06, ph), (px - 0.08, py + 0.02, ph + 0.98), 0.14)
    flagpole(r, px + 0.42, py - 0.05, 1.45, player, ph, 0.025, (0.42, 0.14), seed=4)
    # reed shed at the back right with sacks
    x0, x1, y0, y1 = 0.18, 0.94, 0.12, 0.94
    for x in (x0 + 0.04, x1 - 0.04):
        for y in (y0 + 0.04, y1 - 0.04):
            post(r, x, y, 1.05, 0.045)
    box((x1 - x0, 0.08, 0.70), ((x0 + x1) / 2, y1 - 0.02, 0.35), mud, parent=r)
    reed_roof(r, x0 - 0.04, y0 - 0.06, x1 + 0.04, y1 + 0.02, 1.02, 1.14, axis="y", edge=player)
    for i in range(7):
        sx_, sy_ = x0 + 0.16 + 0.17 * (i % 4), y1 - 0.20 - 0.18 * (i // 4)
        sack(r, sx_, sy_, 0.0, 1.0, ("linen", "wool", "linen", (0.86, 0.80, 0.62))[i % 4], "red")
    for i in range(3):
        sack(r, x0 + 0.24 + 0.17 * i, y1 - 0.30, 0.25, 0.9, "linen", "red")
    # the yard
    logs(r, -0.40, -0.58, 0.0, (4, 3, 2), L=0.86, r=0.06)
    blocks(r, 0.18, -0.50, 0.0, 0.1)
    gold_heap(r, 0.60, -0.42, 0.0, 1.0)
    crate(r, 0.75, -0.80, 0.0, 0.9, 0.3, "wheat")
    basket(r, 0.22, -0.88, 0.0, 0.8, "wheat")
    jar(r, -0.88, -0.20, 0.0, 0.8, "terracotta", "white")
    return r


# --- farm -------------------------------------------------------------------------------

def _crop_rows(root, x0, x1, y0, y1, kind, z=0.05, axis="x", seed=0):
    """Rows of a crop filling a basin."""
    rnd = random.Random(seed)
    if kind == "wheat":
        cols, hgt, wd, step = ((0.95, 0.78, 0.30), (0.90, 0.70, 0.24)), 0.24, 0.10, 0.12
    elif kind == "green":
        cols, hgt, wd, step = ((0.40, 0.66, 0.22), (0.32, 0.58, 0.18)), 0.20, 0.10, 0.12
    elif kind == "flax":
        cols, hgt, wd, step = ((0.36, 0.60, 0.30), (0.30, 0.52, 0.26)), 0.26, 0.09, 0.11
    else:  # low leafy rows
        cols, hgt, wd, step = ((0.20, 0.50, 0.18), (0.26, 0.56, 0.20)), 0.10, 0.11, 0.14
    span = (y1 - y0) if axis == "x" else (x1 - x0)
    n = max(1, int(span / step))
    for i in range(n):
        c = (i + 0.5) / n
        if axis == "x":
            yy = y0 + (y1 - y0) * c
            secs = [(x1 - x0, wd, 0, 0, 0), (x1 - x0 - 0.02, wd * 0.8, hgt * 0.75, 0, 0),
                    (x1 - x0 - 0.05, wd * 0.25, hgt, 0, 0)]
            loc = ((x0 + x1) / 2, yy, z)
        else:
            xx = x0 + (x1 - x0) * c
            secs = [(wd, y1 - y0, 0, 0, 0), (wd * 0.8, y1 - y0 - 0.02, hgt * 0.75, 0, 0),
                    (wd * 0.25, y1 - y0 - 0.05, hgt, 0, 0)]
            loc = (xx, (y0 + y1) / 2, z)
        loft(secs, mottle(cols[i % 2], 0.18, 14.0 + seed), loc, root)
        if kind == "flax":
            for k in range(6):
                t = (k + rnd.uniform(0.1, 0.9)) / 6
                if axis == "x":
                    p = (x0 + (x1 - x0) * t, yy, z + hgt)
                else:
                    p = (xx, y0 + (y1 - y0) * t, z + hgt)
                sphere(0.022, p, M((0.45, 0.62, 0.95)), parent=root, segs=6, scale=(1, 1, 0.6))
        if kind == "low":
            for k in range(5):
                t = (k + 0.5) / 5
                p = (x0 + (x1 - x0) * t, yy, z + 0.06) if axis == "x" else (xx, y0 + (y1 - y0) * t, z + 0.06)
                sphere(0.06, p, M(cols[(i + k) % 2]), parent=root, segs=8, scale=(1, 1, 0.7))


def shaduf(root, x, y, z=0.0, rz=0.0, player=None, lift=0.35):
    """A well sweep: two uprights, a pole on a pivot, a mud counterweight and a
    bucket on a rope. The bucket end is toward +X before rz."""
    e = _sub(root, (x, y, z), rz, 1.0, "shaduf")
    zp = 0.98
    for sy in (-1, 1):
        rod((0, sy * 0.10, 0), (0, sy * 0.07, zp), 0.035, "wood_dark", e, segs=6)
        box((0.14, 0.12, 0.16), (0, sy * 0.10, 0.08), mottle("mud", 0.1, 5.0), parent=e, bevel=0.03)
    rod((0, -0.12, zp), (0, 0.12, zp), 0.022, "wood", e, segs=6)
    a, b = Vector((-0.46, 0, zp - lift)), Vector((0.42, 0, zp + lift * 1.2))
    rod(a, b, 0.022, "wood", e, segs=6)
    sphere(0.10, tuple(a - Vector((0.02, 0, 0.04))), mottle("mud_dk", 0.1, 6.0), parent=e, segs=10,
           scale=(1, 1, 0.85))
    rod(b, (b.x, 0, 0.22), 0.004, "linen", e, segs=3)
    lathe([(0.05, 0), (0.075, 0.07), (0.07, 0.10)], M("leather"), (b.x, 0, 0.14), e, segs=8)
    if player is not None:
        box((0.012, 0.16, 0.20), (0.02, 0.0, zp - 0.20), M(player), parent=e)
    return e


def farm(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """2x2: an irrigated plot: basins between earth dykes, crops in rows, a
    canal on the +X side with a shaduf, a reed shade-shed in the back corner."""
    r = empty("eg_farm", loc, rot_z, parent)
    earth = mottle((0.50, 0.33, 0.20), 0.12, 3.0)
    box((1.94, 1.94, 0.05), (0, 0, 0.025), earth, parent=r, bevel=0.02)
    dyke = mottle((0.58, 0.40, 0.25), 0.10, 4.0)
    z = 0.05

    def ridge(x0, y0, x1, y1, w=0.10, h=0.07):
        if abs(x1 - x0) > abs(y1 - y0):
            loft([(x1 - x0, w, 0), (x1 - x0, w * 0.4, h)], dyke, ((x0 + x1) / 2, y0, z), r)
        else:
            loft([(w, y1 - y0, 0), (w * 0.4, y1 - y0, h)], dyke, (x0, (y0 + y1) / 2, z), r)

    E = 0.92
    cx0 = 0.64  # the canal runs from here to the +X edge
    ridge(-E, -E, cx0, -E)
    ridge(-E, E, cx0, E)
    ridge(-E, -E, -E, E)
    ridge(cx0, -E, cx0, E, 0.12, 0.09)
    ridge(-E, 0.0, cx0, 0.0)
    ridge(-0.16, -E, -0.16, E)
    # the canal
    box((0.24, 1.86, 0.02), (0.79, 0.0, z + 0.004), mat(PAL["water"], rough=0.12, emit=0.06), parent=r)
    box((0.04, 1.88, 0.07), (0.93, 0.0, z + 0.035), dyke, parent=r)
    for k, (lx, ly) in enumerate(((0.80, 0.55), (0.76, 0.30), (0.84, -0.20))):
        cyl(0.04, 0.005, (lx, ly, z + 0.018), M("leaf"), parent=r, segs=8)
    # little sluice gaps in the dyke into each basin, water in the furrows
    for yy in (-0.45, 0.45):
        box((0.14, 0.10, 0.02), (cx0, yy, z + 0.012), mat(PAL["water"], rough=0.12), parent=r)
    # the crops
    _crop_rows(r, -0.86, -0.22, 0.06, 0.86, "wheat", z, "x", 1)
    _crop_rows(r, -0.10, 0.58, 0.06, 0.86, "flax", z, "y", 2)
    _crop_rows(r, -0.86, -0.22, -0.86, -0.06, "green", z, "x", 3)
    _crop_rows(r, -0.10, 0.58, -0.86, -0.06, "low", z, "y", 4)
    # the shaduf on the canal bank
    shaduf(r, 0.40, -0.62, z, 0.0, player, 0.30)
    # a reed shade at the back left over a mat, baskets and a sickle
    x0, x1, y0, y1 = -0.92, -0.42, 0.42, 0.92
    for xx in (x0 + 0.04, x1 - 0.04):
        for yy in (y0 + 0.04, y1 - 0.04):
            post(r, xx, yy, 0.90, 0.035, z)
    reed_roof(r, x0 - 0.03, y0 - 0.05, x1 + 0.03, y1 + 0.02, z + 0.88, z + 0.98, axis="y", edge=player)
    mat_reed(r, (x0 + x1) / 2, (y0 + y1) / 2, z, 0.44, 0.44, ("straw", "thatch"))
    basket(r, x0 + 0.15, y0 + 0.16, z + 0.012, 0.8, "wheat")
    jar(r, x1 - 0.12, y1 - 0.14, z + 0.012, 0.7, "terracotta", "white")
    lathe([(0.14, -0.01), (0.15, 0.01), (0.13, 0.01), (0.12, -0.01)], "iron", (x1 - 0.12, y0 + 0.10, z + 0.03),
          r, segs=10, closed=True, rot=(0.0, 0.0, 0.0))
    # sheaves at the field corner
    for k, (sx_, sy_) in enumerate(((-0.30, 0.28), (-0.30, 0.45))):
        lathe([(0.0, 0), (0.07, 0.0), (0.05, 0.12), (0.09, 0.30), (0.0, 0.33)], mottle("wheat", 0.1, 10.0),
              (sx_, sy_, z), r, segs=8)
        lathe([(0.055, 0.10), (0.055, 0.13)], "leather", (sx_, sy_, z), r, segs=8)
    return r


# --- dock -------------------------------------------------------------------------------

def boat(root, loc, rz=0.0, L=1.3, w=0.36, h=0.16, rise=0.20, player=RED, sail=True, hull="cedar"):
    """A planked river boat with upturned ends and a painted eye, its bow
    toward +X before rz; the hull bottom at z=0 (the waterline)."""
    e = _sub(root, loc, rz, 1.0, "boat")
    n = 14
    secs = []
    for i in range(n + 1):
        t = -1 + 2 * i / n
        hw = w / 2 * max(0.03, 1 - t * t) ** 0.6
        zb = rise * t ** 4
        zt = h + rise * 1.25 * t ** 4
        secs.append([(t * L / 2, -hw, zt), (t * L / 2, -hw * 0.92, zb + (zt - zb) * 0.35),
                     (t * L / 2, 0.0, zb), (t * L / 2, hw * 0.92, zb + (zt - zb) * 0.35),
                     (t * L / 2, hw, zt)])
    vs = [p for s in secs for p in s]
    fs = []
    for i in range(n):
        for j in range(4):
            a = i * 5 + j
            fs.append((a, a + 1, a + 6, a + 5))
    mesh(vs, fs, stripemat((hull, (0.70, 0.46, 0.26), hull, "red"), 0.028, "z"), parent=e, smooth=True,
         name="hull")
    for side in (0, 4):
        for i in range(n):
            rod(secs[i][side], secs[i + 1][side], 0.014, "yellow" if side == 0 else "wood_dark", e, segs=4)
    # the eye on the bow, both sides
    for sy in (-1, 1):
        pt = secs[n - 2][0 if sy < 0 else 4]
        sphere(0.035, (pt[0] - 0.02, pt[1] * 1.02, pt[2] - 0.05), M("white"), parent=e, segs=8,
               scale=(1.4, 0.3, 1.0))
        sphere(0.018, (pt[0] - 0.015, pt[1] * 1.06, pt[2] - 0.05), M("dark"), parent=e, segs=6,
               scale=(1, 0.3, 1))
    for x in (-0.25, 0.0, 0.25):
        box((0.05, w * 0.85, 0.02), (x * L / 1.3, 0, h * 0.75), M("wood"), parent=e)
    # a cabin of reed mats and a steering oar
    box((0.24, w * 0.7, 0.16), (-0.30 * L / 1.3, 0, h + 0.03), stripemat(("straw", "thatch"), 0.03, "x"),
        parent=e)
    box((0.27, w * 0.76, 0.03), (-0.30 * L / 1.3, 0, h + 0.12), M(player), parent=e)
    rod((-L * 0.42, 0.10, h + 0.25), (-L * 0.56, 0.18, -0.02), 0.014, "wood", e, segs=5)
    if sail:
        zm = 1.25
        rod((0.08, 0, 0.05), (0.08, 0, zm), 0.02, "wood", e, segs=6)
        rod((0.08, -0.34, zm - 0.06), (0.08, 0.34, zm - 0.06), 0.012, "wood", e, segs=5)
        rod((0.08, -0.30, zm - 0.58), (0.08, 0.30, zm - 0.58), 0.010, "wood", e, segs=5)
        cloth(e, [(0.09, -0.33, zm - 0.07), (0.09, 0.33, zm - 0.07), (0.09, 0.30, zm - 0.57),
                  (0.09, -0.30, zm - 0.57)], player, 0.0, stripes=(player, "linen"), period=0.11)
        for sy in (-1, 1):
            rod((0.08, 0, zm), (L * 0.48, sy * 0.02, h + rise * 1.2), 0.003, "linen", e, segs=3)
    return e


def net(root, p0, p1, z0, z1):
    """A fishing net hung to dry between two poles."""
    rod((p0[0], p0[1], z0), (p0[0], p0[1], z1 + 0.05), 0.02, "wood", root, segs=5)
    rod((p1[0], p1[1], z0), (p1[0], p1[1], z1 + 0.05), 0.02, "wood", root, segs=5)
    vs, fs = [], []
    n = 6
    for i in range(n + 1):
        t = i / n
        x, y = p0[0] + (p1[0] - p0[0]) * t, p0[1] + (p1[1] - p0[1]) * t
        sag = 0.06 * math.sin(math.pi * t)
        vs += [(x, y, z1 - sag), (x, y, z0 + 0.10 + 0.05 * math.sin(t * 9))]
    for i in range(n):
        fs.append((2 * i, 2 * i + 2, 2 * i + 3, 2 * i + 1))
    mesh(vs, fs, stripemat(((0.30, 0.26, 0.20), "linen", "linen"), 0.04, "h", rough=0.9), parent=root,
         name="net")


def dock(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """3x3: a timber pier on palm-log piles over the water round an open berth
    with a moored boat; a reed boat-shed on the back, nets, fish, a flag.
    Built to stand wholly on the water (the game draws the water)."""
    r = empty("eg_dock", loc, rot_z, parent)
    zd, td = 0.24, 0.06
    pl_ = ((0.80, 0.58, 0.36), (0.74, 0.52, 0.31), (0.84, 0.63, 0.40), (0.70, 0.49, 0.30), (0.78, 0.56, 0.34))
    deck = stripemat(pl_, 0.075, "x")
    deck2 = stripemat(pl_, 0.075, "y")
    E = 1.46
    bx = 0.78   # half the berth's width
    by = 0.28   # the berth's back
    parts = [(-E, E, by, E, deck), (-E, -bx, -E, by, deck2), (bx, E, -E, by, deck2)]
    piles = set()
    for (x0, x1, y0, y1, m) in parts:
        box((x1 - x0, y1 - y0, td), ((x0 + x1) / 2, (y0 + y1) / 2, zd - td / 2), m, parent=r)
        for (xa, ya, xb, yb) in ((x0, y0, x1, y0), (x1, y0, x1, y1), (x0, y1, x1, y1), (x0, y0, x0, y1)):
            if abs(xb - xa) > 0.01:
                box((xb - xa + 0.04, 0.06, 0.08), ((xa + xb) / 2, ya, zd - 0.05), M("wood_dark"), parent=r)
            else:
                box((0.06, yb - ya + 0.04, 0.08), (xa, (ya + yb) / 2, zd - 0.05), M("wood_dark"), parent=r)
        nx, ny = max(2, int((x1 - x0) / 0.55) + 1), max(2, int((y1 - y0) / 0.55) + 1)
        for i in range(nx):
            for j in range(ny):
                if 0 < i < nx - 1 and 0 < j < ny - 1:
                    continue
                px = x0 + 0.05 + (x1 - x0 - 0.10) * i / (nx - 1)
                py = y0 + 0.05 + (y1 - y0 - 0.10) * j / (ny - 1)
                piles.add((round(px, 3), round(py, 3)))
    for (px, py) in piles:
        rod((px, py, 0.0), (px, py, zd - 0.05), 0.05, (0.50, 0.33, 0.19), r, r_top=0.045, segs=8)
    # mooring posts at the berth's mouth, rope rings
    for sx in (-1, 1):
        for yy in (-E + 0.10, -0.50):
            rod((sx * (bx + 0.07), yy, zd), (sx * (bx + 0.07), yy, zd + 0.30), 0.045, "wood_dark", r, segs=8)
            lathe([(0.05, -0.012), (0.075, 0.0), (0.05, 0.012)], "straw", (sx * (bx + 0.07), yy, zd + 0.20), r,
                  segs=10, closed=True)
    # the boat in its berth, bow toward the open water
    boat(r, (0.0, -0.55, 0.0), -math.pi / 2, 1.40, 0.42, 0.17, 0.22, player)
    for sx in (-1, 1):
        rod((sx * (bx + 0.07), -0.50, zd + 0.22), (sx * 0.17, -0.20, 0.17), 0.005, "straw", r, segs=3)
    # the boat shed on the back of the pier
    x0, x1, y0, y1 = -1.30, 0.60, by + 0.12, E - 0.06
    for xx in (x0 + 0.05, -0.35, x1 - 0.05):
        for yy in (y0 + 0.05, y1 - 0.05):
            post(r, xx, yy, 1.20, 0.05, zd)
    box((x1 - x0, 0.08, 0.80), ((x0 + x1) / 2, y1 - 0.02, zd + 0.40), stripemat(("straw", "thatch"), 0.04, "x"),
        parent=r)
    reed_roof(r, x0 - 0.05, y0 - 0.08, x1 + 0.05, y1 + 0.03, zd + 1.16, zd + 1.30, axis="y", edge=player)
    # a reed skiff being lashed on trestles under it
    for xx in (-0.95, -0.15):
        for sy in (-1, 1):
            rod((xx, 0.85 + sy * 0.12, zd), (xx, 0.85, zd + 0.28), 0.015, "wood_dark", r, segs=5)
    sk = boat(r, (-0.55, 0.85, zd + 0.26), 0.0, 1.05, 0.30, 0.10, 0.18, player, sail=False, hull="straw")
    for k in range(4):
        rod((-1.20 + 0.05 * k, 1.30, zd), (-1.15 + 0.06 * k, 1.20, zd + 0.95), 0.012, "wood", r, segs=4)  # oars
    lathe([(0.10, -0.03), (0.17, -0.03), (0.17, 0.03), (0.10, 0.03)], "straw", (0.30, 0.70, zd + 0.03), r,
          segs=14, closed=True)
    # nets on the right arm, fish baskets and jars
    net(r, (1.10, -1.10), (1.10, -0.10), zd, zd + 0.85)
    for i, (fx, fy) in enumerate(((1.30, -0.70), (1.30, -0.40), (0.98, 0.70), (1.20, 0.95))):
        basket(r, fx, fy, zd, 0.8, (0.62, 0.70, 0.76) if i % 2 == 0 else "wheat")
    jar(r, 1.30, 0.60, zd, 0.8, "terracotta", "white")
    jar(r, 1.32, 0.38, zd, 0.65, "white", "blue")
    crate(r, -1.15, -0.10, zd, 0.9, 0.2, (0.62, 0.70, 0.76))
    sack(r, -1.20, 0.12, zd, 0.8, "linen", "red")
    # a derrick at the end of the left arm, the flag at the end of the right
    rod((-1.12, -1.20, zd), (-1.12, -1.20, zd + 1.45), 0.035, "wood", r, segs=6)
    rod((-1.12, -1.20, zd + 1.15), (-0.62, -1.05, zd + 1.40), 0.025, "wood", r, segs=6)
    rod((-0.62, -1.05, zd + 1.40), (-0.62, -1.05, zd + 0.55), 0.004, "linen", r, segs=3)
    crate(r, -0.62, -1.05, zd + 0.32, 0.8, 0.0)
    box((0.12, 0.12, 0.12), (1.20, -1.30, zd + 0.06), M("stone_lt"), parent=r)
    flagpole(r, 1.20, -1.30, 1.75, player, zd + 0.10, 0.03, (0.60, 0.20), seed=6)
    return r


# --- government centre -------------------------------------------------------------------

def government_centre(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """2x2: a palace, not a temple: no pylons. A broad white palace with a
    deep porch of six papyrus columns across its whole front, and on the
    porch roof the Window of Appearances, a balcony hung with the owner's
    cloth; flags at the palace corners, an awning and wind-catcher on the
    roof, sphinxes on the approach."""
    r = empty("eg_government_centre", loc, rot_z, parent)
    pad(r, 2, 2, "stone_lt")
    white = brickmat("white", 0.26, 0.09, 0.03, 0.90, blotch=0.05)
    box((1.92, 1.92, 0.08), (0, 0, 0.04), brickmat("stone_lt", 0.4, 0.4, 0.05, 0.86), parent=r)
    z0 = 0.08
    # the palace: wide, its top a little lower than a temple's pylons
    ax, ay, aw, ad, ah = 0.0, 0.46, 1.78, 0.92, 1.36
    box((aw, ad, ah), (ax, ay, z0 + ah / 2), white, parent=r)
    dado(r, aw, ad, z0, (ax, ay), "red", "blue", 0.16)
    band(r, aw, ad, z0 + ah - 0.24, 0.2, stripemat(("red", "white", "blue", "white", "green", "white"),
                                                   0.10, "h"), (ax, ay))
    zt = cavetto(r, aw, ad, z0 + ah, 0.14, 0.05, (ax, ay), 0.09)
    yA, xA = ay - ad / 2, ax + aw / 2
    for dx in (-0.62, 0.62):
        window(r, "y", yA, ax + dx, z0 + 1.0, 0.18, 0.18)
    for dy in (0.22, 0.62):
        window(r, "x", xA, dy, z0 + 0.95, 0.16, 0.18)
    # the porch: a raised floor, six columns, a flat roof with a cornice
    py0, py1 = -0.46, yA
    pz = z0 + 0.1
    box((aw, py1 - py0 + 0.02, 0.1), (ax, (py0 + py1) / 2, z0 + 0.05), M("stone_lt"), parent=r)
    for i in range(3):
        hh = 0.1 * (3 - i) / 4
        box((0.5, 0.08, hh), (ax, py0 - 0.04 - i * 0.08, z0 + hh / 2), M("stone_lt"), parent=r)
    ch = 0.78
    for i, x in enumerate((-0.75, -0.45, -0.15, 0.15, 0.45, 0.75)):
        column(r, x, py0 + 0.1, ch, 0.06, pz, style=i % 3, abacus=True)
    zr = pz + ch
    box((aw + 0.02, py1 - py0 + 0.04, 0.08), (ax, (py0 + py1) / 2 + 0.01, zr + 0.04), M("white"), parent=r)
    band(r, aw + 0.02, py1 - py0 + 0.04, zr, 0.06, stripemat(("blue", "yellow", "red", "yellow"), 0.08, "h"),
         (ax, (py0 + py1) / 2 + 0.01))
    slab(r, "y", py0 - 0.01, ax, pz, 0.4, 0.62, 0.01, "dark")
    # the Window of Appearances on the porch roof: a balcony rail hung with
    # the owner's cloth, a little canopy over the king's place
    zb = zr + 0.08
    for x in (-0.8, -0.4, 0.0, 0.4, 0.8):
        box((0.035, 0.035, 0.2), (x, py0 + 0.02, zb + 0.1), M("cedar"), parent=r)
    box((1.66, 0.04, 0.035), (0.0, py0 + 0.02, zb + 0.2), M("gold"), parent=r)
    for x in (-0.6, 0.0, 0.6):
        box((0.36, 0.012, 0.3), (x, py0 - 0.005, zb + 0.04), M(player), parent=r)
        box((0.36, 0.014, 0.03), (x, py0 - 0.007, zb - 0.09), gold(), parent=r)
        box((0.1, 0.014, 0.1), (x, py0 - 0.009, zb + 0.06), gold(), rot=(0, math.pi / 4, 0), parent=r)
    awning(r, -0.3, py0 + 0.12, 0.3, py1 - 0.08, zb, 0.42, player, sag=0.04, lean=0.06)
    # the palace roof: awning and wind-catcher, flags at its front corners
    malqaf(r, ax + 0.5, ay + 0.15, zt - 0.02, 0.34, 0.40, 0.36)
    for sx in (-1, 1):
        flagpole(r, ax + sx * (aw / 2 - 0.08), yA + 0.08, 1.0, player, zt, 0.028, (0.5, 0.16), seed=sx + 3)
    # sphinxes on the approach
    for sx in (-1, 1):
        e = _sub(r, (sx * 0.5, -0.78, z0 - 0.01), 0.0, 0.26, "sph")
        sphinx(parent=e, player=player)
    return r


# --- the wonder: the great pyramid ----------------------------------------------------------

def wonder(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None):
    """5x5: the Great Pyramid, cased white with a gilded capstone, in its
    walled court: a mortuary chapel, a sphinx-lined causeway out through a
    pylon gate between obelisks, queens' pyramids, flags."""
    r = empty("eg_wonder", loc, rot_z, parent)
    pad(r, 5, 5)
    box((4.86, 4.86, 0.05), (0, 0, 0.025), brickmat("sand", 0.6, 0.6, 0.03, 0.9), parent=r)
    cx, cy, B, H = -0.40, 0.42, 3.55, 2.95
    z0 = 0.10
    box((B + 0.36, B + 0.36, z0), (cx, cy, z0 / 2), brickmat("stone_lt", 0.45, 0.10, 0.05), parent=r)
    box((B + 0.38, B + 0.38, 0.03), (cx, cy, z0 - 0.02), M("red"), parent=r)
    warm = brickmat((0.97, 0.925, 0.82), 0.62, 0.20, 0.03, 0.86, rough=0.55, blotch=0.035, msize=0.008)
    cool = brickmat((0.93, 0.94, 1.0), 0.62, 0.20, 0.022, 0.90, rough=0.55, blotch=0.035, msize=0.008)
    back = brickmat((0.97, 0.93, 0.86), 0.62, 0.20, 0.022, 0.90, rough=0.6, blotch=0.035)
    _pyr_faces(r, cx, cy, B, H, z0, (warm, cool, back, back))
    k = 0.075
    loft([(B + 0.05, B + 0.05, 0), (B * (1 - k) + 0.035, B * (1 - k) + 0.035, H * k)],
         brickmat("granite", 0.55, 0.11, 0.05, 0.85, rough=0.5), (cx, cy, z0), r)
    t = 0.80
    cb = B * (1 - t) + 0.03
    loft([(cb, cb, 0), (0, 0, H * (1 - t) + 0.025)],
         mat(PAL["gold"], rough=0.22, metal=0.75, emit=0.65), (cx, cy, z0 + H * t), r)
    loft([(cb + 0.06, cb + 0.06, 0), (cb + 0.025, cb + 0.025, 0.045)], M("blue"),
         (cx, cy, z0 + H * t - 0.04), r)
    b = B / 2
    for ex, ey in ((-1, -1), (1, -1), (1, 1)):
        rod((cx + ex * b, cy + ey * b, z0), (cx, cy, z0 + H), 0.014, (1.0, 0.98, 0.93), r, segs=4)
    # a painted band of the king's names across the sunlit face, low down
    v = Vector((0, b, H))
    ln = v.length
    p = Paint((cx, cy - b, z0), (1, 0, 0), v, r, off=0.008)
    hw = lambda bb: b * (1 - bb / ln) - 0.06  # noqa: E731
    p.rect(-hw(0.40), 0.40, hw(0.40), 0.43, "blue")
    p.rect(-hw(0.44), 0.44, hw(0.44), 0.455, "yellow")
    p.done()

    # the enclosure, whitewashed, round-topped, a pylon gate on the causeway
    E, wh, wt = 2.36, 0.30, 0.10
    wm = brickmat("white", 0.3, 0.09, 0.03, 0.9)

    def wall(x0, y0, x1, y1):
        sx, sy = abs(x1 - x0) + wt, abs(y1 - y0) + wt
        if sy > sx:
            sy -= 2 * wt + 0.002
        mx, my = (x0 + x1) / 2, (y0 + y1) / 2
        box((sx, sy, wh), (mx, my, wh / 2), wm, parent=r)
        box((sx + 0.02, sy + 0.02, 0.05), (mx, my, 0.08), M("red"), parent=r)
        if sx > sy:
            cyl(wt / 2, sx - 0.006, (mx, my, wh), M("white"), rot=(0, math.pi / 2, 0), parent=r, segs=10)
        else:
            cyl(wt / 2, sy - 0.006, (mx, my, wh), M("white"), rot=(math.pi / 2, 0, 0), parent=r, segs=10)

    gx = cx
    wall(-E, -E, gx - 0.50, -E)
    wall(gx + 0.50, -E, E, -E)
    wall(E, -E, E, E)
    wall(-E, E, E, E)
    wall(-E, -E, -E, E)
    # the gap in the wall: two gate piers with the owner's cloth hung between
    for sx in (-1, 1):
        px = gx + sx * 0.50
        box((0.18, 0.18, 0.62), (px, -E, 0.31), brickmat("white", 0.2, 0.08), parent=r)
        box((0.20, 0.20, 0.05), (px, -E, 0.645), M("red"), parent=r)
        lathe([(0.07, 0), (0.05, 0.07), (0.0, 0.12)], gold(), (px, -E, 0.67), r, segs=4, rot=(0, 0, math.pi / 4))
        box((0.012, 0.012, 0.012), (px, -E, 0.0), M("white"), parent=r)
    rod((gx - 0.42, -E - 0.09, 0.56), (gx + 0.42, -E - 0.09, 0.56), 0.008, "wood", r, segs=4)
    hangings(r, gx - 0.40, gx + 0.40, -E - 0.09, 0.56, player, 3, 0.20)

    # the mortuary chapel against the sunlit face
    cw, cd, ch = 1.30, 0.52, 0.86
    yb = cy - B / 2
    ycen = yb - cd / 2 + 0.18
    box((cw + 0.6, cd + 0.72, 0.08), (cx, ycen - 0.20, 0.04 + z0 / 2), brickmat("stone_lt", 0.25, 0.07),
        parent=r)
    zc = 0.13
    loft([(cw, cd, 0), (cw - 0.06, cd - 0.04, ch)], brickmat("white", 0.28, 0.11, 0.035, 0.9),
         (cx, ycen, zc), r)
    band(r, cw - 0.005, cd, zc + 0.02, 0.07, "blue", (cx, ycen))
    band(r, cw - 0.012, cd, zc + 0.10, 0.025, "yellow", (cx, ycen), out=0.004)
    cavetto(r, cw - 0.06, cd - 0.04, zc + ch, 0.14, 0.05, (cx, ycen), 0.09)
    yf = ycen - cd / 2
    p = Paint((cx, yf, zc), (1, 0, 0), (0, 0.02, ch), r)
    p.rect(-0.18, 0, 0.18, 0.70, "red")
    p.rect(-0.13, 0, 0.13, 0.65, "dark", 1)
    winged_disc(p, 0, 0.74, 0.04, 0.24, 1)
    for s in (-1, 1):
        figure(p, s * 0.44, 0.16, 0.46, facing=-s, crown="white" if s < 0 else "god", layer=0)
    p.done()
    yp = yf - 0.30
    for i, dx in enumerate((-0.48, -0.16, 0.16, 0.48)):
        column(r, cx + dx, yp, 0.80, 0.07, zc, style=i)
    box((cw + 0.02, 0.50, 0.08), (cx, yf - 0.19, zc + 0.84), M("stone_lt"), parent=r)
    box((cw + 0.024, 0.504, 0.03), (cx, yf - 0.19, zc + 0.84), M("red"), parent=r)
    cavetto(r, cw + 0.02, 0.50, zc + 0.88, 0.10, 0.045, (cx, yf - 0.19), 0.08)
    for s in (-1, 1):
        flagpole(r, cx + s * 0.84, yp - 0.04, 2.05, player, zc, 0.028, (0.48, 0.15), seed=s)
        box((0.11, 0.11, 0.10), (cx + s * 0.84, yp - 0.04, zc + 0.03), M("stone_lt"), parent=r)

    # the causeway, lined with sphinxes, and two obelisks at its foot
    y_c0, y_c1 = yp - 0.18, -E + 0.16
    box((0.40, y_c0 - y_c1, 0.07), (cx, (y_c0 + y_c1) / 2, 0.065), brickmat("stone_lt", 0.2, 0.2, 0.05),
        parent=r)
    for i, yy in enumerate((-1.05, -1.70)):
        for s in (-1, 1):
            e = _sub(r, (cx + s * 0.40, yy, 0.03), 0.0, 0.30, "sph")
            e.rotation_euler = (0, 0, s * math.pi / 2)
            sphinx(parent=e, player=player)
    for s in (-1, 1):
        _obelisk(r, cx + s * 0.95, -1.95, 1.65, 0.15, 0.03, seed=17 + s)

    # the +X strip: queens' pyramids, offering tables, palms
    for (qx, qy, qb, qh) in ((1.86, 1.55, 0.80, 0.66), (1.86, 0.60, 0.66, 0.54)):
        box((qb + 0.10, qb + 0.10, 0.06), (qx, qy, 0.08), M("stone_lt"), parent=r)
        _pyr_faces(r, qx, qy, qb, qh, 0.11, (warm, cool, back, back))
        loft([(qb * 0.22, qb * 0.22, 0), (0, 0, qh * 0.22)], gold(), (qx, qy, 0.11 + qh * 0.80), r)
    palm(r, 1.80, -0.40, 1.55, seed=4)
    palm(r, 1.95, -1.55, 1.25, seed=9, lean=(-0.06, 0.05))
    palm(r, -2.0, 1.2, 1.35, seed=13, lean=(0.05, -0.04))
    box((0.30, 0.20, 0.13), (1.30, -1.95, 0.115), M("stone_lt"), parent=r)
    box((0.302, 0.202, 0.03), (1.30, -1.95, 0.16), M("red"), parent=r)
    basket(r, 1.25, -1.95, 0.18, 0.6, "flower_red")
    jar(r, 1.38, -1.95, 0.18, 0.5, "white", "blue")
    jar(r, -1.60, -2.05, 0.05, 0.8, "terracotta", "white")
    jar(r, -1.78, -1.95, 0.05, 0.65, "white", "red")
    return r


# --- walls and the gate ----------------------------------------------------------------------
#
# The game draws a wall tile as its post plus an arm toward each wall beside
# it (crates/view/src/walls.rs): eight arm frames in WALL_DIRECTIONS order,
# each anchored on the post. The game's tile +x is Blender's +X, its +y is
# Blender's -Y (screen right-down and left-down). An arm reaches the tile's
# edge (or its corner, for a diagonal), where the neighbour's arm meets it.
# The gate stands across its walls' line, one of four orientations (the first
# four directions), shut or open.

WALL_DIRECTIONS = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)]


def wall_dir(k):
    """Direction k as a unit vector in Blender XY, its angle, and the reach
    of an arm along it (to the edge, or the corner)."""
    dx, dy = WALL_DIRECTIONS[k]
    x, y = float(dx), float(-dy)
    n = math.hypot(x, y)
    return (x / n, y / n), math.atan2(y, x), 0.5 * n


def _along(u, s, v=0.0):
    return (u[0] * s - u[1] * v, u[1] * s + u[0] * v)


def arm_plan(k, pier, width, reach=None):
    """The plan of an arm `width` wide from a square pier of half-size `pier`
    to the edge; a diagonal one's inner end follows the pier's two faces."""
    u, _, edge = wall_dir(k)
    reach = edge if reach is None else reach
    h = width / 2
    dx, dy = WALL_DIRECTIONS[k]
    if dx and dy:
        c = pier * math.sqrt(2)
        pts = [(c - h, -h), (reach, -h), (reach, h), (c - h, h), (c, 0.0)]
    else:
        pts = [(pier, -h), (reach, -h), (reach, h), (pier, h)]
    return [_along(u, s, v) for s, v in pts]


def plan_prism(root, pts, z0, h, m, name="plan"):
    n = len(pts)
    vs = [(x, y, z0) for x, y in pts] + [(x, y, z0 + h) for x, y in pts]
    fs = [tuple(range(n))[::-1], tuple(range(n, 2 * n))]
    fs += [(i, (i + 1) % n, n + (i + 1) % n, n + i) for i in range(n)]
    return mesh(vs, fs, M(m), parent=root, name=name)


def _round_merlon(root, x, y, z, ang, w, h, t, m):
    box((w, t, h), (x, y, z + h / 2), m, parent=root, rot=(0, 0, ang))
    cyl(w / 2, t + 0.004, (x, y, z + h), m, rot=(math.pi / 2, 0, ang), parent=root, segs=10)


S_PIER, S_W, S_H = 0.18, 0.34, 1.06     # the stone wall's pier half-size, arm width, arm height


def _stone_arm(r, k):
    u, ang, reach = wall_dir(k)
    dx, dy = WALL_DIRECTIONS[k]
    body = brickmat("stone", 0.34, 0.12, 0.06, 0.84)
    plan_prism(r, arm_plan(k, S_PIER, S_W), 0.0, S_H, body, "wall")
    rr = reach - 0.004   # decoration stops a hair short of the edge: no coplanar faces where arms meet
    plan_prism(r, arm_plan(k, S_PIER + 0.004, S_W + 0.024, rr), 0.0, 0.11, brickmat("stone_dk", 0.34, 0.11), "foot")
    plan_prism(r, arm_plan(k, S_PIER + 0.004, S_W + 0.012, rr), S_H - 0.20, 0.07, M("white"), "band")
    plan_prism(r, arm_plan(k, S_PIER + 0.006, S_W + 0.016, rr - 0.002), S_H - 0.175, 0.025, M("red"), "band")
    plan_prism(r, arm_plan(k, S_PIER + 0.004, S_W + 0.05, rr), S_H, 0.045, M("stone_lt"), "coping")
    inner = S_PIER * (math.sqrt(2) if dx and dy else 1.0)
    mm = brickmat("stone_lt", 0.2, 0.06)
    s = reach - 0.09
    while s - 0.06 >= inner + 0.02:
        x, y = _along(u, s)
        _round_merlon(r, x, y, S_H + 0.045, ang, 0.13, 0.11, 0.10, mm)
        s -= 0.19


def _stone_post(r, player):
    """A slim pier hardly wider than the wall and level with it (a run must
    read as one wall, not a row of pillars), banded in the owner's colour
    where the arms carry their red band, a coping a little proud and one
    round merlon."""
    body = brickmat("stone", 0.30, 0.12, 0.06, 0.84)
    P, h = S_PIER, S_H
    box((2 * P + 0.04, 2 * P + 0.04, 0.12), (0, 0, 0.06), brickmat("stone_dk", 0.3, 0.12), parent=r)
    box((2 * P, 2 * P, h), (0, 0, h / 2), body, parent=r)
    w = 2 * P + 0.004
    band(r, w, w, h - 0.40, 0.22, player, (0, 0), out=0.008)
    band(r, w, w, h - 0.43, 0.025, "yellow", (0, 0), out=0.01)
    band(r, w, w, h - 0.18, 0.025, "yellow", (0, 0), out=0.01)
    box((2 * P + 0.06, 2 * P + 0.06, 0.06), (0, 0, h + 0.03), M("stone_lt"), parent=r)
    _round_merlon(r, 0, 0, h + 0.06, math.pi / 4, 0.14, 0.12, 0.12, brickmat("stone_lt", 0.2, 0.06))


def stone_wall(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None, arm=None, joins=()):
    """1x1, a tile of a fortress wall in sandstone: part="post" is the square
    buttress (banded in the owner's colour, under a cavetto) when arm is None;
    arm=k (0..7) is the crenellated stretch of wall alone toward neighbour k
    (WALL_DIRECTIONS); joins=(k, ...) adds those arms to the post (previews)."""
    r = empty("eg_stone_wall", loc, rot_z, parent)
    if arm is None:
        _stone_post(r, player)
    else:
        _stone_arm(r, arm)
    for j in joins:
        _stone_arm(r, j)
    return r


P_POST, P_STEP, P_LOG = 0.10, 0.12, 0.052


def _palm_log(r, x, y, z0, h, rad, seed=0, tip=0.10):
    rnd = random.Random(seed)
    n = 4
    for i in range(n):
        za, zb = z0 + h * i / n, z0 + h * (i + 1) / n
        rod((x, y, za), (x, y, zb + 0.004), rad * (1.0 - 0.04 * i), (0.60, 0.42, 0.24) if (i + seed) % 2 else
            (0.50, 0.33, 0.19), r, r_top=rad * (0.96 - 0.04 * i), segs=7)
    lean = rnd.uniform(-0.01, 0.01)
    rod((x, y, z0 + h), (x + lean, y, z0 + h + tip), rad * 0.85, (0.72, 0.54, 0.32), r, r_top=0.0, segs=7)


def _palisade_arm(r, k):
    u, ang, reach = wall_dir(k)
    dx, dy = WALL_DIRECTIONS[k]
    inner = P_POST * (1.0 if not (dx and dy) else 1.0) + P_LOG
    # a packed-mud footing
    pts = arm_plan(k, P_POST * (0.75 if dx and dy else 1.0), 0.20, reach - 0.003)
    plan_prism(r, pts, 0.0, 0.10, mottle("mud", 0.1, 5.0), "foot")
    s, i = reach - P_STEP / 2, 0
    while s - P_LOG >= inner - 0.03:
        x, y = _along(u, s)
        _palisade_log = 0.90 + (0.05 if i % 2 else 0.0) + 0.03 * ((i * 7 + k) % 3)
        _palm_log(r, x, y, 0.0, _palisade_log, P_LOG, i + k)
        s -= P_STEP
        i += 1
    s0, s1 = P_POST + 0.02, reach
    for z in (0.28, 0.70):
        a, b = _along(u, s0), _along(u, s1)
        rod((a[0], a[1], z), (b[0], b[1], z), P_LOG + 0.012, stripemat(("straw", "thatch"), 0.03, "h"), r,
            segs=6)


def _palisade_post(r, player):
    box((0.30, 0.30, 0.10), (0, 0, 0.05), mottle("mud", 0.1, 5.0), parent=r, bevel=0.03)
    _palm_log(r, 0, 0, 0.0, 1.12, P_POST, 3, tip=0.0)
    lathe([(P_POST * 1.05, 0), (P_POST * 1.25, 0.05), (0, 0.10)], "wood_dark", (0, 0, 1.12), r, segs=8)
    lathe([(P_POST * 1.02, 0.0), (P_POST * 1.02, 0.14)], M(player), (0, 0, 0.84), r, segs=10)
    lathe([(P_POST * 1.04, 0.0), (P_POST * 1.04, 0.025)], gold(), (0, 0, 0.98), r, segs=10)
    lathe([(P_POST * 1.04, 0.0), (P_POST * 1.04, 0.025)], gold(), (0, 0, 0.815), r, segs=10)
    # palm-leaf tufts tied at the top
    for i in range(5):
        a = i * 2 * math.pi / 5
        rod((0, 0, 1.18), (0.16 * math.cos(a), 0.16 * math.sin(a), 1.32), 0.012, "palm", r, r_top=0.0, segs=4)


def palisade_wall(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None, arm=None, joins=()):
    """1x1, a tile of palm-log stockade: a stout banded log when arm is None;
    arm=k (0..7) the row of sharpened logs lashed with reed rope alone,
    toward neighbour k; joins=(k, ...) adds arms to the post (previews)."""
    r = empty("eg_palisade_wall", loc, rot_z, parent)
    if arm is None:
        _palisade_post(r, player)
    else:
        _palisade_arm(r, arm)
    for j in joins:
        _palisade_arm(r, j)
    return r


def gate(loc=(0, 0, 0), rot_z=0.0, player=RED, parent=None, line=0, open=False):
    """1x1: a pylon gate across the line of its walls (line 0..3, the first
    four WALL_DIRECTIONS): two battered towers, a corniced portal, doors shut
    or swung back, banners in the owner's colour on both faces."""
    r = empty("eg_gate", loc, rot_z, parent)
    u, ang, reach = wall_dir(line)
    L = 2 * reach
    th = ang
    if math.sin(th) + math.cos(th) < -1e-6:
        th += math.pi
    e = _sub(r, (0, 0, 0), th, 1.0, "gate")
    stone = brickmat("stone", 0.30, 0.12, 0.06, 0.84)
    pw = 0.40                       # the passage
    tl = (L - 0.03 - pw) / 2        # each tower along the line (a hair inside the tile)
    D, H = 0.66, 1.78
    box((L - 0.04, D + 0.08, 0.06), (0, 0, 0.03), brickmat("stone_lt", 0.3, 0.06), parent=e)
    for sx in (-1, 1):
        cx = sx * (pw / 2 + tl / 2)
        loft([(tl, D, 0), (tl - 0.10, D - 0.14, H)], stone, (cx, 0, 0.0), e)
        for ex in (-1, 1):
            for ey in (-1, 1):
                rod((cx + ex * tl / 2, ey * D / 2, 0), (cx + ex * (tl - 0.10) / 2, ey * (D - 0.14) / 2, H), 0.016,
                    "yellow", e, segs=5)
        cavetto(e, tl - 0.10, D - 0.14, H, 0.15, 0.05, (cx, 0), 0.085)
        for fy in (-1, 1):
            v = Vector((0, -fy * 0.07, H))
            p = Paint((cx, fy * D / 2, 0), (fy * -1, 0, 0) if fy > 0 else (1, 0, 0), v, e)
            ln = v.length
            hw = lambda b: tl / 2 - 0.05 * (b / ln) - 0.02  # noqa: E731
            p.rect(-hw(0.10), 0.10, hw(0.10), 0.17, "blue")
            p.rect(-hw(0.18), 0.18, hw(0.18), 0.20, "yellow")
            p.rect(-hw(0.21), 0.21, hw(0.21), 0.24, "red")
            p.rect(-hw(ln), ln - 0.20, hw(ln), ln - 0.03, "white")
            for i in range(3):
                a = (i - 1) * (tl - 0.16) / 3
                p.rect(a - 0.025, ln - 0.18, a + 0.025, ln - 0.06, ("blue", "red", "green")[i], 1)
            p.done()
            # a long banner on each face
            yb = fy * ((D - 0.14) / 2 + 0.05)
            bw = min(0.20, tl - 0.12)
            rod((cx - bw / 2 - 0.02, yb, H - 0.24), (cx + bw / 2 + 0.02, yb, H - 0.24), 0.012, gold(), e, segs=5)
            vs = [(cx - bw / 2, yb + fy * 0.004, H - 0.25), (cx + bw / 2, yb + fy * 0.004, H - 0.25),
                  (cx + bw / 2, yb + fy * 0.03, H - 1.12), (cx, yb + fy * 0.034, H - 1.22),
                  (cx - bw / 2, yb + fy * 0.03, H - 1.12)]
            mesh(vs, [(0, 1, 2, 3, 4)], M(player, rough=0.7), parent=e, name="banner")
            cyl(0.05, 0.01, (cx, yb + fy * 0.02, H - 0.62), gold(), rot=(math.pi / 2, 0, 0), parent=e, segs=10)
    # the portal: jambs, a lintel with the winged disc, its own cornice
    ph = 1.18
    for sx in (-1, 1):
        box((0.05, D - 0.18, ph), (sx * (pw / 2 - 0.025), 0, ph / 2), M("stone_lt"), parent=e)
    box((pw, D - 0.18, 0.26), (0, 0, ph + 0.13), M("stone_lt"), parent=e)
    for fy in (-1, 1):
        pl = Paint((0, fy * ((D - 0.18) / 2 + 0.002), ph), (1, 0, 0) if fy < 0 else (-1, 0, 0), (0, 0, 1), e)
        pl.rect(-pw / 2, 0, pw / 2, 0.26, "white")
        winged_disc(pl, 0, 0.13, 0.035, 0.17, 1)
        pl.done()
    cavetto(e, pw, D - 0.18, ph + 0.26, 0.09, 0.035, (0, 0), 0.07)
    box((pw - 0.10, D - 0.20, 0.02), (0, 0, 0.065), M("stone_dk"), parent=e)
    # doors: two cedar leaves with bronze studs
    dw, dh, dt = pw / 2 - 0.05, ph - 0.08, 0.05
    leaf = stripemat(("cedar", (0.56, 0.32, 0.18)), 0.045, "x" if not open else "y")
    for sx in (-1, 1):
        if not open:
            c = (sx * dw / 2, 0.0, 0.06 + dh / 2)
            box((dw, dt, dh), c, leaf, parent=e)
            for zz in (0.25, 0.60, 0.95):
                for fy in (-1, 1):
                    box((dw - 0.03, 0.01, 0.025), (c[0], fy * dt / 2, zz), M("bronze", metal=0.6, rough=0.4),
                        parent=e)
        else:
            hx = sx * (pw / 2 - 0.05)
            c = (hx - sx * dt / 2, -dw / 2, 0.06 + dh / 2)
            box((dt, dw, dh), c, leaf, parent=e)
            for zz in (0.25, 0.60, 0.95):
                box((0.012, dw - 0.03, 0.025), (hx - sx * dt - 0.004 * sx, -dw / 2, zz),
                    M("bronze", metal=0.6, rough=0.4), parent=e)
    return r


def _wall_links(t, at):
    """walls.rs's joining rules: which arms the wall at game tile t shows, and
    the line of a gate; `at` maps game tiles to "wall" / "gate"."""
    def has(q, k):
        dx, dy = WALL_DIRECTIONS[k]
        return (q[0] + dx, q[1] + dy) in at

    def line(q):
        for o in (0, 2, 1, 3):
            if has(q, o) and has(q, o + 4):
                return o
        for o in (0, 2, 1, 3):
            if has(q, o) or has(q, o + 4):
                return o
        return 0

    if at.get(t) == "gate":
        return line(t)
    out = []
    for k, (dx, dy) in enumerate(WALL_DIRECTIONS):
        q = (t[0] + dx, t[1] + dy)
        if q not in at:
            continue
        if dx and dy and ((t[0] + dx, t[1]) in at or (t[0], t[1] + dy) in at):
            continue
        if at[q] == "gate" and line(q) != k % 4:
            continue
        out.append(k)
    return tuple(out)


def wall_run(root, kind, walls, gates=(), origin=(0, 0), player=RED, opened=()):
    """Walls (game tiles) joined as the game joins them, with gates in them."""
    at = {t: "wall" for t in walls}
    at.update({t: "gate" for t in gates})
    f = globals()[kind]
    for t in walls:
        f((origin[0] + t[0], origin[1] - t[1], 0), player=player, parent=root, joins=_wall_links(t, at))
    for t in gates:
        gate((origin[0] + t[0], origin[1] - t[1], 0), player=player, parent=root, line=_wall_links(t, at),
             open=t in opened)


def showcase():
    """All of them on plain ground, in screen rows (u = screen-right, d = away)."""
    def at(u, d):
        k = math.sqrt(2)
        return ((u - d) / k, (u + d) / k, 0)

    r = empty("eg_showcase")
    wonder(at(-8.6, 9.6), parent=r)
    town_center(at(-0.6, 8.8), player=BLUE, parent=r)
    dock(at(6.2, 8.8), parent=r)
    row = (temple2, government_centre, academy, market, storehouse, siege_workshop)
    for i, f in enumerate(row):
        f(at(-8.25 + 3.3 * i, 3.6), player=(RED, BLUE)[i % 2], parent=r)
    row = (barracks, archery_range, stable, farm)
    for i, f in enumerate(row):
        f(at(-9.9 + 3.3 * i, -0.6), player=(BLUE, RED)[i % 2], parent=r)
    for i, v in enumerate((0, 1, 2)):
        house(at(3.3 + 3.3 * i, -0.6), variant=v, player=(RED, BLUE)[i % 2], parent=r)
    watch_tower(at(-0.9, -4.6), parent=r)
    o = at(-6.6, -5.0)
    wall_run(r, "stone_wall", [(0, 0), (1, 0), (2, 0), (4, 0), (5, 0), (5, 1)], [(3, 0)],
             (round(o[0]), round(o[1])), RED)
    o = at(4.0, -5.0)
    wall_run(r, "palisade_wall", [(0, 0), (1, 0), (3, 0), (4, 0), (0, 1), (0, 2)], [(2, 0)],
             (round(o[0]), round(o[1])), BLUE, opened=((2, 0),))
    return r


# --- the sets the batch exporter makes (docs/look/buildings.md) ---------------------------

_ARMS = {"arm": [{"arm": k} for k in range(8)]}

SETS = {
    "town_center_egyptian": dict(func="egyptian.town_center", cls="LargeBuilding", tiles=3, kwargs={},
                                 fit=None),
    "house_egyptian": dict(func="egyptian.house", cls="MediumBuilding", tiles=2, kwargs={"variant": 0},
                           fit=None),
    "barracks_egyptian": dict(func="egyptian.barracks", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "temple_egyptian": dict(func="egyptian.temple2", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "watch_tower_egyptian": dict(func="egyptian.watch_tower", cls="SmallBuilding", tiles=1, kwargs={},
                                 fit=None),
    "academy_egyptian": dict(func="egyptian.academy", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "archery_range_egyptian": dict(func="egyptian.archery_range", cls="MediumBuilding", tiles=2, kwargs={},
                                   fit=None),
    "dock_egyptian": dict(func="egyptian.dock", cls="LargeBuilding", tiles=3, kwargs={}, fit=None,
                          stages="flat"),
    "farm_egyptian": dict(func="egyptian.farm", cls="MediumBuilding", tiles=2, kwargs={}, fit=None,
                          stages="flat"),
    "government_centre_egyptian": dict(func="egyptian.government_centre", cls="MediumBuilding", tiles=2,
                                       kwargs={}, fit=None),
    "market_egyptian": dict(func="egyptian.market", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "siege_workshop_egyptian": dict(func="egyptian.siege_workshop", cls="MediumBuilding", tiles=2,
                                    kwargs={}, fit=None),
    "stable_egyptian": dict(func="egyptian.stable", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "storehouse_egyptian": dict(func="egyptian.storehouse", cls="MediumBuilding", tiles=2, kwargs={},
                                fit=None),
    "wonder_egyptian": dict(func="egyptian.wonder", cls="Wonder", tiles=5, kwargs={}, fit=None),
    "palisade_wall_egyptian": dict(func="egyptian.palisade_wall", cls="SmallBuilding", tiles=1, kwargs={},
                                   fit=None, anims=_ARMS),
    "stone_wall_egyptian": dict(func="egyptian.stone_wall", cls="SmallBuilding", tiles=1, kwargs={},
                                fit=None, anims=_ARMS),
    "gate_egyptian": dict(func="egyptian.gate", cls="SmallBuilding", tiles=1, kwargs={}, fit=None,
                          anims={"shut": [{"line": n, "open": False} for n in range(4)],
                                 "open": [{"line": n, "open": True} for n in range(4)]}),
}

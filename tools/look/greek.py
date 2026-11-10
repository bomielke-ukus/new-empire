"""Greek buildings for the new look: painted marble, terracotta tile, bronze.

Every builder is centred on the origin on its footprint (in tiles), takes
`loc=(x, y, 0), rot_z=0.0, player=BLUE, parent=None` and returns its root
empty. The faces the game camera sees are -Y (screen left, sunlit) and +X
(screen right), so doors and the best detail are on those sides.

    temple()        3 x 3      town_center()   3 x 3
    house(variant)  2 x 2      barracks()      2 x 2
    watch_tower()   1 x 1      market_stall()  1 x 1
    amphora() cart() crate() well() brazier() olive_tree()   props
    showcase()      all of them, laid out as a small village
"""
import math
import random
from math import atan2, cos, hypot, pi, sin

import bpy
import bmesh
from mathutils import Vector

import lib
from lib import BLUE, PAL, RED, blob, box, cyl, empty, mat, mesh, pyramid, sphere

# ---------------------------------------------------------------- colours


def tint(c, f, shift=(0, 0, 0)):
    c = PAL[c] if isinstance(c, str) else c
    return tuple(max(0.0, min(1.0, v * f + s)) for v, s in zip(c, shift))


def bronze():
    return mat("bronze", rough=0.38, metal=0.65)


def gold():
    return mat("gold", rough=0.3, metal=0.55)


def _node_mat(key, build):
    """Procedural materials, cached in the kit's cache so reset() clears them."""
    key = ("greek",) + key
    if key not in lib._mats:
        m = bpy.data.materials.new(key[1])
        m.use_nodes = True
        build(m.node_tree, m.node_tree.nodes["Principled BSDF"])
        lib._mats[key] = m
    return lib._mats[key]


def noisy(colour, amt=0.10, scale=5.0, rough=0.9):
    """A wall colour with soft blotches, so plaster is never perfectly flat."""
    rgb = PAL[colour] if isinstance(colour, str) else tuple(colour)

    def build(nt, bsdf):
        tc = nt.nodes.new("ShaderNodeTexCoord")
        n = nt.nodes.new("ShaderNodeTexNoise")
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = 3.0
        ramp = nt.nodes.new("ShaderNodeValToRGB")
        ramp.color_ramp.elements[0].position = 0.32
        ramp.color_ramp.elements[1].position = 0.72
        ramp.color_ramp.elements[0].color = (*lib._lin(tint(rgb, 1 - amt, (0.0, -0.01, -0.03))), 1)
        ramp.color_ramp.elements[1].color = (*lib._lin(tint(rgb, 1 + amt * 0.3)), 1)
        nt.links.new(tc.outputs["Object"], n.inputs["Vector"])
        nt.links.new(n.outputs["Fac"], ramp.inputs["Fac"])
        nt.links.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])
        bsdf.inputs["Roughness"].default_value = rough
    return _node_mat(("noisy", rgb, amt, scale), build)


def courses(colour="limestone", scale=7.0, mortar=0.72, vary=0.16):
    """Stone / brick courses on vertical faces (rows along Z)."""
    rgb = PAL[colour] if isinstance(colour, str) else tuple(colour)

    def build(nt, bsdf):
        tc = nt.nodes.new("ShaderNodeTexCoord")
        sep = nt.nodes.new("ShaderNodeSeparateXYZ")
        add = nt.nodes.new("ShaderNodeMath")
        add.operation = "ADD"
        comb = nt.nodes.new("ShaderNodeCombineXYZ")
        br = nt.nodes.new("ShaderNodeTexBrick")
        br.inputs["Color1"].default_value = (*lib._lin(tint(rgb, 1 + vary * 0.4)), 1)
        br.inputs["Color2"].default_value = (*lib._lin(tint(rgb, 1 - vary, (0.02, 0, -0.02))), 1)
        br.inputs["Mortar"].default_value = (*lib._lin(tint(rgb, mortar)), 1)
        br.inputs["Scale"].default_value = scale
        br.inputs["Mortar Size"].default_value = 0.035
        br.inputs["Brick Width"].default_value = 0.9
        br.inputs["Row Height"].default_value = 0.42
        nt.links.new(tc.outputs["Object"], sep.inputs[0])
        nt.links.new(sep.outputs["X"], add.inputs[0])
        nt.links.new(sep.outputs["Y"], add.inputs[1])
        nt.links.new(add.outputs[0], comb.inputs["X"])
        nt.links.new(sep.outputs["Z"], comb.inputs["Y"])
        nt.links.new(comb.outputs[0], br.inputs["Vector"])
        nt.links.new(br.outputs["Color"], bsdf.inputs["Base Color"])
        bsdf.inputs["Roughness"].default_value = 0.92
    return _node_mat(("courses", rgb, scale, mortar, vary), build)


TILES = [(0.84, 0.40, 0.22), (0.78, 0.33, 0.18), (0.88, 0.47, 0.26), (0.72, 0.30, 0.17)]
FRUIT = [(0.86, 0.12, 0.10), (0.98, 0.78, 0.12), (0.45, 0.14, 0.45), (0.96, 0.50, 0.10),
         (0.42, 0.62, 0.16), (0.70, 0.10, 0.22)]

# ------------------------------------------------------------- primitives


def lathe(profile, m, loc=(0, 0, 0), segs=12, parent=None, smooth=True, rot=(0, 0, 0),
          s=1.0, name="lathe"):
    """Revolve (r, z) points around Z; capped both ends."""
    bm = bmesh.new()
    rings = []
    for r, z in profile:
        rings.append([bm.verts.new((max(r, 0.002) * s * cos(2 * pi * i / segs),
                                    max(r, 0.002) * s * sin(2 * pi * i / segs), z * s))
                      for i in range(segs)])
    for a, b in zip(rings, rings[1:]):
        for i in range(segs):
            j = (i + 1) % segs
            bm.faces.new((a[i], a[j], b[j], b[i]))
    bm.faces.new(list(reversed(rings[0])))
    bm.faces.new(rings[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return lib._obj(bm, name, m if isinstance(m, bpy.types.Material) else mat(m), loc, rot,
                    smooth, parent, 0.0)


def rod(p0, p1, r=0.02, m="wood", parent=None, segs=6, r_top=None):
    """A thin cylinder from p0 to p1."""
    a, b = Vector(p0), Vector(p1)
    d = b - a
    return cyl(r, d.length, (a + b) / 2, m, rot=d.to_track_quat("Z", "Y").to_euler(),
               parent=parent, segs=segs, r_top=r_top, name="rod")


def cloth(w, h, m, loc=(0, 0, 0), rot_z=0.0, parent=None, wave=0.05, taper=0.0, nx=8,
          seed=0, hang=False):
    """A waving cloth. Flying (default): attached along its left edge at `loc`
    (top corner), flying out along local +X. `hang=True`: attached along its
    top edge, centred on `loc`, hanging down. `taper` 1.0 makes a pennant."""
    ph = seed * 1.7
    vs, fs = [], []
    nz = 3
    for i in range(nx + 1):
        u = i / nx
        for j in range(nz + 1):
            v = j / nz
            if hang:
                x = (u - 0.5) * w
                z = -v * h
                y = wave * sin(u * 5.0 + ph) * (0.3 + v) + wave * 0.5 * sin(v * 4 + ph)
            else:
                hh = h * (1 - taper * u)
                x = u * w
                z = -(h - hh) / 2 - v * hh - 0.10 * w * u * u
                y = wave * sin(u * 6.5 + ph + v * 0.8) * (0.25 + u)
            vs.append((x, y, z))
    for i in range(nx):
        for j in range(nz):
            a = i * (nz + 1) + j
            fs.append((a, a + nz + 1, a + nz + 2, a + 1))
    return mesh(vs, fs, m, loc, (0, 0, rot_z), parent, smooth=True, name="cloth")


def flag(loc, h=2.0, player=BLUE, parent=None, w=0.55, fh=0.36, rot_z=pi / 4, pole_r=0.022,
         pennant=False, seed=0):
    """A pole standing at `loc` with a flying flag in the player colour."""
    x, y, z = loc
    cyl(pole_r, h, (x, y, z + h / 2), "wood_dark", parent=parent, segs=8)
    sphere(pole_r * 2.2, (x, y, z + h + pole_r), gold(), parent=parent, segs=8)
    top = (x, y, z + h - 0.03)
    if pennant:
        cloth(w, fh, mat(player, rough=0.7), top, rot_z, parent, taper=0.92, seed=seed, wave=0.04)
    else:             # three bands: player colour, a gold stripe, player colour
        zz = z + h - 0.03
        for frac, m in ((0.4, mat(player, rough=0.7)), (0.2, mat("gold", rough=0.6)),
                        (0.4, mat(player, rough=0.7))):
            cloth(w, fh * frac, m, (x, y, zz), rot_z, parent, seed=seed, wave=0.05)
            zz -= fh * frac
    return top


def banner(loc, h=1.7, player=BLUE, parent=None, w=0.32, bh=0.62, rot_z=0.0, seed=0):
    """A pole with a crossbar and a hanging banner (faces local -Y)."""
    e = empty("banner", loc, rot_z, parent)
    cyl(0.022, h, (0, 0, h / 2), "wood_dark", parent=e, segs=8)
    sphere(0.045, (0, 0, h + 0.02), gold(), parent=e, segs=8)
    zt = h - 0.1
    rod((-w / 2 - 0.04, -0.03, zt), (w / 2 + 0.04, -0.03, zt), 0.016, "wood_dark", e)
    for sx in (-1, 1):
        sphere(0.026, (sx * (w / 2 + 0.05), -0.03, zt), gold(), parent=e, segs=6)
    cloth(w, bh, mat(player, rough=0.7), (0, -0.035, zt), 0, e, hang=True, wave=0.018, seed=seed)
    cloth(w, 0.07, gold(), (0, -0.05, zt - bh * 0.72), 0, e, hang=True, wave=0.012, seed=seed)
    cyl(0.07, 0.012, (0, -0.06, zt - bh * 0.36), gold(), rot=(pi / 2, 0, 0), parent=e, segs=10)
    return e


def column(loc, h, r=0.1, parent=None, shaft="marble", band=None, band2=None, flutes=10,
           lower=None):
    """A Doric column standing at `loc`: fluted tapered shaft, echinus, abacus,
    painted necking bands. `lower` paints the bottom third (stoa style)."""
    x, y, z = loc
    ch = min(0.16, h * 0.14)            # capital height
    sh = h - ch
    bm = bmesh.new()
    n = flutes * 2
    lo, hi = [], []
    for i in range(n):
        a = 2 * pi * i / n
        k = 1.0 if i % 2 == 0 else 0.84
        lo.append(bm.verts.new((r * k * cos(a), r * k * sin(a), 0)))
        hi.append(bm.verts.new((r * 0.8 * k * cos(a), r * 0.8 * k * sin(a), sh)))
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new((lo[i], lo[j], hi[j], hi[i]))
    bm.faces.new(hi)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    lib._obj(bm, "shaft", mat(shaft) if isinstance(shaft, (str, tuple)) else shaft,
             (x, y, z), (0, 0, 0), False, parent, 0.0)
    if lower:
        cyl(r * 1.03, sh * 0.34, (x, y, z + sh * 0.17), lower, parent=parent,
            r_top=r * 0.97, segs=12)
    zt = z + sh
    if band:
        cyl(r * 0.86, 0.035, (x, y, zt - 0.05), band, parent=parent, segs=12)
    if band2:
        cyl(r * 0.86, 0.02, (x, y, zt - 0.095), band2, parent=parent, segs=12)
    cyl(r * 0.82, ch * 0.55, (x, y, zt + ch * 0.275), shaft, parent=parent, r_top=r * 1.3, segs=12)
    box((r * 2.75, r * 2.75, ch * 0.45), (x, y, zt + ch * 0.775), shaft, parent=parent)


# ------------------------------------------------------------------ roofs


def _slope(e, sx, y0, z0, y1, z1, antefix="tile_dark", seed=0, pitch=0.15, rows=None):
    """One tiled slope in e's frame: from the eave (y0, z0) up to (y1, z1),
    `sx` wide along X. Overlapping pan rows plus cover-tile ridges."""
    rnd = random.Random(seed)
    dy, dz = y1 - y0, z1 - z0
    L = hypot(dy, dz)
    th = atan2(dz, dy)
    sg = 1 if dy > 0 else -1
    ny, nz = -dz / L * sg, abs(dy) / L
    k = rows or max(3, round(L / 0.17))
    step = L / k
    for i in range(k):
        t = (i + 0.5) / k
        box((sx, step * 1.32, 0.036),
            (0, y0 + dy * t + ny * 0.03, z0 + dz * t + nz * 0.03),
            mat(TILES[rnd.randrange(len(TILES))], rough=0.8), rot=(th - sg * 0.08, 0, 0),
            parent=e, name="tiles")
    n = max(2, round(sx / pitch))
    cov = mat("tile_dark", rough=0.8)
    cm = (y0 + dy * 0.5 + ny * 0.07, z0 + dz * 0.5 + nz * 0.07)
    for j in range(n + 1):
        x = -sx / 2 + j * sx / n
        edge = j in (0, n)
        cyl(0.034 if edge else 0.024, L + 0.03, (x, cm[0], cm[1]), cov,
            rot=(atan2(-dy, dz), 0, 0), parent=e, segs=6, name="cover")
        if antefix:
            sphere(0.04, (x, y0 + ny * 0.06, z0 + nz * 0.06), antefix, parent=e, segs=6,
                   scale=(1, 0.6, 1.1))


def gable_roof(length, span, h, loc, parent=None, axis="x", over=0.1, gable=None,
               antefix="tile_dark", seed=0):
    """A tiled gable roof whose eaves' wall line sits at loc.z. `length` runs
    along the ridge, which lies along `axis`. `gable` fills the end triangles."""
    e = empty("roof", loc, 0.0 if axis == "x" else pi / 2, parent)
    Y = span / 2
    a = h / Y
    y0, z0 = Y + over, -over * a
    sx = length + 2 * over
    _slope(e, sx, -y0, z0, 0, h, antefix, seed)
    _slope(e, sx, y0, z0, 0, h, antefix, seed + 1)
    cyl(0.05, sx + 0.04, (0, 0, h + 0.05), mat("tile_dark", rough=0.8), rot=(0, pi / 2, 0),
        parent=e, segs=8)
    if gable is not None:
        for sgn in (-1, 1):
            x = sgn * length / 2
            mesh([(x, -Y, 0), (x, Y, 0), (x, 0, h)], [(0, 1, 2)], gable, parent=e)
    return e


def shed_roof(sx, sy, h, loc, parent=None, rot_z=0.0, over=0.08, antefix="tile_dark", seed=0):
    """A lean-to: low eave at local -Y (z = loc.z), high edge at +Y (z + h)."""
    e = empty("shed", loc, rot_z, parent)
    a = h / sy
    _slope(e, sx + 2 * over, -sy / 2 - over, -over * a, sy / 2, h, antefix, seed)
    return e


def hip_roof(base, h, loc, parent=None, tiers=5, finial=True):
    """A pyramidal tiled cap: stepped tiers for the tile rows, cover-tile
    ridges fanning down each face, heavier hip ridges."""
    x, y, z = loc
    for i in range(tiers):
        b0 = base * (1 - i / tiers) + 0.07
        b1 = base * (1 - (i + 1) / tiers)
        pyramid(b0, h / tiers + 0.035, (x, y, z + i * h / tiers), mat(TILES[(i * 3) % 4], rough=0.8),
                parent=parent, top=max(b1 - 0.01, 0.0))
    cov = mat("tile_dark", rough=0.8)
    d = base / 2 + 0.035
    for k in range(4):
        a = k * pi / 2
        ca, sa = cos(a), sin(a)
        for j in range(-3, 4):
            t = j / 4.0
            ex, ey = d * (ca - t * sa), d * (sa + t * ca)
            f = 1 - abs(t)          # how far up the face this ridge runs
            rod((x + ex, y + ey, z + 0.035),
                (x + d * ((1 - f) * ca - t * sa), y + d * ((1 - f) * sa + t * ca), z + 0.035 + h * f),
                0.02, cov, parent)
        rod((x + d * (ca - sa), y + d * (sa + ca), z + 0.03), (x, y, z + h + 0.03), 0.034, cov, parent)
        sphere(0.045, (x + d * (ca - sa), y + d * (sa + ca), z + 0.04), "tile_dark", parent=parent, segs=6)
    if finial:
        sphere(0.06, (x, y, z + h + 0.04), gold(), parent=parent, segs=8)


# ------------------------------------------------------------ wall pieces


def wall_block(sx, sy, h, loc, parent=None, wall="whitewash", dado="red_ochre",
               line="egyptian_blue", base="limestone", dado_h=0.26, base_h=0.12, bevel=0.012):
    """A plastered block standing on loc (centre of its footprint): stone base
    course, a painted dado band with a thin line over it, blotchy plaster."""
    x, y, z = loc
    box((sx, sy, h), (x, y, z + h / 2), noisy(wall), parent=parent, bevel=bevel, name="wall")
    if base:
        box((sx + 0.05, sy + 0.05, base_h), (x, y, z + base_h / 2), courses(base, 9.0),
            parent=parent, bevel=0.01)
    if dado:
        box((sx + 0.016, sy + 0.016, dado_h), (x, y, z + base_h + dado_h / 2),
            noisy(dado, 0.12, 9.0), parent=parent)
        if line:
            box((sx + 0.022, sy + 0.022, 0.035), (x, y, z + base_h + dado_h + 0.03), line,
                parent=parent)


def face(loc, rz=0.0, parent=None):
    """A frame whose local -Y looks outward: rz=0 for a -Y wall, pi/2 for +X,
    pi for +Y, -pi/2 for -X."""
    return empty("face", loc, rz, parent)


def door(loc, rz=0.0, parent=None, w=0.42, h=0.95, colour="egyptian_blue", frame="wood_dark",
         metal=False, lintel="wood_dark"):
    """A double door on a wall plane; loc is its bottom centre on the wall."""
    e = face(loc, rz, parent)
    box((w + 0.12, 0.05, h + 0.06), (0, -0.012, (h + 0.06) / 2), frame, parent=e)
    lm = bronze() if metal else mat(colour, rough=0.6)
    dk = mat(tint(colour, 0.6)) if not metal else mat(tint("bronze", 0.6), 0.4, 0.6)
    box((w, 0.03, h), (0, -0.036, h / 2), lm, parent=e)
    box((0.014, 0.036, h), (0, -0.04, h / 2), dk, parent=e)
    for zf in (0.2, 0.52, 0.84):
        box((w, 0.036, 0.03), (0, -0.04, h * zf), dk, parent=e)
    for sx in (-1, 1):
        sphere(0.024, (sx * 0.05, -0.062, h * 0.5), gold(), parent=e, segs=6)
        if metal:
            for zf in (0.36, 0.68):
                sphere(0.02, (sx * w * 0.27, -0.058, h * zf), gold(), parent=e, segs=6)
    box((w + 0.24, 0.08, 0.07), (0, -0.02, h + 0.085), lintel, parent=e)
    box((w + 0.2, 0.16, 0.05), (0, -0.08, 0.025), "limestone", parent=e, bevel=0.01)
    return e


def window(loc, rz=0.0, parent=None, w=0.22, h=0.3, colour="egyptian_blue", shutters=True):
    """A small window; loc is its centre on the wall plane."""
    e = face(loc, rz, parent)
    box((w, 0.03, h), (0, -0.004, 0), mat((0.16, 0.11, 0.09)), parent=e)
    box((w + 0.08, 0.06, 0.035), (0, -0.02, -h / 2 - 0.015), "limestone", parent=e)
    box((w + 0.08, 0.05, 0.04), (0, -0.015, h / 2 + 0.02), "wood_dark", parent=e)
    box((0.02, 0.02, h), (0, -0.01, 0), "wood_dark", parent=e)
    if shutters:
        for sx in (-1, 1):
            box((w * 0.5, 0.022, h), (sx * (w * 0.76 + 0.01), -0.022, 0),
                mat(colour, rough=0.6), parent=e)
            for zf in (-0.28, 0.28):
                box((w * 0.5, 0.028, 0.02), (sx * (w * 0.76 + 0.01), -0.024, h * zf),
                    mat(tint(colour, 0.6)), parent=e)
    return e


def awning(loc, rz=0.0, parent=None, w=1.0, d=0.6, drop=0.25, player=BLUE, other="linen",
           stripes=6, valance=0.09, sag=0.035):
    """A striped cloth awning in a frame: high edge on local y=0 at loc, sloping
    down to local -Y by `drop`. Stripes alternate player colour and linen."""
    e = face(loc, rz, parent)
    ms = [mat(player, rough=0.75), mat(other, rough=0.8)]
    sw = w / stripes
    for i in range(stripes):
        x0, x1 = -w / 2 + i * sw, -w / 2 + (i + 1) * sw
        vs = [(x0, 0, 0), (x1, 0, 0), (x1, -d / 2, -drop / 2 - sag), (x0, -d / 2, -drop / 2 - sag),
              (x1, -d, -drop), (x0, -d, -drop),
              (x1, -d - 0.004, -drop - valance), ((x0 + x1) / 2, -d - 0.004, -drop - valance * 1.7),
              (x0, -d - 0.004, -drop - valance)]
        mesh(vs, [(0, 1, 2, 3), (3, 2, 4, 5), (5, 4, 6, 7, 8)], ms[i % 2], parent=e, smooth=True)
    return e


def foliage(points, r=0.12, parent=None, seed=0, mats=("leaf", "leaf_dark", "leaf_olive"),
            grapes=0, flowers=None):
    rnd = random.Random(seed)
    for i, p in enumerate(points):
        rr = r * rnd.uniform(0.75, 1.2)
        blob(rr, p, mats[rnd.randrange(len(mats))], (1, 1, 0.7), parent, seed + i, 0.3, 1)
        if flowers and rnd.random() < 0.7:
            sphere(0.035, (p[0] + rnd.uniform(-rr, rr) * 0.6, p[1] - rr * 0.6,
                           p[2] + rr * 0.35), mat(flowers, emit=0.15), parent=parent, segs=6)
    for i in range(grapes):
        p = points[rnd.randrange(len(points))]
        gx, gy = p[0] + rnd.uniform(-0.06, 0.06), p[1] + rnd.uniform(-0.06, 0.06)
        for k in range(3):
            sphere(0.035 - k * 0.008, (gx, gy, p[2] - r * 0.7 - k * 0.04),
                   mat((0.42, 0.16, 0.5), rough=0.5), parent=parent, segs=6)


# ------------------------------------------------------------------ props


def amphora(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, s=0.6, colour="terracotta",
            tilt=0.0, band="black"):
    """A transport amphora, `s` tall. Painted shoulder band."""
    r = empty("amphora", loc, rot_z, parent)
    r.rotation_euler = (tilt, 0, rot_z)
    prof = [(0.03, 0), (0.05, 0.03), (0.13, 0.2), (0.175, 0.4), (0.165, 0.52), (0.1, 0.63),
            (0.055, 0.69), (0.05, 0.82), (0.08, 0.85), (0.08, 0.88), (0.04, 0.88)]
    lathe(prof, mat(colour, rough=0.6), parent=r, s=s, segs=10)
    if band:
        lathe([(0.172, 0.34), (0.181, 0.4), (0.172, 0.47)], band, parent=r, s=s, segs=10)
        lathe([(0.118, 0.585), (0.1, 0.635)], "yellow_ochre", parent=r, s=s, segs=10)
    for sg in (-1, 1):
        a, b, c = (sg * 0.05 * s, 0, 0.8 * s), (sg * 0.15 * s, 0, 0.76 * s), (sg * 0.15 * s, 0, 0.56 * s)
        rod(a, b, 0.018 * s, mat(colour, rough=0.6), r)
        rod(b, c, 0.018 * s, mat(colour, rough=0.6), r)
    return r


def pithos(loc=(0, 0, 0), parent=None, s=0.6, colour=(0.74, 0.42, 0.26)):
    """A fat storage jar for oil or grain."""
    prof = [(0.12, 0), (0.24, 0.2), (0.3, 0.5), (0.26, 0.76), (0.17, 0.9), (0.2, 0.96),
            (0.2, 1.0), (0.14, 1.0), (0.14, 0.93)]
    o = lathe(prof, mat(colour, rough=0.7), loc, parent=parent, s=s, segs=12)
    for zf in (0.36, 0.62):
        lathe([(0.29 if zf < 0.5 else 0.295, zf - 0.02), (0.31, zf), (0.29, zf + 0.02)],
              mat(tint(colour, 0.7)), loc, parent=parent, s=s, segs=12)
    return o


def pot(loc=(0, 0, 0), parent=None, s=0.22, plant="leaf", flowers=None, seed=0,
        colour="terracotta"):
    """A flower pot with a bush in it."""
    x, y, z = loc
    lathe([(0.3, 0), (0.42, 0.7), (0.5, 0.78), (0.5, 1.0), (0.38, 1.0), (0.38, 0.9)],
          mat(colour, rough=0.7), loc, parent=parent, s=s, segs=10)
    if plant:
        foliage([(x, y, z + s * 1.35), (x + s * 0.3, y - s * 0.2, z + s * 1.2),
                 (x - s * 0.3, y + s * 0.1, z + s * 1.25)], s * 0.55, parent, seed,
                (plant, "leaf_dark"), flowers=flowers)


def basket(loc=(0, 0, 0), parent=None, s=0.2, fruit=0, seed=0, n=7):
    """A wicker basket heaped with produce (`fruit` picks the colour)."""
    x, y, z = loc
    rnd = random.Random(seed)
    lathe([(0.7, 0), (1.0, 0.75), (1.06, 0.8), (1.06, 0.9), (0.9, 0.9), (0.85, 0.7)],
          mat("thatch", rough=0.95), loc, parent=parent, s=s, segs=10)
    lathe([(0.92, 0.3), (0.96, 0.36), (0.94, 0.42)], mat(tint("thatch", 0.7)), loc,
          parent=parent, s=s, segs=10)
    if fruit is not None:
        c = FRUIT[fruit % len(FRUIT)]
        cyl(s * 0.86, 0.02, (x, y, z + s * 0.72), mat(tint(c, 0.7)), parent=parent, segs=10)
        for i in range(n):
            a = i * 2.4
            rr = s * 0.55 * (i / n) ** 0.5
            sphere(s * 0.3, (x + rr * cos(a), y + rr * sin(a), z + s * (0.98 - 0.25 * i / n)),
                   mat(tint(c, rnd.uniform(0.85, 1.1)), rough=0.45), parent=parent, segs=6)


def crate(loc=(0, 0, 0), rot_z=0.0, parent=None, s=0.3, fruit=None, seed=0):
    """A slatted wooden crate, optionally open and full of produce."""
    r = empty("crate", loc, rot_z, parent)
    box((s, s, s * 0.8), (0, 0, s * 0.4), mat("cedar"), parent=r)
    dk = mat("wood_dark")
    for sx in (-1, 1):
        for sy in (-1, 1):
            box((s * 0.14, s * 0.14, s * 0.86), (sx * s * 0.46, sy * s * 0.46, s * 0.43), dk, parent=r)
    for zf in (0.12, 0.7):
        box((s * 1.03, s * 1.03, s * 0.1), (0, 0, s * zf), mat("wood"), parent=r)
    if fruit is not None:
        rnd = random.Random(seed)
        c = FRUIT[fruit % len(FRUIT)]
        for i in range(8):
            sphere(s * 0.17, (rnd.uniform(-1, 1) * s * 0.3, rnd.uniform(-1, 1) * s * 0.3,
                              s * 0.84), mat(tint(c, rnd.uniform(0.85, 1.1)), rough=0.45),
                   parent=r, segs=6)
    return r


def sack(loc, parent=None, s=0.16, seed=0, colour="wool"):
    x, y, z = loc
    blob(s, (x, y, z + s * 0.75), colour, (1, 0.85, 0.85), parent, seed, 0.12, 2)
    sphere(s * 0.25, (x, y, z + s * 1.55), tint(colour, 0.8), parent=parent, segs=6)


def bench(loc, rz=0.0, parent=None, w=0.5, m="wood"):
    e = face(loc, rz, parent)
    box((w, 0.16, 0.035), (0, 0, 0.2), m, parent=e)
    for sx in (-1, 1):
        box((0.04, 0.14, 0.2), (sx * (w / 2 - 0.05), 0, 0.1), "wood_dark", parent=e)
    return e


def cart(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, load=True):
    """A two-wheeled ox cart, shafts toward local +X, about 1.0 x 0.6."""
    r = empty("cart", loc, rot_z, parent)
    wr = 0.21
    box((0.62, 0.4, 0.045), (-0.05, 0, wr + 0.04), "cedar", parent=r)
    for sy in (-1, 1):
        box((0.62, 0.03, 0.14), (-0.05, sy * 0.2, wr + 0.13), "wood", parent=r)
        for k in range(4):
            box((0.03, 0.04, 0.2), (-0.32 + k * 0.18, sy * 0.2, wr + 0.14), "wood_dark", parent=r)
        # wheel: felloe, plank disc, cross bars, hub
        y = sy * 0.26
        cyl(wr, 0.045, (-0.05, y, wr), "wood_dark", rot=(pi / 2, 0, 0), parent=r, segs=14)
        cyl(wr * 0.86, 0.05, (-0.05, y, wr), "wood", rot=(pi / 2, 0, 0), parent=r, segs=14)
        for a in (0.4, 0.4 + pi / 2):
            box((wr * 1.75, 0.058, 0.035), (-0.05, y, wr), "wood_dark", rot=(0, a, 0), parent=r)
        cyl(0.045, 0.09, (-0.05, y, wr), bronze(), rot=(pi / 2, 0, 0), parent=r, segs=8)
        rod((0.2, sy * 0.13, wr + 0.04), (0.78, sy * 0.1, 0.03), 0.018, "wood_dark", r)
    box((0.03, 0.4, 0.2), (-0.36, 0, wr + 0.16), "wood", parent=r)
    rod((0.74, -0.16, 0.06), (0.74, 0.16, 0.06), 0.02, "wood", r)
    box((0.04, 0.04, wr + 0.02), (0.22, 0, (wr + 0.02) / 2), "wood_dark", parent=r)
    if load:
        z = wr + 0.065
        amphora((-0.22, 0.07, z), 0.5, parent=r, s=0.36)
        amphora((-0.2, -0.09, z), 1.2, parent=r, s=0.34, colour=(0.86, 0.62, 0.36), band="red_ochre")
        sack((0.0, 0.05, z), r, 0.11, 3)
        sack((0.12, -0.06, z), r, 0.1, 5, colour="linen")
        blob(0.1, (-0.02, -0.08, z + 0.07), "straw", (1.2, 1, 0.6), r, 2, 0.2, 1)
        box((0.36, 0.2, 0.012), (0.04, 0.0, z + 0.2), mat(player, rough=0.75),
            rot=(0.12, 0.08, 0.2), parent=r)
    return r


def well(loc=(0, 0, 0), rot_z=0.0, parent=None):
    """A stone well with a windlass and bucket, about 0.6 across."""
    r = empty("well", loc, rot_z, parent)
    lathe([(0.27, 0), (0.27, 0.3), (0.29, 0.31), (0.29, 0.36), (0.19, 0.36), (0.19, 0.2)],
          courses("limestone", 16.0), parent=r, segs=14, smooth=False)
    cyl(0.19, 0.02, (0, 0, 0.22), mat("water", rough=0.2), parent=r, segs=14)
    for sx in (-1, 1):
        box((0.05, 0.05, 0.82), (sx * 0.3, 0, 0.41), "wood_dark", parent=r)
    rod((-0.36, 0, 0.7), (0.4, 0, 0.7), 0.03, "wood", r, 8)
    rod((0.4, 0, 0.7), (0.4, 0, 0.6), 0.014, "wood_dark", r)
    rod((0.4, 0, 0.6), (0.46, 0, 0.6), 0.014, "wood_dark", r)
    cyl(0.045, 0.2, (0, 0, 0.7), "wool", rot=(0, pi / 2, 0), parent=r, segs=8)
    rod((0.06, 0, 0.7), (0.06, -0.02, 0.5), 0.008, "wool", r, 4)
    lathe([(0.05, 0), (0.07, 0.11), (0.06, 0.11)], "cedar", (0.06, -0.02, 0.39), parent=r, segs=8)
    box((0.7, 0.14, 0.03), (0, 0, 0.84), mat(TILES[0]), rot=(0.5, 0, 0), parent=r)
    box((0.7, 0.14, 0.03), (0, 0.0, 0.84), mat(TILES[1]), rot=(-0.5, 0, 0), parent=r)
    return r


def fire(loc, parent=None, s=1.0):
    x, y, z = loc
    cyl(0.07 * s, 0.03 * s, (x, y, z + 0.01 * s), mat((0.12, 0.07, 0.05)), parent=parent, segs=8)
    cyl(0.055 * s, 0.2 * s, (x, y, z + 0.11 * s), mat((1.0, 0.36, 0.05), emit=5.0),
        parent=parent, r_top=0.0, segs=7)
    cyl(0.035 * s, 0.14 * s, (x + 0.02 * s, y - 0.02 * s, z + 0.08 * s),
        mat((1.0, 0.8, 0.2), emit=8.0), parent=parent, r_top=0.0, segs=6)
    cyl(0.03 * s, 0.12 * s, (x - 0.035 * s, y + 0.02 * s, z + 0.07 * s),
        mat((1.0, 0.5, 0.08), emit=6.0), parent=parent, r_top=0.0, segs=6)


def brazier(loc=(0, 0, 0), parent=None, s=1.0):
    """A bronze tripod brazier with a fire, 0.5 tall at s=1."""
    x, y, z = loc
    for k in range(3):
        a = k * 2 * pi / 3 + 0.5
        rod((x + 0.13 * s * cos(a), y + 0.13 * s * sin(a), z),
            (x + 0.07 * s * cos(a), y + 0.07 * s * sin(a), z + 0.34 * s), 0.014 * s, bronze(), parent)
    lathe([(0.05, 0.3), (0.13, 0.38), (0.15, 0.44), (0.12, 0.44), (0.1, 0.4)], bronze(),
          loc, parent=parent, s=s, segs=10)
    fire((x, y, z + 0.4 * s), parent, s * 0.9)


def shield(loc, rz=0.0, parent=None, r=0.15, player=BLUE, blazon=0):
    """A round bronze hoplite shield hung on a wall plane, player-colour face."""
    e = face(loc, rz, parent)
    sphere(r, (0, 0, 0), bronze(), scale=(1, 0.28, 1), parent=e, segs=12)
    sphere(r * 0.86, (0, -r * 0.07, 0), mat(player, rough=0.5), scale=(1, 0.26, 1), parent=e, segs=12)
    g = gold()
    y = -r * 0.3
    if blazon % 3 == 0:      # boss and ring of dots
        sphere(r * 0.26, (0, y, 0), g, scale=(1, 0.4, 1), parent=e, segs=8)
    elif blazon % 3 == 1:    # lambda / chevron
        for sx in (-1, 1):
            box((r * 0.16, 0.012, r * 0.95), (sx * r * 0.2, y, 0), "white", rot=(0, sx * 0.42, 0), parent=e)
    else:                    # cross of bars
        box((r * 1.1, 0.012, r * 0.16), (0, y, 0), "white", parent=e)
        box((r * 0.16, 0.012, r * 1.1), (0, y, 0), "white", parent=e)
    return e


def spear(p0, p1, parent=None):
    a, b = Vector(p0), Vector(p1)
    rod(a, b, 0.012, "wood", parent)
    d = (b - a).normalized()
    rod(b, b + d * 0.14, 0.024, bronze(), parent, r_top=0.002)


def olive_tree(loc=(0, 0, 0), parent=None, s=1.0, seed=0):
    """A small gnarled olive, about 1.3 tall at s=1."""
    x, y, z = loc
    rnd = random.Random(seed)
    rod((x, y, z), (x + 0.06 * s, y + 0.03 * s, z + 0.5 * s), 0.07 * s, mat((0.42, 0.34, 0.26)),
        parent, 7, 0.045 * s)
    rod((x + 0.06 * s, y + 0.03 * s, z + 0.48 * s), (x - 0.12 * s, y, z + 0.8 * s), 0.04 * s,
        mat((0.42, 0.34, 0.26)), parent, 6, 0.02 * s)
    rod((x + 0.06 * s, y + 0.03 * s, z + 0.48 * s), (x + 0.2 * s, y + 0.06 * s, z + 0.8 * s),
        0.04 * s, mat((0.42, 0.34, 0.26)), parent, 6, 0.02 * s)
    for i in range(6):
        a = i * 1.1 + rnd.random()
        d = 0.24 * s * (0.4 + 0.6 * rnd.random())
        blob(0.26 * s * rnd.uniform(0.8, 1.1),
             (x + d * cos(a), y + d * sin(a), z + s * rnd.uniform(0.85, 1.15)),
             mat([(0.46, 0.58, 0.34), (0.36, 0.5, 0.28), (0.56, 0.66, 0.42)][i % 3], rough=0.9),
             (1, 1, 0.75), parent, seed + i, 0.28, 2)


def chicken(loc, rz=0.0, parent=None, colour="white"):
    e = empty("chicken", loc, rz, parent)
    sphere(0.06, (0, 0, 0.08), colour, scale=(1.3, 0.9, 1), parent=e, segs=8)
    sphere(0.035, (0.07, 0, 0.14), colour, parent=e, segs=6)
    sphere(0.018, (0.075, 0, 0.18), "flower_red", parent=e, segs=6)
    cyl(0.012, 0.035, (0.115, 0, 0.14), "yellow_ochre", rot=(0, pi / 2, 0), parent=e, r_top=0, segs=5)
    box((0.05, 0.05, 0.07), (-0.09, 0, 0.13), colour, rot=(0, -0.6, 0), parent=e)
    return e


def laundry(p0, p1, parent=None, seed=0, colours=("linen", "red_ochre", "egyptian_blue", "yellow_ochre")):
    """A line between two points with cloths hanging from it."""
    a, b = Vector(p0), Vector(p1)
    rod(a, b, 0.006, "wool", parent, 4)
    d = b - a
    rz = atan2(d.y, d.x)
    rnd = random.Random(seed)
    n = max(2, int(d.length / 0.24))
    for i in range(n):
        t = (i + 0.5) / n
        p = a + d * t
        cloth(0.17, rnd.uniform(0.2, 0.34), mat(colours[(i + seed) % len(colours)], rough=0.8),
              (p.x, p.y, p.z), rz, parent, hang=True, wave=0.012, nx=3, seed=seed + i)


def pergola(x0, x1, y0, y1, z, parent=None, seed=0, grapes=6, post="wood"):
    """Four posts, beams, slats and a vine over the top."""
    for x in (x0, x1):
        for y in (y0, y1):
            box((0.05, 0.05, z), (x, y, z / 2), post, parent=parent)
    for y in (y0, y1):
        rod((x0 - 0.06, y, z), (x1 + 0.06, y, z), 0.025, "wood_dark", parent)
    n = max(3, int((x1 - x0) / 0.14))
    for i in range(n + 1):
        x = x0 + (x1 - x0) * i / n
        rod((x, y0 - 0.07, z + 0.03), (x, y1 + 0.07, z + 0.03), 0.016, "wood", parent)
    rnd = random.Random(seed)
    pts = [(rnd.uniform(x0, x1), rnd.uniform(y0, y1), z + 0.07 + rnd.uniform(0, 0.04))
           for _ in range(int((x1 - x0) * (y1 - y0) * 34) + 6)]
    foliage(pts, 0.12, parent, seed, ("leaf", "leaf_dark", (0.36, 0.62, 0.2)), grapes=grapes)
    # the vine's stem up one post
    rod((x1 + 0.03, y0 - 0.03, 0), (x1, y0, z), 0.018, "leaf_dark", parent)


def low_wall(p0, p1, h=0.4, t=0.09, parent=None, m=None, cap="terracotta"):
    """A yard wall between two points (axis-aligned), with a tile/stone cap."""
    (x0, y0), (x1, y1) = p0, p1
    sx, sy = abs(x1 - x0) + t, abs(y1 - y0) + t
    c = ((x0 + x1) / 2, (y0 + y1) / 2)
    box((sx, sy, h), (c[0], c[1], h / 2), m or noisy("whitewash"), parent=parent, bevel=0.01)
    box((sx + 0.03, sy + 0.03, 0.08), (c[0], c[1], 0.04), courses("limestone", 12.0), parent=parent)
    if cap:
        box((sx + 0.04, sy + 0.04, 0.045), (c[0], c[1], h + 0.02), cap, parent=parent, bevel=0.012)


def pier(x, y, h, parent=None, m=None, cap="terracotta", t=0.15, ball=None):
    """A gate or corner post for yard walls."""
    box((t, t, h), (x, y, h / 2), m or noisy("whitewash"), parent=parent, bevel=0.01)
    box((t + 0.04, t + 0.04, 0.05), (x, y, h + 0.02), cap, parent=parent, bevel=0.012)
    if ball:
        sphere(0.05, (x, y, h + 0.08), ball, parent=parent, segs=8)


# ----------------------------------------------------------------- temple


def _pediment(e, w, h, figures=True):
    """In frame e (facing local -Y, base centred on origin): blue tympanum,
    raking cornices, painted figures."""
    mesh([(-w / 2, 0, 0), (w / 2, 0, 0), (0, 0, h)], [(0, 1, 2)], mat("egyptian_blue", rough=0.7), parent=e)
    box((w, 0.3, 0.02), (0, 0.15, 0.0), "marble", parent=e)
    L = hypot(w / 2, h)
    a = atan2(h, w / 2)
    for sg in (-1, 1):
        cx, cz = sg * w / 4, h / 2
        box((L + 0.12, 0.14, 0.07), (cx - sg * 0.01, -0.03, cz + 0.035), "marble",
            rot=(0, sg * a, 0), parent=e)
        box((L + 0.12, 0.15, 0.022), (cx - sg * 0.01, -0.035, cz + 0.075), "red_ochre",
            rot=(0, sg * a, 0), parent=e)
        box((L + 0.05, 0.02, 0.025), (cx, -0.105, cz - 0.01), gold(), rot=(0, sg * a, 0), parent=e)
    if not figures:
        return
    y = -0.045
    # the god in the middle, attendants shrinking toward the corners
    skin, robe = mat("marble"), None
    cols = ["yellow_ochre", "red_ochre", "white", "yellow_ochre", "red_ochre"]
    for i, fx in enumerate((0.0, 0.2, 0.4, 0.62, 0.86)):
        for sg in ((1,) if i == 0 else (-1, 1)):
            x = sg * fx * w / 2
            room = h * (1 - fx) - 0.05
            fh = max(0.09, min(room * 0.86, 0.4))
            if i >= 3:       # reclining figures and a corner beast
                box((0.16, 0.04, 0.06), (x, y, 0.045), cols[i], parent=e, rot=(0, sg * 0.15, 0))
                sphere(0.032, (x - sg * 0.09, y, 0.085), skin, parent=e, segs=6)
            else:
                box((0.085, 0.04, fh * 0.72), (x, y, fh * 0.36 + 0.01), cols[i], parent=e)
                sphere(0.036, (x, y, fh * 0.78 + 0.02), skin, parent=e, segs=6)
                if i == 0:   # spear and shield for the god
                    rod((x + 0.08, y, 0.02), (x + 0.08, y, fh + 0.06), 0.008, gold(), e)
                    cyl(0.055, 0.02, (x - 0.085, y - 0.01, fh * 0.45), gold(), rot=(pi / 2, 0, 0),
                        parent=e, segs=8)
                else:
                    box((0.03, 0.03, 0.1), (x - sg * 0.055, y, fh * 0.5), cols[i], parent=e,
                        rot=(0, sg * 0.7, 0))


def _acroterion(loc, parent, s=1.0, rz=0.0):
    e = face(loc, rz, parent)
    g = gold()
    box((0.1 * s, 0.1 * s, 0.05 * s), (0, 0, 0.025 * s), "marble", parent=e)
    for k in range(-2, 3):
        a = k * 0.42
        rod((0, 0, 0.05 * s), (sin(a) * 0.2 * s, 0, 0.05 * s + cos(a) * 0.2 * s), 0.028 * s, g, e, 6, 0.008 * s)
    sphere(0.045 * s, (0, 0, 0.07 * s), g, parent=e, segs=8)
    return e


def temple(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """3 x 3. A peripteral Doric temple, front (pediment, doors, altar) on -Y."""
    r = empty("temple", loc, rot_z, parent)
    yc = 0.3
    # stylobate: three steps
    for i, (sx, sy) in enumerate(((2.92, 2.34), (2.78, 2.2), (2.64, 2.06))):
        box((sx, sy, 0.09), (0, yc, 0.045 + i * 0.09), courses("marble", 5.0, 0.8, 0.07) if i < 2 else noisy("marble", 0.06, 3.0),
            parent=r, bevel=0.012)
    z0 = 0.27
    # front stair and painted floor strip
    for i in range(2):
        box((0.9, 0.14, 0.09 * (2 - i)), (0, yc - 1.17 - 0.07 - i * 0.12, 0.045 * (2 - i)),
            noisy("marble", 0.06, 3.0), parent=r, bevel=0.01)
    box((0.7, 0.5, 0.012), (0, -0.5, z0 + 0.004), mat("red_ochre"), parent=r)
    # cella
    cx, cy0, cy1, ch = 0.74, -0.14, 1.06, 1.36
    wall_block(cx * 2, cy1 - cy0, ch, (0, (cy0 + cy1) / 2, z0), r, wall="plaster",
               dado="red_ochre", line="yellow_ochre", base=None, dado_h=0.34, base_h=0.0)
    box((cx * 2 + 0.02, cy1 - cy0 + 0.02, 0.07), (0, (cy0 + cy1) / 2, z0 + ch - 0.2),
        "egyptian_blue", parent=r)
    door((0, cy0, z0), 0, r, w=0.56, h=1.02, metal=True, frame="egyptian_blue", lintel=gold())
    for sx in (-1, 1):   # painted panels either side of the door, and on the flank
        box((0.2, 0.02, 0.4), (sx * 0.55, cy0 - 0.012, z0 + 0.75), "yellow_ochre", parent=r)
        box((0.13, 0.024, 0.3), (sx * 0.55, cy0 - 0.014, z0 + 0.75), "red_ochre", parent=r)
    # colonnade
    ch_ = 1.36
    xs = [-1.16 + i * 2.32 / 5 for i in range(6)]
    ys = [-0.6 + i * 1.8 / 4 for i in range(5)]
    pts = [(x, ys[0]) for x in xs] + [(x, ys[-1]) for x in xs]
    pts += [(sx * 1.16, y) for y in ys[1:-1] for sx in (-1, 1)]
    for x, y in pts:
        column((x, y, z0), ch_, 0.105, r, band="red_ochre", band2="egyptian_blue")
    z1 = z0 + ch_
    # entablature
    ex, ey0, ey1 = 1.3, -0.74, 1.34
    ec = (0, (ey0 + ey1) / 2)
    box((ex * 2, ey1 - ey0, 0.15), (ec[0], ec[1], z1 + 0.075), noisy("marble", 0.05, 3.0), parent=r)
    box((ex * 2 + 0.03, ey1 - ey0 + 0.03, 0.028), (ec[0], ec[1], z1 + 0.15), gold(), parent=r)
    zf, fh = z1 + 0.164, 0.2
    box((ex * 2 - 0.02, ey1 - ey0 - 0.02, fh), (ec[0], ec[1], zf + fh / 2), mat("red_ochre", rough=0.7), parent=r)
    tri = mat("egyptian_blue", rough=0.6)

    def frieze(n, length, mk):
        for i in range(n):
            u = -length / 2 + 0.06 + i * (length - 0.12) / (n - 1)
            mk(u, 0.11, tri, 0.014)                 # triglyph
            for k in (-1, 1):
                mk(u + k * 0.028, 0.012, mat("lapis"), 0.018)
            if i < n - 1:                           # a gold boss on each red metope
                um = u + (length - 0.12) / (n - 1) / 2
                mk(um, 0.05, gold(), 0.006, 0.05)

    for sg in (-1, 1):
        frieze(11, ex * 2, lambda u, w, m, d, h=fh: box(
            (w, 0.03, h), (u, (ey0 if sg < 0 else ey1) + sg * (d - 0.005), zf + fh / 2), m, parent=r))
        frieze(9, ey1 - ey0, lambda u, w, m, d, h=fh: box(
            (0.03, w, h), (sg * (ex + d - 0.005), ec[1] + u, zf + fh / 2), m, parent=r))
    z2 = zf + fh
    box((ex * 2 + 0.14, ey1 - ey0 + 0.14, 0.07), (ec[0], ec[1], z2 + 0.035), "marble", parent=r, bevel=0.01)
    box((ex * 2 + 0.06, ey1 - ey0 + 0.06, 0.03), (ec[0], ec[1], z2 - 0.005), "yellow_ochre", parent=r)
    z3 = z2 + 0.07
    # roof, ridge along Y so the pediment faces the sunlit front
    rh = 0.6
    gable_roof(ey1 - ey0 - 0.04, ex * 2 + 0.06, rh, (0, ec[1], z3), r, axis="y", over=0.06,
               antefix=gold(), seed=3)
    _pediment(face((0, ey0 - 0.02, z3), 0, r), ex * 2 + 0.1, rh)
    _pediment(face((0, ey1 + 0.02, z3), pi, r), ex * 2 + 0.1, rh, figures=False)
    for y, rz in ((ey0 - 0.06, 0), (ey1 + 0.06, pi)):
        _acroterion((0, y, z3 + rh + 0.06), r, 1.25, rz)
        for sx in (-1, 1):
            _acroterion((sx * (ex + 0.02), y, z3 + 0.02), r, 0.8, rz)
    # altar with fire, offerings, laurels and banners out front
    ay = -1.24
    box((0.62, 0.36, 0.05), (0, ay, 0.025), noisy("marble", 0.06, 3.0), parent=r, bevel=0.01)
    box((0.5, 0.26, 0.26), (0, ay, 0.18), noisy("marble", 0.06, 3.0), parent=r, bevel=0.01)
    box((0.51, 0.27, 0.05), (0, ay, 0.2), "egyptian_blue", parent=r)
    box((0.51, 0.27, 0.02), (0, ay, 0.245), gold(), parent=r)
    box((0.56, 0.32, 0.04), (0, ay, 0.33), "marble", parent=r, bevel=0.01)
    for sx in (-1, 1):
        box((0.06, 0.3, 0.05), (sx * 0.25, ay, 0.375), "red_ochre", parent=r)
    fire((0, ay, 0.35), r, 1.25)
    for sx, fr in ((-1, 0), (1, 1)):
        x = sx * 0.52
        lathe([(0.05, 0), (0.025, 0.04), (0.025, 0.2), (0.1, 0.27), (0.11, 0.3), (0.09, 0.3)],
              bronze(), (x, ay - 0.02, 0), parent=r, segs=10)
        for k in range(5):
            sphere(0.032, (x + 0.045 * cos(k * 1.3), ay - 0.02 + 0.045 * sin(k * 1.3), 0.31),
                   mat(FRUIT[fr + (k % 2) * 2], rough=0.4), parent=r, segs=6)
        pot((sx * 0.92, ay + 0.04, 0), r, 0.2, "leaf_dark", seed=4 + sx)
        banner((sx * 1.3, ay + 0.02, 0), 1.75 if sx < 0 else 1.25, player, r, seed=sx + 2,
               bh=0.62 if sx < 0 else 0.5)
    amphora((-0.7, ay - 0.1, 0), 0.4, parent=r, s=0.42, colour=(0.9, 0.62, 0.3), band="black")
    return r


# ------------------------------------------------------------ town centre


def town_center(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """3 x 3. A two-storey hall with a portico, a lower wing, and a paved
    market court (well, cart, awning) open to -Y / +X."""
    r = empty("town_center", loc, rot_z, parent)
    box((2.94, 2.94, 0.05), (0, 0, 0.022), courses("limestone", 3.5, 0.82, 0.1), parent=r, bevel=0.012)
    # main hall, two storeys, ridge along X
    mx0, mx1, my0, my1 = -1.4, 1.4, 0.5, 1.4
    mc = ((mx0 + mx1) / 2, (my0 + my1) / 2)
    h1, h2 = 1.15, 0.9
    wall_block(mx1 - mx0, my1 - my0, h1 + h2, (mc[0], mc[1], 0.03), r, dado_h=0.3)
    zs = 0.03 + h1
    box((mx1 - mx0 + 0.05, my1 - my0 + 0.05, 0.1), (mc[0], mc[1], zs), "egyptian_blue", parent=r)
    box((mx1 - mx0 + 0.07, my1 - my0 + 0.07, 0.025), (mc[0], mc[1], zs + 0.06), "yellow_ochre", parent=r)
    zt = 0.03 + h1 + h2
    box((mx1 - mx0 + 0.06, my1 - my0 + 0.06, 0.09), (mc[0], mc[1], zt - 0.045), "red_ochre", parent=r)
    for i in range(14):      # meander suggested by gold blocks along the top band
        box((0.07, 0.02, 0.04), (mx0 + 0.15 + i * 0.2, my0 - 0.035, zt - 0.045), gold(), parent=r)
    for i in range(4):
        box((0.02, 0.07, 0.04), (mx1 + 0.035, my0 + 0.16 + i * 0.2, zt - 0.045), gold(), parent=r)
    gable_roof(mx1 - mx0, my1 - my0, 0.52, (mc[0], mc[1], zt), r, over=0.13, gable=noisy("whitewash"), seed=11)
    cyl(0.13, 0.03, (mx1 + 0.01, mc[1], zt + 0.17), "egyptian_blue", rot=(0, pi / 2, 0), parent=r, segs=12)
    cyl(0.07, 0.04, (mx1 + 0.015, mc[1], zt + 0.17), gold(), rot=(0, pi / 2, 0), parent=r, segs=10)
    for sx in (-1, 1):
        _acroterion((sx * (mx1 + 0.12), mc[1], zt + 0.58), r, 0.8, pi / 2)
    for i in range(5):       # upper windows over the portico
        window((-0.25 + i * 0.37, my0, 0.03 + h1 + 0.5), 0, r, 0.17, 0.3)
    window((mx1, mc[1] - 0.2, 0.03 + h1 + 0.48), pi / 2, r, 0.17, 0.3)
    window((mx1, mc[1] + 0.22, 0.03 + h1 + 0.48), pi / 2, r, 0.17, 0.3)
    window((mx1, mc[1], 0.75), pi / 2, r, 0.2, 0.3, colour="red_ochre")
    # west wing, one storey, ridge along Y
    wx0, wx1, wy0, wy1 = -1.4, -0.5, -1.36, 0.5
    wc = ((wx0 + wx1) / 2, (wy0 + wy1) / 2)
    hw = 1.2
    wall_block(wx1 - wx0, wy1 - wy0, hw, (wc[0], wc[1], 0.03), r, dado="egyptian_blue", line="yellow_ochre")
    box((wx1 - wx0 + 0.05, wy1 - wy0 + 0.05, 0.07), (wc[0], wc[1], 0.03 + hw - 0.035), "red_ochre", parent=r)
    gable_roof(wy1 - wy0, wx1 - wx0, 0.42, (wc[0], wc[1], 0.03 + hw), r, axis="y", over=0.12,
               gable=noisy("whitewash"), seed=17)
    _acroterion((wc[0], wy0 - 0.12, 0.03 + hw + 0.47), r, 0.75, 0)
    window((wc[0] - 0.02, wy0, 0.74), 0, r, 0.22, 0.32)
    cyl(0.09, 0.03, (wc[0], wy0 - 0.01, 0.03 + hw + 0.16), "egyptian_blue", rot=(pi / 2, 0, 0), parent=r, segs=10)
    door((wx1, -0.05, 0.04), pi / 2, r, w=0.34, h=0.86, colour="red_ochre")
    # portico along the hall's front
    px0, px1, py = -0.5, 1.4, 0.02
    shed_roof(px1 - px0 - 0.04, my0 - py + 0.1, 0.27, ((px0 + px1) / 2 + 0.02, (my0 + py) / 2 - 0.05, 1.06), r, over=0.06, seed=5)
    box((px1 - px0, 0.09, 0.09), ((px0 + px1) / 2, py + 0.06, 1.02), "wood_dark", parent=r)
    box((0.09, my0 - py, 0.09), (px1 - 0.05, (my0 + py) / 2, 1.02), "wood_dark", parent=r)
    for i in range(4):
        column((px0 + 0.2 + i * 0.55, py + 0.06, 0.045), 0.93, 0.075, r, band="egyptian_blue",
               lower=mat("red_ochre", rough=0.7), flutes=8)
    box((px1 - px0, my0 - py + 0.05, 0.03), ((px0 + px1) / 2, (my0 + py) / 2, 0.055), noisy("marble", 0.06), parent=r)
    door((0.45, my0, 0.06), 0, r, w=0.5, h=0.9, colour="egyptian_blue", lintel=gold())
    bench((1.08, my0 - 0.12, 0.06), 0, r, 0.4)
    amphora((-0.2, my0 - 0.14, 0.06), 0.3, parent=r, s=0.5)
    # market awning on the wing's court side
    ay0, ay1 = -1.25, -0.4
    awning((wx1 + 0.02, (ay0 + ay1) / 2, 1.0), pi / 2, r, w=ay1 - ay0, d=0.62, drop=0.24, player=player)
    for y in (ay0 + 0.03, ay1 - 0.03):
        rod((wx1 + 0.62, y, 0.04), (wx1 + 0.62, y, 0.77), 0.02, "wood_dark", r)
    box((0.3, 0.66, 0.04), (wx1 + 0.3, -0.84, 0.36), "wood", parent=r)
    for y in (-1.12, -0.56):
        box((0.26, 0.04, 0.32), (wx1 + 0.3, y, 0.2), "wood_dark", parent=r)
    for i, y in enumerate((-1.05, -0.84, -0.63)):
        basket((wx1 + 0.3, y, 0.38), r, 0.1, i, i)
    crate((wx1 + 0.72, -1.2, 0.045), 0.2, r, 0.24, fruit=3, seed=2)
    amphora((wx1 + 0.7, -0.42, 0.045), 0.8, parent=r, s=0.5, colour=(0.88, 0.6, 0.32), band="red_ochre")
    # court furniture
    well((0.98, -0.42, 0.045), 0.5, r)
    cart((0.52, -1.08, 0.045), 0.35, player, r)
    crate((1.22, -1.16, 0.045), -0.2, r, 0.3)
    crate((1.2, -1.14, 0.29), 0.5, r, 0.22, fruit=0, seed=5)
    amphora((1.3, -0.82, 0.045), 0.2, parent=r, s=0.55)
    amphora((1.12, -0.86, 0.045), 1.0, parent=r, s=0.46, colour=(0.62, 0.3, 0.2), band="yellow_ochre")
    sack((0.05, -0.38, 0.045), r, 0.13, 7)
    sack((0.2, -0.3, 0.045), r, 0.11, 8, colour="linen")
    pot((-0.36, -0.14, 0.045), r, 0.2, "leaf", flowers="flower_red", seed=3)
    # the flag: a tall pole on the ridge
    flag((0.3, mc[1], zt + 0.5), 1.25, player, r, w=1.0, fh=0.62, pole_r=0.03, seed=2)
    return r


# ----------------------------------------------------------------- houses


def _yard(r, m="sand"):
    box((1.92, 1.92, 0.035), (0, 0, 0.012), noisy(m, 0.1, 3.0, 1.0), parent=r, bevel=0.012)


def house(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, variant=0):
    """2 x 2. variant 0: cottage with a vine pergola; 1: two storeys with an
    outside stair and a roof terrace; 2: courtyard house with a lean-to roof."""
    r = empty("house%d" % variant, loc, rot_z, parent)
    _yard(r)
    z = 0.03
    if variant == 0:
        x0, x1, y0, y1, h = -0.95, 0.32, -0.02, 0.95, 1.05
        c = ((x0 + x1) / 2, (y0 + y1) / 2)
        wall_block(x1 - x0, y1 - y0, h, (c[0], c[1], z), r, dado="red_ochre", line="egyptian_blue")
        gable_roof(x1 - x0, y1 - y0, 0.46, (c[0], c[1], z + h), r, over=0.13, gable=noisy("whitewash"), seed=1)
        door((-0.08, y0, z), 0, r, 0.36, 0.82, "egyptian_blue")
        window((-0.62, y0, z + 0.66), 0, r, 0.2, 0.28, "egyptian_blue")
        window((x1, c[1] + 0.02, z + 0.66), pi / 2, r, 0.2, 0.28, "egyptian_blue")
        cyl(0.07, 0.03, (x1 + 0.01, c[1], z + h + 0.17), "egyptian_blue", rot=(0, pi / 2, 0), parent=r, segs=10)
        pergola(x1 + 0.1, 0.9, 0.08, 0.86, 0.92, r, seed=4)
        bench((x1 + 0.16, 0.45, z), pi / 2, r, 0.5)
        low_wall((-0.93, -0.93), (-0.18, -0.93), 0.36, parent=r)
        low_wall((0.28, -0.93), (0.93, -0.93), 0.36, parent=r)
        low_wall((0.93, -0.93), (0.93, -0.05), 0.36, parent=r)
        for x in (-0.18, 0.28):
            pier(x, -0.93, 0.5, r, ball="terracotta")
        pier(0.93, -0.93, 0.44, r)
        pier(-0.93, -0.93, 0.44, r)
        pithos((0.62, -0.52, z), r, 0.5)
        olive_tree((-0.66, -0.56, z), r, 0.85, seed=2)
        pot((-0.36, y0 - 0.12, z), r, 0.17, "leaf", flowers="flower_red", seed=1)
        pot((0.2, y0 - 0.1, z), r, 0.15, "leaf", flowers="flower_yellow", seed=2)
        amphora((-0.84, y0 - 0.12, z), 0.3, parent=r, s=0.44)
        basket((0.32, -0.5, z), r, 0.11, 4, 3)
        flag((x0 + 0.06, c[1], z + h + 0.46), 0.5, player, r, w=0.34, fh=0.2, pennant=True)
    elif variant == 1:
        x0, x1, y0, y1 = -0.92, 0.22, 0.05, 0.95
        h1, h2 = 0.98, 0.84
        c = ((x0 + x1) / 2, (y0 + y1) / 2)
        wall_block(x1 - x0, y1 - y0, h1 + h2, (c[0], c[1], z), r, dado="egyptian_blue", line="yellow_ochre")
        box((x1 - x0 + 0.04, y1 - y0 + 0.04, 0.06), (c[0], c[1], z + h1), "red_ochre", parent=r)
        gable_roof(x1 - x0, y1 - y0, 0.42, (c[0], c[1], z + h1 + h2), r, over=0.12, gable=noisy("whitewash"), seed=21)
        # the outside stair along the sunlit wall, up to a landing and a red door
        n, sw = 6, 0.3
        for i in range(n):
            hh = (i + 1) * h1 / (n + 1)
            box((0.15, sw, hh), (x1 - 0.1 - i * 0.14, y0 - sw / 2, z + hh / 2),
                noisy("whitewash") if i % 2 else noisy("plaster"), parent=r, bevel=0.008)
        lx = x1 - 0.1 - n * 0.14 - 0.15
        box((0.44, sw, h1), (lx, y0 - sw / 2, z + h1 / 2), noisy("whitewash"), parent=r, bevel=0.008)
        box((0.48, sw + 0.03, 0.04), (lx, y0 - sw / 2, z + h1 + 0.01), "wood", parent=r)
        door((lx, y0, z + h1 + 0.02), 0, r, 0.3, 0.66, "red_ochre")
        rod((lx - 0.2, y0 - sw, z + h1 + 0.02), (lx - 0.2, y0 - sw, z + h1 + 0.3), 0.016, "wood_dark", r)
        rod((lx - 0.2, y0 - sw, z + h1 + 0.3), (lx + 0.22, y0 - sw, z + h1 + 0.3), 0.014, "wood_dark", r)
        rod((lx + 0.22, y0 - sw, z + h1 + 0.3), (x1 - 0.05, y0 - sw, z + 0.42), 0.014, "wood_dark", r)
        window((x1 - 0.3, y0, z + h1 + 0.44), 0, r, 0.18, 0.26, "red_ochre")
        window((x1, c[1], z + h1 + 0.44), pi / 2, r, 0.18, 0.26, "red_ochre")
        door((x1, c[1] - 0.14, z), pi / 2, r, 0.3, 0.74, "red_ochre")
        # flat-roofed annex with a terrace
        ax0, ax1, ay0, ay1, ah = x1, 0.93, 0.3, 0.95, 0.86
        ac = ((ax0 + ax1) / 2, (ay0 + ay1) / 2)
        wall_block(ax1 - ax0, ay1 - ay0, ah, (ac[0], ac[1], z), r, dado="egyptian_blue", line="yellow_ochre")
        box((ax1 - ax0 + 0.05, ay1 - ay0 + 0.05, 0.06), (ac[0], ac[1], z + ah + 0.02), noisy("plaster"), parent=r)
        box((ax1 - ax0 - 0.06, ay1 - ay0 - 0.06, 0.02), (ac[0], ac[1], z + ah + 0.05), mat("mudbrick"), parent=r)
        window((ax1, ac[1], z + 0.56), pi / 2, r, 0.16, 0.22, "red_ochre")
        pot((ax1 - 0.12, ay0 + 0.1, z + ah + 0.05), r, 0.14, "leaf", flowers="flower_red", seed=5)
        pot((ax1 - 0.12, ay1 - 0.12, z + ah + 0.05), r, 0.16, "leaf_olive", seed=6)
        sack((ax0 + 0.2, ay1 - 0.2, z + ah + 0.05), r, 0.1, 2)
        # laundry from the annex to a pole in the yard
        rod((0.84, -0.8, z), (0.84, -0.8, z + 0.95), 0.02, "wood_dark", r)
        laundry((0.84, -0.8, z + 0.92), (ax1 - 0.05, ay0, z + ah + 0.04), r, seed=1)
        low_wall((0.93, -0.93), (0.1, -0.93), 0.32, parent=r)
        pier(0.1, -0.93, 0.42, r, ball="terracotta")
        pier(0.93, -0.93, 0.42, r)
        pithos((0.5, -0.1, z), r, 0.46)
        amphora((0.68, 0.14, z), 0.6, parent=r, s=0.42)
        bench((-0.2, -0.62, z), 0.1, r, 0.46)
        basket((-0.56, -0.66, z), r, 0.12, 1, 1)
        pot((x1 + 0.13, y0 - 0.38, z), r, 0.16, "leaf", flowers="flower_yellow", seed=7)
        olive_tree((-0.74, -0.74, z), r, 0.7, seed=5)
        flag((x1 - 0.06, c[1], z + h1 + h2 + 0.42), 0.5, player, r, w=0.34, fh=0.2, pennant=True)
    else:
        # back range with a lean-to roof toward the yard
        x0, x1, y0, y1, h = -0.95, 0.95, 0.3, 0.95, 1.0
        c = ((x0 + x1) / 2, (y0 + y1) / 2)
        wall_block(x1 - x0, y1 - y0, h, (c[0], c[1], z), r, dado="yellow_ochre", line="red_ochre")
        rise = 0.42
        # side walls up to the roof line
        for sx in (x0, x1):
            mesh([(sx, y0, z + h), (sx, y1, z + h), (sx, y1, z + h + rise)], [(0, 1, 2)], noisy("whitewash"), parent=r)
        box((x1 - x0, 0.06, rise), (c[0], y1 - 0.03, z + h + rise / 2), noisy("whitewash"), parent=r)
        shed_roof(x1 - x0, y1 - y0, rise, (c[0], c[1], z + h), r, over=0.11, seed=31)
        door((0.34, y0, z), 0, r, 0.36, 0.8, "egyptian_blue")
        window((0.76, y0, z + 0.64), 0, r, 0.15, 0.26, "egyptian_blue")
        window((x1, c[1], z + 0.64), pi / 2, r, 0.18, 0.26, "egyptian_blue")
        # west wing with flat roof, parapet, a cloth shade
        wx0, wx1, wy0, wy1, hw = -0.95, -0.28, -0.93, 0.3, 0.9
        wc = ((wx0 + wx1) / 2, (wy0 + wy1) / 2)
        wall_block(wx1 - wx0, wy1 - wy0, hw, (wc[0], wc[1], z), r, dado="yellow_ochre", line="red_ochre")
        box((wx1 - wx0 + 0.05, wy1 - wy0 + 0.05, 0.07), (wc[0], wc[1], z + hw + 0.02), noisy("plaster"), parent=r)
        box((wx1 - wx0 - 0.07, wy1 - wy0 - 0.07, 0.02), (wc[0], wc[1], z + hw + 0.055), mat("mudbrick"), parent=r)
        window((wc[0], wy0, z + 0.58), 0, r, 0.18, 0.26, "egyptian_blue")
        door((wx1, -0.2, z), pi / 2, r, 0.3, 0.72, "egyptian_blue")
        zt = z + hw + 0.06
        for (px, py) in ((wx0 + 0.08, wy0 + 0.08), (wx1 - 0.08, wy0 + 0.08), (wx0 + 0.08, -0.25), (wx1 - 0.08, -0.25)):
            rod((px, py, zt), (px, py, zt + 0.5), 0.016, "wood_dark", r)
        awning((wc[0], -0.22, zt + 0.52), 0, r, w=wx1 - wx0 - 0.1, d=0.62, drop=0.06, player=player, stripes=4, valance=0.05)
        pot((wx1 - 0.14, 0.14, zt), r, 0.15, "leaf", flowers="flower_red", seed=9)
        amphora((wx0 + 0.2, 0.1, zt), 0.3, parent=r, s=0.34)
        # yard: wall with a gate, vine on the wall, jar, chickens
        low_wall((wx1, -0.93), (0.3, -0.93), 0.34, parent=r)
        low_wall((0.72, -0.93), (0.93, -0.93), 0.34, parent=r)
        low_wall((0.93, -0.93), (0.93, 0.3), 0.34, parent=r)
        for x in (0.3, 0.72):
            pier(x, -0.93, 0.5, r, ball="terracotta", t=0.12)
        pier(0.93, -0.93, 0.42, r)
        box((0.36, 0.035, 0.4), (0.51, -0.93, 0.23), mat("egyptian_blue", rough=0.6), parent=r)
        for zz in (0.12, 0.34):
            box((0.4, 0.045, 0.035), (0.51, -0.93, zz), mat(tint("egyptian_blue", 0.6)), parent=r)
        foliage([(-0.15 + i * 0.09, y0 - 0.05, z + 0.5 + 0.35 * abs(sin(i * 1.3))) for i in range(5)] +
                [(-0.2 + i * 0.1, y0 - 0.05, z + 0.9) for i in range(4)], 0.1, r, 3,
                ("leaf", "leaf_dark"), flowers=(0.85, 0.2, 0.6))
        rod((-0.14, y0 - 0.04, z), (-0.1, y0 - 0.03, z + 0.8), 0.016, "leaf_dark", r)
        pithos((0.7, 0.0, z), r, 0.5)
        pithos((0.42, -0.02, z), r, 0.38, (0.8, 0.5, 0.3))
        chicken((0.25, -0.35, z), 0.6, r)
        chicken((0.5, -0.5, z), 2.4, r, (0.72, 0.42, 0.2))
        chicken((0.02, -0.55, z), -1.0, r)
        basket((0.72, -0.62, z), r, 0.12, 4, 6)
        bench((0.0, y0 - 0.34, z), 0, r, 0.4)
    return r


# --------------------------------------------------------------- barracks


def barracks(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A walled drill yard, gate on -Y, colonnaded stoa along the back."""
    r = empty("barracks", loc, rot_z, parent)
    _yard(r)
    z = 0.03
    # stoa: rooms at the back, lean-to roof over a colonnade
    bx0, bx1, by0, by1, bh = -0.95, 0.95, 0.56, 0.95, 1.3
    wall_block(bx1 - bx0, by1 - by0, bh, (0, (by0 + by1) / 2, z), r, dado="red_ochre", line="yellow_ochre", dado_h=0.3)
    fy = 0.06
    rise = 0.5
    zr = z + 1.06
    for sx in (bx0, bx1):
        mesh([(sx, by0, z + bh), (sx, by1, z + bh), (sx, by1, zr + rise), (sx, by0, zr + rise * (by0 - fy) / (by1 - fy))],
             [(0, 1, 2, 3)], noisy("whitewash"), parent=r)
    shed_roof(bx1 - bx0, by1 - fy, rise, (0, (by1 + fy) / 2, zr), r, over=0.09, seed=41)
    box((bx1 - bx0, 0.08, 0.08), (0, fy + 0.05, zr - 0.035), "wood_dark", parent=r)
    for i in range(5):
        column((bx0 + 0.1 + i * 0.425, fy + 0.05, z), 1.0, 0.07, r, band="egyptian_blue",
               lower=mat("red_ochre", rough=0.7), flutes=8)
    box((bx1 - bx0, by0 - fy + 0.02, 0.03), (0, (by0 + fy) / 2, z + 0.012), noisy("limestone", 0.08), parent=r)
    door((-0.62, by0, z + 0.02), 0, r, 0.3, 0.8, "red_ochre")
    # rack of spears and shields against the stoa wall
    rod((-0.2, by0 - 0.05, z + 0.62), (0.86, by0 - 0.05, z + 0.62), 0.018, "wood_dark", r)
    rod((-0.2, by0 - 0.05, z + 0.2), (0.86, by0 - 0.05, z + 0.2), 0.018, "wood_dark", r)
    for i in range(6):
        x = -0.14 + i * 0.19
        spear((x, by0 - 0.1, z + 0.03), (x + 0.02, by0 - 0.04, z + 0.98), r)
    for i in range(3):
        shield((0.0 + i * 0.36, by0 - 0.1, z + 0.4), 0, r, 0.15, player, i)
    # yard walls with a gate
    stone = courses("limestone", 9.0)
    wh = 0.5
    for a, b in (((-0.93, -0.93), (-0.3, -0.93)), ((0.3, -0.93), (0.93, -0.93)),
                 ((0.93, -0.93), (0.93, fy)), ((-0.93, -0.93), (-0.93, fy))):
        low_wall(a, b, wh, 0.1, r, m=stone, cap="red_ochre")
    for sx in (-1, 1):
        pier(sx * 0.93, -0.93, 0.58, r, stone, "red_ochre")
        box((0.16, 0.16, 0.95), (sx * 0.3, -0.93, 0.475), stone, parent=r, bevel=0.012)
        box((0.2, 0.2, 0.05), (sx * 0.3, -0.93, 0.97), "red_ochre", parent=r)
        fire((sx * 0.3, -0.93, 0.99), r, 0.7)
    box((0.86, 0.1, 0.09), (0, -0.93, 0.9), "wood_dark", parent=r)
    cloth(0.4, 0.24, mat(player, rough=0.7), (0, -0.99, 0.86), 0, r, hang=True, wave=0.01)
    cyl(0.06, 0.015, (0, -1.0, 0.75), gold(), rot=(pi / 2, 0, 0), parent=r, segs=8)
    # shields on the outer walls
    shield((-0.68, -0.99, 0.3), 0, r, 0.14, player, 1)
    shield((0.68, -0.99, 0.3), 0, r, 0.14, player, 2)
    for i, y in enumerate((-0.66, -0.3)):
        shield((0.99, y, 0.3), pi / 2, r, 0.14, player, i)
    # the yard: training dummy, target, weapon stand
    dx, dy = 0.48, -0.4
    cyl(0.03, 0.9, (dx, dy, z + 0.45), "wood_dark", parent=r, segs=6)
    box((0.14, 0.14, 0.04), (dx, dy, z + 0.02), "wood_dark", parent=r)
    cyl(0.1, 0.34, (dx, dy, z + 0.62), "straw", parent=r, segs=8, r_top=0.085)
    for zz in (0.52, 0.7):
        cyl(0.104, 0.025, (dx, dy, z + zz), "leather", parent=r, segs=8)
    rod((dx - 0.26, dy - 0.02, z + 0.72), (dx + 0.26, dy + 0.02, z + 0.72), 0.022, "wood", r)
    sphere(0.085, (dx, dy, z + 0.92), "wool", parent=r, segs=8)
    sphere(0.09, (dx, dy, z + 0.95), bronze(), scale=(1, 1, 0.75), parent=r, segs=8)
    box((0.02, 0.1, 0.1), (dx, dy, z + 1.04), mat(player), parent=r)
    shield((dx - 0.2, dy - 0.07, z + 0.62), 0.3, r, 0.13, player, 0)
    # straw archery target on a tripod
    tx, ty = -0.52, -0.36
    for k, (c_, rr) in enumerate((("straw", 0.2), ("flower_red", 0.14), ("white", 0.08), ("flower_red", 0.035))):
        cyl(rr, 0.06 + k * 0.012, (tx, ty, z + 0.42), c_, rot=(pi / 2 - 0.25, 0, -0.5), parent=r, segs=12)
    for sx in (-1, 1):
        rod((tx + sx * 0.13, ty + sx * 0.08 - 0.02, z), (tx, ty + 0.03, z + 0.56), 0.016, "wood_dark", r)
    rod((tx - 0.1, ty + 0.2, z), (tx, ty + 0.03, z + 0.56), 0.016, "wood_dark", r)
    # stacked spears, helmet, a bench
    for k in range(3):
        a = k * 2.1
        spear((0.02 + 0.1 * cos(a), -0.62 + 0.1 * sin(a), z), (0.02, -0.62, z + 0.9), r)
    bench((-0.45, -0.72, z), 0, r, 0.4)
    sphere(0.06, (-0.5, -0.72, z + 0.26), bronze(), parent=r, segs=8)
    box((0.015, 0.1, 0.07), (-0.5, -0.72, z + 0.33), mat(player), parent=r)
    banner((0.86, -0.02, z), 1.9, player, r, 0.34, 0.66, seed=3)
    return r


# ----------------------------------------------------------- small things


def watch_tower(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """1 x 1. Stone base, timber lookout with a tiled cap, brazier, pennant."""
    r = empty("watch_tower", loc, rot_z, parent)
    box((0.96, 0.96, 0.08), (0, 0, 0.04), courses("limestone", 10.0), parent=r, bevel=0.012)
    hb = 1.3
    pyramid(0.84, hb, (0, 0, 0.06), courses("limestone", 8.0, 0.7, 0.2), parent=r, top=0.7)
    box((0.76, 0.76, 0.07), (0, 0, 0.06 + hb * 0.62), mat("red_ochre", rough=0.8), parent=r)
    box((0.87, 0.87, 0.1), (0, 0, 0.13), courses("sandstone", 10.0), parent=r)
    door((0.0, -0.415, 0.08), 0, r, 0.26, 0.6, "egyptian_blue")
    for zz in (1.08,):
        box((0.07, 0.03, 0.22), (0, -0.365, zz), mat((0.15, 0.1, 0.08)), parent=r)
        box((0.03, 0.07, 0.22), (0.365, 0, zz), mat((0.15, 0.1, 0.08)), parent=r)
    shield((0.39, 0.0, 0.62), pi / 2, r, 0.16, player, 1)
    # timber platform on brackets
    zp = 0.06 + hb
    for s in (-1, 1):
        for t in (-1, 1):
            rod((s * 0.34, t * 0.34, zp - 0.3), (s * 0.5, t * 0.5, zp), 0.025, "wood_dark", r)
    box((1.06, 1.06, 0.07), (0, 0, zp + 0.035), "cedar", parent=r)
    for i in range(7):
        box((0.02, 1.07, 0.075), (-0.45 + i * 0.15, 0, zp + 0.036), "wood_dark", parent=r)
    zf = zp + 0.07
    for s in (-1, 1):
        for t in (-1, 1):
            box((0.06, 0.06, 0.82), (s * 0.47, t * 0.47, zf + 0.41), "wood_dark", parent=r)
        box((1.0, 0.035, 0.05), (0, s * 0.48, zf + 0.3), "wood", parent=r)
        box((0.035, 1.0, 0.05), (s * 0.48, 0, zf + 0.3), "wood", parent=r)
        for k in range(-2, 3):
            box((0.03, 0.025, 0.28), (k * 0.17, s * 0.48, zf + 0.15), "cedar", parent=r)
            box((0.025, 0.03, 0.28), (s * 0.48, k * 0.17, zf + 0.15), "cedar", parent=r)
    # player-colour cloth over the front rails
    cloth(0.5, 0.26, mat(player, rough=0.7), (0, -0.51, zf + 0.32), 0, r, hang=True, wave=0.008, nx=4)
    cyl(0.06, 0.012, (0, -0.525, zf + 0.2), gold(), rot=(pi / 2, 0, 0), parent=r, segs=8)
    cloth(0.5, 0.26, mat(player, rough=0.7), (0.51, 0, zf + 0.32), pi / 2, r, hang=True, wave=0.008, nx=4)
    cyl(0.06, 0.012, (0.525, 0, zf + 0.2), gold(), rot=(0, pi / 2, 0), parent=r, segs=8)
    brazier((0.15, 0.15, zf), r, 1.15)
    box((1.04, 1.04, 0.06), (0, 0, zf + 0.82), "wood_dark", parent=r)
    hip_roof(1.2, 0.5, (0, 0, zf + 0.85), r, finial=False)
    flag((0, 0, zf + 1.3), 0.6, player, r, w=0.5, fh=0.24, pennant=True)
    # ladder up the right side
    for t in (-1, 1):
        rod((0.62, 0.2 + t * 0.09, 0.02), (0.5, 0.2 + t * 0.09, zp), 0.014, "wood", r)
    for i in range(8):
        f = (i + 0.7) / 8.5
        rod((0.62 - 0.12 * f, 0.11, 0.02 + f * (zp - 0.02)), (0.62 - 0.12 * f, 0.29, 0.02 + f * (zp - 0.02)), 0.011, "wood", r)
    amphora((-0.2, -0.47, 0.06), 0.4, parent=r, s=0.3)
    return r


def market_stall(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """1 x 1. A striped awning over a counter of produce, open to -Y and +X."""
    r = empty("market_stall", loc, rot_z, parent)
    zb, zf = 1.2, 0.98
    for x in (-0.4, 0.4):
        rod((x, 0.38, 0), (x, 0.38, zb), 0.024, "wood_dark", r)
        rod((x, -0.4, 0), (x, -0.4, zf), 0.024, "wood_dark", r)
        rod((x, 0.4, zb), (x, -0.42, zf), 0.018, "wood", r)
    rod((-0.44, -0.4, zf), (0.44, -0.4, zf), 0.018, "wood", r)
    rod((-0.44, 0.38, zb), (0.44, 0.38, zb), 0.018, "wood", r)
    awning((0, 0.46, zb + 0.05), 0, r, w=1.04, d=0.94, drop=zb - zf, player=player, stripes=6,
           sag=0.0, valance=0.11)
    # counter
    box((0.84, 0.44, 0.04), (0, -0.1, 0.44), "cedar", parent=r)
    box((0.86, 0.03, 0.2), (0, -0.32, 0.33), mat(player, rough=0.75), parent=r)
    box((0.86, 0.035, 0.03), (0, -0.322, 0.26), gold(), parent=r)
    for x in (-0.37, 0.37):
        for y in (-0.28, 0.08):
            box((0.05, 0.05, 0.42), (x, y, 0.21), "wood_dark", parent=r)
    for i, x in enumerate((-0.28, 0.0, 0.28)):
        basket((x, -0.17, 0.46), r, 0.125, i, i)
    basket((-0.15, 0.06, 0.46), r, 0.11, 3, 5)
    basket((0.15, 0.06, 0.46), r, 0.11, 4, 6)
    # hanging strings of onions/garlic and a fish or two from the front beam
    for i, x in enumerate((-0.3, -0.1, 0.12, 0.3)):
        rod((x, -0.4, zf), (x, -0.4, zf - 0.12), 0.005, "wool", r, 4)
        if i % 2 == 0:
            for k in range(3):
                sphere(0.03, (x + (k % 2) * 0.02, -0.4, zf - 0.13 - k * 0.045),
                       mat(("flower_white", (0.75, 0.3, 0.4))[i // 2]), parent=r, segs=6)
        else:
            sphere(0.035, (x, -0.4, zf - 0.2), mat((0.6, 0.68, 0.74), rough=0.3), scale=(0.5, 0.35, 2.2),
                   parent=r, segs=6)
    # around it
    amphora((0.36, 0.3, 0), 0.4, parent=r, s=0.5)
    amphora((0.18, 0.34, 0), 1.3, parent=r, s=0.42, colour=(0.88, 0.6, 0.32), band="red_ochre")
    crate((-0.3, 0.26, 0), 0.1, r, 0.24, fruit=0, seed=2)
    crate((0.38, -0.46, 0), 0.3, r, 0.2, fruit=1, seed=3)
    sack((-0.42, -0.46, 0), r, 0.1, 4)
    return r


def props(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """A little cluster of the loose props, for a look at them together."""
    r = empty("props", loc, rot_z, parent)
    amphora((-0.9, -0.2, 0), 0.3, parent=r)
    amphora((-0.65, -0.35, 0), 1.0, parent=r, s=0.5, colour=(0.88, 0.6, 0.32), band="red_ochre")
    amphora((-0.75, 0.05, 0.08), 0.2, parent=r, s=0.5, tilt=1.35)
    pithos((-0.3, 0.3, 0), r)
    crate((0.0, -0.4, 0), 0.2, r, 0.3, fruit=0)
    crate((0.3, -0.1, 0), -0.3, r, 0.26)
    basket((0.1, 0.15, 0), r, 0.14, 1, 1)
    basket((-0.25, -0.2, 0), r, 0.12, 2, 2)
    cart((1.0, 0.2, 0), 0.4, player, r)
    well((0.5, 1.0, 0), 0.4, r)
    brazier((-0.9, 0.7, 0), r)
    pot((0.55, -0.55, 0), r, 0.2, "leaf", flowers="flower_red")
    sack((0.75, -0.4, 0), r, 0.14, 1)
    return r


# --------------------------------------------------------------- showcase


def showcase():
    """Everything as one small town, two players' colours."""
    r = empty("greek_showcase")

    def at(u, v):       # u: screen-right, v: away from the camera
        return (u - v, u + v, 0)

    box((60, 60, 0.02), (0, 0, 0.0), noisy((0.42, 0.64, 0.24), 0.16, 0.5, 1.0), parent=r)
    box((26, 1.5, 0.03), at(0.0, -2.9), noisy("sand", 0.1, 1.0, 1.0), rot=(0, 0, pi / 4), parent=r)
    # back row
    watch_tower(at(-6.6, 5.8), player=RED, parent=r)
    temple(at(-3.4, 5.5), parent=r)
    town_center(at(1.0, 5.8), parent=r)
    watch_tower(at(4.4, 5.2), parent=r)
    house(at(6.6, 5.2), variant=1, parent=r)
    # middle row, north of the road
    house(at(-6.2, 0.4), variant=0, parent=r)
    house(at(-3.6, 0.2), variant=1, parent=r)
    market_stall(at(-1.5, -0.6), parent=r)
    market_stall(at(-0.1, -0.3), player=RED, parent=r)
    house(at(2.0, 0.4), variant=2, parent=r)
    barracks(at(5.0, 0.2), parent=r)
    cart(at(0.9, -2.9), pi / 4, parent=r)
    props(at(-4.6, -3.1), parent=r)
    # front row, the red player's
    house(at(-6.0, -6.7), variant=2, player=RED, parent=r)
    barracks(at(-3.2, -6.7), player=RED, parent=r)
    town_center(at(0.6, -7.4), player=RED, parent=r)
    house(at(3.8, -6.5), variant=0, player=RED, parent=r)
    temple(at(6.6, -6.7), player=RED, parent=r)
    for i, (u, v, sc) in enumerate(((7.4, 0.8, 1.2), (-7.6, 2.9, 1.1), (3.2, 2.7, 1.0), (-1.6, 2.6, 1.1),
                                    (-1.4, -5.4, 1.1), (2.4, -4.4, 0.9), (5.6, -3.9, 1.2), (-7.6, -3.9, 1.0),
                                    (-1.0, 8.5, 1.2), (5.6, 8.2, 1.1), (-5.2, 8.2, 1.0))):
        olive_tree(at(u, v), r, sc, i + 3)
    return r

"""Greek buildings for the new look: painted marble, terracotta tile, bronze.

Every builder is centred on the origin on its footprint (in tiles), takes
`loc=(x, y, 0), rot_z=0.0, player=BLUE, parent=None` and returns its root
empty. The faces the game camera sees are -Y (screen left, sunlit) and +X
(screen right), so doors and the best detail are on those sides.

    temple()        3 x 3      town_center()   3 x 3
    house(variant)  2 x 2      barracks()      2 x 2
    watch_tower()   1 x 1      market_stall()  1 x 1
    academy()       2 x 2      archery_range() 2 x 2
    government_centre() 2 x 2  market()        2 x 2
    siege_workshop() 2 x 2     stable()        2 x 2
    storehouse()    2 x 2      farm()          2 x 2 (flat)
    dock()          3 x 3 (over water, none modelled)
    wonder()        5 x 5      gate(line, open) 1 x 1
    stone_wall(arm) 1 x 1      palisade_wall(arm) 1 x 1   (walls: see "walls and a gate")
    amphora() cart() crate() well() brazier() olive_tree()   props
    showcase()      the first buildings, laid out as a small village
    showcase_all()  every set together (render_showcase_all(path))

SETS at the bottom declares the colour sets (docs/look/buildings.md).
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
# Other roofs, so not every Greek building wears the same red: blue-grey
# slate (the academy), straw thatch (the storehouse).
SLATE = dict(palette=[(0.44, 0.50, 0.58), (0.38, 0.44, 0.52), (0.48, 0.54, 0.60), (0.40, 0.45, 0.50)],
             covers=(0.30, 0.34, 0.40), antefix=None)
THATCH = dict(palette=[(0.80, 0.64, 0.32), (0.74, 0.58, 0.28), (0.84, 0.70, 0.38), (0.70, 0.54, 0.26)],
              covers=None, antefix=None, thick=0.07)
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


def _slope(e, sx, y0, z0, y1, z1, antefix="tile_dark", seed=0, pitch=0.15, rows=None, palette=None,
           covers="tile_dark", thick=0.036):
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
        pal = palette or TILES
        box((sx, step * 1.32, thick),
            (0, y0 + dy * t + ny * 0.03, z0 + dz * t + nz * 0.03),
            mat(pal[rnd.randrange(len(pal))], rough=0.8 if covers else 0.95), rot=(th - sg * 0.08, 0, 0),
            parent=e, name="tiles")
    if covers is None:
        return
    n = max(2, round(sx / pitch))
    cov = mat(covers, rough=0.8)
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
               antefix="tile_dark", seed=0, kind=None):
    """A tiled gable roof whose eaves' wall line sits at loc.z. `length` runs
    along the ridge, which lies along `axis`. `gable` fills the end triangles."""
    e = empty("roof", loc, 0.0 if axis == "x" else pi / 2, parent)
    Y = span / 2
    a = h / Y
    y0, z0 = Y + over, -over * a
    sx = length + 2 * over
    kind = dict(kind or {})
    if kind:
        antefix = kind.pop("antefix", antefix)
    ridge = kind.get("covers", "tile_dark") or tint(kind["palette"][0], 0.82)
    _slope(e, sx, -y0, z0, 0, h, antefix, seed, **kind)
    _slope(e, sx, y0, z0, 0, h, antefix, seed + 1, **kind)
    cyl(0.05 if "thick" not in kind else 0.07, sx + 0.04, (0, 0, h + 0.05), mat(ridge, rough=0.8),
        rot=(0, pi / 2, 0), parent=e, segs=8)
    if gable is not None:
        for sgn in (-1, 1):
            x = sgn * length / 2
            mesh([(x, -Y, 0), (x, Y, 0), (x, 0, h)], [(0, 1, 2)], gable, parent=e)
    return e


def shed_roof(sx, sy, h, loc, parent=None, rot_z=0.0, over=0.08, antefix="tile_dark", seed=0, kind=None):
    """A lean-to: low eave at local -Y (z = loc.z), high edge at +Y (z + h)."""
    e = empty("shed", loc, rot_z, parent)
    a = h / sy
    kind = dict(kind or {})
    if kind:
        antefix = kind.pop("antefix", antefix)
    _slope(e, sx + 2 * over, -sy / 2 - over, -over * a, sy / 2, h, antefix, seed, **kind)
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
    box((t + 0.07, t + 0.07, 0.1), (x, y, 0.05), courses("limestone", 12.0), parent=parent)
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


# ------------------------------------------------------- more helpers, props


def _pad(r, side, m="sand", inset=0.08):
    """The yard under a building: a soft-edged slab `side` tiles across."""
    box((side - inset, side - inset, 0.035), (0, 0, 0.012), noisy(m, 0.1, 3.0, 1.0), parent=r, bevel=0.012)


def clumps(points, r, m, parent=None, seed=0, rough=0.25, name="clumps"):
    """Many lumpy little blobs as ONE mesh (cheap): `points` are (loc, scale)."""
    rnd = random.Random(seed)
    bm = bmesh.new()
    for loc, sc in points:
        ret = bmesh.ops.create_icosphere(bm, subdivisions=1, radius=r)
        vs = ret["verts"]
        a = rnd.uniform(0, 6.28)
        for v in vs:
            v.co *= 1.0 + rnd.uniform(-rough, rough)
            x, y = v.co.x * cos(a) - v.co.y * sin(a), v.co.x * sin(a) + v.co.y * cos(a)
            v.co = Vector((x * sc[0] + loc[0], y * sc[1] + loc[1], v.co.z * sc[2] + loc[2]))
    return lib._obj(bm, name, m, (0, 0, 0), (0, 0, 0), False, parent, 0.0)


def _band(sx, sy, z, loc, parent, m, h=0.07, out=0.04):
    """A painted band round a block at height z (its centre)."""
    box((sx + out, sy + out, h), (loc[0], loc[1], z), m, parent=parent)


def arc_rod(c, rad, a0, a1, parent, r=0.012, m="wood", plane="xz", n=6, r_end=None):
    """A bent rod: an arc of radius `rad` about `c` from angle a0 to a1, in
    the plane named (xz: angles from +X toward +Z; yz; xy)."""
    pts = []
    for i in range(n + 1):
        a = a0 + (a1 - a0) * i / n
        u, v = rad * cos(a), rad * sin(a)
        if plane == "xz":
            pts.append((c[0] + u, c[1], c[2] + v))
        elif plane == "yz":
            pts.append((c[0], c[1] + u, c[2] + v))
        else:
            pts.append((c[0] + u, c[1] + v, c[2]))
    for i, (p, q) in enumerate(zip(pts, pts[1:])):
        rr = r if r_end is None else r + (r_end - r) * abs(i + 0.5 - n / 2) / (n / 2)
        rod(p, q, rr, m, parent, 5)
    return pts


def bow(loc, rz=0.0, parent=None, h=0.5, m="wood_dark"):
    """A strung bow standing upright in a frame facing local -Y."""
    e = face(loc, rz, parent)
    a = 0.62
    rad = h / 2 / sin(a)
    arc_rod((-rad * cos(a) + 0.0, 0, 0), rad, -a, a, e, 0.011, m, "xz", 6)
    rod((0.0 + rad * cos(a) - rad * cos(a), 0, -h / 2), (0, 0, h / 2), 0.003, "wool", e, 4)
    box((0.03, 0.024, 0.06), (rad - rad * cos(a), 0, 0), "leather", parent=e)
    return e


def quiver(loc, parent=None, player=BLUE, tilt=0.15, rz=0.0):
    """A leather quiver full of fletched arrows, a band in player colour."""
    e = empty("quiver", loc, rz, parent)
    e.rotation_euler = (tilt, 0, rz)
    cyl(0.045, 0.3, (0, 0, 0.15), "leather", parent=e, segs=8, r_top=0.05)
    cyl(0.052, 0.05, (0, 0, 0.24), mat(player, rough=0.7), parent=e, segs=8)
    for k in range(5):
        a = k * 1.3
        p = (0.022 * cos(a), 0.022 * sin(a), 0.3)
        rod(p, (p[0], p[1], 0.4), 0.005, "wood", e, 4)
        box((0.006, 0.03, 0.05), (p[0], p[1], 0.38), ("white", "red_ochre")[k % 2], parent=e)
    return e


def target(loc, rz=0.0, parent=None, r=0.2, arrows=3, seed=0):
    """A round straw target with painted rings on a tripod, facing local -Y."""
    e = face(loc, rz, parent)
    cyl(r, 0.08, (0, 0, 0), "straw", rot=(pi / 2 - 0.12, 0, 0), parent=e, segs=16)
    for k, (c_, rr) in enumerate((("white", 0.78), ("flower_red", 0.6), ("white", 0.4),
                                  ("yellow_ochre", 0.2))):
        cyl(r * rr, 0.01, (0, -0.042 - k * 0.003, 0.005), c_, rot=(pi / 2 - 0.12, 0, 0), parent=e, segs=14)
    rnd = random.Random(seed)
    for i in range(arrows):
        x, z = rnd.uniform(-0.6, 0.6) * r, rnd.uniform(-0.6, 0.6) * r
        rod((x, -0.04, z), (x + rnd.uniform(-0.03, 0.03), -0.24, z + 0.03), 0.005, "wood", e, 4)
        box((0.006, 0.04, 0.03), (x, -0.22, z + 0.03), "white", parent=e)
    for sx in (-1, 1):
        rod((sx * r * 0.7, 0.04, -loc[2]), (sx * r * 0.35, 0.03, r * 0.4), 0.014, "wood_dark", e)
    rod((0, 0.25, -loc[2]), (0, 0.04, r * 0.3), 0.014, "wood_dark", e)
    return e


def hay_bale(loc, rz=0.0, parent=None, s=(0.3, 0.2, 0.16)):
    e = empty("bale", loc, rz, parent)
    box(s, (0, 0, s[2] / 2), noisy("straw", 0.16, 14.0), parent=e, bevel=0.02)
    for fx in (-0.25, 0.25):
        box((0.012, s[1] + 0.008, s[2] + 0.008), (fx * s[0], 0, s[2] / 2), mat(tint("thatch", 0.7)), parent=e)
    return e


def log(p0, p1, r, parent, bark="wood", end=(0.86, 0.68, 0.44)):
    """A trimmed log with pale sawn ends."""
    a, b = Vector(p0), Vector(p1)
    d = (b - a).normalized()
    rod(a, b, r, bark, parent, 8)
    for p in (a, b):
        rod(p - d * 0.004, p + d * 0.004, r * 0.86, end, parent, 8)


def timber_stack(loc, rz=0.0, parent=None, n=(4, 3, 2), L=0.9, r=0.055, tarp=None):
    """Logs piled in a pyramid along local Y."""
    e = empty("timber", loc, rz, parent)
    for row, k in enumerate(n):
        for i in range(k):
            x = (i - (k - 1) / 2) * r * 2.05
            z = r + row * r * 1.75
            log((x, -L / 2 + row * 0.02, z), (x, L / 2 - row * 0.03, z), r * (0.95 + 0.08 * ((i + row) % 2)), e)
    if tarp is not None:
        zt = r + (len(n) - 1) * r * 1.75 + r
        box((r * 2.4 * n[-1] + 0.06, L * 0.5, 0.014), (0, L * 0.18, zt), mat(tarp, rough=0.75), parent=e)
        for sx in (-1, 1):
            box((0.014, L * 0.5, r * 2.2), (sx * (r * 1.2 * n[-1] + 0.03), L * 0.18, zt - r * 1.1),
                mat(tarp, rough=0.75), rot=(0, sx * 0.25, 0), parent=e)
    return e


def stone_pile(loc, parent=None, seed=0, blocks=4, rocks=5):
    x, y, z = loc
    rnd = random.Random(seed)
    st = courses("limestone", 22.0, 0.8, 0.1)
    for i in range(blocks):
        bx, by = x + (i % 2) * 0.2 - 0.1, y + (i // 2) * 0.2 - 0.1
        box((0.18, 0.18, 0.13), (bx, by, z + 0.065), noisy((0.78, 0.76, 0.72), 0.12, 8.0), parent=parent,
            rot=(0, 0, rnd.uniform(-0.1, 0.1)), bevel=0.01)
    box((0.2, 0.17, 0.12), (x, y, z + 0.19), noisy((0.78, 0.76, 0.72), 0.12, 8.0), parent=parent,
        rot=(0, 0, 0.2), bevel=0.01)
    for i in range(rocks):
        a = i * 1.9 + seed
        blob(rnd.uniform(0.05, 0.08), (x + 0.26 * cos(a), y + 0.26 * sin(a), z + 0.03), "rock",
             (1, 1, 0.7), parent, seed + i, 0.3, 1)
    return st


def gold_pile(loc, parent=None, seed=0, chest=True):
    """An open chest heaped with gold, bars and loose nuggets beside it."""
    x, y, z = loc
    g = gold()
    rnd = random.Random(seed)
    if chest:
        box((0.36, 0.24, 0.16), (x, y, z + 0.08), "cedar", parent=parent, bevel=0.008)
        for fx in (-1, 1):
            box((0.025, 0.25, 0.17), (x + fx * 0.13, y, z + 0.085), bronze(), parent=parent)
        box((0.36, 0.03, 0.22), (x, y + 0.14, z + 0.24), "cedar", rot=(-0.45, 0, 0), parent=parent)
        blob(0.16, (x, y, z + 0.16), g, (1.1, 0.75, 0.55), parent, seed, 0.15, 2)
    else:
        blob(0.16, (x, y, z + 0.02), g, (1.2, 1.0, 0.55), parent, seed, 0.2, 2)
    for i in range(6):
        a = i * 1.1 + 0.3
        d = rnd.uniform(0.17, 0.24)
        sphere(0.028, (x + d * cos(a), y + d * sin(a) * 0.8, z + 0.02), g, parent=parent, segs=6)
    for i in range(3):
        box((0.1, 0.045, 0.035), (x + 0.24, y - 0.14 + i * 0.012, z + 0.018 + i * 0.036), g,
            rot=(0, 0, 0.3 * (i % 2)), parent=parent)


def sack_pile(loc, parent=None, seed=0, n=(4, 2, 1), s=0.12, colours=("wool", "linen", "wool")):
    x, y, z = loc
    k = 0
    for row, cnt in enumerate(n):
        for i in range(cnt):
            sx = x + (i - (cnt - 1) / 2) * s * 1.7
            sack((sx, y + (row % 2) * 0.02, z + row * s * 1.3), parent, s, seed + k, colours[k % len(colours)])
            k += 1


def trough(loc, rz=0.0, parent=None, L=0.6):
    e = empty("trough", loc, rz, parent)
    box((L, 0.2, 0.18), (0, 0, 0.09), courses("limestone", 16.0), parent=e, bevel=0.01)
    box((L - 0.06, 0.13, 0.02), (0, 0, 0.165), mat("water", rough=0.15), parent=e)
    return e


def fence(points, parent=None, h=0.42, m="wood", post="wood_dark", rails=2):
    """A post-and-rail fence along a polyline of (x, y) points."""
    for j, ((x0, y0), (x1, y1)) in enumerate(zip(points, points[1:])):
        L = hypot(x1 - x0, y1 - y0)
        n = max(1, round(L / 0.4))
        for i in range(1 if j else 0, n + 1):     # a corner post once, not twice (coplanar: black)
            t = i / n
            box((0.045, 0.045, h + 0.04), (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, (h + 0.04) / 2), post,
                parent=parent)
        for k in range(rails):
            zz = h * (0.45 + 0.5 * k / max(1, rails - 1))
            rod((x0, y0, zz), (x1, y1, zz), 0.016, m, parent, 5)


def horse_head(loc, rz=0.0, parent=None, coat="cedar", mane="hair_black", blaze=False):
    """A horse's head and neck leaning out over a half-door: the frame's
    local -Y is outward, loc on the door's top edge."""
    e = face(loc, rz, parent)
    e.scale = (1.3, 1.3, 1.3)
    cm = mat(coat, rough=0.7)
    rod((0, 0.14, -0.02), (0, -0.08, 0.16), 0.065, cm, e, 8, 0.05)
    rod((0, 0.14, 0.04), (0, -0.06, 0.215), 0.022, mane, e, 5)
    hd = empty("head", (0, -0.09, 0.17), 0, e)
    hd.rotation_euler = (0.55, 0, 0)
    box((0.1, 0.11, 0.24), (0, -0.01, -0.07), cm, parent=hd, bevel=0.025)
    box((0.085, 0.09, 0.08), (0, -0.015, -0.2), cm, parent=hd, bevel=0.025)
    for sx in (-1, 1):
        sphere(0.014, (sx * 0.05, -0.03, 0.0), "black", parent=hd, segs=6)
        cyl(0.018, 0.07, (sx * 0.03, 0.03, 0.07), cm, parent=hd, r_top=0.004, segs=5)
    if blaze:
        box((0.025, 0.012, 0.18), (0, -0.07, -0.1), "white", parent=hd)
    box((0.11, 0.115, 0.02), (0, -0.01, -0.12), "leather", parent=hd)
    return e


def _horse(loc, facing, parent, coat="cedar", cloth=None, pose="stand"):
    """A whole horse, from the figures kit."""
    import figures as fg
    root, b = fg._start("horse", loc, facing, parent)
    fg._quad(b, "horse", pose, coat=coat, cloth=cloth)
    b.finish(root)
    return root


def _stall(loc, rz=0.0, parent=None, player=BLUE, w=0.7, d=0.5, seed=0, goods=0, cloth_c=None, other="linen"):
    """A small market stall: counter, striped awning on four posts. Front
    (and the awning's low edge) toward local -Y."""
    e = empty("stall", loc, rz, parent)
    zb, zf = 0.95, 0.8
    for x in (-w / 2, w / 2):
        rod((x, d / 2, 0), (x, d / 2, zb), 0.02, "wood_dark", e)
        rod((x, -d / 2, 0), (x, -d / 2, zf), 0.02, "wood_dark", e)
    awning((0, d / 2 + 0.04, zb + 0.03), 0, e, w=w + 0.12, d=d + 0.14, drop=zb - zf + 0.03,
           player=cloth_c or player, other=other, stripes=5, sag=0.0, valance=0.08)
    box((w - 0.04, d * 0.6, 0.035), (0, -d * 0.12, 0.4), "cedar", parent=e)
    box((w - 0.02, 0.025, 0.16), (0, -d * 0.42, 0.31), mat(player, rough=0.75), parent=e)
    box((w - 0.02, 0.03, 0.025), (0, -d * 0.42 - 0.002, 0.24), gold(), parent=e)
    for x in (-w / 2 + 0.05, w / 2 - 0.05):
        box((0.04, 0.04, 0.38), (x, -d * 0.12, 0.19), "wood_dark", parent=e)
    rnd = random.Random(seed)
    if goods == 0:      # produce
        for i in range(3):
            basket(((i - 1) * w * 0.3, -d * 0.12, 0.42), e, 0.09, rnd.randrange(6), seed + i)
    elif goods == 1:    # pots and jugs
        for i in range(4):
            x = (i - 1.5) * w * 0.22
            lathe([(0.03, 0), (0.06, 0.05), (0.05, 0.1), (0.025, 0.13), (0.03, 0.15), (0.02, 0.15)],
                  mat(["terracotta", "black", (0.86, 0.6, 0.32), "red_ochre"][i], rough=0.6),
                  (x, -d * 0.12, 0.42), parent=e, segs=8)
    else:               # cloth bolts
        for i in range(3):
            cyl(0.05, d * 0.5, ((i - 1) * w * 0.28, -d * 0.12, 0.47),
                mat(["red_ochre", "yellow_ochre", "tyrian"][i], rough=0.8), rot=(pi / 2, 0, 0), parent=e, segs=8)
    return e


# ------------------------------------------------------------- storehouse


def storehouse(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A storeroom at the back, a timber lean-to on the +X side over
    logs and dressed stone, sacks of grain and a chest of gold in the yard."""
    r = empty("storehouse", loc, rot_z, parent)
    _pad(r, 2, "sand")
    z = 0.03
    x0, x1, y0, y1, h = -0.95, 0.2, 0.22, 0.95, 1.08
    c = ((x0 + x1) / 2, (y0 + y1) / 2)
    wall_block(x1 - x0, y1 - y0, h, (c[0], c[1], z), r, dado="yellow_ochre", line="red_ochre", dado_h=0.3)
    _band(x1 - x0, y1 - y0, z + h - 0.04, c, r, "red_ochre")
    gable_roof(x1 - x0, y1 - y0, 0.5, (c[0], c[1], z + h), r, over=0.16, gable=noisy("whitewash"), seed=51,
               kind=THATCH)
    cyl(0.08, 0.03, (x0 - 0.0, c[1], z + h + 0.15), "egyptian_blue", rot=(0, pi / 2, 0), parent=r, segs=10)
    door((-0.4, y0, z), 0, r, w=0.5, h=0.84, colour="red_ochre")
    box((0.62, 0.22, 0.04), (-0.4, y0 - 0.1, z + 0.02), "wood", parent=r)
    for x in (-0.82, 0.06):
        box((0.16, 0.02, 0.07), (x, y0 - 0.005, z + 0.82), mat((0.15, 0.1, 0.08)), parent=r)
        box((0.2, 0.03, 0.02), (x, y0 - 0.01, z + 0.78), "wood_dark", parent=r)
    # a pulley hoist over the door
    rod((-0.4, y0 + 0.02, z + h + 0.12), (-0.4, y0 - 0.3, z + h + 0.12), 0.024, "wood_dark", r)
    cyl(0.04, 0.025, (-0.4, y0 - 0.27, z + h + 0.06), "wood", rot=(0, pi / 2, 0), parent=r, segs=8)
    rod((-0.4, y0 - 0.3, z + h + 0.04), (-0.4, y0 - 0.3, z + 0.62), 0.005, "wool", r, 4)
    sack((-0.4, y0 - 0.3, z + 0.42), r, 0.09, 11)
    # lean-to on posts down the +X side
    lx0, lx1, ly0, ly1 = 0.3, 0.94, -0.12, 0.92
    zl, rise = 0.78, 0.3
    shed_roof(ly1 - ly0, lx1 - lx0, rise, ((lx0 + lx1) / 2, (ly0 + ly1) / 2, zl), r, rot_z=pi / 2, over=0.05, seed=53,
              kind=THATCH)
    for y in (ly0 + 0.03, (ly0 + ly1) / 2, ly1 - 0.03):
        box((0.06, 0.06, zl), (lx1 - 0.04, y, zl / 2), "wood_dark", parent=r)
        box((0.06, 0.06, zl + rise), (lx0 + 0.03, y, (zl + rise) / 2), "wood_dark", parent=r)
        rod((lx1 - 0.04, y, zl - 0.2), (lx1 - 0.2, y, zl - 0.02), 0.016, "wood_dark", r)
    box((0.07, ly1 - ly0 + 0.08, 0.07), (lx1 - 0.04, (ly0 + ly1) / 2, zl - 0.02), "wood", parent=r)
    box((0.07, ly1 - ly0 + 0.08, 0.07), (lx0 + 0.03, (ly0 + ly1) / 2, zl + rise - 0.03), "wood", parent=r)
    cloth(0.5, 0.2, mat(player, rough=0.7), (lx1 + 0.0, (ly0 + ly1) / 2 - 0.1, zl - 0.04), pi / 2, r,
          hang=True, wave=0.008, nx=5)
    cyl(0.055, 0.012, (lx1 + 0.015, (ly0 + ly1) / 2 - 0.1, zl - 0.14), gold(), rot=(0, pi / 2, 0), parent=r, segs=8)
    timber_stack((0.62, 0.4, z), 0, r, (4, 3, 2), 0.86, 0.06, tarp=player)
    stone_pile((0.58, -0.5, z), r, seed=3)
    # the yard: grain, gold, oil
    sack_pile((-0.62, -0.12, z), r, seed=2)
    sack_pile((-0.7, -0.52, z), r, seed=7, n=(3, 2), s=0.11, colours=("linen", "wool"))
    gold_pile((-0.05, -0.58, z), r, seed=4)
    for i, (x, y) in enumerate(((-0.12, -0.08), (0.06, -0.14), (-0.02, 0.06))):
        amphora((x, y, z), i * 0.7, parent=r, s=0.42, colour=((0.88, 0.6, 0.32), "terracotta")[i % 2],
                band=("red_ochre", "black")[i % 2])
    pithos((0.3, -0.82, z), r, 0.42)
    crate((-0.3, -0.84, z), 0.2, r, 0.22)
    banner((-0.93, -0.92, z), 1.45, player, r, 0.28, 0.5, seed=5)
    flag((c[0] - 0.3, c[1], z + h + 0.42), 0.5, player, r, w=0.34, fh=0.2, pennant=True)
    return r


# ----------------------------------------------------------------- market


def market(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. An agora corner: a stoa of shops along the back, stalls under
    striped awnings, jars and baskets everywhere, a fountain."""
    r = empty("market", loc, rot_z, parent)
    box((1.92, 1.92, 0.04), (0, 0, 0.015), courses("limestone", 3.8, 0.82, 0.1), parent=r, bevel=0.012)
    z = 0.035
    # stoa: back wall of shops, a colonnade, a lean-to roof
    bx0, bx1, by0, by1, bh = -0.95, 0.95, 0.72, 0.95, 1.18
    wall_block(bx1 - bx0, by1 - by0, bh, (0, (by0 + by1) / 2, z), r, dado="egyptian_blue", line="yellow_ochre")
    fy, zr, rise = 0.32, 1.0, 0.36
    for sx in (bx0, bx1):
        mesh([(sx, by0, z + bh), (sx, by1, z + bh), (sx, by1, zr + rise), (sx, fy, zr)],
             [(0, 1, 2, 3)], noisy("whitewash"), parent=r)
        box((0.05, by0 - fy, zr), (sx + (0.025 if sx < 0 else -0.025), (by0 + fy) / 2, zr / 2), noisy("whitewash"), parent=r)
    # the stoa is roofed with a vine trellis, not tile: green and open, so
    # the market is a square of stalls and not another red-roofed house
    for i in range(11):
        x = bx0 + 0.05 + i * (bx1 - bx0 - 0.1) / 10
        box((0.035, by1 - fy + 0.06, 0.04), (x, (by1 + fy) / 2, zr + rise * 0.5), "wood", rot=(atan2(rise, by1 - fy), 0, 0),
            parent=r)
    rnd_v = random.Random(62)
    pts = []
    for i in range(16):
        t = rnd_v.random()
        x = bx0 + 0.1 + (bx1 - bx0 - 0.2) * (i + rnd_v.random()) / 16
        pts.append((x, fy + (by1 - fy) * t, zr + rise * t + 0.06))
    foliage(pts, 0.13, r, seed=63, mats=("leaf", "leaf_olive", "leaf_dark"))
    for k in range(6):
        sphere(0.035, (bx0 + 0.25 + k * 0.29, fy + 0.04, zr - 0.06), "tyrian", parent=r, segs=6, scale=(1, 1, 1.4))
    box((bx1 - bx0, 0.07, 0.08), (0, fy + 0.04, zr - 0.04), "red_ochre", parent=r)
    for i in range(9):
        box((0.05, 0.02, 0.04), (bx0 + 0.15 + i * 0.205, fy + 0.0, zr - 0.04), gold(), parent=r)
    for i in range(4):
        column((bx0 + 0.24 + i * 0.49, fy + 0.04, z), zr - z - 0.06, 0.06, r, band="egyptian_blue",
               lower=mat("red_ochre", rough=0.7), flutes=8)
    box((bx1 - bx0, by0 - fy, 0.03), (0, (by0 + fy) / 2, z + 0.01), noisy("marble", 0.06), parent=r)
    # shop doorways with goods in front
    for i, x in enumerate((-0.6, 0.0, 0.6)):
        box((0.3, 0.02, 0.62), (x, by0 - 0.005, z + 0.33), mat((0.16, 0.11, 0.09)), parent=r)
        box((0.38, 0.05, 0.05), (x, by0 - 0.02, z + 0.66), "wood_dark", parent=r)
        cloth(0.32, 0.22, mat(["red_ochre", "yellow_ochre", "egyptian_blue"][i], rough=0.8),
              (x, by0 - 0.03, z + 0.64), 0, r, hang=True, wave=0.01, nx=4)
    # cloth bolts and pots under the colonnade
    for i in range(4):
        cyl(0.045, 0.24, (-0.78 + i * 0.1, 0.58, z + 0.05), mat(["tyrian", "red_ochre", "linen", "yellow_ochre"][i], rough=0.8),
            rot=(pi / 2, 0, 0), parent=r, segs=8)
    for i in range(3):
        cyl(0.045, 0.24, (-0.73 + i * 0.1, 0.58, z + 0.13), mat(["egyptian_blue", "malachite", "linen"][i], rough=0.8),
            rot=(pi / 2, 0, 0), parent=r, segs=8)
    for i in range(5):
        amphora((0.32 + i * 0.12, 0.6 - (i % 2) * 0.05, z), i * 0.5, parent=r, s=0.36,
                colour=((0.88, 0.6, 0.32), "terracotta", (0.62, 0.3, 0.2))[i % 3])
    laundry((bx0 + 0.04, fy + 0.04, zr - 0.12), (bx0 + 0.75, fy + 0.04, zr - 0.12), r, seed=2,
            colours=("red_ochre", "linen", "tyrian", "yellow_ochre"))
    # stalls in the square
    # stalls in many colours, the owner's one of them
    _stall((-0.48, -0.2, z), 0, r, player, 0.72, 0.5, seed=1, goods=0)
    _stall((0.56, -0.38, z), pi / 2, r, player, 0.66, 0.46, seed=2, goods=1, cloth_c="yellow_ochre", other="red_ochre")
    # fountain
    fx, fy_ = 0.2, 0.08
    lathe([(0.2, 0), (0.2, 0.14), (0.17, 0.14), (0.17, 0.1)], courses("marble", 14.0), (fx, fy_, z), parent=r,
          segs=14, smooth=False)
    cyl(0.17, 0.02, (fx, fy_, z + 0.1), mat("water", rough=0.15), parent=r, segs=14)
    cyl(0.035, 0.32, (fx, fy_, z + 0.24), "marble", parent=r, segs=8)
    lathe([(0.02, 0), (0.09, 0.04), (0.1, 0.06), (0.03, 0.06)], "marble", (fx, fy_, z + 0.38), parent=r, segs=10)
    cyl(0.012, 0.16, (fx, fy_, z + 0.38), mat("foam", rough=0.1, emit=0.2), parent=r, segs=6, r_top=0.03)
    # loose goods
    for i, (x, y) in enumerate(((-0.86, -0.75), (-0.72, -0.84), (-0.82, -0.58))):
        amphora((x, y, z), i * 1.1, parent=r, s=0.44, colour=((0.88, 0.6, 0.32), "terracotta")[i % 2],
                band=("red_ochre", "black")[i % 2])
    pithos((-0.2, -0.8, z), r, 0.4)
    basket((0.02, -0.72, z), r, 0.11, 3, 2)
    basket((0.1, -0.88, z), r, 0.1, 0, 5)
    crate((0.86, 0.12, z), 0.3, r, 0.22, fruit=4, seed=1)
    crate((0.84, -0.86, z), -0.2, r, 0.22, fruit=1, seed=3)
    sack((-0.12, -0.55, z), r, 0.1, 5)
    # a balance on a post
    bxp, byp = -0.05, -0.36
    box((0.05, 0.05, 0.62), (bxp, byp, z + 0.31), "wood_dark", parent=r)
    rod((bxp - 0.18, byp, z + 0.6), (bxp + 0.18, byp, z + 0.6), 0.01, bronze(), r)
    for sx in (-1, 1):
        rod((bxp + sx * 0.18, byp, z + 0.6), (bxp + sx * 0.18, byp, z + 0.45), 0.003, "wool", r, 4)
        cyl(0.06, 0.012, (bxp + sx * 0.18, byp, z + 0.44), bronze(), parent=r, segs=10)
    flag((0.0, 0.84, z + bh + 0.0), 0.75, player, r, w=0.5, fh=0.3, seed=4)
    return r


# ----------------------------------------------------------------- stable


def stable(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A row of stalls at the back with horses looking out over the
    half-doors, a hay loft, a paddock with a horse, trough and hay."""
    r = empty("stable", loc, rot_z, parent)
    _pad(r, 2, "earth")
    z = 0.03
    x0, x1, y0, y1, h = -0.95, 0.62, 0.3, 0.95, 1.0
    c = ((x0 + x1) / 2, (y0 + y1) / 2)
    wall_block(x1 - x0, y1 - y0, h, (c[0], c[1], z), r, dado="red_ochre", line="yellow_ochre", dado_h=0.22)
    gable_roof(x1 - x0, y1 - y0, 0.4, (c[0], c[1], z + h), r, over=0.13, gable=noisy("whitewash"), seed=71)
    _band(x1 - x0, y1 - y0, z + h - 0.04, c, r, "egyptian_blue")
    coats = [("cedar", "hair_black", True), ((0.93, 0.9, 0.84), (0.7, 0.66, 0.6), False),
             ((0.24, 0.17, 0.13), "hair_black", True), ((0.70, 0.48, 0.26), (0.36, 0.22, 0.13), False)]
    for i, x in enumerate((-0.74, -0.36, 0.02, 0.4)):
        box((0.3, 0.02, 0.74), (x, y0 - 0.006, z + 0.4), mat((0.12, 0.08, 0.06)), parent=r)
        box((0.36, 0.05, 0.05), (x, y0 - 0.02, z + 0.79), "wood_dark", parent=r)
        for sx in (-1, 1):
            box((0.04, 0.05, 0.78), (x + sx * 0.17, y0 - 0.02, z + 0.39), "wood_dark", parent=r)
        # half-door, planked, painted
        box((0.3, 0.035, 0.42), (x, y0 - 0.03, z + 0.22), mat(("egyptian_blue", "red_ochre")[i % 2], rough=0.6), parent=r)
        box((0.32, 0.045, 0.035), (x, y0 - 0.035, z + 0.44), "wood_dark", parent=r)
        rod((x - 0.13, y0 - 0.05, z + 0.06), (x + 0.13, y0 - 0.05, z + 0.38), 0.012, "wood_dark", r)
        cm, mn, bl = coats[i]
        horse_head((x, y0 - 0.06, z + 0.45), 0, r, cm, mn, bl)
    # hay loft door in the gable end, hay spilling out, a hoist beam
    window((x1, c[1], z + h + 0.12), pi / 2, r, 0.22, 0.2, "egyptian_blue", shutters=True)
    blob(0.1, (x1 + 0.03, c[1], z + h + 0.05), "straw", (0.6, 1.4, 0.6), r, 3, 0.3, 1)
    rod((x1, c[1], z + h + 0.34), (x1 + 0.3, c[1], z + h + 0.34), 0.02, "wood_dark", r)
    rod((x1 + 0.28, c[1], z + h + 0.34), (x1 + 0.28, c[1], z + 0.5), 0.004, "wool", r, 4)
    # a feed shed on the +X end: hay under a lean-to
    shed_roof(0.62, 0.3, 0.2, (0.78, 0.62, 0.72), r, rot_z=pi / 2, over=0.04, seed=73)
    for y in (0.36, 0.9):
        box((0.04, 0.04, 0.72), (0.92, y, 0.36), "wood_dark", parent=r)
    for i, (x, y, zz) in enumerate(((0.79, 0.45, z), (0.79, 0.77, z), (0.79, 0.61, z + 0.16))):
        hay_bale((x, y, zz), pi / 2, r, (0.3, 0.2, 0.16))
    # paddock: post-and-rail fence round the front, a horse, trough, hay
    fence([(-0.93, 0.25), (-0.93, -0.93), (-0.1, -0.93)], r, 0.42)
    fence([(0.3, -0.93), (0.93, -0.93), (0.93, 0.25)], r, 0.42)
    for sx in (-0.1, 0.3):
        box((0.07, 0.07, 0.6), (sx, -0.93, 0.3), "wood", parent=r)
        sphere(0.04, (sx, -0.93, 0.62), gold(), parent=r, segs=6)
    # saddle cloths in player colour over the rails
    for (x, y, rz) in ((-0.93, -0.45, pi / 2), (0.6, -0.93, 0.0)):
        e = face((x, y, 0.42), rz, r)
        cloth(0.3, 0.2, mat(player, rough=0.7), (0, -0.02, 0.0), 0, e, hang=True, wave=0.01, nx=4)
        cloth(0.3, 0.2, mat(player, rough=0.7), (0, 0.02, 0.0), pi, e, hang=True, wave=0.01, nx=4)
        box((0.3, 0.05, 0.02), (0, 0, 0.0), gold(), parent=e)
    _horse((0.36, -0.5, z), -1.1, r, coat=(0.62, 0.38, 0.2), cloth=[gold(), mat(player, rough=0.7), gold()])
    trough((-0.45, -0.62, z), 0, r, 0.55)
    hay_bale((-0.62, -0.2, z), 0.4, r)
    blob(0.12, (-0.28, -0.22, z + 0.02), "straw", (1.4, 1, 0.5), r, 5, 0.3, 1)
    # bucket and pitchfork
    lathe([(0.05, 0), (0.06, 0.1), (0.055, 0.1)], "cedar", (-0.1, -0.7, z), parent=r, segs=8)
    rod((-0.88, 0.0, z), (-0.8, 0.05, z + 0.62), 0.01, "wood", r)
    for k in (-1, 0, 1):
        rod((-0.8 + k * 0.02, 0.05, z + 0.62), (-0.79 + k * 0.025, 0.06, z + 0.74), 0.005, "iron", r)
    banner((-0.93, 0.3, z), 1.5, player, r, 0.28, 0.5, rot_z=0, seed=7)
    return r


# ---------------------------------------------------------- archery range


def archery_range(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. Three painted targets against an earth bank at the back, a
    shooting line, lane pennants, and a shelter with bow racks."""
    r = empty("archery_range", loc, rot_z, parent)
    _pad(r, 2, "sand")
    z = 0.03
    # earth bank and a stone backstop along the back
    bx0, bx1 = -0.93, 0.36
    box((bx1 - bx0, 0.2, 0.62), ((bx0 + bx1) / 2, 0.84, z + 0.31), courses("limestone", 8.0), parent=r, bevel=0.012)
    box((bx1 - bx0 + 0.04, 0.24, 0.05), ((bx0 + bx1) / 2, 0.84, z + 0.64), "red_ochre", parent=r, bevel=0.01)
    for i in range(5):
        blob(0.16, (-0.82 + i * 0.28, 0.66, z + 0.05), ("grass", "earth", "grass_dry")[i % 3], (1.2, 0.9, 0.7), r, i, 0.25, 1)
    # targets, lanes, the shooting line
    # (the targets kept left of the shelter's line of sight: nothing stands
    # between them and the camera)
    for i, x in enumerate((-0.74, -0.34, 0.06)):
        target((x, 0.52, z + 0.58), 0.0, r, 0.23, 3, seed=i)
        box((0.025, 1.2, 0.008), (x - 0.2, 0.0, z + 0.012), "whitewash", parent=r)
    box((0.025, 1.2, 0.008), (0.26, 0.0, z + 0.012), "whitewash", parent=r)
    box((1.36, 0.04, 0.008), (-0.25, -0.6, z + 0.012), "whitewash", parent=r)
    for i, x in enumerate((-0.92, -0.54, -0.14, 0.26)):
        flag((x, 0.7, z), 1.1 if i % 3 == 0 else 0.95, player, r, w=0.3, fh=0.16, pennant=True, seed=i)
    # a low rail on the shooting line, with quivers and a bow on it
    for x in (-0.9, -0.3, 0.3, 0.9):
        box((0.05, 0.05, 0.4), (x, -0.76, z + 0.2), "wood_dark", parent=r)
    rod((-0.92, -0.76, z + 0.36), (0.92, -0.76, z + 0.36), 0.02, "wood", r)
    quiver((-0.62, -0.81, z), r, player, 0.2)
    quiver((-0.02, -0.81, z), r, player, -0.15)
    quiver((0.55, -0.81, z), r, player, 0.1)
    bow((-0.45, -0.805, z + 0.28), 0.0, r, 0.42)
    bow((0.72, -0.805, z + 0.28), 0.0, r, 0.42, "leather")
    # the shelter in the +X back corner, behind and right of the targets:
    # posts, a tiled lean-to, a bow rack
    sx0, sx1, sy0, sy1 = 0.6, 0.95, 0.12, 0.95
    shed_roof(sy1 - sy0, sx1 - sx0, 0.24, ((sx0 + sx1) / 2, (sy0 + sy1) / 2, 0.98), r, rot_z=pi / 2, over=0.05, seed=81)
    for y in (sy0 + 0.04, (sy0 + sy1) / 2, sy1 - 0.04):
        box((0.06, 0.06, 1.22), (sx0 + 0.04, y, 0.61), "wood_dark", parent=r)
        box((0.06, 0.06, 0.98), (sx1 - 0.04, y, 0.49), "wood_dark", parent=r)
    box((0.06, sy1 - sy0, 0.06), (sx1 - 0.04, (sy0 + sy1) / 2, 0.96), "wood", parent=r)
    box((0.06, sy1 - sy0, 0.06), (sx0 + 0.04, (sy0 + sy1) / 2, 1.2), "wood", parent=r)
    cloth(0.6, 0.2, mat(player, rough=0.7), (sx1 + 0.0, (sy0 + sy1) / 2, 0.93), pi / 2, r, hang=True, wave=0.01, nx=5)
    cyl(0.055, 0.012, (sx1 + 0.015, (sy0 + sy1) / 2, 0.83), gold(), rot=(0, pi / 2, 0), parent=r, segs=8)
    # an A-frame rack of bows, side-on to the camera
    rx, ry = (sx0 + sx1) / 2 + 0.0, (sy0 + sy1) / 2 - 0.05
    for y in (ry - 0.3, ry + 0.3):
        for sx in (-1, 1):
            rod((rx + sx * 0.1, y, z), (rx, y, z + 0.66), 0.014, "wood_dark", r)
    rod((rx, ry - 0.34, z + 0.64), (rx, ry + 0.34, z + 0.64), 0.016, "wood", r)
    for i in range(4):
        bow((rx - 0.03, ry - 0.24 + i * 0.16, z + 0.4), pi / 2, r, 0.46, ("wood_dark", "leather")[i % 2])
    for i in range(2):
        quiver((0.75, -0.2 - i * 0.2, z), r, player, -0.12, rz=pi / 2)
    # a barrel of arrows, a jar, spare straw for the targets
    bx, by = 0.68, -0.5
    lathe([(0.1, 0), (0.12, 0.12), (0.1, 0.24), (0.09, 0.24)], "cedar", (bx, by, z), parent=r, segs=10)
    for k in range(7):
        a = k * 0.9
        p = (bx + 0.05 * cos(a), by + 0.05 * sin(a), z + 0.2)
        rod(p, (p[0] + 0.02 * cos(a), p[1] + 0.02 * sin(a), z + 0.42), 0.005, "wood", r, 4)
        box((0.006, 0.03, 0.05), (p[0] + 0.02 * cos(a), p[1] + 0.02 * sin(a), z + 0.4), ("white", "red_ochre")[k % 2], parent=r)
    amphora((0.38, -0.5, z), 0.6, parent=r, s=0.32)
    hay_bale((-0.62, -0.38, z), 0.3, r)
    return r


# --------------------------------------------------------- siege workshop


def wheel(loc, parent=None, r=0.2, rot=(pi / 2, 0, 0), spokes=6, m="wood_dark"):
    """A spoked wheel; rot turns its axle (default along Y)."""
    e = empty("wheel", loc, 0, parent)
    e.rotation_euler = rot
    lathe([(r * 0.82, -0.025), (r, -0.025), (r, 0.025), (r * 0.82, 0.025)], m, parent=e, segs=16,
          smooth=False)
    for k in range(spokes):
        a = k * pi / spokes
        box((r * 1.7, 0.024, 0.024), (0, 0, 0), "wood", rot=(0, 0, a), parent=e)
    cyl(r * 0.2, 0.08, (0, 0, 0), bronze(), parent=e, segs=8)
    return e


def catapult(loc, rz=0.0, parent=None, player=BLUE, done=False):
    """A torsion stone-thrower on a wheeled bed, long axis local X, shooting
    toward -X. done=False: half built, one wheel off, its arm on trestles."""
    e = empty("catapult", loc, rz, parent)
    wr = 0.16
    zb = wr + 0.04
    for sy in (-1, 1):
        box((1.0, 0.07, 0.08), (0, sy * 0.2, zb), "cedar", parent=e, bevel=0.008)
    for x in (-0.44, -0.1, 0.2, 0.44):
        box((0.07, 0.48, 0.06), (x, 0, zb + 0.06), "wood", parent=e)
    box((0.1, 0.5, 0.05), (-0.1, 0, zb + 0.1), mat(player, rough=0.7), parent=e)
    for x, sy in ((-0.32, -1), (0.32, -1), (-0.32, 1), (0.32, 1)):
        if not done and (x, sy) == (0.32, -1):
            continue
        wheel((x, sy * 0.28, wr), e, wr)
    for x in (-0.32, 0.32):
        rod((x, -0.3, wr), (x, 0.3, wr), 0.02, "wood_dark", e)
    # the torsion frame: two uprights and a crossbar with bronze-capped springs
    for sy in (-1, 1):
        box((0.08, 0.07, 0.55), (-0.1, sy * 0.18, zb + 0.32), "cedar", parent=e)
        rod((0.24, sy * 0.18, zb + 0.06), (-0.08, sy * 0.18, zb + 0.5), 0.022, "wood", e)
        cyl(0.05, 0.08, (-0.1, sy * 0.24, zb + 0.28), bronze(), rot=(pi / 2, 0, 0), parent=e, segs=10)
    box((0.1, 0.46, 0.08), (-0.1, 0, zb + 0.6), "cedar", parent=e)
    cyl(0.06, 0.3, (-0.1, 0, zb + 0.28), "wool", rot=(pi / 2, 0, 0), parent=e, segs=10)
    # the arm
    if done:
        rod((-0.1, 0, zb + 0.28), (0.4, 0, zb + 0.18), 0.035, "wood", e)
        lathe([(0.02, 0), (0.08, 0.04), (0.09, 0.07), (0.07, 0.07)], "leather", (0.42, 0, zb + 0.18), parent=e, segs=10)
    else:   # mounted, standing up against the crossbar: the shape that says
        # "stone-thrower" at a glance; one wheel still leaning by the bed
        tip = (-0.4, 0, zb + 1.15)
        rod((-0.1, 0, zb + 0.28), tip, 0.05, "wood_dark", e, 8, r_top=0.036)
        lathe([(0.02, 0), (0.12, 0.05), (0.13, 0.09), (0.1, 0.09)], "leather", tip, parent=e, segs=10,
              rot=(0, -0.6, 0))
        sphere(0.07, (tip[0] - 0.03, 0, tip[2] + 0.08), "rock", parent=e, segs=10)
        for k in range(2):
            rod((-0.1, (-1) ** k * 0.06, zb + 0.28), (0.36, (-1) ** k * 0.18, zb + 0.08), 0.008, "wool", e, 4)
        wheel((0.34, -0.42, wr + 0.0), e, wr, rot=(pi / 2, 0.35, 0))
    return e


def sawhorse(loc, rz=0.0, parent=None):
    e = empty("sawhorse", loc, rz, parent)
    for x in (-0.18, 0.18):
        for sy in (-1, 1):
            rod((x, sy * 0.12, 0), (x, 0, 0.32), 0.016, "wood_dark", e)
    rod((-0.24, 0, 0.32), (0.24, 0, 0.32), 0.025, "wood", e)
    log((-0.4, 0.0, 0.4), (0.38, 0.03, 0.4), 0.06, e)
    box((0.3, 0.008, 0.06), (0.05, 0.0, 0.48), "iron", rot=(0, 0.2, 0), parent=e)
    box((0.04, 0.03, 0.1), (0.2, 0.0, 0.53), "wood", rot=(0, 0.2, 0), parent=e)
    return e


def timber_frame(r, x0, x1, y0, y1, h, rise=0.36, wood="cedar", dark="wood_dark", planked=0.45, seed=0):
    """An open timber shed frame, ridge along X: posts, wall plates, tie
    beams, rafters, and planks over only the back part of the roof
    (`planked` of each slope, from the ridge down), so it reads as light
    framing, not a building."""
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    nx = max(2, round((x1 - x0) / 0.45))
    xs = [x0 + (x1 - x0) * i / nx for i in range(nx + 1)]
    for x in xs:
        for y in (y0, y1):
            box((0.07, 0.07, h), (x, y, h / 2), wood, parent=r)
        rod((x, y0, h), (x, y1, h), 0.026, dark, r)                       # tie beam
        rod((x, y0 - 0.06, h - 0.03), (x, cy, h + rise), 0.024, wood, r)   # rafters
        rod((x, y1 + 0.06, h - 0.03), (x, cy, h + rise), 0.024, wood, r)
    for y in (y0, y1):
        box((x1 - x0 + 0.1, 0.07, 0.07), (cx, y, h - 0.02), dark, parent=r)
        for x in xs[:-1]:                                                 # knee braces
            rod((x, y, h - 0.28), (x + 0.2, y, h - 0.03), 0.016, dark, r)
    rod((x0 - 0.05, cy, h + rise), (x1 + 0.05, cy, h + rise), 0.03, dark, r)
    # planks on the back slope, part way down
    rnd = random.Random(seed)
    L = hypot(y1 - cy + 0.06, rise)
    n = max(2, round(L * planked / 0.12))
    th = atan2(rise, y1 - cy + 0.06)
    for k in range(n):
        t = (k + 0.5) / (n / planked) if planked else 0
        yy = cy + (y1 - cy + 0.06) * t
        zz = h + rise - rise * t + 0.03
        box((x1 - x0 + 0.08 - rnd.uniform(0, 0.25), 0.12, 0.018), (cx - rnd.uniform(0, 0.1), yy, zz),
            ("wood", "cedar")[k % 2], rot=(th, 0, 0), parent=r)


def siege_workshop(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. The engine is the hero: a big stone-thrower nearly built in the
    sunny front of the yard, side-on, its throwing arm up, one wheel still
    off; behind it a light open timber frame (no walls, half its roof
    planked) over a workbench, timber, a sawhorse, stone shot."""
    r = empty("siege_workshop", loc, rot_z, parent)
    _pad(r, 2, "sand")
    z = 0.03
    x0, x1, y0, y1, h = -0.9, 0.45, 0.3, 0.9, 0.95
    fresh = (0.78, 0.62, 0.42)        # new-sawn, pale: the frame stays behind the engine
    timber_frame(r, x0, x1, y0, y1, h, 0.34, wood=fresh, dark=tint(fresh, 0.85), seed=91)
    cloth(0.6, 0.2, mat(player, rough=0.7), ((x0 + x1) / 2, y0 - 0.04, z + h - 0.02), 0, r, hang=True, wave=0.012,
          nx=6)
    box((0.6, 0.012, 0.025), ((x0 + x1) / 2, y0 - 0.055, z + h - 0.19), gold(), parent=r)
    yb = y1 - 0.12
    box((0.7, 0.24, 0.04), (-0.4, yb - 0.08, z + 0.42), "cedar", parent=r)
    for x in (-0.7, -0.1):
        box((0.05, 0.2, 0.4), (x, yb - 0.08, z + 0.2), "wood_dark", parent=r)
    box((0.28, 0.008, 0.08), (-0.4, yb - 0.1, z + 0.45), "iron", parent=r)
    for i in range(4):
        box((0.9, 0.12, 0.025), (-0.4, 0.5 + i * 0.002, z + 0.013 + i * 0.026), ("cedar", "wood")[i % 2],
            rot=(0, 0, 0.03 * i), parent=r)
    # the engine: big, in front, in the sun, side-on to the camera
    e = catapult((0.12, -0.3, z), pi / 4 + pi, r, player, done=False)
    e.scale = (1.45, 1.45, 1.45)
    timber_stack((0.72, 0.42, z), pi / 2, r, (3, 2, 1), 0.7, 0.05)
    for i in range(6):
        a = i * 1.05
        d = 0.0 if i == 0 else 0.11
        sphere(0.07, (-0.72 + d * cos(a), -0.15 + d * sin(a), z + 0.07 + (0.1 if i == 0 else 0)), "rock",
               parent=r, segs=10)
    for k in range(3):
        cyl(0.11 - k * 0.025, 0.03, (-0.55, -0.82, z + 0.015 + k * 0.03), "wool", parent=r, segs=12)
    sawhorse((0.62, 0.0, z), 1.2, r)
    banner((-0.9, -0.9, z), 1.4, player, r, 0.26, 0.46, seed=2)
    return r


# ---------------------------------------------------------------- academy


def statue(loc, rz=0.0, parent=None, m="marble", s=1.0, scroll=True, beard=True, plinth="marble"):
    """A robed philosopher on a plinth, facing local -Y, about 1.2 tall at s=1."""
    e = face(loc, rz, parent)
    e.scale = (s, s, s)
    box((0.34, 0.34, 0.08), (0, 0, 0.04), plinth, parent=e, bevel=0.01)
    box((0.28, 0.28, 0.42), (0, 0, 0.29), noisy(plinth, 0.05, 6.0) if isinstance(plinth, str) else plinth,
        parent=e, bevel=0.01)
    box((0.34, 0.34, 0.06), (0, 0, 0.53), plinth, parent=e, bevel=0.01)
    box((0.29, 0.29, 0.02), (0, 0, 0.47), "egyptian_blue", parent=e)
    z = 0.56
    lathe([(0.11, 0), (0.12, 0.08), (0.1, 0.3), (0.105, 0.42), (0.12, 0.5), (0.1, 0.56), (0.05, 0.6)], m,
          (0, 0, z), parent=e, segs=10)
    rod((0.11, 0, z + 0.52), (-0.08, -0.05, z + 0.15), 0.035, m, e, 6)        # the himation's sweep
    sphere(0.075, (0, -0.005, z + 0.68), m, parent=e, segs=10)
    if beard:
        sphere(0.05, (0, -0.045, z + 0.635), m, scale=(1, 0.8, 1.2), parent=e, segs=8)
    rod((-0.11, 0, z + 0.5), (-0.13, -0.08, z + 0.32), 0.03, m, e, 6)
    rod((0.11, 0, z + 0.5), (0.1, -0.12, z + 0.4), 0.03, m, e, 6)
    if scroll:
        cyl(0.025, 0.14, (0.1, -0.14, z + 0.4), "linen", rot=(0, pi / 2, 0.3), parent=e, segs=8)
    return e


def scroll_rack(loc, rz=0.0, parent=None, w=0.6, h=0.7, seed=0):
    """Pigeonhole shelves full of scrolls, against a wall (front -Y)."""
    e = face(loc, rz, parent)
    rnd = random.Random(seed)
    box((w, 0.16, h), (0, 0.0, h / 2), "cedar", parent=e)
    nx, nz = max(2, round(w / 0.15)), max(2, round(h / 0.17))
    for i in range(nx):
        for k in range(nz):
            x = -w / 2 + (i + 0.5) * w / nx
            zz = (k + 0.5) * h / nz
            box((w / nx - 0.03, 0.02, h / nz - 0.03), (x, -0.075, zz), mat((0.2, 0.13, 0.08)), parent=e)
            for j in range(3):
                cyl(0.022, 0.08, (x - 0.03 + j * 0.03, -0.09, zz - 0.03 + (j % 2) * 0.04),
                    ("linen", "wool", "straw")[rnd.randrange(3)], rot=(pi / 2, 0, 0), parent=e, segs=6)
    return e


def academy(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A colonnaded court round a philosopher's statue: a library
    hall at the back, stoas on two sides, scrolls, an exedra bench."""
    r = empty("academy", loc, rot_z, parent)
    box((1.92, 1.92, 0.04), (0, 0, 0.015), courses("marble", 4.0, 0.84, 0.07), parent=r, bevel=0.012)
    z = 0.035
    # library hall at the back
    x0, x1, y0, y1, h = -0.95, 0.95, 0.5, 0.95, 1.3
    c = (0, (y0 + y1) / 2)
    wall_block(x1 - x0, y1 - y0, h, (c[0], c[1], z), r, wall="plaster", dado="egyptian_blue", line="yellow_ochre")
    _band(x1 - x0, y1 - y0, z + h - 0.05, c, r, "red_ochre", 0.1)
    for i in range(10):
        box((0.07, 0.02, 0.04), (x0 + 0.15 + i * 0.185, y0 - 0.035, z + h - 0.05), gold(), parent=r)
    gable_roof(x1 - x0, y1 - y0, 0.42, (c[0], c[1], z + h), r, over=0.1, gable=mat("egyptian_blue", rough=0.6), seed=101,
               kind=SLATE)
    for sx in (-1, 1):
        _acroterion((sx * (x1 + 0.08), c[1], z + h + 0.46), r, 0.7, pi / 2)
    cyl(0.1, 0.03, (x1 + 0.01, c[1], z + h + 0.15), gold(), rot=(0, pi / 2, 0), parent=r, segs=12)
    door((0.0, y0, z), 0, r, w=0.4, h=0.86, colour="egyptian_blue", lintel=gold())
    scroll_rack((-0.52, y0, z + 0.02), 0, r, 0.56, 0.72, seed=1)
    scroll_rack((0.52, y0, z + 0.02), 0, r, 0.56, 0.72, seed=2)
    window((x1, c[1], z + 0.82), pi / 2, r, 0.2, 0.3, "egyptian_blue")
    # stoa along the left (-X) side with a lean-to roof
    sx0, sx1, sy0, sy1 = -0.95, -0.45, -0.95, 0.5
    wall_block(0.14, sy1 - sy0, 1.12, (sx0 + 0.07, (sy0 + sy1) / 2, z), r, wall="plaster", dado="egyptian_blue", line="yellow_ochre")
    shed_roof(sy1 - sy0 + 0.04, sx1 - sx0, 0.22, ((sx0 + sx1) / 2, (sy0 + sy1) / 2, 0.92), r, rot_z=pi / 2, over=0.06, seed=103,
              kind=SLATE)
    box((0.07, sy1 - sy0, 0.08), (sx1 - 0.02, (sy0 + sy1) / 2, 0.9), "red_ochre", parent=r)
    ys = [sy0 + 0.08 + i * (sy1 - sy0 - 0.16) / 3 for i in range(4)]
    for y in ys:
        column((sx1 - 0.02, y, z), 0.84, 0.055, r, band="egyptian_blue", band2="red_ochre", flutes=8)
    for i in range(3):
        y = (ys[i] + ys[i + 1]) / 2
        e = face((sx1 - 0.02, y, 0.86), pi / 2, r)
        cloth(0.2, 0.36, mat(player, rough=0.7), (0, -0.03, 0), 0, e, hang=True, wave=0.01, nx=4)
        cyl(0.045, 0.012, (0, -0.05, -0.17), gold(), rot=(pi / 2, 0, 0), parent=e, segs=8)
    for i, y in enumerate((-0.55, -0.05)):
        bench((sx0 + 0.2, y, z), -pi / 2, r, 0.4, "marble")
    # the court: statue, exedra, a table of scrolls, a cypress
    statue((0.12, -0.12, z), 0.35, r, s=1.0)
    ex, ey = 0.52, -0.52
    pts = []
    for i in range(7):
        a = -pi * 0.15 + i * pi * 0.65 / 6
        pts.append((ex + 0.36 * cos(a + pi * 0.25), ey + 0.36 * sin(a + pi * 0.25)))
    for i, (px, py) in enumerate(pts):
        box((0.15, 0.13, 0.2), (px, py, z + 0.1), noisy("marble", 0.05, 6.0), rot=(0, 0, atan2(py - ey, px - ex)),
            parent=r, bevel=0.01)
    box((0.3, 0.2, 0.03), (ex - 0.02, ey + 0.0, z + 0.36), "cedar", parent=r)
    for sx in (-1, 1):
        box((0.03, 0.16, 0.34), (ex - 0.02 + sx * 0.12, ey, z + 0.17), "wood_dark", parent=r)
    cyl(0.025, 0.2, (ex - 0.02, ey, z + 0.39), "linen", rot=(0, pi / 2, 0.4), parent=r, segs=8)
    box((0.16, 0.12, 0.004), (ex - 0.06, ey - 0.02, z + 0.378), "linen", rot=(0, 0, 0.3), parent=r)
    # an armillary sphere on a stand
    cyl(0.025, 0.4, (ex + 0.1, ey + 0.08, z + 0.2), bronze(), parent=r, segs=8)
    for k, rot in enumerate(((0, 0, 0), (pi / 2, 0, 0), (pi / 2, 0, pi / 2), (0.4, 0, 0))):
        e = empty("ring", (ex + 0.1, ey + 0.08, z + 0.52), 0, r)
        e.rotation_euler = rot
        lathe([(0.1, -0.006), (0.11, -0.006), (0.11, 0.006), (0.1, 0.006)], gold(), parent=e, segs=16, smooth=False)
    sphere(0.025, (ex + 0.1, ey + 0.08, z + 0.52), "egyptian_blue", parent=r, segs=8)
    # cypress and pots
    for k in range(4):
        blob(0.14 - k * 0.025, (0.8, 0.28, z + 0.25 + k * 0.22), "leaf_dark", (0.9, 0.9, 1.5), r, k + 3, 0.2, 1)
    rod((0.8, 0.28, z), (0.8, 0.28, z + 0.2), 0.03, "wood_dark", r)
    pot((-0.3, -0.82, z), r, 0.17, "leaf", flowers="flower_red", seed=11)
    pot((0.84, -0.86, z), r, 0.16, "leaf_olive", seed=12)
    banner((0.2, -0.9, z), 1.4, player, r, 0.28, 0.48, seed=11)
    return r


# ------------------------------------------------------ government centre


def government_centre(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A council house on a podium: a two-storey hall, a pedimented
    Ionic porch up a broad stair, banners and bronze statues either side."""
    r = empty("government_centre", loc, rot_z, parent)
    _pad(r, 2, "sand")
    z = 0.03
    # podium with a moulded edge
    px0, px1, py0, py1, ph = -0.92, 0.92, -0.42, 0.94, 0.26
    box((px1 - px0, py1 - py0, ph), ((px0 + px1) / 2, (py0 + py1) / 2, z + ph / 2), courses("marble", 6.0, 0.8, 0.06),
        parent=r, bevel=0.01)
    box((px1 - px0 + 0.04, py1 - py0 + 0.04, 0.04), ((px0 + px1) / 2, (py0 + py1) / 2, z + ph - 0.01), "marble",
        parent=r, bevel=0.01)
    zp = z + ph + 0.01
    # broad stair down the front
    n = 4
    for i in range(n):
        hh = ph * (n - i) / (n + 1)
        box((1.22 - i * 0.0, 0.12, hh), (0, py0 - 0.06 - i * 0.12, z + hh / 2), noisy("marble", 0.05, 3.0),
            parent=r, bevel=0.008)
    for sx in (-1, 1):
        box((0.14, 0.5, ph + 0.06), (sx * 0.68, py0 - 0.23, z + (ph + 0.06) / 2), courses("marble", 6.0, 0.8, 0.06),
            parent=r, bevel=0.01)
    # the hall: two storeys, ridge along X
    hx0, hx1, hy0, hy1, h = -0.86, 0.86, 0.12, 0.9, 1.62
    hc = (0, (hy0 + hy1) / 2)
    wall_block(hx1 - hx0, hy1 - hy0, h, (hc[0], hc[1], zp), r, wall="plaster", dado="red_ochre", line="yellow_ochre",
               base=None, dado_h=0.3, base_h=0.0)
    _band(hx1 - hx0, hy1 - hy0, zp + 0.86, hc, r, "egyptian_blue", 0.08)
    _band(hx1 - hx0, hy1 - hy0, zp + 0.9, hc, r, gold(), 0.02, 0.06)
    zt = zp + h
    box((hx1 - hx0 + 0.08, hy1 - hy0 + 0.08, 0.12), (hc[0], hc[1], zt - 0.06), "red_ochre", parent=r)
    for i in range(13):
        box((0.06, 0.02, 0.05), (hx0 + 0.1 + i * 0.128, hy0 - 0.045, zt - 0.06), gold(), parent=r)
    for i in range(6):
        box((0.02, 0.06, 0.05), (hx1 + 0.045, hy0 + 0.08 + i * 0.125, zt - 0.06), gold(), parent=r)
    box((hx1 - hx0 + 0.14, hy1 - hy0 + 0.14, 0.05), (hc[0], hc[1], zt + 0.02), "marble", parent=r, bevel=0.01)
    gable_roof(hx1 - hx0, hy1 - hy0, 0.42, (hc[0], hc[1], zt + 0.045), r, over=0.1, gable=mat("egyptian_blue", rough=0.6),
               antefix=gold(), seed=111)
    for sx in (-1, 1):
        _acroterion((sx * (hx1 + 0.1), hc[1], zt + 0.5), r, 0.85, pi / 2)
    cyl(0.12, 0.03, (hx1 + 0.03, hc[1], zt + 0.2), gold(), rot=(0, pi / 2, 0), parent=r, segs=12)
    for i, x in enumerate((-0.66, -0.4, 0.4, 0.66)):
        window((x, hy0, zp + 1.22), 0, r, 0.15, 0.32, "egyptian_blue")
    for y in (hy0 + 0.2, hy0 + 0.55):
        window((hx1, y, zp + 1.22), pi / 2, r, 0.15, 0.32, "egyptian_blue")
        window((hx1, y, zp + 0.55), pi / 2, r, 0.15, 0.3, "egyptian_blue")
    # the porch: four columns, an entablature, a pediment facing -Y
    qx, qy0, qy1, qh = 0.5, -0.36, hy0, 1.12
    box((qx * 2, qy1 - qy0, 0.03), (0, (qy0 + qy1) / 2, zp + 0.005), noisy("marble", 0.05, 3.0), parent=r)
    door((0, hy0, zp), 0, r, w=0.44, h=0.92, metal=True, frame="egyptian_blue", lintel=gold())
    for x in (-0.42, -0.14, 0.14, 0.42):
        column((x, qy0 + 0.08, zp), qh, 0.065, r, band=gold(), band2="egyptian_blue", flutes=10)
        lathe([(0.07, 0), (0.09, 0.03), (0.07, 0.05)], "marble", (x, qy0 + 0.08, zp), parent=r, segs=10)
        sphere(0.04, (x - 0.07, qy0 + 0.05, zp + qh - 0.08), "marble", parent=r, segs=8, scale=(0.6, 1, 1))
        sphere(0.04, (x + 0.07, qy0 + 0.05, zp + qh - 0.08), "marble", parent=r, segs=8, scale=(0.6, 1, 1))
    ze = zp + qh
    box((qx * 2 + 0.08, qy1 - qy0 + 0.04, 0.12), (0, (qy0 + qy1) / 2 - 0.02, ze + 0.06), "marble", parent=r)
    box((qx * 2 + 0.09, qy1 - qy0 + 0.05, 0.05), (0, (qy0 + qy1) / 2 - 0.02, ze + 0.14), "egyptian_blue", parent=r)
    for i in range(9):
        box((0.05, 0.02, 0.035), (-qx + 0.07 + i * 0.108, qy0 - 0.045, ze + 0.14), gold(), parent=r)
    box((qx * 2 + 0.12, qy1 - qy0 + 0.08, 0.04), (0, (qy0 + qy1) / 2 - 0.02, ze + 0.185), "marble", parent=r)
    zr = ze + 0.205
    gable_roof(qy1 - qy0 + 0.04, qx * 2 + 0.06, 0.34, (0, (qy0 + qy1) / 2 - 0.0, zr), r, axis="y", over=0.06,
               antefix=gold(), seed=113)
    _pediment(face((0, qy0 - 0.06, zr), 0, r), qx * 2 + 0.12, 0.34, figures=False)
    sh = shield((0, qy0 - 0.08, zr + 0.13), 0, r, 0.1, player, 0)
    _acroterion((0, qy0 - 0.08, zr + 0.36), r, 0.9, 0)
    # banners and statues flanking the stair
    for sx in (-1, 1):
        banner((sx * 0.84, py0 - 0.36, z), 1.85, player, r, 0.3, 0.66, seed=sx + 5)
        statue((sx * 0.68, py0 - 0.3, z + ph + 0.06), 0.0, r, m=bronze(), s=0.62, scroll=False, beard=True, plinth="marble")
        brazier((sx * 0.8, -0.2, zp), r, 0.7)
    flag((0.0, hc[1] + 0.05, zt + 0.47), 0.8, player, r, w=0.62, fh=0.36, pole_r=0.025, seed=6)
    return r


# ------------------------------------------------------------------- farm


def farm(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, ripe=True):
    """2 x 2. A ploughed plot of wheat in rows, low enough to see people
    working round it: a stone kerb, a tool shed on the far corner, a
    scarecrow in the owner's colours, sheaves on the stubble at the front."""
    r = empty("farm", loc, rot_z, parent)
    z = 0.0
    # tilled earth and a low dry-stone kerb
    box((1.94, 1.94, 0.04), (0, 0, 0.01), noisy("earth", 0.14, 4.0, 1.0), parent=r, bevel=0.015)
    kerb = noisy("rock", 0.14, 9.0)
    for (a, b) in (((-0.94, -0.94), (0.94, -0.94)), ((0.94, -0.94), (0.94, 0.94)),
                   ((0.94, 0.94), (-0.94, 0.94)), ((-0.94, 0.94), (-0.94, -0.94))):
        cx_, cy_ = (a[0] + b[0]) / 2, (a[1] + b[1]) / 2
        along_x = abs(b[0] - a[0]) > abs(b[1] - a[1])
        box((abs(b[0] - a[0]) + 0.08, abs(b[1] - a[1]) - 0.08, 0.08 if along_x else 0.075), (cx_, cy_, 0.04), kerb,
            parent=r, bevel=0.025)
    rnd = random.Random(7)
    dk = mat(tint("earth", 0.72))
    rows = [-0.78 + i * 0.13 for i in range(13)]
    wheat = [mat(tint("wheat", f), rough=0.9) for f in (0.92, 1.0, 1.06)]
    pts = [[], [], []]
    stalk = mat(tint("grass_dry", 0.9))
    for j, y in enumerate(rows):
        box((1.76, 0.055, 0.035), (0.0, y, 0.04), dk if j % 2 else mat(tint("earth", 0.85)), parent=r)
        if y < -0.45:                       # harvested: stubble
            for i in range(14):
                x = -0.8 + i * 0.12 + rnd.uniform(-0.02, 0.02)
                cyl(0.012, 0.06, (x, y, 0.08), "straw", parent=r, segs=4)
            continue
        for i in range(22):
            x = -0.86 + i * 0.082 + rnd.uniform(-0.015, 0.015)
            if x < -0.36 and y > 0.3:      # the shed's corner
                continue
            hh = rnd.uniform(0.2, 0.26) if ripe else rnd.uniform(0.08, 0.12)
            pts[rnd.randrange(3)].append(((x, y + rnd.uniform(-0.012, 0.012), 0.05 + hh * 0.6),
                                          (0.8, 0.75, hh / 0.09)))
    for k in range(3):
        clumps(pts[k], 0.045, wheat[k] if ripe else mat(("leaf", "grass", "leaf_olive")[k]), r, seed=k)
    # tool shed on the far corner
    sx, sy = -0.66, 0.66
    wall_block(0.46, 0.42, 0.62, (sx, sy, 0.03), r, dado="red_ochre", line="yellow_ochre", dado_h=0.14, base_h=0.06)
    gable_roof(0.46, 0.42, 0.24, (sx, sy, 0.65), r, over=0.08, gable=noisy("whitewash"), seed=121)
    door((sx + 0.02, sy - 0.21, 0.03), 0, r, w=0.2, h=0.46, colour="egyptian_blue")
    rod((sx + 0.26, sy - 0.12, 0.03), (sx + 0.26, sy - 0.18, 0.62), 0.01, "wood", r)
    box((0.03, 0.12, 0.05), (sx + 0.26, sy - 0.19, 0.64), "iron", parent=r)
    # scarecrow in the owner's colours
    qx, qy = 0.2, 0.18
    rod((qx, qy, 0.0), (qx, qy, 0.86), 0.018, "wood_dark", r)
    rod((qx - 0.24, qy, 0.64), (qx + 0.24, qy, 0.64), 0.016, "wood_dark", r)
    cloth(0.32, 0.36, mat(player, rough=0.75), (qx, qy - 0.02, 0.68), 0, r, hang=True, wave=0.03, nx=5)
    cloth(0.32, 0.36, mat(player, rough=0.75), (qx, qy + 0.02, 0.68), pi, r, hang=True, wave=0.03, nx=5)
    sphere(0.075, (qx, qy, 0.8), "linen", parent=r, segs=8)
    cyl(0.16, 0.025, (qx, qy, 0.86), "straw", parent=r, segs=12)
    cyl(0.08, 0.06, (qx, qy, 0.89), "straw", parent=r, segs=10, r_top=0.03)
    for sx_ in (-1, 1):
        blob(0.04, (qx + sx_ * 0.25, qy, 0.62), "straw", (1, 1, 1.4), r, 9 + sx_, 0.3, 1)
    # sheaves on the stubble, a basket, a sickle
    for i, (x, y) in enumerate(((-0.5, -0.66), (-0.2, -0.7), (0.15, -0.64), (0.5, -0.7))):
        e = empty("sheaf", (x, y, 0.04), i * 0.7, r)
        cyl(0.07, 0.26, (0, 0, 0.13), stalk, parent=e, segs=8, r_top=0.05)
        blob(0.08, (0, 0, 0.3), wheat[i % 3], (1, 1, 1.3), e, i, 0.3, 1)
        cyl(0.062, 0.03, (0, 0, 0.16), "thatch", parent=e, segs=8)
    basket((0.72, -0.74, 0.04), r, 0.11, 1, 3)
    for k in range(4):
        blob(0.035, (0.72 + 0.03 * cos(k * 1.6), -0.74 + 0.03 * sin(k * 1.6), 0.15), wheat[0], (1, 1, 1), r, k, 0.2, 1)
    arc_rod((0.42, -0.84, 0.05), 0.07, 0.2, 2.8, r, 0.008, "iron", "xy", 5)
    rod((0.49, -0.83, 0.05), (0.62, -0.8, 0.05), 0.012, "wood", r)
    return r


# ------------------------------------------------------------------- dock


def skiff(loc, rz=0.0, parent=None, player=BLUE, L=1.0):
    """A small fishing boat along local X, its waterline at loc.z, a furled
    sail in player colour on the yard, eyes on the bow."""
    e = empty("skiff", loc, rz, parent)
    hull = []
    n = 9
    for i in range(n + 1):
        t = i / n
        x = (t - 0.5) * L
        w = 0.19 * sin(pi * (0.08 + 0.84 * t)) ** 0.7
        zt = 0.17 + 0.1 * ((2 * t - 1) ** 4)
        hull.append((x, w, zt))
    vs, fs = [], []
    for x, w, zt in hull:
        vs += [(x, -w, zt), (x, -w * 0.75, 0.03), (x, 0, -0.03), (x, w * 0.75, 0.03), (x, w, zt)]
    for i in range(n):
        a, b = i * 5, (i + 1) * 5
        for k in range(4):
            fs.append((a + k, b + k, b + k + 1, a + k + 1))
    mesh(vs, fs, mat((0.22, 0.15, 0.1)), parent=e, smooth=True, name="hull")
    for sg in (-1, 1):   # painted strake
        pts = [(x, sg * (w + 0.004), zt - 0.04) for x, w, zt in hull]
        for p, q in zip(pts, pts[1:]):
            rod(p, q, 0.016, "red_ochre", e, 5)
    for i in range(n):
        x, w, zt = hull[i]
        if 1 <= i <= n - 2:
            box((0.02, w * 1.9, 0.02), (x, 0, zt - 0.06), "wood", parent=e)
    sphere(0.03, (L * 0.42, -0.07, 0.19), "white", parent=e, segs=6)
    sphere(0.016, (L * 0.43, -0.085, 0.19), "black", parent=e, segs=6)
    rod((0.05, 0, 0.05), (0.05, 0, 0.95), 0.016, "wood_dark", e)
    rod((0.05, -0.32, 0.86), (0.05, 0.32, 0.86), 0.012, "wood", e)
    cyl(0.05, 0.6, (0.05, -0.005, 0.83), mat(player, rough=0.7), rot=(pi / 2, 0, 0), parent=e, segs=8)
    for t in (-0.18, 0.0, 0.18):
        cyl(0.055, 0.03, (0.05, t, 0.83), "wool", rot=(pi / 2, 0, 0), parent=e, segs=8)
    rod((0.05, 0, 0.95), (L * 0.45, 0, 0.26), 0.004, "wool", e, 4)
    rod((0.05, 0, 0.95), (-L * 0.45, 0, 0.25), 0.004, "wool", e, 4)
    for i in range(2):
        rod((-0.25 + i * 0.5, -0.3, 0.2), (-0.1 + i * 0.5, 0.0, 0.15), 0.01, "wood", e, 4)
    return e


def net_rack(loc, rz=0.0, parent=None, w=0.7, h=0.8):
    """Two poles with a fishing net hung between them to dry, cork floats."""
    e = face(loc, rz, parent)
    for x in (-w / 2, w / 2):
        rod((x, 0, 0), (x, 0, h), 0.02, "wood_dark", e)
    rod((-w / 2 - 0.05, 0, h - 0.03), (w / 2 + 0.05, 0, h - 0.03), 0.014, "wood", e)
    vs, fs = [], []
    nx, nz = 8, 4
    for i in range(nx + 1):
        for k in range(nz + 1):
            u, v = i / nx, k / nz
            vs.append(((u - 0.5) * (w - 0.04), 0.02 * sin(u * 9), h - 0.05 - v * h * 0.62 - 0.08 * sin(pi * u) * v))
    for i in range(nx):
        for k in range(nz):
            a = i * (nz + 1) + k
            fs.append((a, a + nz + 1, a + nz + 2, a + 1))
    ob = mesh(vs, fs, mat((0.62, 0.55, 0.42), rough=0.95), parent=e, smooth=True, name="net")
    wf = ob.modifiers.new("net", "WIREFRAME")
    wf.thickness = 0.008
    for i in range(nx + 1):
        sphere(0.018, ((i / nx - 0.5) * (w - 0.04), 0.0, h - 0.05 - h * 0.62 - 0.08 * sin(pi * i / nx)),
               "yellow_ochre", parent=e, segs=6)
    return e


def _deck(r, x0, x1, y0, y1, z, along="x", seed=0, piles=True):
    """A planked deck on piles, top at z, open air beneath."""
    rnd = random.Random(seed)
    th = 0.05
    pl = [mat("cedar"), mat(tint("cedar", 0.88)), mat(tint("cedar", 1.08)), mat("wood")]
    if along == "x":
        n = max(1, round((y1 - y0) / 0.12))
        for i in range(n):
            yy = y0 + (i + 0.5) * (y1 - y0) / n
            box((x1 - x0, (y1 - y0) / n - 0.012, th), ((x0 + x1) / 2, yy, z - th / 2), pl[rnd.randrange(4)], parent=r)
    else:
        n = max(1, round((x1 - x0) / 0.12))
        for i in range(n):
            xx = x0 + (i + 0.5) * (x1 - x0) / n
            box(((x1 - x0) / n - 0.012, y1 - y0, th), (xx, (y0 + y1) / 2, z - th / 2), pl[rnd.randrange(4)], parent=r)
    box((x1 - x0, y1 - y0, 0.02), ((x0 + x1) / 2, (y0 + y1) / 2, z - th - 0.01), mat((0.2, 0.13, 0.08)), parent=r)
    dk = mat("wood_dark")
    for (a, b) in (((x0, y0), (x1, y0)), ((x1, y0), (x1, y1)), ((x0, y1), (x1, y1)), ((x0, y0), (x0, y1))):
        box((abs(b[0] - a[0]) + 0.07, abs(b[1] - a[1]) + 0.07, 0.08), ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2, z - th - 0.03),
            dk, parent=r)
    if piles:
        nxp = max(1, round((x1 - x0) / 0.5))
        nyp = max(1, round((y1 - y0) / 0.5))
        for i in range(nxp + 1):
            for k in range(nyp + 1):
                px, py = x0 + (x1 - x0) * i / nxp, y0 + (y1 - y0) * k / nyp
                cyl(0.05, z - 0.04, (px, py, (z - 0.04) / 2), "wood_dark", parent=r, segs=8)
                cyl(0.053, 0.08, (px, py, 0.04), mat((0.16, 0.2, 0.14), rough=0.6), parent=r, segs=8)


def dock(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """3 x 3, all of it over water (the ground draws the water; there is
    none here, only open air under the deck): a back landing stage, two
    piers round a slip with a boat shed over it and a skiff in it, nets,
    crates, jars, a crane."""
    r = empty("dock", loc, rot_z, parent)
    zd = 0.26
    E = 1.44
    # landing stage at the back, two piers forward round the slip
    _deck(r, -E, E, 0.5, E, zd, "x", 1)
    _deck(r, -E, -0.62, -E, 0.5, zd, "y", 2)
    _deck(r, 0.58, E, -E, 0.5, zd, "y", 3)
    # boat shed over the slip: posts on the piers, a tiled roof along Y
    sx0, sx1, sy0, sy1 = -0.7, 0.66, -0.55, 1.1
    for y in (sy0, (sy0 + sy1) / 2, sy1):
        for x in (sx0, sx1):
            box((0.08, 0.08, 1.3), (x, y, zd + 0.65), "wood_dark", parent=r)
    for x in (sx0, sx1):
        box((0.1, sy1 - sy0 + 0.1, 0.1), (x, (sy0 + sy1) / 2, zd + 1.3), "wood", parent=r)
    box((sx1 - sx0 + 0.1, 0.1, 0.1), ((sx0 + sx1) / 2, sy0, zd + 1.3), "wood", parent=r)
    box((sx1 - sx0 + 0.1, 0.1, 0.1), ((sx0 + sx1) / 2, sy1, zd + 1.3), "wood", parent=r)
    gable_roof(sy1 - sy0, sx1 - sx0, 0.5, ((sx0 + sx1) / 2, (sy0 + sy1) / 2, zd + 1.35), r, axis="y", over=0.12,
               gable=noisy("whitewash"), seed=131)
    gf = face(((sx0 + sx1) / 2, sy0 - 0.02, zd + 1.35), 0, r)
    mesh([(-0.6, 0, 0.02), (0.6, 0, 0.02), (0, 0, 0.44)], [(0, 1, 2)], noisy("whitewash"), parent=gf)
    cyl(0.1, 0.03, (0, -0.02, 0.17), mat(player, rough=0.6), rot=(pi / 2, 0, 0), parent=gf, segs=12)
    cyl(0.05, 0.035, (0, -0.03, 0.17), gold(), rot=(pi / 2, 0, 0), parent=gf, segs=10)
    box((1.4, 0.06, 0.12), (0, 0.01, -0.03), "egyptian_blue", parent=gf)
    flag(((sx0 + sx1) / 2, (sy0 + sy1) / 2 + 0.2, zd + 1.85), 0.6, player, r, w=0.5, fh=0.28, seed=5)
    skiff((-0.02, -0.15, 0.0), pi / 2 + 0.04, r, player, 1.3)
    # the landing stage: crates, jars, sacks, a crane
    z = zd
    crate((-1.15, 1.1, z), 0.2, r, 0.28)
    crate((-1.15, 1.1, z + 0.22), 0.6, r, 0.22, fruit=None)
    crate((-0.85, 1.18, z), -0.1, r, 0.26, fruit=5, seed=2)
    for i in range(4):
        amphora((-1.25 + i * 0.13, 0.72 + (i % 2) * 0.06, z), i * 0.6, parent=r, s=0.4,
                colour=((0.88, 0.6, 0.32), "terracotta")[i % 2], band=("red_ochre", "black")[i % 2])
    sack_pile((1.0, 1.1, z), r, seed=3, n=(3, 2), s=0.1)
    pithos((1.25, 0.7, z), r, 0.4)
    # a swinging crane on the right pier
    kx, ky = 1.2, 0.2
    box((0.18, 0.18, 0.08), (kx, ky, z + 0.04), "wood_dark", parent=r)
    rod((kx, ky, z), (kx, ky, z + 1.25), 0.035, "wood_dark", r)
    rod((kx, ky, z + 1.2), (kx - 0.55, ky - 0.4, z + 1.35), 0.025, "wood", r)
    rod((kx, ky, z + 0.7), (kx - 0.3, ky - 0.22, z + 1.24), 0.018, "wood", r)
    rod((kx - 0.55, ky - 0.4, z + 1.35), (kx - 0.55, ky - 0.4, z + 0.6), 0.004, "wool", r, 4)
    e = empty("load", (kx - 0.55, ky - 0.4, z + 0.42), 0.4, r)
    crate((0, 0, 0), 0, e, 0.22)
    cyl(0.08, 0.1, (kx, ky, z + 0.35), "wood", rot=(pi / 2, 0, 0), parent=r, segs=8)
    # nets on the left pier, fish baskets, bollards, rope, pennants on the pier heads
    net_rack((-1.03, -0.45, z), pi / 2, r, 0.75, 0.82)
    net_rack((-1.03, -1.05, z), pi / 2, r, 0.5, 0.7)
    for i, (x, y) in enumerate(((-1.25, 0.15), (-0.85, 0.2))):
        basket((x, y, z), r, 0.1, 0, i)
        for k in range(3):
            sphere(0.03, (x + 0.03 * cos(k * 2), y + 0.03 * sin(k * 2), z + 0.11), mat((0.6, 0.68, 0.74), rough=0.3),
                   scale=(2.2, 0.8, 0.6), parent=r, segs=6)
    for (x, y) in ((-0.68, -1.36), (0.64, -1.36), (-0.68, 0.0), (0.64, 0.0)):
        cyl(0.05, 0.2, (x, y, z + 0.1), "wood_dark", parent=r, segs=8)
        sphere(0.05, (x, y, z + 0.2), "wood_dark", parent=r, segs=8)
    for k in range(3):
        cyl(0.1 - k * 0.022, 0.03, (1.15, -0.6, z + 0.015 + k * 0.03), "wool", parent=r, segs=12)
    for (x, y) in ((-1.38, -1.38), (1.38, -1.38)):
        flag((x, y, z), 1.0, player, r, w=0.4, fh=0.2, pennant=True, seed=int(x * 3))
    crate((1.0, -1.15, z), 0.4, r, 0.24, fruit=3, seed=4)
    amphora((1.25, -0.95, z), 0.2, parent=r, s=0.4)
    brazier((0.95, -0.25, z), r, 0.7)
    # a ladder down to the water from the left pier head
    for t in (-0.1, 0.1):
        rod((-0.6, -1.0 + t, 0.0), (-0.6, -1.0 + t, z + 0.25), 0.012, "wood", r)
    for k in range(3):
        rod((-0.6, -1.1, 0.06 + k * 0.08), (-0.6, -0.9, 0.06 + k * 0.08), 0.008, "wood", r)
    return r


# ----------------------------------------------------------------- wonder


def _athena(loc, rz=0.0, parent=None, player=BLUE, s=1.0):
    """A colossal gold-and-ivory Athena (local -Y forward), 1.55 tall at
    s=1 to the spear tip: crested helmet, spear, a shield in player colour,
    a cloak in player colour behind."""
    e = face(loc, rz, parent)
    e.scale = (s, s, s)
    g = mat("gold", rough=0.26, metal=0.75)
    iv = mat((0.96, 0.9, 0.78), rough=0.45)
    pm = mat(player, rough=0.5)
    # the peplos: a fluted column of drapery, a belted waist, a bust
    lathe([(0.17, 0.0), (0.19, 0.04), (0.155, 0.2), (0.12, 0.42), (0.1, 0.52), (0.12, 0.6), (0.135, 0.68),
           (0.12, 0.76), (0.06, 0.8)], g, (0, 0, 0), parent=e, segs=16)
    for k in range(10):
        a = pi + (k - 4.5) * 0.17
        rod((0.17 * cos(a) * 0.98, 0.17 * sin(a) * 0.98, 0.02), (0.11 * cos(a), 0.11 * sin(a), 0.48), 0.012, g, e, 5)
    lathe([(0.105, 0.5), (0.122, 0.53), (0.105, 0.56)], g, (0, 0, 0), parent=e, segs=14)    # girdle
    # shoulders, the aegis over the chest with a gorgon boss
    box((0.34, 0.15, 0.08), (0, 0, 0.76), g, parent=e, bevel=0.035)
    sphere(0.12, (0, -0.03, 0.68), g, scale=(1.1, 0.75, 0.6), parent=e, segs=12)
    sphere(0.028, (0, -0.12, 0.69), iv, parent=e, segs=8)
    # cloak hanging behind from the shoulders
    cloth(0.32, 0.66, pm, (0, 0.085, 0.8), 0, e, hang=True, wave=0.02, nx=6)
    cloth(0.32, 0.66, pm, (0, 0.09, 0.8), pi, e, hang=True, wave=0.02, nx=6)
    # neck, head, helmet with a tall crest
    cyl(0.04, 0.08, (0, 0, 0.84), iv, parent=e, segs=10)
    sphere(0.085, (0, -0.01, 0.95), iv, scale=(0.9, 1, 1.1), parent=e, segs=12)
    sphere(0.09, (0, 0.01, 0.985), g, scale=(0.98, 1.05, 0.88), parent=e, segs=12)
    box((0.17, 0.035, 0.035), (0, -0.075, 1.025), g, parent=e)
    for sx in (-1, 1):
        box((0.03, 0.06, 0.09), (sx * 0.075, -0.04, 0.93), g, parent=e)
    box((0.03, 0.26, 0.13), (0, 0.03, 1.12), pm, parent=e, bevel=0.012)
    arc_rod((0, 0.03, 1.0), 0.115, 0.25, pi - 0.25, e, 0.018, g, "yz", 6)
    # arms: right raised with the spear, left on the shield
    rod((0.16, 0, 0.77), (0.25, -0.04, 0.6), 0.042, iv, e, 8, 0.036)
    rod((0.25, -0.04, 0.6), (0.27, -0.12, 0.74), 0.036, iv, e, 8, 0.03)
    rod((0.28, -0.13, 0.0), (0.28, -0.13, 1.42), 0.014, g, e, 6)
    rod((0.28, -0.13, 1.42), (0.28, -0.13, 1.56), 0.03, g, e, 6, 0.002)
    rod((-0.16, 0, 0.77), (-0.23, -0.02, 0.56), 0.042, iv, e, 8, 0.036)
    sh = empty("shield", (-0.3, -0.05, 0.36), 0, e)
    sh.rotation_euler = (0.1, 0, 0.35)
    cyl(0.26, 0.04, (0, 0, 0), g, rot=(pi / 2, 0, 0), parent=sh, segs=24)
    cyl(0.225, 0.045, (0, -0.003, 0), pm, rot=(pi / 2, 0, 0), parent=sh, segs=24)
    sphere(0.07, (0, -0.03, 0), g, scale=(1, 0.5, 1), parent=sh, segs=10)
    for k in range(8):
        a = k * pi / 4
        sphere(0.018, (0.16 * cos(a), -0.03, 0.16 * sin(a)), g, parent=sh, segs=6)
    # the serpent coiled by the shield
    for k in range(6):
        sphere(0.04 - k * 0.003, (-0.18 + 0.08 * cos(k * 1.2), 0.02 + 0.08 * sin(k * 1.2), 0.05 + k * 0.05),
               g, parent=e, segs=8)
    # Nike on the right hand
    sphere(0.025, (0.27, -0.16, 0.8), g, parent=e, segs=8)
    for sx in (-1, 1):
        box((0.08, 0.008, 0.05), (0.27 + sx * 0.04, -0.16, 0.84), g, rot=(0, sx * 0.5, 0), parent=e)
    return e


def wonder(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """5 x 5. The Greek wonder: a great marble temple on a stepped platform,
    its roof gilt-edged, and before it a colossal gold-and-ivory Athena on a
    high pedestal with spear, crested helm and a shield in player colour."""
    r = empty("wonder", loc, rot_z, parent)
    # stepped platform
    for i, w in enumerate((4.92, 4.72, 4.52)):
        box((w, w, 0.11), (0, 0, 0.055 + i * 0.11), courses("marble", 3.0, 0.82, 0.07) if i < 2 else noisy("marble", 0.05, 2.0),
            parent=r, bevel=0.012)
    z0 = 0.33
    box((1.3, 3.0, 0.012), (0, -0.85, z0 + 0.006), mat("red_ochre", rough=0.7), parent=r)
    box((1.4, 3.0, 0.008), (0, -0.85, z0 + 0.002), gold(), parent=r)
    # the temple: stylobate, peripteral Ionic colonnade, cella, roof
    ex, ey0, ey1 = 1.85, -0.25, 2.25
    ec = (ey0 + ey1) / 2
    for i, d in enumerate((0.0, 0.1)):
        box((ex * 2 + 0.36 - d * 2, ey1 - ey0 + 0.36 - d * 2, 0.08), (0, ec, z0 + 0.04 + i * 0.08),
            noisy("marble", 0.05, 2.5), parent=r, bevel=0.01)
    zs = z0 + 0.16
    cx_, cy0, cy1, ch = 1.15, 0.35, 1.85, 1.9
    wall_block(cx_ * 2, cy1 - cy0, ch, (0, (cy0 + cy1) / 2, zs), r, wall="plaster", dado="red_ochre", line="yellow_ochre",
               base=None, dado_h=0.4, base_h=0.0)
    box((cx_ * 2 + 0.02, cy1 - cy0 + 0.02, 0.09), (0, (cy0 + cy1) / 2, zs + ch - 0.3), "egyptian_blue", parent=r)
    door((0, cy0, zs), 0, r, w=0.7, h=1.35, metal=True, frame="egyptian_blue", lintel=gold())
    for sx in (-1, 1):
        box((0.3, 0.02, 0.55), (sx * 0.72, cy0 - 0.012, zs + 1.0), "yellow_ochre", parent=r)
        box((0.2, 0.024, 0.42), (sx * 0.72, cy0 - 0.014, zs + 1.0), "red_ochre", parent=r)
        sphere(0.06, (sx * 0.72, cy0 - 0.03, zs + 1.0), gold(), parent=r, segs=8, scale=(1, 0.4, 1))
    chh = 1.9
    xs = [-ex + 0.15 + i * (2 * ex - 0.3) / 7 for i in range(8)]
    ys = [ey0 + 0.15 + i * (ey1 - ey0 - 0.3) / 5 for i in range(6)]
    pts = [(x, ys[0]) for x in xs] + [(x, ys[-1]) for x in xs] + [(sx * xs[-1], y) for y in ys[1:-1] for sx in (-1, 1)]
    for x, y in pts:
        column((x, y, zs), chh, 0.13, r, band=gold(), band2="egyptian_blue", flutes=12)
        lathe([(0.15, 0), (0.17, 0.03), (0.15, 0.06)], "marble", (x, y, zs), parent=r, segs=12)
        for sx in (-1, 1):   # Ionic volutes
            cyl(0.045, 0.06, (x + sx * 0.12, y - 0.0, zs + chh - 0.07), gold(), rot=(pi / 2, 0, 0), parent=r, segs=10)
    z1 = zs + chh
    eL = ex * 2 + 0.1
    box((eL, ey1 - ey0 + 0.1, 0.18), (0, ec, z1 + 0.09), noisy("marble", 0.05, 3.0), parent=r)
    box((eL + 0.03, ey1 - ey0 + 0.13, 0.035), (0, ec, z1 + 0.18), gold(), parent=r)
    zf, fh = z1 + 0.2, 0.26
    box((eL - 0.02, ey1 - ey0 + 0.08, fh), (0, ec, zf + fh / 2), mat("egyptian_blue", rough=0.6), parent=r)
    # the frieze: a gold procession on blue, front and right side
    rnd = random.Random(5)
    for i in range(22):
        x = -eL / 2 + 0.12 + i * (eL - 0.24) / 21
        hh = rnd.uniform(0.13, 0.18)
        box((0.05, 0.02, hh), (x, ey0 - 0.06, zf + 0.04 + hh / 2), gold(), parent=r)
        sphere(0.025, (x, ey0 - 0.065, zf + 0.06 + hh), gold(), parent=r, segs=6)
    for i in range(14):
        y = ey0 + 0.1 + i * (ey1 - ey0 - 0.2) / 13
        hh = rnd.uniform(0.13, 0.18)
        box((0.02, 0.05, hh), (eL / 2 + 0.01, y, zf + 0.04 + hh / 2), gold(), parent=r)
        sphere(0.025, (eL / 2 + 0.015, y, zf + 0.06 + hh), gold(), parent=r, segs=6)
    z2 = zf + fh
    box((eL + 0.16, ey1 - ey0 + 0.26, 0.08), (0, ec, z2 + 0.04), "marble", parent=r, bevel=0.01)
    box((eL + 0.08, ey1 - ey0 + 0.18, 0.03), (0, ec, z2 - 0.005), "red_ochre", parent=r)
    z3 = z2 + 0.08
    rh = 0.78
    gable_roof(ey1 - ey0 + 0.1, eL + 0.1, rh, (0, ec, z3), r, axis="y", over=0.08, antefix=gold(), seed=141)
    _pediment(face((0, ey0 - 0.07, z3), 0, r), eL + 0.18, rh)
    _pediment(face((0, ey1 + 0.07, z3), pi, r), eL + 0.18, rh, figures=False)
    for y, rz in ((ey0 - 0.1, 0), (ey1 + 0.1, pi)):
        _acroterion((0, y, z3 + rh + 0.06), r, 1.8, rz)
        for sx in (-1, 1):
            _acroterion((sx * (eL / 2 + 0.04), y, z3 + 0.02), r, 1.2, rz)
    # the colossus on its pedestal, before the temple
    py = -1.25
    for i, (w, hh) in enumerate(((1.1, 0.12), (0.9, 0.7), (1.02, 0.1))):
        zz = z0 + sum(h_ for _, h_ in ((1.1, 0.12), (0.9, 0.7), (1.02, 0.1))[:i])
        box((w, w, hh), (0, py, zz + hh / 2), noisy("marble", 0.05, 4.0) if i != 1 else courses("marble", 6.0, 0.85, 0.05),
            parent=r, bevel=0.012)
    zt = z0 + 0.92
    _band(0.9, 0.9, z0 + 0.62, (0, py), r, "egyptian_blue", 0.12, 0.02)
    for k in range(5):
        box((0.1, 0.02, 0.07), (-0.32 + k * 0.16, py - 0.465, z0 + 0.62), gold(), parent=r)
        box((0.02, 0.1, 0.07), (0.465, py - 0.32 + k * 0.16, z0 + 0.62), gold(), parent=r)
    cloth(0.5, 0.36, mat(player, rough=0.7), (0, py - 0.47, z0 + 0.5), 0, r, hang=True, wave=0.006, nx=5)
    cyl(0.08, 0.012, (0, py - 0.49, z0 + 0.32), gold(), rot=(pi / 2, 0, 0), parent=r, segs=10)
    _athena((0, py, zt), 0.55, r, player, s=2.6)
    # braziers on columns, banners, offerings along the way
    for sx in (-1, 1):
        x = sx * 1.25
        column((x, -2.0, z0), 1.0, 0.09, r, band=gold(), band2="egyptian_blue", flutes=10)
        brazier((x, -2.0, z0 + 1.0), r, 1.2)
        for k, y in enumerate((-2.25, -0.55)):
            banner((sx * 2.18, y, z0), 2.0 if k == 0 else 1.7, player, r, 0.36, 0.78, seed=k + sx * 3)
        lathe([(0.06, 0), (0.03, 0.06), (0.03, 0.3), (0.14, 0.4), (0.16, 0.44), (0.13, 0.44)], gold(),
              (sx * 0.72, -1.7, z0), parent=r, segs=12)
        pot((sx * 1.85, -1.55, z0), r, 0.24, "leaf_dark", seed=sx + 9)
        pot((sx * 1.85, 0.1, z0), r, 0.22, "leaf", flowers="flower_red", seed=sx + 12)
        olive_tree((sx * 2.05, -1.05, z0), r, 1.0, seed=sx + 20)
        for k in range(3):
            amphora((sx * (0.75 + k * 0.14), -0.62 - (k % 2) * 0.08, z0), k * 0.8, parent=r, s=0.4,
                    colour=((0.88, 0.6, 0.32), "black", "terracotta")[k])
    flag((0, ec + 0.5, z3 + rh + 0.3), 1.2, player, r, w=1.0, fh=0.58, pole_r=0.035, seed=7)
    return r


# ------------------------------------------------------- walls and a gate
#
# The game draws a wall tile as its post (the set's idle frame) plus, for
# each wall of its owner beside it, an "arm" frame toward that neighbour,
# each anchored on the post's ground point (crates/view/src/walls.rs). Arm
# k reaches toward walls.rs's WALL_DIRECTIONS[k], a game tile offset; under
# lib's camera the game's +x is Blender +X and its +y is Blender -Y (iso.rs:
# screen x = (wx - wy) * 32). An arm stops at the tile's edge, or its corner
# for a diagonal, where the neighbour's arm meets it. A gate stands across
# the line its walls run along: line o is the direction WALL_DIRECTIONS[o].
#
#   stone_wall()                      the post alone (idle)
#   stone_wall(arm=k)                 arm k alone (frame k of "arm")
#   stone_wall(joins=mask)            a post with its arms, for pictures
# (docs/look/buildings.md, "Walls and gates")
#   gate(line=o, open=False|True)     "shut"/"open" frame o; idle = line 0 shut

WALL_DIRECTIONS = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)]


def wall_dir(k):
    """Direction k as a Blender unit vector, its angle, and the arm's reach
    (the tile's edge, or its corner on a diagonal)."""
    dx, dy = WALL_DIRECTIONS[k]
    x, y = float(dx), float(-dy)
    n = hypot(x, y)
    return (x / n, y / n), atan2(y, x), 0.5 * n


def plan_prism(pts, z0, z1, m, parent=None, name="prism"):
    """A vertical prism over a convex (x, y) outline, from z0 to z1."""
    n = len(pts)
    vs = [(x, y, z0) for x, y in pts] + [(x, y, z1) for x, y in pts]
    fs = [tuple(range(n)), tuple(range(n, 2 * n))] + [(i, (i + 1) % n, n + (i + 1) % n, n + i) for i in range(n)]
    return mesh(vs, fs, m, parent=parent, name=name)


def _arm_outline(k, pier, width, grow=0.0, trim=0.0):
    """The plan of arm k from a square pier (half-size `pier`) out to the
    tile's edge, `width` wide (+ grow each side); a diagonal arm's inner
    end follows the pier's corner so it neither gaps nor pokes through."""
    (ux, uy), _, reach = wall_dir(k)
    reach -= trim         # a trimmed part (a band wider than the arm) must not be
    h = width / 2 + grow  # coplanar with the arm's end, or it shades black
    dx, dy = WALL_DIRECTIONS[k]
    if dx and dy:
        c = pier * math.sqrt(2.0)
        sv = [(c - h, -h), (reach, -h), (reach, h), (c - h, h), (c, 0.0)]
    else:
        sv = [(pier - 0.01, -h), (reach, -h), (reach, h), (pier - 0.01, h)]
    return [(ux * s - uy * v, uy * s + ux * v) for s, v in sv]


def _along(k, s, v=0.0):
    (ux, uy), _, _ = wall_dir(k)
    return (ux * s - uy * v, uy * s + ux * v)


# The pier is barely wider than the wall and its top level with the
# merlons: a run must read as one wall, not a row of towers.
STONE_PIER, STONE_W, STONE_H, STONE_POST_H = 0.165, 0.3, 1.0, 1.06


def stone_wall(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, arm=None, joins=0, post=True):
    """1 x 1. A tile of dressed-stone curtain wall: a square pier with a
    player-colour band and crenellated cap; `joins` (a bit per
    WALL_DIRECTIONS index) adds an arm of battlemented wall toward each.
    `arm` = k draws arm k alone (the set's "arm" frame k), no post."""
    if arm is not None:
        joins, post = 1 << arm, False
    r = empty("stone_wall", loc, rot_z, parent)
    stone = courses("limestone", 7.0, 0.72, 0.16)
    P, W, H = STONE_PIER, STONE_W, STONE_H
    if post:
        box((P * 2 + 0.08, P * 2 + 0.08, 0.1), (0, 0, 0.05), courses("sandstone", 9.0), parent=r, bevel=0.012)
        box((P * 2, P * 2, STONE_POST_H), (0, 0, STONE_POST_H / 2), stone, parent=r, bevel=0.01)
        # the owner's band at the arms' red band, so it runs on along the wall
        box((P * 2 + 0.03, P * 2 + 0.03, 0.12), (0, 0, STONE_H - 0.235), mat(player, rough=0.7), parent=r)
        box((P * 2 + 0.04, P * 2 + 0.04, 0.018), (0, 0, STONE_H - 0.165), gold(), parent=r)
        box((P * 2 + 0.04, P * 2 + 0.04, 0.018), (0, 0, STONE_H - 0.305), gold(), parent=r)
        box((P * 2 + 0.05, P * 2 + 0.05, 0.05), (0, 0, STONE_POST_H + 0.025), "limestone", parent=r, bevel=0.008)
        for sx in (-1, 1):
            for sy in (-1, 1):
                box((0.085, 0.085, 0.13), (sx * (P - 0.035), sy * (P - 0.035), STONE_POST_H + 0.115), stone,
                    parent=r, bevel=0.006)
        box((0.05, 0.02, 0.15), (0, -P - 0.004, 0.75), mat((0.12, 0.08, 0.06)), parent=r)
        box((0.02, 0.05, 0.15), (P + 0.004, 0, 0.75), mat((0.12, 0.08, 0.06)), parent=r)
    for k in range(8):
        if not joins & (1 << k):
            continue
        _, _, reach = wall_dir(k)
        plan_prism(_arm_outline(k, P, W, 0.04, 0.003), 0.0, 0.08, courses("sandstone", 9.0), r, "arm_base")
        plan_prism(_arm_outline(k, P, W), 0.0, H, stone, r, "arm")
        plan_prism(_arm_outline(k, P, W, 0.02, 0.003), H - 0.27, H - 0.2, mat("red_ochre", rough=0.8), r, "arm_band")
        plan_prism(_arm_outline(k, P, W, 0.025, 0.003), H, H + 0.04, mat("limestone"), r, "arm_cap")
        dx, dy = WALL_DIRECTIONS[k]
        s0 = P * (math.sqrt(2) if dx and dy else 1.0) + 0.04
        n = max(1, int((reach - s0) / 0.17 + 0.5))
        step = (reach - s0) / n
        for i in range(n):
            s = s0 + (i + 0.5) * step
            for sv in (-1, 1):
                x, y = _along(k, s, sv * (W / 2 - 0.035))
                box((0.085, 0.07, 0.13), (x, y, H + 0.105), stone, rot=(0, 0, wall_dir(k)[1]), parent=r, bevel=0.006)
    return r


PAL_POST_R, PAL_LOG_R = 0.11, 0.055


def _stake(p, h, r_, parent, m, tip):
    cyl(r_, h, (p[0], p[1], h / 2), m, parent=parent, segs=7)
    cyl(r_, r_ * 3.2, (p[0], p[1], h + r_ * 1.6), tip, parent=parent, segs=7, r_top=0.004)


def palisade_wall(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, arm=None, joins=0, post=True):
    """1 x 1. A palisade of sharpened logs: a stout lashed post of three
    trunks with a player-colour binding; arms of stakes on two rails.
    `arm`, `joins`, `post` as for stone_wall."""
    if arm is not None:
        joins, post = 1 << arm, False
    r = empty("palisade_wall", loc, rot_z, parent)
    bark = [mat((0.44, 0.3, 0.18), rough=0.95), mat((0.5, 0.35, 0.21), rough=0.95), mat((0.38, 0.26, 0.16), rough=0.95)]
    tip = mat((0.82, 0.64, 0.42), rough=0.9)
    rnd = random.Random(11)
    if post:
        blob(0.2, (0, 0, 0.0), "earth", (1.1, 1.1, 0.35), r, 3, 0.2, 1)
        for i, a in enumerate((0.5, 0.5 + 2.1, 0.5 + 4.2)):
            _stake((0.075 * cos(a), 0.075 * sin(a)), 1.28 - i * 0.06, PAL_POST_R * 0.8, r, bark[i], tip)
        for zz in (0.42, 0.9):
            cyl(0.2, 0.08, (0, 0, zz), mat(player, rough=0.75), parent=r, segs=12)
            cyl(0.205, 0.015, (0, 0, zz - 0.045), "wool", parent=r, segs=12)
    for k in range(8):
        if not joins & (1 << k):
            continue
        _, ang, reach = wall_dir(k)
        dx, dy = WALL_DIRECTIONS[k]
        s0 = 0.17 if not (dx and dy) else 0.19
        n = max(2, int((reach - s0) / (PAL_LOG_R * 2.05) + 0.5))
        step = (reach - s0) / n
        for i in range(n):
            s = s0 + (i + 0.5) * step
            p = _along(k, s, rnd.uniform(-0.012, 0.012))
            _stake(p, rnd.uniform(0.98, 1.1), PAL_LOG_R * rnd.uniform(0.92, 1.08), r, bark[rnd.randrange(3)], tip)
        for zz, side in ((0.32, 1), (0.78, 1)):
            a, b = _along(k, s0 - 0.06, side * 0.07), _along(k, reach - 0.002, side * 0.07)
            rod((a[0], a[1], zz), (b[0], b[1], zz), 0.026, "wood_dark", r, 6)
        a, b = _along(k, s0 + 0.02, 0.1), _along(k, reach - 0.02, 0.1)
        rod((a[0], a[1], 0.05), (b[0], b[1], 0.72), 0.02, "wood_dark", r, 6)
        blob(0.1, (*_along(k, (s0 + reach) / 2, 0.0), -0.02), "earth", (reach * 3.2, 1.6, 0.6), r, k, 0.15, 1).rotation_euler = (0, 0, ang)
    return r


def gate(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, line=0, open=False):
    """1 x 1. A gatehouse set into a stone wall: two crenellated towers
    with player-colour banners, a timber walkway, bronze-studded doors.
    `line` (0-3) is the direction its walls run (WALL_DIRECTIONS[line]);
    `open` swings the doors back."""
    r = empty("gate", loc, rot_z, parent)
    (ux, uy), ang, reach = wall_dir(line)
    e = empty("gate_line", (0, 0, 0), ang, r)
    stone = courses("limestone", 7.0, 0.72, 0.16)
    T, D, TH = 0.15, 0.2, 1.62          # tower half-length (along), half-depth, height
    tx = reach - T                      # tower centre along the line
    for sx in (-1, 1):
        x = sx * tx
        box((T * 2 + 0.06, D * 2 + 0.06, 0.1), (x, 0, 0.05), courses("sandstone", 9.0), parent=e, bevel=0.012)
        box((T * 2, D * 2, TH), (x, 0, TH / 2), stone, parent=e, bevel=0.01)
        box((T * 2 + 0.06, D * 2 + 0.06, 0.06), (x, 0, TH + 0.03), "limestone", parent=e, bevel=0.01)
        for mx in (-1, 1):
            for my in (-1, 1):
                box((0.1, 0.1, 0.15), (x + mx * (T - 0.02), my * (D - 0.02), TH + 0.135), stone, parent=e, bevel=0.008)
                box((0.12, 0.12, 0.025), (x + mx * (T - 0.02), my * (D - 0.02), TH + 0.22), "red_ochre", parent=e)
        for sy in (-1, 1):
            f = face((x, sy * D, TH - 0.12), 0 if sy < 0 else pi, e)
            cloth(0.2, 0.5, mat(player, rough=0.7), (0, -0.02, 0), 0, f, hang=True, wave=0.01, nx=4)
            cyl(0.045, 0.012, (0, -0.04, -0.24), gold(), rot=(pi / 2, 0, 0), parent=f, segs=8)
            box((0.26, 0.04, 0.04), (0, -0.01, 0.01), "wood_dark", parent=f)
            box((0.05, 0.02, 0.14), (0, -0.005, -0.72), mat((0.12, 0.08, 0.06)), parent=f)
    # the opening: lintel, walkway with a parapet, the doors
    ow = tx - T                          # half-width of the opening
    zl = 1.08
    box((ow * 2 + 0.04, D * 2 - 0.04, 0.12), (0, 0, zl + 0.06), "wood_dark", parent=e)
    box((ow * 2 + 0.02, D * 2 + 0.02, 0.05), (0, 0, zl - 0.0), mat("egyptian_blue", rough=0.6), parent=e)
    box((ow * 2, D * 2 - 0.04, 0.14), (0, 0, zl + 0.19), stone, parent=e)
    for sy in (-1, 1):
        n = max(1, int(ow * 2 / 0.16 + 0.5))
        for i in range(n):
            xx = -ow + (i + 0.5) * ow * 2 / n
            box((0.075, 0.07, 0.12), (xx, sy * (D - 0.055), zl + 0.32), stone, parent=e, bevel=0.006)
    sh = face((0, -D - 0.01, zl + 0.18), 0, e)
    shield((0, -0.01, 0), 0, sh, 0.09, player, 0)
    sh2 = face((0, D + 0.01, zl + 0.18), pi, e)
    shield((0, -0.01, 0), 0, sh2, 0.09, player, 0)
    box((ow * 2, D * 2, 0.02), (0, 0, 0.01), courses("sandstone", 9.0), parent=e)
    lw = ow - 0.01
    dm = mat("cedar", rough=0.7)
    for sx in (-1, 1):
        f = empty("leaf", (sx * ow, 0.0, 0), 0, e)          # hinged at the tower face
        f.rotation_euler = (0, 0, (sx * 1.45) if open else 0.0)
        box((lw, 0.05, zl - 0.04), (-sx * lw / 2, 0, (zl - 0.04) / 2 + 0.01), dm, parent=f)
        for zz in (0.22, 0.55, 0.88):
            box((lw, 0.065, 0.04), (-sx * lw / 2, 0, zz), "wood_dark", parent=f)
            for k in range(3):
                for sy in (-1, 1):
                    sphere(0.014, (-sx * (0.04 + k * (lw - 0.08) / 2), sy * 0.035, zz), bronze(), parent=f, segs=6)
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


def _gate_line(t, at):
    """walls.rs gate_line: the line a gate at tile t stands across."""
    def has(k):
        dx, dy = WALL_DIRECTIONS[k]
        return (t[0] + dx, t[1] + dy) in at
    for o in (0, 2, 1, 3):
        if has(o) and has(o + 4):
            return o
    for o in (0, 2, 1, 3):
        if has(o) or has(o + 4):
            return o
    return 0


def _wall_links(t, at):
    """walls.rs wall_links: which neighbours the wall at tile t joins."""
    m = 0
    for k, (dx, dy) in enumerate(WALL_DIRECTIONS):
        p = at.get((t[0] + dx, t[1] + dy))
        if p is None:
            continue
        if dx and dy and ((t[0] + dx, t[1]) in at or (t[0], t[1] + dy) in at):
            continue
        if p == "gate" and _gate_line((t[0] + dx, t[1] + dy), at) != k % 4:
            continue
        m |= 1 << k
    return m


def wall_layout(at, kind="stone_wall", origin=(0, 0, 0), player=BLUE, parent=None, open_gates=False):
    """Walls and gates on game tiles {(x, y): "wall" | "gate"}, joined as the
    game joins them: each wall tile its post plus its arms, each gate turned
    along its line. Game (x, y) sits at model (x, -y) from `origin`."""
    fn = globals()[kind]
    for t, p in at.items():
        loc = (origin[0] + t[0], origin[1] - t[1], origin[2])
        if p == "gate":
            gate(loc, player=player, parent=parent, line=_gate_line(t, at), open=open_gates)
        else:
            fn(loc, player=player, parent=parent, joins=_wall_links(t, at))


def showcase_all():
    """Every Greek building set together on plain ground, two players."""
    r = empty("greek_all")

    def at(u, v):       # u: screen-right, v: away from the camera
        return (u - v, u + v, 0)

    box((80, 80, 0.02), (0, 0, -0.01), noisy((0.42, 0.64, 0.24), 0.16, 0.5, 1.0), parent=r)
    # back row: the big ones
    wonder(at(-7.0, 7.0), parent=r)
    temple(at(-2.4, 6.6), parent=r)
    town_center(at(1.4, 6.6), parent=r)
    dock(at(5.4, 6.6), parent=r)
    # middle row
    for u, fn in ((-9.0, academy), (-6.0, government_centre), (-3.0, market), (0.0, storehouse),
                  (3.0, siege_workshop), (6.0, stable)):
        fn(at(u, 2.6), parent=r)
    # front row, the red player's
    archery_range(at(-9.0, -0.6), player=RED, parent=r)
    barracks(at(-6.0, -0.6), player=RED, parent=r)
    house(at(-3.0, -0.6), player=RED, parent=r, variant=1)
    farm(at(0.0, -0.6), player=RED, parent=r)
    farm(at(1.0, -1.6), player=RED, parent=r)
    watch_tower(at(3.4, -0.4), player=RED, parent=r)
    house(at(6.0, -0.6), player=RED, parent=r, variant=0)
    # walls along the front: a stone wall with a gate, a palisade
    at_ = {(x, 0): "wall" for x in range(0, 7)}
    at_[(3, 0)] = "gate"
    at_.update({(6, y): "wall" for y in range(1, 4)})
    o = at(-8.5, -3.8)
    wall_layout(at_, "stone_wall", (round(o[0]), round(o[1]), 0), BLUE, r)
    at_ = {(i, -i): "wall" for i in range(0, 6)}
    at_.update({(5 + i, -5): "wall" for i in range(1, 4)})
    at_[(2, -2)] = "gate"
    o = at(1.5, -4.6)
    wall_layout(at_, "palisade_wall", (round(o[0]), round(o[1]), 0), RED, r, open_gates=True)
    for i, (u, v, sc) in enumerate(((-11.2, 4.8, 1.1), (8.2, 4.4, 1.2), (8.6, -2.0, 1.0), (-1.5, 4.6, 0.9),
                                    (-11.2, -2.6, 1.0), (4.6, 1.0, 0.9))):
        olive_tree(at(u, v), r, sc, i + 7)
    return r


def render_showcase_all(out, samples=32, res=(1800, 1125)):
    """/usr/bin/python3.13 -c 'import sys; sys.path.insert(0, "tools/look");
    import greek; greek.render_showcase_all("out.png")'"""
    lib.reset()
    showcase_all()
    c = (-1.0 - 1.9, -1.0 + 1.9, 0)
    lib.camera((c[0], c[1], 1.2), 22.5, res)
    lib.light()
    lib.render(out, samples)


# ------------------------------------------------- sets (tools/look/export_all.py)
# The colour sets this module draws (docs/look/buildings.md, "SETS"). Art
# agents: add a line per kind; export_all.py exports each with its stages.
SETS = {
    "house": dict(func="greek.house", cls="MediumBuilding", tiles=2, kwargs={"variant": 0}),
    "town_center": dict(func="greek.town_center", cls="LargeBuilding", tiles=3, kwargs={}),
    "temple": dict(func="greek.temple", cls="MediumBuilding", tiles=2, kwargs={}, fit=2, model_tiles=3),
    "barracks": dict(func="greek.barracks", cls="MediumBuilding", tiles=2, kwargs={}),
    "watch_tower": dict(func="greek.watch_tower", cls="SmallBuilding", tiles=1, kwargs={}),
    # phase 3: the rest of the Greek buildings, each modelled on its own footprint
    "academy": dict(func="greek.academy", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "archery_range": dict(func="greek.archery_range", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "government_centre": dict(func="greek.government_centre", cls="MediumBuilding", tiles=2, kwargs={},
                              fit=None),
    "market": dict(func="greek.market", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "siege_workshop": dict(func="greek.siege_workshop", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "stable": dict(func="greek.stable", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "storehouse": dict(func="greek.storehouse", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "farm": dict(func="greek.farm", cls="MediumBuilding", tiles=2, kwargs={}, fit=None, stages="flat"),
    "dock": dict(func="greek.dock", cls="LargeBuilding", tiles=3, kwargs={}, fit=None, stages="flat"),
    "wonder": dict(func="greek.wonder", cls="Wonder", tiles=5, kwargs={}, fit=None),
    "stone_wall": dict(func="greek.stone_wall", cls="SmallBuilding", tiles=1, kwargs={}, fit=None,
                       anims={"arm": [{"arm": k} for k in range(8)]}),
    "palisade_wall": dict(func="greek.palisade_wall", cls="SmallBuilding", tiles=1, kwargs={}, fit=None,
                          anims={"arm": [{"arm": k} for k in range(8)]}),
    "gate": dict(func="greek.gate", cls="SmallBuilding", tiles=1, kwargs={}, fit=None,
                 anims={"shut": [{"line": k, "open": False} for k in range(4)],
                        "open": [{"line": k, "open": True} for k in range(4)]}),
}

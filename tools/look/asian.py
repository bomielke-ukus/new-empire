"""East Asian buildings for the new look (Shang, Zhou, Han; Yamato; Choson):
timber-frame halls on rammed-earth and stone platforms, sweeping upturned
eaves under dark grey-green tile, vermilion lacquered posts and doors, pale
infill walls, bracket sets, ridge ornaments, paper lattice windows, bamboo,
lanterns, bronze cauldrons, drums and bells.

Conventions as greek.py: every builder is centred on the origin on its
footprint (tiles), takes `loc=(x, y, 0), rot_z=0.0, player=BLUE,
parent=None` and returns its root empty. The camera sees -Y (sunlit, screen
left) and +X (screen right), so fronts and detail face those sides. Player
colour (banners, streamers, door curtains, eave valances, shields, sails,
awnings) comes only from `player`.

    town_center()  3 x 3     house(variant)   2 x 2    barracks()      2 x 2
    temple()       2 x 2     watch_tower()    1 x 1    academy()       2 x 2
    archery_range() 2 x 2    dock()           3 x 3    farm(ripe)      2 x 2
    government_centre() 2 x 2  market()       2 x 2    siege_workshop() 2 x 2
    stable()       2 x 2     storehouse()     2 x 2    wonder()        5 x 5
    palisade_wall(arm, joins, post) 1 x 1     stone_wall(arm, joins, post) 1 x 1
    gate(line, open) 1 x 1
    showcase_all() / render_showcase_all(path)   every set together

SETS at the bottom declares the colour sets (docs/look/buildings.md).
"""
import math
import random
from math import atan2, cos, hypot, pi, sin

import bpy  # noqa: F401  (before bmesh)
import bmesh
from mathutils import Vector

import lib
from lib import BLUE, RED, blob, box, cyl, empty, mat, sphere
import greek as g
from greek import (WALL_DIRECTIONS, _arm_outline, _along, _gate_line, _wall_links, cloth, face,
                   noisy, plan_prism, rod, tint, wall_dir)

# ---------------------------------------------------------------- colours

# Lacquer is a dark oxblood, not cinnabar: at game size a bright vermilion
# hall reads as a red player's. The player's cloth is the brightest
# saturated thing on a building; the timber is plain brown.
VERMILION = (0.44, 0.15, 0.11)    # oxblood lacquer: doors, beams, rails
VERM_DK = (0.28, 0.10, 0.07)
TIMBER = (0.46, 0.32, 0.20)       # posts and frames, oiled wood
TILE = (0.33, 0.41, 0.39)          # grey-green glazed-less tile
TILE_DK = (0.22, 0.28, 0.28)
TEMPLE_TILE = (0.16, 0.38, 0.30)   # the round hall's green glaze
THATCH = (0.74, 0.60, 0.32)
RIDGE = (0.17, 0.20, 0.21)
SOFFIT = (0.30, 0.20, 0.14)
PLASTER = (0.95, 0.92, 0.84)
OCHRE = (0.88, 0.76, 0.54)
RAMMED = (0.78, 0.63, 0.43)
STONE = (0.70, 0.69, 0.65)
STONE_DK = (0.52, 0.52, 0.50)
TEAL = (0.12, 0.50, 0.44)          # painted beams and brackets (qing-lu)
JADE = (0.40, 0.66, 0.50)
PAPER = (0.97, 0.93, 0.80)
LATTICE = (0.38, 0.16, 0.10)
LACQUER = (0.10, 0.08, 0.08)
BAMBOO = (0.56, 0.68, 0.30)
BAMBOO_DRY = (0.80, 0.70, 0.40)
BRONZE_G = (0.36, 0.52, 0.42)      # old bronze, verdigris
PINE = (0.16, 0.38, 0.24)
WATER = (0.22, 0.48, 0.52)


def gold():
    return g.gold()


def bronze():
    return mat((0.66, 0.50, 0.26), rough=0.4, metal=0.6)


def verd():
    return mat(BRONZE_G, rough=0.55, metal=0.3)


def layered(colour=RAMMED, scale=9.0, amt=0.10):
    """Rammed earth: soft horizontal ramming courses (bands along Z)."""
    rgb = tuple(colour)

    def build(nt, bsdf):
        tc = nt.nodes.new("ShaderNodeTexCoord")
        wv = nt.nodes.new("ShaderNodeTexWave")
        wv.wave_type = "BANDS"
        wv.bands_direction = "Z"
        wv.inputs["Scale"].default_value = scale
        wv.inputs["Distortion"].default_value = 1.5
        wv.inputs["Detail"].default_value = 2.0
        n = nt.nodes.new("ShaderNodeTexNoise")
        n.inputs["Scale"].default_value = 4.0
        mix = nt.nodes.new("ShaderNodeMath")
        mix.operation = "MULTIPLY_ADD"
        mix.inputs[2].default_value = 0.0
        ramp = nt.nodes.new("ShaderNodeValToRGB")
        ramp.color_ramp.elements[0].position = 0.25
        ramp.color_ramp.elements[1].position = 0.8
        ramp.color_ramp.elements[0].color = (*lib._lin(tint(rgb, 1 - amt, (0, -0.01, -0.02))), 1)
        ramp.color_ramp.elements[1].color = (*lib._lin(tint(rgb, 1 + amt * 0.4)), 1)
        nt.links.new(tc.outputs["Object"], wv.inputs["Vector"])
        nt.links.new(tc.outputs["Object"], n.inputs["Vector"])
        nt.links.new(wv.outputs["Fac"], mix.inputs[0])
        mix.inputs[1].default_value = 0.7
        nt.links.new(n.outputs["Fac"], mix.inputs[2])
        nt.links.new(mix.outputs[0], ramp.inputs["Fac"])
        nt.links.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])
        bsdf.inputs["Roughness"].default_value = 0.95
    return g._node_mat(("asian_layered", rgb, scale, amt), build)


def stone(colour=STONE, scale=6.0):
    return g.courses(colour, scale, 0.78, 0.10)


# ------------------------------------------------------------------ roofs


def roof(parent, c, z, sx, sy, h, over=0.22, ridge=None, lift=0.16, drop=0.12, thick=0.06,
         tile=TILE, ornaments=True, finial=False, seed=0, ridge_h=0.07, horns=True):
    """A tiled hip roof with sweeping concave slopes and upturned corners.
    `c` (x, y) is its centre, `z` the wall top it sits on, (sx, sy) the
    wall plan it covers (ridge along X: sx >= sy), `h` the rise to the ridge,
    `over` the eave overhang, `ridge` the ridge's half-length (default sx/2 -
    sy/2, a true hip; 0 a pyramid; near sx/2 + over a hip-and-gable look),
    `lift` how far the corners curl up. Tile ribs run down the slopes."""
    cx, cy = c
    a, b = sx / 2 + over, sy / 2 + over
    rl = max(0.0, (sx - sy) / 2) if ridge is None else ridge
    rl = min(rl, a - 0.02)
    T = 8
    k = 0.32
    # the slope must clear the wall's top beam (which stands ~0.09 proud of
    # the wall line): put the eave so the roof passes just above it there
    tw = max(0.0, over - 0.09) / max(b, 1e-3)
    ze = min(z - drop, z + 0.02 - h * tw * (k + (1 - k) * tw)) if drop < 0 else z + 0.02 - h * tw * (k + (1 - k) * tw)

    def zt(t):
        return ze + h * (t * (k + (1 - k) * t))

    # perimeter samples: front (-Y), right (+X), back (+Y), left (-X)
    nlong = max(4, int(round(2 * a / 0.11)))
    nshort = max(4, int(round(2 * b / 0.11)))
    if nlong % 2:
        nlong += 1
    if nshort % 2:
        nshort += 1
    per = []          # (side, s) with s in [-1, 1) along each side
    for side, n in ((0, nlong), (1, nshort), (2, nlong), (3, nshort)):
        for i in range(n):
            per.append((side, -1 + 2 * i / n))

    def point(side, s, t):
        A = a + (rl - a) * t
        B = b * (1 - t)
        up = lift * (s ** 4) * (1 - t) ** 2
        out = 0.35 * lift * (s ** 4) * (1 - t) ** 2
        if side == 0:
            x, y = s * A, -B
            x += math.copysign(out, s) if s else 0
            y -= out
        elif side == 1:
            x, y = A, s * B
            y += math.copysign(out, s) if s else 0
            x += out
        elif side == 2:
            x, y = -s * A, B
            x -= math.copysign(out, s) if s else 0
            y += out
        else:
            x, y = -A, -s * B
            y -= math.copysign(out, s) if s else 0
            x -= out
        return x, y, zt(t) + up

    bm = bmesh.new()
    N = len(per)
    rings = []
    tmax = 0.985
    for j in range(T + 1):
        t = tmax * (j / T) ** 0.9
        ring = []
        for i, (side, s) in enumerate(per):
            x, y, zz = point(side, s, t)
            rib = 0.022 * (1 - 0.6 * t) if i % 2 else 0.0
            ring.append(bm.verts.new((cx + x, cy + y, zz + rib)))
        rings.append(ring)
    # the eave's fascia: a lip below the bottom ring
    lip = [bm.verts.new((v.co.x, v.co.y, v.co.z - thick)) for v in rings[0]]
    faces_tile, faces_dk, faces_lip = [], [], []
    for j in range(T):
        r0, r1 = rings[j], rings[j + 1]
        for i in range(N):
            i2 = (i + 1) % N
            f = bm.faces.new((r0[i], r0[i2], r1[i2], r1[i]))
            (faces_dk if (i // 2) % 2 else faces_tile).append(f)
    for i in range(N):
        i2 = (i + 1) % N
        faces_lip.append(bm.faces.new((lip[i], lip[i2], rings[0][i2], rings[0][i])))
    ctr = bm.verts.new((cx, cy, ze - thick + 0.02))
    for i in range(N):
        faces_lip.append(bm.faces.new((lip[(i + 1) % N], lip[i], ctr)))
    top = bm.verts.new((cx, cy, zt(1.0) + 0.01))
    for i in range(N):
        faces_tile.append(bm.faces.new((rings[-1][i], rings[-1][(i + 1) % N], top)))
    for f in faces_dk:
        f.material_index = 1
    for f in faces_lip:
        f.material_index = 2
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    ob = lib._obj(bm, "roof", mat(tile, rough=0.7), (0, 0, 0), (0, 0, 0), False, parent, 0.0)
    ob.data.materials.append(mat(tint(tile, 0.8), rough=0.7))
    ob.data.materials.append(mat(SOFFIT, rough=0.8))
    if not ornaments:
        return ob
    rm = mat(RIDGE, rough=0.6)
    zr = zt(1.0)
    # hip ridges: from the ridge ends down to each corner, riding up the curl
    for side, sgn in ((0, -1), (0, 1), (2, -1), (2, 1)):
        pts = [point(side, sgn * 1.0, t * tmax) for t in (0, 0.2, 0.45, 0.7, 1.0)]
        pts = [(cx + x, cy + y, zz + 0.03) for x, y, zz in pts]
        for p, q in zip(pts, pts[1:]):
            rod(p, q, 0.03, rm, parent, 6)
        if horns:      # an upswept horn at the corner
            x, y, zz = pts[0]
            d = Vector((x - cx, y - cy, 0)).normalized()
            tip = (x + d.x * 0.08, y + d.y * 0.08, zz + 0.1)
            rod((x, y, zz), tip, 0.03, rm, parent, 6, r_top=0.012)
    if rl > 0.05:
        box((2 * rl + 0.06, 0.09, ridge_h), (cx, cy, zr + ridge_h / 2 - 0.01), rm, parent=parent)
        box((2 * rl + 0.02, 0.1, 0.02), (cx, cy, zr + ridge_h - 0.01), mat(tint(RIDGE, 1.3)), parent=parent)
        for sg in (-1, 1):   # chiwei: the ridge ends curl up and in like fish tails
            x = cx + sg * (rl + 0.02)
            e = empty("chiwei", (x, cy, zr), 0, parent)
            box((0.09, 0.1, 0.2), (0, 0, 0.1), rm, rot=(0, -sg * 0.25, 0), parent=e)
            box((0.07, 0.1, 0.12), (-sg * 0.06, 0, 0.21), rm, rot=(0, -sg * 0.9, 0), parent=e)
            sphere(0.03, (-sg * 0.1, 0, 0.24), rm, parent=e, segs=6)
    if finial or rl <= 0.05:
        cyl(0.05, 0.06, (cx, cy, zr + 0.02), rm, parent=parent, segs=8)
        sphere(0.06, (cx, cy, zr + 0.1), gold(), parent=parent, segs=10)
        cyl(0.012, 0.18, (cx, cy, zr + 0.22), gold(), parent=parent, segs=6, r_top=0.003)
    return ob


# ------------------------------------------------------------- wall pieces


def platform(sx, sy, h, loc, parent, m=None, edge=STONE, steps=None, step_w=0.6):
    """A rammed-earth or stone platform, dressed-stone kerb at the top edge.
    `steps`: "y" for a stair down the -Y face, "x" down the +X face."""
    x, y, z = loc
    box((sx, sy, h), (x, y, z + h / 2), m or stone(), parent=parent, bevel=0.012)
    box((sx + 0.03, sy + 0.03, 0.045), (x, y, z + h - 0.02), mat(edge, rough=0.85), parent=parent, bevel=0.01)
    if steps:
        n = max(2, int(h / 0.08 + 0.5))
        for i in range(n):
            hh = h * (n - i) / (n + 1)
            if steps == "y":
                box((step_w, 0.1, hh), (x, y - sy / 2 - 0.05 - i * 0.1, z + hh / 2), mat(edge, rough=0.85),
                    parent=parent, bevel=0.006)
            else:
                box((0.1, step_w, hh), (x + sx / 2 + 0.05 + i * 0.1, y, z + hh / 2), mat(edge, rough=0.85),
                    parent=parent, bevel=0.006)


def lattice(loc, rz=0.0, parent=None, w=0.26, h=0.3, frame=VERMILION, n=(3, 3)):
    """A paper lattice window on a wall plane (loc its centre)."""
    e = face(loc, rz, parent)
    box((w, 0.03, h), (0, -0.006, 0), mat(PAPER, rough=0.9, emit=0.05), parent=e)
    fm = mat(frame, rough=0.6)
    box((w + 0.06, 0.05, 0.04), (0, -0.02, h / 2 + 0.02), fm, parent=e)
    box((w + 0.06, 0.05, 0.04), (0, -0.02, -h / 2 - 0.02), fm, parent=e)
    for sx in (-1, 1):
        box((0.035, 0.05, h + 0.06), (sx * (w / 2 + 0.012), -0.02, 0), fm, parent=e)
    lm = mat(LATTICE, rough=0.7)
    for i in range(1, n[0]):
        box((0.014, 0.034, h), (-w / 2 + i * w / n[0], -0.024, 0), lm, parent=e)
    for k in range(1, n[1]):
        box((w, 0.034, 0.014), (0, -0.026, -h / 2 + k * h / n[1]), lm, parent=e)
    return e


def door(loc, rz=0.0, parent=None, w=0.42, h=0.9, colour=VERMILION, studs=True, curtain=None):
    """A vermilion double door with gold studs and ring pulls; `curtain`
    (a colour) hangs a short cloth over its top instead of a plain lintel."""
    e = face(loc, rz, parent)
    box((w + 0.1, 0.05, h + 0.06), (0, -0.01, (h + 0.06) / 2), mat(LACQUER, rough=0.6), parent=e)
    dm = mat(colour, rough=0.45)
    for sx in (-1, 1):
        box((w / 2 - 0.01, 0.03, h), (sx * w / 4, -0.036, h / 2), dm, parent=e)
    if studs:
        gm = gold()
        for sx in (-1, 1):
            for zf in (0.3, 0.55, 0.8):
                box((w / 2 - 0.08, 0.012, 0.025), (sx * w / 4, -0.054, h * zf), gm, parent=e)
            cyl(0.028, 0.012, (sx * 0.05, -0.056, h * 0.45), gm, rot=(pi / 2, 0, 0), parent=e, segs=8)
    box((w + 0.18, 0.08, 0.07), (0, -0.02, h + 0.07), mat(VERM_DK, rough=0.6), parent=e)
    box((w + 0.16, 0.16, 0.04), (0, -0.07, 0.02), mat(STONE, rough=0.85), parent=e, bevel=0.008)
    if curtain is not None:
        cloth(w + 0.04, h * 0.32, mat(curtain, rough=0.75), (0, -0.075, h + 0.03), 0, e, hang=True,
              wave=0.01, nx=5)
        box((w + 0.02, 0.006, 0.02), (0, -0.092, h + 0.03 - h * 0.26), mat((0.95, 0.78, 0.30), rough=0.5),
            parent=e)
    return e


def hall(parent, x0, x1, y0, y1, z, h, wall=PLASTER, posts=None, bays=None, sill=0.22,
         base_m=None, brackets=True, beam=VERMILION, band=TEAL):
    """The timber frame of a hall: pale infill walls between vermilion posts
    standing proud of them, a sill, a painted beam under the eaves and
    bracket sets on the posts. Returns the posts' (x, y) list."""
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    sx, sy = x1 - x0, y1 - y0
    box((sx, sy, h), (cx, cy, z + h / 2), noisy(wall, 0.07, 5.0), parent=parent, name="wall")
    # the sill as four skirts on the wall's faces, not a block inside it: a
    # construction stage's cut would cap both at one height (black)
    sm = base_m or mat(tint(OCHRE, 0.82), rough=0.9)
    for yy in (y0 - 0.008, y1 + 0.008):
        box((sx + 0.03, 0.014, sill), (cx, yy, z + sill / 2), sm, parent=parent)
    for xx in (x0 - 0.008, x1 + 0.008):
        box((0.014, sy + 0.002, sill), (xx, cy, z + sill / 2), sm, parent=parent)
    nx = bays or max(1, int(round(sx / 0.55)))
    ny = max(1, int(round(sy / 0.55)))
    pts = []
    for i in range(nx + 1):
        for yy in (y0, y1):
            pts.append((x0 + i * sx / nx, yy))
    for k in range(1, ny):
        for xx in (x0, x1):
            pts.append((xx, y0 + k * sy / ny))
    pm = mat(TIMBER, rough=0.6)
    for (px, py) in pts:
        cyl(0.055, h, (px, py, z + h / 2), pm, parent=parent, segs=10)
        cyl(0.075, 0.05, (px, py, z + 0.025), mat(STONE, rough=0.8), parent=parent, segs=10)
    bm_ = mat(beam, rough=0.55)
    # (its top a hair under the wall's: coplanar faces render black once a
    # construction stage cuts the roof away)
    box((sx + 0.13, sy + 0.13, 0.08), (cx, cy, z + h - 0.046), bm_, parent=parent)
    box((sx + 0.14, sy + 0.14, 0.035), (cx, cy, z + h - 0.11), mat(band, rough=0.6), parent=parent)
    if brackets:
        tm, gm = mat(band, rough=0.6), mat(tint(OCHRE, 0.9), rough=0.7)
        for (px, py) in pts:
            # a block, a bracket arm outward, a top block: the dougong in miniature
            box((0.1, 0.1, 0.06), (px, py, z + h + 0.033), gm, parent=parent)
            ox = 0.06 if px >= x1 - 1e-6 else (-0.06 if px <= x0 + 1e-6 else 0)
            oy = 0.06 if py >= y1 - 1e-6 else (-0.06 if py <= y0 + 1e-6 else 0)
            box((0.1 + abs(ox) * 2.6, 0.1 + abs(oy) * 2.6, 0.05), (px + ox, py + oy, z + h + 0.085), tm,
                parent=parent)
            box((0.08, 0.08, 0.05), (px + ox * 1.6, py + oy * 1.6, z + h + 0.13), gm, parent=parent)
        box((sx + 0.14, sy + 0.14, 0.05), (cx, cy, z + h + 0.155), mat(VERM_DK, rough=0.6), parent=parent)
    return pts


def building(parent, x0, x1, y0, y1, z, h, rh, over=0.22, ridge=None, lift=0.16, wall=PLASTER,
             brackets=True, bays=None, tile=TILE, seed=0, finial=False):
    """A hall and its roof; returns the roof's eave z."""
    hall(parent, x0, x1, y0, y1, z, h, wall=wall, brackets=brackets, bays=bays)
    zt = z + h + (0.18 if brackets else 0.0)
    sx, sy = x1 - x0, y1 - y0
    if sx >= sy:
        roof(parent, ((x0 + x1) / 2, (y0 + y1) / 2), zt, sx, sy, rh, over, ridge, lift, tile=tile, seed=seed,
             finial=finial)
    else:
        e = empty("rot", ((x0 + x1) / 2, (y0 + y1) / 2, 0), pi / 2, parent)
        roof(e, (0, 0), zt, sy, sx, rh, over, ridge, lift, tile=tile, seed=seed, finial=finial)
    return zt


def tiled_coping(parent, p0, p1, z, w, h=0.12, over=0.05, tile=TILE_DK):
    """A little gabled tile coping along a wall top from p0 to p1 (x, y)."""
    a, b = Vector((p0[0], p0[1], 0)), Vector((p1[0], p1[1], 0))
    d = b - a
    L = d.length
    e = empty("coping", ((a.x + b.x) / 2, (a.y + b.y) / 2, z), atan2(d.y, d.x), parent)
    lib.prism((L, w + 2 * over, h), (0, 0, 0), mat(tile, rough=0.7), parent=e)
    box((L, 0.05, 0.04), (0, 0, h), mat(RIDGE, rough=0.6), parent=e)
    box((L, w + 2 * over + 0.004, 0.025), (0, 0, -0.012), mat(tint(tile, 0.75)), parent=e)
    return e


def yard_wall(parent, p0, p1, h=0.45, t=0.1, m=None, tile=TILE_DK):
    """A rammed-earth or plastered yard wall with a tiled coping."""
    (x0, y0), (x1, y1) = p0, p1
    c = ((x0 + x1) / 2, (y0 + y1) / 2)
    if abs(x1 - x0) >= abs(y1 - y0):
        sx, sy = abs(x1 - x0) + t, t
        box((sx, sy, h), (c[0], c[1], h / 2), m or layered(), parent=parent, bevel=0.008)
        box((sx + 0.03, sy + 0.03, 0.07), (c[0], c[1], 0.035), stone(STONE_DK, 10.0), parent=parent)
        tiled_coping(parent, (min(x0, x1) - t / 2 - 0.02, y0), (max(x0, x1) + t / 2 + 0.02, y1), h, t, 0.1)
    else:
        # a wall along Y butts into the X walls at its ends, a hair thicker
        # and lower so no face of it lies on theirs
        tt = t + 0.014
        sy = abs(y1 - y0)
        box((tt, sy, h - 0.006), (c[0], c[1], (h - 0.006) / 2), m or layered(), parent=parent, bevel=0.008)
        box((tt + 0.044, sy, 0.064), (c[0], c[1], 0.032), stone(STONE_DK, 10.0), parent=parent)
        tiled_coping(parent, (x0, min(y0, y1)), (x1, max(y0, y1)), h - 0.012, tt, 0.1)


# -------------------------------------------------------------- player cloth


def streamer(loc, h=1.8, player=BLUE, parent=None, w=0.24, bh=0.78, rz=0.0, seed=0):
    """A tall pole with a long narrow banner hung from a crossbar, a black
    toothed border and a gilt finial: the East Asian standard."""
    e = empty("streamer", loc, rz, parent)
    cyl(0.022, h, (0, 0, h / 2), mat(LACQUER, rough=0.5), parent=e, segs=8)
    cyl(0.03, 0.12, (0, 0, h + 0.05), gold(), parent=e, segs=8, r_top=0.004)
    zt = h - 0.08
    rod((-w / 2 - 0.05, -0.03, zt), (w / 2 + 0.05, -0.03, zt), 0.016, mat(LACQUER), e)
    for sx in (-1, 1):
        sphere(0.025, (sx * (w / 2 + 0.06), -0.03, zt), gold(), parent=e, segs=6)
    pm = mat(player, rough=0.7)
    cloth(w, bh, pm, (0, -0.036, zt), 0, e, hang=True, wave=0.02, seed=seed, nx=5)
    # teeth along the bottom (player colour) and a dark border stripe down each side
    for i in range(3):
        x = -w / 2 + (i + 0.5) * w / 3
        lib.mesh([(x - w / 6, 0, 0), (x + w / 6, 0, 0), (x, 0, -0.1)], [(0, 1, 2)], pm,
               loc=(0, -0.036 + 0.02 * sin(1.0 + seed * 1.7 + x * 5) * 0, zt - bh), parent=e)
    for sx in (-1, 1):
        cloth(0.035, bh, mat(LACQUER, rough=0.7), (sx * (w / 2 - 0.017), -0.05, zt), 0, e, hang=True,
              wave=0.012, seed=seed, nx=2)
    sphere(0.04, (0, -0.06, zt - bh * 0.35), gold(), scale=(1, 0.3, 1), parent=e, segs=8)
    return e


def flag(loc, h=1.4, player=BLUE, parent=None, w=0.5, fh=0.32, seed=0, pennant=False):
    """A pole with a flying flag (or a swallow-tailed pennant) in player colour."""
    x, y, z = loc
    cyl(0.022, h, (x, y, z + h / 2), mat(LACQUER, rough=0.5), parent=parent, segs=8)
    cyl(0.03, 0.1, (x, y, z + h + 0.04), gold(), parent=parent, segs=8, r_top=0.004)
    top = (x, y, z + h - 0.02)
    pm = mat(player, rough=0.7)
    if pennant:
        cloth(w, fh, pm, top, pi / 4, parent, taper=0.85, seed=seed, wave=0.04)
    else:
        cloth(w, fh, pm, top, pi / 4, parent, seed=seed, wave=0.05)
        cloth(w * 0.98, 0.05, mat(LACQUER, rough=0.7), (x, y, z + h - 0.02 - fh * 0.82), pi / 4, parent,
              seed=seed, wave=0.05)
    return top


def valance(parent, p0, p1, z, player, drop=0.16, rz=0.0):
    """A player-colour cloth hung along an eave beam (local -Y outward)."""
    a, b = Vector(p0), Vector(p1)
    d = b - a
    e = face(((a.x + b.x) / 2, (a.y + b.y) / 2, z), rz, parent)
    L = d.length
    n = max(2, int(L / 0.42))
    pm = mat(player, rough=0.7)
    gm = mat((0.95, 0.78, 0.30), rough=0.5)
    em = mat(PAPER, rough=0.7)
    for i in range(n):
        x = -L / 2 + (i + 0.5) * L / n
        cloth(L / n - 0.02, drop, pm, (x, -0.01, 0), 0, e, hang=True, wave=0.012, nx=4, seed=i)
        lib.mesh([(x - L / n / 2 + 0.01, 0, -drop), (x + L / n / 2 - 0.01, 0, -drop), (x, 0, -drop - 0.07)],
               [(0, 1, 2)], pm, loc=(0, -0.012, 0), parent=e)
        # a pattern, not a flat sheet: a gold hem and, on alternate
        # panels, a pale roundel
        box((L / n - 0.03, 0.006, 0.022), (x, -0.032, -drop * 0.78), gm, parent=e)
        if i % 2 == 0 and drop >= 0.1:
            cyl(min(0.045, drop * 0.26), 0.006, (x, -0.034, -drop * 0.38), em, rot=(pi / 2, 0, 0), parent=e,
                segs=12)
    return e


def shield(loc, rz=0.0, parent=None, player=BLUE, s=1.0):
    """A tall rectangular lacquered shield, player-colour face, a gold
    taotie-ish mask motif."""
    e = face(loc, rz, parent)
    box((0.22 * s, 0.03, 0.38 * s), (0, 0, 0), mat(LACQUER, rough=0.5), parent=e, bevel=0.01)
    box((0.18 * s, 0.03, 0.34 * s), (0, -0.012, 0), mat(player, rough=0.55), parent=e)
    gm = gold()
    for sx in (-1, 1):
        sphere(0.025 * s, (sx * 0.045 * s, -0.03, 0.04 * s), gm, scale=(1, 0.4, 1), parent=e, segs=6)
    box((0.1 * s, 0.012, 0.025 * s), (0, -0.03, -0.05 * s), gm, parent=e)
    return e


def lantern(loc, parent=None, s=1.0, cord=0.12):
    """A warm paper lantern hanging on a cord, black caps, gold tassel
    (amber, not red: red is a player's colour)."""
    x, y, z = loc
    if cord:
        rod((x, y, z + cord), (x, y, z), 0.004, mat(LACQUER), parent, 4)
    lm = mat((0.98, 0.78, 0.42), rough=0.6, emit=0.5)
    sphere(0.07 * s, (x, y, z - 0.08 * s), lm, scale=(1, 1, 1.15), parent=parent, segs=10)
    for dz in (0.0, -0.165):
        cyl(0.035 * s, 0.025 * s, (x, y, z + dz * s), mat(LACQUER), parent=parent, segs=8)
    cyl(0.01 * s, 0.07 * s, (x, y, z - 0.21 * s), gold(), parent=parent, segs=5)


# ------------------------------------------------------------------ props


def bamboo(loc, parent=None, n=7, h=1.5, seed=0, spread=0.16):
    """A clump of bamboo: jointed green canes, feathery leaf sprays."""
    x, y, z = loc
    rnd = random.Random(seed)
    cm = [mat(BAMBOO, rough=0.5), mat(tint(BAMBOO, 0.85), rough=0.5), mat((0.68, 0.72, 0.34), rough=0.5)]
    jm = mat(tint(BAMBOO, 0.6))
    pts = []
    for i in range(n):
        a = rnd.uniform(0, 2 * pi)
        d = rnd.uniform(0, spread)
        bx, by = x + d * cos(a), y + d * sin(a)
        hh = h * rnd.uniform(0.7, 1.05)
        lean = (rnd.uniform(-0.12, 0.12), rnd.uniform(-0.12, 0.12))
        top = (bx + lean[0], by + lean[1], z + hh)
        rod((bx, by, z), top, 0.018, cm[i % 3], parent, 6, r_top=0.012)
        for k in range(1, 5):
            f = k / 5
            sphere(0.02, (bx + lean[0] * f, by + lean[1] * f, z + hh * f), jm, scale=(1, 1, 0.4),
                   parent=parent, segs=6)
        pts.append(top)
        pts.append((bx + lean[0] * 0.75 + rnd.uniform(-0.08, 0.08), by + lean[1] * 0.75, z + hh * 0.78))
    lm = [mat((0.36, 0.58, 0.22), rough=0.8), mat((0.26, 0.48, 0.18), rough=0.8), mat((0.46, 0.64, 0.26), rough=0.8)]
    for k in range(3):
        g.clumps([(p, (1.3, 1.3, 0.55)) for j, p in enumerate(pts) if j % 3 == k], 0.11, lm[k], parent,
                 seed=seed + k, rough=0.35)


def pine(loc, parent=None, s=1.0, seed=0):
    """A painted-scroll pine: a leaning trunk, flat layered tufts."""
    x, y, z = loc
    rnd = random.Random(seed)
    bark = mat((0.40, 0.28, 0.20), rough=0.9)
    p0 = (x, y, z)
    p1 = (x + 0.12 * s, y + 0.02 * s, z + 0.6 * s)
    p2 = (x - 0.05 * s, y, z + 1.15 * s)
    rod(p0, p1, 0.06 * s, bark, parent, 7, r_top=0.045 * s)
    rod(p1, p2, 0.045 * s, bark, parent, 7, r_top=0.025 * s)
    tufts = [(p2, 0.32), ((p1[0] + 0.25 * s, p1[1] - 0.05 * s, p1[2] + 0.15 * s), 0.26),
             ((p1[0] - 0.22 * s, p1[1] + 0.05 * s, p1[2] + 0.3 * s), 0.24),
             ((p2[0] + 0.18 * s, p2[1] - 0.04 * s, p2[2] - 0.2 * s), 0.22)]
    for i, (c, r) in enumerate(tufts):
        rod(p1 if i else p2, c, 0.02 * s, bark, parent, 5)
        blob(r * s, c, mat((PINE, tint(PINE, 1.25), tint(PINE, 0.9))[i % 3], rough=0.85),
             (1.25, 1.1, 0.42), parent, seed + i, 0.25, 2)


def ding(loc, parent=None, s=1.0):
    """A Shang bronze ritual cauldron: a deep bowl on three legs, two handles."""
    x, y, z = loc
    vm = verd()
    for k in range(3):
        a = k * 2 * pi / 3 + 0.4
        cyl(0.035 * s, 0.22 * s, (x + 0.13 * s * cos(a), y + 0.13 * s * sin(a), z + 0.11 * s), vm, parent=parent,
            segs=6)
    g.lathe([(0.1, 0.0), (0.2, 0.06), (0.22, 0.2), (0.23, 0.26), (0.2, 0.26), (0.19, 0.12)], vm,
            (x, y, z + 0.18 * s), parent=parent, s=s, segs=14)
    g.lathe([(0.222, 0.12), (0.232, 0.16), (0.222, 0.2)], bronze(), (x, y, z + 0.18 * s), parent=parent, s=s,
            segs=14)
    for sx in (-1, 1):
        box((0.03 * s, 0.1 * s, 0.12 * s), (x + sx * 0.18 * s, y, z + 0.5 * s), vm, parent=parent)
        box((0.03 * s, 0.03 * s, 0.03 * s), (x + sx * 0.18 * s, y, z + 0.57 * s), vm, parent=parent)
    cyl(0.17 * s, 0.01, (x, y, z + 0.4 * s), mat((0.25, 0.2, 0.18)), parent=parent, segs=12)


def incense(loc, parent=None, s=1.0):
    """A ding with smoking incense sticks."""
    ding(loc, parent, s)
    x, y, z = loc
    for k in range(3):
        rod((x - 0.04 * s + k * 0.04 * s, y, z + 0.4 * s), (x - 0.05 * s + k * 0.05 * s, y, z + 0.6 * s), 0.006,
            mat((0.75, 0.25, 0.12)), parent, 4)
        sphere(0.008, (x - 0.05 * s + k * 0.05 * s, y, z + 0.6 * s), mat((1, 0.6, 0.2), emit=4), parent=parent,
               segs=4)
    blob(0.06 * s, (x, y, z + 0.78 * s), mat((0.92, 0.92, 0.92), alpha=0.5, rough=1.0), (1, 1, 1.6), parent, 3,
         0.3, 1)


def drum(loc, parent=None, s=1.0, rz=0.0, player=BLUE):
    """A big barrel drum on a vermilion stand, its heads facing local X."""
    e = empty("drum", loc, rz, parent)
    e.scale = (s, s, s)
    pm = mat(VERMILION, rough=0.45)
    for sx in (-1, 1):
        for sy in (-1, 1):
            rod((sx * 0.2, sy * 0.16, 0), (sx * 0.12, sy * 0.1, 0.42), 0.02, pm, e, 6)
    box((0.3, 0.26, 0.04), (0, 0, 0.42), pm, parent=e)
    g.lathe([(0.2, -0.17), (0.25, -0.08), (0.26, 0.0), (0.25, 0.08), (0.2, 0.17)], mat(VERMILION, rough=0.4),
            (0, 0, 0.7), rot=(0, pi / 2, 0), parent=e, segs=16)
    for sx in (-1, 1):
        cyl(0.2, 0.01, (sx * 0.172, 0, 0.7), mat((0.92, 0.84, 0.66), rough=0.7), rot=(0, pi / 2, 0), parent=e,
            segs=16)
        cyl(0.07, 0.012, (sx * 0.176, 0, 0.7), mat(player, rough=0.6), rot=(0, pi / 2, 0), parent=e, segs=12)
    for k in range(10):
        a = k * 2 * pi / 10
        for sx in (-1, 1):
            sphere(0.012, (sx * 0.16, 0.24 * cos(a), 0.7 + 0.24 * sin(a)), gold(), parent=e, segs=4)
    cyl(0.255, 0.04, (0, 0, 0.7), mat(LACQUER), rot=(0, pi / 2, 0), parent=e, segs=16)
    rod((0.25, -0.12, 0.38), (0.35, -0.2, 0.62), 0.012, mat(LACQUER), e, 5)
    return e


def bell(loc, parent=None, s=1.0, rz=0.0):
    """A bronze bell hanging in a small vermilion frame with a tiled cap."""
    e = empty("bell", loc, rz, parent)
    e.scale = (s, s, s)
    pm = mat(VERMILION, rough=0.45)
    for sx in (-1, 1):
        cyl(0.035, 0.95, (sx * 0.28, 0, 0.475), pm, parent=e, segs=8)
    box((0.7, 0.08, 0.07), (0, 0, 0.9), pm, parent=e)
    g.lathe([(0.03, 0), (0.13, 0.02), (0.15, 0.08), (0.17, 0.3), (0.16, 0.34), (0.06, 0.38), (0.02, 0.42)], verd(),
            (0, 0, 0.42), parent=e, segs=14)
    for k in range(3):
        for sx in (-1, 1):
            sphere(0.018, (sx * 0.08, -0.15 + 0.004 * k, 0.56 + k * 0.07), bronze(), parent=e, segs=5)
    cyl(0.012, 0.06, (0, 0, 0.86), mat(LACQUER), parent=e, segs=5)
    lib.prism((0.76, 0.3, 0.14), (0, 0, 0.94), mat(TILE_DK, rough=0.7), parent=e)
    box((0.76, 0.05, 0.04), (0, 0, 1.08), mat(RIDGE), parent=e)
    return e


def jar(loc, parent=None, s=1.0, colour=(0.42, 0.30, 0.22), glaze=None):
    """A stoneware storage jar with a lid."""
    x, y, z = loc
    g.lathe([(0.08, 0), (0.15, 0.06), (0.19, 0.2), (0.17, 0.33), (0.1, 0.4), (0.1, 0.43)], mat(colour, rough=0.4),
            loc, parent=parent, s=s, segs=12)
    if glaze:
        g.lathe([(0.173, 0.28), (0.182, 0.3), (0.165, 0.34)], mat(glaze, rough=0.3), loc, parent=parent, s=s, segs=12)
    cyl(0.11 * s, 0.03 * s, (x, y, z + 0.445 * s), mat(tint(colour, 0.7)), parent=parent, segs=10)


def silk_bolts(loc, parent=None, n=4, seed=0, rz=0.0):
    e = empty("silk", loc, rz, parent)
    cols = [(0.86, 0.28, 0.20), (0.94, 0.78, 0.30), (0.28, 0.58, 0.48), (0.95, 0.92, 0.84), (0.55, 0.24, 0.50)]
    for i in range(n):
        cyl(0.05, 0.28, ((i - (n - 1) / 2) * 0.105, 0, 0.05), mat(cols[(i + seed) % 5], rough=0.5),
            rot=(pi / 2, 0, 0), parent=e, segs=10)
    if n > 2:
        for i in range(n - 1):
            cyl(0.05, 0.28, ((i - (n - 2) / 2) * 0.105, 0, 0.14), mat(cols[(i + seed + 2) % 5], rough=0.5),
                rot=(pi / 2, 0, 0), parent=e, segs=10)
    return e


def slips(loc, parent=None, rz=0.0, s=1.0):
    """A rolled bundle of bamboo writing slips, tied."""
    e = empty("slips", loc, rz, parent)
    cyl(0.045 * s, 0.22 * s, (0, 0, 0.045 * s), mat(BAMBOO_DRY, rough=0.7), rot=(pi / 2, 0, 0), parent=e, segs=10)
    for dy in (-0.06, 0.06):
        cyl(0.047 * s, 0.015 * s, (0, dy * s, 0.045 * s), mat(VERM_DK), rot=(pi / 2, 0, 0), parent=e, segs=10)
    return e


def straw_hat(loc, parent=None, s=1.0):
    x, y, z = loc
    cyl(0.16 * s, 0.09 * s, (x, y, z + 0.045 * s), mat(BAMBOO_DRY, rough=0.9), parent=parent, segs=12, r_top=0.01)


def person(loc, rz=0.0, parent=None, robe=(0.35, 0.30, 0.45), sit=False, hat=True, s=1.0):
    """A simple robed figure, 1.0 tall standing (0.6 kneeling), facing local -Y."""
    e = empty("person", loc, rz, parent)
    e.scale = (s, s, s)
    rm = mat(robe, rough=0.8)
    skin = mat("skin_tan", rough=0.6)
    if sit:
        box((0.3, 0.32, 0.16), (0, 0.02, 0.08), rm, parent=e, bevel=0.04)
        cyl(0.12, 0.34, (0, 0.04, 0.32), rm, parent=e, segs=10, r_top=0.08)
        zh = 0.56
    else:
        cyl(0.15, 0.62, (0, 0, 0.31), rm, parent=e, segs=10, r_top=0.09)
        zh = 0.74
        cyl(0.11, 0.06, (0, 0, 0.6), mat(tint(robe, 0.6)), parent=e, segs=10)
    for sx in (-1, 1):
        rod((sx * 0.1, 0, zh - 0.04), (sx * 0.05, -0.14, zh - 0.22), 0.04, rm, e, 6)
    sphere(0.025, (0, -0.16, zh - 0.22), skin, parent=e, segs=6)
    sphere(0.09, (0, 0, zh + 0.1), skin, parent=e, segs=10)
    sphere(0.085, (0, 0.012, zh + 0.13), mat("hair_black"), scale=(1, 1, 0.8), parent=e, segs=10)
    if hat:
        box((0.06, 0.1, 0.1), (0, 0.0, zh + 0.23), mat(LACQUER), parent=e)
    return e


def fence(points, parent=None, h=0.42, m=None, post=None):
    """A bamboo fence along a polyline: lashed posts, two rails."""
    g.fence(points, parent, h, m or mat(BAMBOO_DRY, rough=0.6), post or mat(tint(BAMBOO_DRY, 0.7)), 2)


def pad(parent, side, colour=(0.80, 0.72, 0.52)):
    box((side - 0.08, side - 0.08, 0.035), (0, 0, 0.012), noisy(colour, 0.1, 3.0, 1.0), parent=parent, bevel=0.012)


def stone_lion(loc, rz=0.0, parent=None, s=1.0):
    """A seated stone guardian lion on a plinth, facing local -Y."""
    e = face(loc, rz, parent)
    e.scale = (s, s, s)
    sm = mat(STONE, rough=0.8)
    box((0.26, 0.34, 0.12), (0, 0, 0.06), stone(STONE_DK, 12.0), parent=e, bevel=0.01)
    sphere(0.11, (0, 0.06, 0.24), sm, scale=(1, 1.2, 1.1), parent=e, segs=10)
    sphere(0.1, (0, -0.04, 0.38), sm, scale=(1.05, 0.9, 1), parent=e, segs=10)
    blob(0.1, (0, -0.02, 0.41), mat(tint(STONE, 0.85), rough=0.9), (1.25, 0.8, 1.1), e, 2, 0.3, 1)
    for sx in (-1, 1):
        rod((sx * 0.06, -0.06, 0.12), (sx * 0.06, -0.08, 0.3), 0.03, sm, e, 6)
        sphere(0.018, (sx * 0.04, -0.13, 0.41), mat(LACQUER), parent=e, segs=5)
    sphere(0.05, (0.06, -0.12, 0.13), sm, parent=e, segs=8)
    return e


def pavilion(parent, cx, cy, z, sx, sy, h, rh, over=0.14, ridge=None, lift=0.12, posts=TIMBER, r=0.04):
    """Four (or six) posts and a curled hip or pyramid roof, open sides."""
    pm = mat(posts, rough=0.45)
    xs = (cx - sx / 2, cx + sx / 2)
    ys = (cy - sy / 2, cy + sy / 2)
    for x in xs:
        for y in ys:
            cyl(r, h, (x, y, z + h / 2), pm, parent=parent, segs=8)
            cyl(r * 1.5, 0.04, (x, y, z + 0.02), mat(STONE, rough=0.8), parent=parent, segs=8)
    box((sx + 0.08, sy + 0.08, 0.07), (cx, cy, z + h - 0.035), mat(VERM_DK, rough=0.6), parent=parent)
    box((sx + 0.1, sy + 0.1, 0.03), (cx, cy, z + h - 0.09), mat(TEAL, rough=0.6), parent=parent)
    if sx >= sy:
        roof(parent, (cx, cy), z + h, sx, sy, rh, over, ridge, lift)
    else:
        e = empty("rot", (cx, cy, 0), pi / 2, parent)
        roof(e, (0, 0), z + h, sy, sx, rh, over, ridge, lift)


# ------------------------------------------------------------ town centre


def town_center(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """3 x 3. A great hall with a double roof on a stone platform at the
    back, a side wing, a walled court with a roofed gateway, a bronze ding,
    a drum, carts and grain."""
    r = empty("town_center", loc, rot_z, parent)
    box((2.94, 2.94, 0.05), (0, 0, 0.022), stone((0.76, 0.74, 0.68), 3.2), parent=r, bevel=0.012)
    z = 0.045
    # the great hall: platform, a lower hall with a skirt roof, an upper storey
    platform(2.7, 1.12, 0.22, (0, 0.82, z), r, steps="y", step_w=0.8)
    zp = z + 0.22
    hx0, hx1, hy0, hy1, hh = -1.18, 1.18, 0.4, 1.2, 1.0
    hall(r, hx0, hx1, hy0, hy1, zp, hh)
    zt = zp + hh + 0.18
    roof(r, (0, (hy0 + hy1) / 2), zt, hx1 - hx0, hy1 - hy0, 0.32, over=0.2, lift=0.16)
    ux0, ux1, uy0, uy1, uh = -0.82, 0.82, 0.58, 1.02, 0.5
    hall(r, ux0, ux1, uy0, uy1, zt + 0.1, uh, bays=4)
    zu = zt + 0.1 + uh + 0.18
    roof(r, (0, (uy0 + uy1) / 2), zu, ux1 - ux0, uy1 - uy0, 0.5, over=0.2, lift=0.18)
    for x in (-0.55, -0.18, 0.18, 0.55):
        lattice((x, uy0, zt + 0.38), 0, r, 0.2, 0.22)
    lattice((ux1, (uy0 + uy1) / 2, zt + 0.38), pi / 2, r, 0.2, 0.22)
    door((0, hy0, zp), 0, r, w=0.5, h=0.82)
    for x in (-0.86, -0.5, 0.5, 0.86):
        lattice((x, hy0, zp + 0.55), 0, r, 0.24, 0.32)
    for y in (0.6, 1.0):
        lattice((hx1, y, zp + 0.55), pi / 2, r, 0.22, 0.3)
    valance(r, (hx0 - 0.04, hy0 - 0.07, 0), (hx1 + 0.04, hy0 - 0.07, 0), zp + hh - 0.08, player, 0.15)
    valance(r, (hx1 + 0.07, hy0, 0), (hx1 + 0.07, hy1, 0), zp + hh - 0.08, player, 0.15, rz=pi / 2)
    for x in (-0.3, 0.3):
        lantern((x, hy0 - 0.12, zp + hh - 0.12), r, 1.0, 0.1)
    # the west wing, ridge along Y, facing the court (+X)
    wx0, wx1, wy0, wy1, wh = -1.38, -0.86, -1.28, 0.12, 0.82
    platform(wx1 - wx0 + 0.1, wy1 - wy0 + 0.04, 0.1, ((wx0 + wx1) / 2, (wy0 + wy1) / 2, z), r)
    building(r, wx0, wx1, wy0, wy1, z + 0.1, wh, 0.38, over=0.1, wall=OCHRE, bays=3)
    door((wx1, -0.6, z + 0.1), pi / 2, r, w=0.36, h=0.7, curtain=player)
    for y in (-1.0, -0.2):
        lattice((wx1, y, z + 0.55), pi / 2, r, 0.22, 0.26)
    lattice(((wx0 + wx1) / 2, wy0, z + 0.55), 0, r, 0.2, 0.26)
    # the court wall and its gateway (front, -Y)
    for a, b in (((-0.8, -1.42), (-0.05, -1.42)), ((0.75, -1.42), (1.42, -1.42)), ((1.42, -1.42), (1.42, 0.25))):
        yard_wall(r, a, b, 0.42, 0.1)
    for x in (-0.05, 0.75):
        cyl(0.06, 1.0, (x, -1.42, 0.5), mat(VERMILION, rough=0.45), parent=r, segs=10)
        cyl(0.09, 0.08, (x, -1.42, 0.04), mat(STONE), parent=r, segs=10)
    box((0.92, 0.12, 0.09), (0.35, -1.42, 0.93), mat(VERM_DK, rough=0.6), parent=r)
    box((0.94, 0.13, 0.04), (0.35, -1.42, 0.86), mat(TEAL, rough=0.6), parent=r)
    roof(r, (0.35, -1.42), 0.98, 0.9, 0.14, 0.2, over=0.1, lift=0.1, drop=0.04)
    box((0.3, 0.03, 0.14), (0.35, -1.5, 0.78), mat(LACQUER), parent=r)
    box((0.24, 0.035, 0.09), (0.35, -1.5, 0.78), gold(), parent=r)
    # the court: the ding, a drum, a cart, grain, jars
    incense((0.25, -0.35, z), r, 1.15)
    drum((1.0, 0.0, z), r, 1.0, 0.0, player)
    g.cart((-0.35, -0.95, z), 0.3, player, r, load=False)
    g.sack_pile((-0.45, -0.95, z + 0.26), r, seed=3, n=(2, 1), s=0.1)
    g.sack_pile((1.05, -1.0, z), r, seed=5, n=(3, 2, 1), s=0.11, colours=("wool", "linen", (0.82, 0.74, 0.5)))
    for i, (x, y) in enumerate(((1.18, -0.55), (1.2, -0.32), (0.98, -0.6))):
        jar((x, y, z), r, 0.9, glaze=(0.24, 0.38, 0.34) if i % 2 else None)
    for x in (-0.68, 0.72):
        streamer((x, 0.05, z), 1.85, player, r, seed=int(x * 5))
    bamboo((-0.6, -1.2, z), r, 6, 1.3, seed=4)
    pine((1.2, 0.3, z), r, 0.75, seed=2)
    flag((0.0, 0.8, zu + 0.45), 1.0, player, r, w=0.9, fh=0.56, seed=2)
    return r


# ----------------------------------------------------------------- houses


def house(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, variant=0):
    """2 x 2. variant 0: a courtyard cottage behind a tiled yard wall, a
    kitchen lean-to, bamboo, jars; 1: a two-storey house with a balcony."""
    r = empty("house%d" % variant, loc, rot_z, parent)
    pad(r, 2)
    z = 0.03
    if variant == 0:
        x0, x1, y0, y1, h = -0.85, 0.3, 0.12, 0.78, 0.82
        platform(x1 - x0 + 0.1, y1 - y0 + 0.08, 0.08, ((x0 + x1) / 2, (y0 + y1) / 2, z), r)
        # a commoner's cottage: rammed earth under a thick thatched roof,
        # no brackets, no tile (tile and lacquer are for the great halls)
        hall(r, x0, x1, y0, y1, z + 0.08, h, wall=OCHRE, brackets=False, bays=2, beam=TIMBER)
        roof(r, ((x0 + x1) / 2, (y0 + y1) / 2), z + 0.08 + h, x1 - x0, y1 - y0, 0.5, over=0.16,
             ridge=(x1 - x0) / 2 - 0.12, lift=0.0, tile=THATCH, ornaments=False, thick=0.1)
        door((-0.27, y0, z + 0.08), 0, r, w=0.34, h=0.68, studs=False, curtain=player)
        valance(r, (x0, y0 - 0.07, 0), (x1, y0 - 0.07, 0), z + 0.08 + h - 0.08, player, 0.12)
        lattice((-0.64, y0, z + 0.52), 0, r, 0.2, 0.24)
        lattice((x1, (y0 + y1) / 2, z + 0.52), pi / 2, r, 0.2, 0.24)
        # kitchen lean-to with a chimney
        kx0, kx1, ky0, ky1 = 0.3, 0.82, 0.3, 0.8
        box((kx1 - kx0, ky1 - ky0, 0.6), ((kx0 + kx1) / 2, (ky0 + ky1) / 2, z + 0.3), layered(), parent=r)
        roof(r, ((kx0 + kx1) / 2, (ky0 + ky1) / 2), z + 0.6, kx1 - kx0, ky1 - ky0, 0.24, over=0.1, lift=0.0,
             ornaments=False, tile=THATCH, thick=0.08)
        box((0.12, 0.12, 0.4), (0.7, 0.68, z + 0.85), layered(), parent=r)
        box((0.16, 0.16, 0.04), (0.7, 0.68, z + 1.06), mat(TILE_DK), parent=r)
        box((0.24, 0.03, 0.34), (0.56, ky0 - 0.005, z + 0.2), mat((0.16, 0.1, 0.08)), parent=r)
        # yard wall with a gate
        for a, b in (((-0.92, -0.92), (-0.22, -0.92)), ((0.28, -0.92), (0.92, -0.92)), ((0.92, -0.92), (0.92, 0.25)),
                     ((-0.92, -0.92), (-0.92, 0.05))):
            yard_wall(r, a, b, 0.4, 0.09)
        for x in (-0.22, 0.28):
            box((0.13, 0.13, 0.62), (x, -0.92, 0.31), layered(), parent=r)
            lib.pyramid(0.2, 0.1, (x, -0.92, 0.62), mat(TILE_DK, rough=0.7), parent=r)
        # the yard: bamboo, jars, a vegetable bed, a well, chickens
        bamboo((-0.68, -0.55, z), r, 6, 1.25, seed=1)
        for i, (x, y) in enumerate(((-0.82, -0.05), (-0.66, 0.0), (0.12, -0.05))):
            jar((x, y, z), r, 0.75 + 0.1 * (i % 2), glaze=(0.3, 0.42, 0.36) if i == 1 else None)
        box((0.5, 0.36, 0.06), (0.55, -0.45, z + 0.03), noisy("earth", 0.15, 6.0), parent=r)
        for i in range(4):
            for k in range(3):
                blob(0.05, (0.36 + i * 0.12, -0.57 + k * 0.12, z + 0.08), ("leaf", "leaf_olive")[(i + k) % 2],
                     (1, 1, 0.6), r, i * 3 + k, 0.3, 1)
        g.chicken((0.05, -0.55, z), 0.6, r)
        g.chicken((-0.2, -0.4, z), 2.4, r, (0.72, 0.42, 0.2))
        g.basket((0.1, -0.25, z), r, 0.1, 4, 3)
        straw_hat((-0.3, -0.18, z), r, 0.8)
        g.laundry((0.42, 0.0, z + 0.62), (0.88, 0.0, z + 0.62), r, seed=2,
                  colours=((0.95, 0.92, 0.84), (0.3, 0.42, 0.6), (0.84, 0.74, 0.5)))
        for x in (0.42, 0.88):
            rod((x, 0.0, z), (x, 0.0, z + 0.64), 0.014, "wood_dark", r)
        flag((x0 + 0.08, (y0 + y1) / 2, z + 0.9 + 0.42), 0.55, player, r, w=0.36, fh=0.22, pennant=True)
    else:
        x0, x1, y0, y1 = -0.8, 0.45, 0.05, 0.75
        h1, h2 = 0.8, 0.6
        platform(x1 - x0 + 0.1, y1 - y0 + 0.08, 0.08, ((x0 + x1) / 2, (y0 + y1) / 2, z), r)
        zz = z + 0.08
        hall(r, x0, x1, y0, y1, zz, h1, wall=OCHRE, brackets=False, bays=2)
        roof(r, ((x0 + x1) / 2, (y0 + y1) / 2), zz + h1, x1 - x0, y1 - y0, 0.12, over=0.12, lift=0.08, ornaments=False)
        hall(r, x0 + 0.08, x1 - 0.08, y0 + 0.08, y1 - 0.04, zz + h1 + 0.04, h2, wall=PLASTER, brackets=False, bays=2)
        roof(r, ((x0 + x1) / 2, (y0 + y1 + 0.04) / 2), zz + h1 + h2 + 0.04, x1 - x0 - 0.16, y1 - y0 - 0.12, 0.36,
             over=0.15, lift=0.14)
        door((-0.15, y0, zz), 0, r, w=0.34, h=0.66, studs=False, curtain=player)
        lattice((-0.55, y0, zz + 0.48), 0, r, 0.2, 0.24)
        lattice((x1, 0.4, zz + 0.48), pi / 2, r, 0.2, 0.24)
        for x in (-0.45, 0.1):
            lattice((x, y0 + 0.08, zz + h1 + 0.32), 0, r, 0.2, 0.22)
        valance(r, (x0, y0 - 0.06, 0), (x1, y0 - 0.06, 0), zz + h1 + 0.02, player, 0.12)
        lantern((0.3, y0 - 0.1, zz + h1 - 0.06), r, 0.9)
        for a, b in (((0.92, -0.92), (0.0, -0.92)), ((0.92, -0.92), (0.92, 0.3))):
            yard_wall(r, a, b, 0.38, 0.09)
        bamboo((0.7, 0.55, z), r, 5, 1.3, seed=6)
        pine((-0.6, -0.55, z), r, 0.7, seed=3)
        for i, (x, y) in enumerate(((0.6, -0.6), (0.72, -0.45), (0.45, -0.7))):
            jar((x, y, z), r, 0.8)
        g.chicken((0.1, -0.4, z), 1.0, r)
        g.basket((-0.2, -0.3, z), r, 0.1, 0, 2)
    return r


# --------------------------------------------------------------- barracks


def ji(p0, p1, parent=None):
    """A dagger-axe: a shaft with a bronze blade set crosswise near the top."""
    a, b = Vector(p0), Vector(p1)
    rod(a, b, 0.012, "wood_dark", parent, 5)
    d = (b - a).normalized()
    side = d.cross(Vector((0, 1, 0)))
    if side.length < 0.1:
        side = d.cross(Vector((1, 0, 0)))
    side.normalize()
    c = b - d * 0.08
    rod(c, c + side * 0.13, 0.016, bronze(), parent, 4, r_top=0.004)
    rod(b, b + d * 0.08, 0.012, bronze(), parent, 4, r_top=0.003)
    sphere(0.016, b - d * 0.2, mat((0.8, 0.2, 0.15)), parent=parent, segs=5)


def gateway(parent, x0, x1, y, h=0.95, rh=0.2, player=None, depth=0.14, plaque=True):
    """A roofed gateway between two posts across x0..x1 at y."""
    for x in (x0, x1):
        cyl(0.055, h, (x, y, h / 2), mat(VERMILION, rough=0.45), parent=parent, segs=10)
        cyl(0.085, 0.07, (x, y, 0.035), mat(STONE), parent=parent, segs=10)
    w = x1 - x0
    box((w + 0.1, depth, 0.08), ((x0 + x1) / 2, y, h - 0.04), mat(VERM_DK, rough=0.6), parent=parent)
    box((w + 0.12, depth + 0.01, 0.035), ((x0 + x1) / 2, y, h - 0.1), mat(TEAL, rough=0.6), parent=parent)
    roof(parent, ((x0 + x1) / 2, y), h, w, depth, rh, over=0.1, lift=0.1, drop=0.04)
    if plaque:
        box((0.24, 0.03, 0.12), ((x0 + x1) / 2, y - depth / 2 - 0.02, h - 0.2), mat(LACQUER), parent=parent)
        box((0.18, 0.035, 0.07), ((x0 + x1) / 2, y - depth / 2 - 0.02, h - 0.2), gold(), parent=parent)


def barracks(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A walled drill yard behind a roofed gate, a hall along the back
    with racks of dagger-axes and shields, a war drum, a straw dummy."""
    r = empty("barracks", loc, rot_z, parent)
    pad(r, 2, (0.62, 0.48, 0.32))
    z = 0.03
    x0, x1, y0, y1, h = -0.74, 0.74, 0.4, 0.78, 0.86
    platform(x1 - x0 + 0.1, y1 - y0 + 0.08, 0.08, (0, (y0 + y1) / 2, z), r)
    building(r, x0, x1, y0, y1, z + 0.08, h, 0.4, over=0.15, wall=OCHRE, bays=4)
    door((0, y0, z + 0.08), 0, r, w=0.36, h=0.7)
    valance(r, (x0, y0 - 0.07, 0), (x1, y0 - 0.07, 0), z + 0.08 + h - 0.08, player, 0.13)
    lattice((x1, (y0 + y1) / 2, z + 0.55), pi / 2, r, 0.18, 0.24)
    # racks of dagger-axes and shields against the hall
    for xr in (-0.5, 0.42):
        rod((xr - 0.2, y0 - 0.12, z + 0.62), (xr + 0.2, y0 - 0.12, z + 0.62), 0.014, "wood_dark", r)
        for sx in (-1, 1):
            rod((xr + sx * 0.2, y0 - 0.12, z), (xr + sx * 0.2, y0 - 0.12, z + 0.66), 0.016, "wood_dark", r)
        for i in range(4):
            x = xr - 0.15 + i * 0.1
            ji((x, y0 - 0.16, z), (x + 0.01, y0 - 0.1, z + 1.0), r)
    for i, x in enumerate((-0.25, 0.2)):
        shield((x, y0 - 0.06, z + 0.3), 0, r, player)
    # yard walls and the gate
    for a, b in (((-0.92, -0.92), (-0.3, -0.92)), ((0.3, -0.92), (0.92, -0.92)),
                 ((0.92, -0.92), (0.92, 0.3)), ((-0.92, -0.92), (-0.92, 0.3))):
        yard_wall(r, a, b, 0.48, 0.1)
    gateway(r, -0.28, 0.28, -0.92, 1.0, 0.22)
    for sx in (-1, 1):
        streamer((sx * 0.42, -1.0, 0.0), 1.55, player, r, w=0.2, bh=0.62, seed=sx + 3)
    for y in (-0.6, -0.15):
        shield((0.98, y, 0.3), pi / 2, r, player, 0.8)
    # the yard: drum, straw dummy, weapon stand
    drum((0.5, -0.3, z), r, 0.85, 0.0, player)
    dx, dy = -0.45, -0.35
    cyl(0.03, 0.9, (dx, dy, z + 0.45), "wood_dark", parent=r, segs=6)
    cyl(0.1, 0.36, (dx, dy, z + 0.6), "straw", parent=r, segs=8, r_top=0.085)
    rod((dx - 0.25, dy, z + 0.7), (dx + 0.25, dy, z + 0.7), 0.022, "wood", r)
    sphere(0.085, (dx, dy, z + 0.9), "straw", parent=r, segs=8)
    shield((dx, dy - 0.11, z + 0.55), 0, r, player, 0.7)
    for k in range(3):
        a = k * 2.1
        ji((0.05 + 0.1 * cos(a), -0.6 + 0.1 * sin(a), z), (0.05, -0.6, z + 0.95), r)
    for x in (-0.75, -0.62):
        jar((x, -0.72, z), r, 0.7)
    return r


# ----------------------------------------------------------------- temple


def round_roof(parent, c, z, R, h, over=0.16, tile=None, lift=0.05, ribs=24, thick=0.05):
    """A round (conical, concave) tiled roof over a drum of radius R whose
    top is at z: tile ribs down the slope, a dark soffit, a gilt knob."""
    tile = tile or TEMPLE_TILE
    cx, cy = c
    k = 0.32
    ze = z + 0.02 - lift
    prof = [(R + over, ze - thick)]
    for j in range(9):
        t = j / 8
        prof.append(((R + over) * (1 - t) + 0.01, ze + h * (t * (k + (1 - k) * t))))
    g.lathe(prof, mat(tile, rough=0.6), (cx, cy, 0), segs=32, parent=parent, smooth=True, name="roof")
    rm = mat(tint(tile, 0.62), rough=0.6)
    for i in range(ribs):
        a = 2 * pi * i / ribs
        pts = [((R + over) * (1 - t) * cos(a) + cx, (R + over) * (1 - t) * sin(a) + cy,
                ze + h * (t * (k + (1 - k) * t)) + 0.012) for t in (0.0, 0.3, 0.6, 0.9)]
        for p, q in zip(pts, pts[1:]):
            rod(p, q, 0.012, rm, parent, 4)
    lathe_m = mat(SOFFIT, rough=0.8)
    cyl(R + over - 0.02, 0.012, (cx, cy, ze - thick - 0.004), lathe_m, parent=parent, segs=32)
    return ze + h


def temple(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A round hall of prayer: a two-eaved round pavilion with deep
    green glazed tile and a gilt knob, its oxblood lattice drum on a
    three-tiered round marble terrace; a smoking bronze ding between stone
    lions before the stair, a bell, the owner's streamers."""
    r = empty("temple", loc, rot_z, parent)
    pad(r, 2, (0.82, 0.8, 0.74))
    z = 0.03
    cx, cy = 0.0, 0.18
    marble = mat((0.93, 0.91, 0.87), rough=0.7)
    zz = z
    for i, (rad, hh) in enumerate(((0.8, 0.1), (0.7, 0.1), (0.6, 0.1))):
        cyl(rad, hh, (cx, cy, zz + hh / 2), marble, parent=r, segs=40)
        cyl(rad + 0.012, 0.025, (cx, cy, zz + hh - 0.012), mat((0.80, 0.78, 0.74), rough=0.7), parent=r, segs=40)
        zz += hh
    # the stair up the front
    for i in range(4):
        hh = 0.3 * (4 - i) / 5
        box((0.4, 0.12, hh), (cx, cy - 0.66 - i * 0.1, z + hh / 2), marble, parent=r, bevel=0.006)
    # the drum: oxblood posts, lattice between them, painted bands
    R1, h1 = 0.44, 0.5
    cyl(R1 - 0.02, h1, (cx, cy, zz + h1 / 2), mat(LATTICE, rough=0.6), parent=r, segs=24)
    for i in range(12):
        a = 2 * pi * (i + 0.5) / 12
        cyl(0.04, h1, (cx + R1 * cos(a), cy + R1 * sin(a), zz + h1 / 2), mat(VERMILION, rough=0.45), parent=r,
            segs=8)
    for i in range(12):
        a = 2 * pi * i / 12
        e = face((cx + (R1 - 0.01) * cos(a), cy + (R1 - 0.01) * sin(a), zz), a + pi / 2, r)
        box((0.15, 0.012, 0.3), (0, -0.005, 0.28), mat(PAPER, rough=0.8), parent=e)
        for k in range(3):
            box((0.012, 0.02, 0.3), (-0.05 + k * 0.05, -0.012, 0.28), mat(LATTICE), parent=e)
    cyl(R1 + 0.05, 0.06, (cx, cy, zz + h1 - 0.03), mat(TEAL, rough=0.6), parent=r, segs=32)
    cyl(R1 + 0.06, 0.03, (cx, cy, zz + h1 + 0.01), mat(VERM_DK, rough=0.6), parent=r, segs=32)
    door((cx, cy - R1 - 0.01, zz), 0, r, w=0.26, h=0.42)
    z1 = round_roof(r, (cx, cy), zz + h1 + 0.03, R1 + 0.04, 0.22, over=0.18)
    # the upper drum and roof
    R2, h2 = 0.3, 0.26
    zu = zz + h1 + 0.1
    cyl(R2, h2 + 0.06, (cx, cy, zu + (h2 + 0.06) / 2), mat(VERMILION, rough=0.5), parent=r, segs=24)
    cyl(R2 + 0.03, 0.05, (cx, cy, zu + h2 + 0.04), mat(TEAL, rough=0.6), parent=r, segs=24)
    z2 = round_roof(r, (cx, cy), zu + h2 + 0.07, R2, 0.34, over=0.16)
    sphere(0.07, (cx, cy, z2 + 0.06), gold(), parent=r, segs=12)
    cyl(0.02, 0.16, (cx, cy, z2 + 0.18), gold(), parent=r, segs=6, r_top=0.004)
    # the court
    incense((0, -0.72, z), r, 0.9)
    for sx in (-1, 1):
        stone_lion((sx * 0.4, -0.78, z), 0, r, 0.85)
        streamer((sx * 0.86, -0.88, z), 1.7, player, r, seed=sx + 7)
    bell((0.8, -0.2, z), r, 0.55, pi / 2)
    pine((-0.84, -0.3, z), r, 0.65, seed=5)
    return r


# ----------------------------------------------------------- watch tower


def watch_tower(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """1 x 1. A Han-style timber watchtower: a rammed-earth lower storey
    under a skirt roof, a railed lookout with a bell, a curled pyramid roof."""
    r = empty("watch_tower", loc, rot_z, parent)
    box((0.94, 0.94, 0.08), (0, 0, 0.04), stone(STONE_DK, 10.0), parent=r, bevel=0.012)
    b = 0.62
    hb = 0.95
    box((b, b, hb), (0, 0, 0.08 + hb / 2), layered(), parent=r, bevel=0.01)
    for sx in (-1, 1):
        for sy in (-1, 1):
            box((0.07, 0.07, hb), (sx * (b / 2 - 0.02), sy * (b / 2 - 0.02), 0.08 + hb / 2),
                mat(VERMILION, rough=0.45), parent=r)
    door((0.0, -b / 2, 0.08), 0, r, w=0.26, h=0.56, studs=False)
    box((0.04, 0.03, 0.16), (b / 2 + 0.002, 0.0, 0.62), mat(LACQUER), parent=r)
    shield((b / 2 + 0.01, -0.16, 0.42), pi / 2, r, player, 0.7)
    zs = 0.08 + hb
    roof(r, (0, 0), zs, b, b, 0.16, over=0.14, ridge=0, lift=0.1, ornaments=False, drop=0.04)
    # the lookout
    zl = zs + 0.1
    box((0.72, 0.72, 0.06), (0, 0, zl), mat("cedar"), parent=r)
    lw = 0.34
    hall(r, -lw / 2 - 0.04, lw / 2 + 0.04, -lw / 2 - 0.04, lw / 2 + 0.04, zl + 0.03, 0.5, bays=1)
    lattice((0, -lw / 2 - 0.04, zl + 0.33), 0, r, 0.18, 0.2)
    lattice((lw / 2 + 0.04, 0, zl + 0.33), pi / 2, r, 0.18, 0.2)
    pm = mat(VERMILION, rough=0.45)
    q = 0.34
    for sx in (-1, 1):
        for sy in (-1, 1):
            cyl(0.02, 0.26, (sx * q, sy * q, zl + 0.16), pm, parent=r, segs=6)
        box((2 * q + 0.04, 0.03, 0.03), (0, sx * q, zl + 0.28), pm, parent=r)
        box((0.03, 2 * q + 0.04, 0.03), (sx * q, 0, zl + 0.28), pm, parent=r)
    cloth(0.5, 0.18, mat(player, rough=0.7), (0, -q - 0.02, zl + 0.27), 0, r, hang=True, wave=0.008, nx=4)
    cloth(0.5, 0.18, mat(player, rough=0.7), (q + 0.02, 0, zl + 0.27), pi / 2, r, hang=True, wave=0.008, nx=4)
    # a bell hung under the eave at the front corner
    g.lathe([(0.02, 0), (0.06, 0.01), (0.07, 0.1), (0.03, 0.14)], verd(), (0.26, -0.26, zl + 0.36), parent=r, segs=10)
    rod((0.26, -0.26, zl + 0.5), (0.26, -0.26, zl + 0.62), 0.005, mat(LACQUER), r, 4)
    zr = zl + 0.03 + 0.5 + 0.18
    roof(r, (0, 0), zr, lw + 0.08, lw + 0.08, 0.42, over=0.17, ridge=0, lift=0.14)
    flag((0, 0, zr + 0.42), 0.55, player, r, w=0.42, fh=0.2, pennant=True)
    # ladder up the +X side
    for t in (-1, 1):
        rod((0.47, 0.18 + t * 0.08, 0.02), (0.36, 0.18 + t * 0.08, zl), 0.012, "wood", r)
    for i in range(8):
        f = (i + 0.7) / 8.5
        rod((0.47 - 0.11 * f, 0.1, 0.02 + f * (zl - 0.02)), (0.47 - 0.11 * f, 0.26, 0.02 + f * (zl - 0.02)), 0.009,
            "wood", r)
    jar((-0.3, -0.38, 0.08), r, 0.6)
    return r


# ---------------------------------------------------------------- academy


def academy(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A scholars' court: a two-storey library (a skirt roof round
    its ground floor, a hip-and-gable roof over the upper one) with its
    lattice doors open on shelves of bamboo books, a square reading
    pavilion, a lesson in the open court, a pine, bamboo, a scholar's rock."""
    r = empty("academy", loc, rot_z, parent)
    box((1.92, 1.92, 0.04), (0, 0, 0.015), stone((0.80, 0.78, 0.72), 4.0), parent=r, bevel=0.012)
    z = 0.035
    x0, x1, y0, y1, h1, h2 = -0.86, 0.14, 0.12, 0.86, 0.7, 0.5
    platform(x1 - x0 + 0.12, y1 - y0 + 0.1, 0.12, ((x0 + x1) / 2, (y0 + y1) / 2, z), r, steps="y", step_w=0.36)
    zz = z + 0.12
    hall(r, x0, x1, y0, y1, zz, h1, wall=PLASTER, brackets=False, bays=3)
    roof(r, ((x0 + x1) / 2, (y0 + y1) / 2), zz + h1, x1 - x0, y1 - y0, 0.14, over=0.16, lift=0.09, ornaments=False)
    ux0, ux1, uy0, uy1 = x0 + 0.12, x1 - 0.12, y0 + 0.12, y1 - 0.08
    hall(r, ux0, ux1, uy0, uy1, zz + h1 + 0.06, h2, wall=PLASTER, brackets=True, bays=2)
    zt = zz + h1 + 0.06 + h2 + 0.18
    roof(r, ((ux0 + ux1) / 2, (uy0 + uy1) / 2), zt, ux1 - ux0, uy1 - uy0, 0.36, over=0.18,
         ridge=(ux1 - ux0) / 2 + 0.06, lift=0.15)
    for i, x in enumerate((-0.7, -0.36, -0.02)):
        if i == 1:
            box((0.26, 0.02, 0.56), (x, y0 - 0.004, zz + 0.3), mat((0.14, 0.1, 0.08)), parent=r)
            g.scroll_rack((x, y0 + 0.02, zz), 0, r, 0.22, 0.46, seed=i)
        else:
            lattice((x, y0, zz + 0.34), 0, r, 0.22, 0.5, n=(3, 5))
    lattice((x1, (y0 + y1) / 2, zz + 0.38), pi / 2, r, 0.22, 0.32)
    for x in (-0.58, -0.16):
        lattice((x, uy0, zz + h1 + 0.3), 0, r, 0.2, 0.22)
    valance(r, (x0, y0 - 0.07, 0), (x1, y0 - 0.07, 0), zz + h1 - 0.08, player, 0.13)
    # the reading pavilion at the back right, a low table of books under it
    px, py = 0.52, 0.46
    box((0.7, 0.7, 0.08), (px, py, z + 0.04), stone(STONE, 8.0), parent=r, bevel=0.01)
    pavilion(r, px, py, z + 0.08, 0.52, 0.52, 0.66, 0.34, over=0.12, ridge=0, lift=0.13)
    box((0.34, 0.2, 0.03), (px, py, z + 0.25), mat(LACQUER, rough=0.4), parent=r)
    slips((px - 0.06, py, z + 0.265), r, 0.3, 0.9)
    # the lesson in the open court
    mx, my = -0.3, -0.3
    box((0.34, 0.26, 0.06), (mx, my, z + 0.03), mat(LACQUER, rough=0.5), parent=r)
    person((mx, my, z + 0.06), 0.0, r, robe=(0.5, 0.32, 0.18), sit=True, s=0.95)
    box((0.18, 0.1, 0.12), (mx, my - 0.2, z + 0.06), mat(LACQUER, rough=0.4), parent=r)
    slips((mx, my - 0.2, z + 0.12), r, pi / 2, 0.7)
    for i, (x, y) in enumerate(((-0.58, -0.62), (-0.3, -0.62), (-0.02, -0.62), (-0.44, -0.86), (-0.16, -0.86))):
        box((0.24, 0.2, 0.01), (x, y, z + 0.006), mat(player, rough=0.8) if i % 2 == 0 else mat(BAMBOO_DRY, rough=0.9),
            parent=r)
        person((x, y, z + 0.012), pi, r, robe=((0.30, 0.36, 0.55), (0.82, 0.78, 0.68), (0.45, 0.24, 0.2))[i % 3],
               sit=True, s=0.85)
    pine((0.6, -0.4, z), r, 0.75, seed=11)
    bamboo((0.84, 0.0, z), r, 5, 1.3, seed=9)
    blob(0.1, (0.3, -0.82, z + 0.16), mat((0.62, 0.64, 0.62), rough=0.7), (0.8, 0.7, 1.9), r, 4, 0.35, 1)
    streamer((0.9, -0.92, z), 1.6, player, r, seed=5)
    return r


# ---------------------------------------------------------- archery range


def archery_range(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. Three straw targets against an earth bank and a tiled wall at
    the back, lane pennants, a shooting rail with quivers and bows, and a
    small roofed pavilion with a bow rack."""
    r = empty("archery_range", loc, rot_z, parent)
    pad(r, 2)
    z = 0.03
    yard_wall(r, (-0.9, 0.86), (0.32, 0.86), 0.62, 0.14)
    for i in range(5):
        blob(0.16, (-0.8 + i * 0.27, 0.68, z + 0.05), ("grass", "earth", "grass_dry")[i % 3], (1.2, 0.9, 0.7), r, i,
             0.25, 1)
    for i, x in enumerate((-0.74, -0.34, 0.06)):
        g.target((x, 0.5, z + 0.56), 0.0, r, 0.23, 3, seed=i)
        box((0.022, 1.2, 0.008), (x - 0.2, 0.0, z + 0.012), "whitewash", parent=r)
    box((0.022, 1.2, 0.008), (0.26, 0.0, z + 0.012), "whitewash", parent=r)
    box((1.3, 0.035, 0.008), (-0.25, -0.6, z + 0.012), "whitewash", parent=r)
    for i, x in enumerate((-0.92, -0.54, -0.14, 0.26)):
        flag((x, 0.7, z), 1.05 if i % 3 == 0 else 0.92, player, r, w=0.3, fh=0.16, pennant=True, seed=i)
    for x in (-0.9, -0.3, 0.3):
        box((0.05, 0.05, 0.4), (x, -0.76, z + 0.2), mat(VERMILION, rough=0.5), parent=r)
    rod((-0.92, -0.76, z + 0.36), (0.32, -0.76, z + 0.36), 0.02, mat(VERMILION, rough=0.5), r)
    g.quiver((-0.62, -0.81, z), r, player, 0.2)
    g.quiver((-0.02, -0.81, z), r, player, -0.15)
    g.bow((-0.45, -0.805, z + 0.28), 0.0, r, 0.42)
    g.bow((0.18, -0.805, z + 0.28), 0.0, r, 0.42, "leather")
    # the pavilion in the +X back corner, clear of the targets, over a bow rack
    sx0, sx1, sy0, sy1 = 0.6, 0.88, 0.26, 0.86
    box((sx1 - sx0 + 0.12, sy1 - sy0 + 0.12, 0.07), ((sx0 + sx1) / 2, (sy0 + sy1) / 2, z + 0.035), stone(STONE, 8.0),
        parent=r)
    pavilion(r, (sx0 + sx1) / 2, (sy0 + sy1) / 2, z + 0.07, sx1 - sx0, sy1 - sy0, 0.8, 0.26, over=0.08, lift=0.1)
    valance(r, (sx1 + 0.05, sy0, 0), (sx1 + 0.05, sy1, 0), z + 0.07 + 0.72, player, 0.14, rz=pi / 2)
    rx, ry = (sx0 + sx1) / 2, (sy0 + sy1) / 2
    for y in (ry - 0.22, ry + 0.22):
        for sx in (-1, 1):
            rod((rx + sx * 0.1, y, z + 0.07), (rx, y, z + 0.6), 0.013, "wood_dark", r)
    rod((rx, ry - 0.26, z + 0.58), (rx, ry + 0.26, z + 0.58), 0.015, "wood", r)
    for i in range(3):
        g.bow((rx - 0.03, ry - 0.16 + i * 0.15, z + 0.38), pi / 2, r, 0.44, ("wood_dark", "leather")[i % 2])
    g.quiver((0.7, -0.45, z), r, player, -0.1, rz=pi / 2)
    g.quiver((0.82, -0.35, z), r, player, 0.15, rz=pi / 2)
    g.hay_bale((-0.62, -0.38, z), 0.3, r)
    jar((0.36, -0.42, z), r, 0.7)
    return r


# ----------------------------------------------------------------- market


def stall(loc, rz=0.0, parent=None, player=BLUE, w=0.66, d=0.46, goods=0, seed=0):
    """A market stall: a counter under a player-colour cloth canopy on four
    bamboo poles, a fringe along its front; goods 0 silk, 1 jars, 2 produce."""
    e = empty("stall", loc, rz, parent)
    zb, zf = 0.95, 0.82
    pm = mat(BAMBOO_DRY, rough=0.6)
    for x in (-w / 2, w / 2):
        rod((x, d / 2, 0), (x, d / 2, zb), 0.018, pm, e, 6)
        rod((x, -d / 2, 0), (x, -d / 2, zf), 0.018, pm, e, 6)
    cm = mat(player, rough=0.75)
    lib.mesh([(-w / 2 - 0.06, d / 2 + 0.05, zb + 0.04), (w / 2 + 0.06, d / 2 + 0.05, zb + 0.04),
              (w / 2 + 0.06, -d / 2 - 0.08, zf - 0.01), (-w / 2 - 0.06, -d / 2 - 0.08, zf - 0.01)], [(0, 1, 2, 3)], cm,
             parent=e)
    # two pale stripes down the canopy, so it is a pattern, not a sheet
    sm = mat(PAPER, rough=0.75)
    for k in (-1, 1):
        xa, xb = k * (w * 0.2) - 0.035, k * (w * 0.2) + 0.035
        lib.mesh([(xa, d / 2 + 0.05, zb + 0.047), (xb, d / 2 + 0.05, zb + 0.047),
                  (xb, -d / 2 - 0.08, zf - 0.003), (xa, -d / 2 - 0.08, zf - 0.003)], [(0, 1, 2, 3)], sm,
                 parent=e)
    cloth(w + 0.12, 0.1, cm, (0, -d / 2 - 0.085, zf - 0.005), 0, e, hang=True, wave=0.01, nx=5)
    box((w + 0.12, 0.01, 0.02), (0, -d / 2 - 0.09, zf - 0.11), gold(), parent=e)
    box((w - 0.04, d * 0.6, 0.035), (0, -d * 0.12, 0.4), "cedar", parent=e)
    for x in (-w / 2 + 0.05, w / 2 - 0.05):
        box((0.04, 0.04, 0.38), (x, -d * 0.12, 0.19), "wood_dark", parent=e)
    box((w - 0.02, 0.02, 0.16), (0, -d * 0.42, 0.3), mat(VERMILION, rough=0.6), parent=e)
    if goods == 0:
        silk_bolts((0, -d * 0.12, 0.42), e, 3, seed)
    elif goods == 1:
        for i in range(3):
            jar(((i - 1) * w * 0.3, -d * 0.12, 0.42), e, 0.45, colour=((0.42, 0.3, 0.22), (0.3, 0.46, 0.4),
                                                                     (0.86, 0.84, 0.76))[i])
    else:
        for i in range(3):
            g.basket(((i - 1) * w * 0.3, -d * 0.12, 0.42), e, 0.09, (i + seed) % 6, seed + i)
    lantern((w / 2, -d / 2, zf - 0.02), e, 0.7, 0.04)
    return e


def market(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A market street: a two-storey shop with a balcony and hanging
    shop banners at the back, stalls under player-colour canopies, silk,
    jars, baskets, a string of lanterns, a balance."""
    r = empty("market", loc, rot_z, parent)
    box((1.92, 1.92, 0.04), (0, 0, 0.015), stone((0.78, 0.76, 0.7), 3.8), parent=r, bevel=0.012)
    z = 0.035
    x0, x1, y0, y1 = -0.84, 0.5, 0.42, 0.84
    h1, h2 = 0.78, 0.52
    hall(r, x0, x1, y0, y1, z, h1, wall=OCHRE, brackets=False, bays=3)
    roof(r, ((x0 + x1) / 2, (y0 + y1) / 2), z + h1, x1 - x0, y1 - y0, 0.12, over=0.12, lift=0.08, ornaments=False)
    hall(r, x0 + 0.06, x1 - 0.06, y0 + 0.08, y1 - 0.04, z + h1 + 0.06, h2, wall=PLASTER, brackets=False, bays=3)
    roof(r, ((x0 + x1) / 2, (y0 + y1 + 0.04) / 2), z + h1 + h2 + 0.06, x1 - x0 - 0.12, y1 - y0 - 0.12, 0.34, over=0.14,
         lift=0.13)
    # the shop front: open counters with goods, hanging signs in player colour
    for i, x in enumerate((-0.6, -0.17, 0.27)):
        box((0.32, 0.02, 0.56), (x, y0 - 0.004, z + 0.3), mat((0.15, 0.1, 0.08)), parent=r)
        box((0.36, 0.14, 0.05), (x, y0 - 0.08, z + 0.3), "cedar", parent=r)
        cloth(0.12, 0.42, mat(player, rough=0.7), (x + 0.13, y0 - 0.06, z + h1 - 0.02), 0, r, hang=True, wave=0.006,
              nx=2)
    silk_bolts((-0.6, y0 - 0.08, z + 0.32), r, 3, 1)
    for k in range(3):
        jar((-0.22 + k * 0.08, y0 - 0.08, z + 0.32), r, 0.32, colour=((0.3, 0.46, 0.4), (0.86, 0.84, 0.76))[k % 2])
    g.basket((0.27, y0 - 0.08, z + 0.32), r, 0.08, 1, 2)
    for x in (-0.45, 0.05):
        lattice((x, y0 + 0.08, z + h1 + 0.3), 0, r, 0.2, 0.22)
    lattice((x1 - 0.06, 0.62, z + h1 + 0.3), pi / 2, r, 0.18, 0.22)
    lattice((x1, 0.62, z + 0.45), pi / 2, r, 0.18, 0.24)
    pm = mat(VERMILION, rough=0.5)
    box((x1 - x0, 0.03, 0.03), ((x0 + x1) / 2, y0 - 0.06, z + h1 + 0.2), pm, parent=r)
    for i in range(7):
        box((0.02, 0.02, 0.16), (x0 + 0.1 + i * 0.19, y0 - 0.06, z + h1 + 0.11), pm, parent=r)
    # stalls and a lantern string
    stall((-0.42, -0.2, z), 0, r, player, goods=0, seed=1)
    stall((0.55, -0.32, z), pi / 2, r, player, 0.6, 0.42, goods=2, seed=2)
    stall((-0.5, -0.82 + 0.25, z), 0, r, player, 0.5, 0.36, goods=1, seed=3).scale = (0.85, 0.85, 0.85)
    for i in range(5):
        t = i / 4
        lantern((-0.84 + t * 0.4, 0.0 - t * 0.0 + 0.2, z + 1.1 - 0.06 * sin(pi * t)), r, 0.6, 0.04)
    rod((-0.88, 0.2, z), (-0.88, 0.2, z + 1.18), 0.016, "wood_dark", r)
    # loose goods, a balance, a cart
    for i, (x, y) in enumerate(((0.82, 0.3), (0.72, 0.4), (0.84, 0.12))):
        jar((x, y, z), r, 0.85, colour=((0.42, 0.3, 0.22), (0.3, 0.46, 0.4), (0.86, 0.84, 0.76))[i])
    g.basket((0.1, -0.8, z), r, 0.11, 3, 2)
    g.basket((0.25, -0.86, z), r, 0.1, 0, 5)
    g.sack((-0.12, -0.6, z), r, 0.1, 5)
    g.crate((0.84, -0.86, z), -0.2, r, 0.22, fruit=1, seed=3)
    bxp, byp = 0.05, -0.2
    box((0.05, 0.05, 0.62), (bxp, byp, z + 0.31), "wood_dark", parent=r)
    rod((bxp - 0.18, byp, z + 0.6), (bxp + 0.18, byp, z + 0.6), 0.01, bronze(), r)
    for sx in (-1, 1):
        rod((bxp + sx * 0.18, byp, z + 0.6), (bxp + sx * 0.18, byp, z + 0.45), 0.003, "wool", r, 4)
        cyl(0.06, 0.012, (bxp + sx * 0.18, byp, z + 0.44), bronze(), parent=r, segs=10)
    flag((x1 - 0.2, 0.62, z + h1 + h2 + 0.3), 0.6, player, r, w=0.44, fh=0.26, seed=4)
    return r


# ----------------------------------------------------------------- stable


def stable(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A row of stalls under a tiled roof with horses looking out
    over the half-doors, a hay shed, a bamboo-fenced paddock with a horse."""
    r = empty("stable", loc, rot_z, parent)
    pad(r, 2, (0.6, 0.47, 0.32))
    z = 0.03
    x0, x1, y0, y1, h = -0.82, 0.5, 0.36, 0.8, 0.82
    platform(x1 - x0 + 0.08, y1 - y0 + 0.06, 0.06, ((x0 + x1) / 2, (y0 + y1) / 2, z), r)
    building(r, x0, x1, y0, y1, z + 0.06, h, 0.36, over=0.14, wall=OCHRE, bays=4)
    zz = z + 0.06
    coats = [("cedar", "hair_black", True), ((0.93, 0.9, 0.84), (0.7, 0.66, 0.6), False),
             ((0.24, 0.17, 0.13), "hair_black", True), ((0.70, 0.48, 0.26), (0.36, 0.22, 0.13), False)]
    for i, x in enumerate((-0.66, -0.33, 0.0, 0.33)):
        box((0.24, 0.02, 0.62), (x, y0 - 0.006, zz + 0.33), mat((0.12, 0.08, 0.06)), parent=r)
        box((0.24, 0.035, 0.36), (x, y0 - 0.03, zz + 0.19), mat(VERMILION, rough=0.5), parent=r)
        box((0.26, 0.045, 0.03), (x, y0 - 0.035, zz + 0.38), mat(LACQUER), parent=r)
        cm, mn, bl = coats[i]
        g.horse_head((x, y0 - 0.06, zz + 0.38), 0, r, cm, mn, bl)
    valance(r, (x0, y0 - 0.07, 0), (x1, y0 - 0.07, 0), zz + h - 0.08, player, 0.12)
    # hay shed on the +X back corner
    pavilion(r, 0.76, 0.56, z, 0.32, 0.5, 0.7, 0.2, over=0.06, lift=0.06, posts=(0.45, 0.3, 0.2), r=0.03)
    for (x, y, k) in ((0.76, 0.44, 0), (0.76, 0.68, 0), (0.76, 0.56, 1)):
        g.hay_bale((x, y, z + k * 0.16), pi / 2, r, (0.24, 0.2, 0.16))
    # paddock
    fence([(-0.93, 0.25), (-0.93, -0.93), (-0.1, -0.93)], r, 0.42)
    fence([(0.3, -0.93), (0.93, -0.93), (0.93, 0.22)], r, 0.42)
    for sx in (-0.1, 0.3):
        cyl(0.04, 0.6, (sx, -0.93, 0.3), mat(VERMILION, rough=0.5), parent=r, segs=8)
        sphere(0.04, (sx, -0.93, 0.62), gold(), parent=r, segs=6)
    for (x, y, rz) in ((-0.93, -0.45, pi / 2), (0.6, -0.93, 0.0)):
        e = face((x, y, 0.42), rz, r)
        cloth(0.3, 0.2, mat(player, rough=0.7), (0, -0.02, 0.0), 0, e, hang=True, wave=0.01, nx=4)
        cloth(0.3, 0.2, mat(player, rough=0.7), (0, 0.02, 0.0), pi, e, hang=True, wave=0.01, nx=4)
    g._horse((0.36, -0.45, z), -1.1, r, coat=(0.62, 0.38, 0.2), cloth=[gold(), mat(player, rough=0.7), gold()])
    g.trough((-0.45, -0.6, z), 0, r, 0.55)
    g.hay_bale((-0.62, -0.2, z), 0.4, r)
    blob(0.12, (-0.28, -0.22, z + 0.02), "straw", (1.4, 1, 0.5), r, 5, 0.3, 1)
    streamer((-0.88, 0.3, z), 1.5, player, r, seed=7)
    return r


# ------------------------------------------------------------- storehouse


def storehouse(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A granary raised on stilts with a ladder to its door, a
    lean-to over timber and dressed stone, sacks of grain, jars and a chest
    of gold in the yard."""
    r = empty("storehouse", loc, rot_z, parent)
    pad(r, 2)
    z = 0.03
    x0, x1, y0, y1 = -0.86, 0.2, 0.3, 0.82
    zf = z + 0.36
    for x in (x0 + 0.05, (x0 + x1) / 2, x1 - 0.05):
        for y in (y0 + 0.05, y1 - 0.05):
            cyl(0.045, zf - z, (x, y, (z + zf) / 2), mat("wood_dark"), parent=r, segs=8)
            cyl(0.09, 0.04, (x, y, zf - 0.04), mat(STONE_DK), parent=r, segs=10)   # rat guards
            cyl(0.07, 0.04, (x, y, z + 0.02), mat(STONE), parent=r, segs=8)
    box((x1 - x0 + 0.08, y1 - y0 + 0.08, 0.06), ((x0 + x1) / 2, (y0 + y1) / 2, zf + 0.03), mat("cedar"), parent=r)
    zw = zf + 0.06
    hall(r, x0, x1, y0, y1, zw, 0.66, wall=(0.70, 0.52, 0.34), brackets=False, bays=2)
    for i in range(6):
        box((x1 - x0 - 0.02, 0.012, 0.012), ((x0 + x1) / 2, y0 - 0.002, zw + 0.12 + i * 0.09),
            mat((0.5, 0.36, 0.22)), parent=r)
    roof(r, ((x0 + x1) / 2, (y0 + y1) / 2), zw + 0.66, x1 - x0, y1 - y0, 0.4, over=0.14, lift=0.14,
         ridge=(x1 - x0) / 2 - 0.05)
    door((-0.35, y0, zw), 0, r, w=0.3, h=0.5, studs=False, curtain=player)
    for t in (-1, 1):
        rod((-0.35 + t * 0.09, y0 - 0.42, z), (-0.35 + t * 0.09, y0 - 0.05, zw + 0.02), 0.013, "wood", r)
    for i in range(5):
        f = (i + 0.6) / 5.5
        rod((-0.44, y0 - 0.42 + 0.37 * f, z + f * (zw - z)), (-0.26, y0 - 0.42 + 0.37 * f, z + f * (zw - z)), 0.009,
            "wood", r)
    g.sack_pile((-0.75, 0.55, z), r, seed=4, n=(3, 2), s=0.09)
    # lean-to on the +X side
    lx0, lx1, ly0, ly1 = 0.36, 0.92, -0.2, 0.9
    pm = mat("wood_dark")
    for y in (ly0 + 0.03, ly1 - 0.03):
        cyl(0.03, 0.72, (lx1 - 0.04, y, z + 0.36), pm, parent=r, segs=8)
        cyl(0.03, 0.72, (lx0 + 0.04, y, z + 0.36), pm, parent=r, segs=8)
    e = empty("rot", ((lx0 + lx1) / 2, (ly0 + ly1) / 2, 0), pi / 2, r)
    roof(e, (0, 0), z + 0.72, ly1 - ly0, lx1 - lx0, 0.2, over=0.06, lift=0.08, ridge=(ly1 - ly0) / 2)
    valance(r, (lx1 + 0.04, ly0, 0), (lx1 + 0.04, ly1, 0), z + 0.68, player, 0.12, rz=pi / 2)
    g.timber_stack((0.64, 0.4, z), 0, r, (4, 3, 2), 0.86, 0.06, tarp=player)
    g.stone_pile((0.62, -0.5, z), r, seed=3)
    g.sack_pile((-0.6, -0.15, z), r, seed=2)
    g.sack_pile((-0.7, -0.55, z), r, seed=7, n=(3, 2), s=0.11, colours=("linen", "wool"))
    g.gold_pile((-0.05, -0.6, z), r, seed=4)
    for i, (x, y) in enumerate(((-0.12, -0.12), (0.06, -0.18), (-0.02, 0.02))):
        jar((x, y, z), r, 0.85, glaze=(0.3, 0.44, 0.38) if i == 1 else None)
    g.crate((0.28, -0.84, z), 0.2, r, 0.22)
    streamer((-0.9, -0.9, z), 1.45, player, r, seed=5)
    return r


# --------------------------------------------------------- siege workshop


def trebuchet(loc, rz=0.0, parent=None, player=BLUE, done=False):
    """A traction trebuchet on a timber base, long axis local X: two A-frame
    trestles carry an axle; done=False: the arm lies on trestles beside it
    and a wheel waits to go on."""
    e = empty("trebuchet", loc, rz, parent)
    w = mat("cedar")
    for sy in (-1, 1):
        box((1.0, 0.08, 0.08), (0, sy * 0.24, 0.04), w, parent=e, bevel=0.008)
        for sx in (-1, 1):
            rod((sx * 0.34, sy * 0.24, 0.06), (0, sy * 0.22, 0.95), 0.03, "wood", e, 6)
        rod((-0.2, sy * 0.24, 0.45), (0.2, sy * 0.24, 0.45), 0.02, "wood_dark", e)
    for x in (-0.45, 0.45):
        box((0.08, 0.56, 0.07), (x, 0, 0.08), w, parent=e)
    cyl(0.035, 0.6, (0, 0, 0.95), mat("wood_dark"), rot=(pi / 2, 0, 0), parent=e, segs=8)
    for sy in (-1, 1):
        cyl(0.05, 0.03, (0, sy * 0.3, 0.95), bronze(), rot=(pi / 2, 0, 0), parent=e, segs=8)
    if True:
        # the arm is up on its axle even half built: the long beam and its
        # sling are what say "trebuchet"; ropes not yet on the short end
        rod((-0.9, 0, 0.52), (0.55, 0, 1.2), 0.045, "wood_dark", e, 8, r_top=0.03)
        rod((-0.9, 0, 0.52), (-0.95, 0, 0.18), 0.006, "wool", e, 4)
        lib.mesh([(-0.08, 0, 0), (0.08, 0, 0), (0.05, 0, -0.1), (-0.05, 0, -0.1)], [(0, 1, 2, 3)], "leather",
                 loc=(-0.95, 0, 0.26), parent=e)
    if done:
        pass
    else:
        for x in (-0.3, 0.45):
            for sx in (-1, 1):
                rod((x + sx * 0.08, -0.6, 0.0), (x, -0.6, 0.26), 0.014, "wood_dark", e)
        rod((-0.3, -0.6, 0.3), (0.45, -0.6, 0.29), 0.035, "wood", e, 8, r_top=0.026)
        for k in range(4):
            cyl(0.08 - k * 0.015, 0.025, (0.6, -0.6, 0.31 + k * 0.025), "wool", parent=e, segs=10)
    cloth(0.32, 0.18, mat(player, rough=0.7), (0, 0.22, 0.98), pi / 4, e, taper=0.85, wave=0.04, seed=3)
    rod((0, 0.22, 0.95), (0, 0.22, 1.2), 0.01, "wood_dark", e)
    return e


def siege_workshop(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. A big open timber shed under a tiled roof at the back, a
    traction trebuchet half built in the yard, timber, wheels, a sawhorse,
    stone shot and rope."""
    r = empty("siege_workshop", loc, rot_z, parent)
    pad(r, 2, (0.6, 0.47, 0.32))
    z = 0.03
    # an open shed of plain timber posts (no walls) at the back
    x0, x1, y0, y1, h = -0.86, 0.4, 0.34, 0.86, 0.92
    pm = mat(TIMBER, rough=0.6)
    for x in (x0 + 0.06, (x0 + x1) / 2):
        cyl(0.05, h, (x, y1 - 0.05, z + h / 2), pm, parent=r, segs=8)
    cyl(0.05, h, (x0 + 0.06, (y0 + y1) / 2, z + h / 2), pm, parent=r, segs=8)
    for x in (x0 + 0.06, (x0 + x1) / 2, x1 - 0.05):
        cyl(0.05, h, (x, y0 + 0.05, z + h / 2), pm, parent=r, segs=8)
    for y in ((y0 + y1) / 2, y1 - 0.05):
        cyl(0.05, h, (x1 - 0.05, y, z + h / 2), pm, parent=r, segs=8)
    box((x1 - x0 + 0.1, 0.1, 0.1), ((x0 + x1) / 2, y0 + 0.05, z + h - 0.05), mat(VERM_DK), parent=r)
    box((0.1, y1 - y0 + 0.1, 0.1), (x1 - 0.05, (y0 + y1) / 2, z + h - 0.05), mat(VERM_DK), parent=r)
    roof(r, ((x0 + x1) / 2, (y0 + y1) / 2), z + h, x1 - x0, y1 - y0, 0.3, over=0.12, lift=0.1)
    valance(r, (x0, y0 - 0.03, 0), (x1, y0 - 0.03, 0), z + h - 0.1, player, 0.14)
    yb = y1 - 0.16
    box((0.7, 0.22, 0.04), (-0.35, yb - 0.06, z + 0.42), "cedar", parent=r)
    for x in (-0.66, -0.04):
        box((0.05, 0.2, 0.4), (x, yb - 0.06, z + 0.2), "wood_dark", parent=r)
    g.wheel((0.22, yb - 0.04, z + 0.3), r, 0.28, rot=(pi / 2 - 0.15, 0, 0))
    for i in range(4):
        box((0.9, 0.12, 0.025), (-0.38, 0.32 + i * 0.002, z + 0.013 + i * 0.026), ("cedar", "wood")[i % 2],
            rot=(0, 0, 0.03 * i), parent=r)
    trebuchet((0.24, -0.06, z), pi / 4, r, player, done=False).scale = (1.3, 1.3, 1.3)
    g.timber_stack((-0.72, -0.62, z), 0.0, r, (3, 2, 1), 0.5, 0.05)
    for i in range(5):
        a = i * 1.25
        d = 0.0 if i == 0 else 0.1
        sphere(0.065, (0.78 + d * cos(a), 0.55 + d * sin(a), z + 0.065 + (0.09 if i == 0 else 0)), "rock", parent=r,
               segs=10)
    g.wheel((-0.3, -0.8, z + 0.03), r, 0.2, rot=(0, 0, 0))
    streamer((-0.92, 0.1, z), 1.5, player, r, seed=2)
    return r


# ------------------------------------------------------ government centre


def que(parent, x, y, z, w=0.3, h=1.1, player=BLUE):
    """A que: a tall rammed-earth gate pillar with a little tiled roof and a
    player-colour hanging."""
    box((w + 0.06, w + 0.06, 0.08), (x, y, z + 0.04), stone(STONE_DK, 10.0), parent=parent)
    box((w, w, h), (x, y, z + h / 2), layered(), parent=parent, bevel=0.008)
    box((w + 0.02, w + 0.02, 0.05), (x, y, z + h - 0.12), mat(VERMILION), parent=parent)
    box((w + 0.04, w + 0.04, 0.04), (x, y, z + h - 0.02), mat(TEAL), parent=parent)
    roof(parent, (x, y), z + h, w, w, 0.2, over=0.1, ridge=0, lift=0.08, drop=0.03)
    cloth(w * 0.7, h * 0.42, mat(player, rough=0.7), (x, y - w / 2 - 0.01, z + h - 0.18), 0, parent, hang=True,
          wave=0.006, nx=3)
    cloth(w * 0.7, h * 0.42, mat(player, rough=0.7), (x + w / 2 + 0.01, y, z + h - 0.18), pi / 2, parent, hang=True,
          wave=0.006, nx=3)


def government_centre(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """2 x 2. Long and low: a single-eaved gallery hall the width of the
    plot under a hip-and-gable roof, on a high rammed-earth terrace up a
    broad stair with a white balustrade; two que pillars and a petition
    drum at the stair's foot, the owner's valances and flag."""
    r = empty("government_centre", loc, rot_z, parent)
    pad(r, 2)
    z = 0.03
    tx0, tx1, ty0, ty1, th = -0.92, 0.92, -0.12, 0.92, 0.42
    box((tx1 - tx0, ty1 - ty0, th), ((tx0 + tx1) / 2, (ty0 + ty1) / 2, z + th / 2), layered(), parent=r, bevel=0.01)
    box((tx1 - tx0 + 0.04, ty1 - ty0 + 0.04, 0.05), ((tx0 + tx1) / 2, (ty0 + ty1) / 2, z + th - 0.02), mat(STONE),
        parent=r, bevel=0.01)
    box((tx1 - tx0 + 0.05, ty1 - ty0 + 0.05, 0.08), ((tx0 + tx1) / 2, (ty0 + ty1) / 2, z + 0.04), stone(STONE_DK, 8.0),
        parent=r)
    n = 5
    for i in range(n):
        hh = th * (n - i) / (n + 1)
        box((0.8, 0.1, hh), (0, ty0 - 0.05 - i * 0.1, z + hh / 2), mat(STONE, rough=0.85), parent=r, bevel=0.006)
    for sx in (-1, 1):
        box((0.08, 0.5, th + 0.06), (sx * 0.44, ty0 - 0.25, z + (th + 0.06) / 2), mat(STONE), parent=r)
    zp = z + th
    white = mat((0.94, 0.92, 0.88))
    for x in [tx0 + 0.06 + i * 0.12 for i in range(5)] + [0.5 + i * 0.12 for i in range(4)]:
        box((0.04, 0.04, 0.14), (x, ty0 + 0.04, zp + 0.07), white, parent=r)
    for xa, xb in ((tx0 + 0.04, -0.42), (0.48, tx1 - 0.04)):
        box((xb - xa, 0.035, 0.03), ((xa + xb) / 2, ty0 + 0.04, zp + 0.13), white, parent=r)
    # the gallery: nine bays, open front with lattice doors, one roof
    x0, x1, y0, y1, h = -0.86, 0.86, 0.3, 0.84, 0.66
    hall(r, x0, x1, y0, y1, zp, h, bays=6)
    zt = zp + h + 0.18
    roof(r, (0, (y0 + y1) / 2), zt, x1 - x0, y1 - y0, 0.3, over=0.18, ridge=(x1 - x0) / 2 + 0.08, lift=0.12)
    door((0, y0, zp), 0, r, w=0.4, h=0.56)
    for x in (-0.6, -0.32, 0.32, 0.6):
        lattice((x, y0, zp + 0.36), 0, r, 0.2, 0.36, n=(3, 4))
    lattice((x1, (y0 + y1) / 2, zp + 0.36), pi / 2, r, 0.22, 0.32)
    box((0.3, 0.03, 0.14), (0, y0 - 0.12, zp + h + 0.02), mat(LACQUER), parent=r)
    box((0.24, 0.035, 0.09), (0, y0 - 0.125, zp + h + 0.02), gold(), parent=r)
    valance(r, (x0, y0 - 0.07, 0), (-0.22, y0 - 0.07, 0), zp + h - 0.08, player, 0.14)
    valance(r, (0.22, y0 - 0.07, 0), (x1, y0 - 0.07, 0), zp + h - 0.08, player, 0.14)
    valance(r, (x1 + 0.07, y0, 0), (x1 + 0.07, y1, 0), zp + h - 0.08, player, 0.14, rz=pi / 2)
    for sx in (-1, 1):
        que(r, sx * 0.74, -0.7, z, 0.26, 1.0, player)
    drum((0.3, -0.82, z), r, 0.55, 0.0, player)
    streamer((-0.32, -0.88, z), 1.45, player, r, w=0.2, bh=0.6, seed=4)
    flag((0.84, 0.0, zp), 1.1, player, r, w=0.5, fh=0.3, seed=6)
    return r


# ------------------------------------------------------------------- farm


def farm(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, ripe=True):
    """2 x 2. Four rice paddies between grassy earth bunds, flooded, the rice
    in rows (golden when `ripe`, bright green shoots when not), a small
    tiled field shelter on the far corner, a scarecrow in the owner's
    colours under a straw hat, sheaves and a basket on the near bund."""
    r = empty("farm", loc, rot_z, parent)
    box((1.94, 1.94, 0.04), (0, 0, 0.01), noisy("earth", 0.14, 4.0, 1.0), parent=r, bevel=0.015)
    bund = noisy((0.46, 0.56, 0.26), 0.15, 6.0, 1.0)
    bh = 0.1
    for y in (-0.92, 0.92):
        box((1.9, 0.12, bh), (0, y, bh / 2), bund, parent=r, bevel=0.03)
    for x in (-0.92, 0.92):
        box((0.121, 1.72, bh - 0.004), (x, 0, (bh - 0.004) / 2), bund, parent=r, bevel=0.03)
    box((1.72, 0.1, bh - 0.008), (0, 0.0, (bh - 0.008) / 2), bund, parent=r, bevel=0.03)
    box((0.1, 1.72, bh - 0.012), (0.0, 0, (bh - 0.012) / 2), bund, parent=r, bevel=0.03)
    wm = mat(WATER, rough=0.08)
    for cx in (-0.46, 0.46):
        for cy in (-0.46, 0.46):
            box((0.78, 0.78, 0.03), (cx, cy, 0.048), wm, parent=r)
    rnd = random.Random(7)
    if ripe:
        ms = [mat((0.84, 0.74, 0.30), rough=0.85), mat((0.76, 0.70, 0.28), rough=0.85), mat((0.62, 0.64, 0.26), rough=0.85)]
    else:
        ms = [mat((0.40, 0.70, 0.22), rough=0.85), mat((0.32, 0.62, 0.20), rough=0.85), mat((0.48, 0.74, 0.26), rough=0.85)]
    pts = [[], [], []]
    for cx in (-0.46, 0.46):
        for cy in (-0.46, 0.46):
            for i in range(7):
                for k in range(7):
                    x, y = cx - 0.32 + i * 0.107, cy - 0.32 + k * 0.107
                    if x < -0.5 and y > 0.42:      # the shelter's corner
                        continue
                    hh = rnd.uniform(0.17, 0.22) if ripe else rnd.uniform(0.09, 0.12)
                    pts[rnd.randrange(3)].append(((x + rnd.uniform(-0.01, 0.01), y + rnd.uniform(-0.01, 0.01),
                                                   0.06 + hh * 0.5), (0.85, 0.85, hh / 0.08)))
    for k in range(3):
        g.clumps(pts[k], 0.04, ms[k], r, seed=k)
    if ripe:   # drooping heads
        hm = mat((0.92, 0.80, 0.34), rough=0.8)
        g.clumps([((p[0] + 0.02, p[1] - 0.01, p[2] + sc[2] * 0.035), (0.7, 0.5, 0.5)) for p, sc in pts[0] + pts[1]],
                 0.04, hm, r, seed=5)
    # the field shelter on the far corner
    sx, sy = -0.66, 0.66
    box((0.44, 0.44, 0.06), (sx, sy, 0.1 + 0.03), stone(STONE_DK, 10.0), parent=r)
    pavilion(r, sx, sy, 0.16, 0.34, 0.34, 0.5, 0.22, over=0.07, ridge=0, lift=0.08, posts=(0.45, 0.3, 0.2), r=0.03)
    jar((sx - 0.06, sy + 0.04, 0.16), r, 0.6)
    straw_hat((sx + 0.08, sy - 0.06, 0.16), r, 0.7)
    # scarecrow
    qx, qy = 0.46, 0.46
    rod((qx, qy, 0.0), (qx, qy, 0.86), 0.018, "wood_dark", r)
    rod((qx - 0.24, qy, 0.64), (qx + 0.24, qy, 0.64), 0.016, "wood_dark", r)
    cloth(0.34, 0.36, mat(player, rough=0.75), (qx, qy - 0.02, 0.68), 0, r, hang=True, wave=0.03, nx=5)
    cloth(0.34, 0.36, mat(player, rough=0.75), (qx, qy + 0.02, 0.68), pi, r, hang=True, wave=0.03, nx=5)
    sphere(0.075, (qx, qy, 0.8), "linen", parent=r, segs=8)
    straw_hat((qx, qy, 0.84), r, 1.0)
    # on the near bund: sheaves (ripe), a basket, a hoe
    if ripe:
        for i, x in enumerate((-0.6, -0.3)):
            e = empty("sheaf", (x, -0.92, bh), i * 0.7, r)
            cyl(0.05, 0.2, (0, 0, 0.1), mat((0.74, 0.66, 0.3)), parent=e, segs=8, r_top=0.035)
            blob(0.06, (0, 0, 0.24), ms[0], (1, 1, 1.3), e, i, 0.3, 1)
            cyl(0.045, 0.02, (0, 0, 0.12), "thatch", parent=e, segs=8)
    g.basket((0.3, -0.92, bh), r, 0.09, 1, 3)
    rod((0.55, -0.9, bh + 0.01), (0.85, -0.94, bh + 0.02), 0.01, "wood", r)
    box((0.03, 0.08, 0.06), (0.86, -0.94, bh + 0.03), "iron", parent=r)
    return r


# ------------------------------------------------------------------- dock


def junk(loc, rz=0.0, parent=None, player=BLUE, L=1.3):
    """A river junk along local X: a flat-bottomed hull with a square raised
    stern, a little tiled deckhouse, a battened lug sail in player colour."""
    e = empty("junk", loc, rz, parent)
    n = 9
    hull = []
    for i in range(n + 1):
        t = i / n
        x = (t - 0.5) * L
        w = 0.22 * (0.55 + 0.45 * sin(pi * min(1.0, 0.15 + 0.85 * t)) ** 0.5)
        zt = 0.17 + 0.16 * max(0.0, (0.35 - t) / 0.35) ** 1.6 + 0.05 * max(0.0, (t - 0.8) / 0.2)
        hull.append((x, w, zt))
    vs, fs = [], []
    for x, w, zt in hull:
        vs += [(x, -w, zt), (x, -w * 0.85, 0.02), (x, w * 0.85, 0.02), (x, w, zt)]
    for i in range(n):
        a, b = i * 4, (i + 1) * 4
        for k in range(3):
            fs.append((a + k, b + k, b + k + 1, a + k + 1))
    fs.append((0, 1, 2, 3))
    m = 4 * n
    fs.append((m + 3, m + 2, m + 1, m))
    lib.mesh(vs, fs, mat((0.30, 0.20, 0.13)), parent=e, smooth=False, name="hull")
    for sg in (-1, 1):
        pts = [(x, sg * (w + 0.004), zt - 0.035) for x, w, zt in hull]
        for p, q in zip(pts, pts[1:]):
            rod(p, q, 0.014, mat(VERMILION, rough=0.5), e, 5)
    box((L * 0.9, 0.36, 0.02), (0.02, 0, 0.16), "cedar", parent=e)
    sphere(0.03, (L * 0.44, -0.16, 0.19), "white", parent=e, segs=6)
    sphere(0.016, (L * 0.45, -0.17, 0.19), "black", parent=e, segs=6)
    # deckhouse aft with a tiled roof
    box((0.3, 0.3, 0.2), (-L * 0.28, 0, 0.27), mat((0.5, 0.34, 0.2)), parent=e)
    roof(e, (-L * 0.28, 0), 0.37, 0.3, 0.3, 0.12, over=0.06, ridge=0.08, lift=0.05, ornaments=False)
    # mast and battened sail (faces local +/-Y)
    mx = 0.08
    rod((mx, 0, 0.16), (mx, 0, 1.25), 0.02, "wood_dark", e)
    pm = mat(player, rough=0.7)
    sv = [(mx - 0.32, 0.03, 0.35), (mx + 0.26, 0.03, 0.4), (mx + 0.2, 0.03, 1.18), (mx - 0.24, 0.03, 1.12)]
    lib.mesh(sv, [(0, 1, 2, 3)], pm, parent=e)
    lib.mesh([(p[0], -0.03, p[2]) for p in sv], [(0, 1, 2, 3)], pm, parent=e)
    for k in range(6):
        f = k / 5
        xa, za = mx - 0.32 + 0.08 * f, 0.35 + 0.77 * f
        xb, zb = mx + 0.26 - 0.06 * f, 0.4 + 0.78 * f
        rod((xa, 0, za), (xb, 0, zb), 0.012, mat(LACQUER), e, 4)
    rod((mx + 0.2, 0, 1.18), (L * 0.46, 0, 0.3), 0.004, "wool", e, 4)
    rod((mx - 0.32, 0, 0.35), (-L * 0.45, 0, 0.42), 0.004, "wool", e, 4)
    box((0.05, 0.02, 0.18), (-L / 2 - 0.02, 0, 0.18), "wood_dark", parent=e)
    return e


def dock(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """3 x 3, all of it over water (none modelled; open air under the deck):
    a landing stage at the back, two piers round a slip under a tiled boat
    shed with a junk in it, nets, jars, sacks, a crane, lanterns."""
    r = empty("dock", loc, rot_z, parent)
    zd = 0.26
    E = 1.44
    g._deck(r, -E, E, 0.5, E, zd, "x", 1)
    g._deck(r, -E, -0.62, -E, 0.5, zd, "y", 2)
    g._deck(r, 0.58, E, -E, 0.5, zd, "y", 3)
    # the boat shed over the slip, ridge along Y
    sx0, sx1, sy0, sy1 = -0.7, 0.66, -0.12, 1.1
    pm = mat(VERMILION, rough=0.45)
    for y in (sy0, (sy0 + sy1) / 2, sy1):
        for x in (sx0, sx1):
            cyl(0.05, 1.2, (x, y, zd + 0.6), pm, parent=r, segs=8)
    for x in (sx0, sx1):
        box((0.1, sy1 - sy0 + 0.12, 0.1), (x, (sy0 + sy1) / 2, zd + 1.2), mat(VERM_DK), parent=r)
    for y in (sy0, sy1):
        box((sx1 - sx0 + 0.12, 0.1, 0.1), ((sx0 + sx1) / 2, y, zd + 1.2), mat(VERM_DK), parent=r)
    box((sx1 - sx0 + 0.14, 0.11, 0.035), ((sx0 + sx1) / 2, sy0, zd + 1.13), mat(TEAL), parent=r)
    e = empty("rot", ((sx0 + sx1) / 2, (sy0 + sy1) / 2, 0), pi / 2, r)
    roof(e, (0, 0), zd + 1.25, sy1 - sy0, sx1 - sx0, 0.5, over=0.14, lift=0.16, ridge=(sy1 - sy0) / 2 - 0.05)
    valance(r, (sx0, sy0 - 0.06, 0), (sx1, sy0 - 0.06, 0), zd + 1.15, player, 0.16)
    for x in (sx0 + 0.15, sx1 - 0.15):
        lantern((x, sy0 - 0.08, zd + 1.0), r, 0.9, 0.06)
    junk((-0.02, -0.68, 0.0), pi / 2 + 0.04, r, player, 1.3)
    # the landing stage
    z = zd
    g.crate((-1.15, 1.1, z), 0.2, r, 0.28)
    g.crate((-1.15, 1.1, z + 0.22), 0.6, r, 0.22)
    for i in range(4):
        jar((-1.25 + i * 0.14, 0.74 + (i % 2) * 0.06, z), r, 0.8, colour=((0.42, 0.3, 0.22), (0.3, 0.46, 0.4))[i % 2])
    g.sack_pile((1.0, 1.1, z), r, seed=3, n=(3, 2), s=0.1)
    silk_bolts((0.95, 0.72, z), r, 3, 2)
    # crane on the right pier
    kx, ky = 1.2, 0.2
    box((0.18, 0.18, 0.08), (kx, ky, z + 0.04), "wood_dark", parent=r)
    rod((kx, ky, z), (kx, ky, z + 1.25), 0.035, "wood_dark", r)
    rod((kx, ky, z + 1.2), (kx - 0.55, ky - 0.4, z + 1.35), 0.025, "wood", r)
    rod((kx, ky, z + 0.7), (kx - 0.3, ky - 0.22, z + 1.24), 0.018, "wood", r)
    rod((kx - 0.55, ky - 0.4, z + 1.35), (kx - 0.55, ky - 0.4, z + 0.6), 0.004, "wool", r, 4)
    g.crate((kx - 0.55, ky - 0.4, z + 0.42), 0.4, r, 0.22)
    # nets on the left pier, fish baskets, bollards, standards on the pier heads
    g.net_rack((-1.03, -0.45, z), pi / 2, r, 0.75, 0.82)
    g.net_rack((-1.03, -1.05, z), pi / 2, r, 0.5, 0.7)
    for i, (x, y) in enumerate(((-1.25, 0.15), (-0.85, 0.2))):
        g.basket((x, y, z), r, 0.1, 0, i)
    for (x, y) in ((-0.68, -1.36), (0.64, -1.36), (-0.68, 0.0), (0.64, 0.0)):
        cyl(0.05, 0.2, (x, y, z + 0.1), "wood_dark", parent=r, segs=8)
        sphere(0.05, (x, y, z + 0.2), "wood_dark", parent=r, segs=8)
    for k in range(3):
        cyl(0.1 - k * 0.022, 0.03, (1.15, -0.6, z + 0.015 + k * 0.03), "wool", parent=r, segs=12)
    streamer((-1.36, -1.36, z), 1.4, player, r, w=0.2, bh=0.56, seed=1)
    streamer((1.36, -1.36, z), 1.4, player, r, w=0.2, bh=0.56, seed=2)
    g.crate((1.0, -1.15, z), 0.4, r, 0.24, fruit=3, seed=4)
    jar((1.22, -0.95, z), r, 0.8)
    for t in (-0.1, 0.1):
        rod((-0.6, -1.0 + t, 0.0), (-0.6, -1.0 + t, z + 0.25), 0.012, "wood", r)
    for k in range(3):
        rod((-0.6, -1.1, 0.06 + k * 0.08), (-0.6, -0.9, 0.06 + k * 0.08), 0.008, "wood", r)
    return r


# ----------------------------------------------------------------- wonder


def balustrade(parent, p0, p1, z, h=0.16, m=None, step=0.16):
    m = m or mat((0.94, 0.92, 0.88), rough=0.7)
    a, b = Vector(p0), Vector(p1)
    d = b - a
    n = max(1, int(d.length / step + 0.5))
    for i in range(n + 1):
        p = a + d * (i / n)
        box((0.05, 0.05, h), (p.x, p.y, z + h / 2), m, parent=parent)
        sphere(0.03, (p.x, p.y, z + h + 0.015), m, parent=parent, segs=6)
    e = empty("rail", ((a.x + b.x) / 2, (a.y + b.y) / 2, z), atan2(d.y, d.x), parent)
    box((d.length, 0.03, 0.03), (0, 0, h * 0.85), m, parent=e)
    box((d.length, 0.025, 0.025), (0, 0, h * 0.3), m, parent=e)


def wonder(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """5 x 5. A palace tower-hall of three roofs on a high three-tier
    terrace with white balustrades and a grand stair; before it a court with
    que pillars, bell and drum pavilions, a giant ding, lions and standards."""
    r = empty("wonder", loc, rot_z, parent)
    box((4.92, 4.92, 0.08), (0, 0, 0.04), stone((0.78, 0.76, 0.7), 2.4), parent=r, bevel=0.012)
    z = 0.08
    # the terrace: three tiers, rammed earth faced with stone, toward the back
    tiers = ((3.9, 2.9, 0.42, 0.75), (3.3, 2.4, 0.38, 0.85), (2.8, 2.0, 0.34, 0.95))
    zz = z
    for i, (sx, sy, h, cy) in enumerate(tiers):
        box((sx, sy, h), (0, cy, zz + h / 2), layered() if i == 1 else stone(STONE, 4.0), parent=r, bevel=0.012)
        box((sx + 0.04, sy + 0.04, 0.05), (0, cy, zz + h - 0.02), mat(STONE), parent=r, bevel=0.01)
        zz += h
        y0 = cy - sy / 2
        balustrade(r, (-sx / 2 + 0.06, y0 + 0.05), (-0.42, y0 + 0.05), zz)
        balustrade(r, (0.42, y0 + 0.05), (sx / 2 - 0.06, y0 + 0.05), zz)
        balustrade(r, (sx / 2 - 0.05, y0 + 0.06), (sx / 2 - 0.05, cy + sy / 2 - 0.06), zz)
    # the grand stair up the middle, with a carved ramp
    ytop = tiers[0][3] - tiers[0][1] / 2
    total = zz - z
    n = 14
    run = 1.25
    for i in range(n):
        hh = total * (n - i) / (n + 1)
        box((0.74, run / n + 0.002, hh), (0, ytop - 0.02 - (i + 0.5) * run / n + 0.36 * (total - hh) / total * 0,
                                           z + hh / 2), mat(STONE, rough=0.85), parent=r, bevel=0.004)
    box((0.26, run, 0.03), (0, ytop - run / 2 - 0.02, z + total / 2 + 0.02), mat((0.94, 0.92, 0.88)),
        rot=(atan2(total, run), 0, 0), parent=r)
    for sx in (-1, 1):
        box((0.08, run + 0.04, 0.08), (sx * 0.4, ytop - run / 2, z + total / 2 + 0.03), mat((0.94, 0.92, 0.88)),
            rot=(atan2(total, run), 0, 0), parent=r)
    zp = zz
    cy = tiers[2][3]
    # the hall: three storeys, each under its own sweeping roof
    x0, x1, y0, y1, h = -1.12, 1.12, cy - 0.62, cy + 0.62, 1.0
    hall(r, x0, x1, y0, y1, zp, h, bays=6)
    zt = zp + h + 0.18
    roof(r, (0, cy), zt, x1 - x0, y1 - y0, 0.34, over=0.26, lift=0.22)
    door((0, y0, zp), 0, r, w=0.56, h=0.82)
    for x in (-0.8, -0.45, 0.45, 0.8):
        lattice((x, y0, zp + 0.5), 0, r, 0.24, 0.44, n=(3, 4))
    for y in (cy - 0.3, cy + 0.3):
        lattice((x1, y, zp + 0.5), pi / 2, r, 0.24, 0.44, n=(3, 4))
    valance(r, (x0, y0 - 0.08, 0), (x1, y0 - 0.08, 0), zp + h - 0.08, player, 0.18)
    valance(r, (x1 + 0.08, y0, 0), (x1 + 0.08, y1, 0), zp + h - 0.08, player, 0.18, rz=pi / 2)
    for x in (-0.5, 0.5):
        lantern((x, y0 - 0.14, zp + h - 0.12), r, 1.2, 0.1)
    u = [(-0.8, 0.8, cy - 0.42, cy + 0.42, 0.62), (-0.5, 0.5, cy - 0.28, cy + 0.28, 0.5)]
    zb = zt + 0.1
    for j, (a0, a1, b0, b1, hh) in enumerate(u):
        hall(r, a0, a1, b0, b1, zb, hh, bays=4 - j)
        for x in ((-0.4, 0.0, 0.4) if j == 0 else (-0.2, 0.2)):
            lattice((x, b0, zb + hh * 0.55), 0, r, 0.2, hh * 0.45)
        lattice((a1, cy, zb + hh * 0.55), pi / 2, r, 0.2, hh * 0.45)
        zr = zb + hh + 0.18
        last = j == len(u) - 1
        roof(r, (0, cy), zr, a1 - a0, b1 - b0, 0.62 if last else 0.3, over=0.24, lift=0.22, finial=last,
             ridge=0.12 if last else None)
        zb = zr + 0.1
    box((0.4, 0.03, 0.2), (0, u[0][2] - 0.02, zt + 0.42), mat(LACQUER), parent=r)
    box((0.32, 0.035, 0.13), (0, u[0][2] - 0.02, zt + 0.42), gold(), parent=r)
    flag((0.0, cy + 0.1, zr + 0.62), 1.0, player, r, w=0.9, fh=0.5, seed=7)
    # the court before the terrace
    ding((0, -1.72, z), r, 1.6)
    for sx in (-1, 1):
        que(r, sx * 0.62, -2.15, z, 0.32, 1.3, player)
        stone_lion((sx * 0.62, -1.55, z), 0, r, 1.15)
        for k, y in enumerate((-2.25, -1.0)):
            streamer((sx * 2.25, y, z), 2.0, player, r, w=0.26, bh=0.9, seed=k + sx * 3)
        pine((sx * 2.1, 1.9, z), r, 1.1, seed=sx + 4)
    # bell pavilion (left) and drum pavilion (right)
    box((0.8, 0.8, 0.12), (-1.65, -1.25, z + 0.06), stone(STONE, 6.0), parent=r)
    pavilion(r, -1.65, -1.25, z + 0.12, 0.6, 0.6, 1.1, 0.4, over=0.14, ridge=0, lift=0.14)
    bell((-1.65, -1.25, z + 0.12), r, 0.95, 0.0)
    box((0.8, 0.8, 0.12), (1.65, -1.25, z + 0.06), stone(STONE, 6.0), parent=r)
    pavilion(r, 1.65, -1.25, z + 0.12, 0.6, 0.6, 1.1, 0.4, over=0.14, ridge=0, lift=0.14)
    drum((1.65, -1.25, z + 0.12), r, 0.95, 0.0, player)
    bamboo((-2.05, -0.3, z), r, 7, 1.5, seed=12)
    bamboo((2.05, -0.3, z), r, 7, 1.5, seed=13)
    return r


# ------------------------------------------------------- walls and a gate
#
# As greek.py's: a wall tile is its post (idle) plus an arm toward each
# neighbour (frame k of "arm" reaches toward WALL_DIRECTIONS[k], game +y is
# model -Y), ending at the tile's edge or corner where the neighbour's arm
# meets it; a gate stands across line l (docs/look/buildings.md).

RAM_PIER, RAM_W, RAM_H, RAM_POST_H = 0.16, 0.3, 0.98, 1.04


def stone_wall(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, arm=None, joins=0, post=True):
    """1 x 1. A rammed-earth wall on a stone footing under a tiled coping:
    a slim square pier a little proud of the wall with a low tiled cap and
    player-colour hangings; arms of wall toward each neighbour in `joins`.
    `arm` = k: arm k alone."""
    if arm is not None:
        joins, post = 1 << arm, False
    r = empty("stone_wall", loc, rot_z, parent)
    earth = layered(RAMMED, 11.0, 0.12)
    P, W, H = RAM_PIER, RAM_W, RAM_H
    if post:
        box((P * 2 + 0.1, P * 2 + 0.1, 0.12), (0, 0, 0.06), stone(STONE_DK, 9.0), parent=r, bevel=0.012)
        box((P * 2 + 0.02, P * 2 + 0.02, RAM_POST_H), (0, 0, RAM_POST_H / 2), earth, parent=r, bevel=0.01)
        box((P * 2 + 0.06, P * 2 + 0.06, 0.04), (0, 0, RAM_POST_H - 0.05), mat(VERMILION, rough=0.5), parent=r)
        lib.pyramid(P * 2 + 0.14, 0.1, (0, 0, RAM_POST_H - 0.01), mat(TILE_DK, rough=0.7), parent=r)
        sphere(0.03, (0, 0, RAM_POST_H + 0.1), mat(RIDGE), parent=r, segs=6)
        for rz, (x, y) in ((0, (0, -P - 0.032)), (pi / 2, (P + 0.032, 0))):
            f = face((x, y, RAM_POST_H - 0.09), rz, r)
            cloth(0.2, 0.4, mat(player, rough=0.7), (0, 0, 0), 0, f, hang=True, wave=0.008, nx=3)
            box((0.26, 0.03, 0.03), (0, 0.004, 0.01), mat(LACQUER), parent=f)
    for k in range(8):
        if not joins & (1 << k):
            continue
        _, ang, reach = wall_dir(k)
        dx, dy = WALL_DIRECTIONS[k]
        # the footing starts a hair further out than the wall so no end face of
        # it lies in the wall's (coplanar faces render black)
        ft = plan_prism(_arm_outline(k, P, W, 0.045, 0.008), 0.0, 0.11, stone(STONE_DK, 9.0), r, "arm_base")
        u = wall_dir(k)[0]
        ft.location = (u[0] * 0.005, u[1] * 0.005, 0)
        plan_prism(_arm_outline(k, P, W), 0.0, H, earth, r, "arm")
        s0 = P * 0.6
        a, b = _along(k, s0), _along(k, reach - 0.003)
        tiled_coping(r, a, b, H, W, 0.12, 0.05)
    return r


def palisade_wall(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, arm=None, joins=0, post=True):
    """1 x 1. A bamboo stockade: a stout lacquered timber post with a tiled
    cap and a player-colour streamer; arms of close-set bamboo poles cut to
    points, lashed to two rails, on an earth berm."""
    if arm is not None:
        joins, post = 1 << arm, False
    r = empty("palisade_wall", loc, rot_z, parent)
    cane = [mat(BAMBOO_DRY, rough=0.55), mat((0.70, 0.66, 0.36), rough=0.55), mat((0.58, 0.62, 0.30), rough=0.55)]
    joint = mat(tint(BAMBOO_DRY, 0.6))
    tip = mat((0.9, 0.84, 0.6), rough=0.7)
    rnd = random.Random(11)
    if post:
        blob(0.2, (0, 0, 0.0), "earth", (1.1, 1.1, 0.35), r, 3, 0.2, 1)
        box((0.18, 0.18, 1.2), (0, 0, 0.6), mat((0.42, 0.28, 0.18), rough=0.8), parent=r, bevel=0.012)
        for zz in (0.35, 0.85):
            box((0.2, 0.2, 0.05), (0, 0, zz), mat(LACQUER), parent=r)
        lib.pyramid(0.3, 0.14, (0, 0, 1.2), mat(TILE_DK, rough=0.7), parent=r)
        sphere(0.03, (0, 0, 1.36), gold(), parent=r, segs=6)
        for rz, (x, y) in ((0, (0, -0.1)), (pi / 2, (0.1, 0))):
            f = face((x, y, 1.12), rz, r)
            cloth(0.15, 0.6, mat(player, rough=0.7), (0, 0, 0), 0, f, hang=True, wave=0.01, nx=3)
    for k in range(8):
        if not joins & (1 << k):
            continue
        _, ang, reach = wall_dir(k)
        dx, dy = WALL_DIRECTIONS[k]
        s0 = 0.14 if not (dx and dy) else 0.16
        n = max(2, int((reach - s0) / 0.074 + 0.5))
        step = (reach - s0) / n
        for i in range(n):
            s = s0 + (i + 0.5) * step
            p = _along(k, s, rnd.uniform(-0.01, 0.01))
            hh = rnd.uniform(0.95, 1.08)
            rr = 0.032
            cyl(rr, hh, (p[0], p[1], hh / 2), cane[rnd.randrange(3)], parent=r, segs=7)
            cyl(rr, 0.09, (p[0], p[1], hh + 0.045), tip, parent=r, segs=7, r_top=0.004)
            for zz in (0.3, 0.62):
                cyl(rr + 0.004, 0.02, (p[0], p[1], zz + rnd.uniform(-0.03, 0.03)), joint, parent=r, segs=7)
        for zz in (0.36, 0.78):
            a, b = _along(k, s0 - 0.05, 0.05), _along(k, reach - 0.002, 0.05)
            rod((a[0], a[1], zz), (b[0], b[1], zz), 0.022, mat((0.42, 0.28, 0.18)), r, 6)
            a, b = _along(k, s0 - 0.05, -0.05), _along(k, reach - 0.002, -0.05)
            rod((a[0], a[1], zz), (b[0], b[1], zz), 0.022, mat((0.42, 0.28, 0.18)), r, 6)
        blob(0.1, (*_along(k, (s0 + reach) / 2, 0.0), -0.02), "earth", (reach * 3.2, 1.6, 0.6), r, k, 0.15,
             1).rotation_euler = (0, 0, ang)
    return r


def gate(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None, line=0, open=False):
    """1 x 1. A gatehouse: two rammed-earth piers carry a timber gate-hall
    under a curled tiled roof; vermilion doors with gold studs, player
    hangings on the piers, a valance on the hall. `line` (0-3) is the
    direction its walls run; `open` swings the doors back."""
    r = empty("gate", loc, rot_z, parent)
    (ux, uy), ang, reach = wall_dir(line)
    e = empty("gate_line", (0, 0, 0), ang, r)
    earth = layered(RAMMED, 11.0, 0.12)
    T, D, PH = 0.15, 0.2, 1.05
    tx = reach - T
    for sx in (-1, 1):
        x = sx * tx
        box((T * 2 + 0.02, D * 2 + 0.06, 0.12), (x - sx * 0.012, 0, 0.06), stone(STONE_DK, 9.0), parent=e, bevel=0.012)
        box((T * 2, D * 2, PH), (x, 0, PH / 2), earth, parent=e, bevel=0.01)
        for sy in (-1, 1):
            f = face((x, sy * (D + 0.01), PH - 0.08), 0 if sy < 0 else pi, e)
            cloth(0.2, 0.52, mat(player, rough=0.7), (0, -0.01, 0), 0, f, hang=True, wave=0.01, nx=3)
            box((0.26, 0.03, 0.03), (0, 0.0, 0.01), mat(LACQUER), parent=f)
    ow = tx - T
    box((ow * 2 + 0.06, D * 2, 0.12), (0, 0, PH - 0.06), mat(VERM_DK, rough=0.6), parent=e)
    box((ow * 2 + 0.02, D * 2 + 0.02, 0.04), (0, 0, PH - 0.15), mat(TEAL, rough=0.6), parent=e)
    # the gate-hall on top
    zh = PH
    L = reach - 0.08
    box((2 * L, D * 2 + 0.06, 0.05), (0, 0, zh + 0.025), mat("cedar"), parent=e)
    pm = mat(VERMILION, rough=0.45)
    hh = 0.36
    for x in (-L + 0.05, 0.0, L - 0.05):
        for sy in (-1, 1):
            cyl(0.03, hh, (x, sy * (D - 0.03), zh + 0.05 + hh / 2), pm, parent=e, segs=8)
    box((2 * L - 0.1, D * 2 - 0.12, hh * 0.8), (0, 0, zh + 0.05 + hh * 0.4), noisy(PLASTER, 0.06, 5.0), parent=e)
    for sy in (-1, 1):
        box((2 * L - 0.04, 0.04, 0.04), (0, sy * D, zh + 0.16), pm, parent=e)
        lattice((-L / 2, sy * (D - 0.06), zh + 0.27), 0 if sy < 0 else pi, e, 0.16, 0.14)
        lattice((L / 2, sy * (D - 0.06), zh + 0.27), 0 if sy < 0 else pi, e, 0.16, 0.14)
    box((2 * L, D * 2 + 0.02, 0.05), (0, 0, zh + 0.05 + hh + 0.02), mat(VERM_DK), parent=e)
    over = 0.1
    roof(e, (0, 0), zh + 0.1 + hh, 2 * (reach - over) - 0.02, D * 2, 0.32, over=over, lift=0.12)
    valance(e, (-ow, -D - 0.03, 0), (ow, -D - 0.03, 0), PH - 0.12, player, 0.12)
    box((ow * 2, D * 2, 0.02), (0, 0, 0.01), stone(STONE_DK, 9.0), parent=e)
    lw = ow - 0.01
    dm = mat(VERMILION, rough=0.45)
    zl = PH - 0.12
    for sx in (-1, 1):
        f = empty("leaf", (sx * ow, 0.0, 0), 0, e)
        f.rotation_euler = (0, 0, (sx * 1.45) if open else 0.0)
        box((lw, 0.05, zl - 0.04), (-sx * lw / 2, 0, (zl - 0.04) / 2 + 0.01), dm, parent=f)
        for zz in (0.25, 0.5, 0.75):
            for k in range(3):
                for sy in (-1, 1):
                    sphere(0.016, (-sx * (0.05 + k * (lw - 0.1) / 2), sy * 0.03, zz), gold(), parent=f, segs=6)
    return r


def wall_layout(at, kind="stone_wall", origin=(0, 0, 0), player=BLUE, parent=None, open_gates=False):
    """Walls and gates on game tiles {(x, y): "wall" | "gate"}, joined as the
    game joins them (greek.wall_layout, with this module's pieces)."""
    fn = globals()[kind]
    for t, p in at.items():
        loc = (origin[0] + t[0], origin[1] - t[1], origin[2])
        if p == "gate":
            gate(loc, player=player, parent=parent, line=_gate_line(t, at), open=open_gates)
        else:
            fn(loc, player=player, parent=parent, joins=_wall_links(t, at))


def walls_demo(loc=(0, 0, 0), rot_z=0.0, player=BLUE, parent=None):
    """A short run of each wall with a corner, a diagonal and a gate."""
    r = empty("walls_demo", loc, rot_z, parent)
    at = {(0, 0): "wall", (1, 0): "gate", (2, 0): "wall", (2, 1): "wall", (3, 2): "wall", (0, -1): "wall"}
    wall_layout(at, "stone_wall", (-1.0, 2.0, 0), player, r)
    at = {(0, 0): "wall", (1, 0): "wall", (2, 0): "gate", (3, 0): "wall", (4, 1): "wall"}
    wall_layout(at, "palisade_wall", (-1.0, -1.0, 0), RED, r, open_gates=True)
    return r


# --------------------------------------------------------------- showcase


def showcase_all():
    """Every Asian building set together on plain ground, two players."""
    r = empty("asian_all")

    def at(u, v):       # u: screen-right, v: away from the camera
        return (u - v, u + v, 0)

    box((80, 80, 0.02), (0, 0, -0.01), noisy((0.42, 0.64, 0.24), 0.16, 0.5, 1.0), parent=r)
    wonder(at(-7.0, 7.0), parent=r)
    temple(at(-2.6, 6.4), parent=r)
    town_center(at(1.2, 6.6), parent=r)
    dock(at(5.4, 6.6), parent=r)
    for u, fn in ((-9.0, academy), (-6.0, government_centre), (-3.0, market), (0.0, storehouse),
                  (3.0, siege_workshop), (6.0, stable)):
        fn(at(u, 2.6), parent=r)
    archery_range(at(-9.0, -0.6), player=RED, parent=r)
    barracks(at(-6.0, -0.6), player=RED, parent=r)
    house(at(-3.0, -0.6), player=RED, parent=r, variant=0)
    farm(at(0.0, -0.6), player=RED, parent=r)
    farm(at(1.0, -1.6), player=RED, parent=r, ripe=False)
    watch_tower(at(3.4, -0.4), player=RED, parent=r)
    house(at(6.0, -0.6), player=RED, parent=r, variant=1)
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
    for i, (u, v, sc) in enumerate(((-11.2, 4.8, 1.1), (8.2, 4.4, 1.2), (8.6, -2.0, 1.0), (-1.2, 4.4, 0.9),
                                    (-11.2, -2.6, 1.0), (4.6, 1.0, 0.9))):
        pine(at(u, v), r, sc, i + 7)
    for i, (u, v) in enumerate(((-4.6, 4.6), (9.0, 1.4), (-10.6, 1.2))):
        bamboo(at(u, v), r, 7, 1.5, seed=i + 20)
    return r


def render_showcase_all(out, samples=32, res=(1800, 1125)):
    """/usr/bin/python3.13 -c 'import sys; sys.path.insert(0, "tools/look");
    import asian; asian.render_showcase_all("out.png")'"""
    lib.reset()
    showcase_all()
    c = (-1.0 - 1.9, -1.0 + 1.9, 0)
    lib.camera((c[0], c[1], 1.2), 22.5, res)
    lib.light()
    lib.render(out, samples)


# ------------------------------------------------- sets (tools/look/export_all.py)
_ARMS = {"arm": [{"arm": k} for k in range(8)]}
SETS = {
    "town_center_asian": dict(func="asian.town_center", cls="LargeBuilding", tiles=3, kwargs={}, fit=None),
    "house_asian": dict(func="asian.house", cls="MediumBuilding", tiles=2, kwargs={"variant": 0}, fit=None),
    "barracks_asian": dict(func="asian.barracks", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "temple_asian": dict(func="asian.temple", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "watch_tower_asian": dict(func="asian.watch_tower", cls="SmallBuilding", tiles=1, kwargs={}, fit=None),
    "academy_asian": dict(func="asian.academy", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "archery_range_asian": dict(func="asian.archery_range", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "government_centre_asian": dict(func="asian.government_centre", cls="MediumBuilding", tiles=2, kwargs={},
                                    fit=None),
    "market_asian": dict(func="asian.market", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "siege_workshop_asian": dict(func="asian.siege_workshop", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "stable_asian": dict(func="asian.stable", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "storehouse_asian": dict(func="asian.storehouse", cls="MediumBuilding", tiles=2, kwargs={}, fit=None),
    "farm_asian": dict(func="asian.farm", cls="MediumBuilding", tiles=2, kwargs={}, fit=None, stages="flat"),
    "dock_asian": dict(func="asian.dock", cls="LargeBuilding", tiles=3, kwargs={}, fit=None, stages="flat"),
    "wonder_asian": dict(func="asian.wonder", cls="Wonder", tiles=5, kwargs={}, fit=None),
    "stone_wall_asian": dict(func="asian.stone_wall", cls="SmallBuilding", tiles=1, kwargs={}, fit=None,
                             anims=_ARMS),
    "palisade_wall_asian": dict(func="asian.palisade_wall", cls="SmallBuilding", tiles=1, kwargs={}, fit=None,
                                anims=_ARMS),
    "gate_asian": dict(func="asian.gate", cls="SmallBuilding", tiles=1, kwargs={}, fit=None,
                       anims={"shut": [{"line": k, "open": False} for k in range(4)],
                              "open": [{"line": k, "open": True} for k in range(4)]}),
}

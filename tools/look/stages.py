"""Construction stages and rubble for any building (docs/look/buildings.md).

export.py builds a building's idle model and then, for each extra frame,
hands the scene to `apply()`, which turns it into:

    ("construction", 0)  the site: the model gone; the footprint pegged out
                         with string, a trench along the walls' line, a pile
                         of blocks and a stack of timber
    ("construction", 1)  the model cut off at 35% of its height, the cut
                         capped, in timber scaffolding, a few blocks by it
    ("construction", 2)  the same at 70%
    ("rubble", 0)        the model gone; a low heap on the footprint of
                         broken blocks, beams and roof tiles over a dust stain

All of it is made here from the model and three colours (`palette()`, from
the idle render), with no per-model work: a Greek ruin comes out white and
terracotta, an Egyptian one sandstone. Everything is deterministic, so the
export's two builds (grey and odd player colour) line up for the mask.
Whatever player-coloured cloth survives a cut keeps its mask; the site, the
scaffolding and the rubble take no player colour.
"""
import math
import random

import bpy  # noqa: F401  (before bmesh)
import bmesh
import numpy as np
from mathutils import Vector

import lib

CUTS = (0.35, 0.70)          # construction frames 1 and 2: share of the height kept
TIMBER = (0.62, 0.46, 0.28)  # fresh-cut scaffolding
EARTH = (0.40, 0.29, 0.19)   # a trench, a stain
STRING = (0.92, 0.88, 0.76)

DEFAULT_PALETTE = {"wall": lib.PAL["limestone"], "roof": lib.PAL["terracotta"],
                   "wood": lib.PAL["wood"]}


# ------------------------------------------------------------ the palette

def palette(rgba, mask=None):
    """The idle render's dominant colours: {"wall", "roof", "wood"} as sRGB
    0..1. `rgba` is the straight-alpha render (0..1), `mask` its player
    mask: only opaque, unplayered pixels count. The wall is the biggest
    cluster; the roof the biggest that differs from it clearly; wood is a
    fixed timber brown darkened toward the wall's darkest cluster."""
    a = rgba[..., 3]
    keep = a > 0.95
    if mask is not None:
        keep &= mask < 0.05
    px = rgba[..., :3][keep]
    if len(px) < 50:
        return dict(DEFAULT_PALETTE)
    # Where each pixel is up the picture: walls are low, roofs high.
    ys = np.nonzero(keep)[0].astype(float)
    ys = (ys - ys.min()) / max(1.0, ys.max() - ys.min())
    step = max(1, len(px) // 20000)
    px, ys = px[::step], ys[::step]
    lum = px @ np.array([0.299, 0.587, 0.114])
    ok = (lum > 0.12) & (lum < 0.97)
    if ok.sum() > 50:
        px, ys = px[ok], ys[ok]
    lum = px @ np.array([0.299, 0.587, 0.114])
    chroma = px / np.maximum(lum[:, None], 1e-3)
    feat = np.hstack([chroma * 0.6, lum[:, None] * 0.6])
    k = 5
    order = np.argsort(lum)
    cent = feat[order[np.linspace(0, len(px) - 1, k).astype(int)]]
    for _ in range(12):
        d = ((feat[:, None, :] - cent[None]) ** 2).sum(-1)
        lab = d.argmin(1)
        for j in range(k):
            if (lab == j).any():
                cent[j] = feat[lab == j].mean(0)
    counts = np.bincount(lab, minlength=k)
    colours = np.array([px[lab == j].mean(0) if counts[j] else np.zeros(3) for j in range(k)])
    # The wall: the commonest colour in the picture's lower half (seen from
    # above, a roof is the commonest overall); the roof: the commonest in
    # its upper part that is clearly not the wall.
    low = np.bincount(lab[ys > 0.5], minlength=k)
    high = np.bincount(lab[ys < 0.4], minlength=k)
    wall = colours[int(np.argmax(low))] if low.sum() else colours[int(np.argmax(counts))]
    ranked = list(np.argsort(-high)) + list(np.argsort(-counts))

    def far(c):
        cw = wall / max(wall.sum(), 1e-3)
        cc = c / max(c.sum(), 1e-3)
        return np.abs(cw - cc).sum() > 0.10 or abs(c.mean() - wall.mean()) > 0.25
    roof = next((colours[j] for j in ranked if counts[j] > 0.04 * len(px) and far(colours[j])),
                np.array(DEFAULT_PALETTE["roof"]))
    # The rendered colours are lit; as base colours they are lit again.
    soften = 0.92
    return {"wall": tuple(float(v) for v in np.clip(wall * soften, 0, 1)),
            "roof": tuple(float(v) for v in np.clip(roof * soften, 0, 1)),
            "wood": DEFAULT_PALETTE["wood"]}


# ------------------------------------------------------------ the model

def _model_objects():
    return [ob for ob in bpy.context.scene.objects
            if ob.type in ("MESH", "CURVE", "SURFACE", "META", "FONT")
            and not ob.get("export_ground")]


def _world_bounds(ob, dg):
    ev = ob.evaluated_get(dg)
    M = ev.matrix_world
    pts = np.array([tuple(M @ Vector(c)) for c in ev.bound_box])
    return pts.min(0), pts.max(0)


def measure(tiles):
    """The standing model's height (of its mass: thin poles and flags do
    not count) and its ground rectangle (x0, y0, x1, y1), inside the
    footprint."""
    dg = bpy.context.evaluated_depsgraph_get()
    t = tiles / 2
    height, lo, hi = 0.0, np.array([t, t]), np.array([-t, -t])
    for ob in _model_objects():
        a, b = _world_bounds(ob, dg)
        area = (b[0] - a[0]) * (b[1] - a[1])
        if area >= max(0.03, 0.01 * tiles * tiles):
            height = max(height, b[2])
        if a[2] < 0.15:
            lo = np.minimum(lo, a[:2])
            hi = np.maximum(hi, b[:2])
    if height <= 0:
        height = max((_world_bounds(ob, dg)[1][2] for ob in _model_objects()), default=1.0)
    if (hi < lo).any():
        lo, hi = np.array([-t * 0.8, -t * 0.8]), np.array([t * 0.8, t * 0.8])
    # The top surface seen from straight above, on a grid over the
    # footprint: a flag or a pole is hardly ever hit, a roof always.
    tops = []
    sc = bpy.context.scene
    for u in np.linspace(-t, t, 25):
        for v in np.linspace(-t, t, 25):
            hit, loc, *_ = sc.ray_cast(dg, Vector((u, v, 50.0)), Vector((0, 0, -1)))
            if hit and loc.z > 0.02:
                tops.append(loc.z)
    if len(tops) > 10:
        height = min(height, float(np.percentile(tops, 98)))
    lo = np.maximum(lo, -t)
    hi = np.minimum(hi, t)
    return height, (lo[0], lo[1], hi[0], hi[1])


def remove_model():
    for ob in _model_objects():
        bpy.data.objects.remove(ob, do_unlink=True)


def _facing(ob, dg):
    """A mesh's area-weighted mean of its faces' world normal z, and of
    its absolute value."""
    me = ob.evaluated_get(dg).data
    n = len(me.polygons)
    if not n:
        return None
    nz = np.empty(n * 3)
    me.polygons.foreach_get("normal", nz)
    area = np.empty(n)
    me.polygons.foreach_get("area", area)
    M = np.array(ob.matrix_world)[:3, :3]
    nw = nz.reshape(-1, 3) @ np.linalg.inv(M)
    nw /= np.maximum(np.linalg.norm(nw, axis=1, keepdims=True), 1e-9)
    tot = max(area.sum(), 1e-9)
    return (nw[:, 2] * area).sum() / tot, (np.abs(nw[:, 2]) * area).sum() / tot


def _flat_plate(ob):
    """Whether a mesh is a few faces all in one plane: open, one-sided."""
    me = ob.data
    if not me.polygons:
        return False
    n0 = me.polygons[0].normal
    return all(abs(p.normal.dot(n0)) > 0.999 for p in me.polygons)


def _roofish(ob, dg):
    """Whether a mesh is mostly faces that look up or down (a roof's slopes,
    its tiles, a ceiling), by area, rather than walls."""
    f = _facing(ob, dg)
    return f is not None and f[1] > 0.55


def _colours(ob):
    """The sRGB base colours of a mesh's materials, where they are plain."""
    out = []
    for m in getattr(ob.data, "materials", []) or []:
        if m is None or m.node_tree is None:
            continue
        b = next((n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
        if b is None:
            continue
        sock = b.inputs["Base Color"]
        if sock.is_linked:
            continue
        lin = np.array(sock.default_value[:3])
        out.append(np.where(lin <= 0.0031308, lin * 12.92, 1.055 * lin ** (1 / 2.4) - 0.055))
    return out


def cut_model(z, height, roof=None, cap=None):
    """Everything above world height `z` taken off, the cuts capped. A
    roof is never half there: what lies above 25% of the `height` and
    mostly looks up or down (roofs, ceilings) goes, and small things (plants, pots) straddling the
    cut go rather than be sliced. Roof-coloured parts (`roof`, sRGB) above
    25% go with the roof (antefixes, ridge tiles). The caps are made of
    `cap` (sRGB; the wall's colour), whatever the faces round them are."""
    cap_mat = lib.mat(cap or DEFAULT_PALETTE["wall"], rough=0.95)
    dg = bpy.context.evaluated_depsgraph_get()
    obs = _model_objects()
    plan = []
    for ob in obs:
        a, b = _world_bounds(ob, dg)
        small = (b - a).max() < 0.35
        if a[2] >= z - 1e-4:
            plan.append((ob, "drop"))
        elif ob.type == "MESH" and (a[2] >= 0.25 * height or b[2] > z + 1e-4) and (
                _roofish(ob, dg) or (roof is not None and any(
                    np.abs(c - np.array(roof)).max() < 0.18 for c in _colours(ob)))):
            # A roof (or its trim) above 25%, or crossing the cut: never
            # half a roof.
            plan.append((ob, "drop"))
        elif ob.type == "MESH" and (a[2] >= 0.25 * height or b[2] > z + 1e-4) and (
                len(ob.data.polygons) <= 4 and _flat_plate(ob)):
            # A one-sided plate (a gable's infill) shows its black back
            # once what stood in front of it is gone.
            plan.append((ob, "drop"))
        elif b[2] > z + 1e-4:
            plan.append((ob, "cut" if ob.type == "MESH" and not small else "drop"))
    for ob, what in plan:
        if what == "drop":
            bpy.data.objects.remove(ob, do_unlink=True)
            continue
        ev = ob.evaluated_get(dg)
        me = bpy.data.meshes.new_from_object(ev)     # modifiers applied, object space
        old = ob.data
        ob.modifiers.clear()
        ob.data = me
        if old.users == 0:
            bpy.data.meshes.remove(old)
        M = ob.matrix_world
        Minv = M.inverted()
        co = Minv @ Vector((0, 0, z))
        no = (M.to_3x3().transposed() @ Vector((0, 0, 1))).normalized()
        bm = bmesh.new()
        bm.from_mesh(me)
        res = bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:],
                                     plane_co=co, plane_no=no, clear_outer=True)
        cut = [e for e in res["geom_cut"] if isinstance(e, bmesh.types.BMEdge)
               and e.is_valid and e.is_boundary]
        if cut:
            # Scanfill: nested loops (a hollow wall's outer and inner face)
            # fill as a ring, not as two discs.
            made = bmesh.ops.triangle_fill(bm, use_beauty=True, use_dissolve=False, edges=cut)
            if cap_mat.name not in me.materials:
                me.materials.append(cap_mat)
            cap_index = list(me.materials).index(me.materials[cap_mat.name])
            N = M.to_3x3().inverted().transposed()
            for f in made["geom"]:
                if not isinstance(f, bmesh.types.BMFace):
                    continue
                f.normal_update()
                # A cap at the cut looks up.
                if (N @ f.normal).z < 0:
                    f.normal_flip()
                f.material_index = cap_index
                f.smooth = False
        bm.to_mesh(me)
        bm.free()
        if not me.polygons:
            bpy.data.objects.remove(ob, do_unlink=True)


# ------------------------------------------------------------ pieces

def _mats(colour, n, rnd, spread=0.08, rough=0.9):
    """`n` materials of `colour` jittered in brightness and warmth."""
    out = []
    for i in range(n):
        f = 1 + spread * (2 * i / max(1, n - 1) - 1)
        c = tuple(min(1.0, max(0.0, v * f + (0.01 if j == 0 else 0) * (i % 2)))
                  for j, v in enumerate(colour))
        out.append(lib.mat(c, rough=rough))
    return out


def flat(verts, faces, m, name="stage_flat"):
    """Flat faces on the ground, every one looking up."""
    bm = bmesh.new()
    vs = [bm.verts.new(p) for p in verts]
    for f in faces:
        face = bm.faces.new([vs[i] for i in f])
        face.normal_update()
        if face.normal.z < 0:
            face.normal_flip()
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    me.materials.append(m)
    ob = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(ob)
    return ob


def clip_below_ground(ob):
    """Takes off what of a mesh is under the ground (the shadow catcher
    does not hide it from the camera)."""
    bpy.context.view_layer.update()
    M = ob.matrix_world
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:],
                           plane_co=M.inverted() @ Vector((0, 0, 0.001)),
                           plane_no=-(M.to_3x3().transposed() @ Vector((0, 0, 1))).normalized(),
                           clear_outer=True)
    bm.to_mesh(ob.data)
    bm.free()


def _box(size, loc, m, rot=(0, 0, 0), name="stage"):
    return lib.box(size, loc=loc, m=m, rot=rot, name=name)


def _beam(p0, p1, w, m, name="timber"):
    """A square timber from p0 to p1."""
    p0, p1 = Vector(p0), Vector(p1)
    d = p1 - p0
    length = d.length
    ob = _box((w, w, length), tuple((p0 + p1) / 2), m, name=name)
    ob.rotation_mode = "QUATERNION"
    ob.rotation_quaternion = d.to_track_quat("Z", "Y")
    return ob


def block_pile(cx, cy, n, colour, rnd, s=1.0):
    """Squared stone blocks stacked in a small pile."""
    mats = _mats(colour, 3, rnd)
    w, d, h = 0.22 * s, 0.14 * s, 0.11 * s
    k = 0
    layer = 0
    while k < n:
        per = max(1, 4 - layer)
        for i in range(per):
            if k >= n:
                break
            x = cx + (i - (per - 1) / 2) * (w + 0.015) + rnd.uniform(-0.01, 0.01)
            y = cy + rnd.uniform(-0.02, 0.02) + (layer % 2) * d * 0.25
            _box((w, d, h), (x, y, h / 2 + layer * h), mats[k % 3],
                 rot=(0, 0, rnd.uniform(-0.08, 0.08)), name="stage_block")
            k += 1
        layer += 1


def timber_stack(cx, cy, n, rnd, length=0.7, axis="x"):
    m = lib.mat(TIMBER, rough=0.85)
    m2 = lib.mat(tuple(v * 0.85 for v in TIMBER), rough=0.85)
    w = 0.06
    for i in range(n):
        layer = i // 3
        j = i % 3
        off = (j - 1) * (w + 0.01) + (layer % 2) * w * 0.4
        z = w / 2 + layer * w
        jit = rnd.uniform(-0.03, 0.03)
        if axis == "x":
            p0, p1 = (cx - length / 2 + jit, cy + off, z), (cx + length / 2 + jit, cy + off, z)
        else:
            p0, p1 = (cx + off, cy - length / 2 + jit, z), (cx + off, cy + length / 2 + jit, z)
        _beam(p0, p1, w, m if i % 2 else m2, name="stage_timber")


def site(rect, tiles, pal, rnd, piles=True):
    """Frame 0: pegs and string round the walls' line, a trench along it,
    a pile of blocks and a stack of timber."""
    x0, y0, x1, y1 = rect
    earth = lib.mat(EARTH, rough=1.0)
    earth_lt = lib.mat(tuple(min(1, v * 1.25) for v in EARTH), rough=1.0)
    wood = lib.mat(TIMBER, rough=0.85)
    string = lib.mat(STRING, rough=0.8)
    # A levelled patch, then the trench on the walls' line.
    i = 0.06
    flat([(x0 + i, y0 + i, 0.004), (x1 - i, y0 + i, 0.004), (x1 - i, y1 - i, 0.004),
          (x0 + i, y1 - i, 0.004)], [(0, 1, 2, 3)], earth_lt, name="stage_patch")
    tw = 0.09
    a, b, c, d = x0 + 0.12, y0 + 0.12, x1 - 0.12, y1 - 0.12
    for (px, py, sx, sy) in (((a + c) / 2, b, c - a + tw, tw), ((a + c) / 2, d, c - a + tw, tw),
                             (a, (b + d) / 2, tw, d - b), (c, (b + d) / 2, tw, d - b)):
        _box((sx, sy, 0.012), (px, py, 0.008), earth, name="stage_trench")
    # Pegs at the corners and along the sides, string between them at knee height.
    pegs = [(x0 + 0.03, y0 + 0.03), (x1 - 0.03, y0 + 0.03), (x1 - 0.03, y1 - 0.03),
            (x0 + 0.03, y1 - 0.03)]
    for (px, py) in pegs:
        _box((0.035, 0.035, 0.22), (px, py, 0.11), wood, name="stage_peg")
    for k in range(4):
        p, q = pegs[k], pegs[(k + 1) % 4]
        _beam((p[0], p[1], 0.17), (q[0], q[1], 0.17), 0.008, string, name="stage_string")
    # The first blocks and the timber, inside the line where they read.
    if not piles:
        return
    span = min(x1 - x0, y1 - y0)
    s = min(1.0, 0.55 + 0.25 * span)
    block_pile(x0 + (x1 - x0) * 0.68, y0 + (y1 - y0) * 0.30, int(5 + 3 * span), pal["wall"], rnd, s)
    timber_stack(x0 + (x1 - x0) * 0.36, y0 + (y1 - y0) * 0.62, 6 if span > 1.2 else 4, rnd,
                 length=min(0.75, 0.5 * span), axis="x")


def scaffold(rect, top, rnd, tiles):
    """Timber scaffolding round `rect` up to `top`: posts, ledgers, braces
    on the two sides the camera sees, and a plank walk along the top."""
    x0, y0, x1, y1 = rect
    o = 0.07
    x0, y0, x1, y1 = x0 - o, y0 - o, x1 + o, y1 + o
    t = tiles / 2
    x0, y0, x1, y1 = max(x0, -t + 0.02), max(y0, -t + 0.02), min(x1, t - 0.02), min(y1, t - 0.02)
    wood = lib.mat(TIMBER, rough=0.85)
    wood2 = lib.mat(tuple(v * 0.82 for v in TIMBER), rough=0.85)
    plank = lib.mat(tuple(min(1, v * 1.12) for v in TIMBER), rough=0.8)
    h = top + 0.18
    pw = 0.035

    def along(a, b, n):
        return [a + (b - a) * k / n for k in range(n + 1)]
    nx = max(1, math.ceil((x1 - x0) / 0.75))
    ny = max(1, math.ceil((y1 - y0) / 0.75))
    xs, ys = along(x0, x1, nx), along(y0, y1, ny)
    posts = set()
    for x in xs:
        posts.add((x, y0))
        posts.add((x, y1))
    for y in ys:
        posts.add((x0, y))
        posts.add((x1, y))
    for (x, y) in sorted(posts):
        _box((pw, pw, h), (x, y, h / 2), wood, name="stage_post")
    levels = [z for z in np.arange(0.32, top + 0.05, 0.32)] or [top * 0.6]
    for z in list(levels) + [top + 0.02]:
        for (p, q) in (((x0, y0), (x1, y0)), ((x1, y0), (x1, y1)), ((x1, y1), (x0, y1)),
                       ((x0, y1), (x0, y0))):
            _beam((p[0], p[1], z), (q[0], q[1], z), 0.028, wood2, name="stage_ledger")
    # Braces on the front faces (-Y and +X), one per bay, alternating.
    for k in range(nx):
        a, b = xs[k], xs[k + 1]
        lo, hi = (a, b) if k % 2 == 0 else (b, a)
        _beam((lo, y0 - 0.01, 0.05), (hi, y0 - 0.01, min(top, 0.9)), 0.024, wood2,
              name="stage_brace")
    for k in range(ny):
        a, b = ys[k], ys[k + 1]
        lo, hi = (a, b) if k % 2 == 0 else (b, a)
        _beam((x1 + 0.01, lo, 0.05), (x1 + 0.01, hi, min(top, 0.9)), 0.024, wood2,
              name="stage_brace")
    # A walk of planks along the top of the two front sides.
    zt = top + 0.035
    _box((x1 - x0, 0.13, 0.018), ((x0 + x1) / 2, y0 + 0.02, zt), plank, name="stage_plank")
    _box((0.13, y1 - y0, 0.018), (x1 - 0.02, (y0 + y1) / 2, zt), plank, name="stage_plank")


def rubble(rect, tiles, pal, rnd):
    """A heap on the footprint: a dust stain, a low mound, broken blocks,
    charred beams, roof tiles and a stub or two of wall."""
    x0, y0, x1, y1 = rect
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    rx, ry = (x1 - x0) / 2, (y1 - y0) / 2
    wall, roof = pal["wall"], pal["roof"]
    dust = tuple(0.55 * w + 0.45 * e for w, e in zip(wall, (0.62, 0.52, 0.40)))
    # The stain: two irregular discs flat on the ground.
    for k, (sc, shade) in enumerate(((1.08, 0.86), (0.85, 0.97))):
        n = 18
        pts = []
        for i in range(n):
            a = 2 * math.pi * i / n
            r = sc * (0.85 + 0.2 * rnd.random())
            pts.append((cx + math.cos(a) * rx * r, cy + math.sin(a) * ry * r, 0.003 + 0.002 * k))
        pts = [(min(max(p[0], -tiles / 2), tiles / 2), min(max(p[1], -tiles / 2), tiles / 2), p[2])
               for p in pts]
        flat([(cx, cy, pts[0][2])] + pts, [(0, i + 1, (i + 1) % n + 1) for i in range(n)],
             lib.mat(tuple(v * shade for v in dust), rough=1.0), name="stage_stain")
    # The mound.
    mh = 0.14 + 0.06 * min(tiles, 3)
    mound = lib.blob(1.0, loc=(cx, cy, -0.02), m=lib.mat(dust, rough=1.0),
                     scale=(rx * 0.82, ry * 0.82, mh), seed=rnd.randrange(1 << 20), rough=0.18,
                     subdiv=3, name="stage_mound")
    clip_below_ground(mound)

    def on_mound(x, y):
        u = ((x - cx) / (rx * 0.82)) ** 2 + ((y - cy) / (ry * 0.82)) ** 2
        return max(0.0, mh * math.sqrt(max(0.0, 1 - u)) - 0.02)

    def spot(spread=0.55):
        while True:
            x, y = rnd.gauss(0, spread), rnd.gauss(0, spread)
            if x * x + y * y < 1:
                return cx + x * rx * 0.95, cy + y * ry * 0.95
    area = (x1 - x0) * (y1 - y0)
    stone = _mats(wall, 4, rnd, spread=0.12)
    # Stubs of wall at two corners, broken off unevenly.
    for (sx, sy, along) in ((x0 + 0.1, y0 + 0.1, "x"), (x1 - 0.1, y1 - 0.1, "y")):
        L = min(0.6, 0.35 * (x1 - x0))
        h = rnd.uniform(0.22, 0.38) * min(1.4, 0.7 + 0.25 * tiles)
        if along == "x":
            _box((L, 0.11, h), (sx + L / 2, sy, h / 2), stone[0], name="stage_stub")
            _box((L * 0.5, 0.11, h * 0.5), (sx + L * 1.1, sy, h * 0.25), stone[1], name="stage_stub")
        else:
            _box((0.11, L, h), (sx, sy - L / 2, h / 2), stone[2], name="stage_stub")
    # Blocks.
    for i in range(int(16 * area) + 6):
        x, y = spot()
        s = rnd.uniform(0.07, 0.2)
        dims = (s * rnd.uniform(0.9, 1.6), s, s * rnd.uniform(0.6, 0.9))
        _box(dims, (x, y, on_mound(x, y) + dims[2] * 0.3), stone[i % 4],
             rot=(rnd.uniform(-0.5, 0.5), rnd.uniform(-0.5, 0.5), rnd.uniform(0, math.pi)),
             name="stage_block")
    # Beams, charred and fallen across the heap.
    char = [lib.mat((0.20, 0.14, 0.10), rough=0.9), lib.mat(tuple(v * 0.8 for v in pal["wood"]),
                                                             rough=0.9)]
    for i in range(3 + int(1.5 * tiles)):
        x, y = spot(0.45)
        a = rnd.uniform(0, math.pi)
        L = rnd.uniform(0.35, 0.55) * min(1.6, 0.6 + 0.4 * tiles)
        dz = rnd.uniform(0.04, 0.22)
        z = on_mound(x, y) + 0.03 + dz / 2
        p0 = (x - math.cos(a) * L / 2, y - math.sin(a) * L / 2, z - dz / 2)
        p1 = (x + math.cos(a) * L / 2, y + math.sin(a) * L / 2, z + dz / 2)
        _beam(p0, p1, rnd.uniform(0.045, 0.07), char[i % 2], name="stage_beam")
    # Roof tiles, broken, scattered over it.
    tiles_m = _mats(roof, 3, rnd, spread=0.1, rough=0.7)
    for i in range(int(22 * area) + 4):
        x, y = spot(0.6)
        _box((rnd.uniform(0.06, 0.11), rnd.uniform(0.04, 0.07), 0.016),
             (x, y, on_mound(x, y) + 0.012), tiles_m[i % 3],
             rot=(rnd.uniform(-0.6, 0.6), rnd.uniform(-0.6, 0.6), rnd.uniform(0, math.pi)),
             name="stage_tile")


# ------------------------------------------------------------ entry

def flat_rubble(rect, tiles, pal, rnd):
    """A field or a jetty fallen: a trampled stain and a few broken rails."""
    x0, y0, x1, y1 = rect
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    rx, ry = (x1 - x0) / 2, (y1 - y0) / 2
    n = 18
    pts = []
    for i in range(n):
        a = 2 * math.pi * i / n
        r = 0.9 + 0.15 * rnd.random()
        pts.append((cx + math.cos(a) * rx * r, cy + math.sin(a) * ry * r, 0.003))
    flat([(cx, cy, 0.003)] + pts, [(0, i + 1, (i + 1) % n + 1) for i in range(n)],
         lib.mat(EARTH, rough=1.0), name="stage_stain")
    m = lib.mat(tuple(v * 0.8 for v in pal["wood"]), rough=0.9)
    for i in range(3 + tiles):
        x, y = cx + rnd.uniform(-rx, rx) * 0.8, cy + rnd.uniform(-ry, ry) * 0.8
        a = rnd.uniform(0, math.pi)
        L = rnd.uniform(0.25, 0.45)
        _beam((x - math.cos(a) * L / 2, y - math.sin(a) * L / 2, 0.03),
              (x + math.cos(a) * L / 2, y + math.sin(a) * L / 2, 0.03 + rnd.uniform(0, 0.06)),
              0.04, m, name="stage_rail")


def apply(stage, tiles, pal=None, seed=0, mode="building"):
    """Turn the freshly built idle model into `stage`: ("construction", 0..2)
    or ("rubble", 0). `tiles` is the game's footprint; `pal` the colours
    from `palette()` (the default's when None: only colours depend on it).
    `mode` "flat" (a farm, a dock): no scaffold or blocks, the site's pegs
    only, and a trampled stain for rubble."""
    kind, index = stage
    pal = pal or DEFAULT_PALETTE
    rnd = random.Random(seed * 7919 + (index + 1) * (13 if kind == "rubble" else 1))
    height, rect = measure(tiles)
    if kind == "rubble":
        remove_model()
        (flat_rubble if mode == "flat" else rubble)(rect, tiles, pal, rnd)
    elif index == 0:
        remove_model()
        site(rect, tiles, pal, rnd, piles=mode != "flat")
    else:
        top = height * CUTS[index - 1]
        cut_model(top, height, pal["roof"], tuple(v * 0.95 for v in pal["wall"]))
    if kind != "rubble" and index > 0 and mode != "flat":
        scaffold(rect, top, rnd, tiles)
        x0, y0, x1, y1 = rect
        # A few blocks still waiting, in front of the work.
        t = tiles / 2
        bx = min(x1 + 0.05, t - 0.2) if tiles > 1 else x1 - 0.12
        block_pile(bx, max(y0 - 0.02, -t + 0.12), 3 if index == 1 else 2, pal["wall"], rnd,
                   0.7 if tiles > 1 else 0.5)
    bpy.context.view_layer.update()

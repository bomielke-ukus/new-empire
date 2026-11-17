"""Export a look builder as a colour sprite set (docs/look/colour-sets.md).

    /usr/bin/python3.13 tools/look/export.py SET_NAME MODULE.FUNC \
        [--class MediumBuilding] [--tiles N] [--kwargs '{"variant": 1}'] \
        [--fit TILES [--model-tiles N]] [--samples 64] [--out assets/look] \
        [--sun-angle DEG] [--debug DIR] [--stages] \
        [--anims '{"arm": [{"arm": 0}, ...]}'] [--hash H]

writes assets/look/SET_NAME/SET_NAME.ron, SET_NAME.png and (when anything on
the subject takes player colour) SET_NAME.mask.png, plus SET_NAME.debug.png
(the sprite over its footprint diamond on grass) into --debug.

    /usr/bin/python3.13 tools/look/export.py --selftest [--debug DIR]

renders a flat 1 x 1 tile through the export camera and checks it comes out
128 x 64 px with its centre on the anchor.

How it works
------------
Camera: lib's game camera (orthographic, 30 degrees up, from the (+X, -Y)
corner), at authored scale 2: one tile is 128 x 64 px, so one world unit
along X or Y is 64 px across and 32 px down/up, and one unit of height is
64 * sqrt(6) / 2 = 78.38 px. The cell is sized from the subject's projected
bounds, its shadow's projection on the ground and the footprint diamond, plus
a margin, rounded up to even numbers; the camera is placed so the subject's
origin (the ground centre of its footprint) lands exactly on the corner of
pixel `anchor`, i.e. at continuous coordinate (ax, ay), which puts the tile
diamond's corners on pixel boundaries (a 2 x 2 diamond spans ax-128..ax+128).

Beauty: the subject is built with `player` = a neutral grey, so every surface
derived from the player colour (darkened, lightened, mixed, striped) comes
out as the grey the same derivation gives, lit by lib.light(). The ground is
a shadow catcher under a transparent film: the sprite keeps only the shadow,
as translucent black. Straight alpha, as Blender writes PNGs.

Mask: the subject is built a second time with a different, odd `player`.
Every colour value in every material's node tree (socket defaults, RGB nodes,
colour-ramp stops) and every colour attribute that differs between the two
builds is derived from the player colour. In the grey build those values are
set to white and all others to black, every material becomes an emission of
its own base colour, and the subject is rendered again with no lights and no
ground: that render's colour is how much of each pixel's subject coverage is
player-coloured, antialiased exactly as the beauty pass. The mask is that
times (subject coverage / beauty alpha), so it is the share of the pixel's
alpha that takes the owner's colour; the beauty RGB is turned to its own
luma (BT.601, as the game's tint uses) in proportion to the mask.

Layout: cells are laid out one row per (animation, facing), one column per
frame (spec "sheet"). A static subject is one "idle" frame, facing S,
unturned. With --stages (a building, docs/look/buildings.md) the sheet is
"construction" (3 frames), "idle", "rubble" (1 frame), all made from the
one idle model by stages.py; --anims adds animations whose every frame is
the same builder called with more keyword arguments (a wall's arms, a
gate's orientations). The idle frame is rendered first: its colours colour
the site, the scaffold's blocks and the rubble (stages.palette()).

Trim: every cell shares one size and anchor; after rendering, the cell is
cut down to the union of every frame's drawn pixels (on even pixels, so
the 2x grid is kept) and the anchor moved with it.

--hash writes "// export-hash: H" into the manifest's header, for
export_all.py to tell an up-to-date set.

Attachment points (docs/look/buildings.md, "Smoke, flags and fire"): the
idle model's empties named attach_<kind> (lib.attach) are projected into the
cell and written into the manifest's `attach` list, in authored pixels like
the anchor: a smoke point lifted, if the model hides it, until the camera
sees it (smoke rises from behind a roof); flag points only for a model
built without its cloth (kwargs "flags": False, none yet); and `--fires N`
fire points (default: by class for a building with stages) found on the
model's visible mass by casting the camera's rays, unless the model places
its own attach_fire empties.

    export.py SET_NAME MODULE.FUNC ... --points-only [--adopt] [--hash H]

rewrites only the attachment points of an exported set, and its hash, in
seconds: it builds the model, checks that what the model renders is what
was rendered (the `// export-geometry:` fingerprint of every mesh's
evaluated vertices, shading and materials, written by every export), and
fails with exit status 3 if not: the set needs a full export. An empty draws
nothing, so adding attach_ empties to a model changes its module's source
(and so the export hash) but not its pictures. `--adopt` takes the
fingerprint of a set exported before fingerprints were written, as it is;
export_all.py allows it only for a set whose hash is up to date.
"""
import os
import sys

# terrain._prop seeds its RNG with hash(kind): fix the hash seed so a prop
# comes out the same shape on every run (and in both builds below).
if os.environ.get("PYTHONHASHSEED") != "0":
    os.environ["PYTHONHASHSEED"] = "0"
    os.execv(sys.executable, [sys.executable] + sys.argv)

import argparse  # noqa: E402
import hashlib  # noqa: E402
import importlib  # noqa: E402
import re  # noqa: E402
import zlib  # noqa: E402
import json  # noqa: E402
import math  # noqa: E402
import shutil  # noqa: E402
import tempfile  # noqa: E402
import time  # noqa: E402

import numpy as np  # noqa: E402
from PIL import Image, ImageDraw  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import bpy  # noqa: E402
import lib  # noqa: E402
import stages  # noqa: E402

# ------------------------------------------------------------------ scale

# Bumped when what export.py writes changes, so export_all.py re-exports.
EXPORT_VERSION = 2

SCALE = 2                                   # authored scale
TILE_W, TILE_H = 64 * SCALE, 32 * SCALE     # 128 x 64
PPU = TILE_W / 2 * math.sqrt(2)             # px per world unit in the image plane (90.51)
EL = math.radians(30)
RIGHT = np.array([1.0, 1.0, 0.0]) / math.sqrt(2)              # screen right
UP = np.array([-math.sin(EL) / math.sqrt(2), math.sin(EL) / math.sqrt(2), math.cos(EL)])
TOWARD_CAMERA = np.array([math.cos(EL) / math.sqrt(2), -math.cos(EL) / math.sqrt(2), math.sin(EL)])

# Size classes (docs/05 section 2.3): footprint in tiles, and whether it turns.
CLASSES = {
    "Foot": (1, True), "Mounted": (1, True), "Heavy": (1, True), "Ship": (1, True),
    "SmallBuilding": (1, False), "MediumBuilding": (2, False),
    "LargeBuilding": (3, False), "Wonder": (5, False), "Terrain": (1, False),
}

# The five authored facings (tools/render rig.json), as the yaw that points a
# subject whose forward is +X (figures.py) the named way on screen under
# lib's camera: S looks at the camera, i.e. along (+X, -Y). A subject modelled
# facing +Y (the old rig's convention) wants 90 degrees less.
FACINGS = {"S": -45.0, "SW": -90.0, "W": -135.0, "NW": -180.0, "N": -225.0}

# Player colours for the two builds. Odd values, so lib.mat's cache cannot
# merge a player material with some other material of the same colour.
GREY = (0.7013, 0.7013, 0.7013)   # beauty: a normally lit cloth lands mid-upper
ODD = (0.2113, 0.8117, 0.3301)    # the second build, to find what player drives

MARGIN = 8                         # px round the bounds, for the soft shadow
SUN_ANGLE = None                   # --sun-angle: the sun's disc in degrees (None: lib's)


def light():
    """lib.light(), with the sun's disc widened by --sun-angle (a softer
    shadow edge; small scenery wants a soft, short shadow)."""
    lib.light()
    if SUN_ANGLE is not None:
        for ob in bpy.context.scene.objects:
            if ob.type == "LIGHT" and ob.data.type == "SUN":
                ob.data.angle = math.radians(SUN_ANGLE)


def project(p, anchor):
    """World points (N, 3) to continuous pixel coordinates (x right, y down)
    in a cell whose origin lands on `anchor`."""
    p = np.atleast_2d(np.asarray(p, float))
    return np.stack([anchor[0] + p @ RIGHT * PPU, anchor[1] - p @ UP * PPU], axis=1)


# ------------------------------------------------------------------ build

def load_builder(target):
    mod, fn = target.rsplit(".", 1)
    return getattr(importlib.import_module(mod), fn)


def build(builder, kwargs, player, scale=1.0, yaw_deg=0.0, spec=None):
    """A fresh scene with the subject at the origin, under a turntable empty
    that carries the export's fit scale and the facing. `spec` is a cell's
    frame (see cells()): more keyword arguments for the builder, and a
    construction stage or the rubble to turn the model into."""
    lib.reset()
    turn = lib.empty("export_turntable")
    kw = dict(kwargs, **(spec or {}).get("kw", {}))
    root = builder(player=player, **kw) if _takes_player(builder) else builder(**kw)
    root.parent = turn
    turn.scale = (scale, scale, scale)
    turn.rotation_euler = (0, 0, math.radians(yaw_deg))
    bpy.context.view_layer.update()
    if spec and spec.get("stage"):
        stages.apply(spec["stage"], spec["tiles"], spec.get("palette"), spec.get("seed", 0),
                     spec.get("mode", "building"))
    return turn


def _takes_player(fn):
    import inspect
    return "player" in inspect.signature(fn).parameters


def subject_points(min_z=None, max_z=None):
    """World positions of every vertex of the subject (evaluated meshes)."""
    dg = bpy.context.evaluated_depsgraph_get()
    out = []
    for ob in bpy.context.scene.objects:
        if ob.type != "MESH" or ob.get("export_ground"):
            continue
        ev = ob.evaluated_get(dg)
        me = ev.data
        n = len(me.vertices)
        if not n:
            continue
        co = np.empty(n * 3, np.float64)
        me.vertices.foreach_get("co", co)
        co = co.reshape(-1, 3)
        M = np.array(ev.matrix_world)
        w = co @ M[:3, :3].T + M[:3, 3]
        if min_z is not None:
            w = w[w[:, 2] >= min_z]
        if max_z is not None:
            w = w[w[:, 2] <= max_z]
        out.append(w)
    return np.concatenate(out) if out else np.zeros((0, 3))


def sun_direction():
    """The direction lib.light()'s sun travels (unit, downward)."""
    for ob in bpy.context.scene.objects:
        if ob.type == "LIGHT" and ob.data.type == "SUN":
            v = np.array(ob.matrix_world)[:3, 2]
            return -v / np.linalg.norm(v)
    raise SystemExit("no sun")


def model_tiles_estimate():
    """The builder's footprint in tiles, from its ground-level extent."""
    p = subject_points(max_z=0.12)
    ext = max(np.abs(p[:, 0]).max(), np.abs(p[:, 1]).max()) * 2
    return max(1, round(ext))


# -------------------------------------------------- what the player drives

def colour_slots():
    """Every colour value the materials and meshes hold, by a stable key:
    {key: (getter, setter)}. Keys depend only on build order, so two builds
    of the same subject with different player colours line up."""
    slots = {}
    for m in sorted(bpy.data.materials, key=lambda m: m.name):
        if m.node_tree is None:
            continue
        for n in m.node_tree.nodes:
            for i, s in enumerate(n.inputs):
                if s.type == "RGBA" and not s.is_linked:
                    slots[(m.name, n.name, "in", i)] = _sock(s)
            if n.type == "RGB":
                slots[(m.name, n.name, "out", 0)] = _sock(n.outputs[0])
            if n.type == "VALTORGB":
                for i, e in enumerate(n.color_ramp.elements):
                    slots[(m.name, n.name, "ramp", i)] = _sock(e, "color")
    for me in sorted(bpy.data.meshes, key=lambda me: me.name):
        for ca in me.color_attributes:
            slots[(me.name, ca.name, "attr", 0)] = _attr(ca)
    return slots


def _sock(s, prop="default_value"):
    def get():
        return np.array(getattr(s, prop)[:3], np.float64)

    def put(v):
        getattr(s, prop)[:3] = [float(x) for x in v]
    return get, put


def _attr(ca):
    n = len(ca.data)

    def get():
        a = np.empty(n * 4, np.float32)
        ca.data.foreach_get("color", a)
        return a.reshape(-1, 4)[:, :3].astype(np.float64)

    def put(v):
        a = np.ones((n, 4), np.float32)
        a[:, :3] = v
        ca.data.foreach_set("color", a.ravel())
    return get, put


def player_slots(builder, kwargs, scale, yaw, spec=None):
    """Values of a build with the ODD player colour, by slot key."""
    build(builder, kwargs, ODD, scale, yaw, spec)
    ground(builder)       # as the GREY build will have it, so the slots line up
    return {k: g() for k, (g, _) in colour_slots().items()}


def mark_player(odd_values):
    """In the current (GREY) build: player-driven values to white, the rest
    to black. Returns how many slots the player drives."""
    slots = colour_slots()
    if set(slots) != set(odd_values):
        raise SystemExit("the two builds differ in structure; the builder is not "
                         "deterministic, cannot derive the mask")
    driven = 0
    for k, (get, put) in slots.items():
        a, b = get(), odd_values[k]
        if a.shape != b.shape:
            raise SystemExit("slot %r changed shape between builds" % (k,))
        diff = np.abs(a - b) > 1e-5
        if a.ndim == 1:
            on = bool(diff.any())
            put(np.ones(3) if on else np.zeros(3))
            driven += on
        else:
            on = diff.any(axis=1)
            put(np.where(on[:, None], 1.0, 0.0))
            driven += int(on.any())
    return driven


def to_emission():
    """Every material shows its own base colour, unlit (for the mask pass)."""
    for m in bpy.data.materials:
        if m.node_tree is None:
            continue
        nt = m.node_tree
        out = next((n for n in nt.nodes if n.type == "OUTPUT_MATERIAL" and n.is_active_output),
                   None) or next((n for n in nt.nodes if n.type == "OUTPUT_MATERIAL"), None)
        bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
        if out is None or bsdf is None:
            continue
        em = nt.nodes.new("ShaderNodeEmission")
        em.inputs["Strength"].default_value = 1.0
        base = bsdf.inputs["Base Color"]
        if base.is_linked:
            nt.links.new(base.links[0].from_socket, em.inputs["Color"])
        else:
            em.inputs["Color"].default_value = base.default_value[:]
        shader = em.outputs[0]
        alpha = bsdf.inputs["Alpha"]
        if alpha.is_linked or alpha.default_value < 1.0:
            mix = nt.nodes.new("ShaderNodeMixShader")
            tr = nt.nodes.new("ShaderNodeBsdfTransparent")
            if alpha.is_linked:
                nt.links.new(alpha.links[0].from_socket, mix.inputs[0])
            else:
                mix.inputs[0].default_value = alpha.default_value
            nt.links.new(tr.outputs[0], mix.inputs[1])
            nt.links.new(em.outputs[0], mix.inputs[2])
            shader = mix.outputs[0]
        nt.links.new(shader, out.inputs["Surface"])
        if out.inputs.get("Displacement") and out.inputs["Displacement"].is_linked:
            nt.links.remove(out.inputs["Displacement"].links[0])


# ------------------------------------------------------------------ render

def ground(builder):
    """The shadow catcher, unless the builder says it stands on nothing
    (`builder.no_ground`: an effect, fx.py, drawn on a building and not on
    the ground, which casts no shadow and may reach below its origin)."""
    if not getattr(builder, "no_ground", False):
        catcher()


def catcher():
    """A big ground plane at z=0 that only shows the shadows on it."""
    ob = lib.mesh([(-30, -30, 0), (30, -30, 0), (30, 30, 0), (-30, 30, 0)], [(0, 1, 2, 3)],
                  lib.mat("grass", rough=1.0), name="export_ground")
    ob["export_ground"] = True
    ob.is_shadow_catcher = True
    return ob


def camera(frame, anchor):
    """lib's camera, scaled to PPU and slid so the origin lands on `anchor`."""
    w, h = frame
    centre = RIGHT * (w / 2 - anchor[0]) / PPU + UP * (anchor[1] - h / 2) / PPU
    cam = lib.camera(tuple(centre), w / PPU / math.sqrt(2), (w, h))
    cam.data.sensor_fit = "HORIZONTAL"
    sc = bpy.context.scene
    sc.render.resolution_percentage = 100
    sc.render.pixel_aspect_x = sc.render.pixel_aspect_y = 1.0
    bpy.context.view_layer.update()
    return cam


def verify_camera(cam, frame, anchor, tiles):
    """Blender's own projection of known points against project(); returns
    the worst error in px."""
    from bpy_extras.object_utils import world_to_camera_view
    from mathutils import Vector
    sc = bpy.context.scene
    t = tiles / 2
    pts = [(0, 0, 0), (1, 0, 0), (0, 1, 0), (0, 0, 1), (-t, -t, 0), (t, -t, 0), (t, t, 0),
           (-t, t, 0), (0.3, -0.7, 1.9)]
    want = project(pts, anchor)
    worst = 0.0
    for p, wv in zip(pts, want):
        v = world_to_camera_view(sc, cam, Vector(p))
        got = (v.x * frame[0], (1 - v.y) * frame[1])
        worst = max(worst, abs(got[0] - wv[0]), abs(got[1] - wv[1]))
    d = project([(1, 0, 0), (0, 1, 0), (0, 0, 1)], (0, 0))
    assert np.allclose(d[0], (64, 32)) and np.allclose(d[1], (64, -32)), d
    return worst, d[2][1]


def render_settings(samples, denoise=True, bounces=3):
    sc = bpy.context.scene
    sc.render.engine = "CYCLES"
    sc.cycles.device = "CPU"
    sc.cycles.samples = samples
    sc.cycles.use_adaptive_sampling = False
    sc.cycles.seed = 0
    sc.cycles.use_denoising = denoise
    sc.cycles.max_bounces = bounces
    sc.render.film_transparent = True
    sc.render.filter_size = 1.5
    sc.view_settings.view_transform = "Standard"
    sc.view_settings.look = "None"
    sc.view_settings.exposure = 0.0
    sc.view_settings.gamma = 1.0
    sc.render.image_settings.file_format = "PNG"
    sc.render.image_settings.color_mode = "RGBA"
    sc.render.image_settings.color_depth = "8"


def render_to(path):
    sc = bpy.context.scene
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)
    return np.asarray(Image.open(path).convert("RGBA"), np.float64) / 255.0


def srgb_to_linear(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def frame_for(points, shadow_dir, tiles):
    """Cell size and anchor holding the subject, its shadow and its diamond."""
    t = tiles / 2
    pts = [points, np.array([(-t, -t, 0), (t, -t, 0), (t, t, 0), (-t, t, 0)], float)]
    if len(points):
        k = points[:, 2:3] / max(1e-6, -shadow_dir[2])
        pts.append(points + k * shadow_dir)       # each point's shadow on z = 0
    xy = project(np.concatenate(pts), (0, 0))
    lo = np.floor(xy.min(axis=0)) - MARGIN
    hi = np.ceil(xy.max(axis=0)) + MARGIN
    anchor = (int(-lo[0]), int(-lo[1]))
    size = [int(hi[0] - lo[0]), int(hi[1] - lo[1])]
    size = tuple(s + (s % 2) for s in size)
    return size, anchor


# ------------------------------------------------------------------ cells

# Timing of each animation in the manifest: (frame_ms, loops), as the old
# sets have them.
TIMING = {"idle": (160, True), "construction": (200, False), "rubble": (200, False)}


def cells(class_name, with_stages=False, anims=None, tiles=1, seed=0, mode="building"):
    """The sheet's rows and columns: [(animation, facing, [spec, ...]), ...],
    a spec per frame: {"kw": more builder kwargs, "stage": None or
    ("construction", i) / ("rubble", 0), "tiles", "seed"}. A static subject
    is one idle frame, the S view (a building is rendered as modelled,
    square to the grid); with stages, a building's construction stages and
    rubble round it; `anims` adds {name: [kwargs per frame]}."""
    def spec(stage=None, kw=None):
        return {"kw": kw or {}, "stage": stage, "tiles": tiles, "seed": seed, "mode": mode}
    out = []
    anims = dict(anims or {})
    if with_stages:
        out.append(("construction", "S", [spec(("construction", i)) for i in range(3)]))
    # An animated idle (an effect, fx.py) is declared as `anims["idle"]`.
    out.append(("idle", "S", [spec(kw=kw) for kw in anims.pop("idle", [{}])]))
    if with_stages:
        out.append(("rubble", "S", [spec(("rubble", 0))]))
    for name, frames in anims.items():
        out.append((name, "S", [spec(kw=kw) for kw in frames]))
    return out


def render_cell(builder, kwargs, scale, yaw, frame, anchor, tiles, samples, tmp, tag, spec=None):
    """Beauty RGBA (straight, 0..1) and mask (0..1) of one cell."""
    t0 = time.time()
    # 1. the ODD build: which colour values the player drives
    odd = player_slots(builder, kwargs, scale, yaw, spec)
    # 2. the GREY build, lit, over a shadow catcher
    build(builder, kwargs, GREY, scale, yaw, spec)
    ground(builder)
    light()
    cam = camera(frame, anchor)
    err, zpx = verify_camera(cam, frame, anchor, tiles)
    if err > 0.01:
        raise SystemExit("camera disagrees with project() by %.4f px" % err)
    render_settings(samples)
    beauty = render_to(os.path.join(tmp, tag + "_beauty.png"))
    t1 = time.time()
    # 3. the coverage pass in the same build: player-driven values white, the
    #    rest black, all emission, no lights, no ground. Its alpha is the
    #    subject's own coverage; its colour, the player-coloured share of it.
    driven = mark_player(odd)
    for ob in list(bpy.context.scene.objects):
        if ob.get("export_ground") or ob.type == "LIGHT":
            ob.hide_render = True
    bpy.context.scene.world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0
    to_emission()
    render_settings(max(16, samples // 2), denoise=False, bounces=0)
    cov = render_to(os.path.join(tmp, tag + "_cover.png"))
    beauty = clean_shadow(beauty, cov[..., 3])
    mask = np.zeros(beauty.shape[:2])
    if driven:
        cov_p = srgb_to_linear(cov[..., 0]) * cov[..., 3]    # premultiplied player coverage
        a = beauty[..., 3]
        mask = np.where(a > 1e-4, cov_p / np.maximum(a, 1e-4), 0.0).clip(0, 1)
        mask[a < 1.5 / 255] = 0
    t2 = time.time()
    print("  cell %s: beauty %.1fs, mask %.1fs, %d player slots, projection error %.5f px, "
          "1 unit of height = %.2f px" % (tag, t1 - t0, t2 - t1, driven, err, -zpx))
    return beauty, mask, driven


SHADOW_FLOOR = 5 / 255   # below this the catcher's shadow is noise and far sky occlusion


def clean_shadow(beauty, s):
    """Split the beauty alpha into the subject (coverage `s`) over its shadow,
    drop the shadow's faint floor and fade it out within MARGIN of the cell's
    edge, so the cell trims to its subject and never ends in a hard line."""
    A = beauty[..., 3]
    sh = np.where(s < 0.999, (A - s) / np.maximum(1 - s, 1e-6), 0.0).clip(0, 1)
    sh = ((sh - SHADOW_FLOOR) / (1 - SHADOW_FLOOR)).clip(0, 1)
    h, w = A.shape
    y, x = np.mgrid[0:h, 0:w]
    d = np.minimum(np.minimum(x, w - 1 - x), np.minimum(y, h - 1 - y)).astype(float)
    sh *= np.clip((d - 1) / (MARGIN - 2), 0, 1) ** 2
    A2 = s + (1 - s) * sh
    premult = beauty[..., :3] * A[..., None]          # the shadow adds no colour
    rgb = np.where(A2[..., None] > 1e-6, premult / np.maximum(A2, 1e-6)[..., None], 0).clip(0, 1)
    return np.dstack([rgb, A2])


def finish(beauty, mask):
    """8-bit sheet (masked pixels turned to their own luma) and mask."""
    rgb = beauty[..., :3] * 255.0
    luma = rgb @ np.array([0.299, 0.587, 0.114])
    rgb = rgb + mask[..., None] * (luma[..., None] - rgb)
    out = np.dstack([rgb, beauty[..., 3] * 255.0])
    return (np.rint(out).clip(0, 255).astype(np.uint8),
            np.rint(mask * 255).clip(0, 255).astype(np.uint8))


# ------------------------------------------------------------------ output

def manifest(name, cls, anchor, frame, anims, has_mask, source, digest=None, geometry=None,
             points=(), timing=None, impacts=None):
    timing = dict(TIMING, **(timing or {}))
    impacts = impacts or {}
    lines = [
        "// Generated by tools/look/export.py from %s." % source,
        "// A colour set (docs/look/colour-sets.md): re-export rather than edit.",
    ] + (["// export-hash: %s" % digest] if digest else []) + (
        ["// export-geometry: %s" % geometry] if geometry else []) + [
        "SpriteSet(",
        '    name: "%s",' % name,
        "    class: %s," % cls,
        "    scale: %d," % SCALE,
        '    palette: "rgba",',
        '    sheet: "%s.png",' % name,
        "    anchor: (%d, %d)," % anchor,
        "    animations: [",
    ]
    for a, n in anims:
        ms, loops = timing.get(a, (1000, False))
        imp = impacts.get(a)
        lines.append('        (name: "%s", frames: %d, frame_ms: %d, loops: %s, impact: %s),'
                     % (a, n, ms, "true" if loops else "false",
                        "None" if imp is None else "Some(%d)" % imp))
    lines += [
        "    ],",
        "    anchor_overrides: [],",
        "    colour: true,",
        "    mask: %s," % ('Some("%s.mask.png")' % name if has_mask else "None"),
        "    frame: Some((%d, %d))," % frame,
    ] + attach_lines(points) + [
        ")",
        "",
    ]
    return "\n".join(lines)


# ------------------------------------------------------- attachment points

# The manifest's names for lib.attach's kinds.
ATTACH_KINDS = {"smoke": "Smoke", "smoke_dark": "SmokeDark", "flag": "Flag", "fire": "Fire"}
# Fire points a building gets by default, by size class.
FIRES = {"SmallBuilding": 2, "MediumBuilding": 3, "LargeBuilding": 4, "Wonder": 5}
RAY_STEP = 4          # px between the camera rays that look for the fire points' surfaces


def attach_lines(points):
    """The manifest's `attach` field for [(kind, (x, y)), ...] (none: no field)."""
    if not points:
        return []
    return (["    attach: ["] +
            ["        (kind: %s, at: (%d, %d))," % (ATTACH_KINDS[k], x, y) for k, (x, y) in points] +
            ["    ],"])


def _ray(origin, direction, skip=0.0):
    """The first mesh the ray from `origin` along `direction` hits beyond
    `skip`: (location, normal, object) or None."""
    from mathutils import Vector
    dg = bpy.context.evaluated_depsgraph_get()
    o = Vector(origin) + Vector(direction) * skip
    hit, loc, normal, _, ob, _ = bpy.context.scene.ray_cast(dg, o, Vector(direction), distance=500)
    return (np.array(loc), np.array(normal), ob) if hit else None


def _seen(p):
    """Whether the camera sees world point `p` (nothing in front of it)."""
    return _ray(p, TOWARD_CAMERA, skip=0.02) is None


def attach_empties():
    """The model's attachment points: [(kind, world position)], by name."""
    out = []
    for ob in sorted(bpy.context.scene.objects, key=lambda o: o.name):
        if ob.type != "EMPTY" or not ob.name.startswith("attach_"):
            continue
        kind = ob.name[len("attach_"):].split(".")[0]
        if kind not in ATTACH_KINDS:
            raise SystemExit("%s: no such attachment kind %r" % (ob.name, kind))
        out.append((kind, np.array(ob.matrix_world.translation)))
    return out


def _burns(ob, s=1.0):
    """Whether fire sits well on `ob`: a part of the building (a wall, a
    roof), not a pole, a rod, a cloth or a plant."""
    if ob.name.startswith(("cloth", "rod")):
        return False
    M = np.array(ob.matrix_world)
    c = np.array([tuple(v) for v in ob.bound_box]) @ M[:3, :3].T + M[:3, 3]
    d = sorted(c.max(axis=0) - c.min(axis=0))
    if d[1] < 0.15 * s or d[0] < 0.05 * s:
        return False               # a pole, or a sheet: a cloth, a door, a sign
    grey = lib._lin(GREY)
    for m in ob.data.materials if ob.type == "MESH" else ():
        b = m and m.node_tree and next((n for n in m.node_tree.nodes
                                        if n.type == "BSDF_PRINCIPLED"), None)
        if b and not b.inputs["Base Color"].is_linked:
            r, g, bl = b.inputs["Base Color"].default_value[:3]
            if g > r * 1.05 and g > bl * 1.05:
                return False       # leaves
            if np.allclose((r, g, bl), grey, atol=1e-3):
                return False       # the player's colour: a flag, a banner, an awning
    return True


def fire_points(n, anchor, frame):
    """`n` places on the model's visible mass where it burns: the camera's
    rays every RAY_STEP px across the cell, kept where they land on a broad
    surface off the ground (not a yard, a pole or a cloth) facing the
    camera, and whose neighbours RAY_STEP*3 px round do too; the first the
    one nearest the upper middle of the mass, then each the farthest from
    those chosen. In cell px."""
    if n <= 0:
        return []
    w, h = frame
    turn = bpy.data.objects.get("export_turntable")
    s = turn.scale[0] if turn else 1.0            # the export's fit
    hits = {}
    for j in range(0, h, RAY_STEP):
        for i in range(0, w, RAY_STEP):
            o = (RIGHT * (i + 0.5 - anchor[0]) / PPU + UP * (anchor[1] - j - 0.5) / PPU
                 + TOWARD_CAMERA * 60)
            r = _ray(o, -TOWARD_CAMERA)
            if r is None:
                continue
            loc, normal, ob = r
            if loc[2] < 0.4 * s or abs(normal @ TOWARD_CAMERA) < 0.3 or not _burns(ob, s):
                continue
            hits[i, j] = loc
    k = RAY_STEP * 3
    keep = [(i, j) for (i, j) in hits
            if all((i + di, j + dj) in hits for di, dj in ((k, 0), (-k, 0), (0, k), (0, -k)))]
    if not keep:
        keep = list(hits)
    if not keep:
        return []
    pts = np.array(keep, float) + 0.5
    lo, hi = pts.min(axis=0), pts.max(axis=0)
    aim = np.array([(lo[0] + hi[0]) / 2, lo[1] + 0.3 * (hi[1] - lo[1])])
    chosen = [int(np.argmin(np.abs(pts - aim).sum(axis=1)))]
    while len(chosen) < min(n, len(pts)):
        d = np.min([np.hypot(*(pts - pts[c]).T) for c in chosen], axis=0)
        chosen.append(int(np.argmax(d)))
    return [tuple(pts[c]) for c in chosen]


def attach_points(kwargs, anchor, frame, fires):
    """The attachment points of the current (idle) build, in the cell whose
    origin lands on `anchor`: [(kind, (x, y))] in authored px."""
    out = []
    own_fire = False
    for kind, p in attach_empties():
        if kind == "flag" and kwargs.get("flags", True):
            continue          # the model draws its own flag cloth: no animated one
        if kind.startswith("smoke"):
            for _ in range(60):          # rise from behind whatever hides the hearth
                if _seen(p):
                    break
                p = p + np.array([0, 0, 0.04])
        own_fire |= kind == "fire"
        out.append((kind, project(p, anchor)[0]))
    if not own_fire:
        out += [("fire", p) for p in fire_points(fires, anchor, frame)]
    return [(k, (int(round(x)), int(round(y)))) for k, (x, y) in out]


def geometry_fingerprint(h=None):
    """A hash of what the current build renders: every mesh object's
    evaluated vertices (world space, to 1e-4), face and loop counts, shading, shadow and
    render flags and materials, and every material's node values and links.
    Empties, which draw nothing, do not count."""
    h = h or hashlib.sha256()
    dg = bpy.context.evaluated_depsgraph_get()
    for ob in sorted(bpy.context.scene.objects, key=lambda o: o.name):
        if ob.type == "EMPTY":
            continue
        h.update(("%s|%s|%d|%d" % (ob.name, ob.type, ob.hide_render, ob.visible_shadow)).encode())
        if ob.type != "MESH":
            continue
        me = ob.evaluated_get(dg).data
        n = len(me.vertices)
        co = np.empty(n * 3, np.float64)
        me.vertices.foreach_get("co", co)
        M = np.array(ob.evaluated_get(dg).matrix_world)
        w = co.reshape(-1, 3) @ M[:3, :3].T + M[:3, 3]
        h.update(np.round(w, 4).tobytes())
        # (The counts, not the loops' indices: those of an evaluated smooth
        # mesh are not the same from one evaluation to the next.)
        h.update(np.array([len(me.polygons), len(me.loops)], np.int64).tobytes())
        sm = np.empty(len(me.polygons), np.int8)
        me.polygons.foreach_get("use_smooth", sm)
        h.update(sm.tobytes())
        h.update("|".join(m.name if m else "-" for m in me.materials).encode())
    for m in sorted(bpy.data.materials, key=lambda m: m.name):
        if m.node_tree is None:
            continue
        for nd in sorted(m.node_tree.nodes, key=lambda nd: nd.name):
            h.update(("%s|%s|%s" % (m.name, nd.name, nd.bl_idname)).encode())
            for s in nd.inputs:
                v = getattr(s, "default_value", None)
                if v is not None and not isinstance(v, str):
                    h.update(np.round(np.atleast_1d(np.array(v, float)), 5).tobytes())
            if nd.type == "VALTORGB":
                for e in nd.color_ramp.elements:
                    h.update(np.round(np.array([e.position, *e.color]), 5).tobytes())
        for lk in m.node_tree.links:
            h.update(("%s.%s>%s.%s" % (lk.from_node.name, lk.from_socket.identifier,
                                       lk.to_node.name, lk.to_socket.identifier)).encode())
    return h


def fingerprint(builder, kwargs, scale, layout):
    """The geometry fingerprint of every model cell of the sheet (the idle
    and the extra animations; the stages and rubble are made from the idle
    by stages.py, which the export hash covers)."""
    h = hashlib.sha256()
    for anim, facing, frames in layout:
        for f in frames:
            if f.get("stage"):
                continue
            build(builder, kwargs, GREY, scale, 0.0, f)
            geometry_fingerprint(h)
    return h.hexdigest()[:16]


def points_only(args, builder, kwargs, scale, layout, fires):
    """--points-only: the set's attachment points and hash rewritten, the
    pictures left as they are, if the model still renders as it did."""
    path = os.path.join(args.out, args.name, args.name + ".ron")
    text = open(path, encoding="utf-8").read()
    m = re.search(r"^\s*anchor\s*:\s*\(\s*(-?\d+)\s*,\s*(-?\d+)\s*\)", text, re.M)
    anchor = (int(m.group(1)), int(m.group(2)))
    m = re.search(r"^\s*frame\s*:\s*Some\(\(\s*(\d+)\s*,\s*(\d+)\s*\)\)", text, re.M)
    frame = (int(m.group(1)), int(m.group(2)))
    geo = fingerprint(builder, kwargs, scale, layout)
    m = re.search(r"^// export-geometry: (\w+)", text, re.M)
    if m is None and not args.adopt:
        print("%s: no geometry fingerprint to check against; --adopt takes this one" % args.name)
        return 3
    if m is not None and m.group(1) != geo:
        print("%s: the model renders differently (geometry %s, was %s): needs a full export"
              % (args.name, geo, m.group(1)))
        return 3
    build(builder, kwargs, GREY, scale, 0.0)
    points = attach_points(kwargs, anchor, frame, fires)
    lines = [ln for ln in text.split("\n")
             if not ln.startswith(("// export-hash:", "// export-geometry:"))]
    head = [i for i, ln in enumerate(lines) if ln.startswith("//")]
    at = head[-1] + 1 if head else 0
    lines[at:at] = (["// export-hash: %s" % args.hash] if args.hash else []) + \
        ["// export-geometry: %s" % geo]
    text = "\n".join(lines)
    text = re.sub(r"\n    attach: \[\n(?:        [^\n]*\n)*    \],", "", text)
    i = text.rindex("\n)")
    text = text[:i] + "".join("\n" + ln for ln in attach_lines(points)) + text[i:]
    with open(path, "w") as fh:
        fh.write(text)
    print("points %s: %s" % (args.name, ", ".join("%s %s" % p for p in points) or "none"))
    return 0


def debug_png(path, sheet, anchor, tiles, frame):
    """Every cell of the sheet over grass with its footprint diamond (and
    the tile grid under it) drawn at the anchor, 2x, for checking by eye."""
    w, h = frame
    rows, cols = sheet.shape[0] // h, sheet.shape[1] // w
    out = Image.new("RGBA", (w * cols * 2, h * rows * 2), (40, 40, 40, 255))
    for j in range(rows):
        for i in range(cols):
            cell = sheet[j * h:(j + 1) * h, i * w:(i + 1) * w]
            if cell[..., 3].max() == 0:
                continue
            out.paste(_debug_cell(cell, anchor, tiles, frame), (i * w * 2, j * h * 2))
    out.save(path)


def _debug_cell(rgba, anchor, tiles, frame):
    w, h = frame
    bg = Image.new("RGBA", (w, h), (86, 140, 60, 255))
    d = ImageDraw.Draw(bg)
    ax, ay = anchor
    t = tiles / 2
    for i in range(tiles + 1):           # grid lines across the footprint
        u = -t + i
        a, b = project([(u, -t, 0), (u, t, 0)], anchor)
        d.line([tuple(a), tuple(b)], fill=(60, 105, 40, 255), width=1)
        a, b = project([(-t, u, 0), (t, u, 0)], anchor)
        d.line([tuple(a), tuple(b)], fill=(60, 105, 40, 255), width=1)
    poly = [tuple(p) for p in project([(-t, -t, 0), (t, -t, 0), (t, t, 0), (-t, t, 0)], anchor)]
    d.polygon(poly, outline=(255, 255, 0, 255))
    over = Image.alpha_composite(bg, Image.fromarray(rgba[:h, :w], "RGBA"))
    d = ImageDraw.Draw(over)
    d.line(poly + [poly[0]], fill=(255, 255, 0, 110), width=1)   # the diamond again, on top
    d.line([(ax - 4, ay), (ax + 4, ay)], fill=(255, 0, 0, 255))
    d.line([(ax, ay - 4), (ax, ay + 4)], fill=(255, 0, 0, 255))
    return over.resize((w * 2, h * 2), Image.NEAREST)


def union_trim(rows, frame, anchor):
    """Cuts every cell down to the union of the drawn pixels of all of
    them, the cut on even pixels (the sheet is 2x: its 1x grid is kept);
    returns the cut rows, the new cell size and anchor."""
    w, h = frame
    x0, y0, x1, y1 = w, h, 0, 0
    for row in rows:
        for rgba, _ in row:
            ys, xs = np.nonzero(rgba[..., 3])
            if len(xs):
                x0, y0 = min(x0, xs.min()), min(y0, ys.min())
                x1, y1 = max(x1, xs.max() + 1), max(y1, ys.max() + 1)
    if x1 <= x0:
        return rows, frame, anchor
    x0, y0 = x0 // 2 * 2, y0 // 2 * 2
    x1, y1 = min(w, -(-x1 // 2) * 2), min(h, -(-y1 // 2) * 2)
    rows = [[(rgba[y0:y1, x0:x1], m[y0:y1, x0:x1]) for rgba, m in row] for row in rows]
    return rows, (int(x1 - x0), int(y1 - y0)), (int(anchor[0] - x0), int(anchor[1] - y0))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("name", nargs="?")
    ap.add_argument("target", nargs="?", help="MODULE.FUNC in tools/look")
    ap.add_argument("--class", dest="cls", default="MediumBuilding", choices=sorted(CLASSES))
    ap.add_argument("--tiles", type=int, help="the game's footprint (default: the class's)")
    ap.add_argument("--kwargs", default="{}", help="JSON keyword arguments for the builder")
    ap.add_argument("--fit", type=float, help="scale the model uniformly so its footprint "
                    "is this many tiles")
    ap.add_argument("--model-tiles", type=float, help="the model's own footprint for --fit "
                    "(default: measured from its ground-level extent)")
    ap.add_argument("--samples", type=int, help="Cycles samples (default: 64 for a building, "
                    "unitexport.SAMPLES for a unit)")
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.dirname(HERE)),
                                                  "assets", "look"))
    ap.add_argument("--debug", default=os.path.join(tempfile.gettempdir(), "look-export"),
                    help="where the debug PNG goes")
    ap.add_argument("--sun-angle", type=float, help="the sun's angular size in degrees, for "
                    "a softer shadow edge (default: lib.light()'s 2.5)")
    ap.add_argument("--stages", nargs="?", const="building", choices=["building", "flat"],
                    help="add construction stages and rubble (docs/look/buildings.md); "
                    "'flat' for a farm or a dock")
    ap.add_argument("--anims", default="{}", help='JSON {"name": [kwargs per frame, ...]}: '
                    "more animations, each frame the builder with those kwargs added")
    ap.add_argument("--hash", help="written into the manifest's header (export_all.py)")
    ap.add_argument("--timing", default="{}", help='JSON {"anim": [frame_ms, loops]}: '
                    "timing other than the defaults (an effect's looping idle)")
    ap.add_argument("--fires", type=int, help="fire points to find on the model (default: "
                    "by class for a building with stages, 1 for a wall or gate, else 0)")
    ap.add_argument("--points-only", action="store_true", help="rewrite only the attachment "
                    "points and the hash of the exported set, if the model renders as it did")
    ap.add_argument("--adopt", action="store_true", help="with --points-only: take the "
                    "model's geometry fingerprint as it is, for a set exported without one")
    ap.add_argument("--scale", type=int, choices=[1, 2], help="a unit: authored pixels per "
                    "1x pixel (default 2, as everything else; 1 renders it at half resolution)")
    ap.add_argument("--keep-work", action="store_true", help="a unit: keep its rendered cells "
                    "in target/look-units after the export")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    global SUN_ANGLE
    SUN_ANGLE = args.sun_angle
    os.makedirs(args.debug, exist_ok=True)
    if args.selftest:
        return selftest(args)
    if not (args.name and args.target):
        ap.error("SET_NAME and MODULE.FUNC are required")

    t_start = time.time()
    builder = load_builder(args.target)
    if getattr(builder, "unit", False):
        # A unit (docs/look/units.md): a turning, animated set from a figure.
        if args.points_only:
            print("points %s: a unit has no attachment points" % args.name)
            return 0
        import unitexport
        return unitexport.main(args, builder)
    args.samples = args.samples or 64
    kwargs = json.loads(args.kwargs)
    tiles = args.tiles or CLASSES[args.cls][0]
    turns = CLASSES[args.cls][1]

    scale = 1.0
    if args.fit:
        build(builder, kwargs, GREY)
        model = args.model_tiles or model_tiles_estimate()
        scale = args.fit / model
        print("fit: model %.2f tiles -> %.2f tiles, scale %.4f" % (model, args.fit, scale))

    seed = zlib.crc32(args.name.encode()) & 0xFFFF
    anims_kw = json.loads(args.anims)
    layout = cells(args.cls, bool(args.stages), anims_kw, tiles, seed,
                   args.stages or "building")
    fires = args.fires
    if fires is None:
        fires = (1 if set(anims_kw) & {"arm", "shut"} else
                 FIRES.get(args.cls, 0) if args.stages == "building" else 0)
    if args.points_only:
        return points_only(args, builder, kwargs, scale, layout, fires)
    # One cell size and anchor for the whole sheet: the union of every cell's.
    lo, hi = np.array([1e9, 1e9]), np.array([-1e9, -1e9])
    for anim, facing, frames in layout:
        yaw = FACINGS[facing] if turns else 0.0
        for f in frames:
            build(builder, kwargs, GREY, scale, yaw, f)
            light()
            (w, h), (ax, ay) = frame_for(subject_points(), sun_direction(), tiles)
            lo = np.minimum(lo, (-ax, -ay))
            hi = np.maximum(hi, (w - ax, h - ay))
    anchor = (int(-lo[0]), int(-lo[1]))
    frame = tuple(int(v) for v in (hi - lo))
    frame = tuple(v + (v % 2) for v in frame)
    print("frame %s, anchor %s" % (frame, anchor))

    # ground-level check: does the base stand inside the footprint?
    build(builder, kwargs, GREY, scale, 0.0)
    base = subject_points(max_z=0.1)
    if len(base):
        reach = max(np.abs(base[:, 0]).max(), np.abs(base[:, 1]).max())
        print("ground-level reach %.3f (footprint half-width %.3f)%s"
              % (reach, tiles / 2, "  ** OUTSIDE **" if reach > tiles / 2 + 1e-3 else ""))

    tmp = tempfile.mkdtemp(prefix="look-export-")
    any_player = False
    done = {}
    # The idle frame first: its colours colour the stages and the rubble.
    order = sorted(((j, i) for j, (_, _, frames) in enumerate(layout) for i in range(len(frames))),
                   key=lambda ji: layout[ji[0]][0] != "idle")
    pal = None
    try:
        for j, i in order:
            anim, facing, frames = layout[j]
            yaw = FACINGS[facing] if turns else 0.0
            spec = dict(frames[i], palette=pal)
            beauty, mask, driven = render_cell(builder, kwargs, scale, yaw, frame, anchor,
                                               tiles, args.samples, tmp,
                                               "%s_%s_%d" % (anim, facing, i), spec)
            any_player |= bool(driven)
            done[j, i] = finish(beauty, mask)
            if anim == "idle" and pal is None:
                pal = stages.palette(beauty, mask)
                print("  palette: " + ", ".join("%s %s" % (k, "#%02x%02x%02x" % tuple(
                    int(round(v * 255)) for v in c)) for k, c in pal.items()))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    rows = [[done[j, i] for i in range(len(frames))] for j, (_, _, frames) in enumerate(layout)]
    for j, row in enumerate(rows):
        for i, (rgba, m) in enumerate(row):
            edge = np.concatenate([rgba[0, :, 3], rgba[-1, :, 3], rgba[:, 0, 3], rgba[:, -1, 3]])
            if edge.max() > 2:
                print("  ** cell %d,%d touches its edge (alpha %d)" % (j, i, edge.max()))
    before = frame
    rows, frame, anchor = union_trim(rows, frame, anchor)
    print("trimmed %s -> %s, anchor %s" % (before, frame, anchor))

    ncols = max(len(r) for r in rows)
    w, h = frame
    sheet = np.zeros((h * len(rows), w * ncols, 4), np.uint8)
    msheet = np.zeros((h * len(rows), w * ncols), np.uint8)
    for j, row in enumerate(rows):
        for i, (rgba, m) in enumerate(row):
            sheet[j * h:(j + 1) * h, i * w:(i + 1) * w] = rgba
            msheet[j * h:(j + 1) * h, i * w:(i + 1) * w] = m

    out = os.path.join(args.out, args.name)
    os.makedirs(out, exist_ok=True)
    Image.fromarray(sheet, "RGBA").save(os.path.join(out, args.name + ".png"), optimize=True)
    mask_path = os.path.join(out, args.name + ".mask.png")
    if any_player:
        Image.fromarray(msheet, "L").save(mask_path, optimize=True)
    elif os.path.exists(mask_path):
        os.remove(mask_path)
    anims = []
    for anim, facing, frames in layout:
        if not anims or anims[-1][0] != anim:
            anims.append((anim, len(frames)))
    source = args.target + ("(%s)" % args.kwargs if kwargs else "") + \
        (" fitted to %g tiles" % args.fit if args.fit else "")
    geo = fingerprint(builder, kwargs, scale, layout)
    build(builder, kwargs, GREY, scale, 0.0)
    points = attach_points(kwargs, anchor, frame, fires)
    if points:
        print("points: " + ", ".join("%s %s" % p for p in points))
    with open(os.path.join(out, args.name + ".ron"), "w") as fh:
        fh.write(manifest(args.name, args.cls, anchor, frame, anims, any_player, source,
                          args.hash, geo, points, json.loads(args.timing)))
    dbg = os.path.join(args.debug, args.name + ".debug.png")
    debug_png(dbg, sheet, anchor, tiles, frame)
    print("wrote %s (%.1fs); debug %s" % (out, time.time() - t_start, dbg))


def selftest(args):
    """A flat white 1 x 1 tile at the origin, rendered through the export
    camera: its coverage must be 128 x 64 / 2 px^2, centred on the anchor
    corner, 128 px wide at the anchor and 64 px tall."""
    frame, anchor = (200, 160), (100, 100)
    lib.reset()
    lib.mesh([(-0.5, -0.5, 0), (0.5, -0.5, 0), (0.5, 0.5, 0), (-0.5, 0.5, 0)], [(0, 1, 2, 3)],
             lib.mat("white", emit=1.0), name="tile")
    bpy.context.scene.world = bpy.data.worlds.new("w")
    cam = camera(frame, anchor)
    err, zpx = verify_camera(cam, frame, anchor, 1)
    render_settings(16, denoise=False, bounces=0)
    a = render_to(os.path.join(args.debug, "selftest.png"))[..., 3]
    ys, xs = np.mgrid[0:frame[1], 0:frame[0]] + 0.5          # pixel centres
    area = a.sum()
    cx, cy = (a * xs).sum() / area, (a * ys).sum() / area
    row = a[anchor[1] - 1:anchor[1] + 1].sum(axis=1)         # rows straddling the anchor
    col = a[:, anchor[0] - 1:anchor[0] + 1].sum(axis=0)
    print("projection error vs Blender: %.5f px; one unit of height: %.3f px (expect 78.384)"
          % (err, -zpx))
    print("tile coverage %.1f px^2 (expect 4096), centroid (%.3f, %.3f) (expect %s)"
          % (area, cx, cy, anchor))
    print("widths of the two rows at the anchor: %.2f %.2f (expect 126 each: the diamond "
          "at half a pixel off its waist)" % tuple(row))
    print("heights of the two columns at the anchor: %.2f %.2f (expect 63.5 each)" % tuple(col))
    ok = (err < 0.01 and abs(area - 4096) < 8 and abs(cx - anchor[0]) < 0.05
          and abs(cy - anchor[1]) < 0.05 and np.allclose(row, 126, atol=0.3)
          and np.allclose(col, 63.5, atol=0.3))
    print("selftest", "OK" if ok else "FAILED")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

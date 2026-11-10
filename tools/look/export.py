"""Export a look builder as a colour sprite set (docs/look/colour-sets.md).

    /usr/bin/python3.13 tools/look/export.py SET_NAME MODULE.FUNC \
        [--class MediumBuilding] [--tiles N] [--kwargs '{"variant": 1}'] \
        [--fit TILES [--model-tiles N]] [--samples 64] [--out assets/look] \
        [--debug DIR]

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
frame (spec "sheet"). Only static subjects are exported now (one "idle"
frame, facing S, building unturned), but `cells()` and `FACINGS` are where
animations and facings go.
"""
import os
import sys

# terrain._prop seeds its RNG with hash(kind): fix the hash seed so a prop
# comes out the same shape on every run (and in both builds below).
if os.environ.get("PYTHONHASHSEED") != "0":
    os.environ["PYTHONHASHSEED"] = "0"
    os.execv(sys.executable, [sys.executable] + sys.argv)

import argparse  # noqa: E402
import importlib  # noqa: E402
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

# ------------------------------------------------------------------ scale

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


def project(p, anchor):
    """World points (N, 3) to continuous pixel coordinates (x right, y down)
    in a cell whose origin lands on `anchor`."""
    p = np.atleast_2d(np.asarray(p, float))
    return np.stack([anchor[0] + p @ RIGHT * PPU, anchor[1] - p @ UP * PPU], axis=1)


# ------------------------------------------------------------------ build

def load_builder(target):
    mod, fn = target.rsplit(".", 1)
    return getattr(importlib.import_module(mod), fn)


def build(builder, kwargs, player, scale=1.0, yaw_deg=0.0):
    """A fresh scene with the subject at the origin, under a turntable empty
    that carries the export's fit scale and the facing."""
    lib.reset()
    turn = lib.empty("export_turntable")
    root = builder(player=player, **kwargs) if _takes_player(builder) else builder(**kwargs)
    root.parent = turn
    turn.scale = (scale, scale, scale)
    turn.rotation_euler = (0, 0, math.radians(yaw_deg))
    bpy.context.view_layer.update()
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


def player_slots(builder, kwargs, scale, yaw):
    """Values of a build with the ODD player colour, by slot key."""
    build(builder, kwargs, ODD, scale, yaw)
    catcher()             # as the GREY build will have it, so the slots line up
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

def cells(class_name):
    """The sheet's rows and columns: [(animation, facing, [frame, ...]), ...].
    Static subjects only for now: one idle frame, the S view (a building is
    rendered as modelled, square to the grid)."""
    return [("idle", "S", [0])]


def render_cell(builder, kwargs, scale, yaw, frame, anchor, tiles, samples, tmp, tag):
    """Beauty RGBA (straight, 0..1) and mask (0..1) of one cell."""
    t0 = time.time()
    # 1. the ODD build: which colour values the player drives
    odd = player_slots(builder, kwargs, scale, yaw)
    # 2. the GREY build, lit, over a shadow catcher
    build(builder, kwargs, GREY, scale, yaw)
    catcher()
    lib.light()
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

def manifest(name, cls, anchor, frame, anims, has_mask, source):
    lines = [
        "// Generated by tools/look/export.py from %s." % source,
        "// A colour set (docs/look/colour-sets.md): re-export rather than edit.",
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
        lines.append('        (name: "%s", frames: %d, frame_ms: 160, loops: true, impact: None),'
                     % (a, n))
    lines += [
        "    ],",
        "    anchor_overrides: [],",
        "    colour: true,",
        "    mask: %s," % ('Some("%s.mask.png")' % name if has_mask else "None"),
        "    frame: Some((%d, %d))," % frame,
        ")",
        "",
    ]
    return "\n".join(lines)


def debug_png(path, rgba, anchor, tiles, frame):
    """The sprite over grass with its footprint diamond (and the tile grid
    under it) drawn at the anchor, 2x, for checking by eye."""
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
    over = over.resize((w * 2, h * 2), Image.NEAREST)
    over.save(path)


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
    ap.add_argument("--samples", type=int, default=64)
    ap.add_argument("--out", default=os.path.join(os.path.dirname(os.path.dirname(HERE)),
                                                  "assets", "look"))
    ap.add_argument("--debug", default=os.path.join(tempfile.gettempdir(), "look-export"),
                    help="where the debug PNG goes")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    os.makedirs(args.debug, exist_ok=True)
    if args.selftest:
        return selftest(args)
    if not (args.name and args.target):
        ap.error("SET_NAME and MODULE.FUNC are required")

    t_start = time.time()
    builder = load_builder(args.target)
    kwargs = json.loads(args.kwargs)
    tiles = args.tiles or CLASSES[args.cls][0]
    turns = CLASSES[args.cls][1]

    scale = 1.0
    if args.fit:
        build(builder, kwargs, GREY)
        model = args.model_tiles or model_tiles_estimate()
        scale = args.fit / model
        print("fit: model %.2f tiles -> %.2f tiles, scale %.4f" % (model, args.fit, scale))

    layout = cells(args.cls)
    # One cell size and anchor for the whole sheet: the union of every cell's.
    lo, hi = np.array([1e9, 1e9]), np.array([-1e9, -1e9])
    for anim, facing, frames in layout:
        yaw = FACINGS[facing] if turns else 0.0
        for f in frames:
            build(builder, kwargs, GREY, scale, yaw)
            lib.light()
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

    rows = []
    tmp = tempfile.mkdtemp(prefix="look-export-")
    any_player = False
    try:
        for anim, facing, frames in layout:
            yaw = FACINGS[facing] if turns else 0.0
            row = []
            for f in frames:
                beauty, mask, driven = render_cell(builder, kwargs, scale, yaw, frame, anchor,
                                                   tiles, args.samples, tmp,
                                                   "%s_%s_%d" % (anim, facing, f))
                any_player |= bool(driven)
                row.append(finish(beauty, mask))
            rows.append(row)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    ncols = max(len(r) for r in rows)
    w, h = frame
    sheet = np.zeros((h * len(rows), w * ncols, 4), np.uint8)
    msheet = np.zeros((h * len(rows), w * ncols), np.uint8)
    for j, row in enumerate(rows):
        for i, (rgba, m) in enumerate(row):
            sheet[j * h:(j + 1) * h, i * w:(i + 1) * w] = rgba
            msheet[j * h:(j + 1) * h, i * w:(i + 1) * w] = m
            edge = np.concatenate([rgba[0, :, 3], rgba[-1, :, 3], rgba[:, 0, 3], rgba[:, -1, 3]])
            if edge.max() > 2:
                print("  ** cell %d,%d touches its edge (alpha %d)" % (j, i, edge.max()))

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
    with open(os.path.join(out, args.name + ".ron"), "w") as fh:
        fh.write(manifest(args.name, args.cls, anchor, frame, anims, any_player, source))
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

"""export.py's path for units (docs/look/units.md): a turning, animated
colour set rendered from a units.py figure.

One Blender scene is set up once (camera, the sun, the shadow catcher, the
contact light) and only the figure is rebuilt between frames: its mesh is
re-posed for each (animation, frame), then turned to each of the five
authored facings S, SW, W, NW, N (the figure turns, not the camera or the
sun; the other three facings are the game's mirrors). Each render is one
Cycles pass written as a multilayer EXR, read back with exr.py: the
denoised beauty and two AOVs, the subject's coverage and its player
coloured share, so the mask needs no second render.

Light: lib.light()'s sun and sky, as the buildings have, but the sun is
linked to the figure only, so it does not throw the figure's long shadow
on the ground; a second sun from almost overhead, linked to the ground
only, gives the figure a soft contact shadow under its feet (and under its
body when it lies).

Resumable: each finished cell is kept in target/look-units/NAME/HASH/ and
a rerun with the same hash renders only what is missing.
"""
import json
import math
import os
import shutil
import time

import numpy as np
from PIL import Image, ImageDraw

import bpy
from mathutils import Vector

import export
import exr
import lib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
WORK = os.path.join(ROOT, "target", "look-units")
FACING_ORDER = ("S", "SW", "W", "NW", "N")
SAMPLES = 20                  # with the denoiser; the AOVs are not denoised
CONTACT = (2.5, 30.0)         # the contact light's strength and angular size (degrees)
DECAY_FADE = 0.55             # the corpse's alpha on the decay's last frame


def set_scale(scale):
    """Render at `scale` authored pixels per 1x pixel: export.py's camera,
    projection and margins are for 2; a unit may be authored at 1, half
    the resolution and a quarter of the memory (docs/look/units.md,
    "Memory")."""
    export.SCALE = scale
    export.TILE_W, export.TILE_H = 64 * scale, 32 * scale
    export.PPU = export.TILE_W / 2 * math.sqrt(2)
    export.MARGIN = 4 * scale


def _collection(name):
    c = bpy.data.collections.new(name)
    bpy.context.scene.collection.children.link(c)
    return c


class Stage:
    """The scene the figure is rendered in, set up once."""

    def __init__(self, samples, out_dir):
        lib.reset()
        sc = bpy.context.scene
        self.fig_c = _collection("figure")
        self.ground_c = _collection("ground")
        ground = export.catcher()
        sc.collection.objects.unlink(ground)
        self.ground_c.objects.link(ground)
        export.light()
        for ob in sc.objects:
            if ob.type == "LIGHT":
                ob.light_linking.receiver_collection = self.fig_c
        cl = bpy.data.lights.new("contact", "SUN")
        cl.energy, cl.angle = CONTACT[0], math.radians(CONTACT[1])
        co = bpy.data.objects.new("contact", cl)
        sc.collection.objects.link(co)
        co.rotation_euler = (0.0, 0.0, 0.0)          # straight down: the shadow under the figure
        co.light_linking.receiver_collection = self.ground_c
        bpy.context.view_layer.update()
        self.contact_dir = np.array(co.matrix_world)[:3, 2] * -1
        vl = sc.view_layers[0]
        for n in ("player", "subject"):
            a = vl.aovs.add()
            a.name, a.type = n, "VALUE"
        export.render_settings(samples)
        s = sc.render.image_settings
        s.media_type = "MULTI_LAYER_IMAGE"
        s.file_format = "OPEN_EXR_MULTILAYER"
        s.exr_codec = "NONE"
        s.color_depth = "32"
        sc.render.use_persistent_data = True
        self.out_dir = out_dir
        self.root = None
        self.marked = set()

    def place(self, figure, anim, i):
        """The figure in frame i of `anim`, replacing the last one."""
        self.clear()
        self.root = figure.build(anim, i, export.GREY)
        for ob in [self.root] + list(self.root.children_recursive):
            for c in ob.users_collection:
                c.objects.unlink(ob)
            self.fig_c.objects.link(ob)
        self.tag_materials()
        bpy.context.view_layer.update()

    def clear(self):
        if self.root is None:
            return
        for ob in list(self.root.children_recursive) + [self.root]:
            me = ob.data if ob.type == "MESH" else None
            bpy.data.objects.remove(ob)
            if me is not None:
                bpy.data.meshes.remove(me)
        self.root = None

    def tag_materials(self):
        """AOV outputs on every material not yet tagged: `subject` 1 on the
        figure's, `player` 1 on those made from the player colour."""
        import units
        player = units.player_materials(export.GREY)
        for m in bpy.data.materials:
            if m.name in self.marked or m.node_tree is None:
                continue
            ground = m.name.startswith("grass")
            for n, v in (("subject", 0.0 if ground else 1.0),
                         ("player", 1.0 if m.name in player else 0.0)):
                o = m.node_tree.nodes.new("ShaderNodeOutputAOV")
                o.aov_name = n
                o.inputs["Value"].default_value = v
            self.marked.add(m.name)

    def turn(self, yaw):
        self.root.rotation_euler = (0, 0, math.radians(yaw))
        bpy.context.view_layer.update()

    def points(self, yaw):
        self.turn(yaw)
        return export.subject_points()

    def border(self, frame, anchor):
        """Render only the part of the cell the figure and its shadow can
        reach in its current pose (Cycles' render border): a figure fills a
        small part of a cell that holds every pose."""
        pts = export.subject_points()
        k = pts[:, 2:3] / max(1e-6, -self.contact_dir[2])
        xy = export.project(np.concatenate([pts, pts + k * self.contact_dir]), anchor)
        m = export.MARGIN + 8
        x0, y0 = np.floor(xy.min(axis=0)) - m
        x1, y1 = np.ceil(xy.max(axis=0)) + m
        w, h = frame
        self.box = (int(max(0, x0)), int(max(0, y0)), int(min(w, x1)), int(min(h, y1)))
        r = bpy.context.scene.render
        r.use_border, r.use_crop_to_border = True, False
        r.border_min_x, r.border_max_x = max(0.0, x0 / w), min(1.0, x1 / w)
        r.border_min_y, r.border_max_y = max(0.0, 1 - y1 / h), min(1.0, 1 - y0 / h)

    def render(self, tag):
        sc = bpy.context.scene
        path = os.path.join(self.out_dir, tag + ".exr")
        sc.render.filepath = path
        bpy.ops.render.render(write_still=True)
        d = exr.read(path)
        os.remove(path)
        k = "ViewLayer.Combined."
        rgb = np.dstack([d[k + c] for c in "RGB"]).astype(np.float64)
        a = d[k + "A"].astype(np.float64).clip(0, 1)
        subj = d["ViewLayer.subject.X"].astype(np.float64).clip(0, 1)
        play = d["ViewLayer.player.X"].astype(np.float64).clip(0, 1)
        lin = np.where(a[..., None] > 1e-5, rgb / np.maximum(a, 1e-5)[..., None], 0).clip(0, 1)
        srgb = np.where(lin <= 0.0031308, 12.92 * lin, 1.055 * np.power(lin, 1 / 2.4) - 0.055)
        # The shadow is cleaned (and faded out at the edges) inside the
        # render border only, so the border's edge never shows as a line.
        x0, y0, x1, y1 = self.box
        beauty = np.zeros(srgb.shape[:2] + (4,))
        beauty[y0:y1, x0:x1] = export.clean_shadow(
            np.dstack([srgb, a])[y0:y1, x0:x1], np.minimum(subj, a)[y0:y1, x0:x1])
        mask = np.where(beauty[..., 3] > 1e-4, play / np.maximum(beauty[..., 3], 1e-4), 0).clip(0, 1)
        mask[beauty[..., 3] < 1.5 / 255] = 0
        return beauty, mask


def layout(figure):
    """[(animation, facing, frames)] in sheet order: the animations in the
    figure's order, the five facings within each."""
    return [(a, f, n) for a, n, *_ in figure.animations for f in FACING_ORDER]


def bounds(stage, figure, tiles=1):
    """One cell size and anchor holding every frame in every facing, its
    contact shadow and the tile's diamond."""
    lo, hi = np.array([1e9, 1e9]), np.array([-1e9, -1e9])
    for a, n, *_ in figure.animations:
        for i in range(n):
            stage.place(figure, a, i)
            for f in FACING_ORDER:
                (w, h), (ax, ay) = export.frame_for(stage.points(export.FACINGS[f]),
                                                    stage.contact_dir, tiles)
                lo = np.minimum(lo, (-ax, -ay))
                hi = np.maximum(hi, (w - ax, h - ay))
    frame = tuple(int(v + (v % 2)) for v in (hi - lo))
    return frame, (int(-lo[0]), int(-lo[1]))


def contact_sheet(path, rows, frame, anchor, names):
    """Every cell over grass at 2x, one row per (animation, facing), blue
    and red rows alternating by facing, labelled."""
    w, h = frame
    ncols = max(len(r) for r in rows)
    lw = 90
    out = Image.new("RGB", (lw + w * ncols * 2, h * len(rows) * 2), (40, 40, 40))
    blue, red = np.array((40, 90, 230), float), np.array((220, 40, 30), float)
    d = ImageDraw.Draw(out)
    for j, row in enumerate(rows):
        base = blue if (j // 5) % 2 == 0 else red
        for i, (rgba, m) in enumerate(row):
            rgb = rgba[..., :3].astype(float)
            l = (rgb @ np.array([0.299, 0.587, 0.114])) / 255.0
            tint = base[None, None] * (0.3 + 0.95 * l[..., None])
            mm = (m / 255.0)[..., None]
            rgb = rgb * (1 - mm) + tint.clip(0, 255) * mm
            al = rgba[..., 3:4] / 255.0
            bg = np.array((86, 140, 60), float)
            cell = (rgb * al + bg * (1 - al)).clip(0, 255).astype(np.uint8)
            im = Image.fromarray(cell).resize((w * 2, h * 2), Image.NEAREST)
            out.paste(im, (lw + i * w * 2, j * h * 2))
            ImageDraw.Draw(out).line([(lw + i * w * 2 + anchor[0] * 2 - 3, j * h * 2 + anchor[1] * 2),
                                      (lw + i * w * 2 + anchor[0] * 2 + 3, j * h * 2 + anchor[1] * 2)],
                                     fill=(255, 255, 0))
        d.text((4, j * h * 2 + 4), names[j], fill=(255, 255, 255))
    out.save(path)


def main(args, factory):
    """Export the set `args.name` from the figure `factory()` makes."""
    t_start = time.time()
    set_scale(args.scale or 2)
    samples = args.samples or SAMPLES
    # Resumable only for a hashed export (export_all.py's): its cells are
    # what that hash renders. Without a hash, start afresh.
    work = os.path.join(WORK, args.name, args.hash or "nohash")
    if not args.hash:
        shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work, exist_ok=True)
    stage = Stage(samples, work)
    figure = factory()
    rows = layout(figure)
    meta = os.path.join(work, "frame.json")
    if os.path.exists(meta):
        frame, anchor = (tuple(v) for v in json.load(open(meta)))
    else:
        frame, anchor = bounds(stage, figure)
        json.dump([frame, anchor], open(meta, "w"))
    print("frame %s, anchor %s" % (frame, anchor))
    export.camera(frame, anchor)
    cells, rendered, t_render = {}, 0, 0.0
    for a, n, *_ in figure.animations:
        for i in range(n):
            todo = [f for f in FACING_ORDER
                    if not os.path.exists(os.path.join(work, "%s_%s_%d.png" % (a, f, i)))]
            if todo:
                stage.place(figure, a, i)
            for f in todo:
                t0 = time.time()
                stage.turn(export.FACINGS[f])
                stage.border(frame, anchor)
                beauty, mask = stage.render("%s_%s_%d" % (a, f, i))
                if a == "decay":
                    beauty[..., 3] *= 1 - (1 - DECAY_FADE) * i / max(1, n - 1)
                rgba, m = export.finish(beauty, mask)
                tag = os.path.join(work, "%s_%s_%d" % (a, f, i))
                Image.fromarray(m, "L").save(tag + ".mask.png")
                Image.fromarray(rgba, "RGBA").save(tag + ".png")     # last: marks it done
                rendered += 1
                t_render += time.time() - t0
            for f in FACING_ORDER:
                tag = os.path.join(work, "%s_%s_%d" % (a, f, i))
                cells[a, f, i] = (np.asarray(Image.open(tag + ".png").convert("RGBA")),
                                  np.asarray(Image.open(tag + ".mask.png").convert("L")))
        print("  %s: %d frames x 5 facings" % (a, n), flush=True)
    if rendered:
        print("rendered %d cells, %.2f s each" % (rendered, t_render / rendered))
    grid = [[cells[a, f, i] for i in range(n)] for a, f, n in rows]
    for j, row in enumerate(grid):
        for i, (rgba, _) in enumerate(row):
            edge = np.concatenate([rgba[0, :, 3], rgba[-1, :, 3], rgba[:, 0, 3], rgba[:, -1, 3]])
            if edge.max() > 2:
                print("  ** cell %s %s %d touches its edge (alpha %d)" % (rows[j][0], rows[j][1], i,
                                                                       edge.max()))
    before = frame
    grid, frame, anchor = export.union_trim(grid, frame, anchor)
    print("trimmed %s -> %s, anchor %s" % (before, frame, anchor))
    w, h = frame
    ncols = max(len(r) for r in grid)
    sheet = np.zeros((h * len(grid), w * ncols, 4), np.uint8)
    msheet = np.zeros((h * len(grid), w * ncols), np.uint8)
    for j, row in enumerate(grid):
        for i, (rgba, m) in enumerate(row):
            sheet[j * h:(j + 1) * h, i * w:(i + 1) * w] = rgba
            msheet[j * h:(j + 1) * h, i * w:(i + 1) * w] = m
    out = os.path.join(args.out, args.name)
    os.makedirs(out, exist_ok=True)
    Image.fromarray(sheet, "RGBA").save(os.path.join(out, args.name + ".png"), optimize=True)
    has_mask = bool(msheet.any())
    mask_path = os.path.join(out, args.name + ".mask.png")
    if has_mask:
        Image.fromarray(msheet, "L").save(mask_path, optimize=True)
    elif os.path.exists(mask_path):
        os.remove(mask_path)
    anims = [(a, n) for a, n, *_ in figure.animations]
    timing = {a: (ms, loops) for a, n, ms, loops, imp in figure.animations}
    impacts = {a: imp for a, n, ms, loops, imp in figure.animations}
    with open(os.path.join(out, args.name + ".ron"), "w") as fh:
        fh.write(export.manifest(args.name, args.cls, anchor, frame, anims, has_mask,
                                 args.target, args.hash, None, (), timing, impacts))
    dbg = os.path.join(args.debug, args.name + ".debug.png")
    contact_sheet(dbg, grid, frame, anchor, ["%s %s" % (a, f) for a, f, _ in rows])
    print("wrote %s (%.1fs); contact sheet %s" % (out, time.time() - t_start, dbg))
    if not args.keep_work:
        shutil.rmtree(work, ignore_errors=True)
    return 0


def preview(name, out, only=None, samples=12):
    """A contact sheet of some of a figure's animations through the
    exporter's own scene, nothing written to assets (for working on a
    figure):

        /usr/bin/python3.13 tools/look/unitexport.py villager OUT.png [chop,mine]
    """
    import tempfile
    import units
    tmp = tempfile.mkdtemp(prefix="look-unit-")
    stage = Stage(samples, tmp)
    fig = units.figure(name)
    anims = [a for a in fig.animations if not only or a[0] in only]
    fig.animations = anims
    frame, anchor = bounds(stage, fig)
    export.camera(frame, anchor)
    grid, names = [], []
    for a, n, *_ in anims:
        for f in FACING_ORDER:
            row = []
            for i in range(n):
                stage.place(fig, a, i)
                stage.turn(export.FACINGS[f])
                stage.border(frame, anchor)
                beauty, mask = stage.render("p")
                row.append(export.finish(beauty, mask))
            grid.append(row)
            names.append("%s %s" % (a, f))
    contact_sheet(out, grid, frame, anchor, names)
    shutil.rmtree(tmp, ignore_errors=True)
    print("wrote", out)


if __name__ == "__main__":
    import sys
    preview(sys.argv[1], sys.argv[2], sys.argv[3].split(",") if len(sys.argv) > 3 else None)

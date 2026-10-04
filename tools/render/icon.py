"""Renders the app's icon: the Town Center, as the game draws it, on a
rounded tile in the colours of its grass.

    blender --background --python tools/render/icon.py

Writes packaging/macos/icon.png (1024 x 1024) and packaging/macos/AppIcon.icns,
which scripts/bundle-mac.sh puts in the app. Like the sprites, the icon is
rendered art: it needs Blender, so it is committed rather than rebuilt in CI.
The .icns is written here, in plain Python, so making it needs no Mac.
"""

import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import bpy  # noqa: E402
import numpy as np  # noqa: E402

import kit  # noqa: E402
import rig as rig_module  # noqa: E402
import slice as subjects  # noqa: E402

SIZE = 1024
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT = os.path.join(ROOT, "packaging", "macos")
WORK = os.path.join(ROOT, "target", "renders", "icon")
# The first player's colour, where the model wears its owner's.
BLUE = kit.srgb(0.22, 0.42, 0.86)


def render_town_center(path):
    kit.clear()
    root = subjects.town_center()
    scene = bpy.context.scene
    rig = rig_module.load_rig()
    rig_module.build(rig, scene)
    rig_module.set_class(rig, scene, "LargeBuilding")
    for mat in bpy.data.materials:
        if mat.name == "player":
            for node in mat.node_tree.nodes:
                if node.bl_idname == "ShaderNodeMix":
                    node.inputs["A"].default_value = BLUE + (1.0,)
    scene.render.resolution_x = SIZE
    scene.render.resolution_y = SIZE
    cam = scene.camera
    # Square on the building, a little above its middle so the flag shows.
    cam.data.ortho_scale = 5.0
    base = rig["camera"]["location"]
    up = rig["camera"]["basis"]["up"]
    cam.location = [base[i] + up[i] * 0.55 for i in range(3)]
    scene.cycles.samples = 128
    scene.frame_set(4)
    root.location = (0.0, 0.0, 0.0)
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)


def rounded_mask(size, inset, radius):
    """1 inside a rounded square `inset` from each edge, softened by a pixel."""
    y, x = np.mgrid[0:size, 0:size].astype(np.float32) + 0.5
    lo, hi = inset + radius, size - inset - radius
    dx = np.maximum(np.maximum(lo - x, x - hi), 0.0)
    dy = np.maximum(np.maximum(lo - y, y - hi), 0.0)
    d = np.sqrt(dx * dx + dy * dy) - radius
    return np.clip(0.5 - d, 0.0, 1.0)


def blur(a, passes, r):
    for _ in range(passes):
        a = sum(np.roll(a, k, axis=0) for k in range(-r, r + 1)) / (2 * r + 1)
        a = sum(np.roll(a, k, axis=1) for k in range(-r, r + 1)) / (2 * r + 1)
    return a


def load(path):
    img = bpy.data.images.load(path)
    w, h = img.size
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)
    bpy.data.images.remove(img)
    # Blender's rows run bottom to top; work top to bottom.
    return px[::-1]


def save(px, path):
    h, w = px.shape[:2]
    img = bpy.data.images.new(os.path.basename(path), w, h, alpha=True)
    img.pixels = px[::-1].ravel().tolist()
    img.filepath_raw = path
    img.file_format = "PNG"
    img.save()
    bpy.data.images.remove(img)


def compose(building):
    """The icon: a rounded tile on the macOS grid (824 of 1024, radius 185),
    grass greens lit from the top, its own soft shadow, the building on it."""
    inset, radius = 100, 185
    tile = rounded_mask(SIZE, inset, radius)
    t = np.linspace(0.0, 1.0, SIZE, dtype=np.float32)[:, None]
    top, bottom = np.array([0.55, 0.70, 0.36]), np.array([0.24, 0.40, 0.18])
    grass = top * (1.0 - t)[..., None] + bottom * t[..., None]
    grass = np.broadcast_to(grass, (SIZE, SIZE, 3)).copy()
    # A pool of light behind the building.
    y, x = np.mgrid[0:SIZE, 0:SIZE].astype(np.float32)
    glow = np.exp(-(((x - SIZE * 0.5) ** 2 + (y - SIZE * 0.45) ** 2) / (2 * (SIZE * 0.3) ** 2)))
    grass = np.clip(grass * (0.85 + 0.35 * glow[..., None]), 0.0, 1.0)
    out = np.zeros((SIZE, SIZE, 4), dtype=np.float32)
    shadow = blur(np.roll(tile, 14, axis=0), 3, 6) * 0.45
    out[..., 3] = shadow
    out[..., :3] = 0.0
    # The tile over its shadow.
    a = tile
    out[..., :3] = grass * a[..., None] + out[..., :3] * (1.0 - a[..., None])
    out[..., 3] = a + out[..., 3] * (1.0 - a)
    # The building over the tile, kept inside it.
    ba = building[..., 3] * tile
    out[..., :3] = building[..., :3] * ba[..., None] + out[..., :3] * (1.0 - ba[..., None])
    out[..., 3] = ba + out[..., 3] * (1.0 - ba)
    # Straight alpha for the PNG.
    rgb = np.where(out[..., 3:4] > 0.0, out[..., :3] / np.maximum(out[..., 3:4], 1e-6), 0.0)
    out[..., :3] = np.clip(rgb, 0.0, 1.0)
    return out


def halve(px):
    """Box-filters to half size, colour weighted by alpha."""
    a = px[..., 3:4]
    pre = px[..., :3] * a
    def pool(v):
        return (v[0::2, 0::2] + v[1::2, 0::2] + v[0::2, 1::2] + v[1::2, 1::2]) / 4.0
    pa = pool(a)
    rgb = np.where(pa > 0.0, pool(pre) / np.maximum(pa, 1e-6), 0.0)
    return np.concatenate([rgb, pa], axis=2)


def icns(sizes, path):
    """An .icns of PNGs: each type is the pixel size it holds."""
    types = {16: [b"icp4"], 32: [b"icp5", b"ic11"], 64: [b"ic12"], 128: [b"ic07"],
             256: [b"ic08", b"ic13"], 512: [b"ic09", b"ic14"], 1024: [b"ic10"]}
    body = b""
    for size, png_path in sorted(sizes.items()):
        data = open(png_path, "rb").read()
        for t in types[size]:
            body += t + struct.pack(">I", len(data) + 8) + data
    with open(path, "wb") as f:
        f.write(b"icns" + struct.pack(">I", len(body) + 8) + body)


def main():
    os.makedirs(WORK, exist_ok=True)
    render = os.path.join(WORK, "town_center.png")
    render_town_center(render)
    icon = compose(load(render))
    save(icon, os.path.join(OUT, "icon.png"))
    sizes = {}
    px = icon
    size = SIZE
    while size >= 16:
        path = os.path.join(WORK, "icon_%d.png" % size)
        save(px, path)
        sizes[size] = path
        px = halve(px)
        size //= 2
    icns(sizes, os.path.join(OUT, "AppIcon.icns"))
    print("wrote %s and %s" % (os.path.join(OUT, "icon.png"),
                               os.path.join(OUT, "AppIcon.icns")))


if __name__ == "__main__":
    main()

"""Look at the ground textures roughly as the game will draw them.

    /usr/bin/python3.13 tools/look/ground_preview.py OUT_DIR [--tex assets/look/terrain]
        [--zoom 2] [--decor]

writes OUT_DIR/ground_iso.png: a 12 x 8 tile-diamond view of a test map (a
few patches of every ground type) drawn with numpy the way docs/look/ground.md
says (texture at the pixel's own world position, the four surrounding tile
centres' types blended bilinearly after a noise wobble of up to 0.35 tile,
foam where the water weight is 0.35..0.6, static water at time 0), and
OUT_DIR/ground_flat_NAME.png: each texture tiled 2 x 2, flat, to check the
seams (with the seam error printed: the mean step across the wrap edge over
the mean step between neighbouring texels inside).

With --sheet, OUT_DIR/decor_sheet.png: every decor_* set (and tree) over
painted grass, at the game's scale and doubled. With --stand,
OUT_DIR/tree_stand.png: a 6 x 6 block of `tree`, one per tile, on grass
with a forest floor under it, at the game's scale.

With --decor, scenery sprites from assets/look/decor_* are scattered over
grass (and reeds by water), and trees from assets/look/tree on forest, in
depth order, so the whole ground can be judged together.
"""
import argparse
import math
import os
import re
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
NAMES = ["grass", "dirt", "desert", "sand", "shallow", "deep", "forest", "snow"]
G, D, DS, S, SH, DP, F, SN = range(8)
_F = np.float32


def test_map(n=26):
    """A small world with patches of every type."""
    m = np.full((n, n), G, np.int32)
    y, x = np.mgrid[0:n, 0:n] + 0.5
    rng = np.random.default_rng(3)
    wob = rng.uniform(-0.6, 0.6, (n, n))
    # sea on the far (top) edge: small x + y on screen is up... the far side
    # of the screen is small x + y; put deep water there, shallow, then beach
    s = x + y + wob * 1.5
    m[s < 15.5] = S
    m[s < 13.5] = SH
    m[s < 11.0] = DP
    # a dirt road and a dirt patch
    m[(np.abs(x - y - 2 + wob) < 0.9) & (s > 15.5)] = D
    m[(np.hypot(x - 15, y - 14) + wob < 2.4)] = D
    # desert to the right (large x - y), snow to the left (large y - x)
    m[(x - y + wob > 7.5) & (s > 15.5)] = DS
    m[(y - x + wob > 8.0) & (s > 14.0)] = SN
    # a forest block and a pond
    m[(np.abs(x - 20.5) < 3 + wob) & (np.abs(y - 18.5) < 2.5)] = F
    m[(np.hypot(x - 13, y - 21) + wob < 1.7)] = SH
    return m


def load(texdir):
    tex = [np.asarray(Image.open(os.path.join(texdir, k + ".png")).convert("RGB"), _F) / 255
           for k in NAMES]
    nz = np.asarray(Image.open(os.path.join(texdir, "noise.png")).convert("L"), _F) / 255
    return tex, nz


def bil(img, u, v):
    n = img.shape[0]
    u = u - 0.5
    v = v - 0.5
    u0, v0 = np.floor(u), np.floor(v)
    fu, fv = u - u0, v - v0
    u0 = u0.astype(np.int64) % n
    v0 = v0.astype(np.int64) % n
    u1, v1 = (u0 + 1) % n, (v0 + 1) % n
    if img.ndim == 3:
        fu, fv = fu[..., None], fv[..., None]
    a = img[v0, u0] * (1 - fu) + img[v0, u1] * fu
    b = img[v1, u0] * (1 - fu) + img[v1, u1] * fu
    return a * (1 - fv) + b * fv


def ground(tex, nz, m, wx, wy):
    """Colour of the ground at world points (wx, wy), per docs/look/ground.md."""
    n = m.shape[0]
    # the wobble: noise.png covers 8 x 8 tiles (32 texels a tile)
    ox = (bil(nz, wx * 32, wy * 32) - 0.5) * 0.7
    oy = (bil(nz, wy * 32 + 101, wx * 32 + 57) - 0.5) * 0.7
    px, py = wx + ox - 0.5, wy + oy - 0.5
    i0, j0 = np.floor(px).astype(np.int64), np.floor(py).astype(np.int64)
    fx, fy = px - i0, py - j0
    w = np.zeros(wx.shape + (8,), _F)
    for di, dj, k in ((0, 0, (1 - fx) * (1 - fy)), (1, 0, fx * (1 - fy)), (0, 1, (1 - fx) * fy),
                      (1, 1, fx * fy)):
        t = m[np.clip(j0 + dj, 0, n - 1), np.clip(i0 + di, 0, n - 1)]
        np.add.at(w.reshape(-1, 8), (np.arange(t.size), t.ravel()), k.ravel())
    col = np.zeros(wx.shape + (3,), _F)
    u, v = wx * 128, wy * 128
    for k in range(8):
        sel = w[..., k] > 1e-4
        if sel.any():
            col[sel] += bil(tex[k], u[sel], v[sel]) * w[..., k][sel][..., None]
    # foam at the shore
    ww = w[..., SH] + w[..., DP]
    br = bil(nz, wx * 32 * 3.1 + 17, wy * 32 * 3.1 + 5)
    foam = np.clip((ww - 0.35) / 0.05, 0, 1) * np.clip((0.6 - ww) / 0.05, 0, 1)
    foam *= np.clip((br - 0.3) / 0.25, 0, 1)
    col += (np.array([0.97, 0.99, 1.0], _F) - col) * (foam * 0.85)[..., None]
    return col


def parse_ron(path):
    t = open(path).read()
    a = re.search(r"anchor:\s*\((\d+),\s*(\d+)\)", t)
    return int(a.group(1)), int(a.group(2))


def load_sprite(name):
    d = os.path.join(ROOT, "assets", "look", name)
    img = np.asarray(Image.open(os.path.join(d, name + ".png")).convert("RGBA"), _F) / 255
    return img, parse_ron(os.path.join(d, name + ".ron"))


def render(tex, nz, m, zoom, size, centre, ssmp=2, decor=False):
    W, H = size
    tw, th = 64 * zoom, 32 * zoom
    cx, cy = centre
    sx0 = (cx - cy) * tw / 2 - W / 2
    sy0 = (cx + cy) * th / 2 - H / 2
    ys, xs = np.mgrid[0:H * ssmp, 0:W * ssmp].astype(_F)
    sx = sx0 + (xs + 0.5) / ssmp
    sy = sy0 + (ys + 0.5) / ssmp
    a, b = sx / (tw / 2), sy / (th / 2)
    wx, wy = (a + b) / 2, (b - a) / 2
    col = ground(tex, nz, m, wx, wy)
    col = col.reshape(H, ssmp, W, ssmp, 3).mean((1, 3))
    if decor:
        col = add_decor(col, m, zoom, sx0, sy0)
    return col


def add_decor(col, m, zoom, sx0, sy0):
    """Scatter the decor sets over the map and trees on the forest."""
    rng = np.random.default_rng(9)
    n = m.shape[0]
    kinds = {}
    for k in os.listdir(os.path.join(ROOT, "assets", "look")):
        if k.startswith("decor_") or k == "tree":
            kinds[k] = load_sprite(k)
    items = []
    for j in range(n):
        for i in range(n):
            t = m[j, i]
            if t == F and "tree" in kinds:
                items.append(("tree", i + 0.5 + rng.uniform(-0.15, 0.15), j + 0.5 + rng.uniform(-0.15, 0.15)))
                continue
            near_water = any(m[min(max(j + dj, 0), n - 1), min(max(i + di, 0), n - 1)] in (SH, DP)
                             for di in (-1, 0, 1) for dj in (-1, 0, 1))
            pool = []
            if t == G:
                if rng.random() < 0.55:
                    pool = [k for k in kinds if k.startswith(("decor_flowers", "decor_tuft"))]
                    if rng.random() < 0.3:
                        pool = [k for k in kinds if k.startswith(("decor_bush", "decor_rock"))]
                if near_water and rng.random() < 0.6:
                    pool = [k for k in kinds if k.startswith("decor_reeds")]
            elif t in (D, DS, S) and rng.random() < 0.15:
                pool = [k for k in kinds if k.startswith(("decor_rock_0", "decor_rock_1", "decor_tuft_2"))]
            if pool:
                items.append((pool[rng.integers(len(pool))], i + rng.uniform(0.2, 0.8),
                              j + rng.uniform(0.2, 0.8)))
    items.sort(key=lambda it: it[1] + it[2])
    H, W = col.shape[:2]
    out = col.copy()
    for k, x, y in items:
        img, (ax, ay) = kinds[k]
        if zoom != 2:
            f = zoom / 2
            im = Image.fromarray((img * 255).astype(np.uint8), "RGBA")
            im = im.resize((max(1, round(im.width * f)), max(1, round(im.height * f))), Image.LANCZOS)
            img = np.asarray(im, _F) / 255
            ax, ay = ax * f, ay * f
        px = (x - y) * 32 * zoom - sx0 - ax
        py = (x + y) * 16 * zoom - sy0 - ay
        x0, y0 = int(round(px)), int(round(py))
        h, w = img.shape[:2]
        if x0 >= W or y0 >= H or x0 + w <= 0 or y0 + h <= 0:
            continue
        cx0, cy0 = max(0, -x0), max(0, -y0)
        cx1, cy1 = min(w, W - x0), min(h, H - y0)
        s = img[cy0:cy1, cx0:cx1]
        d = out[y0 + cy0:y0 + cy1, x0 + cx0:x0 + cx1]
        a = s[..., 3:4]
        d[:] = d * (1 - a) + s[..., :3] * a
    return out


def paste(out, img, px, py):
    """Composite a straight-alpha sprite (floats) onto out at integer px, py."""
    H, W = out.shape[:2]
    h, w = img.shape[:2]
    x0, y0 = int(round(px)), int(round(py))
    cx0, cy0 = max(0, -x0), max(0, -y0)
    cx1, cy1 = min(w, W - x0), min(h, H - y0)
    if cx1 <= cx0 or cy1 <= cy0:
        return
    s = img[cy0:cy1, cx0:cx1]
    d = out[y0 + cy0:y0 + cy1, x0 + cx0:x0 + cx1]
    a = s[..., 3:4]
    d[:] = d * (1 - a) + s[..., :3] * a


def sheet(tex, nz, out):
    names = sorted(k for k in os.listdir(os.path.join(ROOT, "assets", "look")) if k.startswith("decor_"))
    names.append("tree")
    cw, ch = 220, 240
    cols = 6
    rows = (len(names) + cols - 1) // cols
    W, H = cw * cols, ch * rows
    m = np.full((40, 40), G, np.int32)
    img = render(tex, nz, m, 2, (W, H), (14, 14), ssmp=1)
    for i, k in enumerate(names):
        spr, (ax, ay) = load_sprite(k)
        cx, cy = (i % cols) * cw + cw // 2, (i // cols) * ch + ch - 50
        paste(img, spr, cx - ax, cy - ay)
    Image.fromarray((np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8)).save(os.path.join(out, "decor_sheet.png"))
    big = Image.open(os.path.join(out, "decor_sheet.png"))
    big.resize((W * 2, H * 2), Image.NEAREST).save(os.path.join(out, "decor_sheet_2x.png"))
    print("wrote", os.path.join(out, "decor_sheet.png"))


def stand(tex, nz, out):
    m = np.full((30, 30), G, np.int32)
    m[12:18, 12:18] = F
    W, H = 1100, 760
    img = render(tex, nz, m, 2, (W, H), (15, 14.2))
    spr, (ax, ay) = load_sprite("tree")
    sx0 = (15 - 14.2) * 64 - W / 2
    sy0 = (15 + 14.2) * 32 - H / 2
    pts = [(i + 0.5, j + 0.5) for j in range(12, 18) for i in range(12, 18)]
    pts.sort(key=lambda p: p[0] + p[1])
    for x, y in pts:
        paste(img, spr, (x - y) * 64 - sx0 - ax, (x + y) * 32 - sy0 - ay)
    Image.fromarray((np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8)).save(os.path.join(out, "tree_stand.png"))
    print("wrote", os.path.join(out, "tree_stand.png"))


def seam_error(t):
    """Mean |step| across the wrap edges over the mean |step| inside."""
    du = np.abs(np.diff(t, axis=1)).mean()
    dv = np.abs(np.diff(t, axis=0)).mean()
    eu = np.abs(t[:, 0] - t[:, -1]).mean()
    ev = np.abs(t[0, :] - t[-1, :]).mean()
    return eu / du, ev / dv


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--tex", default=os.path.join(ROOT, "assets", "look", "terrain"))
    ap.add_argument("--zoom", type=float, default=2.0)
    ap.add_argument("--decor", action="store_true")
    ap.add_argument("--centre", default="13,14")
    ap.add_argument("--sheet", action="store_true")
    ap.add_argument("--stand", action="store_true")
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    tex, nz = load(args.tex)
    if args.sheet or args.stand:
        if args.sheet:
            sheet(tex, nz, args.out)
        if args.stand:
            stand(tex, nz, args.out)
        return 0
    for k, t in zip(NAMES, tex):
        eu, ev = seam_error(t)
        flat = np.tile(t, (2, 2, 1))
        Image.fromarray((flat * 255 + 0.5).astype(np.uint8)).save(
            os.path.join(args.out, "ground_flat_%s.png" % k))
        print("%-8s seam step / inner step: u %.2f  v %.2f" % (k, eu, ev))
    eu, ev = seam_error(nz)
    print("noise    seam step / inner step: u %.2f  v %.2f  range %.2f..%.2f" % (eu, ev, nz.min(), nz.max()))
    m = test_map()
    z = args.zoom
    size = (int(12 * 64 * z), int(8 * 64 * z))      # 12 tiles across, 8 tile-heights x 2
    c = tuple(float(v) for v in args.centre.split(","))
    img = render(tex, nz, m, z, size, c, decor=args.decor)
    name = "ground_iso%s%s.png" % ("_decor" if args.decor else "", "" if z == 2 else "_z%g" % z)
    Image.fromarray((np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8)).save(
        os.path.join(args.out, name))
    print("wrote", os.path.join(args.out, name))


if __name__ == "__main__":
    sys.exit(main())

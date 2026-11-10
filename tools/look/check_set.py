"""Check a colour sprite set (docs/look/colour-sets.md) and preview it.

    /usr/bin/python3.13 tools/look/check_set.py assets/look/NAME [--out DIR]

Loads NAME.ron, its sheet and its mask; checks the layout against the
manifest (sheet = frame x (columns, rows), mask the sheet's size, anchors in
the cell, straight 8-bit RGBA, greyscale mask, masked pixels grey); then
composites every cell over green three ways - as it is, owned by a blue
player and owned by a red one, using the spec's tint formula with an 8-step
ramp made by scaling a base colour from dark to light - next to the mask, and
writes OUT/NAME.check.png (2x). Exits non-zero on a layout error.
"""
import argparse
import os
import re
import sys
import tempfile

import numpy as np
from PIL import Image, ImageDraw

TURNING = {"Foot", "Mounted", "Heavy", "Ship"}
FACINGS = 5
GREEN = (86, 140, 60)


def ramp(base):
    """Eight steps from dark to light: the base colour scaled 0.3x .. 1.25x."""
    base = np.array(base, float)
    return np.array([np.clip(base * (0.30 + 0.95 * i / 7), 0, 255) for i in range(8)])


BLUE = ramp((40, 90, 230))
RED = ramp((220, 40, 30))


def parse_ron(text):
    """The few fields this needs, from a SpriteSet manifest."""
    text = re.sub(r"//[^\n]*", "", text)

    def field(name, pat=r'"([^"]*)"'):
        m = re.search(r"\b%s\s*:\s*%s" % (name, pat), text)
        return m.group(1) if m else None
    num2 = r"\(\s*(-?\d+)\s*,\s*(-?\d+)\s*\)"
    m = re.search(r"\banchor\s*:\s*" + num2, text)
    anchor = (int(m.group(1)), int(m.group(2)))
    m = re.search(r"\bframe\s*:\s*Some\(\s*" + num2 + r"\s*\)", text)
    frame = (int(m.group(1)), int(m.group(2))) if m else None
    mask = field("mask", r'Some\(\s*"([^"]*)"\s*\)')
    anims = [(n, int(f)) for n, f in re.findall(
        r'\(\s*name\s*:\s*"([^"]+)"\s*,\s*frames\s*:\s*(\d+)', text)]
    return {
        "name": field("name"), "class": field("class", r"(\w+)"),
        "scale": int(field("scale", r"(\d+)")), "sheet": field("sheet"),
        "anchor": anchor, "frame": frame, "mask": mask, "animations": anims,
        "colour": field("colour", r"(true|false)") == "true",
    }


def tint(rgb, mask, rmp):
    """The spec's formula, in sRGB bytes."""
    l = (rgb @ np.array([0.299, 0.587, 0.114])) / 255.0
    t = np.clip(l, 0, 1) * 7
    i0 = np.floor(t).astype(int).clip(0, 7)
    i1 = np.minimum(i0 + 1, 7)
    f = (t - i0)[..., None]
    tn = rmp[i0] * (1 - f) + rmp[i1] * f
    m = (mask / 255.0)[..., None]
    return rgb * (1 - m) + tn * m


def over_green(rgb, a):
    a = (a / 255.0)[..., None]
    return rgb * a + np.array(GREEN, float) * (1 - a)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("set_dir")
    ap.add_argument("--out", default=os.path.join(tempfile.gettempdir(), "look-check"))
    args = ap.parse_args()
    name = os.path.basename(os.path.normpath(args.set_dir))
    errors = []

    man = parse_ron(open(os.path.join(args.set_dir, name + ".ron")).read())
    if man["name"] != name:
        errors.append("manifest name %r is not the directory's %r" % (man["name"], name))
    if not man["colour"]:
        errors.append("colour: true is missing")
    if man["scale"] != 2:
        errors.append("scale is %d, not 2" % man["scale"])
    if man["frame"] is None:
        errors.append("no frame: Some((w, h))")
    sheet_img = Image.open(os.path.join(args.set_dir, man["sheet"]))
    if sheet_img.mode != "RGBA":
        errors.append("sheet is %s, not 8-bit RGBA" % sheet_img.mode)
    sheet = np.asarray(sheet_img.convert("RGBA")).astype(float)
    H, W = sheet.shape[:2]
    if man["mask"]:
        mimg = Image.open(os.path.join(args.set_dir, man["mask"]))
        if mimg.mode != "L":
            errors.append("mask is %s, not 8-bit greyscale" % mimg.mode)
        mask = np.asarray(mimg.convert("L")).astype(float)
        if mask.shape != (H, W):
            errors.append("mask is %dx%d, the sheet %dx%d" % (mask.shape[1], mask.shape[0], W, H))
            mask = np.zeros((H, W))
    else:
        mask = np.zeros((H, W))

    fw, fh = man["frame"] or (0, 0)
    turns = man["class"] in TURNING
    rows = [(a, f) for a, n in man["animations"] for f in range(FACINGS if turns else 1)]
    cols = max(n for _, n in man["animations"])
    if (W, H) != (fw * cols, fh * len(rows)):
        errors.append("sheet is %dx%d; frame %dx%d x %d columns x %d rows wants %dx%d"
                      % (W, H, fw, fh, cols, len(rows), fw * cols, fh * len(rows)))
    ax, ay = man["anchor"]
    if not (0 <= ax < fw and 0 <= ay < fh):
        errors.append("anchor %s outside the %dx%d cell" % (man["anchor"], fw, fh))

    # Masked pixels must be grey; nothing fully transparent may be masked.
    a = sheet[..., 3]
    full = mask >= 255
    if full.any():
        rgb = sheet[..., :3][full]
        spread = (rgb.max(axis=1) - rgb.min(axis=1)).max()
        l = rgb @ np.array([0.299, 0.587, 0.114])
        print("masked pixels: %d full, %d partial; grey spread max %d; luma p5/p50/p95 "
              "%.0f/%.0f/%.0f" % (full.sum(), ((mask > 0) & ~full).sum(), spread,
                                 *np.percentile(l, [5, 50, 95])))
        if spread > 2:
            errors.append("fully masked pixels are not grey (spread %d)" % spread)
    if ((mask > 0) & (a == 0)).any():
        errors.append("%d masked pixels are fully transparent" % ((mask > 0) & (a == 0)).sum())
    print("%s: %s, sheet %dx%d, frame %dx%d, anchor %s, %d rows x %d columns, mask %s"
          % (name, man["class"], W, H, fw, fh, man["anchor"], len(rows), cols,
             man["mask"] or "none"))
    soft = (a > 0) & (a < 255)
    print("alpha: %d opaque, %d translucent (edges and shadow), %d clear"
          % ((a == 255).sum(), soft.sum(), (a == 0).sum()))

    # Previews: per cell, [as is | blue | red | mask] over green, 2x.
    os.makedirs(args.out, exist_ok=True)
    panels = []
    for r in range(len(rows)):
        for c in range(cols):
            cell = sheet[r * fh:(r + 1) * fh, c * fw:(c + 1) * fw]
            m = mask[r * fh:(r + 1) * fh, c * fw:(c + 1) * fw]
            if cell[..., 3].max() == 0:
                continue
            rgb, al = cell[..., :3], cell[..., 3]
            views = [over_green(rgb, al), over_green(tint(rgb, m, BLUE), al),
                     over_green(tint(rgb, m, RED), al), np.dstack([m, m, m])]
            row = np.concatenate([np.pad(v, ((4, 4), (4, 4), (0, 0)), constant_values=30)
                                  for v in views], axis=1)
            panels.append(row)
    img = Image.fromarray(np.concatenate(panels, axis=0).clip(0, 255).astype(np.uint8), "RGB")
    d = ImageDraw.Draw(img)
    d.text((8, 6), "%s  as is | blue | red | mask" % name, fill=(255, 255, 255))
    img = img.resize((img.width * 2, img.height * 2), Image.NEAREST)
    path = os.path.join(args.out, name + ".check.png")
    img.save(path)
    print("preview", path)
    for e in errors:
        print("ERROR:", e)
    print("OK" if not errors else "FAILED")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())

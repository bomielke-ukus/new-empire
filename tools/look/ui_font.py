"""Bakes the new look's UI typefaces into a glyph atlas (docs/look/ui.md).

    python3.13 tools/look/ui_font.py            # writes assets/ui/font.png, font.txt

Two faces, both under the SIL Open Font License 1.1 (assets/ui/OFL.txt):
Carlito Bold, a sturdy humanist sans, for the HUD's small text, and Caladea
Bold, a serif with some classical weight, for headings and titles. Each is
rendered by FreeType (hinted, antialiased) at every cap height the UI asks
for at the UI scales the game offers, so text is drawn at its own pixel
size and never scaled: crisp at 1x, 1.5x and 2x and between.

The atlas is coverage only (one grey channel); the game colours it with its
inks when it builds its atlas (`view::skin`). The metrics file is plain
text, one record a line:

    face NAME TRACKING_EM                    a face; kerning pairs follow it
    kern A B EM                              kerning between two characters, em
    bake FACE CAP SIZE_PX ASCENT DRAWN       a face at one cap height, and
                                             the height its H is drawn
    glyph CODE X Y W H BX BY ADVANCE         a glyph of the bake above it:
                                             its rectangle in the atlas, its
                                             top left from the pen on the
                                             baseline, and its hinted advance,
                                             whole px (the game rounds the
                                             tracking and kerning to whole px
                                             too, so pens stay on the grid)

The text the UI draws is upper case (`view::font`), so lower case is not
baked; the small sizes carry all printable ASCII but lower case, the large
title sizes only what titles and banners use.
"""
import os
import sys

from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FONTS = "/usr/share/fonts/truetype/crosextra"
FACES = {
    # name: (file, tracking in em, caps baked, caps baked with the title set)
    "sans": ("Carlito-Bold.ttf", 0.035, [7, 9, 11, 14, 18, 21, 28], []),
    "serif": ("Caladea-Bold.ttf", 0.07, [14, 18, 21, 26, 28], [32, 35, 42, 52, 56]),
}
FULL = "".join(chr(c) for c in range(32, 127) if not ("a" <= chr(c) <= "z"))
TITLE = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 '.,:!-?"
ATLAS_W = 1024


def cap_height(path, size):
    f = ImageFont.truetype(path, size, layout_engine=ImageFont.Layout.RAQM)
    l, t, r, b = f.getbbox("H", anchor="ls")
    return b - t


def size_for(path, cap):
    """The pixel size whose H is `cap` tall unhinted: the face's own
    proportions, which the hinter then snaps to the pixel grid."""
    em = cap_height(path, 1000) / 1000
    size = cap / em
    # Nudged until the hinted H is exactly `cap`, the nearest size that is.
    step = -0.05 if cap_height(path, size) > cap else 0.05
    s = size
    while abs(s - size) < 0.2 * size and cap_height(path, s) != cap:
        s += step
    return s if cap_height(path, s) == cap else size


def kerning(path, chars):
    """Kerning pairs in em, from the shaper at a large size."""
    big = 200
    f = ImageFont.truetype(path, big, layout_engine=ImageFont.Layout.RAQM)
    adv = {c: f.getlength(c) for c in chars}
    out = []
    for a in chars:
        for b in chars:
            if a == " " or b == " ":
                continue
            k = f.getlength(a + b) - adv[a] - adv[b]
            if abs(k) >= 0.01 * big:
                out.append((a, b, k / big))
    return out


def render(path, size, chars):
    f = ImageFont.truetype(path, size, layout_engine=ImageFont.Layout.RAQM)
    # The hinted advance, a whole pixel, so a line's pens land on the pixel
    # grid the glyphs were hinted for and the spacing stays even.
    hinted = ImageFont.truetype(path, size, layout_engine=ImageFont.Layout.BASIC)
    ascent, _ = f.getmetrics()
    glyphs = []
    for ch in chars:
        adv = hinted.getlength(ch)
        if ch == " ":
            glyphs.append((ch, None, 0, 0, adv))
            continue
        pad = int(size)
        canvas = Image.new("L", (int(size * 3) + 2 * pad, int(size * 3) + 2 * pad), 0)
        ox, oy = pad, pad + int(size * 2)
        ImageDraw.Draw(canvas).text((ox, oy), ch, font=f, fill=255, anchor="ls")
        box = canvas.getbbox()
        if box is None:
            glyphs.append((ch, None, 0, 0, adv))
            continue
        img = canvas.crop(box)
        glyphs.append((ch, img, box[0] - ox, box[1] - oy, adv))
    return ascent, glyphs


def main():
    out_dir = os.path.join(ROOT, "assets", "ui")
    os.makedirs(out_dir, exist_ok=True)
    lines = []
    placed = []  # (img, record index)
    for face, (file, track, caps, title_caps) in FACES.items():
        path = os.path.join(FONTS, file)
        lines.append(f"face {face} {track:.4f}")
        for a, b, k in kerning(path, [c for c in FULL]):
            lines.append(f"kern {ord(a)} {ord(b)} {k:.4f}")
        for cap, chars in [(c, FULL) for c in caps] + [(c, TITLE) for c in title_caps]:
            size = size_for(path, cap)
            ascent, glyphs = render(path, size, chars)
            drawn = cap_height(path, size)
            lines.append(f"bake {face} {cap} {size:.2f} {ascent} {drawn}")
            for ch, img, bx, by, adv in glyphs:
                lines.append([ord(ch), img, bx, by, adv])
                if img is not None:
                    placed.append(len(lines) - 1)
    # Shelf-pack the glyphs, tallest first, one pixel apart.
    order = sorted(placed, key=lambda i: -lines[i][1].height)
    x = y = shelf = 0
    spots = {}
    for i in order:
        img = lines[i][1]
        if x + img.width + 1 > ATLAS_W:
            x, y, shelf = 0, y + shelf + 1, 0
        spots[i] = (x, y)
        x += img.width + 1
        shelf = max(shelf, img.height)
    height = y + shelf
    atlas = Image.new("L", (ATLAS_W, height), 0)
    text = []
    for i, rec in enumerate(lines):
        if isinstance(rec, str):
            text.append(rec)
            continue
        code, img, bx, by, adv = rec
        if img is None:
            text.append(f"glyph {code} 0 0 0 0 0 0 {adv:.3f}")
            continue
        gx, gy = spots[i]
        atlas.paste(img, (gx, gy))
        text.append(f"glyph {code} {gx} {gy} {img.width} {img.height} {bx} {by} {adv:.3f}")
    atlas.save(os.path.join(out_dir, "font.png"), optimize=True)
    with open(os.path.join(out_dir, "font.txt"), "w") as f:
        f.write("# Baked by tools/look/ui_font.py; see its docstring. Do not edit.\n")
        f.write("\n".join(text) + "\n")
    print(f"font.png {ATLAS_W}x{height}, {len(placed)} glyphs", file=sys.stderr)


if __name__ == "__main__":
    main()

"""Paint the ground textures (docs/look/ground.md) with numpy and Pillow.

    /usr/bin/python3.13 tools/look/ground.py [--out assets/look/terrain] [--only grass,dirt]

writes grass.png, dirt.png, desert.png, sand.png, shallow.png, deep.png,
forest.png and snow.png (512 x 512 RGB, 4 x 4 tiles, tiling on both axes) and
noise.png (256 x 256 grey, 8 x 8 tiles, tiling). Every texture is made from
fixed seeds, so a run gives the same bytes.

How the painting tiles
----------------------
- Noise is made in the frequency domain (white noise, filtered, transformed
  back), so it is periodic on the 512 grid by construction; anisotropic
  noise is the same with the frequencies squeezed along one direction.
- Strokes, pebbles and leaves are drawn with Pillow on a supersampled canvas
  with a margin round the 512 square; the margins are folded back onto the
  opposite edges ("over"), so a pebble cut by the edge reappears whole.
- Relief (pebbles, ripples, drifts) is a periodic height field lit from the
  sun of tools/look/lib.py (toward the sun: +x, -y), shaded with periodic
  differences.

Seen through the game's projection
----------------------------------
Texel (u, v) is world (x, y) * 128. On screen x runs along u - v and y along
u + v at half the scale, so the texture diagonal u = v is screen-vertical and
is foreshortened 2:1. Things that stand up (grass blades, dry wisps) are
drawn along that diagonal and twice as long as they should look; things that
lie flat (pebbles, leaves, shells) are drawn round and come out as ellipses
like the rest of the ground.
"""
import argparse
import math
import os

import numpy as np
from PIL import Image, ImageDraw

N = 512                     # texels across a texture (4 tiles)
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(os.path.dirname(os.path.dirname(HERE)), "assets", "look", "terrain")
NAMES = ["grass", "dirt", "desert", "sand", "shallow", "deep", "forest", "snow"]

SUN = np.array([0.30, -0.80])           # toward the sun, in (u, v) = (x, y)
SUN = SUN / np.linalg.norm(SUN)
UPSCR = np.array([-1.0, -1.0]) / math.sqrt(2)     # screen-up, in (u, v)
_F = np.float32


# --------------------------------------------------------------- noise

def _erf(x):
    # Abramowitz-Stegun 7.1.26: enough to flatten a histogram
    s = np.sign(x)
    x = np.abs(x)
    t = 1 / (1 + 0.3275911 * x)
    y = 1 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
             + 0.254829592) * t * np.exp(-x * x)
    return s * y


def noise(seed, size, n=N, stretch=1.0, angle=0.0, octaves=1, gain=0.5, uniform=True):
    """Periodic smooth noise of feature size `size` texels, 0..1.

    stretch > 1 elongates the features `stretch` times along `angle`
    (radians, in (u, v); 0 is along u). octaves add finer copies (fbm).
    uniform=True flattens the histogram (good for thresholds)."""
    rng = np.random.default_rng(seed)
    k = np.fft.fftfreq(n)
    ku, kv = np.meshgrid(k, k)
    c, s = math.cos(angle), math.sin(angle)
    ka = (ku * c + kv * s) * stretch          # along the stretch: lower frequencies
    kb = -ku * s + kv * c
    r2 = ka * ka + kb * kb
    tot = np.zeros((n, n))
    amp = 1.0
    for o in range(octaves):
        sig = size / 2.6 / (2 ** o)
        w = rng.standard_normal((n, n))
        f = np.exp(-2 * math.pi ** 2 * sig ** 2 * r2)
        f[0, 0] = 0
        a = np.real(np.fft.ifft2(np.fft.fft2(w) * f))
        tot += amp * a / (a.std() + 1e-12)
        amp *= gain
    tot /= tot.std() + 1e-12
    if uniform:
        return (0.5 * (1 + _erf(tot / math.sqrt(2)))).astype(_F)
    return (0.5 + tot / 5.0).clip(0, 1).astype(_F)


def ss(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def sample(img, u, v):
    """Bilinear, wrapping sample of a (n, n[, c]) array at texel coords."""
    n = img.shape[0]
    u = np.asarray(u, _F) - 0.5
    v = np.asarray(v, _F) - 0.5
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


def warp(img, du, dv):
    """img displaced by the fields du, dv (texels), wrapping."""
    n = img.shape[0]
    v, u = np.mgrid[0:n, 0:n].astype(_F) + 0.5
    return sample(img, u + du, v + dv)


def relief(h, k):
    """Sun shading of a periodic height field (texel units): ~1, brighter
    on slopes facing the sun."""
    hu = (np.roll(h, -1, 1) - np.roll(h, 1, 1)) / 2
    hv = (np.roll(h, -1, 0) - np.roll(h, 1, 0)) / 2
    return (1 - k * (hu * SUN[0] + hv * SUN[1])).astype(_F)


def mix(c, col, m):
    c += (np.asarray(col, _F) - c) * np.asarray(m, _F)[..., None]


def rgb(*c):
    return np.array(c, _F) / 255.0


def fill(col):
    out = np.empty((N, N, 3), _F)
    out[:] = col
    return out


# --------------------------------------------------------------- drawing

class Layer:
    """Pillow drawing on a supersampled canvas, wrapping: a shape that
    crosses an edge is drawn again shifted by the texture size, so it carries
    on across the opposite edge, in drawing order. fold() returns
    premultiplied RGBA (N, N, 4) floats. Pillow overwrites: a later shape
    replaces an earlier one under it."""

    def __init__(self, margin=2, ss=3):
        self.m, self.s = margin, ss
        size = (N + 2 * margin) * ss
        self.img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        self.d = ImageDraw.Draw(self.img)

    def _p(self, x, y):
        return ((x + self.m) * self.s, (y + self.m) * self.s)

    def _copies(self, pts, pad=0.0):
        """The shifts (ox, oy) that bring the shape over the canvas."""
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        lo_x, hi_x, lo_y, hi_y = min(xs) - pad, max(xs) + pad, min(ys) - pad, max(ys) + pad
        lim0, lim1 = -self.m - 1, N + self.m + 1
        return [(ox, oy) for ox in (-N, 0, N) for oy in (-N, 0, N)
                if hi_x + ox >= lim0 and lo_x + ox <= lim1 and hi_y + oy >= lim0 and lo_y + oy <= lim1]

    @staticmethod
    def _c(col, a=1.0):
        col = np.clip(np.asarray(col, float), 0, 1)
        return tuple(int(round(v * 255)) for v in col) + (int(round(a * 255)),)

    def ellipse(self, x, y, rx, ry, col, a=1.0, angle=0.0):
        if angle == 0.0 or abs(rx - ry) < 1e-3:
            for ox, oy in self._copies([(x, y)], max(rx, ry)):
                x0, y0 = self._p(x + ox - rx, y + oy - ry)
                x1, y1 = self._p(x + ox + rx, y + oy + ry)
                self.d.ellipse([x0, y0, x1, y1], fill=self._c(col, a))
        else:
            ca, sa = math.cos(angle), math.sin(angle)
            pts = []
            for i in range(14):
                t = 2 * math.pi * i / 14
                ex, ey = rx * math.cos(t), ry * math.sin(t)
                pts.append((x + ex * ca - ey * sa, y + ex * sa + ey * ca))
            self.poly(pts, col, a)

    def poly(self, pts, col, a=1.0):
        for ox, oy in self._copies(pts):
            self.d.polygon([self._p(px + ox, py + oy) for px, py in pts], fill=self._c(col, a))

    def line(self, pts, col, width, a=1.0):
        for ox, oy in self._copies(pts, width):
            self.d.line([self._p(px + ox, py + oy) for px, py in pts], fill=self._c(col, a),
                        width=max(1, int(round(width * self.s))), joint="curve")

    def blade(self, x, y, dx, dy, w, base, tip, a=1.0):
        """A tapering blade from (x, y) along (dx, dy); base colour at the
        root, tip colour on the upper part."""
        L = math.hypot(dx, dy)
        px, py = -dy / L * w / 2, dx / L * w / 2
        self.poly([(x - px, y - py), (x + px, y + py), (x + dx, y + dy)], base, a)
        mx, my = x + dx * 0.45, y + dy * 0.45
        self.poly([(mx - px * 0.55, my - py * 0.55), (mx + px * 0.55, my + py * 0.55),
                   (x + dx, y + dy)], tip, a)

    def fold(self):
        sm = self.img.resize((N + 2 * self.m,) * 2, Image.BOX)
        B = np.asarray(sm, _F) / 255.0
        B[..., :3] *= B[..., 3:4]
        return B[self.m:self.m + N, self.m:self.m + N].copy()


def over(c, layer, opacity=1.0):
    """Composite a folded layer (premultiplied) over the RGB array c."""
    a = layer[..., 3:4] * opacity
    c[:] = c * (1 - a) + layer[..., :3] * opacity


def scatter(rng, n, density=None, jitter_tries=6):
    """n points in [0, N)^2; with a density field (0..1) by rejection."""
    pts = []
    while len(pts) < n:
        k = (n - len(pts)) * jitter_tries
        x = rng.uniform(0, N, k)
        y = rng.uniform(0, N, k)
        if density is not None:
            keep = rng.uniform(0, 1, k) < density[(y.astype(int)) % N, (x.astype(int)) % N]
            x, y = x[keep], y[keep]
        pts.extend(zip(x.tolist(), y.tolist()))
    return pts[:n]


def jit(rng, col, k=0.08):
    """A colour varied in brightness and a little in hue."""
    col = np.asarray(col, float)
    return np.clip(col * rng.uniform(1 - k, 1 + k) + rng.uniform(-k, k, 3) * 0.25, 0, 1)


def up_dir(rng, spread=0.35):
    """A screen-up direction in texel space, leaning a little."""
    a = math.atan2(UPSCR[1], UPSCR[0]) + rng.normal(0, spread)
    return math.cos(a), math.sin(a)


def pebble(L, rng, x, y, r, col, sq=0.8, shadow=0.45):
    """A flat-lying pebble: soft shadow away from the sun, body, sunlit cap."""
    ang = rng.uniform(0, math.pi)
    rx, ry = r, r * rng.uniform(sq, 1.0)
    L.ellipse(x - SUN[0] * r * 0.55, y - SUN[1] * r * 0.55, rx * 1.1, ry * 1.1, (0.12, 0.09, 0.07),
              shadow, ang)
    L.ellipse(x, y, rx, ry, np.asarray(col) * 0.82, 1.0, ang)
    L.ellipse(x + SUN[0] * r * 0.28, y + SUN[1] * r * 0.28, rx * 0.68, ry * 0.62, col, 1.0, ang)
    L.ellipse(x + SUN[0] * r * 0.42, y + SUN[1] * r * 0.42, rx * 0.34, ry * 0.28,
              np.clip(np.asarray(col) * 1.10 + 0.03, 0, 1), 1.0, ang)


# --------------------------------------------------------------- textures

def clump(L, rng, x, y, n, length, width, base, tip, spread=0.55, shadow=0.35):
    """A tuft standing up on screen: a soft shadow cast away from the sun, then
    n blades fanned round screen-up from one root."""
    if shadow:
        L.ellipse(x - SUN[0] * length * 0.15, y - SUN[1] * length * 0.15, length * 0.32,
                  length * 0.22, (0.06, 0.12, 0.03), shadow, -math.pi / 4)
    for _ in range(n):
        dx, dy = up_dir(rng, spread)
        ln = length * rng.uniform(0.6, 1.1)
        bx, by = x + rng.normal(0, width), y + rng.normal(0, width)
        L.blade(bx, by, dx * ln, dy * ln, width * rng.uniform(0.8, 1.2), jit(rng, base),
                jit(rng, tip))


def grass():
    rng = np.random.default_rng(101)
    big = noise(1, 340, octaves=3, gain=0.6)
    mid = noise(2, 110, octaves=3, gain=0.6)
    sml = noise(3, 34, octaves=2)
    dryn = noise(4, 160, octaves=3, gain=0.55)
    hue = noise(9, 200, octaves=2)
    c = fill(rgb(108, 154, 50))
    mix(c, rgb(90, 140, 46), ss(0.5, 0.1, big) * 0.3)             # broad lush swales
    mix(c, rgb(128, 166, 56), ss(0.5, 0.9, big) * 0.3)            # broad sunny stretches
    mix(c, rgb(80, 130, 42), ss(0.45, 0.10, mid) * 0.35)
    mix(c, rgb(140, 174, 60), ss(0.58, 0.95, mid) * 0.3)
    mix(c, rgb(156, 160, 66), ss(0.66, 0.95, dryn) * 0.35)        # dry, yellower ground
    mix(c, rgb(96, 150, 62), ss(0.6, 0.95, hue) * 0.2)            # a cooler, bluer green
    mix(c, rgb(76, 128, 42), ss(0.35, 0.10, sml) * 0.40)          # small-scale life
    mix(c, rgb(136, 176, 62), ss(0.65, 0.90, sml) * 0.35)
    c *= relief(noise(6, 70, uniform=False) * 46, 0.25)[..., None]

    tone = mid * 0.45 + sml * 0.55
    fams = [  # (base, tip) colours: lush dark, mid, sunny, dry
        (rgb(40, 92, 30), rgb(96, 156, 48)),
        (rgb(64, 120, 38), rgb(138, 186, 62)),
        (rgb(98, 146, 44), rgb(186, 210, 86)),
        (rgb(124, 128, 50), rgb(214, 200, 108)),
    ]

    def fam(x, y):
        t = tone[int(y) % N, int(x) % N] + rng.normal(0, 0.18)
        if dryn[int(y) % N, int(x) % N] > 0.7 and rng.random() < 0.45:
            return fams[3]
        return fams[0 if t < 0.35 else 1 if t < 0.62 else 2]

    # a short underlayer of blades everywhere, then bold tufts on top
    L = Layer()
    for x, y in scatter(rng, 14000):
        b, t = fam(x, y)
        dx, dy = up_dir(rng, 0.5)
        ln = rng.uniform(12, 22)
        L.blade(x, y, dx * ln, dy * ln, rng.uniform(2.2, 3.2), jit(rng, b), jit(rng, t))
    over(c, L.fold(), 0.9)
    L = Layer()
    dens = 0.35 + 0.65 * ss(0.2, 0.8, noise(10, 60))
    for x, y in scatter(rng, 1900, dens):
        b, t = fam(x, y)
        clump(L, rng, x, y, rng.integers(4, 8), rng.uniform(20, 34), rng.uniform(2.8, 4.0), b, t,
              shadow=0.30)
    over(c, L.fold())

    # clover and little flowers lying in the turf, in loose drifts
    L = Layer()
    for x, y in scatter(rng, 60, ss(0.4, 0.8, noise(7, 120))):
        for _ in range(rng.integers(3, 7)):
            cx, cy = x + rng.normal(0, 6), y + rng.normal(0, 6)
            col = jit(rng, rgb(56, 122, 44), 0.1)
            for k in range(3):
                a = k * 2.09 + rng.uniform(0, 1)
                L.ellipse(cx + math.cos(a) * 2.2, cy + math.sin(a) * 2.2, 2.1, 2.1, col)
    blooms = [(rgb(252, 250, 240), rgb(250, 204, 50), 0.40),       # daisy
              (rgb(252, 214, 36), None, 0.25),                     # buttercup
              (rgb(130, 156, 246), None, 0.15),                    # speedwell
              (rgb(232, 52, 40), None, 0.10),                      # poppy
              (rgb(196, 140, 236), None, 0.10)]                    # mallow
    fl = ss(0.3, 0.9, noise(8, 70))
    for x, y in scatter(rng, 130, 0.15 + 0.85 * fl):
        r = rng.random()
        for petal, eye, p in blooms:
            if r < p:
                break
            r -= p
        for _ in range(rng.integers(1, 4)):
            fx, fy = x + rng.normal(0, 4), y + rng.normal(0, 4)
            rad = rng.uniform(2.2, 3.2)
            L.ellipse(fx - SUN[0] * 1.2, fy - SUN[1] * 1.2, rad, rad, (0.1, 0.2, 0.05), 0.35)
            L.ellipse(fx, fy, rad, rad, jit(rng, petal, 0.05))
            if eye is not None:
                L.ellipse(fx, fy, rad * 0.4, rad * 0.4, eye)
    over(c, L.fold())
    return c


def dirt():
    rng = np.random.default_rng(202)
    big = noise(11, 260, octaves=2)
    mid = noise(12, 70, octaves=3)
    sml = noise(13, 14, octaves=2)
    c = fill(rgb(160, 124, 84))
    mix(c, rgb(186, 154, 108), ss(0.55, 0.88, big) * 0.45)        # dusty, sun-baked
    mix(c, rgb(132, 100, 66), ss(0.42, 0.12, big) * 0.40)         # damper, darker
    mix(c, rgb(196, 164, 118), ss(0.66, 0.92, mid) * 0.45)
    mix(c, rgb(134, 100, 68), ss(0.35, 0.08, mid) * 0.45)
    c *= (0.92 + 0.16 * sml)[..., None]
    h = noise(14, 40, uniform=False) * 30 + noise(15, 9, uniform=False) * 6
    c *= relief(h, 0.35)[..., None]

    # cracks: short dark fissures in the drier ground, lit on their far lip
    L = Layer()
    for x, y in scatter(rng, 34, ss(0.4, 0.8, big)):
        a = rng.uniform(0, 2 * math.pi)
        pts = [(x, y)]
        for _ in range(rng.integers(3, 7)):
            a += rng.normal(0, 0.45)
            st = rng.uniform(4, 9)
            pts.append((pts[-1][0] + math.cos(a) * st, pts[-1][1] + math.sin(a) * st))
            if rng.random() < 0.15:           # a branch
                b = a + rng.choice((-1, 1)) * rng.uniform(0.9, 1.5)
                q = pts[-1]
                L.line([q, (q[0] + math.cos(b) * 6, q[1] + math.sin(b) * 6)], rgb(104, 76, 50), 1.4,
                       0.8)
        L.line([(p[0] + SUN[0] * 1.4, p[1] + SUN[1] * 1.4) for p in pts], rgb(210, 180, 136), 1.4, 0.6)
        L.line(pts, rgb(110, 80, 52), 1.8, 0.75)
    over(c, L.fold())

    # grit and pebbles: warm stones, flat-lying, a soft highlight
    L = Layer()
    for x, y in scatter(rng, 1600):
        col = jit(rng, rgb(132, 102, 72) if rng.random() < 0.65 else rgb(206, 182, 148), 0.1)
        L.ellipse(x, y, 1.1, 1.1, col, 0.8)
    for x, y in scatter(rng, 200, 0.3 + ss(0.3, 0.7, mid)):
        r = rng.uniform(2.4, 5.0) if rng.random() < 0.85 else rng.uniform(5.5, 8)
        col = jit(rng, [rgb(178, 150, 116), rgb(198, 176, 142), rgb(150, 116, 84),
                        rgb(170, 154, 134), rgb(196, 160, 112)][rng.integers(5)], 0.06)
        pebble(L, rng, x, y, r, col, sq=0.65, shadow=0.5)
    # a few dry grass wisps
    for x, y in scatter(rng, 70, ss(0.35, 0.1, big)):
        for _ in range(rng.integers(3, 7)):
            dx, dy = up_dir(rng, 0.6)
            ln = rng.uniform(7, 14)
            L.blade(x + rng.normal(0, 2), y + rng.normal(0, 2), dx * ln, dy * ln, 1.6,
                    jit(rng, rgb(150, 136, 66)), jit(rng, rgb(214, 196, 116)))
    over(c, L.fold())
    return c


def ripples(seed, cycles_u, cycles_v, bend, lee=0.7):
    """A periodic ripple height (0..1): crests across the (cycles_u, cycles_v)
    direction, bent by noise, gentle on the windward side, steep on the lee."""
    v, u = np.mgrid[0:N, 0:N].astype(_F)
    du = (noise(seed, 170, uniform=False) - 0.5) * bend
    dv = (noise(seed + 1, 170, uniform=False) - 0.5) * bend
    ph = ((u + du) * cycles_u + (v + dv) * cycles_v) / N + (noise(seed + 2, 60, uniform=False) - 0.5) * 0.6
    saw = ph % 1.0
    return np.where(saw < lee, saw / lee, (1 - saw) / (1 - lee)).astype(_F)


def desert():
    rng = np.random.default_rng(303)
    big = noise(21, 300, octaves=2)
    mid = noise(22, 90, octaves=2)
    c = fill(rgb(226, 180, 112))
    mix(c, rgb(240, 204, 142), ss(0.5, 0.9, big) * 0.45)
    mix(c, rgb(210, 156, 92), ss(0.45, 0.1, big) * 0.40)
    mix(c, rgb(232, 190, 124), ss(0.6, 0.95, mid) * 0.35)
    mix(c, rgb(212, 160, 96), ss(0.35, 0.05, mid) * 0.35)
    # ripples with crests running across the screen, on soft dune swells
    rip = ripples(23, 9, 15, 80, lee=0.62)
    amp = 0.3 + 0.7 * ss(0.25, 0.75, noise(26, 140))
    h = rip * 3.2 * amp + noise(27, 120, uniform=False) * 50 + noise(28, 40, uniform=False) * 10
    c *= np.clip(relief(h, 0.40), 0.86, 1.2)[..., None]
    c *= (0.95 + 0.08 * noise(29, 5))[..., None]                       # grain
    # wind streaks: faint, long, pale
    wl = noise(30, 8, stretch=10, angle=math.atan2(11, 6) + math.pi / 2)
    mix(c, rgb(246, 222, 172), ss(0.90, 0.99, wl) * 0.25)
    L = Layer()
    for x, y in scatter(rng, 1100):                                    # darker grit
        L.ellipse(x, y, 0.9, 0.9, jit(rng, rgb(176, 124, 76), 0.1), 0.55)
    for x, y in scatter(rng, 120, 0.2 + ss(0.4, 0.8, mid)):            # small stones
        r = rng.uniform(1.8, 4.2)
        col = jit(rng, [rgb(184, 136, 90), rgb(206, 182, 148), rgb(156, 108, 72)][rng.integers(3)])
        pebble(L, rng, x, y, r, col, shadow=0.35)
    over(c, L.fold())
    return c


def sand():
    rng = np.random.default_rng(404)
    big = noise(31, 280, octaves=2)
    mid = noise(32, 70, octaves=2)
    c = fill(rgb(240, 222, 172))
    mix(c, rgb(248, 236, 200), ss(0.5, 0.9, big) * 0.6)
    mix(c, rgb(224, 200, 146), ss(0.45, 0.1, big) * 0.5)
    mix(c, rgb(230, 208, 158), ss(0.35, 0.05, mid) * 0.3)
    rip = ripples(33, 9, 9, 60, lee=0.5)
    h = rip * 1.6 * ss(0.3, 0.8, noise(35, 150)) + noise(36, 60, uniform=False) * 18
    c *= relief(h, 0.40)[..., None]
    c *= (0.96 + 0.07 * noise(37, 3.5))[..., None]                     # fine grain
    L = Layer()
    for x, y in scatter(rng, 2200):                                    # coloured grains
        col = [rgb(200, 170, 122), rgb(176, 156, 134), rgb(255, 250, 236)][rng.integers(3)]
        L.ellipse(x, y, 0.75, 0.75, jit(rng, col, 0.06), 0.65)
    for x, y in scatter(rng, 200, 0.3 + ss(0.4, 0.8, mid)):            # shells and bits
        r = rng.uniform(1.8, 3.2)
        col = [rgb(255, 250, 240), rgb(252, 208, 196), rgb(240, 230, 210), rgb(220, 200, 228),
               rgb(250, 236, 214)][rng.integers(5)]
        a = rng.uniform(0, math.pi)
        L.ellipse(x - SUN[0] * 1.0, y - SUN[1] * 1.0, r, r * 0.7, (0.42, 0.32, 0.2), 0.35, a)
        L.ellipse(x, y, r, r * 0.7, col, 1.0, a)
        if rng.random() < 0.5:                                         # a ribbed cockle
            L.line([(x, y), (x + math.cos(a) * r * 0.8, y + math.sin(a) * r * 0.8)],
                   np.asarray(col) * 0.8, 0.5)
    for x, y in scatter(rng, 24):
        pebble(L, rng, x, y, rng.uniform(1.8, 3.2), jit(rng, rgb(204, 194, 180)), shadow=0.3)
    over(c, L.fold())
    return c


def worley(seed, cells, n=N):
    """Periodic cellular noise: (F1, F2) distances in texels to the nearest
    and second-nearest of one jittered point per cell of a cells x cells grid."""
    rng = np.random.default_rng(seed)
    cs = n / cells
    pts = (np.stack(np.meshgrid(np.arange(cells), np.arange(cells)), -1) + rng.uniform(0.1, 0.9, (cells, cells, 2))) * cs
    v, u = np.mgrid[0:n, 0:n].astype(_F) + 0.5
    ci, cj = (u // cs).astype(int), (v // cs).astype(int)
    f1 = np.full((n, n), 1e9, _F)
    f2 = np.full((n, n), 1e9, _F)
    for di in (-1, 0, 1):
        for dj in (-1, 0, 1):
            i, j = ci + di, cj + dj
            p = pts[j % cells, i % cells]
            px = p[..., 0] + np.floor_divide(i, cells) * n
            py = p[..., 1] + np.floor_divide(j, cells) * n
            d = np.hypot(u - px, v - py)
            f2 = np.where(d < f1, f1, np.minimum(f2, d))
            f1 = np.minimum(f1, d)
    return f1, f2


def _caustics(seed, cells, width):
    f1, f2 = worley(seed, cells)
    cs = N / cells
    du = (noise(seed + 5, cs * 1.2, uniform=False) - 0.5) * cs * 0.9
    dv = (noise(seed + 6, cs * 1.2, uniform=False) - 0.5) * cs * 0.9
    d = warp(f2 - f1, du, dv)                      # bend the straight cell walls
    wob = (noise(seed + 7, cs, uniform=False) - 0.5) * width * 2
    return ss(width, 0.0, d + wob)


def shallow():
    rng = np.random.default_rng(505)
    big = noise(41, 260, octaves=2)
    mid = noise(42, 80, octaves=2)
    # the sandy bottom, seen through clear water: ripples, weed, stones
    bed = fill(rgb(214, 204, 156))
    mix(bed, rgb(146, 160, 112), ss(0.65, 0.92, mid) * 0.65)           # weed
    mix(bed, rgb(232, 222, 180), ss(0.4, 0.1, mid) * 0.5)
    rip = ripples(43, 10, 10, 60, lee=0.5)
    bed *= relief(rip * 2.0 + noise(44, 40, uniform=False) * 20, 0.45)[..., None]
    L = Layer()
    for x, y in scatter(rng, 30):
        pebble(L, rng, x, y, rng.uniform(2.5, 5), jit(rng, rgb(176, 170, 150)), sq=0.6, shadow=0.3)
    over(bed, L.fold())
    water = rgb(8, 188, 206)
    depth = 0.64 + 0.22 * (big - 0.5)
    c = bed * (1 - depth[..., None]) + water * depth[..., None]
    mix(c, rgb(30, 160, 190), ss(0.5, 0.1, big) * 0.30)                # a touch deeper
    mix(c, rgb(120, 214, 196), ss(0.6, 0.95, big) * 0.25)              # sandier
    ca = _caustics(45, 14, 2.6)
    ca2 = _caustics(47, 24, 1.8)
    mix(c, rgb(210, 252, 238), np.clip(ca * 0.38 + ca2 * 0.22, 0, 0.6))
    c *= (0.97 + 0.06 * noise(49, 18, stretch=3, angle=-math.pi / 4))[..., None]
    return c


def deep():
    big = noise(51, 280, octaves=2)
    mid = noise(52, 90, octaves=2)
    c = fill(rgb(36, 112, 200))
    mix(c, rgb(26, 86, 176), ss(0.45, 0.1, big) * 0.55)
    mix(c, rgb(46, 132, 214), ss(0.55, 0.9, big) * 0.45)
    mix(c, rgb(38, 120, 204), ss(0.6, 0.9, mid) * 0.4)
    # soft waves: long crests across the screen (along u - v), lit on the sun side
    w1 = noise(53, 34, stretch=4, angle=-math.pi / 4, uniform=False)
    w2 = noise(54, 15, stretch=3, angle=-math.pi / 4 + 0.3, uniform=False)
    c *= relief(w1 * 28 + w2 * 9, 0.5)[..., None]
    sh = noise(55, 18, stretch=4, angle=-math.pi / 4)
    mix(c, rgb(82, 164, 228), ss(0.84, 0.98, sh) * 0.30)               # soft sheen, no dashes
    return c


def forest():
    rng = np.random.default_rng(606)
    big = noise(61, 260, octaves=2)
    mid = noise(62, 70, octaves=2)
    moss = noise(65, 90, octaves=2)
    c = fill(rgb(72, 98, 40))
    mix(c, rgb(112, 92, 54), ss(0.55, 0.9, big) * 0.55)                # brown litter ground
    mix(c, rgb(46, 76, 34), ss(0.45, 0.1, big) * 0.5)                  # damp dark
    mix(c, rgb(92, 134, 46), ss(0.6, 0.92, moss) * 0.6)                # moss
    c *= relief(noise(63, 46, uniform=False) * 34, 0.35)[..., None]
    c *= (0.9 + 0.18 * noise(64, 6))[..., None]
    L = Layer()
    litter = [rgb(168, 112, 50), rgb(190, 140, 62), rgb(132, 88, 44), rgb(140, 128, 58),
              rgb(204, 160, 74), rgb(110, 76, 42), rgb(176, 96, 46)]
    for x, y in scatter(rng, 1700, 0.1 + 0.9 * ss(0.35, 0.8, big)):  # fallen leaves
        col = jit(rng, litter[rng.integers(len(litter))], 0.10)
        r = rng.uniform(2.6, 4.6)
        a = rng.uniform(0, math.pi)
        L.ellipse(x - SUN[0] * 0.9, y - SUN[1] * 0.9, r, r * 0.55, (0.10, 0.08, 0.04), 0.45, a)
        L.ellipse(x, y, r, r * 0.55, col, 1.0, a)
        L.line([(x - math.cos(a) * r * 0.85, y - math.sin(a) * r * 0.85),
                (x + math.cos(a) * r * 0.85, y + math.sin(a) * r * 0.85)], np.asarray(col) * 0.72, 0.6)
    for x, y in scatter(rng, 80):                                      # twigs
        a = rng.uniform(0, math.pi)
        ln = rng.uniform(10, 22)
        p0, p1 = (x, y), (x + math.cos(a) * ln, y + math.sin(a) * ln)
        L.line([(p0[0] - SUN[0] * 1.2, p0[1] - SUN[1] * 1.2), (p1[0] - SUN[0] * 1.2, p1[1] - SUN[1] * 1.2)],
               (0.08, 0.06, 0.03), 2.0, 0.45)
        col = jit(rng, rgb(120, 88, 58))
        L.line([p0, p1], col, 1.6)
        if rng.random() < 0.6:
            b = a + rng.choice((-1, 1)) * 0.7
            q = (x + math.cos(a) * ln * 0.5, y + math.sin(a) * ln * 0.5)
            L.line([q, (q[0] + math.cos(b) * ln * 0.4, q[1] + math.sin(b) * ln * 0.4)], col, 1.1)
    for x, y in scatter(rng, 90, ss(0.5, 0.85, moss)):                 # moss patches
        col = jit(rng, rgb(98, 142, 46), 0.1)
        for _ in range(rng.integers(3, 7)):
            r = rng.uniform(2.5, 5)
            mx, my = x + rng.normal(0, 4), y + rng.normal(0, 4)
            L.ellipse(mx, my, r, r * 0.85, np.asarray(col) * rng.uniform(0.85, 1.05), 0.85)
    over(c, L.fold())
    L = Layer()
    for x, y in scatter(rng, 520, 0.2 + 0.8 * ss(0.3, 0.7, mid)):      # ferns and sprigs
        clump(L, rng, x, y, rng.integers(3, 7), rng.uniform(16, 28), rng.uniform(2.4, 3.4),
              rgb(36, 78, 28), rgb(96, 150, 50), spread=0.55, shadow=0.35)
    over(c, L.fold())
    return c


def snow():
    rng = np.random.default_rng(707)
    big = noise(71, 280, octaves=2)
    c = fill(rgb(236, 242, 250))
    mix(c, rgb(220, 230, 246), ss(0.45, 0.1, big) * 0.45)
    # soft drifts: a smooth height field, lit; shadows go a gentle blue
    h = (noise(72, 110, octaves=2, uniform=False) * 70
         + noise(73, 36, stretch=2.0, angle=-0.7, uniform=False) * 12)
    sh = relief(h, 0.7)
    lo = np.clip(1 - sh, 0, 1)
    hi = np.clip(sh - 1, 0, 1)
    mix(c, rgb(196, 212, 240), np.clip(lo * 1.4, 0, 0.5))
    mix(c, rgb(254, 254, 255), np.clip(hi * 2.0, 0, 0.7))
    c *= (0.98 + 0.03 * noise(74, 4))[..., None]
    L = Layer(ss=2)                                                    # sparkles
    for x, y in scatter(rng, 180):
        L.ellipse(x, y, 0.7, 0.7, (1, 1, 1))
        if rng.random() < 0.35:
            L.line([(x - 2.0, y + 2.0), (x + 2.0, y - 2.0)], (1, 1, 1), 0.5, 0.8)
    over(c, L.fold())
    L = Layer()                                                        # a few stones, a dry stalk
    for x, y in scatter(rng, 12):
        pebble(L, rng, x, y, rng.uniform(2, 3.5), jit(rng, rgb(150, 146, 152)), shadow=0.2)
    over(c, L.fold())
    return c


def noise_png():
    n = noise(91, 70, n=256, octaves=3, gain=0.45)
    return (np.clip(n, 0, 1) * 255 + 0.5).astype(np.uint8)


PAINTERS = {"grass": grass, "dirt": dirt, "desert": desert, "sand": sand, "shallow": shallow,
            "deep": deep, "forest": forest, "snow": snow}


def save_rgb(c, path):
    a = (np.clip(c, 0, 1) * 255 + 0.5).astype(np.uint8)
    Image.fromarray(a, "RGB").save(path, optimize=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--out", default=OUT)
    ap.add_argument("--only", help="comma-separated names (and/or 'noise')")
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    want = args.only.split(",") if args.only else NAMES + ["noise"]
    for name in want:
        path = os.path.join(args.out, name + ".png")
        if name == "noise":
            Image.fromarray(noise_png(), "L").save(path, optimize=True)
        else:
            c = PAINTERS[name]()
            save_rgb(c, path)
            m = np.clip(c, 0, 1).reshape(-1, 3).mean(0) * 255
            print("%-8s mean RGB %3d %3d %3d" % (name, *m))
        print("wrote", path)


if __name__ == "__main__":
    main()

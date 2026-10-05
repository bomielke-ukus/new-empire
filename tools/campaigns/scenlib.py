"""Helpers for drawing scenario maps and writing scenario RON files."""
import math
import os


def h(x, y, s=0):
    v = (x * 374761393 + y * 668265263 + s * 2147483647) & 0xFFFFFFFF
    v = ((v ^ (v >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((v ^ (v >> 16)) & 0xFFFF) / 65536.0


def smooth(x, y, scale, s):
    gx, gy = x / scale, y / scale
    x0, y0 = int(math.floor(gx)), int(math.floor(gy))
    fx, fy = gx - x0, gy - y0
    fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    a, b = h(x0, y0, s), h(x0 + 1, y0, s)
    c, d = h(x0, y0 + 1, s), h(x0 + 1, y0 + 1, s)
    return (a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy


class Grid:
    def __init__(self, n, fill="g"):
        self.n = n
        self.t = [[fill] * n for _ in range(n)]

    def inside(self, x, y):
        return 0 <= x < self.n and 0 <= y < self.n

    def get(self, x, y):
        return self.t[y][x] if self.inside(x, y) else None

    def set(self, x, y, c, over=None):
        if self.inside(x, y) and (over is None or self.t[y][x] in over):
            self.t[y][x] = c

    def each(self):
        for y in range(self.n):
            for x in range(self.n):
                yield x, y

    def where(self, f, c, over=None):
        for x, y in self.each():
            if f(x, y):
                self.set(x, y, c, over)

    def rect(self, x0, y0, x1, y1, c, over=None):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                self.set(x, y, c, over)

    def poly(self, pts, c, over=None, wobble=0.0, seed=0):
        """Fills a polygon; `wobble` tiles of noise roughen its edge."""
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        for y in range(max(0, int(min(ys)) - 3), min(self.n, int(max(ys)) + 4)):
            for x in range(max(0, int(min(xs)) - 3), min(self.n, int(max(xs)) + 4)):
                px = x + 0.5 + (smooth(x, y, 4, seed) - 0.5) * 2 * wobble
                py = y + 0.5 + (smooth(x, y, 4, seed + 7) - 0.5) * 2 * wobble
                if point_in(px, py, pts):
                    self.set(x, y, c, over)

    def line(self, pts, width, c, over=None):
        """A thick path through the points."""
        for (ax, ay), (bx, by) in zip(pts, pts[1:]):
            steps = int(max(abs(bx - ax), abs(by - ay)) * 2) + 1
            for i in range(steps + 1):
                t = i / steps
                cx, cy = ax + (bx - ax) * t, ay + (by - ay) * t
                r = width / 2
                for y in range(int(cy - r) - 1, int(cy + r) + 2):
                    for x in range(int(cx - r) - 1, int(cx + r) + 2):
                        if (x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2 <= r * r:
                            self.set(x, y, c, over)

    def blob(self, cx, cy, rx, ry, c, seed, fill=0.85, over="gda"):
        for y in range(cy - ry, cy + ry + 1):
            for x in range(cx - rx, cx + rx + 1):
                d = ((x - cx) / max(rx, 1)) ** 2 + ((y - cy) / max(ry, 1)) ** 2
                if d <= 1.0 and h(x, y, seed) < fill + 0.3 * (1 - d) - 0.3:
                    self.set(x, y, c, over)

    def shores(self):
        """Sand where land meets water, shallows where water meets land."""
        n = self.n
        t = [row[:] for row in self.t]
        for x, y in self.each():
            c = t[y][x]
            near = [t[y + dy][x + dx] for dx in (-1, 0, 1) for dy in (-1, 0, 1)
                    if 0 <= x + dx < n and 0 <= y + dy < n]
            if c == "W" and any(v in "gdas" for v in near):
                self.t[y][x] = "w"
            elif c in "gd" and any(v in "wW" for v in near):
                self.t[y][x] = "s"

    def rows(self):
        return ["".join(r) for r in self.t]

    def clear(self, x0, y0, x1, y1, to="g"):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                if self.get(x, y) == "f":
                    self.t[y][x] = to

    def land(self, x, y, w=1, h_=1):
        for yy in range(y, y + h_):
            for xx in range(x, x + w):
                c = self.get(xx, yy)
                assert c is not None and c in "gdasn", ((x, y), (xx, yy), c)


def point_in(x, y, pts):
    inside = False
    j = len(pts) - 1
    for i in range(len(pts)):
        xi, yi = pts[i]
        xj, yj = pts[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi + 1e-9) + xi:
            inside = not inside
        j = i
    return inside


def heights(g, target):
    """Corner heights from `target(cx, cy)` (0..3), flat by the water,
    relaxed so neighbours are at most a level apart."""
    n = g.n
    H = [[0] * (n + 1) for _ in range(n + 1)]
    for cy in range(n + 1):
        for cx in range(n + 1):
            near = [g.get(tx, ty) for ty in (cy - 1, cy) for tx in (cx - 1, cx)
                    if g.inside(tx, ty)]
            if any(t in "wWs" for t in near):
                H[cy][cx] = 0
            else:
                H[cy][cx] = max(0, min(3, int(round(target(cx, cy)))))
    changed = True
    while changed:
        changed = False
        for cy in range(n + 1):
            for cx in range(n + 1):
                for dx, dy in ((1, 0), (0, 1), (1, 1), (1, -1)):
                    nx, ny = cx + dx, cy + dy
                    if 0 <= nx <= n and 0 <= ny <= n:
                        a, b = H[cy][cx], H[ny][nx]
                        if a > b + 1:
                            H[cy][cx] = b + 1
                            changed = True
                        elif b > a + 1:
                            H[ny][nx] = a + 1
                            changed = True
    return ["".join(str(v) for v in row) for row in H]


# --- RON ---------------------------------------------------------------

def q(s):
    assert '"' not in s and "\\" not in s, s
    font = set("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 :/-.%+()?,![]=;'")
    bad = {c for c in s.upper() if c not in font}
    assert not bad, (bad, s)
    return '"' + s + '"'


def ron_list(items, indent="    "):
    if not items:
        return "[]"
    return "[\n" + "".join(f"{indent}    {i},\n" for i in items) + f"{indent}]"


def place(kind, owner, at, count=1, tag=None):
    s = f"(kind: {q(kind)}, owner: {owner}, at: ({at[0]}, {at[1]})"
    if count != 1:
        s += f", count: {count}"
    if tag:
        s += f", tag: Some({q(tag)})"
    return s + ")"


def placed(kind, owner, at, count=1):
    """A placement as a trigger's Place action."""
    return "Place(" + place(kind, owner, at, count) + ")"


def objective(oid, text, goal, hidden=False, optional=False):
    s = f"(id: {q(oid)}, text: {q(text)}, goal: {goal}"
    if hidden:
        s += ", hidden: true"
    if optional:
        s += ", optional: true"
    return s + ")"


def trigger(then, when=(), tid=None, repeat=False):
    parts = []
    if tid:
        parts.append(f"id: Some({q(tid)})")
    if when:
        parts.append("when: [" + ", ".join(when) + "]")
    parts.append("then: [" + ", ".join(then) + "]")
    if repeat:
        parts.append("repeat: true")
    return "(" + ", ".join(parts) + ")"


def say(text):
    return f"Say({q(text)})"


def lose(text):
    return f"Lose({q(text)})"


def area(x0, y0, x1, y1):
    return f"(from: ({x0}, {y0}), to: ({x1}, {y1}))"


def side(name, control, civ, age="Stone", stockpile=None, techs=(), start=None):
    s = f"(name: {q(name)}, control: {control}, civ: Some({civ}), age: {age}"
    if start is not None:
        s += f", start: Some(({start[0]}, {start[1]}))"
    if stockpile is not None:
        s += f", stockpile: ({', '.join(str(v) for v in stockpile)})"
    if techs:
        s += ", techs: [" + ", ".join(q(t) for t in techs) + "]"
    return s + ")"


def write(out, name, title, briefing, seed, g, hs, sides, placements, objectives,
          triggers, note):
    text = f"// {note}\n"
    text += "(\n"
    text += f"    title: {q(title)},\n"
    text += "    briefing: [\n" + "".join(f"        {q(p)},\n" for p in briefing) + "    ],\n"
    text += f"    seed: Some({seed}),\n"
    text += "    map: Drawn((\n        terrain: [\n"
    text += "".join(f'            "{r}",\n' for r in g.rows())
    text += "        ],\n        heights: [\n"
    text += "".join(f'            "{r}",\n' for r in hs)
    text += "        ],\n    )),\n"
    text += "    sides: " + ron_list(sides) + ",\n"
    text += "    placements: " + ron_list(placements) + ",\n"
    text += "    objectives: " + ron_list(objectives) + ",\n"
    text += "    triggers: " + ron_list(triggers) + ",\n"
    text += ")\n"
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, name + ".ron"), "w") as f:
        f.write(text)


def campaign(out, title, about, order, names):
    with open(os.path.join(out, "campaign.ron"), "w") as f:
        f.write(
            "(\n"
            f"    title: {q(title)},\n"
            f"    about: {q(about)},\n"
            f"    order: {order},\n"
            "    scenarios: [" + ", ".join(q(n) for n in names) + "],\n"
            ")\n"
        )

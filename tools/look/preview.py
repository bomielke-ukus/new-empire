"""Render one asset alone, to look at it:

    /usr/bin/python3.13 tools/look/preview.py MODULE.FUNC OUT.png [width_tiles] [res]

Calls MODULE.FUNC() with no arguments (it should build at the origin and
return its root), on a plain grass patch, through the game's camera."""
import importlib
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib  # noqa: E402

target, out = sys.argv[1], sys.argv[2]
width = float(sys.argv[3]) if len(sys.argv) > 3 else 6.0
res = int(sys.argv[4]) if len(sys.argv) > 4 else 720
lib.reset()
mod, fn = target.rsplit(".", 1)
m = importlib.import_module(mod)
if not getattr(m, "OWN_GROUND", False):
    lib.box((40, 40, 0.2), (0, 0, -0.1), lib.mat("grass", rough=1.0))
getattr(m, fn)()
lib.camera((0, 0, 0.6), width, (res, int(res * 0.75)))
lib.light()
lib.render(out, samples=32)
print("wrote", out)

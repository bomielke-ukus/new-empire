"""Builds one of the slice's subjects into a Blender file, ready to render.

    blender --background --python tools/render/slice.py -- --subject spearman --save out.blend

`scripts/render-sprites.sh` runs this, then render_sheet.py, then
`atlas compose`, for every subject in SUBJECTS. Each entry names the sprite
set, its size class and how to build it; the models are made from the kit's
primitives (kit.py), so they are greybox in spirit: shapes that read at game
size, one consistent style, and every one rebuildable from this file.

Buildings take the class their footprint needs: one tile is SmallBuilding,
two MediumBuilding, three LargeBuilding (the renderer draws them at those
sizes, `crates/view/src/sprites.rs`).
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import bpy  # noqa: E402

import kit  # noqa: E402


# --------------------------------------------------------------------------
# Units.

def spearman():
    h = kit.Humanoid("Spearman", helmet="cone")
    h.hold(kit.spear("spear"))
    h.carry_shield(kit.round_shield("shield", face="hide"))
    h.animate("thrust")
    return h.root


# --------------------------------------------------------------------------
# Buildings.

def house():
    """A Stone Age house on two tiles: mudbrick walls, a thatched roof, a
    cloth of the owner's colour over the door."""
    b = kit.Building("House", 2)
    w, d, wall = 1.30, 1.30, 0.50
    kit.walls(b, "house", w, d, wall, "mudbrick")
    kit.scaffold(b, "house", w + 0.1, d + 0.1, wall + 0.25)
    b.add(kit.pyramid("house_roof", (w + 0.25, d + 0.25, 0.55), "thatch", (0.0, 0.0, wall)),
          kit.FINISHED)
    b.add(kit.box("house_cloth", (w * 0.36, 0.03, 0.12), "player",
                  (0.0, d / 2 + 0.02, wall * 0.62)), kit.FINISHED)
    kit.rubble(b, "house", 1.3, 0.28, mats=("mudbrick", "thatch"))
    b.finish()
    return b.root


# name: (builder, size class, "unit" or "building")
SUBJECTS = {
    "spearman": (spearman, "Foot", "unit"),
    "house": (house, "MediumBuilding", "building"),
}


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    if "--list" in argv:
        for name, (_, cls, what) in sorted(SUBJECTS.items()):
            print("%s %s %s" % (name, cls, what))
        return
    name = argv[argv.index("--subject") + 1]
    if name not in SUBJECTS:
        raise SystemExit("no subject %r; the slice has %s" % (name, sorted(SUBJECTS)))
    build, _, _ = SUBJECTS[name]
    kit.clear()
    root = build()
    bpy.context.view_layer.update()
    print("built %s as %s" % (name, root.name))
    if "--save" in argv:
        path = os.path.abspath(argv[argv.index("--save") + 1])
        bpy.ops.wm.save_as_mainfile(filepath=path)
        print("saved %s" % path)


if __name__ == "__main__":
    main()

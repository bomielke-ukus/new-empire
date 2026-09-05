"""Renders one subject through the rig, one PNG per (animation, facing, frame).

    blender villager.blend --background --python tools/render/render_sheet.py -- \
        --subject Villager --class Foot --out build/renders/villager \
        --anim idle=1-4 --anim walk=5-12 --anim attack=13-18 \
        --anim death=19-26 --anim decay=27-30

Then turn those into a validated sprite sheet:

    cargo run -p atlas -- compose --renders build/renders/villager \
        --set villager --class Foot --out assets/sprites/villager

This script does not know the sheet layout, the palette, or anything about
indices. It renders frames and names them; `atlas compose` owns everything
after that, so the layout is defined in exactly one place (tools/atlas
manifest.rs) rather than in two that can drift apart.

The subject is parented to a turntable empty and the EMPTY is rotated, so an
animation that keys the subject's own rotation cannot fight the facing.
"""

import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rig as rig_module  # noqa: E402

TURNTABLE = "ne_turntable"


def _parse(argv):
    out = {"anims": [], "subject": None, "cls": None, "out": None}
    names = {"--subject": "subject", "--class": "cls", "--out": "out"}
    i = 0
    while i < len(argv):
        key = argv[i]
        value = argv[i + 1] if i + 1 < len(argv) else None
        if value is None:
            raise SystemExit("%s needs a value" % key)
        if key in names:
            out[names[key]] = value
        elif key == "--anim":
            name, _, span = value.partition("=")
            if not span:
                raise SystemExit("--anim wants name=first-last, got %r" % value)
            first, _, last = span.partition("-")
            out["anims"].append((name, int(first), int(last or first)))
        else:
            raise SystemExit("unknown flag %s" % key)
        i += 2
    for required in ("subject", "cls", "out"):
        if out[required] is None:
            raise SystemExit("--%s is required" % required.replace("cls", "class"))
    if not out["anims"]:
        raise SystemExit("at least one --anim name=first-last is required")
    return out


def turntable(subject):
    """Puts `subject` under an empty whose Z rotation sets the facing."""
    empty = bpy.data.objects.get(TURNTABLE)
    if empty is None:
        empty = bpy.data.objects.new(TURNTABLE, None)
        bpy.context.scene.collection.objects.link(empty)
    empty.location = (0.0, 0.0, 0.0)
    empty.rotation_euler = (0.0, 0.0, 0.0)
    if subject.parent is not empty:
        # keep_transform equivalent: record, reparent, restore.
        world = subject.matrix_world.copy()
        subject.parent = empty
        subject.matrix_parent_inverse = empty.matrix_world.inverted()
        subject.matrix_world = world
    return empty


def render(cfg):
    rig = rig_module.load_rig()
    scene = bpy.context.scene
    rig_module.build(rig, scene)

    if cfg["cls"] not in rig["classes"]:
        raise SystemExit("unknown size class %r; the rig has %s"
                         % (cfg["cls"], sorted(rig["classes"])))
    spec = rig_module.set_class(rig, scene, cfg["cls"])

    subject = bpy.data.objects.get(cfg["subject"])
    if subject is None:
        raise SystemExit("no object named %r in this file; it has %s"
                         % (cfg["subject"], sorted(o.name for o in bpy.data.objects)))
    empty = turntable(subject)

    out_dir = os.path.abspath(cfg["out"])
    os.makedirs(out_dir, exist_ok=True)
    print("rendering %s at %dx%d, ortho_scale %.6f"
          % (cfg["subject"], spec["render_px"][0], spec["render_px"][1],
             spec["ortho_scale"]))

    total = 0
    for name, first, last in cfg["anims"]:
        for facing in rig["facings"]:
            rig_module.face(rig, empty, facing["name"])
            for index, frame in enumerate(range(first, last + 1)):
                scene.frame_set(frame)
                path = os.path.join(
                    out_dir, "%s_%s_%02d.png" % (name, facing["name"], index)
                )
                scene.render.filepath = path
                bpy.ops.render.render(write_still=True)
                total += 1
        print("  %s: %d frames x %d facings"
              % (name, last - first + 1, len(rig["facings"])))

    print("wrote %d renders to %s" % (total, out_dir))
    print("next: cargo run -p atlas -- compose --renders %s --set %s --class %s "
          "--out assets/sprites/%s"
          % (cfg["out"], cfg["subject"].lower(), cfg["cls"], cfg["subject"].lower()))


if __name__ == "__main__":
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    render(_parse(argv))

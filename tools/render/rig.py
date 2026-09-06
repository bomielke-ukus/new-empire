"""Builds the New Empire render rig in Blender, from assets/render/rig.json.

    blender --background --python tools/render/rig.py -- --check
    blender --background --python tools/render/rig.py -- --save tools/render/rig.blend
    blender subject.blend --python tools/render/rig.py        # rig an existing file

The rig file is the source of truth and is checked against the specs by
`cargo run -p atlas -- rig`, which CI runs. This script only applies it, so
there are no camera or light numbers written down here — if a number appears in
this file that is not in rig.json, it is a bug.

Why the camera never moves: facings come from turning the SUBJECT. If you orbit
the camera instead, the key light swings around the subject with it and every
facing is lit differently, which is the fastest way to ruin a sprite set.
"""

import json
import math
import os
import sys

import bpy
from mathutils import Euler

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RIG_PATH = os.path.join(REPO, "assets", "render", "rig.json")


def load_rig(path=RIG_PATH):
    with open(path) as f:
        return json.load(f)


def _euler(degrees):
    return Euler([math.radians(d) for d in degrees], "XYZ")


def clear_rig():
    """Removes every camera and light, leaving the subject alone.

    Every light, not just the ones this script made: the rig's three suns are
    the whole lighting model, and a stray lamp left in a subject file - Blender's
    startup Light is the usual culprit - changes the shading on every frame
    rendered from it without announcing itself. Removing one is worth saying out
    loud, because it means the file was lit for something else.
    """
    for obj in list(bpy.data.objects):
        if obj.type not in ("CAMERA", "LIGHT"):
            continue
        if obj.type == "LIGHT" and not obj.name.startswith("ne_"):
            print("rig: removing stray light %r; the rig's three suns are the "
                  "whole lighting model" % obj.name)
        bpy.data.objects.remove(obj, do_unlink=True)


def build_camera(rig, scene):
    spec = rig["camera"]
    data = bpy.data.cameras.new("ne_camera")
    data.type = spec["type"]
    data.clip_start = spec["clip_start"]
    data.clip_end = spec["clip_end"]
    # ortho_scale is per size class and is set by set_class() at render time.
    data.ortho_scale = rig["classes"]["Foot"]["ortho_scale"]

    cam = bpy.data.objects.new("ne_camera", data)
    cam.location = spec["location"]
    cam.rotation_euler = _euler(spec["rotation_euler_xyz_deg"])
    scene.collection.objects.link(cam)
    scene.camera = cam
    return cam


def build_lights(rig, scene):
    lights = []
    for spec in rig["lights"]:
        data = bpy.data.lights.new("ne_%s" % spec["name"], type=spec["type"])
        data.energy = spec["energy"]
        data.color = spec["colour"]
        # Sun "angle" is the angular diameter of the source: bigger is softer.
        data.angle = math.radians(spec["angle_deg"])
        obj = bpy.data.objects.new("ne_%s" % spec["name"], data)
        # A sun's position is irrelevant; only its rotation matters. Park it
        # somewhere out of the way so it does not sit inside the subject.
        obj.location = [c * 10.0 for c in spec["direction_toward_light"]]
        obj.rotation_euler = _euler(spec["rotation_euler_xyz_deg"])
        scene.collection.objects.link(obj)
        lights.append(obj)
    return lights


def configure_render(rig, scene):
    spec = rig["render"]

    # EEVEE Next in Blender 4.2+, plain EEVEE before that. Cycles would be more
    # accurate and is not worth it at 192 px; the engine is the one place we
    # fall back rather than fail.
    engines = {e.identifier for e in
               bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items}
    for candidate in (spec["engine"], spec["engine_fallback"], "CYCLES"):
        if candidate in engines:
            scene.render.engine = candidate
            break

    if hasattr(scene, "eevee"):
        scene.eevee.taa_render_samples = spec["samples"]
    if scene.render.engine == "CYCLES":
        scene.cycles.samples = spec["samples"]

    scene.render.film_transparent = spec["film_transparent"]
    scene.render.filter_size = spec["filter_width"]

    # The trap. Blender ships AgX (4.x) or Filmic, both of which desaturate and
    # roll off highlights, so the colours you texture are not the colours that
    # reach `atlas quantize` and the palette match lands somewhere else.
    scene.view_settings.view_transform = spec["view_transform"]
    scene.view_settings.look = spec["look"]
    scene.view_settings.exposure = spec["exposure"]
    scene.view_settings.gamma = spec["gamma"]

    image = scene.render.image_settings
    image.file_format = spec["file_format"]
    image.color_mode = spec["colour_mode"]
    image.color_depth = spec["colour_depth"]

    # No world lighting: the three suns are the whole lighting model, and an
    # ambient world colour would flatten them.
    if scene.world is None:
        scene.world = bpy.data.worlds.new("ne_world")
    scene.world.use_nodes = False
    scene.world.color = (0.0, 0.0, 0.0)


def set_class(rig, scene, class_name):
    """Points the camera and the output resolution at one size class.

    As well as the resolution and the orthographic scale, this slides the camera
    along its own up axis so the subject's origin lands on the class's anchor -
    the ground contact point - rather than at the frame centre. Leave the shift
    out and half of every frame is empty ground below the subject's feet.
    """
    spec = rig["classes"][class_name]
    scene.render.resolution_x = spec["render_px"][0]
    scene.render.resolution_y = spec["render_px"][1]
    scene.render.resolution_percentage = 100

    cam = scene.camera
    cam.data.ortho_scale = spec["ortho_scale"]
    base = rig["camera"]["location"]
    up = rig["camera"]["basis"]["up"]
    shift = spec["camera_up_shift"]
    cam.location = [base[i] + up[i] * shift for i in range(3)]
    return spec


def face(rig, subject, facing_name):
    """Turns `subject` so it faces `facing_name` on screen."""
    for f in rig["facings"]:
        if f["name"] == facing_name:
            subject.rotation_euler.z = math.radians(f["subject_z_rotation_deg"])
            return f
    raise SystemExit(
        "unknown facing %r; the rig authors %s"
        % (facing_name, [f["name"] for f in rig["facings"]])
    )


def build(rig=None, scene=None):
    rig = rig or load_rig()
    scene = scene or bpy.context.scene
    clear_rig()
    cam = build_camera(rig, scene)
    lights = build_lights(rig, scene)
    configure_render(rig, scene)
    return rig, cam, lights


def check(rig, scene):
    """Re-derives the camera basis from what Blender actually built.

    `atlas rig` proves the numbers in rig.json describe the projection the specs
    require. This proves Blender agrees about what those numbers mean — which is
    a different claim, and the one that catches a Euler-order or handedness
    surprise after a Blender upgrade.
    """
    # matrix_world is only recomputed when the dependency graph evaluates, so
    # reading it straight after setting rotation_euler returns the identity and
    # every check below "fails" against an unrotated camera.
    bpy.context.view_layer.update()

    cam = scene.camera
    m = cam.matrix_world.to_3x3()
    right, up, forward = m.col[0], m.col[1], -m.col[2]
    problems = []

    def close(got, want, what):
        if (got - want).length > 1e-5:
            problems.append("%s: Blender gives %s, rig.json declares %s"
                            % (what, [round(c, 6) for c in got], want))

    from mathutils import Vector
    close(right, Vector(rig["camera"]["basis"]["right"]), "camera right")
    close(up, Vector(rig["camera"]["basis"]["up"]), "camera up")
    close(forward, Vector(rig["camera"]["basis"]["forward"]), "camera forward")

    # A tile must project exactly twice as wide as tall (docs/05 section 1).
    corners = [Vector(c) for c in ((0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0))]
    xs = [c.dot(right) for c in corners]
    ys = [-c.dot(up) for c in corners]
    ratio = (max(xs) - min(xs)) / (max(ys) - min(ys))
    if abs(ratio - 2.0) > 1e-5:
        problems.append("a tile projects %.6f:1, not 2:1" % ratio)

    for spec in rig["lights"]:
        obj = bpy.data.objects.get("ne_%s" % spec["name"])
        if obj is None:
            problems.append("light %r was not built" % spec["name"])
            continue
        aimed = obj.matrix_world.to_3x3().col[2]
        close(aimed, Vector(spec["direction_toward_light"]), "light %r" % spec["name"])

    if scene.view_settings.view_transform != rig["render"]["view_transform"]:
        problems.append("view transform is %r, not %r"
                        % (scene.view_settings.view_transform,
                           rig["render"]["view_transform"]))

    print("engine: %s" % scene.render.engine)
    print("view transform: %s" % scene.view_settings.view_transform)
    print("camera right   %s" % [round(c, 6) for c in right])
    print("camera up      %s" % [round(c, 6) for c in up])
    print("camera forward %s" % [round(c, 6) for c in forward])
    print("tile ratio     %.6f" % ratio)
    if problems:
        for p in problems:
            print("FAIL %s" % p)
        return 1
    print("Blender agrees with the rig")
    return 0


def _args():
    return sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []


if __name__ == "__main__":
    args = _args()
    rig, _, _ = build()
    status = 0
    if "--check" in args:
        status = check(rig, bpy.context.scene)
    if "--save" in args:
        path = args[args.index("--save") + 1]
        bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(path))
        print("saved %s" % path)
    sys.exit(status)

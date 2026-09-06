"""Builds the greybox villager: the first subject through the render pipeline.

    blender --background --python tools/render/greybox_villager.py -- --save tools/render/villager.blend

Greybox in the literal sense — boxes. The point of this model is not to look
good, it is to prove the route in `docs/08-art-production.md` end to end:
geometry through the frozen rig, through `atlas quantize`, through
`atlas compose`, out the far side as a sprite set that passes the same gate any
other art does. It exercises the things that are actually uncertain — whether a
render quantises into a readable 40x48 figure against the ancient palette,
whether the magenta player-colour key survives shading into the reserved ramp,
whether five facings mirror into eight without looking wrong.

Proportions follow `assets/render/rig.json`: one Blender unit is one tile, and
the figure is 0.86 units tall, about 34 px at 1x in a 48 px frame. The head is
deliberately oversized against real proportion, as every isometric RTS does it,
because at 34 px a realistic head is four pixels and reads as nothing.

Limbs pivot at hip and shoulder rather than at their centres, so a leg swing
rotates about the joint. Everything hangs off `vg_body`, one level below the
root, so the root stays free for `render_sheet.py` to parent to its turntable.

Animation frames, matching what render_sheet.py is invoked with:

    idle 1-4    walk 5-12    attack 13-18    death 19-26    decay 27-30
"""

import math
import os
import sys

import bpy

ROOT = "Villager"
BODY = "vg_body"

# (name, width x, depth y, height z, pivot, rest location, material)
#   pivot "top" hangs the box below its origin, so limbs rotate about the joint.
PARTS = [
    ("vg_leg_l", 0.10, 0.11, 0.34, "top", (-0.062, 0.0, 0.34), "trouser"),
    ("vg_leg_r", 0.10, 0.11, 0.34, "top", (0.062, 0.0, 0.34), "trouser"),
    ("vg_torso", 0.27, 0.17, 0.32, "bottom", (0.0, 0.0, 0.34), "tunic"),
    ("vg_head", 0.19, 0.18, 0.17, "bottom", (0.0, 0.01, 0.66), "skin"),
    ("vg_hair", 0.20, 0.19, 0.05, "bottom", (0.0, 0.01, 0.81), "hair"),
    ("vg_arm_l", 0.075, 0.09, 0.26, "top", (-0.17, 0.0, 0.64), "skin"),
    ("vg_arm_r", 0.075, 0.09, 0.26, "top", (0.17, 0.0, 0.64), "skin"),
    ("vg_tool", 0.045, 0.045, 0.30, "top", (0.17, 0.10, 0.62), "wood"),
    ("vg_blade", 0.12, 0.05, 0.09, "top", (0.17, 0.10, 0.36), "stone"),
]

# Base colours. The tunic is pure magenta: `atlas quantize` maps that hue into
# the reserved player ramp at 240-247 by lightness, so the shading the renderer
# produces survives into the indexed sprite. Nothing else may be near it.
MATERIALS = {
    "tunic": (1.0, 0.0, 1.0),
    "skin": (0.82, 0.60, 0.44),
    "hair": (0.16, 0.11, 0.08),
    "trouser": (0.34, 0.25, 0.17),
    "wood": (0.42, 0.29, 0.16),
    "stone": (0.52, 0.52, 0.55),
}

# Half the standing height. A body pitched flat about its feet lies out this
# far from the origin, so this is how far back it has to slide to stay on its
# own tile.
FALLEN_CENTRE = 0.43

FIRST, LAST = 1, 30
SPANS = {
    "idle": (1, 4),
    "walk": (5, 12),
    "attack": (13, 18),
    "death": (19, 26),
    "decay": (27, 30),
}


def material(name):
    mat = bpy.data.materials.get(name)
    if mat is not None:
        return mat
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    r, g, b = MATERIALS[name]
    bsdf.inputs["Base Color"].default_value = (r, g, b, 1.0)
    # Matte: a specular highlight on a 34 px figure is one bright pixel that
    # moves between frames, which reads as noise rather than as shine.
    bsdf.inputs["Roughness"].default_value = 0.9
    for maybe in ("Specular IOR Level", "Specular"):
        if maybe in bsdf.inputs:
            bsdf.inputs[maybe].default_value = 0.1
            break
    return mat


def box(name, w, d, h, pivot, location, mat_name):
    """A box whose origin sits at `pivot`, so rotation happens at the joint."""
    x0, x1 = -w / 2.0, w / 2.0
    y0, y1 = -d / 2.0, d / 2.0
    if pivot == "top":
        z0, z1 = -h, 0.0
    elif pivot == "bottom":
        z0, z1 = 0.0, h
    else:
        z0, z1 = -h / 2.0, h / 2.0

    verts = [
        (x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
        (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1),
    ]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4),
             (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate()
    mesh.update()
    mesh.materials.append(material(mat_name))

    obj = bpy.data.objects.new(name, mesh)
    obj.location = location
    bpy.context.scene.collection.objects.link(obj)
    return obj


def clear():
    """Empties the scene.

    Blender starts with a 2x2x2 Cube, a Light and a Camera. The Cube is eight
    times the height of the villager and sits exactly where the camera is
    pointed, so leaving it in renders a wall of grey and nothing else; the Light
    quietly breaks the three-sun lighting contract the rig depends on. This
    script owns the file it builds, so it starts from nothing.
    """
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)


def build():
    clear()
    root = bpy.data.objects.new(ROOT, None)
    bpy.context.scene.collection.objects.link(root)
    body = bpy.data.objects.new(BODY, None)
    bpy.context.scene.collection.objects.link(body)
    body.parent = root

    parts = {}
    for name, w, d, h, pivot, loc, mat in PARTS:
        obj = box(name, w, d, h, pivot, loc, mat)
        obj.parent = body
        parts[name] = obj
    return root, body, parts


def swing(deg):
    return math.radians(deg)


def pose(anim, i, count):
    """Local transforms for one frame: {object name: (dz, rot_x)} plus body.

    Everything a frame does is a bob and a set of limb swings. Crude on
    purpose — at 34 px the silhouette is the whole performance.
    """
    body = {"dy": 0.0, "dz": 0.0, "rot_x": 0.0, "scale_z": 1.0}
    limbs = {n: 0.0 for n in ("vg_leg_l", "vg_leg_r", "vg_arm_l", "vg_arm_r")}
    tool = 0.0

    if anim == "idle":
        # Breathing: a single pixel of vertical movement is enough to stop a
        # standing unit looking like a statue.
        body["dz"] = 0.006 * math.sin(2.0 * math.pi * i / count)
        limbs["vg_arm_l"] = swing(4.0)
        limbs["vg_arm_r"] = swing(-4.0)

    elif anim == "walk":
        phase = 2.0 * math.pi * i / count
        limbs["vg_leg_l"] = swing(26.0) * math.sin(phase)
        limbs["vg_leg_r"] = swing(26.0) * math.sin(phase + math.pi)
        limbs["vg_arm_l"] = swing(18.0) * math.sin(phase + math.pi)
        limbs["vg_arm_r"] = swing(18.0) * math.sin(phase)
        # Two bobs per stride, highest as each leg passes under the body.
        body["dz"] = 0.018 * abs(math.sin(phase))
        tool = limbs["vg_arm_r"]

    elif anim == "attack":
        # Wind up, then strike. The blow lands on index 3, which is what the
        # manifest tags as the impact frame (docs/05 section 2.2).
        keys = [-55.0, -70.0, -20.0, 75.0, 60.0, 30.0]
        angle = keys[min(i, len(keys) - 1)]
        limbs["vg_arm_r"] = swing(angle)
        limbs["vg_arm_l"] = swing(-angle * 0.25)
        limbs["vg_leg_l"] = swing(10.0)
        limbs["vg_leg_r"] = swing(-8.0)
        body["rot_x"] = swing(-angle * 0.12)
        tool = swing(angle)

    elif anim == "death":
        t = i / float(count - 1)
        # Pitch forward and drop. Eased so the fall accelerates.
        body["rot_x"] = swing(-88.0) * (t * t)
        body["dz"] = -0.05 * t
        # Pitching about the feet lays the body out entirely on one side of the
        # origin, so the corpse ends up a half-tile from where the unit died and
        # the sprite's anchor is no longer under it. Slide it back by half its
        # own length as it goes down, so it comes to rest across its own tile.
        body["dy"] = -FALLEN_CENTRE * (t * t)
        limbs["vg_arm_l"] = swing(-50.0) * t
        limbs["vg_arm_r"] = swing(-70.0) * t
        limbs["vg_leg_l"] = swing(18.0) * t
        limbs["vg_leg_r"] = swing(-12.0) * t
        tool = swing(-70.0) * t

    elif anim == "decay":
        t = (i + 1) / float(count)
        # The corpse settles where death left it. There is no ground plane to
        # sink into and no alpha to fade in an indexed pipeline, so decay is a
        # slow slump; the dissolve itself is the renderer's job.
        body["rot_x"] = swing(-88.0)
        body["dy"] = -FALLEN_CENTRE
        body["dz"] = -0.05 - 0.02 * t
        body["scale_z"] = 1.0 - 0.18 * t
        limbs["vg_arm_l"] = swing(-50.0)
        limbs["vg_arm_r"] = swing(-70.0)
        limbs["vg_leg_l"] = swing(18.0)
        limbs["vg_leg_r"] = swing(-12.0)
        tool = swing(-70.0)

    return body, limbs, tool


def animate(body_obj, parts):
    rest = {name: tuple(loc) for name, _, _, _, _, loc, _ in PARTS}
    scene = bpy.context.scene
    scene.frame_start, scene.frame_end = FIRST, LAST

    for anim, (first, last) in SPANS.items():
        count = last - first + 1
        for i in range(count):
            frame = first + i
            b, limbs, tool = pose(anim, i, count)

            body_obj.location = (0.0, b["dy"], b["dz"])
            body_obj.rotation_euler = (b["rot_x"], 0.0, 0.0)
            body_obj.scale = (1.0, 1.0, b["scale_z"])
            body_obj.keyframe_insert("location", frame=frame)
            body_obj.keyframe_insert("rotation_euler", frame=frame)
            body_obj.keyframe_insert("scale", frame=frame)

            for name, obj in parts.items():
                obj.location = rest[name]
                angle = limbs.get(name, 0.0)
                if name in ("vg_tool", "vg_blade"):
                    angle = tool
                obj.rotation_euler = (angle, 0.0, 0.0)
                obj.keyframe_insert("location", frame=frame)
                obj.keyframe_insert("rotation_euler", frame=frame)

    # Every frame is keyed, so hold each pose rather than easing between them:
    # the sprite samples frames, it does not play the curve.
    for obj in [body_obj] + list(parts.values()):
        if obj.animation_data and obj.animation_data.action:
            for fcurve in obj.animation_data.action.fcurves:
                for kp in fcurve.keyframe_points:
                    kp.interpolation = "LINEAR"


def _args():
    return sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []


if __name__ == "__main__":
    args = _args()
    root, body, parts = build()
    animate(body, parts)
    bpy.context.view_layer.update()
    print("built %s: %d parts, frames %d-%d" % (ROOT, len(parts), FIRST, LAST))
    if "--save" in args:
        path = os.path.abspath(args[args.index("--save") + 1])
        bpy.ops.wm.save_as_mainfile(filepath=path)
        print("saved %s" % path)

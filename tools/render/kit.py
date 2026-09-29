"""The modelling kit the slice's subjects are built from.

    import kit        # from a script run by Blender, with tools/render on sys.path

Everything here follows the conventions in README.md: one Blender unit is one
tile, the subject stands on the origin facing +Y, player colour is pure
magenta, and nothing asymmetric carries meaning. The kit gives a subject three
things: primitives (boxes, prisms, cylinders, cones) with matte materials;
a humanoid whose limbs swing about their joints through the five animations a
mobile set needs; and a building whose objects are shown or hidden per frame,
so one file holds its construction stages, its finished state and its rubble.

Models are built by code rather than by hand so that a change to the kit
re-renders every subject the same way, and a fresh checkout can rebuild the
art from nothing (`scripts/render-sprites.sh`).
"""

import math

import bpy

# Frame spans every mobile subject is keyed on, and render_sheet.py is invoked
# with (docs/05 section 2.2).
MOBILE_SPANS = {
    "idle": (1, 4),
    "walk": (5, 12),
    "attack": (13, 18),
    "death": (19, 26),
    "decay": (27, 30),
}

# And every building: three construction stages, the finished building, its
# rubble.
BUILDING_SPANS = {
    "construction": (1, 3),
    "idle": (4, 4),
    "rubble": (5, 5),
}

def srgb(r, g, b):
    """A colour as it should look on screen, in the linear values Blender's
    materials take: a base colour is linear light, so 0.5 renders as a pale
    0.73 on screen."""
    def lin(c):
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    return (lin(r), lin(g), lin(b))


# Base colours, linear. The first six are the greybox villager's, kept so
# every figure's skin and clothes match it; the rest are written as the
# screen colour wanted. Earthy, mid-valued and matte, so the ancient palette
# has a ramp for each; magenta is the player colour and nothing else may be
# near it.
COLOURS = {
    "player": (1.0, 0.0, 1.0),
    "skin": (0.82, 0.60, 0.44),
    "hair": (0.16, 0.11, 0.08),
    "trouser": (0.34, 0.25, 0.17),
    "wood": (0.42, 0.29, 0.16),
    "stone": (0.52, 0.52, 0.55),
    "hide": srgb(0.55, 0.40, 0.26),
    "linen": srgb(0.85, 0.80, 0.68),
    "wood_dark": srgb(0.36, 0.25, 0.16),
    "stone_light": srgb(0.72, 0.70, 0.66),
    "bronze": srgb(0.72, 0.50, 0.24),
    "iron": srgb(0.55, 0.56, 0.60),
    "thatch": srgb(0.70, 0.58, 0.32),
    "mudbrick": srgb(0.62, 0.45, 0.30),
    "plaster": srgb(0.86, 0.80, 0.68),
    "clay_roof": srgb(0.62, 0.30, 0.18),
    "horse": srgb(0.50, 0.34, 0.20),
    "horse_dark": srgb(0.28, 0.19, 0.12),
    "rope": srgb(0.64, 0.54, 0.36),
    "earth": srgb(0.42, 0.32, 0.22),
    "straw": srgb(0.80, 0.70, 0.40),
    "crop": srgb(0.46, 0.58, 0.24),
    "slab": srgb(0.55, 0.51, 0.45),
    "white": srgb(0.88, 0.86, 0.80),
}


def material(name):
    mat = bpy.data.materials.get(name)
    if mat is not None:
        return mat
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    r, g, b = COLOURS[name]
    bsdf.inputs["Base Color"].default_value = (r, g, b, 1.0)
    # Matte: a highlight on a 34 px figure is one bright pixel that moves
    # between frames, which reads as noise rather than as shine.
    bsdf.inputs["Roughness"].default_value = 0.9
    for maybe in ("Specular IOR Level", "Specular"):
        if maybe in bsdf.inputs:
            bsdf.inputs[maybe].default_value = 0.1
            break
    return mat


def clear():
    """Empties the scene: Blender's startup Cube, Light and Camera included."""
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)


def _mesh(name, verts, faces, mat, location=(0.0, 0.0, 0.0), rotation=(0.0, 0.0, 0.0)):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate()
    mesh.update()
    mesh.materials.append(material(mat))
    obj = bpy.data.objects.new(name, mesh)
    obj.location = location
    obj.rotation_euler = rotation
    bpy.context.scene.collection.objects.link(obj)
    return obj


def box(name, size, mat, location=(0.0, 0.0, 0.0), pivot="bottom", rotation=(0.0, 0.0, 0.0)):
    """A box `size` = (x, y, z). Its origin sits at `pivot`: "bottom" and
    "top" centre the other two axes, so a limb hung from "top" swings about
    its joint; "centre" is the middle."""
    w, d, h = size
    x0, x1 = -w / 2.0, w / 2.0
    y0, y1 = -d / 2.0, d / 2.0
    z0, z1 = {"bottom": (0.0, h), "top": (-h, 0.0), "centre": (-h / 2.0, h / 2.0)}[pivot]
    verts = [
        (x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
        (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1),
    ]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4),
             (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return _mesh(name, verts, faces, mat, location, rotation)


def gable(name, size, mat, location=(0.0, 0.0, 0.0), ridge="x"):
    """A gable roof: a triangular prism `size` = (x, y, height), its base on
    `location`, the ridge along `ridge`."""
    w, d, h = size
    x0, x1 = -w / 2.0, w / 2.0
    y0, y1 = -d / 2.0, d / 2.0
    if ridge == "x":
        verts = [(x0, y0, 0), (x1, y0, 0), (x1, y1, 0), (x0, y1, 0), (x0, 0, h), (x1, 0, h)]
        faces = [(0, 3, 2, 1), (0, 1, 5, 4), (2, 3, 4, 5), (0, 4, 3), (1, 2, 5)]
    else:
        verts = [(x0, y0, 0), (x1, y0, 0), (x1, y1, 0), (x0, y1, 0), (0, y0, h), (0, y1, h)]
        faces = [(0, 3, 2, 1), (1, 2, 5, 4), (3, 0, 4, 5), (0, 1, 4), (2, 3, 5)]
    return _mesh(name, verts, faces, mat, location)


def pyramid(name, size, mat, location=(0.0, 0.0, 0.0)):
    """A four-sided pyramid `size` = (x, y, height) on `location`."""
    w, d, h = size
    x0, x1 = -w / 2.0, w / 2.0
    y0, y1 = -d / 2.0, d / 2.0
    verts = [(x0, y0, 0), (x1, y0, 0), (x1, y1, 0), (x0, y1, 0), (0, 0, h)]
    faces = [(0, 3, 2, 1), (0, 1, 4), (1, 2, 4), (2, 3, 4), (3, 0, 4)]
    return _mesh(name, verts, faces, mat, location)


def cylinder(name, radius, height, mat, location=(0.0, 0.0, 0.0), sides=10,
             pivot="bottom", rotation=(0.0, 0.0, 0.0), top_radius=None):
    """A cylinder along z (a cone or frustum when `top_radius` differs)."""
    top = radius if top_radius is None else top_radius
    z0, z1 = {"bottom": (0.0, height), "top": (-height, 0.0),
              "centre": (-height / 2.0, height / 2.0)}[pivot]
    verts, faces = [], []
    for i in range(sides):
        a = 2.0 * math.pi * i / sides
        verts.append((radius * math.cos(a), radius * math.sin(a), z0))
    for i in range(sides):
        a = 2.0 * math.pi * i / sides
        verts.append((top * math.cos(a), top * math.sin(a), z1))
    for i in range(sides):
        j = (i + 1) % sides
        faces.append((i, j, sides + j, sides + i))
    faces.append(tuple(reversed(range(sides))))
    if top > 0.0:
        faces.append(tuple(range(sides, 2 * sides)))
    return _mesh(name, verts, faces, mat, location, rotation)


def cone(name, radius, height, mat, location=(0.0, 0.0, 0.0), sides=10):
    """A cone along z, point up, base on `location`."""
    verts = [(radius * math.cos(2 * math.pi * i / sides),
              radius * math.sin(2 * math.pi * i / sides), 0.0) for i in range(sides)]
    verts.append((0.0, 0.0, height))
    faces = [tuple(reversed(range(sides)))]
    faces += [(i, (i + 1) % sides, sides) for i in range(sides)]
    return _mesh(name, verts, faces, mat, location)


def empty(name, parent=None, location=(0.0, 0.0, 0.0)):
    obj = bpy.data.objects.new(name, None)
    obj.location = location
    bpy.context.scene.collection.objects.link(obj)
    if parent is not None:
        obj.parent = parent
    return obj


def hold_frames(objects):
    """Every frame is keyed, so each pose is held rather than eased into:
    the sprite samples frames, it does not play the curve."""
    for obj in objects:
        anim = obj.animation_data
        if anim is None or anim.action is None:
            continue
        curves = getattr(anim.action, "fcurves", None)
        if curves is None:
            # Blender 4.4+ layered actions keep curves in channel bags.
            curves = [c for layer in anim.action.layers for strip in layer.strips
                      for bag in strip.channelbags for c in bag.fcurves]
        for fcurve in curves:
            for kp in fcurve.keyframe_points:
                kp.interpolation = "CONSTANT"


# --------------------------------------------------------------------------
# The humanoid.

ARM_LENGTH = 0.26
SHOULDER_Z = 0.64
HIP_Z = 0.34

# Half the standing height: a body pitched flat about its feet lies out this
# far, so this is how far back it slides to stay on its own tile.
FALLEN_CENTRE = 0.43


class Humanoid:
    """A figure 0.86 units tall, the villager's proportions, built from boxes.

    `dress` picks the clothes: "tunic" (legs showing) or "robe" (a skirt to
    the ground, for the priest). Heads can carry a `helmet` ("cap", "crest",
    "cone" or None). A `weapon` is held in the right hand and a `shield` on
    the left arm. Parts are parented to `body`, one level below the root, so
    the root stays free for render_sheet.py's turntable.
    """

    def __init__(self, root_name, tunic="player", dress="tunic", helmet=None,
                 helmet_mat="bronze", hair=True):
        self.root = empty(root_name)
        self.body = empty(root_name + "_body", parent=self.root)
        self.parts = {}
        add = self._add
        legs = "trouser" if dress == "tunic" else tunic
        add("leg_l", box("leg_l", (0.10, 0.11, HIP_Z), legs, (-0.062, 0.0, HIP_Z), "top"))
        add("leg_r", box("leg_r", (0.10, 0.11, HIP_Z), legs, (0.062, 0.0, HIP_Z), "top"))
        add("torso", box("torso", (0.27, 0.17, 0.32), tunic, (0.0, 0.0, HIP_Z)))
        if dress == "robe":
            # A skirt to the ground hides the legs' swing but keeps the walk.
            add("skirt", box("skirt", (0.29, 0.19, HIP_Z), tunic, (0.0, 0.0, 0.0)))
        add("head", box("head", (0.19, 0.18, 0.17), "skin", (0.0, 0.01, 0.66)))
        if hair and helmet is None:
            add("hair", box("hair", (0.20, 0.19, 0.05), "hair", (0.0, 0.01, 0.81)))
        if helmet == "cap":
            add("helmet", box("helmet", (0.21, 0.20, 0.08), helmet_mat, (0.0, 0.01, 0.79)))
        elif helmet == "crest":
            add("helmet", box("helmet", (0.21, 0.20, 0.08), helmet_mat, (0.0, 0.01, 0.79)))
            add("crest", box("crest", (0.04, 0.22, 0.07), "player", (0.0, 0.0, 0.86)))
        elif helmet == "band":
            add("hair", box("hair", (0.20, 0.19, 0.05), "hair", (0.0, 0.01, 0.81)))
            add("helmet", box("band", (0.205, 0.195, 0.04), "linen", (0.0, 0.01, 0.76)))
        elif helmet == "cone":
            add("helmet", box("helmet_rim", (0.21, 0.20, 0.04), helmet_mat, (0.0, 0.01, 0.79)))
            add("helmet_top", pyramid("helmet_top", (0.19, 0.18, 0.12), helmet_mat,
                                      (0.0, 0.01, 0.83)))
        add("arm_l", box("arm_l", (0.075, 0.09, ARM_LENGTH), "skin",
                         (-0.17, 0.0, SHOULDER_Z), "top"))
        add("arm_r", box("arm_r", (0.075, 0.09, ARM_LENGTH), "skin",
                         (0.17, 0.0, SHOULDER_Z), "top"))
        self.weapon = None
        self.weapon_hand = "right"
        self.weapon_lean = 0.0
        self.shield = None

    def _add(self, key, obj):
        obj.parent = self.body
        self.parts[key] = obj
        return obj

    def hold(self, weapon, hand="right", lean=22.0):
        """`weapon` is an empty whose children are the weapon, gripped at its
        origin. It follows the `hand` and keeps its own angle, leaned `lean`
        degrees out from the body so it shows past the figure's outline
        when the figure faces the viewer."""
        weapon.parent = self.body
        self.weapon = weapon
        self.weapon_hand = hand
        self.weapon_lean = _deg(lean) * (1.0 if hand == "right" else -1.0)

    def wear(self, key, obj):
        """Something that rides on the body, a quiver say, and moves with it."""
        return self._add(key, obj)

    def carry_shield(self, shield):
        """`shield` hangs on the left forearm and swings with it."""
        shield.parent = self.parts["arm_l"]
        self.shield = shield

    def animate(self, style):
        """Keys all five animations. `style` is the attack: "swing" (an
        overhead blow), "thrust" (a spear), "shoot" (a bow), "sling" or
        "bless"."""
        scene = bpy.context.scene
        scene.frame_start, scene.frame_end = 1, 30
        rest = {k: tuple(o.location) for k, o in self.parts.items()}
        for anim, (first, last) in MOBILE_SPANS.items():
            count = last - first + 1
            for i in range(count):
                frame = first + i
                body, limbs, weapon_angle = humanoid_pose(anim, i, count, style)
                self.body.location = (0.0, body["dy"], body["dz"])
                self.body.rotation_euler = (body["rot_x"], 0.0, 0.0)
                self.body.scale = (1.0, 1.0, body["scale_z"])
                for obj_path in ("location", "rotation_euler", "scale"):
                    self.body.keyframe_insert(obj_path, frame=frame)
                for key, obj in self.parts.items():
                    obj.location = rest[key]
                    obj.rotation_euler = (limbs.get(key, 0.0), 0.0, 0.0)
                    obj.keyframe_insert("location", frame=frame)
                    obj.keyframe_insert("rotation_euler", frame=frame)
                if self.weapon is not None:
                    arm, side = (("arm_l", -1.0) if self.weapon_hand == "left"
                                 else ("arm_r", 1.0))
                    self.weapon.location = hand(limbs.get(arm, 0.0), side=side)
                    self.weapon.rotation_euler = (weapon_angle, self.weapon_lean, 0.0)
                    self.weapon.keyframe_insert("location", frame=frame)
                    self.weapon.keyframe_insert("rotation_euler", frame=frame)
        hold_frames([self.body, self.weapon] + list(self.parts.values()))


def hand(arm_angle, side):
    """Where the right (side 1) or left (-1) hand is, in body space, with the
    arm swung `arm_angle` about the shoulder."""
    x = 0.17 * side
    y = ARM_LENGTH * math.sin(arm_angle)
    z = SHOULDER_Z - ARM_LENGTH * math.cos(arm_angle)
    return (x, y, z)


def _deg(d):
    return math.radians(d)


def humanoid_pose(anim, i, count, style):
    """One frame: the body's bob and pitch, each limb's swing, the weapon's
    angle. Positive swing brings a limb forward (+Y). At 34 px the silhouette
    is the whole performance, so the motion is broad and simple."""
    body = {"dy": 0.0, "dz": 0.0, "rot_x": 0.0, "scale_z": 1.0}
    limbs = {"leg_l": 0.0, "leg_r": 0.0, "arm_l": 0.0, "arm_r": 0.0}
    # The weapon's angle from upright: 0 holds it point up, -90 points it
    # forward.
    carry = {"thrust": _deg(-12.0), "swing": _deg(-20.0), "shoot": 0.0,
             "sling": 0.0, "bless": 0.0}.get(style, 0.0)
    weapon = carry

    if anim == "idle":
        body["dz"] = 0.006 * math.sin(2.0 * math.pi * i / count)
        limbs["arm_l"] = _deg(4.0)
        limbs["arm_r"] = _deg(10.0)

    elif anim == "walk":
        phase = 2.0 * math.pi * i / count
        limbs["leg_l"] = _deg(26.0) * math.sin(phase)
        limbs["leg_r"] = _deg(26.0) * math.sin(phase + math.pi)
        limbs["arm_l"] = _deg(18.0) * math.sin(phase + math.pi)
        limbs["arm_r"] = _deg(10.0) + _deg(8.0) * math.sin(phase)
        body["dz"] = 0.018 * abs(math.sin(phase))

    elif anim == "attack":
        # The blow lands on index 3, the manifest's impact frame.
        if style == "thrust":
            keys = [20.0, 35.0, 10.0, 80.0, 70.0, 40.0]
            limbs["arm_r"] = _deg(keys[i])
            limbs["arm_l"] = _deg(keys[i] * 0.6)
            weapon = _deg(-90.0) if i >= 2 else _deg(-60.0)
            body["rot_x"] = _deg(8.0) if i == 3 else 0.0
            limbs["leg_l"] = _deg(16.0)
            limbs["leg_r"] = _deg(-10.0)
        elif style == "shoot":
            # Bow up at full draw, loosed on the impact frame.
            draw = [60.0, 80.0, 85.0, 85.0, 80.0, 60.0]
            limbs["arm_l"] = _deg(draw[i])
            limbs["arm_r"] = _deg(draw[i] - (35.0 if i < 3 else 10.0))
        elif style == "sling":
            keys = [-40.0, -120.0, -200.0, -280.0, -330.0, -360.0]
            limbs["arm_r"] = _deg(keys[i])
            limbs["arm_l"] = _deg(20.0)
            weapon = _deg(keys[i])
        elif style == "bless":
            lift = [30.0, 70.0, 110.0, 140.0, 120.0, 60.0]
            limbs["arm_l"] = _deg(lift[i])
            limbs["arm_r"] = _deg(lift[i])
        else:
            keys = [-55.0, -70.0, -20.0, 75.0, 60.0, 30.0]
            limbs["arm_r"] = _deg(keys[i])
            limbs["arm_l"] = _deg(-keys[i] * 0.25)
            weapon = _deg(keys[i]) + carry
            body["rot_x"] = _deg(-keys[i] * 0.12)
            limbs["leg_l"] = _deg(10.0)
            limbs["leg_r"] = _deg(-8.0)

    elif anim in ("death", "decay"):
        if anim == "death":
            t = i / float(count - 1)
            ease = t * t
        else:
            t = 1.0
            ease = 1.0
        body["rot_x"] = _deg(-88.0) * ease
        body["dy"] = -FALLEN_CENTRE * ease
        body["dz"] = -0.05 * t
        limbs["arm_l"] = _deg(-50.0) * t
        limbs["arm_r"] = _deg(-70.0) * t
        limbs["leg_l"] = _deg(18.0) * t
        limbs["leg_r"] = _deg(-12.0) * t
        weapon = carry + _deg(-70.0) * t
        if anim == "decay":
            s = (i + 1) / float(count)
            body["dz"] = -0.05 - 0.02 * s
            body["scale_z"] = 1.0 - 0.18 * s

    return body, limbs, weapon


# --------------------------------------------------------------------------
# Weapons, each gripped at its empty's origin and standing point up.

def spear(name):
    grip = empty(name)
    shaft = cylinder(name + "_shaft", 0.026, 1.00, "wood", (0.0, 0.0, -0.32), sides=6)
    tip = cone(name + "_tip", 0.05, 0.16, "bronze", (0.0, 0.0, 0.68), sides=6)
    for part in (shaft, tip):
        part.parent = grip
    return grip


def club(name, head="wood_dark"):
    grip = empty(name)
    handle = cylinder(name + "_handle", 0.03, 0.32, "wood", (0.0, 0.0, -0.06), sides=6)
    knob = cylinder(name + "_head", 0.075, 0.18, head, (0.0, 0.0, 0.22), sides=8,
                    top_radius=0.06)
    for part in (handle, knob):
        part.parent = grip
    return grip


def axe(name):
    grip = empty(name)
    handle = cylinder(name + "_handle", 0.028, 0.42, "wood", (0.0, 0.0, -0.10), sides=6)
    blade = box(name + "_blade", (0.045, 0.20, 0.15), "bronze", (0.0, 0.08, 0.18))
    for part in (handle, blade):
        part.parent = grip
    return grip


def sword(name):
    grip = empty(name)
    hilt = box(name + "_hilt", (0.035, 0.035, 0.09), "wood_dark", (0.0, 0.0, -0.05))
    guard = box(name + "_guard", (0.13, 0.04, 0.03), "bronze", (0.0, 0.0, 0.04))
    blade = box(name + "_blade", (0.055, 0.02, 0.38), "iron", (0.0, 0.0, 0.07))
    for part in (hilt, guard, blade):
        part.parent = grip
    return grip


def bow(name):
    grip = empty(name)
    upper = box(name + "_upper", (0.035, 0.035, 0.34), "wood", (0.0, 0.04, 0.0),
                rotation=(_deg(-14.0), 0, 0))
    lower = box(name + "_lower", (0.035, 0.035, 0.34), "wood", (0.0, 0.04, 0.0), pivot="top",
                rotation=(_deg(14.0), 0, 0))
    string = box(name + "_string", (0.012, 0.012, 0.66), "linen", (0.0, -0.04, -0.33))
    for part in (upper, lower, string):
        part.parent = grip
    return grip


def sling(name):
    grip = empty(name)
    cord = box(name + "_cord", (0.018, 0.018, 0.24), "rope", (0.0, 0.0, -0.24))
    pouch = box(name + "_pouch", (0.075, 0.075, 0.075), "hide", (0.0, 0.0, -0.30))
    for part in (cord, pouch):
        part.parent = grip
    return grip


def staff(name):
    grip = empty(name)
    shaft = cylinder(name + "_shaft", 0.026, 0.84, "wood", (0.0, 0.0, -0.32), sides=6)
    top = box(name + "_top", (0.10, 0.10, 0.10), "bronze", (0.0, 0.0, 0.52))
    for part in (shaft, top):
        part.parent = grip
    return grip


def quiver(name):
    """A quiver on the back, its arrows showing over the right shoulder."""
    root = empty(name)
    case = box(name + "_case", (0.07, 0.07, 0.30), "hide", (0.07, -0.11, 0.42),
               rotation=(0.0, _deg(-18.0), 0.0))
    fletch = box(name + "_fletch", (0.06, 0.06, 0.08), "white", (0.12, -0.11, 0.70),
                 rotation=(0.0, _deg(-18.0), 0.0))
    for part in (case, fletch):
        part.parent = root
    return root


def round_shield(name, face="player"):
    """A round shield seen edge-on from the side, face-on from the front."""
    root = empty(name, location=(-0.05, 0.02, -0.16))
    disc = cylinder(name + "_disc", 0.13, 0.03, face, (0.0, 0.0, 0.0), sides=12,
                    pivot="centre", rotation=(0.0, _deg(90.0), 0.0))
    boss = cylinder(name + "_boss", 0.04, 0.035, "bronze", (-0.02, 0.0, 0.0), sides=8,
                    pivot="centre", rotation=(0.0, _deg(90.0), 0.0))
    for part in (disc, boss):
        part.parent = root
    return root


# --------------------------------------------------------------------------
# Buildings.

class Building:
    """A building as five frames: construction stages 1-3, the finished
    building (4), its rubble (5). Each object is tagged with the frames it
    appears on and keyed visible there and hidden elsewhere, so one file
    renders them all. The footprint is `tiles` on a side, centred on the
    origin. The camera sits out at +X +Y, so the +X and +Y faces and the top
    are what shows: doors, banners and player colour go there."""

    def __init__(self, root_name, tiles):
        self.root = empty(root_name)
        self.tiles = tiles
        self.shown = []

    def add(self, obj, frames):
        obj.parent = self.root
        self.shown.append((obj, set(frames)))
        return obj

    def finish(self):
        scene = bpy.context.scene
        scene.frame_start, scene.frame_end = 1, 5
        for obj, frames in self.shown:
            for frame in range(1, 6):
                hidden = frame not in frames
                obj.hide_render = hidden
                obj.hide_viewport = hidden
                obj.keyframe_insert("hide_render", frame=frame)
                obj.keyframe_insert("hide_viewport", frame=frame)
        hold_frames([o for o, _ in self.shown])


# Which frames a building part shows on: the foundation from the first stage,
# the walls from the second, the roof and dressing only when finished, and a
# scaffold during the last stage.
FOUNDATION = (1, 2, 3, 4)
WALLS = (2, 3, 4)
HALF_WALLS = (2,)
FULL_WALLS = (3, 4)
SCAFFOLD = (3,)
FINISHED = (4,)
RUBBLE = (5,)


def rubble(b, name, spread, height, mats=("stone", "mudbrick"), count=9, seed=1):
    """Broken pieces across the footprint, for the rubble frame."""
    import random
    rng = random.Random(seed)
    half = spread / 2.0
    for i in range(count):
        w = rng.uniform(0.10, 0.28) * spread
        d = rng.uniform(0.10, 0.28) * spread
        h = rng.uniform(0.25, 1.0) * height
        x = rng.uniform(-half + w / 2, half - w / 2)
        y = rng.uniform(-half + d / 2, half - d / 2)
        piece = box("%s_rubble_%d" % (name, i), (w, d, h), mats[i % len(mats)], (x, y, 0.0),
                    rotation=(0.0, 0.0, rng.uniform(-0.6, 0.6)))
        b.add(piece, RUBBLE)


def scaffold(b, name, w, d, h):
    """Poles at the corners and a rail round the top, for the last stage."""
    for i, (x, y) in enumerate([(-w / 2, -d / 2), (w / 2, -d / 2), (w / 2, d / 2), (-w / 2, d / 2)]):
        b.add(cylinder("%s_pole_%d" % (name, i), 0.025, h, "wood", (x, y, 0.0), sides=5), SCAFFOLD)
    b.add(box(name + "_rail_x0", (w, 0.04, 0.04), "wood", (0.0, -d / 2, h * 0.7)), SCAFFOLD)
    b.add(box(name + "_rail_x1", (w, 0.04, 0.04), "wood", (0.0, d / 2, h * 0.7)), SCAFFOLD)
    b.add(box(name + "_rail_y0", (0.04, d, 0.04), "wood", (-w / 2, 0.0, h * 0.7)), SCAFFOLD)
    b.add(box(name + "_rail_y1", (0.04, d, 0.04), "wood", (w / 2, 0.0, h * 0.7)), SCAFFOLD)


def walls(b, name, w, d, h, mat, door=True):
    """A walled block with its full height on the finished frames and half
    its height on the second stage; a door on the +Y face, one of the two the
    camera sees (it sits out at +X +Y)."""
    b.add(box(name + "_walls_half", (w, d, h * 0.5), mat), HALF_WALLS)
    b.add(box(name + "_walls", (w, d, h), mat), FULL_WALLS)
    if door:
        b.add(box(name + "_door", (w * 0.22, 0.02, h * 0.62), "wood_dark",
                  (0.0, d / 2 + 0.005, 0.0)), FULL_WALLS)
    b.add(box(name + "_foundation", (w + 0.08, d + 0.08, 0.04), "slab"), FOUNDATION)


def foundation(b, name, w, d, mat="slab"):
    """The plot marked out: a low slab, there from the first stage on."""
    b.add(box(name + "_foundation", (w, d, 0.04), mat), FOUNDATION)


def flag(b, name, x, y, base, height=0.55, frames=FINISHED):
    """A pole with a flag of the owner's colour, flying toward +X so the
    camera sees it broadside."""
    b.add(cylinder(name + "_pole", 0.02, height, "wood_dark", (x, y, base), sides=5), frames)
    b.add(box(name + "_flag", (0.22, 0.02, 0.14), "player",
              (x + 0.11, y, base + height - 0.16)), frames)


def posts(b, name, points, height, frames, radius=0.035, mat="wood"):
    """Upright posts at `points`, standing on the ground."""
    for i, (x, y) in enumerate(points):
        b.add(cylinder("%s_%d" % (name, i), radius, height, mat, (x, y, 0.0), sides=6), frames)


def rail(b, name, a, c, z, frames, mat="wood", thick=0.035):
    """A rail from point `a` to point `c` at height `z`."""
    (x0, y0), (x1, y1) = a, c
    length = math.hypot(x1 - x0, y1 - y0)
    angle = math.atan2(y1 - y0, x1 - x0)
    b.add(box(name, (length, thick, thick), mat, ((x0 + x1) / 2, (y0 + y1) / 2, z),
              pivot="centre", rotation=(0.0, 0.0, angle)), frames)


def disc(b, name, radius, thick, mat, location, frames, facing="y"):
    """A flat disc standing on edge, its face toward +Y (or +X)."""
    rot = (_deg(90.0), 0.0, 0.0) if facing == "y" else (0.0, _deg(90.0), 0.0)
    b.add(cylinder(name, radius, thick, mat, location, sides=12, pivot="centre",
                   rotation=rot), frames)


# --------------------------------------------------------------------------
# The horse and its rider.

class Rider:
    """A horse 0.72 units nose to tail with a rider on its back, facing +Y.

    The horse's legs swing from the hip in diagonal pairs at the walk; the
    rider sits, carries the `weapon` in the right hand and strikes with it.
    Everything hangs off `body`, one level below the root, so the root stays
    free for render_sheet.py's turntable.
    """

    LEG = 0.40
    BACK = 0.62

    def __init__(self, root_name, coat="horse", tunic="player", helmet=None,
                 helmet_mat="bronze", saddle_cloth=True):
        self.root = empty(root_name)
        self.body = empty(root_name + "_body", parent=self.root)
        self.parts = {}
        add = self._add
        leg, back = self.LEG, self.BACK
        add("barrel", box("barrel", (0.24, 0.66, back - leg), coat, (0.0, 0.0, leg)))
        add("neck", box("neck", (0.13, 0.16, 0.30), coat, (0.0, 0.30, back - 0.08),
                        rotation=(_deg(-30.0), 0.0, 0.0)))
        add("head", box("head", (0.12, 0.26, 0.12), coat, (0.0, 0.44, back + 0.16)))
        add("mane", box("mane", (0.04, 0.16, 0.10), "horse_dark", (0.0, 0.26, back + 0.10),
                        rotation=(_deg(-30.0), 0.0, 0.0)))
        add("tail", box("tail", (0.05, 0.05, 0.28), "horse_dark", (0.0, -0.33, back - 0.02),
                        "top", rotation=(_deg(-25.0), 0.0, 0.0)))
        for key, x, y in (("leg_fl", -0.08, 0.25), ("leg_fr", 0.08, 0.25),
                          ("leg_bl", -0.08, -0.25), ("leg_br", 0.08, -0.25)):
            add(key, box(key, (0.07, 0.08, leg), coat, (x, y, leg), "top"))
        if saddle_cloth:
            add("cloth", box("cloth", (0.28, 0.24, 0.10), tunic, (0.0, -0.02, back - 0.06)))
        # The rider, seated: legs down the horse's sides, torso on its back.
        seat = back + 0.02
        add("r_leg_l", box("r_leg_l", (0.08, 0.10, 0.24), "trouser", (-0.15, 0.02, seat), "top"))
        add("r_leg_r", box("r_leg_r", (0.08, 0.10, 0.24), "trouser", (0.15, 0.02, seat), "top"))
        add("r_torso", box("r_torso", (0.24, 0.15, 0.28), tunic, (0.0, -0.02, seat)))
        add("r_head", box("r_head", (0.17, 0.16, 0.15), "skin", (0.0, -0.01, seat + 0.29)))
        top = seat + 0.44
        if helmet is None:
            add("r_hair", box("r_hair", (0.18, 0.17, 0.05), "hair", (0.0, -0.01, top)))
        elif helmet == "cone":
            add("r_helmet", box("r_helmet", (0.19, 0.18, 0.04), helmet_mat, (0.0, -0.01, top)))
            add("r_helmet_top", pyramid("r_helmet_top", (0.17, 0.16, 0.11), helmet_mat,
                                        (0.0, -0.01, top + 0.04)))
        else:
            add("r_helmet", box("r_helmet", (0.19, 0.18, 0.07), helmet_mat, (0.0, -0.01, top)))
        self.shoulder_z = seat + 0.27
        add("r_arm_l", box("r_arm_l", (0.07, 0.08, 0.22), "skin", (-0.155, 0.0, self.shoulder_z), "top"))
        add("r_arm_r", box("r_arm_r", (0.07, 0.08, 0.22), "skin", (0.155, 0.0, self.shoulder_z), "top"))
        self.weapon = None

    def _add(self, key, obj):
        obj.parent = self.body
        self.parts[key] = obj
        return obj

    def hold(self, weapon, lean=18.0):
        weapon.parent = self.body
        self.weapon = weapon
        self.weapon_lean = _deg(lean)

    def animate(self):
        scene = bpy.context.scene
        scene.frame_start, scene.frame_end = 1, 30
        rest = {k: (tuple(o.location), tuple(o.rotation_euler)) for k, o in self.parts.items()}
        for anim, (first, last) in MOBILE_SPANS.items():
            count = last - first + 1
            for i in range(count):
                frame = first + i
                body, swings, weapon_angle = rider_pose(anim, i, count)
                self.body.location = (body["dx"], 0.0, body["dz"])
                self.body.rotation_euler = (0.0, body["roll"], 0.0)
                self.body.scale = (1.0, 1.0, body["scale_z"])
                for path in ("location", "rotation_euler", "scale"):
                    self.body.keyframe_insert(path, frame=frame)
                for key, obj in self.parts.items():
                    loc, rot = rest[key]
                    obj.location = loc
                    obj.rotation_euler = (rot[0] + swings.get(key, 0.0), rot[1], rot[2])
                    obj.keyframe_insert("location", frame=frame)
                    obj.keyframe_insert("rotation_euler", frame=frame)
                if self.weapon is not None:
                    arm = swings.get("r_arm_r", 0.0)
                    self.weapon.location = (0.155, 0.22 * math.sin(arm),
                                            self.shoulder_z - 0.22 * math.cos(arm))
                    self.weapon.rotation_euler = (weapon_angle, self.weapon_lean, 0.0)
                    self.weapon.keyframe_insert("location", frame=frame)
                    self.weapon.keyframe_insert("rotation_euler", frame=frame)
        hold_frames([self.body, self.weapon] + list(self.parts.values()))


# Rolled onto its side about its hooves, a horse and rider lie out along +X
# with their middle about this far from the origin; the body slides back by
# it as it falls, so the carcass stays on its own tile.
FALLEN_HORSE = 0.45


def rider_pose(anim, i, count):
    """One frame for horse and rider: the body's bob, roll and slide, each
    part's swing about x, the rider's weapon angle."""
    body = {"dx": 0.0, "dz": 0.0, "roll": 0.0, "scale_z": 1.0}
    s = {}
    weapon = _deg(-15.0)

    if anim == "idle":
        s["head"] = _deg(4.0) * math.sin(2.0 * math.pi * i / count)
        s["tail"] = _deg(6.0) * math.sin(2.0 * math.pi * i / count + 1.0)
        s["r_arm_r"] = _deg(15.0)

    elif anim == "walk":
        phase = 2.0 * math.pi * i / count
        a = _deg(24.0) * math.sin(phase)
        # Diagonal pairs: near fore with off hind, then the other two.
        s["leg_fl"], s["leg_br"] = a, a
        s["leg_fr"], s["leg_bl"] = -a, -a
        s["head"] = _deg(5.0) * math.sin(2.0 * phase)
        s["tail"] = _deg(8.0) * math.sin(phase)
        body["dz"] = 0.015 * abs(math.sin(phase))
        s["r_arm_r"] = _deg(15.0)
        s["r_arm_l"] = _deg(10.0)

    elif anim == "attack":
        keys = [20.0, 40.0, 10.0, 85.0, 70.0, 35.0]
        s["r_arm_r"] = _deg(keys[i])
        s["r_arm_l"] = _deg(20.0)
        weapon = _deg(-90.0) if i >= 2 else _deg(-55.0)
        s["head"] = _deg(-8.0) if i == 3 else 0.0

    elif anim in ("death", "decay"):
        t = i / float(count - 1) if anim == "death" else 1.0
        ease = t * t
        body["roll"] = _deg(85.0) * ease
        body["dx"] = -FALLEN_HORSE * ease
        body["dz"] = -0.02 * t
        for leg in ("leg_fl", "leg_fr", "leg_bl", "leg_br"):
            s[leg] = _deg(20.0) * t
        s["head"] = _deg(25.0) * t
        s["r_arm_r"] = _deg(-40.0) * t
        weapon = _deg(-15.0) + _deg(-60.0) * t
        if anim == "decay":
            k = (i + 1) / float(count)
            body["scale_z"] = 1.0 - 0.15 * k
            body["dz"] = -0.02 - 0.02 * k

    return body, s, weapon


def lance(name):
    grip = empty(name)
    shaft = cylinder(name + "_shaft", 0.024, 1.05, "wood", (0.0, 0.0, -0.40), sides=6)
    tip = cone(name + "_tip", 0.045, 0.14, "bronze", (0.0, 0.0, 0.65), sides=6)
    for part in (shaft, tip):
        part.parent = grip
    return grip


def javelin(name):
    grip = empty(name)
    shaft = cylinder(name + "_shaft", 0.022, 0.60, "wood", (0.0, 0.0, -0.20), sides=6)
    tip = cone(name + "_tip", 0.04, 0.10, "bronze", (0.0, 0.0, 0.40), sides=6)
    for part in (shaft, tip):
        part.parent = grip
    return grip

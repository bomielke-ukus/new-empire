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

# A wall: the building's five frames, its post alone as the finished one,
# then an arm toward each of the eight neighbours it can join.
WALL_SPANS = dict(BUILDING_SPANS, arm=(6, 13))

# A gate: the building's five, the finished one shut along the game's x,
# then shut and open in each of its four orientations.
GATE_SPANS = dict(BUILDING_SPANS, shut=(6, 9), open=(10, 13))

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
    "thatch": srgb(0.62, 0.53, 0.34),
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
    "horn": srgb(0.30, 0.26, 0.22),
    "leaf": srgb(0.26, 0.46, 0.18),
    "leaf_dark": srgb(0.18, 0.34, 0.14),
    "bark": srgb(0.34, 0.24, 0.15),
    "berry": srgb(0.72, 0.14, 0.14),
    "gold": srgb(0.92, 0.74, 0.22),
    "rock": srgb(0.50, 0.49, 0.47),
    "rock_light": srgb(0.66, 0.64, 0.60),
    "white": srgb(0.88, 0.86, 0.80),
    # The dark of a window or a doorway: the room behind it, unlit.
    "opening": srgb(0.10, 0.08, 0.07),
    # The war elephant's hide and toenails; tusks.
    "elephant": srgb(0.5, 0.48, 0.46),
    "elephant_toe": srgb(0.66, 0.63, 0.58),
    "ivory": srgb(0.93, 0.9, 0.82),
    # Boats: the tarred hull below the strakes; fish, and the foam they
    # break the water with.
    "pitch": srgb(0.20, 0.15, 0.11),
    "fish": srgb(0.64, 0.68, 0.72),
    "fish_dark": srgb(0.32, 0.38, 0.44),
    "foam": srgb(0.86, 0.90, 0.92),
    # Split-wood shingles, the Tool Age's roofs; slate, the Iron Age's.
    "shingle": srgb(0.46, 0.37, 0.27),
    "slate": srgb(0.36, 0.39, 0.44),
    # The other architectures (docs/07 D34). Egyptian: sandstone, white
    # limestone, and paint of blue and red ochre. Mesopotamian: baked brick
    # and blue glaze. East Asian: rammed earth, red lacquer, dark tile.
    "sandstone": srgb(0.80, 0.70, 0.52),
    "limestone": srgb(0.90, 0.86, 0.76),
    "paint_blue": srgb(0.24, 0.40, 0.60),
    "paint_red": srgb(0.64, 0.27, 0.17),
    "baked_brick": srgb(0.52, 0.36, 0.26),
    "glaze": srgb(0.20, 0.36, 0.60),
    "rammed": srgb(0.68, 0.56, 0.38),
    "lacquer": srgb(0.58, 0.20, 0.13),
    "tile_dark": srgb(0.26, 0.28, 0.32),
}


# How each material's surface is broken up, so a wall reads as brick and a
# post as wood rather than as flat colour: a pattern in object space, how far
# it moves the colour either side of the base (which stays the material's
# colour on average, so the palette match does not move), and how much relief
# it gives the surface. Patterns: "noise", "grain" (noise drawn out along
# the object's length), "brick" (courses on the vertical faces) and
# "courses" (horizontal bands, roof tiles).
SURFACES = {
    "player": ("noise", 30.0, 0.05, 0.1),
    "skin": ("noise", 30.0, 0.04, 0.05),
    "hair": ("grain", 60.0, 0.10, 0.2),
    "trouser": ("noise", 40.0, 0.06, 0.1),
    "hide": ("noise", 35.0, 0.10, 0.2),
    "linen": ("noise", 45.0, 0.06, 0.1),
    "rope": ("grain", 60.0, 0.12, 0.2),
    "wood": ("grain", 30.0, 0.16, 0.25),
    "wood_dark": ("grain", 30.0, 0.16, 0.25),
    "bark": ("grain", 45.0, 0.24, 0.5),
    "stone": ("noise", 18.0, 0.14, 0.4),
    "stone_light": ("brick", 3.0, 0.10, 0.3),
    "rock": ("noise", 16.0, 0.16, 0.5),
    "rock_light": ("noise", 16.0, 0.14, 0.5),
    "slab": ("noise", 20.0, 0.10, 0.3),
    "mudbrick": ("brick", 4.0, 0.12, 0.3),
    "plaster": ("noise", 10.0, 0.06, 0.1),
    "white": ("noise", 10.0, 0.05, 0.1),
    "thatch": ("grain", 24.0, 0.12, 0.3),
    "straw": ("grain", 24.0, 0.10, 0.2),
    "clay_roof": ("courses", 40.0, 0.14, 0.3),
    "shingle": ("courses", 45.0, 0.16, 0.35),
    "slate": ("courses", 40.0, 0.12, 0.3),
    "sandstone": ("brick", 2.5, 0.08, 0.3),
    "limestone": ("brick", 2.0, 0.05, 0.2),
    "paint_blue": ("noise", 30.0, 0.04, 0.05),
    "paint_red": ("noise", 30.0, 0.04, 0.05),
    "baked_brick": ("brick", 5.0, 0.12, 0.3),
    "glaze": ("brick", 5.0, 0.08, 0.2),
    "rammed": ("courses", 14.0, 0.10, 0.2),
    "lacquer": ("noise", 30.0, 0.05, 0.1),
    "tile_dark": ("courses", 45.0, 0.14, 0.35),
    "earth": ("noise", 25.0, 0.12, 0.3),
    "crop": ("noise", 30.0, 0.20, 0.5),
    "leaf": ("noise", 30.0, 0.20, 0.6),
    "leaf_dark": ("noise", 30.0, 0.20, 0.6),
    "horse": ("noise", 30.0, 0.06, 0.1),
    "horse_dark": ("noise", 30.0, 0.06, 0.1),
    "horn": ("grain", 40.0, 0.10, 0.1),
    "bronze": ("noise", 30.0, 0.08, 0.1),
    "iron": ("noise", 30.0, 0.08, 0.1),
    "gold": ("noise", 30.0, 0.10, 0.1),
    "berry": ("noise", 40.0, 0.06, 0.1),
}


def material(name):
    mat = bpy.data.materials.get(name)
    if mat is not None:
        return mat
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = nodes.get("Principled BSDF")
    r, g, b = COLOURS[name]
    # Matte: a highlight on a 34 px figure is one bright pixel that moves
    # between frames, which reads as noise rather than as shine.
    bsdf.inputs["Roughness"].default_value = 0.9
    for maybe in ("Specular IOR Level", "Specular"):
        if maybe in bsdf.inputs:
            bsdf.inputs[maybe].default_value = 0.1
            break
    surface = SURFACES.get(name)
    if surface is None:
        bsdf.inputs["Base Color"].default_value = (r, g, b, 1.0)
        return mat
    pattern, scale, vary, relief = surface
    fac = _pattern(nodes, links, pattern, scale)
    # The base colour times a factor about 1. A brick wall is mostly brick
    # (the pattern's light end) with thin dark mortar, so its factor is
    # skewed for the wall as a whole to average the base colour.
    spread = nodes.new("ShaderNodeMapRange")
    low, high = (1.6, 0.2) if pattern == "brick" else (1.0, 1.0)
    spread.inputs["To Min"].default_value = 1.0 - vary * low
    spread.inputs["To Max"].default_value = 1.0 + vary * high
    links.new(fac, spread.inputs["Value"])
    mix = nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs["Factor"].default_value = 1.0
    mix.inputs["A"].default_value = (r, g, b, 1.0)
    links.new(spread.outputs["Result"], mix.inputs["B"])
    links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = relief
    bump.inputs["Distance"].default_value = 0.02
    links.new(fac, bump.inputs["Height"])
    links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    return mat


def _pattern(nodes, links, pattern, scale):
    """A 0-1 pattern socket in object space."""
    coord = nodes.new("ShaderNodeTexCoord").outputs["Object"]
    if pattern == "brick":
        # Courses on the walls whichever way they face: along x + y, up z.
        sep = nodes.new("ShaderNodeSeparateXYZ")
        links.new(coord, sep.inputs["Vector"])
        add = nodes.new("ShaderNodeMath")
        add.operation = "ADD"
        links.new(sep.outputs["X"], add.inputs[0])
        links.new(sep.outputs["Y"], add.inputs[1])
        comb = nodes.new("ShaderNodeCombineXYZ")
        links.new(add.outputs["Value"], comb.inputs["X"])
        links.new(sep.outputs["Z"], comb.inputs["Y"])
        brick = nodes.new("ShaderNodeTexBrick")
        brick.inputs["Scale"].default_value = scale
        brick.inputs["Color1"].default_value = (1.0, 1.0, 1.0, 1.0)
        brick.inputs["Color2"].default_value = (0.8, 0.8, 0.8, 1.0)
        brick.inputs["Mortar"].default_value = (0.35, 0.35, 0.35, 1.0)
        brick.inputs["Mortar Size"].default_value = 0.025
        links.new(comb.outputs["Vector"], brick.inputs["Vector"])
        grey = nodes.new("ShaderNodeRGBToBW")
        links.new(brick.outputs["Color"], grey.inputs["Color"])
        return grey.outputs["Val"]
    if pattern == "courses":
        wave = nodes.new("ShaderNodeTexWave")
        wave.wave_type = "BANDS"
        wave.bands_direction = "Z"
        wave.inputs["Scale"].default_value = scale
        wave.inputs["Distortion"].default_value = 1.0
        links.new(coord, wave.inputs["Vector"])
        return wave.outputs["Fac"]
    mapping = nodes.new("ShaderNodeMapping")
    stretch = 0.12 if pattern == "grain" else 1.0
    mapping.inputs["Scale"].default_value = (scale, scale, scale * stretch)
    links.new(coord, mapping.inputs["Vector"])
    noise = nodes.new("ShaderNodeTexNoise")
    noise.inputs["Detail"].default_value = 4.0
    links.new(mapping.outputs["Vector"], noise.inputs["Vector"])
    return noise.outputs["Fac"]


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
    soften(obj, 0.12, 2)
    return obj


# The parts of a figure or an animal that are flesh, and so rounded.
BODY_PARTS = {
    "leg_l", "leg_r", "arm_l", "arm_r", "torso", "head", "skirt",
    "shin_l", "shin_r", "fore_l", "fore_r", "kilt",
    "r_leg_l", "r_leg_r", "r_arm_l", "r_arm_r", "r_torso", "r_head",
    "barrel", "neck", "belly", "mane", "tail",
    "leg_fl", "leg_fr", "leg_bl", "leg_br",
}


def soften(obj, fraction, segments, limit=0.03):
    """Rounds `obj`'s sharp edges by `fraction` of its thinnest side (at
    most `limit`) in `segments` steps, smooth-shaded with the flat faces
    kept flat: an edge that catches the light reads as made, a hard box
    edge as a placeholder. Called again, it rounds further."""
    if obj.type != "MESH" or not obj.data.vertices:
        return obj
    xs = [v.co for v in obj.data.vertices]
    dims = [max(c[i] for c in xs) - min(c[i] for c in xs) for i in range(3)]
    sides = [d for d in dims if d > 1e-6]
    if not sides:
        return obj
    bevel = obj.modifiers.get("soften") or obj.modifiers.new("soften", "BEVEL")
    bevel.width = min(limit, min(sides) * fraction)
    bevel.segments = segments
    bevel.limit_method = "ANGLE"
    bevel.angle_limit = math.radians(40.0)
    bevel.harden_normals = True
    for poly in obj.data.polygons:
        poly.use_smooth = True
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


def clump(name, radius, mat, location=(0.0, 0.0, 0.0), seed=0, lumps=0.28, squash=1.0):
    """A lumpy ball of leaves: an icosphere pushed in and out by noise, so
    a crown reads as foliage rather than as a solid."""
    import bmesh
    from mathutils import Vector, noise
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=3, radius=radius)
    offset = Vector((seed * 7.31, seed * 3.17, seed * 5.53))
    for v in bm.verts:
        d = v.co.normalized()
        n = noise.noise(d * 2.6 + offset) + 0.5 * noise.noise(d * 6.0 + offset)
        v.co = d * radius * (1.0 + lumps * n)
        v.co.z *= squash
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    for poly in mesh.polygons:
        poly.use_smooth = True
    mesh.materials.append(material(mat))
    obj = bpy.data.objects.new(name, mesh)
    obj.location = location
    bpy.context.scene.collection.objects.link(obj)
    return obj


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
#
# A figure 0.86 units tall with jointed limbs: a thigh from the hip and a
# shin from the knee down to a sandal, an upper arm from the shoulder and a
# forearm from the elbow down to a hand; a torso broad at the shoulders
# over a kilt, a neck, and a head with a nose, eyes and hair, or a helmet.
# At 70 px on a Retina screen the shapes read; boxes read as toys.

ARM_LENGTH = 0.26
SHOULDER_Z = 0.64
HIP_Z = 0.34
UPPER_ARM = 0.14
FOREARM = 0.12
THIGH = 0.165
SHIN = 0.14
FOOT_H = 0.035
SHOULDER_X = 0.165
# Where the head's centre sits, and its half-sizes.
HEAD_Z = 0.748
HEAD = (0.072, 0.078, 0.088)

# Half the standing height: a body pitched flat about its feet lies out this
# far, so this is how far back it slides to stay on its own tile.
FALLEN_CENTRE = 0.43


def _recalc(obj):
    """Points every face of `obj` outward."""
    import bmesh
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(obj.data)
    bm.free()
    for poly in obj.data.polygons:
        poly.use_smooth = True
    return obj


def ellipsoid(name, radii, mat, location=(0.0, 0.0, 0.0), segments=14, rings=9,
              upper=False):
    """An ellipsoid with half-sizes `radii` about `location`, smooth; only
    its upper half, closed flat, when `upper` (a dome: a helmet, a cap)."""
    rx, ry, rz = radii
    first = rings // 2 if upper else 1
    verts = [] if upper else [(0.0, 0.0, -rz)]
    lats = []
    for r in range(first, rings):
        phi = math.pi * r / rings - math.pi / 2.0
        lats.append(phi)
    if upper:
        lats = [0.0] + [p for p in lats if p > 1e-6]
    for phi in lats:
        for s in range(segments):
            th = 2.0 * math.pi * s / segments
            verts.append((rx * math.cos(phi) * math.cos(th),
                          ry * math.cos(phi) * math.sin(th), rz * math.sin(phi)))
    verts.append((0.0, 0.0, rz))
    top = len(verts) - 1
    base = 0 if upper else 1
    faces = []
    if upper:
        faces.append(tuple(range(segments)))
    else:
        for s in range(segments):
            faces.append((0, base + s, base + (s + 1) % segments))
    for r in range(len(lats) - 1):
        for s in range(segments):
            a = base + r * segments + s
            b = base + r * segments + (s + 1) % segments
            faces.append((a, b, b + segments, a + segments))
    last = base + (len(lats) - 1) * segments
    for s in range(segments):
        faces.append((last + s, last + (s + 1) % segments, top))
    obj = _mesh(name, verts, faces, mat, location)
    return _recalc(obj)


def loft(name, bottom, top, height, mat, location=(0.0, 0.0, 0.0), lean=0.0):
    """A box `height` tall whose bottom is `bottom` = (x, y) across and its
    top `top`, the top pushed `lean` forward: a torso, a kilt, a cloth."""
    (bw, bd), (tw, td) = bottom, top
    verts = [(-bw / 2, -bd / 2, 0.0), (bw / 2, -bd / 2, 0.0), (bw / 2, bd / 2, 0.0),
             (-bw / 2, bd / 2, 0.0),
             (-tw / 2, -td / 2 + lean, height), (tw / 2, -td / 2 + lean, height),
             (tw / 2, td / 2 + lean, height), (-tw / 2, td / 2 + lean, height)]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4),
             (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return _mesh(name, verts, faces, mat, location)


def oval(name, bottom, top, height, mat, location=(0.0, 0.0, 0.0), lean=0.0, sides=16):
    """A frustum of oval section `height` tall: half-sizes `bottom` = (x, y)
    at its foot and `top` at its head, the top pushed `lean` forward. A
    torso that narrows to the waist, a kilt that flares."""
    (bx, by), (tx, ty) = bottom, top
    verts = []
    for rx, ry, z, dy in ((bx, by, 0.0, 0.0), (tx, ty, height, lean)):
        for k in range(sides):
            a = 2.0 * math.pi * k / sides
            verts.append((rx * math.cos(a), ry * math.sin(a) + dy, z))
    faces = [tuple(reversed(range(sides))), tuple(range(sides, 2 * sides))]
    for k in range(sides):
        j = (k + 1) % sides
        faces.append((k, j, sides + j, sides + k))
    return _recalc(_mesh(name, verts, faces, mat, location))


def oval_ring(name, outer, inner, height, mat, location=(0.0, 0.0, 0.0), sides=16):
    """A band of oval section `height` tall, open in the middle: half-sizes
    `outer` = (x, y) outside and `inner` inside. A boat's rail."""
    (ox, oy), (ix, iy) = outer, inner
    verts = []
    for rx, ry, z in ((ox, oy, 0.0), (ox, oy, height), (ix, iy, height), (ix, iy, 0.0)):
        for k in range(sides):
            a = 2.0 * math.pi * k / sides
            verts.append((rx * math.cos(a), ry * math.sin(a), z))
    faces = []
    for ring in range(4):
        nxt = (ring + 1) % 4
        for k in range(sides):
            j = (k + 1) % sides
            faces.append((ring * sides + k, ring * sides + j, nxt * sides + j, nxt * sides + k))
    return _recalc(_mesh(name, verts, faces, mat, location))


def limb(name, length, r_top, r_bottom, mat, location=(0.0, 0.0, 0.0),
         rotation=(0.0, 0.0, 0.0)):
    """A limb hung from its joint at `location`: a tapered round length."""
    return cylinder(name, r_bottom, length, mat, location, sides=12, pivot="top",
                    rotation=rotation, top_radius=r_top)


def head_and_helmet(add, n, centre, helmet=None, helmet_mat="bronze", hair=True,
                    beard=False):
    """The head about `centre`: skin, a nose, two eyes, and hair, a helmet or
    both. `add(key, obj)` takes each piece onto the body. Helmets: "cap" (a
    dome with a rim), "crest" (a dome, cheek pieces and a crest of the
    owner's colour), "cone" (a point, a rim and a nose guard), "band" (a
    headband over the hair)."""
    x, y, z = centre
    hx, hy, hz = HEAD
    add("head", ellipsoid(n + "_head", HEAD, "skin", (x, y, z)))
    add("nose", box(n + "_nose", (0.022, 0.03, 0.03), "skin", (x, y + hy + 0.004, z - 0.03)))
    for side in (-1.0, 1.0):
        add("eye_%s" % ("l" if side < 0 else "r"),
            box(n + "_eye", (0.016, 0.01, 0.012), "opening",
                (x + side * 0.027, y + hy - 0.005, z + 0.005)))
    if beard:
        add("beard", ellipsoid(n + "_beard", (0.058, 0.04, 0.045), "hair",
                               (x, y + hy * 0.6, z - hz * 0.7)))
    if hair and helmet in (None, "band"):
        add("hair", ellipsoid(n + "_hair", (hx + 0.006, hy + 0.004, hz * 0.78), "hair",
                              (x, y - 0.012, z + 0.024)))
    top = z + hz * 0.25
    if helmet in ("cap", "crest"):
        add("helmet", ellipsoid(n + "_helmet", (hx + 0.011, hy + 0.011, hz * 0.8), helmet_mat,
                                (x, y, top), upper=True))
        add("helmet_rim", cylinder(n + "_rim", hx + 0.015, 0.016, helmet_mat,
                                   (x, y, top - 0.006), sides=14))
        if helmet == "crest":
            # A crescent of horsehair from brow to nape, seated on the dome.
            outer = [(0.1 * math.cos(math.pi * k / 10), 0.075 * math.sin(math.pi * k / 10))
                     for k in range(11)]
            inner = [(0.082 * math.cos(math.pi * k / 10), 0.035 * math.sin(math.pi * k / 10))
                     for k in range(10, -1, -1)]
            add("crest", profile_x(n + "_crest", outer + inner, 0.03, "player",
                                   (x, y - 0.006, top + hz * 0.45)))
            for side in (-1.0, 1.0):
                add("cheek_%s" % ("l" if side < 0 else "r"),
                    box(n + "_cheek", (0.014, 0.05, 0.065), helmet_mat,
                        (x + side * (hx + 0.006), y + 0.035, z - 0.05)))
    elif helmet == "cone":
        add("helmet", cylinder(n + "_rim", hx + 0.016, 0.02, helmet_mat,
                               (x, y, top - 0.006), sides=14))
        add("helmet_top", cone(n + "_cone", hx + 0.012, 0.13, helmet_mat, (x, y, top),
                               sides=14))
        add("nasal", box(n + "_nasal", (0.016, 0.012, 0.06), helmet_mat,
                         (x, y + hy + 0.01, z - 0.045)))
    elif helmet == "band":
        add("helmet", cylinder(n + "_band", hx + 0.008, 0.022, "linen",
                               (x, y - 0.004, z + 0.012), sides=14))


class Humanoid:
    """A figure 0.86 units tall, the villager's proportions, jointed.

    `dress` picks the clothes: "tunic" (a kilt and bare legs) or "robe" (a
    skirt to the ground, for the priest). Heads can carry a `helmet` ("cap",
    "crest", "cone", "band" or None). A `weapon` is held in the right hand
    and a `shield` on the left forearm. Parts are parented to `body`, one
    level below the root, so the root stays free for render_sheet.py's
    turntable; the shins hang from the thighs and the forearms from the
    upper arms, so a knee and an elbow bend.
    """

    # Which age's dress it wears past the Stone Age (`age_dress`).
    COSTUME = "soldier"

    def __init__(self, root_name, tunic="player", dress="tunic", helmet=None,
                 helmet_mat="bronze", hair=True, beard=False, legs="skin"):
        self.root = empty(root_name)
        self.body = empty(root_name + "_body", parent=self.root)
        self.parts = {}
        n = root_name
        add = self._add
        legs = legs if dress == "tunic" else tunic
        for side, x in (("l", -0.062), ("r", 0.062)):
            thigh = add("leg_" + side, limb(n + "_thigh_" + side, THIGH, 0.05, 0.04, legs,
                                            (x, 0.0, HIP_Z)))
            shin = add("shin_" + side, limb(n + "_shin_" + side, SHIN, 0.04, 0.03, legs,
                                            (0.0, 0.0, -THIGH)), parent=thigh)
            foot = box(n + "_foot_" + side, (0.068, 0.12, FOOT_H), "hide",
                       (0.0, 0.026, -SHIN - FOOT_H + 0.004))
            foot.parent = shin
        if dress == "robe":
            add("skirt", oval(n + "_skirt", (0.15, 0.11), (0.115, 0.08), HIP_Z + 0.05, tunic))
        else:
            add("kilt", oval(n + "_kilt", (0.135, 0.1), (0.11, 0.075), 0.18, tunic,
                             (0.0, 0.0, HIP_Z - 0.13)))
        # The trunk: narrow at the waist, broad at the chest, the shoulders
        # a rounded yoke across its top.
        add("torso", oval(n + "_torso", (0.1, 0.068), (0.128, 0.078),
                          SHOULDER_Z - 0.03 - (HIP_Z + 0.02), tunic,
                          (0.0, 0.0, HIP_Z + 0.02), lean=0.01))
        add("yoke", ellipsoid(n + "_yoke", (0.165, 0.08, 0.05), tunic,
                              (0.0, 0.01, SHOULDER_Z - 0.03)))
        add("waist", oval(n + "_belt", (0.104, 0.072), (0.104, 0.072), 0.03, "hide",
                          (0.0, 0.0, HIP_Z + 0.02)))
        add("neck", cylinder(n + "_neck", 0.036, 0.07, "skin", (0.0, 0.006, SHOULDER_Z - 0.005),
                             sides=12))
        head_and_helmet(add, n, (0.0, 0.012, HEAD_Z), helmet, helmet_mat, hair, beard)
        for side, x in (("l", -SHOULDER_X), ("r", SHOULDER_X)):
            upper = add("arm_" + side, limb(n + "_upper_" + side, UPPER_ARM, 0.038, 0.03,
                                            "skin", (x, 0.0, SHOULDER_Z - 0.005)))
            sleeve = limb(n + "_sleeve_" + side, 0.065, 0.047, 0.04, tunic)
            sleeve.parent = upper
            fore = add("fore_" + side, limb(n + "_fore_" + side, FOREARM, 0.031, 0.024, "skin",
                                            (0.0, 0.0, -UPPER_ARM)), parent=upper)
            grip = ellipsoid(n + "_hand_" + side, (0.027, 0.03, 0.034), "skin",
                             (0.0, 0.004, -FOREARM - 0.014))
            grip.parent = fore
        self.weapon = None
        self.weapon_hand = "right"
        self.weapon_lean = 0.0
        self.weapon_follows = False
        self.shield = None
        if STYLE_AGE:
            age_dress(self, STYLE_AGE, self.COSTUME)

    def _add(self, key, obj, parent=None):
        obj.parent = parent if parent is not None else self.body
        self.parts[key] = obj
        # A body is rounded: the ends of its limbs, its trunk; what it
        # wears keeps its shape, or a cone helmet's rim turns into a brim.
        if key in BODY_PARTS:
            soften(obj, 0.42, 3, limit=0.06)
        return obj

    def hold(self, weapon, hand="right", lean=22.0, follows=False):
        """`weapon` is an empty whose children are the weapon, gripped at its
        origin. It follows the `hand` and keeps its own angle, leaned `lean`
        degrees out from the body so it shows past the figure's outline
        when the figure faces the viewer; with `follows` it turns with the
        forearm too, as a club or an axe does in a blow."""
        weapon.parent = self.body
        self.weapon = weapon
        self.weapon_hand = hand
        self.weapon_lean = _deg(lean) * (1.0 if hand == "right" else -1.0)
        self.weapon_follows = follows

    def wear(self, key, obj):
        """Something that rides on the body, a quiver say, and moves with it."""
        return self._add(key, obj)

    def carry_shield(self, shield):
        """`shield` hangs on the left forearm and swings with it."""
        shield.parent = self.parts["fore_l"]
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
                    side = "l" if self.weapon_hand == "left" else "r"
                    arm, fore = limbs.get("arm_" + side, 0.0), limbs.get("fore_" + side, 0.0)
                    self.weapon.location = hand(arm, -1.0 if side == "l" else 1.0, fore)
                    turn = fore if self.weapon_follows else 0.0
                    self.weapon.rotation_euler = (weapon_angle + turn, self.weapon_lean, 0.0)
                    self.weapon.keyframe_insert("location", frame=frame)
                    self.weapon.keyframe_insert("rotation_euler", frame=frame)
        hold_frames([self.body, self.weapon] + list(self.parts.values()))


def hand(arm_angle, side, fore=0.0):
    """Where the right (side 1) or left (-1) hand grips, in body space, with
    the upper arm swung `arm_angle` about the shoulder and the forearm bent
    a further `fore` at the elbow."""
    x = SHOULDER_X * side
    ey = UPPER_ARM * math.sin(arm_angle)
    ez = SHOULDER_Z - 0.005 - UPPER_ARM * math.cos(arm_angle)
    reach = FOREARM + 0.014
    a = arm_angle + fore
    return (x, ey + reach * math.sin(a), ez - reach * math.cos(a))


def _deg(d):
    return math.radians(d)


def humanoid_pose(anim, i, count, style):
    """One frame: the body's bob and pitch, each limb's swing, the weapon's
    angle. Positive swing brings a limb forward (+Y); a positive elbow
    brings the forearm up in front, a negative knee the shin back. At 70 px
    the silhouette is most of the performance, so the motion is broad."""
    body = {"dy": 0.0, "dz": 0.0, "rot_x": 0.0, "scale_z": 1.0}
    limbs = {"leg_l": 0.0, "leg_r": 0.0, "arm_l": 0.0, "arm_r": 0.0,
             "fore_l": _deg(12.0), "fore_r": _deg(18.0)}
    # The weapon's angle from upright: 0 holds it point up, -90 points it
    # forward.
    carry = {"thrust": _deg(-12.0), "swing": _deg(-20.0), "shoot": 0.0,
             "sling": 0.0, "bless": 0.0}.get(style, 0.0)
    weapon = carry

    if anim == "idle":
        body["dz"] = 0.006 * math.sin(2.0 * math.pi * i / count)
        limbs["arm_l"] = _deg(4.0)
        limbs["arm_r"] = _deg(10.0)
        limbs["fore_r"] = _deg(30.0)

    elif anim == "walk":
        phase = 2.0 * math.pi * i / count
        limbs["leg_l"] = _deg(28.0) * math.sin(phase)
        limbs["leg_r"] = _deg(28.0) * math.sin(phase + math.pi)
        # The knee folds as the leg comes through, lifting the foot.
        limbs["shin_l"] = _deg(-42.0) * max(0.0, math.cos(phase))
        limbs["shin_r"] = _deg(-42.0) * max(0.0, math.cos(phase + math.pi))
        limbs["arm_l"] = _deg(18.0) * math.sin(phase + math.pi)
        limbs["arm_r"] = _deg(10.0) + _deg(8.0) * math.sin(phase)
        limbs["fore_l"] = _deg(20.0) + _deg(10.0) * max(0.0, math.sin(phase + math.pi))
        limbs["fore_r"] = _deg(30.0)
        body["dz"] = 0.018 * abs(math.sin(phase))

    elif anim == "attack":
        # The blow lands on index 3, the manifest's impact frame.
        if style == "thrust":
            keys = [20.0, 35.0, 10.0, 80.0, 70.0, 40.0]
            elbows = [70.0, 80.0, 90.0, 5.0, 15.0, 45.0]
            limbs["arm_r"] = _deg(keys[i])
            limbs["fore_r"] = _deg(elbows[i])
            limbs["arm_l"] = _deg(keys[i] * 0.6)
            limbs["fore_l"] = _deg(40.0)
            weapon = _deg(-90.0) if i >= 2 else _deg(-60.0)
            body["rot_x"] = _deg(8.0) if i == 3 else 0.0
            limbs["leg_l"] = _deg(18.0)
            limbs["leg_r"] = _deg(-12.0)
            limbs["shin_r"] = _deg(-10.0)
        elif style == "shoot":
            # The bow out on a straight left arm, the right hand drawing the
            # string back to the cheek, loosed on the impact frame.
            draw = [60.0, 80.0, 85.0, 85.0, 80.0, 60.0]
            limbs["arm_l"] = _deg(draw[i])
            limbs["fore_l"] = _deg(5.0)
            limbs["arm_r"] = _deg(draw[i] - (20.0 if i < 3 else 5.0))
            limbs["fore_r"] = _deg(95.0 if i < 3 else 30.0)
        elif style == "sling":
            keys = [-40.0, -120.0, -200.0, -280.0, -330.0, -360.0]
            limbs["arm_r"] = _deg(keys[i])
            limbs["fore_r"] = _deg(10.0)
            limbs["arm_l"] = _deg(20.0)
            weapon = _deg(keys[i])
        elif style == "bless":
            lift = [30.0, 70.0, 110.0, 140.0, 120.0, 60.0]
            limbs["arm_l"] = _deg(lift[i])
            limbs["arm_r"] = _deg(lift[i])
            limbs["fore_l"] = limbs["fore_r"] = _deg(20.0)
        else:
            # Wound back over the shoulder with the elbow folded, then the
            # arm comes over and straightens into the blow.
            keys = [-55.0, -70.0, -20.0, 75.0, 60.0, 30.0]
            elbows = [60.0, 75.0, 50.0, 5.0, 15.0, 30.0]
            limbs["arm_r"] = _deg(keys[i])
            limbs["fore_r"] = _deg(elbows[i])
            limbs["arm_l"] = _deg(-keys[i] * 0.25)
            limbs["fore_l"] = _deg(35.0)
            weapon = _deg(keys[i]) + carry
            body["rot_x"] = _deg(-keys[i] * 0.12)
            limbs["leg_l"] = _deg(12.0)
            limbs["leg_r"] = _deg(-10.0)
            limbs["shin_r"] = _deg(-12.0)

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
        limbs["fore_l"] = _deg(12.0) + _deg(30.0) * t
        limbs["fore_r"] = _deg(18.0) - _deg(10.0) * t
        limbs["leg_l"] = _deg(18.0) * t
        limbs["leg_r"] = _deg(-12.0) * t
        limbs["shin_l"] = _deg(-25.0) * t
        weapon = carry + _deg(-70.0) * t
        if anim == "decay":
            s = (i + 1) / float(count)
            body["dz"] = -0.05 - 0.02 * s
            body["scale_z"] = 1.0 - 0.18 * s

    return body, limbs, weapon


# --------------------------------------------------------------------------
# Weapons, each gripped at its empty's origin and standing point up.

def blade(name, width, length, thick, mat, location=(0.0, 0.0, 0.0), widest=0.35):
    """A leaf-shaped blade standing on `location`: widest `widest` of the
    way up, coming to a point, a ridge down each face."""
    w, t, h = width / 2.0, thick / 2.0, length
    m = h * widest
    verts = [(0.0, 0.0, 0.0), (w, 0.0, m), (0.0, t, m), (-w, 0.0, m), (0.0, -t, m),
             (0.0, 0.0, h)]
    faces = [(0, 2, 1), (0, 3, 2), (0, 4, 3), (0, 1, 4),
             (1, 2, 5), (2, 3, 5), (3, 4, 5), (4, 1, 5)]
    return _recalc(_mesh(name, verts, faces, mat, location))


def profile_x(name, points, thick, mat, location=(0.0, 0.0, 0.0)):
    """A flat shape drawn in y and z, `points` round its outline, `thick`
    across in x: an axe's blade, a crest."""
    n = len(points)
    t = thick / 2.0
    verts = [(-t, y, z) for y, z in points] + [(t, y, z) for y, z in points]
    faces = [tuple(range(n)), tuple(range(2 * n - 1, n - 1, -1))]
    for i in range(n):
        j = (i + 1) % n
        faces.append((i, j, n + j, n + i))
    return _recalc(_mesh(name, verts, faces, mat, location))


def _grip(name, parts):
    grip = empty(name)
    for part in parts:
        part.parent = grip
    return grip


def spear(name, metal="bronze", length=1.0):
    return _grip(name, [
        cylinder(name + "_shaft", 0.017, length, "wood", (0.0, 0.0, -0.32), sides=8),
        blade(name + "_blade", 0.055, 0.17, 0.016, metal, (0.0, 0.0, length - 0.34)),
        cylinder(name + "_butt", 0.004, 0.06, metal, (0.0, 0.0, -0.38), sides=8,
                 top_radius=0.019),
    ])


def club(name, head="wood_dark"):
    return _grip(name, [
        cylinder(name + "_handle", 0.02, 0.32, "wood", (0.0, 0.0, -0.06), sides=8,
                 top_radius=0.026),
        clump(name + "_head", 0.06, head, (0.0, 0.0, 0.28), seed=3, lumps=0.22, squash=1.4),
    ])


def axe(name, metal="bronze"):
    edge = [(0.0, 0.12), (0.05, 0.11), (0.12, 0.07), (0.16, 0.13), (0.17, 0.2),
            (0.16, 0.27), (0.12, 0.31), (0.05, 0.25), (0.0, 0.24)]
    return _grip(name, [
        cylinder(name + "_handle", 0.018, 0.44, "wood", (0.0, 0.0, -0.1), sides=8),
        profile_x(name + "_blade", edge, 0.018, metal),
        cylinder(name + "_socket", 0.028, 0.13, metal, (0.0, 0.0, 0.115), sides=8),
    ])


def sword(name, metal="bronze"):
    return _grip(name, [
        cylinder(name + "_hilt", 0.016, 0.1, "wood_dark", (0.0, 0.0, -0.06), sides=8),
        ellipsoid(name + "_pommel", (0.024, 0.024, 0.02), metal, (0.0, 0.0, -0.065)),
        box(name + "_guard", (0.1, 0.03, 0.022), metal, (0.0, 0.0, 0.035)),
        blade(name + "_blade", 0.06, 0.4, 0.012, metal, (0.0, 0.0, 0.055), widest=0.6),
    ])


def rod(name, a, b, radius, mat, sides=6, top_radius=None):
    """A round rod from `a` to `b`, both in the y-z plane (x = a[0])."""
    dy, dz = b[1] - a[1], b[2] - a[2]
    length = math.hypot(dy, dz)
    angle = math.atan2(-dy, dz)
    return cylinder(name, radius, length, mat, a, sides=sides,
                    rotation=(angle, 0.0, 0.0), top_radius=top_radius)


def beam(name, a, b, radius, mat, sides=6):
    """A round rod from `a` to `b`, pointing any way: what `rod` is for
    rods across x as well as y and z."""
    dx, dy, dz = b[0] - a[0], b[1] - a[1], b[2] - a[2]
    length = math.sqrt(dx * dx + dy * dy + dz * dz)
    tilt = math.acos(max(-1.0, min(1.0, dz / length)))
    turn = math.atan2(dy, dx)
    return cylinder(name, radius, length, mat, a, sides=sides, rotation=(0.0, tilt, turn))


def bow(name):
    """A recurve bow: the limbs curving back from the grip toward the
    archer and their tips flicking forward again, the string straight
    between the tips."""
    curve = [(0.03, 0.0), (0.022, 0.1), (0.0, 0.2), (-0.018, 0.28), (-0.01, 0.335)]
    parts = []
    for sign, tag in ((1.0, "u"), (-1.0, "d")):
        pts = [(0.0, y, z * sign) for y, z in curve]
        for k in range(len(pts) - 1):
            r = 0.013 if k < 2 else 0.009
            parts.append(rod("%s_limb%d%s" % (name, k, tag), pts[k], pts[k + 1], r, "wood"))
    tip = curve[-1]
    parts.append(rod(name + "_string", (0.0, tip[0], -tip[1]), (0.0, tip[0], tip[1]), 0.0035,
                     "linen", sides=4))
    parts.append(cylinder(name + "_grip", 0.017, 0.08, "hide", (0.0, 0.03, -0.04), sides=8))
    return _grip(name, parts)


def sling(name):
    return _grip(name, [
        cylinder(name + "_cord", 0.007, 0.24, "rope", (0.0, 0.0, -0.24), sides=5),
        ellipsoid(name + "_pouch", (0.035, 0.035, 0.03), "hide", (0.0, 0.0, -0.27)),
    ])


def staff(name):
    return _grip(name, [
        cylinder(name + "_shaft", 0.018, 0.84, "wood", (0.0, 0.0, -0.32), sides=8),
        ellipsoid(name + "_top", (0.045, 0.045, 0.05), "bronze", (0.0, 0.0, 0.55)),
    ])


def quiver(name):
    """A quiver on the back, its arrows showing over the right shoulder."""
    root = empty(name)
    tilt = (0.0, _deg(-18.0), 0.0)
    case = cylinder(name + "_case", 0.036, 0.30, "hide", (0.07, -0.11, 0.42), sides=10,
                    rotation=tilt, top_radius=0.04)
    case.parent = root
    for k, dx in enumerate((-0.015, 0.0, 0.015)):
        shaft = cylinder("%s_arrow%d" % (name, k), 0.005, 0.12, "wood",
                         (0.165 + dx, -0.11 + dx, 0.70), sides=4, rotation=tilt)
        fletch = box("%s_fletch%d" % (name, k), (0.012, 0.03, 0.05), "white",
                     (0.19 + dx, -0.11 + dx, 0.79), rotation=tilt)
        shaft.parent = fletch.parent = root
    return root


def round_shield(name, face="player", radius=0.13, rim="bronze"):
    """A round shield on the forearm, seen edge-on from the side, face-on
    from the front: a face, a rim of metal or hide, a boss."""
    root = empty(name, location=(-0.05, 0.02, -0.06))
    side = (0.0, _deg(90.0), 0.0)
    disc = cylinder(name + "_disc", radius, 0.022, face, (-0.004, 0.0, 0.0), sides=18,
                    pivot="centre", rotation=side)
    ring = cylinder(name + "_rim", radius + 0.012, 0.016, rim, (0.006, 0.0, 0.0), sides=18,
                    pivot="centre", rotation=side)
    boss = ellipsoid(name + "_boss", (0.022, radius * 0.28, radius * 0.28), rim,
                     (-0.018, 0.0, 0.0))
    for part in (disc, ring, boss):
        part.parent = root
    return root


def tower_shield(name, face="player"):
    """The legionary's tall curved shield: a face of the owner's colour,
    metal edging and a boss."""
    root = empty(name, location=(-0.06, 0.03, -0.02))
    w, h = 0.21, 0.36
    parts = []
    for k, y in enumerate((-w / 3.0, 0.0, w / 3.0)):
        bow_out = -0.02 if k != 1 else -0.03
        parts.append(box("%s_panel%d" % (name, k), (0.02, w / 3.0 + 0.004, h), face,
                         (bow_out, y, -h / 2.0)))
    for z in (-h / 2.0, h / 2.0):
        parts.append(box(name + "_edge", (0.026, w + 0.01, 0.016), "iron", (-0.028, 0.0, z - 0.008)))
    parts.append(ellipsoid(name + "_boss", (0.025, 0.045, 0.045), "iron", (-0.045, 0.0, 0.0)))
    parts.append(box(name + "_band", (0.024, 0.02, h * 0.8), "gold", (-0.04, 0.0, -h * 0.4)))
    for part in parts:
        part.parent = root
    return root


# --------------------------------------------------------------------------
# Buildings.

class Building:
    """A building as five frames: construction stages 1-3, the finished
    building (4), its rubble (5), and for walls and gates the pieces after
    them (WALL_SPANS, GATE_SPANS). Each object is tagged with the frames it
    appears on and keyed visible there and hidden elsewhere, so one file
    renders them all. The footprint is `tiles` on a side, centred on the
    origin. The camera sits out at +X +Y, so the +X and +Y faces and the top
    are what shows: doors, banners and player colour go there."""

    def __init__(self, root_name, tiles, frames=5):
        self.root = empty(root_name)
        self.tiles = tiles
        self.frames = frames
        self.shown = []

    def add(self, obj, frames):
        obj.parent = self.root
        self.shown.append((obj, set(frames)))
        return obj

    # Whether Building.finish restyles it for its architecture: a Wonder of
    # another architecture is its own model.
    restyle = True

    def finish(self):
        if STYLE_ARCH != "greek":
            if self.restyle:
                style_architecture(self, STYLE_ARCH, STYLE_AGE)
        elif STYLE_AGE:
            style_building(self, STYLE_AGE)
        scene = bpy.context.scene
        scene.frame_start, scene.frame_end = 1, self.frames
        for obj, frames in self.shown:
            for frame in range(1, self.frames + 1):
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
STAGES = (1, 2, 3)
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


def walls(b, name, w, d, h, mat, door=True, windows=True):
    """A walled block with its full height on the finished frames and half
    its height on the second stage, on a stone plinth; a framed door on the
    +Y face and small windows on the two faces the camera sees (it sits out
    at +X +Y)."""
    b.add(box(name + "_walls_half", (w, d, h * 0.5), mat), HALF_WALLS)
    b.add(box(name + "_walls", (w, d, h), mat), FULL_WALLS)
    b.add(box(name + "_plinth", (w + 0.05, d + 0.05, min(0.08, h * 0.14)), "stone"), WALLS)
    if door:
        dw, dh, y = w * 0.22, h * 0.62, d / 2
        b.add(box(name + "_door", (dw, 0.02, dh), "wood_dark", (0.0, y + 0.005, 0.0)),
              FULL_WALLS)
        for side in (-1.0, 1.0):
            b.add(box("%s_jamb%d" % (name, side > 0), (0.035, 0.04, dh), "wood",
                      (side * (dw / 2 + 0.0175), y + 0.01, 0.0)), FULL_WALLS)
        b.add(box(name + "_lintel", (dw + 0.14, 0.05, 0.045), "wood", (0.0, y + 0.015, dh)),
              FULL_WALLS)
        b.add(box(name + "_step", (dw + 0.08, 0.08, 0.025), "stone", (0.0, y + 0.04, 0.0)),
              FULL_WALLS)
    if windows:
        z = h * 0.42
        along_x = [-w * 0.33, w * 0.33] if door else [0.0]
        along_y = [-d * 0.25, d * 0.25] if d >= 0.9 else [0.0]
        for i, x in enumerate(along_x if w >= 0.9 else []):
            window(b, "%s_win_y%d" % (name, i), (x, d / 2, z), "y")
        for i, y in enumerate(along_y):
            window(b, "%s_win_x%d" % (name, i), (w / 2, y, z), "x")
    b.add(box(name + "_foundation", (w + 0.08, d + 0.08, 0.04), "slab"), FOUNDATION)


def window(b, name, at, face, frames=FULL_WALLS):
    """A small window in the wall face at `at`, facing +X or +Y: the dark
    of the room behind, a lintel above and a sill below."""
    x, y, z = at
    def sized(along, out, tall):
        return (out, along, tall) if face == "x" else (along, out, tall)
    off = (0.006, 0.0) if face == "x" else (0.0, 0.006)
    b.add(box(name, sized(0.1, 0.02, 0.09), "opening", (x + off[0], y + off[1], z)), frames)
    b.add(box(name + "_lintel", sized(0.15, 0.035, 0.025), "wood",
              (x + 2 * off[0], y + 2 * off[1], z + 0.09)), frames)
    b.add(box(name + "_sill", sized(0.14, 0.04, 0.018), "wood",
              (x + 2 * off[0], y + 2 * off[1], z - 0.018)), frames)


def thatch_roof(b, name, size, location, frames=FINISHED):
    """A thatched roof in two courses over a thick eave, with a knot of
    straw at the top: the edge of a real thatch is a hand's breadth deep,
    and a single pyramid reads as a lid."""
    w, d, h = size
    x, y, z = location
    root2 = math.sqrt(2.0)
    b.add(cylinder(name + "_eave", (max(w, d) + 0.04) / root2, 0.07, "thatch", (x, y, z - 0.05),
                   sides=4, top_radius=(max(w, d) - 0.06) / root2,
                   rotation=(0.0, 0.0, math.radians(45.0))), frames)
    b.add(pyramid(name, (w, d, h), "thatch", (x, y, z)), frames)
    b.add(pyramid(name + "_cap", (w * 0.5, d * 0.5, h * 0.52), "thatch", (x, y, z + h * 0.5)),
          frames)
    b.add(cylinder(name + "_knot", 0.03, 0.06, "wood_dark", (x, y, z + h - 0.01), sides=6),
          frames)


def ridge(b, name, length, location, mat, axis="x", frames=FINISHED, radius=0.035):
    """A roll along a gable's ridge: bound straw, or ridge tiles."""
    rot = (0.0, math.radians(90.0), 0.0) if axis == "x" else (math.radians(90.0), 0.0, 0.0)
    b.add(cylinder(name, radius, length, mat, location, sides=8, pivot="centre", rotation=rot),
          frames)


def vigas(b, name, w, d, z, frames=FINISHED):
    """Roof beams whose ends stand out of the two walls the camera sees."""
    for i, t in enumerate([-0.35, -0.12, 0.12, 0.35]):
        b.add(cylinder("%s_y%d" % (name, i), 0.025, 0.12, "wood", (t * w, d / 2 + 0.03, z),
                       sides=6, pivot="centre", rotation=(math.radians(90.0), 0.0, 0.0)), frames)
        b.add(cylinder("%s_x%d" % (name, i), 0.025, 0.12, "wood", (w / 2 + 0.03, t * d, z),
                       sides=6, pivot="centre", rotation=(0.0, math.radians(90.0), 0.0)), frames)


def woodpile(b, name, at, frames=FINISHED):
    """A stack of split logs against a wall."""
    x, y = at
    for i, (dx, dz) in enumerate([(-0.05, 0.0), (0.05, 0.0), (0.0, 0.07)]):
        b.add(cylinder("%s_%d" % (name, i), 0.035, 0.26, "wood", (x + dx, y, 0.035 + dz),
                       sides=6, pivot="centre", rotation=(math.radians(90.0), 0.0, 0.0)), frames)


def jar(b, name, at, frames=FINISHED, h=0.2):
    """A clay storage jar."""
    x, y = at
    b.add(cylinder(name, 0.06, h, "clay_roof", (x, y, 0.0), sides=10, top_radius=0.045), frames)


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


def prism(name, plan, height, mat, z=0.0):
    """A solid standing on the polygon `plan` (XY points, counter-clockwise,
    concave allowed), `height` tall from `z`."""
    n = len(plan)
    verts = [(x, y, z) for x, y in plan] + [(x, y, z + height) for x, y in plan]
    faces = [tuple(reversed(range(n))), tuple(range(n, 2 * n))]
    faces += [(i, (i + 1) % n, n + (i + 1) % n, n + i) for i in range(n)]
    return _mesh(name, verts, faces, mat)


# --------------------------------------------------------------------------
# Walls and gates.
#
# A wall is laid a tile at a time, and the game draws each tile as its post
# plus an arm toward each wall of the same owner beside it
# (`crates/view/src/scene.rs`). The arms are frames of their own, one per
# direction, in this order of the game's tile offsets. The game's +x is
# Blender's +Y and its +y is Blender's +X (the camera's right is (-1, 1, 0),
# rig.json), so a direction (dx, dy) points along Blender (dy, dx).
WALL_DIRECTIONS = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)]


def wall_direction(k):
    """Direction `k` as a unit vector in Blender's XY, its angle, and how
    far an arm reaches along it: the tile's edge, or its corner for a
    diagonal, where the neighbour's arm meets it."""
    dx, dy = WALL_DIRECTIONS[k]
    x, y = float(dy), float(dx)
    n = math.hypot(x, y)
    return (x / n, y / n), math.atan2(y, x), 0.5 * n


def arm_frames(k):
    """The one frame arm `k` shows on."""
    return (WALL_SPANS["arm"][0] + k,)


def along(unit, s, v=0.0):
    """The point `s` along `unit` and `v` to its left."""
    ux, uy = unit
    return (ux * s - uy * v, uy * s + ux * v)


def arm_plan(k, pier, width):
    """The plan of a solid arm `width` wide from a square pier of half-size
    `pier` to the edge of the tile. A diagonal arm leaves by the pier's
    corner, so its inner end follows the pier's two faces: the arms are
    drawn as sprites apart from the pier, and an end cut square would
    either leave a gap beside the corner or poke out through it."""
    unit, _, reach = wall_direction(k)
    h = width / 2.0
    dx, dy = WALL_DIRECTIONS[k]
    if dx and dy:
        c = pier * math.sqrt(2.0)
        pts = [(c - h, -h), (reach, -h), (reach, h), (c - h, h), (c, 0.0)]
    else:
        pts = [(pier, -h), (reach, -h), (reach, h), (pier, h)]
    return [along(unit, s, v) for s, v in pts]


def gate_orientation(o):
    """Orientation `o` (0-3) of a gate: the direction of its wall's line,
    WALL_DIRECTIONS[o], as a unit vector, its angle and the run's length
    across the tile."""
    unit, angle, reach = wall_direction(o)
    return unit, angle, 2.0 * reach


# --------------------------------------------------------------------------
# The ages. A settlement and its people change as their owner advances
# (`docs/02` section 4): the same buildings in the materials of each age,
# the same figures in each age's dress. `STYLE_AGE` is the age being built,
# 0 to 3 for Stone to Iron; slice.py sets it for an aged subject
# (`house_bronze`), and Building.finish and Humanoid apply it.

STYLE_AGE = 0

# What each age builds in, by the Stone Age material it replaces.
AGE_MATERIALS = {
    1: {"thatch": "shingle"},
    2: {"thatch": "clay_roof", "mudbrick": "plaster"},
    3: {"thatch": "slate", "clay_roof": "slate", "mudbrick": "stone_light",
        "plaster": "stone_light"},
}


def style_building(b, age):
    """Rebuilds `b`'s materials and adds its age's work to every walled
    block in it: a timber frame in the Tool Age, a stone base course in the
    Bronze, a cornice and pilasters of dressed stone in the Iron."""
    swaps = AGE_MATERIALS.get(age, {})
    for obj in b.root.children_recursive:
        if obj.type != "MESH" or not obj.data.materials:
            continue
        name = obj.data.materials[0].name
        if name in swaps:
            obj.data.materials[0] = material(swaps[name])
    walls = [(obj, frames) for obj, frames in b.shown
             if obj.name.endswith("_walls") and obj.type == "MESH"]
    for obj, frames in walls:
        xs = [v.co for v in obj.data.vertices]
        w = max(c.x for c in xs) - min(c.x for c in xs)
        d = max(c.y for c in xs) - min(c.y for c in xs)
        h = max(c.z for c in xs) - min(c.z for c in xs)
        cx, cy, z0 = obj.location.x, obj.location.y, obj.location.z
        name = obj.name
        shown = tuple(frames)
        if age == 1:
            timber_frame(b, name, (cx, cy, z0), w, d, h, shown)
        elif age == 2:
            b.add(box(name + "_course", (w + 0.03, d + 0.03, h * 0.22), "stone_light",
                      (cx, cy, z0)), shown)
        elif age == 3:
            b.add(box(name + "_cornice", (w + 0.07, d + 0.07, 0.05), "white",
                      (cx, cy, z0 + h - 0.03)), shown)
            for sx in (-1.0, 1.0):
                for sy in (-1.0, 1.0):
                    b.add(box("%s_pilaster_%d%d" % (name, sx > 0, sy > 0), (0.07, 0.07, h),
                              "white", (cx + sx * w / 2, cy + sy * d / 2, z0)), shown)


def timber_frame(b, name, at, w, d, h, frames):
    """Posts and rails of dark timber standing proud of the two wall faces
    the camera sees, clear of the door and the windows."""
    cx, cy, z0 = at
    beam = 0.035
    for i, t in enumerate([-0.5, -0.17, 0.17, 0.5]):
        b.add(box("%s_post_y%d" % (name, i), (beam, 0.02, h), "wood_dark",
                  (cx + t * w, cy + d / 2 + 0.008, z0)), frames)
    for i, t in enumerate([-0.5, 0.0, 0.5]):
        b.add(box("%s_post_x%d" % (name, i), (0.02, beam, h), "wood_dark",
                  (cx + w / 2 + 0.008, cy + t * d, z0)), frames)
    for i, z in enumerate([h * 0.62, h - beam]):
        b.add(box("%s_rail_y%d" % (name, i), (w, 0.02, beam), "wood_dark",
                  (cx, cy + d / 2 + 0.01, z0 + z)), frames)
        b.add(box("%s_rail_x%d" % (name, i), (0.02, d, beam), "wood_dark",
                  (cx + w / 2 + 0.01, cy, z0 + z)), frames)


# --------------------------------------------------------------------------
# The architectures (`docs/02` section 11, `docs/07` D34). The sets as first
# built are the Greek; the Egyptian, the Mesopotamian and the East Asian are
# the same buildings restyled in Building.finish, as the ages restyle the
# Greek: each its own materials in each age, its own roofs in place of the
# thatch and the gables, and its own work on the walls. What stood on a
# roof is set on the new one. `STYLE_ARCH` is the architecture being built;
# slice.py sets it for `house_egyptian_bronze`.

STYLE_ARCH = "greek"

# What each architecture builds in, age by age, by the Greek Stone Age
# material it replaces.
ARCH_MATERIALS = {
    "egyptian": {
        0: {"thatch": "straw"},
        1: {"thatch": "straw", "mudbrick": "plaster"},
        2: {"thatch": "straw", "mudbrick": "sandstone", "plaster": "sandstone",
            "stone_light": "sandstone", "white": "limestone", "clay_roof": "sandstone"},
        3: {"thatch": "straw", "mudbrick": "limestone", "plaster": "limestone",
            "stone_light": "sandstone", "white": "limestone", "clay_roof": "sandstone",
            "slate": "sandstone"},
    },
    "mesopotamian": {
        0: {"thatch": "straw"},
        1: {"thatch": "straw", "plaster": "mudbrick"},
        2: {"thatch": "straw", "mudbrick": "baked_brick", "plaster": "baked_brick",
            "stone_light": "baked_brick", "white": "glaze", "clay_roof": "baked_brick"},
        3: {"thatch": "straw", "mudbrick": "baked_brick", "plaster": "baked_brick",
            "stone_light": "baked_brick", "white": "glaze", "clay_roof": "baked_brick",
            "slate": "baked_brick"},
    },
    "asian": {
        0: {"mudbrick": "rammed"},
        1: {"mudbrick": "rammed", "plaster": "rammed"},
        2: {"mudbrick": "plaster", "stone_light": "stone", "white": "lacquer",
            "clay_roof": "tile_dark"},
        3: {"mudbrick": "plaster", "stone_light": "stone", "white": "lacquer",
            "clay_roof": "tile_dark", "slate": "tile_dark"},
    },
}

# The walled blocks an architecture dresses, by the end of their names.
BLOCKS = ("_walls", "_upper", "_cella", "_hut", "tower_body")


def frustum(name, bottom, top, height, mat, location=(0.0, 0.0, 0.0)):
    """A box narrowing (or flaring) from `bottom` = (w, d) at its foot to
    `top` at its head, `height` tall: a battered wall, a cornice."""
    (bw, bd), (tw, td) = bottom, top
    verts = [(-bw / 2, -bd / 2, 0.0), (bw / 2, -bd / 2, 0.0), (bw / 2, bd / 2, 0.0),
             (-bw / 2, bd / 2, 0.0), (-tw / 2, -td / 2, height), (tw / 2, -td / 2, height),
             (tw / 2, td / 2, height), (-tw / 2, td / 2, height)]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return _mesh(name, verts, faces, mat, location)


def rim(name, inner, outer_bottom, outer_top, height, mat, location=(0.0, 0.0, 0.0)):
    """A rectangular band round an opening `inner` = (w, d), its outside
    `outer_bottom` at its foot and `outer_top` at its head: a cornice that
    flares out over a wall, open over the roof inside it."""
    loops = [(outer_bottom, 0.0), (outer_top, height), (inner, height), (inner, 0.0)]
    verts = []
    for (w, d), z in loops:
        verts += [(-w / 2, -d / 2, z), (w / 2, -d / 2, z), (w / 2, d / 2, z), (-w / 2, d / 2, z)]
    faces = []
    for ring in range(4):
        nxt = (ring + 1) % 4
        for k in range(4):
            j = (k + 1) % 4
            faces.append((ring * 4 + k, ring * 4 + j, nxt * 4 + j, nxt * 4 + k))
    return _recalc(_mesh(name, verts, faces, mat, location))


def hip_roof(name, w, d, h, mat, location, flare=0.05):
    """A hipped roof over `w` by `d`, `h` high, its ridge along the longer
    side and its corners turned up by `flare` over eaves that sag between
    them. Returns the roof and its height above its foot at a point."""
    x, y, z = location
    long_x = w >= d
    half_long, half_short = (w, d) if long_x else (d, w)
    half_long, half_short = half_long / 2.0, half_short / 2.0
    r = max(half_long - half_short, 0.002)
    # The eaves round from the -x -y corner, a corner then a middle.
    ring = [(-w / 2, -d / 2, flare), (0.0, -d / 2, 0.0), (w / 2, -d / 2, flare), (w / 2, 0.0, 0.0),
            (w / 2, d / 2, flare), (0.0, d / 2, 0.0), (-w / 2, d / 2, flare), (-w / 2, 0.0, 0.0)]
    ridge = [(-r, 0.0, h), (r, 0.0, h)] if long_x else [(0.0, -r, h), (0.0, r, h)]
    verts = ring + ridge
    a, b2 = 8, 9
    if long_x:
        faces = [(0, 1, a), (1, 2, b2), (1, b2, a),        # -y slope
                 (4, 5, b2), (5, 6, a), (5, a, b2),        # +y slope
                 (2, 3, b2), (3, 4, b2),                   # +x hip
                 (6, 7, a), (7, 0, a)]                     # -x hip
    else:
        faces = [(2, 3, a), (3, 4, b2), (3, b2, a),        # +x slope
                 (6, 7, b2), (7, 0, a), (7, a, b2),        # -x slope
                 (4, 5, b2), (5, 6, b2),                   # +y hip
                 (0, 1, a), (1, 2, a)]                     # -y hip
    faces.append(tuple(reversed(range(8))))
    roof = _recalc(_mesh(name, verts, faces, mat, location))

    def height_at(px, py):
        u, v = abs(px - x), abs(py - y)
        along, across = (u, v) if long_x else (v, u)
        t = min(1.0 - across / half_short, 1.0 - max(0.0, along - r) / max(half_long - r, 1e-6))
        return z + h * max(0.0, min(1.0, t))
    return roof, height_at


def _bounds(objs):
    """World-space bounds of `objs`, by their vertices: (x0, x1, y0, y1, z0,
    z1). (A turned object's bound_box is its own axes' box, turned.)"""
    pts = [o.matrix_world @ v.co for o in objs for v in o.data.vertices]
    return (min(p.x for p in pts), max(p.x for p in pts), min(p.y for p in pts),
            max(p.y for p in pts), min(p.z for p in pts), max(p.z for p in pts))


def style_architecture(b, arch, age):
    """Rebuilds `b` in `arch` as of `age`: its materials, its roofs and the
    work on its walls and columns."""
    swaps = ARCH_MATERIALS[arch].get(age, {})
    for obj in b.root.children_recursive:
        if obj.type != "MESH" or not obj.data.materials:
            continue
        name = obj.data.materials[0].name
        if name in swaps:
            obj.data.materials[0] = material(swaps[name])
    bpy.context.view_layer.update()
    _restyle_roofs(b, arch, age)
    for obj, frames in list(b.shown):
        if obj.type == "MESH" and obj.name.endswith(BLOCKS):
            _dress_block(b, obj, tuple(frames), arch, age)
        elif obj.type == "MESH" and "_col_" in obj.name:
            _dress_column(b, obj, tuple(frames), arch, age)


def _restyle_roofs(b, arch, age):
    """Takes every roof off `b` (a part named for a roof, a ridge, an East
    Asian building's parapet) and builds the architecture's in its place;
    what stood on the old roof stands on the new."""
    groups = {}
    for obj, frames in b.shown:
        n = obj.name
        if "_roof" in n:
            key = n.split("_roof")[0]
        elif n.endswith("_ridge"):
            key = n[:-len("_ridge")]
        else:
            continue
        groups.setdefault(key, []).append((obj, frames))
    for key, parts in groups.items():
        objs = [o for o, _ in parts]
        frames = set().union(*[f for _, f in parts])
        x0, x1, y0, y1, z0, z1 = _bounds(objs)
        names = {o.name for o in objs}
        b.shown = [(o, f) for o, f in b.shown if o.name not in names]
        for o in objs:
            bpy.data.objects.remove(o, do_unlink=True)
        if arch == "asian":
            # A hipped roof comes down over the walls' tops.
            for o, f in list(b.shown):
                if "parapet" in o.name:
                    b.shown.remove((o, f))
                    bpy.data.objects.remove(o, do_unlink=True)
        # What stood on it: wholly above its foot and within its eaves.
        riders = []
        for o, f in b.shown:
            if o.type not in ("MESH",) or not (set(f) & frames):
                continue
            ox0, ox1, oy0, oy1, oz0, _ = _bounds([o])
            cx, cy = (ox0 + ox1) / 2, (oy0 + oy1) / 2
            if (oz0 >= z0 + 0.05 and x0 <= cx <= x1 and y0 <= cy <= y1
                    and "parapet" not in o.name):
                riders.append((o, cx, cy, oz0))
        height_at = _arch_roof(b, key, (x0, x1, y0, y1, z0, z1), tuple(sorted(frames)), arch,
                               age, [(cx, cy) for _, cx, cy, _ in riders])
        # Riders move together by the name they share (a flag's pole and
        # its cloth), set on the roof under the lowest of them.
        by_name = {}
        for o, cx, cy, oz0 in riders:
            by_name.setdefault(o.name.rsplit("_", 1)[0], []).append((o, cx, cy, oz0))
        for group in by_name.values():
            _, cx, cy, low = min(group, key=lambda g: g[3])
            dz = height_at(cx, cy) - low
            for o, _, _, _ in group:
                o.location.z += dz
    bpy.context.view_layer.update()


def _arch_roof(b, key, bounds, frames, arch, age, keep_clear=()):
    """`arch`'s roof over `bounds`, the old roof's, with nothing of its own
    on the points `keep_clear` (where a flag will stand); returns its
    height at a point."""
    x0, x1, y0, y1, z0, z1 = bounds
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    w, d = x1 - x0, y1 - y0
    if arch == "asian":
        mat = "thatch" if age < 2 else "tile_dark"
        h = max(0.18, min(0.55, 0.32 * min(w, d)))
        roof, height_at = hip_roof(key + "_hip", w + 0.06, d + 0.06, h, mat, (cx, cy, z0),
                                   flare=0.06 if age < 2 else 0.09)
        b.add(roof, frames)
        if age >= 2:
            # A ridge of tile with its ends turned up, gilt in the Iron Age.
            long_x = w >= d
            r = max(abs(w - d) / 2.0, 0.04)
            size = (2 * r + 0.08, 0.06, 0.05) if long_x else (0.06, 2 * r + 0.08, 0.05)
            b.add(box(key + "_hip_ridge", size, "tile_dark", (cx, cy, z0 + h - 0.02)), frames)
            ends = [(-r, 0.0), (r, 0.0)] if long_x else [(0.0, -r), (0.0, r)]
            for i, (ex, ey) in enumerate(ends):
                b.add(cone(key + "_hip_horn%d" % i, 0.035, 0.12,
                           "gold" if age >= 3 else "tile_dark",
                           (cx + ex, cy + ey, z0 + h + 0.02), sides=6), frames)
        return height_at
    # Egyptian and Mesopotamian: a flat earthen or brick roof on the walls.
    slab = {"egyptian": {0: "mudbrick", 1: "plaster", 2: "sandstone", 3: "sandstone"},
            "mesopotamian": {0: "mudbrick", 1: "mudbrick", 2: "baked_brick",
                             3: "baked_brick"}}[arch][age]
    sw, sd = max(w - 0.18, 0.3), max(d - 0.18, 0.3)
    top = z0 + 0.1
    b.add(box(key + "_flat", (sw, sd, 0.1), slab, (cx, cy, z0)), frames)
    if arch == "egyptian":
        # A low parapet under a cavetto cornice, painted from the Tool Age,
        # and on a roof big enough to live on, a wind-catcher turned to the
        # north wind.
        lip = "limestone" if age >= 2 else "plaster"
        wall = {0: "mudbrick", 1: "plaster", 2: "sandstone", 3: "limestone"}[age]
        t = 0.05
        for k, (px, py, pw, pd) in enumerate(((0.0, -sd / 2 + t / 2, sw, t),
                                              (0.0, sd / 2 - t / 2, sw, t),
                                              (-sw / 2 + t / 2, 0.0, t, sd),
                                              (sw / 2 - t / 2, 0.0, t, sd))):
            b.add(box("%s_parapet%d" % (key, k), (pw, pd, 0.08), wall, (cx + px, cy + py, top)),
                  frames)
        b.add(rim(key + "_cornice", (sw - 2 * t, sd - 2 * t), (sw, sd), (sw + 0.1, sd + 0.1),
                  0.06, lip, (cx, cy, top + 0.06)), frames)
        if age >= 1:
            b.add(rim(key + "_band", (sw - 2 * t, sd - 2 * t), (sw + 0.02, sd + 0.02),
                      (sw + 0.02, sd + 0.02), 0.03, "paint_blue", (cx, cy, top + 0.025)), frames)
        corners = [(cx - sw / 2 + 0.25, cy - sd / 2 + 0.25), (cx + sw / 2 - 0.25, cy - sd / 2 + 0.25),
                   (cx - sw / 2 + 0.25, cy + sd / 2 - 0.25)]
        clear = [c for c in corners
                 if all(math.hypot(c[0] - px, c[1] - py) > 0.35 for px, py in keep_clear)]
        if min(sw, sd) >= 1.0 and clear:
            mx, my = clear[0]
            b.add(box(key + "_malqaf", (0.24, 0.2, 0.3), wall, (mx, my, top)), frames)
            b.add(frustum(key + "_malqaf_hood", (0.24, 0.2), (0.26, 0.06), 0.12, wall,
                          (mx, my + 0.0, top + 0.3)), frames)
            b.add(box(key + "_malqaf_mouth", (0.16, 0.02, 0.16), "opening",
                      (mx, my + 0.1, top + 0.12)), frames)
        return lambda px, py: top
    # Mesopotamian: crenellated, the merlons stepped from the Bronze Age.
    mat = "mudbrick" if age < 2 else "baked_brick"
    _merlons(b, key + "_crenel", (cx, cy, top), sw, sd, mat, frames, stepped=age >= 2)
    return lambda px, py: top


def _merlons(b, name, at, w, d, mat, frames, stepped=False, size=0.07):
    """Merlons round the edge of a `w` by `d` top at `at`."""
    cx, cy, z = at
    step = size * 2.0
    k = 0
    for along, fixed, axis in ((w, d, "x"), (d, w, "y")):
        n = max(2, int(along / step))
        # The corners once, with the first edges.
        for i in range(n + 1) if axis == "x" else range(1, n):
            t = -along / 2 + along * i / n
            for side in (-1.0, 1.0):
                x, y = (t, side * fixed / 2) if axis == "x" else (side * fixed / 2, t)
                b.add(box("%s_%d" % (name, k), (size, size, size), mat,
                          (cx + x, cy + y, z)), frames)
                if stepped:
                    b.add(box("%s_%d_top" % (name, k), (size * 0.55, size * 0.55, size * 0.5),
                              mat, (cx + x, cy + y, z + size)), frames)
                k += 1


def _dress_block(b, obj, shown, arch, age):
    """`arch`'s work on a walled block: Egyptian, a cornice, painted bands
    and, from the Bronze Age, a battered foot; Mesopotamian, buttresses and,
    from the Bronze Age, a band of blue glaze, glazed above it in the Iron;
    East Asian, a podium and posts, lacquered from the Bronze Age, with
    brackets under the eaves."""
    xs = [v.co for v in obj.data.vertices]
    w = max(c.x for c in xs) - min(c.x for c in xs)
    d = max(c.y for c in xs) - min(c.y for c in xs)
    h = max(c.z for c in xs) - min(c.z for c in xs)
    cx, cy, z0 = obj.location.x, obj.location.y, obj.location.z
    name = obj.name
    wall = obj.data.materials[0].name
    if arch == "egyptian":
        lip = "limestone" if age >= 2 else "plaster"
        b.add(rim(name + "_cavetto", (w - 0.1, d - 0.1), (w, d), (w + 0.1, d + 0.1), 0.07, lip,
                  (cx, cy, z0 + h - 0.06)), shown)
        if age >= 1:
            for k, (mat, z) in enumerate((("paint_blue", h - 0.1), ("paint_red", h - 0.14))):
                b.add(box("%s_paint%d" % (name, k), (w + 0.012, d + 0.012, 0.03), mat,
                          (cx, cy, z0 + z)), shown)
        if age >= 2:
            b.add(frustum(name + "_batter", (w + 0.12, d + 0.12), (w + 0.01, d + 0.01),
                          h * 0.3, wall, (cx, cy, z0)), shown)
    elif arch == "mesopotamian":
        n_x, n_y = max(2, int(w / 0.3)), max(2, int(d / 0.3))
        for i in range(n_x + 1):
            x = -w / 2 + w * i / n_x
            b.add(box("%s_buttress_y%d" % (name, i), (0.07, 0.04, h), wall,
                      (cx + x, cy + d / 2 + 0.01, z0)), shown)
        for i in range(n_y + 1):
            y = -d / 2 + d * i / n_y
            b.add(box("%s_buttress_x%d" % (name, i), (0.04, 0.07, h), wall,
                      (cx + w / 2 + 0.01, cy + y, z0)), shown)
        if age >= 2:
            b.add(box(name + "_glaze", (w + 0.05, d + 0.05, 0.08), "glaze",
                      (cx, cy, z0 + h * 0.72)), shown)
        if age >= 3:
            # Glazed to the top: a band round the wall, short of its top face.
            b.add(rim(name + "_glaze_top", (w - 0.04, d - 0.04), (w + 0.03, d + 0.03),
                      (w + 0.03, d + 0.03), h * 0.18 - 0.01, "glaze", (cx, cy, z0 + h * 0.82)),
                  shown)
            for i in range(n_x):
                x = -w / 2 + w * (i + 0.5) / n_x
                b.add(cylinder("%s_rosette_y%d" % (name, i), 0.025, 0.02, "gold",
                               (cx + x, cy + d / 2 + 0.03, z0 + h * 0.76), sides=8,
                               pivot="centre", rotation=(math.radians(90.0), 0.0, 0.0)), shown)
    elif arch == "asian":
        b.add(box(name + "_podium", (w + 0.16, d + 0.16, 0.07), "stone" if age >= 2 else "earth",
                  (cx, cy, z0)), shown)
        post = "lacquer" if age >= 2 else "wood_dark"
        n_x, n_y = (2, 2) if age == 0 else (max(2, int(w / 0.4)), max(2, int(d / 0.4)))
        for i in range(n_x + 1):
            x = -w / 2 + w * i / n_x
            b.add(box("%s_post_y%d" % (name, i), (0.05, 0.03, h), post,
                      (cx + x, cy + d / 2 + 0.01, z0)), shown)
            if age >= 2:
                b.add(box("%s_bracket_y%d" % (name, i), (0.1, 0.06, 0.05), "wood_dark",
                          (cx + x, cy + d / 2 + 0.03, z0 + h - 0.05)), shown)
        for i in range(n_y + 1):
            y = -d / 2 + d * i / n_y
            b.add(box("%s_post_x%d" % (name, i), (0.03, 0.05, h), post,
                      (cx + w / 2 + 0.01, cy + y, z0)), shown)
            if age >= 2:
                b.add(box("%s_bracket_x%d" % (name, i), (0.06, 0.1, 0.05), "wood_dark",
                          (cx + w / 2 + 0.03, cy + y, z0 + h - 0.05)), shown)
        if age >= 1:
            b.add(box(name + "_beam_y", (w, 0.03, 0.04), post, (cx, cy + d / 2 + 0.012, z0 + h - 0.04)),
                  shown)
            b.add(box(name + "_beam_x", (0.03, d, 0.04), post, (cx + w / 2 + 0.012, cy, z0 + h - 0.04)),
                  shown)


def _dress_column(b, obj, shown, arch, age):
    """An Egyptian column's flared papyrus capital, painted; the others'
    columns take their materials from the swaps."""
    if arch != "egyptian":
        return
    xs = [v.co for v in obj.data.vertices]
    r = (max(c.x for c in xs) - min(c.x for c in xs)) / 2
    h = max(c.z for c in xs) - min(c.z for c in xs)
    x, y, z = obj.location
    b.add(cylinder(obj.name + "_capital", r, 0.08, "paint_blue" if age >= 2 else "leaf",
                   (x, y, z + h - 0.08), sides=8, top_radius=r * 1.9), shown)
    b.add(cylinder(obj.name + "_foot", r * 1.3, 0.04, "sandstone", (x, y, z), sides=8), shown)


def age_dress(h, age, costume):
    """A figure's dress for its age. Soldiers: a cap and shoulder wraps of
    hide in the Tool Age; a bronze cap, pads and greaves in the Bronze;
    iron ones and a cape in the Iron, where a bronze helmet turns iron too.
    Villagers: a linen cap, then a straw hat with a brim, then a dark hood
    and a cape. The head carries most of it: on a sprite 34 px tall the
    head is what reads. What it wears keeps its owner's tunic in view,
    where the colour that says whose it is lies."""
    body = h.parts
    hx, hy, hz = HEAD
    top = HEAD_Z + hz * 0.25

    def on(part, key, piece):
        piece.parent = body[part] if part else h.body
        soften(piece, 0.3, 2, limit=0.02)
        return piece

    def dome(key, mat, grow=0.011, tall=0.8, at=0.0):
        return on(None, key, ellipsoid("%s_%s" % (h.root.name, key),
                                       (hx + grow, hy + grow, hz * tall), mat,
                                       (0.0, 0.012 - at, top), upper=True))

    if costume == "villager":
        mat = {1: "linen", 2: "straw", 3: "wood_dark"}[age]
        if age == 3:
            # A hood: down over the back of the head and the neck.
            dome("hat", mat, grow=0.016, tall=0.95, at=0.012)
            on(None, "hood_back", ellipsoid(h.root.name + "_hood", (hx + 0.012, 0.05, hz),
                                            mat, (0.0, -0.04, HEAD_Z - 0.01)))
        else:
            dome("hat", mat, tall=0.7)
        if age == 2:
            on(None, "brim", cylinder(h.root.name + "_brim", 0.15, 0.012, "straw",
                                      (0.0, 0.012, top - 0.004), sides=18))
    else:
        metal = {1: "hide", 2: "bronze", 3: "iron"}[age]
        if "helmet" not in body:
            dome("cap", metal)
            on(None, "cap_rim", cylinder(h.root.name + "_cap_rim", hx + 0.015, 0.016, metal,
                                         (0.0, 0.012, top - 0.006), sides=14))
        elif age == 3:
            for key in ("helmet", "helmet_top", "helmet_rim", "nasal", "cheek_l", "cheek_r"):
                part = body.get(key)
                if part is not None and part.data.materials[0].name == "bronze":
                    part.data.materials[0] = material("iron")
        for side in ("l", "r"):
            on("arm_" + side, "pad_" + side,
               box("%s_pad_%s" % (h.root.name, side), (0.095, 0.1, 0.075), metal,
                   (0.0, 0.0, -0.07)))
            if age >= 2:
                on("shin_" + side, "greave_" + side,
                   limb("%s_greave_%s" % (h.root.name, side), 0.11, 0.046, 0.038, metal,
                        (0.0, 0.004, -0.02)))
    if age == 3:
        on(None, "cape", loft(h.root.name + "_cape", (0.27, 0.03), (0.25, 0.03), 0.34,
                              "hide" if costume == "villager" else "wood_dark",
                              (0.0, -0.1, HIP_Z - 0.04)))


# --------------------------------------------------------------------------
# The villager: the figure, a tool for each job, and the loads it carries
# home. Its five animations are every unit's; the tasks and the carry walks
# come after them (docs/05 section 2.2), and the game picks one by what the
# villager is doing (`crates/view/src/scene.rs`).

VILLAGER_SPANS = dict(MOBILE_SPANS, **{
    "chop": (31, 36),
    "mine": (37, 42),
    "forage": (43, 48),
    "farm": (49, 54),
    "build": (55, 60),
    "carry_wood": (61, 68),
    "carry_food": (69, 76),
    "carry_gold": (77, 84),
    "carry_stone": (85, 92),
})


def _tool(name, handle, head_size, head_mat, head_at, below=0.06):
    """A handle standing up from the grip with a head at `head_at` along
    it, sticking out toward +Y."""
    grip = empty(name)
    parts = [cylinder(name + "_handle", 0.024, handle, "wood", (0.0, 0.0, -below), sides=6),
             box(name + "_head", head_size, head_mat, (0.0, head_size[1] / 2 - 0.02, head_at))]
    for p in parts:
        p.parent = grip
    return grip


def hatchet(name):
    """The villager's own tool, a stone-headed hatchet: it fells trees and
    fights with it."""
    return _tool(name, 0.36, (0.05, 0.12, 0.09), "stone", 0.21)


def pick(name):
    """A pick for stone and gold: a long head across the handle."""
    grip = _tool(name, 0.40, (0.04, 0.14, 0.04), "stone", 0.30)
    back = box(name + "_back", (0.04, 0.10, 0.035), "stone", (0.0, -0.06, 0.30))
    back.parent = grip
    return grip


def hoe(name):
    """A long-handled hoe, its blade flat and forward."""
    return _tool(name, 0.58, (0.10, 0.12, 0.02), "stone", 0.46, below=0.12)


def mallet(name):
    """A wooden mallet for building and repair."""
    return _tool(name, 0.26, (0.08, 0.13, 0.08), "wood_dark", 0.16, below=0.04)


def logs(name):
    """Three logs on the right shoulder, lying fore and aft."""
    root = empty(name)
    for i, (x, z) in enumerate([(0.12, 0.70), (0.19, 0.70), (0.155, 0.76)]):
        log = cylinder("%s_%d" % (name, i), 0.04, 0.46, "wood", (x, 0.02, z), sides=6,
                       pivot="centre", rotation=(_deg(90.0), 0.0, 0.0))
        log.parent = root
    return root


def basket(name, fill):
    """A basket held in front in both hands, heaped with `fill`."""
    root = empty(name)
    parts = [cylinder(name + "_basket", 0.10, 0.10, "straw", (0.0, 0.17, 0.38), sides=8,
                      top_radius=0.12)]
    for i, (x, y) in enumerate([(-0.04, 0.15), (0.04, 0.19), (0.0, 0.13), (0.03, 0.12),
                                (-0.03, 0.2)]):
        parts.append(box("%s_%d" % (name, i), (0.06, 0.06, 0.05), fill, (x, y, 0.47)))
    for p in parts:
        p.parent = root
    return root


def block(name):
    """A dressed stone block held in front in both hands."""
    root = empty(name)
    b = box(name + "_stone", (0.18, 0.15, 0.13), "rock_light", (0.0, 0.17, 0.38))
    b.parent = root
    return root


def villager_pose(anim, i, count):
    """One frame of a task or a carry walk: the body, each limb's swing, and
    the held tool's angle about X (0 holds it head up, -90 forward, 180
    down). Every task loops, so its last frame leads back to its first;
    the blow lands on the fourth."""
    body = {"dy": 0.0, "dz": 0.0, "rot_x": 0.0, "scale_z": 1.0}
    limbs = {"leg_l": _deg(12.0), "leg_r": _deg(-10.0), "arm_l": 0.0, "arm_r": 0.0,
             "fore_l": _deg(15.0), "fore_r": _deg(10.0), "shin_r": _deg(-8.0)}
    tool = 0.0
    swings = {
        # (right arm, tool, body pitch), a key per frame.
        "chop": [(150, 50, 0), (170, 75, 2), (130, -20, -2), (75, -105, -8), (60, -120, -9),
                 (105, -40, -4)],
        "mine": [(140, 40, 0), (170, 70, 4), (120, -40, -4), (45, -155, -16), (38, -165, -18),
                 (90, -70, -8)],
        "farm": [(100, -30, -8), (130, 20, -4), (90, -70, -10), (45, -145, -18),
                 (35, -155, -20), (65, -110, -14)],
        "build": [(110, -10, -4), (140, 40, -2), (100, -60, -5), (62, -120, -8),
                  (55, -125, -8), (80, -80, -6)],
    }
    if anim in swings:
        arm, angle, pitch = swings[anim][i % len(swings[anim])]
        limbs["arm_r"] = _deg(arm)
        # Both hands on the long tools; the free hand steadies the work.
        limbs["arm_l"] = _deg(arm * 0.85) if anim in ("mine", "farm") else _deg(55.0)
        tool = _deg(angle)
        body["rot_x"] = _deg(pitch)
        if anim in ("mine", "farm"):
            limbs["leg_l"], limbs["leg_r"] = _deg(18.0), _deg(-14.0)
    elif anim == "forage":
        # Bent to the bush, picking with one hand and then the other.
        reach = [(80, 30), (95, 50), (70, 85), (40, 95), (60, 70), (85, 40)][i % 6]
        limbs["arm_r"], limbs["arm_l"] = _deg(reach[0]), _deg(reach[1])
        body["rot_x"] = _deg(-14.0 if i % 3 else -10.0)
    elif anim.startswith("carry_"):
        phase = 2.0 * math.pi * i / count
        limbs["leg_l"] = _deg(24.0) * math.sin(phase)
        limbs["leg_r"] = _deg(24.0) * math.sin(phase + math.pi)
        limbs["shin_l"] = _deg(-38.0) * max(0.0, math.cos(phase))
        limbs["shin_r"] = _deg(-38.0) * max(0.0, math.cos(phase + math.pi))
        body["dz"] = 0.016 * abs(math.sin(phase))
        if anim == "carry_wood":
            # A hand up to the logs on the shoulder; the other arm swings.
            limbs["arm_r"] = _deg(125.0)
            limbs["arm_l"] = _deg(16.0) * math.sin(phase + math.pi)
        else:
            # Both arms under the load in front, the elbows bent to it.
            limbs["arm_r"] = limbs["arm_l"] = _deg(20.0)
            limbs["fore_r"] = limbs["fore_l"] = _deg(55.0)
    return body, limbs, tool


class Villager(Humanoid):
    """The villager: the soldiers' figure in the owner's tunic, hatchet in
    hand, with a tool for each job and the loads it carries home. Each tool
    and load is shown only on the frames of its own animation."""

    COSTUME = "villager"

    def __init__(self, root_name):
        super().__init__(root_name)
        self.hold(hatchet(root_name + "_hatchet"), lean=0.0)
        hatchet_frames = set(range(1, 31)) | _span("chop")
        self.tools = [(self.weapon, hatchet_frames)]
        for tool, anim in [(pick, "mine"), (hoe, "farm"), (mallet, "build")]:
            obj = tool("%s_%s" % (root_name, anim))
            obj.parent = self.body
            self.tools.append((obj, _span(anim)))
        self.loads = []
        for make, anim in [(lambda n: logs(n), "carry_wood"),
                           (lambda n: basket(n, "berry"), "carry_food"),
                           (lambda n: basket(n, "gold"), "carry_gold"),
                           (lambda n: block(n), "carry_stone")]:
            obj = make("%s_%s" % (root_name, anim))
            obj.parent = self.body
            self.loads.append((obj, _span(anim)))

    def animate(self, style="swing"):
        """Keys the five animations, then the tasks and the carry walks."""
        super().animate(style)
        scene = bpy.context.scene
        last = max(b for _, b in VILLAGER_SPANS.values())
        scene.frame_start, scene.frame_end = 1, last
        # Limbs only turn; where each one hangs never changes.
        rest = {k: tuple(o.location) for k, o in self.parts.items()}
        for anim, (first, end) in VILLAGER_SPANS.items():
            if anim in MOBILE_SPANS:
                continue
            count = end - first + 1
            for i in range(count):
                frame = first + i
                body, limbs, angle = villager_pose(anim, i, count)
                self.body.location = (0.0, body["dy"], body["dz"])
                self.body.rotation_euler = (body["rot_x"], 0.0, 0.0)
                self.body.scale = (1.0, 1.0, body["scale_z"])
                for path in ("location", "rotation_euler", "scale"):
                    self.body.keyframe_insert(path, frame=frame)
                for key, obj in self.parts.items():
                    obj.location = rest[key]
                    obj.rotation_euler = (limbs.get(key, 0.0), 0.0, 0.0)
                    obj.keyframe_insert("location", frame=frame)
                    obj.keyframe_insert("rotation_euler", frame=frame)
                for tool, _ in self.tools:
                    tool.location = hand(limbs.get("arm_r", 0.0), 1.0, limbs.get("fore_r", 0.0))
                    tool.rotation_euler = (angle, 0.0, 0.0)
                    tool.keyframe_insert("location", frame=frame)
                    tool.keyframe_insert("rotation_euler", frame=frame)
        for obj, frames in self.tools + self.loads:
            meshes = [c for c in obj.children_recursive if c.type == "MESH"]
            for frame in range(1, last + 1):
                hidden = frame not in frames
                for m in meshes:
                    m.hide_render = hidden
                    m.hide_viewport = hidden
                    m.keyframe_insert("hide_render", frame=frame)
                    m.keyframe_insert("hide_viewport", frame=frame)
        everything = [self.body] + list(self.parts.values())
        everything += [o for o, _ in self.tools]
        everything += [c for o, _ in self.tools + self.loads for c in o.children_recursive]
        hold_frames(everything)


def _span(anim):
    first, last = VILLAGER_SPANS[anim]
    return set(range(first, last + 1))


# --------------------------------------------------------------------------
# The horse and its rider.

def horse(add, n, coat="horse", mane="horse_dark"):
    """A horse facing +Y about the origin, its hooves on the ground: a barrel between a deep chest and a round rump, a neck arched
    up to a long head with ears, eyes and a dark muzzle, a mane and a tail,
    and legs jointed at the knee and the hock down to dark hooves. Its parts
    go onto the body through `add(key, obj, parent=None)` under the keys
    `rider_pose` swings: "head", "neck", "tail", "leg_fl" and "shin_fl" and
    the like."""
    leg = Rider.LEG
    add("barrel", ellipsoid(n + "_barrel", (0.14, 0.3, 0.16), coat, (0.0, -0.01, 0.5)))
    add("chest", ellipsoid(n + "_chest", (0.13, 0.14, 0.165), coat, (0.0, 0.19, 0.515)))
    add("rump", ellipsoid(n + "_rump", (0.145, 0.15, 0.155), coat, (0.0, -0.21, 0.53)))
    # The neck leans forward and up from the withers; the head hangs
    # from the poll, nose down, and nods about it.
    tilt = _deg(-40.0)
    neck = add("neck", cylinder(n + "_neck", 0.088, 0.3, coat, (0.0, 0.24, 0.58), sides=14,
                                rotation=(tilt, 0.0, 0.0), top_radius=0.06))
    crest = cylinder(n + "_mane", 0.032, 0.32, mane, (0.0, -0.06, -0.01), sides=6,
                     top_radius=0.024)
    crest.parent = neck
    poll = (0.0, 0.24 - 0.3 * math.sin(tilt), 0.58 + 0.3 * math.cos(tilt))
    head = add("head", limb(n + "_head", 0.27, 0.062, 0.044, coat, poll,
                            rotation=(_deg(60.0), 0.0, 0.0)))
    for side in (-1.0, 1.0):
        ear = cone(n + "_ear", 0.016, 0.06, coat, (side * 0.026, 0.01, -0.01), sides=6)
        ear.parent = head
        eye = ellipsoid(n + "_eye", (0.012, 0.012, 0.012), "opening",
                        (side * 0.045, 0.0, -0.07))
        eye.parent = head
    nose = ellipsoid(n + "_muzzle", (0.04, 0.042, 0.035), "horse_dark", (0.0, 0.0, -0.25))
    nose.parent = head
    add("tail", limb(n + "_tail", 0.3, 0.04, 0.02, mane, (0.0, -0.34, 0.58),
                     rotation=(_deg(-22.0), 0.0, 0.0)))
    for key, x, y in (("fl", -0.075, 0.2), ("fr", 0.075, 0.2),
                      ("bl", -0.08, -0.22), ("br", 0.08, -0.22)):
        upper = add("leg_" + key, limb(n + "_leg_" + key, 0.19, 0.062, 0.04, coat,
                                       (x, y, leg)))
        lower = add("shin_" + key, limb(n + "_cannon_" + key, 0.185, 0.03, 0.026, coat,
                                        (0.0, 0.0, -0.19)), parent=upper)
        hoof = limb(n + "_hoof_" + key, 0.045, 0.026, 0.032, "horse_dark",
                    (0.0, 0.004, -0.185))
        hoof.parent = lower


class Rider:
    """A horse 0.8 units nose to tail with a rider on its back, facing +Y.

    The horse has a barrel between a deep chest and a round rump, a neck
    arched up to a long head with ears and a muzzle, a mane and a tail, and
    jointed legs down to dark hooves; the legs swing from the shoulder and
    the hip in diagonal pairs at the walk and fold at the knee and the hock
    as they come through. The rider sits astride with the thighs forward
    and the shins down the horse's sides, carries the `weapon` in the right
    hand and strikes with it. Everything hangs off `body`, one level below
    the root, so the root stays free for render_sheet.py's turntable.
    """

    LEG = 0.42
    BACK = 0.62

    def __init__(self, root_name, coat="horse", tunic="player", helmet=None,
                 helmet_mat="bronze", saddle_cloth=True, mane="horse_dark", barding=None,
                 cuirass=None):
        self.root = empty(root_name)
        self.body = empty(root_name + "_body", parent=self.root)
        self.parts = {}
        n = root_name
        add = self._add
        leg, back = self.LEG, self.BACK
        horse(add, n, coat, mane)
        if barding:
            # Armour on the horse: a plate over the chest and a cloth of the
            # owner's colour hanging to the belly.
            add("peytral", ellipsoid(n + "_peytral", (0.135, 0.05, 0.11), barding,
                                     (0.0, 0.3, 0.53)))
            add("caparison", oval(n + "_caparison", (0.16, 0.3), (0.15, 0.28), 0.2, tunic,
                                  (0.0, -0.04, back - 0.17)))
        if saddle_cloth:
            add("cloth", loft(n + "_cloth", (0.3, 0.27), (0.27, 0.24), 0.05, tunic,
                              (0.0, -0.04, back - 0.045)))
            add("saddle", loft(n + "_saddle", (0.17, 0.2), (0.15, 0.17), 0.035, "hide",
                               (0.0, -0.04, back)))
        # The rider, astride: thighs forward along the horse's flanks, shins
        # hanging down its sides.
        seat = back + 0.035
        for side, x in (("l", -0.1), ("r", 0.1)):
            out = -1.0 if side == "l" else 1.0
            # Forward and a little down (80 degrees about x), splayed out
            # round the barrel (about z); the shin turned back to hang.
            thigh = add("r_leg_" + side, limb(n + "_r_thigh_" + side, 0.13, 0.05, 0.04, "trouser",
                                              (x, -0.02, seat + 0.03),
                                              rotation=(_deg(80.0), 0.0, _deg(-15.0) * out)))
            shin = add("r_shin_" + side, limb(n + "_r_shin_" + side, 0.15, 0.038, 0.03, "trouser",
                                              (0.0, 0.0, -0.13),
                                              rotation=(_deg(-74.0), 0.0, 0.0)), parent=thigh)
            boot = box(n + "_r_boot_" + side, (0.06, 0.1, 0.035), "hide",
                       (0.0, 0.02, -0.17))
            boot.parent = shin
        add("r_kilt", oval(n + "_r_kilt", (0.125, 0.1), (0.11, 0.08), 0.07, tunic,
                           (0.0, -0.03, seat)))
        add("r_torso", oval(n + "_r_torso", (0.095, 0.064), (0.12, 0.074), 0.23, tunic,
                            (0.0, -0.03, seat + 0.04), lean=0.015))
        add("r_yoke", ellipsoid(n + "_r_yoke", (0.155, 0.075, 0.048), tunic,
                                (0.0, -0.02, seat + 0.27)))
        if cuirass:
            add("r_cuirass", oval(n + "_r_cuirass", (0.1, 0.07), (0.127, 0.08), 0.2, cuirass,
                                  (0.0, -0.03, seat + 0.06), lean=0.015))
        add("r_neck", cylinder(n + "_r_neck", 0.033, 0.06, "skin", (0.0, -0.02, seat + 0.29),
                               sides=12))
        head_and_helmet(lambda k, o: add("r_" + k, o), n + "_r",
                        (0.0, -0.018, seat + 0.29 + 0.1), helmet, helmet_mat)
        self.shoulder = (0.15, -0.02, seat + 0.285)
        for side, x in (("l", -0.15), ("r", 0.15)):
            upper = add("r_arm_" + side, limb(n + "_r_upper_" + side, 0.13, 0.036, 0.029, "skin",
                                              (x, -0.02, seat + 0.285)))
            sleeve = limb(n + "_r_sleeve_" + side, 0.06, 0.044, 0.038, tunic)
            sleeve.parent = upper
            fore = add("r_fore_" + side, limb(n + "_r_fore_" + side, 0.11, 0.029, 0.023, "skin",
                                              (0.0, 0.0, -0.13)), parent=upper)
            grip = ellipsoid(n + "_r_hand_" + side, (0.025, 0.028, 0.03), "skin",
                             (0.0, 0.004, -0.124))
            grip.parent = fore
        self.weapon = None

    def _add(self, key, obj, parent=None):
        obj.parent = parent if parent is not None else self.body
        self.parts[key] = obj
        # Rounded ends to the trunk's boxes; the round parts are round.
        if key in ("cloth", "saddle"):
            soften(obj, 0.42, 3, limit=0.06)
        return obj

    def hold(self, weapon, lean=18.0):
        weapon.parent = self.body
        self.weapon = weapon
        self.weapon_lean = _deg(lean)

    def grip(self, arm, fore):
        """Where the rider's right hand is, the arm swung `arm` and the
        elbow bent `fore`."""
        x, y, z = self.shoulder
        ey, ez = y + 0.13 * math.sin(arm), z - 0.13 * math.cos(arm)
        a, reach = arm + fore, 0.124
        return (x, ey + reach * math.sin(a), ez - reach * math.cos(a))

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
                    self.weapon.location = self.grip(swings.get("r_arm_r", 0.0),
                                                     swings.get("r_fore_r", 0.0))
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
    s = {"r_fore_r": _deg(30.0), "r_fore_l": _deg(55.0), "r_arm_l": _deg(15.0)}
    weapon = _deg(-15.0)

    if anim == "idle":
        s["head"] = _deg(4.0) * math.sin(2.0 * math.pi * i / count)
        s["tail"] = _deg(6.0) * math.sin(2.0 * math.pi * i / count + 1.0)
        s["r_arm_r"] = _deg(15.0)

    elif anim == "walk":
        phase = 2.0 * math.pi * i / count
        a = _deg(24.0) * math.sin(phase)
        # Diagonal pairs: near fore with off hind, then the other two. A leg
        # folds at the knee or hock as it comes forward.
        bend = _deg(-45.0) * max(0.0, math.cos(phase))
        other = _deg(-45.0) * max(0.0, math.cos(phase + math.pi))
        s["leg_fl"], s["leg_br"] = a, a
        s["leg_fr"], s["leg_bl"] = -a, -a
        s["shin_fl"], s["shin_br"] = bend, -bend * 0.6
        s["shin_fr"], s["shin_bl"] = other, -other * 0.6
        s["head"] = _deg(5.0) * math.sin(2.0 * phase)
        s["neck"] = _deg(3.0) * math.sin(2.0 * phase)
        s["tail"] = _deg(8.0) * math.sin(phase)
        body["dz"] = 0.015 * abs(math.sin(phase))
        s["r_arm_r"] = _deg(15.0)
        s["r_arm_l"] = _deg(10.0)

    elif anim == "attack":
        keys = [20.0, 40.0, 10.0, 85.0, 70.0, 35.0]
        elbows = [60.0, 75.0, 85.0, 5.0, 15.0, 40.0]
        s["r_arm_r"] = _deg(keys[i])
        s["r_fore_r"] = _deg(elbows[i])
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
        for leg in ("shin_fl", "shin_fr", "shin_bl", "shin_br"):
            s[leg] = _deg(-30.0) * t
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


# --------------------------------------------------------------------------
# Armour for the later ages' soldiers.

def cuirass(h, mat):
    """A breastplate over the figure's trunk, in `mat`."""
    return h.wear("cuirass", oval(h.root.name + "_cuirass", (0.106, 0.074), (0.133, 0.083),
                                  SHOULDER_Z - 0.05 - (HIP_Z + 0.03), mat,
                                  (0.0, 0.0, HIP_Z + 0.03), lean=0.01))


def greaves(h, mat):
    """Shin guards on both legs."""
    for side in ("l", "r"):
        g = limb("%s_greave_%s" % (h.root.name, side), 0.11, 0.047, 0.039, mat,
                 (0.0, 0.004, -0.02))
        g.parent = h.parts["shin_" + side]


def wheel(name, radius, mat="wood_dark", spokes=8, thick=0.03):
    """A spoked wheel on an empty at its hub, turning about x: a rim of
    rods, spokes, a hub."""
    hub = empty(name)
    rim = []
    n = 14
    for k in range(n):
        a0, a1 = 2 * math.pi * k / n, 2 * math.pi * (k + 1) / n
        p0 = (0.0, radius * math.cos(a0), radius * math.sin(a0))
        p1 = (0.0, radius * math.cos(a1), radius * math.sin(a1))
        rim.append(rod("%s_rim%d" % (name, k), p0, p1, thick * 0.6, mat, sides=6))
    for k in range(spokes):
        a = 2 * math.pi * k / spokes
        rim.append(rod("%s_spoke%d" % (name, k), (0.0, 0.0, 0.0),
                       (0.0, radius * math.cos(a), radius * math.sin(a)), thick * 0.3, mat,
                       sides=5))
    rim.append(cylinder(name + "_hub", thick * 1.2, thick * 2.2, mat, (0.0, 0.0, 0.0), sides=10,
                        pivot="centre", rotation=(0.0, _deg(90.0), 0.0)))
    for part in rim:
        part.parent = hub
    return hub


# --------------------------------------------------------------------------
# The chariot: a horse, a car on two wheels, an archer standing in it.

class Chariot:
    """A horse drawing a two-wheeled car with an archer standing in it, 1.55
    units from the horse's nose to the car's tail, facing +Y. The horse
    walks as a rider's does; the wheels turn; the archer stands, and at the
    attack draws and looses."""

    CAR_Y = -0.62

    def __init__(self, root_name, coat="horse", tunic="player"):
        self.root = empty(root_name)
        self.body = empty(root_name + "_body", parent=self.root)
        self.parts = {}
        n = root_name
        add = self._add
        horse(add, n, coat)
        y = self.CAR_Y
        add("collar", oval(n + "_collar", (0.12, 0.05), (0.1, 0.045), 0.06, tunic,
                           (0.0, 0.3, 0.6)))
        add("pole", rod(n + "_pole", (0.0, y + 0.12, 0.3), (0.0, 0.3, 0.6), 0.02, "wood"))
        add("floor", box(n + "_floor", (0.4, 0.3, 0.04), "wood", (0.0, y, 0.3)))
        add("front", loft(n + "_front", (0.4, 0.04), (0.34, 0.04), 0.3, tunic,
                          (0.0, y + 0.15, 0.32)))
        for side in (-1.0, 1.0):
            add("rail_%d" % side, rod(n + "_rail", (side * 0.19, y + 0.15, 0.6),
                                      (side * 0.19, y - 0.12, 0.42), 0.012, "wood"))
        add("axle", cylinder(n + "_axle", 0.018, 0.54, "wood_dark", (0.0, y, 0.21), sides=8,
                             pivot="centre", rotation=(0.0, _deg(90.0), 0.0)))
        self.wheels = []
        for side in (-1.0, 1.0):
            w = wheel("%s_wheel_%d" % (n, side), 0.21)
            w.parent = self.body
            w.location = (side * 0.25, y, 0.21)
            self.wheels.append(w)
        self.archer = Humanoid(n + "_archer", tunic=tunic, helmet="cap", helmet_mat="bronze")
        self.archer.hold(bow(n + "_bow"), hand="left", lean=0.0)
        self.archer.root.parent = self.body
        self.archer.root.location = (0.0, y, 0.32)
        self.archer.root.scale = (0.92, 0.92, 0.92)

    def _add(self, key, obj, parent=None):
        obj.parent = parent if parent is not None else self.body
        self.parts[key] = obj
        return obj

    def animate(self):
        scene = bpy.context.scene
        scene.frame_start, scene.frame_end = 1, 30
        rest = {k: (tuple(o.location), tuple(o.rotation_euler)) for k, o in self.parts.items()}
        arest = {k: tuple(o.location) for k, o in self.archer.parts.items()}
        a = self.archer
        for anim, (first, last) in MOBILE_SPANS.items():
            count = last - first + 1
            for i in range(count):
                frame = first + i
                body, swings, _ = rider_pose(anim, i, count)
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
                turn = 0.0
                if anim == "walk":
                    turn = -2.0 * math.pi * i / count
                for w in self.wheels:
                    w.rotation_euler = (turn, 0.0, 0.0)
                    w.keyframe_insert("rotation_euler", frame=frame)
                # The archer stands in the car: the walk is the car's.
                pose = {"walk": "idle", "death": "idle", "decay": "idle"}.get(anim, anim)
                b, limbs, weapon_angle = humanoid_pose(pose, i if pose == anim else 0,
                                                       count, "shoot")
                a.body.location = (0.0, b["dy"], b["dz"])
                a.body.rotation_euler = (b["rot_x"], 0.0, 0.0)
                for path in ("location", "rotation_euler"):
                    a.body.keyframe_insert(path, frame=frame)
                for key, obj in a.parts.items():
                    obj.location = arest[key]
                    obj.rotation_euler = (limbs.get(key, 0.0), 0.0, 0.0)
                    obj.keyframe_insert("location", frame=frame)
                    obj.keyframe_insert("rotation_euler", frame=frame)
                a.weapon.location = hand(limbs.get("arm_l", 0.0), -1.0, limbs.get("fore_l", 0.0))
                a.weapon.rotation_euler = (weapon_angle, a.weapon_lean, 0.0)
                a.weapon.keyframe_insert("location", frame=frame)
                a.weapon.keyframe_insert("rotation_euler", frame=frame)
        hold_frames([self.body, a.body, a.weapon] + self.wheels + list(self.parts.values())
                    + list(a.parts.values()))


# --------------------------------------------------------------------------
# The war elephant.

class Elephant:
    """A war elephant 1.5 units from trunk to tail, facing +Y: a great grey
    body on four pillar legs, a head with fanned ears, ivory tusks and a
    trunk of three joints, a howdah of the owner's colour on its back with
    a driver on its neck. It walks in diagonal pairs, its trunk swaying;
    at the attack it rears its head and swings the trunk; it falls onto its
    side."""

    def __init__(self, root_name, tunic="player"):
        self.root = empty(root_name)
        self.body = empty(root_name + "_body", parent=self.root)
        self.parts = {}
        n = root_name
        add = self._add
        hide = "elephant"
        add("barrel", ellipsoid(n + "_barrel", (0.28, 0.5, 0.3), hide, (0.0, -0.03, 0.74)))
        add("rump", ellipsoid(n + "_rump", (0.26, 0.24, 0.28), hide, (0.0, -0.36, 0.76)))
        head = add("head", ellipsoid(n + "_head", (0.21, 0.2, 0.24), hide, (0.0, 0.5, 0.86)))
        for side in (-1.0, 1.0):
            ear = ellipsoid(n + "_ear", (0.025, 0.16, 0.2), hide, (side * 0.2, -0.1, 0.02))
            ear.rotation_euler = (0.0, 0.0, _deg(-25.0) * side)
            ear.parent = head
            tusk = rod(n + "_tusk", (side * 0.09, 0.15, -0.12), (side * 0.12, 0.36, -0.2), 0.026,
                       "ivory", sides=8, top_radius=0.008)
            tusk.parent = head
            eye = ellipsoid(n + "_eye", (0.016, 0.016, 0.016), "opening",
                            (side * 0.17, 0.1, 0.06))
            eye.parent = head
        t1 = add("trunk1", limb(n + "_trunk1", 0.26, 0.085, 0.065, hide, (0.0, 0.66, 0.82),
                                rotation=(_deg(18.0), 0.0, 0.0)))
        t2 = add("trunk2", limb(n + "_trunk2", 0.24, 0.065, 0.048, hide, (0.0, 0.0, -0.26),
                                rotation=(_deg(-10.0), 0.0, 0.0)), parent=t1)
        add("trunk3", limb(n + "_trunk3", 0.2, 0.048, 0.034, hide, (0.0, 0.0, -0.24),
                           rotation=(_deg(-25.0), 0.0, 0.0)), parent=t2)
        add("tail", limb(n + "_tail", 0.32, 0.03, 0.015, hide, (0.0, -0.58, 0.84),
                         rotation=(_deg(-12.0), 0.0, 0.0)))
        for key, x, y in (("fl", -0.16, 0.28), ("fr", 0.16, 0.28),
                          ("bl", -0.16, -0.36), ("br", 0.16, -0.36)):
            upper = add("leg_" + key, limb(n + "_leg_" + key, 0.3, 0.13, 0.115, hide, (x, y, 0.6)))
            lower = add("shin_" + key, limb(n + "_shin_" + key, 0.24, 0.115, 0.105, hide,
                                            (0.0, 0.0, -0.3)), parent=upper)
            foot = cylinder(n + "_foot_" + key, 0.125, 0.06, "elephant_toe", (0.0, 0.0, -0.3),
                            sides=14, top_radius=0.11)
            foot.parent = lower
        # The howdah: a blanket of the owner's colour and a low wooden
        # platform with a rim of it, on the back.
        add("blanket", oval(n + "_blanket", (0.3, 0.36), (0.28, 0.34), 0.08, tunic,
                            (0.0, -0.08, 0.96)))
        add("howdah", loft(n + "_howdah", (0.42, 0.5), (0.46, 0.54), 0.16, "wood",
                           (0.0, -0.1, 1.04)))
        add("howdah_rim", loft(n + "_rim", (0.48, 0.56), (0.48, 0.56), 0.04, tunic,
                               (0.0, -0.1, 1.2)))
        # The driver astride the neck.
        add("d_torso", oval(n + "_d_torso", (0.085, 0.06), (0.11, 0.068), 0.22, "linen",
                            (0.0, 0.36, 1.08), lean=0.01))
        head_and_helmet(lambda k, o: add("d_" + k, o), n + "_d", (0.0, 0.372, 1.39), "band")
        for side in (-1.0, 1.0):
            add("d_arm_%d" % side, limb(n + "_d_arm", 0.2, 0.033, 0.026, "skin",
                                        (side * 0.13, 0.38, 1.28),
                                        rotation=(_deg(40.0), 0.0, 0.0)))
            add("d_leg_%d" % side, limb(n + "_d_leg", 0.24, 0.045, 0.034, "linen",
                                        (side * 0.12, 0.36, 1.1),
                                        rotation=(_deg(30.0), 0.0, _deg(25.0) * side)))
        self.weapon = None

    def _add(self, key, obj, parent=None):
        obj.parent = parent if parent is not None else self.body
        self.parts[key] = obj
        return obj

    def animate(self):
        scene = bpy.context.scene
        scene.frame_start, scene.frame_end = 1, 30
        rest = {k: (tuple(o.location), tuple(o.rotation_euler)) for k, o in self.parts.items()}
        for anim, (first, last) in MOBILE_SPANS.items():
            count = last - first + 1
            for i in range(count):
                frame = first + i
                body, swings = elephant_pose(anim, i, count)
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
        hold_frames([self.body] + list(self.parts.values()))


# An elephant on its side lies out along +X about this far from its middle.
FALLEN_ELEPHANT = 0.6


def elephant_pose(anim, i, count):
    """One frame of the elephant: the body's bob, roll and slide, each
    part's swing about x."""
    body = {"dx": 0.0, "dz": 0.0, "roll": 0.0, "scale_z": 1.0}
    s = {}
    if anim == "idle":
        w = math.sin(2.0 * math.pi * i / count)
        s["trunk1"], s["trunk2"] = _deg(4.0) * w, _deg(8.0) * w
        s["tail"] = _deg(6.0) * w
    elif anim == "walk":
        phase = 2.0 * math.pi * i / count
        a = _deg(16.0) * math.sin(phase)
        bend = _deg(-22.0) * max(0.0, math.cos(phase))
        other = _deg(-22.0) * max(0.0, math.cos(phase + math.pi))
        s["leg_fl"], s["leg_br"] = a, a
        s["leg_fr"], s["leg_bl"] = -a, -a
        s["shin_fl"], s["shin_br"] = bend, -bend * 0.5
        s["shin_fr"], s["shin_bl"] = other, -other * 0.5
        s["trunk1"] = _deg(8.0) * math.sin(phase)
        s["trunk2"] = _deg(10.0) * math.sin(phase + 0.8)
        s["head"] = _deg(3.0) * math.sin(2.0 * phase)
        s["tail"] = _deg(10.0) * math.sin(phase)
        body["dz"] = 0.02 * abs(math.sin(phase))
    elif anim == "attack":
        # The head up and the trunk lifted, then swung down on the blow.
        head = [-6.0, -14.0, -18.0, 10.0, 6.0, 0.0]
        trunk = [-30.0, -60.0, -80.0, 30.0, 15.0, 0.0]
        s["head"] = _deg(head[i])
        s["trunk1"] = _deg(trunk[i])
        s["trunk2"] = _deg(trunk[i] * 0.6)
        s["leg_fl"] = s["leg_fr"] = _deg(-12.0 if i < 3 else 8.0)
        body["dz"] = 0.04 if i in (1, 2) else 0.0
    elif anim in ("death", "decay"):
        t = i / float(count - 1) if anim == "death" else 1.0
        ease = t * t
        body["roll"] = _deg(82.0) * ease
        body["dx"] = -FALLEN_ELEPHANT * ease
        body["dz"] = -0.03 * t
        for leg in ("leg_fl", "leg_fr", "leg_bl", "leg_br"):
            s[leg] = _deg(15.0) * t
        s["trunk1"] = _deg(30.0) * t
        if anim == "decay":
            k = (i + 1) / float(count)
            body["scale_z"] = 1.0 - 0.12 * k
            body["dz"] = -0.03 - 0.03 * k
    return body, s


# --------------------------------------------------------------------------
# Siege engines.

class Engine:
    """A siege engine on wheels, facing +Y. `kind` is "thrower" (a torsion
    arm that flings a stone from a sling: the Stone Thrower), "catapult" (the
    same, larger, ironbound, a bucket for the stone) or "ballista" (a great
    crossbow on a carriage). It rolls on its wheels; at the attack the arm
    comes over and strikes its crossbar, or the bow's string is drawn back
    and loosed, on the impact frame; it falls apart on its side."""

    def __init__(self, root_name, kind="thrower", tunic="player"):
        self.root = empty(root_name)
        self.body = empty(root_name + "_body", parent=self.root)
        self.parts = {}
        self.kind = kind
        n = root_name
        add = self._add
        big = 1.15 if kind == "catapult" else 1.0
        fit = "iron" if kind == "catapult" else "bronze"
        r = 0.16 * big
        self.wheels = []
        wheels = ((-0.27, 0.32), (0.27, 0.32), (-0.27, -0.32), (0.27, -0.32))
        if kind == "ballista":
            wheels = ((-0.25, -0.15), (0.25, -0.15))
        for k, (x, y) in enumerate(wheels):
            w = wheel("%s_wheel%d" % (n, k), r, spokes=6)
            w.parent = self.body
            w.location = (x * big, y * big, r)
            self.wheels.append(w)
        if kind == "ballista":
            add("trail", rod(n + "_trail", (0.0, -0.15, r), (0.0, -0.62, 0.04), 0.035, "wood"))
            add("axle", cylinder(n + "_axle", 0.02, 0.5, "wood_dark", (0.0, -0.15, r), sides=8,
                                 pivot="centre", rotation=(0.0, _deg(90.0), 0.0)))
            add("mount", box(n + "_mount", (0.1, 0.1, 0.22), "wood", (0.0, -0.12, r)))
            add("stock", box(n + "_stock", (0.1, 0.8, 0.07), "wood", (0.0, -0.05, r + 0.22)))
            add("frame", box(n + "_frame", (0.2, 0.12, 0.2), fit, (0.0, 0.25, r + 0.16)))
            add("shield", loft(n + "_shield", (0.4, 0.03), (0.36, 0.03), 0.13, tunic,
                               (0.0, 0.36, r + 0.04)))
            self.arms = []
            for side in (-1.0, 1.0):
                arm = add("arm_%d" % side, limb(n + "_bowarm", 0.34, 0.028, 0.02, "wood",
                                                (side * 0.08, 0.26, r + 0.3),
                                                rotation=(0.0, _deg(-80.0) * side, 0.0)))
                self.arms.append(arm)
            add("string", rod(n + "_string", (-0.4, 0.18, r + 0.3), (0.4, 0.18, r + 0.3), 0.006,
                              "linen", sides=4))
            add("bolt", rod(n + "_bolt", (0.0, 0.0, r + 0.3), (0.0, 0.55, r + 0.3), 0.014,
                            "wood", sides=6))
            tip = add("bolt_tip", blade(n + "_tip", 0.04, 0.08, 0.02, "iron", (0.0, 0.55, r + 0.3)))
            tip.rotation_euler = (_deg(-90.0), 0.0, 0.0)
        else:
            L = 0.9 * big
            for side in (-1.0, 1.0):
                add("beam_%d" % side, box(n + "_beam", (0.07, L, 0.08), "wood",
                                          (side * 0.2 * big, 0.0, r + 0.02)))
                add("upright_%d" % side, box(n + "_upright", (0.06, 0.07, 0.4 * big), "wood",
                                             (side * 0.2 * big, 0.12, r + 0.1)))
            for y in (-0.38 * big, 0.38 * big):
                add("cross_%d" % int(y * 100), box(n + "_cross", (0.46 * big, 0.06, 0.06), "wood",
                                                   (0.0, y, r + 0.06)))
            add("stop", box(n + "_stop", (0.46 * big, 0.08, 0.08), fit,
                            (0.0, 0.12, r + 0.1 + 0.4 * big)))
            add("bundle", cylinder(n + "_skein", 0.07 * big, 0.4 * big, "rope", (0.0, -0.12, r + 0.12),
                                   sides=10, pivot="centre", rotation=(0.0, _deg(90.0), 0.0)))
            # The arm hangs from the skein at the back, laid back to rest.
            arm = add("throw_arm", limb(n + "_arm", 0.62 * big, 0.04, 0.03, "wood",
                                        (0.0, -0.12, r + 0.12),
                                        rotation=(_deg(-100.0), 0.0, 0.0)))
            if kind == "catapult":
                cup = ellipsoid(n + "_bucket", (0.09, 0.09, 0.05), "iron",
                                (0.0, 0.0, -0.62 * big), upper=True)
                cup.parent = arm
                for k, y in enumerate((-0.2, 0.0, 0.2)):
                    band = cylinder("%s_band%d" % (n, k), 0.034, 0.03, fit, (0.0, 0.0, -0.2 + y * 0.6),
                                    sides=10)
                    band.parent = arm
            else:
                sling_cord = limb(n + "_sling", 0.16, 0.008, 0.008, "rope", (0.0, 0.0, -0.62))
                sling_cord.parent = arm
                pouch = ellipsoid(n + "_pouch", (0.04, 0.04, 0.035), "hide",
                                  (0.0, 0.0, -0.79))
                pouch.parent = arm
            stone = ellipsoid(n + "_stone", (0.05, 0.05, 0.05), "stone",
                              (0.0, 0.0, -0.62 * big - (0.03 if kind == "catapult" else 0.17)))
            stone.parent = arm
            self.stone = stone
            add("banner", box(n + "_banner", (0.03, 0.14, 0.12), tunic,
                              (0.2 * big + 0.03, 0.12, r + 0.3 * big)))

    def _add(self, key, obj, parent=None):
        obj.parent = parent if parent is not None else self.body
        self.parts[key] = obj
        return obj

    def animate(self):
        scene = bpy.context.scene
        scene.frame_start, scene.frame_end = 1, 30
        rest = {k: (tuple(o.location), tuple(o.rotation_euler)) for k, o in self.parts.items()}
        stone = getattr(self, "stone", None)
        for anim, (first, last) in MOBILE_SPANS.items():
            count = last - first + 1
            for i in range(count):
                frame = first + i
                body, swings, loaded = engine_pose(self.kind, anim, i, count)
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
                for w in self.wheels:
                    w.rotation_euler = (swings.get("wheels", 0.0), 0.0, 0.0)
                    w.keyframe_insert("rotation_euler", frame=frame)
                if stone is not None:
                    stone.hide_render = not loaded
                    stone.keyframe_insert("hide_render", frame=frame)
        hold_frames([self.body] + self.wheels + list(self.parts.values())
                    + ([stone] if stone is not None else []))


# A siege engine on its side lies out along +X about this far from its middle.
FALLEN_ENGINE = 0.3


def engine_pose(kind, anim, i, count):
    """One frame of a siege engine: the body's roll and slide, each part's
    swing about x (the arm's, the wheels'), and whether the stone is in the
    sling."""
    body = {"dx": 0.0, "dz": 0.0, "roll": 0.0, "scale_z": 1.0}
    s = {}
    loaded = True
    if anim == "walk":
        s["wheels"] = -2.0 * math.pi * i / count
    elif anim == "attack":
        if kind == "ballista":
            draw = [0.0, -0.06, -0.12, 0.0, 0.0, 0.0]
            s["string"] = 0.0
            s["arm_-1"] = s["arm_1"] = _deg(draw[i] * 100.0)
            loaded = i < 3
        else:
            # Wound down, held, then the arm comes over to the crossbar on
            # the blow and the stone is gone; the crew winds it back.
            arm = [-8.0, -14.0, -16.0, 95.0, 80.0, 30.0]
            s["throw_arm"] = _deg(arm[i])
            loaded = i < 3
            body["dz"] = 0.015 if i == 3 else 0.0
    elif anim in ("death", "decay"):
        t = i / float(count - 1) if anim == "death" else 1.0
        ease = t * t
        body["roll"] = _deg(75.0) * ease
        body["dx"] = -FALLEN_ENGINE * ease
        s["throw_arm"] = _deg(40.0) * t
        loaded = False
        if anim == "decay":
            k = (i + 1) / float(count)
            body["scale_z"] = 1.0 - 0.2 * k
    return body, s, loaded


# --------------------------------------------------------------------------
# Boats (`docs/07` D33): a hull facing +Y on the water line at z = 0.

# Each boat's hull: length, beam, height of the sides, and what it carries.
BOATS = {
    "fishing": (0.78, 0.30, 0.13),
    "transport": (1.25, 0.52, 0.17),
    "trade": (1.15, 0.48, 0.19),
    "archer": (1.20, 0.38, 0.16),
    "galley": (1.30, 0.42, 0.18),
    "catapult": (1.35, 0.50, 0.20),
}


def water_line(name, parent, z=0.002):
    """A holdout at the water line: whatever lies below it does not render,
    as if under the water the sprite is drawn over. It hides the shadow
    there too."""
    h = 4.0
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata([(-h, -h, 0.0), (h, -h, 0.0), (h, h, 0.0), (-h, h, 0.0)], [],
                     [(0, 1, 2, 3)])
    water = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(water)
    water.is_holdout = True
    water.location = (0.0, 0.0, z)
    water.parent = parent
    return water


class Boat:
    """A boat facing +Y, its keel on the water line: a planked hull tarred
    below, a deck, a mast and a sail of the owner's colour, and what its
    kind carries. "fishing": a net over the stern and one fisher; "transport":
    benches and a wide hull for passengers; "trade": bales and jars;
    "archer": archers along the rail; "galley": two banks of oars, shields
    of the owner's colour along the rail, a bronze ram and archers;
    "catapult": a catapult on deck. It rocks at rest, pulls its oars at the
    walk, looses (or throws) at the attack, and heels over and sinks."""

    def __init__(self, root_name, kind):
        self.root = empty(root_name)
        self.body = empty(root_name + "_body", parent=self.root)
        self.parts = {}
        self.oars = []
        self.kind = kind
        n = root_name
        add = self._add
        L, B, H = BOATS[kind]
        hl, hb = L / 2.0, B / 2.0
        # The hull: tarred at the water, planked above, a stem curling up at
        # the bow and a stern post; foam where it meets the water.
        add("foam", oval(n + "_foam", (hb * 0.88, hl * 0.95), (hb * 0.88, hl * 0.95), 0.006,
                         "foam"))
        add("keel", oval(n + "_keel", (hb * 0.72, hl * 0.84), (hb * 0.9, hl * 0.95), H * 0.45,
                         "pitch"))
        # The deck lies a little below the rail: its own top, nothing on it.
        add("strakes", oval(n + "_strakes", (hb * 0.9, hl * 0.95), (hb * 0.97, hl * 0.985),
                            H * 0.55 - 0.035, "wood", (0.0, 0.0, H * 0.45)))
        add("deck", oval(n + "_deck", (hb * 0.97, hl * 0.985), (hb * 0.97, hl * 0.985), 0.015,
                         "wood_dark", (0.0, 0.0, H - 0.035)))
        add("wale", oval_ring(n + "_wale", (hb * 1.02, hl * 1.01), (hb * 0.9, hl * 0.93), 0.05,
                              "wood_dark", (0.0, 0.0, H - 0.04)))
        add("stem", rod(n + "_stem", (0.0, hl * 0.92, H * 0.6), (0.0, hl * 1.08, H + 0.12),
                        0.025, "wood_dark", sides=6))
        add("stern", rod(n + "_sternpost", (0.0, -hl * 0.9, H * 0.6), (0.0, -hl * 1.02, H + 0.16),
                         0.025, "wood_dark", sides=6))
        # The mast, a yard and the sail, furled for the catapult's arm.
        mast_h = {"fishing": 0.5, "catapult": 0.62}.get(kind, 0.72)
        mast_y = {"catapult": hl * 0.35, "fishing": hl * 0.15}.get(kind, 0.0)
        add("mast", cylinder(n + "_mast", 0.018, mast_h, "wood", (0.0, mast_y, H), sides=6))
        sail_w = B * (1.3 if kind in ("transport", "trade") else 1.05)
        add("yard", beam(n + "_yard", (-sail_w / 2, mast_y + 0.02, H + mast_h * 0.92),
                         (sail_w / 2, mast_y + 0.02, H + mast_h * 0.92), 0.014, "wood_dark",
                         sides=6))
        sail_h = mast_h * (0.62 if kind != "catapult" else 0.3)
        sail = "linen" if kind in ("fishing", "trade") else "player"
        sail_z = H + mast_h * 0.92 - sail_h
        add("sail", box(n + "_sail", (sail_w, 0.012, sail_h), sail, (0.0, mast_y + 0.03, sail_z)))
        if kind != "catapult":
            # Reef bands across it, a rope at its foot.
            for k, f in enumerate((0.0, 0.36, 0.68)):
                add("reef%d" % k, box("%s_reef%d" % (n, k), (sail_w + 0.004, 0.018, 0.012), "rope",
                                      (0.0, mast_y + 0.03, sail_z + sail_h * f)))
        if sail == "linen":
            add("sail_band", box(n + "_sail_band", (sail_w + 0.004, 0.016, sail_h * 0.18), "player",
                                 (0.0, mast_y + 0.03, H + mast_h * 0.92 - sail_h * 0.55)))
        add("pennant", box(n + "_pennant", (0.012, 0.12, 0.05), "player",
                           (0.0, mast_y - 0.06, H + mast_h + 0.0)))
        if kind in ("transport", "galley", "archer"):
            # Oars, pivoting on the wale; two banks for the galley.
            banks = (0, 1) if kind == "galley" else (0,)
            count = {"transport": 4, "archer": 4, "galley": 5}[kind]
            for side in (-1.0, 1.0):
                for bank in banks:
                    for k in range(count):
                        y = -hl * 0.55 + (hl * 1.1) * k / max(count - 1, 1)
                        z = H - 0.02 - bank * 0.06
                        pivot = empty("%s_oarlock_%d_%d_%d" % (n, int(side), bank, k), parent=self.body,
                                      location=(side * hb * 0.98, y, z))
                        # Out from the rail and down to the water line,
                        # where the blade dips.
                        reach = side * (0.3 + bank * 0.06)
                        oar = beam("%s_oar_%d_%d_%d" % (n, int(side), bank, k), (0.0, 0.0, 0.0),
                                   (reach, 0.0, 0.01 - z), 0.013, "wood", sides=5)
                        oar.parent = pivot
                        blade_ = box("%s_blade_%d_%d_%d" % (n, int(side), bank, k),
                                     (0.09, 0.035, 0.008), "wood", (reach, 0.0, 0.006 - z),
                                     rotation=(0.0, side * math.atan2(z - 0.01, abs(reach)), 0.0))
                        blade_.parent = pivot
                        self.oars.append((pivot, side))
        if kind == "fishing":
            add("net", box(n + "_net", (B * 0.7, 0.16, 0.05), "rope", (0.0, -hl * 0.55, H)))
            self._crew(n + "_fisher", (0.0, -hl * 0.15, H), "linen")
        elif kind == "transport":
            for k, y in enumerate((-hl * 0.45, -hl * 0.1, hl * 0.25, hl * 0.6)):
                add("bench%d" % k, box("%s_bench%d" % (n, k), (B * 0.82, 0.05, 0.025), "wood",
                                       (0.0, y, H - 0.02)))
            self._crew(n + "_helm", (0.0, -hl * 0.8, H), "linen")
        elif kind == "trade":
            for k, (x, y) in enumerate(((-0.1, -0.3), (0.1, -0.25), (0.0, 0.25), (-0.1, 0.32))):
                add("bale%d" % k, box("%s_bale%d" % (n, k), (0.14, 0.14, 0.11), "linen" if k % 2 else "hide",
                                      (x, y * hl / 0.6, H - 0.02)))
            for k, y in enumerate((-0.05, 0.08)):
                add("jar%d" % k, ellipsoid("%s_jar%d" % (n, k), (0.05, 0.05, 0.08), "clay_roof",
                                           (0.12, y, H + 0.06)))
            self._crew(n + "_helm", (0.0, -hl * 0.8, H), "linen")
        elif kind in ("archer", "galley"):
            n_arch = 3 if kind == "archer" else 4
            for k in range(n_arch):
                y = -hl * 0.5 + hl * 1.0 * k / max(n_arch - 1, 1)
                side = -1.0 if k % 2 else 1.0
                self._crew("%s_archer%d" % (n, k), (side * hb * 0.5, y, H), "linen", archer=True)
            if kind == "galley":
                add("ram", cone(n + "_ram", 0.05, 0.16, "bronze", (0.0, hl * 1.02, H * 0.3)))
                self.parts["ram"].rotation_euler = (_deg(-90.0), 0.0, 0.0)
                for side in (-1.0, 1.0):
                    for k in range(5):
                        y = -hl * 0.6 + hl * 1.2 * k / 4
                        sh = round_shield("%s_shield_%d_%d" % (n, int(side), k), radius=0.06)
                        sh.parent = self.body
                        sh.location = (side * (hb + 0.01), y, H + 0.03)
                        sh.rotation_euler = (0.0, 0.0, _deg(90.0 * side))
        elif kind == "catapult":
            add("frame_l", box(n + "_frame_l", (0.05, 0.36, 0.2), "wood", (-0.12, 0.0, H)))
            add("frame_r", box(n + "_frame_r", (0.05, 0.36, 0.2), "wood", (0.12, 0.0, H)))
            add("skein", cylinder(n + "_skein", 0.06, 0.3, "rope", (0.0, -0.1, H + 0.1),
                                  sides=10, pivot="centre", rotation=(0.0, _deg(90.0), 0.0)))
            arm = add("throw_arm", limb(n + "_arm", 0.45, 0.035, 0.028, "wood",
                                        (0.0, -0.1, H + 0.1), rotation=(_deg(-100.0), 0.0, 0.0)))
            cup = ellipsoid(n + "_bucket", (0.07, 0.07, 0.04), "iron", (0.0, 0.0, -0.45), upper=True)
            cup.parent = arm
            stone = ellipsoid(n + "_stone", (0.045, 0.045, 0.045), "stone", (0.0, 0.0, -0.47))
            stone.parent = arm
            self.stone = stone
            self._crew(n + "_loader", (0.15, -hl * 0.3, H), "linen")
            self._crew(n + "_helm", (0.0, -hl * 0.85, H), "linen")

    def _add(self, key, obj, parent=None):
        obj.parent = parent if parent is not None else self.body
        self.parts[key] = obj
        return obj

    def _crew(self, name, at, coat, archer=False):
        """A sailor or an archer standing on the deck: legs, a tunic
        belted in the owner's colour, a head, and a bow if an archer."""
        x, y, z = at
        self._add(name + "_legs", oval(name + "_legs", (0.03, 0.025), (0.035, 0.03), 0.07, "skin",
                                       (x, y, z)))
        self._add(name + "_body", oval(name + "_coat", (0.045, 0.04), (0.05, 0.045), 0.11, coat,
                                       (x, y, z + 0.06)))
        self._add(name + "_belt", oval(name + "_belt", (0.049, 0.044), (0.049, 0.044), 0.025,
                                       "player", (x, y, z + 0.1)))
        self._add(name + "_head", ellipsoid(name + "_head", (0.035, 0.035, 0.04), "skin",
                                            (x, y, z + 0.2)))
        self._add(name + "_cap", ellipsoid(name + "_cap", (0.037, 0.037, 0.02), "bronze" if archer
                                           else "hair", (x, y, z + 0.225), upper=True))
        if archer:
            b = bow("%s_bow" % name)
            b.parent = self.body
            b.scale = (0.7, 0.7, 0.7)
            b.location = (x + 0.06, y + 0.04, z + 0.1)
            self.parts[name + "_bow"] = b

    def animate(self):
        scene = bpy.context.scene
        scene.frame_start, scene.frame_end = 1, 30
        rest = {k: (tuple(o.location), tuple(o.rotation_euler)) for k, o in self.parts.items()}
        stone = getattr(self, "stone", None)
        # The water, for a boat going down: it hides whatever has sunk.
        water = water_line(self.root.name + "_water", self.root)
        # What floats once it has gone: planks, a cask, the foam over it.
        self.wreck = []
        L, B, H = BOATS[self.kind]
        for k, (x, y, turn) in enumerate(((-0.12, 0.2, 0.4), (0.15, -0.05, -0.7),
                                          (-0.05, -0.28, 1.3), (0.2, 0.25, 2.0))):
            plank = box("%s_drift%d" % (self.root.name, k), (0.05, L * 0.28, 0.02), "wood",
                        (x, y, 0.002), rotation=(0.0, 0.0, turn))
            plank.parent = self.root
            self.wreck.append(plank)
        cask = cylinder(self.root.name + "_cask", 0.04, 0.05, "wood_dark", (0.05, 0.08, 0.002),
                        sides=10)
        cask.parent = self.root
        self.wreck.append(cask)
        for j in range(12):
            a = 2.0 * math.pi * j / 12
            r = 0.22
            bubble = ellipsoid("%s_swirl%d" % (self.root.name, j), (0.03, 0.03, 0.006), "foam",
                               (r * math.cos(a), r * math.sin(a), 0.002))
            bubble.parent = self.root
            self.wreck.append(bubble)
        for anim, (first, last) in MOBILE_SPANS.items():
            count = last - first + 1
            for i in range(count):
                frame = first + i
                body, sweep, swings, loaded = boat_pose(self.kind, anim, i, count)
                self.body.location = (0.0, 0.0, body["dz"])
                self.body.rotation_euler = (body["pitch"], body["roll"], 0.0)
                self.body.scale = (1.0, 1.0, body["scale_z"])
                for path in ("location", "rotation_euler", "scale"):
                    self.body.keyframe_insert(path, frame=frame)
                for key, obj in self.parts.items():
                    loc, rot = rest[key]
                    obj.location = loc
                    obj.rotation_euler = (rot[0] + swings.get(key, 0.0), rot[1], rot[2])
                    obj.keyframe_insert("location", frame=frame)
                    obj.keyframe_insert("rotation_euler", frame=frame)
                for pivot, side in self.oars:
                    # Trailing aft at rest, swept about that in the stroke.
                    pivot.rotation_euler = (0.0, 0.0, (sweep - OAR_RAKE) * side)
                    pivot.keyframe_insert("rotation_euler", frame=frame)
                water.hide_render = not body["under"]
                water.keyframe_insert("hide_render", frame=frame)
                for piece in self.wreck:
                    # Breaking up as it goes down, left on the water after.
                    piece.hide_render = not (anim == "decay"
                                             or (anim == "death" and i >= count // 2))
                    piece.keyframe_insert("hide_render", frame=frame)
                if stone is not None:
                    stone.hide_render = not loaded
                    stone.keyframe_insert("hide_render", frame=frame)
        hold_frames([self.body, water] + [p for p, _ in self.oars] + list(self.parts.values())
                    + self.wreck + ([stone] if stone is not None else []))


def boat_pose(kind, anim, i, count):
    """One frame of a boat: the hull's rise, roll and pitch, the oars'
    sweep, each part's swing about x (the catapult's arm), and whether the
    stone is in the bucket."""
    body = {"dz": 0.0, "roll": 0.0, "pitch": 0.0, "scale_z": 1.0, "under": False}
    s = {}
    sweep = 0.0
    loaded = True
    t = 2.0 * math.pi * i / count
    if anim == "idle":
        body["dz"] = 0.008 * math.sin(t)
        body["roll"] = _deg(2.0) * math.sin(t)
    elif anim == "walk":
        body["dz"] = 0.01 * math.sin(2.0 * t)
        body["pitch"] = _deg(2.5) * math.sin(t)
        sweep = _deg(28.0) * math.sin(t)
    elif anim == "attack":
        body["roll"] = _deg(1.5) * math.sin(t)
        if kind == "catapult":
            # The arm lies aft; it throws up over the top and past it.
            arm = [-8.0, -14.0, -16.0, -95.0, -80.0, -30.0]
            s["throw_arm"] = _deg(arm[i])
            loaded = i < 3
        else:
            body["dz"] = 0.006 * math.sin(t)
    elif anim == "death":
        # Heels over, the bow lifting, and goes down until the masthead and
        # the head of the sail are all that stand out of the water.
        k = i / float(count - 1)
        ease = k * k
        body["roll"] = _deg(30.0) * k
        body["pitch"] = _deg(12.0) * ease
        body["dz"] = -0.62 * ease
        body["under"] = True
        sweep = _deg(10.0)
        loaded = False
    elif anim == "decay":
        # Gone under: what floated off is left on the water.
        body["roll"] = _deg(30.0)
        body["pitch"] = _deg(12.0)
        body["dz"] = -2.0
        body["under"] = True
        loaded = False
    return body, sweep, s, loaded


# How far a boat's oars trail aft when it is not pulling them.
OAR_RAKE = _deg(22.0)


# --------------------------------------------------------------------------
# A grazing animal: the herd the villagers hunt.

class Animal:
    """A light four-legged animal, a gazelle's size (0.5 units nose to tail),
    facing +Y. Legs swing in diagonal pairs at the walk; at rest it grazes;
    its 'attack' is a head toss; it falls onto its side and slides back onto
    its own tile, where it lies as the carcass."""

    LEG = 0.30
    BACK = 0.46

    def __init__(self, root_name, coat="hide", belly="white", horns="horn"):
        self.root = empty(root_name)
        self.body = empty(root_name + "_body", parent=self.root)
        self.parts = {}
        leg, back = self.LEG, self.BACK
        add = self._add
        add("barrel", box("barrel", (0.16, 0.46, back - leg), coat, (0.0, 0.0, leg)))
        add("belly", box("belly", (0.14, 0.36, 0.04), belly, (0.0, 0.0, leg - 0.01)))
        add("neck", box("neck", (0.08, 0.10, 0.24), coat, (0.0, 0.20, back - 0.06), "bottom",
                        rotation=(_deg(-25.0), 0.0, 0.0)))
        add("head", box("head", (0.08, 0.16, 0.08), coat, (0.0, 0.33, back + 0.12)))
        add("horn_l", box("horn_l", (0.02, 0.02, 0.14), horns, (-0.03, 0.29, back + 0.19),
                          rotation=(_deg(-25.0), 0.0, 0.0)))
        add("horn_r", box("horn_r", (0.02, 0.02, 0.14), horns, (0.03, 0.29, back + 0.19),
                          rotation=(_deg(-25.0), 0.0, 0.0)))
        add("tail", box("tail", (0.03, 0.03, 0.10), belly, (0.0, -0.23, back - 0.02), "top",
                        rotation=(_deg(-30.0), 0.0, 0.0)))
        for key, x, y in (("leg_fl", -0.05, 0.17), ("leg_fr", 0.05, 0.17),
                          ("leg_bl", -0.05, -0.17), ("leg_br", 0.05, -0.17)):
            add(key, box(key, (0.035, 0.04, leg), coat, (x, y, leg), "top"))

    def _add(self, key, obj):
        obj.parent = self.body
        self.parts[key] = obj
        # A body is rounded, limbs, head and trunk all but capsules; what it
        # wears keeps its shape, or a cone helmet's rim turns into a brim.
        if key in BODY_PARTS:
            soften(obj, 0.42, 3, limit=0.06)
        return obj

    def animate(self):
        scene = bpy.context.scene
        scene.frame_start, scene.frame_end = 1, 30
        rest = {k: (tuple(o.location), tuple(o.rotation_euler)) for k, o in self.parts.items()}
        for anim, (first, last) in MOBILE_SPANS.items():
            count = last - first + 1
            for i in range(count):
                frame = first + i
                body, swings = animal_pose(anim, i, count)
                self.body.location = (body["dx"], 0.0, body["dz"])
                self.body.rotation_euler = (0.0, body["roll"], 0.0)
                for path in ("location", "rotation_euler"):
                    self.body.keyframe_insert(path, frame=frame)
                for key, obj in self.parts.items():
                    loc, rot = rest[key]
                    obj.location = loc
                    obj.rotation_euler = (rot[0] + swings.get(key, 0.0), rot[1], rot[2])
                    obj.keyframe_insert("location", frame=frame)
                    obj.keyframe_insert("rotation_euler", frame=frame)
        hold_frames([self.body] + list(self.parts.values()))


# Rolled onto its side, the animal's middle lies about this far along +X.
FALLEN_ANIMAL = 0.24


def animal_pose(anim, i, count):
    body = {"dx": 0.0, "dz": 0.0, "roll": 0.0}
    s = {}
    graze = ("neck", "head", "horn_l", "horn_r")
    if anim == "idle":
        # Head down to graze and up again.
        dip = _deg(45.0) * (0.5 - 0.5 * math.cos(2.0 * math.pi * i / count))
        for k in graze:
            s[k] = dip
        s["tail"] = _deg(10.0) * math.sin(2.0 * math.pi * i / count)
    elif anim == "walk":
        phase = 2.0 * math.pi * i / count
        a = _deg(30.0) * math.sin(phase)
        s["leg_fl"], s["leg_br"] = a, a
        s["leg_fr"], s["leg_bl"] = -a, -a
        body["dz"] = 0.02 * abs(math.sin(phase))
    elif anim == "attack":
        toss = [0.0, 20.0, 40.0, -30.0, -10.0, 0.0]
        for k in graze:
            s[k] = _deg(toss[i])
    else:
        t = i / float(count - 1) if anim == "death" else 1.0
        ease = t * t
        body["roll"] = _deg(85.0) * ease
        body["dx"] = -FALLEN_ANIMAL * ease
        for leg in ("leg_fl", "leg_fr", "leg_bl", "leg_br"):
            s[leg] = _deg(25.0) * t
        for k in graze:
            s[k] = _deg(30.0) * t
        if anim == "decay":
            body["dz"] = -0.01 * (i + 1)
    return body, s

"""The shared kit for the new look (docs/10, "the revamp", phase 0).

World: Blender units are tiles; the ground is the XY plane, Z up. A person
is 1.0 tall (the game's chunky scale: a house is 2 x 2 tiles). The camera
is the game's: orthographic, 30 degrees above the horizon, looking from the
(+X, -Y) corner, so screen-right is (+X, +Y) and far is (-X, +Y).

Everything is built with bmesh (no bpy.ops), so it runs headless:

    /usr/bin/python3.13 tools/look/preview.py greek.house out.png
"""
import math

import bpy  # before bmesh: the module build needs it loaded first
import bmesh  # noqa: E402
from mathutils import Matrix, Vector

# --- colours: sRGB 0..1, named for what they are -------------------------
# Period pigments and materials, pushed toward sunlit and saturated.
PAL = {
    "marble": (0.93, 0.90, 0.84), "limestone": (0.88, 0.80, 0.64),
    "sandstone": (0.83, 0.66, 0.42), "mudbrick": (0.71, 0.52, 0.36),
    "plaster": (0.95, 0.91, 0.80), "whitewash": (0.97, 0.96, 0.92),
    "terracotta": (0.80, 0.36, 0.20), "tile_dark": (0.62, 0.25, 0.15),
    "thatch": (0.78, 0.63, 0.30), "straw": (0.90, 0.78, 0.40),
    "wood": (0.52, 0.34, 0.20), "wood_dark": (0.33, 0.21, 0.13),
    "cedar": (0.62, 0.36, 0.22), "bronze": (0.80, 0.56, 0.24),
    "gold": (1.00, 0.76, 0.22), "iron": (0.42, 0.44, 0.48),
    "egyptian_blue": (0.10, 0.36, 0.78), "lapis": (0.12, 0.22, 0.62),
    "red_ochre": (0.74, 0.20, 0.14), "yellow_ochre": (0.90, 0.68, 0.20),
    "malachite": (0.12, 0.56, 0.36), "tyrian": (0.50, 0.12, 0.38),
    "linen": (0.94, 0.90, 0.80), "wool": (0.84, 0.76, 0.62),
    "leather": (0.50, 0.30, 0.16), "skin_light": (0.90, 0.68, 0.52),
    "skin_tan": (0.76, 0.52, 0.36), "skin_dark": (0.50, 0.32, 0.22),
    "hair_black": (0.10, 0.08, 0.07), "hair_brown": (0.30, 0.19, 0.11),
    "grass": (0.36, 0.62, 0.20), "grass_dry": (0.66, 0.66, 0.28),
    "leaf": (0.22, 0.50, 0.18), "leaf_olive": (0.44, 0.54, 0.32),
    "leaf_dark": (0.12, 0.34, 0.16), "palm": (0.30, 0.56, 0.20),
    "earth": (0.52, 0.36, 0.22), "sand": (0.92, 0.82, 0.58),
    "rock": (0.60, 0.57, 0.52), "water": (0.10, 0.52, 0.70),
    "water_deep": (0.04, 0.26, 0.52), "foam": (0.96, 0.98, 1.0),
    "wheat": (0.92, 0.76, 0.28), "flower_red": (0.90, 0.16, 0.14),
    "flower_yellow": (0.98, 0.84, 0.16), "flower_white": (0.98, 0.97, 0.94),
    "black": (0.05, 0.05, 0.06), "white": (0.98, 0.98, 0.98),
}
# Player colours for the picture: blue and red, saturated.
BLUE = (0.10, 0.34, 0.86)
RED = (0.84, 0.12, 0.10)


def _lin(c):
    """sRGB to linear, for shader inputs."""
    return tuple(((v + 0.055) / 1.055) ** 2.4 if v > 0.04045 else v / 12.92 for v in c)


_mats = {}


def mat(colour, rough=0.85, metal=0.0, emit=0.0, alpha=1.0, name=None):
    """A cached Principled material. `colour` is a PAL name or an sRGB tuple."""
    rgb = PAL[colour] if isinstance(colour, str) else tuple(colour)
    key = (rgb, rough, metal, emit, alpha)
    if key in _mats:
        return _mats[key]
    m = bpy.data.materials.new(name or (colour if isinstance(colour, str) else "c"))
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (*_lin(rgb), 1.0)
    b.inputs["Roughness"].default_value = rough
    b.inputs["Metallic"].default_value = metal
    if emit:
        b.inputs["Emission Color"].default_value = (*_lin(rgb), 1.0)
        b.inputs["Emission Strength"].default_value = emit
    if alpha < 1.0:
        b.inputs["Alpha"].default_value = alpha
    _mats[key] = m
    return m


def _as_mat(m):
    return m if isinstance(m, bpy.types.Material) else mat(m)


def _obj(bm, name, material, loc, rot, smooth, parent, bevel):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    if smooth:
        for p in me.polygons:
            p.use_smooth = True
        me.set_sharp_from_angle(angle=math.radians(40))
    ob = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(ob)
    if material is not None:
        me.materials.append(_as_mat(material))
    ob.location = loc
    ob.rotation_euler = rot
    if bevel:
        mod = ob.modifiers.new("bevel", "BEVEL")
        mod.width = bevel
        mod.segments = 2
        mod.limit_method = "ANGLE"
    if parent is not None:
        ob.parent = parent
    return ob


def empty(name="root", loc=(0, 0, 0), rot_z=0.0, parent=None):
    """A parent for an asset's parts; build under it, then move/turn it."""
    e = bpy.data.objects.new(name, None)
    bpy.context.scene.collection.objects.link(e)
    e.location = loc
    e.rotation_euler = (0, 0, rot_z)
    if parent is not None:
        e.parent = parent
    return e


def box(size, loc=(0, 0, 0), m="plaster", rot=(0, 0, 0), parent=None, bevel=0.0,
        name="box"):
    """A box `size` = (x, y, z), centred on `loc`."""
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=Vector(size), verts=bm.verts)
    return _obj(bm, name, m, loc, rot, False, parent, bevel)


def cyl(r, h, loc=(0, 0, 0), m="marble", rot=(0, 0, 0), parent=None, r_top=None,
        segs=16, bevel=0.0, name="cyl"):
    """A cylinder or cone frustum along Z, centred on `loc`; r_top=0 is a cone."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=segs, radius1=r,
                          radius2=r if r_top is None else r_top, depth=h)
    return _obj(bm, name, m, loc, rot, True, parent, bevel)


def sphere(r, loc=(0, 0, 0), m="leaf", scale=(1, 1, 1), parent=None, rot=(0, 0, 0),
           segs=16, name="sphere"):
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=segs, v_segments=max(8, segs // 2), radius=r)
    bmesh.ops.scale(bm, vec=Vector(scale), verts=bm.verts)
    return _obj(bm, name, m, loc, rot, True, parent, 0.0)


def blob(r, loc=(0, 0, 0), m="rock", scale=(1, 1, 1), parent=None, seed=0, rough=0.25,
         subdiv=2, name="blob"):
    """A lumpy icosphere: rocks, foliage clumps, bushes."""
    import random
    rnd = random.Random(seed)
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdiv, radius=r)
    for v in bm.verts:
        v.co *= 1.0 + rnd.uniform(-rough, rough)
    bmesh.ops.scale(bm, vec=Vector(scale), verts=bm.verts)
    return _obj(bm, name, m, loc, (0, 0, rnd.uniform(0, 6.28)), False, parent, 0.0)


def prism(size, loc=(0, 0, 0), m="terracotta", rot=(0, 0, 0), parent=None,
          overhang=0.0, name="prism"):
    """A gable roof: base (x, y), height z, ridge along X, base centred on `loc`
    (the base sits at loc.z, the ridge at loc.z + size.z)."""
    x, y, z = size[0] / 2 + overhang, size[1] / 2 + overhang, size[2]
    bm = bmesh.new()
    v = [bm.verts.new(p) for p in ((-x, -y, 0), (x, -y, 0), (x, y, 0), (-x, y, 0),
                                   (-x, 0, z), (x, 0, z))]
    for f in ((0, 1, 5, 4), (2, 3, 4, 5), (0, 4, 3), (1, 2, 5), (3, 2, 1, 0)):
        bm.faces.new([v[i] for i in f])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _obj(bm, name, m, loc, rot, False, parent, 0.0)


def pyramid(base, h, loc=(0, 0, 0), m="limestone", parent=None, top=0.0, name="pyramid"):
    """A square pyramid (or frustum with `top` > 0), base centred at loc.z."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=4, radius1=base / math.sqrt(2),
                          radius2=top / math.sqrt(2), depth=h)
    bmesh.ops.rotate(bm, cent=(0, 0, 0), matrix=Matrix.Rotation(math.pi / 4, 3, "Z"),
                     verts=bm.verts)
    bmesh.ops.translate(bm, vec=(0, 0, h / 2), verts=bm.verts)
    return _obj(bm, name, m, loc, (0, 0, 0), False, parent, 0.0)


def mesh(verts, faces, m="plaster", loc=(0, 0, 0), rot=(0, 0, 0), parent=None,
         smooth=False, name="mesh"):
    """Anything else: vertices and faces (index tuples)."""
    bm = bmesh.new()
    vs = [bm.verts.new(p) for p in verts]
    for f in faces:
        bm.faces.new([vs[i] for i in f])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _obj(bm, name, m, loc, rot, smooth, parent, 0.0)


# --- the scene -----------------------------------------------------------

def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    _mats.clear()


def camera(centre=(0, 0, 0), width_tiles=12.0, res=(1600, 1000)):
    """The game's camera, `width_tiles` tile-diagonals of ground across."""
    sc = bpy.context.scene
    cam = bpy.data.cameras.new("cam")
    cam.type = "ORTHO"
    cam.ortho_scale = width_tiles * math.sqrt(2)
    cam.clip_start, cam.clip_end = 0.1, 400
    ob = bpy.data.objects.new("cam", cam)
    sc.collection.objects.link(ob)
    el = math.radians(30)
    d = Vector((math.cos(el) / math.sqrt(2), -math.cos(el) / math.sqrt(2), math.sin(el)))
    ob.location = Vector(centre) + d * 100
    ob.rotation_euler = (-d).to_track_quat("-Z", "Y").to_euler()
    sc.camera = ob
    sc.render.resolution_x, sc.render.resolution_y = res
    return ob


def light(sun_strength=3.4, sky_strength=0.55):
    """A warm afternoon sun from the upper left of the screen, a blue sky
    filling the shadows."""
    sc = bpy.context.scene
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = sun_strength
    sun.color = (1.0, 0.93, 0.80)
    sun.angle = math.radians(2.5)
    ob = bpy.data.objects.new("sun", sun)
    sc.collection.objects.link(ob)
    # From the viewer's left shoulder: the screen-left faces (-Y) are fully lit,
    # the screen-right faces (+X) glancingly, and shadows fall away to the right.
    ob.rotation_euler = Vector((-0.30, 0.80, -0.80)).to_track_quat("-Z", "Y").to_euler()
    w = bpy.data.worlds.new("sky")
    w.use_nodes = True
    bg = w.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.62, 0.76, 1.0, 1.0)
    bg.inputs["Strength"].default_value = sky_strength
    sc.world = w


def render(path, samples=48):
    sc = bpy.context.scene
    sc.render.engine = "CYCLES"
    sc.cycles.device = "CPU"
    sc.cycles.samples = samples
    sc.cycles.use_denoising = True
    sc.cycles.max_bounces = 3
    sc.render.film_transparent = False
    sc.view_settings.view_transform = "Standard"
    sc.view_settings.look = "None"
    sc.render.image_settings.file_format = "PNG"
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)

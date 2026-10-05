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

def villager():
    """The owner's tunic, a stone hatchet, and a tool or a load for each job
    (kit.Villager)."""
    v = kit.Villager("Villager")
    v.animate()
    return v.root


def spearman():
    h = kit.Humanoid("Spearman", helmet="cone")
    h.hold(kit.spear("spear"))
    h.carry_shield(kit.round_shield("shield", face="hide"))
    h.animate("thrust")
    return h.root


def clubman():
    """Stone Age: bare-headed, a hide kilt, a club."""
    h = kit.Humanoid("Clubman", beard=True)
    h.hold(kit.club("club"), follows=True)
    h.animate("swing")
    return h.root


def axeman():
    """The clubman's line upgraded: a bronze cap and a bronze axe."""
    h = kit.Humanoid("Axeman", helmet="cap", helmet_mat="bronze")
    h.hold(kit.axe("axe"), follows=True)
    h.animate("swing")
    return h.root


def slinger():
    """A sling whirled overhead; a headband and no armour to slow him."""
    h = kit.Humanoid("Slinger", helmet="band")
    h.hold(kit.sling("sling"))
    h.animate("sling")
    return h.root


def bowman():
    """A bow held out in the left hand, drawn with the right."""
    h = kit.Humanoid("Bowman", helmet="cap", helmet_mat="linen")
    h.hold(kit.bow("bow"), hand="left", lean=0.0)
    h.wear("quiver", kit.quiver("quiver"))
    h.animate("shoot")
    return h.root


def scout():
    """A light rider bare-headed on a pale horse, a javelin in hand."""
    r = kit.Rider("Scout", coat="straw", helmet=None)
    r.hold(kit.javelin("javelin"))
    r.animate()
    return r.root


def light_cavalry():
    """A bronze-helmeted rider on a bay horse with a lance."""
    r = kit.Rider("LightCavalry", coat="horse", helmet="cone")
    r.hold(kit.lance("lance"))
    r.animate()
    return r.root


def swordsman():
    """Bronze Age line infantry: a bronze cap, a leather cuirass, a round
    shield of the owner's colour and a short broad sword."""
    h = kit.Humanoid("Swordsman", helmet="cap", helmet_mat="bronze")
    kit.cuirass(h, "hide")
    h.hold(kit.sword("sword"), follows=True)
    h.carry_shield(kit.round_shield("shield", radius=0.12))
    h.animate("swing")
    return h.root


def hoplite():
    """The Academy's heavy infantry: a crested bronze helmet, a bronze
    breastplate and greaves, a long spear and the great round shield."""
    h = kit.Humanoid("Hoplite", helmet="crest", helmet_mat="bronze")
    kit.cuirass(h, "bronze")
    kit.greaves(h, "bronze")
    h.hold(kit.spear("spear", length=1.2))
    h.carry_shield(kit.round_shield("aspis", radius=0.17))
    h.animate("thrust")
    return h.root


def legionary():
    """The Hoplite line's last tier: an iron helmet with a crest, iron
    mail, a tall curved shield and an iron sword."""
    h = kit.Humanoid("Legionary", helmet="crest", helmet_mat="iron")
    kit.cuirass(h, "iron")
    h.hold(kit.sword("gladius", metal="iron"), follows=True)
    h.carry_shield(kit.tower_shield("scutum"))
    h.animate("swing")
    return h.root


def chariot_archer():
    """A bowman standing in a two-wheeled car behind a bay horse."""
    c = kit.Chariot("ChariotArcher")
    c.animate()
    return c.root


def horse_archer():
    """A light rider on a pale horse with a bow and a quiver at the hip."""
    r = kit.Rider("HorseArcher", coat="straw", helmet="cap", helmet_mat="hide")
    r.hold(kit.bow("bow"), lean=0.0)
    r.animate()
    return r.root


def heavy_cavalry():
    """The shock rider: a dark horse in a bronze peytral and a caparison of
    the owner's colour, a crested helmet, a breastplate, a heavy lance."""
    r = kit.Rider("HeavyCavalry", coat="horse_dark", mane="hair", helmet="crest",
                  helmet_mat="bronze", barding="bronze", cuirass="bronze")
    r.hold(kit.spear("lance", length=1.25))
    r.animate()
    return r.root


def war_elephant():
    """A war elephant with a howdah of the owner's colour and its driver."""
    e = kit.Elephant("WarElephant")
    e.animate()
    return e.root


def stone_thrower():
    """A torsion engine on four wheels that flings a stone from a sling."""
    e = kit.Engine("StoneThrower", "thrower")
    e.animate()
    return e.root


def catapult():
    """The Stone Thrower made larger and ironbound, a bucket for the stone."""
    e = kit.Engine("Catapult", "catapult")
    e.animate()
    return e.root


def ballista():
    """A great crossbow on a two-wheeled carriage, a shield of the owner's
    colour across its front."""
    e = kit.Engine("Ballista", "ballista")
    e.animate()
    return e.root


def priest():
    """A long robe of undyed linen with a stole of the owner's colour down
    its front, shaven and bearded, a tall staff with a bronze head; at its
    chant it lifts its hands (kit's "bless")."""
    h = kit.Humanoid("Priest", tunic="linen", dress="robe", hair=False, beard=True)
    h.wear("stole", kit.box("Priest_stole", (0.05, 0.02, 0.34), "player",
                            (0.0, 0.082, kit.HIP_Z - 0.06)))
    h.hold(kit.staff("staff"))
    h.animate("bless")
    return h.root


def relic():
    """A gilded casket on a low stone plinth, the poles it is carried by
    along its sides, two small figures kneeling on its lid."""
    root = kit.empty("Relic")
    parts = [
        kit.box("relic_plinth", (0.4, 0.32, 0.06), "stone_light"),
        kit.box("relic_chest", (0.27, 0.18, 0.16), "gold", (0.0, 0.0, 0.06)),
        kit.box("relic_band", (0.282, 0.19, 0.03), "wood_dark", (0.0, 0.0, 0.12)),
        kit.box("relic_lid", (0.3, 0.21, 0.03), "gold", (0.0, 0.0, 0.22)),
    ]
    for side, y in (("l", -0.11), ("r", 0.11)):
        parts.append(kit.cylinder("relic_pole_" + side, 0.013, 0.48, "wood",
                                  (-0.24, y, 0.13), sides=8,
                                  rotation=(0.0, kit._deg(90.0), 0.0)))
    for k, x in enumerate((-0.075, 0.075)):
        parts.append(kit.clump("relic_figure_%d" % k, 0.05, "gold", (x, 0.0, 0.28),
                               seed=90 + k, lumps=0.25, squash=1.3))
    for p in parts:
        p.parent = root
    return root


def wonder():
    """The Iron Age's monument: a ziggurat of dressed stone in three great
    steps with a stair up its face, a white shrine ringed with columns on
    its top under a gilded roof, and the owner's colour on banners at the
    corners of its lowest step. Iron Age only, so built in the Iron Age's
    materials whatever the set's name."""
    kit.STYLE_AGE = 3
    b = kit.Building("Wonder", 5)
    kit.foundation(b, "wonder", 4.7, 4.7)
    z = 0.0
    tiers = [(4.4, 0.5, kit.WALLS), (3.3, 0.45, kit.FULL_WALLS), (2.25, 0.4, kit.FINISHED)]
    for k, (w, h, frames) in enumerate(tiers):
        b.add(kit.box("wonder_tier%d" % k, (w, w, h), "stone_light", (0.0, 0.0, z)), frames)
        # A band of white stone just under each step's edge; not level
        # with the step's top, where two faces would fight.
        b.add(kit.box("wonder_cornice%d" % k, (w + 0.08, w + 0.08, 0.06), "white",
                      (0.0, 0.0, z + h - 0.1)), frames)
        z += h
    # The stair up the +Y face, in flights between the steps.
    for k in range(9):
        y = 2.2 - k * 0.13
        b.add(kit.box("wonder_stair%d" % k, (0.7, 0.16, 0.15 * (k + 1)), "stone",
                      (0.0, y, 0.0)), kit.FULL_WALLS)
    # The shrine.
    top = z
    b.add(kit.box("wonder_cella", (1.3, 1.3, 0.8), "white", (0.0, 0.0, top)), kit.FINISHED)
    import math as _m
    for i in range(16):
        a = 2 * _m.pi * i / 16
        b.add(kit.cylinder("wonder_col_%d" % i, 0.06, 0.8, "white",
                           (0.92 * _m.cos(a), 0.92 * _m.sin(a), top), sides=8), kit.FINISHED)
    b.add(kit.box("wonder_entablature", (2.05, 2.05, 0.12), "white", (0.0, 0.0, top + 0.8)),
          kit.FINISHED)
    b.add(kit.pyramid("wonder_roof", (2.15, 2.15, 0.7), "gold", (0.0, 0.0, top + 0.92)),
          kit.FINISHED)
    # The owner's colour: banners on poles at the lowest step's corners, and
    # a band along the shrine's frieze.
    for i, (x, y) in enumerate([(2.1, 2.1), (2.1, -2.1), (-2.1, 2.1)]):
        b.add(kit.cylinder("wonder_pole_%d" % i, 0.02, 1.1, "wood", (x, y, 0.5), sides=6),
              kit.FINISHED)
        b.add(kit.box("wonder_banner_%d" % i, (0.03, 0.32, 0.42), "player",
                      (x, y - 0.17, 1.12)), kit.FINISHED)
    b.add(kit.box("wonder_frieze", (2.07, 2.07, 0.05), "player", (0.0, 0.0, top + 0.83)),
          kit.FINISHED)
    kit.scaffold(b, "wonder", 3.4, 3.4, 1.4)
    kit.rubble(b, "wonder", 3.6, 0.7, mats=("stone_light", "white", "stone"), count=16)
    b.finish()
    return b.root


# --------------------------------------------------------------------------
# Buildings.

def _wonder_banners(b, mats=("wood",)):
    """The owner's banners on poles at three corners of a Wonder's court,
    as the Greek's (`wonder`)."""
    for i, (x, y) in enumerate([(2.1, 2.1), (2.1, -2.1), (-2.1, 2.1)]):
        b.add(kit.cylinder("wonder_pole_%d" % i, 0.02, 1.1, mats[0], (x, y, 0.1), sides=6),
              kit.FINISHED)
        b.add(kit.box("wonder_banner_%d" % i, (0.03, 0.32, 0.42), "player",
                      (x, y - 0.17, 0.72)), kit.FINISHED)


def wonder_egyptian():
    """The Egyptian's monument: a pyramid cased in white limestone on a
    sandstone court, its capstone gilt, two obelisks before it, and the
    owner's colour on banners and along the court's edge. It rises in
    courses of sandstone, cased only when finished."""
    import math as _m
    kit.STYLE_AGE = 3
    b = kit.Building("WonderEgyptian", 5)
    b.restyle = False
    kit.foundation(b, "wonder", 4.7, 4.7, "sandstone")
    b.add(kit.box("wonder_court", (4.5, 4.5, 0.1), "sandstone"), kit.WALLS)
    px, py, base, height, z0 = -0.35, -0.35, 3.2, 2.5, 0.1
    steps = 6
    for k in range(steps):
        t = k / steps
        frames = (2, 3) if k < 2 else (3,)
        b.add(kit.box("wonder_core%d" % k, (base * (1 - t), base * (1 - t), height / steps),
                      "sandstone", (px, py, z0 + height * t)), frames)
    b.add(kit.pyramid("wonder_pyramid", (base, base, height), "limestone", (px, py, z0)),
          kit.FINISHED)
    # A touch proud of the casing, so the two faces do not fight.
    cap = 0.13
    b.add(kit.pyramid("wonder_capstone", (base * cap, base * cap, height * cap), "gold",
                      (px, py, z0 + height * (1 - cap) - 0.004)), kit.FINISHED)
    for i, (x, y) in enumerate([(1.8, 0.75), (0.75, 1.8)]):
        b.add(kit.box("wonder_obelisk_base%d" % i, (0.34, 0.34, 0.14), "sandstone",
                      (x, y, z0)), kit.FINISHED)
        b.add(kit.frustum("wonder_obelisk%d" % i, (0.17, 0.17), (0.11, 0.11), 1.25, "stone",
                          (x, y, z0 + 0.14)), kit.FINISHED)
        b.add(kit.pyramid("wonder_obelisk_tip%d" % i, (0.115, 0.115, 0.1), "gold",
                          (x, y, z0 + 1.39)), kit.FINISHED)
    for i, (size, at) in enumerate([((0.03, 4.4, 0.05), (2.26, 0.0, 0.03)),
                                    ((4.4, 0.03, 0.05), (0.0, 2.26, 0.03))]):
        b.add(kit.box("wonder_band%d" % i, size, "player", at), kit.FINISHED)
    _wonder_banners(b)
    kit.scaffold(b, "wonder", 3.4, 3.4, 1.4)
    kit.rubble(b, "wonder", 3.6, 0.7, mats=("sandstone", "limestone", "stone"), count=16)
    b.finish()
    return b.root


def wonder_mesopotamian():
    """The Mesopotamian's monument: a ziggurat of baked brick in three
    buttressed, crenellated tiers, a stair up its face, and a shrine
    glazed blue with gold rosettes on its top; the owner's colour on
    banners and along the lowest tier."""
    kit.STYLE_AGE = 3
    b = kit.Building("WonderMesopotamian", 5)
    b.restyle = False
    kit.foundation(b, "wonder", 4.7, 4.7, "baked_brick")
    z = 0.0
    tiers = [(4.2, 0.55, kit.WALLS), (3.1, 0.5, kit.FULL_WALLS), (2.1, 0.45, kit.FINISHED)]
    for k, (w, h, frames) in enumerate(tiers):
        b.add(kit.box("wonder_tier%d" % k, (w, w, h), "baked_brick", (0.0, 0.0, z)), frames)
        n = max(2, int(w / 0.35))
        for i in range(n + 1):
            t = -w / 2 + w * i / n
            b.add(kit.box("wonder_buttress_y%d_%d" % (k, i), (0.1, 0.05, h), "baked_brick",
                          (t, w / 2 + 0.02, z)), frames)
            b.add(kit.box("wonder_buttress_x%d_%d" % (k, i), (0.05, 0.1, h), "baked_brick",
                          (w / 2 + 0.02, t, z)), frames)
        kit._merlons(b, "wonder_crenel%d" % k, (0.0, 0.0, z + h), w, w, "baked_brick",
                     tuple(f for f in frames if f >= 3) or frames, stepped=True, size=0.09)
        z += h
    for k in range(10):
        b.add(kit.box("wonder_stair%d" % k, (0.6, 0.16, 0.13 * (k + 1)), "baked_brick",
                      (0.0, 2.2 - k * 0.14, 0.0)), kit.FULL_WALLS)
    top = z
    b.add(kit.box("wonder_shrine", (1.1, 1.1, 0.6), "glaze", (0.0, 0.0, top)), kit.FINISHED)
    b.add(kit.box("wonder_shrine_door", (0.26, 0.02, 0.38), "opening", (0.0, 0.555, top)),
          kit.FINISHED)
    for i, t in enumerate([-0.35, 0.0, 0.35]):
        b.add(kit.cylinder("wonder_rosette_y%d" % i, 0.05, 0.02, "gold", (t, 0.56, top + 0.48),
                           sides=10, pivot="centre", rotation=(kit._deg(90.0), 0.0, 0.0)),
              kit.FINISHED)
        b.add(kit.cylinder("wonder_rosette_x%d" % i, 0.05, 0.02, "gold", (0.56, t, top + 0.48),
                           sides=10, pivot="centre", rotation=(0.0, kit._deg(90.0), 0.0)),
              kit.FINISHED)
    kit._merlons(b, "wonder_shrine_crenel", (0.0, 0.0, top + 0.6), 1.1, 1.1, "glaze",
                 kit.FINISHED, stepped=True, size=0.08)
    for i, (size, at) in enumerate([((0.03, 4.0, 0.06), (2.13, 0.0, 0.2)),
                                    ((4.0, 0.03, 0.06), (0.0, 2.13, 0.2))]):
        b.add(kit.box("wonder_band%d" % i, size, "player", at), kit.FINISHED)
    _wonder_banners(b)
    kit.scaffold(b, "wonder", 3.4, 3.4, 1.4)
    kit.rubble(b, "wonder", 3.6, 0.7, mats=("baked_brick", "glaze", "mudbrick"), count=16)
    b.finish()
    return b.root


def wonder_asian():
    """The East Asian's monument: a great hall of red-lacquered posts and
    white walls on two terraces of rammed earth, under two tiers of dark
    tiled roof with gilt horns, gate towers either side of its stair, and
    the owner's colour on banners and along the hall's frieze."""
    kit.STYLE_AGE = 3
    b = kit.Building("WonderAsian", 5)
    b.restyle = False
    kit.foundation(b, "wonder", 4.7, 4.7, "stone")
    b.add(kit.box("wonder_terrace0", (4.3, 4.3, 0.35), "rammed"), kit.WALLS)
    b.add(kit.box("wonder_terrace0_lip", (4.36, 4.36, 0.05), "stone", (0.0, 0.0, 0.32)),
          kit.WALLS)
    b.add(kit.box("wonder_terrace1", (3.2, 3.2, 0.35), "rammed", (0.0, 0.0, 0.35)),
          kit.FULL_WALLS)
    b.add(kit.box("wonder_terrace1_lip", (3.26, 3.26, 0.05), "stone", (0.0, 0.0, 0.67)),
          kit.FULL_WALLS)
    for k in range(8):
        b.add(kit.box("wonder_stair%d" % k, (0.7, 0.16, 0.09 * (k + 1)), "stone",
                      (0.0, 2.15 - k * 0.14, 0.0)), kit.FULL_WALLS)
    hz, hw, hd, hh = 0.72, 2.3, 1.7, 0.75
    b.add(kit.box("wonder_hall", (hw, hd, hh), "plaster", (0.0, 0.0, hz)), kit.FINISHED)
    for i in range(7):
        x = -hw / 2 + hw * i / 6
        b.add(kit.box("wonder_post_y%d" % i, (0.08, 0.05, hh), "lacquer", (x, hd / 2 + 0.02, hz)),
              kit.FINISHED)
        b.add(kit.box("wonder_bracket_y%d" % i, (0.14, 0.08, 0.06), "wood_dark",
                      (x, hd / 2 + 0.04, hz + hh - 0.06)), kit.FINISHED)
    for i in range(5):
        y = -hd / 2 + hd * i / 4
        b.add(kit.box("wonder_post_x%d" % i, (0.05, 0.08, hh), "lacquer", (hw / 2 + 0.02, y, hz)),
              kit.FINISHED)
        b.add(kit.box("wonder_bracket_x%d" % i, (0.08, 0.14, 0.06), "wood_dark",
                      (hw / 2 + 0.04, y, hz + hh - 0.06)), kit.FINISHED)
    b.add(kit.box("wonder_door", (0.36, 0.02, 0.5), "lacquer", (0.0, hd / 2 + 0.01, hz)),
          kit.FINISHED)
    b.add(kit.box("wonder_frieze_y", (hw - 0.1, 0.03, 0.08), "player",
                  (0.0, hd / 2 + 0.05, hz + hh - 0.18)), kit.FINISHED)
    b.add(kit.box("wonder_frieze_x", (0.03, hd - 0.1, 0.08), "player",
                  (hw / 2 + 0.05, 0.0, hz + hh - 0.18)), kit.FINISHED)
    lower, _ = kit.hip_roof("wonder_eave", 3.1, 2.5, 0.4, "tile_dark", (0.0, 0.0, hz + hh),
                            flare=0.14)
    b.add(lower, kit.FINISHED)
    uz = hz + hh + 0.18
    b.add(kit.box("wonder_upper", (1.5, 1.05, 0.42), "plaster", (0.0, 0.0, uz)), kit.FINISHED)
    for i in range(4):
        x = -0.75 + 1.5 * i / 3
        b.add(kit.box("wonder_upper_post%d" % i, (0.06, 0.04, 0.42), "lacquer",
                      (x, 0.535, uz)), kit.FINISHED)
    upper, _ = kit.hip_roof("wonder_roof", 2.3, 1.75, 0.6, "tile_dark", (0.0, 0.0, uz + 0.42),
                            flare=0.16)
    b.add(upper, kit.FINISHED)
    r = (2.3 - 1.75) / 2
    b.add(kit.box("wonder_ridge", (2 * r + 0.12, 0.08, 0.07), "tile_dark",
                  (0.0, 0.0, uz + 0.42 + 0.57)), kit.FINISHED)
    for i, x in enumerate((-r, r)):
        b.add(kit.cone("wonder_horn%d" % i, 0.06, 0.2, "gold", (x, 0.0, uz + 0.42 + 0.62),
                       sides=6), kit.FINISHED)
    for i, x in enumerate((-0.75, 0.75)):
        b.add(kit.box("wonder_que%d" % i, (0.34, 0.34, 1.0), "rammed", (x, 1.95, 0.0)),
              kit.FINISHED)
        b.add(kit.box("wonder_que_band%d" % i, (0.36, 0.36, 0.08), "lacquer", (x, 1.95, 0.85)),
              kit.FINISHED)
        cap, _ = kit.hip_roof("wonder_que_roof%d" % i, 0.5, 0.5, 0.22, "tile_dark",
                              (x, 1.95, 1.0), flare=0.06)
        b.add(cap, kit.FINISHED)
    _wonder_banners(b)
    kit.scaffold(b, "wonder", 3.4, 3.4, 1.6)
    kit.rubble(b, "wonder", 3.6, 0.7, mats=("rammed", "plaster", "tile_dark", "lacquer"),
               count=16)
    b.finish()
    return b.root


def fishing_boat():
    """A small open boat with a linen sail banded in the owner's colour, a
    net heaped over the stern and its fisher."""
    b = kit.Boat("FishingBoat", "fishing")
    b.animate()
    return b.root


def transport():
    """A broad, deep hull with benches for its passengers, a square sail of
    the owner's colour and four oars a side."""
    b = kit.Boat("Transport", "transport")
    b.animate()
    return b.root


def trade_boat():
    """A merchantman: a deep hull laden with bales and jars under a linen
    sail banded in the owner's colour."""
    b = kit.Boat("TradeBoat", "trade")
    b.animate()
    return b.root


def archer_ship():
    """A light ship of archers, the Tool Age's: a sail of the owner's
    colour, four oars a side, three archers at the rail."""
    b = kit.Boat("ArcherShip", "archer")
    b.animate()
    return b.root


def war_galley():
    """The Bronze Age's warship: a longer hull, two banks of oars, shields
    of the owner's colour along the rail, a bronze ram and four archers."""
    b = kit.Boat("WarGalley", "galley")
    b.animate()
    return b.root


def catapult_ship():
    """The Iron Age's warship: a heavy hull with a catapult on its deck, the
    sail furled to the yard to clear the arm."""
    b = kit.Boat("CatapultShip", "catapult")
    b.animate()
    return b.root


def fish():
    """A shoal breaking the water: three backs with their fins and tails out,
    one fish leaping clear, and rings of ripples round them, silver over
    dark."""
    import math as _m
    root = kit.empty("Fish")
    kit.water_line("fish_water", root)
    parts = []

    def one(k, x, y, turn, scale, z, nose_up):
        body = kit.ellipsoid("fish_body_%d" % k, (0.055 * scale, 0.17 * scale, 0.05 * scale),
                             "fish", (0.0, 0.0, 0.0))
        back = kit.ellipsoid("fish_back_%d" % k, (0.035 * scale, 0.14 * scale, 0.03 * scale),
                             "fish_dark", (0.0, 0.0, 0.03 * scale))
        fin = kit.blade("fish_fin_%d" % k, 0.07 * scale, 0.07 * scale, 0.008, "fish_dark",
                        (0.0, 0.0, 0.04 * scale))
        fin.rotation_euler = (0.0, 0.0, _m.pi / 2)
        holder = kit.empty("fish_%d" % k, parent=root, location=(x, y, z))
        holder.rotation_euler = (nose_up, 0.0, turn)
        for p in (body, back, fin):
            p.parent = holder
        for side in (-1.0, 1.0):
            lobe = kit.blade("fish_tail_%d_%d" % (k, int(side)), 0.06 * scale, 0.1 * scale, 0.008,
                             "fish_dark", (0.0, -0.15 * scale, 0.0))
            lobe.rotation_euler = (_m.radians(-90.0 - 35.0 * side), 0.0, _m.pi / 2)
            lobe.parent = holder

    def ripples(k, x, y, radii):
        for j, r in enumerate(radii):
            ring = kit.oval_ring("fish_ripple_%d_%d" % (k, j), (r, r * 0.92), (r - 0.014, r * 0.92 - 0.014),
                                 0.004, "foam", (x, y, 0.002), sides=24)
            ring.parent = root

    # Backs awash: their middles at the water line.
    for k, (x, y, turn, scale) in enumerate([(-0.16, 0.06, 0.5, 1.0), (0.14, -0.1, -1.0, 0.9),
                                             (0.04, 0.2, 2.3, 0.8)]):
        one(k, x, y, turn, scale, 0.0, 0.0)
        ripples(k, x, y, (0.2 * scale, 0.27 * scale))
    # And one in the air over its splash.
    one(3, 0.02, -0.12, -2.4, 0.85, 0.16, _m.radians(-30.0))
    ripples(3, 0.0, -0.06, (0.08, 0.13))
    for j in range(8):
        a = 2 * _m.pi * j / 8
        drop = kit.ellipsoid("fish_splash_%d" % j, (0.012, 0.012, 0.02), "foam",
                             (0.1 * _m.cos(a), -0.06 + 0.09 * _m.sin(a), 0.03))
        drop.parent = root
    return root


def dock():
    """A Dock on its piles in the water: a deck of planks, mooring posts
    along its edges, a shed with a slate roof, a crane over the water, nets
    and coiled rope, and the owner's flag."""
    b = kit.Building("Dock", 3)
    half = 1.35
    deck_z = 0.22
    # Piles first, standing in the water.
    for k, x in enumerate((-half + 0.1, 0.0, half - 0.1)):
        for j, y in enumerate((-half + 0.1, 0.0, half - 0.1)):
            b.add(kit.cylinder("dock_pile_%d_%d" % (k, j), 0.06, deck_z + 0.02, "wood_dark",
                               (x, y, 0.0), sides=8), kit.FOUNDATION)
    # The deck: half laid in the second stage, whole from the third.
    b.add(kit.box("dock_deck_half", (2 * half, half, 0.06), "wood_dark", (0.0, -half / 2, deck_z)),
          kit.HALF_WALLS)
    b.add(kit.box("dock_deck", (2 * half, 2 * half, 0.06), "wood_dark", (0.0, 0.0, deck_z)),
          kit.FULL_WALLS)
    for k in range(13):
        x = -half + 0.1 + k * (2 * half - 0.2) / 12
        b.add(kit.box("dock_plank_%d" % k, (0.13, 2 * half - 0.04, 0.008), "wood",
                      (x, 0.0, deck_z + 0.06)), kit.FULL_WALLS)
    # Mooring posts along the two edges the camera sees.
    for k in range(4):
        t = -half + 0.2 + k * (2 * half - 0.4) / 3
        b.add(kit.cylinder("dock_bollard_x%d" % k, 0.04, 0.16, "wood_dark",
                           (half - 0.08, t, deck_z + 0.06), sides=8), kit.FINISHED)
        b.add(kit.cylinder("dock_bollard_y%d" % k, 0.04, 0.16, "wood_dark",
                           (t, half - 0.08, deck_z + 0.06), sides=8), kit.FINISHED)
    # A shed at the back corner.
    sx, sy = -half + 0.5, -half + 0.5
    b.add(kit.box("dock_shed", (0.8, 0.8, 0.5), "wood", (sx, sy, deck_z + 0.06)), kit.FINISHED)
    b.add(kit.gable("dock_shed_roof", (0.9, 0.9, 0.3), "slate", (sx, sy, deck_z + 0.56)),
          kit.FINISHED)
    b.add(kit.box("dock_shed_door", (0.2, 0.02, 0.32), "opening",
                  (sx + 0.1, sy + 0.41, deck_z + 0.06)), kit.FINISHED)
    b.add(kit.box("dock_shed_cloth", (0.7, 0.03, 0.09), "player", (sx, sy + 0.42, deck_z + 0.44)),
          kit.FINISHED)
    b.add(kit.box("dock_shed_cloth_side", (0.03, 0.7, 0.09), "player",
                  (sx + 0.42, sy, deck_z + 0.44)), kit.FINISHED)
    # A crane out over the water, a rope and a bale hanging from it.
    cx, cy = half - 0.35, -0.2
    b.add(kit.cylinder("dock_crane_post", 0.05, 0.9, "wood", (cx, cy, deck_z + 0.06), sides=8),
          kit.FINISHED)
    b.add(kit.beam("dock_crane_jib", (cx, cy, deck_z + 0.9), (cx + 0.5, cy + 0.2, deck_z + 1.0),
                   0.03, "wood", sides=6), kit.FINISHED)
    b.add(kit.rod("dock_crane_rope", (cx + 0.5, cy + 0.2, deck_z + 1.0),
                  (cx + 0.5, cy + 0.2, deck_z + 0.45), 0.008, "rope", sides=4), kit.FINISHED)
    b.add(kit.box("dock_crane_bale", (0.14, 0.14, 0.12), "hide",
                  (cx + 0.5, cy + 0.2, deck_z + 0.33)), kit.FINISHED)
    # Nets, rope, jars.
    b.add(kit.box("dock_net", (0.5, 0.35, 0.05), "rope", (0.3, 0.6, deck_z + 0.06)), kit.FINISHED)
    b.add(kit.cylinder("dock_coil", 0.1, 0.05, "rope", (-0.4, 0.75, deck_z + 0.06), sides=12),
          kit.FINISHED)
    for k in range(3):
        b.add(kit.cylinder("dock_jar%d" % k, 0.06, 0.2, "clay_roof",
                           (0.75 + 0.15 * k, 0.9, deck_z + 0.06), sides=10, top_radius=0.045),
              kit.FINISHED)
    # Crates and casks by the crane, and nets hung to dry along the front.
    for k, (x, y) in enumerate([(0.55, -0.75), (0.75, -0.6), (0.62, -0.62)]):
        b.add(kit.box("dock_crate_%d" % k, (0.18, 0.18, 0.16), "wood",
                      (x, y, deck_z + 0.06 + (0.16 if k == 2 else 0.0))), kit.FINISHED)
    for k, (x, y) in enumerate([(0.2, -0.95), (0.05, -0.85)]):
        b.add(kit.cylinder("dock_cask_%d" % k, 0.08, 0.18, "wood_dark", (x, y, deck_z + 0.06),
                           sides=12), kit.FINISHED)
    for k, x in enumerate((-1.0, -0.25)):
        b.add(kit.cylinder("dock_rack_%d" % k, 0.025, 0.45, "wood", (x, half - 0.3, deck_z + 0.06),
                           sides=6), kit.FINISHED)
    b.add(kit.box("dock_rack_bar", (0.8, 0.03, 0.03), "wood", (-0.625, half - 0.3, deck_z + 0.48)),
          kit.FINISHED)
    b.add(kit.box("dock_rack_net", (0.72, 0.015, 0.3), "rope", (-0.625, half - 0.3, deck_z + 0.18)),
          kit.FINISHED)
    kit.flag(b, "dock_flag", half - 0.2, half - 0.2, deck_z + 0.06, height=0.9)
    kit.scaffold(b, "dock", 2.4, 2.4, 0.9)
    # Rubble: stumps of piles and planks adrift.
    for k, (x, y) in enumerate([(-0.9, -0.8), (0.6, -0.9), (-0.7, 0.7), (0.8, 0.6), (0.0, 0.1)]):
        b.add(kit.cylinder("dock_stump_%d" % k, 0.06, 0.1, "wood_dark", (x, y, 0.0), sides=8),
              kit.RUBBLE)
    for k in range(7):
        plank = kit.box("dock_drift_%d" % k, (0.5, 0.08, 0.03), "wood",
                        (-0.9 + 0.3 * k, -0.6 + 0.2 * (k % 4), 0.02))
        plank.rotation_euler = (0.0, 0.0, 0.5 * k)
        b.add(plank, kit.RUBBLE)
    b.finish()
    return b.root


def house():
    """A Stone Age house on two tiles: mudbrick walls, a thatched roof, a
    cloth of the owner's colour over the door."""
    b = kit.Building("House", 2)
    w, d, wall = 1.30, 1.30, 0.50
    kit.walls(b, "house", w, d, wall, "mudbrick")
    kit.scaffold(b, "house", w + 0.1, d + 0.1, wall + 0.25)
    kit.thatch_roof(b, "house_roof", (w + 0.25, d + 0.25, 0.55), (0.0, 0.0, wall))
    kit.woodpile(b, "house_wood", (w / 2 + 0.1, -0.35))
    kit.jar(b, "house_jar", (-0.4, d / 2 + 0.1))
    b.add(kit.box("house_cloth", (w * 0.36, 0.03, 0.12), "player",
                  (0.0, d / 2 + 0.02, wall * 0.62)), kit.FINISHED)
    kit.rubble(b, "house", 1.3, 0.28, mats=("mudbrick", "thatch"))
    b.finish()
    return b.root


def town_center():
    """Three tiles: a broad mudbrick hall, an upper storey, a thatched roof,
    the owner's flag above it and their cloth over the door."""
    b = kit.Building("TownCenter", 3)
    w, wall = 2.2, 0.62
    kit.walls(b, "tc", w, w, wall, "mudbrick")
    kit.scaffold(b, "tc", w + 0.1, w + 0.1, wall + 0.35)
    b.add(kit.box("tc_upper", (1.3, 1.3, 0.38), "plaster", (0.0, 0.0, wall)), kit.FINISHED)
    b.add(kit.box("tc_eave", (w + 0.1, w + 0.1, 0.06), "wood_dark", (0.0, 0.0, wall)),
          kit.FINISHED)
    kit.thatch_roof(b, "tc_roof", (1.6, 1.6, 0.5), (0.0, 0.0, wall + 0.38))
    for i, x in enumerate([-0.45, 0.45]):
        kit.window(b, "tc_up_y%d" % i, (x * 0.8, 0.65, wall + 0.17), "y", kit.FINISHED)
        kit.window(b, "tc_up_x%d" % i, (0.65, x * 0.8, wall + 0.17), "x", kit.FINISHED)
    for i, at in enumerate([(-0.85, w / 2 + 0.12), (0.75, w / 2 + 0.1), (w / 2 + 0.12, 0.7)]):
        kit.jar(b, "tc_jar_%d" % i, at)
    kit.woodpile(b, "tc_wood", (w / 2 + 0.12, -0.6))
    b.add(kit.box("tc_cloth", (0.6, 0.03, 0.16), "player", (0.0, w / 2 + 0.02, wall * 0.7)),
          kit.FINISHED)
    b.add(kit.box("tc_cloth_x", (0.03, 0.6, 0.16), "player", (w / 2 + 0.02, 0.0, wall * 0.7)),
          kit.FINISHED)
    kit.flag(b, "tc", 0.0, 0.0, wall + 0.8, height=0.5)
    kit.rubble(b, "tc", 2.2, 0.35, mats=("mudbrick", "plaster", "thatch"), count=14, seed=3)
    b.finish()
    return b.root


def storehouse():
    """An open shed on posts under a thatched gable, sacks and jars stacked
    inside, the owner's cloth along the eave."""
    b = kit.Building("Storehouse", 2)
    w, d, h = 1.5, 1.3, 0.55
    kit.foundation(b, "store", w + 0.08, d + 0.08, "earth")
    corners = [(-w / 2, -d / 2), (w / 2, -d / 2), (w / 2, d / 2), (-w / 2, d / 2)]
    kit.posts(b, "store_post_half", corners, h * 0.5, kit.HALF_WALLS, radius=0.05)
    kit.posts(b, "store_post", corners, h, kit.FULL_WALLS, radius=0.05)
    b.add(kit.box("store_back", (w, 0.06, h), "wood", (0.0, -d / 2, 0.0)), kit.FULL_WALLS)
    b.add(kit.box("store_side", (0.06, d, h), "wood", (-w / 2, 0.0, 0.0)), kit.FULL_WALLS)
    b.add(kit.gable("store_roof", (w + 0.25, d + 0.25, 0.45), "thatch", (0.0, 0.0, h)),
          kit.FINISHED)
    kit.ridge(b, "store_ridge", w + 0.3, (0.0, 0.0, h + 0.45), "straw")
    b.add(kit.box("store_cloth", (w * 0.8, 0.03, 0.10), "player", (0.0, d / 2 + 0.14, h - 0.02)),
          kit.FINISHED)
    for i, (x, y) in enumerate([(-0.4, -0.3), (-0.15, -0.35), (0.15, -0.3), (-0.4, 0.05)]):
        b.add(kit.box("store_sack_%d" % i, (0.22, 0.2, 0.22), "linen", (x, y, 0.0)),
              kit.FINISHED)
    for i, (x, y) in enumerate([(0.35, -0.3), (0.45, 0.0)]):
        b.add(kit.cylinder("store_jar_%d" % i, 0.09, 0.26, "clay_roof", (x, y, 0.0),
                           top_radius=0.06), kit.FINISHED)
    kit.rubble(b, "store", 1.3, 0.22, mats=("wood", "thatch", "linen"))
    b.finish()
    return b.root


def barracks():
    """Mudbrick walls under a flat roof, a rack of spears by the door, the
    owner's flag on the roof."""
    b = kit.Building("Barracks", 2)
    w, d, wall = 1.5, 1.4, 0.58
    kit.walls(b, "barracks", w, d, wall, "mudbrick")
    kit.scaffold(b, "barracks", w + 0.1, d + 0.1, wall + 0.2)
    kit.vigas(b, "barracks_viga", w, d, wall - 0.06)
    b.add(kit.box("barracks_roof", (w + 0.1, d + 0.1, 0.07), "wood_dark", (0.0, 0.0, wall)),
          kit.FINISHED)
    b.add(kit.box("barracks_parapet", (w + 0.1, 0.06, 0.1), "mudbrick",
                  (0.0, d / 2 + 0.02, wall + 0.07)), kit.FINISHED)
    b.add(kit.box("barracks_parapet_x", (0.06, d + 0.1, 0.1), "mudbrick",
                  (w / 2 + 0.02, 0.0, wall + 0.07)), kit.FINISHED)
    b.add(kit.box("barracks_band", (0.03, d * 0.7, 0.12), "player",
                  (w / 2 + 0.02, 0.0, wall * 0.62)), kit.FINISHED)
    # The spear rack against the +X wall.
    kit.rail(b, "barracks_rack", (w / 2 + 0.12, -0.45), (w / 2 + 0.12, 0.2), 0.3, kit.FINISHED)
    for i, y in enumerate([-0.35, -0.2, -0.05, 0.1]):
        b.add(kit.cylinder("barracks_spear_%d" % i, 0.02, 0.62, "wood", (w / 2 + 0.16, y, 0.0),
                           sides=5, rotation=(0.0, kit._deg(-10.0), 0.0)), kit.FINISHED)
        b.add(kit.cone("barracks_tip_%d" % i, 0.035, 0.1, "bronze",
                       (w / 2 + 0.16 - 0.11, y, 0.6), sides=5), kit.FINISHED)
    kit.flag(b, "barracks", -0.4, -0.4, wall + 0.07, height=0.55)
    kit.rubble(b, "barracks", 1.4, 0.28)
    b.finish()
    return b.root


def farm():
    """Two tiles of tilled earth in rows of green, a low fence at the
    corners, and a post with the owner's cloth."""
    b = kit.Building("Farm", 2)
    s = 1.75
    b.add(kit.box("farm_plot", (s, s, 0.03), "earth"), kit.FOUNDATION)
    rows = 6
    for i in range(rows):
        y = -s / 2 + (i + 0.5) * s / rows
        b.add(kit.box("farm_furrow_half_%d" % i, (s * 0.5, 0.12, 0.05), "earth",
                      (-s * 0.25, y, 0.03)), kit.HALF_WALLS)
        b.add(kit.box("farm_furrow_%d" % i, (s * 0.92, 0.12, 0.05), "earth",
                      (0.0, y, 0.03)), (3,))
        # A row of plants, each its own leafy tuft.
        for j in range(9):
            x = -s * 0.44 + (j + 0.5) * s * 0.88 / 9
            b.add(kit.clump("farm_crop_%d_%d" % (i, j), 0.075, "crop", (x, y, 0.08),
                            seed=100 + i * 9 + j, lumps=0.35, squash=0.9), kit.FINISHED)
    c = s / 2
    for i, (x, y) in enumerate([(-c, -c), (c, -c), (c, c), (-c, c)]):
        b.add(kit.cylinder("farm_stake_%d" % i, 0.03, 0.18, "wood", (x, y, 0.0), sides=5),
              (2, 3, 4))
    b.add(kit.cylinder("farm_marker", 0.025, 0.4, "wood", (c - 0.1, c - 0.1, 0.0), sides=5),
          (1, 2, 3, 4))
    b.add(kit.box("farm_cloth", (0.16, 0.02, 0.10), "player", (c - 0.02, c - 0.1, 0.28)),
          (1, 2, 3, 4))
    b.add(kit.box("farm_bare", (s, s, 0.035), "earth"), kit.RUBBLE)
    b.add(kit.box("farm_stubble", (s * 0.5, s * 0.4, 0.05), "straw", (0.3, -0.2, 0.0)),
          kit.RUBBLE)
    b.add(kit.box("farm_rubble_cloth", (0.16, 0.10, 0.02), "player", (c - 0.2, c - 0.2, 0.035)),
          kit.RUBBLE)
    b.finish()
    return b.root


def archery_range():
    """A thatched hut at the back of an open yard, two straw targets with the
    owner's colour at their centre."""
    b = kit.Building("ArcheryRange", 2)
    kit.foundation(b, "range", 1.8, 1.8, "earth")
    hx, hy, hw, hh = -0.45, -0.45, 0.75, 0.48
    b.add(kit.box("range_hut_half", (hw, hw, hh * 0.5), "mudbrick", (hx, hy, 0.0)),
          kit.HALF_WALLS)
    b.add(kit.box("range_hut", (hw, hw, hh), "mudbrick", (hx, hy, 0.0)), kit.FULL_WALLS)
    b.add(kit.box("range_door", (0.2, 0.02, 0.3), "wood_dark", (hx, hy + hw / 2 + 0.005, 0.0)),
          kit.FULL_WALLS)
    kit.thatch_roof(b, "range_roof", (hw + 0.2, hw + 0.2, 0.4), (hx, hy, hh))
    kit.scaffold(b, "range", hw + 0.1, hw + 0.1, hh + 0.2)
    for i, (x, y) in enumerate([(0.45, 0.1), (0.1, 0.55)]):
        kit.posts(b, "range_stand_%d" % i, [(x - 0.12, y), (x + 0.12, y)], 0.42,
                  (3, 4), radius=0.025)
        kit.disc(b, "range_target_%d" % i, 0.2, 0.06, "straw", (x, y, 0.45), kit.FINISHED)
        kit.disc(b, "range_eye_%d" % i, 0.08, 0.07, "player", (x, y + 0.01, 0.45), kit.FINISHED)
    c = 0.88
    kit.posts(b, "range_fence", [(c, -c), (c, c), (-c, c)], 0.2, kit.FINISHED, radius=0.025)
    kit.rail(b, "range_rail_x", (c, -c), (c, c), 0.16, kit.FINISHED)
    kit.rail(b, "range_rail_y", (-c, c), (c, c), 0.16, kit.FINISHED)
    kit.rubble(b, "range", 1.4, 0.22, mats=("mudbrick", "straw", "thatch"))
    b.finish()
    return b.root


def stable():
    """A long low stable under a thatched gable at the back, a railed
    paddock in front, hay, and the owner's cloth on the stable wall."""
    b = kit.Building("Stable", 2)
    kit.foundation(b, "stable", 1.8, 1.8, "earth")
    w, d, wall, y0 = 1.6, 0.8, 0.46, -0.45
    b.add(kit.box("stable_walls_half", (w, d, wall * 0.5), "wood", (0.0, y0, 0.0)),
          kit.HALF_WALLS)
    b.add(kit.box("stable_walls", (w, d, wall), "wood", (0.0, y0, 0.0)), kit.FULL_WALLS)
    for i, x in enumerate([-0.45, 0.0, 0.45]):
        b.add(kit.box("stable_stall_%d" % i, (0.28, 0.02, 0.3), "wood_dark",
                      (x, y0 + d / 2 + 0.005, 0.0)), kit.FULL_WALLS)
    b.add(kit.gable("stable_roof", (w + 0.2, d + 0.3, 0.4), "thatch", (0.0, y0, wall)),
          kit.FINISHED)
    kit.ridge(b, "stable_ridge", w + 0.25, (0.0, y0, wall + 0.4), "straw")
    b.add(kit.box("stable_cloth", (0.03, d * 0.7, 0.12), "player", (w / 2 + 0.02, y0, wall * 0.6)),
          kit.FINISHED)
    kit.scaffold(b, "stable", w + 0.1, d + 0.1, wall + 0.2)
    c = 0.85
    kit.posts(b, "stable_fence", [(c, 0.0), (c, c), (0.0, c), (-c, c)], 0.26, kit.FINISHED,
              radius=0.025)
    kit.rail(b, "stable_rail_x", (c, 0.0), (c, c), 0.2, kit.FINISHED)
    kit.rail(b, "stable_rail_y", (-c, c), (c, c), 0.2, kit.FINISHED)
    b.add(kit.pyramid("stable_hay", (0.4, 0.35, 0.25), "straw", (-0.5, 0.35, 0.0)),
          kit.FINISHED)
    kit.rubble(b, "stable", 1.4, 0.22, mats=("wood", "thatch", "straw"))
    b.finish()
    return b.root


def market():
    """Three stalls under awnings of the owner's colour, with crates and jars
    in the square between them. It has no walls for the ages to restyle, so
    its square shows the age: beaten earth in the Tool Age, stone in the
    Bronze, white paving round a stone obelisk in the Iron."""
    b = kit.Building("Market", 2)
    # Another architecture paves it in its own stone (kit.ARCH_MATERIALS).
    iron = "white" if kit.STYLE_ARCH == "greek" else "stone_light"
    square = {1: "earth", 3: iron}.get(kit.STYLE_AGE, "stone_light")
    kit.foundation(b, "market", 1.8, 1.8, square)
    if kit.STYLE_AGE == 3:
        b.add(kit.box("market_plinth", (0.2, 0.2, 0.06), "stone_light", (0.42, -0.05, 0.0)),
              kit.FINISHED)
        b.add(kit.pyramid("market_obelisk", (0.1, 0.1, 0.62), "stone_light",
                          (0.42, -0.05, 0.06)), kit.FINISHED)
    stalls = [(-0.45, -0.45), (0.45, -0.4), (-0.4, 0.45)]
    for i, (x, y) in enumerate(stalls):
        pts = [(x - 0.3, y - 0.25), (x + 0.3, y - 0.25), (x + 0.3, y + 0.25), (x - 0.3, y + 0.25)]
        kit.posts(b, "market_half_%d" % i, pts, 0.22, kit.HALF_WALLS, radius=0.03)
        kit.posts(b, "market_post_%d" % i, pts, 0.45, kit.FULL_WALLS, radius=0.03)
        b.add(kit.box("market_counter_%d" % i, (0.6, 0.18, 0.22), "wood",
                      (x, y + 0.12, 0.0)), kit.FULL_WALLS)
        b.add(kit.gable("market_awning_%d" % i, (0.75, 0.62, 0.2), "player", (x, y, 0.45)),
              kit.FINISHED)
    for i, (x, y) in enumerate([(0.35, 0.3), (0.55, 0.45), (0.2, 0.6)]):
        b.add(kit.box("market_crate_%d" % i, (0.18, 0.18, 0.16), "wood", (x, y, 0.0)),
              kit.FINISHED)
    b.add(kit.cylinder("market_jar", 0.08, 0.22, "clay_roof", (0.5, 0.05, 0.0), top_radius=0.05),
          kit.FINISHED)
    kit.rubble(b, "market", 1.4, 0.2, mats=("wood", "stone_light"))
    b.finish()
    return b.root


def watch_tower():
    """One tile: a stone tower with a timber platform and a thatched roof on
    posts, the owner's flag above."""
    b = kit.Building("WatchTower", 1)
    w, h = 0.6, 0.62
    b.add(kit.box("tower_found", (w + 0.1, w + 0.1, 0.04), "slab"), kit.FOUNDATION)
    b.add(kit.box("tower_half", (w, w, h * 0.5), "stone_light"), kit.HALF_WALLS)
    b.add(kit.box("tower_body", (w, w, h), "stone_light"), kit.FULL_WALLS)
    b.add(kit.box("tower_platform", (w + 0.14, w + 0.14, 0.05), "wood", (0.0, 0.0, h)),
          kit.FINISHED)
    c = (w + 0.08) / 2
    for i, (x, y) in enumerate([(-c, -c), (c, -c), (c, c), (-c, c)]):
        b.add(kit.cylinder("tower_post_%d" % i, 0.025, 0.2, "wood", (x, y, h + 0.05), sides=5),
              kit.FINISHED)
    kit.thatch_roof(b, "tower_roof", (w + 0.2, w + 0.2, 0.2), (0.0, 0.0, h + 0.25))
    b.add(kit.box("tower_band", (w * 0.7, 0.03, 0.1), "player", (0.0, w / 2 + 0.02, h * 0.7)),
          kit.FINISHED)
    kit.scaffold(b, "tower", w + 0.08, w + 0.08, h + 0.1)
    kit.rubble(b, "tower", 0.8, 0.2, mats=("stone_light", "wood"), count=6)
    b.finish()
    return b.root


def temple():
    """Bronze Age: a stepped stone platform, a plastered shrine with columns
    before its door, a clay-tiled gable, the owner's cloth over the door."""
    b = kit.Building("Temple", 2)
    kit.foundation(b, "temple", 1.8, 1.8)
    b.add(kit.box("temple_step0", (1.7, 1.7, 0.12), "stone_light"), (2, 3, 4))
    b.add(kit.box("temple_step1", (1.4, 1.4, 0.12), "stone_light", (0.0, 0.0, 0.12)), (2, 3, 4))
    base = 0.24
    w, d, wall = 0.95, 0.85, 0.5
    b.add(kit.box("temple_cella", (w, d, wall), "plaster", (0.0, -0.15, base)), kit.FULL_WALLS)
    b.add(kit.box("temple_door", (0.22, 0.02, 0.32), "wood_dark", (0.0, -0.15 + d / 2 + 0.005, base)),
          kit.FULL_WALLS)
    for i, x in enumerate([-0.4, -0.13, 0.13, 0.4]):
        b.add(kit.cylinder("temple_col_%d" % i, 0.05, wall, "white", (x, 0.5, base), sides=8),
              kit.FULL_WALLS)
    b.add(kit.box("temple_lintel", (1.0, 0.16, 0.06), "white", (0.0, 0.5, base + wall)),
          kit.FINISHED)
    b.add(kit.gable("temple_roof", (1.1, 1.35, 0.35), "clay_roof", (0.0, 0.05, base + wall),
                    ridge="y"), kit.FINISHED)
    kit.ridge(b, "temple_ridge", 1.4, (0.0, 0.05, base + wall + 0.35), "clay_roof", axis="y",
              radius=0.03)
    # The owner's colour where the columns and eaves do not hide it: along
    # the lintel's face and the lower step's +X side.
    b.add(kit.box("temple_cloth", (0.8, 0.03, 0.07), "player",
                  (0.0, 0.595, base + wall - 0.02)), kit.FINISHED)
    b.add(kit.box("temple_band", (0.03, 1.3, 0.07), "player", (0.86, 0.0, 0.03)),
          kit.FINISHED)
    kit.scaffold(b, "temple", 1.2, 1.2, base + wall + 0.1)
    kit.rubble(b, "temple", 1.4, 0.25, mats=("plaster", "stone_light", "white"))
    b.finish()
    return b.root


def academy():
    """Bronze Age: a plastered hall with a colonnade on its two open sides
    under a clay-tiled gable, the owner's flag on the ridge."""
    b = kit.Building("Academy", 2)
    w, d, wall = 1.2, 1.1, 0.55
    kit.walls(b, "academy", w, d, wall, "plaster")
    for i, x in enumerate([-0.5, -0.17, 0.17, 0.5, 0.78]):
        b.add(kit.cylinder("academy_col_y%d" % i, 0.045, wall, "white", (x, 0.78, 0.0), sides=8),
              kit.FULL_WALLS)
    for i, y in enumerate([-0.45, -0.12, 0.2, 0.5]):
        b.add(kit.cylinder("academy_col_x%d" % i, 0.045, wall, "white", (0.78, y, 0.0), sides=8),
              kit.FULL_WALLS)
    b.add(kit.box("academy_cornice", (1.72, 1.72, 0.06), "white", (0.12, 0.12, wall)),
          kit.FINISHED)
    b.add(kit.gable("academy_roof", (1.8, 1.8, 0.34), "clay_roof", (0.12, 0.12, wall + 0.06)),
          kit.FINISHED)
    kit.ridge(b, "academy_ridge", 1.85, (0.12, 0.12, wall + 0.4), "clay_roof", radius=0.03)
    b.add(kit.box("academy_band", (0.03, 0.8, 0.1), "player", (w / 2 + 0.02, 0.0, wall * 0.65)),
          kit.FINISHED)
    kit.flag(b, "academy", -0.5, 0.12, wall + 0.3, height=0.4)
    kit.scaffold(b, "academy", w + 0.1, d + 0.1, wall + 0.2)
    kit.rubble(b, "academy", 1.4, 0.25, mats=("plaster", "white"))
    b.finish()
    return b.root


def siege_workshop():
    """Bronze Age: an open timber shed with a great wheel and stacked logs,
    the owner's cloth on the beam."""
    b = kit.Building("SiegeWorkshop", 2)
    kit.foundation(b, "siege", 1.8, 1.8, "earth")
    w, d, h = 1.5, 1.2, 0.6
    corners = [(-w / 2, -d / 2), (w / 2, -d / 2), (w / 2, d / 2), (-w / 2, d / 2)]
    kit.posts(b, "siege_half", corners, h * 0.5, kit.HALF_WALLS, radius=0.05)
    kit.posts(b, "siege_post", corners, h, kit.FULL_WALLS, radius=0.05)
    # An open shed has no walls for the ages to restyle (kit.style_building):
    # in the Iron Age its back wall is stone and its roof slate.
    iron = kit.STYLE_AGE == 3
    b.add(kit.box("siege_back", (w, 0.06, h), "stone_light" if iron else "wood_dark",
                  (0.0, -d / 2, 0.0)), kit.FULL_WALLS)
    b.add(kit.gable("siege_roof", (w + 0.2, d + 0.2, 0.35), "slate" if iron else "wood_dark",
                    (0.0, 0.0, h)), kit.FINISHED)
    b.add(kit.box("siege_cloth", (w * 0.6, 0.03, 0.1), "player", (0.0, d / 2 + 0.12, h - 0.03)),
          kit.FINISHED)
    kit.disc(b, "siege_wheel", 0.26, 0.07, "wood_dark", (0.2, 0.1, 0.26), kit.FINISHED,
             facing="x")
    for i, (y, z) in enumerate([(-0.25, 0.06), (-0.1, 0.06), (-0.18, 0.18)]):
        b.add(kit.cylinder("siege_log_%d" % i, 0.07, 0.9, "wood", (-0.35, y, z), sides=6,
                           pivot="centre", rotation=(0.0, kit._deg(90.0), 0.0)), kit.FINISHED)
    kit.rubble(b, "siege", 1.4, 0.22, mats=("wood", "wood_dark"))
    b.finish()
    return b.root


def government_centre():
    """Bronze Age: a plastered hall on a stone platform, a second storey, a
    clay-tiled roof, and the owner's flags on two corners."""
    b = kit.Building("GovernmentCentre", 2)
    kit.foundation(b, "gov", 1.8, 1.8)
    b.add(kit.box("gov_platform", (1.7, 1.7, 0.1), "stone_light"), (2, 3, 4))
    w, wall = 1.35, 0.5
    b.add(kit.box("gov_walls", (w, w, wall), "plaster", (0.0, 0.0, 0.1)), kit.FULL_WALLS)
    b.add(kit.box("gov_door", (0.28, 0.02, 0.32), "wood_dark", (0.0, w / 2 + 0.005, 0.1)),
          kit.FULL_WALLS)
    b.add(kit.box("gov_upper", (0.85, 0.85, 0.3), "plaster", (0.0, 0.0, 0.1 + wall)),
          kit.FINISHED)
    b.add(kit.pyramid("gov_roof", (1.0, 1.0, 0.3), "clay_roof", (0.0, 0.0, 0.4 + wall)),
          kit.FINISHED)
    b.add(kit.box("gov_band", (w * 0.8, 0.03, 0.1), "player", (0.0, w / 2 + 0.02, 0.1 + wall * 0.7)),
          kit.FINISHED)
    kit.flag(b, "gov_a", w / 2 - 0.05, -w / 2 + 0.05, 0.1 + wall, height=0.4)
    kit.flag(b, "gov_b", -w / 2 + 0.05, w / 2 - 0.05, 0.1 + wall, height=0.4)
    kit.scaffold(b, "gov", w + 0.1, w + 0.1, wall + 0.3)
    kit.rubble(b, "gov", 1.5, 0.25, mats=("plaster", "stone_light", "clay_roof"))
    b.finish()
    return b.root


# --------------------------------------------------------------------------
# Walls and the gate. A wall tile's finished frame is its post alone; the
# game adds an arm toward each wall beside it (kit.WALL_DIRECTIONS), and the
# arms of two tiles meet at the edge or the corner they share.

POST_R = 0.10          # the palisade's post
STAKE_R = 0.045        # and the stakes of its arms
STAKE_STEP = 0.10


def palisade_wall():
    """Sharpened logs: a stout post with the owner's band, and from it a row
    of stakes lashed with rope toward each neighbour."""
    b = kit.Building("PalisadeWall", 1, frames=13)
    b.add(kit.box("palisade_found", (0.4, 0.4, 0.03), "earth"), kit.STAGES)
    b.add(kit.cylinder("palisade_post_half", POST_R, 0.3, "wood_dark", sides=8),
          kit.HALF_WALLS)
    b.add(kit.cylinder("palisade_post", POST_R, 0.62, "wood_dark", sides=8), kit.FULL_WALLS)
    b.add(kit.cone("palisade_post_tip", POST_R, 0.1, "wood_dark", (0.0, 0.0, 0.62), sides=8),
          kit.FINISHED)
    b.add(kit.cylinder("palisade_band", POST_R + 0.008, 0.08, "player", (0.0, 0.0, 0.42),
                       sides=8), kit.FINISHED)
    for k in range(8):
        unit, angle, reach = kit.wall_direction(k)
        frames = kit.arm_frames(k)
        # Packed from the edge in, so the gap where two tiles' rows meet is
        # the same as the gap between any two stakes.
        s, i = reach - STAKE_STEP / 2.0, 0
        while s - STAKE_R >= POST_R:
            x, y = kit.along(unit, s)
            h = 0.47 if i % 2 else 0.5
            b.add(kit.cylinder("palisade_arm%d_stake%d" % (k, i), STAKE_R, h, "wood",
                               (x, y, 0.0), sides=6), frames)
            b.add(kit.cone("palisade_arm%d_tip%d" % (k, i), STAKE_R, 0.07, "wood", (x, y, h),
                           sides=6), frames)
            s -= STAKE_STEP
            i += 1
        x, y = kit.along(unit, (POST_R + reach) / 2.0)
        for j, z in enumerate((0.12, 0.34)):
            b.add(kit.box("palisade_arm%d_lash%d" % (k, j), (reach - POST_R, 2 * STAKE_R + 0.02,
                                                             0.03), "rope", (x, y, z),
                          rotation=(0.0, 0.0, angle)), frames)
    for i, (x, y, a) in enumerate([(-0.12, 0.05, 0.4), (0.1, -0.1, -0.9), (0.02, 0.18, 1.6)]):
        b.add(kit.cylinder("palisade_log_%d" % i, 0.05, 0.5, "wood", (x, y, 0.05), sides=6,
                           pivot="centre", rotation=(0.0, kit._deg(90.0), a)), kit.RUBBLE)
    b.add(kit.cylinder("palisade_stump", POST_R, 0.12, "wood_dark", sides=8), kit.RUBBLE)
    b.finish()
    return b.root


PIER = 0.16            # half the stone wall's pier
STONE_W = 0.26         # its arms' thickness


def stone_arm(b, name, k, frames, pier=PIER, reach=None):
    """A stretch of stone wall from a pier of half-size `pier` toward
    direction `k`: the courses, a coping and merlons along the top."""
    unit, _, edge = kit.wall_direction(k)
    reach = edge if reach is None else reach
    b.add(kit.prism(name + "_body", _cut(kit.arm_plan(k, pier, STONE_W), unit, edge, reach),
                    0.44, "stone_light"), frames)
    b.add(kit.prism(name + "_coping", _cut(kit.arm_plan(k, pier, STONE_W + 0.04), unit, edge,
                                           reach), 0.04, "stone", z=0.44), frames)
    dx, dy = kit.WALL_DIRECTIONS[k]
    inner = pier * (2.0 ** 0.5 if dx and dy else 1.0)
    s, i = reach - 0.075, 0
    while s - 0.035 >= inner + 0.035:
        x, y = kit.along(unit, s)
        b.add(kit.box("%s_merlon%d" % (name, i), (0.07, STONE_W + 0.04, 0.08), "stone_light",
                      (x, y, 0.48), rotation=(0.0, 0.0, kit.wall_direction(k)[1])), frames)
        s -= 0.15
        i += 1


def _cut(plan, unit, edge, reach):
    """`plan` with its far end pulled back from `edge` to `reach`."""
    if abs(reach - edge) < 1e-9:
        return plan
    out = []
    for x, y in plan:
        s = x * unit[0] + y * unit[1]
        if abs(s - edge) < 1e-6:
            x, y = x - unit[0] * (edge - reach), y - unit[1] * (edge - reach)
        out.append((x, y))
    return out


def stone_wall():
    """Dressed stone: a square pier banded in the owner's colour under its
    cap, and a crenellated wall toward each neighbour."""
    b = kit.Building("StoneWall", 1, frames=13)
    b.add(kit.box("stone_found", (0.46, 0.46, 0.04), "slab"), kit.STAGES)
    b.add(kit.box("stone_pier_half", (2 * PIER, 2 * PIER, 0.3), "stone_light"), kit.HALF_WALLS)
    b.add(kit.box("stone_pier", (2 * PIER, 2 * PIER, 0.6), "stone_light"), kit.FULL_WALLS)
    b.add(kit.box("stone_band", (2 * PIER + 0.01, 2 * PIER + 0.01, 0.07), "player",
                  (0.0, 0.0, 0.5)), kit.FINISHED)
    b.add(kit.box("stone_cap", (2 * PIER + 0.05, 2 * PIER + 0.05, 0.05), "stone",
                  (0.0, 0.0, 0.6)), kit.FINISHED)
    kit.scaffold(b, "stone", 2 * PIER + 0.1, 2 * PIER + 0.1, 0.62)
    for k in range(8):
        stone_arm(b, "stone_arm%d" % k, k, kit.arm_frames(k))
    kit.rubble(b, "stone", 0.62, 0.18, mats=("stone_light", "stone"), count=7, seed=5)
    b.finish()
    return b.root


GATE_PASSAGE = 0.44
GATE_TOWER = 0.28


def gate():
    """A stone gatehouse across the wall's line: two towers with the owner's
    banners, a lintel over the passage, and two doors, shut or swung back.
    It stands in one of four orientations, the line of the wall it is set
    into; the game picks one (kit.GATE_SPANS). The stages are square, the
    same whichever way it will face."""
    b = kit.Building("Gate", 1, frames=13)
    shut0 = kit.GATE_SPANS["shut"][0]
    open0 = kit.GATE_SPANS["open"][0]
    b.add(kit.box("gate_found", (0.8, 0.8, 0.04), "slab"), kit.STAGES)
    b.add(kit.box("gate_half", (0.6, 0.6, 0.26), "stone_light"), kit.HALF_WALLS)
    b.add(kit.box("gate_block", (0.6, 0.6, 0.52), "stone_light"), kit.SCAFFOLD)
    kit.scaffold(b, "gate", 0.7, 0.7, 0.62)
    p, t = GATE_PASSAGE, GATE_TOWER
    for o in range(4):
        unit, angle, length = kit.gate_orientation(o)
        # Shown shut on the finished frame as well, in the first orientation.
        both = (shut0 + o, open0 + o) + ((4,) if o == 0 else ())
        shut = (shut0 + o,) + ((4,) if o == 0 else ())
        opened = (open0 + o,)
        # The side away from the camera (it sits out at +X +Y), where the
        # doors swing to so the passage shows.
        left = (-unit[1], unit[0])
        away = -1.0 if left[0] + left[1] > 0.0 else 1.0
        rot = (0.0, 0.0, angle)
        for side in (-1.0, 1.0):
            mid = side * (p / 2.0 + t / 2.0)
            x, y = kit.along(unit, mid)
            b.add(kit.box("gate%d_tower%d" % (o, side > 0), (t, 0.38, 0.7), "stone_light",
                          (x, y, 0.0), rotation=rot), both)
            b.add(kit.box("gate%d_cap%d" % (o, side > 0), (t + 0.04, 0.42, 0.05), "stone",
                          (x, y, 0.7), rotation=rot), both)
            for face in (-1.0, 1.0):
                bx, by = kit.along(unit, mid, face * 0.2)
                b.add(kit.box("gate%d_banner%d%d" % (o, side > 0, face > 0),
                              (0.12, 0.02, 0.22), "player", (bx, by, 0.36), rotation=rot),
                      both)
            # A diagonal wall is longer than a tile is wide: a stretch of
            # wall carries each tower out to the corner.
            if length > 1.01:
                k = o if side > 0 else (o + 4) % 8
                stone_arm(b, "gate%d_stub%d" % (o, side > 0), k, both,
                          pier=(p / 2.0 + t) / 2.0 ** 0.5)
            # The doors: across the passage, or each swung back to lie along
            # its tower's inner face.
            leaf = p / 2.0 - 0.01
            x, y = kit.along(unit, side * p / 4.0)
            b.add(kit.box("gate%d_door%d" % (o, side > 0), (leaf, 0.05, 0.5), "wood_dark",
                          (x, y, 0.0), rotation=rot), shut)
            b.add(kit.box("gate%d_bar%d" % (o, side > 0), (leaf, 0.07, 0.04), "iron",
                          (x, y, 0.3), rotation=rot), shut)
            x, y = kit.along(unit, side * (p / 2.0 - 0.035), away * leaf / 2.0)
            b.add(kit.box("gate%d_open%d" % (o, side > 0), (0.05, leaf, 0.5), "wood_dark",
                          (x, y, 0.0), rotation=rot), opened)
        x, y = kit.along(unit, 0.0)
        b.add(kit.box("gate%d_lintel" % o, (p + 0.04, 0.38, 0.12), "stone_light",
                      (x, y, 0.5), rotation=rot), both)
    kit.rubble(b, "gate", 0.8, 0.22, mats=("stone_light", "wood_dark", "stone"), count=8,
               seed=9)
    b.finish()
    return b.root


# --------------------------------------------------------------------------
# What the map is made of. These belong to nobody, so they wear no player
# colour (`atlas` knows them as neutral sets); a node has one standing frame.

def tree():
    """A broadleaf tree: a tapering trunk forking into boughs under a crown
    of leaf clumps, darker beneath and lit on top."""
    import math as _m
    root = kit.empty("Tree")
    parts = [kit.cylinder("tree_trunk", 0.075, 0.55, "bark", sides=10, top_radius=0.04)]
    for i, (a, tilt) in enumerate([(0.3, 28.0), (2.4, 34.0), (4.3, 30.0)]):
        bough = kit.cylinder("tree_bough_%d" % i, 0.03, 0.3, "bark", (0.0, 0.0, 0.42), sides=6,
                             top_radius=0.015)
        bough.rotation_euler = (kit._deg(tilt) * _m.cos(a), kit._deg(tilt) * _m.sin(a), 0.0)
        parts.append(bough)
    clumps = [(0.0, 0.0, 0.86, 0.26, "leaf"), (0.17, 0.08, 0.72, 0.21, "leaf_dark"),
              (-0.15, 0.12, 0.74, 0.2, "leaf"), (0.04, -0.18, 0.7, 0.2, "leaf_dark"),
              (-0.08, -0.06, 1.0, 0.18, "leaf"), (0.12, -0.04, 0.97, 0.16, "leaf"),
              (-0.17, -0.12, 0.62, 0.15, "leaf_dark"), (0.14, 0.18, 0.9, 0.15, "leaf")]
    for i, (x, y, z, r, mat) in enumerate(clumps):
        parts.append(kit.clump("tree_crown_%d" % i, r, mat, (x, y, z), seed=i + 1,
                               squash=0.85))
    for p in parts:
        p.parent = root
    return root


def berry_bush():
    """A low bush of leaf clumps hung with red berries."""
    import math as _m
    root = kit.empty("BerryBush")
    parts = []
    for i, (x, y, z, r) in enumerate([(0.0, 0.0, 0.16, 0.2), (0.16, 0.06, 0.12, 0.15),
                                      (-0.14, 0.1, 0.12, 0.15), (0.05, -0.15, 0.11, 0.14),
                                      (-0.08, -0.1, 0.22, 0.13)]):
        parts.append(kit.clump("bush_%d" % i, r, "leaf_dark" if i % 2 else "leaf", (x, y, z),
                               seed=20 + i, squash=0.75))
    for i in range(14):
        a = 2 * _m.pi * i / 14 + 0.3
        r = 0.24 if i % 2 else 0.15
        z = 0.12 if i % 2 else 0.27
        parts.append(kit.clump("bush_berry_%d" % i, 0.035, "berry",
                               (r * _m.cos(a), r * _m.sin(a), z), seed=40 + i, lumps=0.05))
    for p in parts:
        p.parent = root
    return root


def _vein(name, rock, fleck):
    """A heap of broken rock, lumpy as rock is, with nuggets of what it is
    worth showing through."""
    import random
    rng = random.Random(7 if fleck == "gold" else 11)
    root = kit.empty(name)
    parts = []
    for i, (x, y, s) in enumerate([(0.0, 0.0, 0.2), (-0.2, 0.12, 0.15), (0.18, -0.15, 0.16),
                                   (0.12, 0.2, 0.13), (-0.15, -0.18, 0.13), (0.24, 0.04, 0.1)]):
        parts.append(kit.clump("%s_rock_%d" % (name, i), s, rock if i % 2 else "rock",
                               (x, y, s * 0.45), seed=60 + i, lumps=0.38, squash=0.75))
    # What it is worth has to show from across the map: big nuggets, many
    # of them, sitting on the rock rather than buried in it.
    import math as _m
    for i in range(20):
        a = rng.uniform(0.0, 6.28)
        r = rng.uniform(0.0, 0.22)
        z = 0.2 * (1.0 - r / 0.3) + 0.05
        parts.append(kit.clump("%s_fleck_%d" % (name, i), rng.uniform(0.045, 0.07), fleck,
                               (r * _m.cos(a), r * _m.sin(a), z), seed=80 + i, lumps=0.3))
    for p in parts:
        p.parent = root
    return root


def gold_mine():
    return _vein("GoldMine", "rock", "gold")


def stone_mine():
    return _vein("StoneMine", "rock_light", "white")


def gazelle():
    """A gazelle: tawny coat, pale belly, short dark horns."""
    a = kit.Animal("Gazelle")
    a.animate()
    # A quarter larger than life, like everything at this scale, so the herd
    # can be seen and clicked.
    a.root.scale = (1.25, 1.25, 1.25)
    return a.root


# --------------------------------------------------------------------------
# The ground's grain. Not sprites: each is a patch of one tile of a ground
# type, rendered through the terrain view and turned by `atlas detail` into
# a greyscale layer the game multiplies over its blended ground colours
# (`crates/view/src/detail.rs`). Whatever is scattered on the tile is laid
# again on the eight tiles around it, so the render is one period of an
# endless field and the layer tiles without a seam. Colour does not survive,
# only light and shade, so the materials matter only for how light or dark
# each part is against the rest.

def _ground(name, base, scatter, seed):
    """A tile of ground: a base under three tiles each way, and
    `scatter(rng)`'s pieces, each (builder, kwargs) placed on every one of
    the nine tiles."""
    import random
    rng = random.Random(seed)
    root = kit.empty(name)
    floor = kit.box(name + "_floor", (3.0, 3.0, 0.02), base, pivot="top")
    floor.parent = root
    pieces = scatter(rng)
    n = 0
    for ox in (-1.0, 0.0, 1.0):
        for oy in (-1.0, 0.0, 1.0):
            for make, (x, y), kw in pieces:
                obj = make("%s_%d" % (name, n), (x + ox, y + oy), **kw)
                obj.parent = root
                n += 1
    return root


def _cell(rng):
    return (rng.uniform(-0.5, 0.5), rng.uniform(-0.5, 0.5))


def _blade(name, at, mat, h, tilt):
    blade = kit.cone(name, 0.014, h, mat, (at[0], at[1], 0.0), sides=4)
    blade.rotation_euler = (tilt[0], tilt[1], 0.0)
    return blade


def _chip(name, at, mat, size, turn, z=0.0):
    return kit.box(name, size, mat, (at[0], at[1], z), rotation=(0.0, 0.0, turn))


def _stripe(name, at, mat, width, height, axis):
    """A low ridge right across the tile, so the copies either side carry
    it on without a break."""
    size = (1.0, width, height) if axis == "x" else (width, 1.0, height)
    return kit.gable(name, size, mat, (at[0], at[1], 0.0), ridge=axis)


def ground_grass():
    """Blades of three greens over dark soil."""
    def scatter(rng):
        out = []
        for _ in range(120):
            mat = rng.choice(["leaf", "leaf", "crop", "leaf_dark"])
            tilt = (rng.uniform(-0.45, 0.45), rng.uniform(-0.45, 0.45))
            out.append((_blade, _cell(rng), dict(mat=mat, h=rng.uniform(0.035, 0.075),
                                                  tilt=tilt)))
        return out
    return _ground("GroundGrass", "leaf_dark", scatter, 21)


def ground_dirt():
    """Packed earth with pebbles and clods."""
    def scatter(rng):
        out = []
        for _ in range(45):
            s = rng.uniform(0.015, 0.04)
            out.append((_chip, _cell(rng), dict(mat=rng.choice(["rock", "rock_light", "slab"]),
                                                 size=(s, s * 0.8, s * 0.6),
                                                 turn=rng.uniform(0, 3.14))))
        for _ in range(16):
            out.append((_chip, _cell(rng), dict(mat="earth", size=(0.06, 0.045, 0.014),
                                                 turn=rng.uniform(0, 3.14))))
        return out
    return _ground("GroundDirt", "earth", scatter, 22)


def ground_desert():
    """Hardpan split into plates, a few stones on it."""
    def scatter(rng):
        out = []
        # Plates on a loose grid, each shoved and turned so the cracks
        # between them wander rather than rule lines.
        k = 4
        for i in range(k):
            for j in range(k):
                at = (-0.5 + (i + 0.5) / k + rng.uniform(-0.05, 0.05),
                      -0.5 + (j + 0.5) / k + rng.uniform(-0.05, 0.05))
                sz = rng.uniform(0.19, 0.25)
                out.append((_chip, at, dict(mat="straw", size=(sz, sz * rng.uniform(0.7, 1.0),
                                                               rng.uniform(0.006, 0.011)),
                                             turn=rng.uniform(-0.6, 0.6))))
        for _ in range(10):
            out.append((_chip, _cell(rng), dict(mat="rock_light", size=(0.025, 0.02, 0.015),
                                                 turn=rng.uniform(0, 3.14), z=0.008)))
        return out
    return _ground("GroundDesert", "earth", scatter, 23)


def ground_sand():
    """Wind ripples."""
    def scatter(rng):
        return [(_stripe, (0.0, -0.5 + (k + 0.5) / 7), dict(mat="straw", width=0.08,
                                                             height=rng.uniform(0.008, 0.013),
                                                             axis="x"))
                for k in range(7)]
    return _ground("GroundSand", "straw", scatter, 24)


def ground_shallow_water():
    """Small, close ripples."""
    def scatter(rng):
        return [(_stripe, (-0.5 + (k + 0.5) / 6, 0.0), dict(mat="plaster", width=0.09,
                                                             height=0.006, axis="y"))
                for k in range(6)]
    return _ground("GroundShallowWater", "plaster", scatter, 25)


def ground_deep_water():
    """Long, slow swells."""
    def scatter(rng):
        return [(_stripe, (-0.5 + (k + 0.5) / 3, 0.0), dict(mat="stone", width=0.22,
                                                             height=0.012, axis="y"))
                for k in range(3)]
    return _ground("GroundDeepWater", "stone", scatter, 26)


def ground_forest_floor():
    """Fallen leaves and twigs on dark soil."""
    def scatter(rng):
        out = []
        for _ in range(75):
            out.append((_chip, _cell(rng), dict(mat=rng.choice(["leaf_dark", "leaf", "straw",
                                                                 "earth"]),
                                                 size=(0.04, 0.025, 0.006),
                                                 turn=rng.uniform(0, 3.14))))
        for _ in range(6):
            out.append((_chip, _cell(rng), dict(mat="wood", size=(0.13, 0.012, 0.012),
                                                 turn=rng.uniform(0, 3.14))))
        return out
    return _ground("GroundForestFloor", "bark", scatter, 27)


def ground_snow():
    """Soft drifts."""
    def mound(name, at, r, h):
        return kit.cylinder(name, r, h, "white", (at[0], at[1], 0.0), sides=10,
                            top_radius=r * 0.4)

    def scatter(rng):
        return [(mound, _cell(rng), dict(r=rng.uniform(0.04, 0.09), h=rng.uniform(0.008, 0.016)))
                for _ in range(26)]
    return _ground("GroundSnow", "white", scatter, 28)


# The game's ground types in `sim::Terrain` order: `atlas detail` stacks the
# layers so.
GROUNDS = ["grass", "dirt", "desert", "sand", "shallow_water", "deep_water", "forest_floor",
           "snow"]


# name: (builder, size class, "unit", "villager", "building", "wall", "gate",
# "node" or "ground")
SUBJECTS = {
    "villager": (villager, "Foot", "villager"),
    "clubman": (clubman, "Foot", "unit"),
    "axeman": (axeman, "Foot", "unit"),
    "spearman": (spearman, "Foot", "unit"),
    "slinger": (slinger, "Foot", "unit"),
    "bowman": (bowman, "Foot", "unit"),
    "scout": (scout, "Mounted", "unit"),
    "light_cavalry": (light_cavalry, "Mounted", "unit"),
    "house": (house, "MediumBuilding", "building"),
    "town_center": (town_center, "LargeBuilding", "building"),
    "storehouse": (storehouse, "MediumBuilding", "building"),
    "barracks": (barracks, "MediumBuilding", "building"),
    "farm": (farm, "MediumBuilding", "building"),
    "archery_range": (archery_range, "MediumBuilding", "building"),
    "stable": (stable, "MediumBuilding", "building"),
    "market": (market, "MediumBuilding", "building"),
    "watch_tower": (watch_tower, "SmallBuilding", "building"),
    "temple": (temple, "MediumBuilding", "building"),
    "academy": (academy, "MediumBuilding", "building"),
    "siege_workshop": (siege_workshop, "MediumBuilding", "building"),
    "government_centre": (government_centre, "MediumBuilding", "building"),
    "palisade_wall": (palisade_wall, "SmallBuilding", "wall"),
    "stone_wall": (stone_wall, "SmallBuilding", "wall"),
    "gate": (gate, "SmallBuilding", "gate"),
    "tree": (tree, "SmallBuilding", "node"),
    "berry_bush": (berry_bush, "SmallBuilding", "node"),
    "gold_mine": (gold_mine, "SmallBuilding", "node"),
    "stone_mine": (stone_mine, "SmallBuilding", "node"),
    "gazelle": (gazelle, "Foot", "unit"),
    "swordsman": (swordsman, "Foot", "unit"),
    "hoplite": (hoplite, "Foot", "unit"),
    "legionary": (legionary, "Foot", "unit"),
    "chariot_archer": (chariot_archer, "Heavy", "unit"),
    "horse_archer": (horse_archer, "Mounted", "unit"),
    "heavy_cavalry": (heavy_cavalry, "Mounted", "unit"),
    "war_elephant": (war_elephant, "Heavy", "unit"),
    "stone_thrower": (stone_thrower, "Heavy", "unit"),
    "catapult": (catapult, "Heavy", "unit"),
    "ballista": (ballista, "Heavy", "unit"),
    "priest": (priest, "Foot", "unit"),
    "relic": (relic, "SmallBuilding", "node"),
    "wonder": (wonder, "Wonder", "building"),
    "wonder_egyptian": (wonder_egyptian, "Wonder", "building"),
    "wonder_mesopotamian": (wonder_mesopotamian, "Wonder", "building"),
    "wonder_asian": (wonder_asian, "Wonder", "building"),
    "dock": (dock, "LargeBuilding", "building"),
    "fishing_boat": (fishing_boat, "Ship", "unit"),
    "transport": (transport, "Ship", "unit"),
    "trade_boat": (trade_boat, "Ship", "unit"),
    "archer_ship": (archer_ship, "Ship", "unit"),
    "war_galley": (war_galley, "Ship", "unit"),
    "catapult_ship": (catapult_ship, "Ship", "unit"),
    "fish": (fish, "SmallBuilding", "node"),
}
SUBJECTS.update({"ground_" + g: (globals()["ground_" + g], "Terrain", "ground") for g in GROUNDS})


# The ages a subject has its own look in, past its own (kit.STYLE_AGE): a
# subject that can stand in the Stone Age is drawn anew for the three after
# it, one that comes with the Tool Age likewise (its own model is never seen
# before then), one that comes with the Bronze Age for the Iron. The set is
# named for its age: `house_tool`, `temple_iron`. The farm, the walls and the
# gate look the same in every age; the ranged soldiers and the riders keep
# theirs (`docs/02` section 4 dresses the villager and the infantry).
AGE_NAMES = {1: "tool", 2: "bronze", 3: "iron"}
AGED = {
    "town_center": (1, 2, 3), "house": (1, 2, 3), "storehouse": (1, 2, 3),
    "barracks": (1, 2, 3), "archery_range": (1, 2, 3), "stable": (1, 2, 3),
    "market": (1, 2, 3), "watch_tower": (1, 2, 3),
    "temple": (3,), "academy": (3,), "siege_workshop": (3,), "government_centre": (3,),
    "villager": (1, 2, 3), "clubman": (1, 2, 3), "axeman": (1, 2, 3), "spearman": (1, 2, 3),
    "swordsman": (3,), "hoplite": (3,),
}


# The architectures besides the Greek (kit.STYLE_ARCH, docs/07 D34): each
# building below drawn again in each, in each of its ages, the set named
# for its architecture before its age (`house_egyptian_bronze`). Each
# architecture's Wonder is its own model (`wonder_egyptian`); the Dock, the
# farm, the walls and the gate are everyone's.
ARCHES = ("egyptian", "mesopotamian", "asian")
ARCHED = ("town_center", "house", "storehouse", "barracks", "archery_range", "stable",
          "market", "watch_tower", "temple", "academy", "siege_workshop", "government_centre")
# What comes with the Bronze Age is built in the Bronze Age's materials in
# an architecture's first look of it.
FIRST_AGE = {"temple": 2, "academy": 2, "siege_workshop": 2, "government_centre": 2}


def subject(name):
    """The builder, class, kind, age and architecture for a subject's
    name."""
    if name in SUBJECTS:
        build, cls, what = SUBJECTS[name]
        return build, cls, what, 0, "greek"
    rest, age = name, 0
    base, _, suffix = name.rpartition("_")
    for a, label in AGE_NAMES.items():
        if suffix == label:
            rest, age = base, a
    arch = "greek"
    for candidate in ARCHES:
        if rest.endswith("_" + candidate):
            rest, arch = rest[:-len(candidate) - 1], candidate
    if rest not in SUBJECTS or (age and age not in AGED.get(rest, ())):
        return None
    if arch != "greek":
        if rest not in ARCHED:
            return None
        age = max(age, FIRST_AGE.get(rest, 0))
    build, cls, what = SUBJECTS[rest]
    return build, cls, what, age, arch


def all_subjects():
    names = list(SUBJECTS)
    for base, ages in AGED.items():
        names += ["%s_%s" % (base, AGE_NAMES[a]) for a in ages]
    for base in ARCHED:
        for arch in ARCHES:
            names.append("%s_%s" % (base, arch))
            names += ["%s_%s_%s" % (base, arch, AGE_NAMES[a]) for a in AGED.get(base, ())]
    return sorted(names)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    if "--list" in argv:
        for name in all_subjects():
            _, cls, what, _, _ = subject(name)
            print("%s %s %s" % (name, cls, what))
        return
    name = argv[argv.index("--subject") + 1]
    found = subject(name)
    if found is None:
        raise SystemExit("no subject %r; the slice has %s" % (name, all_subjects()))
    build, _, _, age, arch = found
    kit.STYLE_AGE = age
    kit.STYLE_ARCH = arch
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

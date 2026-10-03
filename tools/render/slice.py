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


def clubman():
    """Stone Age: bare-headed, a hide kilt, a club."""
    h = kit.Humanoid("Clubman")
    h.hold(kit.club("club"))
    h.animate("swing")
    return h.root


def axeman():
    """The clubman's line upgraded: a bronze cap and a bronze axe."""
    h = kit.Humanoid("Axeman", helmet="cap", helmet_mat="bronze")
    h.hold(kit.axe("axe"))
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
    b.add(kit.pyramid("tc_roof", (1.6, 1.6, 0.5), "thatch", (0.0, 0.0, wall + 0.38)),
          kit.FINISHED)
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
        b.add(kit.box("farm_crop_%d" % i, (s * 0.92, 0.14, 0.12), "crop",
                      (0.0, y, 0.03)), kit.FINISHED)
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
    b.add(kit.pyramid("range_roof", (hw + 0.2, hw + 0.2, 0.4), "thatch", (hx, hy, hh)),
          kit.FINISHED)
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
    in the square between them."""
    b = kit.Building("Market", 2)
    kit.foundation(b, "market", 1.8, 1.8, "stone_light")
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
    b.add(kit.pyramid("tower_roof", (w + 0.2, w + 0.2, 0.2), "thatch", (0.0, 0.0, h + 0.25)),
          kit.FINISHED)
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
    b.add(kit.box("siege_back", (w, 0.06, h), "wood_dark", (0.0, -d / 2, 0.0)), kit.FULL_WALLS)
    b.add(kit.gable("siege_roof", (w + 0.2, d + 0.2, 0.35), "wood_dark", (0.0, 0.0, h)),
          kit.FINISHED)
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
    """A broadleaf tree: a trunk and a rounded, two-tone canopy."""
    root = kit.empty("Tree")
    parts = [
        kit.cylinder("tree_trunk", 0.07, 0.5, "bark", sides=7, top_radius=0.05),
        kit.cylinder("tree_crown_low", 0.34, 0.28, "leaf_dark", (0.0, 0.0, 0.42), sides=8,
                     top_radius=0.36),
        kit.cylinder("tree_crown_mid", 0.36, 0.22, "leaf", (0.0, 0.0, 0.70), sides=8,
                     top_radius=0.28),
        kit.cone("tree_crown_top", 0.28, 0.22, "leaf", (0.0, 0.0, 0.92), sides=8),
    ]
    for p in parts:
        p.parent = root
    return root


def berry_bush():
    """A low round bush dotted with red berries."""
    root = kit.empty("BerryBush")
    parts = [
        kit.cylinder("bush_low", 0.30, 0.18, "leaf_dark", sides=8, top_radius=0.33),
        kit.cylinder("bush_top", 0.33, 0.14, "leaf", (0.0, 0.0, 0.18), sides=8, top_radius=0.2),
    ]
    import math as _m
    for i in range(9):
        a = 2 * _m.pi * i / 9 + 0.3
        r = 0.26 if i % 2 else 0.18
        z = 0.14 if i % 2 else 0.26
        parts.append(kit.box("bush_berry_%d" % i, (0.06, 0.06, 0.06), "berry",
                             (r * _m.cos(a), r * _m.sin(a), z)))
    for p in parts:
        p.parent = root
    return root


def _vein(name, rock, fleck):
    """A heap of broken rock with flecks of what it is worth."""
    import random
    rng = random.Random(7 if fleck == "gold" else 11)
    root = kit.empty(name)
    parts = []
    for i, (x, y, s) in enumerate([(0.0, 0.0, 0.34), (-0.2, 0.12, 0.24), (0.18, -0.15, 0.26),
                                   (0.12, 0.2, 0.2), (-0.15, -0.18, 0.2)]):
        parts.append(kit.box("%s_rock_%d" % (name, i), (s, s * 0.9, s * 0.8), rock if i % 2 else
                             "rock", (x, y, 0.0), rotation=(0.0, 0.0, rng.uniform(0.0, 1.5))))
    for i in range(11):
        parts.append(kit.box("%s_fleck_%d" % (name, i), (0.08, 0.08, 0.08), fleck,
                             (rng.uniform(-0.22, 0.22), rng.uniform(-0.22, 0.22),
                              rng.uniform(0.14, 0.28))))
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


# name: (builder, size class, "unit", "building", "wall", "gate" or "node")
SUBJECTS = {
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

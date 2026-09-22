//! Turns simulation state into a sorted list of sprite instances.

use sim::kinds;
use sim::{GatherPhase, NavState, Order, Simulation, Vec2Fx, DECAY_TICKS, RUBBLE_TICKS, TICK_MS};

/// How long the age-up sweep takes to cross a settlement, in ms.
pub const SWEEP_MS: u32 = 1800;
/// How long each building glows as the sweep passes it.
const GLOW_MS: u32 = 700;

/// The age-up light sweep (`docs/02` [GD-AGE-02]): presentation state the
/// app keeps, passed in so the scene is still a pure function of its inputs.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Sweep {
    /// Whose settlement lights up.
    pub player: u8,
    /// Milliseconds since the age completed.
    pub elapsed_ms: u32,
}

use crate::fog;
use crate::fx_to_f32;
use crate::iso;
use crate::palette;
use crate::sprites::{Anim, Atlas};

/// One sprite to draw, in world-screen space at 1×.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SpriteInstance {
    /// Top-left in world-screen px.
    pub x: f32,
    /// Top-left in world-screen px.
    pub y: f32,
    /// Size in px at 1×.
    pub w: f32,
    /// Size in px at 1×.
    pub h: f32,
    /// Atlas rectangle, px.
    pub u: u16,
    /// Atlas rectangle, px.
    pub v: u16,
    /// Atlas rectangle, px.
    pub uw: u16,
    /// Atlas rectangle, px.
    pub vh: u16,
    /// Palette row (0 neutral, 1..=8 players).
    pub row: u8,
    /// Mirror horizontally.
    pub flip: bool,
    /// Draw order; larger draws later (closer to the viewer).
    pub depth: f32,
    /// Entity slot, for stable ordering and picking; `u32::MAX` for overlays.
    pub slot: u32,
    /// Positioned in window pixels rather than world-screen space.
    pub screen: bool,
    /// Light out of 255: full for what is in sight, [`fog::EXPLORED`] for
    /// what is drawn from memory.
    pub light: u8,
}

/// A building the player is about to place.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Ghost {
    /// What.
    pub kind: sim::entity::KindId,
    /// Anchor tile.
    pub x: i32,
    /// Anchor tile.
    pub y: i32,
    /// Whether placement would succeed.
    pub ok: bool,
    /// Player colour row.
    pub row: u8,
    /// Who is placing, for checking each tile of a run.
    pub player: u8,
    /// The placing player's age, so the ghost is drawn as the building will be.
    pub age: u8,
    /// For a wall being dragged: the tile the run started on. The ghost
    /// then covers every tile of the straight run from there to `(x, y)`,
    /// each hatched for whether it can be placed (`UX-PLACE-03`).
    pub run: Option<(i32, i32)>,
}

/// What a frame shows besides the world: the selection, a placement
/// ghost, the age-up sweep, and whose eyes it is seen through.
#[derive(Clone, Copy, Default, Debug)]
pub struct SceneOptions<'a> {
    /// Slots drawn with selection rings.
    pub selected: &'a [u32],
    /// The building being placed.
    pub ghost: Option<Ghost>,
    /// The age-up light sweep in progress.
    pub sweep: Option<Sweep>,
    /// The player whose fog applies (`GD-FOG-01`): others' units and
    /// buildings only where in sight, buildings and nodes remembered where
    /// seen once, drawn dimmed. `None` shows everything, for tools.
    pub viewer: Option<u8>,
}

/// A frame's worth of sprites, sorted back to front.
#[derive(Clone, Default, Debug)]
pub struct Scene {
    /// World sprites in draw order.
    pub sprites: Vec<SpriteInstance>,
    /// Screen-space sprites (the HUD), drawn after the world in order.
    pub ui: Vec<SpriteInstance>,
}

impl Scene {
    /// Builds the scene. `prev` holds last tick's positions by slot for
    /// interpolation at `alpha`; pass `None` to draw current positions.
    pub fn build(sim: &Simulation, atlas: &Atlas, prev: Option<&[Vec2Fx]>, alpha: f32) -> Scene {
        Scene::build_with(sim, atlas, prev, alpha, &[], None)
    }

    /// Builds the scene with selection rings under `selected` slots and an
    /// optional placement ghost.
    pub fn build_with(
        sim: &Simulation,
        atlas: &Atlas,
        prev: Option<&[Vec2Fx]>,
        alpha: f32,
        selected: &[u32],
        ghost: Option<Ghost>,
    ) -> Scene {
        Scene::build_full(
            sim,
            atlas,
            prev,
            alpha,
            &SceneOptions {
                selected,
                ghost,
                ..SceneOptions::default()
            },
        )
    }

    /// Builds the scene with everything: selection rings, a placement
    /// ghost, the light sweep while an age-up is being celebrated, and the
    /// viewer's fog.
    pub fn build_full(
        sim: &Simulation,
        atlas: &Atlas,
        prev: Option<&[Vec2Fx]>,
        alpha: f32,
        opts: &SceneOptions,
    ) -> Scene {
        let SceneOptions {
            selected,
            ghost,
            sweep,
            viewer,
        } = *opts;
        let world = sim.world();
        let map = sim.map();
        let fog = viewer.and_then(|p| sim.fog(p));
        let mut sprites = Vec::with_capacity(world.len() + selected.len() + 2);
        // The sweep crosses the player's buildings left to right in screen
        // space, so it needs their extent before any of them is placed.
        let sweep_span = sweep.and_then(|s| {
            let xs = world
                .slots()
                .map(|s| s.index())
                .filter(|&i| world.owner[i] == s.player && kinds::info(world.kind[i]).footprint > 0)
                .map(|i| iso::project(fx_to_f32(world.pos[i].x), fx_to_f32(world.pos[i].y), 0.0).0);
            let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
            (lo <= hi).then_some((s, lo, hi))
        });
        for slot in world.slots() {
            let i = slot.index();
            if world.inside[i].is_some() {
                // Garrisoned: inside, and nothing to draw.
                continue;
            }
            let kind = world.kind[i];
            let info = kinds::info(kind);
            if let Some(f) = fog {
                // Someone else's is drawn only with a tile of it in sight;
                // the viewer's own are always in sight of themselves.
                let (ax, ay) = sim::nav::anchor_tile(world.pos[i], info.footprint as i32);
                if Some(world.owner[i]) != viewer && !f.in_sight(kind, ax, ay) {
                    continue;
                }
            }
            // A site rises in three stages (`docs/03` §6.2): pegs, then the
            // building's lower half, then most of it, then the building.
            let stage = world.construction[i].map(|done| {
                let total = info.build_work().max(1);
                (done / (total / 3).max(1)).min(2)
            });
            // Buildings and villagers wear their owner's age.
            let age = sim
                .player(world.owner[i])
                .map_or(0, |p| p.age.index() as u8);
            let look = atlas.variant(kind, age);
            // What the unit is doing decides which animation plays; the clock
            // is game time plus a per-slot phase so a crowd does not march in
            // lockstep. Presentation only: nothing here feeds the simulation.
            let anim = if info.mobile {
                let walking = world.move_target[i].is_some()
                    || matches!(&world.nav[i], Some(n) if n.state == NavState::Walking);
                let working = matches!(
                    world.order[i],
                    Order::Gather {
                        phase: GatherPhase::Working,
                        ..
                    } | Order::Build { working: true, .. }
                ) || (world.reload[i] > 0
                    && info
                        .combat
                        .reload_ticks
                        .saturating_sub(world.reload[i] as u32)
                        < 6);
                if world.dying[i] > 0 {
                    // Falls, then lies: the death animation's length decides
                    // when the corpse frame takes over.
                    let dead_ms = (DECAY_TICKS - world.dying[i]) as u32 * TICK_MS;
                    let death_ms = atlas
                        .anim_info(look, Anim::Death)
                        .map_or(0, |a| a.frames * a.frame_ms);
                    if dead_ms < death_ms {
                        Anim::Death
                    } else {
                        Anim::Decay
                    }
                } else if walking {
                    Anim::Walk
                } else if working {
                    Anim::Work
                } else {
                    Anim::Idle
                }
            } else if kind == kinds::GATE && world.dying[i] == 0 && sim.gate_open(i) {
                // An open gate is the gate's work frame.
                Anim::Work
            } else {
                Anim::Idle
            };
            let time_ms = if world.dying[i] > 0 {
                let span = if info.mobile {
                    DECAY_TICKS
                } else {
                    RUBBLE_TICKS
                };
                (span - world.dying[i]) as u32 * TICK_MS + (alpha * TICK_MS as f32) as u32
            } else if anim == Anim::Work && info.combat.attack > 0 && world.reload[i] > 0 {
                info.combat
                    .reload_ticks
                    .saturating_sub(world.reload[i] as u32)
                    * TICK_MS
                    + (alpha * TICK_MS as f32) as u32
            } else {
                sim.tick() as u32 * TICK_MS
                    + (alpha * TICK_MS as f32) as u32
                    + (i as u32 * 61) % 1000
            };
            let looked_up = if stage == Some(0) {
                atlas.site(info.footprint).map(|f| (f, false))
            } else if !info.mobile && world.dying[i] > 0 {
                // Rubble where it stood, for as long as it lies.
                atlas.rubble(info.footprint).map(|f| (f, false))
            } else {
                atlas.frame_at(look, world.facing[i], anim, time_ms)
            };
            let Some((frame, flip)) = looked_up else {
                continue;
            };
            let cur = world.pos[i];
            let (px, py) = match prev {
                Some(p) if i < p.len() && kinds::info(kind).mobile && close(p[i], cur) => {
                    let (ax, ay) = (fx_to_f32(p[i].x), fx_to_f32(p[i].y));
                    let (bx, by) = (fx_to_f32(cur.x), fx_to_f32(cur.y));
                    (ax + (bx - ax) * alpha, ay + (by - ay) * alpha)
                }
                _ => (fx_to_f32(cur.x), fx_to_f32(cur.y)),
            };
            let h = iso::ground_height(map, px, py);
            let (gx, gy) = iso::project(px, py, h);
            let (ax, ay) = frame.draw_anchor();
            let anchor_x = if flip { frame.draw_w() - ax } else { ax };
            let footprint = kinds::info(kind).footprint.max(1) as f32;
            // Corpses lie under whatever walks over them.
            let depth =
                px + py + (footprint - 1.0) * 0.5 - if world.dying[i] > 0 { 0.5 } else { 0.0 };
            let row = palette::row_for_owner(world.owner[i]);
            if selected.contains(&(i as u32)) {
                if let Some(r) = atlas.ring(kinds::info(kind).footprint) {
                    sprites.push(overlay(r, gx, gy, row, depth - 0.01, i as u32));
                }
            }
            if let Some((s, lo, hi)) = sweep_span {
                if world.owner[i] == s.player
                    && info.footprint > 0
                    && world.construction[i].is_none()
                {
                    // Each building lights up in turn as the front passes it.
                    let along = if hi > lo { (gx - lo) / (hi - lo) } else { 0.0 };
                    let starts = (along * (SWEEP_MS - GLOW_MS) as f32) as u32;
                    if (starts..starts + GLOW_MS).contains(&s.elapsed_ms) {
                        // Over the building, not under it: the hatch reads
                        // as light on the walls, and under it nothing shows.
                        if let Some(g) = atlas.glow(info.footprint) {
                            sprites.push(overlay(g, gx, gy, 0, depth + 0.02, i as u32));
                        }
                    }
                }
            }
            // A node being used up is drawn smaller as it goes: a bush
            // thins, a vein shrinks, down to half. Trees fall instead
            // (`feedback.rs`) and farms are reseeded.
            let worn = match info.resource {
                Some((_, base))
                    if kind != kinds::FARM && kind != kinds::TREE && world.dying[i] == 0 =>
                {
                    let left = world.resource[i].max(0) as f32 / base.max(1) as f32;
                    0.5 + 0.5 * left.min(1.0)
                }
                _ => 1.0,
            };
            let mut sprite = SpriteInstance {
                x: (gx - anchor_x * worn).round(),
                y: (gy - ay * worn).round(),
                w: frame.draw_w() * worn,
                h: frame.draw_h() * worn,
                u: frame.x,
                v: frame.y,
                uw: frame.w,
                vh: frame.h,
                row,
                flip,
                depth,
                slot: i as u32,
                screen: false,
                light: fog::VISIBLE,
            };
            // Rising: only the lower part of the building is there yet.
            if let Some(stage @ 1..=2) = stage {
                let keep = if stage == 1 { 0.5 } else { 0.85 };
                let cut = (f32::from(frame.h) * (1.0 - keep)) as u16;
                sprite.v += cut;
                sprite.vh -= cut;
                let full = sprite.h;
                sprite.h = (full * keep).round();
                sprite.y += full - sprite.h;
            }
            sprites.push(sprite);
        }
        // What was seen once and is out of sight now, as it was then, in the
        // explored light: buildings and nodes only, never a unit, and not
        // pickable (`GD-FOG-01`).
        if let Some(f) = fog {
            for ((x, y), m) in f.memories() {
                let info = kinds::info(m.kind);
                let fp = info.footprint.max(1) as i32;
                let frame = if m.site {
                    atlas.site(info.footprint)
                } else {
                    atlas
                        .frame(atlas.variant(m.kind, m.age), 0)
                        .map(|(frame, _)| frame)
                };
                let Some(frame) = frame else {
                    continue;
                };
                let centre = sim::nav::building_centre(x, y, fp);
                let (cx, cy) = (fx_to_f32(centre.x), fx_to_f32(centre.y));
                let h = iso::ground_height(map, cx, cy);
                let (gx, gy) = iso::project(cx, cy, h);
                let depth = cx + cy + (fp as f32 - 1.0) * 0.5;
                let mut s = overlay(
                    frame,
                    gx,
                    gy,
                    palette::row_for_owner(m.owner),
                    depth,
                    u32::MAX,
                );
                s.light = fog::EXPLORED;
                sprites.push(s);
            }
        }
        // Direction and team-coloured fletching connect flight to its source.
        for p in sim.projectiles() {
            if fog.is_some_and(|f| !f.visible(p.pos.x.floor(), p.pos.y.floor())) {
                continue;
            }
            let facing = (p.aim - p.pos).angle().facing8();
            if let Some((arrow, flip)) = atlas.arrow(facing) {
                let (x, y) = (fx_to_f32(p.pos.x), fx_to_f32(p.pos.y));
                let h = iso::ground_height(map, x, y);
                let (gx, gy) = iso::project(x, y, h);
                let mut sprite = overlay(
                    arrow,
                    gx,
                    gy - 24.0,
                    palette::row_for_owner(p.owner),
                    x + y + 0.75,
                    u32::MAX,
                );
                sprite.flip = flip;
                sprites.push(sprite);
            }
        }
        if let Some(g) = ghost {
            let fp = kinds::info(g.kind).footprint.max(1);
            // One tile, or the run being dragged: every tile of it, each
            // checked on its own.
            // Ground never seen cannot be built on: nothing is known of it.
            let known = |t: (i32, i32)| fog.is_none_or(|f| f.explored(t.0, t.1));
            let tiles: Vec<((i32, i32), bool)> = match g.run {
                Some(from) => sim::nav::line_tiles(from, (g.x, g.y))
                    .into_iter()
                    .map(|t| {
                        (
                            t,
                            known(t) && sim.can_place(g.player, g.kind, t.0, t.1).is_ok(),
                        )
                    })
                    .collect(),
                None => vec![((g.x, g.y), g.ok && known((g.x, g.y)))],
            };
            for ((x, y), ok) in tiles {
                let centre = sim::nav::building_centre(x, y, fp as i32);
                let (cx, cy) = (fx_to_f32(centre.x), fx_to_f32(centre.y));
                let h = iso::ground_height(map, cx, cy);
                let (gx, gy) = iso::project(cx, cy, h);
                let depth = cx + cy + (fp as f32 - 1.0) * 0.5;
                if let Some((frame, _)) = atlas.frame(atlas.variant(g.kind, g.age), 0) {
                    sprites.push(overlay(frame, gx, gy, g.row, depth + 0.5, u32::MAX));
                }
                // The valid/blocked hatch draws over the ghost so it always shows.
                if let Some(f) = atlas.footprint(fp, ok) {
                    sprites.push(overlay(f, gx, gy, 0, depth + 0.51, u32::MAX));
                }
            }
        }
        waypoint_marks(sim, atlas, selected, &mut sprites);
        sprites.sort_by(|a, b| a.depth.total_cmp(&b.depth).then(a.slot.cmp(&b.slot)));
        Scene {
            sprites,
            ui: Vec::new(),
        }
    }
}

/// Where an order is headed, for the line a selected unit's queued jobs
/// are drawn along (`UX-CMD-11`): the point of a move, the thing of a
/// gather, build, repair, attack or garrison while it stands.
pub fn order_point(sim: &Simulation, order: &Order) -> Option<Vec2Fx> {
    let world = sim.world();
    let of = |id: sim::EntityId| world.slot(id).map(|s| world.pos[s.index()]);
    match *order {
        Order::Idle => None,
        Order::Move { target } | Order::AttackMove { target } | Order::Flee { target, .. } => {
            Some(target)
        }
        Order::Patrol { to, .. } => Some(to),
        Order::Attack { target, .. } => of(target),
        Order::Garrison { building } | Order::Repair { building, .. } => of(building),
        Order::Gather { node, .. } => of(node),
        Order::Build { site, .. } => of(site),
    }
}

/// Every queued job is shown (`UX-CMD-11`): for each selected unit with
/// something queued, a dotted line from where it is through where its
/// current job and each queued one is headed, with a flag at each of
/// those. A queued building is its site, already standing at its pegs.
fn waypoint_marks(
    sim: &Simulation,
    atlas: &Atlas,
    selected: &[u32],
    sprites: &mut Vec<SpriteInstance>,
) {
    let world = sim.world();
    let map = sim.map();
    let dot = *atlas.solid(palette::WHITE);
    let flag = *atlas.solid(palette::GOLD);
    let edge = *atlas.solid(palette::BLACK);
    let mut mark = |frame: &crate::sprites::Frame, x: f32, y: f32, size: f32, lift: f32| {
        let (gx, gy) = iso::project(x, y, iso::ground_height(map, x, y));
        sprites.push(SpriteInstance {
            x: (gx - size / 2.0).round(),
            y: (gy - size / 2.0 - lift).round(),
            w: size,
            h: size,
            u: frame.x,
            v: frame.y,
            uw: frame.w,
            vh: frame.h,
            row: 0,
            flip: false,
            // The dots lie on the ground, under whatever stands there; a
            // flag is lifted and drawn over the thing it marks, or it
            // would hide under the bush or the site it points at.
            depth: if lift > 0.0 { x + y + 1.0 } else { x + y - 0.6 },
            slot: u32::MAX,
            screen: false,
            light: 255,
        });
    };
    for &slot in selected {
        let i = slot as usize;
        if !world.is_live(i) || world.queue_at(i).is_empty() || world.dying[i] > 0 {
            continue;
        }
        let mut points: Vec<Vec2Fx> = Vec::new();
        points.extend(order_point(sim, &world.order[i]));
        for pending in world.queue_at(i) {
            points.extend(order_point(sim, &pending.order));
        }
        let mut from = (fx_to_f32(world.pos[i].x), fx_to_f32(world.pos[i].y));
        for (n, p) in points.iter().enumerate() {
            let to = (fx_to_f32(p.x), fx_to_f32(p.y));
            let (dx, dy) = (to.0 - from.0, to.1 - from.1);
            let len = (dx * dx + dy * dy).sqrt();
            let steps = (len / 0.5).floor() as usize;
            for k in 1..steps {
                let t = k as f32 / steps as f32;
                mark(&dot, from.0 + dx * t, from.1 + dy * t, 2.0, 0.0);
            }
            // The flag: a gold square on a black one, at every point after
            // the current job's, which the unit is already headed for.
            if n > 0 || points.len() == world.queue_at(i).len() {
                mark(&edge, to.0, to.1, 8.0, 14.0);
                mark(&flag, to.0, to.1, 6.0, 14.0);
                // The pole.
                mark(&edge, to.0, to.1, 2.0, 6.0);
            }
            from = to;
        }
    }
}

/// A world-space overlay sprite anchored on a ground point.
pub(crate) fn overlay(
    frame: &crate::sprites::Frame,
    gx: f32,
    gy: f32,
    row: u8,
    depth: f32,
    slot: u32,
) -> SpriteInstance {
    let (ax, ay) = frame.draw_anchor();
    SpriteInstance {
        x: (gx - ax).round(),
        y: (gy - ay).round(),
        w: frame.draw_w(),
        h: frame.draw_h(),
        u: frame.x,
        v: frame.y,
        uw: frame.w,
        vh: frame.h,
        row,
        flip: false,
        depth,
        slot,
        screen: false,
        light: fog::VISIBLE,
    }
}

/// Interpolation guard: only blend positions that are plausibly one tick
/// apart, so a respawned or teleported unit does not smear across the map.
fn close(a: Vec2Fx, b: Vec2Fx) -> bool {
    a.manhattan(b) < sim::Fx::from_int(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{MapKind, MapSpec, SimConfig};

    #[test]
    fn scene_matches_world_and_is_depth_sorted() {
        let sim = Simulation::new(5, SimConfig::default());
        let atlas = Atlas::placeholder();
        let scene = Scene::build(&sim, &atlas, None, 0.0);
        assert_eq!(scene.sprites.len(), sim.world().len());
        for w in scene.sprites.windows(2) {
            assert!(w[0].depth <= w[1].depth);
        }
        let tc = scene
            .sprites
            .iter()
            .find(|s| sim.world().kind[s.slot as usize] == kinds::TOWN_CENTER)
            .unwrap();
        assert_eq!((tc.uw, tc.vh), (192, 144));
        assert_eq!(tc.row, 1, "player 0 draws with palette row 1");
        let tree = scene
            .sprites
            .iter()
            .find(|s| sim.world().kind[s.slot as usize] == kinds::TREE)
            .unwrap();
        assert_eq!(tree.row, 0, "gaia draws neutral");
    }

    #[test]
    fn selection_rings_and_ghosts_are_added_in_order() {
        let sim = Simulation::new(5, SimConfig::default());
        let atlas = Atlas::placeholder();
        let first = sim.world().slots().next().unwrap().index() as u32;
        let (sx, sy) = sim.starts()[0];
        let ghost = Ghost {
            kind: kinds::HOUSE,
            x: sx + 4,
            y: sy,
            ok: sim.can_place(0, kinds::HOUSE, sx + 4, sy).is_ok(),
            row: 1,
            player: 0,
            age: 0,
            run: None,
        };
        let scene = Scene::build_with(&sim, &atlas, None, 0.0, &[first], Some(ghost));
        assert_eq!(scene.sprites.len(), sim.world().len() + 3);
        let fp = kinds::info(sim.world().kind[first as usize]).footprint;
        let ring = *atlas.ring(fp).unwrap();
        let is_ring = |s: &SpriteInstance| s.slot == first && s.u == ring.x && s.v == ring.y;
        let ring_pos = scene.sprites.iter().position(is_ring).unwrap();
        let unit_pos = scene
            .sprites
            .iter()
            .position(|s| s.slot == first && !is_ring(s))
            .unwrap();
        assert!(ring_pos < unit_pos, "ring draws under its unit");
        assert_eq!(
            scene.sprites.iter().filter(|s| s.slot == u32::MAX).count(),
            2,
            "footprint and ghost building"
        );
    }

    /// The visible half of an age-up: every building of the player's swaps to
    /// its new-age variant, and the light sweep crosses them in turn.
    ///
    /// REQ: GD-AGE-02
    /// REQ: RM-M3-01
    #[test]
    fn buildings_wear_their_owners_age_and_the_sweep_lights_them() {
        let mut sim = Simulation::new(
            5,
            SimConfig {
                starting_stockpile: [5000; 4],
                wander: false,
                ..SimConfig::default()
            },
        );
        let atlas = Atlas::placeholder();
        let tc_of = |scene: &Scene, sim: &Simulation| {
            scene
                .sprites
                .iter()
                .find(|s| {
                    s.slot != u32::MAX && sim.world().kind[s.slot as usize] == kinds::TOWN_CENTER
                })
                .map(|s| (s.u, s.v))
                .unwrap()
        };
        let stone = tc_of(&Scene::build(&sim, &atlas, None, 0.0), &sim);

        // Two Stone Age buildings, then the advance, then sixty seconds.
        let (sx, sy) = sim.starts()[0];
        let fp = kinds::info(kinds::BARRACKS).footprint as i32;
        for (dx, kind) in [(6, kinds::BARRACKS), (12, kinds::STOREHOUSE)] {
            sim.issue(sim::Command {
                player: 0,
                kind: sim::CommandKind::Spawn {
                    kind,
                    pos: sim::nav::building_centre(sx + dx, sy, fp),
                },
            });
        }
        for _ in 0..3 {
            sim.step();
        }
        let tc = sim
            .world()
            .slots()
            .find(|s| sim.world().kind[s.index()] == kinds::TOWN_CENTER)
            .map(|s| sim.world().id_at(s))
            .unwrap();
        sim.issue(sim::Command {
            player: 0,
            kind: sim::CommandKind::Research {
                building: tc,
                tech: sim::tech::AGE_TOOL,
            },
        });
        for _ in 0..(sim::tech::info(sim::tech::AGE_TOOL).unwrap().ticks() + 5) {
            sim.step();
        }
        assert_eq!(sim.player(0).unwrap().age, sim::Age::Tool);
        let tool = tc_of(&Scene::build(&sim, &atlas, None, 0.0), &sim);
        assert_ne!(stone, tool, "the Tool Age Town Center is a different frame");

        let sweep = Some(Sweep {
            player: 0,
            elapsed_ms: 10,
        });
        let lit = Scene::build_full(
            &sim,
            &atlas,
            None,
            0.0,
            &SceneOptions {
                sweep,
                ..SceneOptions::default()
            },
        );
        let glow = *atlas
            .glow(kinds::info(kinds::TOWN_CENTER).footprint)
            .unwrap();
        let glows = lit
            .sprites
            .iter()
            .filter(|s| s.u == glow.x && s.v == glow.y)
            .count();
        assert!(
            glows >= 1,
            "at the start of the sweep the leftmost building glows"
        );
        let done = Scene::build_full(
            &sim,
            &atlas,
            None,
            0.0,
            &SceneOptions {
                sweep: Some(Sweep {
                    player: 0,
                    elapsed_ms: SWEEP_MS + 1,
                }),
                ..SceneOptions::default()
            },
        );
        assert_eq!(
            done.sprites.len(),
            lit.sprites.len() - glows,
            "and then it is over"
        );
    }

    /// Through a player's eyes: someone else's unit out of sight is not
    /// drawn, their building in sight is drawn live, and once out of sight
    /// it is drawn from memory, dimmed, in the age it was seen, and not
    /// pickable. Without a viewer everything is drawn.
    ///
    /// REQ: GD-FOG-01
    #[test]
    fn a_fogged_scene_draws_what_is_in_sight_and_remembers_the_rest_dimmed() {
        let mut sim = Simulation::new(
            3,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 64,
                    players: 2,
                },
                wander: false,
                ..SimConfig::default()
            },
        );
        let spawn = |sim: &mut Simulation, player: u8, kind, pos: Vec2Fx| {
            sim.issue(sim::Command {
                player,
                kind: sim::CommandKind::Spawn { kind, pos },
            });
        };
        spawn(&mut sim, 0, kinds::CLUBMAN, sim::nav::centre((10, 10)));
        spawn(&mut sim, 1, kinds::CLUBMAN, sim::nav::centre((40, 40)));
        let fp = kinds::info(kinds::HOUSE).footprint as i32;
        spawn(
            &mut sim,
            1,
            kinds::HOUSE,
            sim::nav::building_centre(12, 12, fp),
        );
        for _ in 0..3 {
            sim.step();
        }
        let atlas = Atlas::placeholder();
        let kind_of = |sim: &Simulation, s: &SpriteInstance| {
            (s.slot != u32::MAX).then(|| sim.world().kind[s.slot as usize])
        };
        let all = Scene::build(&sim, &atlas, None, 0.0);
        assert_eq!(all.sprites.len(), 3, "no viewer: everything");
        let mine = Scene::build_full(
            &sim,
            &atlas,
            None,
            0.0,
            &SceneOptions {
                viewer: Some(0),
                ..SceneOptions::default()
            },
        );
        let kinds_seen: Vec<_> = mine.sprites.iter().map(|s| kind_of(&sim, s)).collect();
        assert_eq!(kinds_seen.len(), 2, "{kinds_seen:?}");
        assert!(kinds_seen.contains(&Some(kinds::HOUSE)), "in sight: live");
        assert!(mine.sprites.iter().all(|s| s.light == fog::VISIBLE));
        let house_live = mine
            .sprites
            .iter()
            .find(|s| kind_of(&sim, s) == Some(kinds::HOUSE))
            .copied()
            .unwrap();

        // Walk away: the house is a memory, their clubman still unseen.
        let me = sim
            .world()
            .slots()
            .find(|s| sim.world().owner[s.index()] == 0)
            .map(|s| sim.world().id_at(s))
            .unwrap();
        sim.issue(sim::Command {
            player: 0,
            kind: sim::CommandKind::Move {
                ids: vec![me],
                target: sim::nav::centre((30, 10)),
            },
        });
        for _ in 0..300 {
            sim.step();
        }
        let later = Scene::build_full(
            &sim,
            &atlas,
            None,
            0.0,
            &SceneOptions {
                viewer: Some(0),
                ..SceneOptions::default()
            },
        );
        assert_eq!(later.sprites.len(), 2, "{:?}", later.sprites);
        let remembered = later.sprites.iter().find(|s| s.slot == u32::MAX).unwrap();
        assert_eq!(remembered.light, fog::EXPLORED, "drawn from memory: dimmed");
        assert_eq!(
            (remembered.u, remembered.v, remembered.x, remembered.y),
            (house_live.u, house_live.v, house_live.x, house_live.y),
            "the same frame where it stood"
        );
        assert_eq!(remembered.row, palette::row_for_owner(1));
        assert!(
            later
                .sprites
                .iter()
                .all(|s| kind_of(&sim, s) != Some(kinds::HOUSE)),
            "not the live building"
        );
        // Their side sees none of mine, and remembers nothing of mine.
        let theirs = Scene::build_full(
            &sim,
            &atlas,
            None,
            0.0,
            &SceneOptions {
                viewer: Some(1),
                ..SceneOptions::default()
            },
        );
        assert!(theirs
            .sprites
            .iter()
            .all(|s| s.slot != u32::MAX && sim.world().owner[s.slot as usize] == 1));
    }

    /// A site is pegs for its first third, the building's lower half for
    /// the second, most of it for the last, and the building when done;
    /// a bush being eaten is drawn smaller as it goes, never below half.
    #[test]
    fn sites_rise_in_three_stages_and_nodes_shrink_as_they_are_used() {
        use sim::{Command, CommandKind, MapKind, MapSpec, SimConfig};
        let atlas = Atlas::placeholder();
        let mut sim = Simulation::new(
            5,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 48,
                    players: 1,
                },
                starting_stockpile: [5000; 4],
                wander: false,
                ..SimConfig::default()
            },
        );
        let spawn = |sim: &mut Simulation, kind, x, y| {
            sim.issue(Command {
                player: 0,
                kind: CommandKind::Spawn {
                    kind,
                    pos: sim::nav::centre((x, y)),
                },
            });
            for _ in 0..3 {
                sim.step();
            }
            let w = sim.world();
            w.slots()
                .filter(|s| w.kind[s.index()] == kind)
                .map(|s| w.id_at(s))
                .last()
                .unwrap()
        };
        let builders: Vec<_> = (0..5)
            .map(|k| spawn(&mut sim, kinds::VILLAGER, 10 + k, 10))
            .collect();
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Build {
                kind: kinds::HOUSE,
                x: 12,
                y: 13,
                ids: builders,
            },
        });
        // The command lands two ticks on.
        for _ in 0..3 {
            sim.step();
        }
        let site_slot = |sim: &Simulation| {
            let w = sim.world();
            w.slots()
                .find(|s| w.kind[s.index()] == kinds::HOUSE)
                .map(|s| s.index())
                .unwrap()
        };
        let house = |scene: &Scene, slot: usize| {
            scene
                .sprites
                .iter()
                .find(|s| s.slot == slot as u32)
                .cloned()
                .expect("the site is drawn")
        };
        let total = kinds::info(kinds::HOUSE).build_work().max(1);
        let mut seen = [false; 4];
        let (mut half_h, mut full_h) = (0.0_f32, 0.0_f32);
        for _ in 0..2000 {
            sim.step();
            let i = site_slot(&sim);
            let scene = Scene::build(&sim, &atlas, None, 0.0);
            let s = house(&scene, i);
            let pegs = atlas.site(kinds::info(kinds::HOUSE).footprint).unwrap();
            let (frame, _) = atlas.frame(kinds::HOUSE, 0).unwrap();
            match sim.world().construction[i] {
                Some(done) if done < total / 3 => {
                    assert_eq!((s.u, s.v), (pegs.x, pegs.y), "pegs");
                    seen[0] = true;
                }
                Some(done) if done < 2 * (total / 3) => {
                    assert_eq!(s.u, frame.x, "the building's own frame");
                    assert!(s.v > frame.y, "clipped from the top");
                    assert!(
                        (s.h - (frame.draw_h() * 0.5).round()).abs() <= 1.0,
                        "half: {}",
                        s.h
                    );
                    half_h = s.h;
                    seen[1] = true;
                }
                Some(_) => {
                    assert!(s.h > half_h && s.h < frame.draw_h(), "most of it: {}", s.h);
                    seen[2] = true;
                }
                None => {
                    assert_eq!((s.u, s.v, s.h), (frame.x, frame.y, frame.draw_h()), "done");
                    full_h = s.h;
                    seen[3] = true;
                }
            }
            if seen[3] {
                break;
            }
        }
        assert_eq!(seen, [true; 4], "every stage was drawn");
        assert!(full_h > half_h);
        // The bush.
        let bush = spawn(&mut sim, kinds::BERRY_BUSH, 20, 20);
        let bush_slot = sim.world().slot(bush).unwrap().index();
        let eaters: Vec<_> = (0..4)
            .map(|k| spawn(&mut sim, kinds::VILLAGER, 18 + k, 18))
            .collect();
        spawn(&mut sim, kinds::STOREHOUSE, 22, 22);
        let (frame, _) = atlas.frame(kinds::BERRY_BUSH, 0).unwrap();
        let before = house(&Scene::build(&sim, &atlas, None, 0.0), bush_slot);
        assert_eq!(before.w, frame.draw_w(), "whole");
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Gather {
                ids: eaters,
                node: bush,
            },
        });
        for _ in 0..1200 {
            sim.step();
        }
        let left = sim.world().resource[bush_slot];
        let base = kinds::info(kinds::BERRY_BUSH).resource.unwrap().1;
        assert!(left > 0 && left < base, "partly eaten: {left} of {base}");
        let after = house(&Scene::build(&sim, &atlas, None, 0.0), bush_slot);
        assert!(
            after.w < before.w && after.w >= before.w * 0.5,
            "thinned: {} of {}",
            after.w,
            before.w
        );
        assert!(after.y > before.y, "still on the ground");
    }

    #[test]
    fn interpolation_blends_only_mobile_and_nearby() {
        let mut sim = Simulation::new(
            1,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 64,
                    players: 1,
                },
                wander: false,
                ..SimConfig::default()
            },
        );
        sim.issue(sim::Command {
            player: 0,
            kind: sim::CommandKind::Spawn {
                kind: kinds::VILLAGER,
                pos: Vec2Fx::from_int(10, 10),
            },
        });
        for _ in 0..3 {
            sim.step();
        }
        let atlas = Atlas::placeholder();
        let prev = vec![Vec2Fx::from_int(9, 10)];
        let half = Scene::build(&sim, &atlas, Some(&prev), 0.5);
        let none = Scene::build(&sim, &atlas, None, 0.5);
        let (gx_half, _) = (half.sprites[0].x, half.sprites[0].y);
        let (gx_none, _) = (none.sprites[0].x, none.sprites[0].y);
        assert!(
            gx_half < gx_none,
            "halfway between x=9 and x=10 is further left"
        );
        let far = vec![Vec2Fx::from_int(50, 50)];
        let guarded = Scene::build(&sim, &atlas, Some(&far), 0.5);
        assert_eq!(
            guarded.sprites[0].x, gx_none,
            "teleports are not interpolated"
        );
    }
}

//! Turns simulation state into a sorted list of sprite instances.

use sim::kinds;
use sim::{GatherPhase, NavState, Order, Simulation, Vec2Fx, RUBBLE_TICKS, TICK_MS};

/// How long a building takes to come down into its rubble, in ticks: under
/// the dust cloud (`feedback.rs`), which outlasts it.
pub const COLLAPSE_TICKS: u16 = 16;

/// How long the age-up sweep takes to cross a settlement, in ms.
pub const SWEEP_MS: u32 = 1800;
/// How long each building glows as the sweep passes it.
const GLOW_MS: u32 = 700;
/// Added to a player's symbol's depth, so it draws over the world: a
/// symbol hidden behind a nearer unit tells nobody anything.
const SYMBOL_DEPTH: f32 = 10_000.0;

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
use crate::sprites::{self, Anim, Atlas};
use crate::symbols::PlayerSymbols;
use crate::walls::{self, Piece};
use std::collections::HashMap;

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
    /// Atlas page.
    pub page: u8,
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

/// The architecture `p` builds in (`docs/02` §11), by
/// [`sprites::arch_index`]: the first set's when the match named no
/// civilization.
fn arch_of(sim: &Simulation, p: sim::PlayerId) -> u8 {
    sim.civ(p)
        .map_or(0, |c| sprites::arch_index(c.info().architecture))
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
    /// Where the players' symbols are drawn (`GD-A11Y-03`).
    pub symbols: PlayerSymbols,
    /// The renderers' clock, seconds ([`crate::ground::wrap_time`]): the
    /// time the water drifts by, which the smoke, fire and flags at the
    /// buildings' attachment points loop by too. 0 in tools and tests.
    pub time: f32,
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
            symbols,
            time,
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
        let pieces = wall_pieces(sim, fog, viewer);
        // What of `owner`'s stands on a tile, for joining a wall's tiles.
        let piece_of = |owner: u8| {
            let pieces = &pieces;
            move |x: i32, y: i32| {
                pieces
                    .get(&(x, y))
                    .and_then(|&(o, p)| (o == owner).then_some(p))
            }
        };
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
            let look = atlas.variant(kind, age, arch_of(sim, world.owner[i]));
            // A tree wears one of the tree's looks, by its tile, and stands
            // a little off the tile's centre (nothing, with no other looks).
            let tree_tile = (kind == kinds::TREE)
                .then(|| sim::nav::anchor_tile(world.pos[i], info.footprint as i32));
            let look = tree_tile.map_or(look, |(x, y)| atlas.tree_look(x, y));
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
                        | Order::Repair { working: true, .. }
                ) || (world.reload[i] > 0
                    && info.combat.attack > 0
                    && info
                        .combat
                        .reload_ticks
                        .saturating_sub(world.reload[i] as u32)
                        < 6)
                    // A priest at its chant (`GD-PRIEST-01`).
                    || matches!(world.order[i], Order::Convert { chant, .. } if chant > 0);
                if world.dying[i] > 0 {
                    // Falls, then lies: the death animation's length decides
                    // when the corpse frame takes over.
                    let dead_ms =
                        sim::decay_ticks(kind).saturating_sub(world.dying[i]) as u32 * TICK_MS;
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
            // A villager shows its job: the task it is at, the load it walks
            // home. A set without them swings and walks as before.
            let anim = match villager_anim(sim, i, anim) {
                Some(task) if atlas.anim_info(look, task).is_some() => task,
                _ => anim,
            };
            let time_ms = if world.dying[i] > 0 {
                let span = if info.mobile {
                    sim::decay_ticks(kind)
                } else {
                    RUBBLE_TICKS
                };
                span.saturating_sub(world.dying[i]) as u32 * TICK_MS
                    + (alpha * TICK_MS as f32) as u32
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
            // A rendered building draws its own stages and rubble; without
            // them, a site is pegs, then the building cropped, and rubble is
            // the footprint's.
            let own_stage = stage.and_then(|s| atlas.stage_frame(look, s as u8));
            let tile = sim::nav::anchor_tile(world.pos[i], info.footprint as i32);
            let at = piece_of(world.owner[i]);
            let standing = stage.is_none() && world.dying[i] == 0;
            // A gate stands across the line of its walls, shut or open.
            let gate = (kind == kinds::GATE && standing)
                .then(|| atlas.gate_frame(look, walls::gate_line(tile, &at), anim == Anim::Work))
                .flatten();
            let looked_up = if let Some(f) = own_stage {
                Some((f, false))
            } else if let Some(f) = gate {
                Some((f, false))
            } else if stage == Some(0) {
                atlas.site(info.footprint).map(|f| (f, false))
            } else if !info.mobile && world.dying[i] > 0 {
                // Rubble where it stood, for as long as it lies.
                atlas
                    .own_rubble(look)
                    .or_else(|| atlas.rubble(info.footprint))
                    .map(|f| (f, false))
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
            let (px, py) = match tree_tile {
                Some((x, y)) => {
                    let (ox, oy) = atlas.tree_offset(x, y);
                    (px + ox, py + oy)
                }
                None => (px, py),
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
                page: frame.page,
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
            if let (Some(stage @ 1..=2), None) = (stage, own_stage) {
                let keep = if stage == 1 { 0.5 } else { 0.85 };
                let cut = (f32::from(frame.h) * (1.0 - keep)) as u16;
                sprite.v += cut;
                sprite.vh -= cut;
                let full = sprite.h;
                sprite.h = (full * keep).round();
                sprite.y += full - sprite.h;
            }
            let top = sprite.y;
            sprites.push(sprite);
            // A colour wall's or gate's shadow lies under the wall's pieces.
            if (kinds::is_wall(kind) || kind == kinds::GATE) && standing {
                push_shadow(atlas, frame, (gx, gy), row, depth, i as u32, &mut sprites);
            }
            // Whose it is, by shape as well as colour (`GD-A11Y-03`): over
            // the selection, or over every unit and building but the walls.
            let marked = match symbols {
                PlayerSymbols::Off => false,
                PlayerSymbols::Selected => selected.contains(&(i as u32)),
                PlayerSymbols::Always => {
                    selected.contains(&(i as u32)) || !(kinds::is_wall(kind) || kind == kinds::GATE)
                }
            };
            if marked && row != 0 && world.dying[i] == 0 {
                if let Some(b) = atlas.symbol(world.owner[i]) {
                    // Above the sprite, and over whatever stands nearer.
                    sprites.push(overlay(
                        b,
                        gx,
                        top - 4.0,
                        row,
                        depth + SYMBOL_DEPTH,
                        i as u32,
                    ));
                }
            }
            // A building coming down: for its first moments it stands over
            // its rubble and sinks into it, faster as it goes. A site has no
            // building to bring down.
            if !info.mobile && world.dying[i] > 0 && world.construction[i].is_none() {
                let since = RUBBLE_TICKS.saturating_sub(world.dying[i]) as f32 + alpha;
                if since < COLLAPSE_TICKS as f32 {
                    if let Some((standing, _)) = atlas.frame_at(look, 0, Anim::Idle, 0) {
                        let t = since / COLLAPSE_TICKS as f32;
                        let mut s = overlay(standing, gx, gy, row, depth + 0.51, i as u32);
                        sink(&mut s, standing, t * t);
                        sprites.push(s);
                    }
                }
            }
            // Smoke from its hearths, its flags, and fire where it is hurt.
            if standing && !info.mobile {
                let hurt = fx_to_f32(world.health[i])
                    / sim.max_health_of(world.owner[i], kind).max(1) as f32;
                push_effects(
                    atlas,
                    look,
                    (gx, gy),
                    tile,
                    hurt,
                    row,
                    depth,
                    time,
                    &mut sprites,
                );
            }
            // A wall reaches for the walls beside it.
            if kinds::is_wall(kind) && standing {
                push_arms(
                    atlas,
                    look,
                    walls::wall_links(tile, &at),
                    (gx, gy),
                    row,
                    depth,
                    i as u32,
                    &mut sprites,
                );
            }
        }
        // What was seen once and is out of sight now, as it was then, in the
        // explored light: buildings and nodes only, never a unit, and not
        // pickable (`GD-FOG-01`).
        if let Some(f) = fog {
            for ((x, y), m) in f.memories() {
                let info = kinds::info(m.kind);
                let fp = info.footprint.max(1) as i32;
                let look = if m.kind == kinds::TREE {
                    atlas.tree_look(x, y)
                } else {
                    atlas.variant(m.kind, m.age, arch_of(sim, m.owner))
                };
                let at = piece_of(m.owner);
                let gate = (m.kind == kinds::GATE && !m.site)
                    .then(|| atlas.gate_frame(look, walls::gate_line((x, y), &at), false))
                    .flatten();
                let frame = if m.site {
                    atlas
                        .stage_frame(look, 0)
                        .or_else(|| atlas.site(info.footprint))
                } else {
                    gate.or_else(|| atlas.frame(look, 0).map(|(frame, _)| frame))
                };
                let Some(frame) = frame else {
                    continue;
                };
                let centre = sim::nav::building_centre(x, y, fp);
                let (cx, cy) = (fx_to_f32(centre.x), fx_to_f32(centre.y));
                let (cx, cy) = if m.kind == kinds::TREE {
                    let (ox, oy) = atlas.tree_offset(x, y);
                    (cx + ox, cy + oy)
                } else {
                    (cx, cy)
                };
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
                if (kinds::is_wall(m.kind) || m.kind == kinds::GATE) && !m.site {
                    let row = palette::row_for_owner(m.owner);
                    let from = sprites.len();
                    push_shadow(atlas, frame, (gx, gy), row, depth, u32::MAX, &mut sprites);
                    for shadow in &mut sprites[from..] {
                        shadow.light = fog::EXPLORED;
                    }
                }
                if kinds::is_wall(m.kind) && !m.site {
                    let from = sprites.len();
                    push_arms(
                        atlas,
                        look,
                        walls::wall_links((x, y), &at),
                        (gx, gy),
                        palette::row_for_owner(m.owner),
                        depth,
                        u32::MAX,
                        &mut sprites,
                    );
                    for arm in &mut sprites[from..] {
                        arm.light = fog::EXPLORED;
                    }
                }
            }
        }
        // Direction and team-coloured fletching connect flight to its source.
        for p in sim.projectiles() {
            if fog.is_some_and(|f| !f.visible(p.pos.x.floor(), p.pos.y.floor())) {
                continue;
            }
            // A stone is a stone whichever way it flies, and it flies high.
            if !p.blast.is_zero() {
                if let Some(stone) = atlas.stone() {
                    let (x, y) = (fx_to_f32(p.pos.x), fx_to_f32(p.pos.y));
                    let (gx, gy) = iso::project(x, y, iso::ground_height(map, x, y));
                    sprites.push(overlay(stone, gx, gy - 32.0, 0, x + y + 0.75, u32::MAX));
                }
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
            // The run joins up as it will stand, and onto the player's walls
            // at its ends; a gate turns along the walls it would go into.
            let run: Vec<(i32, i32)> = tiles.iter().map(|&(t, _)| t).collect();
            let at = |x: i32, y: i32| {
                if run.contains(&(x, y)) {
                    Some(Piece::Wall)
                } else {
                    piece_of(g.player)(x, y)
                }
            };
            let look = atlas.variant(g.kind, g.age, arch_of(sim, g.player));
            for ((x, y), ok) in tiles {
                let centre = sim::nav::building_centre(x, y, fp as i32);
                let (cx, cy) = (fx_to_f32(centre.x), fx_to_f32(centre.y));
                let h = iso::ground_height(map, cx, cy);
                let (gx, gy) = iso::project(cx, cy, h);
                let depth = cx + cy + (fp as f32 - 1.0) * 0.5;
                let gate = (g.kind == kinds::GATE)
                    .then(|| atlas.gate_frame(look, walls::gate_line((x, y), &at), false))
                    .flatten();
                if let Some(frame) = gate.or_else(|| atlas.frame(look, 0).map(|(f, _)| f)) {
                    sprites.push(overlay(frame, gx, gy, g.row, depth + 0.5, u32::MAX));
                    push_shadow(
                        atlas,
                        frame,
                        (gx, gy),
                        g.row,
                        depth + 0.5,
                        u32::MAX,
                        &mut sprites,
                    );
                }
                if kinds::is_wall(g.kind) {
                    push_arms(
                        atlas,
                        look,
                        walls::wall_links((x, y), &at),
                        (gx, gy),
                        g.row,
                        depth + 0.5,
                        u32::MAX,
                        &mut sprites,
                    );
                }
                // The valid/blocked hatch draws over the ghost so it always shows.
                if let Some(f) = atlas.footprint(fp, ok) {
                    sprites.push(overlay(f, gx, gy, 0, depth + 0.51, u32::MAX));
                }
            }
        }
        carried_relics(sim, atlas, fog, viewer, prev, alpha, &mut sprites);
        waypoint_marks(sim, atlas, selected, &mut sprites);
        sprites.sort_by(|a, b| a.depth.total_cmp(&b.depth).then(a.slot.cmp(&b.slot)));
        Scene {
            sprites,
            ui: Vec::new(),
        }
    }
}

/// A relic in a priest's hands is drawn over its head, so a carrier can be
/// picked out of a crowd (`GD-WIN-03`). A relic in a Temple is not drawn.
fn carried_relics(
    sim: &Simulation,
    atlas: &Atlas,
    fog: Option<&sim::Fog>,
    viewer: Option<u8>,
    prev_pos: Option<&[Vec2Fx]>,
    alpha: f32,
    sprites: &mut Vec<SpriteInstance>,
) {
    let world = sim.world();
    let Some((frame, _)) = atlas.frame(kinds::RELIC, 0) else {
        return;
    };
    for s in world.slots() {
        let r = s.index();
        if world.kind[r] != kinds::RELIC {
            continue;
        }
        let Some(cs) = world.inside[r].and_then(|c| world.slot(c)) else {
            continue;
        };
        let c = cs.index();
        if world.kind[c] != kinds::PRIEST || world.inside[c].is_some() || world.dying[c] > 0 {
            continue;
        }
        let now = world.pos[c];
        if let Some(f) = fog {
            let t = sim::nav::tile_of(now);
            if Some(world.owner[c]) != viewer && !f.in_sight(kinds::PRIEST, t.0, t.1) {
                continue;
            }
        }
        // Where the priest is drawn: between its last position and this
        // one, as the scene draws it.
        let was = prev_pos.and_then(|p| p.get(c)).copied().unwrap_or(now);
        let x = fx_to_f32(was.x) + (fx_to_f32(now.x) - fx_to_f32(was.x)) * alpha;
        let y = fx_to_f32(was.y) + (fx_to_f32(now.y) - fx_to_f32(was.y)) * alpha;
        let (gx, gy) = iso::project(x, y, iso::ground_height(sim.map(), x, y));
        sprites.push(overlay(frame, gx, gy - 40.0, 0, x + y + 0.02, c as u32));
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
        Order::Move { target }
        | Order::AttackMove { target }
        | Order::Flee { target, .. }
        | Order::Unload { at: target } => Some(target),
        Order::Patrol { to, .. } => Some(to),
        Order::Attack { target, .. }
        | Order::Convert { target, .. }
        | Order::Trade { market: target, .. } => of(target),
        Order::Relic { relic, temple } => temple.and_then(of).or_else(|| of(relic)),
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
            page: frame.page,
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

/// What villager `i` is doing, as the animation its set may have for it:
/// a task while `anim` is the work swing, a load while it is the walk.
fn villager_anim(sim: &Simulation, i: usize, anim: Anim) -> Option<Anim> {
    let world = sim.world();
    match anim {
        Anim::Walk => match world.carry[i] {
            Some((res, n)) if n > 0 => Some(match res {
                kinds::Resource::Food => Anim::CarryFood,
                kinds::Resource::Wood => Anim::CarryWood,
                kinds::Resource::Stone => Anim::CarryStone,
                kinds::Resource::Gold => Anim::CarryGold,
            }),
            _ => None,
        },
        Anim::Work => match world.order[i] {
            Order::Gather {
                node,
                phase: GatherPhase::Working,
                ..
            } => {
                let node = world.slot(node)?.index();
                Some(match world.kind[node] {
                    kinds::TREE => Anim::Chop,
                    kinds::GOLD_MINE | kinds::STONE_MINE => Anim::Mine,
                    kinds::FARM => Anim::Farm,
                    // A bush, or a carcass being butchered.
                    _ => Anim::Forage,
                })
            }
            Order::Build { working: true, .. } | Order::Repair { working: true, .. } => {
                Some(Anim::Build)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Lowers `sprite`, drawn from `frame`, `gone` (0 to 1) of its height into
/// the ground: its top rows drawn that much lower, its bottom cut away.
fn sink(sprite: &mut SpriteInstance, frame: &crate::sprites::Frame, gone: f32) {
    let cut = ((f32::from(frame.h) * gone.clamp(0.0, 1.0)) as u16).min(frame.h);
    let scale = f32::from(frame.scale.max(1));
    sprite.vh = frame.h - cut;
    sprite.h = f32::from(sprite.vh) / scale;
    sprite.y += (f32::from(cut) / scale).round();
}

/// The walls and gates in view or remembered, by tile, with their owners:
/// what a wall's tiles join to (`walls.rs`). Only what the viewer is shown
/// counts, so a wall's arm never points at one hidden in the fog.
fn wall_pieces(
    sim: &Simulation,
    fog: Option<&sim::fog::Fog>,
    viewer: Option<u8>,
) -> HashMap<(i32, i32), (u8, Piece)> {
    let piece = |kind| {
        if kinds::is_wall(kind) {
            Some(Piece::Wall)
        } else if kind == kinds::GATE {
            Some(Piece::Gate)
        } else {
            None
        }
    };
    let world = sim.world();
    let mut out = HashMap::new();
    for slot in world.slots() {
        let i = slot.index();
        let Some(p) = piece(world.kind[i]) else {
            continue;
        };
        if world.dying[i] > 0 {
            continue;
        }
        let (x, y) = sim::nav::anchor_tile(world.pos[i], 1);
        if fog.is_some_and(|f| Some(world.owner[i]) != viewer && !f.in_sight(world.kind[i], x, y)) {
            continue;
        }
        out.insert((x, y), (world.owner[i], p));
    }
    if let Some(f) = fog {
        for (t, m) in f.memories() {
            if let Some(p) = piece(m.kind) {
                out.entry(t).or_insert((m.owner, p));
            }
        }
    }
    out
}

/// A wall tile's arms toward the neighbours in `links`, placed on its post's
/// ground point and ordered about the post's `depth`.
#[allow(clippy::too_many_arguments)]
fn push_arms(
    atlas: &Atlas,
    look: sim::KindId,
    links: u8,
    (gx, gy): (f32, f32),
    row: u8,
    depth: f32,
    slot: u32,
    sprites: &mut Vec<SpriteInstance>,
) {
    for k in 0..walls::WALL_DIRECTIONS.len() {
        if links & 1 << k == 0 {
            continue;
        }
        if let Some(arm) = atlas.wall_arm(look, k as u8) {
            sprites.push(overlay(arm, gx, gy, row, depth + walls::arm_depth(k), slot));
            push_shadow(atlas, arm, (gx, gy), row, depth, slot, sprites);
        }
    }
}

/// How far in front of its building an effect is drawn: over the building
/// and under whatever stands nearer.
pub const FX_DEPTH: f32 = 0.03;

/// Health, as a share of the most, below which a building burns at its
/// fire points: one small fire below the first, two below the second,
/// every point below the third with the first two large, and every one
/// large below the last.
pub const BURNS_BELOW: [f32; 4] = [0.66, 0.5, 0.33, 0.15];

/// The effects to draw at a standing building's fire points at `health`
/// (its share of its most): one per fire point that burns, in order, large
/// or not.
pub fn fires(points: usize, health: f32) -> Vec<bool> {
    let [one, two, all, blaze] = BURNS_BELOW;
    let (n, large) = if health < blaze {
        (points, points)
    } else if health < all {
        (points, 2)
    } else if health < two {
        (2, 0)
    } else if health < one {
        (1, 0)
    } else {
        (0, 0)
    };
    (0..n.min(points)).map(|k| k < large).collect()
}

/// The smoke, flags and fires of a standing building drawn in `look` with
/// its ground point at `(gx, gy)` (`docs/look/buildings.md`, "Smoke, flags
/// and fire"): each effect's frame at the attachment point, from the
/// renderers' clock `time` (seconds) plus a phase by a hash of the
/// building's tile and the point, so no two chimneys puff in step; drawn
/// just in front of the building, never picked. A look without
/// attachment points (every one with the new art off) draws none.
#[allow(clippy::too_many_arguments)]
fn push_effects(
    atlas: &Atlas,
    look: sim::KindId,
    (gx, gy): (f32, f32),
    tile: (i32, i32),
    health: f32,
    row: u8,
    depth: f32,
    time: f32,
    sprites: &mut Vec<SpriteInstance>,
) {
    use crate::sheets::AttachKind;
    use sprites::Effect;
    let points = atlas.attachments(look);
    if points.is_empty() {
        return;
    }
    let fire_points = points.iter().filter(|p| p.kind == AttachKind::Fire).count();
    let burning = fires(fire_points, health);
    let clock = (time.max(0.0) * 1000.0) as u32;
    let mut nth_fire = 0;
    for (k, p) in points.iter().enumerate() {
        let (effect, owner_row) = match p.kind {
            AttachKind::Smoke => (Effect::Smoke, 0),
            AttachKind::SmokeDark => (Effect::SmokeDark, 0),
            AttachKind::Flag => (Effect::Flag, row),
            AttachKind::Fire => {
                let n = nth_fire;
                nth_fire += 1;
                match burning.get(n) {
                    Some(&true) => (Effect::FireLarge, 0),
                    Some(&false) => (Effect::FireSmall, 0),
                    None => continue,
                }
            }
        };
        let phase = sprites::tile_hash(tile.0, tile.1, 0xF0 + k as u32) % 60_000;
        let Some(frame) = atlas.effect(effect, clock.wrapping_add(phase)) else {
            continue;
        };
        sprites.push(overlay(
            frame,
            gx + p.dx,
            gy + p.dy,
            owner_row,
            depth + FX_DEPTH + k as f32 * 0.001,
            u32::MAX,
        ));
    }
}

/// How far behind its wall piece a colour wall's shadow is drawn
/// ([`Atlas::shadow_of`]): behind the pieces of the tile behind it (its
/// post at 1, the arm reaching back from it at 1.25) and in front of the
/// ground and what lies on it further back, so a post's shadow falling
/// along the wall darkens the ground under its neighbour's arm, not the
/// arm.
pub const WALL_SHADOW_DEPTH: f32 = 1.4;

/// The shadow of a wall piece drawn with `frame` at `depth`, if it casts
/// one apart (a colour wall or gate), anchored as the piece is.
fn push_shadow(
    atlas: &Atlas,
    frame: &crate::sprites::Frame,
    (gx, gy): (f32, f32),
    row: u8,
    depth: f32,
    slot: u32,
    sprites: &mut Vec<SpriteInstance>,
) {
    if let Some(shadow) = atlas.shadow_of(frame) {
        sprites.push(overlay(
            shadow,
            gx,
            gy,
            row,
            depth - WALL_SHADOW_DEPTH,
            slot,
        ));
    }
}

/// A world-space overlay sprite anchored on a ground point.
/// Marks over the tiles given, hatched green or red, as the scenario
/// editor's brush shows the ground it will paint (`GD-CAMP-06`). They are
/// world sprites, to be sorted in with the rest.
pub fn tile_marks(
    sim: &Simulation,
    atlas: &Atlas,
    tiles: &[(i32, i32)],
    ok: bool,
) -> Vec<SpriteInstance> {
    let map = sim.map();
    let Some(frame) = atlas.footprint(1, ok) else {
        return Vec::new();
    };
    tiles
        .iter()
        .map(|&(x, y)| {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let h = iso::ground_height(map, cx, cy);
            let (gx, gy) = iso::project(cx, cy, h);
            overlay(frame, gx, gy, 0, cx + cy + 0.51, u32::MAX)
        })
        .collect()
}

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
        page: frame.page,
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

    /// Whose a thing is shows by shape too (`GD-A11Y-03`): over the
    /// selection by default, over every unit and building but the walls
    /// with ALWAYS, nowhere with OFF, and never over nature's. Each in its
    /// owner's colour, over the world.
    /// REQ: GD-A11Y-03
    #[test]
    fn player_symbols_show_where_the_setting_says() {
        let sim = Simulation::new(5, SimConfig::default());
        let atlas = Atlas::placeholder();
        let world = sim.world();
        let theirs: Vec<u32> = world
            .slots()
            .map(|s| s.index() as u32)
            .filter(|&i| world.owner[i as usize] == 1)
            .collect();
        let selected = &theirs[..2];
        let symbols_in = |mode: PlayerSymbols| {
            let scene = Scene::build_full(
                &sim,
                &atlas,
                None,
                0.0,
                &SceneOptions {
                    selected,
                    symbols: mode,
                    ..SceneOptions::default()
                },
            );
            scene
                .sprites
                .into_iter()
                .filter(|s| {
                    (0..8u8).any(|o| {
                        let f = atlas.symbol(o).unwrap();
                        (s.u, s.v) == (f.x, f.y) && s.page == f.page
                    })
                })
                .collect::<Vec<_>>()
        };
        let picked = symbols_in(PlayerSymbols::Selected);
        assert_eq!(picked.len(), 2, "one over each selected");
        let square = atlas.symbol(1).unwrap();
        for s in &picked {
            assert!(selected.contains(&s.slot));
            assert_eq!((s.u, s.v), (square.x, square.y), "player 1's square");
            assert_eq!(s.row, palette::row_for_owner(1), "in player 1's colour");
            let own = world.pos[s.slot as usize];
            assert!(
                s.depth > fx_to_f32(own.x) + fx_to_f32(own.y) + 100.0,
                "over the world"
            );
        }
        let owned = world
            .slots()
            .filter(|s| world.owner[s.index()] != kinds::GAIA)
            .count();
        assert_eq!(symbols_in(PlayerSymbols::Always).len(), owned);
        assert!(symbols_in(PlayerSymbols::Off).is_empty());
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
        // The ring, the selection's symbol, and the ghost.
        assert_eq!(scene.sprites.len(), sim.world().len() + 4);
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
    fn buildings_are_drawn_in_their_owners_architecture() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/sprites");
        let (sheets, _) = crate::sheets::load_all(&dir);
        let atlas = Atlas::with_sheets(&sheets);
        let drawn = |civs: Vec<sim::Civ>| {
            let sim = Simulation::new(
                5,
                SimConfig {
                    civs,
                    ..SimConfig::default()
                },
            );
            let scene = Scene::build(&sim, &atlas, None, 0.0);
            (0..2u8)
                .map(|p| {
                    scene
                        .sprites
                        .iter()
                        .find(|s| {
                            s.slot != u32::MAX
                                && sim.world().kind[s.slot as usize] == kinds::TOWN_CENTER
                                && sim.world().owner[s.slot as usize] == p
                        })
                        .map(|s| (s.page, s.u, s.v))
                        .unwrap()
                })
                .collect::<Vec<_>>()
        };
        let frame_of = |look| {
            let f = atlas.frame(look, 0).unwrap().0;
            (f.page, f.x, f.y)
        };
        // The Egyptians' Town Center and the Shang's, each its own.
        let looks = drawn(vec![sim::Civ::Egyptians, sim::Civ::Shang]);
        assert_eq!(
            looks[0],
            frame_of(sprites::look_id(kinds::TOWN_CENTER, 0, 1))
        );
        assert_eq!(
            looks[1],
            frame_of(sprites::look_id(kinds::TOWN_CENTER, 0, 3))
        );
        // The Greeks', and a match that names no civilization, the first set.
        let greek = drawn(vec![sim::Civ::Greeks, sim::Civ::Phoenicians]);
        assert_eq!(greek[0], frame_of(kinds::TOWN_CENTER));
        assert_eq!(drawn(vec![]), greek);
    }

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

    /// A wall's tiles join into a run: each draws its post and an arm toward
    /// each wall beside it, a gate stands across its walls' line, and a run
    /// being dragged joins up as it will stand (`walls.rs`).
    #[test]
    fn walls_join_into_runs_and_gates_turn_along_them() {
        use sim::{Command, CommandKind};
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/sprites");
        let (sheets, errors) = crate::sheets::load_all(&dir);
        assert!(errors.is_empty(), "{errors:?}");
        let atlas = Atlas::with_sheets(&sheets);
        let mut sim = Simulation::new(
            5,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 48,
                    players: 1,
                },
                wander: false,
                ..SimConfig::default()
            },
        );
        let layout = [
            (kinds::PALISADE_WALL, (10, 10)),
            (kinds::PALISADE_WALL, (11, 10)),
            (kinds::PALISADE_WALL, (12, 10)),
            (kinds::GATE, (13, 10)),
            (kinds::PALISADE_WALL, (14, 10)),
            (kinds::STONE_WALL, (20, 20)),
            (kinds::STONE_WALL, (21, 21)),
        ];
        for (kind, t) in layout {
            sim.issue(Command {
                player: 0,
                kind: CommandKind::Spawn {
                    kind,
                    pos: sim::nav::centre(t),
                },
            });
        }
        for _ in 0..3 {
            sim.step();
        }
        let ghost = Ghost {
            kind: kinds::PALISADE_WALL,
            x: 33,
            y: 30,
            ok: true,
            row: 1,
            player: 0,
            age: 0,
            run: Some((30, 30)),
        };
        let scene = Scene::build_with(&sim, &atlas, None, 0.0, &[], Some(ghost));
        let w = sim.world();
        let slot_at = |t: (i32, i32)| {
            w.slots()
                .map(|s| s.index())
                .find(|&i| sim::nav::anchor_tile(w.pos[i], 1) == t && w.kind[i] != kinds::VILLAGER)
                .unwrap() as u32
        };
        let is = |s: &SpriteInstance, f: &crate::sprites::Frame| (s.u, s.v) == (f.x, f.y);
        // The arms each tile shows, by direction.
        let arms = |t: (i32, i32), kind| -> Vec<u8> {
            let slot = slot_at(t);
            (0..8u8)
                .filter(|&k| {
                    let f = atlas.wall_arm(kind, k).unwrap();
                    scene.sprites.iter().any(|s| s.slot == slot && is(s, f))
                })
                .collect()
        };
        let pal = kinds::PALISADE_WALL;
        assert_eq!(arms((10, 10), pal), [0]);
        assert_eq!(arms((11, 10), pal), [0, 4]);
        assert_eq!(arms((12, 10), pal), [0, 4], "into the gate along its line");
        assert_eq!(arms((14, 10), pal), [4]);
        assert_eq!(arms((20, 20), kinds::STONE_WALL), [1]);
        assert_eq!(arms((21, 21), kinds::STONE_WALL), [5]);
        // Nothing hostile near, so the gate stands open across x.
        let gate = slot_at((13, 10));
        let open = atlas.gate_frame(kinds::GATE, 0, true).unwrap();
        assert!(scene.sprites.iter().any(|s| s.slot == gate && is(s, open)));
        // Arms toward the viewer sort after their post, the rest before.
        let slot = slot_at((11, 10));
        let post = scene
            .sprites
            .iter()
            .position(|s| s.slot == slot && is(s, atlas.frame(pal, 0).unwrap().0))
            .unwrap();
        let arm = |k| {
            let f = atlas.wall_arm(pal, k).unwrap();
            scene
                .sprites
                .iter()
                .position(|s| s.slot == slot && is(s, f))
                .unwrap()
        };
        assert!(arm(4) < post && post < arm(0));
        // The dragged run: four posts and the three joints between them,
        // an arm from each side.
        let ghosts: Vec<_> = scene
            .sprites
            .iter()
            .filter(|s| s.slot == u32::MAX)
            .collect();
        let ghost_arms = (0..8u8)
            .map(|k| {
                let f = atlas.wall_arm(pal, k).unwrap();
                ghosts.iter().filter(|s| is(s, f)).count()
            })
            .collect::<Vec<_>>();
        assert_eq!(ghost_arms, [3, 0, 0, 0, 3, 0, 0, 0]);
    }

    /// A villager shows its job (`docs/05` §2.2): the chop at a tree, then
    /// the wood carried home on the walk back.
    #[test]
    fn a_villager_chops_and_carries_the_wood_home() {
        use sim::{Command, CommandKind};
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/sprites");
        let (sheets, errors) = crate::sheets::load_all(&dir);
        assert!(errors.is_empty(), "{errors:?}");
        let atlas = Atlas::with_sheets(&sheets);
        let mut sim = Simulation::new(
            1,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Inland,
                    size: 64,
                    players: 1,
                },
                wander: false,
                ..SimConfig::default()
            },
        );
        let w = sim.world();
        let v = w
            .slots()
            .find(|s| w.kind[s.index()] == kinds::VILLAGER)
            .unwrap();
        let (vi, vid) = (v.index(), w.id_at(v));
        let tree = w
            .slots()
            .filter(|s| w.kind[s.index()] == kinds::TREE)
            .min_by_key(|s| (w.pos[vi].distance_sq_raw(w.pos[s.index()]), s.index()))
            .map(|s| w.id_at(s))
            .unwrap();
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Gather {
                ids: vec![vid],
                node: tree,
            },
        });
        // The animation the villager's sprite is drawn from.
        let anim_of = |sim: &Simulation| {
            let scene = Scene::build(sim, &atlas, None, 0.0);
            let s = scene.sprites.iter().find(|s| s.slot == vi as u32).unwrap();
            atlas
                .frames()
                .iter()
                .find(|f| (f.x, f.y) == (s.u, s.v))
                .map(|f| f.anim)
                .unwrap()
        };
        let mut seen = Vec::new();
        for _ in 0..3000 {
            sim.step();
            let a = anim_of(&sim);
            if seen.last() != Some(&a) {
                seen.push(a);
            }
            if a == Anim::CarryWood {
                break;
            }
        }
        let chop = seen.iter().position(|&a| a == Anim::Chop);
        let carry = seen.iter().position(|&a| a == Anim::CarryWood);
        assert!(
            matches!((chop, carry), (Some(c), Some(k)) if c < k),
            "chops, then carries: {seen:?}"
        );
        assert!(!seen.contains(&Anim::Work), "no swing stands in: {seen:?}");
    }

    /// A building that falls sinks into its rubble under the dust for its
    /// first moments, then only the rubble is left.
    #[test]
    fn a_falling_building_sinks_into_its_rubble() {
        use sim::{Command, CommandKind};
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/sprites");
        let (sheets, _) = crate::sheets::load_all(&dir);
        let atlas = Atlas::with_sheets(&sheets);
        let mut sim = Simulation::new(
            5,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 48,
                    players: 2,
                },
                wander: false,
                ..SimConfig::default()
            },
        );
        let spawn = |sim: &mut Simulation, player, kind, x, y| {
            sim.issue(Command {
                player,
                kind: CommandKind::Spawn {
                    kind,
                    pos: sim::nav::centre((x, y)),
                },
            });
        };
        spawn(&mut sim, 1, kinds::HOUSE, 30, 30);
        for k in 0..6 {
            spawn(&mut sim, 0, kinds::CLUBMAN, 26 + k, 27);
        }
        for _ in 0..3 {
            sim.step();
        }
        let w = sim.world();
        let house_slot = w
            .slots()
            .find(|s| w.kind[s.index()] == kinds::HOUSE)
            .unwrap();
        let (hi, house) = (house_slot.index(), w.id_at(house_slot));
        let clubs: Vec<_> = w
            .slots()
            .filter(|s| w.kind[s.index()] == kinds::CLUBMAN)
            .map(|s| w.id_at(s))
            .collect();
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Attack {
                ids: clubs,
                target: house,
            },
        });
        for _ in 0..6000 {
            sim.step();
            if sim.world().dying[hi] > 0 {
                break;
            }
        }
        assert!(sim.world().dying[hi] > 0, "the house fell");
        let standing = atlas.frame_at(kinds::HOUSE, 0, Anim::Idle, 0).unwrap().0;
        let drawn = |sim: &Simulation| {
            let scene = Scene::build(sim, &atlas, None, 0.0);
            scene
                .sprites
                .iter()
                .filter(|s| s.slot == hi as u32 && (s.u, s.v) == (standing.x, standing.y))
                .copied()
                .collect::<Vec<_>>()
        };
        let first = drawn(&sim);
        assert_eq!(first.len(), 1, "the house still stands over its rubble");
        for _ in 0..COLLAPSE_TICKS / 2 {
            sim.step();
        }
        let half = drawn(&sim);
        assert_eq!(half.len(), 1);
        assert!(
            half[0].vh < first[0].vh && half[0].y > first[0].y,
            "sinking"
        );
        for _ in 0..COLLAPSE_TICKS {
            sim.step();
        }
        assert!(drawn(&sim).is_empty(), "only the rubble is left");
    }

    /// A colour building (`docs/look/buildings.md`) is drawn as an indexed
    /// set is: its own construction stage by progress, uncropped and on
    /// one anchor with the finished building; then the building; then,
    /// fallen, its own rubble with the building sinking into it.
    #[test]
    fn a_building_burns_by_its_health() {
        // None while it is two thirds whole; one small fire, then two, then
        // every point with the first two large, then every one large.
        assert_eq!(fires(3, 1.0), Vec::<bool>::new());
        assert_eq!(fires(3, 0.7), Vec::<bool>::new());
        assert_eq!(fires(3, 0.6), vec![false]);
        assert_eq!(fires(3, 0.45), vec![false, false]);
        assert_eq!(fires(4, 0.3), vec![true, true, false, false]);
        assert_eq!(fires(4, 0.1), vec![true; 4]);
        // Never more than the building has points for.
        assert_eq!(fires(1, 0.45), vec![false]);
        assert_eq!(fires(0, 0.1), Vec::<bool>::new());
    }

    /// The new art's smoke and fires (`docs/look/buildings.md`, "Smoke,
    /// flags and fire"): a finished, standing building smokes at its
    /// smoke point, drawn just in front of it and never picked, its frame
    /// by the renderers' clock and a phase of its own; it burns at its fire
    /// points as it is hurt; a site, rubble and a remembered building do
    /// neither.
    #[test]
    fn smoke_and_fire_rise_from_a_buildings_attachment_points() {
        use crate::sheets::{fixture, Sheet};
        use crate::sprites::{Effect, FX_SETS};
        use sim::{Command, CommandKind};
        let dir = fixture::scratch("scene-fx");
        let mut sheets =
            vec![Sheet::load(&fixture::write_attached_building(&dir, "house")).unwrap()];
        for name in FX_SETS {
            sheets.push(Sheet::load(&fixture::write_fx(&dir, name, 8)).unwrap());
        }
        let atlas = Atlas::with_sheets(&sheets);
        // The points, from the cell to 1x px off the ground point.
        let points = atlas.attachments(kinds::HOUSE);
        let (ax, ay) = (fixture::ANCHOR.0 as f32, fixture::ANCHOR.1 as f32);
        let off = |(x, y): (i32, i32)| ((x as f32 - ax) / 2.0, (y as f32 - ay) / 2.0);
        assert_eq!(points.len(), 4);
        assert_eq!(points[0].kind, crate::sheets::AttachKind::Smoke);
        assert_eq!((points[0].dx, points[0].dy), off(fixture::SMOKE_AT));
        for (p, &at) in points[1..].iter().zip(&fixture::FIRE_AT) {
            assert_eq!(p.kind, crate::sheets::AttachKind::Fire);
            assert_eq!((p.dx, p.dy), off(at));
        }
        assert!(atlas.attachments(kinds::BARRACKS).is_empty());

        let mut sim = Simulation::new(
            5,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 48,
                    players: 2,
                },
                starting_stockpile: [5000; 4],
                wander: false,
                ..SimConfig::default()
            },
        );
        let spawn = |sim: &mut Simulation, player, kind, pos| {
            sim.issue(Command {
                player,
                kind: CommandKind::Spawn { kind, pos },
            });
        };
        let fp = kinds::info(kinds::HOUSE).footprint as i32;
        let tiles = [(10, 10), (13, 10), (16, 10), (19, 10)];
        for &(x, y) in &tiles {
            spawn(
                &mut sim,
                0,
                kinds::HOUSE,
                sim::nav::building_centre(x, y, fp),
            );
        }
        for _ in 0..3 {
            sim.step();
        }
        let houses: Vec<usize> = {
            let w = sim.world();
            w.slots()
                .filter(|s| w.kind[s.index()] == kinds::HOUSE)
                .map(|s| s.index())
                .collect()
        };
        assert_eq!(houses.len(), 4);
        let at = |sim: &Simulation, time: f32, viewer: Option<u8>| {
            Scene::build_full(
                sim,
                &atlas,
                None,
                0.0,
                &SceneOptions {
                    time,
                    viewer,
                    ..SceneOptions::default()
                },
            )
        };
        let frames_of = |e: Effect| -> Vec<(u16, u16)> {
            (0..8)
                .map(|k| {
                    let f = atlas.effect(e, k * 100).unwrap();
                    (f.x, f.y)
                })
                .collect()
        };
        let (smoke, small, large) = (
            frames_of(Effect::Smoke),
            frames_of(Effect::FireSmall),
            frames_of(Effect::FireLarge),
        );
        let effects = |scene: &Scene, of: &[(u16, u16)]| -> Vec<SpriteInstance> {
            scene
                .sprites
                .iter()
                .filter(|s| of.contains(&(s.u, s.v)))
                .copied()
                .collect()
        };
        let scene = at(&sim, 0.0, None);
        let puffs = effects(&scene, &smoke);
        assert_eq!(puffs.len(), 4, "each finished house smokes");
        assert!(effects(&scene, &small).is_empty() && effects(&scene, &large).is_empty());
        for s in &puffs {
            assert_eq!(s.slot, u32::MAX, "never picked");
            assert_eq!(s.row, 0);
        }
        // Each puff at its house's smoke point, just in front of it.
        for &h in &houses {
            let house = scene.sprites.iter().find(|s| s.slot == h as u32).unwrap();
            let idle = atlas.frame(kinds::HOUSE, 0).unwrap().0;
            let (gx, gy) = (
                house.x + idle.draw_anchor().0,
                house.y + idle.draw_anchor().1,
            );
            let puff = puffs
                .iter()
                .find(|s| (s.depth - house.depth - FX_DEPTH).abs() < 0.01)
                .unwrap();
            let f = atlas.effect(Effect::Smoke, 0).unwrap();
            let want = (
                (gx + points[0].dx - f.draw_anchor().0).round(),
                (gy + points[0].dy - f.draw_anchor().1).round(),
            );
            // (Each rounded to the pixel on its own.)
            assert!(
                puffs
                    .iter()
                    .any(|s| (s.x - want.0).abs() <= 1.0 && (s.y - want.1).abs() <= 1.0),
                "a puff at {want:?}: {puffs:?}"
            );
            assert!(puff.depth > house.depth, "in front of its house");
            let pos = scene.sprites.iter().position(|s| s == puff).unwrap();
            let hpos = scene.sprites.iter().position(|s| s == house).unwrap();
            assert!(pos > hpos, "drawn after its house");
        }
        // The same scene from the same clock; the chimneys out of step;
        // the clock moves them on.
        assert_eq!(at(&sim, 0.0, None).sprites, scene.sprites, "deterministic");
        let phases: std::collections::HashSet<(u16, u16)> =
            puffs.iter().map(|s| (s.u, s.v)).collect();
        assert!(phases.len() > 1, "not every chimney puffs in step");
        let later = effects(&at(&sim, 0.35, None), &smoke);
        assert_ne!(
            later.iter().map(|s| (s.u, s.v)).collect::<Vec<_>>(),
            puffs.iter().map(|s| (s.u, s.v)).collect::<Vec<_>>(),
            "the smoke rises with the clock"
        );
        // Remembered out of sight: no smoke. Their clubman sees the houses
        // and walks away.
        spawn(&mut sim, 1, kinds::CLUBMAN, sim::nav::centre((15, 13)));
        for _ in 0..5 {
            sim.step();
        }
        let seen = at(&sim, 0.0, Some(1));
        let live = seen
            .sprites
            .iter()
            .filter(|s| houses.contains(&(s.slot as usize)))
            .count();
        assert!(live > 0);
        assert_eq!(effects(&seen, &smoke).len(), live, "in sight, they smoke");
        let theirs = {
            let w = sim.world();
            w.slots()
                .find(|s| w.kind[s.index()] == kinds::CLUBMAN)
                .map(|s| w.id_at(s))
                .unwrap()
        };
        sim.issue(Command {
            player: 1,
            kind: CommandKind::Move {
                ids: vec![theirs],
                target: sim::nav::centre((15, 42)),
            },
        });
        for _ in 0..400 {
            sim.step();
        }
        let fogged = at(&sim, 0.0, Some(1));
        assert!(
            fogged.sprites.iter().any(|s| s.light == fog::EXPLORED),
            "the houses remembered"
        );
        assert!(effects(&fogged, &smoke).is_empty(), "no smoke from memory");
        // Hurt: their clubmen on the first house; it burns as it goes down,
        // then lies as rubble, which neither smokes nor burns.
        let target = houses[0];
        let id = {
            let w = sim.world();
            w.id_at(w.slots().find(|s| s.index() == target).unwrap())
        };
        for k in 0..6 {
            spawn(&mut sim, 1, kinds::CLUBMAN, sim::nav::centre((9 + k, 13)));
        }
        for _ in 0..3 {
            sim.step();
        }
        let clubs: Vec<_> = {
            let w = sim.world();
            w.slots()
                .filter(|s| w.kind[s.index()] == kinds::CLUBMAN)
                .map(|s| w.id_at(s))
                .collect()
        };
        sim.issue(Command {
            player: 1,
            kind: CommandKind::Attack {
                ids: clubs,
                target: id,
            },
        });
        let max = sim.max_health_of(0, kinds::HOUSE) as f32;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..6000 {
            sim.step();
            let w = sim.world();
            if w.dying[target] > 0 {
                break;
            }
            let health = fx_to_f32(w.health[target]) / max;
            let scene = at(&sim, 0.0, None);
            let burning = fires(3, health);
            let want_small = burning.iter().filter(|&&l| !l).count();
            let want_large = burning.len() - want_small;
            assert_eq!(
                (effects(&scene, &small).len(), effects(&scene, &large).len()),
                (want_small, want_large),
                "at {health}"
            );
            assert_eq!(
                effects(&scene, &smoke).len(),
                4,
                "still smoking while it stands"
            );
            seen.insert((want_small, want_large));
        }
        assert!(sim.world().dying[target] > 0, "the house fell");
        assert!(seen.contains(&(1, 0)) && seen.contains(&(1, 2)), "{seen:?}");
        let fallen = at(&sim, 0.0, None);
        assert_eq!(effects(&fallen, &smoke).len(), 3, "rubble does not smoke");
        assert!(effects(&fallen, &small).is_empty() && effects(&fallen, &large).is_empty());
        // A site neither smokes nor burns, until it is finished.
        for k in 0..3 {
            spawn(&mut sim, 0, kinds::VILLAGER, sim::nav::centre((30 + k, 20)));
        }
        for _ in 0..3 {
            sim.step();
        }
        let villagers: Vec<_> = {
            let w = sim.world();
            w.slots()
                .filter(|s| w.kind[s.index()] == kinds::VILLAGER)
                .map(|s| w.id_at(s))
                .collect()
        };
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Build {
                kind: kinds::HOUSE,
                x: 31,
                y: 23,
                ids: villagers,
            },
        });
        let mut rose = false;
        for _ in 0..3000 {
            sim.step();
            let w = sim.world();
            let site = w
                .slots()
                .any(|s| w.kind[s.index()] == kinds::HOUSE && w.construction[s.index()].is_some());
            let n = effects(&at(&sim, 0.0, None), &smoke).len();
            if site {
                rose = true;
                assert_eq!(n, 3, "a site does not smoke");
            } else if rose {
                assert_eq!(n, 4, "finished, it does");
                break;
            }
        }
        assert!(rose);
    }

    #[test]
    fn a_colour_building_rises_through_its_stages_and_falls_to_its_rubble() {
        use crate::sheets::fixture;
        use crate::sprites::COLOUR_PAGE;
        use sim::{Command, CommandKind};
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/sprites");
        let (mut sheets, _) = crate::sheets::load_all(&dir);
        sheets.push(fixture::building("scene", "house"));
        let atlas = Atlas::with_sheets(&sheets);
        let mut sim = Simulation::new(
            5,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 48,
                    players: 2,
                },
                starting_stockpile: [5000; 4],
                wander: false,
                ..SimConfig::default()
            },
        );
        let spawn = |sim: &mut Simulation, player, kind, x, y| {
            sim.issue(Command {
                player,
                kind: CommandKind::Spawn {
                    kind,
                    pos: sim::nav::centre((x, y)),
                },
            });
        };
        for k in 0..5 {
            spawn(&mut sim, 0, kinds::VILLAGER, 10 + k, 10);
        }
        for _ in 0..3 {
            sim.step();
        }
        let w = sim.world();
        let builders: Vec<_> = w
            .slots()
            .filter(|s| w.kind[s.index()] == kinds::VILLAGER && w.owner[s.index()] == 0)
            .map(|s| w.id_at(s))
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
        for _ in 0..3 {
            sim.step();
        }
        let hi = {
            let w = sim.world();
            w.slots()
                .find(|s| w.kind[s.index()] == kinds::HOUSE)
                .map(|s| s.index())
                .unwrap()
        };
        let drawn = |sim: &Simulation| {
            Scene::build(sim, &atlas, None, 0.0)
                .sprites
                .into_iter()
                .filter(|s| s.slot == hi as u32)
                .collect::<Vec<_>>()
        };
        // Where a sprite puts its frame's anchor: the same ground point for
        // every stage, the building and its rubble.
        let ground = |s: &SpriteInstance, f: &crate::sprites::Frame| {
            let (ax, ay) = f.draw_anchor();
            (s.x + ax, s.y + ay)
        };
        let idle = *atlas.frame(kinds::HOUSE, 0).unwrap().0;
        assert_ne!(idle.page & COLOUR_PAGE, 0);
        let total = kinds::info(kinds::HOUSE).build_work().max(1);
        let mut seen = [false; 4];
        let mut at = None;
        for _ in 0..3000 {
            sim.step();
            let s = drawn(&sim)[0];
            let frame = match sim.world().construction[hi] {
                Some(done) => {
                    let stage = (done / (total / 3).max(1)).min(2) as u8;
                    seen[stage as usize] = true;
                    *atlas.stage_frame(kinds::HOUSE, stage).unwrap()
                }
                None => {
                    seen[3] = true;
                    idle
                }
            };
            assert_ne!(s.page & COLOUR_PAGE, 0);
            assert_eq!((s.u, s.v, s.uw, s.vh), (frame.x, frame.y, frame.w, frame.h));
            assert_eq!(s.h, frame.draw_h(), "a stage of its own is not cropped");
            let g = ground(&s, &frame);
            let a: (f32, f32) = *at.get_or_insert(g);
            assert!(
                (a.0 - g.0).abs() <= 1.0 && (a.1 - g.1).abs() <= 1.0,
                "every stage on one anchor: {a:?} {g:?}"
            );
            if seen[3] {
                break;
            }
        }
        assert_eq!(seen, [true; 4], "every stage was drawn");
        // Brought down by their clubmen: the rubble, and the building
        // sinking into it at first.
        for k in 0..6 {
            spawn(&mut sim, 1, kinds::CLUBMAN, 14 + k, 17);
        }
        for _ in 0..3 {
            sim.step();
        }
        let w = sim.world();
        let clubs: Vec<_> = w
            .slots()
            .filter(|s| w.kind[s.index()] == kinds::CLUBMAN)
            .map(|s| w.id_at(s))
            .collect();
        let house = w.id_at(w.slots().find(|s| s.index() == hi).unwrap());
        sim.issue(Command {
            player: 1,
            kind: CommandKind::Attack {
                ids: clubs,
                target: house,
            },
        });
        for _ in 0..6000 {
            sim.step();
            if sim.world().dying[hi] > 0 {
                break;
            }
        }
        assert!(sim.world().dying[hi] > 0, "the house fell");
        let rubble = *atlas.own_rubble(kinds::HOUSE).unwrap();
        assert_ne!(rubble.page & COLOUR_PAGE, 0);
        let sprites = drawn(&sim);
        let r = sprites
            .iter()
            .find(|s| (s.u, s.v) == (rubble.x, rubble.y))
            .expect("its own rubble");
        let (g, a) = (ground(r, &rubble), at.unwrap());
        assert!(
            (a.0 - g.0).abs() <= 1.0 && (a.1 - g.1).abs() <= 1.0,
            "the rubble on its footprint: {a:?} {g:?}"
        );
        assert!(
            sprites.iter().any(|s| (s.u, s.v) == (idle.x, idle.y)),
            "the building sinking into it"
        );
    }
}

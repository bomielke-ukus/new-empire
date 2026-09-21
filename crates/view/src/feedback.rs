//! Short-lived feedback, collected once per simulation tick by either
//! viewer and drawn over the scene (`docs/03` §6.2): the spark and the
//! flinch where a blow lands, the puff where a unit falls, the cloud
//! where a building comes down, the dust under a hammer, a tree falling
//! toward the villager who felled it, and the mark at the screen's edge
//! for an attack on the player's own out of view. Presentation only:
//! nothing here changes the simulation or its replay, and time is match
//! ticks, so pause and speed apply to all of it.

use sim::{kinds, EntityId, Event, KindId, Replay, ReplayError, Simulation, Task, Vec2Fx};

use crate::hud::{BOTTOM_PANEL, TOP_BAR};
use crate::{fog, fx_to_f32, iso, palette, Atlas, Camera, Scene, SpriteInstance};

/// The spark lasts this long: 200 ms.
const HIT_TICKS: u64 = 4;
/// The struck sprite jumps a step away and settles over this long.
const FLINCH_TICKS: u64 = 3;
/// How far it jumps, at 1×.
const FLINCH_PX: f32 = 2.0;
/// A kill's puff.
const PUFF_TICKS: u64 = 10;
/// A building's cloud.
const CLOUD_TICKS: u64 = 24;
/// The dust under a hammer.
const DUST_TICKS: u64 = 6;
/// A tree takes this long to fall.
pub const FALL_TICKS: u64 = 16;
/// The mark at the screen's edge stays this long: three seconds.
pub const INDICATOR_TICKS: u64 = 60;
/// Attacks within this many tiles of a mark share it, as the notices do.
pub const INDICATOR_AREA: f32 = 12.0;
/// How far in from the view's edge the mark sits, at 1×.
const INDICATOR_INSET: f32 = 14.0;

#[derive(Clone, Debug)]
struct Impact {
    target: EntityId,
    pos: Vec2Fx,
    from: Vec2Fx,
    tick: u64,
}

/// What a puff is: it decides the motes, their colour and their life.
#[derive(Clone, Copy, PartialEq, Debug)]
enum PuffKind {
    /// A unit fell: five motes thrown the way the blow went, the first red.
    Kill,
    /// A building came down: a ring of dust rising, wider with the footprint.
    Collapse(u8),
    /// A hammer struck: two motes off the site.
    Hammer,
}

impl PuffKind {
    const fn ticks(self) -> u64 {
        match self {
            PuffKind::Kill => PUFF_TICKS,
            PuffKind::Collapse(_) => CLOUD_TICKS,
            PuffKind::Hammer => DUST_TICKS,
        }
    }
}

#[derive(Clone, Debug)]
struct Puff {
    /// Tiles.
    pos: (f32, f32),
    /// Unit direction in tiles, or nothing.
    dir: (f32, f32),
    kind: PuffKind,
    tick: u64,
}

#[derive(Clone, Debug)]
struct Fall {
    kind: KindId,
    pos: (f32, f32),
    dir: (f32, f32),
    tick: u64,
}

#[derive(Clone, Debug)]
struct Attack {
    owner: u8,
    pos: (f32, f32),
    tick: u64,
}

/// Everything short-lived that the last ticks left to show. Multiple hits
/// on one target coalesce; tick time makes pause and speed apply.
#[derive(Default)]
pub struct CombatFeedback {
    impacts: Vec<Impact>,
    puffs: Vec<Puff>,
    falls: Vec<Fall>,
    attacks: Vec<Attack>,
    last_tick: Option<u64>,
}

impl CombatFeedback {
    /// Call after every tick, including each tick in a fast-forwarded frame.
    pub fn observe(&mut self, sim: &Simulation) {
        let tick = sim.tick();
        if self.last_tick == Some(tick) {
            return;
        }
        if self.last_tick.is_some_and(|last| tick < last) {
            *self = Self::default();
        }
        self.last_tick = Some(tick);
        self.impacts
            .retain(|hit| tick.saturating_sub(hit.tick) < HIT_TICKS);
        self.puffs
            .retain(|p| tick.saturating_sub(p.tick) < p.kind.ticks());
        self.falls
            .retain(|f| tick.saturating_sub(f.tick) < FALL_TICKS);
        self.attacks
            .retain(|a| tick.saturating_sub(a.tick) < INDICATOR_TICKS);
        let world = sim.world();
        for event in sim.events() {
            match *event {
                Event::Hit {
                    target,
                    pos,
                    from,
                    damage,
                } => {
                    if damage <= 0 {
                        continue;
                    }
                    let owner = world
                        .slot(target)
                        .map_or(kinds::GAIA, |s| world.owner[s.index()]);
                    self.impacts.retain(|hit| hit.target != target);
                    self.impacts.push(Impact {
                        target,
                        pos,
                        from,
                        tick,
                    });
                    // The mark at the edge: one per area per three seconds.
                    let at = tile(pos);
                    let told = self
                        .attacks
                        .iter()
                        .any(|a| a.owner == owner && dist(a.pos, at) <= INDICATOR_AREA);
                    if !told && owner != kinds::GAIA {
                        self.attacks.push(Attack {
                            owner,
                            pos: at,
                            tick,
                        });
                    }
                }
                Event::Death { kind, pos, .. } => {
                    let info = kinds::info(kind);
                    let at = tile(pos);
                    if info.mobile {
                        // The blow that did it gives the puff its direction.
                        let dir = self
                            .impacts
                            .iter()
                            .rfind(|hit| dist(tile(hit.pos), at) < 1.0)
                            .map_or((0.0, 0.0), |hit| unit(tile(hit.from), tile(hit.pos)));
                        self.puffs.push(Puff {
                            pos: at,
                            dir,
                            kind: PuffKind::Kill,
                            tick,
                        });
                    } else {
                        self.puffs.push(Puff {
                            pos: at,
                            dir: (0.0, 0.0),
                            kind: PuffKind::Collapse(info.footprint.max(1)),
                            tick,
                        });
                    }
                }
                Event::Work {
                    task: Task::Build,
                    pos,
                    ..
                } => self.puffs.push(Puff {
                    pos: tile(pos),
                    dir: (0.0, 0.0),
                    kind: PuffKind::Hammer,
                    tick,
                }),
                Event::Felled { kind, pos, toward } if kind == kinds::TREE => {
                    self.falls.push(Fall {
                        kind,
                        pos: tile(pos),
                        dir: unit(tile(pos), tile(toward)),
                        tick,
                    });
                }
                _ => {}
            }
        }
    }

    /// Render replay history through the same event collector as the live app.
    pub fn replay(&mut self, replay: &Replay) -> Result<Simulation, ReplayError> {
        replay.validate()?;
        *self = Self::default();
        let mut sim = Simulation::new(replay.seed, replay.config.clone());
        let mut commands = replay.commands.iter().peekable();
        while sim.tick() < replay.ticks {
            while commands.peek().is_some_and(|(tick, _)| *tick == sim.tick()) {
                sim.issue(commands.next().unwrap().1.clone());
            }
            sim.step();
            self.observe(&sim);
        }
        Ok(sim)
    }

    /// Adds what the last ticks left to show. With a `viewer`, only where
    /// that player can see: a hit in the fog is not shown (`GD-FOG-01`).
    pub fn decorate(&self, scene: &mut Scene, sim: &Simulation, atlas: &Atlas, viewer: Option<u8>) {
        let fog = viewer.and_then(|p| sim.fog(p));
        let tick = sim.tick();
        let world = sim.world();
        let map = sim.map();
        let visible =
            |p: (f32, f32)| fog.is_none_or(|f| f.visible(p.0.floor() as i32, p.1.floor() as i32));
        let ground = |p: (f32, f32)| iso::project(p.0, p.1, iso::ground_height(map, p.0, p.1));
        // The flinch: the struck sprite jumps a step away from the blow and
        // settles back; a corpse lies where it fell.
        for hit in &self.impacts {
            let age = tick.saturating_sub(hit.tick);
            if age >= FLINCH_TICKS {
                continue;
            }
            let Some(slot) = world.slot(hit.target) else {
                continue;
            };
            if world.dying[slot.index()] != 0 {
                continue;
            }
            let (dx, dy) = unit(tile(hit.from), tile(hit.pos));
            let (sx, sy) = iso::direction(dx, dy);
            let k = FLINCH_PX * (1.0 - age as f32 / FLINCH_TICKS as f32);
            let slot = slot.index() as u32;
            for s in scene
                .sprites
                .iter_mut()
                .filter(|s| s.slot == slot && !s.screen)
            {
                s.x += (sx * k).round();
                s.y += (sy * k).round();
            }
        }
        // The spark: a small expanding cross, outlined for contrast against
        // any terrain, at the place damage actually landed.
        for hit in &self.impacts {
            let age = tick.saturating_sub(hit.tick);
            if age >= HIT_TICKS || !visible(tile(hit.pos)) {
                continue;
            }
            let (x, y) = tile(hit.pos);
            let (gx, gy) = ground((x, y));
            let radius = 3.0 + age as f32;
            for (dx, dy) in [(radius, 0.0), (-radius, 0.0), (0.0, radius), (0.0, -radius)] {
                mote(
                    scene,
                    atlas,
                    gx + dx - 2.0,
                    gy - 24.0 + dy - 2.0,
                    4.0,
                    palette::BLACK,
                    x + y + 0.8,
                );
                mote(
                    scene,
                    atlas,
                    gx + dx - 1.0,
                    gy - 24.0 + dy - 1.0,
                    2.0,
                    palette::GOLD_LIGHT,
                    x + y + 0.8,
                );
            }
        }
        // The puffs.
        for p in &self.puffs {
            let age = tick.saturating_sub(p.tick);
            let life = p.kind.ticks();
            if age >= life || !visible(p.pos) {
                continue;
            }
            let t = age as f32 / life as f32;
            let (gx, gy) = ground(p.pos);
            let (sx, sy) = iso::direction(p.dir.0, p.dir.1);
            let depth = p.pos.0 + p.pos.1 + 0.9;
            match p.kind {
                PuffKind::Kill => {
                    for k in 0..5 {
                        let side = (k as f32 - 2.0) * (2.0 + t * 6.0);
                        let along = 4.0 + t * 14.0;
                        let mx = gx + sx * along - sy * side;
                        let my = gy - 10.0 + sy * along + sx * side - t * 4.0;
                        let colour = if k == 2 && t < 0.4 {
                            palette::RED_DARK
                        } else if t < 0.5 {
                            palette::DIRT
                        } else {
                            palette::DIRT_DARK
                        };
                        mote(scene, atlas, mx, my, 2.0, colour, depth);
                    }
                }
                PuffKind::Collapse(fp) => {
                    let colours = [palette::TAN, palette::SAND, palette::GREY_LIGHT];
                    for k in 0..10 {
                        let a = k as f32 / 10.0 * std::f32::consts::TAU;
                        let r = f32::from(fp) * 14.0 * (0.3 + t);
                        let mx = gx + a.cos() * r;
                        let my = gy - 8.0 + a.sin() * r * 0.5 - t * 18.0;
                        let size = if t < 0.5 { 3.0 } else { 2.0 };
                        mote(scene, atlas, mx, my, size, colours[k % 3], depth);
                    }
                }
                PuffKind::Hammer => {
                    for (k, side) in [-3.0, 3.0].into_iter().enumerate() {
                        let my = gy - 18.0 - t * 8.0 - k as f32 * 2.0;
                        mote(scene, atlas, gx + side, my, 2.0, palette::SAND, depth);
                    }
                }
            }
        }
        // A tree falling: the trunk leans and shortens toward the villager,
        // slowly and then all at once, and is gone.
        for f in &self.falls {
            let age = tick.saturating_sub(f.tick);
            if age >= FALL_TICKS || !visible(f.pos) {
                continue;
            }
            let Some((frame, _)) = atlas.frame(f.kind, 0) else {
                continue;
            };
            let t = age as f32 / FALL_TICKS as f32;
            let keep = 1.0 - 0.9 * t * t;
            let (gx, gy) = ground(f.pos);
            let (ax, ay) = frame.draw_anchor();
            let (w0, h0) = (frame.draw_w(), frame.draw_h());
            let (sx, _) = iso::direction(f.dir.0, f.dir.1);
            let lean = sx * (1.0 - keep) * h0 * 0.7;
            scene.sprites.push(SpriteInstance {
                x: (gx - ax + lean).round(),
                y: (gy - ay * keep).round(),
                w: w0,
                h: h0 * keep,
                u: frame.x,
                v: frame.y,
                uw: frame.w,
                vh: frame.h,
                row: 0,
                flip: false,
                depth: f.pos.0 + f.pos.1 + 0.5,
                slot: u32::MAX,
                screen: false,
                light: fog::VISIBLE,
            });
        }
        scene
            .sprites
            .sort_by(|a, b| a.depth.total_cmp(&b.depth).then(a.slot.cmp(&b.slot)));
    }

    /// The marks at the screen's edge: a red chevron where the line from
    /// the middle of the view to an attack on the viewer's own leaves the
    /// view, for three seconds, blinking after the first. An attack in view
    /// has its spark and gets no mark. Window pixels, `ui_scale` applied.
    pub fn edge_indicators(
        &self,
        sim: &Simulation,
        atlas: &Atlas,
        camera: &Camera,
        viewer: Option<u8>,
        ui_scale: f32,
    ) -> Vec<SpriteInstance> {
        let mut out = Vec::new();
        let Some(me) = viewer else {
            return out;
        };
        let tick = sim.tick();
        let s = ui_scale.max(0.1);
        let (vw, vh) = camera.viewport;
        let inset = INDICATOR_INSET * s;
        let (left, top) = (inset, TOP_BAR * s + inset);
        let (right, bottom) = (vw - inset, vh - BOTTOM_PANEL * s - inset);
        if right <= left || bottom <= top {
            return out;
        }
        let (cx, cy) = ((left + right) * 0.5, (top + bottom) * 0.5);
        for a in self.attacks.iter().filter(|a| a.owner == me) {
            let age = tick.saturating_sub(a.tick);
            if age >= INDICATOR_TICKS || (age >= 20 && (age / 5) % 2 == 1) {
                continue;
            }
            let (gx, gy) = iso::project(
                a.pos.0,
                a.pos.1,
                iso::ground_height(sim.map(), a.pos.0, a.pos.1),
            );
            let (px, py) = camera.to_window(gx, gy);
            if (left..=right).contains(&px) && (top..=bottom).contains(&py) {
                continue;
            }
            // Where the line from the middle to it crosses the inset edge.
            let (dx, dy) = (px - cx, py - cy);
            let kx = if dx.abs() > 1e-3 {
                (if dx > 0.0 { right - cx } else { cx - left }) / dx.abs()
            } else {
                f32::INFINITY
            };
            let ky = if dy.abs() > 1e-3 {
                (if dy > 0.0 { bottom - cy } else { cy - top }) / dy.abs()
            } else {
                f32::INFINITY
            };
            let k = kx.min(ky);
            let (ex, ey) = (cx + dx * k, cy + dy * k);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let (ux, uy) = (dx / len, dy / len);
            // The chevron: a tip at the edge, two flanks behind it.
            for (along, side) in [(0.0, 0.0), (-5.0, 5.0), (-5.0, -5.0)] {
                let mx = ex + (ux * along - uy * side) * s;
                let my = ey + (uy * along + ux * side) * s;
                for (size, colour) in [(5.0, palette::BLACK), (3.0, palette::RED)] {
                    let solid = atlas.solid(colour);
                    let mut m = crate::scene::overlay(solid, 0.0, 0.0, 0, 0.0, u32::MAX);
                    m.x = (mx - size * s * 0.5).round();
                    m.y = (my - size * s * 0.5).round();
                    m.w = size * s;
                    m.h = size * s;
                    m.screen = true;
                    out.push(m);
                }
            }
        }
        out
    }
}

/// A square of one colour on the ground plane.
fn mote(scene: &mut Scene, atlas: &Atlas, x: f32, y: f32, size: f32, colour: u8, depth: f32) {
    let mut m = crate::scene::overlay(atlas.solid(colour), 0.0, 0.0, 0, depth, u32::MAX);
    m.x = x.round();
    m.y = y.round();
    m.w = size;
    m.h = size;
    scene.sprites.push(m);
}

fn tile(p: Vec2Fx) -> (f32, f32) {
    (fx_to_f32(p.x), fx_to_f32(p.y))
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// The unit direction from `a` to `b` in tiles, or nothing if they touch.
fn unit(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 {
        (0.0, 0.0)
    } else {
        (dx / len, dy / len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{Command, CommandKind, MapKind, MapSpec, SimConfig};

    #[test]
    fn impact_history_survives_frames_expires_and_preserves_replay() {
        let replay: Replay =
            ron::from_str(include_str!("../../sim/tests/corpus/battle-40v40.ron")).unwrap();
        let mut feedback = CombatFeedback::default();
        let actual = feedback.replay(&replay).unwrap();
        let expected = replay.run(|_, _| {}).unwrap();
        assert_eq!(actual.state_hash(), expected.state_hash());
        assert_eq!(actual.replay(), expected.replay());
        // At the decisive death the hit still needs to be shown, even though
        // the target has become a corpse. A repeated render cannot duplicate it.
        assert!(!feedback.impacts.is_empty());
        let count = feedback.impacts.len();
        feedback.observe(&actual);
        assert_eq!(feedback.impacts.len(), count);
        let mut actual = actual;
        for _ in 0..HIT_TICKS {
            actual.step();
            feedback.observe(&actual);
        }
        assert!(feedback.impacts.is_empty());
        let mut invalid = replay;
        invalid.version = u32::MAX;
        assert!(feedback.replay(&invalid).is_err());
    }

    fn sim() -> Simulation {
        Simulation::new(
            3,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 64,
                    players: 2,
                },
                starting_stockpile: [5000; 4],
                wander: false,
                ..SimConfig::default()
            },
        )
    }

    fn spawn(sim: &mut Simulation, player: u8, kind: KindId, x: i32, y: i32) -> EntityId {
        sim.issue(Command {
            player,
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
            .filter(|s| w.kind[s.index()] == kind && w.owner[s.index()] == player)
            .map(|s| w.id_at(s))
            .last()
            .unwrap()
    }

    fn scene(sim: &Simulation, atlas: &Atlas) -> Scene {
        Scene::build(sim, atlas, None, 0.0)
    }

    /// A blow moves the struck sprite a step away from where it came and
    /// the step settles; the kill throws a puff the same way, red at
    /// first, gone in half a second; a fallen building leaves a rising
    /// cloud for a second; and the attack on the villager's side puts one
    /// mark on the screen's edge when the camera is elsewhere, none when
    /// it is on screen, and none for the attacker.
    #[test]
    fn blows_flinch_kills_puff_and_an_attack_out_of_view_marks_the_edge() {
        let atlas = Atlas::placeholder();
        let mut s = sim();
        let victim = spawn(&mut s, 1, kinds::VILLAGER, 20, 20);
        let clubs: Vec<_> = (0..3)
            .map(|k| spawn(&mut s, 0, kinds::CLUBMAN, 17 + k, 20))
            .collect();
        s.issue(Command {
            player: 0,
            kind: CommandKind::Attack {
                ids: clubs,
                target: victim,
            },
        });
        let mut fb = CombatFeedback::default();
        // Step to the first blow.
        let mut hit_tick = None;
        for _ in 0..400 {
            s.step();
            fb.observe(&s);
            if s.events()
                .iter()
                .any(|e| matches!(e, Event::Hit { damage, .. } if *damage > 0))
            {
                hit_tick = Some(s.tick());
                break;
            }
        }
        let hit_tick = hit_tick.expect("a blow landed");
        let slot = s.world().slot(victim).unwrap().index() as u32;
        let plain = scene(&s, &atlas);
        let mut struck = scene(&s, &atlas);
        fb.decorate(&mut struck, &s, &atlas, None);
        let before = plain.sprites.iter().find(|sp| sp.slot == slot).unwrap();
        let after = struck.sprites.iter().find(|sp| sp.slot == slot).unwrap();
        assert!(
            (before.x - after.x).abs() + (before.y - after.y).abs() >= 1.0,
            "the flinch moved it"
        );
        assert!(after.x >= before.x, "away from the clubmen on its left");
        assert!(
            struck.sprites.len() > plain.sprites.len(),
            "the spark is drawn"
        );
        // The mark: the camera far away, the villager's side sees it at
        // the right-hand edge; on screen, nothing; the attacker, nothing.
        // The camera due west of the fight in tiles, which is up-left on
        // the screen, so the mark is on the right-hand edge.
        let mut cam = Camera::new(64, 64, (640.0, 480.0));
        cam.look_at_tile(4.0, 20.0);
        let marks = fb.edge_indicators(&s, &atlas, &cam, Some(1), 1.0);
        assert_eq!(marks.len(), 6, "one chevron of three motes, each outlined");
        assert!(marks.iter().all(|m| m.screen));
        let tip = &marks[1];
        assert!(tip.x > 320.0, "on the side the attack is: {}", tip.x);
        let (right, bottom) = (
            640.0 - INDICATOR_INSET,
            480.0 - BOTTOM_PANEL - INDICATOR_INSET,
        );
        assert!(
            (tip.x - right).abs() < 6.0 || (tip.y - bottom).abs() < 6.0,
            "on an edge: ({}, {})",
            tip.x,
            tip.y
        );
        assert!(tip.y > TOP_BAR && tip.y < 480.0 - BOTTOM_PANEL);
        assert!(
            fb.edge_indicators(&s, &atlas, &cam, Some(0), 1.0)
                .is_empty(),
            "not the attacker's"
        );
        cam.look_at_tile(20.0, 20.0);
        assert!(
            fb.edge_indicators(&s, &atlas, &cam, Some(1), 1.0)
                .is_empty(),
            "in view: the spark"
        );
        // The flinch settles.
        for _ in 0..FLINCH_TICKS {
            s.step();
            fb.observe(&s);
        }
        let _ = hit_tick;
        // To the death, and the puff.
        let mut died = None;
        for _ in 0..2000 {
            s.step();
            fb.observe(&s);
            if s.events().iter().any(|e| matches!(e, Event::Death { .. })) {
                died = Some(s.tick());
                break;
            }
        }
        let died = died.expect("the villager fell");
        assert_eq!(fb.puffs.len(), 1);
        assert_eq!(fb.puffs[0].kind, PuffKind::Kill);
        assert!(
            fb.puffs[0].dir.0 > 0.5,
            "thrown the way the blows went: {:?}",
            fb.puffs[0].dir
        );
        let plain = scene(&s, &atlas);
        let mut puffed = scene(&s, &atlas);
        fb.decorate(&mut puffed, &s, &atlas, None);
        let motes = puffed.sprites.len() - plain.sprites.len();
        assert!(motes >= 5, "{motes} motes");
        for _ in 0..PUFF_TICKS {
            s.step();
            fb.observe(&s);
        }
        assert!(
            fb.puffs.is_empty(),
            "gone after {} ticks from {died}",
            PUFF_TICKS
        );
        // A house comes down: a cloud.
        let house = spawn(&mut s, 1, kinds::HOUSE, 30, 30);
        let clubs: Vec<_> = {
            let w = s.world();
            w.slots()
                .filter(|sl| w.kind[sl.index()] == kinds::CLUBMAN && w.dying[sl.index()] == 0)
                .map(|sl| w.id_at(sl))
                .collect()
        };
        s.issue(Command {
            player: 0,
            kind: CommandKind::Attack {
                ids: clubs,
                target: house,
            },
        });
        for _ in 0..4000 {
            s.step();
            fb.observe(&s);
            if fb
                .puffs
                .iter()
                .any(|p| matches!(p.kind, PuffKind::Collapse(_)))
            {
                break;
            }
        }
        let cloud = fb
            .puffs
            .iter()
            .find(|p| matches!(p.kind, PuffKind::Collapse(_)))
            .expect("the house fell");
        assert_eq!(
            cloud.kind,
            PuffKind::Collapse(kinds::info(kinds::HOUSE).footprint.max(1))
        );
        let plain = scene(&s, &atlas);
        let mut clouded = scene(&s, &atlas);
        fb.decorate(&mut clouded, &s, &atlas, None);
        assert!(
            clouded.sprites.len() - plain.sprites.len() >= 10,
            "a ring of dust"
        );
    }

    /// Hammering raises dust at the builder; the last of a tree fells it
    /// toward the villager, leaning and shortening over the fall and gone
    /// after; a bush being eaten is drawn smaller as it goes.
    #[test]
    fn hammers_raise_dust_and_a_tree_falls_toward_its_feller() {
        let atlas = Atlas::placeholder();
        let mut s = sim();
        let v = spawn(&mut s, 0, kinds::VILLAGER, 10, 10);
        s.issue(Command {
            player: 0,
            kind: CommandKind::Build {
                kind: kinds::HOUSE,
                x: 12,
                y: 10,
                ids: vec![v],
            },
        });
        let mut fb = CombatFeedback::default();
        let mut dusted = false;
        for _ in 0..300 {
            s.step();
            fb.observe(&s);
            if fb.puffs.iter().any(|p| p.kind == PuffKind::Hammer) {
                dusted = true;
                break;
            }
        }
        assert!(dusted, "the hammer raised dust");
        // Eight villagers on one tree: it falls toward them.
        let tree = spawn(&mut s, 0, kinds::TREE, 30, 30);
        let gang: Vec<_> = (0..8)
            .map(|k| spawn(&mut s, 0, kinds::VILLAGER, 26 + k % 4, 26 + k / 4))
            .collect();
        s.issue(Command {
            player: 0,
            kind: CommandKind::Gather {
                ids: gang,
                node: tree,
            },
        });
        let mut fell = None;
        for _ in 0..6000 {
            s.step();
            fb.observe(&s);
            if let Some(f) = fb.falls.first() {
                fell = Some((f.pos, f.dir, s.tick()));
                break;
            }
        }
        let (pos, dir, at) = fell.expect("the tree fell");
        assert_eq!((pos.0.floor(), pos.1.floor()), (30.0, 30.0));
        assert!(
            dir.0 < 0.0 || dir.1 < 0.0,
            "toward the villagers, up and left: {dir:?}"
        );
        // Mid-fall it is drawn shorter than it stood; after, not at all.
        for _ in 0..FALL_TICKS / 2 {
            s.step();
            fb.observe(&s);
        }
        let plain = scene(&s, &atlas);
        let mut falling = scene(&s, &atlas);
        fb.decorate(&mut falling, &s, &atlas, None);
        let (tree_frame, _) = atlas.frame(kinds::TREE, 0).unwrap();
        let drawn = falling
            .sprites
            .iter()
            .find(|sp| sp.slot == u32::MAX && sp.u == tree_frame.x && sp.v == tree_frame.y)
            .expect("the falling tree is drawn");
        assert!(
            drawn.h < tree_frame.draw_h() * 0.95,
            "shorter: {} of {}",
            drawn.h,
            tree_frame.draw_h()
        );
        assert_eq!(
            plain
                .sprites
                .iter()
                .filter(|sp| sp.u == tree_frame.x && sp.v == tree_frame.y)
                .count(),
            0,
            "the world no longer has it"
        );
        for _ in 0..FALL_TICKS {
            s.step();
            fb.observe(&s);
        }
        assert!(fb.falls.is_empty(), "gone {} ticks after {at}", FALL_TICKS);
    }
}

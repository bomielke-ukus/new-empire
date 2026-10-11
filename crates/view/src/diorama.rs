//! The live picture behind the campaign screens: a Greek town and an
//! Egyptian one, each raided from its far side while its villagers work
//! and its towers shoot, galleys fighting at sea. It is a scenario like any
//! other (`assets/menu/diorama.ron`, drawn by `tools/campaigns/menu.py`),
//! played headless, given its few orders here, and begun again when the
//! raids are spent. The menu panel stands in the middle of the window, so
//! the fights are set to either side of it.

use crate::camera::{Camera, ZOOM_LEVELS};
use crate::feedback::CombatFeedback;
use crate::scene::{Scene, SceneOptions};
use crate::sprites::Atlas;
use crate::symbols::PlayerSymbols;
use sim::scenario::Scenario;
use sim::{kinds, CommandKind, Simulation, Vec2Fx, TICK_MS};

/// The scenario, built into the game so it needs no file beside it.
const SOURCE: &str = include_str!("../../../assets/menu/diorama.ron");

/// A raid: where it gathers, whose it is, and the town it falls on.
struct Raid {
    from: (i32, i32),
    owner: u8,
    town: (i32, i32),
}

/// The Egyptians' on the Greek town, and the Greeks' on the Egyptian.
const RAIDS: [Raid; 2] = [
    Raid {
        from: (4, 44),
        owner: 1,
        town: (13, 31),
    },
    Raid {
        from: (44, 4),
        owner: 0,
        town: (31, 13),
    },
];
/// How near its gathering point a soldier must stand to ride with a raid.
const RAID_REACH: i32 = 8;
/// When the raids set out, in ticks: long enough to see the towns at
/// peace first.
const SET_OUT: u64 = 60;
/// Every so often idle villagers go back to work.
const WORK_EVERY: u64 = 40;
/// The longest a round runs before it begins again, in ticks.
const LONGEST: u64 = 20 * 100;
/// How long after both raids are spent the picture lingers.
const LINGER: u64 = 20 * 6;
/// The tile the camera looks at: between the towns.
const FOCUS: (f32, f32) = (22.0, 22.0);
/// The width, in sprite pixels at 1×, the camera keeps in view: both
/// towns and the ground their raids come over.
const SPAN: f32 = 1300.0;

/// The diorama as it stands.
pub struct Diorama {
    scenario: Scenario,
    sim: Simulation,
    prev: Vec<Vec2Fx>,
    feedback: CombatFeedback,
    /// Milliseconds of play owed to the next tick.
    owed_ms: f32,
    /// The raids' soldiers, once they have set out.
    raiders: Vec<sim::EntityId>,
    /// Ticks since the last raider fell, once all have.
    spent: u64,
    /// How many times it has begun again.
    pub rounds: u32,
}

impl Diorama {
    /// The diorama at its first tick, or why the scenario would not read.
    pub fn new() -> Result<Diorama, String> {
        let scenario: Scenario = ron::from_str(SOURCE).map_err(|e| e.to_string())?;
        let problems = scenario.problems();
        if !problems.is_empty() {
            return Err(problems.join("; "));
        }
        let sim = Simulation::new(scenario.match_seed(), scenario.config());
        Ok(Diorama {
            scenario,
            sim,
            prev: Vec::new(),
            feedback: CombatFeedback::default(),
            owed_ms: 0.0,
            raiders: Vec::new(),
            spent: 0,
            rounds: 0,
        })
    }

    /// The world as it stands.
    pub fn sim(&self) -> &Simulation {
        &self.sim
    }

    /// Plays on by `dt` seconds of real time, at the game's own speed. A
    /// long pause (the window hidden) is not made up.
    pub fn advance(&mut self, dt: f32) {
        self.owed_ms += (dt * 1000.0).min(250.0);
        while self.owed_ms >= TICK_MS as f32 {
            self.owed_ms -= TICK_MS as f32;
            self.tick();
        }
    }

    /// One tick: its orders, the step, and whether to begin again.
    pub fn tick(&mut self) {
        self.orders();
        self.prev = self.sim.world().pos.clone();
        self.sim.step();
        self.feedback.observe(&self.sim);
        let tick = self.sim.tick();
        if tick > SET_OUT && self.raiders.iter().all(|&id| !self.alive(id)) {
            self.spent += 1;
        }
        if tick >= LONGEST || self.spent >= LINGER {
            self.begin_again();
        }
    }

    fn begin_again(&mut self) {
        self.sim = Simulation::new(self.scenario.match_seed(), self.scenario.config());
        self.prev.clear();
        self.feedback = CombatFeedback::default();
        self.raiders.clear();
        self.spent = 0;
        self.rounds += 1;
    }

    fn alive(&self, id: sim::EntityId) -> bool {
        let w = self.sim.world();
        w.slot(id).is_some_and(|s| w.dying[s.index()] == 0)
    }

    /// The raids set out once; idle villagers go back to work.
    fn orders(&mut self) {
        let tick = self.sim.tick();
        if tick == SET_OUT {
            for Raid { from, owner, town } in RAIDS {
                let at = Vec2Fx::from_int(from.0, from.1);
                let reach = sim::Fx::from_int(RAID_REACH);
                let w = self.sim.world();
                let ids: Vec<_> = w
                    .slots()
                    .filter(|s| {
                        let i = s.index();
                        let k = kinds::info(w.kind[i]);
                        w.owner[i] == owner
                            && k.mobile
                            && !k.naval
                            && w.kind[i] != kinds::VILLAGER
                            && w.pos[i].distance(at) <= reach
                    })
                    .map(|s| w.id_at(s))
                    .collect();
                self.raiders.extend(&ids);
                self.issue(
                    owner,
                    CommandKind::AttackMove {
                        ids,
                        target: Vec2Fx::from_int(town.0, town.1),
                    },
                );
            }
        }
        if tick.is_multiple_of(WORK_EVERY) {
            // Villagers who ran from a raid come out once it has passed.
            for (tc, owner) in self.safe_shelters() {
                self.issue(owner, CommandKind::Ungarrison { building: tc });
            }
            for owner in 0..2u8 {
                for v in self.sim.idle_villagers(owner) {
                    if let Some(node) = self.work_for(v, owner) {
                        self.issue(owner, CommandKind::Gather { ids: vec![v], node });
                    }
                }
            }
        }
    }

    /// Town Centers with someone inside and no enemy soldier near.
    fn safe_shelters(&self) -> Vec<(sim::EntityId, u8)> {
        let w = self.sim.world();
        let near = sim::Fx::from_int(12);
        w.slots()
            .filter(|s| {
                let i = s.index();
                w.kind[i] == kinds::TOWN_CENTER
                    && w.dying[i] == 0
                    && !self.sim.garrison_of(w.id_at(*s)).is_empty()
                    && !w.slots().any(|e| {
                        let j = e.index();
                        w.owner[j] != w.owner[i]
                            && w.owner[j] != kinds::GAIA
                            && w.dying[j] == 0
                            && kinds::info(w.kind[j]).mobile
                            && w.kind[j] != kinds::VILLAGER
                            && w.pos[j].distance(w.pos[i]) <= near
                    })
            })
            .map(|s| (w.id_at(s), w.owner[s.index()]))
            .collect()
    }

    /// The nearest thing for a villager to work: its side's farm, a bush,
    /// a vein or a tree.
    fn work_for(&self, villager: sim::EntityId, owner: u8) -> Option<sim::EntityId> {
        let w = self.sim.world();
        let at = w.pos[w.slot(villager)?.index()];
        w.slots()
            .filter(|s| {
                let i = s.index();
                let k = w.kind[i];
                w.dying[i] == 0
                    && w.construction[i].is_none()
                    && w.resource[i] > 0
                    && (k == kinds::FARM && w.owner[i] == owner
                        || matches!(k, kinds::BERRY_BUSH | kinds::GOLD_MINE | kinds::TREE))
            })
            .min_by_key(|s| at.distance_sq_raw(w.pos[s.index()]))
            .map(|s| w.id_at(s))
    }

    fn issue(&mut self, player: u8, kind: CommandKind) {
        self.sim.issue(sim::Command { player, kind });
    }

    /// A camera on the diorama for a window `viewport` device pixels wide
    /// at `dpi`: between the towns, as near as keeps both in view.
    pub fn camera(&self, viewport: (f32, f32), dpi: f32) -> Camera {
        let map = self.sim.map();
        let mut c = Camera::new(map.width(), map.height(), viewport);
        c.dpi = dpi;
        let wide = viewport.0 / dpi.max(0.5);
        let index = (0..ZOOM_LEVELS.len())
            .rev()
            .find(|&i| SPAN * ZOOM_LEVELS[i] <= wide)
            .unwrap_or(0);
        c.set_zoom_index(index);
        c.look_at_tile(FOCUS.0, FOCUS.1);
        c
    }

    /// The world's sprites, between the last tick and the next, with the
    /// hits, dust and falls of the fighting; everything in sight, nobody's
    /// symbols, nothing selected.
    pub fn scene(&self, atlas: &Atlas) -> Scene {
        let alpha = (self.owed_ms / TICK_MS as f32).clamp(0.0, 1.0);
        let prev = (self.prev.len() == self.sim.world().pos.len()).then_some(&self.prev[..]);
        let mut scene = Scene::build_full(
            &self.sim,
            atlas,
            prev,
            alpha,
            &SceneOptions {
                symbols: PlayerSymbols::Off,
                ..SceneOptions::default()
            },
        );
        self.feedback.decorate(&mut scene, &self.sim, atlas, None);
        scene
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The diorama reads, its raids set out and fight at the towns, and
    /// it begins again, with every invariant holding all the while.
    #[test]
    fn the_diorama_plays_its_raids_and_begins_again() {
        let mut d = Diorama::new().expect("the shipped diorama reads");
        let hp = |d: &Diorama, owner: u8| -> i64 {
            let w = d.sim().world();
            w.slots()
                .filter(|s| w.owner[s.index()] == owner && w.dying[s.index()] == 0)
                .map(|s| w.health[s.index()].raw() as i64)
                .sum()
        };
        let before = (hp(&d, 0), hp(&d, 1));
        let working = |d: &Diorama| {
            let w = d.sim().world();
            w.slots()
                .filter(|s| {
                    let i = s.index();
                    w.kind[i] == kinds::VILLAGER && matches!(w.order[i], sim::Order::Gather { .. })
                })
                .count()
        };
        for _ in 0..SET_OUT + WORK_EVERY + 2 {
            d.tick();
        }
        assert!(working(&d) >= 12, "the villagers work: {}", working(&d));
        for _ in 0..20 * 40 {
            d.tick();
            d.sim().check().expect("invariants");
        }
        assert!(!d.raiders.is_empty(), "the raids set out");
        assert!(
            hp(&d, 0) < before.0 && hp(&d, 1) < before.1,
            "both towns are fought over"
        );
        for _ in 0..LONGEST {
            d.tick();
        }
        assert!(d.rounds >= 1, "it begins again");
        assert!(d.sim().tick() < LONGEST);
    }

    /// Both towns are in view at any window size the game opens.
    #[test]
    fn the_camera_keeps_both_towns_in_view() {
        let d = Diorama::new().unwrap();
        for (viewport, dpi) in [
            ((1280.0, 720.0), 1.0),
            ((2560.0, 1600.0), 2.0),
            ((3840.0, 2160.0), 2.0),
        ] {
            let c = d.camera(viewport, dpi);
            for Raid { town, .. } in RAIDS {
                let (sx, sy) = crate::iso::project(town.0 as f32, town.1 as f32, 0.0);
                let (wx, wy) = c.to_window(sx, sy);
                assert!(
                    (0.0..viewport.0).contains(&wx) && (0.0..viewport.1).contains(&wy),
                    "{viewport:?}: {town:?} at {wx}, {wy}"
                );
            }
        }
    }
}

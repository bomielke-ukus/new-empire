//! A player for the shipped scenarios' tests, played by rote: villagers
//! kept at work, buildings put down near where they are wanted, soldiers
//! sent at what the scenario sends.

use super::*;
use sim::kinds::{self, Resource};
use sim::scenario::{ObjectiveStatus, Scenario};
use sim::{
    Command, CommandKind, EntityId, KindId, Order, PlayerId, Simulation, Vec2Fx, TICKS_PER_SECOND,
};
use std::collections::BTreeMap;

pub const SECOND: u32 = TICKS_PER_SECOND;
pub const MINUTE: u32 = 60 * SECOND;
pub const ME: PlayerId = 0;

pub fn load(campaign: &str, name: &str) -> Simulation {
    let path = format!(
        "{}/../../assets/campaigns/{campaign}/{name}.ron",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let sc: Scenario = ron::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert!(sc.problems().is_empty(), "{name}: {:?}", sc.problems());
    Simulation::new(sc.match_seed(), sc.config())
}

/// The player's side, played by rote.
pub struct Bot {
    pub sim: Simulation,
    /// What each villager gathers when she has nothing to do.
    pub jobs: BTreeMap<EntityId, Resource>,
}

impl Bot {
    pub fn new(campaign: &str, name: &str) -> Bot {
        Bot {
            sim: load(campaign, name),
            jobs: BTreeMap::new(),
        }
    }

    pub fn issue(&mut self, kind: CommandKind) {
        self.sim.issue(Command { player: ME, kind });
    }

    pub fn status(&self, id: &str) -> ObjectiveStatus {
        self.sim
            .objective(id)
            .expect("an objective of the scenario")
    }

    /// The player's finished entities of a kind.
    pub fn mine(&self, kind: KindId) -> Vec<EntityId> {
        owned(&self.sim, ME, kind)
            .into_iter()
            .filter(|&id| {
                let i = index_of(&self.sim, id);
                self.sim.world().construction[i].is_none() && self.sim.world().dying[i] == 0
            })
            .collect()
    }

    pub fn tile_of(&self, id: EntityId) -> (i32, i32) {
        let p = pos_of(&self.sim, id);
        (p.x.floor(), p.y.floor())
    }

    /// The nearest free place for a building of `kind` to `near`.
    pub fn site(&self, kind: KindId, near: (i32, i32)) -> (i32, i32) {
        for r in 0..20i32 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let (x, y) = (near.0 + dx, near.1 + dy);
                    if self.sim.can_place(ME, kind, x, y).is_ok() {
                        return (x, y);
                    }
                }
            }
        }
        panic!("nowhere to put a {} near {near:?}", kinds::info(kind).name)
    }

    /// Sets `n` villagers not already building to build a `kind` near
    /// `near`; they go back to their work after.
    pub fn build(&mut self, kind: KindId, near: (i32, i32), n: usize) {
        let (x, y) = self.site(kind, near);
        let ids: Vec<EntityId> = self
            .mine(kinds::VILLAGER)
            .into_iter()
            .filter(|&v| {
                !matches!(
                    self.sim.world().order[index_of(&self.sim, v)],
                    Order::Build { .. }
                )
            })
            .take(n)
            .collect();
        self.issue(CommandKind::Build { kind, x, y, ids });
        // The order lands after the command delay; wait for it so the
        // next builder chosen is another.
        for _ in 0..=sim::COMMAND_DELAY {
            self.sim.step();
        }
    }

    pub fn train(&mut self, at: KindId, kind: KindId, n: usize) {
        let building = self.mine(at)[0];
        for _ in 0..n {
            self.issue(CommandKind::Train { building, kind });
        }
    }

    /// Every villager has a job; new ones get `default`.
    pub fn assign(&mut self, default: Resource) {
        for v in self.mine(kinds::VILLAGER) {
            self.jobs.entry(v).or_insert(default);
        }
    }

    pub fn set_job(&mut self, n: usize, job: Resource) {
        for v in self.mine(kinds::VILLAGER).into_iter().take(n) {
            self.jobs.insert(v, job);
        }
    }

    /// Idle villagers go back to their work: the nearest tree, or the
    /// nearest bush, or the nearest gazelle to hunt.
    pub fn work(&mut self) {
        let idle = self.sim.idle_villagers(ME);
        for v in idle {
            let job = *self.jobs.get(&v).unwrap_or(&Resource::Food);
            let from = pos_of(&self.sim, v);
            let near = |kind: KindId| -> Option<EntityId> {
                let w = self.sim.world();
                w.slots()
                    .filter(|s| w.kind[s.index()] == kind && w.dying[s.index()] == 0)
                    .min_by_key(|s| from.distance_sq_raw(w.pos[s.index()]))
                    .map(|s| w.id_at(s))
            };
            let kind = match job {
                Resource::Wood => near(kinds::TREE).map(|n| (n, false)),
                _ => near(kinds::BERRY_BUSH)
                    .map(|n| (n, false))
                    .or_else(|| near(kinds::GAZELLE).map(|n| (n, true))),
            };
            match kind {
                Some((node, false)) => self.issue(CommandKind::Gather { ids: vec![v], node }),
                Some((animal, true)) => self.issue(CommandKind::Attack {
                    ids: vec![v],
                    target: animal,
                }),
                None => {}
            }
        }
    }

    /// Plays on, keeping everyone at work, until `done` or `limit`.
    pub fn until(&mut self, limit: u32, what: &str, done: impl Fn(&Bot) -> bool) {
        let start = self.sim.tick();
        while !done(self) {
            assert!(
                self.sim.tick() - start < limit as u64,
                "{what}: not done in {} seconds; objectives {:?}, outcome {:?}, stockpile {:?}",
                limit / SECOND,
                self.sim
                    .objectives()
                    .iter()
                    .map(|o| (o.text.to_string(), o.status))
                    .collect::<Vec<_>>(),
                self.sim.outcome(),
                self.sim.player(ME).map(|p| p.stockpile),
            );
            if self.sim.tick().is_multiple_of(2 * SECOND as u64) {
                self.work();
            }
            self.sim.step();
        }
    }

    pub fn soldiers(&self, kinds_: &[KindId]) -> Vec<EntityId> {
        kinds_.iter().flat_map(|&k| self.mine(k)).collect()
    }

    pub fn attack_move(&mut self, ids: Vec<EntityId>, to: (i32, i32)) {
        self.issue(CommandKind::AttackMove {
            ids,
            target: Vec2Fx::from_int(to.0, to.1),
        });
    }

    /// Soldiers with nothing to do go for the nearest of `enemies`' within
    /// `reach` tiles of them, and stay where they are if there is none.
    pub fn press_within(&mut self, enemies: &[PlayerId], reach: i32) {
        let idle = self.sim.idle_soldiers(ME);
        let limit = (reach as i64 * 65536).pow(2) as u64;
        for s in idle {
            let from = pos_of(&self.sim, s);
            let target = {
                let w = self.sim.world();
                w.slots()
                    .filter(|t| enemies.contains(&w.owner[t.index()]) && w.dying[t.index()] == 0)
                    .map(|t| (from.distance_sq_raw(w.pos[t.index()]), t))
                    .filter(|(d, _)| *d <= limit)
                    .min_by_key(|(d, _)| *d)
                    .map(|(_, t)| w.id_at(t))
            };
            if let Some(target) = target {
                self.issue(CommandKind::Attack {
                    ids: vec![s],
                    target,
                });
            }
        }
    }

    /// Soldiers with nothing to do go for the nearest of `enemies`'.
    pub fn press(&mut self, enemies: &[PlayerId]) {
        let idle = self.sim.idle_soldiers(ME);
        for s in idle {
            let from = pos_of(&self.sim, s);
            let target = {
                let w = self.sim.world();
                w.slots()
                    .filter(|t| enemies.contains(&w.owner[t.index()]) && w.dying[t.index()] == 0)
                    .min_by_key(|t| from.distance_sq_raw(w.pos[t.index()]))
                    .map(|t| w.id_at(t))
            };
            if let Some(target) = target {
                self.issue(CommandKind::Attack {
                    ids: vec![s],
                    target,
                });
            }
        }
    }

    /// Plays on as [`Bot::until`], the soldiers pressing on `enemies`.
    pub fn fight(
        &mut self,
        limit: u32,
        what: &str,
        enemies: &[PlayerId],
        done: impl Fn(&Bot) -> bool,
    ) {
        let start = self.sim.tick();
        while !done(self) {
            let elapsed = self.sim.tick() - start;
            assert!(
                elapsed < limit as u64,
                "{what}: not done in {} seconds; objectives {:?}, outcome {:?}, mine {} soldiers, theirs {:?}",
                limit / SECOND,
                self.sim
                    .objectives()
                    .iter()
                    .map(|o| (o.text.to_string(), o.status))
                    .collect::<Vec<_>>(),
                self.sim.outcome(),
                self.sim.idle_soldiers(ME).len(),
                enemies.iter().map(|&e| self.standing(e)).collect::<Vec<_>>(),
            );
            if elapsed.is_multiple_of(5 * SECOND as u64) {
                self.press(enemies);
            }
            if self.sim.tick().is_multiple_of(2 * SECOND as u64) {
                self.work();
            }
            self.sim.step();
        }
    }

    pub fn standing(&self, owner: PlayerId) -> usize {
        let w = self.sim.world();
        w.slots()
            .filter(|s| w.owner[s.index()] == owner && w.dying[s.index()] == 0)
            .count()
    }

    pub fn minutes(&self) -> f32 {
        self.sim.tick() as f32 / MINUTE as f32
    }
}

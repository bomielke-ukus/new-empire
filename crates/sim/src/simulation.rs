//! The simulation itself: state, the tick, and the systems that run in it.
//!
//! M0 scope: entities that can be spawned, ordered around, and that wander
//! when idle. That is enough to exercise fixed-point movement, the RNG, the
//! command queue and the state hash — the four things determinism depends on.
//! Real game systems land on top of this skeleton in M2–M4.

use crate::command::{Command, CommandKind, CommandQueue, PlayerId};
use crate::entity::{EntityId, KindId, Slot, World};
use crate::fx::Fx;
use crate::hash::{HashState, StateHasher};
use crate::replay::Replay;
use crate::rng::Rng;
use crate::vec2::Vec2Fx;
use serde::{Deserialize, Serialize};

/// Simulation ticks per second of game time.
pub const TICKS_PER_SECOND: u32 = 20;
/// Milliseconds of game time per tick.
pub const TICK_MS: u32 = 1000 / TICKS_PER_SECOND;

/// Match parameters that are fixed for the whole match.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SimConfig {
    /// Map edge length in tiles (maps are square).
    pub map_size: i32,
    /// Hard cap on live entities; spawns beyond it are ignored.
    pub max_entities: u32,
    /// Movement speed in tiles per tick.
    pub unit_speed: Fx,
    /// Whether idle units pick random nearby destinations. Exercises the RNG.
    pub wander: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            map_size: 128,
            max_entities: 2000,
            // One tile per second.
            unit_speed: Fx::from_ratio(1, TICKS_PER_SECOND as i32),
            wander: true,
        }
    }
}

impl HashState for SimConfig {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i32(self.map_size);
        h.write_u32(self.max_entities);
        h.write(&self.unit_speed);
        h.write_bool(self.wander);
    }
}

/// A running match.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Simulation {
    seed: u64,
    tick: u64,
    config: SimConfig,
    rng: Rng,
    world: World,
    queue: CommandQueue,
    /// Every command ever issued, with its issue tick. This *is* the replay.
    log: Vec<(u64, Command)>,
}

impl Simulation {
    /// A fresh match at tick 0.
    pub fn new(seed: u64, config: SimConfig) -> Simulation {
        Simulation {
            seed,
            tick: 0,
            rng: Rng::new(seed),
            world: World::new(),
            queue: CommandQueue::new(),
            log: Vec::new(),
            config,
        }
    }

    /// The seed this match was created with.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Ticks completed so far.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Match parameters.
    pub fn config(&self) -> &SimConfig {
        &self.config
    }

    /// Read access to entities.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// The RNG's draw count; handy in desync diagnostics.
    pub fn rng_draws(&self) -> u64 {
        self.rng.draws()
    }

    /// Issues a command at the current tick. Returns the tick it will run on.
    pub fn issue(&mut self, command: Command) -> u64 {
        self.log.push((self.tick, command.clone()));
        self.queue.schedule(self.tick, command)
    }

    /// Advances the match by one tick. Fixed system order, no exceptions.
    pub fn step(&mut self) {
        self.apply_commands();
        self.wander();
        self.movement();
        self.tick += 1;
    }

    /// Canonical hash of everything that matters. Equal hashes on two
    /// machines at the same tick mean they are in the same world.
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.write_u64(self.tick);
        h.write(&self.config);
        h.write(&self.rng);
        h.write(&self.world);
        h.write_u64(self.queue.pending_len() as u64);
        h.finish()
    }

    /// Everything needed to reproduce this match up to the current tick.
    pub fn replay(&self) -> Replay {
        Replay {
            version: Replay::VERSION,
            seed: self.seed,
            config: self.config.clone(),
            ticks: self.tick,
            commands: self.log.clone(),
        }
    }

    // ----- systems -------------------------------------------------------

    fn apply_commands(&mut self) {
        for cmd in self.queue.drain_due(self.tick) {
            self.apply(cmd);
        }
    }

    fn apply(&mut self, cmd: Command) {
        match cmd.kind {
            CommandKind::Spawn { kind, pos } => {
                self.spawn(kind, cmd.player, pos);
            }
            CommandKind::Despawn { id } => {
                if let Some(slot) = self.owned_slot(id, cmd.player) {
                    let id = self.world.id_at(slot);
                    self.world.despawn(id);
                }
            }
            CommandKind::Move { ids, target } => {
                let target = self.clamp_to_map(target);
                for id in ids {
                    if let Some(slot) = self.owned_slot(id, cmd.player) {
                        self.world.move_target[slot.index()] = Some(target);
                    }
                }
            }
            CommandKind::Stop { ids } => {
                for id in ids {
                    if let Some(slot) = self.owned_slot(id, cmd.player) {
                        self.world.move_target[slot.index()] = None;
                    }
                }
            }
        }
    }

    fn spawn(&mut self, kind: KindId, owner: PlayerId, pos: Vec2Fx) -> Option<EntityId> {
        if self.world.len() as u32 >= self.config.max_entities {
            return None;
        }
        let pos = self.clamp_to_map(pos);
        Some(self.world.spawn(kind, owner, pos, Fx::from_int(100)))
    }

    /// Idle units occasionally pick a destination within three tiles.
    fn wander(&mut self) {
        if !self.config.wander {
            return;
        }
        let slots: Vec<Slot> = self.world.slots().collect();
        for slot in slots {
            let i = slot.index();
            if self.world.move_target[i].is_some() {
                continue;
            }
            if !self.rng.chance(1, 200) {
                continue;
            }
            let dx = self.rng.range_i32(-3, 4);
            let dy = self.rng.range_i32(-3, 4);
            let target = self.clamp_to_map(self.world.pos[i] + Vec2Fx::from_int(dx, dy));
            self.world.move_target[i] = Some(target);
        }
    }

    fn movement(&mut self) {
        let speed = self.config.unit_speed;
        for slot in self.world.slots().collect::<Vec<_>>() {
            let i = slot.index();
            let Some(target) = self.world.move_target[i] else {
                continue;
            };
            let next = self.world.pos[i].move_toward(target, speed);
            self.world.pos[i] = next;
            if next == target {
                self.world.move_target[i] = None;
            }
        }
    }

    // ----- helpers -------------------------------------------------------

    fn owned_slot(&self, id: EntityId, player: PlayerId) -> Option<Slot> {
        let slot = self.world.slot(id)?;
        (self.world.owner[slot.index()] == player).then_some(slot)
    }

    fn clamp_to_map(&self, p: Vec2Fx) -> Vec2Fx {
        let max = Fx::from_int(self.config.map_size) - Fx::EPSILON;
        Vec2Fx::new(p.x.clamp(Fx::ZERO, max), p.y.clamp(Fx::ZERO, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet() -> SimConfig {
        SimConfig {
            wander: false,
            ..SimConfig::default()
        }
    }

    fn spawn_cmd(player: PlayerId, x: i32, y: i32) -> Command {
        Command {
            player,
            kind: CommandKind::Spawn {
                kind: 1,
                pos: Vec2Fx::from_int(x, y),
            },
        }
    }

    #[test]
    fn commands_take_effect_after_the_delay() {
        let mut sim = Simulation::new(1, quiet());
        sim.issue(spawn_cmd(0, 5, 5));
        sim.step();
        assert_eq!(sim.world().len(), 0, "tick 0 -> 1: not yet");
        sim.step();
        assert_eq!(sim.world().len(), 0, "tick 1 -> 2: not yet");
        sim.step();
        assert_eq!(sim.world().len(), 1, "executes at the start of tick 2");
    }

    #[test]
    fn move_walks_at_speed_and_arrives_exactly() {
        let mut sim = Simulation::new(1, quiet());
        sim.issue(spawn_cmd(0, 0, 0));
        for _ in 0..3 {
            sim.step();
        }
        let id = sim.world().ids().next().unwrap();
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Move {
                ids: vec![id],
                target: Vec2Fx::from_int(3, 0),
            },
        });
        // Delay of 2 ticks, then 3 tiles at 1 tile/second = 60 ticks.
        for _ in 0..(2 + 60) {
            sim.step();
        }
        let s = sim.world().slot(id).unwrap();
        assert_eq!(sim.world().pos[s.index()], Vec2Fx::from_int(3, 0));
        assert_eq!(sim.world().move_target[s.index()], None);
    }

    #[test]
    fn players_cannot_command_each_others_units() {
        let mut sim = Simulation::new(1, quiet());
        sim.issue(spawn_cmd(0, 0, 0));
        for _ in 0..3 {
            sim.step();
        }
        let id = sim.world().ids().next().unwrap();
        sim.issue(Command {
            player: 1,
            kind: CommandKind::Move {
                ids: vec![id],
                target: Vec2Fx::from_int(3, 0),
            },
        });
        sim.issue(Command {
            player: 1,
            kind: CommandKind::Despawn { id },
        });
        for _ in 0..5 {
            sim.step();
        }
        let s = sim.world().slot(id).expect("still alive");
        assert_eq!(sim.world().pos[s.index()], Vec2Fx::ZERO);
    }

    #[test]
    fn spawns_are_clamped_and_capped() {
        let mut sim = Simulation::new(
            1,
            SimConfig {
                max_entities: 2,
                ..quiet()
            },
        );
        sim.issue(spawn_cmd(0, 1000, -1000));
        sim.issue(spawn_cmd(0, 1, 1));
        sim.issue(spawn_cmd(0, 2, 2));
        for _ in 0..3 {
            sim.step();
        }
        assert_eq!(sim.world().len(), 2);
        let first = sim.world().slots().next().unwrap();
        let p = sim.world().pos[first.index()];
        assert_eq!(p.x.floor(), 127);
        assert_eq!(p.y, Fx::ZERO);
    }

    #[test]
    fn identical_runs_hash_identically_and_diverge_on_seed() {
        let run = |seed: u64| {
            let mut sim = Simulation::new(seed, SimConfig::default());
            for i in 0..20 {
                sim.issue(spawn_cmd(i % 2, i as i32 * 3, 10));
            }
            let mut hashes = Vec::new();
            for _ in 0..500 {
                sim.step();
                hashes.push(sim.state_hash());
            }
            hashes
        };
        assert_eq!(run(99), run(99));
        assert_ne!(run(99), run(100));
    }

    #[test]
    fn wander_actually_moves_things() {
        let mut sim = Simulation::new(3, SimConfig::default());
        for i in 0..10 {
            sim.issue(spawn_cmd(0, 50 + i, 50));
        }
        for _ in 0..600 {
            sim.step();
        }
        let moved = sim
            .world()
            .slots()
            .filter(|s| sim.world().pos[s.index()].y != Fx::from_int(50))
            .count();
        assert!(moved > 0);
        assert!(sim.rng_draws() > 0);
    }
}

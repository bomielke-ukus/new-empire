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
use crate::kinds;
use crate::map::TileMap;
use crate::mapgen::{self, MapSpec};
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
    /// The map to generate.
    pub map: MapSpec,
    /// Hard cap on live entities; spawns beyond it are ignored.
    pub max_entities: u32,
    /// Whether idle mobile units pick random nearby destinations.
    pub wander: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            map: MapSpec::default(),
            max_entities: 4000,
            wander: true,
        }
    }
}

impl HashState for SimConfig {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.map.kind as u8);
        h.write_u16(self.map.size);
        h.write_u8(self.map.players);
        h.write_u32(self.max_entities);
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
    map: TileMap,
    /// Hash of the immutable map, folded into every state hash.
    map_hash: u64,
    starts: Vec<(i32, i32)>,
    world: World,
    queue: CommandQueue,
    /// Every command ever issued, with its issue tick. This *is* the replay.
    log: Vec<(u64, Command)>,
}

impl Simulation {
    /// A fresh match at tick 0, with its map generated and populated.
    pub fn new(seed: u64, config: SimConfig) -> Simulation {
        let generated = mapgen::generate(seed, &config.map);
        let mut map_hasher = StateHasher::new();
        generated.tiles.hash_state(&mut map_hasher);
        let mut sim = Simulation {
            seed,
            tick: 0,
            rng: Rng::new(seed),
            map: generated.tiles,
            map_hash: map_hasher.finish(),
            starts: generated.starts,
            world: World::new(),
            queue: CommandQueue::new(),
            log: Vec::new(),
            config,
        };
        for s in &generated.spawns {
            sim.spawn(s.kind, s.owner, s.pos);
        }
        sim
    }

    /// Terrain and elevation.
    pub fn map(&self) -> &TileMap {
        &self.map
    }

    /// Each player's Town Center tile.
    pub fn starts(&self) -> &[(i32, i32)] {
        &self.starts
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
        h.write_u64(self.map_hash);
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
                        if kinds::info(self.world.kind[slot.index()]).mobile {
                            self.world.move_target[slot.index()] = Some(target);
                        }
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
        let info = kinds::info(kind);
        let pos = self.clamp_to_map(pos);
        let resource = info.resource.map_or(0, |(_, amount)| amount);
        Some(self.world.spawn_with_resource(
            kind,
            owner,
            pos,
            Fx::from_int(info.max_health),
            resource,
        ))
    }

    /// Idle mobile units occasionally pick a destination within three tiles.
    fn wander(&mut self) {
        if !self.config.wander {
            return;
        }
        let slots: Vec<Slot> = self.world.slots().collect();
        for slot in slots {
            let i = slot.index();
            if self.world.move_target[i].is_some() || !kinds::info(self.world.kind[i]).mobile {
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
        for slot in self.world.slots().collect::<Vec<_>>() {
            let i = slot.index();
            let Some(target) = self.world.move_target[i] else {
                continue;
            };
            let speed = kinds::info(self.world.kind[i]).speed_per_second / TICKS_PER_SECOND as i32;
            let here = self.world.pos[i];
            if here != target {
                self.world.facing[i] = (target - here).angle().facing8();
            }
            let next = here.move_toward(target, speed);
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
        let max_x = Fx::from_int(self.map.width()) - Fx::EPSILON;
        let max_y = Fx::from_int(self.map.height()) - Fx::EPSILON;
        Vec2Fx::new(p.x.clamp(Fx::ZERO, max_x), p.y.clamp(Fx::ZERO, max_y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::mapgen::MapKind;

    fn flat() -> SimConfig {
        SimConfig {
            map: MapSpec {
                kind: MapKind::Flat,
                size: 128,
                players: 2,
            },
            ..SimConfig::default()
        }
    }

    fn quiet() -> SimConfig {
        SimConfig {
            wander: false,
            ..flat()
        }
    }

    fn spawn_cmd(player: PlayerId, x: i32, y: i32) -> Command {
        Command {
            player,
            kind: CommandKind::Spawn {
                kind: kinds::VILLAGER,
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
        // Delay of 2 ticks, then 3 tiles at a villager's 0.9 tiles/s (67 ticks).
        for _ in 0..(2 + 67) {
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
            let mut sim = Simulation::new(seed, flat());
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
    fn generated_map_is_populated_and_static_things_stay_put() {
        let mut sim = Simulation::new(11, SimConfig::default());
        let start_gazelles: Vec<_> = sim
            .world()
            .slots()
            .filter(|s| sim.world().kind[s.index()] == kinds::GAZELLE)
            .map(|s| sim.world().pos[s.index()])
            .collect();
        let n = sim.world().len();
        assert!(n > 200, "inland map should have scenery: {n}");
        assert_eq!(sim.starts().len(), 2);
        let trees: Vec<_> = sim
            .world()
            .slots()
            .filter(|s| sim.world().kind[s.index()] == kinds::TREE)
            .collect();
        assert!(!trees.is_empty());
        let before: Vec<_> = trees.iter().map(|s| sim.world().pos[s.index()]).collect();
        let wood = sim.world().resource[trees[0].index()];
        assert_eq!(wood, 75);
        // Nobody may order a tree around, not even its owner (gaia has no player).
        let tree_id = sim.world().id_at(trees[0]);
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Move {
                ids: vec![tree_id],
                target: Vec2Fx::ZERO,
            },
        });
        for _ in 0..200 {
            sim.step();
        }
        let after: Vec<_> = trees.iter().map(|s| sim.world().pos[s.index()]).collect();
        assert_eq!(before, after, "static entities moved");
        // Gazelles, on the other hand, wander.
        let gazelles: Vec<_> = sim
            .world()
            .slots()
            .filter(|s| sim.world().kind[s.index()] == kinds::GAZELLE)
            .map(|s| sim.world().pos[s.index()])
            .collect();
        assert!(gazelles.len() >= 6);
        assert!(gazelles.iter().any(|p| p.x.frac() != Fx::ZERO
            || p.y.frac() != Fx::ZERO
            || !start_gazelles.contains(p)));
    }

    #[test]
    fn map_hash_is_part_of_state() {
        let a = Simulation::new(1, SimConfig::default());
        let b = Simulation::new(
            1,
            SimConfig {
                map: MapSpec {
                    size: 96,
                    ..MapSpec::default()
                },
                ..SimConfig::default()
            },
        );
        assert_ne!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn wander_actually_moves_things() {
        let mut sim = Simulation::new(3, flat());
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

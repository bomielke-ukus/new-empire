//! Fixtures shared by the behaviour test files.
//!
//! Deliberately thin. `crates/sim/src/simulation.rs` has its own private
//! versions of most of these; they cannot be reached from an integration
//! test, and duplicating them per file was worse than one shared module.

#![allow(dead_code)] // Each test file uses a different subset.

use sim::{Command, CommandKind, EntityId, KindId, PlayerId, SimConfig, Simulation, Vec2Fx};

/// The default map: an Inland generation with the standard start kit.
pub fn inland(seed: u64) -> Simulation {
    Simulation::new(seed, SimConfig::default())
}

pub fn run(sim: &mut Simulation, ticks: u32) {
    for _ in 0..ticks {
        sim.step();
    }
}

/// Live entities of `kind` belonging to `player`, in slot order.
pub fn owned(sim: &Simulation, player: PlayerId, kind: KindId) -> Vec<EntityId> {
    sim.world()
        .slots()
        .filter(|s| sim.world().owner[s.index()] == player && sim.world().kind[s.index()] == kind)
        .map(|s| sim.world().id_at(s))
        .collect()
}

pub fn index_of(sim: &Simulation, id: EntityId) -> usize {
    sim.world().slot(id).expect("entity is alive").index()
}

pub fn pos_of(sim: &Simulation, id: EntityId) -> Vec2Fx {
    sim.world().pos[index_of(sim, id)]
}

/// The nearest live entity of `kind` to `from`, by squared distance.
pub fn nearest_kind(sim: &Simulation, kind: KindId, from: Vec2Fx) -> EntityId {
    sim.world()
        .slots()
        .filter(|s| sim.world().kind[s.index()] == kind)
        .min_by_key(|s| from.distance_sq_raw(sim.world().pos[s.index()]))
        .map(|s| sim.world().id_at(s))
        .expect("no entity of that kind on the map")
}

pub fn spawn(player: PlayerId, kind: KindId, pos: Vec2Fx) -> Command {
    Command {
        player,
        kind: CommandKind::Spawn { kind, pos },
    }
}

pub fn move_to(player: PlayerId, ids: Vec<EntityId>, target: Vec2Fx) -> Command {
    Command {
        player,
        kind: CommandKind::Move { ids, target },
    }
}

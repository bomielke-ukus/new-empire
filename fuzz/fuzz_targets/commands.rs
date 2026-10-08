//! The command stream is the simulation's only input (`docs/04` §3): from the
//! player, from the computer opponents, and later from the network. Any
//! sequence of commands, well-formed or not, naming live entities, dead
//! ones, other sides' or none, must leave every invariant standing: no
//! panic, nothing out of the map, no resource made or lost by accident.
//!
//! The bytes are read as a script: how many commands this tick, each one's
//! variant and fields, then how many ticks to run. Entity handles are mostly
//! picked from what is alive, so the commands reach the systems behind
//! validation, and sometimes forged, so the validation is tested too.
#![no_main]

use libfuzzer_sys::fuzz_target;
use sim::kinds::{self, Resource};
use sim::mapgen::{MapKind, MapSpec};
use sim::{
    Command, CommandKind, EntityId, Formation, Fx, Rally, SimConfig, Simulation, Stance, Vec2Fx,
};
use std::sync::OnceLock;

/// Ticks one input may run: two minutes of game time.
const MAX_TICKS: u64 = 2400;

/// Two starting worlds, generated once and copied per input: a land map
/// with its starts, and one with a sea for boats.
fn worlds() -> &'static [Simulation; 2] {
    static WORLDS: OnceLock<[Simulation; 2]> = OnceLock::new();
    WORLDS.get_or_init(|| {
        let world = |kind| {
            Simulation::new(
                7,
                SimConfig {
                    map: MapSpec {
                        kind,
                        size: 48,
                        players: 2,
                    },
                    ..SimConfig::default()
                },
            )
        };
        [world(MapKind::Inland), world(MapKind::Coastal)]
    })
}

/// The fuzzer's bytes, read front to back; zeros once they run out.
struct Script<'a> {
    bytes: &'a [u8],
}

impl Script<'_> {
    fn done(&self) -> bool {
        self.bytes.is_empty()
    }

    fn u8(&mut self) -> u8 {
        match self.bytes.split_first() {
            Some((&b, rest)) => {
                self.bytes = rest;
                b
            }
            None => 0,
        }
    }

    fn u16(&mut self) -> u16 {
        u16::from_le_bytes([self.u8(), self.u8()])
    }

    fn i32(&mut self) -> i32 {
        i32::from_le_bytes([self.u8(), self.u8(), self.u8(), self.u8()])
    }

    fn bool(&mut self) -> bool {
        self.u8() & 1 == 1
    }

    fn pick<T: Copy>(&mut self, from: &[T]) -> T {
        from[self.u8() as usize % from.len()]
    }

    /// A player: one of the match's two, mostly, or one it does not have.
    fn player(&mut self) -> u8 {
        match self.u8() % 8 {
            0..=3 => 0,
            4..=6 => 1,
            _ => 2 + self.u8() % 6,
        }
    }

    /// A kind: a real one, mostly, or an id past the table.
    fn kind(&mut self) -> u16 {
        let n = kinds::all().len() as u16;
        match self.u8() % 16 {
            0 => self.u16(),
            _ => self.u16() % n,
        }
    }

    /// A tile position on the map, mostly; anywhere at all sometimes.
    fn pos(&mut self) -> Vec2Fx {
        if self.u8().is_multiple_of(16) {
            return Vec2Fx::new(Fx::from_raw(self.i32()), Fx::from_raw(self.i32()));
        }
        let x = (self.u8() % 48) as i32;
        let y = (self.u8() % 48) as i32;
        sim::nav::centre((x, y))
    }

    /// A live entity's handle, mostly; a forged or stale one sometimes.
    fn entity(&mut self, sim: &Simulation) -> EntityId {
        let w = sim.world();
        let live = w.len();
        if live == 0 || self.u8().is_multiple_of(16) {
            return EntityId::from_parts(self.u16() as u32, self.u8() as u32);
        }
        let n = self.u16() as usize % live;
        let slot = w.slots().nth(n).expect("n is below the live count");
        w.id_at(slot)
    }

    fn entities(&mut self, sim: &Simulation) -> Vec<EntityId> {
        let n = self.u8() % 6;
        (0..n).map(|_| self.entity(sim)).collect()
    }

    fn command(&mut self, sim: &Simulation, depth: u8) -> CommandKind {
        match self.u8() % 29 {
            0 => CommandKind::Spawn {
                kind: self.kind(),
                pos: self.pos(),
            },
            1 => CommandKind::Despawn {
                id: self.entity(sim),
            },
            2 => CommandKind::Move {
                ids: self.entities(sim),
                target: self.pos(),
            },
            3 => CommandKind::Stop {
                ids: self.entities(sim),
            },
            4 => CommandKind::Gather {
                ids: self.entities(sim),
                node: self.entity(sim),
            },
            5 => CommandKind::Build {
                kind: self.kind(),
                x: (self.u8() % 52) as i32 - 2,
                y: (self.u8() % 52) as i32 - 2,
                ids: self.entities(sim),
            },
            6 => CommandKind::Assist {
                ids: self.entities(sim),
                site: self.entity(sim),
            },
            7 => CommandKind::Repair {
                ids: self.entities(sim),
                building: self.entity(sim),
            },
            8 if depth == 0 => CommandKind::Queued(Box::new(self.command(sim, 1))),
            9 => CommandKind::Train {
                building: self.entity(sim),
                kind: self.kind(),
            },
            10 => CommandKind::CancelTrain {
                building: self.entity(sim),
            },
            11 => CommandKind::SetRally {
                building: self.entity(sim),
                rally: match self.u8() % 3 {
                    0 => Rally::None,
                    1 => Rally::Point(self.pos()),
                    _ => Rally::Entity(self.entity(sim)),
                },
            },
            12 => CommandKind::Research {
                building: self.entity(sim),
                tech: self.u16() % (sim::tech::all().len() as u16 + 2),
            },
            13 => CommandKind::SetFarmReseed {
                farms: self.entities(sim),
                enabled: self.bool(),
            },
            14 => CommandKind::SetAutoReseed {
                enabled: self.bool(),
            },
            15 => CommandKind::Attack {
                ids: self.entities(sim),
                target: self.entity(sim),
            },
            16 => CommandKind::AttackMove {
                ids: self.entities(sim),
                target: self.pos(),
            },
            17 => CommandKind::Patrol {
                ids: self.entities(sim),
                target: self.pos(),
            },
            18 => CommandKind::SetStance {
                ids: self.entities(sim),
                stance: self.pick(&Stance::ALL),
            },
            19 => CommandKind::SetFormation {
                ids: self.entities(sim),
                formation: self.pick(&Formation::ALL),
            },
            20 => CommandKind::Garrison {
                ids: self.entities(sim),
                building: self.entity(sim),
            },
            21 => CommandKind::Ungarrison {
                building: self.entity(sim),
            },
            22 => CommandKind::Resign,
            23 => CommandKind::Cheat {
                resource: self.pick(&Resource::ALL),
            },
            24 => CommandKind::Relic {
                ids: self.entities(sim),
                target: self.entity(sim),
            },
            25 => CommandKind::Trade {
                ids: self.entities(sim),
                dock: self.entity(sim),
            },
            26 => CommandKind::Unload {
                ids: self.entities(sim),
                target: self.pos(),
            },
            // A selection far past any real one, for the bound.
            27 => CommandKind::Stop {
                ids: vec![self.entity(sim); sim::MAX_COMMAND_IDS + 1],
            },
            _ => CommandKind::Move {
                ids: self.entities(sim),
                target: self.pos(),
            },
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let mut script = Script { bytes: data };
    let mut sim = worlds()[script.u8() as usize % 2].clone();
    while !script.done() && sim.tick() < MAX_TICKS {
        for _ in 0..script.u8() % 4 {
            let command = Command {
                player: script.player(),
                kind: script.command(&sim, 0),
            };
            // Invalid commands are issued too: the simulation must ignore
            // what it cannot carry out, whatever validation said.
            let _ = command.validate();
            sim.issue(command);
        }
        // `sim/debug-checks` makes a broken invariant panic inside `step`.
        for _ in 0..1 + script.u8() % 16 {
            sim.step();
        }
    }
});

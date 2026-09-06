//! The simulation's external surface, exercised with input it should refuse
//! rather than crash on.
//!
//! `docs/04` §2 rule 5 says commands are the only way into the simulation,
//! which makes the command interface its entire attack surface — and every
//! command can arrive from a replay file, or later from the network.

use sim::{Command, CommandKind, EntityId, Rally, SimConfig, Simulation, Vec2Fx};

/// A replay can name any player index inside `MAX_PLAYERS`, but a match may
/// have as few as one player. Every command from a player the match does not
/// have must be an inert no-op: such a player owns nothing, so there is
/// nothing for it to act on. The alternative is an index panic reachable from
/// a file.
#[test]
fn every_command_from_a_player_the_match_does_not_have() {
    let mut sim = Simulation::new(1, SimConfig::default());
    let n = sim.players().len() as u8;
    let real: Vec<EntityId> = sim.world().ids().take(6).collect();
    for player in n..8 {
        for kind in [
            CommandKind::Spawn {
                kind: 1,
                pos: Vec2Fx::from_int(5, 5),
            },
            CommandKind::Despawn { id: real[0] },
            CommandKind::Move {
                ids: real.clone(),
                target: Vec2Fx::from_int(9, 9),
            },
            CommandKind::Stop { ids: real.clone() },
            CommandKind::Gather {
                ids: real.clone(),
                node: real[1],
            },
            CommandKind::Build {
                kind: 3,
                x: 12,
                y: 12,
                ids: real.clone(),
            },
            CommandKind::Assist {
                ids: real.clone(),
                site: real[1],
            },
            CommandKind::Train {
                building: real[0],
                kind: 1,
            },
            CommandKind::CancelTrain { building: real[0] },
            CommandKind::SetRally {
                building: real[0],
                rally: Rally::Point(Vec2Fx::from_int(3, 3)),
            },
        ] {
            let c = Command { player, kind };
            assert!(c.validate().is_ok(), "validate rejected {c:?}");
            sim.issue(c);
        }
    }
    for _ in 0..30 {
        sim.step();
    }
    assert_eq!(sim.tick(), 30);
    sim.check().expect("invariants must still hold");
}

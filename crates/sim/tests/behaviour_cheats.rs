//! Cheat codes (`docs/02` `GD-CHEAT-01`): a code gives the player 1000 of
//! a resource; a computer opponent's code does nothing; a replay holds it.

mod common;

use common::*;
use sim::kinds::Resource;
use sim::{Command, CommandKind, Simulation, Source, CHEAT_AMOUNT};

fn cheat(player: u8, resource: Resource) -> Command {
    Command {
        player,
        kind: CommandKind::Cheat { resource },
    }
}

/// REQ: GD-CHEAT-01
#[test]
fn a_code_gives_the_player_a_thousand_and_a_computer_nothing() {
    let mut sim = inland(41);
    let before: Vec<[i32; 4]> = (0..2).map(|p| sim.player(p).unwrap().stockpile).collect();
    for r in Resource::ALL {
        sim.issue(cheat(0, r));
        sim.issue_from(cheat(1, r), Source::Ai);
    }
    run(&mut sim, 3);
    let after: Vec<[i32; 4]> = (0..2).map(|p| sim.player(p).unwrap().stockpile).collect();
    for r in Resource::ALL {
        let i = r.index();
        assert_eq!(after[0][i], before[0][i] + CHEAT_AMOUNT, "{r:?}");
        assert_eq!(
            after[1][i], before[1][i],
            "a computer's code does nothing: {r:?}"
        );
    }
}

/// A code is an order like any other: the recording holds it, who gave it
/// and all, and plays back to the same world.
///
/// REQ: GD-CHEAT-01
#[test]
fn a_replay_with_codes_plays_back_the_same() {
    let mut sim: Simulation = inland(42);
    run(&mut sim, 5);
    sim.issue(cheat(0, Resource::Gold));
    sim.issue_from(cheat(1, Resource::Food), Source::Ai);
    run(&mut sim, 20);
    let replay = sim.replay();
    let hash = replay.verify().expect("the replay plays back");
    assert_eq!(hash, sim.state_hash());
}

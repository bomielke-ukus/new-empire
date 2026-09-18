//! Victory and defeat (`docs/02` §10): conquest, resigning, and the score
//! that decides a match at a time limit.

mod common;
use common::{owned, run, spawn};
use sim::{kinds, Command, CommandKind, MapKind, MapSpec, SimConfig, Simulation};

/// A flat map with a Town Center and a villager for each of two sides:
/// the least that keeps both in the match.
fn flat() -> Simulation {
    let mut sim = Simulation::new(
        7,
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
    let fp = kinds::info(kinds::TOWN_CENTER).footprint as i32;
    for (p, (x, y)) in [(0u8, (10, 10)), (1u8, (36, 36))] {
        sim.issue(spawn(
            p,
            kinds::TOWN_CENTER,
            sim::nav::building_centre(x, y, fp),
        ));
        sim.issue(spawn(p, kinds::VILLAGER, sim::nav::centre((x, y + 3))));
    }
    run(&mut sim, 3);
    sim
}

/// A side with no units and no building that can make one is out; the
/// last side standing has won. Until then there is no winner.
///
/// REQ: GD-WIN-01
#[test]
fn the_last_side_standing_wins_by_conquest() {
    let mut sim = flat();
    assert!(sim.standing(0) && sim.standing(1));
    assert_eq!(sim.winner(), None);
    // Take everything of player 1's away but a house: a house makes
    // nothing, so it does not keep the side in the match.
    let house = kinds::info(kinds::HOUSE).footprint as i32;
    sim.issue(spawn(
        1,
        kinds::HOUSE,
        sim::nav::building_centre(40, 40, house),
    ));
    run(&mut sim, 3);
    let theirs: Vec<_> = {
        let w = sim.world();
        w.slots()
            .filter(|s| w.owner[s.index()] == 1 && w.kind[s.index()] != kinds::HOUSE)
            .map(|s| w.id_at(s))
            .collect()
    };
    for id in theirs {
        sim.issue(Command {
            player: 1,
            kind: CommandKind::Despawn { id },
        });
    }
    run(&mut sim, 3);
    assert_eq!(owned(&sim, 1, kinds::HOUSE).len(), 1);
    assert!(!sim.standing(1), "a house alone keeps nobody in the match");
    assert!(sim.standing(0));
    assert_eq!(sim.winner(), Some(0));
    assert!(sim.over());
}

/// A single villager keeps a side in the match; a lone Town Center does
/// too, because it can train one.
///
/// REQ: GD-WIN-01
#[test]
fn a_villager_or_a_town_center_keeps_a_side_standing() {
    let mut sim = flat();
    let tc = owned(&sim, 1, kinds::TOWN_CENTER)[0];
    let units: Vec<_> = {
        let w = sim.world();
        w.slots()
            .filter(|s| w.owner[s.index()] == 1 && kinds::info(w.kind[s.index()]).mobile)
            .map(|s| w.id_at(s))
            .collect()
    };
    for id in units {
        sim.issue(Command {
            player: 1,
            kind: CommandKind::Despawn { id },
        });
    }
    run(&mut sim, 3);
    assert!(sim.standing(1), "the Town Center can still train");
    sim.issue(Command {
        player: 1,
        kind: CommandKind::Despawn { id: tc },
    });
    sim.issue(spawn(1, kinds::VILLAGER, sim::nav::centre((30, 30))));
    run(&mut sim, 3);
    assert!(sim.standing(1), "one villager is still a side");
    assert_eq!(sim.winner(), None);
}

/// Resigning takes a side out at once; its later commands are ignored.
///
/// REQ: GD-WIN-01
#[test]
fn resigning_is_defeat_and_silences_the_side() {
    let mut sim = flat();
    sim.issue(Command {
        player: 1,
        kind: CommandKind::Resign,
    });
    run(&mut sim, 3);
    assert!(!sim.standing(1));
    assert_eq!(sim.winner(), Some(0));
    let before = sim.world().len();
    sim.issue(spawn(1, kinds::VILLAGER, sim::nav::centre((30, 30))));
    run(&mut sim, 3);
    assert_eq!(
        sim.world().len(),
        before,
        "a resigned side's orders do nothing"
    );
    let replay = sim.replay();
    assert!(replay
        .commands
        .iter()
        .any(|(_, c)| c.kind == CommandKind::Resign));
    replay.verify().expect("a resignation replays");
}

/// The score counts what a side gathered and what it has standing, so a
/// side that gathered more and lost nothing is ahead.
#[test]
fn the_score_counts_what_was_gathered_and_what_stands() {
    let mut sim = flat();
    let (a, b) = (sim.score(0), sim.score(1));
    assert_eq!(a, b, "the same start kit scores the same");
    assert!(a > 0);
    sim.issue(spawn(0, kinds::CLUBMAN, sim::nav::centre((10, 10))));
    run(&mut sim, 3);
    assert!(sim.score(0) > sim.score(1), "a standing soldier counts");
    let id = owned(&sim, 0, kinds::CLUBMAN)[0];
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Despawn { id },
    });
    run(&mut sim, 3);
    assert_eq!(sim.score(0), sim.score(1), "and stops counting when gone");
}

/// The declared bonus of the hardest difficulty: a gather rate a player
/// cannot have, applied by the match setup, not by the opponent.
#[test]
fn a_gather_bonus_is_a_match_setting_the_setup_screen_can_refuse() {
    let mut config = SimConfig {
        map: MapSpec {
            kind: MapKind::Flat,
            size: 48,
            players: 2,
        },
        wander: false,
        gather_bonus_pct: vec![0, 25],
        ..SimConfig::default()
    };
    config.validate().expect("a modest bonus is allowed");
    let sim = Simulation::new(1, config.clone());
    let (plain, bonus) = (sim.modifiers(0), sim.modifiers(1));
    assert!(bonus.gather_rate(kinds::Resource::Wood) > plain.gather_rate(kinds::Resource::Wood));
    config.gather_bonus_pct = vec![0, 500];
    assert!(
        config.validate().is_err(),
        "five hundred percent is refused"
    );
    config.gather_bonus_pct = vec![-10, 0];
    assert!(config.validate().is_err(), "as is a handicap below zero");
}

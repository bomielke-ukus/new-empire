//! Fog of war (`docs/02` §9 `GD-FOG-01`; `docs/04` §6) and the path
//! budget's priority (`TA-PATH-06`).

mod common;

use common::*;
use sim::{
    kinds, Command, CommandKind, NavState, SimConfig, Simulation, Source, Vec2Fx, Visibility,
};

/// A flat, empty map: nothing but what the test spawns.
fn arena() -> Simulation {
    Simulation::new(
        5,
        SimConfig {
            map: sim::MapSpec {
                kind: sim::MapKind::Flat,
                size: 64,
                players: 2,
            },
            wander: false,
            ..SimConfig::default()
        },
    )
}

fn at(x: i32, y: i32) -> Vec2Fx {
    sim::nav::centre((x, y))
}

fn state(sim: &Simulation, p: u8, x: i32, y: i32) -> Visibility {
    sim.fog(p).unwrap().state(x, y)
}

/// REQ: GD-FOG-01
#[test]
fn a_tile_is_unexplored_then_visible_then_explored_and_a_building_is_remembered() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::VILLAGER, at(10, 10)));
    sim.issue(spawn(1, kinds::HOUSE, at(14, 10)));
    sim.issue(spawn(1, kinds::CLUBMAN, at(50, 50)));
    run(&mut sim, 3);
    // A villager sees four tiles around it, and no further.
    assert_eq!(state(&sim, 0, 10, 10), Visibility::Visible);
    assert_eq!(state(&sim, 0, 14, 10), Visibility::Visible);
    assert_eq!(state(&sim, 0, 10, 15), Visibility::Unexplored);
    assert_eq!(state(&sim, 0, 50, 50), Visibility::Unexplored);
    let fog = sim.fog(0).unwrap();
    assert_eq!(fog.visible_count(), fog.explored_count());
    assert!(
        fog.memories().all(|(_, m)| m.kind != kinds::HOUSE),
        "in sight is not a memory"
    );
    // Their side sees its own things and nothing of mine.
    assert_eq!(state(&sim, 1, 50, 50), Visibility::Visible);
    assert_eq!(state(&sim, 1, 14, 10), Visibility::Visible);
    // A house sees four tiles past its edge: five from its centre.
    assert_eq!(state(&sim, 1, 10, 10), Visibility::Visible);
    assert_eq!(state(&sim, 1, 8, 10), Visibility::Unexplored);

    // The villager walks off: the ground it saw stays explored, the house
    // stays remembered where it stood, and the memory outlives the house.
    let vill = owned(&sim, 0, kinds::VILLAGER)[0];
    sim.issue(move_to(0, vec![vill], at(10, 30)));
    run(&mut sim, 600);
    assert_eq!(state(&sim, 0, 10, 10), Visibility::Explored);
    assert_eq!(state(&sim, 0, 10, 30), Visibility::Visible);
    let house = owned(&sim, 1, kinds::HOUSE)[0];
    let (tile, memory) = sim
        .fog(0)
        .unwrap()
        .memories()
        .find(|(_, m)| m.kind == kinds::HOUSE)
        .expect("remembered");
    assert_eq!(memory.owner, 1);
    sim.issue(Command {
        player: 1,
        kind: CommandKind::Despawn { id: house },
    });
    run(&mut sim, 3);
    assert!(owned(&sim, 1, kinds::HOUSE).is_empty());
    assert!(
        sim.fog(0).unwrap().remembered(tile.0, tile.1).is_some(),
        "gone, but not seen to go"
    );
    // Back to look: the memory is corrected.
    sim.issue(move_to(0, vec![vill], at(12, 10)));
    run(&mut sim, 600);
    assert_eq!(state(&sim, 0, tile.0, tile.1), Visibility::Visible);
    assert!(sim.fog(0).unwrap().remembered(tile.0, tile.1).is_none());
}

/// Sight belongs to the standing and the living: a unit inside a building
/// sees nothing of its own, and neither does a corpse.
#[test]
fn garrisoned_units_and_corpses_do_not_see() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::WATCH_TOWER, at(20, 20)));
    sim.issue(spawn(0, kinds::SCOUT, at(22, 20)));
    run(&mut sim, 3);
    let before = sim.fog(0).unwrap().visible_count();
    let scout = owned(&sim, 0, kinds::SCOUT)[0];
    let tower = owned(&sim, 0, kinds::WATCH_TOWER)[0];
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Garrison {
            ids: vec![scout],
            building: tower,
        },
    });
    run(&mut sim, 100);
    let w = sim.world();
    assert_eq!(w.inside[index_of(&sim, scout)], Some(tower), "went in");
    let inside = sim.fog(0).unwrap().visible_count();
    assert!(
        inside < before,
        "the scout's wide sight is gone: {inside} < {before}"
    );
    // Out again, and killed: a corpse sees nothing either.
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Ungarrison { building: tower },
    });
    run(&mut sim, 3);
    let out = sim.fog(0).unwrap().visible_count();
    assert!(out > inside);
    // Off to one side, so the two discs no longer overlap; then the tower
    // goes, and its disc with it.
    sim.issue(move_to(0, vec![scout], at(50, 50)));
    run(&mut sim, 400);
    let apart = sim.fog(0).unwrap().visible_count();
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Despawn { id: tower },
    });
    run(&mut sim, 3);
    let alone = sim.fog(0).unwrap().visible_count();
    assert!(alone < apart, "the tower's sight went with it");
}

/// The player's path requests are served before a computer opponent's when
/// the destination budget binds; nobody is dropped.
///
/// REQ: TA-PATH-06
#[test]
fn player_issued_paths_are_planned_before_ai_issued_ones() {
    let mut sim = arena();
    // A palisade across the map with one gap, so no goal is in a straight
    // line of sight and every trip needs a field.
    for y in 0..64 {
        if y != 32 {
            sim.issue(spawn(1, kinds::PALISADE_WALL, at(30, y)));
        }
    }
    // Twelve units for the player and twenty-four for the opponent, each
    // sent to its own distinct destination: thirty-six destinations
    // against a budget of sixteen a tick.
    for k in 0..24 {
        if k < 12 {
            sim.issue(spawn(0, kinds::CLUBMAN, at(10, 8 + k * 2)));
        }
        sim.issue(spawn(0, kinds::CLUBMAN, at(12, 8 + k * 2)));
    }
    run(&mut sim, 3);
    let clubs = owned(&sim, 0, kinds::CLUBMAN);
    assert_eq!(clubs.len(), 36);
    let (by_player, by_ai) = clubs.split_at(12);
    for (k, id) in by_ai.iter().enumerate() {
        sim.issue_from(move_to(0, vec![*id], at(50, 8 + k as i32 * 2)), Source::Ai);
    }
    for (k, id) in by_player.iter().enumerate() {
        sim.issue(move_to(0, vec![*id], at(52, 9 + k as i32 * 2)));
    }
    // Commands land two ticks on; the tick they land, the budget is spent
    // on the player's first.
    run(&mut sim, 3);
    let planning = |sim: &Simulation, ids: &[sim::EntityId]| {
        ids.iter()
            .filter(|id| {
                matches!(&sim.world().nav[index_of(sim, **id)], Some(n) if n.state == NavState::Planning)
            })
            .count()
    };
    let walking = |sim: &Simulation, ids: &[sim::EntityId]| {
        ids.iter()
            .filter(|id| {
                matches!(&sim.world().nav[index_of(sim, **id)], Some(n) if n.state == NavState::Walking)
            })
            .count()
    };
    assert_eq!(
        planning(&sim, by_player),
        0,
        "every player-issued trip has a heading"
    );
    assert!(
        planning(&sim, by_ai) > 0,
        "some AI-issued trips wait a tick: {} walking",
        walking(&sim, by_ai)
    );
    let w = sim.world();
    assert!(by_ai.iter().all(|id| w.priority[index_of(&sim, *id)] == 1));
    assert!(by_player
        .iter()
        .all(|id| w.priority[index_of(&sim, *id)] == 0));
    // Nobody is dropped: a few ticks on, everyone is under way.
    run(&mut sim, 6);
    assert_eq!(planning(&sim, by_ai), 0, "served, not dropped");
    // And the replay carries who issued what.
    let replay = sim.replay();
    assert_eq!(replay.sources.len(), replay.commands.len());
    assert_eq!(
        replay.sources.iter().filter(|s| **s == Source::Ai).count(),
        24
    );
    replay.verify().expect("identical at every tick");
}

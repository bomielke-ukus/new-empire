//! Priests (`docs/02` §5.5): the chant that turns an enemy unit, the faith
//! it spends, what cannot be turned, and the healing.

mod common;

use common::*;
use sim::priests::{CHANT_MAX, CHANT_MIN, CONVERT_RANGE, FAITH_TICKS, HEAL_HP};
use sim::{kinds, Command, CommandKind, EntityId, Event, Fx, Order, SimConfig, Simulation, Stance};

/// A flat, empty map: nothing but what the test spawns.
fn arena() -> Simulation {
    Simulation::new(
        7,
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

fn at(x: i32, y: i32) -> sim::Vec2Fx {
    sim::nav::centre((x, y))
}

/// Spawns one `kind` for `player` and returns it.
fn put(sim: &mut Simulation, player: u8, kind: sim::KindId, x: i32, y: i32) -> EntityId {
    let before = owned(sim, player, kind);
    sim.issue(spawn(player, kind, at(x, y)));
    run(sim, 3);
    *owned(sim, player, kind)
        .iter()
        .find(|id| !before.contains(id))
        .expect("spawned")
}

fn passive(sim: &mut Simulation, player: u8, ids: Vec<EntityId>) {
    sim.issue(Command {
        player,
        kind: CommandKind::SetStance {
            ids,
            stance: Stance::Passive,
        },
    });
}

/// Spawns one of player 1's, passive before anything is near it: a unit
/// told to stand down only after it has picked a fight may already have
/// loosed.
fn foe(sim: &mut Simulation, kind: sim::KindId, x: i32, y: i32) -> EntityId {
    let id = put(sim, 1, kind, x, y);
    passive(sim, 1, vec![id]);
    run(sim, 3);
    id
}

fn send(sim: &mut Simulation, priest: EntityId, target: EntityId) {
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![priest],
            target,
        },
    });
}

fn owner_of(sim: &Simulation, id: EntityId) -> u8 {
    sim.world().owner[index_of(sim, id)]
}

fn health(sim: &Simulation, id: EntityId) -> Fx {
    sim.world().health[index_of(sim, id)]
}

/// Steps until a chant begins, and returns the tick it began on.
fn until_chant(sim: &mut Simulation, limit: u32) -> Option<u64> {
    for _ in 0..limit {
        sim.step();
        if sim
            .events()
            .iter()
            .any(|e| matches!(e, Event::Chant { .. }))
        {
            return Some(sim.tick());
        }
    }
    None
}

/// Steps until a unit changes sides, and returns the tick it did.
fn until_converted(sim: &mut Simulation, limit: u32) -> Option<u64> {
    for _ in 0..limit {
        sim.step();
        if sim
            .events()
            .iter()
            .any(|e| matches!(e, Event::Converted { .. }))
        {
            return Some(sim.tick());
        }
    }
    None
}

/// REQ: GD-PRIEST-01
#[test]
fn a_priest_walks_into_range_chants_and_the_unit_changes_sides_for_good() {
    let mut sim = arena();
    let sword = foe(&mut sim, kinds::SWORDSMAN, 30, 20);
    let priest = put(&mut sim, 0, kinds::PRIEST, 10, 20);
    send(&mut sim, priest, sword);
    let began = until_chant(&mut sim, 600).expect("the priest chants");
    // It chanted from within its reach, not from where it started.
    let gap = pos_of(&sim, priest).distance(pos_of(&sim, sword));
    assert!(
        gap <= Fx::from_int(CONVERT_RANGE + 2),
        "chanted from {gap:?}"
    );
    assert!(
        gap >= Fx::from_int(CONVERT_RANGE - 2),
        "walked too close: {gap:?}"
    );
    assert_eq!(owner_of(&sim, sword), 1, "not yet");
    let done = until_converted(&mut sim, 400).expect("the swordsman turns");
    let took = done - began;
    assert!(
        (u64::from(CHANT_MIN)..=u64::from(CHANT_MAX) + 1).contains(&took),
        "the chant took {took} ticks"
    );
    assert_eq!(owner_of(&sim, sword), 0);
    let i = index_of(&sim, sword);
    assert_eq!(sim.world().order[i], Order::Idle);
    // For good: it stays turned, and it fights for its new side.
    run(&mut sim, 200);
    assert_eq!(owner_of(&sim, sword), 0);
    assert_eq!(sim.player(0).unwrap().pop, 2);
    assert_eq!(sim.player(1).unwrap().pop, 0);
}

/// REQ: GD-PRIEST-01
#[test]
fn the_chant_is_drawn_from_the_simulation_and_differs_between_matches() {
    let mut lengths = Vec::new();
    for seed in [1, 2, 3, 4, 5, 6] {
        let mut sim = Simulation::new(
            seed,
            SimConfig {
                map: sim::MapSpec {
                    kind: sim::MapKind::Flat,
                    size: 64,
                    players: 2,
                },
                wander: false,
                ..SimConfig::default()
            },
        );
        let club = foe(&mut sim, kinds::CLUBMAN, 14, 20);
        let priest = put(&mut sim, 0, kinds::PRIEST, 10, 20);
        send(&mut sim, priest, club);
        let began = until_chant(&mut sim, 100).expect("chants");
        let done = until_converted(&mut sim, 400).expect("converts");
        lengths.push(done - began);
    }
    lengths.sort_unstable();
    lengths.dedup();
    assert!(
        lengths.len() > 1,
        "every chant the same length: {lengths:?}"
    );
}

/// REQ: GD-PRIEST-02
#[test]
fn a_conversion_spends_the_faith_and_the_next_waits_forty_seconds() {
    let mut sim = arena();
    let a = foe(&mut sim, kinds::CLUBMAN, 14, 20);
    let b = foe(&mut sim, kinds::CLUBMAN, 14, 23);
    let priest = put(&mut sim, 0, kinds::PRIEST, 10, 20);
    send(&mut sim, priest, a);
    let first = until_converted(&mut sim, 400).expect("the first turns");
    // Spent, and already a tick on its way back.
    assert!(sim.world().reload[index_of(&sim, priest)] >= FAITH_TICKS - 1);
    send(&mut sim, priest, b);
    // It goes, and waits in reach for its faith.
    let second = until_chant(&mut sim, u32::from(FAITH_TICKS) + 50).expect("chants again");
    let waited = second - first;
    assert!(
        waited >= u64::from(FAITH_TICKS),
        "chanted again after {waited} ticks"
    );
    assert!(
        waited <= u64::from(FAITH_TICKS) + 5,
        "waited {waited} ticks"
    );
    until_converted(&mut sim, 400).expect("the second turns");
    assert_eq!(owner_of(&sim, b), 0);
}

/// REQ: GD-PRIEST-02
#[test]
fn faith_comes_back_at_its_own_pace_inside_a_building_too() {
    let mut sim = arena();
    let a = foe(&mut sim, kinds::CLUBMAN, 14, 20);
    let priest = put(&mut sim, 0, kinds::PRIEST, 10, 20);
    let tc = put(&mut sim, 0, kinds::TOWN_CENTER, 10, 26);
    send(&mut sim, priest, a);
    until_converted(&mut sim, 400).expect("turns");
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Garrison {
            ids: vec![priest],
            building: tc,
        },
    });
    run(&mut sim, 100);
    let i = index_of(&sim, priest);
    assert_eq!(sim.world().inside[i], Some(tc));
    assert!(sim.world().reload[i] > 0, "the door gave the faith back");
}

/// REQ: GD-PRIEST-03
#[test]
fn buildings_cannot_be_converted_but_siege_can() {
    let mut sim = arena();
    let house = put(&mut sim, 1, kinds::HOUSE, 14, 20);
    let engine = foe(&mut sim, kinds::STONE_THROWER, 14, 26);
    let priest = put(&mut sim, 0, kinds::PRIEST, 10, 20);
    assert!(sim.convertible(house, 0).is_none());
    assert!(sim.convertible(engine, 0).is_some());
    send(&mut sim, priest, house);
    run(&mut sim, 300);
    assert_eq!(owner_of(&sim, house), 1);
    assert_eq!(sim.world().order[index_of(&sim, priest)], Order::Idle);
    send(&mut sim, priest, engine);
    until_converted(&mut sim, 400).expect("the engine turns");
    assert_eq!(owner_of(&sim, engine), 0);
}

/// REQ: GD-PRIEST-03
#[test]
fn nature_and_the_priests_own_side_are_not_converted() {
    let mut sim = inland(5);
    let tc = owned(&sim, 0, kinds::TOWN_CENTER)[0];
    let c = pos_of(&sim, tc);
    let gazelle = nearest_kind(&sim, kinds::GAZELLE, c);
    let friend = owned(&sim, 0, kinds::VILLAGER)[0];
    assert!(sim.convertible(friend, 0).is_none());
    assert!(sim.convertible(gazelle, 0).is_none());
    let priest = put(&mut sim, 0, kinds::PRIEST, c.x.floor() + 3, c.y.floor() + 3);
    send(&mut sim, priest, gazelle);
    run(&mut sim, 5);
    assert_eq!(sim.world().order[index_of(&sim, priest)], Order::Idle);
}

/// Wounds `victim` (player 0's) with an enemy clubman beside it, then
/// takes the clubman away.
fn wound(sim: &mut Simulation, victim: EntityId, x: i32, y: i32) {
    passive(sim, 0, vec![victim]);
    let foe = put(sim, 1, kinds::BOWMAN, x, y);
    sim.issue(Command {
        player: 1,
        kind: CommandKind::Attack {
            ids: vec![foe],
            target: victim,
        },
    });
    let full = health(sim, victim);
    for _ in 0..1200 {
        sim.step();
        if full - health(sim, victim) >= Fx::from_int(25) {
            break;
        }
    }
    sim.issue(Command {
        player: 1,
        kind: CommandKind::Despawn { id: foe },
    });
    run(sim, 1);
    // Called back from running home, and left standing.
    sim.issue(move_to(0, vec![victim], at(x, y)));
    run(sim, 3);
    for _ in 0..600 {
        if sim.world().order[index_of(sim, victim)] == Order::Idle {
            return;
        }
        sim.step();
    }
    panic!("never came back");
}

/// REQ: GD-PRIEST-04
#[test]
fn a_priest_heals_a_wounded_unit_beside_it_three_hit_points_a_second() {
    let mut sim = arena();
    let sword = put(&mut sim, 0, kinds::SWORDSMAN, 20, 20);
    wound(&mut sim, sword, 21, 20);
    let low = health(&sim, sword);
    let max = Fx::from_int(kinds::info(kinds::SWORDSMAN).max_health);
    assert!(max - low >= Fx::from_int(25), "wounded to {low:?}");
    let pos = pos_of(&sim, sword);
    let (x, y) = (pos.x.floor(), pos.y.floor());
    put(&mut sim, 0, kinds::PRIEST, x + 2, y);
    let start = health(&sim, sword);
    run(&mut sim, 100);
    let gained = health(&sim, sword) - start;
    // Five seconds: five heals, give or take the one in flight.
    assert!(
        gained >= Fx::from_int(4 * HEAL_HP) && gained <= Fx::from_int(6 * HEAL_HP),
        "gained {gained:?}"
    );
    run(&mut sim, 2000);
    assert_eq!(health(&sim, sword), max, "healed to full and no further");
}

/// REQ: GD-PRIEST-04
#[test]
fn no_healing_for_siege_or_the_enemy_or_while_converting() {
    let mut sim = arena();
    let engine = put(&mut sim, 0, kinds::STONE_THROWER, 20, 20);
    wound(&mut sim, engine, 21, 20);
    let engine_low = health(&sim, engine);
    let pos = pos_of(&sim, engine);
    let (x, y) = (pos.x.floor(), pos.y.floor());
    put(&mut sim, 0, kinds::PRIEST, x + 2, y);
    run(&mut sim, 100);
    assert_eq!(health(&sim, engine), engine_low, "timber is not healed");

    // An enemy's wounded unit beside the priest stays wounded.
    let mut sim = arena();
    let theirs = put(&mut sim, 1, kinds::CLUBMAN, 20, 20);
    let mine = put(&mut sim, 0, kinds::CLUBMAN, 21, 20);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![mine],
            target: theirs,
        },
    });
    passive(&mut sim, 1, vec![theirs]);
    run(&mut sim, 40);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Despawn { id: mine },
    });
    run(&mut sim, 1);
    let low = health(&sim, theirs);
    let max = Fx::from_int(kinds::info(kinds::CLUBMAN).max_health);
    assert!(low < max);
    let p = pos_of(&sim, theirs);
    put(&mut sim, 0, kinds::PRIEST, p.x.floor() + 2, p.y.floor());
    run(&mut sim, 100);
    assert_eq!(health(&sim, theirs), low, "the enemy is not healed");

    // A priest at its chant heals nobody.
    let mut sim = arena();
    let sword = put(&mut sim, 0, kinds::SWORDSMAN, 20, 20);
    wound(&mut sim, sword, 21, 20);
    let pos = pos_of(&sim, sword);
    let (x, y) = (pos.x.floor(), pos.y.floor());
    let them = foe(&mut sim, kinds::CLUBMAN, x + 6, y);
    let priest = put(&mut sim, 0, kinds::PRIEST, x + 2, y);
    send(&mut sim, priest, them);
    until_chant(&mut sim, 50).expect("chants");
    let before = health(&sim, sword);
    run(&mut sim, u32::from(CHANT_MIN) - 5);
    assert_eq!(health(&sim, sword), before, "healed while chanting");
}

/// A priest hit while it chants keeps chanting; idle, it runs like a
/// villager.
#[test]
fn a_priest_at_its_chant_stands_its_ground() {
    let mut sim = arena();
    let target = foe(&mut sim, kinds::CLUBMAN, 15, 20);
    let archer = foe(&mut sim, kinds::BOWMAN, 13, 24);
    let priest = put(&mut sim, 0, kinds::PRIEST, 10, 20);
    assert_eq!(sim.world().stance[index_of(&sim, priest)], Stance::Passive);
    send(&mut sim, priest, target);
    until_chant(&mut sim, 50).expect("chants");
    sim.issue(Command {
        player: 1,
        kind: CommandKind::Attack {
            ids: vec![archer],
            target: priest,
        },
    });
    run(&mut sim, 40);
    let i = index_of(&sim, priest);
    assert!(
        matches!(sim.world().order[i], Order::Convert { .. }),
        "{:?}",
        sim.world().order[i]
    );
}

/// The soldiers who were fighting a unit stop when it comes over to
/// their side.
#[test]
fn a_converted_unit_is_no_longer_its_new_sides_target() {
    let mut sim = arena();
    let foe = foe(&mut sim, kinds::HOPLITE, 15, 20);
    let priest = put(&mut sim, 0, kinds::PRIEST, 10, 20);
    let ally = put(&mut sim, 0, kinds::CLUBMAN, 17, 22);
    send(&mut sim, priest, foe);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![ally],
            target: foe,
        },
    });
    until_converted(&mut sim, 400).expect("turns");
    run(&mut sim, 30);
    assert_eq!(owner_of(&sim, foe), 0);
    assert!(!matches!(
        sim.world().order[index_of(&sim, ally)],
        Order::Attack { .. }
    ));
    let max = Fx::from_int(kinds::info(kinds::HOPLITE).max_health);
    let hp = health(&sim, foe);
    run(&mut sim, 60);
    assert!(health(&sim, foe) >= hp && hp > Fx::ZERO && hp <= max);
}

/// A match with a conversion in it replays to the same hash.
#[test]
fn a_conversion_replays_identically() {
    let play = || {
        let mut sim = arena();
        let foe = foe(&mut sim, kinds::SWORDSMAN, 18, 20);
        let priest = put(&mut sim, 0, kinds::PRIEST, 10, 20);
        send(&mut sim, priest, foe);
        run(&mut sim, 600);
        (owner_of(&sim, foe), sim.state_hash())
    };
    let a = play();
    let b = play();
    assert_eq!(a, b);
}

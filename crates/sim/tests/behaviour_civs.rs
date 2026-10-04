//! The civilizations (`docs/02` §11, `docs/07` D32): what each may not
//! have, and its bonuses where they bite.

mod common;

use common::*;
use sim::kinds::Resource;
use sim::{
    kinds, tech, Civ, Command, CommandKind, EntityId, Fx, PlaceError, ResearchError, SimConfig,
    Simulation, Stance, TrainError,
};

/// A flat map for two sides with the given civilizations, rich enough to
/// buy anything.
fn arena(civs: [Civ; 2]) -> Simulation {
    Simulation::new(
        7,
        SimConfig {
            map: sim::MapSpec {
                kind: sim::MapKind::Flat,
                size: 64,
                players: 2,
            },
            wander: false,
            starting_stockpile: [20_000; 4],
            civs: civs.to_vec(),
            ..SimConfig::default()
        },
    )
}

/// Spawns one `kind` for `player` and returns it.
fn put(sim: &mut Simulation, player: u8, kind: sim::KindId, x: i32, y: i32) -> EntityId {
    let before: Vec<EntityId> = sim.world().slots().map(|s| sim.world().id_at(s)).collect();
    let fp = kinds::info(kind).footprint as i32;
    let pos = if fp > 1 {
        sim::nav::building_centre(x, y, fp)
    } else {
        sim::nav::centre((x, y))
    };
    sim.issue(spawn(player, kind, pos));
    run(sim, 3);
    let w = sim.world();
    w.slots()
        .map(|s| w.id_at(s))
        .find(|id| !before.contains(id) && w.kind[id.index()] == kind)
        .expect("spawned")
}

fn hp(sim: &Simulation, id: EntityId) -> Fx {
    sim.world().health[index_of(sim, id)]
}

/// Each civilization is refused what it is denied, with its name, and its
/// buildings do not offer it.
#[test]
fn a_civilization_cannot_have_what_it_is_denied() {
    let mut sim = arena([Civ::Egyptians, Civ::Greeks]);
    assert_eq!(
        sim.can_build(0, kinds::ACADEMY),
        Err(PlaceError::Denied {
            civ: Civ::Egyptians
        })
    );
    let stable = put(&mut sim, 0, kinds::STABLE, 10, 10);
    assert!(!sim.roster(0, kinds::STABLE).contains(&kinds::HEAVY_CAVALRY));
    assert_eq!(
        sim.can_train(0, stable, kinds::HEAVY_CAVALRY),
        Err(TrainError::Denied {
            civ: Civ::Egyptians
        })
    );
    // The Greeks have the Academy, and no chariots.
    assert!(!matches!(
        sim.can_build(1, kinds::ACADEMY),
        Err(PlaceError::Denied { .. })
    ));
    let range = put(&mut sim, 1, kinds::ARCHERY_RANGE, 40, 40);
    assert!(!sim
        .roster(1, kinds::ARCHERY_RANGE)
        .contains(&kinds::CHARIOT_ARCHER));
    assert_eq!(
        sim.can_train(1, range, kinds::CHARIOT_ARCHER),
        Err(TrainError::Denied { civ: Civ::Greeks })
    );
    // And a denied technology.
    let mut sim = arena([Civ::Shang, Civ::Greeks]);
    let workshop = put(&mut sim, 0, kinds::SIEGE_WORKSHOP, 10, 10);
    assert_eq!(
        sim.can_research(0, workshop, tech::TORSION),
        Err(ResearchError::Denied { civ: Civ::Shang })
    );
}

/// A match with no civilizations named denies nothing.
#[test]
fn without_civilizations_nothing_is_denied() {
    let sim = Simulation::new(
        7,
        SimConfig {
            map: sim::MapSpec {
                kind: sim::MapKind::Flat,
                size: 64,
                players: 2,
            },
            ..SimConfig::default()
        },
    );
    assert_eq!(sim.civ(0), None);
    for k in kinds::all() {
        assert_eq!(sim.cost_of(0, k.id), k.cost, "{}", k.name);
        assert_eq!(sim.max_health_of(0, k.id), k.max_health, "{}", k.name);
    }
}

/// Bonuses that are numbers: the Egyptians' gold, the Sumerians' farms,
/// the Assyrians' villagers, from the first tick.
#[test]
fn standing_bonuses_hold_from_the_start() {
    let sim = arena([Civ::Egyptians, Civ::Sumerians]);
    assert_eq!(sim.modifiers(0).gather_rate_pct[Resource::Gold.index()], 20);
    assert_eq!(sim.modifiers(1).gather_rate_pct[Resource::Gold.index()], 0);
    assert_eq!(
        sim.modifiers(1).farm_yield(250),
        500,
        "farms twice the food"
    );
    let sim = arena([Civ::Assyrians, Civ::Phoenicians]);
    assert_eq!(sim.modifiers(0).villager_speed_pct, 10);
    assert_eq!(sim.modifiers(1).gather_rate_pct[Resource::Wood.index()], 30);
}

/// Hit points: the Egyptians' chariots and the Babylonians' and Shang's
/// walls are stronger, and stay so through a repair.
#[test]
fn stronger_kinds_have_more_hit_points() {
    let mut sim = arena([Civ::Egyptians, Civ::Babylonians]);
    let chariot = put(&mut sim, 0, kinds::CHARIOT_ARCHER, 10, 10);
    let base = kinds::info(kinds::CHARIOT_ARCHER).max_health;
    assert_eq!(hp(&sim, chariot), Fx::from_int(base * 133 / 100));
    let tower = put(&mut sim, 1, kinds::WATCH_TOWER, 40, 40);
    let base = kinds::info(kinds::WATCH_TOWER).max_health;
    assert_eq!(hp(&sim, tower), Fx::from_int(base * 160 / 100));
    let mut sim = arena([Civ::Shang, Civ::Greeks]);
    let wall = put(&mut sim, 0, kinds::STONE_WALL, 10, 10);
    let base = kinds::info(kinds::STONE_WALL).max_health;
    assert_eq!(hp(&sim, wall), Fx::from_int(base * 2));
    let theirs = put(&mut sim, 1, kinds::STONE_WALL, 40, 40);
    assert_eq!(hp(&sim, theirs), Fx::from_int(base));
}

/// Costs: Shang villagers are cheaper, and that is what is charged.
#[test]
fn a_cheaper_kind_is_charged_less() {
    let mut sim = arena([Civ::Shang, Civ::Phoenicians]);
    assert_eq!(sim.cost_of(0, kinds::VILLAGER), [35, 0, 0, 0]);
    assert_eq!(sim.cost_of(1, kinds::VILLAGER), [50, 0, 0, 0]);
    let elephant = kinds::info(kinds::WAR_ELEPHANT).cost;
    assert_eq!(
        sim.cost_of(1, kinds::WAR_ELEPHANT),
        elephant.map(|c| c * 75 / 100)
    );
    let tc = put(&mut sim, 0, kinds::TOWN_CENTER, 10, 10);
    let food = sim.player(0).unwrap().stockpile[0];
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Train {
            building: tc,
            kind: kinds::VILLAGER,
        },
    });
    run(&mut sim, 3);
    assert_eq!(sim.player(0).unwrap().stockpile[0], food - 35);
    // And refunded the same when the order is taken back.
    sim.issue(Command {
        player: 0,
        kind: CommandKind::CancelTrain { building: tc },
    });
    run(&mut sim, 3);
    assert_eq!(sim.player(0).unwrap().stockpile[0], food);
}

/// Speed: Greek hoplites and Persian elephants are faster.
#[test]
fn faster_kinds_walk_faster() {
    let sim = arena([Civ::Greeks, Civ::Persians]);
    let base = kinds::info(kinds::HOPLITE).speed_per_second;
    assert_eq!(
        sim.civ_speed(0, kinds::HOPLITE),
        base.mul_div(Fx::from_int(125), Fx::from_int(100))
    );
    assert_eq!(sim.civ_speed(1, kinds::HOPLITE), base);
    let base = kinds::info(kinds::WAR_ELEPHANT).speed_per_second;
    assert_eq!(
        sim.civ_speed(1, kinds::WAR_ELEPHANT),
        base.mul_div(Fx::from_int(150), Fx::from_int(100))
    );
}

/// The Greeks have the Legion in the Bronze Age; others wait for the Iron.
#[test]
fn the_greeks_have_the_legion_early() {
    let sim = arena([Civ::Greeks, Civ::Persians]);
    assert_eq!(sim.tech_age(0, tech::LEGION), sim::Age::Bronze);
    assert_eq!(sim.tech_age(1, tech::LEGION), sim::Age::Iron);
}

/// Rate of fire: an Assyrian bowman waits less between arrows.
#[test]
fn faster_archers_wait_less_between_shots() {
    let mut sim = arena([Civ::Assyrians, Civ::Greeks]);
    let target = put(&mut sim, 1, kinds::HOPLITE, 14, 10);
    sim.issue(Command {
        player: 1,
        kind: CommandKind::SetStance {
            ids: vec![target],
            stance: Stance::Passive,
        },
    });
    let bow = put(&mut sim, 0, kinds::BOWMAN, 10, 10);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![bow],
            target,
        },
    });
    let base = kinds::info(kinds::BOWMAN).combat.reload_ticks;
    let mut most = 0u32;
    for _ in 0..200 {
        sim.step();
        most = most.max(u32::from(sim.world().reload[index_of(&sim, bow)]));
    }
    assert_eq!(most, base * 100 / 120);
}

/// An Egyptian priest converts from two tiles further.
#[test]
fn an_egyptian_priest_reaches_further() {
    let mut sim = arena([Civ::Egyptians, Civ::Greeks]);
    let foe = put(&mut sim, 1, kinds::CLUBMAN, 19, 10);
    sim.issue(Command {
        player: 1,
        kind: CommandKind::SetStance {
            ids: vec![foe],
            stance: Stance::Passive,
        },
    });
    run(&mut sim, 3);
    // Nine tiles off: beyond any other priest's reach, inside this one's.
    let priest = put(&mut sim, 0, kinds::PRIEST, 10, 10);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![priest],
            target: foe,
        },
    });
    run(&mut sim, 5);
    let i = index_of(&sim, priest);
    assert!(
        matches!(sim.world().order[i], sim::Order::Convert { chant, .. } if chant > 0),
        "{:?}",
        sim.world().order[i]
    );
    assert_eq!(
        pos_of(&sim, priest),
        sim::nav::centre((10, 10)),
        "did not walk"
    );
}

/// A match with civilizations replays to the same hash.
#[test]
fn a_match_with_civilizations_replays() {
    let play = || {
        let mut sim = arena([Civ::Sumerians, Civ::Shang]);
        let tc = put(&mut sim, 0, kinds::TOWN_CENTER, 10, 10);
        for _ in 0..3 {
            sim.issue(Command {
                player: 0,
                kind: CommandKind::Train {
                    building: tc,
                    kind: kinds::VILLAGER,
                },
            });
        }
        run(&mut sim, 400);
        sim.state_hash()
    };
    assert_eq!(play(), play());
    let mut sim = arena([Civ::Sumerians, Civ::Shang]);
    run(&mut sim, 10);
    sim.replay().verify().expect("replays");
}

/// A unit converted to a side whose kind has fewer hit points keeps no
/// more than that side's full health.
#[test]
fn a_converted_unit_has_no_more_than_its_new_sides_health() {
    let mut sim = arena([Civ::Egyptians, Civ::Greeks]);
    let chariot = put(&mut sim, 0, kinds::CHARIOT_ARCHER, 14, 10);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::SetStance {
            ids: vec![chariot],
            stance: Stance::Passive,
        },
    });
    run(&mut sim, 3);
    let base = kinds::info(kinds::CHARIOT_ARCHER).max_health;
    assert_eq!(hp(&sim, chariot), Fx::from_int(base * 133 / 100));
    let priest = put(&mut sim, 1, kinds::PRIEST, 10, 10);
    sim.issue(Command {
        player: 1,
        kind: CommandKind::Attack {
            ids: vec![priest],
            target: chariot,
        },
    });
    for _ in 0..400 {
        sim.step();
        if sim.world().owner[index_of(&sim, chariot)] == 1 {
            break;
        }
    }
    assert_eq!(sim.world().owner[index_of(&sim, chariot)], 1);
    assert_eq!(hp(&sim, chariot), Fx::from_int(base));
    sim.check().unwrap();
}

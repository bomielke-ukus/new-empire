//! The Bronze and Iron Ages' soldiers (`docs/02` §5.2-§5.4): where each is
//! trained and from when, the two line upgrades, the two-population
//! engines and elephants, and the siege engines' stones, which land where
//! they were aimed and hurt whatever they land among (`GD-COMBAT-04`).

mod common;

use common::*;
use sim::{
    kinds, tech, Command, CommandKind, Event, Fx, SimConfig, Simulation, Stance, TrainError, Vec2Fx,
};

fn rich(seed: u64) -> Simulation {
    Simulation::new(
        seed,
        SimConfig {
            starting_stockpile: [20_000; 4],
            ..SimConfig::default()
        },
    )
}

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

fn at(x: i32, y: i32) -> Vec2Fx {
    sim::nav::centre((x, y))
}

fn train(sim: &mut Simulation, building: sim::EntityId, kind: sim::KindId) {
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Train { building, kind },
    });
}

fn research(sim: &mut Simulation, building: sim::EntityId, tech: sim::TechId) {
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Research { building, tech },
    });
}

/// Every military building beside the Town Center, two of each age's to
/// advance on, and houses for the army. Returns the Barracks, the Range,
/// the Stable, the Academy and the Siege Workshop.
fn military(sim: &mut Simulation) -> [sim::EntityId; 5] {
    let tc = owned(sim, 0, kinds::TOWN_CENTER)[0];
    let c = pos_of(sim, tc);
    let place = |dx: i32, dy: i32| Vec2Fx::new(c.x + Fx::from_int(dx), c.y + Fx::from_int(dy));
    for (kind, d) in [
        (kinds::BARRACKS, (6, 0)),
        (kinds::STOREHOUSE, (-6, 4)),
        (kinds::ARCHERY_RANGE, (6, 4)),
        (kinds::STABLE, (-6, 0)),
        (kinds::ACADEMY, (6, -4)),
        (kinds::SIEGE_WORKSHOP, (-6, -4)),
        (kinds::HOUSE, (0, 6)),
        (kinds::HOUSE, (3, 6)),
        (kinds::HOUSE, (-3, 6)),
        (kinds::HOUSE, (0, -6)),
        (kinds::HOUSE, (3, -6)),
    ] {
        sim.issue(spawn(0, kind, place(d.0, d.1)));
    }
    run(sim, 3);
    [
        kinds::BARRACKS,
        kinds::ARCHERY_RANGE,
        kinds::STABLE,
        kinds::ACADEMY,
        kinds::SIEGE_WORKSHOP,
    ]
    .map(|k| owned(sim, 0, k)[0])
}

fn advance(sim: &mut Simulation, age: tech::TechId, to: tech::Age) {
    let tc = owned(sim, 0, kinds::TOWN_CENTER)[0];
    research(sim, tc, age);
    run(sim, tech::info(age).unwrap().ticks() + 5);
    assert_eq!(sim.player(0).unwrap().age, to);
}

/// REQ: GD-AGE-01
#[test]
fn the_later_ages_train_their_soldiers_where_they_belong() {
    let mut sim = rich(31);
    let [barracks, range, stable, academy, workshop] = military(&mut sim);
    assert_eq!(
        sim.roster(0, kinds::BARRACKS),
        vec![
            kinds::CLUBMAN,
            kinds::AXEMAN,
            kinds::SPEARMAN,
            kinds::SWORDSMAN
        ]
    );
    assert_eq!(
        sim.roster(0, kinds::ARCHERY_RANGE),
        vec![
            kinds::SLINGER,
            kinds::BOWMAN,
            kinds::CHARIOT_ARCHER,
            kinds::HORSE_ARCHER
        ]
    );
    assert_eq!(
        sim.roster(0, kinds::STABLE),
        vec![
            kinds::SCOUT,
            kinds::LIGHT_CAVALRY,
            kinds::HEAVY_CAVALRY,
            kinds::WAR_ELEPHANT
        ]
    );
    assert_eq!(
        sim.roster(0, kinds::ACADEMY),
        vec![kinds::HOPLITE, kinds::LEGIONARY]
    );
    assert_eq!(
        sim.roster(0, kinds::SIEGE_WORKSHOP),
        vec![kinds::STONE_THROWER, kinds::CATAPULT, kinds::BALLISTA]
    );
    let bronze = [
        (barracks, kinds::SWORDSMAN),
        (range, kinds::CHARIOT_ARCHER),
        (stable, kinds::HEAVY_CAVALRY),
        (academy, kinds::HOPLITE),
        (workshop, kinds::STONE_THROWER),
    ];
    let iron = [
        (range, kinds::HORSE_ARCHER),
        (stable, kinds::WAR_ELEPHANT),
        (workshop, kinds::BALLISTA),
    ];
    for (b, k) in bronze {
        assert_eq!(
            sim.can_train(0, b, k),
            Err(TrainError::AgeLocked {
                needs: tech::Age::Bronze
            }),
            "{}",
            kinds::info(k).name
        );
    }
    advance(&mut sim, tech::AGE_TOOL, tech::Age::Tool);
    advance(&mut sim, tech::AGE_BRONZE, tech::Age::Bronze);
    for (b, k) in bronze {
        assert_eq!(sim.can_train(0, b, k), Ok(()), "{}", kinds::info(k).name);
    }
    for (b, k) in iron {
        assert_eq!(
            sim.can_train(0, b, k),
            Err(TrainError::AgeLocked {
                needs: tech::Age::Iron
            }),
            "{}",
            kinds::info(k).name
        );
    }
    advance(&mut sim, tech::AGE_IRON, tech::Age::Iron);
    for (b, k) in bronze.into_iter().chain(iron) {
        assert_eq!(sim.can_train(0, b, k), Ok(()), "{}", kinds::info(k).name);
        train(&mut sim, b, k);
    }
    let pop = sim.player(0).unwrap().pop;
    // The Range and the Workshop queue two each; give them time for both.
    let slowest = [
        kinds::CHARIOT_ARCHER,
        kinds::HORSE_ARCHER,
        kinds::STONE_THROWER,
        kinds::BALLISTA,
    ]
    .map(|k| kinds::info(k).build_ticks())
    .iter()
    .sum::<u32>();
    run(&mut sim, slowest + 10);
    for (_, k) in bronze.into_iter().chain(iron) {
        assert_eq!(owned(&sim, 0, k).len(), 1, "{}", kinds::info(k).name);
    }
    // Siege and the elephant take two each (`GD-POP-03`).
    let two = [kinds::STONE_THROWER, kinds::BALLISTA, kinds::WAR_ELEPHANT];
    let expected: u32 = bronze
        .into_iter()
        .chain(iron)
        .map(|(_, k)| if two.contains(&k) { 2 } else { 1 })
        .sum();
    assert_eq!(sim.player(0).unwrap().pop, pop + expected);
}

/// The Legion upgrade turns every Hoplite into a Legionary, and Torsion
/// every Stone Thrower into a Catapult.
#[test]
fn legion_and_torsion_move_their_lines_on() {
    let mut sim = rich(32);
    let [_, _, _, academy, workshop] = military(&mut sim);
    advance(&mut sim, tech::AGE_TOOL, tech::Age::Tool);
    advance(&mut sim, tech::AGE_BRONZE, tech::Age::Bronze);
    train(&mut sim, academy, kinds::HOPLITE);
    train(&mut sim, workshop, kinds::STONE_THROWER);
    run(
        &mut sim,
        kinds::info(kinds::STONE_THROWER).build_ticks() + 5,
    );
    assert_eq!(owned(&sim, 0, kinds::HOPLITE).len(), 1);
    assert_eq!(owned(&sim, 0, kinds::STONE_THROWER).len(), 1);
    assert_eq!(
        sim.can_train(0, academy, kinds::LEGIONARY),
        Err(TrainError::AgeLocked {
            needs: tech::Age::Iron
        })
    );
    advance(&mut sim, tech::AGE_IRON, tech::Age::Iron);
    assert_eq!(
        sim.can_train(0, academy, kinds::LEGIONARY),
        Err(TrainError::NeedsTech { tech: tech::LEGION })
    );
    research(&mut sim, academy, tech::LEGION);
    research(&mut sim, workshop, tech::TORSION);
    run(&mut sim, tech::info(tech::LEGION).unwrap().ticks() + 5);
    assert!(owned(&sim, 0, kinds::HOPLITE).is_empty());
    assert!(owned(&sim, 0, kinds::STONE_THROWER).is_empty());
    let legionary = owned(&sim, 0, kinds::LEGIONARY);
    assert_eq!(legionary.len(), 1);
    assert_eq!(
        sim.world().health[index_of(&sim, legionary[0])],
        Fx::from_int(160)
    );
    assert_eq!(owned(&sim, 0, kinds::CATAPULT).len(), 1);
    assert_eq!(
        sim.roster(0, kinds::ACADEMY),
        vec![kinds::LEGIONARY],
        "the Academy trains Legionaries now"
    );
    assert_eq!(
        sim.can_train(0, workshop, kinds::STONE_THROWER),
        Err(TrainError::Superseded {
            by: kinds::CATAPULT
        })
    );
}

/// Where the first stone lands, and what each of `watch` lost on the tick
/// it came down.
fn first_landing(sim: &mut Simulation, watch: &[sim::EntityId], limit: u32) -> (Vec2Fx, Vec<Fx>) {
    let health = |sim: &Simulation| -> Vec<Fx> {
        watch
            .iter()
            .map(|&id| sim.world().health[index_of(sim, id)])
            .collect()
    };
    for _ in 0..limit {
        let before = health(sim);
        sim.step();
        if let Some(pos) = sim.events().iter().find_map(|e| match e {
            Event::Landed { pos, .. } => Some(*pos),
            _ => None,
        }) {
            let lost = before
                .iter()
                .zip(health(sim))
                .map(|(b, a)| *b - a)
                .collect();
            return (pos, lost);
        }
    }
    panic!("no stone landed in {limit} ticks");
}

/// REQ: GD-COMBAT-04
#[test]
fn a_stone_hurts_everything_it_lands_among_friends_included_but_not_its_thrower() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::STONE_THROWER, at(10, 20)));
    sim.issue(spawn(1, kinds::SWORDSMAN, at(17, 20)));
    run(&mut sim, 3);
    // One of the thrower's own stands at the enemy's shoulder.
    let shoulder = at(17, 20) + Vec2Fx::new(Fx::from_ratio(3, 10), Fx::ZERO);
    sim.issue(spawn(0, kinds::SWORDSMAN, shoulder));
    // And one well clear of the blast.
    sim.issue(spawn(0, kinds::SWORDSMAN, at(17, 23)));
    run(&mut sim, 3);
    let thrower = owned(&sim, 0, kinds::STONE_THROWER)[0];
    let enemy = owned(&sim, 1, kinds::SWORDSMAN)[0];
    let mine = owned(&sim, 0, kinds::SWORDSMAN);
    let (friend, clear) = if pos_of(&sim, mine[0]).y < pos_of(&sim, mine[1]).y {
        (mine[0], mine[1])
    } else {
        (mine[1], mine[0])
    };
    let everyone = vec![thrower, enemy, friend, clear];
    // Passive, so the two swordsmen side by side do not trade blows before
    // the stone comes down: it is the only thing that touches them.
    for (p, ids, stance) in [
        (0, vec![thrower], Stance::StandGround),
        (0, vec![friend, clear], Stance::Passive),
        (1, vec![enemy], Stance::Passive),
    ] {
        sim.issue(Command {
            player: p,
            kind: CommandKind::SetStance { ids, stance },
        });
    }
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![thrower],
            target: enemy,
        },
    });
    let (landed, lost) = first_landing(&mut sim, &everyone, 400);
    assert!(
        landed.distance(at(17, 20)) < Fx::from_ratio(1, 10),
        "it lands where the target stood: {landed:?}"
    );
    let stone = Fx::from_int(40);
    assert_eq!(lost[1], stone, "the target takes the stone, armour or not");
    assert_eq!(lost[2], stone, "and so does the friend beside it");
    assert_eq!(lost[3], Fx::ZERO, "one clear of the blast is not hit");
    assert_eq!(lost[0], Fx::ZERO, "the thrower is never hit");
}

/// A stone flies to where its target stood: a target that walks on is not
/// there when it lands.
#[test]
fn a_target_that_moves_on_is_missed() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::STONE_THROWER, at(10, 20)));
    sim.issue(spawn(1, kinds::HEAVY_CAVALRY, at(17, 20)));
    run(&mut sim, 3);
    let thrower = owned(&sim, 0, kinds::STONE_THROWER)[0];
    let rider = owned(&sim, 1, kinds::HEAVY_CAVALRY)[0];
    sim.issue(Command {
        player: 1,
        kind: CommandKind::SetStance {
            ids: vec![rider],
            stance: Stance::StandGround,
        },
    });
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![thrower],
            target: rider,
        },
    });
    // The rider rides off across the throw the moment the stone is loosed.
    for _ in 0..400 {
        sim.step();
        if !sim.projectiles().is_empty() {
            break;
        }
    }
    assert!(!sim.projectiles().is_empty(), "a stone is in the air");
    sim.issue(move_to(1, vec![rider], at(17, 40)));
    let (landed, lost) = first_landing(&mut sim, &[rider], 200);
    assert!(landed.distance(at(17, 20)) < Fx::from_ratio(1, 10));
    assert_eq!(lost[0], Fx::ZERO, "the rider is gone from there");
}

/// A building is caught when the blast reaches its footprint.
#[test]
fn a_stone_aimed_at_a_building_hits_it() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::STONE_THROWER, at(10, 20)));
    sim.issue(spawn(1, kinds::HOUSE, at(18, 20)));
    run(&mut sim, 3);
    let thrower = owned(&sim, 0, kinds::STONE_THROWER)[0];
    let house = owned(&sim, 1, kinds::HOUSE)[0];
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![thrower],
            target: house,
        },
    });
    let (_, lost) = first_landing(&mut sim, &[house], 400);
    assert_eq!(lost[0], Fx::from_int(40));
}

/// A Catapult's blast reaches twice as far as a Stone Thrower's.
#[test]
fn the_catapult_throws_wider() {
    let thrower = kinds::info(kinds::STONE_THROWER);
    let catapult = kinds::info(kinds::CATAPULT);
    assert_eq!(thrower.blast(), Fx::from_ratio(1, 2));
    assert_eq!(catapult.blast(), Fx::ONE);
    assert!(catapult.combat.attack > thrower.combat.attack);
    // The ballista's bolt is an arrow: it follows its target, no blast.
    assert!(kinds::info(kinds::BALLISTA).blast().is_zero());
    // Every stone does the damage settled against its target to everything
    // it lands among, which is exact only while siege has no class bonus.
    for k in kinds::all() {
        if !k.blast().is_zero() {
            assert!(k.combat.bonuses.is_empty(), "{}", k.name);
            assert_eq!(k.combat.damage, kinds::DamageType::Siege, "{}", k.name);
        }
    }
}

//! Relics and the two victories on a clock (`docs/02` §10, `docs/07`
//! D31): relics on the map, a priest carrying one home, the gold it earns
//! there, what happens to it when its priest or its Temple falls, and the
//! Wonder and the relics each held ten minutes.

mod common;

use common::*;
use sim::kinds::Resource;
use sim::relics::{RELICS_PER_MAP, RELIC_GOLD_TICKS};
use sim::victory::HOLD_TICKS;
use sim::{
    kinds, Command, CommandKind, EntityId, Fx, Order, SimConfig, Simulation, Stance, Victory,
};

/// A flat, empty map for two sides, each with a Town Center and a
/// villager so both stay in the match.
fn arena() -> Simulation {
    let mut sim = Simulation::new(
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
    );
    let fp = kinds::info(kinds::TOWN_CENTER).footprint as i32;
    for (p, (x, y)) in [(0u8, (6, 6)), (1u8, (56, 56))] {
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

fn at(x: i32, y: i32) -> sim::Vec2Fx {
    sim::nav::centre((x, y))
}

/// Spawns one `kind` for `player` and returns it.
fn put(sim: &mut Simulation, player: u8, kind: sim::KindId, x: i32, y: i32) -> EntityId {
    let before: Vec<EntityId> = sim.world().slots().map(|s| sim.world().id_at(s)).collect();
    if kinds::info(kind).footprint > 1 {
        let fp = kinds::info(kind).footprint as i32;
        sim.issue(spawn(player, kind, sim::nav::building_centre(x, y, fp)));
    } else {
        sim.issue(spawn(player, kind, at(x, y)));
    }
    run(sim, 3);
    let w = sim.world();
    w.slots()
        .map(|s| w.id_at(s))
        .find(|id| !before.contains(id) && w.kind[id.index()] == kind)
        .expect("spawned")
}

fn fetch(sim: &mut Simulation, priest: EntityId, target: EntityId) {
    sim.issue(Command {
        player: sim.world().owner[index_of(sim, priest)],
        kind: CommandKind::Relic {
            ids: vec![priest],
            target,
        },
    });
}

fn inside(sim: &Simulation, id: EntityId) -> Option<EntityId> {
    sim.world().inside[index_of(sim, id)]
}

fn owner(sim: &Simulation, id: EntityId) -> u8 {
    sim.world().owner[index_of(sim, id)]
}

fn gold(sim: &Simulation, p: u8) -> i32 {
    sim.player(p).unwrap().stockpile[Resource::Gold.index()]
}

/// Runs until `done` holds, at most `limit` ticks.
fn until(sim: &mut Simulation, limit: u32, done: impl Fn(&Simulation) -> bool) -> bool {
    for _ in 0..limit {
        if done(sim) {
            return true;
        }
        sim.step();
    }
    done(sim)
}

/// A generated map has its relics in the open, nature's, away from every
/// start and apart, each standing on its tile.
///
/// REQ: GD-WIN-03
#[test]
fn a_generated_map_has_its_relics_in_the_open() {
    for seed in [1, 2, 3] {
        let sim = inland(seed);
        let w = sim.world();
        let relics: Vec<usize> = w
            .slots()
            .map(|s| s.index())
            .filter(|&i| w.kind[i] == kinds::RELIC)
            .collect();
        assert_eq!(relics.len(), RELICS_PER_MAP, "seed {seed}");
        let tcs: Vec<sim::Vec2Fx> = w
            .slots()
            .filter(|s| w.kind[s.index()] == kinds::TOWN_CENTER)
            .map(|s| w.pos[s.index()])
            .collect();
        for &r in &relics {
            assert_eq!(w.owner[r], kinds::GAIA);
            assert!(w.inside[r].is_none());
            let p = w.pos[r];
            assert!(!sim.nav().passable(p.x.floor(), p.y.floor()));
            for tc in &tcs {
                assert!(
                    p.distance(*tc) > Fx::from_int(12),
                    "seed {seed}: by a start"
                );
            }
            for &o in &relics {
                if o != r {
                    assert!(p.distance(w.pos[o]) >= Fx::from_int(10), "seed {seed}");
                }
            }
        }
    }
}

/// A relic is carried home: the priest walks to it and takes it up, its
/// tile opens, and it goes into the nearest Temple of the priest's side's.
///
/// REQ: GD-WIN-03
#[test]
fn a_priest_carries_a_relic_into_its_temple() {
    let mut sim = arena();
    let relic = put(&mut sim, 0, kinds::RELIC, 30, 20);
    assert_eq!(owner(&sim, relic), kinds::GAIA, "nature's on the ground");
    assert!(!sim.nav().passable(30, 20), "it stands on its tile");
    let temple = put(&mut sim, 0, kinds::TEMPLE, 16, 20);
    let priest = put(&mut sim, 0, kinds::PRIEST, 22, 20);
    fetch(&mut sim, priest, relic);
    assert!(
        until(&mut sim, 600, |s| inside(s, relic) == Some(priest)),
        "taken up"
    );
    assert_eq!(owner(&sim, relic), 0);
    assert!(sim.nav().passable(30, 20), "its tile is open again");
    assert!(
        until(&mut sim, 900, |s| inside(s, relic) == Some(temple)),
        "carried home"
    );
    assert_eq!(sim.relics_held(0), 1);
    let pi = index_of(&sim, priest);
    run(&mut sim, 2);
    assert_eq!(sim.world().order[pi], Order::Idle);
    assert_eq!(sim.carried_relic(priest), None);
}

/// A relic held in a Temple earns its side a gold every two seconds.
///
/// REQ: GD-WIN-03
#[test]
fn a_held_relic_earns_gold() {
    let mut sim = arena();
    let relic = put(&mut sim, 0, kinds::RELIC, 20, 20);
    let temple = put(&mut sim, 0, kinds::TEMPLE, 16, 20);
    let priest = put(&mut sim, 0, kinds::PRIEST, 18, 22);
    fetch(&mut sim, priest, relic);
    assert!(until(&mut sim, 900, |s| inside(s, relic) == Some(temple)));
    let before = gold(&sim, 0);
    let ticks = 20 * RELIC_GOLD_TICKS as u32;
    run(&mut sim, ticks);
    let earned = gold(&sim, 0) - before;
    assert!((19..=21).contains(&earned), "earned {earned}");
    // Not the other side's, and nothing for a relic lying about.
    assert_eq!(sim.relics_held(1), 0);
}

/// With no Temple to take it to, the priest stands holding it; sent to
/// a Temple built later, it takes it there.
///
/// REQ: GD-WIN-03
#[test]
fn without_a_temple_the_priest_holds_the_relic() {
    let mut sim = arena();
    let relic = put(&mut sim, 0, kinds::RELIC, 24, 20);
    let priest = put(&mut sim, 0, kinds::PRIEST, 20, 20);
    fetch(&mut sim, priest, relic);
    assert!(until(&mut sim, 600, |s| inside(s, relic) == Some(priest)));
    run(&mut sim, 20);
    assert_eq!(sim.world().order[index_of(&sim, priest)], Order::Idle);
    assert_eq!(inside(&sim, relic), Some(priest));
    let temple = put(&mut sim, 0, kinds::TEMPLE, 30, 26);
    fetch(&mut sim, priest, temple);
    assert!(until(&mut sim, 900, |s| inside(s, relic) == Some(temple)));
}

/// A priest that falls drops its relic where it fell, nature's again and
/// standing on a tile of its own; another side's priest can take it.
///
/// REQ: GD-WIN-03
#[test]
fn a_fallen_priest_drops_its_relic() {
    let mut sim = arena();
    let relic = put(&mut sim, 0, kinds::RELIC, 24, 20);
    let priest = put(&mut sim, 0, kinds::PRIEST, 20, 20);
    fetch(&mut sim, priest, relic);
    assert!(until(&mut sim, 600, |s| inside(s, relic) == Some(priest)));
    run(&mut sim, 5);
    // A swordsman of the other side's, sent at it.
    let sword = put(&mut sim, 1, kinds::SWORDSMAN, 26, 22);
    sim.issue(Command {
        player: 1,
        kind: CommandKind::Attack {
            ids: vec![sword],
            target: priest,
        },
    });
    assert!(
        until(&mut sim, 600, |s| inside(s, relic).is_none()),
        "dropped"
    );
    assert_eq!(owner(&sim, relic), kinds::GAIA);
    let p = pos_of(&sim, relic);
    assert!(
        !sim.nav().passable(p.x.floor(), p.y.floor()),
        "blocks its tile"
    );
    sim.check().unwrap();
    // The other side's to take now.
    let theirs = put(&mut sim, 1, kinds::PRIEST, p.x.floor() + 3, p.y.floor());
    fetch(&mut sim, theirs, relic);
    assert!(until(&mut sim, 600, |s| inside(s, relic) == Some(theirs)));
    assert_eq!(owner(&sim, relic), 1);
}

/// A Temple that falls drops every relic it held round its rubble.
///
/// REQ: GD-WIN-03
#[test]
fn a_fallen_temple_drops_its_relics() {
    let mut sim = arena();
    let a = put(&mut sim, 0, kinds::RELIC, 22, 18);
    let b = put(&mut sim, 0, kinds::RELIC, 22, 24);
    let temple = put(&mut sim, 0, kinds::TEMPLE, 16, 20);
    let priest = put(&mut sim, 0, kinds::PRIEST, 19, 21);
    for r in [a, b] {
        fetch(&mut sim, priest, r);
        assert!(until(&mut sim, 900, |s| inside(s, r) == Some(temple)));
    }
    assert_eq!(sim.relics_held(0), 2);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Despawn { id: temple },
    });
    run(&mut sim, 3);
    for r in [a, b] {
        assert_eq!(inside(&sim, r), None);
        assert_eq!(owner(&sim, r), kinds::GAIA);
    }
    assert_ne!(pos_of(&sim, a), pos_of(&sim, b), "a tile each");
    sim.check().unwrap();
}

/// A priest converted with a relic in hand brings it over.
///
/// REQ: GD-WIN-03
#[test]
fn a_converted_priest_brings_its_relic() {
    let mut sim = arena();
    let relic = put(&mut sim, 0, kinds::RELIC, 24, 20);
    let carrier = put(&mut sim, 1, kinds::PRIEST, 22, 20);
    sim.issue(Command {
        player: 1,
        kind: CommandKind::SetStance {
            ids: vec![carrier],
            stance: Stance::Passive,
        },
    });
    fetch(&mut sim, carrier, relic);
    assert!(until(&mut sim, 600, |s| inside(s, relic) == Some(carrier)));
    let ours = put(&mut sim, 0, kinds::PRIEST, 17, 20);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![ours],
            target: carrier,
        },
    });
    assert!(until(&mut sim, 600, |s| owner(s, carrier) == 0));
    assert_eq!(owner(&sim, relic), 0);
    assert_eq!(inside(&sim, relic), Some(carrier));
    sim.check().unwrap();
}

/// Every relic on the map in one side's Temples for ten minutes wins; the
/// clock stops the moment one is lost.
///
/// REQ: GD-WIN-03
#[test]
fn every_relic_held_ten_minutes_wins() {
    let mut sim = arena();
    let a = put(&mut sim, 0, kinds::RELIC, 22, 18);
    let b = put(&mut sim, 0, kinds::RELIC, 22, 24);
    let temple = put(&mut sim, 0, kinds::TEMPLE, 16, 20);
    let priest = put(&mut sim, 0, kinds::PRIEST, 19, 21);
    fetch(&mut sim, priest, a);
    assert!(until(&mut sim, 900, |s| inside(s, a) == Some(temple)));
    run(&mut sim, 40);
    assert_eq!(sim.relic_clock(), None, "one of two is not all");
    fetch(&mut sim, priest, b);
    assert!(until(&mut sim, 900, |s| inside(s, b) == Some(temple)));
    run(&mut sim, 40);
    let (holder, left) = sim.relic_clock().expect("the clock runs");
    assert_eq!(holder, 0);
    assert!(left <= HOLD_TICKS && left > HOLD_TICKS - 60, "{left}");
    // Half way, lose the Temple: the clock stops, and the relics lie about.
    run(&mut sim, (HOLD_TICKS / 2) as u32);
    assert_eq!(sim.winner(), None);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Despawn { id: temple },
    });
    run(&mut sim, 40);
    assert_eq!(sim.relic_clock(), None);
    // A new Temple and the relics back: the clock starts over.
    let temple = put(&mut sim, 0, kinds::TEMPLE, 30, 30);
    for r in [a, b] {
        fetch(&mut sim, priest, r);
        assert!(until(&mut sim, 1200, |s| inside(s, r) == Some(temple)));
    }
    run(&mut sim, 40);
    let (_, left) = sim.relic_clock().expect("running again");
    assert!(left > HOLD_TICKS - 60, "started over: {left}");
    run(&mut sim, left as u32 + 40);
    assert_eq!(sim.victory(), Some((0, Victory::Relics)));
    assert_eq!(sim.winner(), Some(0));
    assert!(sim.over());
    assert!(sim.standing(1), "won on the clock, not by conquest");
}

/// A Wonder held ten minutes wins; one brought down stops its clock. It
/// is an Iron Age building.
///
/// REQ: GD-WIN-02
#[test]
fn a_wonder_held_ten_minutes_wins() {
    let mut sim = arena();
    assert!(matches!(
        sim.can_build(0, kinds::WONDER),
        Err(sim::PlaceError::AgeLocked { .. })
    ));
    let theirs = put(&mut sim, 1, kinds::WONDER, 40, 40);
    run(&mut sim, 40);
    let clocks = sim.wonder_clocks();
    assert_eq!(clocks.len(), 1);
    assert_eq!((clocks[0].0, clocks[0].1), (theirs, 1));
    // Brought down half way: no clock, no winner.
    run(&mut sim, (HOLD_TICKS / 2) as u32);
    sim.issue(Command {
        player: 1,
        kind: CommandKind::Despawn { id: theirs },
    });
    run(&mut sim, 40);
    assert!(sim.wonder_clocks().is_empty());
    let ours = put(&mut sim, 0, kinds::WONDER, 30, 30);
    run(&mut sim, 40);
    let left = sim.wonder_clocks()[0].2;
    assert!(left > HOLD_TICKS - 60, "{left}");
    run(&mut sim, left as u32 - 40);
    assert_eq!(sim.winner(), None, "not quite");
    run(&mut sim, 80);
    assert_eq!(sim.victory(), Some((0, Victory::Wonder)));
    assert!(sim.over());
    assert_eq!(sim.wonder_clocks()[0].0, ours);
}

/// A Wonder still going up has no clock.
///
/// REQ: GD-WIN-02
#[test]
fn a_wonder_site_has_no_clock() {
    let mut sim = Simulation::new(
        3,
        SimConfig {
            starting_stockpile: [20_000; 4],
            ..SimConfig::default()
        },
    );
    let tc = owned(&sim, 0, kinds::TOWN_CENTER)[0];
    for age in [
        sim::tech::AGE_TOOL,
        sim::tech::AGE_BRONZE,
        sim::tech::AGE_IRON,
    ] {
        // The ages' buildings, spawned finished beside the Town Center.
        let c = pos_of(&sim, tc);
        for (k, d) in [
            (kinds::STOREHOUSE, (-6, 4)),
            (kinds::BARRACKS, (6, 0)),
            (kinds::ARCHERY_RANGE, (6, 4)),
            (kinds::STABLE, (-6, 0)),
            (kinds::MARKET, (-6, -4)),
            (kinds::TEMPLE, (6, -4)),
            (kinds::ACADEMY, (0, 8)),
            (kinds::SIEGE_WORKSHOP, (0, -8)),
        ] {
            if owned(&sim, 0, k).is_empty() {
                sim.issue(spawn(
                    0,
                    k,
                    sim::Vec2Fx::new(c.x + Fx::from_int(d.0), c.y + Fx::from_int(d.1)),
                ));
            }
        }
        run(&mut sim, 3);
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Research {
                building: tc,
                tech: age,
            },
        });
        run(&mut sim, sim::tech::info(age).unwrap().ticks() + 5);
    }
    assert_eq!(sim.player(0).unwrap().age, sim::Age::Iron);
    assert_eq!(sim.can_build(0, kinds::WONDER), Ok(()));
    let villagers = owned(&sim, 0, kinds::VILLAGER);
    let c = pos_of(&sim, tc);
    // The first clear ground east of the Town Center.
    let (x, y) = (8..40)
        .flat_map(|dx| (-12..12).map(move |dy| (dx, dy)))
        .map(|(dx, dy)| (c.x.floor() + dx, c.y.floor() + dy))
        .find(|&(x, y)| sim.can_place(0, kinds::WONDER, x, y).is_ok())
        .expect("room for a Wonder");
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Build {
            ids: villagers,
            kind: kinds::WONDER,
            x,
            y,
        },
    });
    run(&mut sim, 400);
    assert_eq!(owned(&sim, 0, kinds::WONDER).len(), 1, "the site is down");
    assert!(sim.wonder_clocks().is_empty(), "a site has no clock");
}

//! Boats and the water (`docs/02` §5.1, §6; `docs/07` D33): the Dock at
//! the shore, the fishing boat it trains, fish, and boats keeping to the
//! water while everything else keeps to the land.
//!
//! REQ: GD-NAVAL-01

mod common;

use common::*;
use sim::kinds::Resource;
use sim::{
    kinds, Command, CommandKind, EntityId, MapKind, MapSpec, Order, PlaceError, SimConfig,
    Simulation,
};

/// A coastal map for two, rich enough to build and train anything.
fn coast() -> Simulation {
    let mut sim = Simulation::new(
        3,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Coastal,
                size: 96,
                players: 2,
            },
            starting_stockpile: [2000; 4],
            wander: false,
            ..SimConfig::default()
        },
    );
    run(&mut sim, 3);
    sim
}

fn wet(sim: &Simulation, t: (i32, i32)) -> bool {
    sim.map().terrain(t.0, t.1).is_water()
}

/// Where player 0 can put a Dock, nearest its start.
fn dock_site(sim: &Simulation) -> (i32, i32) {
    let (sx, sy) = sim.starts()[0];
    let n = sim.map().width();
    (0..n)
        .flat_map(|y| (0..n).map(move |x| (x, y)))
        .filter(|&(x, y)| sim.can_place(0, kinds::DOCK, x, y).is_ok())
        .min_by_key(|&(x, y)| ((x - sx).pow(2) + (y - sy).pow(2), x, y))
        .expect("somewhere on the coast takes a Dock")
}

/// Spawns one `kind` for `player` at tile `(x, y)` and returns it.
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
        .find(|id| !before.contains(id) && w.kind[w.slot(*id).unwrap().index()] == kind)
        .expect("spawned")
}

fn tile_of(sim: &Simulation, id: EntityId) -> (i32, i32) {
    sim::nav::tile_of(pos_of(sim, id))
}

/// A finished Dock of player 0's at the nearest site, and a fishing boat
/// on the water beside it.
fn dock_and_boat(sim: &mut Simulation) -> (EntityId, EntityId) {
    let (x, y) = dock_site(sim);
    let dock = put(sim, 0, kinds::DOCK, x, y);
    let water = sim
        .water_grid()
        .nearest_passable(x, y, 4, None)
        .expect("water by the Dock");
    let boat = put(sim, 0, kinds::FISHING_BOAT, water.0, water.1);
    (dock, boat)
}

/// A Dock stands in the water against the shore: not on land, and not out
/// in open water where nobody could build it.
#[test]
fn a_dock_goes_in_the_water_against_the_shore() {
    let sim = coast();
    let (x, y) = dock_site(&sim);
    let fp = kinds::info(kinds::DOCK).footprint as i32;
    for t in sim::nav::footprint_tiles(x, y, fp) {
        assert!(wet(&sim, t), "{t:?} is water");
    }
    let ring = sim::nav::footprint_ring(x, y, fp);
    assert!(
        ring.iter().any(|&t| sim.nav().passable(t.0, t.1)),
        "land beside it"
    );
    assert!(
        ring.iter().any(|&t| sim.water_grid().passable(t.0, t.1)),
        "water beside it"
    );
    // On land, where every other building goes, it does not.
    let (sx, sy) = sim.starts()[0];
    assert_eq!(
        sim.can_place(0, kinds::DOCK, sx + 6, sy + 6),
        Err(PlaceError::Blocked)
    );
    // Out in open water, with no shore to build it from, it does not.
    let n = sim.map().width();
    let open = (0..n)
        .flat_map(|y| (0..n).map(move |x| (x, y)))
        .find(|&(x, y)| {
            (-4..=4).all(|dy| (-4..=4).all(|dx| wet(&sim, (x + dx, y + dy))))
                && sim.water_grid().footprint_clear(x, y, fp)
        })
        .expect("open water on a coast");
    assert_eq!(
        sim.can_place(0, kinds::DOCK, open.0, open.1),
        Err(PlaceError::NeedsShore)
    );
}

/// A villager builds a Dock from the shore beside it.
#[test]
fn a_villager_builds_a_dock_from_the_shore() {
    let mut sim = coast();
    let (x, y) = dock_site(&sim);
    let villager = owned(&sim, 0, kinds::VILLAGER)[0];
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Build {
            kind: kinds::DOCK,
            x,
            y,
            ids: vec![villager],
        },
    });
    for _ in 0..6000 {
        sim.step();
        let done = owned(&sim, 0, kinds::DOCK)
            .first()
            .is_some_and(|&d| sim.world().construction[index_of(&sim, d)].is_none());
        if done {
            assert!(!wet(&sim, tile_of(&sim, villager)), "built from the land");
            return;
        }
    }
    panic!("the Dock was never finished");
}

/// A Dock trains a fishing boat, and it comes out onto the water.
#[test]
fn a_dock_trains_a_fishing_boat_onto_the_water() {
    let mut sim = coast();
    let (x, y) = dock_site(&sim);
    let dock = put(&mut sim, 0, kinds::DOCK, x, y);
    let wood = sim.player(0).unwrap().stockpile[Resource::Wood.index()];
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Train {
            building: dock,
            kind: kinds::FISHING_BOAT,
        },
    });
    run(&mut sim, 31 * 20);
    let boats = owned(&sim, 0, kinds::FISHING_BOAT);
    assert_eq!(boats.len(), 1, "trained");
    assert!(wet(&sim, tile_of(&sim, boats[0])), "on the water");
    assert_eq!(
        sim.player(0).unwrap().stockpile[Resource::Wood.index()],
        wood - kinds::info(kinds::FISHING_BOAT).cost[Resource::Wood.index()]
    );
}

/// A fishing boat fishes, sails its catch to the Dock, and the food is the
/// side's.
#[test]
fn a_fishing_boat_brings_its_catch_to_the_dock() {
    let mut sim = coast();
    let (dock, boat) = dock_and_boat(&mut sim);
    let fish = nearest_kind(&sim, kinds::FISH, pos_of(&sim, dock));
    let left = sim.world().resource[index_of(&sim, fish)];
    let food = sim.player(0).unwrap().stockpile[Resource::Food.index()];
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Gather {
            ids: vec![boat],
            node: fish,
        },
    });
    let mut on_water = true;
    for _ in 0..4000 {
        sim.step();
        on_water &= wet(&sim, tile_of(&sim, boat));
        if sim.player(0).unwrap().stockpile[Resource::Food.index()] > food {
            break;
        }
    }
    assert!(on_water, "the boat never left the water");
    let caught = sim.player(0).unwrap().stockpile[Resource::Food.index()] - food;
    assert_eq!(
        caught,
        kinds::carry_base(kinds::FISHING_BOAT),
        "a full hold"
    );
    assert!(sim.world().resource[index_of(&sim, fish)] < left);
    assert!(matches!(
        sim.world().order[index_of(&sim, boat)],
        Order::Gather { .. }
    ));
}

/// Fish are a boat's to work and the land's nodes a villager's.
#[test]
fn fish_are_a_boats_and_the_land_a_villagers() {
    let mut sim = coast();
    let (_, boat) = dock_and_boat(&mut sim);
    let villager = owned(&sim, 0, kinds::VILLAGER)[0];
    let fish = nearest_kind(&sim, kinds::FISH, pos_of(&sim, villager));
    let tree = nearest_kind(&sim, kinds::TREE, pos_of(&sim, boat));
    for (who, node) in [(villager, fish), (boat, tree)] {
        sim.issue(Command {
            player: 0,
            kind: CommandKind::Gather {
                ids: vec![who],
                node,
            },
        });
    }
    run(&mut sim, 2);
    for who in [villager, boat] {
        assert!(
            !matches!(sim.world().order[index_of(&sim, who)], Order::Gather { .. }),
            "refused"
        );
    }
}

/// Sent inland, a boat goes as near as the water takes it and stops there;
/// a villager sent out to sea stops at the shore. Neither ever stands on
/// the other's element.
#[test]
fn boats_keep_to_the_water_and_walkers_to_the_land() {
    let mut sim = coast();
    let (_, boat) = dock_and_boat(&mut sim);
    let villager = owned(&sim, 0, kinds::VILLAGER)[0];
    let (sx, sy) = sim.starts()[0];
    let sea = tile_of(&sim, boat);
    sim.issue(move_to(0, vec![boat], sim::nav::centre((sx, sy))));
    sim.issue(move_to(0, vec![villager], sim::nav::centre(sea)));
    // The coast lies some sixty tiles from the start: three minutes'
    // walk is ample.
    for _ in 0..3600 {
        sim.step();
        assert!(wet(&sim, tile_of(&sim, boat)), "the boat on land");
        assert!(!wet(&sim, tile_of(&sim, villager)), "the villager at sea");
    }
    for who in [boat, villager] {
        assert!(
            matches!(sim.world().order[index_of(&sim, who)], Order::Idle),
            "{:?} settled",
            sim.world().order[index_of(&sim, who)]
        );
    }
    sim.check().expect("invariants hold");
}

/// A boat and a walker ordered together each go by their own element.
#[test]
fn a_mixed_group_parts_by_element() {
    let mut sim = coast();
    let (dock, boat) = dock_and_boat(&mut sim);
    let villager = owned(&sim, 0, kinds::VILLAGER)[0];
    let target = pos_of(&sim, dock);
    sim.issue(move_to(0, vec![villager, boat], target));
    run(&mut sim, 3600);
    assert!(wet(&sim, tile_of(&sim, boat)));
    assert!(!wet(&sim, tile_of(&sim, villager)));
    for who in [boat, villager] {
        assert!(pos_of(&sim, who).distance(target) < sim::Fx::from_int(6));
    }
}

/// The match replays, boats and all.
#[test]
fn a_match_with_boats_replays() {
    let mut sim = coast();
    let (dock, boat) = dock_and_boat(&mut sim);
    let fish = nearest_kind(&sim, kinds::FISH, pos_of(&sim, dock));
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Gather {
            ids: vec![boat],
            node: fish,
        },
    });
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Train {
            building: dock,
            kind: kinds::FISHING_BOAT,
        },
    });
    run(&mut sim, 2000);
    sim.replay().verify().expect("replays");
}

/// Open water `d` tiles out from land near player 0's coast, and the land
/// tile it faces.
fn offshore(sim: &Simulation, d: i32) -> ((i32, i32), (i32, i32)) {
    let (x, y) = dock_site(sim);
    let n = sim.map().width();
    let mut best = None;
    for ty in 0..n {
        for tx in 0..n {
            if !sim.water_grid().passable(tx, ty) {
                continue;
            }
            // The nearest land, and whether it is exactly `d` out.
            let land = (1..=d + 1).find_map(|r| {
                (-r..=r)
                    .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
                    .filter(|&(dx, dy)| dx.abs().max(dy.abs()) == r)
                    .map(|(dx, dy)| (tx + dx, ty + dy))
                    .find(|&(lx, ly)| sim.nav().passable(lx, ly))
                    .map(|l| (r, l))
            });
            if let Some((r, l)) = land {
                if r == d {
                    let k = (tx - x).pow(2) + (ty - y).pow(2);
                    if best.is_none_or(|(bk, _, _)| k < bk) {
                        best = Some((k, (tx, ty), l));
                    }
                }
            }
        }
    }
    let (_, sea, land) = best.expect("a coast");
    (sea, land)
}

fn attack(sim: &mut Simulation, player: u8, ids: Vec<EntityId>, target: EntityId) {
    sim.issue(Command {
        player,
        kind: CommandKind::Attack { ids, target },
    });
}

fn alive(sim: &Simulation, id: EntityId) -> bool {
    sim.world()
        .slot(id)
        .is_some_and(|s| sim.world().dying[s.index()] == 0)
}

/// An archer ship sinks a fishing boat.
///
/// REQ: GD-NAVAL-02
#[test]
fn an_archer_ship_sinks_a_fishing_boat() {
    let mut sim = coast();
    let (sea, _) = offshore(&sim, 4);
    let ship = put(&mut sim, 0, kinds::ARCHER_SHIP, sea.0, sea.1);
    let prey = sim
        .water_grid()
        .nearest_passable(sea.0 + 3, sea.1, 3, Some(sea))
        .expect("water nearby");
    let boat = put(&mut sim, 1, kinds::FISHING_BOAT, prey.0, prey.1);
    attack(&mut sim, 0, vec![ship], boat);
    run(&mut sim, 1200);
    assert!(!alive(&sim, boat), "sunk");
    assert!(alive(&sim, ship));
}

/// Across the shore, bow meets bow: a warship shoots a villager on the
/// beach unprompted, and a bowman on the beach shoots the warship.
///
/// REQ: GD-NAVAL-02
#[test]
fn warships_and_the_shore_shoot_each_other() {
    let mut sim = coast();
    let (sea, land) = offshore(&sim, 3);
    let ship = put(&mut sim, 0, kinds::ARCHER_SHIP, sea.0, sea.1);
    let villager = put(&mut sim, 1, kinds::VILLAGER, land.0, land.1);
    let hp = sim.world().health[index_of(&sim, villager)];
    run(&mut sim, 200);
    assert!(
        !alive(&sim, villager) || sim.world().health[index_of(&sim, villager)] < hp,
        "the ship took the villager on its own"
    );
    let bowman = put(&mut sim, 1, kinds::BOWMAN, land.0, land.1);
    let hull = sim.world().health[index_of(&sim, ship)];
    attack(&mut sim, 1, vec![bowman], ship);
    run(&mut sim, 200);
    assert!(
        sim.world().health[index_of(&sim, ship)] < hull,
        "hit from the beach"
    );
}

/// A clubman cannot fight a ship: told to, it does nothing, and a ship off
/// the beach does not draw it into the sea.
///
/// REQ: GD-NAVAL-02
#[test]
fn a_clubman_cannot_fight_a_ship() {
    let mut sim = coast();
    let (sea, land) = offshore(&sim, 2);
    let boat = put(&mut sim, 1, kinds::FISHING_BOAT, sea.0, sea.1);
    let clubman = put(&mut sim, 0, kinds::CLUBMAN, land.0, land.1);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::SetStance {
            ids: vec![clubman],
            stance: sim::Stance::Aggressive,
        },
    });
    attack(&mut sim, 0, vec![clubman], boat);
    run(&mut sim, 2);
    assert!(!matches!(
        sim.world().order[index_of(&sim, clubman)],
        Order::Attack { .. }
    ));
    run(&mut sim, 200);
    assert!(!matches!(
        sim.world().order[index_of(&sim, clubman)],
        Order::Attack { .. }
    ));
    assert!(alive(&sim, boat));
}

/// A warship sent at something far inland sails as near as the water goes
/// and, out of reach there, gives up rather than waiting at the shore.
///
/// REQ: GD-NAVAL-02
#[test]
fn a_warship_gives_up_on_what_is_out_of_reach_inland() {
    let mut sim = coast();
    let (sea, _) = offshore(&sim, 2);
    let ship = put(&mut sim, 0, kinds::ARCHER_SHIP, sea.0, sea.1);
    let tc = owned(&sim, 1, kinds::TOWN_CENTER)[0];
    attack(&mut sim, 0, vec![ship], tc);
    let mut ended = false;
    for _ in 0..6000 {
        sim.step();
        assert!(wet(&sim, tile_of(&sim, ship)));
        if !matches!(
            sim.world().order[index_of(&sim, ship)],
            Order::Attack { .. }
        ) {
            ended = true;
            break;
        }
    }
    assert!(ended, "still on the attack");
}

/// The Greeks' ships are 30% faster (`docs/02` §11).
#[test]
fn greek_ships_are_faster() {
    let base = kinds::info(kinds::WAR_GALLEY).speed_per_second;
    let greek = sim::Civ::Greeks.speed_pct(kinds::WAR_GALLEY);
    assert_eq!(greek, 30);
    assert!(base.is_positive());
}

/// Open water by the shore at least `apart` tiles from `from`, with the
/// land it faces: a second landing place along the coast.
fn landing_away_from(sim: &Simulation, from: (i32, i32), apart: i32) -> (i32, i32) {
    let n = sim.map().width();
    (0..n)
        .flat_map(|y| (0..n).map(move |x| (x, y)))
        .filter(|&(x, y)| sim.nav().passable(x, y))
        .filter(|&(x, y)| (x - from.0).abs().max((y - from.1).abs()) >= apart)
        .filter(|&(x, y)| {
            (-1..=1).any(|dy| (-1..=1).any(|dx| sim.water_grid().passable(x + dx, y + dy)))
        })
        .min_by_key(|&(x, y)| ((x - from.0).pow(2) + (y - from.1).pow(2), x, y))
        .expect("more coast")
}

fn board(sim: &mut Simulation, ids: Vec<EntityId>, transport: EntityId) {
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Garrison {
            ids,
            building: transport,
        },
    });
}

fn aboard(sim: &Simulation, id: EntityId) -> Option<EntityId> {
    sim.world().inside[index_of(sim, id)]
}

/// Three clubmen board a transport from the beach, ride it along the coast
/// and are put ashore where they were sent.
///
/// REQ: GD-NAVAL-03
#[test]
fn a_transport_carries_soldiers_and_puts_them_ashore() {
    let mut sim = coast();
    let (sea, land) = offshore(&sim, 3);
    let boat = put(&mut sim, 0, kinds::TRANSPORT, sea.0, sea.1);
    let men: Vec<EntityId> = (0..3)
        .map(|k| {
            let t = sim
                .nav()
                .nearest_passable(land.0, land.1 + k, 3, None)
                .unwrap();
            put(&mut sim, 0, kinds::CLUBMAN, t.0, t.1)
        })
        .collect();
    board(&mut sim, men.clone(), boat);
    for _ in 0..1200 {
        sim.step();
        if men.iter().all(|&m| aboard(&sim, m) == Some(boat)) {
            break;
        }
    }
    for &m in &men {
        assert_eq!(aboard(&sim, m), Some(boat), "aboard");
    }
    // Riding: wherever the boat goes, they are.
    let there = landing_away_from(&sim, land, 15);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Unload {
            ids: vec![boat],
            target: sim::nav::centre(there),
        },
    });
    run(&mut sim, 20);
    for &m in &men {
        assert_eq!(pos_of(&sim, m), pos_of(&sim, boat), "riding");
    }
    for _ in 0..3000 {
        sim.step();
        if men.iter().all(|&m| aboard(&sim, m).is_none()) {
            break;
        }
    }
    for &m in &men {
        assert_eq!(aboard(&sim, m), None, "ashore");
        assert!(!wet(&sim, tile_of(&sim, m)), "on land");
        assert!(
            pos_of(&sim, m).distance(sim::nav::centre(there)) < sim::Fx::from_int(6),
            "where they were sent"
        );
    }
    assert!(matches!(
        sim.world().order[index_of(&sim, boat)],
        Order::Idle
    ));
    sim.check().expect("invariants hold");
    sim.replay().verify().expect("replays");
}

/// At sea, nobody gets off; and a boat cannot go aboard anything.
///
/// REQ: GD-NAVAL-03
#[test]
fn nobody_steps_off_at_sea_and_boats_do_not_board() {
    let mut sim = coast();
    let (sea, land) = offshore(&sim, 1);
    let boat = put(&mut sim, 0, kinds::TRANSPORT, sea.0, sea.1);
    let man = put(&mut sim, 0, kinds::CLUBMAN, land.0, land.1);
    board(&mut sim, vec![man], boat);
    run(&mut sim, 200);
    assert_eq!(aboard(&sim, man), Some(boat));
    let (far, _) = offshore(&sim, 6);
    sim.issue(move_to(0, vec![boat], sim::nav::centre(far)));
    run(&mut sim, 1200);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Ungarrison { building: boat },
    });
    run(&mut sim, 2);
    assert_eq!(aboard(&sim, man), Some(boat), "still aboard at sea");
    let fisher = put(&mut sim, 0, kinds::FISHING_BOAT, far.0, far.1 + 1);
    board(&mut sim, vec![fisher], boat);
    run(&mut sim, 2);
    assert!(!matches!(
        sim.world().order[index_of(&sim, fisher)],
        Order::Garrison { .. }
    ));
}

/// A transport that sinks takes everyone aboard down with it.
///
/// REQ: GD-NAVAL-03
#[test]
fn a_sunk_transport_drowns_its_passengers() {
    let mut sim = coast();
    let (sea, land) = offshore(&sim, 2);
    let boat = put(&mut sim, 0, kinds::TRANSPORT, sea.0, sea.1);
    let man = put(&mut sim, 0, kinds::CLUBMAN, land.0, land.1);
    board(&mut sim, vec![man], boat);
    run(&mut sim, 200);
    assert_eq!(aboard(&sim, man), Some(boat));
    let pop = sim.player(0).unwrap().pop;
    let galleys: Vec<EntityId> = (0..3)
        .map(|k| {
            let w = sim
                .water_grid()
                .nearest_passable(sea.0 + 2, sea.1 + k, 4, Some(sea))
                .unwrap();
            put(&mut sim, 1, kinds::WAR_GALLEY, w.0, w.1)
        })
        .collect();
    attack(&mut sim, 1, galleys, boat);
    run(&mut sim, 1200);
    assert!(!alive(&sim, boat), "sunk");
    assert!(sim.world().slot(man).is_none(), "drowned");
    assert!(sim.player(0).unwrap().pop < pop);
    sim.check().expect("invariants hold");
}

/// A trade boat takes wood out to another side's Dock and brings gold
/// home, the more the further it sails; with no wood it waits at home.
///
/// REQ: GD-NAVAL-04
#[test]
fn a_trade_boat_turns_wood_into_gold() {
    let mut sim = coast();
    let (hx, hy) = dock_site(&sim);
    let home = put(&mut sim, 0, kinds::DOCK, hx, hy);
    // Another side's Dock along the coast, by water from ours.
    let n = sim.map().width();
    let fp = kinds::info(kinds::DOCK).footprint as i32;
    let here = sim.water_grid().nearest_passable(hx, hy, 4, None).unwrap();
    let (mx, my) = (0..n)
        .flat_map(|y| (0..n).map(move |x| (x, y)))
        .filter(|&(x, y)| (x - hx).abs().max((y - hy).abs()) >= 25)
        .filter(|&(x, y)| sim.can_place(1, kinds::DOCK, x, y).is_ok())
        .find(|&(x, y)| {
            sim::nav::footprint_ring(x, y, fp).iter().any(|&t| {
                sim.water_grid().passable(t.0, t.1) && sim.water_grid().connected(here, t)
            })
        })
        .expect("a second Dock site on the same sea");
    let market = put(&mut sim, 1, kinds::DOCK, mx, my);
    let boat = put(&mut sim, 0, kinds::TRADE_BOAT, here.0, here.1);
    let start = sim.player(0).unwrap().stockpile;
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Trade {
            ids: vec![boat],
            dock: market,
        },
    });
    let gold = |sim: &Simulation| sim.player(0).unwrap().stockpile[Resource::Gold.index()];
    let mut trips = 0;
    let mut last = gold(&sim);
    for _ in 0..12_000 {
        sim.step();
        if gold(&sim) > last {
            trips += 1;
            last = gold(&sim);
            if trips == 2 {
                break;
            }
        }
    }
    assert_eq!(trips, 2, "two round trips");
    let tiles = pos_of(&sim, home).distance(pos_of(&sim, market)).floor();
    let earned = gold(&sim) - start[Resource::Gold.index()];
    assert_eq!(earned, 2 * sim::transport::trade_gold(tiles));
    assert!(
        earned > 2 * sim::transport::TRADE_LOAD,
        "a profit for the distance"
    );
    let wood = sim.player(0).unwrap().stockpile[Resource::Wood.index()];
    assert!(wood <= start[Resource::Wood.index()] - 2 * sim::transport::TRADE_LOAD);
    assert!(matches!(
        sim.world().order[index_of(&sim, boat)],
        Order::Trade { .. }
    ));
    sim.replay().verify().expect("replays");
}

/// A trade boat trades only at another side's Dock.
///
/// REQ: GD-NAVAL-04
#[test]
fn a_trade_boat_does_not_trade_at_home() {
    let mut sim = coast();
    let (dock, fisher) = dock_and_boat(&mut sim);
    let here = tile_of(&sim, fisher);
    let boat = put(&mut sim, 0, kinds::TRADE_BOAT, here.0, here.1);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Trade {
            ids: vec![boat],
            dock,
        },
    });
    run(&mut sim, 2);
    assert!(matches!(
        sim.world().order[index_of(&sim, boat)],
        Order::Idle
    ));
}

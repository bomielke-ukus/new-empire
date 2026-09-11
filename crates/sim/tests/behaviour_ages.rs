//! Ages, technology and farms, from `docs/02` §3.3, §4 and the M3 roadmap
//! item.
//!
//! These drive the simulation through commands only, the way a match does,
//! because the age gate is a rule about what a player can *do* with a Town
//! Center rather than a number in a table. Where a test needs a building to
//! exist without waiting for villagers to build it, it uses the `Spawn`
//! scenario command, which places a finished one.

mod common;
use common::{index_of, owned, run, spawn};
use sim::kinds::{self, Resource};
use sim::tech::{self, AGE_BRONZE, AGE_TOOL, CARRYING_BASKETS, WOODWORKING};
use sim::{
    Age, Command, CommandKind, EntityId, Item, MapKind, MapSpec, Order, PlaceError, PlayerId,
    ResearchError, SimConfig, Simulation, Vec2Fx, TICKS_PER_SECOND,
};

/// An Inland match where money is never the obstacle, so a test about the
/// building gate is about the building gate.
fn rich_inland(seed: u64) -> Simulation {
    Simulation::new(
        seed,
        SimConfig {
            wander: false,
            starting_stockpile: [5000; 4],
            ..SimConfig::default()
        },
    )
}

/// A bare flat map with nothing on it; the test spawns what it needs.
fn flat(players: u8, stockpile: [i32; 4]) -> Simulation {
    Simulation::new(
        1,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Flat,
                size: 48,
                players,
            },
            wander: false,
            starting_stockpile: stockpile,
            ..SimConfig::default()
        },
    )
}

fn town_center(sim: &Simulation, p: PlayerId) -> EntityId {
    owned(sim, p, kinds::TOWN_CENTER)[0]
}

fn research(p: PlayerId, building: EntityId, tech: u16) -> Command {
    Command {
        player: p,
        kind: CommandKind::Research { building, tech },
    }
}

/// The first anchor near `p`'s start where `kind` can go right now.
fn free_spot(sim: &Simulation, p: PlayerId, kind: u16) -> (i32, i32) {
    let (sx, sy) = sim.starts()[p as usize];
    for dy in [0, 5, -5, 10, -10] {
        for dx in 4..=30 {
            if sim.can_place(p, kind, sx + dx, sy + dy).is_ok() {
                return (sx + dx, sy + dy);
            }
        }
    }
    panic!("no room for {} near the start", kinds::info(kind).name);
}

/// Spawns a finished building of `kind` near `p`'s start and returns it.
fn spawn_building(sim: &mut Simulation, p: PlayerId, kind: u16) -> EntityId {
    let before = owned(sim, p, kind);
    let (x, y) = free_spot(sim, p, kind);
    let fp = kinds::info(kind).footprint as i32;
    sim.issue(spawn(p, kind, sim::nav::building_centre(x, y, fp)));
    run(sim, 3);
    let after = owned(sim, p, kind);
    *after
        .iter()
        .find(|id| !before.contains(id))
        .expect("the building was spawned")
}

/// Orders `p`'s villagers to build `kind` near the start and waits for it.
fn build_and_wait(sim: &mut Simulation, p: PlayerId, kind: u16) -> EntityId {
    let villagers = owned(sim, p, kinds::VILLAGER);
    let before = owned(sim, p, kind);
    let (x, y) = free_spot(sim, p, kind);
    sim.issue(Command {
        player: p,
        kind: CommandKind::Build {
            kind,
            x,
            y,
            ids: villagers,
        },
    });
    run(sim, 3);
    let site = *owned(sim, p, kind)
        .iter()
        .find(|id| !before.contains(id))
        .expect("the site was placed");
    let limit = kinds::info(kind).build_ticks() + 20 * TICKS_PER_SECOND;
    for _ in 0..limit {
        if sim.world().construction[index_of(sim, site)].is_none() {
            return site;
        }
        sim.step();
    }
    panic!(
        "{} was not finished within {limit} ticks",
        kinds::info(kind).name
    );
}

fn run_until(sim: &mut Simulation, limit: u32, done: impl Fn(&Simulation) -> bool) -> bool {
    for _ in 0..limit {
        if done(sim) {
            return true;
        }
        sim.step();
    }
    done(sim)
}

// ---------------------------------------------------------------------------
// The age gate — docs/02 §4
// ---------------------------------------------------------------------------

/// REQ: GD-AGE-01
#[test]
fn advancing_an_age_needs_two_buildings_of_the_current_age() {
    let mut sim = rich_inland(21);
    let tc = town_center(&sim, 0);
    let tool = tech::info(AGE_TOOL).unwrap();
    assert_eq!(tool.cost, [400, 0, 0, 0], "the Tool Age costs 400 food");
    assert_eq!(tool.seconds, 60);

    // Money alone does not do it.
    assert_eq!(
        sim.can_research(0, tc, AGE_TOOL),
        Err(ResearchError::NeedBuildings { have: 0, need: 2 }),
        "a rich player with only a Town Center cannot advance"
    );
    let food = sim.player(0).unwrap().stockpile[Resource::Food.index()];
    sim.issue(research(0, tc, AGE_TOOL));
    run(&mut sim, 5);
    assert_eq!(
        sim.player(0).unwrap().stockpile[Resource::Food.index()],
        food,
        "a refused advance costs nothing"
    );
    assert!(!sim.tech_queued(0, AGE_TOOL));

    // Houses do not count, however many.
    spawn_building(&mut sim, 0, kinds::HOUSE);
    spawn_building(&mut sim, 0, kinds::HOUSE);
    assert_eq!(sim.age_buildings(0, Age::Stone), 0);
    assert_eq!(
        sim.can_research(0, tc, AGE_TOOL),
        Err(ResearchError::NeedBuildings { have: 0, need: 2 })
    );

    // Two Stone Age buildings do.
    spawn_building(&mut sim, 0, kinds::BARRACKS);
    assert_eq!(sim.age_buildings(0, Age::Stone), 1);
    spawn_building(&mut sim, 0, kinds::STOREHOUSE);
    assert_eq!(sim.age_buildings(0, Age::Stone), 2);
    assert_eq!(sim.can_research(0, tc, AGE_TOOL), Ok(()));

    sim.issue(research(0, tc, AGE_TOOL));
    run(&mut sim, 3);
    assert_eq!(
        sim.player(0).unwrap().stockpile[Resource::Food.index()],
        food - 400,
        "the advance is paid for on queueing"
    );
    assert!(sim.tech_queued(0, AGE_TOOL));
    assert_eq!(sim.player(0).unwrap().age, Age::Stone, "not yet");

    let arrived = run_until(&mut sim, tool.ticks() + 5, |s| {
        s.player(0).unwrap().age == Age::Tool
    });
    assert!(arrived, "sixty seconds later the player is in the Tool Age");
    assert!(sim.player(0).unwrap().has_researched(AGE_TOOL));
    assert_eq!(
        sim.can_research(0, tc, AGE_TOOL),
        Err(ResearchError::AlreadyResearched)
    );
}

/// The Tool Age buildings that count toward Bronze are Tool Age buildings,
/// not the Stone Age ones already standing, and not Farms.
///
/// REQ: GD-AGE-01
#[test]
fn the_next_age_counts_only_buildings_of_the_age_you_are_in() {
    let mut sim = rich_inland(22);
    let tc = town_center(&sim, 0);
    spawn_building(&mut sim, 0, kinds::BARRACKS);
    spawn_building(&mut sim, 0, kinds::STOREHOUSE);
    sim.issue(research(0, tc, AGE_TOOL));
    let tool_ticks = tech::info(AGE_TOOL).unwrap().ticks();
    assert!(run_until(&mut sim, tool_ticks + 10, |s| {
        s.player(0).unwrap().age == Age::Tool
    }));

    assert_eq!(
        sim.can_research(0, tc, AGE_BRONZE),
        Err(ResearchError::NeedBuildings { have: 0, need: 2 }),
        "the two Stone Age buildings do not carry over"
    );
    spawn_building(&mut sim, 0, kinds::FARM);
    spawn_building(&mut sim, 0, kinds::FARM);
    assert_eq!(
        sim.age_buildings(0, Age::Tool),
        0,
        "farms are fields, not a commitment"
    );
    assert_eq!(kinds::info(kinds::MARKET).age, Age::Tool);
    assert_eq!(kinds::info(kinds::ARCHERY_RANGE).age, Age::Tool);
    spawn_building(&mut sim, 0, kinds::MARKET);
    spawn_building(&mut sim, 0, kinds::ARCHERY_RANGE);
    assert_eq!(sim.age_buildings(0, Age::Tool), 2);
    assert_eq!(sim.can_research(0, tc, AGE_BRONZE), Ok(()));
}

/// Buildings of a later age cannot be placed before it, and the refusal
/// names the age so a build menu can say why.
///
/// REQ: GD-AGE-01
#[test]
fn later_age_buildings_are_locked_until_the_age_arrives() {
    let mut sim = rich_inland(23);
    let (sx, sy) = sim.starts()[0];
    assert_eq!(
        sim.can_place(0, kinds::MARKET, sx + 6, sy),
        Err(PlaceError::AgeLocked { needs: Age::Tool })
    );
    assert_eq!(
        sim.can_place(0, kinds::SIEGE_WORKSHOP, sx + 6, sy),
        Err(PlaceError::AgeLocked { needs: Age::Bronze })
    );
    // The command is refused too, not just the query.
    let villagers = owned(&sim, 0, kinds::VILLAGER);
    let (x, y) = free_spot(&sim, 0, kinds::HOUSE);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Build {
            kind: kinds::MARKET,
            x,
            y,
            ids: villagers,
        },
    });
    run(&mut sim, 5);
    assert!(owned(&sim, 0, kinds::MARKET).is_empty());
    assert_eq!(sim.player(0).unwrap().stockpile, [5000; 4], "nothing paid");
}

// ---------------------------------------------------------------------------
// Technology
// ---------------------------------------------------------------------------

/// A technology is researched at its building, in its age, after its
/// prerequisites, once; and it changes the numbers it says it changes.
#[test]
fn technology_is_gated_queued_and_applied_through_modifiers() {
    let mut sim = rich_inland(24);
    let tc = town_center(&sim, 0);
    let storehouse = spawn_building(&mut sim, 0, kinds::STOREHOUSE);
    let barracks = spawn_building(&mut sim, 0, kinds::BARRACKS);

    assert_eq!(
        sim.can_research(0, tc, WOODWORKING),
        Err(ResearchError::WrongBuilding),
        "Woodworking is a Storehouse technology"
    );
    assert_eq!(
        sim.can_research(0, storehouse, WOODWORKING),
        Err(ResearchError::AgeLocked { needs: Age::Tool }),
        "and a Tool Age one"
    );
    assert_eq!(
        sim.can_research(0, barracks, 999),
        Err(ResearchError::UnknownTech)
    );

    sim.issue(research(0, tc, AGE_TOOL));
    let tool_ticks = tech::info(AGE_TOOL).unwrap().ticks();
    assert!(run_until(&mut sim, tool_ticks + 10, |s| {
        s.player(0).unwrap().age == Age::Tool
    }));

    assert_eq!(
        sim.can_research(0, storehouse, CARRYING_BASKETS),
        Err(ResearchError::AgeLocked { needs: Age::Bronze }),
        "Carrying Baskets is Bronze Age"
    );
    assert_eq!(sim.can_research(0, storehouse, WOODWORKING), Ok(()));
    let wood_before = sim.modifiers(0).gather_rate(Resource::Wood);
    sim.issue(research(0, storehouse, WOODWORKING));
    run(&mut sim, 3);
    assert_eq!(
        sim.can_research(0, storehouse, WOODWORKING),
        Err(ResearchError::AlreadyQueued)
    );
    let queued = sim.world().production[index_of(&sim, storehouse)]
        .as_ref()
        .map(|q| q.queue.iter().map(|i| i.item).collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(queued, vec![Item::Tech(WOODWORKING)]);

    let ticks = tech::info(WOODWORKING).unwrap().ticks();
    assert!(run_until(&mut sim, ticks + 5, |s| {
        s.player(0).unwrap().has_researched(WOODWORKING)
    }));
    let m = sim.modifiers(0);
    assert_eq!(m.gather_rate_pct[Resource::Wood.index()], 15);
    assert_eq!(
        m.gather_rate(Resource::Wood),
        wood_before.mul_div(sim::Fx::from_int(115), sim::Fx::from_int(100)),
        "wood is gathered 15% faster"
    );
    assert_eq!(
        m.gather_rate(Resource::Food),
        Resource::Food.gather_rate(),
        "food is not"
    );
    assert_eq!(
        sim.can_research(0, storehouse, WOODWORKING),
        Err(ResearchError::AlreadyResearched)
    );
}

/// Cancelling a queued technology refunds it, like cancelling a villager.
#[test]
fn a_cancelled_technology_is_refunded() {
    let mut sim = rich_inland(25);
    let tc = town_center(&sim, 0);
    spawn_building(&mut sim, 0, kinds::BARRACKS);
    spawn_building(&mut sim, 0, kinds::STOREHOUSE);
    let before = sim.player(0).unwrap().stockpile;
    sim.issue(research(0, tc, AGE_TOOL));
    run(&mut sim, 3);
    assert_eq!(
        sim.player(0).unwrap().stockpile[Resource::Food.index()],
        before[Resource::Food.index()] - 400
    );
    sim.issue(Command {
        player: 0,
        kind: CommandKind::CancelTrain { building: tc },
    });
    run(&mut sim, 3);
    assert_eq!(sim.player(0).unwrap().stockpile, before, "refunded in full");
    assert!(!sim.tech_queued(0, AGE_TOOL));
    assert_eq!(sim.player(0).unwrap().age, Age::Stone);
}

// ---------------------------------------------------------------------------
// Farms — docs/02 §3.3
// ---------------------------------------------------------------------------

/// Spawns a Town Center, a Farm beside it and eight villagers on the bare
/// map, and sets the villagers farming. Returns the farm.
fn farmstead(sim: &mut Simulation) -> EntityId {
    let tc_fp = kinds::info(kinds::TOWN_CENTER).footprint as i32;
    let farm_fp = kinds::info(kinds::FARM).footprint as i32;
    sim.issue(spawn(
        0,
        kinds::TOWN_CENTER,
        sim::nav::building_centre(10, 10, tc_fp),
    ));
    sim.issue(spawn(
        0,
        kinds::FARM,
        sim::nav::building_centre(10 + tc_fp + 1, 10, farm_fp),
    ));
    for n in 0..8 {
        sim.issue(spawn(
            0,
            kinds::VILLAGER,
            Vec2Fx::from_int(9 + n % 4, 10 + tc_fp + 2 + n / 4),
        ));
    }
    run(sim, 3);
    let farm = owned(sim, 0, kinds::FARM)[0];
    let villagers = owned(sim, 0, kinds::VILLAGER);
    assert_eq!(villagers.len(), 8);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Gather {
            ids: villagers,
            node: farm,
        },
    });
    run(sim, 3);
    farm
}

fn farm_food(sim: &Simulation, farm: EntityId) -> i32 {
    sim.world().resource[index_of(sim, farm)]
}

/// REQ: GD-ECON-05
#[test]
fn farms_reseed_themselves_for_sixty_wood_until_the_wood_runs_out() {
    assert_eq!(
        kinds::info(kinds::FARM).resource,
        Some((Resource::Food, 250))
    );
    assert_eq!(kinds::FARM_RESEED_COST, [0, 60, 0, 0]);

    // 100 wood: one reseed, then 40 left, which is not enough for another.
    let mut sim = flat(1, [0, 100, 0, 0]);
    assert!(sim.player(0).unwrap().auto_reseed, "on by default");
    let farm = farmstead(&mut sim);
    assert_eq!(farm_food(&sim, farm), 250);

    // Eight villagers at 0.45/s empty 250 food in about 70 seconds.
    let reseeded = run_until(&mut sim, 20 * 150, |s| {
        s.player(0).unwrap().stockpile[Resource::Wood.index()] == 40
    });
    assert!(reseeded, "the first exhaustion cost 60 wood");
    assert!(
        farm_food(&sim, farm) > 0,
        "and the farm is full again, not gone"
    );
    assert!(
        sim.world().slot(farm).is_some(),
        "an exhausted farm is never removed"
    );

    // The second exhaustion cannot be paid for: the farm stays, empty, and
    // the villagers deliver what they hold and stop.
    let drained = run_until(&mut sim, 20 * 200, |s| {
        s.player(0).unwrap().gathered[Resource::Food.index()] == 500
    });
    assert!(drained, "both seedings were gathered and delivered in full");
    run(&mut sim, 40);
    assert_eq!(farm_food(&sim, farm), 0, "empty");
    assert!(sim.world().slot(farm).is_some(), "still standing");
    assert_eq!(sim.player(0).unwrap().stockpile[Resource::Wood.index()], 40);
    for v in owned(&sim, 0, kinds::VILLAGER) {
        assert_eq!(
            sim.world().order[index_of(&sim, v)],
            Order::Idle,
            "nothing left to farm"
        );
    }
}

/// REQ: GD-ECON-05
#[test]
fn auto_reseed_can_be_switched_off_and_on() {
    let mut sim = flat(1, [0, 1000, 0, 0]);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::SetAutoReseed { enabled: false },
    });
    let farm = farmstead(&mut sim);
    assert!(!sim.player(0).unwrap().auto_reseed);

    let drained = run_until(&mut sim, 20 * 150, |s| {
        s.player(0).unwrap().gathered[Resource::Food.index()] == 250
    });
    assert!(drained);
    run(&mut sim, 40);
    assert_eq!(
        farm_food(&sim, farm),
        0,
        "not reseeded while the toggle is off"
    );
    assert_eq!(
        sim.player(0).unwrap().stockpile[Resource::Wood.index()],
        1000,
        "and nothing was paid"
    );

    sim.issue(Command {
        player: 0,
        kind: CommandKind::SetAutoReseed { enabled: true },
    });
    run(&mut sim, 4);
    assert_eq!(
        farm_food(&sim, farm),
        250,
        "switched on, it reseeds at once"
    );
    assert_eq!(
        sim.player(0).unwrap().stockpile[Resource::Wood.index()],
        940
    );
}

/// A farm is its owner's. Another player's villager sent to it is ignored.
#[test]
fn only_the_owner_may_work_a_farm() {
    let mut sim = flat(2, [0, 100, 0, 0]);
    let farm_fp = kinds::info(kinds::FARM).footprint as i32;
    sim.issue(spawn(
        0,
        kinds::FARM,
        sim::nav::building_centre(20, 20, farm_fp),
    ));
    sim.issue(spawn(1, kinds::VILLAGER, Vec2Fx::from_int(24, 20)));
    sim.issue(spawn(0, kinds::VILLAGER, Vec2Fx::from_int(24, 22)));
    run(&mut sim, 3);
    let farm = owned(&sim, 0, kinds::FARM)[0];
    let theirs = owned(&sim, 1, kinds::VILLAGER)[0];
    let mine = owned(&sim, 0, kinds::VILLAGER)[0];
    for (p, v) in [(1, theirs), (0, mine)] {
        sim.issue(Command {
            player: p,
            kind: CommandKind::Gather {
                ids: vec![v],
                node: farm,
            },
        });
    }
    run(&mut sim, 3);
    assert_eq!(sim.world().order[index_of(&sim, theirs)], Order::Idle);
    assert!(matches!(
        sim.world().order[index_of(&sim, mine)],
        Order::Gather { node, .. } if node == farm
    ));
}

/// A farm under construction holds nothing; it is seeded when finished.
#[test]
fn a_farm_is_seeded_when_it_is_finished() {
    let mut sim = rich_inland(26);
    let tc = town_center(&sim, 0);
    spawn_building(&mut sim, 0, kinds::BARRACKS);
    spawn_building(&mut sim, 0, kinds::STOREHOUSE);
    sim.issue(research(0, tc, AGE_TOOL));
    let tool_ticks = tech::info(AGE_TOOL).unwrap().ticks();
    assert!(run_until(&mut sim, tool_ticks + 10, |s| {
        s.player(0).unwrap().age == Age::Tool
    }));
    let villagers = owned(&sim, 0, kinds::VILLAGER);
    let (x, y) = free_spot(&sim, 0, kinds::FARM);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Build {
            kind: kinds::FARM,
            x,
            y,
            ids: villagers,
        },
    });
    run(&mut sim, 3);
    let farm = owned(&sim, 0, kinds::FARM)[0];
    assert!(sim.world().construction[index_of(&sim, farm)].is_some());
    assert_eq!(farm_food(&sim, farm), 0, "a site has no food in it");
    let wood = sim.player(0).unwrap().stockpile[Resource::Wood.index()];
    assert!(run_until(
        &mut sim,
        kinds::info(kinds::FARM).build_ticks() + 20 * TICKS_PER_SECOND,
        |s| s.world().construction[index_of(s, farm)].is_none()
    ));
    assert_eq!(farm_food(&sim, farm), 250, "seeded on completion");
    assert_eq!(
        sim.player(0).unwrap().stockpile[Resource::Wood.index()],
        wood,
        "the first seeding is part of the build price"
    );
}

// ---------------------------------------------------------------------------
// The milestone — docs/06 M3
// ---------------------------------------------------------------------------

/// Stone → Tool → Bronze with commands only: build, advance, build, advance.
/// Nothing is spawned; the villagers do the work.
///
/// REQ: RM-M3-01
#[test]
fn a_player_can_go_from_stone_to_bronze_in_a_live_match() {
    let mut sim = rich_inland(27);
    let tc = town_center(&sim, 0);
    assert_eq!(sim.player(0).unwrap().age, Age::Stone);

    build_and_wait(&mut sim, 0, kinds::BARRACKS);
    build_and_wait(&mut sim, 0, kinds::STOREHOUSE);
    assert_eq!(sim.can_research(0, tc, AGE_TOOL), Ok(()));
    sim.issue(research(0, tc, AGE_TOOL));
    let tool_ticks = tech::info(AGE_TOOL).unwrap().ticks();
    assert!(run_until(&mut sim, tool_ticks + 10, |s| {
        s.player(0).unwrap().age == Age::Tool
    }));

    build_and_wait(&mut sim, 0, kinds::MARKET);
    build_and_wait(&mut sim, 0, kinds::ARCHERY_RANGE);
    assert_eq!(sim.can_research(0, tc, AGE_BRONZE), Ok(()));
    sim.issue(research(0, tc, AGE_BRONZE));
    let bronze_ticks = tech::info(AGE_BRONZE).unwrap().ticks();
    assert!(run_until(&mut sim, bronze_ticks + 10, |s| {
        s.player(0).unwrap().age == Age::Bronze
    }));
    assert_eq!(
        sim.player(0).unwrap().researched,
        vec![AGE_TOOL, AGE_BRONZE],
        "both advances are on the record, in order"
    );
    assert!(sim.check().is_ok());
}

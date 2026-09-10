//! The economy and population requirements from `docs/02`, §3.3 and §3.4.
//!
//! These are numbers a designer chose and a player feels. They are cheap to
//! assert and expensive to get wrong quietly: a gather rate that drifts, or a
//! House that stops paying for itself, changes the pace of every match without
//! failing anything else in the repository.
//!
//! Two of them do not match the code, and this file says so out loud rather
//! than asserting whichever value happens to be there. See
//! `the_gather_rate_is_not_the_single_base_rate_the_spec_describes` and
//! `the_population_cap_is_not_range_checked`.

mod common;
use common::{index_of, inland, nearest_kind, owned, pos_of, run};
use sim::kinds::{self, Resource, CARRY_CAPACITY};
use sim::{Command, CommandKind, GatherPhase, Order, SimConfig, Simulation};

// ---------------------------------------------------------------------------
// Gathering — docs/02 §3.3
// ---------------------------------------------------------------------------

/// REQ: GD-ECON-02
#[test]
fn a_villager_carries_ten_before_walking_home() {
    assert_eq!(
        CARRY_CAPACITY, 10,
        "carry capacity is half of [GD-ECON-02]; the walk home is the other half"
    );
}

/// The spec says one **base** rate of 0.45/s. The table has four rates, and
/// only wood is 0.45 — food, stone and gold are 0.40.
///
/// This test pins what the code does and names the disagreement, because the
/// alternative is asserting 0.45 (which fails) or asserting 0.40 (which
/// silently blesses a departure from the design). Which one is wrong is a
/// design decision: either wood is deliberately faster and `docs/02` should
/// say so, or the table should be flattened to 0.45.
///
/// REQ: GD-ECON-02
#[test]
fn the_gather_rate_is_not_the_single_base_rate_the_spec_describes() {
    let rates = [
        (Resource::Food, 40),
        (Resource::Wood, 45),
        (Resource::Stone, 40),
        (Resource::Gold, 40),
    ];
    for (r, hundredths) in rates {
        assert_eq!(
            r.gather_rate(),
            sim::Fx::from_ratio(hundredths, 100),
            "{} gathers at {hundredths}/100 per second",
            r.name()
        );
    }
    let mut distinct: Vec<i32> = rates.iter().map(|(_, h)| *h).collect();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        distinct,
        vec![40, 45],
        "there are two rates in the table, not the one base rate [GD-ECON-02] describes"
    );
}

/// REQ: GD-ECON-03
#[test]
fn the_storehouse_and_town_center_accept_every_resource() {
    for kind in [kinds::STOREHOUSE, kinds::TOWN_CENTER] {
        let info = kinds::info(kind);
        assert!(
            info.dropoff,
            "{} must be a drop-off — [GD-ECON-03] has one building type, not one per resource",
            info.name
        );
    }
    // The claim is that *every* resource goes to the same building, so no kind
    // may narrow it. There is no per-resource field to check: acceptance is a
    // single boolean, which is the design decision D5 made concrete. Assert
    // that nothing else in the table claims to be a drop-off, so a future
    // Granary cannot appear without this test noticing.
    let dropoffs: Vec<&str> = kinds::all()
        .iter()
        .filter(|i| i.dropoff)
        .map(|i| i.name)
        .collect();
    assert_eq!(
        dropoffs,
        vec!["Town Center", "Storehouse"],
        "only these two accept resources"
    );
}

/// Placement is the skill [GD-ECON-04] names, so the villager must actually
/// prefer the nearer building — otherwise placement buys nothing.
///
/// REQ: GD-ECON-04
#[test]
fn a_gatherer_walks_to_the_nearer_of_two_drop_offs() {
    let mut sim = inland(11);
    let villager = owned(&sim, 0, kinds::VILLAGER)[0];

    // A tree, and a storehouse as close beside it as a free tile allows. The
    // tree's own tile is blocked, so walk outward until a spawn takes.
    let tree = nearest_kind(&sim, kinds::TREE, sim.world().pos[index_of(&sim, villager)]);
    let tree_pos = pos_of(&sim, tree);
    let tree_tile = sim::nav::tile_of(tree_pos);

    // Choose the tile *before* issuing anything: commands are scheduled
    // COMMAND_DELAY ticks ahead, so a spawn-and-check loop issues several
    // before the first one lands and ends up building a row of storehouses.
    let site = (2..8)
        .flat_map(|r| [(r, 0), (-r, 0), (0, r), (0, -r)])
        .map(|(dx, dy)| (tree_tile.0 + dx, tree_tile.1 + dy))
        .find(|&(x, y)| sim.nav().in_bounds(x, y) && sim.nav().footprint_clear(x, y, 2))
        .expect("no free tile near the tree to put a storehouse on");
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Spawn {
            kind: kinds::STOREHOUSE,
            pos: sim::nav::centre(site),
        },
    });
    run(&mut sim, 5);
    let built = owned(&sim, 0, kinds::STOREHOUSE);
    assert_eq!(built.len(), 1, "exactly one storehouse, at {site:?}");
    let near = built[0];
    let town = owned(&sim, 0, kinds::TOWN_CENTER)[0];

    // Sanity: the storehouse really is the closer of the two to the tree.
    assert!(
        tree_pos.distance_sq_raw(pos_of(&sim, near)) < tree_pos.distance_sq_raw(pos_of(&sim, town)),
        "the fixture is wrong: the storehouse must be nearer the tree than the \
         Town Center is, or this test proves nothing"
    );

    sim.issue(Command {
        player: 0,
        kind: CommandKind::Gather {
            ids: vec![villager],
            node: tree,
        },
    });

    // Walk to the tree, fill up, and start carrying. Whichever building the
    // villager heads for is the one `nearest_dropoff` chose.
    let mut chosen = None;
    for _ in 0..20 * 120 {
        sim.step();
        if let Order::Gather {
            phase: GatherPhase::ToDropoff { dropoff },
            ..
        } = sim.world().order[index_of(&sim, villager)]
        {
            chosen = Some(dropoff);
            break;
        }
    }
    assert_eq!(
        chosen,
        Some(near),
        "the villager must carry to the storehouse beside the tree, not back to \
         the Town Center — [GD-ECON-04] is the whole reason placement matters"
    );
}

// ---------------------------------------------------------------------------
// Population — docs/02 §3.4
// ---------------------------------------------------------------------------

/// REQ: GD-POP-01
#[test]
fn a_house_gives_five_population_for_thirty_wood() {
    let house = kinds::info(kinds::HOUSE);
    assert_eq!(house.pop_provided, 5);
    assert_eq!(house.cost, [0, 30, 0, 0], "30 wood and nothing else");
    assert_eq!(
        kinds::info(kinds::TOWN_CENTER).pop_provided,
        5,
        "the Town Center provides 5 too, which is the opening cap"
    );
}

/// The cap is enforced against `pop_cap_max`, and a House raises the player's
/// cap toward it. Both halves are needed: a cap that nothing enforces is a
/// number in a struct.
///
/// REQ: GD-POP-01
/// REQ: GD-POP-02
#[test]
fn houses_raise_the_cap_and_the_cap_is_enforced() {
    let mut sim = inland(5);
    assert_eq!(
        SimConfig::default().pop_cap_max,
        75,
        "[GD-POP-02] default cap"
    );
    let opening = sim.player(0).expect("player 0").pop_cap;
    assert_eq!(opening, 5, "one Town Center, five population");

    let (sx, sy) = sim.starts()[0];
    let builders = owned(&sim, 0, kinds::VILLAGER);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Build {
            kind: kinds::HOUSE,
            x: sx + 4,
            y: sy,
            ids: builders[..2].to_vec(),
        },
    });
    run(&mut sim, 20 * 40);
    assert_eq!(
        sim.player(0).expect("player 0").pop_cap,
        opening + 5,
        "a finished House adds its five"
    );
}

/// `docs/02` says the cap is "configurable 50–200". Nothing range-checks it:
/// `SimConfig::pop_cap_max` is a bare `u32` and the simulation honours
/// whatever it is given, including 0 and `u32::MAX`.
///
/// That may well be correct — the range reads like a skirmish-setup slider,
/// and that screen is M6 — but until something enforces it, the requirement
/// is not met by the simulation, and this test records that rather than
/// leaving [GD-POP-02] looking covered.
///
/// REQ: GD-POP-02
#[test]
fn the_population_cap_is_not_range_checked() {
    for cap in [0, 1, 49, 201, u32::MAX] {
        let config = SimConfig {
            pop_cap_max: cap,
            ..SimConfig::default()
        };
        let mut sim = Simulation::new(3, config);
        run(&mut sim, 20);
        assert!(
            sim.check().is_ok(),
            "the simulation accepts pop_cap_max = {cap} without complaint"
        );
    }
}

/// REQ: GD-POP-03
#[test]
fn a_villager_costs_fifty_food() {
    assert_eq!(
        kinds::info(kinds::VILLAGER).cost,
        [50, 0, 0, 0],
        "50 food and nothing else"
    );
    assert_eq!(
        kinds::info(kinds::VILLAGER).pop_cost,
        1,
        "every unit costs one population except siege and elephants, neither of \
         which exists yet"
    );
}

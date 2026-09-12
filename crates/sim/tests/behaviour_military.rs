//! Military units and their buildings (`docs/02` §5, §6; `docs/06` M4).
//!
//! Training at the Barracks, Archery Range and Stable; the age and line
//! gates on the roster; the Axe line upgrade; and the technologies that
//! fold into the damage model.

mod common;

use common::*;
use sim::{
    kinds, tech, Command, CommandKind, Elevation, Fx, Item, SimConfig, Simulation, TrainError,
    Vec2Fx,
};

/// The default map with a stockpile deep enough to train and research
/// everything the tests ask for.
fn rich(seed: u64) -> Simulation {
    Simulation::new(
        seed,
        SimConfig {
            starting_stockpile: [5000, 5000, 5000, 5000],
            ..SimConfig::default()
        },
    )
}

/// A Stone Age settlement with a finished Barracks, Archery Range and
/// Stable beside the Town Center, and houses enough to train from all
/// three. Returns the three buildings.
fn garrison(sim: &mut Simulation) -> (sim::EntityId, sim::EntityId, sim::EntityId) {
    let tc = owned(sim, 0, kinds::TOWN_CENTER)[0];
    let at = pos_of(sim, tc);
    let place = |dx: i32, dy: i32| Vec2Fx::new(at.x + Fx::from_int(dx), at.y + Fx::from_int(dy));
    // The Storehouse is the second Stone Age building the Tool Age asks for.
    for (kind, d) in [
        (kinds::BARRACKS, (6, 0)),
        (kinds::ARCHERY_RANGE, (6, 4)),
        (kinds::STABLE, (-6, 0)),
        (kinds::STOREHOUSE, (-6, 4)),
        (kinds::HOUSE, (0, 6)),
        (kinds::HOUSE, (3, 6)),
        (kinds::HOUSE, (-3, 6)),
    ] {
        sim.issue(spawn(0, kind, place(d.0, d.1)));
    }
    run(sim, 3);
    let one = |sim: &Simulation, k| owned(sim, 0, k)[0];
    (
        one(sim, kinds::BARRACKS),
        one(sim, kinds::ARCHERY_RANGE),
        one(sim, kinds::STABLE),
    )
}

fn train(sim: &mut Simulation, building: sim::EntityId, kind: sim::KindId) {
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Train { building, kind },
    });
}

fn research(sim: &mut Simulation, building: sim::EntityId, tech: tech::TechId) {
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Research { building, tech },
    });
}

fn advance_to_tool(sim: &mut Simulation) {
    let tc = owned(sim, 0, kinds::TOWN_CENTER)[0];
    research(sim, tc, tech::AGE_TOOL);
    run(sim, tech::info(tech::AGE_TOOL).unwrap().ticks() + 5);
    assert_eq!(sim.player(0).unwrap().age, tech::Age::Tool);
}

/// REQ: RM-M4-01
#[test]
fn the_barracks_trains_a_clubman_for_fifty_food_and_it_walks_out() {
    let mut sim = rich(11);
    let (barracks, _, _) = garrison(&mut sim);
    let food = sim.player(0).unwrap().stockpile[0];
    let pop = sim.player(0).unwrap().pop;
    train(&mut sim, barracks, kinds::CLUBMAN);
    run(&mut sim, 3);
    assert_eq!(
        sim.player(0).unwrap().stockpile[0],
        food - 50,
        "paid on queueing"
    );
    assert!(owned(&sim, 0, kinds::CLUBMAN).is_empty(), "not out yet");
    run(&mut sim, kinds::info(kinds::CLUBMAN).build_ticks() + 2);
    let clubmen = owned(&sim, 0, kinds::CLUBMAN);
    assert_eq!(clubmen.len(), 1, "one Clubman trained");
    assert_eq!(sim.player(0).unwrap().pop, pop + 1);
    let at = pos_of(&sim, clubmen[0]);
    let b = pos_of(&sim, barracks);
    assert!(
        at.distance(b) < Fx::from_int(4),
        "stepped out beside the Barracks: {at:?}"
    );
    let i = index_of(&sim, clubmen[0]);
    assert_eq!(sim.world().health[i], Fx::from_int(40));
}

/// REQ: RM-M4-01
#[test]
fn each_building_trains_its_own_roster_and_refuses_the_rest() {
    let mut sim = rich(12);
    let (barracks, range, stable) = garrison(&mut sim);
    assert_eq!(
        sim.can_train(0, barracks, kinds::BOWMAN),
        Err(TrainError::WrongBuilding)
    );
    assert_eq!(
        sim.can_train(0, range, kinds::VILLAGER),
        Err(TrainError::WrongBuilding)
    );
    assert_eq!(
        sim.can_train(0, stable, kinds::TREE),
        Err(TrainError::UnknownKind)
    );
    assert_eq!(
        sim.can_train(1, barracks, kinds::CLUBMAN),
        Err(TrainError::NotYourBuilding)
    );
    assert_eq!(sim.can_train(0, barracks, kinds::CLUBMAN), Ok(()));
    // A refused command changes nothing.
    let stock = sim.player(0).unwrap().stockpile;
    train(&mut sim, barracks, kinds::BOWMAN);
    run(&mut sim, 3);
    assert_eq!(sim.player(0).unwrap().stockpile, stock);
    assert_eq!(
        sim.roster(0, kinds::BARRACKS),
        vec![kinds::CLUBMAN, kinds::AXEMAN, kinds::SPEARMAN]
    );
    assert_eq!(
        sim.roster(0, kinds::ARCHERY_RANGE),
        vec![kinds::SLINGER, kinds::BOWMAN]
    );
    assert_eq!(
        sim.roster(0, kinds::STABLE),
        vec![kinds::SCOUT, kinds::LIGHT_CAVALRY]
    );
    assert_eq!(sim.roster(0, kinds::TOWN_CENTER), vec![kinds::VILLAGER]);
    assert!(sim.roster(0, kinds::HOUSE).is_empty());
}

/// REQ: GD-AGE-01
#[test]
fn tool_age_units_wait_for_the_tool_age() {
    let mut sim = rich(13);
    let (barracks, range, stable) = garrison(&mut sim);
    for (b, k) in [
        (barracks, kinds::SPEARMAN),
        (range, kinds::BOWMAN),
        (range, kinds::SLINGER),
        (stable, kinds::LIGHT_CAVALRY),
    ] {
        assert_eq!(
            sim.can_train(0, b, k),
            Err(TrainError::AgeLocked {
                needs: tech::Age::Tool
            }),
            "{}",
            kinds::info(k).name
        );
    }
    assert_eq!(
        sim.can_train(0, stable, kinds::SCOUT),
        Ok(()),
        "the Scout is Stone Age"
    );
    advance_to_tool(&mut sim);
    for (b, k) in [
        (barracks, kinds::SPEARMAN),
        (range, kinds::BOWMAN),
        (range, kinds::SLINGER),
        (stable, kinds::LIGHT_CAVALRY),
    ] {
        assert_eq!(sim.can_train(0, b, k), Ok(()), "{}", kinds::info(k).name);
        train(&mut sim, b, k);
    }
    // The Range queues the Bowman and the Slinger one after the other.
    run(
        &mut sim,
        kinds::info(kinds::BOWMAN).build_ticks() + kinds::info(kinds::SLINGER).build_ticks() + 5,
    );
    for k in [
        kinds::SPEARMAN,
        kinds::BOWMAN,
        kinds::SLINGER,
        kinds::LIGHT_CAVALRY,
    ] {
        assert_eq!(owned(&sim, 0, k).len(), 1, "{}", kinds::info(k).name);
    }
}

/// The Axe upgrade turns every Clubman into an Axeman, the ones in the
/// queue included, and the Barracks trains Axemen from then on.
///
/// REQ: RM-M4-01
#[test]
fn the_axe_upgrade_moves_the_whole_clubman_line_on() {
    let mut sim = rich(14);
    let (barracks, _, _) = garrison(&mut sim);
    train(&mut sim, barracks, kinds::CLUBMAN);
    run(&mut sim, kinds::info(kinds::CLUBMAN).build_ticks() + 5);
    let club = owned(&sim, 0, kinds::CLUBMAN)[0];
    // Wound it, so the upgrade's hit points can be seen to carry over.
    let ci = index_of(&sim, club);
    let wounded = Fx::from_int(30);
    assert!(sim.world().health[ci] > wounded);
    advance_to_tool(&mut sim);
    assert_eq!(
        sim.can_train(0, barracks, kinds::AXEMAN),
        Err(TrainError::NeedsTech { tech: tech::AXE })
    );
    // One more Clubman queued while the Axe is researched.
    train(&mut sim, barracks, kinds::CLUBMAN);
    research(&mut sim, barracks, tech::AXE);
    run(&mut sim, 3);
    let queued: Vec<Item> = sim.world().production[index_of(&sim, barracks)]
        .as_ref()
        .unwrap()
        .queue
        .iter()
        .map(|q| q.item)
        .collect();
    assert_eq!(
        queued,
        vec![Item::Unit(kinds::CLUBMAN), Item::Tech(tech::AXE)]
    );
    // The Clubman comes out first; then the Axe completes.
    run(
        &mut sim,
        kinds::info(kinds::CLUBMAN).build_ticks() + tech::info(tech::AXE).unwrap().ticks() + 5,
    );
    assert!(sim.player(0).unwrap().has_researched(tech::AXE));
    assert!(
        owned(&sim, 0, kinds::CLUBMAN).is_empty(),
        "no Clubman is left"
    );
    let axemen = owned(&sim, 0, kinds::AXEMAN);
    assert_eq!(axemen.len(), 2, "both Clubmen became Axemen");
    let ai = index_of(&sim, club);
    assert_eq!(
        sim.world().kind[ai],
        kinds::AXEMAN,
        "the same entity, upgraded"
    );
    assert_eq!(
        sim.world().health[ai],
        Fx::from_int(50),
        "full Clubman health becomes full Axeman health"
    );
    assert_eq!(
        sim.can_train(0, barracks, kinds::CLUBMAN),
        Err(TrainError::Superseded { by: kinds::AXEMAN })
    );
    assert_eq!(sim.can_train(0, barracks, kinds::AXEMAN), Ok(()));
    assert_eq!(
        sim.roster(0, kinds::BARRACKS),
        vec![kinds::AXEMAN, kinds::SPEARMAN]
    );
    // A Clubman queued after the upgrade lands comes out an Axeman.
    train(&mut sim, barracks, kinds::AXEMAN);
    run(&mut sim, kinds::info(kinds::AXEMAN).build_ticks() + 5);
    assert_eq!(owned(&sim, 0, kinds::AXEMAN).len(), 3);
}

/// REQ: GD-COMBAT-02
#[test]
fn technology_changes_what_a_hit_does() {
    let mut sim = rich(15);
    let (barracks, range, _) = garrison(&mut sim);
    let store = owned(&sim, 0, kinds::STOREHOUSE)[0];
    advance_to_tool(&mut sim);
    let tc = pos_of(&sim, owned(&sim, 0, kinds::TOWN_CENTER)[0]);
    let here = |dx: i32| Vec2Fx::new(tc.x + Fx::from_int(dx), tc.y + Fx::from_int(3));
    sim.issue(spawn(0, kinds::CLUBMAN, here(0)));
    sim.issue(spawn(0, kinds::BOWMAN, here(1)));
    sim.issue(spawn(1, kinds::CLUBMAN, here(2)));
    sim.issue(spawn(1, kinds::LIGHT_CAVALRY, here(3)));
    run(&mut sim, 3);
    let club = owned(&sim, 0, kinds::CLUBMAN)[0];
    let bow = owned(&sim, 0, kinds::BOWMAN)[0];
    let enemy_club = owned(&sim, 1, kinds::CLUBMAN)[0];
    let enemy_cav = owned(&sim, 1, kinds::LIGHT_CAVALRY)[0];
    let tree = nearest_kind(&sim, kinds::TREE, tc);
    assert_eq!(sim.damage_between(club, enemy_club), Some(3));
    assert_eq!(sim.damage_between(bow, enemy_club), Some(5));
    assert_eq!(sim.damage_between(enemy_cav, club), Some(7));
    assert_eq!(
        sim.damage_between(club, tree),
        None,
        "a tree is not a target"
    );
    // Toolworking: +2 for infantry and cavalry, ours only.
    research(&mut sim, store, tech::TOOLWORKING);
    research(&mut sim, barracks, tech::LEATHER_ARMOUR);
    research(&mut sim, range, tech::FLETCHING);
    run(&mut sim, tech::info(tech::TOOLWORKING).unwrap().ticks() + 5);
    assert_eq!(sim.damage_between(club, enemy_club), Some(5), "Toolworking");
    assert_eq!(
        sim.damage_between(enemy_club, club),
        Some(2),
        "Leather Armour, theirs unchanged"
    );
    assert_eq!(
        sim.damage_between(bow, enemy_club),
        Some(6),
        "Fletching's +1 attack"
    );
    assert_eq!(
        sim.damage_between(enemy_cav, club),
        Some(6),
        "their cavalry meets our armour"
    );
    let m = sim.modifiers(0);
    assert_eq!(
        sim::combat::range_of(kinds::info(kinds::BOWMAN), &m),
        6,
        "Fletching's +1 range"
    );
    // Nothing does less than one, whatever the ground.
    for e in [Elevation::Uphill, Elevation::Level, Elevation::Downhill] {
        let d = sim::combat::damage(
            1,
            sim::DamageType::Melee,
            e,
            sim::Armour {
                melee: 9,
                pierce: 9,
            },
            0,
        );
        assert_eq!(d, 1);
    }
}

/// REQ: TA-DET-01
#[test]
fn a_training_and_upgrading_match_replays_identically() {
    let build = || {
        let mut sim = rich(16);
        let (barracks, range, stable) = garrison(&mut sim);
        advance_to_tool(&mut sim);
        for k in [kinds::CLUBMAN, kinds::SPEARMAN, kinds::CLUBMAN] {
            train(&mut sim, barracks, k);
        }
        train(&mut sim, range, kinds::BOWMAN);
        train(&mut sim, stable, kinds::LIGHT_CAVALRY);
        research(&mut sim, barracks, tech::AXE);
        // Three units and the Axe, one after the other at the Barracks.
        run(&mut sim, 20 * (26 * 3 + 40) + 10);
        sim
    };
    let a = build();
    let b = build();
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(owned(&a, 0, kinds::AXEMAN).len(), 2);
    assert_eq!(owned(&a, 0, kinds::CLUBMAN).len(), 0);
}

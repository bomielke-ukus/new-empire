//! The learning campaign (`docs/03` §7, `docs/07` D35), played through by
//! a plain bot: villagers kept at work, buildings put down near where they
//! are wanted, soldiers sent at what the scenario sends. Each scenario is
//! won by doing what it asks, in about the time a new player would take,
//! and its raids come when it says.

mod common;

use common::bot::*;
use common::*;
use sim::kinds::{self, Resource};
use sim::scenario::ObjectiveStatus;
use sim::{tech, CommandKind, Outcome, Rally};

/// Scenario 1 teaches gathering and building: each objective appears when
/// the one before it is done, in the order a village grows.
///
/// REQ: GD-CAMP-05
#[test]
fn hunters_on_the_bank_is_won_by_gathering_and_building() {
    let mut b = Bot::new("learning", "hunters");
    assert_eq!(b.status("wood"), ObjectiveStatus::Open);
    assert_eq!(b.status("house"), ObjectiveStatus::Hidden);
    let tc = b.mine(kinds::TOWN_CENTER)[0];
    let home = b.tile_of(tc);
    b.assign(Resource::Wood);
    b.until(3 * MINUTE, "wood", |b| {
        b.status("wood") == ObjectiveStatus::Done
    });
    b.until(2 * SECOND, "the house shown", |b| {
        b.status("house") == ObjectiveStatus::Open
    });
    b.build(kinds::HOUSE, (home.0 - 4, home.1 + 1), 1);
    b.until(2 * MINUTE, "house", |b| {
        b.status("house") == ObjectiveStatus::Done
    });
    // New villagers go to the berries.
    let bush = nearest_kind(&b.sim, kinds::BERRY_BUSH, pos_of(&b.sim, tc));
    b.issue(CommandKind::SetRally {
        building: tc,
        rally: Rally::Entity(bush),
    });
    b.train(kinds::TOWN_CENTER, kinds::VILLAGER, 4);
    b.until(4 * MINUTE, "people", |b| {
        b.status("people") == ObjectiveStatus::Done
    });
    b.assign(Resource::Food);
    // The storehouse by the far forest, and two to cut there.
    b.until(4 * MINUTE, "wood for the storehouse", |b| {
        b.sim.player(ME).unwrap().stockpile[Resource::Wood.index()] >= 100
    });
    b.build(kinds::STOREHOUSE, (44, 16), 2);
    b.until(3 * MINUTE, "store", |b| {
        b.status("store") == ObjectiveStatus::Done
    });
    b.set_job(3, Resource::Food);
    b.until(5 * MINUTE, "food", |b| b.sim.outcome().is_some());
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
    eprintln!("hunters won at {:.1} minutes", b.minutes());
    assert!(b.minutes() < 15.0);
}

/// Scenario 2 teaches advancing an age: two Stone Age buildings and 400
/// food, the advance at the Town Center, then what the Tool Age opens.
///
/// REQ: GD-CAMP-05
#[test]
fn the_black_land_is_won_by_advancing_to_the_tool_age() {
    let mut b = Bot::new("learning", "black-land");
    let tc = b.mine(kinds::TOWN_CENTER)[0];
    let home = b.tile_of(tc);
    b.assign(Resource::Food);
    b.build(kinds::STOREHOUSE, (40, 40), 1);
    b.build(kinds::BARRACKS, (home.0 - 5, home.1 + 4), 1);
    b.until(3 * MINUTE, "the two buildings", |b| {
        b.status("store") == ObjectiveStatus::Done && b.status("barracks") == ObjectiveStatus::Done
    });
    b.set_job(2, Resource::Wood);
    b.until(5 * MINUTE, "400 food", |b| {
        b.sim.player(ME).unwrap().stockpile[Resource::Food.index()] >= 400
    });
    b.issue(CommandKind::Research {
        building: tc,
        tech: tech::AGE_TOOL,
    });
    b.until(2 * MINUTE, "tool", |b| {
        b.status("tool") == ObjectiveStatus::Done
    });
    assert_eq!(
        b.status("farms"),
        ObjectiveStatus::Open,
        "the farms are shown"
    );
    for n in 0..3 {
        b.until(4 * MINUTE, "wood for a farm", |b| {
            b.sim.player(ME).unwrap().stockpile[Resource::Wood.index()] >= 75
        });
        b.build(kinds::FARM, (home.0 + 4, home.1 - 3 + n * 3), 1);
        b.until(10 * SECOND, "the farm laid out", |b| {
            owned(&b.sim, ME, kinds::FARM).len() > n as usize
        });
    }
    b.until(4 * MINUTE, "farms", |b| b.sim.outcome().is_some());
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
    eprintln!("the black land won at {:.1} minutes", b.minutes());
    assert!(b.minutes() < 18.0);
}

/// Scenario 3 teaches combat: soldiers trained, a raid beaten at home,
/// then the raiders' camp destroyed. The raid comes once the soldiers are
/// ready, and the Town Center's loss would lose it.
///
/// REQ: GD-CAMP-05
#[test]
fn raiders_from_the_west_is_won_by_beating_the_raid_and_burning_the_camp() {
    let mut b = Bot::new("learning", "raiders");
    let tc = b.mine(kinds::TOWN_CENTER)[0];
    let home = b.tile_of(tc);
    b.assign(Resource::Food);
    b.set_job(3, Resource::Wood);
    b.train(kinds::BARRACKS, kinds::CLUBMAN, 3);
    assert_eq!(b.standing(1), 0, "no raiders before the soldiers are ready");
    b.until(3 * MINUTE, "army", |b| {
        b.status("army") == ObjectiveStatus::Done
    });
    b.until(2 * SECOND, "the raid", |b| b.standing(1) == 4);
    let army = b.soldiers(&[kinds::CLUBMAN]);
    b.attack_move(army, (home.0 - 3, home.1 + 1));
    b.train(kinds::BARRACKS, kinds::CLUBMAN, 3);
    b.until(3 * MINUTE, "raid", |b| {
        b.status("raid") == ObjectiveStatus::Done
    });
    assert_eq!(b.status("camp"), ObjectiveStatus::Open);
    b.until(2 * MINUTE, "the second training", |b| {
        b.mine(kinds::CLUBMAN).len() >= 6
    });
    let army = b.soldiers(&[kinds::CLUBMAN]);
    eprintln!("raiders: {} clubmen go to the camp", army.len());
    b.attack_move(army, (9, 10));
    b.fight(6 * MINUTE, "camp", &[1, 2], |b| b.sim.outcome().is_some());
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
    eprintln!("raiders won at {:.1} minutes", b.minutes());
}

/// Scenario 4 teaches counters: spearmen against the Hyksos riders,
/// slingers against their axemen, then both against Avaris.
///
/// REQ: GD-CAMP-05
#[test]
fn spears_against_horses_is_won_by_answering_each_enemy_with_its_counter() {
    let mut b = Bot::new("learning", "horses");
    let tc = b.mine(kinds::TOWN_CENTER)[0];
    let home = b.tile_of(tc);
    b.assign(Resource::Food);
    b.set_job(4, Resource::Wood);
    b.train(kinds::BARRACKS, kinds::SPEARMAN, 4);
    b.until(3 * MINUTE, "spears", |b| {
        b.status("spears") == ObjectiveStatus::Done
    });
    b.until(2 * SECOND, "the riders", |b| b.standing(1) == 3);
    let spears = b.soldiers(&[kinds::SPEARMAN]);
    b.attack_move(spears, (home.0 + 1, home.1 - 4));
    b.until(3 * MINUTE, "horses", |b| {
        b.status("horses") == ObjectiveStatus::Done
    });
    let spears_left = b.mine(kinds::SPEARMAN).len();
    eprintln!("horses: {spears_left} of 6 spearmen stand after the riders");
    assert!(spears_left >= 3, "the counter wins clearly");
    // A queue holds five.
    b.train(kinds::ARCHERY_RANGE, kinds::SLINGER, 5);
    b.until(3 * MINUTE, "five slingers", |b| {
        !b.mine(kinds::SLINGER).is_empty()
    });
    b.train(kinds::ARCHERY_RANGE, kinds::SLINGER, 1);
    b.until(4 * MINUTE, "slings", |b| {
        b.status("slings") == ObjectiveStatus::Done
    });
    b.until(2 * SECOND, "the axemen", |b| b.standing(1) == 5);
    let army = b.soldiers(&[kinds::SPEARMAN, kinds::SLINGER]);
    b.attack_move(army, (home.0 + 1, home.1 - 4));
    b.until(3 * MINUTE, "foot", |b| {
        b.status("foot") == ObjectiveStatus::Done
    });
    let slings_left = b.mine(kinds::SLINGER).len();
    eprintln!("horses: {slings_left} of 6 slingers stand after the axemen");
    assert!(slings_left >= 3, "the counter wins clearly");
    // A mixed army for Avaris.
    b.train(kinds::BARRACKS, kinds::SPEARMAN, 4);
    b.train(kinds::ARCHERY_RANGE, kinds::SLINGER, 4);
    b.until(4 * MINUTE, "the army for Avaris", |b| {
        b.mine(kinds::SPEARMAN).len() + b.mine(kinds::SLINGER).len() >= 12
    });
    let army = b.soldiers(&[kinds::SPEARMAN, kinds::SLINGER]);
    eprintln!("horses: {} go to Avaris", army.len());
    b.attack_move(army, (34, 9));
    b.fight(6 * MINUTE, "avaris", &[1, 2], |b| b.sim.outcome().is_some());
    assert_eq!(b.sim.outcome(), Some(&Outcome::Won));
    eprintln!("spears against horses won at {:.1} minutes", b.minutes());
}

/// A player who has not trained the soldiers asked for is raided all the
/// same at five minutes, and the objective is not marked done for them.
///
/// REQ: GD-CAMP-03
#[test]
fn the_raid_comes_at_five_minutes_whether_or_not_the_army_is_ready() {
    let mut b = Bot::new("learning", "raiders");
    run(&mut b.sim, 300 * SECOND - 2 * SECOND);
    assert_eq!(b.standing(1), 0);
    run(&mut b.sim, 3 * SECOND);
    assert_eq!(b.standing(1), 4, "the raid came on time");
    assert_eq!(b.status("raid"), ObjectiveStatus::Open);
    assert_eq!(
        b.status("army"),
        ObjectiveStatus::Open,
        "two clubmen are not five"
    );
}

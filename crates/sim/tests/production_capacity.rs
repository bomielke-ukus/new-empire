//! Paid production must survive a full entity store. Drive commands and
//! check the queue, money, rally and invariant after capacity is released.
mod common;
use common::{owned, run, spawn};
use sim::kinds;
use sim::{
    Command, CommandKind, EntityId, Item, MapKind, MapSpec, Order, Rally, SimConfig, Simulation,
    Vec2Fx,
};

fn full_store() -> (Simulation, EntityId, EntityId) {
    let mut sim = Simulation::new(
        1,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Flat,
                size: 48,
                players: 1,
            },
            max_entities: 3,
            wander: false,
            ..SimConfig::default()
        },
    );
    for (kind, x) in [
        (kinds::TOWN_CENTER, 10),
        (kinds::SCOUT, 20),
        (kinds::SCOUT, 24),
    ] {
        sim.issue(spawn(0, kind, Vec2Fx::from_int(x, 10)));
    }
    run(&mut sim, 3);
    let tc = owned(&sim, 0, kinds::TOWN_CENTER)[0];
    let blocker = owned(&sim, 0, kinds::SCOUT)[0];
    assert_eq!(sim.world().len(), 3);
    assert!(sim.player(0).unwrap().pop < sim.player(0).unwrap().pop_cap);
    (sim, tc, blocker)
}

fn issue(sim: &mut Simulation, kind: CommandKind) {
    sim.issue(Command { player: 0, kind });
}

#[test]
fn paid_training_waits_at_capacity_and_spawns_once_when_a_slot_opens() {
    let (mut sim, tc, blocker) = full_store();
    let before = sim.player(0).unwrap().stockpile;
    let target = Vec2Fx::from_int(30, 30);
    issue(
        &mut sim,
        CommandKind::SetRally {
            building: tc,
            rally: Rally::Point(target),
        },
    );
    for _ in 0..2 {
        issue(
            &mut sim,
            CommandKind::Train {
                building: tc,
                kind: kinds::VILLAGER,
            },
        );
    }
    let duration = kinds::info(kinds::VILLAGER).build_ticks();
    run(&mut sim, duration * 2 + 10);
    let q = &sim.world().production[tc.index()].as_ref().unwrap().queue;
    assert_eq!(q.len(), 2, "neither paid item may be lost while full");
    assert_eq!(q[0].item, Item::Unit(kinds::VILLAGER));
    assert_eq!(q[0].progress, duration);
    assert_eq!(q[1].progress, 0, "the tail waits behind the head");
    assert_eq!(sim.player(0).unwrap().stockpile[0], before[0] - 100);
    sim.check().unwrap();
    issue(&mut sim, CommandKind::Despawn { id: blocker });
    run(&mut sim, 3);
    let villagers = owned(&sim, 0, kinds::VILLAGER);
    assert_eq!(villagers.len(), 1);
    assert_eq!(
        sim.world().order[villagers[0].index()],
        Order::Move { target }
    );
    assert_eq!(sim.world().len(), 3);
    run(&mut sim, duration + 10);
    assert_eq!(owned(&sim, 0, kinds::VILLAGER).len(), 1);
    assert_eq!(
        sim.world().production[tc.index()]
            .as_ref()
            .unwrap()
            .queue
            .len(),
        1
    );
    assert_eq!(sim.player(0).unwrap().stockpile[0], before[0] - 100);
    sim.check().unwrap();
    assert!(
        sim.replay().verify().is_ok(),
        "capacity recovery replays deterministically"
    );
}

#[test]
fn cancelling_completed_training_at_capacity_refunds_it_in_full() {
    let (mut sim, tc, _) = full_store();
    let before = sim.player(0).unwrap().stockpile;
    issue(
        &mut sim,
        CommandKind::Train {
            building: tc,
            kind: kinds::VILLAGER,
        },
    );
    run(&mut sim, kinds::info(kinds::VILLAGER).build_ticks() + 10);
    issue(&mut sim, CommandKind::CancelTrain { building: tc });
    run(&mut sim, 3);
    assert_eq!(sim.player(0).unwrap().stockpile, before);
    assert!(sim.world().production[tc.index()]
        .as_ref()
        .unwrap()
        .queue
        .is_empty());
    assert!(owned(&sim, 0, kinds::VILLAGER).is_empty());
    sim.check().unwrap();
}

//! Waypoints (`docs/03` `UX-CMD-04`): Shift-queued jobs of any kind,
//! taken in order as each finishes; a job given outright, or a stop, ends
//! the queue; a group keeps its formation through queued moves.

mod common;

use common::*;
use sim::{kinds, Command, CommandError, CommandKind, Fx, Order, SimConfig, Simulation, Vec2Fx};

fn arena() -> Simulation {
    Simulation::new(
        11,
        SimConfig {
            map: sim::MapSpec {
                kind: sim::MapKind::Flat,
                size: 64,
                players: 2,
            },
            wander: false,
            starting_stockpile: [5000; 4],
            ..SimConfig::default()
        },
    )
}

fn at(x: i32, y: i32) -> Vec2Fx {
    sim::nav::centre((x, y))
}

fn cmd(kind: CommandKind) -> Command {
    Command { player: 0, kind }
}

fn queued(kind: CommandKind) -> Command {
    cmd(CommandKind::Queued(Box::new(kind)))
}

fn unit(sim: &mut Simulation, kind: sim::KindId, x: i32, y: i32) -> sim::EntityId {
    let before = owned(sim, 0, kind).len();
    sim.issue(spawn(0, kind, at(x, y)));
    run(sim, 3);
    owned(sim, 0, kind)[before]
}

fn near(sim: &Simulation, id: sim::EntityId, x: i32, y: i32) -> bool {
    (pos_of(sim, id) - at(x, y)).length() < Fx::from_int(2)
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

/// Three points queued are visited in order, the first taken at once by
/// an idle unit; a job given outright while two remain ends them; a stop
/// ends them too; and a waypoint may hold only a job.
/// REQ: UX-CMD-04
#[test]
fn queued_moves_are_taken_in_order_and_a_direct_order_ends_the_queue() {
    let mut sim = arena();
    let v = unit(&mut sim, kinds::VILLAGER, 10, 10);
    let vi = index_of(&sim, v);
    for (x, y) in [(16, 10), (16, 16), (10, 16)] {
        sim.issue(queued(CommandKind::Move {
            ids: vec![v],
            target: at(x, y),
        }));
    }
    run(&mut sim, 3);
    assert!(
        matches!(sim.world().order[vi], Order::Move { .. }),
        "the first taken at once"
    );
    assert_eq!(sim.world().queue_at(vi).len(), 2, "two behind it");
    assert!(
        run_until(&mut sim, 600, |s| s.world().queue_at(vi).len() == 1),
        "the second taken"
    );
    assert!(
        near(&sim, v, 16, 10),
        "at the first point: {:?}",
        pos_of(&sim, v)
    );
    assert!(
        run_until(&mut sim, 600, |s| s.world().queue_at(vi).is_empty()),
        "the third taken"
    );
    assert!(
        near(&sim, v, 16, 16),
        "at the second: {:?}",
        pos_of(&sim, v)
    );
    assert!(
        run_until(&mut sim, 600, |s| s.world().order[vi] == Order::Idle),
        "and done"
    );
    assert!(near(&sim, v, 10, 16), "at the third: {:?}", pos_of(&sim, v));

    // A job given outright ends what was queued.
    for (x, y) in [(20, 16), (20, 20)] {
        sim.issue(queued(CommandKind::Move {
            ids: vec![v],
            target: at(x, y),
        }));
    }
    run(&mut sim, 3);
    assert_eq!(sim.world().queue_at(vi).len(), 1);
    sim.issue(cmd(CommandKind::Move {
        ids: vec![v],
        target: at(10, 10),
    }));
    run(&mut sim, 3);
    assert!(
        sim.world().queue_at(vi).is_empty(),
        "cleared by the direct order"
    );
    assert!(run_until(&mut sim, 600, |s| s.world().order[vi] == Order::Idle));
    assert!(near(&sim, v, 10, 10));

    // A stop ends them too.
    for (x, y) in [(20, 16), (20, 20)] {
        sim.issue(queued(CommandKind::Move {
            ids: vec![v],
            target: at(x, y),
        }));
    }
    run(&mut sim, 3);
    sim.issue(cmd(CommandKind::Stop { ids: vec![v] }));
    run(&mut sim, 3);
    assert_eq!(sim.world().order[vi], Order::Idle);
    assert!(sim.world().queue_at(vi).is_empty(), "cleared by the stop");

    // Only a job can be a waypoint.
    let bad = queued(CommandKind::Stop { ids: vec![v] });
    assert_eq!(bad.validate(), Err(CommandError::NotQueueable));
    let nested = queued(CommandKind::Queued(Box::new(CommandKind::Move {
        ids: vec![v],
        target: at(1, 1),
    })));
    assert_eq!(nested.validate(), Err(CommandError::NotQueueable));
    sim.check().unwrap();
}

/// Mixed kinds queue (`UX-CMD-04`): a walk, then a build whose site is
/// placed and paid the moment it is queued, then a gather; the villager
/// does each in turn. A patrol queued after a walk starts from where the
/// walk ended.
/// REQ: UX-CMD-04
#[test]
fn mixed_jobs_queue_and_a_queued_building_is_placed_at_once() {
    let mut sim = arena();
    let v = unit(&mut sim, kinds::VILLAGER, 10, 10);
    let vi = index_of(&sim, v);
    let bush = unit(&mut sim, kinds::BERRY_BUSH, 24, 10);
    let wood = sim.player(0).unwrap().stockpile[1];
    sim.issue(cmd(CommandKind::Move {
        ids: vec![v],
        target: at(14, 10),
    }));
    sim.issue(queued(CommandKind::Build {
        kind: kinds::HOUSE,
        x: 18,
        y: 10,
        ids: vec![v],
    }));
    sim.issue(queued(CommandKind::Gather {
        ids: vec![v],
        node: bush,
    }));
    run(&mut sim, 3);
    assert_eq!(
        sim.player(0).unwrap().stockpile[1],
        wood - kinds::info(kinds::HOUSE).cost[1],
        "the site is paid when queued"
    );
    assert_eq!(owned(&sim, 0, kinds::HOUSE).len(), 1, "and placed");
    assert_eq!(sim.world().queue_at(vi).len(), 2);
    assert!(
        run_until(&mut sim, 900, |s| matches!(
            s.world().order[index_of(s, v)],
            Order::Build { working: true, .. }
        )),
        "walked there and set to work"
    );
    assert!(
        run_until(&mut sim, 3000, |s| matches!(
            s.world().order[index_of(s, v)],
            Order::Gather { .. }
        )),
        "built it and went to gather"
    );
    assert_eq!(sim.world().queue_at(vi).len(), 0);
    let house = owned(&sim, 0, kinds::HOUSE)[0];
    assert!(
        sim.world().construction[index_of(&sim, house)].is_none(),
        "the house stands"
    );

    // A patrol after a walk starts where the walk ended.
    let s = unit(&mut sim, kinds::CLUBMAN, 30, 30);
    let si = index_of(&sim, s);
    sim.issue(cmd(CommandKind::Move {
        ids: vec![s],
        target: at(34, 30),
    }));
    sim.issue(queued(CommandKind::Patrol {
        ids: vec![s],
        target: at(34, 38),
    }));
    assert!(run_until(&mut sim, 600, |sm| matches!(
        sm.world().order[si],
        Order::Patrol { .. }
    )));
    if let Order::Patrol { from, .. } = sim.world().order[si] {
        assert!(
            (from - at(34, 30)).length() < Fx::from_int(2),
            "from {from:?}"
        );
    }
    sim.check().unwrap();
}

/// A group keeps its formation through a queued move: two soldiers get
/// two different points, as they would from a direct order.
/// REQ: UX-CMD-04
#[test]
fn a_queued_group_move_keeps_the_formation() {
    let mut sim = arena();
    let a = unit(&mut sim, kinds::CLUBMAN, 10, 10);
    let b = unit(&mut sim, kinds::CLUBMAN, 11, 10);
    sim.issue(cmd(CommandKind::Move {
        ids: vec![a, b],
        target: at(10, 14),
    }));
    sim.issue(queued(CommandKind::Move {
        ids: vec![a, b],
        target: at(20, 20),
    }));
    run(&mut sim, 3);
    let goals: Vec<Vec2Fx> = [a, b]
        .iter()
        .map(
            |u| match sim.world().queue_at(index_of(&sim, *u))[0].order {
                Order::Move { target } => target,
                o => panic!("{o:?}"),
            },
        )
        .collect();
    assert_ne!(goals[0], goals[1], "each has its slot in the formation");
    sim.check().unwrap();
}

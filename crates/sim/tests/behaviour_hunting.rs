//! Hunting (`docs/02` `GD-ECON-06`, `docs/07` D15): an animal is food
//! only once killed; it runs when hit; the carcass lies where it fell,
//! gathered by villagers until it is taken or, after a while, gone.

mod common;

use common::*;
use sim::{kinds, Command, CommandKind, Order, Simulation, Then, CARCASS_TICKS};

fn run_until(sim: &mut Simulation, limit: u32, done: impl Fn(&Simulation) -> bool) -> bool {
    for _ in 0..limit {
        if done(sim) {
            return true;
        }
        sim.step();
    }
    done(sim)
}

fn live(sim: &Simulation, id: sim::EntityId) -> bool {
    sim.world().slot(id).is_some()
}

fn carcass(sim: &Simulation, id: sim::EntityId) -> bool {
    sim.world()
        .slot(id)
        .is_some_and(|s| sim.world().dying[s.index()] > 0)
}

/// A villager sent at a gazelle hunts it: the animal runs when hit, the
/// villager follows and kills it, and without another word gathers the
/// carcass and carries the food home. Hunted animals do not respawn.
/// REQ: GD-ECON-06
/// REQ: GD-ECON-01
#[test]
fn a_villager_hunts_a_gazelle_and_gathers_its_carcass() {
    let mut sim = inland(3);
    let v = owned(&sim, 0, kinds::VILLAGER)[0];
    let vi = index_of(&sim, v);
    let home = pos_of(&sim, v);
    let gazelle = nearest_kind(&sim, kinds::GAZELLE, home);
    let gazelles_before = sim
        .world()
        .slots()
        .filter(|s| sim.world().kind[s.index()] == kinds::GAZELLE)
        .count();
    let start = pos_of(&sim, gazelle);
    let food_before = sim.player(0).unwrap().stockpile[0];
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![v],
            target: gazelle,
        },
    });
    run(&mut sim, 3);
    assert!(
        matches!(
            sim.world().order[vi],
            Order::Attack {
                then: Then::Hunt(a),
                ..
            } if a == gazelle
        ),
        "a hunt: {:?}",
        sim.world().order[vi]
    );
    // The first hit sends it running.
    assert!(
        run_until(&mut sim, 1200, |s| s.world().health[index_of(s, gazelle)]
            < sim::Fx::from_int(kinds::info(kinds::GAZELLE).max_health)),
        "hit it"
    );
    let gi = index_of(&sim, gazelle);
    assert!(sim.world().move_target[gi].is_some(), "and it runs");
    // Then the kill, the carcass, and the gathering.
    assert!(run_until(&mut sim, 4000, |s| carcass(s, gazelle)), "killed");
    let gi = index_of(&sim, gazelle);
    assert_eq!(
        sim.world().resource[gi],
        kinds::info(kinds::GAZELLE).resource.unwrap().1,
        "its food is on the carcass"
    );
    let fell = pos_of(&sim, gazelle);
    assert!(
        run_until(&mut sim, 200, |s| matches!(
            s.world().order[index_of(s, v)],
            Order::Gather { node, .. } if node == gazelle
        )),
        "the hunter gathers it without being told"
    );
    assert!(
        run_until(&mut sim, 1500, |s| s.world().carry[index_of(s, v)]
            .is_some()),
        "carrying food"
    );
    assert_eq!(
        pos_of(&sim, gazelle),
        fell,
        "the carcass lies where it fell"
    );
    assert!(
        run_until(&mut sim, 3000, |s| s.player(0).unwrap().stockpile[0]
            > food_before),
        "and food came home"
    );
    let gazelles_after = sim
        .world()
        .slots()
        .filter(|s| {
            sim.world().kind[s.index()] == kinds::GAZELLE && sim.world().dying[s.index()] == 0
        })
        .count();
    assert_eq!(gazelles_after, gazelles_before - 1, "one fewer, for good");
    assert_ne!(start, fell, "it did not die where it stood");
    sim.check().unwrap();
}

/// A carcass nobody gathers is gone after its time; a villager sent to a
/// carcass gathers it as a node, and while one does, it does not rot.
/// REQ: GD-ECON-06
#[test]
fn a_carcass_left_alone_decays_and_one_sent_for_is_gathered() {
    let mut sim = inland(3);
    let v = owned(&sim, 0, kinds::VILLAGER)[0];
    let home = pos_of(&sim, v);
    let gazelle = nearest_kind(&sim, kinds::GAZELLE, home);
    // A soldier makes the kill and stands down; the carcass is nobody's.
    sim.issue(spawn(0, kinds::CLUBMAN, pos_of(&sim, gazelle)));
    run(&mut sim, 3);
    let club = owned(&sim, 0, kinds::CLUBMAN)[0];
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![club],
            target: gazelle,
        },
    });
    assert!(run_until(&mut sim, 4000, |s| carcass(s, gazelle)), "killed");
    run(&mut sim, 2);
    assert_eq!(
        sim.world().order[index_of(&sim, club)],
        Order::Idle,
        "a soldier does not gather"
    );
    // Sent for it, a villager gathers the carcass as it would a bush.
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Gather {
            ids: vec![v],
            node: gazelle,
        },
    });
    run(&mut sim, 3);
    assert!(matches!(
        sim.world().order[index_of(&sim, v)],
        Order::Gather { node, .. } if node == gazelle
    ));
    // While it is gathered its clock stands still ("decays if left").
    let gi = index_of(&sim, gazelle);
    let held = sim.world().dying[gi];
    run(&mut sim, 300);
    assert!(
        matches!(
            sim.world().order[index_of(&sim, v)],
            Order::Gather { node, .. } if node == gazelle
        ),
        "still at it"
    );
    assert_eq!(sim.world().dying[gi], held, "no rot while tended");
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Stop { ids: vec![v] },
    });
    // Left alone, it is gone when its time is up, not before.
    let gi = index_of(&sim, gazelle);
    let left = sim.world().dying[gi] as u32;
    assert!(left > 0 && left <= CARCASS_TICKS as u32);
    run(&mut sim, left - 5);
    assert!(carcass(&sim, gazelle), "still lying");
    run(&mut sim, 10);
    assert!(!live(&sim, gazelle), "gone");
    sim.check().unwrap();
}

/// Ticks a villager at `node` takes to fill its hands from empty, once it
/// is working there.
fn ticks_to_fill(sim: &mut Simulation, v: sim::EntityId, node: sim::EntityId) -> u32 {
    assert!(
        run_until(sim, 2000, |s| matches!(
            s.world().order[index_of(s, v)],
            Order::Gather { node: n, phase: sim::GatherPhase::Working, .. } if n == node
        ) && s.world().carry[index_of(s, v)]
            .is_none_or(|(_, a)| a == 0)),
        "working at it, empty-handed"
    );
    let mut ticks = 0;
    while sim.world().carry[index_of(sim, v)].is_none_or(|(_, a)| a < 10) {
        sim.step();
        ticks += 1;
        assert!(ticks < 2000, "never filled");
    }
    ticks
}

/// Hunting pays from the first minute (`GD-ECON-06`): every start has
/// four gazelles within ten tiles of its Town Center; a villager kills one in
/// two hits, it bolts a short way and no more; and meat comes off the
/// carcass faster than berries off a bush.
/// REQ: GD-ECON-06
/// REQ: GD-ECON-02
#[test]
fn the_home_herd_is_near_dies_in_two_hits_and_its_meat_comes_fast() {
    let mut sim = inland(3);
    for player in 0..2u8 {
        let tc = pos_of(&sim, owned(&sim, player, kinds::TOWN_CENTER)[0]);
        let near = sim
            .world()
            .slots()
            .filter(|s| {
                let i = s.index();
                sim.world().kind[i] == kinds::GAZELLE
                    && (sim.world().pos[i] - tc).length() <= sim::Fx::from_int(10)
            })
            .count();
        assert!(near >= 4, "player {player}: {near} gazelles near home");
    }
    let v = owned(&sim, 0, kinds::VILLAGER)[0];
    let home = pos_of(&sim, v);
    let gazelle = nearest_kind(&sim, kinds::GAZELLE, home);
    let start = pos_of(&sim, gazelle);
    let hits = std::cell::Cell::new(0);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Attack {
            ids: vec![v],
            target: gazelle,
        },
    });
    let last = std::cell::Cell::new(sim::Fx::from_int(kinds::info(kinds::GAZELLE).max_health));
    assert!(
        run_until(&mut sim, 2000, |s| {
            let h = s.world().health[index_of(s, gazelle)];
            if h < last.get() {
                hits.set(hits.get() + 1);
                last.set(h);
            }
            carcass(s, gazelle)
        }),
        "killed"
    );
    assert_eq!(hits.get(), 2, "two hits");
    let fell = pos_of(&sim, gazelle);
    assert!(
        (fell - start).length() <= sim::Fx::from_int(sim::BOLT_TILES + 1),
        "it bolted a short way: {:?} from {:?}",
        fell,
        start
    );
    let meat = ticks_to_fill(&mut sim, v, gazelle);
    // The same villager's work at a bush, from empty hands.
    let bush = nearest_kind(&sim, kinds::BERRY_BUSH, home);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::Gather {
            ids: vec![v],
            node: bush,
        },
    });
    let berries = ticks_to_fill(&mut sim, v, bush);
    // 10 food at 0.75 a second is 267 ticks; at 0.45, 445.
    assert!(meat < 280, "meat: {meat} ticks");
    assert!(berries > 430, "berries: {berries} ticks");
    sim.check().unwrap();
}

//! Fighting (`docs/02` §8, §8.1; `docs/03` `UX-CMD-02`, `-03`, `-07`,
//! `-08`): attack orders, hits and projectiles, stances, formations,
//! death, and villagers running for it.

mod common;

use common::*;
use sim::{
    kinds, Command, CommandKind, Event, Formation, Fx, Order, SimConfig, Simulation, Stance,
    Vec2Fx, DECAY_TICKS,
};

/// A flat, empty map: nothing but what the test spawns, so no start kit
/// scout wanders into a fight.
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

fn cmd(player: u8, kind: CommandKind) -> Command {
    Command { player, kind }
}

fn health(sim: &Simulation, id: sim::EntityId) -> Fx {
    sim.world().health[index_of(sim, id)]
}

fn alive(sim: &Simulation, id: sim::EntityId) -> bool {
    sim.world()
        .slot(id)
        .is_some_and(|s| sim.world().dying[s.index()] == 0)
}

fn stance(sim: &mut Simulation, player: u8, ids: Vec<sim::EntityId>, stance: Stance) {
    sim.issue(cmd(player, CommandKind::SetStance { ids, stance }));
}

#[test]
fn an_attack_order_closes_hits_on_the_reload_and_kills() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::AXEMAN, at(10, 10)));
    sim.issue(spawn(1, kinds::CLUBMAN, at(18, 10)));
    run(&mut sim, 3);
    let (mine, theirs) = (
        owned(&sim, 0, kinds::AXEMAN)[0],
        owned(&sim, 1, kinds::CLUBMAN)[0],
    );
    // The other one stands its ground: it hits back, but never runs.
    stance(&mut sim, 1, vec![theirs], Stance::StandGround);
    sim.issue(cmd(
        0,
        CommandKind::Attack {
            ids: vec![mine],
            target: theirs,
        },
    ));
    run(&mut sim, 3);
    assert!(matches!(
        sim.world().order[index_of(&sim, mine)],
        Order::Attack { target, .. } if target == theirs
    ));
    // Walks the eight tiles, then lands 5 a hit every thirty ticks.
    let mut ticks_to_first_hit = None;
    for t in 0..400 {
        sim.step();
        if health(&sim, theirs) < Fx::from_int(40) {
            ticks_to_first_hit = Some(t);
            break;
        }
    }
    let first = ticks_to_first_hit.expect("a hit landed");
    assert!(first > 60 && first < 300, "walked first: {first}");
    assert_eq!(health(&sim, theirs), Fx::from_int(35), "one hit of 5");
    run(&mut sim, kinds::RELOAD_TICKS);
    assert_eq!(
        health(&sim, theirs),
        Fx::from_int(30),
        "the next hit on the reload"
    );
    // Eight hits kill a Clubman; it hits back for 3 each time. Then the
    // corpse lies for a while.
    run(&mut sim, kinds::RELOAD_TICKS * 7);
    assert!(!alive(&sim, theirs), "dead");
    assert!(alive(&sim, mine), "and the Axeman took its blows");
    assert!(health(&sim, mine) < Fx::from_int(50), "some of them");
    assert!(
        sim.world().slot(theirs).is_some(),
        "the corpse is still there"
    );
    assert_eq!(
        sim.player(1).unwrap().pop,
        0,
        "a corpse takes no population"
    );
    assert_eq!(
        sim.world().order[index_of(&sim, mine)],
        Order::Idle,
        "the winner stands down"
    );
    run(&mut sim, DECAY_TICKS as u32 + 2);
    assert!(sim.world().slot(theirs).is_none(), "and then it is gone");
}

/// REQ: GD-COMBAT-01
#[test]
fn a_bowman_shoots_from_range_and_the_arrow_takes_time_to_land() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::BOWMAN, at(10, 10)));
    sim.issue(spawn(1, kinds::CLUBMAN, at(14, 10)));
    run(&mut sim, 3);
    let (bow, club) = (
        owned(&sim, 0, kinds::BOWMAN)[0],
        owned(&sim, 1, kinds::CLUBMAN)[0],
    );
    // A clubman standing its ground cannot answer four tiles away.
    stance(&mut sim, 1, vec![club], Stance::StandGround);
    run(&mut sim, 3);
    let start = pos_of(&sim, bow);
    sim.issue(cmd(
        0,
        CommandKind::Attack {
            ids: vec![bow],
            target: club,
        },
    ));
    run(&mut sim, 4);
    assert!(!sim.projectiles().is_empty(), "an arrow is in the air");
    assert_eq!(health(&sim, club), Fx::from_int(40), "not landed yet");
    run(&mut sim, 12);
    assert_eq!(health(&sim, club), Fx::from_int(35), "landed for 5");
    assert!(sim.projectiles().is_empty());
    assert_eq!(
        pos_of(&sim, bow),
        start,
        "never moved: four tiles is in range"
    );
    // Eight arrows kill it.
    run(&mut sim, kinds::RELOAD_TICKS * 8);
    assert!(!alive(&sim, club));
}

/// REQ: GD-STANCE-01
#[test]
fn stances_decide_who_picks_a_fight_and_how_far_they_chase() {
    let mut sim = arena();
    // Four of ours, one per stance, in a row; a passive enemy walks past
    // at four tiles, within everyone's sight but only the archer's reach.
    let stances = [
        Stance::Aggressive,
        Stance::Defensive,
        Stance::StandGround,
        Stance::Passive,
    ];
    // Ours in a row along y = 10, stances set before there is anyone to
    // fight, so nobody is defensive by default for a tick and engages
    // before its stance lands.
    for (k, _) in stances.iter().enumerate() {
        sim.issue(spawn(0, kinds::CLUBMAN, at(10 + k as i32 * 3, 10)));
    }
    run(&mut sim, 3);
    let ours = owned(&sim, 0, kinds::CLUBMAN);
    for (id, st) in ours.iter().zip(stances) {
        stance(&mut sim, 0, vec![*id], st);
    }
    run(&mut sim, 3);
    // Their cavalry appears within sight (four tiles) of the first three
    // and within nobody's reach, and is made passive at once, so it is
    // the target and never the aggressor.
    sim.issue(spawn(1, kinds::LIGHT_CAVALRY, at(13, 12)));
    run(&mut sim, 3);
    let cav = owned(&sim, 1, kinds::LIGHT_CAVALRY)[0];
    stance(&mut sim, 1, vec![cav], Stance::Passive);
    run(&mut sim, 12);
    let attacking =
        |sim: &Simulation, id| matches!(sim.world().order[index_of(sim, id)], Order::Attack { .. });
    assert!(attacking(&sim, ours[0]), "aggressive engages");
    assert!(attacking(&sim, ours[1]), "defensive engages");
    assert!(!attacking(&sim, ours[2]), "stand ground waits for reach");
    assert!(!attacking(&sim, ours[3]), "passive never");
    // The cavalry rides far away at twice their speed; the defensive one
    // gives up at its leash (its sight), the aggressive one chases twice
    // as far, both come home.
    let home = [at(10, 10), at(13, 10), at(16, 10), at(19, 10)];
    sim.issue(cmd(
        1,
        CommandKind::Move {
            ids: vec![cav],
            target: at(15, 50),
        },
    ));
    run(&mut sim, 50);
    let far0 = pos_of(&sim, ours[0]).distance(home[0]);
    let far1 = pos_of(&sim, ours[1]).distance(home[1]);
    assert!(
        far0 > far1,
        "aggressive chased further: {far0:?} vs {far1:?}"
    );
    assert!(far0 > Fx::from_int(2), "and was still chasing: {far0:?}");
    run(&mut sim, 20 * 25);
    for (k, id) in ours.iter().enumerate() {
        assert_eq!(
            sim.world().order[index_of(&sim, *id)],
            Order::Idle,
            "{k} stood down"
        );
        assert!(
            pos_of(&sim, *id).distance(home[k]) < Fx::from_int(2),
            "{k} came home: {:?} vs {:?}",
            pos_of(&sim, *id),
            home[k]
        );
    }
    assert!(alive(&sim, cav), "it got away");
}

/// REQ: GD-STANCE-02
#[test]
fn a_villager_hit_runs_for_the_town_center_and_raises_the_alarm_once() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::TOWN_CENTER, at(10, 30)));
    sim.issue(spawn(0, kinds::VILLAGER, at(20, 10)));
    sim.issue(spawn(0, kinds::VILLAGER, at(21, 10)));
    sim.issue(spawn(1, kinds::CLUBMAN, at(22, 10)));
    run(&mut sim, 3);
    let villagers = owned(&sim, 0, kinds::VILLAGER);
    let club = owned(&sim, 1, kinds::CLUBMAN)[0];
    sim.issue(cmd(
        1,
        CommandKind::Attack {
            ids: vec![club],
            target: villagers[0],
        },
    ));
    let mut alarms = 0;
    let mut fled_at = None;
    for t in 0..200 {
        sim.step();
        alarms += sim
            .events()
            .iter()
            .filter(|e| matches!(e, Event::Alarm { player: 0, .. }))
            .count();
        if fled_at.is_none()
            && matches!(
                sim.world().order[index_of(&sim, villagers[0])],
                Order::Flee { .. }
            )
        {
            fled_at = Some(t);
        }
    }
    assert!(fled_at.is_some(), "ran after the first hit");
    assert_eq!(alarms, 1, "one alarm for the raid, not one per hit");
    assert_eq!(
        sim.world().order[index_of(&sim, villagers[1])],
        Order::Idle,
        "the other one was not hit and stays"
    );
    // The runner heads for the Town Center, not away at random.
    let tc = at(10, 30);
    run(&mut sim, 20 * 10);
    let p = pos_of(&sim, villagers[0]);
    assert!(
        p.distance(tc) < Fx::from_int(6) || !alive(&sim, villagers[0]),
        "near the Town Center or dead trying: {p:?}"
    );
}

/// REQ: UX-CMD-02
#[test]
fn attack_move_engages_on_the_way_and_then_carries_on() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::AXEMAN, at(10, 10)));
    sim.issue(spawn(1, kinds::VILLAGER, at(20, 10)));
    run(&mut sim, 3);
    let axe = owned(&sim, 0, kinds::AXEMAN)[0];
    let vill = owned(&sim, 1, kinds::VILLAGER)[0];
    // Their villager will not run: it has no Town Center and stands still.
    stance(&mut sim, 1, vec![vill], Stance::StandGround);
    let far = at(40, 10);
    sim.issue(cmd(
        0,
        CommandKind::AttackMove {
            ids: vec![axe],
            target: far,
        },
    ));
    let mut engaged = false;
    for _ in 0..20 * 40 {
        sim.step();
        if matches!(sim.world().order[index_of(&sim, axe)], Order::Attack { .. }) {
            engaged = true;
        }
    }
    assert!(engaged, "stopped to fight what it saw");
    assert!(!alive(&sim, vill), "and killed it");
    let p = pos_of(&sim, axe);
    assert!(p.distance(far) < Fx::from_int(2), "then went on: {p:?}");
    assert_eq!(sim.world().order[index_of(&sim, axe)], Order::Idle);
}

/// REQ: UX-CMD-03
#[test]
fn a_patrol_walks_back_and_forth() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::SPEARMAN, at(10, 10)));
    run(&mut sim, 3);
    let spear = owned(&sim, 0, kinds::SPEARMAN)[0];
    sim.issue(cmd(
        0,
        CommandKind::Patrol {
            ids: vec![spear],
            target: at(18, 10),
        },
    ));
    let mut legs = Vec::new();
    for _ in 0..20 * 40 {
        sim.step();
        if let Order::Patrol { leg, .. } = sim.world().order[index_of(&sim, spear)] {
            if legs.last() != Some(&leg) {
                legs.push(leg);
            }
        }
    }
    assert!(legs.len() >= 4, "turned round at each end: {legs:?}");
    assert!(matches!(
        sim.world().order[index_of(&sim, spear)],
        Order::Patrol { .. }
    ));
}

/// REQ: UX-CMD-08
#[test]
fn a_formation_forms_a_line_across_the_way_and_holds_the_slowest_pace() {
    let mut sim = arena();
    for k in 0..6 {
        sim.issue(spawn(0, kinds::CLUBMAN, at(10, 8 + k)));
    }
    sim.issue(spawn(0, kinds::LIGHT_CAVALRY, at(10, 14)));
    run(&mut sim, 3);
    let mut group = owned(&sim, 0, kinds::CLUBMAN);
    let cav = owned(&sim, 0, kinds::LIGHT_CAVALRY)[0];
    group.push(cav);
    sim.issue(cmd(
        0,
        CommandKind::SetFormation {
            ids: group.clone(),
            formation: Formation::Line,
        },
    ));
    sim.issue(cmd(
        0,
        CommandKind::Move {
            ids: group.clone(),
            target: at(30, 11),
        },
    ));
    // Walking +x: a line runs across y. The cavalry, alone, would arrive
    // in ten seconds; held to the clubmen's pace it is still with them.
    run(&mut sim, 20 * 8);
    let cav_x = pos_of(&sim, cav).x;
    let club_x: Vec<Fx> = group[..6].iter().map(|id| pos_of(&sim, *id).x).collect();
    let lead = club_x.iter().copied().fold(Fx::ZERO, Fx::max);
    assert!(
        cav_x - lead < Fx::from_int(3),
        "the cavalry kept pace: {cav_x:?} vs {lead:?}"
    );
    run(&mut sim, 20 * 30);
    for id in &group {
        assert_eq!(sim.world().order[index_of(&sim, *id)], Order::Idle);
    }
    let xs: Vec<Fx> = group.iter().map(|id| pos_of(&sim, *id).x).collect();
    let ys: Vec<Fx> = group.iter().map(|id| pos_of(&sim, *id).y).collect();
    let (min_x, max_x) = (
        xs.iter().copied().fold(Fx::MAX, Fx::min),
        xs.iter().copied().fold(Fx::MIN, Fx::max),
    );
    let (min_y, max_y) = (
        ys.iter().copied().fold(Fx::MAX, Fx::min),
        ys.iter().copied().fold(Fx::MIN, Fx::max),
    );
    assert!(max_x - min_x < Fx::from_int(3), "ranks are shallow: {xs:?}");
    assert!(max_y - min_y > Fx::from_ratio(5, 2), "and wide: {ys:?}");
    // No formation: the same group spreads round the point instead.
    sim.issue(cmd(
        0,
        CommandKind::SetFormation {
            ids: group.clone(),
            formation: Formation::None,
        },
    ));
    sim.issue(cmd(
        0,
        CommandKind::Move {
            ids: group.clone(),
            target: at(30, 30),
        },
    ));
    run(&mut sim, 20 * 30);
    let ys: Vec<Fx> = group.iter().map(|id| pos_of(&sim, *id).y).collect();
    let spread =
        ys.iter().copied().fold(Fx::MIN, Fx::max) - ys.iter().copied().fold(Fx::MAX, Fx::min);
    assert!(spread < Fx::from_int(4), "a clump, not a line: {ys:?}");
}

/// REQ: TA-DET-01
#[test]
fn a_battle_replays_identically() {
    let fight = || {
        let mut sim = arena();
        for k in 0..6 {
            sim.issue(spawn(0, kinds::CLUBMAN, at(10, 8 + k)));
            sim.issue(spawn(0, kinds::BOWMAN, at(8, 8 + k)));
            sim.issue(spawn(1, kinds::AXEMAN, at(24, 8 + k)));
            sim.issue(spawn(1, kinds::SLINGER, at(26, 8 + k)));
        }
        run(&mut sim, 3);
        let mut mine = owned(&sim, 0, kinds::CLUBMAN);
        mine.extend(owned(&sim, 0, kinds::BOWMAN));
        let mut theirs = owned(&sim, 1, kinds::AXEMAN);
        theirs.extend(owned(&sim, 1, kinds::SLINGER));
        sim.issue(cmd(
            0,
            CommandKind::AttackMove {
                ids: mine,
                target: at(26, 11),
            },
        ));
        sim.issue(cmd(
            1,
            CommandKind::AttackMove {
                ids: theirs,
                target: at(8, 11),
            },
        ));
        run(&mut sim, 20 * 60);
        sim
    };
    let a = fight();
    let b = fight();
    assert_eq!(a.state_hash(), b.state_hash());
    let dead = a
        .world()
        .slots()
        .filter(|s| a.world().dying[s.index()] > 0 || a.world().health[s.index()] == Fx::ZERO)
        .count();
    let gone = 24 - a.world().len();
    assert!(
        dead + gone > 0,
        "somebody died: {dead} corpses, {gone} gone"
    );
    let replay = a.replay();
    let c = replay.run(|_, _| {}).expect("replays");
    assert_eq!(c.state_hash(), a.state_hash());
}

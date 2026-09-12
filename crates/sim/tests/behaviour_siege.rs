//! Buildings in combat (`docs/02` §6, §8; `docs/03` `UX-CMD-09`,
//! `UX-PLACE-03`; `docs/06` M4): destruction and rubble, towers that
//! shoot, walls and gates, and garrison.

mod common;

use common::*;
use sim::{
    kinds, tech, Command, CommandKind, Event, Fx, Order, PlaceError, SimConfig, Simulation, Stance,
    Vec2Fx, RUBBLE_TICKS,
};

/// A flat, empty map with a deep stockpile: nothing but what the test
/// spawns, and nothing it cannot afford.
fn arena() -> Simulation {
    Simulation::new(
        9,
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

fn cmd(player: u8, kind: CommandKind) -> Command {
    Command { player, kind }
}

fn health(sim: &Simulation, id: sim::EntityId) -> Fx {
    sim.world().health[index_of(sim, id)]
}

fn standing(sim: &Simulation, id: sim::EntityId) -> bool {
    sim.world()
        .slot(id)
        .is_some_and(|s| sim.world().dying[s.index()] == 0)
}

fn inside(sim: &Simulation, id: sim::EntityId) -> Option<sim::EntityId> {
    sim.world().inside[index_of(sim, id)]
}

fn attack(sim: &mut Simulation, player: u8, ids: Vec<sim::EntityId>, target: sim::EntityId) {
    sim.issue(cmd(player, CommandKind::Attack { ids, target }));
}

fn garrison(sim: &mut Simulation, player: u8, ids: Vec<sim::EntityId>, building: sim::EntityId) {
    sim.issue(cmd(player, CommandKind::Garrison { ids, building }));
}

/// Steps until `done` or `limit` ticks; true if it happened.
fn run_until(sim: &mut Simulation, limit: u32, done: impl Fn(&Simulation) -> bool) -> bool {
    for _ in 0..limit {
        if done(sim) {
            return true;
        }
        sim.step();
    }
    done(sim)
}

/// Spawns a finished building for `player` and returns it.
fn building(sim: &mut Simulation, player: u8, kind: sim::KindId, x: i32, y: i32) -> sim::EntityId {
    let before = owned(sim, player, kind).len();
    sim.issue(spawn(player, kind, at(x, y)));
    run(sim, 3);
    let all = owned(sim, player, kind);
    assert_eq!(
        all.len(),
        before + 1,
        "spawned a {}",
        kinds::info(kind).name
    );
    all[before]
}

/// Takes player 0 to `age` the honest way: the buildings the gate asks for
/// are spawned, the advances are researched at the Town Center.
fn advance_to(sim: &mut Simulation, age: tech::Age, tc: sim::EntityId) {
    let mut spot = 2;
    for (advance, needs) in [
        (tech::AGE_TOOL, [kinds::BARRACKS, kinds::STOREHOUSE]),
        (tech::AGE_BRONZE, [kinds::ARCHERY_RANGE, kinds::STABLE]),
    ] {
        if sim.player(0).unwrap().age >= age {
            return;
        }
        for kind in needs {
            building(sim, 0, kind, 50, spot);
            spot += 3;
        }
        sim.issue(cmd(
            0,
            CommandKind::Research {
                building: tc,
                tech: advance,
            },
        ));
        run(sim, tech::info(advance).unwrap().ticks() + 5);
    }
    assert!(sim.player(0).unwrap().age >= age);
}

/// A palisade of `player` across the whole map at `x`, with a gate at
/// `gate_y` if given. Returns the gate.
fn wall_across(
    sim: &mut Simulation,
    player: u8,
    x: i32,
    gate_y: Option<i32>,
) -> Option<sim::EntityId> {
    let size = sim.map().width();
    for y in 0..size {
        let kind = if Some(y) == gate_y {
            kinds::GATE
        } else {
            kinds::PALISADE_WALL
        };
        sim.issue(spawn(player, kind, at(x, y)));
    }
    run(sim, 3);
    owned(sim, player, kinds::GATE).first().copied()
}

/// A building hit to nothing becomes rubble: the footprint opens at once,
/// the rubble lies for a minute and then goes.
#[test]
fn a_wall_falls_to_rubble_and_the_breach_opens() {
    let mut sim = arena();
    for k in 0..3 {
        sim.issue(spawn(0, kinds::AXEMAN, at(10, 9 + k)));
    }
    let wall = building(&mut sim, 1, kinds::PALISADE_WALL, 16, 10);
    let axes = owned(&sim, 0, kinds::AXEMAN);
    assert!(!sim.nav().passable(16, 10), "a wall blocks its tile");
    // Five attack less two melee armour: three a hit, nine a volley.
    assert_eq!(sim.damage_between(axes[0], wall), Some(3));
    attack(&mut sim, 0, axes.clone(), wall);
    let mut deaths = 0;
    let fell = run_until(&mut sim, 900, |s| {
        s.world()
            .slot(wall)
            .is_none_or(|w| s.world().dying[w.index()] > 0)
    });
    assert!(fell, "three axemen bring a palisade down inside 45 seconds");
    let w = index_of(&sim, wall);
    assert_eq!(sim.world().dying[w], RUBBLE_TICKS, "rubble for a minute");
    assert!(sim.nav().passable(16, 10), "the breach is open at once");
    // The tick it fell: one death event, and the attackers let go.
    deaths += sim
        .events()
        .iter()
        .filter(|e| matches!(e, Event::Death { kind, .. } if *kind == kinds::PALISADE_WALL))
        .count();
    assert_eq!(deaths, 1);
    run(&mut sim, 3);
    for a in &axes {
        assert!(
            !matches!(sim.world().order[index_of(&sim, *a)], Order::Attack { .. }),
            "rubble is not a target"
        );
    }
    run(&mut sim, RUBBLE_TICKS as u32 + 2);
    assert!(sim.world().slot(wall).is_none(), "the rubble is gone");
}

/// REQ: GD-BUILD-01
#[test]
fn a_site_takes_damage_and_its_destruction_refunds_nothing() {
    let mut sim = arena();
    sim.issue(spawn(0, kinds::VILLAGER, at(10, 10)));
    sim.issue(spawn(1, kinds::CLUBMAN, at(24, 12)));
    run(&mut sim, 3);
    let vill = owned(&sim, 0, kinds::VILLAGER)[0];
    let club = owned(&sim, 1, kinds::CLUBMAN)[0];
    let wood = sim.player(0).unwrap().stockpile[1];
    sim.issue(cmd(
        0,
        CommandKind::Build {
            kind: kinds::HOUSE,
            x: 20,
            y: 12,
            ids: vec![vill],
        },
    ));
    run(&mut sim, 3);
    let site = owned(&sim, 0, kinds::HOUSE)[0];
    assert_eq!(
        sim.player(0).unwrap().stockpile[1],
        wood - 30,
        "paid on placing"
    );
    assert!(sim.world().construction[index_of(&sim, site)].is_some());
    // The clubman knocks the pegs down before the villager gets far.
    attack(&mut sim, 1, vec![club], site);
    let fell = run_until(&mut sim, 200, |s| !standing(s, site));
    assert!(fell, "a site takes damage like anything else");
    assert_eq!(
        sim.player(0).unwrap().stockpile[1],
        wood - 30,
        "a destroyed site refunds nothing"
    );
    run(&mut sim, 3);
    assert!(
        !matches!(sim.world().order[index_of(&sim, vill)], Order::Build { .. }),
        "the builder stops"
    );
}

/// A Watch Tower shoots what passes within its range without being told,
/// and each unit garrisoned inside adds an arrow to the volley.
///
/// REQ: UX-CMD-09
#[test]
fn a_watch_tower_shoots_on_its_own_and_its_garrison_adds_arrows() {
    let mut sim = arena();
    let tower = building(&mut sim, 0, kinds::WATCH_TOWER, 20, 20);
    sim.issue(spawn(1, kinds::CLUBMAN, at(24, 20)));
    run(&mut sim, 3);
    let club = owned(&sim, 1, kinds::CLUBMAN)[0];
    // It stands there and takes it.
    sim.issue(cmd(
        1,
        CommandKind::SetStance {
            ids: vec![club],
            stance: Stance::StandGround,
        },
    ));
    let start = health(&sim, club);
    let shot = run_until(&mut sim, 60, |s| s.projectiles().len() == 1);
    assert!(shot, "one arrow from an empty tower");
    assert!(
        run_until(&mut sim, 60, |s| health(s, club) < start),
        "and it lands"
    );
    assert_eq!(start - health(&sim, club), Fx::from_int(4), "four a hit");

    // Two bowmen go inside: the tower fires three a volley.
    sim.issue(spawn(0, kinds::BOWMAN, at(17, 20)));
    sim.issue(spawn(0, kinds::BOWMAN, at(17, 21)));
    run(&mut sim, 3);
    let bows = owned(&sim, 0, kinds::BOWMAN);
    garrison(&mut sim, 0, bows.clone(), tower);
    let went_in = run_until(&mut sim, 200, |s| {
        bows.iter().all(|b| inside(s, *b) == Some(tower))
    });
    assert!(went_in, "the bowmen walk in");
    assert_eq!(sim.garrison_of(tower), bows);
    for b in &bows {
        assert_eq!(
            pos_of(&sim, *b),
            pos_of(&sim, tower),
            "inside, at the centre"
        );
    }
    let volley = run_until(&mut sim, 60, |s| s.projectiles().len() >= 3);
    assert!(volley, "three arrows a volley: {}", sim.projectiles().len());
    // The clubman cannot hit back at anyone inside.
    attack(&mut sim, 1, vec![club], bows[0]);
    run(&mut sim, 3);
    assert!(
        !matches!(sim.world().order[index_of(&sim, club)], Order::Attack { target, .. } if target == bows[0]),
        "a garrisoned unit is not a target"
    );
    assert!(
        run_until(&mut sim, 400, |s| !standing(s, club)),
        "the tower wins"
    );
}

/// Units walk into a Town Center, stop counting as idle but still count
/// toward population, come out round its footprint when let out, and step
/// out unhurt when the building falls.
///
/// REQ: UX-CMD-09
#[test]
fn garrison_ungarrison_and_a_fallen_tower_lets_its_garrison_out() {
    let mut sim = arena();
    let tc = building(&mut sim, 0, kinds::TOWN_CENTER, 20, 20);
    for k in 0..3 {
        sim.issue(spawn(0, kinds::VILLAGER, at(15, 19 + k)));
    }
    run(&mut sim, 3);
    let vills = owned(&sim, 0, kinds::VILLAGER);
    assert_eq!(sim.idle_villagers(0).len(), 3);
    let pop = sim.player(0).unwrap().pop;
    garrison(&mut sim, 0, vills.clone(), tc);
    let went_in = run_until(&mut sim, 300, |s| {
        vills.iter().all(|v| inside(s, *v) == Some(tc))
    });
    assert!(went_in);
    assert!(sim.idle_villagers(0).is_empty(), "sheltering is not idling");
    assert_eq!(sim.player(0).unwrap().pop, pop, "still counted");
    // Orders do not reach them.
    sim.issue(move_to(0, vills.clone(), at(5, 5)));
    run(&mut sim, 3);
    assert!(vills.iter().all(|v| inside(&sim, *v) == Some(tc)));
    sim.issue(cmd(0, CommandKind::Ungarrison { building: tc }));
    run(&mut sim, 3);
    let mut tiles = Vec::new();
    for v in &vills {
        assert_eq!(inside(&sim, *v), None);
        let p = pos_of(&sim, *v);
        let t = sim::nav::tile_of(p);
        assert!(sim.nav().passable(t.0, t.1), "on open ground: {t:?}");
        assert!(
            p.distance(at(20, 20)) <= Fx::from_ratio(5, 2),
            "beside the footprint: {p:?}"
        );
        tiles.push(t);
    }
    tiles.sort_unstable();
    tiles.dedup();
    assert_eq!(tiles.len(), 3, "one tile each");

    // A tower with two inside falls: they step out, unhurt.
    let tower = building(&mut sim, 0, kinds::WATCH_TOWER, 30, 30);
    garrison(&mut sim, 0, vills[..2].to_vec(), tower);
    assert!(run_until(&mut sim, 800, |s| {
        vills[..2].iter().all(|v| inside(s, *v) == Some(tower))
    }));
    // Enough of them to win the race against three arrows a volley.
    for k in 0..8 {
        sim.issue(spawn(1, kinds::AXEMAN, at(34 + k / 4, 29 + k % 4)));
    }
    run(&mut sim, 3);
    let axes = owned(&sim, 1, kinds::AXEMAN);
    attack(&mut sim, 1, axes, tower);
    assert!(
        run_until(&mut sim, 1200, |s| !standing(s, tower)),
        "the tower falls"
    );
    for v in &vills[..2] {
        assert_eq!(inside(&sim, *v), None, "let out as it fell");
        assert_eq!(health(&sim, *v), Fx::from_int(25), "unhurt");
        let t = sim::nav::tile_of(pos_of(&sim, *v));
        assert!(sim.nav().passable(t.0, t.1));
    }
}

/// A gate in a wall lets its owner through and shuts on an enemy, who
/// cannot pass; it opens again once they have gone.
#[test]
fn a_gate_lets_the_owner_through_and_shuts_on_an_enemy() {
    let mut sim = arena();
    let gate = wall_across(&mut sim, 0, 20, Some(15)).unwrap();
    let gi = index_of(&sim, gate);
    assert!(sim.gate_open(gi), "a finished gate stands open");
    assert!(!sim.nav().passable(20, 14) && sim.nav().passable(20, 15));
    // Ours walks through.
    sim.issue(spawn(0, kinds::CLUBMAN, at(15, 15)));
    run(&mut sim, 3);
    let mine = owned(&sim, 0, kinds::CLUBMAN)[0];
    sim.issue(move_to(0, vec![mine], at(25, 15)));
    let through = run_until(&mut sim, 400, |s| pos_of(s, mine).x > Fx::from_int(24));
    assert!(through, "the owner passes: {:?}", pos_of(&sim, mine));
    // Theirs finds it shut.
    sim.issue(spawn(1, kinds::CLUBMAN, at(15, 15)));
    run(&mut sim, 3);
    let theirs = owned(&sim, 1, kinds::CLUBMAN)[0];
    sim.issue(cmd(
        1,
        CommandKind::SetStance {
            ids: vec![theirs],
            stance: Stance::Passive,
        },
    ));
    sim.issue(move_to(1, vec![theirs], at(25, 15)));
    let shut = run_until(&mut sim, 200, |s| !s.gate_open(gi));
    assert!(shut, "the gate shuts as the enemy comes near");
    assert!(
        pos_of(&sim, theirs).x < Fx::from_int(20),
        "and it is still outside"
    );
    run(&mut sim, 700);
    assert!(
        pos_of(&sim, theirs).x < Fx::from_int(20),
        "no way through: {:?}",
        pos_of(&sim, theirs)
    );
    assert_eq!(
        sim.world().order[index_of(&sim, theirs)],
        Order::Idle,
        "it gave up"
    );
    // It leaves; the gate opens.
    sim.issue(move_to(1, vec![theirs], at(5, 15)));
    let opened = run_until(&mut sim, 300, |s| s.gate_open(gi));
    assert!(opened, "open again once the enemy has gone");
}

/// A gate is set onto one of the player's own wall segments, which it
/// replaces and refunds; a wall run is built segment by segment by the
/// villagers sent to its first one.
///
/// REQ: UX-PLACE-03
#[test]
fn a_gate_replaces_a_wall_segment_and_builders_carry_on_along_a_run() {
    let mut sim = arena();
    let tc = building(&mut sim, 0, kinds::TOWN_CENTER, 10, 10);
    sim.issue(spawn(0, kinds::VILLAGER, at(14, 14)));
    run(&mut sim, 3);
    let vill = owned(&sim, 0, kinds::VILLAGER)[0];
    assert_eq!(
        sim.can_place(0, kinds::PALISADE_WALL, 20, 10),
        Err(PlaceError::AgeLocked {
            needs: tech::Age::Tool
        })
    );
    advance_to(&mut sim, tech::Age::Tool, tc);
    // A run of three, the villager sent to the first.
    let run_tiles = sim::nav::line_tiles((20, 10), (20, 12));
    for (n, (x, y)) in run_tiles.iter().enumerate() {
        sim.issue(cmd(
            0,
            CommandKind::Build {
                kind: kinds::PALISADE_WALL,
                x: *x,
                y: *y,
                ids: if n == 0 { vec![vill] } else { vec![] },
            },
        ));
    }
    let built = run_until(&mut sim, 900, |s| {
        let walls = owned(s, 0, kinds::PALISADE_WALL);
        walls.len() == 3
            && walls
                .iter()
                .all(|w| s.world().construction[index_of(s, *w)].is_none())
    });
    assert!(built, "one villager finishes the run on its own");
    assert_eq!(
        sim.can_place(0, kinds::GATE, 20, 11),
        Err(PlaceError::AgeLocked {
            needs: tech::Age::Bronze
        })
    );
    advance_to(&mut sim, tech::Age::Bronze, tc);
    assert_eq!(
        sim.can_place(0, kinds::GATE, 20, 11),
        Ok(()),
        "onto our wall"
    );
    assert_eq!(
        sim.can_place(0, kinds::GATE, 30, 30),
        Ok(()),
        "or onto clear ground"
    );
    assert!(
        matches!(
            sim.can_place(0, kinds::GATE, 10, 10),
            Err(PlaceError::NeedsWall)
        ),
        "not onto another building"
    );
    let before = sim.player(0).unwrap().stockpile;
    sim.issue(cmd(
        0,
        CommandKind::Build {
            kind: kinds::GATE,
            x: 20,
            y: 11,
            ids: vec![vill],
        },
    ));
    run(&mut sim, 3);
    let after = sim.player(0).unwrap().stockpile;
    assert_eq!(after[1], before[1] + 5, "the segment's wood comes back");
    assert_eq!(after[2], before[2] - 30, "the gate's stone is paid");
    assert_eq!(owned(&sim, 0, kinds::PALISADE_WALL).len(), 2);
    let gate = owned(&sim, 0, kinds::GATE)[0];
    assert!(!sim.gate_open(index_of(&sim, gate)), "a site is solid");
    let done = run_until(&mut sim, 900, |s| {
        s.world().construction[index_of(s, gate)].is_none()
    });
    assert!(done);
    assert!(sim.gate_open(index_of(&sim, gate)), "finished, it opens");
}

/// A column on attack-move walled out of its destination breaks the wall
/// where it stands and goes on through the breach; it does not stop for
/// walls it merely passes.
///
/// REQ: UX-CMD-02
#[test]
fn an_attack_move_column_breaks_a_wall_in_its_way() {
    let mut sim = arena();
    wall_across(&mut sim, 1, 30, None);
    for k in 0..6 {
        sim.issue(spawn(0, kinds::AXEMAN, at(27, 28 + k)));
    }
    run(&mut sim, 3);
    let axes = owned(&sim, 0, kinds::AXEMAN);
    sim.issue(cmd(
        0,
        CommandKind::AttackMove {
            ids: axes.clone(),
            target: at(40, 30),
        },
    ));
    let engaged = run_until(&mut sim, 200, |s| {
        axes.iter().any(|a| {
            matches!(
                s.world().order[index_of(s, *a)],
                Order::Attack { target, .. }
                    if s.world().slot(target).is_some_and(|t| s.world().kind[t.index()] == kinds::PALISADE_WALL)
            )
        })
    });
    assert!(engaged, "nothing else to fight: the wall will do");
    let breached = run_until(&mut sim, 2400, |s| (0..64).any(|y| s.nav().passable(30, y)));
    assert!(breached, "a segment falls");
    let through = run_until(&mut sim, 1200, |s| {
        axes.iter().any(|a| pos_of(s, *a).x > Fx::from_int(31))
    });
    assert!(through, "and the column goes on through the breach");
}

/// A villager hit near home runs into the Town Center, and the Town
/// Center shoots back with the arrows its garrison gives it.
///
/// REQ: GD-STANCE-02
#[test]
fn a_villager_shelters_in_the_town_center_and_it_shoots_back() {
    let mut sim = arena();
    let tc = building(&mut sim, 0, kinds::TOWN_CENTER, 20, 20);
    sim.issue(spawn(0, kinds::VILLAGER, at(24, 20)));
    sim.issue(spawn(1, kinds::CLUBMAN, at(26, 20)));
    run(&mut sim, 3);
    let vill = owned(&sim, 0, kinds::VILLAGER)[0];
    let club = owned(&sim, 1, kinds::CLUBMAN)[0];
    // Held back for a moment, so the Town Center's silence is its own.
    sim.issue(cmd(
        1,
        CommandKind::SetStance {
            ids: vec![club],
            stance: Stance::Passive,
        },
    ));
    run(&mut sim, 40);
    assert!(
        sim.projectiles().is_empty(),
        "an empty Town Center fires nothing"
    );
    attack(&mut sim, 1, vec![club], vill);
    let sheltered = run_until(&mut sim, 300, |s| inside(s, vill) == Some(tc));
    assert!(sheltered, "the villager runs inside");
    assert!(
        matches!(sim.world().order[index_of(&sim, vill)], Order::Idle),
        "safe, and still"
    );
    let start = health(&sim, club);
    assert!(
        run_until(&mut sim, 120, |s| health(s, club) < start),
        "the Town Center shoots with the villager's arrow"
    );
    assert!(run_until(&mut sim, 200, |s| s.projectiles().len() == 1));
}

/// REQ: TA-DET-01
#[test]
fn a_siege_replays_identically() {
    let play = |seed: u64| {
        let mut sim = Simulation::new(
            seed,
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
        );
        let tower = building(&mut sim, 0, kinds::WATCH_TOWER, 30, 30);
        wall_across(&mut sim, 0, 33, Some(30));
        for k in 0..4 {
            sim.issue(spawn(0, kinds::BOWMAN, at(28, 28 + k)));
            sim.issue(spawn(1, kinds::AXEMAN, at(40, 26 + k * 2)));
            sim.issue(spawn(1, kinds::SLINGER, at(42, 27 + k * 2)));
        }
        run(&mut sim, 3);
        let bows = owned(&sim, 0, kinds::BOWMAN);
        garrison(&mut sim, 0, bows[..2].to_vec(), tower);
        let theirs: Vec<_> = owned(&sim, 1, kinds::AXEMAN)
            .into_iter()
            .chain(owned(&sim, 1, kinds::SLINGER))
            .collect();
        sim.issue(cmd(
            1,
            CommandKind::AttackMove {
                ids: theirs,
                target: at(20, 30),
            },
        ));
        run(&mut sim, 1500);
        sim.check().unwrap();
        let hurt = owned(&sim, 1, kinds::AXEMAN).len() < 4
            || owned(&sim, 1, kinds::AXEMAN)
                .iter()
                .any(|a| health(&sim, *a) < Fx::from_int(50));
        assert!(hurt, "the tower and the bowmen did something");
        (sim.tick(), sim.state_hash(), sim.replay())
    };
    let (end, a, replay) = play(3);
    let (_, b, _) = play(3);
    assert_eq!(a, b, "two runs agree");
    // And the command log alone rebuilds it.
    let mut again = Simulation::new(replay.seed, replay.config.clone());
    for (tick, c) in &replay.commands {
        while again.tick() < *tick {
            again.step();
        }
        again.issue(c.clone());
    }
    while again.tick() < end {
        again.step();
    }
    assert_eq!(again.state_hash(), a, "the replay agrees");
}

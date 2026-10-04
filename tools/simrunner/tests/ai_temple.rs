//! The opponent's priests and its answer to the clocks (`docs/02` §5.5,
//! §10), in headless matches through its own commands and view.
//!
//! REQ: GD-AI-01

use ai::{Difficulty, Opponent};
use fogged::{kinds, FoggedView};
use sim::{Command, CommandKind, MapKind, MapSpec, Order, SimConfig, Simulation, Source};

fn inland(seed: u64, stockpile: i32) -> Simulation {
    Simulation::new(
        seed,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Inland,
                size: 96,
                players: 2,
            },
            starting_stockpile: [stockpile; 4],
            ..SimConfig::default()
        },
    )
}

/// Runs the opponents until `ticks`, issuing their commands as the AI.
fn play(sim: &mut Simulation, opponents: &mut [Opponent], ticks: u64) {
    while sim.tick() < ticks {
        for bot in opponents.iter_mut() {
            let commands = {
                let view = FoggedView::new(sim, bot.player());
                bot.think(&view)
            };
            for c in commands {
                c.validate().expect("a well-formed command");
                sim.issue_from(c, Source::Ai);
            }
        }
        sim.step();
        if sim.tick().is_multiple_of(200) {
            sim.check().expect("invariants hold");
        }
    }
}

fn spawn(sim: &mut Simulation, player: u8, kind: u16, x: i32, y: i32) {
    let fp = kinds::info(kind).footprint as i32;
    let pos = if fp > 1 {
        sim::nav::building_centre(x, y, fp)
    } else {
        sim::nav::centre((x, y))
    };
    sim.issue(Command {
        player,
        kind: CommandKind::Spawn { kind, pos },
    });
}

/// A Wonder of the enemy's standing finished: the opponent sends every
/// idle soldier at it, whatever the size of its army.
///
/// REQ: GD-WIN-02
#[test]
fn an_enemy_wonder_draws_the_army() {
    let seed = 4;
    let mut sim = inland(seed, 200);
    let mut bots = [Opponent::new(1, Difficulty::Standard, seed)];
    play(&mut sim, &mut bots, 600);
    let (sx, sy) = sim.starts()[1];
    for k in 0..3 {
        spawn(&mut sim, 1, kinds::SWORDSMAN, sx - 3, sy + 3 + k);
    }
    let (wx, wy) = sim.starts()[0];
    spawn(&mut sim, 0, kinds::WONDER, wx + 8, wy + 8);
    play(&mut sim, &mut bots, 700);
    let w = sim.world();
    let wonder = w
        .slots()
        .find(|s| w.kind[s.index()] == kinds::WONDER)
        .map(|s| w.pos[s.index()])
        .expect("the Wonder stands");
    let sent: Vec<_> = w
        .slots()
        .filter(|s| w.owner[s.index()] == 1 && w.kind[s.index()] == kinds::SWORDSMAN)
        .map(|s| w.order[s.index()])
        .collect();
    assert_eq!(sent.len(), 3);
    for order in sent {
        match order {
            Order::AttackMove { target } => {
                assert!(target.distance(wonder) < sim::Fx::from_int(3), "{target:?}")
            }
            other => panic!("not sent at the Wonder: {other:?}"),
        }
    }
}

/// A rich Hard opponent reaches the Bronze Age, builds a Temple, trains
/// priests and fetches the relics it finds into it.
///
/// REQ: GD-WIN-03
#[test]
fn the_opponents_priests_fetch_relics_home() {
    let seed = 2;
    let mut sim = inland(seed, 5000);
    let mut bots = [Opponent::new(1, Difficulty::Hard, seed)];
    play(&mut sim, &mut bots, 30_000);
    let w = sim.world();
    let priests = w
        .slots()
        .filter(|s| w.owner[s.index()] == 1 && w.kind[s.index()] == kinds::PRIEST)
        .count();
    assert!(priests >= 1, "trained priests");
    assert!(sim.relics_held(1) >= 1, "a relic in its Temple");
    sim.replay().verify().expect("replays");
}

/// Opponents with civilizations play a match and field nothing their
/// civilization is denied (`docs/02` §11).
#[test]
fn opponents_keep_to_their_civilizations() {
    let seed = 5;
    let mut sim = Simulation::new(
        seed,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Inland,
                size: 96,
                players: 2,
            },
            starting_stockpile: [3000; 4],
            civs: vec![sim::Civ::Egyptians, sim::Civ::Greeks],
            ..SimConfig::default()
        },
    );
    let mut bots = [
        Opponent::new(0, Difficulty::Hard, seed),
        Opponent::new(1, Difficulty::Hard, seed),
    ];
    play(&mut sim, &mut bots, 24_000);
    let w = sim.world();
    let mut fielded = 0;
    for s in w.slots() {
        let i = s.index();
        let owner = w.owner[i];
        if let Some(civ) = sim.civ(owner) {
            fielded += 1;
            assert!(
                civ.allows(w.kind[i]),
                "{} have a {}",
                civ.name(),
                kinds::info(w.kind[i]).name
            );
        }
    }
    assert!(fielded > 40, "both sides built up: {fielded}");
    assert!(
        sim.player(0).unwrap().age >= sim::Age::Bronze
            || sim.player(1).unwrap().age >= sim::Age::Bronze,
        "a side reached the Bronze Age"
    );
    sim.replay().verify().expect("replays");
}

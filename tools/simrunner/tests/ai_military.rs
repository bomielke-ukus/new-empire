//! The military manager and scouting, in headless matches through the
//! player's own commands and view. Here and not in `crates/ai` because
//! building the match needs `sim`.
//!
//! REQ: GD-AI-01

use ai::{Difficulty, Opponent};
use fogged::{kinds, FoggedView};
use sim::{Command, CommandKind, Event, MapKind, MapSpec, SimConfig, Simulation, Source};

fn inland(seed: u64, players: u8) -> Simulation {
    Simulation::new(
        seed,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Inland,
                size: 96,
                players,
            },
            ..SimConfig::default()
        },
    )
}

/// Runs the opponents for `ticks`, issuing their commands as the AI, and
/// counts every death by owner.
fn play(sim: &mut Simulation, opponents: &mut [Opponent], ticks: u64) -> [u32; 8] {
    let mut deaths = [0u32; 8];
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
        for e in sim.events() {
            if let Event::Death { owner, .. } = *e {
                if let Some(d) = deaths.get_mut(owner as usize) {
                    *d += 1;
                }
            }
        }
        if sim.tick().is_multiple_of(200) {
            sim.check().expect("invariants hold");
        }
    }
    deaths
}

fn count(sim: &Simulation, player: u8, keep: impl Fn(u16) -> bool) -> usize {
    let w = sim.world();
    w.slots()
        .filter(|s| {
            let i = s.index();
            w.owner[i] == player && w.dying[i] == 0 && keep(w.kind[i])
        })
        .count()
}

fn is_soldier(kind: u16) -> bool {
    let k = kinds::info(kind);
    k.mobile && k.combat.attack > 0 && kind != kinds::VILLAGER && kind != kinds::SCOUT
}

/// Hard against Easy for twenty minutes: Hard's scout has seen most of
/// the map, Hard has soldiers and has taken something of Easy's, and Easy
/// has scouted nothing beyond its start. The match replays.
#[test]
fn hard_scouts_the_map_and_raids_easy() {
    let seed = 1;
    let mut sim = inland(seed, 2);
    let mut bots = [
        Opponent::new(0, Difficulty::Hard, seed),
        Opponent::new(1, Difficulty::Easy, seed),
    ];
    let deaths = play(&mut sim, &mut bots, 24_000);
    let explored = |p: u8| {
        let f = sim.fog(p).unwrap();
        f.explored_count() * 100 / (f.width() * f.height()) as usize
    };
    assert!(explored(0) >= 40, "Hard scouted {}%", explored(0));
    assert!(explored(1) <= 15, "Easy does not scout: {}%", explored(1));
    assert!(
        deaths[1] >= 3,
        "Hard's raids cost Easy something: {} deaths",
        deaths[1]
    );
    assert!(
        count(&sim, 0, is_soldier) + deaths[0] as usize >= 6,
        "Hard raised an army"
    );
    assert!(
        sim.player(0).unwrap().age.index() > sim.player(1).unwrap().age.index()
            || count(&sim, 0, |k| k == kinds::VILLAGER) > count(&sim, 1, |k| k == kinds::VILLAGER),
        "Hard is ahead"
    );
    sim.replay().verify().expect("replays");
}

/// An alarm at home is answered: a Standard opponent with three clubmen
/// by its Town Center is raided by two enemy clubmen sent at a house; its
/// soldiers go to the alarm and the raiders die.
#[test]
fn soldiers_answer_an_alarm_at_home() {
    let seed = 4;
    let mut sim = inland(seed, 2);
    let mut bots = [Opponent::new(1, Difficulty::Standard, seed)];
    // Let the opponent settle in, then give it soldiers and a raid.
    play(&mut sim, &mut bots, 600);
    let (sx, sy) = sim.starts()[1];
    let spawn = |sim: &mut Simulation, player: u8, kind, x: i32, y: i32| {
        sim.issue(Command {
            player,
            kind: CommandKind::Spawn {
                kind,
                pos: sim::nav::centre((x, y)),
            },
        });
    };
    for k in 0..3 {
        spawn(&mut sim, 1, kinds::CLUBMAN, sx - 3, sy + 3 + k);
    }
    let house = sim
        .world()
        .slots()
        .find(|s| sim.world().owner[s.index()] == 1 && sim.world().kind[s.index()] == kinds::HOUSE)
        .map(|s| sim.world().id_at(s));
    let raid_at = house.map_or(sim::nav::centre((sx + 6, sy)), |h| {
        sim.world().pos[sim.world().slot(h).unwrap().index()]
    });
    let ax = (raid_at.x.floor() + 8).min(sim.map().width() - 3);
    for k in 0..2 {
        spawn(&mut sim, 0, kinds::CLUBMAN, ax, raid_at.y.floor() + k);
    }
    play(&mut sim, &mut bots, 605);
    let raiders: Vec<_> = sim
        .world()
        .slots()
        .filter(|s| {
            sim.world().owner[s.index()] == 0 && sim.world().kind[s.index()] == kinds::CLUBMAN
        })
        .map(|s| sim.world().id_at(s))
        .collect();
    assert_eq!(raiders.len(), 2);
    sim.issue(Command {
        player: 0,
        kind: CommandKind::AttackMove {
            ids: raiders,
            target: raid_at,
        },
    });
    let deaths = play(&mut sim, &mut bots, 605 + 1500);
    assert_eq!(
        count(&sim, 0, |k| k == kinds::CLUBMAN),
        0,
        "the raiders are dead; deaths {deaths:?}"
    );
    assert!(
        count(&sim, 1, |k| k == kinds::CLUBMAN) >= 1,
        "and the defenders are not all dead"
    );
}

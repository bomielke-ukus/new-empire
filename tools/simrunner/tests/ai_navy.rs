//! The opponent at sea (`docs/02` §5.6, `docs/07` D33), in headless
//! matches through its own commands and view: a Dock and fishing boats
//! where there is water near home, and on Islands a navy that finds the
//! enemy and an army carried over to it.
//!
//! REQ: GD-AI-01

use ai::{Difficulty, Opponent};
use fogged::{kinds, FoggedView};
use sim::{MapKind, MapSpec, SimConfig, Simulation, Source};

fn on(kind: MapKind, seed: u64) -> Simulation {
    Simulation::new(
        seed,
        SimConfig {
            map: MapSpec {
                kind,
                size: 96,
                players: 2,
            },
            ..SimConfig::default()
        },
    )
}

fn play(
    sim: &mut Simulation,
    opponents: &mut [Opponent],
    ticks: u64,
    until: impl Fn(&Simulation) -> bool,
) {
    while sim.tick() < ticks && !until(sim) {
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
        if sim.tick().is_multiple_of(500) {
            sim.check().expect("invariants hold");
        }
    }
}

fn count(sim: &Simulation, player: u8, kind: u16) -> usize {
    let w = sim.world();
    w.slots()
        .filter(|s| {
            let i = s.index();
            w.owner[i] == player && w.kind[i] == kind && w.construction[i].is_none()
        })
        .count()
}

/// On Islands a Standard opponent builds a Dock and fishes inside twenty
/// minutes.
#[test]
fn a_standard_opponent_fishes() {
    let seed = 2;
    let mut sim = on(MapKind::Islands, seed);
    let mut bots = [
        Opponent::new(0, Difficulty::Standard, seed),
        Opponent::new(1, Difficulty::Easy, seed),
    ];
    play(&mut sim, &mut bots, 24_000, |sim| {
        count(sim, 0, kinds::FISHING_BOAT) >= 2
    });
    assert!(count(&sim, 0, kinds::DOCK) >= 1, "a Dock");
    assert!(count(&sim, 0, kinds::FISHING_BOAT) >= 2, "fishing boats");
    sim.replay().verify().expect("replays");
}

/// On Islands, Hard finds Easy across the sea, carries its army over and
/// beats it inside an hour.
///
/// REQ: GD-NAVAL-03
#[test]
fn hard_carries_its_army_across_and_wins() {
    let seed = 1;
    let mut sim = on(MapKind::Islands, seed);
    let mut bots = [
        Opponent::new(0, Difficulty::Hard, seed),
        Opponent::new(1, Difficulty::Easy, seed),
    ];
    play(&mut sim, &mut bots, 72_000, |sim| sim.winner().is_some());
    assert_eq!(sim.winner(), Some(0), "by tick {}", sim.tick());
    sim.replay().verify().expect("replays");
}

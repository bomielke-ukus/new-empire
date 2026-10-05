//! The opponent's technologies and its advance to the Iron Age
//! (`docs/02` §7, §12), in headless matches through its own commands and
//! view.
//!
//! REQ: GD-AI-01

use ai::{Difficulty, Opponent};
use fogged::{kinds, tech, FoggedView};
use sim::{Age, MapKind, MapSpec, SimConfig, Simulation, Source};

fn inland(seed: u64) -> Simulation {
    Simulation::new(
        seed,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Inland,
                size: 96,
                players: 2,
            },
            ..SimConfig::default()
        },
    )
}

/// Runs the opponents until `until` says stop or `ticks` pass.
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
        if sim.tick().is_multiple_of(200) {
            sim.check().expect("invariants hold");
        }
    }
}

/// The technologies a side has researched that are neither an age nor a
/// line upgrade.
fn improvements(sim: &Simulation, player: u8) -> Vec<&'static str> {
    sim.player(player)
        .unwrap()
        .researched
        .iter()
        .filter_map(|t| tech::info(*t))
        .filter(|t| t.advances_age().is_none() && t.upgrades_line().is_none())
        .map(|t| t.name)
        .collect()
}

/// Twenty-five minutes of Standard against Easy: Standard has researched
/// Woodworking and the improvements for what it gathers and fields;
/// Easy, whose order keeps it simple, has researched none.
#[test]
fn standard_researches_and_easy_does_not() {
    let seed = 2;
    let mut sim = inland(seed);
    let mut bots = [
        Opponent::new(0, Difficulty::Standard, seed),
        Opponent::new(1, Difficulty::Easy, seed),
    ];
    play(&mut sim, &mut bots, 30_000, |_| false);
    let standard = improvements(&sim, 0);
    assert!(standard.contains(&"Woodworking"), "{standard:?}");
    assert!(standard.len() >= 4, "{standard:?}");
    assert!(
        improvements(&sim, 1).is_empty(),
        "{:?}",
        improvements(&sim, 1)
    );
    sim.replay().verify().expect("replays");
}

/// Hard against Easy from the ordinary start: Hard reaches the Iron Age
/// inside forty minutes, saving for it after ten minutes in the Bronze
/// Age whatever its army.
#[test]
fn hard_reaches_the_iron_age() {
    let seed = 1;
    let mut sim = inland(seed);
    let mut bots = [
        Opponent::new(0, Difficulty::Hard, seed),
        Opponent::new(1, Difficulty::Easy, seed),
    ];
    play(&mut sim, &mut bots, 48_000, |sim| {
        sim.player(0).unwrap().age == Age::Iron
    });
    assert_eq!(
        sim.player(0).unwrap().age,
        Age::Iron,
        "by tick {}",
        sim.tick()
    );
    sim.replay().verify().expect("replays");
}

/// Hard against Easy from the ordinary start, for an hour: after ten
/// minutes in the Iron Age Hard saves for a Wonder, and one stands. Easy
/// is gone long before, so the match is over and no clock runs; the
/// Wonder is looked for in the world.
///
/// REQ: GD-WIN-02
#[test]
fn hard_raises_a_wonder_from_an_ordinary_start() {
    let seed = 1;
    let mut sim = inland(seed);
    let mut bots = [
        Opponent::new(0, Difficulty::Hard, seed),
        Opponent::new(1, Difficulty::Easy, seed),
    ];
    let stands = |sim: &Simulation| {
        let w = sim.world();
        w.slots().any(|s| {
            let i = s.index();
            w.kind[i] == kinds::WONDER && w.owner[i] == 0 && w.construction[i].is_none()
        })
    };
    play(&mut sim, &mut bots, 72_000, stands);
    assert!(
        stands(&sim),
        "no Wonder of Hard's stands by tick {}",
        sim.tick()
    );
    sim.replay().verify().expect("replays");
}

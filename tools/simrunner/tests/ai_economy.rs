//! The economy manager, playing a headless match through the player's
//! own commands and the player's own view. It lives here and not in
//! `crates/ai` because building the match needs `sim`, which the `ai`
//! crate must not depend on, not even for its tests.
//!
//! REQ: GD-AI-01

use ai::{Difficulty, Opponent};
use fogged::{kinds, FoggedView};
use sim::{Age, EntityId, MapKind, MapSpec, SimConfig, Simulation, Source};

/// Runs `opponents` on their sides for `ticks`, issuing every command as
/// the AI, with the invariants checked on the way. Returns the match and
/// the longest a villager of `watched` stood idle, in ticks.
fn play(seed: u64, opponents: &mut [Opponent], ticks: u64, watched: u8) -> (Simulation, u64) {
    let mut sim = Simulation::new(
        seed,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Inland,
                size: 96,
                players: 2,
            },
            ..SimConfig::default()
        },
    );
    let mut streaks: std::collections::BTreeMap<EntityId, u64> = std::collections::BTreeMap::new();
    let mut longest = 0u64;
    while sim.tick() < ticks {
        for bot in opponents.iter_mut() {
            let commands = {
                let view = FoggedView::new(&sim, bot.player());
                bot.think(&view)
            };
            for c in commands {
                assert_eq!(
                    c.player,
                    bot.player(),
                    "an opponent orders only its own side"
                );
                c.validate().expect("a well-formed command");
                sim.issue_from(c, Source::Ai);
            }
        }
        sim.step();
        if sim.tick().is_multiple_of(100) {
            sim.check().expect("invariants hold");
        }
        let idle = sim.idle_villagers(watched);
        streaks.retain(|id, _| idle.contains(id));
        for id in idle {
            let n = streaks.entry(id).or_insert(0u64);
            *n += 1;
            longest = longest.max(*n);
        }
    }
    (sim, longest)
}

fn count(sim: &Simulation, player: u8, kind: u16, finished: bool) -> usize {
    let w = sim.world();
    w.slots()
        .filter(|s| {
            let i = s.index();
            w.owner[i] == player
                && w.kind[i] == kind
                && w.dying[i] == 0
                && (!finished || w.construction[i].is_none())
        })
        .count()
}

/// A Standard opponent left alone for eight minutes grows: it trains
/// villagers to its target, houses them ahead of the cap, gathers every
/// resource it is short of, puts up the two buildings the Tool Age needs
/// and takes the age. Nobody stands idle for long. It does all of this
/// through validated commands from its own view, and the match replays.
#[test]
fn a_standard_opponent_runs_an_economy_and_reaches_the_tool_age() {
    let seed = 1;
    let mut bots = [Opponent::new(1, Difficulty::Standard, seed)];
    let (sim, longest_idle) = play(seed, &mut bots, 9600, 1);
    let me = sim.player(1).unwrap();
    let villagers = count(&sim, 1, kinds::VILLAGER, true);
    let houses = count(&sim, 1, kinds::HOUSE, true);
    assert!(villagers >= 9, "villagers: {villagers}");
    assert!(houses >= 2, "houses: {houses}, cap {}", me.pop_cap);
    assert!(me.pop <= me.pop_cap, "never over the cap");
    assert!(
        me.gathered[0] > 300 && me.gathered[1] > 300,
        "food and wood come in: {:?}",
        me.gathered
    );
    assert_eq!(count(&sim, 1, kinds::STOREHOUSE, false), 1, "a Storehouse");
    assert_eq!(
        count(&sim, 1, kinds::BARRACKS, false),
        1,
        "and a Barracks, for the age"
    );
    assert_eq!(me.age, Age::Tool, "the Tool Age is taken");
    assert!(
        longest_idle <= 400,
        "a villager stood idle for {longest_idle} ticks"
    );
    // Player 0 was never touched.
    assert_eq!(count(&sim, 0, kinds::VILLAGER, true), 3);
    assert!(bots[0].thoughts() > 100);
    let replay = sim.replay();
    assert!(replay.sources.iter().all(|s| *s == Source::Ai));
    replay.verify().expect("the match replays identically");
}

/// The same seed and difficulty think the same thoughts: two matches are
/// one match.
#[test]
fn an_opponent_is_deterministic() {
    let seed = 5;
    let (a, _) = play(
        seed,
        &mut [Opponent::new(1, Difficulty::Hard, seed)],
        1500,
        1,
    );
    let (b, _) = play(
        seed,
        &mut [Opponent::new(1, Difficulty::Hard, seed)],
        1500,
        1,
    );
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.replay(), b.replay());
}

/// Easy aims lower and thinks slower than Standard: after the same time
/// it has fewer villagers, and it never leaves the Tool Age.
#[test]
fn easy_grows_slower_than_standard() {
    let seed = 3;
    let (easy, _) = play(
        seed,
        &mut [Opponent::new(1, Difficulty::Easy, seed)],
        4800,
        1,
    );
    let (standard, _) = play(
        seed,
        &mut [Opponent::new(1, Difficulty::Standard, seed)],
        4800,
        1,
    );
    let (e, s) = (
        count(&easy, 1, kinds::VILLAGER, true),
        count(&standard, 1, kinds::VILLAGER, true),
    );
    assert!(e < s, "Easy {e} villagers, Standard {s}");
    assert!(e >= 6, "but Easy still plays: {e}");
    assert_eq!(bots_order(Difficulty::Easy).last_age, Age::Tool);
}

fn bots_order(d: Difficulty) -> ai::economy::BuildOrder {
    *Opponent::new(1, d, 1).order()
}

/// Two opponents in one match share nothing but the map, and the match
/// still replays.
#[test]
fn two_opponents_play_the_same_match() {
    let seed = 9;
    let mut bots = [
        Opponent::new(0, Difficulty::Standard, seed),
        Opponent::new(1, Difficulty::Standard, seed),
    ];
    let (sim, _) = play(seed, &mut bots, 2400, 0);
    for p in 0..2 {
        assert!(count(&sim, p, kinds::VILLAGER, true) >= 5, "player {p}");
    }
    sim.replay().verify().expect("replays");
}

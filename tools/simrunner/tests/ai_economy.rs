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

/// An opponent hunts the herd near its town (`GD-AI-02`): its villagers
/// are sent at the gazelles by the same order a player gives, the animals
/// die, and their food comes home. Nothing is hunted far from a drop-off.
///
/// REQ: GD-AI-02
/// REQ: GD-ECON-06
#[test]
fn an_opponent_hunts_the_herd_near_its_town() {
    let mut bots = [Opponent::new(0, Difficulty::Standard, 2)];
    let start = Simulation::new(
        2,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Inland,
                size: 96,
                players: 2,
            },
            ..SimConfig::default()
        },
    );
    let w = start.world();
    let gazelles: Vec<EntityId> = w
        .slots()
        .filter(|s| w.kind[s.index()] == kinds::GAZELLE)
        .map(|s| w.id_at(s))
        .collect();
    assert!(!gazelles.is_empty());
    let (sim, _) = play(2, &mut bots, 6000, 0);
    let replay = sim.replay();
    let hunts: Vec<EntityId> = replay
        .commands
        .iter()
        .filter(|(_, c)| c.player == 0)
        .filter_map(|(_, c)| match c.kind {
            sim::CommandKind::Attack { target, .. } if gazelles.contains(&target) => Some(target),
            _ => None,
        })
        .collect();
    assert!(!hunts.is_empty(), "the opponent went hunting");
    let dead = hunts
        .iter()
        .filter(|g| {
            sim.world()
                .slot(**g)
                .is_none_or(|s| sim.world().dying[s.index()] > 0)
        })
        .count();
    assert!(dead > 0, "and killed: {dead} of {}", hunts.len());
    // Every animal hunted was near a drop-off of its side when it was
    // chosen: the home herd, not the far ones.
    let (sx, sy) = start.starts()[0];
    let home = sim::nav::centre((sx, sy));
    for g in &hunts {
        let at = start.world().pos[start.world().slot(*g).unwrap().index()];
        assert!(
            at.distance(home) < sim::Fx::from_int(ai::economy::HUNT_RANGE + 12),
            "hunted one far from home: {at:?}"
        );
    }
    sim.check().unwrap();
}

/// An opponent repairs a building an enemy damaged (`GD-AI-02`), once the
/// enemy is gone: a villager is sent with the same order a player gives,
/// and the building is whole again.
///
/// REQ: GD-AI-02
/// REQ: GD-BUILD-02
#[test]
fn an_opponent_repairs_a_building_once_the_raiders_are_gone() {
    let mut sim = Simulation::new(
        4,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Inland,
                size: 96,
                players: 2,
            },
            ..SimConfig::default()
        },
    );
    let (sx, sy) = sim.starts()[0];
    // A house of the opponent's, and three raiders of the other side's at it.
    sim.issue(sim::Command {
        player: 0,
        kind: sim::CommandKind::Spawn {
            kind: kinds::HOUSE,
            pos: sim::nav::centre((sx - 4, sy + 4)),
        },
    });
    for dx in 0..3 {
        sim.issue(sim::Command {
            player: 1,
            kind: sim::CommandKind::Spawn {
                kind: kinds::AXEMAN,
                pos: sim::nav::centre((sx - 7 + dx, sy + 7)),
            },
        });
    }
    for _ in 0..3 {
        sim.step();
    }
    let w = sim.world();
    let house = w
        .slots()
        .find(|s| w.owner[s.index()] == 0 && w.kind[s.index()] == kinds::HOUSE)
        .map(|s| w.id_at(s))
        .unwrap();
    let raiders: Vec<EntityId> = w
        .slots()
        .filter(|s| w.owner[s.index()] == 1 && w.kind[s.index()] == kinds::AXEMAN)
        .map(|s| w.id_at(s))
        .collect();
    sim.issue(sim::Command {
        player: 1,
        kind: sim::CommandKind::Attack {
            ids: raiders.clone(),
            target: house,
        },
    });
    let max = sim::Fx::from_int(kinds::info(kinds::HOUSE).max_health);
    let hi = sim.world().slot(house).unwrap().index();
    for _ in 0..2000 {
        if sim.world().health[hi] < max / 2 {
            break;
        }
        sim.step();
    }
    assert!(sim.world().health[hi] < max / 2, "the raid hurt it");
    for r in raiders {
        sim.issue(sim::Command {
            player: 1,
            kind: sim::CommandKind::Despawn { id: r },
        });
    }
    sim.step();

    let mut bot = Opponent::new(0, Difficulty::Standard, 4);
    let mut repaired_at = None;
    while sim.tick() < 4000 {
        let commands = {
            let view = FoggedView::new(&sim, 0);
            bot.think(&view)
        };
        for c in commands {
            c.validate().unwrap();
            sim.issue_from(c, Source::Ai);
        }
        sim.step();
        if sim.world().health[hi] >= max {
            repaired_at = Some(sim.tick());
            break;
        }
    }
    assert!(repaired_at.is_some(), "the house was mended");
    let asked = sim.replay().commands.iter().any(|(_, c)| {
        c.player == 0
            && matches!(c.kind, sim::CommandKind::Repair { building, .. } if building == house)
    });
    assert!(asked, "by a repair order");
    sim.check().unwrap();
}

//! Bounded combat trials driven entirely by recorded commands.
//!
//! These are regression fixtures, not an AI or an overall balance score.
use sim::{kinds, Command, CommandKind, EntityId, Fx, KindId, Replay, Simulation, Vec2Fx};

pub const BATTLE_LIMIT: u64 = 6_000;
pub const INACTIVE_LIMIT: u64 = 400;
pub const DEFAULT_TRIALS: u32 = 10;

pub fn config() -> sim::SimConfig {
    sim::SimConfig {
        map: sim::MapSpec {
            kind: sim::MapKind::Flat,
            size: 64,
            players: 2,
        },
        wander: false,
        ..sim::SimConfig::default()
    }
}

/// Mix melee, ranged and cavalry without requiring technology cheats.
/// Spawn commands are the same debug setup mechanism the existing corpus uses.
pub fn armies() -> [Vec<KindId>; 2] {
    [
        [
            vec![kinds::SPEARMAN; 20],
            vec![kinds::SLINGER; 10],
            vec![kinds::BOWMAN; 10],
        ]
        .concat(),
        [
            vec![kinds::AXEMAN; 20],
            vec![kinds::BOWMAN; 10],
            vec![kinds::LIGHT_CAVALRY; 10],
        ]
        .concat(),
    ]
}

/// Seed varies deployment spacing, offset and axis. Swapping exchanges
/// owners and starting sides, keeping each army's composition unchanged.
pub fn setup(rosters: &[Vec<KindId>; 2], seed: u64, swapped: bool) -> Simulation {
    let mut sim = Simulation::new(seed, config());
    let mut rng = sim::Rng::new(seed);
    let gap = 12 + rng.below(5) as i32;
    let offset = rng.range_i32(-3, 4);
    let rotate = rng.chance(1, 2);
    let point = |x, y| {
        if rotate {
            sim::nav::centre((y, x))
        } else {
            sim::nav::centre((x, y))
        }
    };
    for (army, roster) in rosters.iter().enumerate() {
        let player = (army ^ usize::from(swapped)) as u8;
        for (n, &kind) in roster.iter().enumerate() {
            let rank = n as i32 / 8;
            let x = if player == 0 {
                32 - gap / 2 - rank * 2
            } else {
                32 + gap / 2 + rank * 2
            };
            let y = 24 + (n as i32 % 8) * 2 + offset;
            sim.issue(Command {
                player,
                kind: CommandKind::Spawn {
                    kind,
                    pos: point(x, y),
                },
            });
        }
    }
    for _ in 0..3 {
        sim.step();
    }
    for player in 0..2 {
        let ids = living(&sim, player);
        assert_eq!(
            ids.len(),
            rosters[player as usize ^ usize::from(swapped)].len(),
            "all fighters spawned"
        );
        sim.issue(Command {
            player,
            kind: CommandKind::AttackMove {
                ids,
                target: point(32, 31 + offset),
            },
        });
    }
    sim
}

pub fn living(sim: &Simulation, player: u8) -> Vec<EntityId> {
    let w = sim.world();
    w.slots()
        .filter(|s| {
            w.owner[s.index()] == player
                && w.dying[s.index()] == 0
                && w.health[s.index()] > Fx::ZERO
        })
        .map(|s| w.id_at(s))
        .collect()
}

pub struct Battle {
    pub replay: Replay,
    pub survivors: [usize; 2],
    pub failure: Option<String>,
    pub longest_inactivity: u64,
}

impl Battle {
    pub fn winner(&self) -> Option<u8> {
        match self.survivors {
            [a, 0] if a > 0 => Some(0),
            [0, b] if b > 0 => Some(1),
            _ => None,
        }
    }
}

/// A unit makes progress by moving at least half a tile, firing/swinging
/// (reload increases), or taking damage. The timer is per unit, so damage
/// elsewhere cannot conceal one stationary soldier for twenty seconds.
pub fn finish(mut sim: Simulation) -> Battle {
    let w = sim.world();
    let mut progress: Vec<(EntityId, Vec2Fx, Fx, u16, u64)> = w
        .slots()
        .map(|s| {
            let i = s.index();
            (w.id_at(s), w.pos[i], w.health[i], w.reload[i], sim.tick())
        })
        .collect();
    let mut failure = None;
    let mut longest_inactivity = 0;
    let mut survivors;
    loop {
        survivors = [living(&sim, 0).len(), living(&sim, 1).len()];
        if survivors.contains(&0) {
            break;
        }
        if sim.tick() >= BATTLE_LIMIT {
            failure = Some(format!(
                "battle did not finish by tick {BATTLE_LIMIT}: {survivors:?} alive"
            ));
            break;
        }
        sim.step();
        if let Err(e) = sim.check() {
            failure = Some(format!("invariant at tick {}: {e}", sim.tick()));
            break;
        }
        let w = sim.world();
        for (id, pos, health, reload, active) in &mut progress {
            let Some(s) = w.slot(*id) else { continue };
            let i = s.index();
            if w.dying[i] > 0 {
                continue;
            }
            if pos.distance_sq_raw(w.pos[i])
                >= Vec2Fx::ZERO.distance_sq_raw(Vec2Fx::new(Fx::from_ratio(1, 2), Fx::ZERO))
                || w.reload[i] > *reload
                || w.health[i] < *health
            {
                *active = sim.tick();
                *pos = w.pos[i];
            }
            *health = w.health[i];
            *reload = w.reload[i];
            let idle = sim.tick() - *active;
            longest_inactivity = longest_inactivity.max(idle);
            if idle >= INACTIVE_LIMIT {
                failure = Some(format!(
                    "unit {id:?} inactive for {idle} ticks at {:?}, order {:?}",
                    w.pos[i], w.order[i]
                ));
                break;
            }
        }
        if failure.is_some() {
            break;
        }
    }
    Battle {
        replay: sim.replay(),
        survivors,
        failure,
        longest_inactivity,
    }
}

pub fn battle_40() -> Battle {
    finish(setup(&armies(), 1, false))
}

/// Equal total resource budgets, not equal headcounts: a Spearman is
/// cheaper than a Light Cavalry. Resource types have equal weight here;
/// production time, upgrades, terrain and micro are outside this check.
pub const MATCHUPS: &[(&str, KindId, KindId, i32)] = &[
    (
        "spearman-cavalry",
        kinds::SPEARMAN,
        kinds::LIGHT_CAVALRY,
        720,
    ),
    ("slinger-infantry", kinds::SLINGER, kinds::AXEMAN, 700),
];

pub fn roster_for_budget(kind: KindId, budget: i32) -> Vec<KindId> {
    let cost: i32 = kinds::info(kind).cost.iter().sum();
    vec![kind; (budget / cost) as usize]
}

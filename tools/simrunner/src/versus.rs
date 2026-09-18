//! Headless matches between computer opponents, decided (`docs/02` §10):
//! by elimination while the clock runs, by score at the time limit. The
//! M5 acceptance (`RM-M5-01`) is twenty of these, Hard against Easy, with
//! the invariants checked every tick and no villager left idle with work
//! in sight, and every outcome recorded so a regression is a diff.

use ai::{Difficulty, Opponent};
use fogged::FoggedView;
use sim::{kinds, EntityId, PlayerId, SimConfig, Simulation, Source};
use std::collections::BTreeMap;

/// Sixty seconds: a villager idle this long with a resource in sight is
/// a stuck unit.
pub const STUCK_TICKS: u64 = 1200;

/// One match to set up.
#[derive(Clone, Debug)]
pub struct Setup {
    /// The map seed.
    pub seed: u64,
    /// The time limit, in ticks.
    pub ticks: u64,
    /// Map size in tiles.
    pub size: u16,
    /// A difficulty per player, in player order.
    pub difficulties: Vec<Difficulty>,
}

impl Setup {
    /// The match's config. Hardest sides get the declared gather bonus.
    pub fn config(&self) -> SimConfig {
        SimConfig {
            map: sim::MapSpec {
                kind: sim::MapKind::Inland,
                size: self.size,
                players: self.difficulties.len() as u8,
            },
            gather_bonus_pct: self
                .difficulties
                .iter()
                .map(|d| match d {
                    Difficulty::Hardest => sim::HARDEST_GATHER_BONUS_PCT,
                    _ => 0,
                })
                .collect(),
            ..SimConfig::default()
        }
    }
}

/// How a match was decided.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    /// One side was the last standing.
    Elimination,
    /// The clock ran out and the higher score won.
    Score,
    /// The clock ran out on equal scores.
    Draw,
}

/// How a match went.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Outcome {
    /// The map seed.
    pub seed: u64,
    /// Ticks played: the limit, or fewer if it ended early.
    pub ticks: u64,
    /// Who won, if anyone.
    pub winner: Option<PlayerId>,
    /// How.
    pub decision: Decision,
    /// Every side's score at the end.
    pub scores: Vec<u64>,
    /// The final state hash.
    pub hash: u64,
}

impl Outcome {
    /// One line, stable across platforms, for the record file.
    pub fn line(&self) -> String {
        let winner = self.winner.map_or("draw".to_string(), |p| format!("p{p}"));
        let scores: Vec<String> = self.scores.iter().map(|s| s.to_string()).collect();
        format!(
            "seed {} ticks {} winner {} by {:?} scores {} hash {:016x}",
            self.seed,
            self.ticks,
            winner,
            self.decision,
            scores.join(","),
            self.hash
        )
    }
}

/// Runs one match to elimination or the time limit. Fails on a malformed
/// command, a broken invariant, or a stuck villager.
pub fn run(setup: &Setup) -> Result<Outcome, String> {
    let players = setup.difficulties.len() as u8;
    if !(2..=sim::MAX_PLAYERS as u8).contains(&players) {
        return Err(format!("{players} players: need 2..={}", sim::MAX_PLAYERS));
    }
    let config = setup.config();
    config.validate().map_err(|e| e.to_string())?;
    let mut sim = Simulation::new(setup.seed, config);
    let mut bots: Vec<Opponent> = setup
        .difficulties
        .iter()
        .enumerate()
        .map(|(p, d)| Opponent::new(p as u8, *d, setup.seed))
        .collect();
    let mut idle: BTreeMap<EntityId, u64> = BTreeMap::new();
    while sim.tick() < setup.ticks && sim.winner().is_none() {
        for bot in &mut bots {
            let commands = {
                let view = FoggedView::new(&sim, bot.player());
                bot.think(&view)
            };
            for c in commands {
                c.validate()
                    .map_err(|e| format!("seed {}: malformed command: {e:?}", setup.seed))?;
                sim.issue_from(c, Source::Ai);
            }
        }
        sim.step();
        sim.check()
            .map_err(|e| format!("seed {}: invariant at tick {}: {e}", setup.seed, sim.tick()))?;
        stuck_check(&sim, &mut idle)?;
    }
    let scores: Vec<u64> = (0..players).map(|p| sim.score(p)).collect();
    let (winner, decision) = match sim.winner() {
        Some(w) => (Some(w), Decision::Elimination),
        None => {
            let best = scores.iter().max().copied().unwrap_or(0);
            let leaders: Vec<PlayerId> = (0..players)
                .filter(|&p| sim.standing(p) && scores[p as usize] == best)
                .collect();
            match leaders.as_slice() {
                [only] => (Some(*only), Decision::Score),
                _ => (None, Decision::Draw),
            }
        }
    };
    Ok(Outcome {
        seed: setup.seed,
        ticks: sim.tick(),
        winner,
        decision,
        scores,
        hash: sim.state_hash(),
    })
}

/// A villager idle for [`STUCK_TICKS`] while its side can see a resource
/// with something left is a stuck unit (`docs/06` M5: "no stuck units").
fn stuck_check(sim: &Simulation, idle: &mut BTreeMap<EntityId, u64>) -> Result<(), String> {
    let mut now = BTreeMap::new();
    for p in 0..sim.players().len() as u8 {
        if !sim.standing(p) {
            continue;
        }
        for id in sim.idle_villagers(p) {
            let n = idle.get(&id).copied().unwrap_or(0) + 1;
            now.insert(id, n);
            if n > STUCK_TICKS && work_in_sight(sim, p) {
                return Err(format!(
                    "seed {}: player {p}'s villager {id:?} idle for {n} ticks at tick {} with work in sight",
                    sim.seed(),
                    sim.tick()
                ));
            }
        }
    }
    *idle = now;
    Ok(())
}

fn work_in_sight(sim: &Simulation, p: PlayerId) -> bool {
    let view = FoggedView::new(sim, p);
    view.sightings().iter().any(|s| {
        let info = kinds::info(s.kind);
        !info.mobile
            && s.resource.is_some_and(|(_, left)| left > 0)
            && (s.owner == kinds::GAIA || (s.owner == p && !s.site))
    })
}

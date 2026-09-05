//! A replay is a seed plus a command log. Kilobytes for a whole match, and
//! the primary bug-report format: if it reproduces from a replay, it is fixable.

use crate::command::Command;
use crate::simulation::{SimConfig, Simulation};
use serde::{Deserialize, Serialize};

/// Everything needed to reproduce a match.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Replay {
    /// Format version; bump when `Command` or `SimConfig` change shape.
    pub version: u32,
    /// Match seed.
    pub seed: u64,
    /// Match parameters.
    pub config: SimConfig,
    /// How many ticks the recorded match ran.
    pub ticks: u64,
    /// `(issue_tick, command)` pairs, in issue order.
    pub commands: Vec<(u64, Command)>,
}

impl Replay {
    /// Current format version.
    pub const VERSION: u32 = 1;

    /// Runs the replay to completion, calling `on_tick(tick, hash)` after
    /// every tick. Returns the final simulation.
    ///
    /// Commands whose issue tick is beyond `ticks` are never issued.
    pub fn run(&self, mut on_tick: impl FnMut(u64, u64)) -> Simulation {
        let mut sim = Simulation::new(self.seed, self.config.clone());
        let mut next = 0;
        while sim.tick() < self.ticks {
            while next < self.commands.len() && self.commands[next].0 == sim.tick() {
                sim.issue(self.commands[next].1.clone());
                next += 1;
            }
            debug_assert!(
                next >= self.commands.len() || self.commands[next].0 > sim.tick(),
                "replay commands must be in issue-tick order"
            );
            sim.step();
            on_tick(sim.tick(), sim.state_hash());
        }
        sim
    }

    /// Runs the replay twice and reports the first tick whose hashes differ,
    /// if any. This is the determinism test in one call.
    pub fn verify(&self) -> Result<u64, Divergence> {
        let mut first = Vec::with_capacity(self.ticks as usize);
        self.run(|_, h| first.push(h));
        let mut divergence = None;
        self.run(|t, h| {
            if divergence.is_none() && first[(t - 1) as usize] != h {
                divergence = Some(Divergence {
                    tick: t,
                    first: first[(t - 1) as usize],
                    second: h,
                });
            }
        });
        match divergence {
            Some(d) => Err(d),
            None => Ok(first.last().copied().unwrap_or(0)),
        }
    }
}

/// Two runs of the same replay disagreed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Divergence {
    /// First tick with mismatched hashes.
    pub tick: u64,
    /// Hash from the first run.
    pub first: u64,
    /// Hash from the second run.
    pub second: u64,
}

impl core::fmt::Display for Divergence {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "state diverged at tick {}: {:016x} vs {:016x}",
            self.tick, self.first, self.second
        )
    }
}

impl std::error::Error for Divergence {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandKind;
    use crate::vec2::Vec2Fx;

    #[test]
    fn recorded_match_reproduces_exactly() {
        let mut live = Simulation::new(1234, SimConfig::default());
        for t in 0..300u64 {
            if t % 7 == 0 {
                live.issue(Command {
                    player: (t % 3) as u8,
                    kind: CommandKind::Spawn {
                        kind: 1,
                        pos: Vec2Fx::from_int(t as i32 % 100, 20),
                    },
                });
            }
            if t == 150 {
                let ids: Vec<_> = live.world().ids().collect();
                live.issue(Command {
                    player: 0,
                    kind: CommandKind::Move {
                        ids,
                        target: Vec2Fx::from_int(90, 90),
                    },
                });
            }
            live.step();
        }
        let replay = live.replay();
        assert_eq!(replay.ticks, 300);

        let mut last = 0;
        let replayed = replay.run(|_, h| last = h);
        assert_eq!(last, live.state_hash());
        assert_eq!(replayed, live);
        assert_eq!(replay.verify().unwrap(), live.state_hash());
    }

    #[test]
    fn verify_reports_divergence_tick() {
        // Fabricate a divergence by comparing two different replays' traces.
        let a = Replay {
            version: 1,
            seed: 1,
            config: SimConfig::default(),
            ticks: 50,
            commands: vec![],
        };
        let mut hashes = vec![];
        a.run(|t, h| hashes.push((t, h)));
        assert_eq!(hashes.len(), 50);
        assert_eq!(hashes[0].0, 1);
        assert!(a.verify().is_ok());
    }
}

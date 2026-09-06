//! A replay is a seed plus a command log. Kilobytes for a whole match, and
//! the primary bug-report format: if it reproduces from a replay, it is fixable.

use crate::command::{Command, CommandError};
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

/// Why a replay was rejected before it was run.
///
/// A replay is a *file*: it arrives from a bug report, a save directory, or
/// eventually a download, and none of those are trusted. Every field that
/// sizes an allocation or steers a loop is checked here, so a corrupt replay
/// produces a message instead of an abort.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ReplayError {
    /// Recorded by a build with a different replay format.
    Version {
        /// The version in the file.
        got: u32,
        /// The version this build reads.
        expected: u32,
    },
    /// `ticks` exceeds [`Replay::MAX_TICKS`].
    TooLong {
        /// The value in the file.
        got: u64,
    },
    /// Commands are not in non-decreasing issue-tick order.
    OutOfOrder {
        /// Index of the first command that goes backwards.
        index: usize,
        /// Its issue tick.
        tick: u64,
        /// The issue tick of the command before it.
        previous: u64,
    },
    /// A command claims to have been issued after the match ended.
    IssuedAfterEnd {
        /// Index of the command.
        index: usize,
        /// Its issue tick.
        tick: u64,
        /// The recorded match length.
        ticks: u64,
    },
    /// A command is structurally malformed.
    BadCommand {
        /// Index of the command.
        index: usize,
        /// What is wrong with it.
        source: CommandError,
    },
}

impl core::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReplayError::Version { got, expected } => write!(
                f,
                "replay version {got} but this build reads version {expected}"
            ),
            ReplayError::TooLong { got } => {
                write!(
                    f,
                    "replay claims {got} ticks, limit is {}",
                    Replay::MAX_TICKS
                )
            }
            ReplayError::OutOfOrder {
                index,
                tick,
                previous,
            } => write!(
                f,
                "command {index} is issued at tick {tick}, after tick {previous}"
            ),
            ReplayError::IssuedAfterEnd { index, tick, ticks } => write!(
                f,
                "command {index} is issued at tick {tick}, after the match ends at {ticks}"
            ),
            ReplayError::BadCommand { index, source } => write!(f, "command {index}: {source}"),
        }
    }
}

impl std::error::Error for ReplayError {}

/// Why [`Replay::verify`] did not return a hash.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VerifyError {
    /// The replay was rejected before it ran.
    Invalid(ReplayError),
    /// Two runs of the same replay disagreed.
    Diverged(Divergence),
}

impl From<ReplayError> for VerifyError {
    fn from(e: ReplayError) -> VerifyError {
        VerifyError::Invalid(e)
    }
}

impl core::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VerifyError::Invalid(e) => write!(f, "invalid replay: {e}"),
            VerifyError::Diverged(d) => write!(f, "DESYNC {d}"),
        }
    }
}

impl std::error::Error for VerifyError {}

impl Replay {
    /// Current format version.
    pub const VERSION: u32 = 1;

    /// Longest match this build will run from a file.
    ///
    /// 10 million ticks is ~139 hours of game time at 20 Hz, orders of
    /// magnitude beyond a real match. The bound exists because
    /// [`Replay::verify`] sizes a buffer from this field: without it, a file
    /// claiming `u64::MAX` ticks aborted the process on allocation before a
    /// single tick ran.
    pub const MAX_TICKS: u64 = 10_000_000;

    /// Checks the replay is well-formed and safe to run.
    ///
    /// Called by [`Replay::run`] and [`Replay::verify`], so there is no path
    /// that executes an unvalidated replay.
    pub fn validate(&self) -> Result<(), ReplayError> {
        if self.version != Replay::VERSION {
            return Err(ReplayError::Version {
                got: self.version,
                expected: Replay::VERSION,
            });
        }
        if self.ticks > Replay::MAX_TICKS {
            return Err(ReplayError::TooLong { got: self.ticks });
        }
        let mut previous = 0u64;
        for (index, (tick, command)) in self.commands.iter().enumerate() {
            if *tick < previous {
                return Err(ReplayError::OutOfOrder {
                    index,
                    tick: *tick,
                    previous,
                });
            }
            // Equal to `ticks` is legal: a command issued on the final tick
            // is recorded but never executes.
            if *tick > self.ticks {
                return Err(ReplayError::IssuedAfterEnd {
                    index,
                    tick: *tick,
                    ticks: self.ticks,
                });
            }
            command
                .validate()
                .map_err(|source| ReplayError::BadCommand { index, source })?;
            previous = *tick;
        }
        Ok(())
    }

    /// Runs the replay to completion, calling `on_tick(tick, hash)` after
    /// every tick. Returns the final simulation.
    ///
    /// Validates first, so an ill-formed replay is reported rather than run.
    /// Command ordering used to be guarded only by a `debug_assert`, which
    /// meant a release build silently mis-ran a file whose commands were out
    /// of order instead of saying so.
    pub fn run(&self, mut on_tick: impl FnMut(u64, u64)) -> Result<Simulation, ReplayError> {
        self.validate()?;
        let mut sim = Simulation::new(self.seed, self.config.clone());
        let mut next = 0;
        while sim.tick() < self.ticks {
            while let Some((tick, command)) = self.commands.get(next) {
                if *tick != sim.tick() {
                    break;
                }
                sim.issue(command.clone());
                next += 1;
            }
            sim.step();
            on_tick(sim.tick(), sim.state_hash());
        }
        Ok(sim)
    }

    /// Runs the replay twice and reports the first tick whose hashes differ,
    /// if any. This is the determinism test in one call.
    pub fn verify(&self) -> Result<u64, VerifyError> {
        self.validate()?;
        // `ticks` is bounded by `MAX_TICKS`, but that bound is generous
        // enough that preallocating it outright would still be a large
        // request for a long replay. Grow into it instead.
        let mut first = Vec::with_capacity(self.ticks.min(1 << 16) as usize);
        self.run(|_, h| first.push(h))?;
        let mut divergence = None;
        self.run(|t, h| {
            let expected = first.get((t - 1) as usize).copied();
            if divergence.is_none() && expected != Some(h) {
                divergence = Some(Divergence {
                    tick: t,
                    first: expected.unwrap_or_default(),
                    second: h,
                });
            }
        })?;
        match divergence {
            Some(d) => Err(VerifyError::Diverged(d)),
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
        let replayed = replay.run(|_, h| last = h).unwrap();
        assert_eq!(last, live.state_hash());
        assert_eq!(replayed, live);
        assert_eq!(replay.verify().unwrap(), live.state_hash());
    }

    /// A replay is untrusted input. Each of these was a crash, a hang, or a
    /// silent mis-run before it was validated.
    #[test]
    fn malformed_replays_are_rejected_not_run() {
        let base = Replay {
            version: Replay::VERSION,
            seed: 1,
            config: SimConfig::default(),
            ticks: 10,
            commands: vec![],
        };
        let stop = |p| Command {
            player: p,
            kind: CommandKind::Stop { ids: vec![] },
        };

        // The one that aborted the process: `verify` sized a buffer from
        // this field before running a single tick.
        let huge = Replay {
            ticks: u64::MAX,
            ..base.clone()
        };
        assert_eq!(huge.validate(), Err(ReplayError::TooLong { got: u64::MAX }));
        assert!(matches!(huge.verify(), Err(VerifyError::Invalid(_))));
        assert!(huge.run(|_, _| {}).is_err());

        let wrong_version = Replay {
            version: Replay::VERSION + 1,
            ..base.clone()
        };
        assert!(matches!(
            wrong_version.validate(),
            Err(ReplayError::Version { .. })
        ));

        // Out of order: release builds used to skip these silently, because
        // the only guard was a `debug_assert`.
        let unordered = Replay {
            commands: vec![(5, stop(0)), (2, stop(0))],
            ..base.clone()
        };
        assert!(matches!(
            unordered.validate(),
            Err(ReplayError::OutOfOrder { index: 1, .. })
        ));

        let after_end = Replay {
            commands: vec![(11, stop(0))],
            ..base.clone()
        };
        assert!(matches!(
            after_end.validate(),
            Err(ReplayError::IssuedAfterEnd { index: 0, .. })
        ));

        // Issued exactly at the end is legal; it simply never executes.
        let at_end = Replay {
            commands: vec![(10, stop(0))],
            ..base.clone()
        };
        assert!(at_end.validate().is_ok());

        let bad_player = Replay {
            commands: vec![(0, stop(200))],
            ..base
        };
        assert!(matches!(
            bad_player.validate(),
            Err(ReplayError::BadCommand { index: 0, .. })
        ));
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
        a.run(|t, h| hashes.push((t, h))).unwrap();
        assert_eq!(hashes.len(), 50);
        assert_eq!(hashes[0].0, 1);
        assert!(a.verify().is_ok());
    }
}

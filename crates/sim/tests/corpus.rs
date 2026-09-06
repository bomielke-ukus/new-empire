//! Replays the committed corpus and compares against the committed digests.
//!
//! Two things are being tested, and they are not the same thing:
//!
//! 1. **Determinism** — the same replay produces the same result twice. That
//!    is what `Replay::verify` checks, and it catches state leaking in from
//!    outside the simulation.
//! 2. **Stability** — the simulation still produces the result it produced
//!    when the corpus was recorded. Nothing else checks this. A refactor that
//!    is perfectly deterministic and quietly changes what a unit does would
//!    otherwise pass every test in the repository, and would invalidate every
//!    replay and save file in existence without saying so.
//!
//! The `.golden` files are the second check. When one changes, that is a
//! behaviour change: either it was intended, in which case
//! `cargo run -p simrunner -- golden --update` records it and the diff goes
//! through review like any other, or it was not, in which case the diff just
//! caught a bug.
//!
//! `simrunner golden` runs the same comparison from the command line so CI can
//! run it under other build profiles; this test exists so a contributor
//! running `cargo test` cannot miss it.
//!
//! The scenarios behind these entries are defined in
//! `tools/simrunner/src/scenarios.rs` and drive M2's real systems — gathering
//! and drop-off, construction, training, group pathing and separation. A
//! corpus that only moved units around would not notice a change to the
//! economy, which is most of what the simulation now does.

use sim::{Replay, Trace};
use std::path::{Path, PathBuf};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

/// Every `<name>.ron` in the corpus, paired with its `<name>.golden`.
fn entries() -> Vec<(String, PathBuf, PathBuf)> {
    let dir = corpus_dir();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("corpus directory is missing") {
        let path = entry.expect("unreadable corpus entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("ron") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("corpus file has no name")
            .to_string();
        let golden = dir.join(format!("{name}.golden"));
        out.push((name, path, golden));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn load(path: &Path) -> Replay {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));
    let replay: Replay =
        ron::from_str(&text).unwrap_or_else(|e| panic!("could not parse {}: {e}", path.display()));
    replay
}

#[test]
fn corpus_is_not_empty() {
    let entries = entries();
    assert!(
        entries.len() >= 10,
        "the corpus has shrunk to {} entries; entries are added when a bug is \
         found and are not removed",
        entries.len()
    );
    for (name, _, golden) in &entries {
        assert!(
            golden.exists(),
            "corpus entry `{name}` has no committed digest; run \
             `cargo run -p simrunner -- golden --update`"
        );
    }
}

// REQ: TA-DET-06
#[test]
fn every_corpus_replay_is_well_formed() {
    for (name, path, _) in entries() {
        let replay = load(&path);
        replay
            .validate()
            .unwrap_or_else(|e| panic!("corpus entry `{name}` is malformed: {e}"));
        assert_eq!(replay.version, Replay::VERSION, "`{name}` version");
    }
}

// REQ: TA-DET-07
/// Stability: the behaviour recorded in the corpus is the behaviour this build
/// produces.
#[test]
fn corpus_matches_committed_digests() {
    let mut failures = Vec::new();
    for (name, path, golden_path) in entries() {
        let replay = load(&path);
        let got = match replay.trace_digest() {
            Ok(t) => t,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let text = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", golden_path.display()));
        let want: Trace = ron::from_str(&text)
            .unwrap_or_else(|e| panic!("could not parse {}: {e}", golden_path.display()));
        if got != want {
            failures.push(format!("{name}\n     expected {want}\n     got      {got}"));
        }
    }
    assert!(
        failures.is_empty(),
        "the simulation no longer reproduces the recorded corpus:\n  {}\n\n\
         Every replay and save file recorded against the previous behaviour is \
         now invalid. If the change was intended, run\n    \
         cargo run -p simrunner -- golden --update\n\
         and put the digest diff in the commit so it is reviewed deliberately.",
        failures.join("\n  ")
    );
}

/// Determinism: the same replay, twice, in this process.
#[test]
fn corpus_replays_are_deterministic() {
    for (name, path, _) in entries() {
        let replay = load(&path);
        replay
            .verify()
            .unwrap_or_else(|e| panic!("corpus entry `{name}`: {e}"));
    }
}

/// Every invariant, at every tick, over the whole corpus. Slower than the
/// digest comparison and worth it: when a digest changes, this is what says
/// *why*.
#[test]
fn corpus_holds_every_invariant_at_every_tick() {
    for (name, path, _) in entries() {
        let replay = load(&path);
        let mut sim = sim::Simulation::new(replay.seed, replay.config.clone());
        let mut next = 0;
        while sim.tick() < replay.ticks {
            while let Some((tick, command)) = replay.commands.get(next) {
                if *tick != sim.tick() {
                    break;
                }
                sim.issue(command.clone());
                next += 1;
            }
            sim.step();
            if let Err(v) = sim.check() {
                panic!("`{name}` broke an invariant at tick {}: {v}", sim.tick());
            }
        }
    }
}

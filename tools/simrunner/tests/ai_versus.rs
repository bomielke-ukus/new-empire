//! The acceptance harness: a match between opponents is decided and
//! recorded in one line, the same line every time and the same line the
//! committed record holds for that seed.
//!
//! REQ: RM-M5-01

use ai::Difficulty;
use simrunner::versus::{self, Decision, Setup};

/// The first line of the committed acceptance record, which CI runs the
/// full twenty of (`.github/workflows/ci.yml`): played here in full, so a
/// change to the opponents that moves the record fails a test before it
/// fails the job.
#[test]
fn the_first_recorded_match_plays_out_as_recorded() {
    let record = include_str!("versus-hard-easy.golden");
    let first = record
        .lines()
        .find(|l| l.starts_with("seed "))
        .expect("a recorded match");
    let ticks: u64 = first
        .split_whitespace()
        .nth(3)
        .and_then(|t| t.parse().ok())
        .expect("ticks in the record");
    let setup = Setup {
        seed: 1,
        ticks: ticks.max(36_000),
        size: 96,
        difficulties: vec![Difficulty::Hard, Difficulty::Easy],
    };
    let outcome = versus::run(&setup).expect("a clean match");
    assert_eq!(outcome.line(), first);
    assert_eq!(outcome.winner, Some(0), "Hard wins seed 1");
}

/// A match with nobody able to fight ends at the limit on score, and a
/// side that is gone loses by elimination.
#[test]
fn a_match_is_decided_by_elimination_or_by_score() {
    let short = Setup {
        seed: 3,
        ticks: 600,
        size: 96,
        difficulties: vec![Difficulty::Standard, Difficulty::Easy],
    };
    let outcome = versus::run(&short).expect("a clean match");
    assert_eq!(outcome.ticks, 600);
    assert!(matches!(outcome.decision, Decision::Score | Decision::Draw));
    assert_eq!(outcome.scores.len(), 2);
    let again = versus::run(&short).expect("a clean match");
    assert_eq!(outcome, again, "the same setup plays the same match");
    assert!(outcome.line().starts_with("seed 3 ticks 600 winner "));
}

//! Property tests for the random number generator.
//!
//! The generator is the one source of variation in the simulation, so its
//! output stream is part of the replay format in everything but name.
//!
//! (The command-queue and entity-store property tests from the same original
//! file are held back: both need updating for the components and command
//! variants M1 and M2 added.)

use proptest::prelude::*;
use sim::{Fx, Rng};

/// The generator is the one source of variation in the simulation, so its
/// output stream is part of the replay format in everything but name. If a
/// refactor changes these numbers, every replay ever recorded becomes
/// unplayable — silently. This vector is the tripwire.
///
/// Values are this implementation's own output, captured deliberately.
#[test]
fn rng_stream_is_frozen() {
    let mut rng = Rng::new(1);
    let got: Vec<u64> = (0..8).map(|_| rng.next_u64()).collect();
    let expected: [u64; 8] = [
        0xB3F2AF6D0FC710C5,
        0x853B559647364CEA,
        0x92F89756082A4514,
        0x642E1C7BC266A3A7,
        0xB27A48E29A233673,
        0x24C123126FFDA722,
        0x123004EF8DF510E6,
        0x61954DCC47B1E89D,
    ];
    assert_eq!(
        got, expected,
        "the RNG stream changed; every existing replay is now invalid"
    );
    assert_eq!(rng.draws(), 8);
}

proptest! {
    #[test]
    fn below_is_always_in_range(seed in any::<u64>(), n in any::<u32>(), calls in 1usize..50) {
        let mut rng = Rng::new(seed);
        for _ in 0..calls {
            let v = rng.below(n);
            if n == 0 {
                prop_assert_eq!(v, 0, "below(0) must be defined, not a divide by zero");
            } else {
                prop_assert!(v < n, "below({n}) returned {v}");
            }
        }
    }

    #[test]
    fn range_i32_is_always_in_range(seed in any::<u64>(), lo in any::<i32>(), hi in any::<i32>()) {
        let mut rng = Rng::new(seed);
        let v = rng.range_i32(lo, hi);
        if hi <= lo {
            prop_assert_eq!(v, lo, "an empty range must yield its bound, not panic");
        } else {
            prop_assert!(v >= lo && v < hi, "range_i32({lo}, {hi}) returned {v}");
        }
    }

    #[test]
    fn unit_fx_is_in_the_half_open_unit_interval(seed in any::<u64>(), calls in 1usize..50) {
        let mut rng = Rng::new(seed);
        for _ in 0..calls {
            let v = rng.unit_fx();
            prop_assert!(v >= Fx::ZERO && v < Fx::ONE, "unit_fx returned {v:?}");
        }
    }

    #[test]
    fn chance_respects_its_bounds(seed in any::<u64>(), den in 1u32..1000) {
        let mut rng = Rng::new(seed);
        for _ in 0..64 {
            prop_assert!(!rng.chance(0, den), "chance(0, {den}) fired");
            prop_assert!(rng.chance(den, den), "chance({den}, {den}) did not fire");
        }
    }

    /// One draw per call, always. Desync diagnosis works by comparing draw
    /// counts to localise where two machines parted company, which only tells
    /// you anything if the count is a function of the code path taken.
    #[test]
    fn every_call_draws_exactly_once(seed in any::<u64>()) {
        let mut rng = Rng::new(seed);
        prop_assert_eq!(rng.draws(), 0);
        rng.next_u64();
        prop_assert_eq!(rng.draws(), 1);
        rng.next_u32();
        prop_assert_eq!(rng.draws(), 2);
        rng.below(10);
        prop_assert_eq!(rng.draws(), 3);
        rng.below(0);
        prop_assert_eq!(rng.draws(), 3, "below(0) short-circuits without drawing");
        rng.range_i32(0, 10);
        prop_assert_eq!(rng.draws(), 4);
        rng.range_i32(5, 5);
        prop_assert_eq!(rng.draws(), 4, "an empty range short-circuits without drawing");
        rng.unit_fx();
        prop_assert_eq!(rng.draws(), 5);
        rng.chance(1, 2);
        prop_assert_eq!(rng.draws(), 6);
    }

    #[test]
    fn rng_serde_round_trip_preserves_the_stream(seed in any::<u64>(), warmup in 0usize..40) {
        let mut rng = Rng::new(seed);
        for _ in 0..warmup {
            rng.next_u64();
        }
        let text = ron::to_string(&rng).unwrap();
        let mut restored: Rng = ron::from_str(&text).unwrap();
        prop_assert_eq!(restored.draws(), rng.draws());
        let a: Vec<u64> = (0..16).map(|_| rng.next_u64()).collect();
        let b: Vec<u64> = (0..16).map(|_| restored.next_u64()).collect();
        prop_assert_eq!(a, b);
    }

    /// Two seeds must not collapse onto the same stream. `Rng::new` runs the
    /// seed through splitmix64 precisely so that adjacent match seeds do not
    /// produce correlated matches.
    #[test]
    fn adjacent_seeds_give_different_streams(seed in any::<u64>()) {
        let mut a = Rng::new(seed);
        let mut b = Rng::new(seed.wrapping_add(1));
        let sa: Vec<u64> = (0..4).map(|_| a.next_u64()).collect();
        let sb: Vec<u64> = (0..4).map(|_| b.next_u64()).collect();
        prop_assert_ne!(sa, sb);
    }
}

/// A statistical smoke test, not a randomness certification: it exists to
/// catch a range-reduction bug that biases everything toward one end, which is
/// the failure mode that would actually reach players (every villager
/// wandering the same way).
#[test]
fn below_is_roughly_uniform() {
    const BUCKETS: u32 = 16;
    const DRAWS: u32 = 160_000;
    let mut rng = Rng::new(0xC0FFEE);
    let mut counts = [0u32; BUCKETS as usize];
    for _ in 0..DRAWS {
        counts[rng.below(BUCKETS) as usize] += 1;
    }
    let expected = DRAWS / BUCKETS;
    for (i, &c) in counts.iter().enumerate() {
        let deviation = (c as i64 - expected as i64).abs();
        assert!(
            deviation < expected as i64 / 10,
            "bucket {i} got {c}, expected about {expected}"
        );
    }
}

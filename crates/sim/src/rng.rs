//! The simulation's one and only random number generator.
//!
//! `xoshiro256**`, seeded through `splitmix64`. Hand-written rather than
//! pulled from a crate so that the sim's dependency list stays empty of
//! anything whose output could change under a version bump. Its state is part
//! of the simulation state and is included in the state hash.

use crate::fx::Fx;
use crate::hash::{HashState, StateHasher};
use serde::{Deserialize, Serialize};

/// Deterministic PRNG (`xoshiro256**`).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rng {
    s: [u64; 4],
    /// Number of draws so far. Purely diagnostic — it makes "where did the
    /// two runs diverge" answerable from a hash.
    draws: u64,
}

/// `splitmix64` step; used only for seeding.
const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    /// Creates a generator from a 64-bit seed.
    pub const fn new(seed: u64) -> Rng {
        let mut st = seed;
        let s = [
            splitmix64(&mut st),
            splitmix64(&mut st),
            splitmix64(&mut st),
            splitmix64(&mut st),
        ];
        Rng { s, draws: 0 }
    }

    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        self.draws += 1;
        result
    }

    /// Next 32 random bits (the high half; xoshiro's low bits are weaker).
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in `0..n`. Returns 0 for `n == 0`.
    ///
    /// Multiply-shift range reduction: a negligible bias for game purposes,
    /// and — unlike rejection sampling — a fixed one draw per call, which
    /// keeps draw counts predictable when debugging desyncs.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        ((self.next_u32() as u64 * n as u64) >> 32) as u32
    }

    /// Uniform in `lo..hi`. Returns `lo` if the range is empty.
    pub fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo.wrapping_add(self.below((hi as i64 - lo as i64) as u32) as i32)
    }

    /// Uniform fixed-point value in `[0, 1)`.
    pub fn unit_fx(&mut self) -> Fx {
        Fx::from_raw((self.next_u32() >> 16) as i32)
    }

    /// Uniform fixed-point value in `[lo, hi)`.
    pub fn range_fx(&mut self, lo: Fx, hi: Fx) -> Fx {
        lo + (hi - lo) * self.unit_fx()
    }

    /// True with probability `num / den`.
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        self.below(den) < num
    }

    /// How many values have been drawn since seeding.
    pub fn draws(&self) -> u64 {
        self.draws
    }
}

impl HashState for Rng {
    fn hash_state(&self, h: &mut StateHasher) {
        for w in self.s {
            h.write_u64(w);
        }
        h.write_u64(self.draws);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_known_answer() {
        // Reference value from the splitmix64 paper/implementation for seed 0.
        let mut st = 0u64;
        assert_eq!(splitmix64(&mut st), 0xE220_A839_7B1D_CDAF);
    }

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_differ() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        let same = (0..64).filter(|_| a.next_u64() == b.next_u64()).count();
        assert_eq!(same, 0);
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut r = Rng::new(7);
        let mut seen = [false; 10];
        for _ in 0..10_000 {
            let v = r.below(10);
            assert!(v < 10);
            seen[v as usize] = true;
        }
        assert!(seen.iter().all(|&s| s));
        assert_eq!(r.below(0), 0);
        assert_eq!(r.below(1), 0);
    }

    #[test]
    fn range_i32_handles_negatives_and_empty() {
        let mut r = Rng::new(9);
        for _ in 0..1000 {
            let v = r.range_i32(-5, 5);
            assert!((-5..5).contains(&v));
        }
        assert_eq!(r.range_i32(3, 3), 3);
        assert_eq!(r.range_i32(4, 3), 4);
    }

    #[test]
    fn unit_fx_is_in_half_open_interval() {
        let mut r = Rng::new(11);
        for _ in 0..10_000 {
            let v = r.unit_fx();
            assert!(v >= Fx::ZERO && v < Fx::ONE);
        }
        let lo = Fx::from_int(2);
        let hi = Fx::from_int(4);
        for _ in 0..1000 {
            let v = r.range_fx(lo, hi);
            assert!(v >= lo && v < hi, "{v}");
        }
    }

    #[test]
    fn chance_is_roughly_calibrated() {
        let mut r = Rng::new(5);
        let hits = (0..10_000).filter(|_| r.chance(1, 4)).count();
        assert!((2200..2800).contains(&hits), "{hits}");
        assert!(!r.chance(0, 10));
        assert!(r.chance(10, 10));
    }

    #[test]
    fn draw_count_tracks_every_call() {
        let mut r = Rng::new(1);
        r.next_u64();
        r.below(5);
        r.unit_fx();
        r.chance(1, 2);
        assert_eq!(r.draws(), 4);
    }
}

//! Angles as 16-bit binary angular measurement (BAM).
//!
//! A full turn is 65536 units, so wrap-around is free integer overflow and
//! there is no π to approximate. Zero points along +x and angles increase
//! toward +y. Sine and cosine come from a committed quarter-wave table with
//! linear interpolation — no `libm`, so no platform drift.

use crate::fx::Fx;
use crate::trig_table::{QUARTER_STEPS, SIN_QUARTER};
use core::ops::{Add, AddAssign, Neg, Sub, SubAssign};
use serde::{Deserialize, Serialize};

/// An angle in 1/65536ths of a turn.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Debug, Serialize, Deserialize,
)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Angle(pub u16);

/// BAM units in a quarter turn.
const QUARTER: u32 = 1 << 14;
/// Bits of BAM below one table step.
const STEP_SHIFT: u32 = 14 - QUARTER_STEPS.trailing_zeros();
const STEP_MASK: u32 = (1 << STEP_SHIFT) - 1;

impl Angle {
    /// 0 turns, along +x.
    pub const ZERO: Angle = Angle(0);
    /// A quarter turn (90°).
    pub const QUARTER: Angle = Angle(QUARTER as u16);
    /// A half turn (180°).
    pub const HALF: Angle = Angle(2 * QUARTER as u16);
    /// Three quarters of a turn (270°).
    pub const THREE_QUARTER: Angle = Angle(3 * QUARTER as u16);

    /// From whole degrees; any integer is accepted and wrapped.
    pub const fn from_degrees(deg: i32) -> Angle {
        let d = deg.rem_euclid(360) as i64;
        Angle((d * 65536 / 360) as u16)
    }

    /// The angle in whole degrees, `0..360`.
    pub const fn to_degrees(self) -> u32 {
        (self.0 as u32 * 360) >> 16
    }

    /// Sine, in `[-1, 1]`.
    pub fn sin(self) -> Fx {
        let a = self.0 as u32;
        let quadrant = a >> 14;
        let q = a & (QUARTER - 1);
        let raw = match quadrant {
            0 => quarter_sin(q),
            1 => quarter_sin(QUARTER - q),
            2 => -quarter_sin(q),
            _ => -quarter_sin(QUARTER - q),
        };
        Fx::from_raw(raw)
    }

    /// Cosine, in `[-1, 1]`.
    pub fn cos(self) -> Fx {
        (self + Angle::QUARTER).sin()
    }

    /// Quantises to one of 8 facings: 0 = +x, 1 = +x+y diagonal, … 7.
    /// Each facing owns the 45° sector centred on it.
    pub const fn facing8(self) -> u8 {
        (((self.0 as u32) + (1 << 12)) >> 13) as u8 & 7
    }
}

/// Sine over the first quarter turn, `q` in `0..=QUARTER`, as raw Q16.16.
fn quarter_sin(q: u32) -> i32 {
    let idx = (q >> STEP_SHIFT) as usize;
    if idx >= QUARTER_STEPS as usize {
        return SIN_QUARTER[QUARTER_STEPS as usize];
    }
    let frac = (q & STEP_MASK) as i32;
    let a = SIN_QUARTER[idx];
    let b = SIN_QUARTER[idx + 1];
    a + (((b - a) * frac) >> STEP_SHIFT)
}

impl Add for Angle {
    type Output = Angle;
    fn add(self, rhs: Angle) -> Angle {
        Angle(self.0.wrapping_add(rhs.0))
    }
}
impl Sub for Angle {
    type Output = Angle;
    fn sub(self, rhs: Angle) -> Angle {
        Angle(self.0.wrapping_sub(rhs.0))
    }
}
impl Neg for Angle {
    type Output = Angle;
    fn neg(self) -> Angle {
        Angle(self.0.wrapping_neg())
    }
}
impl AddAssign for Angle {
    fn add_assign(&mut self, rhs: Angle) {
        *self = *self + rhs;
    }
}
impl SubAssign for Angle {
    fn sub_assign(&mut self, rhs: Angle) {
        *self = *self - rhs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One-thousandth, the tolerance for interpolated values.
    const TOL: Fx = Fx::from_raw(66);

    fn close(a: Fx, b: Fx) -> bool {
        (a - b).abs() <= TOL
    }

    #[test]
    fn cardinal_points_are_exact() {
        assert_eq!(Angle::ZERO.sin(), Fx::ZERO);
        assert_eq!(Angle::ZERO.cos(), Fx::ONE);
        assert_eq!(Angle::QUARTER.sin(), Fx::ONE);
        assert_eq!(Angle::QUARTER.cos(), Fx::ZERO);
        assert_eq!(Angle::HALF.sin(), Fx::ZERO);
        assert_eq!(Angle::HALF.cos(), -Fx::ONE);
        assert_eq!(Angle::THREE_QUARTER.sin(), -Fx::ONE);
        assert_eq!(Angle::THREE_QUARTER.cos(), Fx::ZERO);
    }

    #[test]
    fn degrees() {
        assert_eq!(Angle::from_degrees(0), Angle::ZERO);
        assert_eq!(Angle::from_degrees(90), Angle::QUARTER);
        assert_eq!(Angle::from_degrees(180), Angle::HALF);
        assert_eq!(Angle::from_degrees(360), Angle::ZERO);
        assert_eq!(Angle::from_degrees(-90), Angle::THREE_QUARTER);
        assert_eq!(Angle::from_degrees(450), Angle::QUARTER);
        assert_eq!(Angle::QUARTER.to_degrees(), 90);
        assert_eq!(Angle::from_degrees(359).to_degrees(), 358); // truncation, not rounding
    }

    #[test]
    fn known_values() {
        assert!(close(Angle::from_degrees(30).sin(), Fx::HALF));
        assert!(close(Angle::from_degrees(60).cos(), Fx::HALF));
        assert!(close(Angle::from_degrees(45).sin(), Fx::from_raw(46341)));
        assert!(close(Angle::from_degrees(210).sin(), -Fx::HALF));
        assert!(close(Angle::from_degrees(300).cos(), Fx::HALF));
    }

    #[test]
    fn odd_symmetry_is_exact() {
        for a in (0..=u16::MAX).step_by(37) {
            let a = Angle(a);
            assert_eq!((-a).sin(), -a.sin(), "{a:?}");
        }
    }

    #[test]
    fn pythagorean_identity_holds_everywhere() {
        for a in (0..=u16::MAX).step_by(13) {
            let a = Angle(a);
            let s = a.sin();
            let c = a.cos();
            let one = s * s + c * c;
            assert!(close(one, Fx::ONE), "{a:?}: {one}");
        }
    }

    #[test]
    fn monotonic_over_first_quarter() {
        let mut prev = Fx::ZERO;
        for a in 0..=(1u32 << 14) {
            let s = Angle(a as u16).sin();
            assert!(s >= prev, "sin not monotonic at {a}");
            prev = s;
        }
    }

    #[test]
    fn wrapping_arithmetic() {
        assert_eq!(Angle(65000) + Angle(1000), Angle(464));
        assert_eq!(Angle(100) - Angle(200), Angle(65436));
        assert_eq!(-Angle::QUARTER, Angle::THREE_QUARTER);
    }

    #[test]
    fn facing8() {
        assert_eq!(Angle::ZERO.facing8(), 0);
        assert_eq!(Angle::from_degrees(22).facing8(), 0);
        assert_eq!(Angle::from_degrees(23).facing8(), 1);
        assert_eq!(Angle::from_degrees(45).facing8(), 1);
        assert_eq!(Angle::QUARTER.facing8(), 2);
        assert_eq!(Angle::HALF.facing8(), 4);
        assert_eq!(Angle::THREE_QUARTER.facing8(), 6);
        assert_eq!(Angle::from_degrees(340).facing8(), 0);
        assert_eq!(Angle::from_degrees(337).facing8(), 7);
    }
}

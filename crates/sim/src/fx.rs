//! `Fx`: a signed Q16.16 fixed-point number.
//!
//! 16 integer bits, 16 fractional bits, stored in an `i32`. Range is
//! ±32767.99998, resolution 1/65536. That covers every quantity the simulation
//! needs — tile coordinates up to 240, hit points, gather rates, speeds — and
//! the arithmetic is plain integer maths, so it is identical on every CPU.
//!
//! Arithmetic **saturates** on overflow rather than wrapping or panicking:
//! a saturated value is a bug, but it is a *deterministic* bug that a state
//! hash will catch, whereas a panic takes the whole match down.
//!
//! There are deliberately no conversions to or from `f32`/`f64` here. The
//! presentation layer does `fx.raw() as f32 / 65536.0` itself.

use core::cmp::Ordering;
use core::fmt;
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};
use serde::{Deserialize, Serialize};

/// Signed Q16.16 fixed-point number.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Fx(i32);

impl Fx {
    /// Number of fractional bits.
    pub const FRAC_BITS: u32 = 16;
    /// Raw representation of 1.0.
    pub const ONE_RAW: i32 = 1 << Self::FRAC_BITS;

    /// 0.0
    pub const ZERO: Fx = Fx(0);
    /// 1.0
    pub const ONE: Fx = Fx(Self::ONE_RAW);
    /// 0.5
    pub const HALF: Fx = Fx(Self::ONE_RAW / 2);
    /// 2.0
    pub const TWO: Fx = Fx(Self::ONE_RAW * 2);
    /// Largest representable value, ≈ 32767.99998.
    pub const MAX: Fx = Fx(i32::MAX);
    /// Smallest representable value, -32768.0.
    pub const MIN: Fx = Fx(i32::MIN);
    /// Smallest positive value, 1/65536.
    pub const EPSILON: Fx = Fx(1);

    /// Wraps a raw Q16.16 bit pattern.
    #[inline]
    pub const fn from_raw(raw: i32) -> Fx {
        Fx(raw)
    }

    /// The raw Q16.16 bit pattern.
    #[inline]
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Converts an integer, saturating outside ±32767.
    #[inline]
    pub const fn from_int(v: i32) -> Fx {
        if v > i16::MAX as i32 {
            Fx::MAX
        } else if v < i16::MIN as i32 {
            Fx::MIN
        } else {
            Fx(v << Self::FRAC_BITS)
        }
    }

    /// `num / den` as a fixed-point value, e.g. `Fx::from_ratio(45, 100)` is 0.45.
    ///
    /// Panics if `den == 0`. Saturates on overflow.
    #[inline]
    pub const fn from_ratio(num: i32, den: i32) -> Fx {
        assert!(den != 0, "Fx::from_ratio: zero denominator");
        saturate(div_round((num as i64) << Self::FRAC_BITS, den as i64))
    }

    /// Largest integer ≤ self.
    #[inline]
    pub const fn floor(self) -> i32 {
        self.0 >> Self::FRAC_BITS
    }

    /// Smallest integer ≥ self.
    #[inline]
    pub const fn ceil(self) -> i32 {
        let f = self.floor();
        if self.0 & (Self::ONE_RAW - 1) == 0 {
            f
        } else {
            f + 1
        }
    }

    /// Nearest integer, halves rounding up (toward +∞).
    #[inline]
    pub const fn round(self) -> i32 {
        Fx(self.0.saturating_add(Self::ONE_RAW / 2)).floor()
    }

    /// Integer part, rounding toward zero.
    #[inline]
    pub const fn trunc(self) -> i32 {
        if self.0 >= 0 {
            self.floor()
        } else {
            -Fx(-self.0).floor()
        }
    }

    /// Fractional part in `[0, 1)`, such that `self == floor + frac`.
    #[inline]
    pub const fn frac(self) -> Fx {
        Fx(self.0 & (Self::ONE_RAW - 1))
    }

    /// Absolute value. `Fx::MIN.abs()` saturates to `Fx::MAX`.
    #[inline]
    pub const fn abs(self) -> Fx {
        Fx(self.0.saturating_abs())
    }

    /// -1, 0 or 1.
    #[inline]
    pub const fn signum(self) -> Fx {
        Fx::from_int(self.0.signum())
    }

    /// True if exactly zero.
    #[inline]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// True if strictly negative.
    #[inline]
    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// True if strictly positive.
    #[inline]
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    /// The smaller of two values.
    #[inline]
    pub fn min(self, other: Fx) -> Fx {
        if self <= other {
            self
        } else {
            other
        }
    }

    /// The larger of two values.
    #[inline]
    pub fn max(self, other: Fx) -> Fx {
        if self >= other {
            self
        } else {
            other
        }
    }

    /// Clamps into `[lo, hi]`. Panics if `lo > hi`.
    #[inline]
    pub fn clamp(self, lo: Fx, hi: Fx) -> Fx {
        assert!(lo <= hi, "Fx::clamp: lo > hi");
        self.max(lo).min(hi)
    }

    /// Saturating multiplication, rounded to nearest.
    #[inline]
    pub const fn saturating_mul(self, other: Fx) -> Fx {
        let prod = self.0 as i64 * other.0 as i64;
        saturate((prod + (1 << (Self::FRAC_BITS - 1))) >> Self::FRAC_BITS)
    }

    /// Division, rounded to nearest. `None` if `other` is zero.
    #[inline]
    pub const fn checked_div(self, other: Fx) -> Option<Fx> {
        if other.0 == 0 {
            None
        } else {
            Some(saturate(div_round(
                (self.0 as i64) << Self::FRAC_BITS,
                other.0 as i64,
            )))
        }
    }

    /// `self * num / den` computed in one 64-bit step, so the intermediate
    /// product cannot overflow. Rounded to nearest. Panics if `den` is zero.
    #[inline]
    pub const fn mul_div(self, num: Fx, den: Fx) -> Fx {
        assert!(den.0 != 0, "Fx::mul_div: zero denominator");
        saturate(div_round(self.0 as i64 * num.0 as i64, den.0 as i64))
    }

    /// Square root. Negative inputs return zero (there is no NaN here, and a
    /// panic would take the match down).
    pub fn sqrt(self) -> Fx {
        if self.0 <= 0 {
            return Fx::ZERO;
        }
        // sqrt(x) in Q16.16 == isqrt(x_raw << 16), because
        // sqrt(raw / 2^16) * 2^16 == sqrt(raw * 2^16).
        Fx(isqrt_u64((self.0 as u64) << Self::FRAC_BITS) as i32)
    }

    /// Linear interpolation `a + (b - a) * t`, with `t` in `[0, 1]`.
    #[inline]
    pub fn lerp(a: Fx, b: Fx, t: Fx) -> Fx {
        a + (b - a) * t
    }
}

/// `p / d` rounded to nearest, halves away from zero. All of `Fx`'s divisions
/// go through this so that rounding is unbiased: a unit walking 100 steps of
/// 0.3 tiles lands on 30, not 29.998.
#[inline]
const fn div_round(p: i64, d: i64) -> i64 {
    let ap = p.unsigned_abs();
    let ad = d.unsigned_abs();
    let q = ((ap + ad / 2) / ad) as i64;
    if (p < 0) != (d < 0) {
        -q
    } else {
        q
    }
}

/// Clamps a 64-bit raw value into the `i32` range.
#[inline]
const fn saturate(v: i64) -> Fx {
    if v > i32::MAX as i64 {
        Fx(i32::MAX)
    } else if v < i32::MIN as i64 {
        Fx(i32::MIN)
    } else {
        Fx(v as i32)
    }
}

/// Floor of the square root of `n`. Newton's method from an over-estimate,
/// which converges monotonically downward to the exact floor.
pub const fn isqrt_u64(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    // 2^ceil(bits/2) is always >= sqrt(n).
    let bits = 64 - n.leading_zeros();
    let mut x = 1u64 << bits.div_ceil(2);
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

impl Add for Fx {
    type Output = Fx;
    #[inline]
    fn add(self, rhs: Fx) -> Fx {
        Fx(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Fx {
    type Output = Fx;
    #[inline]
    fn sub(self, rhs: Fx) -> Fx {
        Fx(self.0.saturating_sub(rhs.0))
    }
}

impl Mul for Fx {
    type Output = Fx;
    #[inline]
    fn mul(self, rhs: Fx) -> Fx {
        self.saturating_mul(rhs)
    }
}

impl Div for Fx {
    type Output = Fx;
    /// Panics on division by zero; that is a bug, not a state.
    #[inline]
    fn div(self, rhs: Fx) -> Fx {
        self.checked_div(rhs).expect("Fx division by zero")
    }
}

impl Neg for Fx {
    type Output = Fx;
    #[inline]
    fn neg(self) -> Fx {
        Fx(self.0.saturating_neg())
    }
}

/// Exact scaling by an integer; cheaper than converting the integer first.
impl Mul<i32> for Fx {
    type Output = Fx;
    #[inline]
    fn mul(self, rhs: i32) -> Fx {
        saturate(self.0 as i64 * rhs as i64)
    }
}

/// Division by an integer, rounded to nearest.
impl Div<i32> for Fx {
    type Output = Fx;
    #[inline]
    fn div(self, rhs: i32) -> Fx {
        assert!(rhs != 0, "Fx division by zero");
        saturate(div_round(self.0 as i64, rhs as i64))
    }
}

impl AddAssign for Fx {
    fn add_assign(&mut self, rhs: Fx) {
        *self = *self + rhs;
    }
}
impl SubAssign for Fx {
    fn sub_assign(&mut self, rhs: Fx) {
        *self = *self - rhs;
    }
}
impl MulAssign for Fx {
    fn mul_assign(&mut self, rhs: Fx) {
        *self = *self * rhs;
    }
}
impl DivAssign for Fx {
    fn div_assign(&mut self, rhs: Fx) {
        *self = *self / rhs;
    }
}

impl core::iter::Sum for Fx {
    fn sum<I: Iterator<Item = Fx>>(iter: I) -> Fx {
        iter.fold(Fx::ZERO, Add::add)
    }
}

impl From<i16> for Fx {
    fn from(v: i16) -> Fx {
        Fx::from_int(v as i32)
    }
}

impl fmt::Display for Fx {
    /// Decimal with four places, computed in integer maths.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let neg = self.0 < 0;
        let mag = (self.0 as i64).unsigned_abs();
        let int = mag >> Fx::FRAC_BITS;
        let frac = ((mag & (Fx::ONE_RAW as u64 - 1)) * 10_000) >> Fx::FRAC_BITS;
        if neg {
            write!(f, "-")?;
        }
        write!(f, "{int}.{frac:04}")
    }
}

impl fmt::Debug for Fx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fx({self})")
    }
}

impl PartialEq<i32> for Fx {
    fn eq(&self, other: &i32) -> bool {
        *self == Fx::from_int(*other)
    }
}

impl PartialOrd<i32> for Fx {
    fn partial_cmp(&self, other: &i32) -> Option<Ordering> {
        self.partial_cmp(&Fx::from_int(*other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(n: i32, d: i32) -> Fx {
        Fx::from_ratio(n, d)
    }

    #[test]
    fn constants() {
        assert_eq!(Fx::ONE.raw(), 65536);
        assert_eq!(Fx::HALF + Fx::HALF, Fx::ONE);
        assert_eq!(Fx::ONE - Fx::EPSILON, Fx::from_raw(65535));
    }

    #[test]
    fn from_int_saturates() {
        assert_eq!(Fx::from_int(5).raw(), 5 << 16);
        assert_eq!(Fx::from_int(-5).raw(), -5 << 16);
        assert_eq!(Fx::from_int(100_000), Fx::MAX);
        assert_eq!(Fx::from_int(-100_000), Fx::MIN);
    }

    #[test]
    fn ratio() {
        assert_eq!(fx(1, 2), Fx::HALF);
        assert_eq!(fx(-1, 2), -Fx::HALF);
        assert_eq!(fx(45, 100).raw(), 29491); // floor(0.45 * 65536)
    }

    #[test]
    fn add_sub_saturate() {
        assert_eq!(Fx::from_int(3) + Fx::from_int(4), Fx::from_int(7));
        assert_eq!(Fx::MAX + Fx::ONE, Fx::MAX);
        assert_eq!(Fx::MIN - Fx::ONE, Fx::MIN);
    }

    #[test]
    fn mul() {
        assert_eq!(Fx::from_int(3) * Fx::from_int(4), Fx::from_int(12));
        assert_eq!(Fx::from_int(-3) * Fx::from_int(4), Fx::from_int(-12));
        assert_eq!(Fx::HALF * Fx::HALF, fx(1, 4));
        assert_eq!(Fx::from_int(200) * Fx::from_int(200), Fx::MAX); // 40000 saturates
        assert_eq!(Fx::from_int(7) * 3, Fx::from_int(21));
        // Fixed-point multiply must agree with exact integer scaling.
        assert_eq!(fx(45, 100) * Fx::from_int(10), fx(45, 100) * 10);
        assert_eq!(fx(-45, 100) * Fx::from_int(10), fx(-45, 100) * 10);
    }

    #[test]
    fn div() {
        assert_eq!(Fx::from_int(12) / Fx::from_int(4), Fx::from_int(3));
        assert_eq!(Fx::ONE / Fx::from_int(4), fx(1, 4));
        assert_eq!(Fx::from_int(-7) / Fx::from_int(2), fx(-7, 2));
        assert_eq!(Fx::from_int(9) / 3, Fx::from_int(3));
        assert_eq!(Fx::ONE.checked_div(Fx::ZERO), None);
        // Rounds to nearest, halves away from zero, in both signs.
        assert_eq!(Fx::from_raw(7) / 2, Fx::from_raw(4));
        assert_eq!(Fx::from_raw(-7) / 2, Fx::from_raw(-4));
        assert_eq!(Fx::from_raw(7) / -2, Fx::from_raw(-4));
        assert_eq!(Fx::from_raw(-7) / -2, Fx::from_raw(4));
        assert_eq!(Fx::from_raw(6) / 4, Fx::from_raw(2));
        assert_eq!(fx(1, 20).raw(), 3277); // 3276.8 rounds up
        assert_eq!(fx(2, 3).raw(), 43691); // 43690.67 rounds up
        assert_eq!(Fx::from_int(1000) / Fx::EPSILON, Fx::MAX);
    }

    #[test]
    #[should_panic(expected = "division by zero")]
    fn div_by_zero_panics() {
        let _ = Fx::ONE / Fx::ZERO;
    }

    #[test]
    fn mul_div_avoids_intermediate_overflow() {
        // 3000 * 2000 would saturate as a product; mul_div keeps it in i64.
        let v = Fx::from_int(3000).mul_div(Fx::from_int(2000), Fx::from_int(4000));
        assert_eq!(v, Fx::from_int(1500));
    }

    #[test]
    fn rounding_family() {
        let v = fx(7, 2); // 3.5
        assert_eq!(v.floor(), 3);
        assert_eq!(v.ceil(), 4);
        assert_eq!(v.round(), 4);
        assert_eq!(v.trunc(), 3);
        assert_eq!(v.frac(), Fx::HALF);

        let n = fx(-7, 2); // -3.5
        assert_eq!(n.floor(), -4);
        assert_eq!(n.ceil(), -3);
        assert_eq!(n.round(), -3);
        assert_eq!(n.trunc(), -3);
        assert_eq!(Fx::from_int(n.floor()) + n.frac(), n);

        assert_eq!(Fx::from_int(5).ceil(), 5);
        assert_eq!(Fx::from_int(-5).ceil(), -5);
    }

    #[test]
    fn abs_neg_signum() {
        assert_eq!(fx(-3, 2).abs(), fx(3, 2));
        assert_eq!(-fx(3, 2), fx(-3, 2));
        assert_eq!(Fx::MIN.abs(), Fx::MAX);
        assert_eq!(-Fx::MIN, Fx::MAX);
        assert_eq!(fx(-3, 2).signum(), -Fx::ONE);
        assert_eq!(Fx::ZERO.signum(), Fx::ZERO);
        assert_eq!(fx(3, 2).signum(), Fx::ONE);
    }

    #[test]
    fn min_max_clamp() {
        assert_eq!(Fx::ONE.min(Fx::TWO), Fx::ONE);
        assert_eq!(Fx::ONE.max(Fx::TWO), Fx::TWO);
        assert_eq!(Fx::from_int(9).clamp(Fx::ZERO, Fx::TWO), Fx::TWO);
        assert_eq!(Fx::from_int(-9).clamp(Fx::ZERO, Fx::TWO), Fx::ZERO);
    }

    #[test]
    fn isqrt() {
        assert_eq!(isqrt_u64(0), 0);
        assert_eq!(isqrt_u64(1), 1);
        assert_eq!(isqrt_u64(2), 1);
        assert_eq!(isqrt_u64(3), 1);
        assert_eq!(isqrt_u64(4), 2);
        assert_eq!(isqrt_u64(15), 3);
        assert_eq!(isqrt_u64(16), 4);
        assert_eq!(isqrt_u64(1 << 40), 1 << 20);
        assert_eq!(isqrt_u64(u64::MAX), (1u64 << 32) - 1);
        // Exhaustive check of the floor property over a range.
        for n in 0..5000u64 {
            let r = isqrt_u64(n);
            assert!(r * r <= n && (r + 1) * (r + 1) > n, "n={n} r={r}");
        }
    }

    #[test]
    fn sqrt() {
        assert_eq!(Fx::from_int(4).sqrt(), Fx::from_int(2));
        assert_eq!(Fx::from_int(9).sqrt(), Fx::from_int(3));
        assert_eq!(fx(1, 4).sqrt(), Fx::HALF);
        assert_eq!(Fx::ZERO.sqrt(), Fx::ZERO);
        assert_eq!(Fx::from_int(-4).sqrt(), Fx::ZERO);
        // sqrt(2) = 1.41421356 -> raw 92681
        assert_eq!(Fx::TWO.sqrt().raw(), 92681);
        // Large values must not overflow the intermediate.
        let big = Fx::from_int(32000).sqrt();
        assert_eq!(big.floor(), 178);
        assert_eq!(Fx::MAX.sqrt().floor(), 181);
    }

    #[test]
    fn lerp() {
        let a = Fx::from_int(10);
        let b = Fx::from_int(20);
        assert_eq!(Fx::lerp(a, b, Fx::ZERO), a);
        assert_eq!(Fx::lerp(a, b, Fx::ONE), b);
        assert_eq!(Fx::lerp(a, b, Fx::HALF), Fx::from_int(15));
    }

    #[test]
    fn display() {
        assert_eq!(Fx::from_int(3).to_string(), "3.0000");
        assert_eq!(fx(-7, 2).to_string(), "-3.5000");
        assert_eq!(fx(1, 3).to_string(), "0.3333");
        assert_eq!(Fx::EPSILON.to_string(), "0.0000");
        assert_eq!(format!("{:?}", Fx::HALF), "Fx(0.5000)");
    }

    #[test]
    fn ordering_and_int_comparison() {
        assert!(Fx::ONE < Fx::TWO);
        assert!(Fx::from_int(-1) < Fx::ZERO);
        assert!(Fx::from_int(3) == 3);
        assert!(fx(7, 2) > 3);
        assert!(fx(7, 2) < 4);
    }
}

//! Two-component fixed-point vector, for positions and displacements in tiles.
//!
//! Squared lengths are computed in 64-bit raw form (Q32.32) because a map is
//! up to 240 tiles across and 240² does not fit in Q16.16. Use
//! [`Vec2Fx::length_sq_raw`] for comparisons and [`Vec2Fx::length`] when you
//! actually need the distance.

use crate::angle::Angle;
use crate::fx::{isqrt_u64, Fx};
use core::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};
use serde::{Deserialize, Serialize};

/// A 2D vector of [`Fx`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug, Serialize, Deserialize)]
pub struct Vec2Fx {
    /// x component.
    pub x: Fx,
    /// y component.
    pub y: Fx,
}

impl Vec2Fx {
    /// (0, 0)
    pub const ZERO: Vec2Fx = Vec2Fx {
        x: Fx::ZERO,
        y: Fx::ZERO,
    };

    /// Constructs from components.
    #[inline]
    pub const fn new(x: Fx, y: Fx) -> Vec2Fx {
        Vec2Fx { x, y }
    }

    /// Constructs from integer components.
    #[inline]
    pub const fn from_int(x: i32, y: i32) -> Vec2Fx {
        Vec2Fx {
            x: Fx::from_int(x),
            y: Fx::from_int(y),
        }
    }

    /// Unit vector at `angle`, scaled by `len`.
    pub fn from_angle(angle: Angle, len: Fx) -> Vec2Fx {
        Vec2Fx {
            x: angle.cos() * len,
            y: angle.sin() * len,
        }
    }

    /// Dot product, saturating.
    ///
    /// The intermediate is `i128`. Each product reaches 2^62, so their sum
    /// reaches 2^63 and overflows a signed 64-bit accumulator: in debug that
    /// panicked, and in release it wrapped and returned a value with the
    /// wrong *sign* (`dot(MIN, MIN)` gave -32768 where it should saturate to
    /// +MAX). The same input giving two answers depending on the build
    /// profile is a desync waiting for a player to find it.
    pub fn dot(self, o: Vec2Fx) -> Fx {
        let sum = (self.x.raw() as i128) * (o.x.raw() as i128)
            + (self.y.raw() as i128) * (o.y.raw() as i128);
        Fx::from_raw(((sum + (1 << 15)) >> 16).clamp(i32::MIN as i128, i32::MAX as i128) as i32)
    }

    /// Squared length as raw Q32.32. Exact; use for comparisons.
    ///
    /// Accumulated unsigned. Two extreme components square to 2^62 each and
    /// 2^63 together, which does not fit the `i64` this used to add in — it
    /// panicked in debug and relied on a wrap in release. It fits the `u64`
    /// this returns with a bit to spare, so accumulating there is exact and
    /// costs nothing.
    #[inline]
    pub fn length_sq_raw(self) -> u64 {
        let x = (self.x.raw() as i64).unsigned_abs();
        let y = (self.y.raw() as i64).unsigned_abs();
        x * x + y * y
    }

    /// Length. Saturates at `Fx::MAX` (only reachable for absurd inputs).
    pub fn length(self) -> Fx {
        // sqrt(x_raw² + y_raw²) == sqrt(x² + y²) · 2^16, i.e. already Q16.16.
        let r = isqrt_u64(self.length_sq_raw());
        Fx::from_raw(r.min(i32::MAX as u64) as i32)
    }

    /// Squared distance as raw Q32.32.
    #[inline]
    pub fn distance_sq_raw(self, o: Vec2Fx) -> u64 {
        (o - self).length_sq_raw()
    }

    /// Distance to `o`.
    #[inline]
    pub fn distance(self, o: Vec2Fx) -> Fx {
        (o - self).length()
    }

    /// `|dx| + |dy|`.
    pub fn manhattan(self, o: Vec2Fx) -> Fx {
        (o.x - self.x).abs() + (o.y - self.y).abs()
    }

    /// `self * num / den` per component with a 64-bit intermediate.
    pub fn scale_ratio(self, num: Fx, den: Fx) -> Vec2Fx {
        Vec2Fx {
            x: self.x.mul_div(num, den),
            y: self.y.mul_div(num, den),
        }
    }

    /// Unit-length copy, or zero if this is the zero vector.
    pub fn normalized_or_zero(self) -> Vec2Fx {
        let len = self.length();
        if len.is_zero() {
            Vec2Fx::ZERO
        } else {
            self.scale_ratio(Fx::ONE, len)
        }
    }

    /// Steps toward `target` by at most `max_step`, landing exactly on it when
    /// within reach. This is the primitive every movement system uses.
    pub fn move_toward(self, target: Vec2Fx, max_step: Fx) -> Vec2Fx {
        let d = target - self;
        let len = d.length();
        if len <= max_step {
            target
        } else {
            self + d.scale_ratio(max_step, len)
        }
    }

    /// Angle of this vector; zero vector yields `Angle::ZERO`.
    ///
    /// Computed by octant reduction and a rational approximation of atan on
    /// `[0, 1]`, accurate to about 0.3°. Good enough for facings and steering;
    /// not for geometry that has to close.
    pub fn angle(self) -> Angle {
        let x = self.x.raw() as i64;
        let y = self.y.raw() as i64;
        if x == 0 && y == 0 {
            return Angle::ZERO;
        }
        let ax = x.abs();
        let ay = y.abs();
        // atan(t) for t in [0,1], in BAM (8192 == 45°):
        //   atan(t) ≈ t·(8192 + 2810·(1 − t)) / 1   with t in Q16
        let (num, den, swap) = if ay <= ax {
            (ay, ax, false)
        } else {
            (ax, ay, true)
        };
        let t = (num << 16) / den; // Q16, 0..=65536
        let one_minus_t = 65536 - t;
        let a = (t * (8192 * 65536 + 2810 * one_minus_t)) >> 32;
        let a = if swap { 16384 - a } else { a };
        let a = match (x >= 0, y >= 0) {
            (true, true) => a,
            (false, true) => 32768 - a,
            (false, false) => 32768 + a,
            (true, false) => 65536 - a,
        };
        Angle(a as u16)
    }
}

impl Add for Vec2Fx {
    type Output = Vec2Fx;
    #[inline]
    fn add(self, o: Vec2Fx) -> Vec2Fx {
        Vec2Fx {
            x: self.x + o.x,
            y: self.y + o.y,
        }
    }
}
impl Sub for Vec2Fx {
    type Output = Vec2Fx;
    #[inline]
    fn sub(self, o: Vec2Fx) -> Vec2Fx {
        Vec2Fx {
            x: self.x - o.x,
            y: self.y - o.y,
        }
    }
}
impl Neg for Vec2Fx {
    type Output = Vec2Fx;
    #[inline]
    fn neg(self) -> Vec2Fx {
        Vec2Fx {
            x: -self.x,
            y: -self.y,
        }
    }
}
impl Mul<Fx> for Vec2Fx {
    type Output = Vec2Fx;
    #[inline]
    fn mul(self, s: Fx) -> Vec2Fx {
        Vec2Fx {
            x: self.x * s,
            y: self.y * s,
        }
    }
}
impl Mul<i32> for Vec2Fx {
    type Output = Vec2Fx;
    #[inline]
    fn mul(self, s: i32) -> Vec2Fx {
        Vec2Fx {
            x: self.x * s,
            y: self.y * s,
        }
    }
}
impl Div<Fx> for Vec2Fx {
    type Output = Vec2Fx;
    #[inline]
    fn div(self, s: Fx) -> Vec2Fx {
        Vec2Fx {
            x: self.x / s,
            y: self.y / s,
        }
    }
}
impl AddAssign for Vec2Fx {
    fn add_assign(&mut self, o: Vec2Fx) {
        *self = *self + o;
    }
}
impl SubAssign for Vec2Fx {
    fn sub_assign(&mut self, o: Vec2Fx) {
        *self = *self - o;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: i32, y: i32) -> Vec2Fx {
        Vec2Fx::from_int(x, y)
    }

    #[test]
    fn arithmetic() {
        assert_eq!(v(1, 2) + v(3, 4), v(4, 6));
        assert_eq!(v(1, 2) - v(3, 4), v(-2, -2));
        assert_eq!(-v(1, 2), v(-1, -2));
        assert_eq!(v(1, 2) * Fx::TWO, v(2, 4));
        assert_eq!(v(1, 2) * 3, v(3, 6));
        assert_eq!(v(4, 6) / Fx::TWO, v(2, 3));
    }

    #[test]
    fn dot_and_lengths() {
        assert_eq!(v(3, 4).dot(v(2, 1)), Fx::from_int(10));
        assert_eq!(v(3, 4).length(), Fx::from_int(5));
        assert_eq!(v(-3, 4).length(), Fx::from_int(5));
        assert_eq!(v(0, 0).length(), Fx::ZERO);
        assert_eq!(v(1, 2).distance(v(4, 6)), Fx::from_int(5));
        assert_eq!(v(1, 2).manhattan(v(4, 6)), Fx::from_int(7));
        // Far corners of a giant map must not overflow.
        let d = v(0, 0).distance(v(240, 240));
        assert_eq!(d.floor(), 339);
        assert!(v(0, 0).distance_sq_raw(v(240, 240)) > v(0, 0).distance_sq_raw(v(200, 200)));
    }

    /// Both accumulators overflowed at the extremes of the type: a panic in
    /// debug, and in release a wrong answer with an inverted sign. Positions
    /// never reach these values today, but `distance_sq_raw` is a comparison
    /// primitive that `nav`, the separation pass and the drop-off search all
    /// lean on, and the next caller should not have to know.
    #[test]
    fn extreme_components_do_not_overflow() {
        let corners = [
            Vec2Fx::new(Fx::MIN, Fx::MIN),
            Vec2Fx::new(Fx::MAX, Fx::MAX),
            Vec2Fx::new(Fx::MIN, Fx::MAX),
            Vec2Fx::new(Fx::MAX, Fx::MIN),
        ];
        for a in corners {
            // Exact, checked against the value computed in a wider type.
            let expect = (a.x.raw() as i128).pow(2) + (a.y.raw() as i128).pow(2);
            assert_eq!(a.length_sq_raw() as i128, expect, "{a:?}");
            assert_eq!(a.length(), Fx::MAX, "length must saturate, not wrap");
            for b in corners {
                let expect = (a.x.raw() as i128) * (b.x.raw() as i128)
                    + (a.y.raw() as i128) * (b.y.raw() as i128);
                let want = ((expect + (1 << 15)) >> 16).clamp(i32::MIN as i128, i32::MAX as i128);
                assert_eq!(a.dot(b).raw() as i128, want, "dot({a:?}, {b:?})");
                // These must merely not blow up.
                let _ = a.distance_sq_raw(b);
                let _ = a.distance(b);
                let _ = a.move_toward(b, Fx::ONE);
            }
        }
        // The exact value, not a wrapped one: 2^62 + 2^62.
        assert_eq!(Vec2Fx::new(Fx::MIN, Fx::MIN).length_sq_raw(), 1u64 << 63);
        // Saturates positive; in release it used to come back as -32768.
        assert_eq!(
            Vec2Fx::new(Fx::MIN, Fx::MIN).dot(Vec2Fx::new(Fx::MIN, Fx::MIN)),
            Fx::MAX
        );
    }

    #[test]
    fn normalize() {
        let n = v(3, 4).normalized_or_zero();
        assert!((n.length() - Fx::ONE).abs() <= Fx::from_raw(2), "{n:?}");
        assert_eq!(n.x, Fx::from_ratio(3, 5));
        assert_eq!(Vec2Fx::ZERO.normalized_or_zero(), Vec2Fx::ZERO);
    }

    #[test]
    fn move_toward_lands_exactly() {
        let start = v(0, 0);
        let target = v(3, 4);
        let step = Fx::ONE;
        let mut p = start;
        let mut steps = 0;
        while p != target {
            p = p.move_toward(target, step);
            steps += 1;
            assert!(steps <= 6, "did not converge: {p:?}");
        }
        assert_eq!(steps, 5);
        // Overshoot is impossible; a big step lands on the target.
        assert_eq!(start.move_toward(target, Fx::from_int(100)), target);
        // Already there.
        assert_eq!(target.move_toward(target, step), target);
        // Each intermediate step has the right length.
        let q = start.move_toward(target, step);
        assert!((q.length() - Fx::ONE).abs() <= Fx::from_raw(2));
    }

    #[test]
    fn from_angle_round_trips_through_facing() {
        for deg in (0..360).step_by(15) {
            let a = Angle::from_degrees(deg);
            let p = Vec2Fx::from_angle(a, Fx::from_int(10));
            let back = p.angle();
            let diff = (back - a).0.min((a - back).0);
            assert!(
                diff <= 60,
                "{deg}°: got {}° (diff {diff} BAM)",
                back.to_degrees()
            );
            assert_eq!(back.facing8(), a.facing8(), "{deg}°");
        }
    }

    #[test]
    fn angle_axes_exact() {
        assert_eq!(v(1, 0).angle(), Angle::ZERO);
        assert_eq!(v(0, 1).angle(), Angle::QUARTER);
        assert_eq!(v(-1, 0).angle(), Angle::HALF);
        assert_eq!(v(0, -1).angle(), Angle::THREE_QUARTER);
        assert_eq!(v(5, 5).angle(), Angle::from_degrees(45));
        assert_eq!(Vec2Fx::ZERO.angle(), Angle::ZERO);
    }
}

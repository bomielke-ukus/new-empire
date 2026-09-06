//! Property tests for the fixed-point maths.
//!
//! These are the operations every other system is built out of, so a bug here
//! is a bug everywhere and a *rounding* bug here is a desync. Each property is
//! checked against a reference computed in `i128` rationals or `f64` — the
//! reference lives here in the test, never in `crates/sim`, which stays
//! float-free.
//!
//! Shrunk counterexamples are written to `crates/sim/proptest-regressions/`
//! and committed, so every failure proptest ever finds becomes a permanent
//! regression test.

use proptest::prelude::*;
use sim::{Angle, Fx, Vec2Fx};

/// `Fx` values across the whole representable range, biased toward the small
/// magnitudes real gameplay uses but including the saturation boundaries.
fn any_fx() -> impl Strategy<Value = Fx> {
    prop_oneof![
        8 => (-(1 << 22)..(1 << 22)).prop_map(Fx::from_raw),
        2 => any::<i32>().prop_map(Fx::from_raw),
        1 => prop::sample::select(vec![
            Fx::ZERO, Fx::ONE, Fx::HALF, Fx::TWO, Fx::MAX, Fx::MIN, Fx::EPSILON,
            Fx::ZERO - Fx::EPSILON,
        ]),
    ]
}

fn nonzero_fx() -> impl Strategy<Value = Fx> {
    any_fx().prop_filter("nonzero", |v| !v.is_zero())
}

/// Exact value of an `Fx` as a rational with denominator 2^16.
fn exact(v: Fx) -> i128 {
    v.raw() as i128
}

const ONE: i128 = 1 << 16;

/// `p / q` rounded to nearest, halves away from zero — the contract `Fx`'s
/// division promises (`docs/07` D10).
fn div_round_ref(p: i128, q: i128) -> i128 {
    let (ap, aq) = (p.abs(), q.abs());
    let mag = (ap + aq / 2) / aq;
    if (p < 0) != (q < 0) {
        -mag
    } else {
        mag
    }
}

fn saturate_ref(v: i128) -> i32 {
    v.clamp(i32::MIN as i128, i32::MAX as i128) as i32
}

proptest! {
    // REQ: TA-FX-01
    #[test]
    fn floor_plus_frac_reconstructs(v in any_fx()) {
        let rebuilt = Fx::from_int(v.floor()) + v.frac();
        prop_assert_eq!(rebuilt, v, "floor {} + frac {}", v.floor(), v.frac());
    }

    #[test]
    fn frac_is_in_unit_interval(v in any_fx()) {
        prop_assert!(v.frac() >= Fx::ZERO && v.frac() < Fx::ONE);
    }

    #[test]
    fn floor_ceil_trunc_bracket_the_value(v in any_fx()) {
        prop_assert!(Fx::from_int(v.floor()) <= v);
        prop_assert!(v.floor() <= v.ceil());
        prop_assert!(v.ceil() - v.floor() <= 1);
        let expected_trunc = if v >= Fx::ZERO { v.floor() } else { v.ceil() };
        prop_assert_eq!(v.trunc(), expected_trunc);
    }

    // REQ: TA-FX-02
    /// Ordering must agree with the raw bit pattern, because the entity store
    /// and the command queue both sort on `Fx`-derived keys and a disagreement
    /// there is a divergence.
    #[test]
    fn ordering_matches_raw(a in any_fx(), b in any_fx()) {
        prop_assert_eq!(a.cmp(&b), a.raw().cmp(&b.raw()));
    }

    // REQ: TA-FX-03
    /// Saturation, not wrapping. A wrap here would be a silent teleport.
    #[test]
    fn add_and_sub_saturate(a in any_fx(), b in any_fx()) {
        let sum = exact(a) + exact(b);
        prop_assert_eq!((a + b).raw(), saturate_ref(sum));
        let diff = exact(a) - exact(b);
        prop_assert_eq!((a - b).raw(), saturate_ref(diff));
    }

    #[test]
    fn saturation_boundaries_are_sticky(v in any_fx()) {
        prop_assert_eq!(Fx::MAX + v.abs(), Fx::MAX);
        prop_assert_eq!(Fx::MIN - v.abs(), Fx::MIN);
    }

    // REQ: TA-FX-04
    #[test]
    fn mul_is_commutative(a in any_fx(), b in any_fx()) {
        prop_assert_eq!(a * b, b * a);
    }

    /// Multiplication is within one ulp of the exact product. The rounding
    /// direction on an exact half differs from division's (mul rounds toward
    /// +inf, div rounds away from zero); that asymmetry is pinned down by
    /// `half_rounding_directions_are_what_the_code_says` below rather than
    /// left to chance.
    #[test]
    fn mul_is_within_one_ulp_of_exact(a in any_fx(), b in any_fx()) {
        let exact_product = exact(a) * exact(b);
        let got = (a * b).raw() as i128;
        // Un-saturated exact result, in raw units.
        let ideal = div_round_ref(exact_product, ONE);
        if ideal >= i32::MIN as i128 && ideal <= i32::MAX as i128 {
            prop_assert!(
                (got - ideal).abs() <= 1,
                "{:?} * {:?}: got {} ideal {}", a, b, got, ideal
            );
        } else {
            prop_assert_eq!(got, saturate_ref(ideal) as i128);
        }
    }

    // REQ: TA-FX-05
    /// Division rounds to nearest, halves away from zero. This is D10, the
    /// decision that stopped units arriving at 2.9992 tiles.
    #[test]
    fn div_rounds_to_nearest_halves_away_from_zero(a in any_fx(), b in nonzero_fx()) {
        let ideal = div_round_ref(exact(a) * ONE, exact(b));
        prop_assert_eq!((a / b).raw(), saturate_ref(ideal));
    }

    #[test]
    fn checked_div_agrees_with_div_and_handles_zero(a in any_fx(), b in nonzero_fx()) {
        prop_assert_eq!(a.checked_div(b), Some(a / b));
        prop_assert_eq!(a.checked_div(Fx::ZERO), None);
    }

    // REQ: TA-FX-06
    /// `mul_div` exists so the intermediate product does not overflow. It must
    /// therefore be at least as accurate as doing it in two steps, and exactly
    /// as accurate as the 128-bit reference.
    #[test]
    fn mul_div_is_exact_to_the_reference(a in any_fx(), n in any_fx(), d in nonzero_fx()) {
        let ideal = div_round_ref(exact(a) * exact(n), exact(d));
        prop_assert_eq!(a.mul_div(n, d).raw(), saturate_ref(ideal));
    }

    #[test]
    fn from_ratio_matches_the_reference(n in -100_000i32..100_000, d in 1i32..100_000) {
        let ideal = div_round_ref((n as i128) * ONE, d as i128);
        prop_assert_eq!(Fx::from_ratio(n, d).raw(), saturate_ref(ideal));
    }

    // REQ: TA-FX-07
    #[test]
    fn sqrt_is_the_floor_of_the_true_root(v in any_fx()) {
        let r = v.sqrt();
        if v <= Fx::ZERO {
            prop_assert_eq!(r, Fx::ZERO);
        } else {
            // r*r <= v < (r+eps)*(r+eps), computed exactly.
            let rr = (r.raw() as i128) * (r.raw() as i128);
            let next = (r.raw() as i128 + 1) * (r.raw() as i128 + 1);
            let target = exact(v) * ONE;
            prop_assert!(rr <= target, "sqrt({:?}) = {:?} is too big", v, r);
            prop_assert!(next > target, "sqrt({:?}) = {:?} is too small", v, r);
        }
    }

    // REQ: TA-FX-08
    #[test]
    fn min_max_clamp_are_consistent(a in any_fx(), b in any_fx(), c in any_fx()) {
        prop_assert_eq!(a.min(b), b.min(a));
        prop_assert_eq!(a.max(b), b.max(a));
        prop_assert!(a.min(b) <= a.max(b));
        let (lo, hi) = if b <= c { (b, c) } else { (c, b) };
        let clamped = a.clamp(lo, hi);
        prop_assert!(clamped >= lo && clamped <= hi);
        if a >= lo && a <= hi {
            prop_assert_eq!(clamped, a);
        }
    }

    // REQ: TA-FX-09
    #[test]
    fn serde_round_trip_is_bit_exact(v in any_fx()) {
        let text = ron::to_string(&v).unwrap();
        let back: Fx = ron::from_str(&text).unwrap();
        prop_assert_eq!(back.raw(), v.raw());
    }

    /// Adding the same value to both sides preserves order, except where
    /// saturation legitimately collapses the difference.
    ///
    /// The pair is sorted rather than filtered with `prop_assume!`, which
    /// would throw away half of every run and exhaust proptest's reject budget
    /// at higher case counts.
    #[test]
    fn addition_is_monotone(p in any_fx(), q in any_fx(), c in any_fx()) {
        let (a, b) = if p <= q { (p, q) } else { (q, p) };
        prop_assert!(a + c <= b + c);
    }
}

/// The half-rounding conventions, pinned so a refactor cannot quietly change
/// them. Multiplication rounds a tie toward positive infinity; division rounds
/// a tie away from zero. They differ, `docs/04` §14 documents only the latter,
/// and a change to either invalidates every replay ever recorded.
#[test]
fn half_rounding_directions_are_what_the_code_says() {
    // 0.5 ulp ties in multiplication: (1.5 raw units) * (1/3) is not a tie, so
    // build the tie directly: raw 3 * raw (1<<15) = 3<<15, /2^16 = 1.5 -> 2.
    let tie_up = Fx::from_raw(3) * Fx::from_raw(1 << 15);
    assert_eq!(tie_up.raw(), 2, "mul rounds a tie toward +inf");
    let tie_down = Fx::from_raw(-3) * Fx::from_raw(1 << 15);
    assert_eq!(tie_down.raw(), -1, "mul rounds a negative tie toward +inf");

    // Division ties go away from zero, in both directions.
    assert_eq!((Fx::from_raw(3) / Fx::TWO).raw(), 2);
    assert_eq!((Fx::from_raw(-3) / Fx::TWO).raw(), -2);
}

/// Walks `start` to `target` in `step`-sized increments, asserting that every
/// step makes strict progress and that the walk terminates inside the bound
/// implied by the geometry.
///
/// The bound is computed in `i64` raw units, not in `Fx`: a long walk in small
/// steps needs more iterations than `Fx` can represent, so computing it in
/// `Fx` silently saturates and the test starts lying.
fn walk_to(start: Vec2Fx, target: Vec2Fx, step: Fx) -> Result<u64, String> {
    // Each step rounds when it scales the direction vector, so it can fall an
    // ulp short of `step`. Budget against the worst-case effective step.
    let effective = (step.raw() as i64 - 1).max(1);
    let bound = start.distance(target).raw() as i64 / effective + 2;

    let mut p = start;
    let mut taken = 0i64;
    while p != target {
        let next = p.move_toward(target, step);
        if next == p {
            return Err(format!("stalled at {p:?} heading for {target:?}"));
        }
        p = next;
        taken += 1;
        if taken > bound {
            return Err(format!(
                "still walking after {taken} steps of {step:?} from {start:?} \
                 to {target:?}; bound was {bound}"
            ));
        }
    }
    Ok(taken as u64)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    // REQ: TA-PATH-01
    /// The anti-stuck property, and the single most important one in the file:
    /// a unit told to walk somewhere reaches it, exactly, in a bounded number
    /// of steps. Every "unit stands still forever" bug in an RTS is this
    /// property failing.
    #[test]
    fn move_toward_always_arrives_in_bounded_steps(
        sx in -60i32..60, sy in -60i32..60,
        tx in -60i32..60, ty in -60i32..60,
        step_num in 5i32..2000,
    ) {
        let start = Vec2Fx::from_int(sx, sy);
        let target = Vec2Fx::from_int(tx, ty);
        prop_assert!(walk_to(start, target, Fx::from_ratio(step_num, 1000)).is_ok());
    }
}

/// The cases the randomised walk deliberately does not reach: a step so small
/// that arrival takes tens of thousands of ticks, a step larger than the whole
/// journey, and a step at the representable minimum. Enumerated rather than
/// sampled so the slow ones run exactly once.
#[test]
fn move_toward_arrives_from_pathological_steps() {
    let cases: [(i32, i32, Fx); 6] = [
        (0, -66, Fx::from_ratio(1, 1000)),
        (200, 200, Fx::from_ratio(1, 500)),
        (-240, 17, Fx::from_ratio(1, 250)),
        (3, 0, Fx::ONE),
        (3, 0, Fx::from_int(100)),
        (1, 1, Fx::EPSILON * Fx::from_int(64)),
    ];
    for (tx, ty, step) in cases {
        let target = Vec2Fx::from_int(tx, ty);
        walk_to(Vec2Fx::ZERO, target, step)
            .unwrap_or_else(|e| panic!("step {step:?} to ({tx}, {ty}): {e}"));
    }
}

/// A zero-length step must not stall *and* must not pretend to arrive: the
/// unit stays put, which is the caller's problem, not a silent teleport.
#[test]
fn move_toward_with_no_step_does_not_move_or_arrive() {
    let start = Vec2Fx::from_int(1, 1);
    let target = Vec2Fx::from_int(5, 5);
    assert_eq!(start.move_toward(target, Fx::ZERO), start);
    assert_eq!(start.move_toward(start, Fx::ZERO), start);
}

proptest! {

    #[test]
    fn move_toward_never_overshoots(
        sx in any_fx(), sy in any_fx(), tx in any_fx(), ty in any_fx(),
        step in any_fx(),
    ) {
        let start = Vec2Fx::new(sx, sy);
        let target = Vec2Fx::new(tx, ty);
        let step = step.abs();
        let before = start.distance_sq_raw(target);
        let after = start.move_toward(target, step).distance_sq_raw(target);
        prop_assert!(after <= before, "moved away from the target");
    }

    #[test]
    fn move_toward_lands_exactly_when_within_reach(
        sx in -100i32..100, sy in -100i32..100, tx in -100i32..100, ty in -100i32..100,
    ) {
        let start = Vec2Fx::from_int(sx, sy);
        let target = Vec2Fx::from_int(tx, ty);
        let reach = start.distance(target) + Fx::EPSILON;
        prop_assert_eq!(start.move_toward(target, reach), target);
    }

    // REQ: TA-VEC-01
    #[test]
    fn length_is_the_floor_of_the_true_length(x in -30_000i32..30_000, y in -30_000i32..30_000) {
        let v = Vec2Fx::new(Fx::from_raw(x), Fx::from_raw(y));
        let got = v.length().raw() as i128;
        let sq = (x as i128) * (x as i128) + (y as i128) * (y as i128);
        prop_assert!(got * got <= sq, "length too large");
        prop_assert!((got + 1) * (got + 1) > sq, "length too small");
    }

    // REQ: TA-VEC-05
    #[test]
    fn dot_matches_the_reference_and_never_overflows(
        ax in any_fx(), ay in any_fx(), bx in any_fx(), by in any_fx(),
    ) {
        let a = Vec2Fx::new(ax, ay);
        let b = Vec2Fx::new(bx, by);
        let exact = (ax.raw() as i128) * (bx.raw() as i128)
            + (ay.raw() as i128) * (by.raw() as i128);
        let ideal = (exact + (1 << 15)) >> 16;
        let got = a.dot(b).raw() as i128;
        prop_assert_eq!(got, ideal.clamp(i32::MIN as i128, i32::MAX as i128));
        prop_assert_eq!(a.dot(b), b.dot(a), "dot is commutative");
    }

    #[test]
    fn squared_length_is_exact_and_never_overflows(x in any_fx(), y in any_fx()) {
        let v = Vec2Fx::new(x, y);
        let exact = (x.raw() as i128) * (x.raw() as i128) + (y.raw() as i128) * (y.raw() as i128);
        prop_assert_eq!(v.length_sq_raw() as i128, exact);
    }

    #[test]
    fn axis_aligned_lengths_are_exact(n in -10_000i32..10_000) {
        prop_assert_eq!(Vec2Fx::new(Fx::from_int(n), Fx::ZERO).length(), Fx::from_int(n).abs());
        prop_assert_eq!(Vec2Fx::new(Fx::ZERO, Fx::from_int(n)).length(), Fx::from_int(n).abs());
    }

    // REQ: TA-VEC-02
    #[test]
    fn normalized_has_unit_length_or_is_zero(x in -1000i32..1000, y in -1000i32..1000) {
        let v = Vec2Fx::from_int(x, y);
        let n = v.normalized_or_zero();
        if x == 0 && y == 0 {
            prop_assert_eq!(n, Vec2Fx::ZERO);
        } else {
            let len = n.length();
            let err = (len - Fx::ONE).abs();
            // One ulp per component, so up to a couple in the length.
            prop_assert!(err.raw() <= 4, "unit vector length {:?} for {:?}", len, v);
        }
    }

    // REQ: TA-VEC-03
    #[test]
    fn distance_is_symmetric_and_zero_only_at_zero(
        ax in -1000i32..1000, ay in -1000i32..1000,
        bx in -1000i32..1000, by in -1000i32..1000,
    ) {
        let a = Vec2Fx::from_int(ax, ay);
        let b = Vec2Fx::from_int(bx, by);
        prop_assert_eq!(a.distance(b), b.distance(a));
        prop_assert_eq!(a.distance(a), Fx::ZERO);
        if a != b {
            prop_assert!(a.distance(b) > Fx::ZERO);
        }
    }

    // REQ: TA-VEC-04
    /// The facing computation only has to be good to a fraction of a degree,
    /// but it has to be good to that everywhere, including the octant seams.
    #[test]
    fn angle_is_accurate_to_half_a_degree(x in -10_000i32..10_000, y in -10_000i32..10_000) {
        prop_assume!(x != 0 || y != 0);
        let v = Vec2Fx::new(Fx::from_raw(x), Fx::from_raw(y));
        let got = v.angle().0 as f64 * 360.0 / 65536.0;
        let want = (y as f64).atan2(x as f64).to_degrees().rem_euclid(360.0);
        let mut err = (got - want).abs();
        if err > 180.0 {
            err = 360.0 - err;
        }
        prop_assert!(err <= 0.5, "angle of ({x}, {y}): got {got}, want {want}");
    }

    #[test]
    fn negating_a_vector_turns_it_half_way(x in -10_000i32..10_000, y in -10_000i32..10_000) {
        prop_assume!(x != 0 || y != 0);
        let v = Vec2Fx::new(Fx::from_raw(x), Fx::from_raw(y));
        let a = v.angle();
        let b = (Vec2Fx::ZERO - v).angle();
        let diff = b.0.wrapping_sub(a.0);
        let err = (diff as i32 - 32768).abs();
        // Half a degree of BAM is ~91 units; the two approximations are
        // computed in different octants so allow a little more.
        prop_assert!(err <= 200, "({x}, {y}): {a:?} vs {b:?}");
    }
}

// REQ: TA-ANG-01
/// Exhaustive over every representable angle. 65536 iterations is nothing, and
/// exhaustive beats sampled when the domain is this small — there is no seam
/// or table boundary left for a sampler to miss.
#[test]
fn trig_identities_hold_for_every_angle() {
    let mut worst_pythagoras = 0i64;
    for raw in 0..=u16::MAX {
        let a = Angle(raw);
        let (s, c) = (a.sin(), a.cos());

        assert!(s.abs() <= Fx::ONE, "sin({raw}) = {s} out of range");
        assert!(c.abs() <= Fx::ONE, "cos({raw}) = {c} out of range");

        // sin^2 + cos^2 == 1, in raw units squared.
        let sum = (s.raw() as i64).pow(2) + (c.raw() as i64).pow(2);
        let one = (Fx::ONE.raw() as i64).pow(2);
        worst_pythagoras = worst_pythagoras.max((sum - one).abs());

        // cos(a) == sin(a + 90deg), the definition the table is built on.
        assert_eq!(c, (a + Angle::QUARTER).sin(), "cos/sin offset at {raw}");
    }
    // The table is 257 entries with linear interpolation, documented as
    // accurate to about 1/1000. Squared, that is a budget of ~2^-10 of one.
    let budget = (Fx::ONE.raw() as i64).pow(2) / 400;
    assert!(
        worst_pythagoras < budget,
        "worst sin^2+cos^2 error {worst_pythagoras} exceeds budget {budget}"
    );
}

// REQ: TA-ANG-02
#[test]
fn angle_quadrant_signs_are_correct() {
    let cases = [
        (0, 0i32, 1i32), // 0 deg: sin 0, cos +
        (45, 1, 1),
        (90, 1, 0),
        (135, 1, -1),
        (180, 0, -1),
        (225, -1, -1),
        (270, -1, 0),
        (315, -1, 1),
    ];
    for (deg, want_sin, want_cos) in cases {
        let a = Angle::from_degrees(deg);
        let (s, c) = (a.sin(), a.cos());
        assert_eq!(
            s.signum().raw().signum(),
            want_sin.signum(),
            "sin sign at {deg}"
        );
        assert_eq!(
            c.signum().raw().signum(),
            want_cos.signum(),
            "cos sign at {deg}"
        );
    }
}

// REQ: TA-ANG-03
/// Wrapping through zero must be continuous — a discontinuity at the seam
/// would make a unit facing due east flicker.
#[test]
fn trig_is_continuous_across_the_wrap() {
    let step_limit = Fx::from_ratio(1, 100);
    for raw in 0..=u16::MAX {
        let a = Angle(raw);
        let b = Angle(raw.wrapping_add(1));
        assert!(
            (b.sin() - a.sin()).abs() <= step_limit,
            "sin jumps between {raw} and {}",
            raw.wrapping_add(1)
        );
        assert!(
            (b.cos() - a.cos()).abs() <= step_limit,
            "cos jumps between {raw} and {}",
            raw.wrapping_add(1)
        );
    }
}

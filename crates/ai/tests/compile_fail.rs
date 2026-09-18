//! The AI boundary is a compile error, not a code review.
//!
//! REQ: TA-AI-01
//!
//! Each file under `tests/ui` tries to reach the simulation's world from
//! this crate and must fail to compile. `trybuild` runs them with this
//! crate's dependencies, which is exactly the point: `sim` is not among
//! them, and `fogged` re-exports no path to it.

#[test]
fn the_ai_cannot_name_the_world() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}

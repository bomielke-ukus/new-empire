// TA-AI-01: mechanical dependency boundary; renderer integration remains owed.
#[test]
fn ai_cannot_name_world_or_simulation() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/raw_world.rs");
    tests.compile_fail("tests/ui/view_world.rs");
    tests.pass("tests/ui/fogged_view.rs");
}

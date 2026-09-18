//! The headless opponent runner: every side an `ai::Opponent` behind a
//! `FoggedView`, invariants on, the recording verified.

#[test]
fn opponents_play_a_short_match_headless_and_it_replays() {
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_simrunner"))
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&[
        "ai",
        "--matches",
        "2",
        "--ticks",
        "120",
        "--players",
        "3",
        "--size",
        "48",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("seed 1:") && text.contains("seed 2:"),
        "{text}"
    );
    assert!(
        text.contains("3 opponents") && text.contains("explored"),
        "{text}"
    );
    assert!(text.contains("2 match(es)"), "{text}");
    assert_eq!(run(&["ai", "--matches", "0"]).status.code(), Some(2));
    assert_eq!(run(&["ai", "--ticks", "0"]).status.code(), Some(2));
}

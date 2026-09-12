//! The replay renderer rejects invalid input before rendering, and accepts
//! a frame at tick zero. The golden-image suite pins an in-battle frame.
use std::path::Path;
use std::process::Command;

fn mapview(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mapview"))
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn replay_options_reject_conflicts_and_out_of_range_ticks() {
    let replay = "crates/sim/tests/corpus/battle-40v40.ron";
    for (extra, expected) in [
        (vec!["--scenario", "battle"], "mutually exclusive"),
        (vec!["--ticks", "6001"], "exceeds the replay's end"),
    ] {
        let mut args = vec!["--replay", replay];
        args.extend(extra);
        let out = mapview(&args);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains(expected));
    }
}

#[test]
fn a_flat_replay_renders_at_tick_zero_without_a_start_kit() {
    let path =
        std::env::temp_dir().join(format!("new-empire-replay-zero-{}.png", std::process::id()));
    let out = mapview(&[
        "--replay",
        "crates/sim/tests/corpus/battle-40v40.ron",
        "--ticks",
        "0",
        "--width",
        "320",
        "--height",
        "240",
        "--out",
        path.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(image::image_dimensions(&path).unwrap(), (320, 240));
    std::fs::remove_file(path).unwrap();
}

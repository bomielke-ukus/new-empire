//! `simrunner verify` on a save file: a save is its own replay
//! (`TA-SAVE-01`), so the tool checks the snapshot against the log inside
//! it and then replays that log twice, as it does for a replay file. A
//! save whose snapshot is not what its log gives is refused and says so.
//!
//! REQ: TA-SAVE-01

use ai::{Difficulty, Opponent};
use fogged::FoggedView;
use save::{Save, View};
use sim::{MapKind, MapSpec, SimConfig, Simulation, Source};
use std::path::PathBuf;
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "new-empire-simrunner-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn played(seed: u64, ticks: u64) -> Save {
    let mut sim = Simulation::new(
        seed,
        SimConfig {
            map: MapSpec {
                kind: MapKind::Inland,
                size: 96,
                players: 2,
            },
            ..SimConfig::default()
        },
    );
    let mut bots = vec![
        Opponent::new(0, Difficulty::Standard, seed),
        Opponent::new(1, Difficulty::Easy, seed),
    ];
    while sim.tick() < ticks {
        for bot in &mut bots {
            let commands = {
                let view = FoggedView::new(&sim, bot.player());
                bot.think(&view)
            };
            for c in commands {
                sim.issue_from(c, Source::Ai);
            }
        }
        sim.step();
    }
    Save::new(&sim, &bots, View::default(), 1_789_000_000)
}

fn verify(path: &std::path::Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_simrunner"))
        .arg("verify")
        .arg(path)
        .output()
        .expect("simrunner runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn verify_accepts_a_save_as_its_own_replay_and_refuses_a_forged_one() {
    let dir = scratch("verify");
    let good = played(9, 300);
    let path = save::write(&dir, &good).unwrap();
    let (ok, text) = verify(&path);
    assert!(ok, "{text}");
    assert!(text.contains("loaded save"), "{text}");
    assert!(text.contains("tick=300"), "{text}");
    assert!(
        text.contains("the snapshot is what its replay gives"),
        "{text}"
    );
    assert!(text.contains("300 ticks identical"), "{text}");

    // The same snapshot claiming another seed: the log inside replays to
    // a different world, and the tool says which hashes disagree.
    let forged = std::fs::read_to_string(&path)
        .unwrap()
        .replace("seed:9", "seed:10");
    let forged_path = dir.join("forged.ron");
    std::fs::write(&forged_path, forged).unwrap();
    let (ok, text) = verify(&forged_path);
    assert!(!ok);
    assert!(text.contains("but its replay reaches"), "{text}");

    // A save from another build is refused by its numbers.
    let old =
        std::fs::read_to_string(&path)
            .unwrap()
            .replacen("state_version:1", "state_version:3", 1);
    let old_path = dir.join("old.ron");
    std::fs::write(&old_path, old).unwrap();
    let (ok, text) = verify(&old_path);
    assert!(!ok);
    assert!(
        text.contains("simulation state version 3 but this build reads version 1"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

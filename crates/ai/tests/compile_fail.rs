//! The AI boundary is a compile error, not a code review.
//!
//! REQ: TA-AI-01
//!
//! Each file under `tests/ui` tries to reach the simulation's world from
//! this crate and must fail to compile. Each is built as a crate of its own
//! with this crate's dependencies and nothing else, which is exactly the
//! point: `sim` is not among them, and `fogged` re-exports no path to it.
//!
//! What is checked is that the compiler rejects the program and why (the
//! error code: an unresolved path, or a private item), not the wording of
//! the message, which changes from one compiler release to the next and
//! would otherwise turn a toolchain update into a red build.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Each case with the error codes that count as the boundary holding:
/// `E0432`/`E0433` for a path that does not resolve, `E0603` for one that
/// reaches something private.
const CASES: &[(&str, &[&str])] = &[
    ("names_the_world", &["E0432", "E0433"]),
    ("reaches_the_crate_root", &["E0432", "E0433"]),
    ("reaches_through_fogged", &["E0603"]),
];

fn crate_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Lays out a crate that depends on `fogged` alone, with the case as its
/// `main.rs`, and returns its manifest.
fn lay_out(case: &str, root: &Path) -> PathBuf {
    let dir = root.join(case);
    fs::create_dir_all(dir.join("src")).unwrap();
    let fogged = crate_dir().join("../fogged");
    let fogged = fogged.canonicalize().unwrap_or(fogged);
    // A literal string: a Windows path has backslashes in it.
    let manifest = format!(
        "[package]\nname = \"ai-boundary-{case}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
         [workspace]\n\n[dependencies]\nfogged = {{ path = '{}' }}\n",
        fogged.display()
    );
    fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    fs::copy(
        crate_dir().join("tests/ui").join(format!("{case}.rs")),
        dir.join("src/main.rs"),
    )
    .unwrap();
    // The workspace's lock, so the same dependency versions are used and
    // nothing has to be resolved afresh.
    if let Ok(lock) = fs::read(crate_dir().join("../../Cargo.lock")) {
        fs::write(dir.join("Cargo.lock"), lock).unwrap();
    }
    dir.join("Cargo.toml")
}

#[test]
fn the_ai_cannot_name_the_world() {
    let root = crate_dir().join("../../target/tests/ai-boundary");
    let target = root.join("target");
    for (case, codes) in CASES {
        let manifest = lay_out(case, &root);
        let out = Command::new(env!("CARGO"))
            .args(["check", "--quiet", "--message-format=json"])
            .arg("--manifest-path")
            .arg(&manifest)
            .env("CARGO_TARGET_DIR", &target)
            .env_remove("RUSTFLAGS")
            .output()
            .unwrap_or_else(|e| panic!("{case}: could not run cargo: {e}"));
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "{case}: compiled, so the AI can reach the world"
        );
        let rejected = codes
            .iter()
            .any(|code| stdout.contains(&format!("\"code\":\"{code}\"")));
        assert!(
            rejected,
            "{case}: expected the compiler to reject it with one of {codes:?}; \
             it said:\n{stdout}\n{stderr}"
        );
    }
}

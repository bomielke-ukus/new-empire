//! Runs the simulation with no window.
//!
//! ```text
//! simrunner determinism [--seed N] [--ticks N] [--players N] [--size N] [--save FILE]
//! simrunner verify FILE
//! simrunner trace  FILE [--out FILE]
//! simrunner record [--dir DIR]
//! simrunner golden [--dir DIR] [--update]
//! simrunner soak   [--matches N] [--seed N] [--ticks N] [--timeout SECS] [--dump DIR]
//! simrunner bench  [--seed N] [--ticks N] [--size N] [--json] [--repeats N] [--stats]
//! simrunner matrix [--out FILE]
//! simrunner battle [--save FILE]
//! simrunner balance [--matches N] [--seed N] [--dump DIR]
//! ```
//!
//! `golden` is the one CI leans on hardest: it replays the committed corpus
//! and compares a digest of *every* tick's hash against a committed value.
//! Running it under several build profiles, operating systems and
//! architectures is what turns "deterministic in this process" into
//! "deterministic, full stop".

mod scenarios;

use scenarios::{Scenario, Style};
use sim::{Replay, SimConfig, Trace};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// Where the committed corpus lives, relative to the repository root.
const CORPUS_DIR: &str = "crates/sim/tests/corpus";

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Flags {
    seed: Option<u64>,
    ticks: Option<u64>,
    players: Option<u8>,
    size: Option<u16>,
    matches: Option<u32>,
    repeats: Option<u32>,
    stats: bool,
    timeout: Option<u64>,
    save: Option<String>,
    out: Option<String>,
    dir: Option<String>,
    dump: Option<String>,
    json: bool,
    update: bool,
    positional: Vec<String>,
}

fn parse(args: &[String]) -> Result<Flags, String> {
    let mut f = Flags::default();
    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str();
        let mut value = |f: &mut Flags| -> Result<String, String> {
            let _ = &f;
            i += 1;
            args.get(i)
                .cloned()
                .ok_or_else(|| format!("{key} needs a value"))
        };
        match key {
            "--json" => f.json = true,
            "--stats" => f.stats = true,
            "--update" => f.update = true,
            "--seed" => {
                let v = value(&mut f)?;
                f.seed = Some(v.parse().map_err(|e| format!("--seed: {e}"))?);
            }
            "--ticks" => {
                let v = value(&mut f)?;
                f.ticks = Some(v.parse().map_err(|e| format!("--ticks: {e}"))?);
            }
            "--players" => {
                let v = value(&mut f)?;
                f.players = Some(v.parse().map_err(|e| format!("--players: {e}"))?);
            }
            "--size" => {
                let v = value(&mut f)?;
                f.size = Some(v.parse().map_err(|e| format!("--size: {e}"))?);
            }
            "--matches" => {
                let v = value(&mut f)?;
                f.matches = Some(v.parse().map_err(|e| format!("--matches: {e}"))?);
            }
            "--repeats" => {
                let v = value(&mut f)?;
                f.repeats = Some(v.parse().map_err(|e| format!("--repeats: {e}"))?);
            }
            "--timeout" => {
                let v = value(&mut f)?;
                f.timeout = Some(v.parse().map_err(|e| format!("--timeout: {e}"))?);
            }
            "--save" => f.save = Some(value(&mut f)?),
            "--out" => f.out = Some(value(&mut f)?),
            "--dir" => f.dir = Some(value(&mut f)?),
            "--dump" => f.dump = Some(value(&mut f)?),
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other => f.positional.push(other.to_string()),
        }
        i += 1;
    }
    if let Some(p) = f.players {
        if p == 0 || p as usize > sim::MAX_PLAYERS {
            return Err(format!("--players must be 1..={}", sim::MAX_PLAYERS));
        }
    }
    Ok(f)
}

/// A scenario built from command-line flags, for ad-hoc runs.
fn ad_hoc(f: &Flags) -> Scenario {
    let players = f.players.unwrap_or(4);
    Scenario {
        name: "ad-hoc",
        purpose: "command line",
        seed: f.seed.unwrap_or(1),
        ticks: f.ticks.unwrap_or(10_000),
        config: sim::SimConfig {
            map: sim::MapSpec {
                kind: sim::MapKind::Inland,
                size: f.size.unwrap_or(128),
                players,
            },
            ..sim::SimConfig::default()
        },
        style: Style::Everything,
    }
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

fn determinism(f: &Flags) -> ExitCode {
    let s = ad_hoc(f);
    println!(
        "synthesising: seed={} ticks={} players={} size={}",
        s.seed, s.ticks, s.config.map.players, s.config.map.size
    );
    let t0 = Instant::now();
    let replay = s.synthesise();
    println!(
        "  {} commands recorded in {:.2?}",
        replay.commands.len(),
        t0.elapsed()
    );

    if let Some(path) = &f.save {
        if let Err(e) = write_replay(Path::new(path), &replay) {
            return fail(&e);
        }
        println!("  saved replay to {path}");
    }
    verify_replay(&replay)
}

fn verify_file(f: &Flags) -> ExitCode {
    let Some(path) = f.positional.first() else {
        return usage("verify needs a file");
    };
    let replay = match read_replay(Path::new(path)) {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };
    println!(
        "loaded {path}: seed={} ticks={} commands={}",
        replay.seed,
        replay.ticks,
        replay.commands.len()
    );
    verify_replay(&replay)
}

fn verify_replay(replay: &Replay) -> ExitCode {
    println!("running twice and comparing every tick...");
    let t0 = Instant::now();
    match replay.verify() {
        Ok(hash) => {
            let per_tick = t0.elapsed() / (2 * replay.ticks.max(1) as u32);
            println!(
                "OK  {} ticks identical, final hash {hash:016x}, {per_tick:.1?}/tick",
                replay.ticks
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e.to_string()),
    }
}

fn trace(f: &Flags) -> ExitCode {
    let Some(path) = f.positional.first() else {
        return usage("trace needs a replay file");
    };
    let replay = match read_replay(Path::new(path)) {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };
    let trace = match replay.trace_digest() {
        Ok(t) => t,
        Err(e) => return fail(&e.to_string()),
    };
    println!("{path}: {trace}");
    if let Some(out) = &f.out {
        if let Err(e) = write_trace(Path::new(out), &trace) {
            return fail(&e);
        }
        println!("wrote {out}");
    }
    ExitCode::SUCCESS
}

/// Regenerates every corpus replay from its scenario definition.
///
/// Separate from `golden --update` on purpose: this rewrites the *inputs*,
/// which is a much bigger claim than rewriting the expected outputs, and the
/// two should never happen in the same commit by accident.
fn record(f: &Flags) -> ExitCode {
    let dir = PathBuf::from(f.dir.as_deref().unwrap_or(CORPUS_DIR));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return fail(&format!("could not create {}: {e}", dir.display()));
    }
    for s in scenarios::corpus() {
        let t0 = Instant::now();
        let replay = s.synthesise();
        let path = dir.join(format!("{}.ron", s.name));
        if let Err(e) = write_replay(&path, &replay) {
            return fail(&e);
        }
        // Report what the scenario actually achieved, not just that it ran.
        // A corpus entry that plays 4,000 ticks and accomplishes nothing looks
        // identical to one that works, right up until it fails to catch a bug.
        let sim = match replay.run(|_, _| {}) {
            Ok(s) => s,
            Err(e) => return fail(&e.to_string()),
        };
        let gathered: i32 = sim.players().iter().flat_map(|p| p.gathered).sum();
        let buildings = sim
            .world()
            .slots()
            .filter(|s| {
                let i = s.index();
                sim.world().owner[i] != sim::kinds::GAIA
                    && !sim::kinds::info(sim.world().kind[i]).mobile
            })
            .count();
        println!(
            "recorded {:<20} {:>6} ticks {:>5} cmds  {:>5} live  {:>5} bldgs  \
             {:>6} gathered  {:.2?}",
            s.name,
            replay.ticks,
            replay.commands.len(),
            sim.world().len(),
            buildings,
            gathered,
            t0.elapsed()
        );
    }
    println!("\nnow run `simrunner golden --update` to refresh the expected digests");
    ExitCode::SUCCESS
}

/// Replays the committed corpus and compares against the committed digests.
fn golden(f: &Flags) -> ExitCode {
    let dir = PathBuf::from(f.dir.as_deref().unwrap_or(CORPUS_DIR));
    let scenarios = scenarios::corpus();
    let mut failures = Vec::new();
    let mut checked = 0;

    for s in &scenarios {
        let replay_path = dir.join(format!("{}.ron", s.name));
        let golden_path = dir.join(format!("{}.golden", s.name));
        let replay = match read_replay(&replay_path) {
            Ok(r) => r,
            Err(e) => {
                failures.push(e);
                continue;
            }
        };
        let t0 = Instant::now();
        let got = match replay.trace_digest() {
            Ok(t) => t,
            Err(e) => {
                failures.push(format!("{}: {e}", s.name));
                continue;
            }
        };

        if f.update {
            if let Err(e) = write_trace(&golden_path, &got) {
                failures.push(e);
                continue;
            }
            println!("updated {:<24} {got}", s.name);
            continue;
        }

        match read_trace(&golden_path) {
            Err(e) => failures.push(e),
            Ok(want) if want == got => {
                checked += 1;
                let per_tick = t0.elapsed() / got.ticks.max(1) as u32;
                println!("ok  {:<24} {got}  {per_tick:.1?}/tick", s.name);
            }
            Ok(want) => {
                println!("FAIL {:<24}", s.name);
                println!("       expected {want}");
                println!("       got      {got}");
                let first = first_divergence(&replay, &want);
                match first {
                    Some(tick) => println!("       diverges from tick {tick}"),
                    None => println!("       per-tick hashes agree; only the summary differs"),
                }
                failures.push(format!(
                    "{}: behaviour changed ({}). If intended, run \
                     `simrunner golden --update` and review the diff.",
                    s.name, s.purpose
                ));
            }
        }
    }

    if f.update {
        println!("\nwrote {} digests", scenarios.len());
        return ExitCode::SUCCESS;
    }
    if failures.is_empty() {
        println!("\nok: {checked} corpus replays match their committed digests");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        for e in &failures {
            eprintln!("error: {e}");
        }
        ExitCode::FAILURE
    }
}

/// Re-runs a replay to find the earliest tick whose hash cannot be part of the
/// committed digest. Best effort: it can only narrow the range when the golden
/// file was produced by a build that also stored the final hash.
fn first_divergence(replay: &Replay, want: &Trace) -> Option<u64> {
    if want.ticks != replay.ticks {
        return Some(0);
    }
    // We only have the digest, not the per-tick trace, so bisect by re-folding
    // prefixes is not possible either. Report the final-hash tick if that is
    // where they differ, otherwise nothing useful.
    let got = replay.trace_digest().ok()?;
    (got.final_hash != want.final_hash).then_some(replay.ticks)
}

fn soak(f: &Flags) -> ExitCode {
    let matches = f.matches.unwrap_or(100);
    let base_seed = f.seed.unwrap_or(0);
    let ticks = f.ticks.unwrap_or(2_000);
    let timeout = Duration::from_secs(f.timeout.unwrap_or(120));
    let dump = f.dump.as_deref().map(PathBuf::from);
    if let Some(d) = &dump {
        if let Err(e) = std::fs::create_dir_all(d) {
            return fail(&format!("could not create {}: {e}", d.display()));
        }
    }

    println!("soak: {matches} matches of up to {ticks} ticks, seeds {base_seed}..");
    let t0 = Instant::now();
    let mut failures = 0;

    for n in 0..matches {
        let seed = base_seed.wrapping_add(n as u64);
        let scenario = randomised_scenario(seed, ticks);
        let replay = scenario.synthesise();

        // Each match runs on its own thread so a hang is reported rather than
        // hanging CI, and `catch_unwind` turns a panic into a failing seed
        // rather than a dead process.
        let to_run = replay.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut violation = None;
                let sim = to_run.run(|_, _| {}).map_err(|e| e.to_string())?;
                if let Err(v) = sim.check() {
                    violation = Some(v.to_string());
                }
                match violation {
                    Some(v) => Err(v),
                    None => Ok(sim.state_hash()),
                }
            }));
            let _ = tx.send(match outcome {
                Ok(r) => r,
                Err(_) => Err("panicked".to_string()),
            });
        });

        match rx.recv_timeout(timeout) {
            Ok(Ok(_)) => {}
            Ok(Err(reason)) => {
                failures += 1;
                eprintln!("FAIL seed {seed}: {reason}");
                dump_replay(&dump, seed, &replay);
            }
            Err(_) => {
                failures += 1;
                eprintln!("HANG seed {seed}: no result after {timeout:?}");
                dump_replay(&dump, seed, &replay);
                // The thread is wedged; do not join it.
                std::mem::forget(handle);
                continue;
            }
        }
        let _ = handle.join();

        if (n + 1) % 25 == 0 {
            println!("  {} / {matches} ({:.1?})", n + 1, t0.elapsed());
        }
    }

    if failures == 0 {
        println!("ok: {matches} matches, no panics, no hangs, no broken invariants");
        ExitCode::SUCCESS
    } else {
        fail(&format!("{failures} of {matches} matches failed"))
    }
}

/// Randomises the whole config space, including the degenerate corners. The
/// point of a soak is the configs nobody thought to write a test for.
fn randomised_scenario(seed: u64, ticks: u64) -> Scenario {
    let mut r = sim::Rng::new(seed ^ 0x50AC);
    // Sizes at both clamps and in between; `MapSpec` clamps to 48..=256.
    let size = match r.below(10) {
        0 => 48,
        1 => 256,
        _ => r.range_i32(48, 200) as u16,
    };
    let style = match r.below(4) {
        0 => Style::Idle,
        1 => Style::Marching,
        2 => Style::Economy,
        _ => Style::Everything,
    };
    Scenario {
        name: "soak",
        purpose: "randomised",
        seed,
        ticks: 200 + (r.next_u64() % ticks.max(1)),
        config: SimConfig {
            map: sim::MapSpec {
                kind: if r.chance(1, 4) {
                    sim::MapKind::Flat
                } else {
                    sim::MapKind::Inland
                },
                size,
                players: 1 + r.below(8) as u8,
            },
            // Both extremes: a cap that refuses almost everything, and one
            // that never binds.
            max_entities: match r.below(8) {
                0 => 0,
                1 => 1,
                2 => 200,
                _ => r.below(6000),
            },
            wander: r.chance(3, 4),
            pop_cap_max: match r.below(6) {
                0 => 0,
                1 => 1,
                _ => r.below(200),
            },
            starting_stockpile: match r.below(4) {
                0 => [0; 4],
                1 => [5000; 4],
                _ => sim::DEFAULT_STOCKPILE,
            },
        },
        style,
    }
}

fn dump_replay(dir: &Option<PathBuf>, seed: u64, replay: &Replay) {
    let Some(dir) = dir else {
        eprintln!("  (pass --dump DIR to save the replay that caused this)");
        return;
    };
    let path = dir.join(format!("soak-{seed}.ron"));
    match write_replay(&path, replay) {
        Ok(()) => eprintln!("  replay written to {}", path.display()),
        Err(e) => eprintln!("  could not write replay: {e}"),
    }
}

fn bench(f: &Flags) -> ExitCode {
    let repeats = f.repeats.unwrap_or(3).max(1);
    let explicit = f.seed.is_some() || f.ticks.is_some() || f.size.is_some();
    let list = if explicit {
        vec![ad_hoc(f)]
    } else {
        scenarios::benchmarks()
    };

    let mut rows = Vec::new();
    for s in &list {
        let replay = s.synthesise();
        // Best-of-N: shared CI runners are noisy, and the fastest run is the
        // one least contaminated by whatever else the machine was doing.
        let mut best: Option<(Duration, Vec<u128>)> = None;
        for _ in 0..repeats {
            let mut per_tick = Vec::with_capacity(replay.ticks as usize);
            let t0 = Instant::now();
            let mut last = Instant::now();
            let sim = match replay.run(|_, _| {
                let now = Instant::now();
                per_tick.push(now.duration_since(last).as_nanos());
                last = now;
            }) {
                Ok(s) => s,
                Err(e) => return fail(&e.to_string()),
            };
            let total = t0.elapsed();
            let _ = sim;
            if best.as_ref().is_none_or(|(b, _)| total < *b) {
                best = Some((total, per_tick));
            }
        }
        if f.stats {
            // One more run, reading the tick diagnostics, to say where the
            // time goes: fields built, tiles flooded, corridor searches.
            let mut sim = sim::Simulation::new(replay.seed, replay.config.clone());
            let mut next = 0;
            let mut sum = [0u64; 9];
            let mut peak = [0u32; 9];
            while sim.tick() < replay.ticks {
                while let Some((tick, command)) = replay.commands.get(next) {
                    if *tick != sim.tick() {
                        break;
                    }
                    sim.issue(command.clone());
                    next += 1;
                }
                sim.step();
                let st = sim.stats();
                let row = [
                    st.path_searches,
                    st.path_nodes,
                    st.path_deferred,
                    st.path_failures,
                    st.corridors,
                    st.full_fields,
                    st.steers,
                    st.fields_live,
                    st.own_fallbacks,
                ];
                for k in 0..9 {
                    sum[k] += row[k] as u64;
                    peak[k] = peak[k].max(row[k]);
                }
            }
            println!(
                "{:<20} builds {} (peak {}) flooded {} (peak {}) deferred {} failures {} \
                 corridors {} (peak {}) full {} steers {} (peak {}) live {} own-goal {}",
                s.name,
                sum[0],
                peak[0],
                sum[1],
                peak[1],
                sum[2],
                sum[3],
                sum[4],
                peak[4],
                sum[5],
                sum[6],
                peak[6],
                peak[7],
                sum[8]
            );
        }
        let (total, mut per_tick) = best.expect("repeats >= 1");
        per_tick.sort_unstable();
        let pick = |q: f64| -> u128 {
            if per_tick.is_empty() {
                return 0;
            }
            let i = ((per_tick.len() as f64 - 1.0) * q) as usize;
            per_tick[i]
        };
        rows.push(BenchRow {
            name: s.name,
            ticks: replay.ticks,
            players: s.config.map.players,
            total_ms: total.as_secs_f64() * 1000.0,
            p50_ns: pick(0.50),
            p99_ns: pick(0.99),
            max_ns: per_tick.last().copied().unwrap_or(0),
        });
    }

    if f.json {
        println!("[");
        for (i, r) in rows.iter().enumerate() {
            let comma = if i + 1 == rows.len() { "" } else { "," };
            println!(
                "  {{\"scenario\":\"{}\",\"ticks\":{},\"players\":{},\"total_ms\":{:.3},\
                 \"p50_ns\":{},\"p99_ns\":{},\"max_ns\":{}}}{comma}",
                r.name, r.ticks, r.players, r.total_ms, r.p50_ns, r.p99_ns, r.max_ns
            );
        }
        println!("]");
    } else {
        println!(
            "{:<18} {:>7} {:>7} {:>10} {:>10} {:>10} {:>10}",
            "scenario", "ticks", "players", "total ms", "p50 us", "p99 us", "max us"
        );
        for r in &rows {
            println!(
                "{:<18} {:>7} {:>7} {:>10.1} {:>10.1} {:>10.1} {:>10.1}",
                r.name,
                r.ticks,
                r.players,
                r.total_ms,
                r.p50_ns as f64 / 1000.0,
                r.p99_ns as f64 / 1000.0,
                r.max_ns as f64 / 1000.0,
            );
        }
    }
    ExitCode::SUCCESS
}

struct BenchRow {
    name: &'static str,
    ticks: u64,
    players: u8,
    total_ms: f64,
    p50_ns: u128,
    p99_ns: u128,
    max_ns: u128,
}

// ---------------------------------------------------------------------------
// File helpers
// ---------------------------------------------------------------------------

fn read_replay(path: &Path) -> Result<Replay, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let replay: Replay =
        ron::from_str(&text).map_err(|e| format!("could not parse {}: {e}", path.display()))?;
    replay
        .validate()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(replay)
}

/// Writes a replay as compact RON.
///
/// Not pretty-printed: a match with a few thousand multi-unit `Move` orders
/// pretty-prints to tens of megabytes, and `docs/04` §10 is right that a
/// replay should be kilobytes. These files are read by machines and diffed
/// only to see *that* they changed, never to read the change.
fn write_replay(path: &Path, replay: &Replay) -> Result<(), String> {
    let text = ron::to_string(replay).map_err(|e| format!("could not serialise replay: {e}"))?;
    std::fs::write(path, format!("{text}\n"))
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}

fn read_trace(path: &Path) -> Result<Trace, String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "could not read {}: {e}\n       (a new corpus entry needs \
             `simrunner golden --update`)",
            path.display()
        )
    })?;
    ron::from_str(&text).map_err(|e| format!("could not parse {}: {e}", path.display()))
}

fn write_trace(path: &Path, trace: &Trace) -> Result<(), String> {
    let text = ron::ser::to_string_pretty(trace, ron::ser::PrettyConfig::default())
        .map_err(|e| format!("could not serialise trace: {e}"))?;
    std::fs::write(path, format!("{text}\n"))
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------

/// The damage matrix (`docs/02` §8) from the kinds table, as Markdown, to
/// stdout or `--out`. Committed at `docs/damage-matrix.md`;
/// `scripts/check-generated.sh` regenerates and diffs it.
fn matrix(f: &Flags) -> ExitCode {
    let md = sim::combat::matrix_markdown();
    match &f.out {
        Some(path) => match std::fs::write(path, md) {
            Ok(()) => {
                println!("wrote {path}");
                ExitCode::SUCCESS
            }
            Err(e) => fail(&format!("{path}: {e}")),
        },
        None => {
            print!("{md}");
            ExitCode::SUCCESS
        }
    }
}

/// The single replay shared by acceptance tests, the corpus and the frame.
fn battle(f: &Flags) -> ExitCode {
    let b = simrunner::arena::battle_40();
    if let Some(path) = &f.save {
        if let Err(e) = write_replay(Path::new(path), &b.replay) {
            return fail(&e);
        }
    }
    println!(
        "40v40: {} ticks, survivors {:?}, longest inactivity {} ticks",
        b.replay.ticks, b.survivors, b.longest_inactivity
    );
    match b.failure {
        Some(e) => fail(&e),
        None if b.survivors == [0, 0] => fail("mutual destruction; expected a surviving force"),
        None => ExitCode::SUCCESS,
    }
}

/// Regression gate for the two explicit counter bonuses in the slice.
fn balance(f: &Flags) -> ExitCode {
    use simrunner::arena;
    let trials = f.matches.unwrap_or(arena::DEFAULT_TRIALS);
    if trials == 0 || trials > 1000 {
        return usage("--matches must be 1..=1000 seed pairs");
    }
    let first = f.seed.unwrap_or(1);
    if first.checked_add(u64::from(trials) - 1).is_none() {
        return usage("seed range overflows");
    }
    let mut failed = false;
    for &(name, counter, target, budget) in arena::MATCHUPS {
        let rosters = [
            arena::roster_for_budget(counter, budget),
            arena::roster_for_budget(target, budget),
        ];
        let mut wins = [0u32; 2];
        for n in 0..trials {
            let seed = first + u64::from(n);
            for swapped in [false, true] {
                let b = arena::finish(arena::setup(&rosters, seed, swapped));
                let won = b.failure.is_none() && b.winner() == Some(u8::from(swapped));
                if won {
                    wins[usize::from(swapped)] += 1;
                } else {
                    eprintln!(
                        "{name}: seed {seed}, swapped {swapped}, survivors {:?}: {}",
                        b.survivors,
                        b.failure.as_deref().unwrap_or("counter did not win")
                    );
                    if let Some(dir) = &f.dump {
                        let path = Path::new(dir).join(format!("{name}-{seed}-{swapped}.ron"));
                        if let Err(e) = std::fs::create_dir_all(dir)
                            .map_err(|e| e.to_string())
                            .and_then(|_| write_replay(&path, &b.replay))
                        {
                            return fail(&e);
                        }
                    }
                }
                if b.failure.is_some() {
                    failed = true;
                }
            }
        }
        println!("{name}: {} vs {} units, {budget} resources each; wins {}/{} left, {}/{} right (need 90% each)",
            rosters[0].len(), rosters[1].len(), wins[0], trials, wins[1], trials);
        if wins.iter().any(|&n| n * 10 < trials * 9) {
            failed = true;
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn usage(err: &str) -> ExitCode {
    eprintln!("error: {err}\n");
    eprintln!("usage:");
    eprintln!(
        "  simrunner determinism [--seed N] [--ticks N] [--players N] [--size N] [--save FILE]"
    );
    eprintln!("  simrunner verify FILE");
    eprintln!("  simrunner trace  FILE [--out FILE]");
    eprintln!("  simrunner record [--dir DIR]");
    eprintln!("  simrunner golden [--dir DIR] [--update]");
    eprintln!("  simrunner soak   [--matches N] [--seed N] [--ticks N] [--timeout S] [--dump DIR]");
    eprintln!(
        "  simrunner bench  [--seed N] [--ticks N] [--size N] [--json] [--repeats N] [--stats]"
    );
    eprintln!("  simrunner matrix [--out FILE]");
    eprintln!("  simrunner battle [--save FILE]");
    eprintln!("  simrunner balance [--matches N] [--seed N] [--dump DIR]");
    ExitCode::from(2)
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(subcommand) = args.first().cloned() else {
        return usage("missing subcommand");
    };
    let flags = match parse(&args[1..]) {
        Ok(f) => f,
        Err(e) => return usage(&e),
    };
    match subcommand.as_str() {
        "determinism" => determinism(&flags),
        "verify" => verify_file(&flags),
        "trace" => trace(&flags),
        "record" => record(&flags),
        "golden" => golden(&flags),
        "soak" => soak(&flags),
        "bench" => bench(&flags),
        "matrix" => matrix(&flags),
        "battle" => battle(&flags),
        "balance" => balance(&flags),
        other => usage(&format!("unknown subcommand {other}")),
    }
}

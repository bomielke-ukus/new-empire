//! Runs the simulation with no window.
//!
//! ```text
//! simrunner determinism [--seed N] [--ticks N] [--players N] [--units N] [--save FILE]
//! simrunner verify FILE
//! simrunner bench   [--seed N] [--ticks N] [--units N]
//! ```
//!
//! `determinism` synthesises a busy command log, runs it twice, and exits
//! non-zero on the first tick whose state hashes disagree. This is M0's
//! acceptance test and runs in CI on every platform.

use sim::{Command, CommandKind, Replay, Rng, SimConfig, Simulation, Vec2Fx};
use std::process::ExitCode;
use std::time::Instant;

struct Args {
    seed: u64,
    ticks: u64,
    players: u8,
    units: u32,
    save: Option<String>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut out = Args {
        seed: 1,
        ticks: 10_000,
        players: 4,
        units: 400,
        save: None,
    };
    let mut i = 0;
    while i < args.len() {
        let key = &args[i];
        let val = args
            .get(i + 1)
            .ok_or_else(|| format!("{key} needs a value"))?;
        match key.as_str() {
            "--seed" => out.seed = val.parse().map_err(|e| format!("--seed: {e}"))?,
            "--ticks" => out.ticks = val.parse().map_err(|e| format!("--ticks: {e}"))?,
            "--players" => out.players = val.parse().map_err(|e| format!("--players: {e}"))?,
            "--units" => out.units = val.parse().map_err(|e| format!("--units: {e}"))?,
            "--save" => out.save = Some(val.clone()),
            _ => return Err(format!("unknown flag {key}")),
        }
        i += 2;
    }
    if out.players == 0 || out.players as usize > sim::MAX_PLAYERS {
        return Err(format!("--players must be 1..={}", sim::MAX_PLAYERS));
    }
    Ok(out)
}

/// Builds a match with `units` entities spread across `players`, then issues a
/// stream of move/stop/despawn/spawn commands over `ticks` ticks. The command
/// stream itself comes from a *separate* RNG so it is reproducible without
/// depending on simulation state.
fn synthesise(a: &Args) -> Replay {
    let config = SimConfig::default();
    let mut sim = Simulation::new(a.seed, config);
    let mut driver = Rng::new(a.seed ^ 0xD1CE);
    let map = sim.config().map_size;

    for n in 0..a.units {
        let player = (n % a.players as u32) as u8;
        let x = driver.range_i32(0, map);
        let y = driver.range_i32(0, map);
        sim.issue(Command {
            player,
            kind: CommandKind::Spawn {
                kind: 1,
                pos: Vec2Fx::from_int(x, y),
            },
        });
    }

    while sim.tick() < a.ticks {
        // Roughly one command per player every 10 ticks — a busy human.
        for player in 0..a.players {
            if !driver.chance(1, 10) {
                continue;
            }
            let owned: Vec<_> = sim
                .world()
                .slots()
                .filter(|s| sim.world().owner[s.index()] == player)
                .map(|s| sim.world().id_at(s))
                .collect();
            if owned.is_empty() {
                continue;
            }
            let pick = |d: &mut Rng, n: usize| -> Vec<_> {
                let start = d.below(owned.len() as u32) as usize;
                owned
                    .iter()
                    .cycle()
                    .skip(start)
                    .take(n.min(owned.len()))
                    .copied()
                    .collect()
            };
            let kind = match driver.below(20) {
                0 => CommandKind::Despawn {
                    id: owned[driver.below(owned.len() as u32) as usize],
                },
                1 => CommandKind::Spawn {
                    kind: 1,
                    pos: Vec2Fx::from_int(driver.range_i32(0, map), driver.range_i32(0, map)),
                },
                2..=4 => CommandKind::Stop {
                    ids: pick(&mut driver, 12),
                },
                _ => {
                    let n = 1 + driver.below(40) as usize;
                    let ids = pick(&mut driver, n);
                    let target = Vec2Fx::from_int(
                        driver.range_i32(-10, map + 10),
                        driver.range_i32(-10, map + 10),
                    );
                    CommandKind::Move { ids, target }
                }
            };
            sim.issue(Command { player, kind });
        }
        sim.step();
    }
    sim.replay()
}

fn determinism(args: &[String]) -> ExitCode {
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    println!(
        "synthesising: seed={} ticks={} players={} units={}",
        a.seed, a.ticks, a.players, a.units
    );
    let t0 = Instant::now();
    let replay = synthesise(&a);
    println!(
        "  {} commands recorded in {:.2?}",
        replay.commands.len(),
        t0.elapsed()
    );

    if let Some(path) = &a.save {
        match ron::ser::to_string_pretty(&replay, ron::ser::PrettyConfig::default()) {
            Ok(s) => match std::fs::write(path, s) {
                Ok(()) => println!("  saved replay to {path}"),
                Err(e) => return fail(&format!("could not write {path}: {e}")),
            },
            Err(e) => return fail(&format!("could not serialise replay: {e}")),
        }
    }

    verify_replay(&replay)
}

fn verify_file(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else {
        return usage("verify needs a file");
    };
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return fail(&format!("could not read {path}: {e}")),
    };
    let replay: Replay = match ron::from_str(&text) {
        Ok(r) => r,
        Err(e) => return fail(&format!("could not parse {path}: {e}")),
    };
    if replay.version != Replay::VERSION {
        return fail(&format!(
            "replay version {} but this build reads version {}",
            replay.version,
            Replay::VERSION
        ));
    }
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
        Err(d) => fail(&format!("DESYNC {d}")),
    }
}

fn bench(args: &[String]) -> ExitCode {
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let replay = synthesise(&a);
    let t0 = Instant::now();
    let sim = replay.run(|_, _| {});
    let dt = t0.elapsed();
    println!(
        "{} ticks, {} live entities at end: {:.2?} total, {:.1?}/tick, rng draws {}",
        replay.ticks,
        sim.world().len(),
        dt,
        dt / replay.ticks.max(1) as u32,
        sim.rng_draws()
    );
    ExitCode::SUCCESS
}

fn usage(err: &str) -> ExitCode {
    eprintln!("error: {err}\n");
    eprintln!("usage:");
    eprintln!(
        "  simrunner determinism [--seed N] [--ticks N] [--players N] [--units N] [--save FILE]"
    );
    eprintln!("  simrunner verify FILE");
    eprintln!("  simrunner bench [--seed N] [--ticks N] [--units N]");
    ExitCode::from(2)
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("determinism") => determinism(&args[1..]),
        Some("verify") => verify_file(&args[1..]),
        Some("bench") => bench(&args[1..]),
        Some(other) => usage(&format!("unknown subcommand {other}")),
        None => usage("missing subcommand"),
    }
}

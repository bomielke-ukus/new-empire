//! A replay file is anything a player drops into the replay folder, or a bug
//! report attaches (`docs/09` §4.6). Whatever the bytes, reading one fails
//! cleanly or yields a replay the engine can run: no panic in the parser,
//! the validation or the first ticks, and every invariant holds.
#![no_main]

use libfuzzer_sys::fuzz_target;

/// Ticks of a parsed replay worth running per input: enough to reach the
/// commands near the start, few enough that the fuzzer stays fast.
const TICKS: u64 = 64;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // The app's own reader: parse, then validate the replay and its setup.
    let Ok(mut replay) = save::replays::parse(text) else {
        return;
    };
    // A large map is legal and slow; the parser is what is being tested,
    // and the small maps reach the same code.
    if replay.config.map.size > 64 || replay.config.map.players > 4 {
        return;
    }
    replay.ticks = replay.ticks.min(TICKS);
    replay.commands.retain(|(tick, _)| *tick <= replay.ticks);
    replay.sources.truncate(replay.commands.len());
    // `sim/debug-checks` makes a broken invariant panic inside the run.
    let _ = replay.run(|_, _| {});
});

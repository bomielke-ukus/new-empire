# Fuzz targets

Two doors into the simulation, fed arbitrary bytes by libFuzzer
(`docs/09` §4.6). Both build `sim` with `debug-checks`, so a broken
invariant, resource conservation included, is a crash like any panic.

| Target | What the bytes are |
|---|---|
| `replay_reader` | A replay file: through `save::replays::parse` (the app's reader), and if it passes, its first 64 ticks run on a map up to 64 tiles |
| `commands` | A script of commands, every variant, against a land and a coastal world: handles mostly live, sometimes forged; valid or not, each is issued and the match runs |

## Running

cargo-fuzz needs a nightly toolchain; the repository's `rust-toolchain.toml`
pins stable, so name it:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run commands -- -max_total_time=600 -max_len=2048 -len_control=0
# Seed the reader with real replays first, or it spends its time on syntax.
mkdir -p corpus/replay_reader && cp ../crates/sim/tests/corpus/battle-40v40.ron corpus/replay_reader/
cargo +nightly fuzz run replay_reader -- -max_total_time=600
```

A crash lands in `artifacts/<target>/`. `cargo +nightly fuzz run <target>
<file>` reproduces it, `cargo +nightly fuzz tmin <target> <file>` shrinks it.
Fix the bug, then pin it with an ordinary test, so it stays fixed without
the fuzzer.

The nightly workflow (`.github/workflows/nightly.yml`) runs each target for
twenty minutes and keeps its corpus between nights.

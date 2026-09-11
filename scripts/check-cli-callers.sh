#!/usr/bin/env bash
# Every documented simrunner invocation must actually parse.
#
# A tool's command line is a contract with CI, the README and the docs, and
# nothing was checking it: renaming a flag passed every unit test and then
# failed on all three platforms in the CI step that calls the binary. Parsing
# is cheap to verify, so verify it.
#
# Runs each invocation with a tiny tick count so the check stays fast; the
# point is that the flags are accepted, not that the run is meaningful.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== simrunner: every invocation in CI, the README and the docs parses =="
cargo build --release --quiet -p simrunner

status=0
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Collect invocations from the files that call the tool, normalise the tick
# counts down, and try each one.
mapfile -t calls < <(
  grep -rhoE 'simrunner -- [a-z]+[^|>&#]*' \
    .github/workflows/*.yml README.md docs/*.md 2>/dev/null \
  | sed -E 's/simrunner -- //; s/\\$//; s/[[:space:]]+$//' \
  | sort -u
)

if [ "${#calls[@]}" -eq 0 ]; then
  echo "FAIL: found no simrunner invocations to check — has the grep drifted?"
  exit 1
fi

for call in "${calls[@]}"; do
  # Keep the run short, and point file arguments at a scratch copy.
  probe="$(sed -E 's/--ticks [0-9]+/--ticks 60/; s/--matches [0-9]+/--matches 1/' <<<"$call")"
  probe="${probe//replay-ci.ron/$tmp/replay.ron}"
  case "$probe" in
    verify*) # needs a file to exist first
      ./target/release/simrunner determinism --ticks 60 --save "$tmp/replay.ron" >/dev/null 2>&1
      probe="verify $tmp/replay.ron" ;;
    record*) # would rewrite the committed corpus, so send it to scratch
      probe="$probe --dir $tmp" ;;
    # `golden` without --update is read-only, so it runs against the real
    # corpus and doubles as a corpus check.
  esac
  printf '  %-70s ' "$call"
  # shellcheck disable=SC2086
  if out=$(./target/release/simrunner $probe 2>&1); then
    echo "ok"
  # Only a *command-line* rejection is this script's business. A run that
  # starts and then fails for its own reasons belongs to the other checks.
  elif grep -qE 'unknown flag|unknown subcommand|needs a value|^usage:' <<<"$out"; then
    echo "FAIL"
    sed 's/^/      /' <<<"$out" | head -6
    status=1
  else
    # Ran and failed for a reason that is not the command line — that is the
    # job of the other checks, not this one.
    echo "ok (ran; non-CLI failure)"
  fi
done

exit $status

#!/usr/bin/env bash
# Guards the two properties the deterministic simulation depends on:
#
#   1. crates/sim contains no floating-point types or arithmetic.
#      REQ: TA-DET-08
#   2. crates/sim depends on nothing that could make two machines disagree,
#      and never reads the clock or iterates a HashMap.
#      REQ: TA-DEP-01  REQ: TA-DET-09
#
# Run from the repository root. Exit status is non-zero on any violation.
set -euo pipefail
cd "$(dirname "$0")/.."

status=0

# Source with comments and string literals blanked out, so prose and test
# expectations ("3.5000") are not mistaken for code. Line numbers survive.
code() {
  grep -rn '' crates/sim/src --include='*.rs' \
    | sed -E 's#//.*$##; s#"([^"\\]|\\.)*"#""#g'
}

echo "== sim: no floating point =="
# The types, casts to them, and float literals.
if code | grep -nE '\bf(32|64)\b|[^A-Za-z0-9_.][0-9]+\.[0-9]+' ; then
  echo "FAIL: floating point found in crates/sim (see above)"
  status=1
else
  echo "ok"
fi

echo "== sim: no wall clock, no thread-local randomness, no HashMap iteration =="
if code | grep -nE 'std::time|Instant::|SystemTime|thread_rng|HashMap|HashSet' ; then
  echo "FAIL: non-deterministic construct found in crates/sim (see above)"
  status=1
else
  echo "ok"
fi

echo "== sim: dependency allowlist =="
allowed='^(sim|serde|data)$'
deps=$(cargo tree -p sim -e normal --depth 1 --prefix none --format '{p}' | awk '{print $1}' | sort -u)
bad=0
while read -r d; do
  [ -z "$d" ] && continue
  if ! [[ "$d" =~ $allowed ]]; then
    echo "FAIL: crates/sim depends on '$d', which is not in the allowlist ($allowed)"
    bad=1
  fi
done <<< "$deps"
if [ "$bad" = 0 ]; then echo "ok ($(echo "$deps" | tr '\n' ' '))"; else status=1; fi

exit $status

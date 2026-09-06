#!/usr/bin/env bash
# Runs the benchmark scenarios and fails if any exceeds its ceiling in
# perf/budgets.ron.
#
# A cliff detector, not a precision instrument — see the commentary in
# perf/budgets.ron for why the numbers are as loose as they are.
set -euo pipefail
cd "$(dirname "$0")/.."

repeats="${PERF_REPEATS:-3}"
echo "== benchmarks (best of $repeats) =="
cargo run --release --quiet -p simrunner -- bench --json --repeats "$repeats" > /tmp/bench.json
cat /tmp/bench.json

python3 - "$PWD/perf/budgets.ron" /tmp/bench.json <<'PY'
import json, re, sys

budgets_path, results_path = sys.argv[1], sys.argv[2]
text = open(budgets_path).read()
# The budget file is small and fixed-shape; a regex beats a RON parser here.
budgets = {}
for block in re.findall(r"\(\s*name:\s*\"([^\"]+)\".*?max_p99_us:\s*([0-9.]+)", text, re.S):
    budgets[block[0]] = float(block[1])

results = json.load(open(results_path))
if not results:
    sys.exit("no benchmark results produced")

failed = []
print()
print(f"{'scenario':<18}{'p99 us':>10}{'ceiling':>10}{'headroom':>11}")
for r in results:
    name = r["scenario"]
    p99 = r["p99_ns"] / 1000.0
    ceiling = budgets.get(name)
    if ceiling is None:
        failed.append(f"{name}: no ceiling in perf/budgets.ron")
        print(f"{name:<18}{p99:>10.1f}{'-':>10}{'-':>11}")
        continue
    headroom = ceiling / p99 if p99 else float("inf")
    print(f"{name:<18}{p99:>10.1f}{ceiling:>10.1f}{headroom:>10.1f}x")
    if p99 > ceiling:
        failed.append(f"{name}: p99 {p99:.1f} us exceeds the {ceiling:.1f} us ceiling")

unseen = set(budgets) - {r["scenario"] for r in results}
for name in sorted(unseen):
    failed.append(f"{name}: has a ceiling but did not run")

print()
if failed:
    for f in failed:
        print(f"FAIL: {f}")
    print("\nIf this is a deliberate cost, raise the ceiling in perf/budgets.ron")
    print("in the same commit and say in the message what bought the time.")
    sys.exit(1)
print("ok: every scenario is inside its ceiling")
PY

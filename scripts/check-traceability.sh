#!/usr/bin/env bash
# Maps every requirement ID in docs/ to the tests that reference it.
#
# This is the half of testing that catches *missing features* rather than
# broken ones. A requirement written in a spec and never implemented produces
# no failing test, because there is no test — which is exactly why a milestone
# can be declared done with a system silently absent.
#
# Requirements are tagged in the specs as [TA-DET-01], [UX-SEL-01] and so on.
# Tests claim them with a `REQ: <id>` marker in a comment. This script pairs
# them up.
#
# Milestones that have not landed yet are reported as planned, not failed:
# a check that is red from day one until M7 gets disabled in week two.
set -euo pipefail
cd "$(dirname "$0")/.."

# Which milestones' requirements must be covered *now*. Extend this as
# milestones land; that edit is the moment the new requirements start being
# enforced, and it belongs in the same commit as the milestone.
LANDED_PREFIXES="${TRACEABILITY_LANDED:-TA-FX TA-VEC TA-ANG TA-RNG TA-CMD TA-ENT TA-DET TA-CLOCK TA-DEP RM-M0 RM-M2 RM-M3 RM-M4 GD-COMBAT GD-STANCE TA-PATH GD-ECON GD-POP GD-AGE GD-BUILD}"

python3 - "$LANDED_PREFIXES" <<'PY'
import re, subprocess, sys
from collections import defaultdict

landed = sys.argv[1].split()
# A requirement is declared either inline as `[TA-DET-01]` or, in the
# invariant tables, as a bold cell `**TA-DET-01**`.
_ID = r"(?:TA|GD|UX|RM)-[A-Z0-9]+-\d+"
ID = re.compile(rf"\[({_ID})\]|\*\*({_ID})\*\*")
ANY = re.compile(_ID)
REQ = re.compile(r"REQ:\s*((?:TA|GD|UX|RM)-[A-Z0-9]+-\d+)")

def files(*globs):
    out = subprocess.run(["git", "ls-files", *globs], capture_output=True, text=True)
    return [f for f in out.stdout.split("\n") if f]

# Requirements are *declared* in the specification documents. Every other
# file — the test plan above all — refers to them. Without this distinction a
# requirement quoted in prose reads as a second declaration and the duplicate
# check fires on it, which is exactly what happened the first time.
DECLARING = [
    "docs/02-game-design-spec.md",
    "docs/03-ux-and-feel-spec.md",
    "docs/04-technical-architecture.md",
    "docs/06-roadmap.md",
]

# Requirements, and where each is declared.
declared = {}
for path in DECLARING:
    for n, line in enumerate(open(path), 1):
        for bracketed, bolded in ID.findall(line):
            rid = bracketed or bolded
            declared.setdefault(rid, []).append(f"{path}:{n}")

# Claims, from anywhere in the tree that is source.
claims = defaultdict(list)
for path in files("crates/**/*.rs", "tools/**/*.rs", "scripts/*", "*.yml", ".github/**/*.yml"):
    try:
        text = open(path, errors="replace").read()
    except (IsADirectoryError, PermissionError):
        continue
    for n, line in enumerate(text.split("\n"), 1):
        for rid in REQ.findall(line):
            claims[rid].append(f"{path}:{n}")

# Individual requirements inside a landed prefix that are knowingly not
# covered. A prefix is the wrong granularity for these: withholding all of
# `TA-PATH` to accommodate one blocked requirement would leave the other five
# unwatched, which is the failure this script exists to prevent. Each entry
# needs a reason, and the reason has to be a blocker rather than a shrug.
# Empty since M4 chunk 1 closed TA-PATH-02; the shape is kept so the next
# deferral has somewhere to go.
DEFERRED = {}

def is_landed(rid):
    return any(rid.startswith(p) for p in landed) and rid not in DEFERRED

covered   = sorted(r for r in declared if claims.get(r))
uncovered = sorted(r for r in declared if not claims.get(r))
blocking  = [r for r in uncovered if is_landed(r)]
planned   = [r for r in uncovered if not is_landed(r)]
orphans   = sorted(r for r in claims if r not in declared)

# A non-declaring document — the test plan above all — cites IDs constantly.
# Those citations rot silently as requirements are renamed, so check them too.
stale = defaultdict(list)
for path in files("docs/*.md"):
    if path in DECLARING:
        continue
    for n, line in enumerate(open(path, errors="replace"), 1):
        for rid in ANY.findall(line):
            if rid not in declared:
                stale[rid].append(f"{path}:{n}")
duplicates = sorted(r for r, w in declared.items() if len(w) > 1)

total = len(declared)
print(f"requirements declared : {total}")
print(f"  covered by a test   : {len(covered)}")
print(f"  planned (not landed): {len(planned)}")
print(f"  MISSING (landed)    : {len(blocking)}")
print()

if planned:
    by_area = defaultdict(list)
    for r in planned:
        by_area["-".join(r.split("-")[:2])].append(r)
    print("planned, no test yet (milestone has not landed):")
    for area in sorted(by_area):
        print(f"  {area:<12} {len(by_area[area]):>3}  {' '.join(sorted(by_area[area])[:6])}"
              + (" ..." if len(by_area[area]) > 6 else ""))
    print()

status = 0

deferred_live = {r: why for r, why in DEFERRED.items()
                 if r in declared and any(r.startswith(p) for p in landed)}
if deferred_live:
    print("deferred inside a landed prefix (each needs a blocker, not a shrug):")
    for r, why in sorted(deferred_live.items()):
        covered_now = " — NOW COVERED, remove it from DEFERRED" if claims.get(r) else ""
        print(f"  {r}  {why}{covered_now}")
    print()
    for r in sorted(deferred_live):
        if claims.get(r):
            print(f"FAIL: {r} is listed in DEFERRED but a test now claims it.")
            print("  Remove the entry; a deferral that has been closed hides the next one.")
            status = 1

for r in sorted(set(DEFERRED) - set(declared)):
    print(f"FAIL: DEFERRED names {r}, which no specification declares.")
    print("  Either the ID is a typo or the requirement was deleted.")
    status = 1

if blocking:
    print("FAIL: these requirements belong to landed work and no test claims them:")
    for r in blocking:
        print(f"  {r}  declared at {declared[r][0]}")
    print("\n  Add `REQ: <id>` to the test that covers each, or move the")
    print("  requirement's prefix out of TRACEABILITY_LANDED with a reason.")
    status = 1

if orphans:
    print("FAIL: tests claim requirements that no document declares:")
    for r in orphans:
        print(f"  {r}  claimed at {claims[r][0]}")
    print("\n  Either the ID is a typo, or a requirement was deleted from the")
    print("  specs without the test that covered it.")
    status = 1

if stale:
    print("FAIL: documents cite requirement IDs that no specification declares:")
    for r in sorted(stale):
        print(f"  {r}  cited at {stale[r][0]}")
    print("\n  A requirement was renamed or removed and the prose that refers")
    print("  to it was not. Fix the citation, or restore the requirement.")
    status = 1

if duplicates:
    print("FAIL: these IDs are declared more than once, so coverage is ambiguous:")
    for r in duplicates:
        print(f"  {r}  at {', '.join(declared[r])}")
    status = 1

if status == 0:
    print("ok: every landed requirement has a test, and every test claims a real one")
sys.exit(status)
PY

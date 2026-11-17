"""Export every building colour set the architecture modules declare
(docs/look/buildings.md, "Batch export").

    /usr/bin/python3.13 tools/look/export_all.py [--only NAME ...] [--jobs N]
        [--force] [--samples N] [--out assets/look] [--debug DIR] [--list]

Imports each of greek.py, egyptian.py, mesopotamian.py and asian.py that
exists, reads its `SETS` table and runs export.py on every entry, with its
construction stages and rubble (--stages). A set whose manifest carries the
same export hash is skipped: the hash covers the entry itself, the source of
the module that draws it and of every tools/look module that module imports
(so lib.py too), stages.py, and export.py's EXPORT_VERSION. Each export is
then checked with check_set.py. --jobs runs several Blender processes at
once (each renders on every core, so 2 is usually the most worth having).

A SETS entry (keys; only func and cls are required):

    "barracks_egyptian": dict(
        func="egyptian.barracks",       # MODULE.FUNC in tools/look
        cls="MediumBuilding",           # size class (docs/05 section 2.3)
        tiles=2,                        # the game's footprint (default: the class's)
        kwargs={},                      # keyword arguments for the builder
        fit=None,                       # scale the model to this many tiles
        model_tiles=None,               # the model's own footprint, for fit
        stages=True,                    # construction and rubble (False: idle only)
        anims={},                       # {"arm": [{"arm": 0}, ...]}: more animations
        sun_angle=None, samples=None,   # export.py's --sun-angle, --samples
        fires=None,                     # fire points (default: export.py's, by class)
        timing={},                      # {"idle": [frame_ms, loops]}: other than the defaults
    )

A unit (units.py, docs/look/units.md) is declared the same way with only
func (a units.py figure factory) and cls (Foot, Mounted, Heavy, Ship);
export.py renders it through unitexport.py, resumably.

--points-only rewrites only the attachment points (and the hash) of every
set, stale or not, without rendering: export.py --points-only, which fails
for a set whose model no longer renders as it did (that set needs a full
export, without --points-only). --adopt also takes the geometry fingerprint
of an up-to-date set exported before fingerprints were written.
"""
import argparse
import concurrent.futures
import hashlib
import importlib
import json
import os
import re
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
MODULES = ("greek", "egyptian", "mesopotamian", "asian", "nature", "fx", "fx_combat", "units",
           "units_foot", "units_naval", "units_mounted")
KEYS = {"func", "cls", "tiles", "kwargs", "fit", "model_tiles", "stages", "anims", "sun_angle",
        "samples", "fires", "timing", "scale"}


def load_sets():
    """{set name: (module, entry)} from every architecture module there is."""
    sys.path.insert(0, HERE)
    out = {}
    for mod in MODULES:
        if not os.path.exists(os.path.join(HERE, mod + ".py")):
            continue
        m = importlib.import_module(mod)
        for name, entry in (getattr(m, "SETS", None) or {}).items():
            bad = set(entry) - KEYS
            if bad or "func" not in entry or "cls" not in entry:
                raise SystemExit("%s.SETS[%r]: unknown keys %s or no func/cls"
                                 % (mod, name, sorted(bad)))
            if name in out:
                raise SystemExit("set %r is declared twice" % name)
            out[name] = (mod, entry)
    return out


def local_imports(mod, seen=None):
    """`mod` and every tools/look module it imports, transitively."""
    seen = seen if seen is not None else set()
    if mod in seen:
        return seen
    path = os.path.join(HERE, mod + ".py")
    if not os.path.exists(path):
        return seen
    seen.add(mod)
    text = open(path, encoding="utf-8").read()
    for m in re.findall(r"^\s*(?:from|import)\s+(\w+)", text, re.M):
        local_imports(m, seen)
    return seen


def export_version():
    text = open(os.path.join(HERE, "export.py"), encoding="utf-8").read()
    m = re.search(r"^EXPORT_VERSION\s*=\s*(\d+)", text, re.M)
    return m.group(1) if m else "0"


# A unit set (docs/look/units.md) is rendered by export.py's unit path: its
# hash covers that path's modules too.
UNIT_SOURCES = {"unitexport", "exr"}


def digest(entry):
    mod = entry["func"].rsplit(".", 1)[0]
    h = hashlib.sha256()
    h.update(json.dumps(entry, sort_keys=True).encode())
    h.update(export_version().encode())
    extra = UNIT_SOURCES if mod.startswith("units") else set()
    for m in sorted(local_imports(mod) | {"stages", "lib"} | extra):
        h.update(m.encode())
        with open(os.path.join(HERE, m + ".py"), "rb") as fh:
            h.update(fh.read())
    return h.hexdigest()[:16]


def current_hash(out, name):
    try:
        text = open(os.path.join(out, name, name + ".ron"), encoding="utf-8").read(2000)
    except OSError:
        return None
    m = re.search(r"^// export-hash: (\w+)", text, re.M)
    return m.group(1) if m else None


def command(name, entry, h, args):
    cmd = [sys.executable, os.path.join(HERE, "export.py"), name, entry["func"],
           "--class", entry["cls"], "--kwargs", json.dumps(entry.get("kwargs") or {}),
           "--out", args.out, "--hash", h]
    if entry.get("tiles"):
        cmd += ["--tiles", str(entry["tiles"])]
    if entry.get("fit"):
        cmd += ["--fit", str(entry["fit"])]
    if entry.get("model_tiles"):
        cmd += ["--model-tiles", str(entry["model_tiles"])]
    st = entry.get("stages", True)
    if st:
        cmd += ["--stages"] + (["flat"] if st == "flat" else [])
    if entry.get("anims"):
        cmd += ["--anims", json.dumps(entry["anims"])]
    if entry.get("timing"):
        cmd += ["--timing", json.dumps(entry["timing"])]
    if entry.get("scale"):
        cmd += ["--scale", str(entry["scale"])]
    if entry.get("fires") is not None:
        cmd += ["--fires", str(entry["fires"])]
    if args.points_only:
        cmd += ["--points-only"] + (["--adopt"] if args.adopt else [])
    if entry.get("sun_angle") is not None:
        cmd += ["--sun-angle", str(entry["sun_angle"])]
    samples = args.samples or entry.get("samples")
    if samples:
        cmd += ["--samples", str(samples)]
    if args.debug:
        cmd += ["--debug", args.debug]
    return cmd


def run_one(name, entry, h, args):
    t = time.time()
    p = subprocess.run(command(name, entry, h, args), capture_output=True, text=True, cwd=ROOT)
    if args.points_only:
        keep = [ln for ln in p.stdout.splitlines() if ln.startswith(("points", name))]
        return name, p.returncode == 0, "\n".join(keep) + (
            "" if p.returncode in (0, 3) else p.stdout[-1500:] + p.stderr[-1500:]) + \
            "\n%.0fs" % (time.time() - t)
    keep = [ln for ln in p.stdout.splitlines()
            if ln.startswith(("frame", "trimmed", "  palette", "  **", "wrote", "fit", "ground",
                              "rendered"))]
    if p.returncode:
        return name, False, "\n".join(keep) + "\n" + p.stdout[-1500:] + p.stderr[-1500:]
    chk = [sys.executable, os.path.join(HERE, "check_set.py"), os.path.join(args.out, name)]
    if args.debug:
        chk += ["--out", args.debug]
    c = subprocess.run(chk, capture_output=True, text=True, cwd=ROOT)
    keep.append((c.stdout.strip().splitlines() or [""])[-1] + c.stderr.strip())
    keep.append("%.0fs" % (time.time() - t))
    return name, c.returncode == 0, "\n".join(keep)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--only", nargs="+", help="export only these sets")
    ap.add_argument("--jobs", type=int, default=1)
    ap.add_argument("--force", action="store_true", help="export even what is up to date")
    ap.add_argument("--samples", type=int, help="override every set's samples")
    ap.add_argument("--out", default=os.path.join(ROOT, "assets", "look"))
    ap.add_argument("--debug", help="where export.py and check_set.py put their pictures")
    ap.add_argument("--points-only", action="store_true", help="rewrite every set's "
                    "attachment points and hash without rendering (export.py --points-only)")
    ap.add_argument("--adopt", action="store_true", help="with --points-only: take the "
                    "geometry fingerprint of up-to-date sets exported without one")
    ap.add_argument("--list", action="store_true", help="list the sets and whether each is "
                    "up to date, export nothing")
    args = ap.parse_args()
    sets = load_sets()
    if args.only:
        missing = [n for n in args.only if n not in sets]
        if missing:
            raise SystemExit("no such set: %s" % ", ".join(missing))
    todo = []
    for name in sorted(sets):
        if args.only and name not in args.only:
            continue
        mod, entry = sets[name]
        h = digest(entry)
        fresh = current_hash(args.out, name) == h
        if args.list:
            print("%-32s %-10s %s" % (name, mod, "up to date" if fresh else "stale"))
            continue
        if args.adopt and not fresh:
            raise SystemExit("%s: --adopt takes a fingerprint only from an up-to-date set" % name)
        if fresh and not args.force and not args.points_only:
            print("%s: up to date" % name)
            continue
        todo.append((name, entry, h))
    if args.list or not todo:
        return 0
    print("exporting %d set(s) with %d job(s)" % (len(todo), args.jobs))
    failed = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futs = [pool.submit(run_one, n, e, h, args) for n, e, h in todo]
        for f in concurrent.futures.as_completed(futs):
            name, ok, log = f.result()
            print("== %s %s\n%s" % (name, "ok" if ok else "FAILED", log), flush=True)
            if not ok:
                failed.append(name)
    if failed:
        print("failed: %s" % ", ".join(sorted(failed)))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

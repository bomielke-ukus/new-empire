#!/usr/bin/env bash
# Validates the GitHub Actions workflow files before CI has to.
#
# An invalid workflow does not fail a job — it fails the whole run instantly,
# before any step executes, and GitHub reports it under the file's path rather
# than its name. That is a slow and confusing way to learn about a typo, and
# twice now it has been learned that way.
#
# Uses actionlint when available (it knows the real schema and the context
# rules); otherwise falls back to the checks below, which cover the mistakes
# actually made here.
set -euo pipefail
cd "$(dirname "$0")/.."

status=0
shopt -s nullglob
files=(.github/workflows/*.yml .github/workflows/*.yaml)
if [ ${#files[@]} -eq 0 ]; then
  echo "no workflow files found"
  exit 0
fi

if command -v actionlint >/dev/null 2>&1; then
  echo "== actionlint =="
  actionlint "${files[@]}" || status=1
  exit $status
fi

echo "== workflows: schema and context rules (actionlint not installed) =="
python3 - "${files[@]}" <<'PY'
import sys, yaml, re

# Contexts GitHub allows in each position. A job-level `env:` may not use
# `runner`, `steps`, `job` or `env` — that is a hard validation error, not a
# warning, and it kills the entire run.
JOB_ENV_FORBIDDEN = ("runner.", "steps.", "job.", "env.")
CONTEXT = re.compile(r"\$\{\{([^}]*)\}\}")

status = 0
for path in sys.argv[1:]:
    try:
        doc = yaml.safe_load(open(path))
    except yaml.YAMLError as e:
        print(f"FAIL {path}: not valid YAML: {e}")
        status = 1
        continue

    if not isinstance(doc, dict) or "jobs" not in doc:
        print(f"FAIL {path}: no `jobs` key")
        status = 1
        continue
    # `on:` parses as the boolean True in YAML 1.1.
    if "on" not in doc and True not in doc:
        print(f"FAIL {path}: no `on` trigger")
        status = 1

    names = set(doc["jobs"])
    for job_id, job in doc["jobs"].items():
        if not isinstance(job, dict):
            print(f"FAIL {path}: job `{job_id}` is not a mapping")
            status = 1
            continue
        if "runs-on" not in job and "uses" not in job:
            print(f"FAIL {path}: job `{job_id}` has no `runs-on`")
            status = 1
        for expr in CONTEXT.findall(str(job.get("env") or {})):
            for bad in JOB_ENV_FORBIDDEN:
                if bad in expr:
                    print(
                        f"FAIL {path}: job `{job_id}` uses `{bad.rstrip('.')}` in a "
                        f"job-level `env:`, which GitHub rejects outright "
                        f"(expression: {expr.strip()})"
                    )
                    status = 1
        needs = job.get("needs") or []
        for dep in [needs] if isinstance(needs, str) else needs:
            if dep not in names:
                print(f"FAIL {path}: job `{job_id}` needs `{dep}`, which does not exist")
                status = 1
        for i, step in enumerate(job.get("steps") or []):
            if not isinstance(step, dict):
                print(f"FAIL {path}: job `{job_id}` step {i} is not a mapping")
                status = 1
            elif "uses" not in step and "run" not in step:
                print(f"FAIL {path}: job `{job_id}` step {i} has neither `uses` nor `run`")
                status = 1
    print(f"ok   {path}")

sys.exit(status)
PY

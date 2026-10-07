#!/usr/bin/env python3
"""Measure pre-merge.yml's job durations and flag anything crowding its budget.

Every pre-merge job overrun so far (emu-c6 2026-09-08, heap-budget-chips
2026-10-06) was discovered by a run cancelled at its `timeout-minutes` with
every test passing. The workflow's own policy (the comment above `jobs:` in
`.github/workflows/pre-merge.yml`, "Job timeouts") sizes each budget at
roughly 2-3x the observed max, but nothing re-measures it. This script is
that measurement, meant to run both ad hoc (`just ci-durations`) and on a
weekly schedule (`.github/workflows/ci-durations.yml`).

For each job name, over the window of runs considered: n, p50, p95, max
duration (successful jobs only; duration = completed_at - started_at),
counts of cancelled and failed jobs, total runner-hours over every
conclusion except `skipped` (a skipped job's completed_at is at or before
its started_at — a negative duration, not a real measurement), and the
job's `timeout-minutes` as written in pre-merge.yml. A job is `over` when
its p95 exceeds half its budget, `tight` when its max exceeds 80% of it,
`ok` otherwise, and "too few runs" with fewer than 10 successful runs.

Overall: total runner-hours, the share of those spent on cancelled jobs,
and queue wait (started_at - created_at) p50/p90/p99/max.

Fetches from the GitHub REST API:
  GET /repos/{repo}/actions/workflows/pre-merge.yml/runs
  GET /repos/{repo}/actions/runs/{id}/jobs?per_page=100
using GH_TOKEN or GITHUB_TOKEN when set (the scheduled workflow's own
token), or shelling out to `gh api` otherwise (local use, needs `gh auth
login`). Requests are sequential with a short sleep between them — a
parallel fan-out trips GitHub's secondary rate limit (measured 2026-10-06).
Each run's job list is cached under `--cache-dir` (default
`target/ci-durations/`), keyed by run id, so a re-run is cheap; a run that
was still in progress when fetched is never cached.

Exit code: 0 when nothing is over or tight, 2 when something is (so a
scheduled run goes red), 1 on a fetch or parse error.

Stdlib only; the runner's python has nothing else.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
WORKFLOW_PATH = ROOT / ".github" / "workflows" / "pre-merge.yml"
DEFAULT_REPO = "PhotomancerArt/lightplayer"
SLEEP_S = 0.7
MIN_RUNS_FOR_VERDICT = 10


# ── GitHub fetch ─────────────────────────────────────────────────────────


def repo_slug(args) -> str:
    return args.repo or os.environ.get("GITHUB_REPOSITORY") or DEFAULT_REPO


def gh_token() -> str | None:
    return os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")


def api_get(path: str, token: str | None) -> dict:
    """One GET against the GitHub REST API: direct with a token, `gh api` without one."""
    if token:
        req = urllib.request.Request(
            f"https://api.github.com{path}",
            headers={
                "Authorization": f"Bearer {token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
                "User-Agent": "lp2025-job-durations",
            },
        )
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as e:
            body = e.read().decode("utf-8", errors="replace")
            raise RuntimeError(f"GET {path} -> HTTP {e.code}: {body[:500]}") from e
        except urllib.error.URLError as e:
            raise RuntimeError(f"GET {path} -> {e}") from e
    out = subprocess.run(["gh", "api", path], capture_output=True, text=True)
    if out.returncode != 0:
        raise RuntimeError(f"gh api {path} failed: {out.stderr.strip()}")
    return json.loads(out.stdout)


def fetch_runs(repo: str, token: str | None, runs_n: int | None, since: str | None) -> list[dict]:
    """Workflow runs for pre-merge.yml, newest first, sequential pages."""
    runs: list[dict] = []
    page = 1
    query = f"created={urllib.parse.quote('>=' + since)}" if since else None
    while True:
        qs = f"per_page=100&page={page}"
        if query:
            qs += f"&{query}"
        data = api_get(f"/repos/{repo}/actions/workflows/pre-merge.yml/runs?{qs}", token)
        batch = data.get("workflow_runs", [])
        runs.extend(batch)
        if not batch or len(batch) < 100:
            break
        if runs_n is not None and len(runs) >= runs_n:
            break
        page += 1
        time.sleep(SLEEP_S)
    if runs_n is not None:
        runs = runs[:runs_n]
    return runs


def fetch_jobs_for_run(
    run: dict, repo: str, token: str | None, cache_dir: Path
) -> tuple[list[dict], bool]:
    """Returns (jobs, made_a_request) — the caller only needs to sleep after
    a real request, never after a cache hit, or a re-run pays the
    sequential sleep for every run regardless of whether it fetched
    anything."""
    run_id = run["id"]
    cache_file = cache_dir / f"{run_id}.json"
    if cache_file.is_file():
        return json.loads(cache_file.read_text())["jobs"], False
    data = api_get(f"/repos/{repo}/actions/runs/{run_id}/jobs?per_page=100", token)
    jobs = data.get("jobs", [])
    # per_page=100 covers every run observed so far (~20 jobs); paginate
    # defensively if that ever changes.
    page = 2
    while len(jobs) < data.get("total_count", len(jobs)):
        time.sleep(SLEEP_S)
        more = api_get(f"/repos/{repo}/actions/runs/{run_id}/jobs?per_page=100&page={page}", token)
        batch = more.get("jobs", [])
        if not batch:
            break
        jobs.extend(batch)
        page += 1
    if run.get("status") == "completed":
        cache_dir.mkdir(parents=True, exist_ok=True)
        cache_file.write_text(json.dumps({"run_id": run_id, "jobs": jobs}))
    return jobs, True


# ── timeout-minutes, parsed from the workflow's own text ───────────────────


JOB_KEY = re.compile(r"^  ([A-Za-z0-9_-]+):\s*(#.*)?$")
JOB_NAME = re.compile(r'^    name:\s*(.+?)\s*(#.*)?$')
JOB_TIMEOUT = re.compile(r"^    timeout-minutes:\s*(\d+)\s*(#.*)?$")


def parse_budgets(text: str) -> dict[str, int]:
    """job id -> timeout-minutes, by scanning `jobs:`-level blocks.

    Job ids sit two spaces under the top-level `jobs:`; a job's own `name:`
    and `timeout-minutes:` sit four spaces in, directly under it — never
    more (a step's `name:`/`timeout-minutes:` is six or eight spaces in).
    A job whose block has no four-space `timeout-minutes:` line is left out
    of the map, and the caller reports that job as having no budget found.
    """
    lines = text.splitlines()
    budgets: dict[str, int] = {}
    cur_id: str | None = None
    block: list[str] = []

    def flush():
        if cur_id is None:
            return
        timeout = None
        for line in block:
            m = JOB_TIMEOUT.match(line)
            if m:
                timeout = int(m.group(1))
                break
        if timeout is not None:
            budgets[cur_id] = timeout

    for line in lines:
        m = JOB_KEY.match(line)
        if m:
            flush()
            cur_id = m.group(1)
            block = []
            continue
        if cur_id is not None:
            block.append(line)
    flush()
    return budgets


def parse_names(text: str) -> dict[str, str]:
    """job id -> its `name:` field, by the same block scan as parse_budgets."""
    lines = text.splitlines()
    names: dict[str, str] = {}
    cur_id: str | None = None
    block: list[str] = []

    def flush():
        if cur_id is None:
            return
        for line in block:
            m = JOB_NAME.match(line)
            if m:
                names[cur_id] = m.group(1)
                return

    for line in lines:
        m = JOB_KEY.match(line)
        if m:
            flush()
            cur_id = m.group(1)
            block = []
            continue
        if cur_id is not None:
            block.append(line)
    flush()
    return names


# ── stats ────────────────────────────────────────────────────────────────


def percentile(sorted_vals: list[float], p: float) -> float:
    """Linear-interpolation percentile, p in [0, 100]. `sorted_vals` must be sorted."""
    if not sorted_vals:
        return 0.0
    if len(sorted_vals) == 1:
        return sorted_vals[0]
    k = (len(sorted_vals) - 1) * (p / 100)
    f = int(k)
    c = min(f + 1, len(sorted_vals) - 1)
    if f == c:
        return sorted_vals[f]
    return sorted_vals[f] * (c - k) + sorted_vals[c] * (k - f)


def parse_ts(s: str | None) -> float | None:
    """An ISO-8601 `Z` timestamp -> unix seconds, or None for a null/missing field."""
    if not s:
        return None
    import datetime

    return datetime.datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ").replace(
        tzinfo=datetime.timezone.utc
    ).timestamp()


class JobStats:
    def __init__(self, name: str):
        self.name = name
        self.success_durations_min: list[float] = []
        self.cancelled = 0
        self.failed = 0
        self.runner_hours = 0.0
        self.cancelled_hours = 0.0

    def add(self, conclusion: str, duration_s: float):
        hours = duration_s / 3600
        self.runner_hours += hours
        if conclusion == "success":
            self.success_durations_min.append(duration_s / 60)
        elif conclusion == "cancelled":
            self.cancelled += 1
            self.cancelled_hours += hours
        elif conclusion == "failure":
            self.failed += 1

    def verdict(self, budget_min: int | None) -> str:
        n = len(self.success_durations_min)
        if n < MIN_RUNS_FOR_VERDICT:
            return "too few runs"
        if budget_min is None:
            return "no budget found"
        vals = sorted(self.success_durations_min)
        p95 = percentile(vals, 95)
        mx = vals[-1]
        if p95 > 0.5 * budget_min:
            return "over"
        if mx > 0.8 * budget_min:
            return "tight"
        return "ok"

    def row(self, budget: int | None) -> dict:
        n = len(self.success_durations_min)
        vals = sorted(self.success_durations_min)
        return {
            "name": self.name,
            "n": n,
            "p50_min": round(percentile(vals, 50), 1) if n else None,
            "p95_min": round(percentile(vals, 95), 1) if n else None,
            "max_min": round(vals[-1], 1) if n else None,
            "cancelled": self.cancelled,
            "failed": self.failed,
            "runner_hours": round(self.runner_hours, 2),
            "timeout_minutes": budget,
            "verdict": self.verdict(budget),
        }


def summarize(all_jobs: list[dict], budgets: dict[str, int], names: dict[str, str]) -> dict:
    """all_jobs: the raw per-job dicts from every run's jobs API response."""
    by_name: dict[str, JobStats] = {}
    queue_waits_min: list[float] = []
    total_hours = 0.0
    cancelled_hours = 0.0

    # `names` maps job id -> name; invert it so a job's own `name` field
    # (what the API returns, and what `--runs`/`--since` group by) can be
    # matched back to a budget even though the workflow keys budgets by job
    # id, not name.
    budget_by_name = {names[jid]: mins for jid, mins in budgets.items() if jid in names}

    for job in all_jobs:
        conclusion = job.get("conclusion")
        if conclusion == "skipped":
            continue
        started = parse_ts(job.get("started_at"))
        completed = parse_ts(job.get("completed_at"))
        created = parse_ts(job.get("created_at"))
        name = job.get("name", "<unnamed>")
        if started is not None and created is not None and started >= created:
            queue_waits_min.append((started - created) / 60)
        if started is None or completed is None or completed < started:
            continue
        duration_s = completed - started
        stats = by_name.setdefault(name, JobStats(name))
        stats.add(conclusion, duration_s)
        total_hours += duration_s / 3600
        if conclusion == "cancelled":
            cancelled_hours += duration_s / 3600

    rows = [
        stats.row(budget_by_name.get(name))
        for name, stats in sorted(by_name.items(), key=lambda kv: kv[0])
    ]
    qw = sorted(queue_waits_min)
    overall = {
        "total_runner_hours": round(total_hours, 1),
        "cancelled_share": round(cancelled_hours / total_hours, 4) if total_hours else 0.0,
        "queue_wait_p50_min": round(percentile(qw, 50), 2) if qw else None,
        "queue_wait_p90_min": round(percentile(qw, 90), 2) if qw else None,
        "queue_wait_p99_min": round(percentile(qw, 99), 2) if qw else None,
        "queue_wait_max_min": round(qw[-1], 2) if qw else None,
    }
    return {"jobs": rows, "overall": overall}


# ── output ───────────────────────────────────────────────────────────────


def render_markdown(result: dict, window_desc: str) -> str:
    lines = [f"## Pre-merge job durations ({window_desc})", ""]
    lines.append(
        "| job | n | p50 | p95 | max | cancelled | failed | runner-h | budget | verdict |"
    )
    lines.append("|---|---|---|---|---|---|---|---|---|---|")
    for r in result["jobs"]:
        p50 = f"{r['p50_min']:.1f}" if r["p50_min"] is not None else "-"
        p95 = f"{r['p95_min']:.1f}" if r["p95_min"] is not None else "-"
        mx = f"{r['max_min']:.1f}" if r["max_min"] is not None else "-"
        budget = f"{r['timeout_minutes']}" if r["timeout_minutes"] is not None else "none found"
        lines.append(
            f"| {r['name']} | {r['n']} | {p50} | {p95} | {mx} | {r['cancelled']} | "
            f"{r['failed']} | {r['runner_hours']:.2f} | {budget} | **{r['verdict']}** |"
        )
    o = result["overall"]
    lines += [
        "",
        f"Total runner-hours: **{o['total_runner_hours']}**, "
        f"{o['cancelled_share'] * 100:.1f}% of it on cancelled jobs.",
        "",
        f"Queue wait (minutes): p50 {o['queue_wait_p50_min']}, "
        f"p90 {o['queue_wait_p90_min']}, p99 {o['queue_wait_p99_min']}, "
        f"max {o['queue_wait_max_min']}.",
    ]
    return "\n".join(lines) + "\n"


# ── self-test ────────────────────────────────────────────────────────────

FIXTURE_WORKFLOW = """\
jobs:
  foo:
    name: Foo job
    runs-on: ubuntu-24.04
    timeout-minutes: 20
    steps:
      - name: not a job name
        timeout-minutes: 5
        run: echo hi
  bar:
    name: Bar job
    runs-on: ubuntu-24.04
    timeout-minutes: 10
"""


def fixture_jobs() -> list[dict]:
    """Two fake runs' worth of job records, by hand, spanning every conclusion."""
    import datetime

    epoch = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)

    def at(offset_s: float) -> str:
        return (epoch + datetime.timedelta(seconds=offset_s)).strftime("%Y-%m-%dT%H:%M:%SZ")

    def job(name, conclusion, created_s, started_s, completed_s):
        return {
            "name": name,
            "conclusion": conclusion,
            "created_at": at(created_s),
            "started_at": at(started_s),
            "completed_at": at(completed_s),
        }

    jobs = []
    # "Foo job": ten successful runs at 4, 6, 8, ..., 22 minutes (budget 20).
    # p50 = 13, p95 = 21.1, max = 22 -> p95 (21.1) > 10 (half of 20) -> "over".
    for minutes in [4, 6, 8, 10, 12, 14, 16, 18, 20, 22]:
        jobs.append(job("Foo job", "success", 0, 0, minutes * 60))
    # Plus two cancelled and one failed "Foo job" run, to exercise the counts.
    jobs.append(job("Foo job", "cancelled", 0, 0, 20 * 60))
    jobs.append(job("Foo job", "cancelled", 0, 0, 20 * 60))
    jobs.append(job("Foo job", "failure", 0, 0, 5 * 60))
    # "Bar job": five successful runs, all under 1 minute of its 10-minute
    # budget -> fewer than MIN_RUNS_FOR_VERDICT successes -> "too few runs".
    for minutes in [1, 2, 1, 2, 1]:
        jobs.append(job("Bar job", "success", 0, 0, minutes * 60))
    # A skipped job: completed_at == started_at, must be excluded entirely.
    jobs.append(job("Bar job", "skipped", 0, 9, 9))
    # Queue wait fixtures on a third, unbudgeted job name: waits of 30 s,
    # 60 s and 600 s, each a 60 s run.
    for wait_s in [30, 60, 600]:
        jobs.append(job("Baz job", "success", 0, wait_s, wait_s + 60))
    # "Qux job": ten successful runs, but it has no job id in the fixture
    # workflow at all (name has no `timeout-minutes:` to find) -> "no budget
    # found" even with plenty of runs.
    for minutes in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]:
        jobs.append(job("Qux job", "success", 0, 0, minutes * 60))
    return jobs


def self_test() -> int:
    failures = []

    def check(label, cond):
        if not cond:
            failures.append(label)

    budgets = parse_budgets(FIXTURE_WORKFLOW)
    names = parse_names(FIXTURE_WORKFLOW)
    check("budgets parsed", budgets == {"foo": 20, "bar": 10})
    check("names parsed", names == {"foo": "Foo job", "bar": "Bar job"})

    vals = [float(x) for x in range(1, 11)]  # 1..10
    check("percentile p50", percentile(vals, 50) == 5.5)
    check("percentile p100 is max", percentile(vals, 100) == 10.0)
    check("percentile p0 is min", percentile(vals, 0) == 1.0)

    result = summarize(fixture_jobs(), budgets, names)
    rows = {r["name"]: r for r in result["jobs"]}

    foo = rows.get("Foo job")
    check("foo present", foo is not None)
    if foo:
        check("foo n == 10", foo["n"] == 10)
        check("foo p50 == 13", foo["p50_min"] == 13.0)
        check(f"foo p95 == 21.1, got {foo['p95_min']}", abs(foo["p95_min"] - 21.1) < 0.05)
        check("foo max == 22", foo["max_min"] == 22.0)
        check("foo cancelled == 2", foo["cancelled"] == 2)
        check("foo failed == 1", foo["failed"] == 1)
        check(f"foo verdict == over, got {foo['verdict']}", foo["verdict"] == "over")

    bar = rows.get("Bar job")
    check("bar present", bar is not None)
    if bar:
        check("bar n == 5 (skipped excluded)", bar["n"] == 5)
        check(
            f"bar verdict == too few runs, got {bar['verdict']}",
            bar["verdict"] == "too few runs",
        )

    baz = rows.get("Baz job")
    check("baz present", baz is not None)
    if baz:
        check(f"baz verdict == too few runs, got {baz['verdict']}", baz["verdict"] == "too few runs")

    qux = rows.get("Qux job")
    check("qux present", qux is not None)
    if qux:
        check("qux n == 10", qux["n"] == 10)
        check("qux has no budget found", qux["timeout_minutes"] is None)
        check(
            f"qux verdict == no budget found, got {qux['verdict']}",
            qux["verdict"] == "no budget found",
        )

    overall = result["overall"]
    expected_hours = (
        sum([4, 6, 8, 10, 12, 14, 16, 18, 20, 22])
        + 20 + 20 + 5
        + 1 + 2 + 1 + 2 + 1
        + 3
        + sum([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
    ) / 60
    check(
        f"total runner-hours ~= {expected_hours:.2f}, got {overall['total_runner_hours']}",
        abs(overall["total_runner_hours"] - expected_hours) < 0.06,
    )
    check("queue wait p50 present", overall["queue_wait_p50_min"] is not None)

    if failures:
        for f in failures:
            print(f"SELF-TEST FAILED: {f}", file=sys.stderr)
        return 1
    print("self-test: ok")
    return 0


# ── main ─────────────────────────────────────────────────────────────────


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--runs", type=int, default=200, help="last N pre-merge.yml runs (default 200)")
    ap.add_argument("--since", help="ISO date; use every run created on or after it, instead of --runs")
    ap.add_argument(
        "--cache-dir",
        default=str(ROOT / "target" / "ci-durations"),
        help="per-run job-list cache (default target/ci-durations/)",
    )
    ap.add_argument("--json", help="also write the full result as JSON to this path")
    ap.add_argument("--repo", help=f"owner/repo (default {DEFAULT_REPO}, or $GITHUB_REPOSITORY)")
    ap.add_argument("--self-test", action="store_true", help="run the offline fixture-based self-test")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    try:
        text = WORKFLOW_PATH.read_text()
    except OSError as e:
        print(f"error: could not read {WORKFLOW_PATH}: {e}", file=sys.stderr)
        return 1
    budgets = parse_budgets(text)
    names = parse_names(text)

    repo = repo_slug(args)
    token = gh_token()
    cache_dir = Path(args.cache_dir)
    runs_n = None if args.since else args.runs

    try:
        runs = fetch_runs(repo, token, runs_n, args.since)
    except RuntimeError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    if not runs:
        print("error: no pre-merge.yml runs found for this window", file=sys.stderr)
        return 1

    all_jobs: list[dict] = []
    for run in runs:
        try:
            jobs, made_request = fetch_jobs_for_run(run, repo, token, cache_dir)
        except RuntimeError as e:
            print(f"error: {e}", file=sys.stderr)
            return 1
        all_jobs.extend(jobs)
        if made_request:
            time.sleep(SLEEP_S)

    result = summarize(all_jobs, budgets, names)

    window_desc = f"since {args.since}" if args.since else f"last {len(runs)} runs"
    markdown = render_markdown(result, window_desc)
    print(markdown)

    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with open(summary_path, "a", encoding="utf-8") as f:
            f.write(markdown)

    if args.json:
        Path(args.json).write_text(json.dumps(result, indent=2))

    any_flagged = any(r["verdict"] in ("over", "tight") for r in result["jobs"])
    return 2 if any_flagged else 0


if __name__ == "__main__":
    sys.exit(main())

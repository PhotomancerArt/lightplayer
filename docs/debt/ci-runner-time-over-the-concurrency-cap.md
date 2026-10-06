---
status: carried
since: 2026-09-08      # emu-c6 first outgrew its budget by accretion
logged: 2026-10-06
area: CI / pre-merge.yml runner time
related:
  - actions-cache-budget.md
  - story-capture-pipeline.md
  - ../../.github/workflows/pre-merge.yml
---
# CI's runner time is bigger than the org's 20 concurrent jobs

**Shape** — the org is on GitHub's free plan: Linux minutes on this public
repo cost nothing, but at most **20 jobs run at once** across every
workflow. A pre-merge run fans out to ~17 jobs, several agent sessions push
at once, and every merge to main runs all of them (main forces every gate).
So runner time is not a bill — it is queue. A minute a job spends is a
minute some other PR's job waits for a runner.

Two things keep the total growing, and both are structural:

- **Jobs grow by accretion and get cut at their budgets.** A test is added
  to whichever recipe its job already runs; nobody re-measures the job until
  it is cancelled at `timeout-minutes` with every test passing. `emu-c6`
  went that way on 2026-09-08 and `heap-budget-chips` on 2026-10-06. The
  workflow's policy (the comment above `jobs:`) is a split, not a bigger
  number.
- **Every emulator job pays a fat-LTO release build of a `-p lp-cli` test
  tree** — about 5–6 minutes on a runner even at sccache's 95 % hit rate,
  because `[profile.release]` is `lto = true`, `codegen-units = 1`, and LTO
  happens at link time, which sccache does not cache. Every extra test
  binary costs another 20–45 s single-threaded link. Five jobs pay it:
  `heap-budget-chips`, `emu-c6-cli`, `emu-esp32v3`, `emu-esp32s3`,
  `emu-c6-layout-migration`.

**Carrying cost** — measured over 399 pre-merge runs, 2026-10-03 → 10-06
(≈ 3.4 days): **673 runner-hours**, about 9 runners busy around the clock.
Of the jobs that ran, half waited 6 s or less for a runner, but one in ten
waited 8 min or more and one in a hundred waited 21 min (max 31 min). The
biggest jobs by runner-hours: Validate (x64) 104 h (p50 20 min), story
baselines 84 h (p50 22 min), heap-budget-chips 81 h (p50 21 min before the
split), emu-esp32v3 68 h, emu-esp32s3 61 h, emu-c6 59 h. Runs cancelled by a
newer push to the same PR were ~14 % of runner time (60 h of the first
420 h measured).

**Workarounds** — measure, don't guess. Per-job p50/p95/max from recent
runs (sequential calls: a parallel loop trips GitHub's secondary rate
limit):

```bash
gh run list --workflow pre-merge.yml -L 200 --json databaseId > runs.json
for id in $(jq -r '.[].databaseId' runs.json); do
  gh api "repos/PhotomancerArt/lightplayer/actions/runs/$id/jobs?per_page=100" > "jobs/$id.json"; sleep 0.7
done
# then: per job name, (completed_at - started_at) over conclusion == success;
# queue wait is started_at - created_at.
```

Inside a job, most of the time is one monolithic step, so read the log:
`gh api repos/PhotomancerArt/lightplayer/actions/jobs/<job id>/logs`, then
look for cargo's `Finished … in` lines (build) and libtest's `test result:
… finished in` lines (run).

**Incident log**

- **2026-09-08** — `emu-c6` cut at its 20-minute budget on every run since
  #599; it had grown a second full `-p lp-cli` test-tree build. Split:
  the lp-cli tests and the chip ratchet moved to `heap-budget-chips`.
- **2026-10-06** — `heap-budget-chips` cancelled at 30 min with every test
  passing (runs 37469170395, 37473996154 on #993): ~25 min of
  `test-emu-c6-cli` plus the ratchet, after the seams, OTA and Wi-Fi tests
  stacked up. #993 raised it to 45 as a stopgap. #997 split it: the link
  half stays beside the ratchet (measured 9.7 / 10.4 min), the boards half
  is the new `emu-c6-cli` job (14.7 min). It also dropped a dev `-p lp-cli`
  build of two parity tests that Validate (x64) already runs (~4 min), and
  made each half one cargo invocation so its LTO links run side by side.
  Measured and rejected: running these tests in the dev profile (emulator
  crates are opt-level 3 there) — on an M2 Max `emu_usb_link_gates` ran
  68 s in dev against 23 s in release, so the build saving goes back out in
  run time.

**Exit criteria** — every job's p95 is under half its `timeout-minutes`;
queue p90 for a pre-merge job is under 2 minutes; and adding a test to an
emulator job is a decision someone measures, not an accretion.

**Paydown candidates, unmeasured** (each its own experiment, with before
and after numbers from this entry's method):

- A host test profile for the emulator suites (`inherits = "release"`,
  `lto = "thin"` or off, more codegen units), so the five jobs above stop
  paying a fat-LTO link per binary. Risk: emulator speed without
  cross-crate inlining (the generic-entry-point note in `Cargo.toml`), and
  the scripts that look in `target/release/`. Firmware profiles must keep
  their LTO — `release-esp32*` inherit from `release`, so this cannot be an
  env override of `release` itself.
- The story build (`dx build --release`) spends ~4 min compiling the one
  `lpa-studio-web` crate at fat LTO. A non-LTO story profile should render
  the same pixels (no fast-math in Rust), and the PR's own story job would
  prove it by reporting zero changed stories — but that job's environment is
  deliberately pinned, so it is the stories owner's call.
- `cargo nextest` for Validate (x64)'s ~5.6 min of serial test-binary
  execution, once tests that share process state are known.
- Fewer pushes per PR from agent sessions (each push cancels a run that was
  already paid for), or a merge queue so main runs once per batch.

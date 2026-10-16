---
status: paying-down
since: 2026-09-13      # DD77 kept lp-core/** out of the emulator filters
logged: 2026-10-15
area: CI / pre-merge.yml path filters (the chip figure jobs)
related:
  - ../chip-figures.md
  - ci-runner-time-over-the-concurrency-cap.md
  - two-green-prs-can-red-main.md
  - ../../.github/workflows/pre-merge.yml
  - ../../scripts/ci/firmware-closure.py
---
# A shared-crate PR skips the chip figure jobs, so its figure moves land red on main

**Shape** — the chip emulator suites and heap ratchets pin numbers read off
each chip's **shipped image** ([chip-figures.md](../chip-figures.md)): the
main stack, the boot chain, the heap ledger. Any crate linked into that image
can move them; a few bytes of statics in `lpa-server` moves the S3's and the
classic's stack. But the jobs that hold them were gated on the emulator
filters (`emu_c6`, `emu_esp32v3`, `emu_esp32s3`), which list the firmware's
own directory and `fw-esp32-common` and stop short of `lp-core/**`,
`lp-base/**`, `lp-shader/**` and `lp-app/lpa-server/**` — director ruling
DD77 (2026-09-13, `lp2025/2026-09-10-0021-xtensa-emulator/director-log.md`):
"adding them would put the 26-min classic job on every core PR … main's full
post-merge run is the backstop". So a PR to a shared crate ran none of those
jobs (or only the C6's, when it also touched `fw-esp32c6`), went green, and
main — which forces every gate on — found the move.

It is structural rather than one bad filter: every hand-written list of
"what links into the firmware" drifts from the real dependency graph, and a
figure is exact by design, so any byte the list misses is a red main.

**Carrying cost** — each incident is a red main for every PR that merges or
rebases until someone re-blesses (`just apply-ci-figures` cannot help: the
patch is on main's run, not a PR's), a re-bless PR with its own full CI run,
and a second full main run. Since 2026-10-14 AGENTS.md also asks the merging
agent to run the skipped jobs on the desk (`just test-emu-c6`,
`just heap-budget-check-chips`) or hand the merge over; #1088 ran the two
Xtensa heap ratchets and not the boot suites, and the boot suites are where
its move was.

**Workarounds** — before merging a PR that touches a crate in a firmware's
closure (`.github/firmware-closure-filters.yml` lists them), check that
`Emulator ESP32v3 (x64)`, `Emulator ESP32-S3 (x64)`, `Emulator C6 (x64)` and
`Heap budget (esp32c6 chip)` actually ran. If main goes red on figures only:
the main run's `figures-patch-*` artifacts hold the patch
(`gh run download <run> -p 'figures-patch-*'`, then `git apply`), or
`just bless-chips <chip>` on the desk; one re-bless PR, merged on green.

**Incident log**

- **2026-10-09** — #1049 (`lp-link`'s inbox, a fragmented-heap fix) merged
  as `2ee9a5e3f` with the emulator and heap jobs path-skipped; main went red
  on all three chips' figures. #1075 (`587bb7169`) re-recorded them ~8 h
  later. The auto-queue filed the decision as
  `_auto/00-yours/2026-10-09-lp-link-runs-firmware-gates` (add
  `lp-base/lp-link/**` to the filters, lean yes) and made its own merges
  refuse such a PR alone; nothing changed the filters.
- **2026-10-14** (10-15 03:34 UTC) — #1088 (the C6's per-read cost margin,
  code in `lpa-server` and `lpc-engine`, a `fw-esp32c6` call site) merged as
  `eee7d33e3` with `Emulator ESP32-S3 (x64)`, `Emulator ESP32v3 (x64)` and
  `Emulator ESP32v3 reference (x64)` skipped (its comment cited DD77; the
  behaviour change is C6-only, but the statics are not). Main's pre-merge
  run 38677009764 failed both Xtensa boot suites: the S3's
  `stack_total_bytes` and the classic's main-stack figures, −16 B. #1142
  (`8a1b81e7e`) re-blessed them 51 min later.

**Paying down (2026-10-15)** — `scripts/ci/firmware-closure.py` asks
`cargo tree` for each chip firmware's normal and build dependencies (default
features, the chip's real target) and writes one `<dir>/**` glob per
in-repo crate to `.github/firmware-closure-filters.yml`; `just
lint-firmware-closure` (in `check-lint`) fails when the file is stale, so a
new path dependency carries its filter. `detect-changes` reads the file in a
second paths-filter step and ORs it into `figures_esp32c6/v3/s3`, which
gate the four figure-holding jobs: `emu-c6`, `heap-budget-chips`,
`emu-esp32v3` and `emu-esp32s3`. `emu-esp32v3-ref`, `emu-c6-cli` and the
layout migration hold no figure of the image and stay on DD77's filters.

Why this does not simply overrule DD77: its cost premise moved. In
September the classic job was one 26-minute job and the longest on the
critical path; it is now split, `emu-esp32v3` is ~20 min and the S3 ~16,
and every PR that newly fires them also runs `Validate (x64)` (~25 min), so
a PR's wall clock does not grow. What it costs is queue time under the
org's 20-job cap. Measured on 335 merged PRs, 2026-09-22 → 10-15: 41 touch
a firmware closure without firing their chip's emulator filter (41 for the
classic and the S3, 31 for the C6), across 114 non-cancelled pre-merge runs
(86 for the C6's). At this run's job times (v3 20, S3 16.3, emu-c6 24.5,
heap 16.4 min) that is ~7,650 runner-minutes over 23 days, **~5.5
runner-hours a day** (≤ 8.3 counting cancelled runs at full length) against
the ~200 a day the [concurrency-cap entry](ci-runner-time-over-the-concurrency-cap.md)
measured: +3–4 %. A figure-only trim of the two Xtensa jobs was measured and
not built: their time is the fat-LTO `lp-cli` test build (~6 min) and the
heap ratchet (~5 min), both of which carry figures, so dropping the frame
images and tests would save 1–3 min a run. And main's backstop is no longer
free: AGENTS.md (2026-10-14) does not accept a path-skipped figure job for a
firmware-linked merge.

**Exit criteria** — retire when a full month passes with no red main from a
figure a skipped job would have caught, and `just ci-durations` shows queue
p90 no worse than the month before. Revisit the C6 half first if queue time
rises: it is the larger cost (`emu-c6` is 24 min) and its shared-crate
figures (`hello.proto`, `heartbeat.total_bytes`, the ledger) move less often
than the Xtensa stacks.

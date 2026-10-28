---
status: carried
since: 2026-09-08
logged: 2026-10-06
area: .github/workflows/pre-merge.yml (the emulator-backed jobs: Heap budget (esp32c6 chip), Emulator C6)
related:
  - .github/workflows/pre-merge.yml (the "Job timeouts" comment above `jobs:`)
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR #993)
  - chip task_acfe5e83 "Split the C6 chip heap CI job"
---
# The emulator CI jobs outgrow their time budgets as image-backed tests accumulate

**Shape** — each emulator-backed lp-cli test boots a real firmware image and
many build their own (a split image is two link passes). They are added to a
small number of recipes (`test-emu-c6-cli`, `test-emu-c6-boot`,
`test-emu-serve`), and each recipe is one job with one `timeout-minutes`. Every
feature that touches the device adds a test to the same job, so the job's time
grows with the product until a healthy run hits the budget and is cancelled
with every test passing. The workflow's policy is to re-measure and **split**,
not to bump (`pre-merge.yml`, the comment above `jobs:`), but the trigger is
always a red PR that has nothing to do with CI.

**Carrying cost** — a cancelled job reads as red CI on a PR whose tests all
passed; diagnosing it means reading step timestamps out of a 3,000-line log.
Stacked PRs feel it first, because they carry several features' tests at once.

**Workarounds** —
- A job cancelled at its budget with no failing test is this entry, not a flake:
  compare the step timestamps (`gh api repos/<repo>/actions/jobs/<id>/logs`)
  against the budget comment above the job.
- Move tests that don't need release builds to a dev-profile recipe (the
  emulator crates build at opt-level 3 in dev): PR #993 moved its two LAN cells
  from `test-emu-c6-cli` (release) to `test-emu-serve`.
- Raise the budget only as headroom, with the measurement in the comment, and
  chip the split.

**Incident log**
- 2026-09-08 — the emu-c6 overrun: the job had grown two whole builds; split
  into `heap-budget-chips` (recorded in the workflow comment, before this entry).
- 2026-10-06 — PR #993 (stacked on #987, the seams foundation, and #989, PR B,
  over #986's OTA): `Heap budget (esp32c6 chip)` was cancelled at its 30-minute
  budget twice with every test passing (runs 37469170395, 37473996154).
  `test-emu-c6-cli` measured **25 min** on the runner, plus ~8 min of ratchet.
  The stacked causes: #987's `emu_seam_led` (a split build, ~100 s of run),
  #986's OTA tests, #989's Wi-Fi settings tests, and #993's two LAN cells (moved
  out to `test-emu-serve`). The budget went to 45 as headroom; the split is chip
  task_acfe5e83.
- 2026-10-06 — paid down for this job by #997 (`claude/split-heap-budget-chips`,
  merged the same day): `test-emu-c6-cli` is now three recipes
  (`-link`, `-boards`, `-agent`) across two jobs, measured 7.8–10.4 min, with
  a 35-minute budget and the comment "growth here wants another split, not a
  bigger number". #993's 45-minute stopgap was dropped in its merge of main.
  The condition (device tests piling into one-job recipes) stays carried.
- 2026-10-21 — PR #1238 (S3 radio M1, run 39620696788): `Emulator ESP32-S3`
  and `Emulator C6` were both cancelled at their 35-minute budget with every
  test step passed — the S3 job in its heap-ratchet step, the C6's in
  "Upload CI images". Main runs them in 18–23 and 21–26 min. The S3's cause was
  this PR's own: `test-emu-esp32s3-boot` gained a second firmware build (the
  radio probe), the radio boots (~85 s) and a frame-parity gate that rendered
  6 emulated seconds on each of two images (~9 min). The gate now stops at its
  ten lit frames (45 s locally). The C6 job's test step took 32 min against
  main's ~20, with no C6 test added — cold caches for the crates the PR
  touched (`lp-cli`, `lp-emu-*`); a rerun is the evidence either way.
- 2026-10-21 — PR #1249 (S3 radio M5, run 39694340977): `Emulator ESP32-S3`
  cancelled at 35 minutes in its heap-ratchet step, every earlier step
  passed (the gate 22 min, P4's split boot 5 min). Main already ran this job
  in 28–30.5 min after P4 (#1248) added the split-image boot; this PR's
  seam harness (one more firmware build and four tests) tipped it over.
  Split, not bumped: `just test-emu-esp32s3-seams` and its own job. The
  re-run without the harness was cancelled the same way (gate 22 min, split
  boot 5, the ratchet at 6 when the budget hit; cold caches for the crates
  the PR touched, as #1238 saw), so the S3's heap ratchet moved into that
  job too: `Emulator ESP32-S3 seams and heap (x64)`, 30 minutes. The S3
  gate job is now the gate and the split boot, ~27 min cold — the next S3
  device test wants a split of the gate itself.
- 2026-10-22 — PR #1264 (M10, PSRAM), run 39882963516: `Emulator ESP32-S3
  (x64)` cancelled at its 35-minute budget in "Upload CI images", after its
  gate (26 min) and split boot (6 min) had both passed. The PR had added a
  shipped-image comparison (four boots, two ROM-up) to `psram_boot.rs`,
  which the gate runs; main's runs of the job that evening took 25–31 min.
  Moved the comparison to the PSRAM recipe in the S3 seams job (22 min of
  30, its PSRAM step 10 min), the split this entry prescribes.
- 2026-10-28 — PR #1319 (the mapping rebuild's integration PR), run
  40650335671 at 832ffa67c: `Emulator ESP32-S3 seams and heap (x64)` cancelled
  at its 30-minute budget in "Heap budget ratchet — the S3's own ledger",
  5 min 33 s into the S3 boot, with every earlier step passed (seams, LCD_CAM,
  PSRAM 10 min, Wi-Fi 7 min, relay 3 min). Main's run of the job at fd2d9ad73
  took 27 min 41 s, its S3 boot 5 min 35 s; this run reached the boot 2.5 min
  later (24 min 38 s against 22 min) and the budget fell inside it. No figure
  moved and no patch was posted for the job. Not split here (the PR is a
  mapping change); a rerun is the evidence either way.
- 2026-10-28 — PR #1319 again, runs 40739992412 (`9d40dc21c`) and
  40749890608 (`434a757d1`): the same job cancelled at its 30-minute budget in
  the same step, the S3 heap ratchet (started at 24 min 22 s: seams 2.6 min,
  PSRAM 9.9 min, Wi-Fi 6.4 min, relay 3.3 min), every earlier step passed.
  So the S3's heap ratchet has **never completed on this PR in CI**. Run
  locally on run 40739992412's own images instead
  (`LP_CI_IMAGES=… just heap-budget-check-chips-s3`): every figure `ok`
  (largestFreeBlock 73,744; stackHighWater 12,208 B in its band). The job
  wants the split this entry prescribes (the ratchet into its own job, or
  PSRAM's 10 minutes out); not done here.

**Exit criteria** — each emulator job's budget sits at 2–3× its measured max
with room to spare, and adding a device test names the job it lands in and that
job's current time (a recipe- or job-level time report a PR can see), so a
budget is never discovered by a cancelled run.

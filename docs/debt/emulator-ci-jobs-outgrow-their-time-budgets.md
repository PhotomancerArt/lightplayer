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

**Exit criteria** — each emulator job's budget sits at 2–3× its measured max
with room to spare, and adding a device test names the job it lands in and that
job's current time (a recipe- or job-level time report a PR can see), so a
budget is never discovered by a cancelled run.

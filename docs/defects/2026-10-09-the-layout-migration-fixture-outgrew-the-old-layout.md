---
status: fixed
found: 2026-10-09      # ci: "C6 layout migration (x64)" on held PR #1092 (run 38023028133); reproduced on origin/main at 0629f43a5
fixed: this change
area: lp-cli/tests/emu_layout_migration.rs × lp-cli `hardware lpfs fixture` (the legacy-layout "fielded board") × the single fw-esp32c6 image
class: budget-exhaustion   # a hard budget (the old layout's 0x300000 app region) is checked only by a path-gated job, so the image crossed it unseen
related:
  - ../adr/2026-10-02-c6-repartition-and-layout-migration.md   # Decision 11: the image may grow past 0x300000 once Studio imports a backup ZIP (met 2026-10-05)
  - ../adr/2026-07-28-esp32c6-flash-budget.md
  - ../debt/ci-runner-time-over-the-concurrency-cap.md          # why the job is gated so narrowly
  - lp2025/2026-10-09-1203-ram-research                         # found by E2's held PR (#1092); not caused by it
---
# The layout-migration test's "fielded board" can no longer hold the image it is built from

**Symptom** — `just test-emu-layout-migration` (CI job "C6 layout migration
(x64)") fails 3 of 4 tests at `lp-cli/tests/emu_layout_migration.rs:281`
(`Work::new_on`) with

```
Error: the merged image (3268832 bytes) reaches the filesystem at 0x310000
```

`lp-cli hardware lpfs fixture` (`lpfs/fixture.rs::build_chip`) refuses to lay a
merged image over the legacy `lpfs` at `0x310000`. On held PR #1092 CI saw
`3269344 bytes` (run 38023028133, job 114128308172: "1 passed; 3 failed").
The same command on a bare `origin/main` (`0629f43a5`) fails the same way, so
the PR did not cause it: its change to the image is **−528 B** locally
(3,268,304 vs 3,268,832 at its merge base, `2fac25657`).

The one test that passes is W11, which builds its chip on the **current**
table (`Layout::Current`, `--table partitions.csv`, `lpfs` at `0x350000`).
`the_bootloader_reads_back_the_chip_byte_for_byte` (step 0), W10
(`migrate_moves_every_file_…`) and W3 (`a_board_whose_files_do_not_fit_…`)
all build a `Layout::Legacy` chip.

**Root cause** — the fixture's premise stopped being true. `Work::new_on`
builds the shipped single image from this tree (`fw_esp32c6_image(&FwImage::SHIPPED)`
→ `cargo build --profile release-esp32`, default features,
`LP_EMU_BUILD_FW=1`), merges it with `scripts/emu/build-merged-image.sh` and
trims the 0xFF tail, then writes it under the frozen pre-2026-10 table
(`legacy_c6_v1_table()`: `factory` `0x10000`+`0x300000`, `lpfs` at `0x310000`).
The merged image must therefore be at most `0x310000` = 3,211,264 B (app
≤ 3,145,728 B). Measured on `origin/main` at `0629f43a5`, release-esp32,
`esp32c6,server,radio`:

| | App/part. size | merged, trimmed | vs `0x310000` |
|---|---:|---:|---:|
| `origin/main` (`0629f43a5`) | 3,203,296 B | 3,268,832 B | **57,568 B over** |
| #1092's merge base (`2fac25657`) | 3,203,296 B | 3,268,832 B | 57,568 B over |
| #1092 head (`4ffa8e318`), local | 3,202,768 B | 3,268,304 B | 57,040 B over |
| #1092 head, CI | 3,203,808 B | 3,269,344 B | 58,080 B over |

(The 1,040 B between local and CI for the same head is not explained; it was
not chased.) ADR Decision 11 *deliberately* allows the image past `0x300000`
(met 2026-10-05; the size check prints `legacy overlap` as information, not a
gate), and the image has since grown by Wi-Fi, the relay and over-the-air
updates. The test comment says the fixture is "the current firmware on the
frozen pre-2026-10 table", which was only ever possible while the current
firmware fit the old region. Nothing said so, and nothing checked it.

**Why nothing noticed** — the job is gated by `c6_layout`, the narrowest
filter in `pre-merge.yml` (the table, the flash layout, the guard, the
migration host path, the emulated flash, the merged-image recipe and the test
itself; ~35 minutes of runner), and it is **deliberately not forced on main
pushes** (Yona, 2026-10-05). Image growth touches none of those paths, so
the job has not run on `main` since the image crossed the line; the first
change to touch a gated path is the first to pay (#1092 edits
`lp-fw/fw-esp32c6/.cargo/config.toml`, which is in the filter). The job's budget
comment records a green 34m40s run on #970 (2026-10-04); the crossing came after.
The commit that crossed it was not bisected.

**Fix** — the fielded board carries a pinned image, never this tree's. The
fixture chip's firmware is `ReferenceImage::SHIPPED_USB_SILICON`: the shipped
feature set at `735af98ae`, the commit flashed on a desk C6 for the 2026-09-07
silicon `boot-idle` capture, built by `scripts/emu/build-reference-image.sh`
in a detached worktree and merged with that commit's own (legacy) partition
table. It is a board that really existed on the old layout, and it does not
grow with the tree. The test checks the pin is pre-repartition (its commit's
`lpfs` row is `0x310000`) and `lpfs fixture` still refuses it if it reached the
filesystem. What `lpfs migrate --merged` writes is unchanged: this tree's
image, so the migration under test is today's host path. No release asset could
serve: no release before the repartition carries firmware, and every released
C6 image since is past the line.

Because today's image now reaches past `0x310000`, W10 runs Decision 7's
retire-first write order, which no earlier run of the test reached. Its "the
old superblock pair is erased" check now reads "retired": erased, or this
image's own bytes where the image covers it. The CI job checks out full history
(the pinned commit) and restores the reference-image cache `emu-c6` saves (the
same image backs its `flash_over_socket` tests), read only.

**On the job's gating** (the question above): it stays PR-only and narrowly
gated. The premise that drifted is pinned now. The image `migrate` writes may
grow up to the 3.25 MB `factory` partition, and `fw-esp32c6-size-check` gates
that on every firmware PR, so a schedule would only re-prove a pin.

**Regression coverage** — the test itself, all four scenarios green again
(`just test-emu-layout-migration`, `lp-emu:esp32c6:t1`). W10 now also covers
the retire-first order.

**Lesson** — a test that builds a fixture "from this tree" inherits every
budget that tree's output is subject to. Here the budget (the old layout's
3 MB app region) was retired by an ADR decision, but the test that depended on
it was gated away from the changes that spent it. A job narrow enough to skip
main is also narrow enough to miss its own premise breaking; either the
premise is pinned (the fixture does not move with the tree) or something
cheap checks it on every main merge (here: `merged <= 0x310000`).

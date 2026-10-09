---
status: open
found: 2026-10-08      # test (storage testbed exhaustive cut sweep, lp-nor-sim)
area: littlefs-rust 0.1.0 (crates.io; light-player/littlefs-rust) — metadata-pair relocation (`block_cycles` > 0)
class: write-ordering
related:
  - 2026-10-08-littlefs-rust-compacts-every-metadata-commit
  - 2026-10-08-littlefs-rust-cross-directory-rename-cut-loses-the-next-entry
  - lp2025/2026-10-07-2337-storage-testbed
---
# A power cut while littlefs-rust relocates a metadata pair can leave lpfs unmountable

**Symptom** — in the storage testbed (`tools/lp-store-bench`), F1 (the
firmware's own littlefs layout) with `block_cycles=1` failed 26–27 of 1,388
cut cases on `save:c20` (per tear model, two seeds). 18–19 were
`remount_failed — mount: Corrupt(littlefs: corrupt)`, a filesystem that no
longer mounts, and the rest were a document gone after the re-run. The same
sweep has 0 failures with `block_cycles` unset (the firmware today) and with
3, 10, 30 or 100. All are **simulator numbers** (`lp-nor-sim`).

**Root cause** — not yet read in the library. `block_cycles=1` makes littlefs
relocate a metadata pair every third compaction, and in today's port every
commit compacts (see the related FCRC defect). So the sweep is mostly cutting
relocations, including ones that cascade into the parent directory's commit.
Upstream littlefs is designed to survive a cut anywhere in a relocation. The
port has already been shown to mishandle the related "move" repair state
(the cross-directory rename defect). The patched-FCRC library ran the same
sweep with 0 failures, but it compacts ~10× less, so far fewer cuts land in
a relocation. That is not evidence the path is fixed.

**Fix** — none. Product exposure today is nil: the firmware runs
`block_cycles = -1` (littlefs-rust's `Config::new` default), so it never
relocates for wear. A pair can still be relocated when a block goes bad,
though, and that is exactly when a worn sector would trigger it. **Do not
set `block_cycles` in `lpfs_config` until this is root-caused.** At 100 the
measured win is real (30 days, 176 sectors, no re-push: hot sector 21,602 →
711 erases, simulator), but an unmountable `lpfs` is formatted by
`init_guarded`, and the board loses every project. The one-line change is
parked as a patch in the testbed plan's `results/lpfs-block-cycles-100.patch`.

**Regression coverage** — none yet. Reproducer: on PR #1040's branch with the
`block_cycles` dial (stacked change), run
`lp-store-bench sweep --candidates 'f1@block_cycles=1' --workloads save:c20 --seeds 1,2`;
each failure line replays with `lp-store-bench replay`.

**Seen again, 2026-10-08 (F3)** — the control candidate F3 (littlefs, one
package per pattern, `block_cycles` 100 by default) reproduces the
"document gone after the re-run" half on another layout. At
`f3@block_cycles=1`, `save:c40` at 128 sectors loses 16–17 of 3,066 cut
cases per tear model (`rerun_wrong_state`, all on step 19, cuts 29–37):
the cut mount reads every file as old or new, then the re-run's first
commit — whose littlefs calls all stay inside `/projects/a/modules/`
(remove, create, write and same-directory rename of one `.pkg.tmp`) — takes
the whole `/.lp` directory with it. Replays with
`lp-store-bench replay` on
`{"driver":"exhaustive","failure":{"detail":"/.lp/access.json: got None B, want Some(153) B","kind":"rerun_wrong_state"},"reproducer":{"case":{"candidate":"f3","config":{"dials":{"block_cycles":"1"},"sectors":128},"cut_after":33,"second":null,"seed":17901088477037630240,"step":19,"tear":"clean","workload":{"corpus":"c40","kind":"save","seed":1}}},"type":"failure"}`.
At F3's own 100 and at −1, `save:c40`, `push:c40` and `panel:c40` had no
failures (lp-nor-sim; PR #1060's report has the counts) — but 100 cuts few
relocations, so that is not evidence the path is safe.

**Lesson** — a littlefs setting that looks like pure policy, with no
on-flash format change, still decides which library code runs. Turning one
on is a code-path change, and it gets cut-swept like one.

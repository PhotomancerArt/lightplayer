---
status: open
found: 2026-10-14      # test (lp-nor-sim calibrated_mid_word sweeps, after the CX1 500-cut re-fit)
area: lp-base/lp-tree-store (record_scan.rs scan_sector; store_mount.rs head resume)
class: single-read-trust
related:
  - docs/defects/2026-10-14-the-calibrated-nor-model-never-tears-a-program-inside-a-word.md
  - docs/reports/2026-10-08-c6-nor-tear-calibration.md
  - lp2025/2026-10-08-1017-tree-store-device-round
---
# The tree store trusts a torn record that reads whole on one mount, and loses what it wrote after it

**Symptom** — under `lp-nor-sim`'s `calibrated_mid_word` (every torn
program caught inside a 4-byte word, the shape CX1 showed 2 times in 79),
the tree store fails four of its own seven cut sweeps. It fails them at
every geometry: its own, 128 and 176 sectors (`LP_TREE_STORE_SWEEP_CUTS=1000000
LP_TREE_STORE_SWEEP_STEPS=12 LP_TREE_STORE_SWEEP_TEARS=calibrated_mid_word
cargo test --release -p lp-tree-store cut_sweep`):

```
step 3 cut 2/7 CalibratedMidWord: remount after re-run        (per-call commits)
step 0 cut 15/24 CalibratedMidWord: remount after re-run      (deflated push)
step 10 cut 114/124 CalibratedMidWord: remount after re-run   (through GC)
get: Corrupt("record CRC")                                     (append in 4 KiB chunks)
```

`lp-store-bench sweep` and `double` (t1 and `t1@codec=host_deflate`,
c40 save/panel/push/repush/switch, 128 and 176 sectors, which read the
same) fail 49 of 30,296 cases at each geometry. The kinds are
`unmountable` (26), `remount_failed` (15), `remount_wrong_state` (7) and
`read_error` (1); the counts are in the calibration report, section 11.
One reproducer: `lp-store-bench replay` of

```json
{"driver":"exhaustive","failure":{"detail":"mount: Corrupt(record CRC)","kind":"unmountable"},"reproducer":{"case":{"candidate":"t1","config":{"dials":{},"sectors":128},"cut_after":41,"second":null,"seed":2830921387294069037,"step":14,"tear":"calibrated_mid_word","workload":{"corpus":"c40","kind":"save","seed":1}}},"type":"failure"}
```

prints `replayed: FAILS — unmountable — mount: Corrupt(record CRC)`.
Under `calibrated` (the mix, where 2 of 79 torn programs are mid-word) and
under `calibrated_before_erase`, all 30,296 cases at each geometry are
clean.

**Root cause** — CX1's second sub-word tear (boot 24, sector 5) left one
word with 14 of its 15 clears landed and one weak bit. The likeliest
reading is that the weak bit is the 15th clear. A record holding such a
word reads **exactly as written** on the reads where that bit reads 0, so
its CRC passes on some reads and fails on others. The store reads it once:

1. After the cut, mount's scan (`scan_sector`) reads the torn record with
   the bit at 0, so its CRC passes. The sector's trusted end moves past it.
   The tail after it reads erased, so the sector is resumed as the head
   (`store_mount.rs`).
2. The re-run (or the next commit) appends its records, its root
   included, after the torn one, and verifies them. They are good.
3. A later mount reads the weak bit as 1, so the torn record's CRC fails.
   `scan_sector` closes the sector there, and nothing after it is trusted.
   The newer roots go with it. Mount falls back a root (the committed write
   is lost: `remount after re-run`, `remount_wrong_state`) or finds no
   complete root (`unmountable`).

The torn record can also be a **root**. The scan offers it as the newest
root on a good read, and the closure walk reads it whole. Then mount's
final `read_record` of the chosen root reads it the other way, and the
whole mount fails: `mount: Corrupt(record CRC)` in the reproducer above.
That path needs no second commit.

Reproduced in a scratch test (sector 1): the torn 146-byte record at
387..533 ends in byte 532, cell `0x76`, weak mask `0x02`. The record
intended `0x74`. The re-run's `panel.json` (130 B) was committed, and after
the remount the store held the old 120 B one.

**Fix** — not in this change; this entry only files the finding. Directions:
don't resume a head after mount (open a fresh sector, so a torn tail is
always at the end of a closed one), or keep anything written after a
mount's last trusted record out of reach of a record whose CRC was not
read twice the same. A second read only lowers the odds. The root path
needs its own answer. The newest root is normally the last record of its
sector, so nothing written after it can vouch for it. At the least, mount
must survive the root reading the other way on a later read: fall back a
root instead of failing, and keep the root's bytes from the read it
trusted. Also check whether GC can copy a record past such
a tail.

**Regression coverage** — none yet. The sweeps above are the reproducer.
`calibrated_mid_word` is in `TearModel::NAMED`, so
`LP_TREE_STORE_SWEEP_TEARS=calibrated_mid_word` runs it. Add it to the
sweeps' default list once the store passes.

**Lesson** — "a CRC that passed" is a property of one read, not of the
cells. The store already knows this for erases: a sector it did not erase
itself this session is erased again before use, because a torn erase can
read `0xFF`. A torn program can likewise read *correct*. Anything that
moves a trust boundary on one read (where a log ends, whether a tail is
blank) has to assume the next read may disagree.

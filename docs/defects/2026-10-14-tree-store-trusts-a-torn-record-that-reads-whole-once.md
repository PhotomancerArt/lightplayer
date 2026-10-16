---
status: fixed      # mount settles each head's tail: re-program its last record, kill a record that failed, never resume an empty head
found: 2026-10-14      # test (lp-nor-sim calibrated_mid_word sweeps, after the CX1 500-cut re-fit)
area: lp-base/lp-tree-store (record_scan.rs scan_sector; store_mount.rs head resume)
class: single-read-trust
fixed: 9c141ca7d, bd07c16f0
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

**Fix** — mount settles the last program of the newest sector of each
kind (`record_scan.rs` `scan_sector`, told which sectors by
`store_mount.rs`) during its first pass, before the closure walk, the root
read or a resume can trust it. Only that program can be torn: every
program before it was read back whole.

- When the sector's records end at erased space, its last record is
  programmed again with the header and payload bytes the scan just
  CRC-checked (`RecordLog::reprogram`), then read back. Program only
  clears, and the bits it clears are the ones the record asked for, so the
  weak clear finishes and the record reads so on every read. A read-back
  that differs closes the sector.
- When they end at a record that did not check (a bad header, a length past
  the sector, a CRC that failed), that record's 16 header bytes are
  programmed to zero (`RecordLog::kill_record`). It then fails to parse on
  every read, so a later mount can never read it whole and offer a root (or
  index a GC copy) the first mount did not see.
- A newest sector that holds no record (its last program was its header's
  magic, which can be weak the same way) is not resumed: the head opens a
  fresh sector.

No byte of the format changes (FORMAT.md gains the writer rule). It costs
nothing on a write; a mount programs at most two records again (2 program
calls each, header and payload, ≤ 1,040 B a record at `record_max` 1,024)
and reads them back. Measured on a c40-shaped store at 128 sectors after a
panel write: 4 program calls, 160 B programmed, 5 more read calls (320,722
→ 320,882 B read). No erase is added, so no wear. RV32 size probe: +560 B
`.text` (`mount` +494 B).

Alternatives, measured the same way (lp-nor-sim, `calibrated_mid_word`;
`lp-store-bench sweep` + `double` at 128 sectors, T1 stored and
host_deflate, the case set the calibration report counts; and the in-crate
sweeps at every cut point):

| fix | bench failures | in-crate sweeps failing | cost |
|---|---:|---|---|
| none (`main`) | 49 | per-call, append, deflated push, GC | — |
| read every record twice at mount | 33 | GC | twice mount's reads; only lowers the odds |
| never resume a head after a mount | 16 (`unmountable`) | GC (a torn GC copy indexed, then read bad) | an erase and a part-empty sector per kind per session |
| re-program the tail only (no kill) | 8 (`unmountable`) | none (the remount test below fails) | the chosen one's, less the kill |
| **re-program the tail + kill a failed one** | **0** | **none** | above |

Re-programming the tail (the director's lean) is necessary but not
sufficient: a torn record that reads *bad* at the first mount and whole at
a later one is the other half, and only making the bad read permanent
closes it. Not resuming the head leaves a torn GC copy that read whole
indexed as the newest copy, so its victim is freed and the record lost on
the next bad read.

Physically, re-programming bytes that already read so is the same kind of
operation as the sector kill the store already does (programming over
programmed cells, clearing only), and the part's own erase pre-programs
every word to `0x00` before lifting it. That a re-program finishes a weak
clear on CX1's part is the model's assumption (`lp-nor-sim` makes a weak
bit solid when it is programmed to 0), not a silicon measurement.

With the fix, at 128 and at 176 sectors, under `calibrated_mid_word`,
`calibrated` and `calibrated_before_erase`: the bench's exhaustive sweeps
14,370 cases each, 0 failures; its double cuts 21,464–22,214 cases each, 0
failures (up from 15,328, because a mount now programs, and the double
driver cuts at every op of the mount that follows a cut); every in-crate
cut sweep at every cut point passes at its own geometry, 128 and 176
sectors. The `lp-emu:esp32c6:t1+net=lan+flash-cut` `fs-tree` cut walk
(CI form, a local firmware build of this change): 164 cuts, 0 failures.
All lp-nor-sim / emulator figures, never silicon.

**Regression coverage** — `calibrated_mid_word` is now in the in-crate
sweeps' default tear list (`test_support::sweep_tears`); against the
unfixed store the default run fails `cut_sweep_per_call_commits` and
`cut_sweep_deflated_push`. `cut_sweep_tests::cut_remounts_read_one_state`
cuts per-call commits and GC churn under `calibrated_mid_word` with 40 tear
seeds a cut and mounts each result five times writing nothing: every mount
must read the first one's state. It fails on the unfixed store and on the
re-program-only variant (per-call step 2, cut 6, seed 33: a torn root read
bad, then whole). The mutant `trust_tail_once` switches the settle off;
`lp-store-bench mutants` (whose set now runs `calibrated_mid_word`) catches
it, and every one of the 12 mutants is caught. The reproducer above
replays as `passes`.

**Lesson** — "a CRC that passed" is a property of one read, not of the
cells. The store already knows this for erases: a sector it did not erase
itself this session is erased again before use, because a torn erase can
read `0xFF`. A torn program can likewise read *correct*. Anything that
moves a trust boundary on one read (where a log ends, whether a tail is
blank) has to assume the next read may disagree.

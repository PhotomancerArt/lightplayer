---
status: fixed
found: 2026-10-14      # hardware-walk (the CX1 sitting: 200 unaligned power cuts, the model's first out-of-sample erase cuts)
fixed: 7d5b25323 (model), 02fad65f4 (analyzer), ec08d0854 (emulator)
area: lp-emu/lp-nor-sim (calibrated_tear.rs, EraseShape) and scripts/emu/flash-tears-analyze.py
class: fidelity
related:
  - docs/reports/2026-10-08-c6-nor-tear-calibration.md
  - docs/defects/2026-10-14-the-calibrated-nor-model-never-tears-a-program-inside-a-word.md
  - lp2025/2026-10-08-1017-tree-store-device-round
---
# The calibrated NOR model never tears an erase inside a word, or as its first cell lifts

**Symptom.** CX1 took 200 power cuts of `flash-tears-unaligned`
(`silicon:esp32c6`, flash JEDEC `0x464016`, transcripts
`lp-emu/transcripts/esp32c6/flash-tears-unaligned/silicon-esp32c6-2026-10-14-609f0e4c9*.txt`).
172 of the cuts landed in an erase. The erase is the same command in both
payloads, so these are the re-fitted `TearMix::CX1`'s first out-of-sample
erase cuts. Every share was inside its interval (report section 12). But
three sectors are in states `calibrated_tear::tear_erase` cannot produce.
None of the 418 aligned erase cuts showed either state.

- **A pre-program caught inside a word.** `-r3.txt` boot 10 (sector 5,
  cycle 6837). Bytes 0–764 are `0x00` (exact, `leading_zero_bytes` 765).
  Word 764–767 has 17 of its 21 clears landed, and page 2 has 2 weak bits.
  The old pattern is whole after it. `EraseShape::Zeroing` only ends on a
  whole word with no weak bits.
- **All zero but one bit, as the erase starts to lift.** In
  `609f0e4c9.txt` boot 50 (sector 3, cycle 2179) that bit is weak (byte
  2154). In `-r4.txt` boot 17 (sector 8, cycle 10184) it is a stable 1
  (byte 2530), with no weak bits. `EraseShape::AllZero` is every bit 0.
  `EraseShape::Erasing` interpolates inside `CX1_ERASING`, whose
  least-lifted row is 32,197 zeros with 181 weak bits, so it cannot draw
  32,767 zeros with 0 or 1 weak bit.

The analyzer hides all three. It sorts the first cut and boot 50 into
`erase:erasing`: a `mixed` sector that kept every old zero but has a weak
bit falls through to that class. It sorts boot 17 into `unknown`: the
lifted bit sits where the old data had a 1, so no old zero was lost and
the zero-run reconstruction does not fit. `--check-model` judges only the
aligned payload's shares (rule 1), so it stays CONFIRMED. Had these cuts
been aligned, the two classes would be NOT MODELLED (rule 5).

**Root cause (model).** The erase shapes were fitted on aligned cuts.
Those caught the pre-program only on whole words, and the lift only after
at least 571 bits had moved. The part can be caught at a finer point in
both phases: part-way through one word's pre-program, and with only one
cell lifted.

**Fixed (2026-10-14).** Both states are shapes of their own, and the model
is re-fitted on all 700 cuts.

- *The analyzer* (02fad65f4) has two new classes. `erase:zeroing-mid-word`
  reads the pre-program off the record's counts: old and new are
  complements, so `landed_new_zeros` counts the landed clears exactly, and
  the page and byte counts must fit a run of whole words, one part-landed
  word and the old pattern after it. `erase:first-lift` is all `0x00` but at
  most 32 bits (1 in 1024), each a stable 1 or weak. Reading the landed
  count found **two more pre-programs caught inside a word**, both filed
  `erase:zeroing` before, with no weak bit: `41e96eead-r6.txt` boot 32
  (aligned, sector 12: the run stops on the word at 3,092, 3 of its 14
  clears landed) and `609f0e4c9-r3.txt` boot 38 (2 of 16). The old zeroing
  path only checked where the run stopped and the `0xFF` count, never the
  landed count. So the aligned set was not clean: 1 of 418 against 2 of 173
  unaligned (plus the 2 first lifts there), and the "1 in 40" split this
  entry worried about is gone. The record places landed clears only to
  their page; that they sit in the word the run had reached is the reading.
- *The model* (7d5b25323): `EraseShape::ZeroingMidWord` (whole `0x00`
  words, one word with part of its clears landed, weak bits possible on
  others, old after; `CX1_ZEROING_MID_WORD` = 2 of 16, 3 of 14, 17 of 21 with
  2 weak) and `EraseShape::FirstLift` (`CX1_FIRST_LIFT` = one weak bit, or
  one stable 1). `TearMix::CX1` is fitted on the 700 cuts with the same
  count estimator: erase 20 zeroing, 3 zeroing inside a word, 87 all zero,
  2 first lift, 101 erasing, 7 reads-`0xFF`-weak, 370 reads `0xFF`, 1 before
  the erase (591); program 74 / 28 / 2 (104). Forced models
  `calibrated_zeroing_mid_word` and `calibrated_first_lift`.
  `--check-model` now judges both payloads' silicon cuts (rule 1: the erase
  is the same command; an unaligned program is read the `rom` way, which is
  how the model tears a page op). Against the 500-cut model the 700 said
  MODEL DIFFERS, the two classes NOT MODELLED; against the re-fit,
  CONFIRMED — **in-sample**, by construction. The next sitting is the next
  out-of-sample test.
- *The emulator* (ec08d0854): FlashCut draws the same shapes and names them
  on the cut line.
- *The store*: under both forced models and the re-fitted mix, every
  in-crate cut sweep, the torn-header test and `cut_remounts_read_one_state`
  pass, and `lp-store-bench` sweep + double (t1, t1@codec=host_deflate) has
  0 failures, at 128 and 176 sectors. The two models joined the in-crate
  default tear list. Report section 13 has the numbers and the long
  emulated walk.

**Regression coverage.** `erase_word_and_lift_self_test` (`just
lint-flash-tears-analyze`) rebuilds all five silicon cuts from the pattern,
checks that each record counts what the cut's record counted, and checks the
class. `lp-nor-sim` tests that the model draws both shapes, alone, through
`NorFlashSim` and in the mix's proportions; the payload's own flow on
`lp-nor-sim` under each forced model (`flash_tears_on_nor_sim`) sorts 248 of
248 erase cuts into the class it names, page-aligned and unaligned.

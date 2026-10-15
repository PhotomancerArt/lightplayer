---
status: open
found: 2026-10-14      # hardware-walk (the CX1 sitting: 200 unaligned power cuts, the model's first out-of-sample erase cuts)
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

**Not fixed here.** The sitting's PR was out-of-sample data only, with no
re-fit. Things to settle before a fix:

- Whether 3 in 172 against 0 in 418 means something. If both sets come
  from the same erase, that split is about 1 in 40. The two images differ
  only in the program plan.
- Whether `Zeroing` grows a mid-word tail (as `ProgramShape::MidWord` did)
  and `CX1_ERASING` grows an onset row, or whether these become shapes of
  their own.
- The analyzer's classes: a zero run whose last word is part-landed, and an
  all-zero sector with a handful of lifted or weak bits.

The tree store's exposure is the same as with `calibrated_mid_word`. A
torn pre-program word with weak bits can read differently from one mount
to the next. See
`docs/defects/2026-10-14-tree-store-trusts-a-torn-record-that-reads-whole-once.md`.
An erase-side version only matters if the store reads an erased-then-cut
sector as anything but not erased. It should not: it never trusts a
sector it did not erase and read back in this session.

**Regression coverage.** None yet. A fix needs analyzer self-tests built
from the pattern for both states (as `mid_word_self_test` does) and
`lp-nor-sim` tests that the model draws them.

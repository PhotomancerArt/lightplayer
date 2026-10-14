---
status: fixed
found: 2026-10-14      # hardware-walk (the CX1 sitting: 500 aligned power cuts, `just flash-tears-analyze --check-model`)
fixed: 09f6ad6e9 (model), c452b79a8, ed63bb129 (emulator), 10ed7b383 (analyzer)
area: lp-emu/lp-nor-sim (calibrated_tear.rs, TearModel::Calibrated) and scripts/emu/flash-tears-analyze.py
class: fidelity
related:
  - docs/reports/2026-10-08-c6-nor-tear-calibration.md
  - lp2025/2026-10-08-1017-tree-store-device-round
---
# The calibrated NOR model never tore a program inside a word, and never cut before an erase

**Symptom** — at 500 aligned power cuts on CX1 (`silicon:esp32c6`, flash
JEDEC `0x464016`), `just flash-tears-analyze --check-model` said MODEL
DIFFERS. Every share `TearMix::CX1` modelled was inside its corrected
interval. Two cuts were of shapes the model could not draw:

- `silicon-esp32c6-2026-10-14-41e96eead.txt` boot 34 (sector 0, cycle
  1328), sorted `program:scattered`: the first 3,872 bytes whole, then the
  next 4-byte word with 3 of its 17 clears landed, in two bytes, and one
  weak bit on a clear that had not landed. Nothing after byte 3,876.
- `-r4.txt` boot 24 (sector 12, cycle 10716), sorted `untouched`: both
  journal copies named the cycle, and the sector read its old pattern whole.

The analyzer also hid a second sub-word tear. Boot 24 of the first
transcript (sector 5, cycle 949) ended on a word, at byte 3,260. That word's
last byte had 14 of its word's 15 clears landed and one weak bit. The
firmware called it `byte-prefix` and the analyzer sorted it `mid-command`,
because the analyzer judged where the landed bytes ended. It did not check
that the last word landed whole.

**Root cause** — `calibrated_tear::tear_program` landed a prefix of whole
4-byte words and nothing else, because the first 200 cuts all ended on a
whole word. The part's program unit can also be caught part-way: 2 of 79
torn programs show it. In the erase path, `NorFlashSim::erase_sector` gave
every calibrated cut at an erase one of the five erase shapes. All of them
change the sector. But the silicon cut is uniform in time, and between the
journal write and the erase command there is a gap: software, then the
command reaching the part. A cut that lands in that gap leaves the sector
exactly as it was. The journal can tell that apart from a torn journal
write: both copies named the cycle. So in a calibrated sweep, the state
"ops before it done, this erase never started" could not be reached. Under
`Clean` it could, but with no other realistic tear. For a program, the same
gap was already modelled: a prefix ending at offset 0 is nothing landed.

Why this is the gap and not an erase that started and moved nothing:
`Wait_SPI_Idle`, write enable and the erase command take tens of µs of
software on this path. The data shows the pre-program moves about a word
every µs: 17 of 500 cuts caught a `0x00` run still growing, 3.4 % of a
33 ms cycle, which is 1.1 ms for 1,024 words. So an issued erase is most
unlikely to sit for long with nothing moved. At a uniform cut, an assumed
gap of ~10–50 µs predicts 0.15–0.75 such cuts in 500. An erase issued but
not yet moving (a few µs) predicts ~0.05. The emulator's own dry run agrees. Its flash completes every
operation inside the register write (`spi1.rs`), so an emulated cut can
land only between operations. Of its 5 unaligned cuts, 2 read old: one with
both copies naming the cycle (the gap before the erase), and one with copy
1 still a cycle behind (the gap between the two copies' journal writes).

**Fix** — the model is re-fitted on all 500 aligned cuts, through
`--model-table`, the estimator every weight already used:

- `ProgramShape::MidWord` (`program_mid_word: 2`). Whole words, then one
  word (any word asking two or more clears) with at least one of its clears
  landed and at least one not, weak bits on clears that did not land, then
  nothing. The landed share and the weak count interpolate `CX1_MID_WORD`
  = (3 of 17, 1 weak), (14 of 15, 1 weak).
- `cut_before_erase: 1` (of 418 cuts at an erase). It is drawn in
  `erase_sector` before any shape. Power is gone, the sector is untouched
  and not tainted, and the cut is counted in
  `NorStats::erase_cuts_before_start`, not in `torn_erases`. It records
  where the cut landed, not an erase shape.
- `calibrated_before_erase` and `calibrated_mid_word` are forced models, so
  a sweep meets each state at every cut, not 1 time in 400 or 40.
- The analyzer sorts both sub-word tears into `program:mid-word`. It reads
  the two journal copies to say which operation was in flight: an old,
  whole sector with both copies naming the cycle is `cut:before-erase`. A
  cut in a copy's wrap erase or in an entry's program is `journal:*`, and
  the slots it damaged count inside the operation in flight, not as
  collateral. All three silicon `complete` cuts turned out to be copy 0's
  wrap.

`--check-model` now confirms: erase family 5 cells at 99 %, program family
2 cells at 97.5 %. That is in-sample, because the model is now fitted on
the cuts it is checked against. The next aligned sitting is its next
out-of-sample test.

**Regression coverage** — `lp-nor-sim`:
`a_mid_word_tear_lands_whole_words_then_part_of_one_word_then_nothing`,
`the_calibrated_mix_tears_a_program_inside_a_word_about_one_time_in_forty`,
`a_cut_before_the_erase_leaves_the_sector_as_it_was_and_untainted`, and
the proportions test over all six cut outcomes at an erase. The analyzer's
`--self-test` (`just lint-flash-tears-analyze`) covers `mid_word_self_test`
(both silicon shapes, rebuilt from the pattern, plus the whole-word and
scattered controls), `journal_self_test` (each journal reading, the two
silicon boots and the emulator's among them) and `check_model_self_test`
(the 200-cut model against the 500 shows exactly these two NOT MODELLED; a
weight of 0 is no weight; the re-fit confirms). Closed loop: the payload's
flow on `lp-nor-sim` (`flash_tears_on_nor_sim`) under `calibrated_mid_word`
and `calibrated_before_erase` sorts every such cut into the new classes.

**Lesson** — a shape sorter that judges only where the landed bytes *end*
can file a bit-level tear as a clean one. The second sub-word tear sat in
`mid-command` until a whole-word check looked inside the last word. When a
calibration finds its first instance of a shape, look for the shape's
quieter variants already filed under neighbouring classes. Separately, the
operation counter is not the cut's clock. A model that tears "the op in
flight" has no gap between ops. If the real cut is uniform in time, the gap
before an op is a state the counter must be able to give.

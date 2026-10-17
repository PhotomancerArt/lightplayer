---
status: open
found: 2026-10-16      # test (A2's exhaustive sweeps, lp-nor-sim simulator)
area: lp-tree-store — counter_wrap_tests.rs, GC and mount under a wrapped sector seq
class: untested-path
related:
  - lp2025/2026-10-16-0848-tree-store-adoption
  - docs/defects/2026-10-08-tree-store-root-sequence-does-not-wrap.md
---
# The sector-sequence wrap sweep fails when every cut point is run

**Symptom** — `the_sector_sequence_wraps_under_cuts` passes at its sampled
cut points (the default), but run with every cut point fails on `main`
(`3fc9aa987`) as well as on A2's branch:

```bash
LP_TREE_STORE_SWEEP_CUTS=1000000 LP_TREE_STORE_SWEEP_STEPS=12 \
  LP_TREE_STORE_SWEEP_TEARS=clean \
  cargo test -p lp-tree-store --release --lib the_sector_sequence_wraps
# step 7 cut 19/58 Clean: re-run failed: NoSpace
```

Under every tear model the same cut fails its re-run; under
`calibrated_mid_word` the first failure is earlier, a mounted store whose
`get` reports `Corrupt("record CRC")`.

**Why it was not seen** — the README's exhaustive command filters on
`cut_sweep`, which leaves the wrap test out; the default run samples 8 cut
points a step.

**What is known** — the test starts the sector counter 8 below `u32::MAX`
on a 10-sector store, so after the wrap the newest sectors carry seqs below
older ones. Two things lean on "the largest seq is the newest": the head
mount settles (a torn record's weak bits, FORMAT.md "Finding the committed
state") and cost-benefit GC's ages. The `calibrated_mid_word` failure fits
the first (the torn sector is not the one settled); the `NoSpace` re-run
fits the second (GC choosing by wrapped ages on a nearly full store). Not
traced further: a wrap needs 2^32 sector opens, ~330 times a 128-sector
part's endurance. A2's fast mount reads a store whose seqs span half the
counter or more as a whole, which is the old behaviour, so it neither causes
nor changes this.

**Next** — order sectors by serial-number arithmetic (RFC 1982) over the
seqs in use, or bound the span, for both the settle and GC's age; then make
the exhaustive command include the wrap test.

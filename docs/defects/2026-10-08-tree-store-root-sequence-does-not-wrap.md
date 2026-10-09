---
status: open
found: 2026-10-08      # test (M3 P2 counter-wrap case, lp-nor-sim)
area: lp-tree-store — root record `seq` (FORMAT.md "Root"), mount's root choice (`record_scan.rs` `RootCandidates`)
class: untested-path
related:
  - lp2025/2026-10-08-1017-tree-store-device-round
  - lp-base/lp-tree-store/FORMAT.md
---
# The tree store's root sequence does not wrap: a commit past `u64::MAX` mounts as the one before the wrap

**Symptom** — a store started with its root sequence a few commits below
`u64::MAX` (a test-only start value; the M3 counter-wrap case) commits
`/after.json` as a root with seq 0 (`wrapping_add`), then remounts at the
previous commit: `/after.json` is gone, with no error and no cut. Under the
cut sweep the same start fails at the first step after the wrap
(`step 3 cut 0/65 Clean: remount after re-run`).

**Root cause** — FORMAT.md defines the root's seq as "larger = later" and
mount keeps the highest-seq CRC-good roots (`RootCandidates::offer`, a plain
`u64` comparison). The writer wraps (`max_root_seq.wrapping_add(1)`), the
reader does not; so after a wrap the pre-wrap root, still on flash until GC
collects its sector, wins. Not reachable in practice: the counter starts at 1
at format and counts commits, so a wrap needs 2^64 commits (half a billion
years at one commit a millisecond). The sector sequence (u32) wraps safely
(`the_sector_sequence_wraps_under_cuts`: 2,036 cuts at every cut point under
clean, byte_prefix, random_bits and calibrated, 0 failures, lp-nor-sim): its
order only picks among identical copies, the head to resume and GC's ages.

**Fix** — none in this change (M3 reports; the store's fixes go in their own
PR). The choice for G2: document the width as the format's limit, or compare
roots by serial-number arithmetic (RFC 1982) — which needs every root on
flash to be within 2^63 of the newest, already true given the one-step
fallback — or refuse a write that would wrap.

**Regression coverage** —
`lp-base/lp-tree-store/src/counter_wrap_tests.rs`
`the_root_sequence_does_not_wrap` pins the loss (it fails once roots order
across a wrap — replace it then). Replay:
`cargo test -p lp-tree-store --lib counter_wrap`.

**Lesson** — "larger = later" in a persisted counter is a promise about its
width; say the width is the limit, or define the order across the wrap, in the
format spec itself, and test the wrap with a test-only start value rather than
reasoning that it cannot happen.

---
status: open
found: 2026-10-10      # test (F1's verification on M3's drivers, lp-store-bench, lp-nor-sim)
area: lp-tree-store `store_space.rs` `ensure_room` (tail-only compaction) × `space_estimate.rs` `fits_after_compaction`
class: split-source-of-truth
related:
  - docs/defects/2026-10-09-tree-store-rerun-after-a-cut-is-refused-at-the-edge.md
  - lp2025/2026-10-08-1017-tree-store-device-round
---
# At the edge, GC cannot pack what the packing bound admits, so a re-run after a cut can still be refused

**Symptom** — after the fix for
`2026-10-09-tree-store-rerun-after-a-cut-is-refused-at-the-edge.md`, the
full-flash driver still meets `rerun_failed: NoSpace` on a 16-sector store:
`cargo run --release -p lp-store-bench -- full-flash --candidates t1
--sectors 16 --corpus-name syn:3:900 --seeds 2 --edge-steps 24
--cuts-per-step 10 --tears clean,byte_prefix,random_bits,calibrated` gives 6
of 379 cases on seed 1 (`step 26 repush cut 33/150` and `82/150`, refused
again after a remount; `step 32 push-f13` at four cuts, which a second
attempt fits) and 2 of 377 on seed 2 (`step 25 save cut 135/305` and
`236/305`, a second attempt fits). Eight seeds: 9 of 2,989 cases.
`lp-store-bench mutants`' unmutated store: 8 of 756 full-flash cases, no
other driver. The 128-sector long walk the first defect named runs clean.
Nothing committed is lost; every case recovers once space is freed. Every
figure is an lp-nor-sim simulator figure, M3's drivers on a local merge of
PR #1069 over F1 (PR #1123).

**Root cause** — instrumented (each GC exit printed every sector's used and
live bytes and the heads): in every case the free count sits at the reserve,
GC has collected every byte of garbage outside the open heads, and the write
still needs one more cold sector. The cold sectors are full of records
near 1 KB (each copy's 900-byte shaders), so each keeps a tail only small
records can use. In `step 32` the cold head had 190 B left and the record
was 207 B, and the largest tail anywhere was 205 B; the hot head held 1,491 B
of old roots, which renewing cannot lend a cold record. Tail-only
compaction then cycles: collecting a sector copies its records in offset
order, its first record is a chunk that does not fit the head's tail, so a
new head opens with the whole sector in it and the old head keeps its tail
(s13, s4, s11, s13, …), until the stall rule stops it. The fault-free step
fitted because the layout the original writes left had small records in the
tails; the cut changed the layout (the attempt's garbage, a closed head) and
GC's whole-sector, in-order copies do not rebuild one as good. The packing
bound counts 4,072 B a sector, which next-fit compaction of near-1 KB records
cannot reach, so "will it fit" has two answers — the bound's and GC's — and at
the edge they disagree. (A second attempt after a remount sometimes fits
because the first attempt's collections and aborted records left another
layout.)

**Fix** — none yet; a ruling. Options, none tried to the end: (a) GC that
packs — fill the head's tail with the victim's records that fit before
opening a sector (best-fit within a victim). Tried as an experiment on the
first two of the four changes only: the two-seed set went from 6 + 0 to
9 + 9 failures and one `no_recovery`, because more steps then fit
fault-free at the edge (the edge is chaotic; counts there are not a quality
measure by themselves); not adopted. (b) A bound GC can always meet — for
instance count each record over a quarter sector as a third of one — at a
capacity cost near full. (c) A re-run that writes less: deduplicate against
the torn attempt's complete records, as the prototype did (README "Found
while building v1", item 3: v1 prunes them at mount for RAM); it needs room
asked for by id rather than by size, and mount to keep the attempt's
records. (d) Accept it: a store this full refuses the re-run until space is
freed.

**Regression coverage** — none yet. Replay: the command above (release,
lp-nor-sim, seconds).

**Lesson** — near full, "fits" depends on the layout, and a power cut
changes the layout without changing the live set. Any rule that admits by
the layout, and any GC that cannot reach the bound it admits by, will
disagree somewhere at the edge; only a cut walk at the edge finds where.

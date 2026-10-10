---
status: open      # accepted for the device round; revisit in the adoption round
found: 2026-10-10      # test (F1's verification, an in-crate cut walk near full, lp-nor-sim)
area: lp-tree-store `store_space.rs` `ensure_room` × `space_estimate.rs` `fits_after_compaction` (a store packed to the reserve)
class: split-source-of-truth
related:
  - docs/defects/2026-10-09-tree-store-rerun-after-a-cut-is-refused-at-the-edge.md
  - lp2025/2026-10-08-1017-tree-store-device-round
---
# Packed to the reserve, a re-run after a cut can still be refused where the step fitted

**Symptom** — after the fix for
`2026-10-09-tree-store-rerun-after-a-cut-is-refused-at-the-edge.md`, M3's
drivers run clean (`lp-store-bench mutants`' unmutated store: 0 failures in
every driver; 24 full-flash seeds: 0 of 8,959 cut cases), but a walk of the
same shape inside the crate still finds refused re-runs: 16 sectors filled
with copies of a small project until one is refused, then re-pushes and
one- or two-shader saves, each cut at 12 points under three tear models and
run again — 11 of 792 re-runs refused `NoSpace` over 4 seeds (lp-nor-sim).
The step had fitted fault-free; the store mounts at the old state (nothing
committed is lost); the re-run is refused.
Pinned by `edge_gc_tests.rs` `a_rerun_near_full_can_still_be_refused` (seed
1: the second edit, two shaders, cut at op 197 of 474).

**Root cause** — instrumented (every `ensure_room` printed its need, the
free count and the heads' room; every collection its victim): the fault-free
step and the re-run run the same sequence of write phases, and GC makes room
for each. The fault-free run's last record (41 B) fits the head's room
without GC. The re-run, starting from the layout the cut left, has GC
compact the store to within 37–40 B of full in every sector (all garbage
collected, tail-only compaction filling each head's tail) — and then its
last record is 41 B: no tail takes it, the head has 10 B, and opening a
sector would leave free under the reserve. Same live set, same records, a
different layout, and the step is 1–4 bytes too big for it. The packing
bound (a byte sum) admits the write; GC cannot place it: two answers to
"will it fit", and near full they disagree.

**Fix** — none, by ruling (DD34 of the tree-store device round: option (c),
accept for this round). At the reserve, a re-run after a cut may be refused
`NoSpace` until space is freed; nothing committed is lost. The status stays
`open`: accepted for the device round, to be revisited in the adoption
round. Recorded in the crate README's "Limits". The options considered:
(a) a re-run that writes less — deduplicate against the torn attempt's
complete records, as the prototype did (README "Found while building v1",
item 3: v1 prunes them at mount for RAM); it needs room asked for by id
rather than by size, and mount to keep the attempt's records; (b) admission
with slack — a write must leave a margin (a record's worth, say) past the
reserve, which a re-run may use, but nothing on flash says a write is a
re-run, so the margin binds every write alike and moves the edge rather than
removing it; (c) accept. The fault-free run is itself at the reserve with no
slack, so any rule that admits by the layout will meet a cut that leaves a
layout a few bytes worse.

**Future idea (for the adoption round)** — mount can see a torn commit:
CRC-good records after the newest root. The first commit after such a mount
could be allowed a one-record margin past the reserve that ordinary writes
are not — (b) without moving the edge for every write. It changes no format
(the margin is a rule in `ensure_room`/`fits_after_compaction`, keyed off
what mount saw), but the margin must come out of the reserve's slack, and the
test above is the first to turn.

**Regression coverage** — `a_rerun_near_full_can_still_be_refused` pins the
current behaviour (it asserts the refusal; when a fix makes it fail, the fix
closes this).

**Lesson** — near full, "fits" depends on the layout, and a power cut
changes the layout without changing the live set. A store that admits by
layout and a GC that can only approach its byte bound will disagree
somewhere at the reserve; only a cut walk at the edge finds where.

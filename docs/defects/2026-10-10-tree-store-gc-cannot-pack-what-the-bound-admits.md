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
are not — (b) without moving the edge for every write. It would change no format
(the margin is a rule in the store, keyed off what mount saw); not built or
measured. `a_rerun_near_full_can_still_be_refused` is the first test it
would turn.

**Regression coverage** — `a_rerun_near_full_can_still_be_refused` pins the
current behaviour (it asserts the refusal; when a fix makes it fail, the fix
closes this).

**More occurrences: M3's tight walks (2026-10-10)** — with the fix merged,
M3's long walks at 28–48 sectors (the edit mix, one corpus in all three
slots, a cut every 10 steps under `clean`, `byte_prefix`, `random_bits`
and `calibrated`) meet this refusal often where the walk sits at the
reserve: c13 at 48 sectors, **4 of 4 walks** stop at a refused re-run
(steps 420, 2,140, 3,350 and 7,080); c20 at 48, 2 of 4 (steps 60 and
90); c20 at 36, 1 of 4 (step 14,690); c13 at 28 and 36, 0 of 8 (100,000
steps each, about 55 % of steps refused before writing). `full-flash` at
48 sectors on c13: 25 of 7,320 cut cases over 8 seeds; the P5 re-race's
full-flash units: `t1` 6 of 1,564 at 176 sectors (0 of 1,449 at 128),
`t1@codec=host_deflate` 3 of 1,682 at 128 and 12 of 1,855 at 176. Every
one is `rerun_failed: NoSpace` with the committed state intact (some
`a second attempt after a remount: fits`, some `: NoSpace`). All
**lp-nor-sim simulator** figures. Twelve of them (the seven walk failures
and five full-flash ones) were replayed with a local, uncommitted build
that printed every `ensure_room` exit: in each, the packing bound admitted
the write, GC collected every victim with garbage, and then tail-only
compaction cycled over two or three sectors that hold no garbage (e.g.
s2, s21, s42, each 3,570 B live of 3,594 used) until the stall rule
stopped it, one sector short. Many of the cycling sectors hold three
full-size 1,024 B records (3,096 B used: header + 3 × 1,024), a 1,000 B
tail no full-size record fits, and the cold head's room at the exit is
1,000 or 3,048 B (4,072 − 3 × 1,024, 4,072 − 1,024): the byte-sum bound
counts tails that record granularity cannot fill. Replay (the first stops
at step 60, the second at step 420):
`cargo run --release -p lp-store-bench -- long --candidates t1 --sectors 48 --seeds 1 --first-seed 4 --steps 1000 --cut-every 10 --check-every 1000 --corpora c20 --tears clean,byte_prefix,random_bits,calibrated --edit-mix`
(and the same with `--corpora c13`). Not fixed here (M3 reports); the
rate is the new fact for the DD34 ruling, which rested on 11 of 792 in a
synthetic in-crate walk.

**Lesson** — near full, "fits" depends on the layout, and a power cut
changes the layout without changing the live set. A store that admits by
layout and a GC that can only approach its byte bound will disagree
somewhere at the reserve; only a cut walk at the edge finds where.

---
status: fixed      # fixed by a67a9f22b (admission by the live set with a packing slack; per-kind FFD bound; GC packs; refusal memory)
found: 2026-10-10      # test (F1's verification, an in-crate cut walk near full, lp-nor-sim)
area: lp-tree-store `store_space.rs` `ensure_room` × `space_estimate.rs` `fits_after_compaction` (a store packed to the reserve)
class: split-source-of-truth
fixed: a67a9f22b
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

**Fix (2026-10-14)** — first accepted for the device round (DD34, option
(c)); the director then asked for the fix (plan step F2,
`fix-near-full-fit.md`). The brief's theory — restore the
first-fit-decreasing bound P11 had replaced with a byte sum — was measured
first and was not enough on its own (lp-nor-sim, c13 at 48 sectors, 4
walks of 10,000 steps; full-flash 48/c13, 8 seeds):

| store | walks clean | full-flash failures |
|---|---:|---:|
| main (byte-sum bound) | 0 / 4 (stop at 421, 2,141, 3,351, 7,131) | 25 / 7,320 |
| FFD bound alone | 0 / 4 | 25 / 7,320 |
| FFD per kind | 1 / 4 | 25 / 7,320 |
| + GC packing (top-off, passes) | 2 / 4 | 0 / 7,349 |
| + admission slack (the fix) | 4 / 4 | 0 / 7,343 |

Two more mechanisms were behind the residual, found by instrumenting the
failures again:

1. **The hot head's room counted for cold records.** In the in-crate case
   the hot head held 42 B live and 3,568 B free; only hot records can use
   it, so the cold records were 169 B from full in 12 sectors, not 4 KB
   in 13. The bound is now first-fit-decreasing **per kind** (a sector
   holds one kind; each head writes its own; GC copies to the same kind).
2. **GC could not reach a packing the bound counted on.** Compaction moved
   one victim at a time and filled the head only from that victim, so a
   sector closed with whatever tail its own records left: three 1,024 B
   chunks and a 1,000 B tail no chunk fits, while the small records that
   could fill the tails sat in other sectors. GC now copies every victim
   largest-first and tops each head off before it closes with the largest
   live records of its kind from sectors the pass has not written; each
   sector is collected at most once a pass, and passes repeat while they
   shrink what the closed sectors waste (replacing the stall rule). That
   fixed full-flash; the walks still stopped where the store was packed
   to 99.8 % (176,014 + 2,750 B of cold records in 44 × 4,072 B: 404 B
   spare, 9 B a sector), which GC reached to within 20–30 B a sector but
   not 9.
3. **Admission by the layout cannot agree with itself at zero slack.** A
   write the layout happens to have room for was taken without asking
   any bound; the same write after a cut, in a layout a few bytes worse,
   needed GC to reach a packing with no slack at all. Admission now reads
   only the live set and the write, never the layout, on **every** write
   phase (`space_estimate::admits`): the live bytes of each kind plus the
   write's must fit the usable sectors less the reserve, each counted at
   its capacity less `PACK_SLACK` (64 B), the hot bytes in sectors of
   their own. It runs on the live upper bounds and marks only when they
   say no, so the fault-free step and its re-run get the same answer by
   construction, and GC has 64 B a sector to get there. (The DD34 ruling
   had set a margin aside — "nothing on flash says a write is a re-run,
   so the margin binds every write alike and moves the edge" — but a
   margin asked only when the layout lacks room binds a re-run and not
   its step; asked on every write it binds both alike, which is the
   point: it moves the edge 64 B a sector and removes the disagreement.)
   The edge moves by 64 B a sector (1.6 %); 16, 32 and 48 B also cleared
   the 10,000-step walks, and 64 B halved the erases a step costs there.

And the brief's second half: **a refusal is remembered** (RAM only). After
GC fails for a write, one needing as many bytes (live plus write) is
refused without GC until a write needs fewer or a sector more is free;
a mount forgets it. Retries no longer run GC each time.

No on-flash format change. Code: the RV32 size probe's `.text` (the C6's
flags) 42,892 → 45,302 B (+2,410 B; the brief guessed ≈ 350 B for the
FFD bound alone). RAM: one bit a sector resident (the head kind, which
GC used to read from each victim's header); transient, 2 B a live record
for the lengths the mark collects, twice while the bound sorts them.

**Verified (lp-nor-sim simulator, `lp-store-bench`, at a67a9f22b):**
long walks of 100,000 steps, edit mix, a cut every 10 under `clean`,
`byte_prefix`, `random_bits` and `calibrated`: c13 at 48 sectors 4 / 4
clean (before: 4 / 4 stopped by step 7,131); c20 at 48 4 / 4 (before 2
of 4 stopped, at steps 60 and 90); c20 at 36 4 / 4 (before 1 of 4); c13
at 36 and at 28 4 / 4 each (as before). Steps refused: 47–55 %. Full flash: 48/c13
0 of 7,343 cut cases (8 seeds; before 25 of 7,320); 128/c20 0 of 4,079
(4 seeds); the P5 re-race's units, `t1` at 176 0 of 2,191 (before 6 of
1,564) and `t1@codec=host_deflate` 0 of 2,263 at 128 and 0 of 2,366 at
176 (before 3 of 1,682 and 12 of 1,855). Wear under refusal churn (c13 at
36 sectors, seed 1, 100,000 steps, 54 % refused): the most erases on one
sector **92,352 → 8,546**, all erases 476,100 → 288,659. `lp-store-bench
mutants`: the unmutated store 0 failures in all seven drivers; 10 of 11
mutants caught — `skip_kill` survived this run (on main one wear-walk
failure caught it: a `byte_prefix` torn erase of a live header that
happens to leave the magic and a higher version byte, a rare bit pattern
whose chance moves with the GC's erase order; see the PR).

**Regression coverage** — `edge_gc_tests.rs`
`a_rerun_near_full_fits_where_the_step_fitted` (the in-crate walk above,
4 seeds, 12 cuts a step, three tears; on the old store it fails at the
case this defect pinned, seed 1, edit 1, cut 197 of 474) replaces
`a_rerun_near_full_can_still_be_refused`;
`a_refused_write_is_not_collected_for_again` pins the refusal memory;
`compaction_near_full_wins_the_tails_back` now asserts no edit is refused
after GC ran; `space_estimate.rs` tests the per-kind bound and the slack.

**What is left** — first-fit-decreasing is still asked only when the
layout lacks room, and it is not an exact packing: a write it refuses that
some layout holds would be refused after a cut where the step fitted. Not
seen in any run above (over these sizes FFD matches an optimum).

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
somewhere at the reserve; only a cut walk at the edge finds where. The cure
was to stop asking the layout: admit by the live set alone, with a slack
GC can always reach. And a brief's theory is a hypothesis: the restored
FFD bound alone moved none of the numbers it was meant to.

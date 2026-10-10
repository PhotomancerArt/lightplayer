---
status: fixed (e61488124, 4d02d2e91)
found: 2026-10-09      # test (M3 P2 long walk, lp-store-bench, lp-nor-sim)
area: lp-tree-store `store_space.rs` `ensure_room` (GC's stall rule) and GC's victim choice
class: budget-exhaustion
fixed: e61488124, 4d02d2e91
related:
  - docs/defects/2026-10-10-tree-store-gc-cannot-pack-what-the-bound-admits.md
  - lp2025/2026-10-08-1017-tree-store-device-round
  - docs/defects/2026-10-08-tree-store-root-sequence-does-not-wrap.md
---
# On a nearly full tree store, re-running a step after a cut is refused `NoSpace`

**Symptom** — `lp-store-bench long` (the M3 long walk: one T1 store,
128 sectors, the edit mix over c40 + c40reuse + c20 — the live set near
full) fails at step 2150, a re-push of slot `b`, cut at op 378 of 615
(`byte_prefix`): after the cut the store mounts at the old state (the cut
lost nothing), but re-running the step is refused —
`rerun_failed: step 2150 cut 378/615 byte_prefix: NoSpace`. The same step
on the pre-step flash, fault-free, succeeds (18 GC collections), and on the
cut flash it succeeds at the **second** attempt, after another mount. Every
figure is an **lp-nor-sim simulator** figure.

**Root cause** — `ensure_room` stops collecting after `reserve + 2`
collections in a row that do not raise the free-sector count above its best
so far (`stalls > self.cfg.reserve + 2`), meant to stop tail-only compaction
that cannot gain. The re-push writes a 38 KB file as 39 records of up to
1,040 B — three to a 4 KiB sector — so it needs 16 free sectors with the
reserve. After the cut the flash mounts with 15 free; GC collects 8 victims
without getting past 14 free, the stall rule fires, and the write is refused
although the packing bound admitted it. The failed attempt's collections
leave a better layout, so the next attempt (6 more collections) fits. Only
liveness is lost — no committed data: the store refuses a write it can hold
until it is asked again. Instrumented by hand while finding it (the GC exit
printed `stalls, free 14 best 14`); not fixed here (M3 reports; the store's
fixes go in their own PR).

**A second mechanism, the same symptom** — the full-flash driver
(`lp-store-bench full-flash`: fill a 16-sector T1 with unique copies until it
refuses, then saves, panel writes and re-pushes at the edge with sampled cuts)
meets `rerun_failed: NoSpace` far more often, and most of those stay refused
after another power cycle — `(a second attempt after a remount: NoSpace)` in
the failure's detail (run_case now tries once more, for the record). With the
stall rule disabled by hand (a local experiment, not committed) 8 of 687
full-flash cases (the mutants command's set) still fail this way, against 23
with it. Many are
cuts at a step's last op (the root's program) or a clean cut after a few of a
panel write's ops: the step fitted with almost no slack, the torn attempt's
records are garbage, and the re-run cannot get that room back. Not proven
which garbage is unreclaimable; the leading guess is the resumed head
sector's — `choose_victim` never collects a head, and the packing bound counts
that garbage as reclaimable. Seed 2 of
`cargo run --release -p lp-store-bench -- full-flash --candidates t1 --sectors
16 --corpus-name syn:3:900 --seeds 2 --edge-steps 24 --cuts-per-step 10
--tears clean,byte_prefix,random_bits,calibrated`: 15 `rerun_failed`, 14 of
them persistent (lp-nor-sim). In every case the committed state is intact
(old) and the store recovers once space is freed (`recovered true`).

**Fix** — `e61488124` and `4d02d2e91` (F1 of the plan, PR #1123). The
second mechanism, instrumented (every GC exit and every mount printed the
sectors' used/live bytes and heads, run against seed 2 above, lp-nor-sim):
of the 23 refused re-runs, 18 had the **hot head closed by the cut** — a
record cut inside the hot head (a clean cut between a record's header and
payload programs, or a torn root) fails its CRC, mount trusts nothing after
it, so the re-run needs a new hot sector where the step had used the head's
tail. Free sat at the reserve; GC collected the closed hot sector, copying its
1,676 live bytes (root, hot directory, panels) into the **cold** head, which
had 32 B left, so it opened a sector for the one it freed: free 3 → 3, and
the re-run still needed a fourth. Of the other five, four had the cold head
closed the same way, each cut after the attempt's own GC had opened a sector
and before it erased its victim (free 2 at the remount), and one was cut
between a victim's copies and its erase with both heads intact. And one of
the 18 never reached GC: the packing bound refused it (live 48,899 + 42 B
over its 48,864 B) on the live bytes the layout had taken before the cut. So the leading guess was half right: the
garbage that could not come back sat in heads — closed ones, which GC did
collect but at the price of a sector, and the open hot head, which it never
collected (the full-flash `no_recovery` case: a hot head of 4,045 B, 1,946
of them old roots, room for 27 B, a 42 B root refused fault-free). Four
changes, each pinned by a test: (1) `ensure_room` collects every victim with
garbage and counts a stall only for a tail-only collection (the defect's
first candidate); (2) `gc_copy.rs` copies a victim's records to the head of
its own kind (read from its sector header), so collecting a closed hot
sector gives a hot head its room back for no sector; (3)
`fits_after_compaction` is usable sectors less the reserve, no longer one
more for the second head, so it never rejects what a layout holds (a cut
leaves the live set, so the bound, as it was); (4) GC renews a head — its
live records to a new head of its kind — when the head's own garbage would
let the write open fewer sectors. No format change. +454 B of `.text` on
the size probe with the C6's flags (42,210 → 42,664; with `lpfs` 57,268 →
57,690), `.rodata` unchanged.

After the fix, both replays (M3's drivers on a local merge, lp-nor-sim): the
long walk above runs its 2,151 steps with 0 failures and 0 refusals (was
`rerun_failed` at step 2150); the full-flash seed-2 run has 2 `rerun_failed`
of 377 cases, neither persistent (was 15 of 322, 14 persistent);
`lp-store-bench mutants`' unmutated store fails 8 of 756 full-flash cases
(was 23 of 655) and nothing in any other driver. What is left is a narrower
cause with the same symptom, filed on its own:
`2026-10-10-tree-store-gc-cannot-pack-what-the-bound-admits.md`.

**Regression coverage** — `lp-base/lp-tree-store/src/edge_gc_tests.rs`:
`spread_garbage_never_refuses_then_accepts` (1: thin garbage over every
sector; a refusal is final after a remount), `a_rerun_after_a_cut_fits_where_the_step_fitted`
(2, 3: a full store, panel writes cut at every op, clean and torn, every
re-run fits) and `a_hot_head_full_of_old_roots_is_renewed` (4). Each fails
with its change taken out alone. Synthetic shapes on 16 sectors rather than
the long walk's prefix (2,150 steps on 128 sectors over the c40 corpora is
neither fast nor in this crate). Replays as above.

**Lesson** — a "give up when it stops improving" heuristic in GC is a
liveness promise the cut sweeps never test, because their workloads never
fill the flash (the c40 workloads peak at 71 of 128 sectors). Only a walk
that keeps the live set near full, and cuts it, meets it.

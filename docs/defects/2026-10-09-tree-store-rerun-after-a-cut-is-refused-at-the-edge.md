---
status: open
found: 2026-10-09      # test (M3 P2 long walk, lp-store-bench, lp-nor-sim)
area: lp-tree-store `store_space.rs` `ensure_room` (GC's stall rule) and GC's victim choice
class: budget-exhaustion
related:
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

**Fix** — none yet. Candidates for the store's PR: count a stall only when a
collection frees nothing it could have (not when the free count merely
fails to beat its best), or keep collecting while the packing bound says the
write fits and victims with garbage remain, with the loop's
`sector_count * 4` bound as the backstop.

**Regression coverage** — none yet. Replay (lp-nor-sim, release, ~25 s):
`cargo run --release -p lp-store-bench -- long --candidates t1 --seeds 1
--first-seed 2 --steps 2151 --edit-mix --corpora c40,c40reuse,c20` →
`FIRST rerun_failed: step 2150 cut 378/615 byte_prefix: NoSpace`. The store
fix should add a test that refuses-then-accepts no longer happens on this
shape (the walk's prefix is `driver_long::long_walk_prefix`).

**Lesson** — a "give up when it stops improving" heuristic in GC is a
liveness promise the cut sweeps never test, because their workloads never
fill the flash (the c40 workloads peak at 71 of 128 sectors). Only a walk
that keeps the live set near full, and cuts it, meets it.

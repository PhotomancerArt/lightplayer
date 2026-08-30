---
status: diagnosed
found: 2026-08-29
diagnosed: 2026-08-29
area: per-tick sample-out clone vs inbound-request transients on the classic heap
related:
  - 2026-08-29-shader-jit-compile-transient-starves-classic-heap.md
  - 2026-08-29-load-project-resets-instead-of-refusing.md
  - ../adr/2026-08-28-project-reads-bounded-streamed-refusable.md
---
# "Flash writes wedge under zook playback" — actually per-tick 12 KB sample-out clone OOM

**Original shape (G1 walk)** — with `/projects/zook-dome` playing, a
4.6 KB `FsRequest::Write` got no response in 20 s, a 12 KB one none in
40 s (C3/C3b FAIL), `loadProject` none in 60 s; link counters zero,
C4 pings fine, identical writes instant when idle or under the flat
small-dome. Suspected littlefs/`with_app_core_stalled` vs multi-wire
RMT.

**Diagnosis (bring-up bench, same board)** — every prior suspect is
exonerated. There is no wedge: **each fs write under zook costs the
board two OOM resets in ~2 s**, after which auto-load restores zook
and heartbeats resume — client-side indistinguishable from a hung
request. Caught live at 19:49:31–33 with the lab buffer:

    write frame (4,841 B) hits the wire
    ====== OOM ======  alloc 12000 B align=2 failed, largest_free=11704
    reset → auto-load zook → hello → first frame served
    shader compile starts → OOM 384 B → reset → red gate → stable black

The 12,000 B backtrace (decoded on the flashed ELF):

    alloc::raw_vec — 12,000 B ask
    LpvmGraphics::read_sample_out          ← `.data().to_vec()`
    FixtureNode::render_control
    …resolver…
    OutputNode::consume                    ← the TICK path, not the write

`read_sample_out` (lp-gfx/lp-gfx-lpvm/src/lpvm_graphics.rs:302) clones
the full sample surface — zook: 1,500 lamps × RGBA16 = 12,000 B —
**fresh from the heap every tick**. Steady-state it recycles the same
hole (zook: largest_free ≈ 17.7 KB while playing). An inbound
request's transients (line buffer + parsed `data` Vec + response) push
largest-free below 12,000 for one tick, and the infallible alloc
resets the board. The write itself COMPLETED (the target file carried
the new payload across the reset); the tick after it died.

This explains every observation at once:
- idle → no ticks → writes instant (C5);
- flat small-dome → fixtures missing → no sample-out clone → C3 passed;
- zook → every tick needs a 12 KB block on a ~17 KB-largest heap;
- small requests (C4 pings, setLogLevel, even a 4.6 KB read) leave
  largest-free ≥ 12,000 → answered in ~0.5 s under full zook load;
- `loadProject` under zook: load transients trip the same tick OOM,
  the reset auto-loads *zook* again → "no response";
- counters zero: every boot zeroes them, and nothing was ever dropped —
  the board just wasn't the same board anymore;
- wire count was a confound: 5-wire zook vs 2-wire flat small-dome
  differed in *fixture heap pressure*, not RMT behavior.

**Fixes, in leverage order**
1. `read_sample_out` should not clone per tick: hand out a borrow or
   copy into a persistent caller buffer in FixtureNode. Also a perf
   win at every scale (big dome: ~240 KB/frame of memcpy+alloc churn).
2. Tick-path allocations this size should be fallible → skip/degrade
   the frame, never `stage_oom_and_reset` (same contract direction as
   D7 refusal-not-reset).
3. `set_oom_context` on tick/render entry — this OOM attributed to
   `node:/zook_dome.sho` with `context=<unset>`, which pointed at the
   shader for a fixture-path alloc.

**Workaround** — unchanged and now explained: `stopAllProjects` before
any write/load against a playing classic (no ticks → no 12 KB ask).

**Diagnostics kept** — fw-esp32v3 `[FLASH]` write/erase trace pairs and
the lpa-server fs-write dispatch/handled markers (both debug-level, arm
via `setLogLevel` — the wire wants `"Debug"`, PascalCase) remain useful
for any future genuinely-flash-side suspicion.

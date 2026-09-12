---
status: fixed
found: 2026-08-29
diagnosed: 2026-08-29
fixed: 2026-08-29
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

**Fixes, in leverage order** — all three landed 2026-08-29 (the
`read_sample_out_into` contract change):
1. ~~`read_sample_out` should not clone per tick~~ **DONE**:
   `LpGraphics::read_sample_out_into(out, &mut [u16])` is the frame-path
   read — exact-length copy into a persistent caller-owned buffer.
   `FixtureNode`'s Direct render, the playlist crossfade, and the shader
   projected-texture fill each hold one; the `Vec`-returning
   `read_sample_out` survives as a default-method convenience for
   tests/tooling only. Also the perf win at every scale (big dome:
   ~240 KB/frame of memcpy+alloc churn gone on every backend).
2. ~~Tick-path allocations this size should be fallible~~ **DONE** for
   the new scratches: they grow through `try_reserve`
   (`lpc-engine::node::ensure_scratch_len`), so an allocation failure
   skips the frame with a `NodeError` instead of staging a reset.
   ~~Remaining exposure: the fixture 1D/TextureArea path still
   materializes an owned `TextureData` per frame via `read_back` (not
   on zook's Direct path); same treatment needs a `read_back_into`
   contract.~~ **DONE** (PR #475): `LpGraphics::read_back_into` fills a
   persistent `FixtureNode` scratch sized via the same fallible
   `ensure_scratch_len`; both TextureArea paths use it.
3. ~~`set_oom_context` on tick/render entry~~ **DONE**: `Engine::tick`
   and `render_texture_product` set a context on entry, so a tick OOM
   attributes to the tick rather than the last-set request scope.

**Workaround** — unchanged and now explained: `stopAllProjects` before
any write/load against a playing classic (no ticks → no 12 KB ask).

**Diagnostics kept** — fw-esp32v3 `[FLASH]` write/erase trace pairs and
the lpa-server fs-write dispatch/handled markers (both debug-level, arm
via `setLogLevel` — the wire wants `"Debug"`, PascalCase) remain useful
for any future genuinely-flash-side suspicion.

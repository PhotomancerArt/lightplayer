---
status: open
found: 2026-08-29
area: fw-esp32v3 flash writes (littlefs / with_app_core_stalled) vs multi-wire RMT playback
related:
  - ../adr/2026-08-25-classic-uart-io-task-executor-isolation.md
  - ../adr/2026-08-28-project-reads-bounded-streamed-refusable.md
---
# Filesystem writes and loadProject wedge while zook-dome plays on the classic

**Shape** — G1 bench walk (2026-08-29, dig2go, wire-evolution round-1
firmware): with `/projects/zook-dome` playing (43 ms ticks), a 4.6 KB
`FsRequest::Write` gets NO response in 20 s and a 12 KB one none in
40 s (starvation-bench C3/C3b FAIL); `loadProject` against the playing
board also went unanswered in 60 s. The link itself is healthy under
the same load: C4 passed 10/10 round-trips (median 0.32 s), and the
new heartbeat `link` counters stayed at zero — the frames arrive and
parse; the *handler* never completes. Under `small-dome` playback
(7 ms ticks, 2 output wires) the identical 4.6 KB write completes in
0.3 s, and idle writes are instant (C5 PASS).

**Discriminators** — not transport (C4/counters); not our branch
(PR #458 touches no flash or RMT code; C3 PASSED on the same board
3 days earlier under the #448 firmware with the heavier 111 ms
"studio" project). The variable is the project's output config: zook
drives 5 repeat instances across physical channels. Suspect: littlefs
flash ops run under `with_app_core_stalled` + esp-storage's masked
ROM windows; with many active RMT wires the stall handshake (or the
refill/doorbell storm around it) may never reach a safe window, so
the write spins while the tick loop waits on the sync handler.

**Workaround** — `stopAllProjects` before any flash write or project
load on a playing classic (the pre-#448 lore, apparently still binding
for FLASH ops even though the transport no longer needs it).

**Next** — reproduce with instrumented timing around
`with_app_core_stalled` under zook vs small-dome; check whether the
wedge is a livelock (never returns) or extreme slowness; bisect wire
count (2 vs 5 outputs) on the same project.

## Diagnosed 2026-08-29 (bench walk follow-up): not a flash stall — an OOM reset loop

The "wedge" was the classic silently resetting, twice, and auto-reloading:
`LpGraphics::read_sample_out` returned `.data().to_vec()` — a fresh heap
clone of the full sample surface — and `FixtureNode`'s Direct render path
called it **every tick** (zook: 1,500 lamps × RGBA16 = 12,000 B allocated
and freed per frame). Any inbound request whose transients pushed
largest-free below that hit the infallible-alloc handler →
`stage_oom_and_reset`; the second reset (shader-compile OOM on auto-load)
plus auto-reload made the sequence read as an unresponsive handler. The
OOM breadcrumb also misattributed: the context was whatever scope the last
request handler set, never the tick.

**Fixed** (same date, `read_sample_out` contract change):
- `LpGraphics::read_sample_out_into(out, &mut [u16])` is now the frame-path
  read — callers copy into a persistent caller-owned buffer (`FixtureNode`,
  the playlist crossfade, and the shader projected-texture path each hold
  one). The `Vec`-returning `read_sample_out` remains as a default-method
  convenience for tests/tooling only. At big-dome scale this also removes
  ~240 KB/frame of alloc+memcpy churn on every backend.
- The scratch buffers grow through `try_reserve`
  (`lpc-engine::node::ensure_scratch_len`): a tick-path allocation failure
  now degrades to a skipped frame (`NodeError`) instead of staging a reset.
- `Engine::tick` / `render_texture_product` set an OOM context on entry, so
  a tick-path OOM attributes to the tick rather than the last-set scope.

Remaining exposure: the fixture 1D/TextureArea path still materializes an
owned `TextureData` per frame via `read_back` (not on zook's Direct path);
same treatment would need a `read_back_into` contract.

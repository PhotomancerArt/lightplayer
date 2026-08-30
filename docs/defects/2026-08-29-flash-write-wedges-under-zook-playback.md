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

**2026-08-29 follow-up (bring-up session, same board/firmware)** — the
4.6 KB write got no response in **120 s** while zook played
(shader red-gated, black output, 5 wires live) — and the whole time
heartbeats kept arriving on schedule and `frame_count` kept climbing
at a steady 22.85 fps. **The tick loop is NOT blocked**, which
falsifies the "tick loop waits on the sync handler" picture above: a
handler stuck inside `with_app_core_stalled` would stall the whole
server loop, heartbeats included. The request dies somewhere between
io_task frame assembly and handler dispatch (or its ack dies on a path
heartbeats don't share). Small C4-shaped requests round-trip under the
identical load, so the discriminating variable is inbound frame size
(4.6 KB ≈ 50 ms of line time > one 43 ms tick) — yet link counters
stayed zero and the same-size write completes under small-dome ticks.
`fw-esp32v3` now carries debug-level `[FLASH]` start/end trace pairs
(flash_storage.rs, armed via `setLogLevel debug`): no `[FLASH]` lines
during a wedged write = the op never reached littlefs; start-without-
end = stall window; endless pairs = a loop above storage. A useful
extra probe: a debug line at fs-request dequeue, to split "never
dispatched" from "response lost". Note `PUSHER_CAP = 4`
(wire_pusher.rs) vs zook's 5 wires — the 4-vs-5 bisect sits exactly on
that boundary.

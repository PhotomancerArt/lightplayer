---
status: fixed
found: 2026-08-29
fixed: 2026-08-29
area: lpa-server LoadProject vs classic ESP32 heap; found during classic bring-up
related:
  - ../adr/2026-08-28-project-reads-bounded-streamed-refusable.md
  - 2026-08-26-project-read-assembly-oom-resets-classic.md
---
# loadProject has no headroom gate — an unaffordable project OOM-resets the device

**Shape** — Reads got refusal-not-reset (ADR
`2026-08-28-project-reads-bounded-streamed-refusable`, D7: a read the heap
cannot afford fails with a structured error behind a
`PROJECT_READ_MIN_HEADROOM_BYTES` = 32 KiB largest-free-block gate), but
`LoadProject` kept the old posture: no pre-gate, infallible allocs all the
way down, so a project the device cannot afford abort-resets the board.
The wire sees a silent reboot; from Studio it masquerades as a hang.

**Verified on silicon** — loading full `examples/small-dome` (5,950 lamps)
on the classic dies in `lpc_mapping::map2d_resolve::resolve` on a 64 KiB
Vec-doubling ask with `largest_free=24576`, silently rebooting the board.
The lamp list grows `push`-by-`push`: the 2048→4096-entry capacity doubling
asks for one 64 KiB block even though the document declares its counts up
front and the whole list is ~93 KiB (16 B/lamp) — the doubling schedule
peaks well past the real need.

**Fix (both halves shipped together):**

1. **Load gate** — `check_load_headroom` in `lp-app/lpa-server/src/server.rs`
   refuses a `LoadProject` when the largest free block is under
   `PROJECT_LOAD_MIN_HEADROOM_BYTES` (64 KiB, documented like the read
   gate's floor), with a structured error naming the free bytes and the
   remedy. It runs on BOTH load paths — the wire handler
   (`handle_load_project`, gated after the unload so the probe reads the
   heap the load actually runs in) and the host-call path
   (`LpServer::load_project`), which boot-time startup loads use — so an
   unaffordable startup project boots to an idle server instead of a reset
   loop. Same probe as the read gate; hosts/browser (no probe) are never
   refused. Pinned by `lp-app/lpa-server/tests/project_load_refusal.rs`.
2. **Exact-size lamp list** — `lpc_mapping::map2d_resolve::resolve`
   pre-sizes the lamp Vec from the document's declared counts
   (`shape_lamp_count` summed over objects — it mirrors the resolver
   exactly, pinned by test), one exact contiguous ask instead of a doubling
   schedule; `resolve_ring`'s position list got the same treatment.

The gate is a floor, not a fit check: a big-enough project can still OOM
past it (small-dome's ~93 KiB lamp ask clears a 64 KiB gate on a 70 KiB
heap). Refusing everything the device could never load — and making the
asks it does make honest — is the shipped scope; per-project cost
prediction is not possible before parsing, which is itself the allocation
being guarded.

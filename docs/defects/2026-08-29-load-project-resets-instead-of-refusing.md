---
status: fixed
found: 2026-08-29
fixed: 2026-08-30
area: lpa-server project load vs the D7 refusal contract (classic-first, all chips)
related:
  - ../adr/2026-08-28-project-reads-bounded-streamed-refusable.md
  - 2026-08-26-project-read-assembly-oom-resets-classic.md
---
# loadProject OOM-resets the device where reads refuse

**Shape** — classic bring-up bench (2026-08-29, dig2go, wire-evolution
round-1 firmware + full small-dome tree on flash): `loadProject
/projects/small-dome` against an idle, empty device produced no response
for 30 s, then the heartbeat showed `uptime_ms=35001` — the board had
OOM-reset mid-load. The recovery ledger names it precisely:

    allocation failed: requested=65536 align=4 free=57644 used=128724
    largest_free=24576 retry_ok=false context=project new: load core project

Backtrace (decoded against the flashed ELF):

    alloc::raw_vec::grow_one                     ← Vec doubling to 64 KiB
    lpc_mapping::map2d_resolve::resolve
    lpc_engine::…::mapping_from_map2d_doc
    ProjectLoader::attach_projected_nodes_filtered
    ProjectLoader::load_project_artifact
    ProjectManager::load_project

**Two distinct problems:**

1. **No headroom gate on load.** ADR 2026-08-28 gives *reads* the
   refusal contract (unaffordable → structured terminal error, 32 KiB
   largest-free-block floor). `loadProject` has no equivalent: an
   unaffordable project aborts the allocator and resets the board. The
   recovery ledger then yellow-flags the project path, but the client
   only ever sees silence + a reboot. Load should refuse with the same
   shape ("free=N; project needs more") — it runs on the same path that
   already fetches `memory_stats`, and a conservative pre-gate (largest
   free block vs a load floor) would convert the reset into an error
   frame. Per-allocation refusal inside the loader is harder (deep
   infallible-alloc call tree) but the entry gate alone would have
   caught this case: largest block was 24,576 B against a 64 KiB ask.

2. **map2d resolve doubles past its known size.** The 64 KiB ask is a
   `Vec::grow_one` doubling while resolving the dome map2d — 5,950
   lamps whose count the document declares up front (the 64 KiB ask is
   the 2048→4096-entry doubling at 16 B/lamp; the whole list is
   ~93 KB). `with_capacity` at the declared count makes the peak ask
   the exact size needed rather than a doubling schedule peaking at
   128 KiB. (Not sufficient here — full small-dome is out of the
   classic's envelope regardless — but it shrinks the transient
   contiguous ask for every marginal load on every chip.)

**Envelope note** — full small-dome (50×119 + door: 128×128 canvas,
6,310 lamps) exceeds the classic's ~186 KB arena by construction:
~128 KB consumed before the lamp-list ask, with the 64 KiB canvas
still to come. The classic bench project remains zook-dome / mini-dome;
small-dome full scale is S3-class. The refusal gate (problem 1) is what
turns that fact from a reset into an error message.

**Repro** — push `examples/small-dome` (branch
`claude/mini-dome-led-accuracy-5a5d17`) to `/projects/small-dome` via
`spikes/serial-lab/scripts/push-dir.py`, then `loadProject` it on a
dig2go classic. 100% reset, idle or playing.

**Fix (2026-08-30, PR #473, both halves together):**

1. **Load gate** — `check_load_headroom` in
   `lp-app/lpa-server/src/server.rs` refuses a `LoadProject` when the
   largest free block is under `PROJECT_LOAD_MIN_HEADROOM_BYTES`
   (64 KiB, documented like the read gate's floor), with a structured
   error naming the free bytes and the remedy. It runs on BOTH load
   paths — the wire handler (`handle_load_project`, gated after the
   unload so the probe reads the heap the load actually runs in) and
   the host-call path (`LpServer::load_project`), which boot-time
   startup loads use — so an unaffordable startup project boots to an
   idle server instead of a reset loop. Same largest-free-block probe
   as the read gate; hosts/browser (no probe) are never refused. Pinned
   by `lp-app/lpa-server/tests/project_load_refusal.rs`.
2. **Exact-size lamp list** — `lpc_mapping::map2d_resolve::resolve`
   pre-sizes the lamp Vec from the document's declared counts
   (`shape_lamp_count` summed over objects — it mirrors the resolver
   exactly, pinned by test), one exact contiguous ask instead of a
   doubling schedule; `resolve_ring`'s position list got the same
   treatment.

The gate is a floor, not a fit check: a big-enough project can still
OOM past it (small-dome's ~93 KiB lamp ask clears a 64 KiB gate on a
70 KiB heap — the envelope note stands). Refusing every board that
could never succeed — and making the asks the loader does make honest —
is the shipped scope; per-project cost prediction is not possible
before parsing, which is itself the allocation being guarded.

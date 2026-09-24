# ADR: ESP32-C6 RAM split — a 72 KB main stack, and the heap reclaims the bootloader's 64 KB

- **Status:** Accepted
- **Date:** 2026-09-02
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The C6's 512 KB of SRAM reaches the app as one `RAM` region of
0x6E610 B (esp-hal `memory.x`) plus `dram2_seg`, the 64 KB
(0x4086E610..0x4087E610) the ESP-IDF second-stage bootloader used as its
loader segment and never touches once the app runs. The firmware placed
its whole heap — `esp_alloc::heap_allocator!(size: 300_000)` — in `RAM`
as a `.bss` array, and the main task's stack is whatever `RAM` has left
above `.bss`: **32,776 B**. `dram2_seg` went unused.

That split held only while the flagship example's compute node never
ticked. The 2026-09-01 bench (defect
`2026-09-01-hir-place-clones-exhaust-c6-heap-at-compute-compile`) fixed
the compile-time OOM that had quarantined meteor's sim node on every
boot, and the very first steady-state run of the real workload overran
the stack: esp-rtos caught `sp` 304 B below `_stack_end` inside the
resolver chain under `ComputeShaderNode::produce` (four demand levels of
`resolve_interned`/`produce_through_host`), and a second overflow
followed on the boot path. A stack overflow on this layout writes into
the top of the heap array — silent corruption until the scheduler
happens to sample the pointer.

Nothing measured stack use. The heap had `[mem]` brackets at every
seam; the stack had an overflow detector and no watermark.

## Decision

1. **Heap in main RAM: 300,000 → 260,000 B.** The main stack grows from
   32,776 B to **72,768 B**.
2. **A second `esp_alloc` region of 65,536 B in `dram2_seg`**, declared
   `#[esp_hal::ram(reclaimed)]` (the attribute esp-hal ships for exactly
   this memory; `esp-bootloader-esp-idf` enables it). Heap total
   **325,536 B**, up from 300,000. `HEAP.free()`/`used()` sum the two
   regions; allocations fill main RAM first.
3. **The stack is measured.** `fw-esp32c6::stack_probe` paints the main
   stack at boot (above esp-hal's guard word — painting over it is
   itself reported as an overflow) and the heartbeat logs
   `[stack] high-water N B of M B` whenever the mark grows. The number
   lives in the bench journal next to the `[mem]` lines.

Measured on the XIAO ESP32-C6 with meteor (firmware `4e463d805743`,
bench 2026-09-02):

| | before (300 KB heap) | after |
|---|---|---|
| main stack | 32,776 B | 72,768 B |
| stack high-water, meteor steady state | overflow (≥ 32,776 + 304 B) | 36,936 B (35,832 B headroom) |
| heap free at boot | 239,504 B | 265,040 B |
| heap free after project load | 194,848 B | 220,384 B |
| heap free after both shader compiles | 128 B short (OOM), then ~125 KB once the compile fix landed | ~150 KB |

Both paths (push from Studio, boot auto-load) run identically: 26 fps,
no reset over a multi-minute soak, ledger green.

## Consequences

- Meteor — and any project whose tick recurses through a few demand
  levels — has a stack margin that is a number, not a hope. The old
  layout was ~4 KB short in steady state for the flagship example.
- Heap headroom after compile went from nothing to ~150 KB on the
  flagship example without giving up a byte of usable RAM; the 64 KB
  region was idle.
- Two heap regions means one large allocation cannot span them: a
  single request above the main region's largest free block falls
  through to the 64 KB region, and above 64 KB fails even when the
  summed `free()` says otherwise. `largest_free_block` (the headroom
  probe) already answers the honest question, and the load-headroom
  gate reads that, not `free()`.
- `[stack]` lines are C6-only for now; the classic (`fw-esp32v3`) and
  the S3 keep their own splits and have no probe. Port the probe before
  trusting either board's stack margin.

## Amendment (2026-09-24): the main heap region gives 24,000 B to the stack for BLE

Linking BLE (`docs/adr/2026-09-24-ble-transport.md`; the `ble` feature is in
`default`, so every C6 carries it whether or not its device store enables it)
moves 36,000 B of static RAM below the main stack: the controller blob's
IRAM-placed link-layer code, its statics, trouble-host's packet pool and the
task pools. The stack fell from 71,152 B to 38,680 B (after moving the BLE
host state to the heap), and meteor's steady-state high-water on the emulator
was ~35.5 KB — 3 KB of margin where this ADR bought 35 KB.

**Decision (Yona, 2026-09-24): cut the heap, one image for every device** —
"a stack overflow crashes; a smaller heap only narrows the compile margin".
The rejected alternative was a separate BLE image, which reverses `ble` in
`default` and doubles what a board can be flashed with.

1. **Heap in main RAM: 260,000 → 236,000 B.** The `dram2_seg` region is
   unchanged at 65,536 B. Heap total **301,536 B** (was 325,536 B).
2. The main stack is **62,664 B** with BLE linked.

Measured on `lp-emu:esp32c6:t1` (the shipped image, BLE linked, disabled —
the emulator never enables it), 2026-09-24:

| | before the cut (BLE linked) | after |
|---|---|---|
| main stack | 38,680 B | 62,664 B |
| stack high-water, idle (first heartbeat) | 12,916 B | 12,916 B |
| stack high-water, meteor steady state (~200 s) | 35,496 B (3,184 B headroom) | 33,896 B (28,768 B headroom) |
| heap free at boot (first heartbeat) | 264,932 B | 240,932 B |
| free before the shader compile, PLAYFUL Choker | — | 136k free / 157k used |
| free before the shader compile, Zook dome | — | 155k free / 138k used |

The "before" meteor figure is PR #810's earlier emulator run; the two meteor
runs are different uploads of the same project, so read the high-water as
~34–35.5 KB, not as the cut having moved it.

With BLE **enabled** the controller and host take a further ~24 KB of heap
at 0 connections and ~3.5 KB per connection (silicon, the transport ADR), so
a BLE-enabled board's compile margin is that much smaller again. The
emulator numbers above are the BLE-disabled board.

**Placement (2026-09-24, same day).** The cut exposed a contiguity cost the
table above does not show. A BLE-enabled board refused choker → zook with
212 KB free, because the load gate reads the largest block and only the main
region can reach 64 KiB. The fix is placement, not size
(`docs/defects/2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut.md`).
The radio blobs' C heap (`malloc` and friends, `lp-fw/fw-esp32c6/src/c_heap.rs`)
fills the reclaimed `dram2_seg` region first. That region can never pass
the gate, so radio blocks there cost the gate nothing, and the main region
is left to Rust. With BLE up on silicon, 44,584 B of `dram2_seg` holds radio
allocations. Allocations that a project outlives no longer keep memory they
grew during the project.

## Alternatives Considered

- **Grow the stack by shrinking the heap alone (no dram2).** Loses
  40 KB of heap the flagship needs no longer but the big dome will; the
  reclaimed region is free RAM with an attribute already designed for
  the purpose.
- **Put the whole heap in a bigger single region.** There is no such
  region: `RAM` and `dram2_seg` are separated by the bootloader's
  reserved layout, and esp-alloc's multi-region heap is the supported
  way to use both.
- **Shrink the tick's stack use instead.** Worth doing when a profile
  says where the 37 KB goes (the resolver's per-level frames are the
  suspect), but the right first move for a corruption-class failure is
  margin, measured; the probe is what makes the follow-up honest.

## Follow-ups

- Profile the tick's stack consumers once the `[stack]` mark is in a
  few more bench journals (big dome, playlists, fluid).
- Port `stack_probe` to the Xtensa firmwares.
- The defect entry
  `2026-09-01-hir-place-clones-exhaust-c6-heap-at-compute-compile.md`
  records the compile-side half of this bench.

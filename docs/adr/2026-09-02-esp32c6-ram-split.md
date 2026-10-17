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

## Amendment (2026-10-01): ESP-NOW asks for lean Wi-Fi driver buffers

The radio's C heap fills `dram2_seg` first (Placement, above). The largest
tenant there is the Wi-Fi driver. The product brings that driver up only for
ESP-NOW and never joins a network, but it used esp-radio's
`ControllerConfig::default()`. That default is sized for a station moving
bulk traffic: 10 static RX buffers, 32 dynamic RX, 32 dynamic TX and a
block-ack window of 6.

**Decision:** every ESP-NOW bring-up uses
`espnow_controller_config()`, with 4 static RX, 8 dynamic RX, 8 dynamic TX
and a window of 3 (`lp-fw/fw-esp32c6/src/hardware/espnow_controller_config.rs`).
The stress builds, the desk meter and the BLE coexistence harness all mirror
the product's bring-up, so they call the same function.

| | default | lean |
|---|---|---|
| radio `dram2_seg` use at `wifi::new`, silicon (XIAO C6, PLAYFUL Choker, 2026-10-01 bench) | 30,404 B | 20,084 B |
| heap used at first heartbeat, `lp-emu:esp32c6:t1`, shipped image | 96,628 B | 86,308 B |
| image size | 2,960,400 B | 2,960,400 B |

The two −10,320 B figures agree to the byte. On silicon, a joined station
showed no change in throughput or round-trip time; the render loop bounds
both. The bench report is in the planning workspace at
`lp2025/2026-10-01-0300-wifi-control-experiments`.

**What it gives up:** the RX DMA ring is 4 descriptors instead of 10. If
more than four frames arrive before the Wi-Fi task drains the ring, the rest
are lost. On the emulator, the two-board `espnow_broadcast_pair` runs
identically on both configs. The `air_delivery` ring test now pins 4
delivered and 8 dropped out of 12 undrained frames. ESP-NOW *traffic* has not
been run on silicon with the lean config; the desk meter
(`desk_espnow_meter`, PR #890's comment) is the check if a loss question
comes up.

**Revisit when Wi-Fi joins a network for real** (LAN or relay control, the
flash budget ADR's radio day). A station carrying a WebSocket might want
deeper RX buffers than ESP-NOW does. The bench measured no loss of
throughput at these counts, but that was with frame-bound traffic.

## Amendment (2026-10-05): the radio's C heap moves out of `dram2_seg`

The Placement amendment above put the radio blobs' C heap in `dram2_seg`
first. That was wrong, and the reason sits in this ADR's own Context:
`dram2_seg` is free only "once the app runs". After any reset, the
second-stage bootloader loads its code into that range again and runs its
stack there. Both bootloaders we ship load into 0x4086B910..0x40876F20,
and their stack sits at the top of `dram2_seg`; the crash's SP was
0x4087E1D0. A warm reset resets only the HP system: the host's RTS
(`rst:0x15`) or a requested reboot (`rst:0x3`). The Bluetooth controller
lives in the modem, so it keeps running, and its DMA wrote into the
bootloader's image-hash check. On silicon that was an illegal instruction
on 11 of 22 warm resets with Bluetooth on, and 0 of 20 with it off
(`docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md`).

**Decision:** `dram2_seg` holds nothing that a radio can DMA into.

1. **The radio gets its own region: 49,152 B (48 KiB) of `.bss` in main
   RAM**, `HEAP_RADIO`, carved from the main heap region (236,000 →
   186,848 B). It ends at 0x408268E0 in this image, far below the lowest
   bootloader load address. The C heap asks for it first, by capability
   tag. When it is full, a block falls back to the main region, and **never**
   to `dram2_seg`.
2. **`dram2_seg` carries no capability tag**, so only a request that asks for
   no capability can land there. Only Rust's global allocator asks for
   none; esp-radio's own `InternalMemory` asks for `Internal` and so cannot
   land there. Rust's allocator tries the regions in this order: main,
   `dram2_seg`, then the radio region as the last resort.
3. **Every reset the firmware makes holds the modem's blocks in reset
   first** (`MODEM_SYSCON.MODEM_RST_CONF`, `board::esp32c6::restart`). That
   stops the radio's DMA at the source on resets the firmware makes itself.
   This matters when the next boot lays RAM out differently: after an
   update, or with Bluetooth switched off. A host RTS reset never runs that
   code, which is why placement is the fix and this is the second half.

The size comes from the boot. With Bluetooth up, the radio's high-water is
35,660 B on `lp-emu:esp32c6:t1` and 35,648 B on silicon (the bench C6,
advertising, no central connected). The `[radio-heap]` heartbeat line
reports it, the same way `[stack]` reports the stack. Each of the two
Bluetooth links adds up to ~3.5 KB.

Verified on silicon on 2026-10-06, with Bluetooth on and the project
running. The fix crashed on 0 of 44 warm resets: 22 on a monolithic build,
22 on the shipped split image, half of each RTS. Unfixed main crashed on 5
of 12 in the same sitting.

| `lp-emu:esp32c6:t1`, first heartbeat, shipped split image | before | after |
|---|---|---|
| heap total / used / free | 301,536 / 89,828 / 211,708 B | unchanged |
| largest free block | 181,600 B | 132,458 B |
| radio region high-water | (in `dram2_seg`) | 35,660 B of 49,152 B |
| main stack | 56,200 B | 56,184 B |

**What it costs:** contiguity, not bytes. The main region is 48 KiB
smaller, so the largest block a single Rust allocation can get falls by
the same amount. In exchange, all of `dram2_seg`'s 64 KiB is Rust's. Before,
the radio held ~35 KB of it on every board with Bluetooth on. The load
gate's 64 KiB block is still the main region's to give.

**Revisit:** the main stack is the one tenant of RAM that neither needs to
survive a reset nor is ever a DMA target. Moving it into `dram2_seg` (an
override of esp-hal's `stack.x`, as `build.rs` already overrides
`rodata.x`) would let the main heap region absorb the 56 KB it frees,
contiguous with the rest, and win the largest block back with interest.

## Amendment (2026-10-16): the freed stack room becomes heap, and the stack is gated

Moving the radio code to flash (#1092, 65.8 KB) and the compiler's lookup
tables out of `.data` (#1110, 5.8 KB) left the main stack, still the residual
of main RAM, at **119,960 B** in the split image (figure build, main
`c97473d8f`; 120,160 B at `f3a77691e`). The flagship needs about 36 KB of it.
RAM research Q3 (Yona's go, 2026-10-16) asked for those bytes as heap, with
the stack held at the worst measured peak plus a margin.

**Decision.**

1. **The margin rule.** The main stack is the deepest measured peak + 12 KiB,
   rounded up to 4 KiB. The 12 KiB is what the stack had over meteor's peak
   from 2026-09-24 to 2026-10-09.
2. **The gate.** `lp-cli/tests/emu_stack_peak.rs` (CI, `Emulator C6 lp-cli
   (x64)`) boots the shipped split image on `lp-emu:esp32c6:t1`, loads meteor
   and edits its shader ten times, loads the PLAYFUL choker tryout, and fails
   in words unless the stack exceeds each peak by 12 KiB. Statics that grow
   now fail CI. They no longer shrink the stack in silence
   (`docs/defects/2026-10-09-the-c6-main-stack-margin-is-unguarded.md`, fixed).
3. **The new split.** `HEAP_MAIN` takes 306,808 B of main RAM, up from
   236,000 B (`HEAP_MAIN_SIZE` = 306,808 − `HEAP_RADIO_SIZE`). The main stack
   is 49,152 B (the shipped build, with its longer version string, has
   49,144 B). Heap total **372,344 B**, up from 301,536 B. `HEAP_RADIO`
   (49,152 B) and `HEAP_DRAM2` (65,536 B) are unchanged, and so is the region
   order: main, `dram2_seg`, radio.

**The peaks.** The census ran every catalog project. On the emulator, at main
`f3a77691e`: meteor peaks at 35,752 B, the same after ten edits. The choker
tryout peaks at 33,852 B. The other 18 patterns and the peaches peak at
32,252 B, and the other projects at 30,652–31,004 B. Studio-style editor reads
with 20 edits (`emu_frag_reads`) and ten edits over the emulated LAN, joined,
both peak at 30,636 B. The update session (core-only) peaks at 13,864–14,056 B
on silicon. fyeah-sign (emulated WDT reset) and small-dome (heap OOM on main)
gave no peak.

On silicon (CX1, ESP32-C6 rev v0.2, boot auto-load, 5 min, release
2026.10.16-8): meteor 35,752 B, the tryout 31,612 B, the choker 29,112 B. A
Bluetooth editing session (Studio over the CDP central, Edit pressed) did not
raise the choker's mark, and neither did a Wi‑Fi join. On main `c97473d8f`
meteor reads 35,736 B on the emulator, a 13,216 B margin.

**What it bought** (CX1, interleaved A B A B, A = release 2026.10.16-8,
B = this change at `29e568028`, before `HEAP_MAIN` moved 200 B for the
statics main had added since; fps is the median of 5 s heartbeats; free and
largest are the minimum over 5 min):

| | A: free min / largest min | B: free min / largest min | fps A / B |
|---|---|---|---|
| choker | 81,236 / 58,624 B | 152,252 / 66,608 B | 32.67–32.68 / 32.69–32.72 |
| meteor | 75,492 / 49,560 B | 146,508 / 65,536 B | 30.97–31.06 / 30.68–30.75 |
| tryout | 63,788 / 35,248 B | 134,964 / 65,528 B | 35.60 / 35.78 |
| choker, Bluetooth editor open | 61,144 / 21,976 B | 132,160 / 65,528 B | 30.72 / 30.88 |

With Bluetooth and the editor open, the release refuses the Edit press's
package hash (`fs gate: … largest block 21984 B; a 27091 B file needs 27603
B`). B serves it, and the editor opens on both. On B there were no faults and
no resets in any run: 10 warm resets, 3 power cycles, an update over the air
from B to the release and from the release to B (`UpToDate`), and a real power
cut 16 s into the latter, mid-core, after which the update resumed at 946,176 B
and finished.

On the emulator, B lets two projects run that main could not: small-dome now
loads and renders, and the tryout survives Studio's editor sync and ten edits.

**What it costs.** First fit tries the main region first, and it is now 70 KB
bigger, so placements move. `dram2_seg` stays whole longer, and the largest
block is 64 KiB or more in every silicon run above. One placement got worse.
On the emulator, a dome-scale read that main's heap refused cleanly is now
admitted and resets the board halfway through
(`docs/defects/2026-10-16-a-read-the-read-cost-admits-resets-a-dome-scale-c6-mid-read.md`,
open; zook-dome, 1,500 lamps, outside the C6's design target). The estimate
behind the admission is what is wrong, and it belongs to admission work (M3).

**Revisit** when the gate fails. The 12 KiB stays. Take the bytes back from
`HEAP_MAIN`, or make the deep path shallower.

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

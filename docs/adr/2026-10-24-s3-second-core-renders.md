# ADR: The S3's second core renders half of every batch

- **Status:** Accepted
- **Date:** 2026-10-24
- **Deciders:** Photomancer (Yona approved the sequence in chat on
  2026-10-24, "start these in your suggested order")
- **Supersedes:** None
- **Superseded by:** None
- **Plan:** `lp2025/2026-10-20-2335-s3-radio-and-ota` (the fps lane, step 4;
  the evidence is the core-1 spike, `core1-spike/report.md` §4–§9, and the
  fps spike, `fps-spike/report.md` §4, both there)

## Context

The ESP32-S3 has two LX7 cores; until now the firmware ran on one and held
the other. The core-1 spike built a render split (the points of each
render-samples batch, half on each core) and named what a product needs
first: a park for every flash writer, a guard against core 1 taking a lock,
panic and stall handling, an idle core that does not spin, and an emulator
that can run it. The fps spike measured the split at +25–35 % on top of the
IRAM hot set.

## Decision

1. **Core 1 runs the upper half of every render-samples batch** of 16 points
   or more (`lpvm_native::helper_core`, injected by the firmware; the engine
   is untouched). Each half runs the same JIT entry against its own vmctx, a
   copy taken after the uniforms are bound, so the frames are byte-for-byte
   what one core renders.
2. **Core 1 waits in `waiti`, woken by `FROM_CPU_INTR2`** from core 0. It
   masks the wake line, checks for a job, then `waiti` (which unmasks and
   waits in one instruction), so a wake is never lost. An idle core 1 fetches
   nothing, and the emulator gives a parked core no host time.
3. **Every flash writer stalls core 1** with RTC_CNTL's stall key for the
   call: esp-storage's `FlashStorage` through `multicore_auto_park`, and the
   split image's raw ROM calls (`split_core`) through
   `core1::with_core1_parked`. Core 1 executes from flash (builtins) and PSRAM
   (the JIT). A cache refill during an erase hangs the chip.
4. **Core 1 never takes a lock.** A stalled core inside a lock would deadlock
   the writer. Its job path allocates, logs and locks nothing. The global
   allocator is a wrapper (`fw-esp32s3/src/guarded_heap.rs`, esp-alloc's own
   `global-allocator` feature off) that sends any request from core 1 to a
   halt instead of the heap.
5. **A core 1 that fails halts alone.** A panic or exception on core 1, or a
   forbidden allocation, halts it with interrupts masked. It takes no lock,
   prints nothing and causes no reset. Core 0's join times out (2 s, longer
   than a block erase another thread may run mid-join), runs the half itself,
   stops splitting for the rest of the boot and says why once
   (`[core1] stopped: …`).
6. **Radio and driver interrupts stay on core 0.** Core 1 enables only its
   wake line.
7. **`two-core` is a default feature of `fw-esp32s3`.** A `pc-sampler` image,
   whose sampler is core 1, builds without it (the `s3_two_core` cfg).
8. **The emulator runs it** (`lp-emu-esp32s3`):
   - core 1 is released at the mask ROM's reset vector, whose own APP-CPU
     path reaches the guest's address;
   - the cross-core wake into `waiti` is modelled;
   - a flash-busy model (`--flash-busy`, `Esp32S3Builder::flash_busy`) holds
     WIP for each program and erase, and stops the run when either core
     touches the flash/PSRAM windows meanwhile (exit 9).

   A single-core image is unchanged: the same instruction count, console and
   frames on the old and new machine.

## Measured

FS3 (XIAO ESP32-S3 Plus 16 MB, antenna attached), split images from reset,
BLE advertising. Two-core against the same tree without the feature, after
the IRAM hot set:

| project (lanes × LEDs) | one core | two cores | |
|---|---:|---:|---:|
| `skirt-620` (2 × 310) | 41.1 | 50.9 | +24 % |
| `n6x248` (6 × 248 = 1,488) | 20.6 | 26.7 | +30 % |
| `n8x256` (8 × 256 = 2,048) | 15.8 | 20.7 | +31 % |

**BLE connected** (lightplayer.app's Studio in Mac Chrome as the central,
driven over CDP, the card polling the picture; board ~1 m away; not a
phone), 1,488 LEDs, 60 s:

- one core: 17–19 fps, per-window p99 50–500 ms;
- two cores: 22–24 fps, per-window p99 ≤ 50–75 ms, the odd frame to 308 ms;
- no reset, no dropped link.

The budget's connected ceiling is p99 ≤ 1 s (`docs/adr/2026-10-06-radio-frame-rate-budget.md`).

**A deploy during render** (two cores, n8x256 running, skirt-620 uploaded):
the project switched, and core 1 kept taking jobs on silicon. In the
emulator under the flash-busy model (`lp-cli/tests/emu_s3_core1_gates.rs`),
nothing overlapped a write.

**Cost:** core 1's 8 KiB stack comes out of the arena (internal heap
−8,192 B). Its IRAM loop and handlers are about 1.6 KB of `.rwtext`, from the
main stack's share; the main stack has ~26 KB of headroom on FS3.

## Consequences

- Core 1's rules are a contract any code on its path must keep. The
  allocator guard and the halt make breaking it a logged, survivable stop,
  not a deadlock.
- A further split (the per-lamp stages, or the fluid solver's independent
  halves) reuses the same hook pattern and the same rules.
- Background compile on core 1 would need the cooperative park (core 1 may
  then lock), not this stall. That is a different design.

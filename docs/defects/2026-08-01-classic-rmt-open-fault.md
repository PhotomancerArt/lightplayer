# Classic ESP32: opening a WS281x channel faults and reset-loops the board

- **Date:** 2026-08-01
- **Status:** **DIAGNOSED** 2026-08-01 — root cause is heap exhaustion, not RMT.
  Blocks M4-P3/P4 until the fix is chosen.
- **Board:** DOM-Z-102 (classic ESP32 rev v3.1), `fw-esp32v3`
- **Plan:** `2026-07-31-1444-classic-esp32-bringup`, M4-P2

## Root cause

`Esp32OutputProvider::open` inserts into a `VecMap<i32, ChannelState>`. Opening
the **third** channel grows that map to capacity 4, which asks the allocator for
one contiguous **12,864-byte** block. There were **11,228 bytes** free. The
allocation fails, and on this chip an allocation failure is fatal.

Measured on silicon, `projects/test/quad-strips-v3`:

```
allocation failed: requested=12864 align=8 free=11228 used=101408
```

The heap is 112,640 B total, so the app is already at **90 % occupancy before
the first channel opens**. This is not an outsized request; it is the straw.

### Where the 12,864 comes from

`size_of::<(i32, ChannelState)>()` is **3,216 B**, and 4 × 3,216 = 12,864.
Of those 3,216 bytes, **3,084 — 96 % — are one field**:

```rust
// lp-core/lpc-shared/src/display_pipeline/pipeline.rs
pub struct DisplayPipeline {
    ...
    lut: [[u32; LUT_LEN]; 3],   // LUT_LEN = 257  =>  3 * 257 * 4 = 3,084 B, INLINE
}
```

Two things make that worse than its size alone suggests:

1. **It is inline in the struct**, so it is not a pointer the `VecMap` copies —
   it is 3 KB of table copied per element on every growth, and `Vec` growth
   holds the old and new buffers simultaneously. Opening channel 3 therefore
   needs 6,432 (old) + 12,864 (new) = **19,296 B** live at the peak.
2. **It is built unconditionally.** `DisplayPipeline::new` calls `build_lut`
   three times with no reference to `options.lut_enabled`, and every
   `output*.json` in `quad-strips-v3` sets `"lut_enabled": false`. The board
   dies carrying 12.3 KB of lookup tables across four channels that the project
   explicitly asked it not to use.

### The full stack, symbolized from the crash record

```
fw_esp32v3::recovery::panic_path::stage_oom_and_reset
fw_esp32v3::on_alloc_error
alloc::raw_vec::handle_error
<alloc::raw_vec::RawVec<(i32, fw_esp32_common::output::provider::ChannelState)>>::grow_one
<fw_esp32_common::output::provider::Esp32OutputProvider as ...::OutputProvider>::open
<lpa_server::project::SharedOutputProvider as ...::OutputProvider>::open
<lpa_server::server::LpServer>::advance_frame
<lpa_server::server::LpServer>::tick_and_send::<...>
...::Executor::run  ->  main  ->  Reset
```

Note `advance_frame`: the channels open on the **first render**, not during
project load. That is why the boot frame guard was not on the stack (`<no
frame>`) and why the fault outlives `boot_firmware`.

## What this overturns

The pre-diagnosis version of this document named a **prime suspect: the
registry/lease layer this port added in front of the backend, which
`led-lab-esp32` does not have.** That suspect is **exonerated**. Nothing in
endpoint resolution, `AnyPin::steal`, or `bind_channel` is involved; the fault
is in the provider's own bookkeeping `Vec`, one frame above the driver. Do not
spend more time diffing against `led-lab-esp32` — it runs on the same silicon
with a far smaller resident heap, which is the only reason it survives.

The three "ruled out by measurement" findings all still stand, and all three are
now *explained* rather than merely excluded:

- **Not stack exhaustion.** Correct — and 110 KB and 64 KB heaps faulted
  identically because *both* OOM'd. Trading heap for stack could only ever have
  made this worse.
- **Not the RMT RAM base.** Correct; execution never reaches the RAM clear.
- **Not a double panic from interrupts.** Correct; masking changes nothing
  because the fault is an allocator return value, not an exception.

## Why it took an RTC ledger to see this

The old §"this fault cannot report itself" was accurate: every print variant
yielded the same ~5 characters, and the one datum that escaped was `L553` from
a file truncated to `/U`. That is now attributable — it was
`.../library/alloc/src/alloc.rs:553`, the standard library's
`handle_alloc_error`. **The line number was pointing at the answer the whole
time and could not be read.**

The instrument that resolved it is `lp-recovery`'s RTC-RAM ledger, pulled
forward from M7 into M4, plus two things built on it:

- A `#[alloc_error_handler]` that records `requested/align/free/used` into the
  crash record, so an OOM arrives as an `Oom` with numbers instead of a panic
  with a formatted string.
- `lpc-shared`'s `xt-map-esp32-classic` feature. The Xtensa stack walker's
  bounds checks were hard-wired to the S3's IRAM/flash windows; on classic
  silicon every frame in this backtrace (`0x400d…`–`0x4013…`) would have been
  rejected and the walk would have reported zero frames.

It named the fault on the **first** boot after flashing.

## Fix directions, cheapest first

Not yet chosen — the second one changes a struct every firmware and the host
share, so it is a design call, not a bring-up call.

1. **`VecMap::with_capacity(channel_count)` in `Esp32OutputProvider`.** Removes
   the growth spike (the 6,432 B of old buffer held during the copy) but *not*
   the steady-state 12,864 B. **Insufficient alone** — 12,864 still exceeds the
   11,228 free.
2. **Stop carrying the LUT inline.** `Option<Box<[[u32; LUT_LEN]; 3]>>`, built
   only when `options.lut_enabled`, drops `ChannelState` from 3,216 B to ~132 B
   and this project's four channels from 12.9 KB to ~0.5 KB. It also stops
   building a table the project disabled. Touches `lpc-shared`, so it changes
   the S3 and C6 too — both currently survive on a larger arena, so this is
   latent there rather than absent.
3. **Reduce the 101,408 B baseline.** The real headroom problem, and the
   `serde`-surface-scale diet M6 was already going to have to describe. Out of
   scope for M4.

## Reproduce

```bash
just build-fw-esp32v3
espflash flash --chip esp32 --port /dev/cu.wchusbserial1130 \
  --partition-table lp-fw/fw-esp32v3/partitions.csv --flash-size 4mb \
  --baud 921600 --after hard-reset --monitor --monitor-baud 921600 \
  target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3
```

The board auto-loads `quad-strips-v3` and OOMs on the first render. Boot 2
prints the record; boot 3 enters **safe mode** and stays up, reachable, at
~830 fps with the crash attached to every heartbeat. That last part is new — the
loop used to be unbreakable from the host, and clearing it meant erasing lpfs.

Notes that still apply:

- `espflash monitor` alone stub-halts the app; use `flash --monitor` under a
  pty, or read the port directly.
- The board's port number moves between sessions (`…serial1140` and
  `…serial1130` both observed). Always pass `--port` and check the chip id.
- To clear a wedged auto-load: `espflash erase-region --port … 0x310000 0xF0000`.
  Safe mode makes this rarely necessary now.

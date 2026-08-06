# ESP32-S3: Overlapped WS281x Transmission on the Second Core

## Status

Future work. Captured 2026-08-05 after the classic ESP32's dual-core RMT ISR
shipped (PR #341, ADR `2026-08-04-rmt-isr-on-app-core.md`). The S3 is also
dual-core; this note records how much of that work transfers and what an S3
adoption milestone actually contains, so it can be sized without rediscovery.

## Amendment (2026-08-05, later the same day)

Written before the wire pusher shipped (ADR
`2026-08-05-ws281x-transmission-on-app-core.md`). The pusher STRENGTHENS the
transfer story: `lp_ws281x::pusher` (mailboxes + scheduler) is chip-free and
carries to the S3 unchanged, host tests and Miri topology included; the
deployment shell (`fw-esp32v3/src/output/rmt/wire_pusher.rs` — statics,
doorbell software interrupt, lost-wakeup guard) ports the same way the
core-1 machinery below does. "Idles in `waiti 0` forever" below describes
the pre-pusher shape; core 1 now runs the pusher loop around that idle.

## What transfers as-is (no new work)

- **The cross-core frame lifecycle** in `lp-ws281x`: the `isr_seq` teardown
  handshake, the SeqCst store/load pairing, the ordering contract in
  `state.rs`. Written against the abstract "ISR on another core" deployment
  and proven by the Miri harness (`just ws281x-miri`) with no chip in the
  loop. The S3's LX7 has the same memory model (`memw`-fenced atomics over
  uncached internal SRAM).
- **The `isr-in-ram` feature** — `.rwtext` is the same IRAM section on both
  Xtensa chips.
- **The provider and engine halves** — `Ws281xOutput::background_tx_safe()`
  (default `false`), the conditional flush in `Esp32OutputProvider`, the
  wait-before-stage duty, and the M4 guard tests are all chip-shared already.

## What ports as (nearly) copied code

`fw-esp32v3/src/output/rmt/shared_driver.rs`'s core-1 machinery —
`start_app_core_isr`, `app_core_main`, the `install_isr` branching — is
esp-hal-portable: `CpuControl` covers both chips, and all three traps are
identical on the S3 (the bind must execute *on* core 1; `Rmt::
set_interrupt_handler` disables the mapping on every other core; a returned
core-1 entry or dropped `AppCoreGuard` is a hardware stall that silently
services nothing). If the S3 adopts, hoist this module into `fw-esp32-common`
rather than duplicating it.

## What must be redone per chip

- **Step zero: the S3 driver has not adopted the M4 start/wait split** — it
  still implements only the blocking `write`. That lands first.
- **The cap arithmetic**: duty = (refills per deadline) × (refill cost). The
  S3's 48-word blocks give 24-word halves (30 µs deadline) at one block per
  channel — a different geometry from the classic's 64-word halves; plug its
  numbers into the formula rather than borrowing the classic's cap. Refill
  cost is code-bound, not bus-bound (refill-floor probe, 2026-08-05), so the
  hoisted fill path's cost is the input.
- **The motivating measurement**: nobody has verified the S3's app path masks
  interrupts long enough to *need* the second core. The classic truncated
  ~99 % of overlapped frames; if the S3's masking is milder, its barrier may
  be cheap enough that core-1 ownership (which future radio work will want to
  negotiate) is not worth claiming. Measure before building.
- **Its own silicon pass** with the per-wire trip/entry-delay telemetry, per
  the standing each-adoption rule from the M4 ADR.

## The one genuinely new hazard: PSRAM

The classic's soundness argument leans on internal DRAM being uncached, so
atomics are the whole cross-core story. The S3 has a data cache in front of
PSRAM: frame bytes living there would make the ISR core's reads
cache-mediated, and the lp-ws281x contract would need a writeback/coherency
story, not just orderings. Today the S3 firmware heaps in internal SRAM so
this is moot — but any S3 dual-core plan must carry **"frames stay in
internal RAM"** as an explicit invariant, or do the coherency design.

## Non-transfer

The C6 is single-core; none of this applies. Its wire-time lever, if ever
needed, is DMA-shaped.

## Sizing

`md`. Most of the risk is retired: the port is mechanical, the protocol is
proven, and the open questions are two measurements (masking severity; cap
from S3 geometry). Natural sequencing: decide it alongside the pin-mux
pooled-slots planning (the classic's P7), since both live in the same "how
many wires, serviced how, on which cores" territory.

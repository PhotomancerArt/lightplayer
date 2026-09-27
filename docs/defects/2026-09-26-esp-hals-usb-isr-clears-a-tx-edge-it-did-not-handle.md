---
status: open
found: 2026-09-26      # emulator (lp-emu:esp32c6:t1), the comms lab's USB soak; code read in the fork
area: third_party/esp-hal src/usb_serial_jtag.rs (`async_interrupt_handler`) × any task that reads and writes USB-Serial-JTAG at once
class: assumed-context
related:
  - docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md
  - lp-fw/fw-esp32-common/src/serial/in_endpoint.rs
  - ~/.photomancer/planning/lp2025/2026-09-26-1720-reliable-device-link/reports/m3-on-target.md
  - ~/.photomancer/planning/lp2025/2026-09-26-1720-reliable-device-link/reports/m3-esp-hal-isr-clear-experiment.patch
---
# esp-hal's USB-Serial-JTAG interrupt handler clears a TX edge it did not handle

**Symptom.** The comms lab's USB soak (`test_comms_lab`, echo traffic both
ways plus a board stream) on the emulated C6: now and then a frame write
through the IN-endpoint gate waits out its whole 250 ms timeout although the
host is draining every packet. Logged on the link's own log channel:
`usb: a frame write timed out (1 so far) at uptime 8356 ms; in_ep_free=true`
— the send buffer is free, and the write future never woke. 9 in 60 s of
emulated soak (`lp-emu:esp32c6:t1`), 0 in the same run with the handler
patched (below).

**Root cause.** esp-hal 1.1.1's `async_interrupt_handler` reads `int_st`,
decides which of `serial_in_empty` (TX) and `serial_out_recv_pkt` (RX) it is
handling, and then writes `int_clr` with **both** bits set, whichever fired.
The TX write future completes only when the handler sees `serial_in_empty`
and clears its enable bit. If the IN packet drains (raising
`serial_in_empty`) after the handler has read `int_st` for an RX interrupt
but before its `int_clr` write, the handler clears the raw TX bit it never
looked at. The TX enable stays set, no second edge comes (the buffer is
already empty), and the future sleeps until the caller's timeout. It needs a
task that has an RX future armed while a TX packet is in flight, which is
exactly a link carrying traffic both ways.

The product's io_task has the same shape (a 1 ms `select` around `read`,
writes through the same gate) and a 250 ms chunk timeout; two timeouts in a
row latch "not draining" there. Whether this is behind any of the product's
chunk timeouts on silicon is **not** measured.

**Fix (in review, not merged): PR #855.** Upstream esp-hal fixed this in
#6089 (clear only the handled bits — the same change as the experiment
patch in the plan directory) and hardened the same driver in #6104 (a lock
around every async `int_ena` read-modify-write; after each `wr_done`, wait
for a new `serial_in_empty` and then `serial_in_ep_data_free`), both
released in esp-hal 1.2.0. PR #855
(https://github.com/PhotomancerArt/lightplayer/pull/855) back-ports them,
with #6097 (`wr_done` in the async flush, which #6104's flush builds on),
into `third_party/esp-hal`; README-LP.md records it as the fork's third diff,
dropped on upgrade to ≥ 1.2.0. This entry closes when that PR merges.

**Regression coverage.** `lp-cli/tests/emu_link_lab.rs` (`just
link-lab-emu`, `#[ignore]`d, not in CI) asserts the board's
`edge.lost_wakes` is 0 in every mix. Against the stock 1.1.1 image it fails
(3 lost wakes in the 16.7 s clean mix, 3 in the stall run); with the
back-port it passes. A longer soak, `lp-cli link lab emu:<ELF>
--echo-secs 300 --stream-secs 30 --seed 2` on `lp-emu:esp32c6:t1@7857245078`:
stock 30 lost wakes in 330.7 s emulated (5.4/min), back-port 0 in 330.5 s.

**Lesson.** A handler that services two sources off one status read must
clear exactly what it read. The IN-endpoint gate fixed the *stale* raw bit
(the 2026-09-13 defect); this is the opposite race, a *fresh* one cleared
too early, and no gate in front of a write can see it.

**Incidents.**

- 2026-09-27 — main went red at a10ef3c8c (#853, a Studio-only change): CI's C6 image hit the lost wake deterministically in three emulator tests (a 250 ms first-write stall dropped the hello).

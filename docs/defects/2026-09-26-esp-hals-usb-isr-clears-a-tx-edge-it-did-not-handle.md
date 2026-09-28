---
status: open           # mitigated by lp-link resends (PR #854); fix = draft PR #855, awaiting Yona
found: 2026-09-26      # emulator (lp-emu:esp32c6:t1), the comms lab's USB soak; code read in the fork
area: third_party/esp-hal src/usb_serial_jtag.rs (`async_interrupt_handler`) × any task that reads and writes USB-Serial-JTAG at once
class: assumed-context
related:
  - docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md
  - lp-fw/fw-esp32-common/src/serial/in_endpoint.rs
  - docs/adr/2026-09-27-lp-link-one-comms-layer.md
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

**Fix (not applied; the plan ships no product change).** Clear only the
bits handled: `int_clr` with `serial_in_empty` only if `tx`, and
`serial_out_recv_pkt` only if `rx` — the experiment patch in the plan
directory. It belongs in `third_party/esp-hal` (README-LP.md's diff list)
and upstream.

**Regression coverage.** None yet. `lp-cli/tests/emu_link_lab.rs` reports
the board's `edge.write_timeouts`; an assertion of 0 there, with the fix,
would pin it.

**Mitigated (2026-09-27, PR #854, the lp-link USB cut-over).** This defect
is not fixed by lp-link, but its user-visible cost is: a frame write that
stalls on the missed edge now just resends once `lp-link`'s own retransmit
timer fires, instead of surfacing as a 250 ms application-level stall (the
comms lab's `not_draining` latch, deleted with the cut-over). It still costs
latency — every stall this defect causes is still a real round trip lost —
and the `docs/adr/2026-07-28-esp32c6-flash-budget.md`-adjacent D12 decision
not to back-port esp-hal's own fix (esp-hal #6089/#6104, released in 1.2.0)
in this PR stands. The actual fix, third_party's `int_clr` patch described
above, is up as **draft PR #855, awaiting Yona** — this entry stays `open`
until that lands.

**Lesson.** A handler that services two sources off one status read must
clear exactly what it read. The IN-endpoint gate fixed the *stale* raw bit
(the 2026-09-13 defect); this is the opposite race, a *fresh* one cleared
too early, and no gate in front of a write can see it. A reliable link atop
a bug like this one turns a stall into a resend, but it does not make the
stall free — the two are complementary, not substitutes.

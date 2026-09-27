---
status: open
found: 2026-09-27      # e2e: porting lp-cli/tests/emu_usb_free_lag.rs to the lp-link image (plan lp2025/2026-09-27-0215-lp-link-usb-cutover, P5)
area: fw-esp32-common serial/in_endpoint.rs (InEndpoint::ready) × the lp-link USB task × lp-emu-esp-common ip/usb_sj.rs free-lag hypothesis
class: assumed-context
related:
  - docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md
  - docs/defects/2026-09-26-esp-hals-usb-isr-clears-a-tx-edge-it-did-not-handle.md
  - docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md
---
# The IN-endpoint gate loses the drain's wake when the free bit lags the empty edge

**Conditional on a hypothesis.** The emulator's USB "free lag"
(`--usb-in-free-lag <ns>`, off by default) models a gap between the drain's
`serial_in_empty` edge and `serial_in_ep_data_free` going high. No document
gives silicon such a gap and nobody has measured one; it is the one
single-writer path the emulator has to the real C6's "a few bytes short"
symptom. Everything below is true only if silicon has that gap.

**Symptom.** `lp-cli/tests/emu_usb_free_lag.rs` on the lp-link image
(`lp-emu:esp32c6:t1`, 40 Hello requests 20 ms apart, the free lag switched on
at 1.9 s). With no lag both images answer 40 of 40. With a lag of 10,306 ns
(just past the ungated image's first write after a drain):

| image | Hellos answered | bytes refused | host link | board |
|---|---:|---:|---|---|
| ungated (`fixture-no-in-endpoint-gate`) | 0 of 40 | 1,561 | 23 damaged, 6 resent | 0 write timeouts |
| gated (shipped) | **0 of 40** | 0 | 0 damaged, 26 resent | **9 write timeouts**, next `ep1` write 250 ms after a drain |

The gate did its first job — nothing was written into the lag — and then
stalled every write for its 250 ms bound.

**Root cause.** `InEndpoint::ready` reads `serial_in_ep_data_free`; if the
buffer is not free it clears the raw `serial_in_empty`, re-reads free, and if
still not free awaits esp-hal's `flush`, which arms `serial_in_empty` and
waits for it. Inside a free lag the drain's `serial_in_empty` has ALREADY
fired: the clear erases it, the recheck still reads "not free" (the lag), and
the flush waits for an edge that will not come again until the next drain —
there is none, because nothing was written. The write bound (250 ms) ends it.

On the `M!` image the gate's check came ~26 cycles AFTER esp-hal's unchecked
write (PR #832's measurement), so a lag short enough to catch esp-hal missed
the gate. On the lp-link image the link task reaches the gate SOONER (gate
check 8,606 ns after a drain, esp-hal's write 9,306 ns), so any lag long
enough to hurt the ungated image puts the gate's check inside it.

**Fix (not applied).** When the recheck still reads not-free, poll
`serial_in_ep_data_free` on a short timer instead of waiting on the edge
(or wait on the edge with a short timeout and re-check), so a lagging free
bit costs microseconds, not the write bound. It touches the gate every C6 and
S3 USB byte passes through, so it wants a desk check against the gate's
silicon record (0 of 1,327 packed frames lost, 2026-09-25) before it lands.

**Regression coverage.** `emu_usb_free_lag.rs` pins today's behaviour under
the lag: the gated image writes nothing into the lag, and its writes time
out. When the fix lands that assertion flips to "every reply answered, no
write timeout" — the test says so where it asserts.

**Lesson.** Moving the writer onto a different task moved WHEN the gate's
check lands relative to the drain, and that ordering was the whole reason the
gate was safe under the hypothesis. A gate that clears an edge it then waits
on is only race-free if the condition it re-checks moves with the edge.

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

**Since the esp-hal back-port (PR #855), esp-hal's own wait has the same
shape.** Upstream #6104 makes `write_async` wait, after every `wr_done`, for
a new `serial_in_empty` and then loop until `serial_in_ep_data_free` is set,
waiting on the edge again while it is not. Inside a free lag that second
wait never ends either. So the ungated image (`fixture-no-in-endpoint-gate`)
no longer writes into the lag — 0 B refused — and stalls exactly like the
gated one: with the back-port merged onto the lp-link image, at a 10,712 ns
lag, **both** images answer 0 of 40 Hellos, 0 B refused, 9 damaged frames
and 9 board write timeouts (`lp-emu:esp32c6:t1`). A fix in the gate alone
would therefore not end the stall: esp-hal's per-packet wait would still sit
out the bound. Upstream's design (like ESP-IDF's ISR) assumes a second
`serial_in_empty` edge follows once the buffer really is free; the
emulator's lag model gives none, which is a question about the model as
much as about the drivers.

**Fix (not applied).** When the recheck still reads not-free, poll
`serial_in_ep_data_free` on a short timer instead of waiting on the edge
(or wait on the edge with a short timeout and re-check), so a lagging free
bit costs microseconds, not the write bound. It touches the gate every C6 and
S3 USB byte passes through, so it wants a desk check against the gate's
silicon record (0 of 1,327 packed frames lost, 2026-09-25) before it lands.

**Regression coverage.** `emu_usb_free_lag.rs` pins today's behaviour under
the lag: neither image writes into the lag (since PR #855, the ungated one
too), and both images' writes time out. A write that times out after its first packet leaves half a frame on
the host; since the host waits `frame_abandon` (3 s, e726f7083) for the rest
instead of the 50 ms text idle, that half frame is closed by the next frame
and counted `damaged` (9 of 9 timeouts at `lp-emu:esp32c6:t1`) where it used
to be dropped as a stale partial, so the test holds damaged to at most the
board's own count of abandoned writes. When the fix lands that assertion flips to "every reply answered, no
write timeout" — the test says so where it asserts.

**The stall has an onset band, not an edge** (2026-09-29, PR #880). The
test used to run ONE lag, 1 µs past the later of the two images' soonest
post-drain touches. On #880's CI image the gated image's soonest `ep1_conf`
read came out at 7,062 ns (a local build of the same tree: 9,537 ns — the
minimum moves with code layout), the lag landed at 8,062 ns, and there the
ungated image lost the wake on 2 packets only: the link resent, all 40
Hellos were answered, and the assertion read the defect as fixed. A sweep of
0–20 µs in 250 ns steps (`lp-emu:esp32c6:t1`; each tree's images driven by
that tree's own link host, lp-emu at main `ca0b3dbd9` and #880 `4f55d5eb2`;
"soonest" is step 1's no-lag minimum, write / `ep1_conf` read):

| image | soonest | no timeout up to | 40 of 40 answered, 1–2 write timeouts | 0 of 40, 9 write timeouts |
|---|---:|---:|---:|---:|
| main ungated, local build | 9,712 / 9,393 ns | 9,250 ns | — | 9,500–20,000 ns |
| main gated, local build | 10,450 / 9,550 ns | 9,500 ns | — | 9,750–20,000 ns |
| #880 ungated, local build | 7,000 / 6,675 ns | 6,500 ns | 6,750–9,250 ns | 9,500–20,000 ns |
| #880 gated, local build | 10,431 / 9,537 ns | 9,500 ns | — | 9,750–20,000 ns |
| #880 gated, CI's image | 10,431 / 7,062 ns | 7,000 ns | 7,250–9,500 ns | 9,750–20,000 ns |

Nothing was written into the lag, and no damage exceeded the board's own
timeouts, anywhere in the sweep. Every image stalls outright from
9.5–9.75 µs, where main's images' soonest touches sit, which is why one lag
1 µs past them always landed in the stall. On #880's tree some images touch
the endpoint as early as ~7 µs after a few drains; the minimum then marks the
start of an intermittent band, not the stall, and whether an image shows that
band depends on its build (the local and CI builds of #880's gated image
differ). #880 made the test climb a ladder of lags, 1.25–3× the later soonest
touch, requiring the stall on at least one rung. When #880 merged main
(2026-10-05) it took main's fix for the same cause instead — the lag chosen
from the *typical* wake, not the soonest (the 2026-10-01 entry below) — and
dropped its ladder: one rule for one cause.

**Lesson.** Moving the writer onto a different task moved WHEN the gate's
check lands relative to the drain, and that ordering was the whole reason the
gate was safe under the hypothesis. A gate that clears an edge it then waits
on is only race-free if the condition it re-checks moves with the edge.

**2026-10-01 — the regression test's lag stopped covering the wake (still
open).** On PR #894 (the secure link) `emu_usb_free_lag.rs` reported this
defect "looks fixed": the ungated image answered 40 of 40 under the lag, with
2 write timeouts that the link's resends recovered. It was not fixed. The test
chose the lag from the run's *soonest* drain-to-`ep1_conf`/`ep1` span, and
that statistic belongs to whichever single drain happened to land while the
CPU was already running: per-drain traces (`lp-emu:esp32c6:t1`, images built
on a desk from 3ecb85468 and from #894's head) show the idle wake unchanged —
the ungated image's check 1,503 cycles (9,393 ns) after a drain, the gated
one's 1,528 (9,550 ns), on ~97 of 98 drains in both trees — while #894's
images add one or two drains that find the CPU awake and wake in 769–1,317
cycles. The run's soonest became one of those (6,418 ns on CI, 4,806 ns on the
desk, for one commit), the lag landed at ~8 µs, below the wake almost every
drain takes, and so almost every wake escaped the lag. The test now chooses the
lag from the median, over the 40 requests, of each request's soonest wake; on
both trees, and on CI's own shipped image for #894, that is 9,718 / 9,550 ns,
the lag is 10,718 ns, and both images show this defect's full signature again:
0 of 40 answered, 9 write timeouts, 9 damaged, 26 resent. The firmware change
did not move the USB path; it moved the phase of the guest's other work against
the host's drain cadence, which any change to the image can.

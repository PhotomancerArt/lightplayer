---
status: retired
logged: 2026-09-08
retired: 2026-09-09
area: lp-emu-esp32c6 radio window (`periph/wifi_stub.rs`), the RX delivery path
found_by: M4 P3, the first payload on this machine that both sends and receives repeatedly
related:
  - lp-emu/esp/lp-emu-esp32c6/tests/espnow_broadcast_pair.rs (`the_air_surfaces_every_delivered_frame` — M4 P3's pin, inverted by the fix)
  - lp-emu/esp/lp-emu-esp32c6/tests/air_delivery.rs (`what_the_guests_isr_reads_after_each_delivery` — the instrumentation that named the cause; `every_delivery_takes_a_descriptor_the_guest_has_not_already_read` — the regression)
  - lp-emu/esp/lp-emu-esp32c6/src/machine.rs (`deliver_air_frame` — the walk, and where the base was re-derived)
  - lp-emu/esp/lp-emu-esp32c6/src/periph/wifi_stub.rs (`rx_write_cursor`, the RX cursors `+0x4088` / `+0x408c`, the reload strobe `+0x4080`, the event bit 14)
  - lp-emu/esp/lp-emu-esp32c6/src/lockstep.rs (`Lockstep::run_until`, `offer_air_frame`)
  - docs/reports/2026-09-09-espnow-broadcast-two-board-silicon-replay.md (the silicon oracle: `gap` 1 on every record)
  - lp-emu/transcripts/esp32c6/espnow-broadcast/ (both emulated captures carry it, in the `gap` field)
  - docs/adr/2026-09-08-virtual-air-claim-policy.md
  - lp-emu/esp/lp-emu-esp32c6/README.md ("The air, and the lockstep pair")
---
# A guest on the emulated air sees every other frame written into its RX ring

**Shape** — the air delivers, the ring accepts, and the receiving guest's
application reports **half**. On a lockstep pair of two `espnow-broadcast`
machines, each machine's records read the peer's events `0, 2, 4, 6, 8, 10` and
never an odd one.

## The measurement

`lp-emu/esp/lp-emu-esp32c6/tests/espnow_broadcast_pair.rs`, on the
`test_espnow_broadcast` image at `04dae2fa201f`, pair horizon 3.5 s:

```text
machine #0: cycles=560000000 sent=33 offered=33 delivered=33 undelivered=0
machine #1: cycles=556000000 sent=33 offered=33 delivered=33 undelivered=0
```

Every frame either machine armed entered the air, was offered to the other, and
was written into its ring. `air_frames_undelivered` is **0** — no frame found a
full ring or an unprogrammed one. And yet:

```text
[espnow-broadcast] rx device=0x7ca88562 event=0  kind=1 payload_len=0
[espnow-broadcast] rx device=0x7ca88562 event=2  kind=1 payload_len=24
[espnow-broadcast] rx device=0x7ca88562 event=4  kind=1 payload_len=0
[espnow-broadcast] rx device=0x7ca88562 event=6  kind=1 payload_len=24
[espnow-broadcast] rx device=0x7ca88562 event=8  kind=1 payload_len=0
[espnow-broadcast] rx device=0x7ca88562 event=10 kind=1 payload_len=24
```

So the loss is **between the ring and the blob's RX path**, not in the air and
not in the payload: the frames that do arrive arrive whole (`len_ok` is true on
every record, and the byte count is the one the sender's event number
prescribes), and the sender's own `tx` records show all six of its sends
completing.

## What it is not

- **Not a phase artefact of the pair's stagger.** The stagger is settable with
  `LP_EMU_C6_ESPNOW_BROADCAST_STAGGER_MS`. At 250 ms (where the peer's frames
  land near this machine's own sends) and at 25 ms (where they land nowhere near
  them) the alternation is **identical** — `gap` is 2 on five of six records
  either way.
- **Not the guest's own transmit stealing the interrupt**, for the same reason:
  at a 25 ms stagger a peer's frame arrives 25.672 ms after this machine's tick
  and 75 ms before its next send, and it is still every other one.
- **Not the payload's own bookkeeping.** The payload records the first six
  frames it is handed by `drain_channel`, in order, and prints one human line
  per frame it is handed. Both agree.
- **Not the duplicate ring in the product driver.** `SeenRing` refuses a repeat
  of one `(source MAC, device id, event id)`; every frame here has a different
  event id.

## Why nothing found it before

Because nothing had sent and received repeatedly. M4 P2 delivered **one** frame
into a ring and proved a guest could see it; M4 U1 completed **one**
transmission and proved a guest's `send` could return. `espnow-broadcast` is the
first payload on this machine whose subject is a *stream* in both directions,
which is what a two-board payload is for.

## Where to look

The RX cursors M4 P2 closed U6 on are the obvious first suspects — `+0x408c`
(the descriptor the hardware last filled) and `+0x4088` (the one it will fill
next) — together with the self-clearing reload strobe at `+0x4080` bit 0 and
the `+0x4c48` bit 14 raise. An off-by-one between "where we wrote" and "where
we told the blob to look next" would present exactly as an alternation: the
blob consumes the descriptor we point it at, and our next write lands where it
has already been.

That is a hypothesis, not a finding. **It has not been tested**, because
`periph/wifi_stub.rs` is M4 P2's, merged, and fenced for M4 P3 — a phase whose
job is to say what the air is, not to change it. Reporting it is the honest
half; the fix wants the RX ring's own instrumentation and a test that offers two
frames in a row, which is a small piece of work for whoever holds that file
next.

## What it costs today

- **The two-board replay's `gap` field.** Silicon delivers a contiguous stream;
  the emulator delivers alternate ones. That is the disagreement M4 P3 reports
  and pins rather than tuning, and it is the one field of the payload that does
  not compare.
- **Nothing else.** The bytes that arrive are right — device ids, kinds and byte
  counts all compare — so the claim "a frame the blob hands the MAC is delivered
  verbatim into every other machine's RX ring" survives; the claim "every frame
  reaches the receiving application" does not, and the README says so.

`lp-emu/esp/lp-emu-esp32c6/tests/espnow_broadcast_pair.rs`'s
`the_air_surfaces_only_every_other_delivered_frame` pins the current behaviour
and **fails the day it is fixed**, which is the point of pinning it: the fix
should re-record the two emulated transcripts and retire this file.

---

## Retired, 2026-09-09 — the write position was re-derived, not carried

**The cause.** `Esp32C6Machine::deliver_air_frame` re-derived its write
position from `RX_DMA_BASE_OFFSET` (`WIFI_MAC+0x4084`) on **every** frame and
took the first descriptor from there that the hardware still owned. The blob
recycles a descriptor it has consumed by moving it to the **tail** of its
chain — `owner` back to 1, `next` set to NULL, the old tail linked to it — and
advances `+0x4084` one ISR **later**. For the width of that window the base
still names a descriptor the guest has already read and handed back, so every
second delivery wrote into the ring's tail and then published that tail's NULL
as `RX_DSCR_NEXT_OFFSET` (`+0x4088`) — the null cursor M4 P2 had already
recorded the blob refusing to follow (`R4 UNMAPPED+0x1143c`). The guest never
surfaced that frame; the base then caught up, the next delivery landed
correctly, and the alternation was exactly that two-step cycle.

**The guest reads that prove it.** `tests/air_delivery.rs::what_the_guests_isr
_reads_after_each_delivery` hands eight frames from a real `espnow-broadcast`
sender to a real `espnow-broadcast` receiver one at a time, with the receiver
run between each, and prints the ring, the descriptor taken, the cursors and
the guest's reads. Before the fix:

```text
  # base             ring wrote into  +0x408c    +0x4088     rx?
  0 0x40811e14 HHHHHHHHHH 0x40811e14  0x40811e14 0x40811e20  yes
  1 0x40811e14          H 0x40811e14  0x40811e14 0x00000000   NO   … W4084=0x40811e20
  2 0x40811e20 HHHHHHHHHH 0x40811e20  0x40811e20 0x40811e2c  yes
  3 0x40811e20          H 0x40811e20  0x40811e20 0x00000000   NO   … W4084=0x40811e2c
consecutive repeats: 4 of 8   deliveries publishing +0x4088 = 0: 4
```

Three things are visible at once and each is a whole step of the argument.
**The `ring` column collapses from ten to one on the losing rows** — walked
from a base that now names the ring's tail, whose `next` is NULL. **The `wrote
into` column repeats** — two deliveries in a row into one descriptor. And
**the guest's own `W4084` write, at the end of every losing window**, is the
base catching up one ISR late. After the fix the same table reads nine
distinct descriptors, zero repeats, zero null cursors.

**What the competing hypotheses would have predicted.** The entry above named
three, all untested:

- *An advance-by-two in the cursors.* Would show `+0x4088` skipping a
  descriptor — `0x40811e2c` after filling `0x40811e14`. It never skips: on
  every delivery `+0x4088` is exactly the filled descriptor's own `next`, one
  step along the chain. Refuted.
- *An off-by-one between "next to fill" and "next to read".* Would have the
  guest reading the **wrong** descriptor — a stale buffer, a short frame,
  `len_ok:false`, or the peer's frames arriving out of order. Every frame that
  arrived arrived whole and in order, both before and after; the missing ones
  were never read wrongly, they were never read at all. Refuted.
- *The reload strobe (`+0x4080` bit 0) consuming a descriptor.* Would drain
  the ring at twice the delivery rate and change the arithmetic of
  `the_eleventh_frame_is_dropped_counted_and_logged` (ten descriptors, ten
  frames). The ring never drains — the guest recycles — and the eleventh-frame
  gate is unchanged by the fix, having been re-run against it. Refuted; the
  strobe is touched on the RX path but consumes nothing.

The stagger and the machine's own transmit were already ruled out by M4 P3 and
are not re-litigated here.

**The fix.** `WifiStub` gains `rx_write_cursor` — the descriptor the modelled
DMA will fill next — written by the same `raise_rx_interrupt` call that writes
`+0x4088`, so the register and the cursor are one fact with one writer. The
delivery walk starts there and falls back to the base, so a cursor gone stale
(a guest that re-posts its ring elsewhere) is followed rather than stranding
the air, and the ring-full policy is untouched. The generalizable rule, and
the reason this is worth a closing section rather than a commit message:
**`+0x4084` is the driver's head and the cursor is the hardware's, and a model
that derives one from the other loses exactly the traffic that arrives inside
the window between them.** Nothing about that is specific to this MAC; it is
true of any descriptor ring a driver recycles behind a DMA engine.

**Regression.**
`tests/espnow_broadcast_pair.rs::the_air_surfaces_every_delivered_frame` — M4
P3's pin, inverted rather than deleted, so the diff shows the day it changed —
and `tests/air_delivery.rs::every_delivery_takes_a_descriptor_the_guest_has
_not_already_read`, which pins the mechanism without going through the
payload's arithmetic at all.

**The replay, both machines.** The silicon pair landed on `main` while this
was in flight (PR #644), so the comparison the report could only describe can
now be run. Against `silicon-esp32c6-2026-09-09-5c1d37627-*`:

| left (emulated `t1`) | compared | equal | differ |
|---|---|---|---|
| the committed pair, pre-fix | 60 | 55 | 5 (`espnow-rx[1..5].gap`, 2 vs 1) |
| the same pair re-run post-fix | 60 | **60** | **0** |

Both machines, both directions. The five fields the report named as the only
divergence are the five the fix closes, and it moves nothing else — which is
the shape a correct fix was required to have.

**What the fix did not do, and someone still owes.** The emulated pair's four
committed transcripts (`lp-emu/transcripts/esp32c6/espnow-broadcast/`, `t1`
and `t2`, one per machine) predate it and still carry `"gap":2`; transcripts
were fenced for this change, so they want re-recording separately, and the
60-of-60 row above was measured on a scratch re-run rather than on a committed
capture. Nothing was tuned toward silicon: the delivery was made correct, and
`gap` reaching 1 — the figure two real XIAO C6s record on every one of their
records, per
`docs/reports/2026-09-09-espnow-broadcast-two-board-silicon-replay.md` —
followed from it.

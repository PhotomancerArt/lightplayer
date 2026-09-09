---
status: open
logged: 2026-09-08
area: lp-emu-esp32c6 radio window (`periph/wifi_stub.rs`), the RX delivery path
found_by: M4 P3, the first payload on this machine that both sends and receives repeatedly
related:
  - lp-emu/esp/lp-emu-esp32c6/tests/espnow_broadcast_pair.rs (`the_air_surfaces_only_every_other_delivered_frame` — the pin)
  - lp-emu/esp/lp-emu-esp32c6/src/periph/wifi_stub.rs (the RX cursors `+0x4088` / `+0x408c`, the reload strobe `+0x4080`, the event bit 14)
  - lp-emu/esp/lp-emu-esp32c6/src/lockstep.rs (`Lockstep::run_until`, `offer_air_frame`)
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

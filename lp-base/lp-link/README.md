# lp-link

A small sans-IO link layer for LightPlayer's device transports: USB serial,
BLE, the classic UART, and later UDP and WebSocket. It sits between a raw pipe
and the wire messages, on **both** ends: the board, Studio (wasm), `lp-cli`,
and the emulator tools run the same crate.

> **Status: in the product, on USB and BLE.** USB: C6 and S3 silicon and
> their emulators, Studio's Web Serial and emulator-tab providers, `lp-cli`'s
> native serial and `serial:tcp`/`serial:ws`. It was built and measured in
> the investigation `lp2025/2026-09-26-1720-reliable-device-link`, proven on
> a C6 in the `test_comms_lab` firmware, and cut over in
> `lp2025/2026-09-27-0215-lp-link-usb-cutover`. BLE: the C6's radio link
> (`fw-esp32-common`'s `radio_link/`) and Studio's Web Bluetooth provider,
> cut over in `lp2025/2026-09-28-1445-ble-on-lp-link` — Datagram framing (one
> frame per GATT write/notification, `max_payload` capped to the
> connection's negotiated MTU), with no ATT long-write reassembly path. The
> classic ESP32's UART and `fw-emu` still speak the old `M!`-line framing
> until their own milestones bring them onto lp-link too. The decision is
> `docs/adr/2026-09-27-lp-link-one-comms-layer.md`; BLE's own is
> `docs/adr/2026-09-24-ble-transport.md`'s dated Amendment.

## Why it exists

Before lp-link the wire had framing and nothing else: no checksum, no resend,
no shared notion of "the other end restarted". Every lost byte became an
app-level failure. The 2026-09-26 investigation found that the board and USB
lose nothing, while macOS + Chromium Web Serial drops ~1 KB runs whenever the
stream carries `0xFF`
(`docs/defects/2026-09-26-web-serial-on-macos-drops-bytes-of-packed-frames.md`).
Our own edges have lost bytes too (esp-hal's USB ISR, BLE long writes, the
classic UART). lp-link makes delivery reliable end to end, and counts every
recovery so edge bugs stay visible.

## Where it runs

lp-link itself only frames bytes; the code that owns a port and speaks the
wire's messages over channel 1 lives at each edge:

- **Board, USB:** `lp-fw/fw-esp32-common/src/usb_link/` — one
  `Link<SelectiveRepeat>` per boot, driven by `usb_link_task.rs`'s link task,
  with replies serialized by `usb_link_transport.rs` (`UsbLinkTransport`, a
  `ServerTransport`). The C6 and S3 enable it behind the `usb-link` feature;
  the classic (v3) does not.
- **Board, BLE (C6 only):** `lp-fw/fw-esp32-common/src/radio_link/` — one
  `Link<SelectiveRepeat>` per open GATT connection (Datagram framing), owned
  by `RadioLinkPort` (heap-leaked, one slot per `RADIO_LINK_SLOTS`) and
  driven by `ble_connection.rs`'s per-connection loop, with replies
  serialized by `link_mux_transport.rs` (`LinkMuxTransport`, wrapping the USB
  transport and sharing its `FRAME_BUF` lease for any reply too big for the
  send ring). Behind `fw-esp32-common`'s `radio-link` feature, pulled in by
  `fw-esp32c6`'s `ble` feature (which also brings up the BT stack itself).
- **Native host:** `lpc_wire::WireLinkPort` — the one type every native
  reader drives (a real serial port, `serial:tcp`, `serial:ws`, the fake
  board double). `lpa-client`'s `transport_serial/link_pump.rs` and `lp-cli`'s
  tools own the port and poll it. No native BLE transport exists (only
  Studio and `spikes/ble-lab` speak BLE).
- **Studio (wasm):** `lpa-link`'s `LinkPortService` (`device_link/
  link_port_service.rs`) — one per open port, wrapping the same
  `WireLinkPort`, for the Web Serial provider
  (`providers/browser_serial_esp32/`), the emulator-tab provider
  (`emulator_tab_link.rs`), and now the Web Bluetooth provider
  (`providers/browser_ble/`, `BleWire` over a `Link<SelectiveRepeat>` on
  `LinkConfig::ble()`).
- **Tools:** `lpc_wire::WireLinkSniffer` — a passive decoder with no session
  of its own, for `lp-cli wire unpack`, the emulator's wire tap, and
  `lp-cli record timeline`.

USB and BLE are on lp-link today (D3 of the USB cut-over plan; the BLE
cut-over is `lp2025/2026-09-28-1445-ble-on-lp-link`). The classic's UART and
`fw-emu` still run the pre-lp-link `M!`-line framing (`lp-fw/fw-esp32-common`'s
`server_msg.rs`, `StreamingMessageRouterTransport`) and their own hosts
(`lpc_wire::WireStream`), unaffected by anything below.

### The proto channel's payload

Channel 1 carries one whole wire message per link message — no `M!` prefix,
no trailing newline; the link's own framing already delimits it
(`lpc_wire::link_payload`):

- **Board → host:** the first byte tags the payload. `{` (`PAYLOAD_TAG_JSON`)
  is plain JSON — the `M!{json}` line's JSON, byte for byte; `L`
  (`PAYLOAD_TAG_PACKED`, the same byte JSON Pack already uses as its frame
  kind) is a learned-dictionary packed frame, written *without* COBS (the
  link already framed it, so there is nothing left to escape).
- **Host → board:** always JSON. Hosts never pack; the device needs only a
  decoder.
- **Packed opt-in, once per `Up`.** `SetEncoding` is an ordinary proto
  request the host sends after the link comes up; only once the board
  answers, over the link, may it write `L` frames. A capture or a plain
  serial monitor that never asks sees nothing but JSON.
- **The hello is first.** The board's `ServerHello` is the first proto
  message after every `Up` (and still answers a direct request for it). The
  boot marker line stays raw text, outside any frame.
- **Both ends reset the payload state together, with the link.** On `Up` or
  `Reset` the board's packed mode reverts to JSON and the host drops its
  learned table; a fresh opt-in follows the next `Up`. There is no
  cross-session dictionary, no epoch, and no re-ask: a payload that fails to
  decode restarts the link instead, because over a link this reliable a
  desync is a bug to count, not a state to recover from mid-session.
- **A link `Reset` fails in-flight requests at once.** The host surfaces it
  as a link-reset event that ends any pending conversation immediately,
  rather than waiting out an idle timeout.
- **A slow writer is not a lost frame.** Loose console text is handed up
  after `idle_flush` (50 ms on USB) of quiet, but a half-received frame waits
  `frame_abandon` (3 s): the C6 writes a frame in 64-byte packets and yields to
  a render tick (~80 ms) between them, and a page's read pump can sit behind a
  long task. At 50 ms each resend of a split frame was abandoned again, and
  the 2026-09-27 rehearsal saw 4 s stalls on every palette cross-fade.

### Logs

The board's `log`/printf output goes into a fixed `LogRing`, drained onto
channel 2 (best-effort) while the link is up. A host renders each channel-2
record, and any raw `Text` outside a frame (early boot, the ROM banner, a
panic), as the same console lines Studio and `lp-cli` always showed.

## Layering

```
wire messages (JSON / learned-dictionary packed, request/response by id)
── lp-link ─────────────────────────────────────────────────────────────
  channels:  0 control · 1 proto (reliable) · 2 log (best effort) · up to 8
  session:   random nonce per boot/page load; SYN handshake → Up / Reset on BOTH ends
  recovery:  selective repeat + SACK bitmap, RTT-based timer, tail-loss probe, receiver window
  frame:     4-byte header (kind/channel/fragment · seq · cumulative ack · window) + payload + CRC-32C
  framing:   stream   → 0x00 · COBS-FF(frame) · 0x00   (no 0x00 or 0xFF on the wire)
             datagram → one frame per BLE notification / UDP packet / WS message
pipe:        USB-Serial-JTAG | BLE NUS | UART | UDP | WebSocket
```

**Scheduling between channels.** The sender keeps one queue per reliable
channel and cuts the next frame from the lowest-numbered channel that has
something waiting: a control message queued behind a 16 KiB proto reply goes
out at the next frame boundary, not after the reply. Within a channel, order is
kept. The receiver reassembles one message per channel, so fragments of two
channels can interleave. Order across channels is not promised; the delivery
property is per channel. Log datagrams get a fair share: after
`datagram_every` (default 4) reliable data frames in a row, a queued datagram
goes next, so a busy proto stream cannot starve the log. All channels still
share one sequence space, so a *lost* proto frame holds back the control frames
behind it until it is resent (head-of-line blocking under loss; a per-channel
sequence space is future work if a measurement ever calls for it).

## Principles

1. **End to end, because every hop is "reliable" and bytes still get lost.**
   USB and BLE check and retry in hardware; our losses happened between hops
   (the host tty, our ISR and FIFO code, the page). Only a check at the two real
   ends covers them all.
2. **Count every recovery, never hide it.** `LinkCounters` (resends, damaged,
   stale-session, duplicates, lost datagrams, resets by reason) ride the
   heartbeat and the recorder.
3. **Both ends reset together.** A reboot, reload, replug or give-up gives one
   `Reset` on each side, and per-link state above (the learned dictionary,
   pending requests) resets in step.
4. **One design, tuned per transport.** Presets in `link_config.rs`: `usb()`,
   `ble()`, `udp()`, `ws()`. WS/TCP use `NoArq` (channels and lifecycle
   only).
5. **Logs are traffic.** `LogRing` + `link_log!` + a `log` adapter. The ring
   keeps the newest lines while the link is down and reports what it dropped.
6. **Stay readable to a plain serial monitor.** Bytes outside frames arrive as
   `Text`. A panic writes a raw `0xFF` text mark first (it can't occur inside
   a COBS-FF frame).
7. **Small and ours.** No_std + alloc, time injected (`Micros`), no executor.
   Prior art was read as specs only; no code was copied.

## Sending without a copy: external messages

A board that already serializes its reply into a buffer it owns (the
firmware's 16 KiB frame buffer) should not pay a second 16 KiB for the link's
send ring. `Link::send_external(channel, len)` queues a reliable message by
length alone; `poll_transmit_with(now, source)` then copies each fragment
from the caller's bytes (`source(offset, out)`) straight into the transmit
window, which keeps them for resends anyway. So:

- nothing lands in the send ring, and `send_budget` can be small (control
  messages and small replies only; a ring message longer than it is
  `TooBig`);
- the caller keeps its buffer unchanged while `external_in_flight()` is
  true, that is until every byte has been cut into frames; after that the
  window holds the rest and the buffer is the caller's again;
- at most one external message at a time (`Full` otherwise); it keeps its
  place in its channel's order, and a lower channel still overtakes it at a
  frame boundary;
- `cancel_external()` withdraws it only before its first fragment is cut.
  After that the peer may hold part of it, so it can only be finished or
  abandoned with the session (`restart`, or any reset, which drops it and
  hands the buffer back).

`poll_transmit` without a source leaves an external message waiting.
`keep_reassembly` is the receive side's twin: a reassembly buffer grown past
it is released once its message is delivered, so one large upload does not
pin `max_message` bytes on a board for the link's life (the presets keep
`max_message`, today's behaviour).

## Where things are

| file | concept |
|---|---|
| `link.rs`, `link_config.rs`, `link_event.rs`, `link_counters.rs` | the `Link<A: Arq>` state machine, presets, events, counters |
| `frame.rs`, `crc.rs`, `cobs.rs`, `deframer.rs` | header and SYN, CRC-32C (keyed with the session), COBS and COBS-FF, stream deframing plus text passthrough |
| `arq/` | `SelectiveRepeat` (chosen), `GoBackN` (`StopAndWait` = window 1), `NoArq` |
| `rtt_estimator.rs`, `seq_num.rs` | RFC 6298 timer with Karn's rule, sequence arithmetic |
| `send_queue.rs`, `tx_queue.rs`, `datagram_queue.rs`, `inbox.rs` | the fixed buffers: messages waiting to be cut into frames, the transmit window, log datagrams waiting to go, and reassembly plus the application's event queue |
| `log_ring.rs` | the board-side log ring and `link_log!` |
| `lab/` | the comms-lab soak protocol (`LabBoard`/`LabHost`) |
| `sim/` (feature `sim`) | the deterministic fault-injecting simulator: USB/BLE/UDP/WS pipe models; drop, corrupt, duplicate, reorder, truncate, stall, reboot |

## Message budget

`max_message` is 17 KiB in every preset (`MAX_MESSAGE`): the wire's one
message budget, 16 KiB (`PROJECT_READ_FRAME_MAX_BYTES` in
`lp-core/lpc-wire/src/budget.rs`), plus slack. lp-base cannot depend on
lp-core, so the edge that wires the link to the wire asserts that the two
agree. `send()` refuses a longer message with `TooBig`. A longer message
arriving from a peer with a bigger limit is dropped fragment by fragment to its
end and counted (`LinkCounters::oversize_messages`); its frames are still
acknowledged, so the sender is not stuck resending, and the session carries
on. `LinkConfig::validate` checks that one largest message fits both the send
budget and the receive budget; every preset passes it.

## Memory

A link's RAM is fixed by its `LinkConfig`. `Link::new` allocates every buffer
the link needs, and after that the only allocation in steady state is the
`Vec` each delivered message (or text chunk) is handed to the application in:
exactly one per message, exactly its length.
`tests/no_steady_state_alloc.rs` checks this with a counting allocator over
40,000 steps of two-way traffic with loss and damage.

Allocated once, in `Link::new`:

| buffer | size | holds |
|---|---|---|
| send ring (`send_queue.rs`) | `send_budget` bytes + `send_queue` descriptors | reliable messages accepted by `send()`, not yet cut into frames |
| transmit window (`tx_queue.rs`) | `tx_window × max_payload` | frames sent and not yet acknowledged |
| reorder buffer (selective repeat) | `rx_window × max_payload` | frames that arrived past a gap |
| datagram slots (`datagram_queue.rs`) | `datagram_queue × max_payload` | log datagrams waiting to go |
| frame scratch | about 3 largest encoded frames | encode, decode, deframing |

Grown on demand, up to a limit, and then kept:

| buffer | limit |
|---|---|
| reassembly buffer, per reliable channel | `max_message` (it grows by doubling, clamped) |
| queued events for `recv()` | the receive budget: each queued event is charged its bytes plus 64 (`EVENT_COST`), and a newer reset replaces an unread one |

`Link::ram_bound(&cfg)` adds all of this up for the worst case, and
`link.ram_bytes()` reports what a link holds now. The simulator checks
`ram_bytes ≤ ram_bound` in every scenario, including the delivery property's
random fault schedules. `just link-bench ram` prints both (host, 64-bit; a
32-bit target's event queue and descriptors are about half the size). With the
`usb()` preset, a link holds about 43 KB at its busiest in the simulator,
most of it the 24 KB send ring. The worst case is much larger (about 128 KB
on the host, about 114 KB on the C6) because it assumes both reliable channels
are reassembling a `max_message` message while the receive budget is full and
unread.

The send ring is sized at one largest message plus what queues behind it. When
it is full, `send()` returns `Full`; it does not grow.

## Time

Time is `Micros`, a `u64` count of microseconds from any epoch the edge likes.
That is an exception to the repo's rule of caller-supplied `f64` epoch seconds
(`docs/adr/2026-07-06-sans-io-core.md`), for two reasons: the C6 has no FPU, so
`f64` arithmetic on every frame would be soft-float, and the link's times are
relative durations (round trips, retransmit and ACK timers, idle flushes), never
timestamps shown to anyone. The edge passes its monotonic clock in; nothing in
the crate reads a clock.

## Fuzzing

`tests/decoder_fuzz.rs` feeds a live link arbitrary input and checks it after
every step: random bytes in random chunks on a stream, random datagrams, and
crafted frames that pass the checksum with any header and body (SYNs with
foreign nonces, data for sequence numbers never sent, fragments that fit no
message, ACKs for nothing), interleaved with a real peer's traffic. Nothing may
panic, the link may never hold more than `Link::ram_bound`, and no counter may
go backwards. When the input stops, the peer and the link must be up again with
everything acknowledged. It runs 256 cases per framing in the normal test run;
`just link-fuzz` runs 20,000 (about 12 s in release). It is proptest, not
cargo-fuzz, so the crate keeps no fuzzing dependency. Its first run found a
real stall (two channels each mid-way through a large message closed the
receive window with neither able to finish), now a named scenario test.

## Prior art (specs only)

| source | what we took |
|---|---|
| HDLC / PPP framing (RFC 1662) | frames on a stream with a delimiter and a checksum; text allowed outside frames |
| COBS (Cheshire & Baker 1999) | delimiter-safe stuffing with a bounded overhead; COBS-FF is our extension that also excludes `0xFF` |
| SLIP (RFC 1055) | the negative example: framing without a checksum |
| V.42 LAPM + V.42bis | compression with a learned dictionary is only safe over a reliable link that resets both ends together |
| MIN | sizing for a microcontroller: small windows, a reset handshake, text passthrough |
| Bluetooth L2CAP ERTM / streaming | a reliable mode and a no-resend mode in one protocol; selective reject |
| KCP | selective repeat with fast resend, tuned for latency on lossy datagrams |
| TCP (RFC 9293, 6298, 2018, 8985) | RTT timer and Karn's rule, SACK, receive window, reordering allowance, tail-loss probe |
| QUIC (RFC 9000, 9002) | a session identity both ends agree on; several channels on one link |
| CRC-32C (Castagnoli; Koopman) | better detection than CRC-32/IEEE at our frame sizes, for the same code |

Not borrowed: PPP's option negotiation, TCP congestion control (needed only
for UDP beyond a LAN), QUIC's encryption and migration.

## Measured (see the plan's `reports/`)

- Simulator, USB at 1 % damage: selective repeat 227 KB/s, 0 messages lost
  (no link layer: 7.7 % lost). The delivery property holds across 15,000
  random fault schedules with reboots.
- CRC over 4 M damaged frames: no checksum passed 1.4 % as plausible frames,
  CRC-16 passed 1, CRC-32C passed 0.
- Silicon (C6, `test_comms_lab`), 30-minute USB soaks via native readers and
  headless Brave Web Serial: 0 damaged frames and 0 app errors with COBS-FF.
  Plain COBS gave 103–437 damaged frames, all resent.
- Cost in the C6 product image (M3, before the hardening below): +14.2 KB
  flash, ~6 KB RAM per link in steady state plus a 4 KB log ring, ~1 s CPU per
  MB moved (~150 cycles per byte).

After the hardening (plan `lp2025/2026-09-27-0155-lp-link-hardening`):

- CPU, host (`just link-bench codec`, M2 Max, 256-byte frames of random bytes;
  compare only against each other): CRC-32C 204 → 424 MB/s (byte table);
  COBS-FF encode 292 → about 1,100–1,350 MB/s and decode 631 → about 1,000
  MB/s (run copies). Not yet re-measured on the C6.
- Flash, `just link-size` (selective repeat, riscv32imac, over a baseline with
  alloc and `core::fmt`): 16.1 KB → 21.5 KB. About 1 KB of that is the CRC
  table, the rest the fixed buffers, per-channel queues and their checks. The
  product-image figure above was not re-measured.
- RAM, `usb()` preset on the C6 (32-bit), by the formula in "Memory": about
  39 KB allocated in `Link::new` (24 KB send ring, 8 KB datagram slots, 2 KB
  each for the transmit and reorder windows), 2.7 KB frame scratch, plus what
  is queued for the application. The simulator's busiest USB run peaks at
  44 KB on the host. The steady-state figure went up because the buffers are
  now held for the link's life instead of allocated per message; the
  allocator no longer sees link traffic at all.

BLE (plan `lp2025/2026-09-28-1445-ble-on-lp-link`), host-test and emulator
figures only — **not yet silicon-validated** (the lab board was unavailable
this phase; the runbook is that plan's P5 phase file, "Silicon soak (pending
the board)"):

- RAM, per open GATT connection, host test at the real firmware config
  (`radio_link::link_mux_transport`'s `two_open_radio_links_cost_this_much_ram`
  and `the_board_config_holds_one_largest_reply_and_costs_less_than_the_preset`,
  `--nocapture`; 64-bit host figures, so a 32-bit target's descriptors and
  event queue are roughly half — never measured on silicon): 7,672 B at rest
  (session up, hello sent) for one link, 15,344 B for two; peak during a
  6 KiB upload on each link, 13,880 B / 27,760 B; worst case (`ram_bound`),
  74,664 B / 149,328 B. The unmodified `ble()` preset costs 33,344 B at rest
  for comparison — BLE's own board config (`radio_link_config.rs`) narrows
  `max_payload`, `send_budget`, `keep_reassembly` and `datagram_queue`
  well below it, the same discipline USB's heap follow-up used.
- Flash, `just fw-esp32c6-size-check` (image / 3,145,728 B): the BLE cut-over
  landed the image 96 B *smaller* than before it (2,949,984 → 2,949,888 B;
  deleting `line_joiner.rs`/`line_chunker.rs`/`prepared_write.rs` slightly
  outweighs the new radio-link-port and payload-codec code).
- Emulator idle heap (`lp-emu:esp32c6:t1`, BLE on, no connection — the
  emulator has no BLE air, so this is the closest an emulator gets):
  usedBytes 96,628 → 96,824 (+196 B, a leaked `RadioLinkPort` replacing
  432 B of what had been static `.bss` — net saving), largestFreeBlock
  185,172 → 184,932 (−240 B).

```bash
cargo test -p lp-link --features sim,lab   # unit tests, scenarios, proptests (CI runs this)
just link-soak 5000                        # the delivery property at depth
just link-fuzz 20000                       # the decoder fuzzer at depth
just link-bench                            # the tables; `codec` is CPU throughput
just link-size                             # riscv32 code size probe
just check-lp-link-targets                 # riscv32 and wasm32 builds, clippy with every feature
just link-lab-emu                          # the comms lab on the emulated C6
```

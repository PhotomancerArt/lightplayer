# lp-link

A small sans-IO link layer for LightPlayer's device transports: USB serial,
BLE, the classic UART, and later UDP and WebSocket. It sits between a raw pipe
and the wire messages, on **both** ends: the board, Studio (wasm), `lp-cli`,
and the emulator tools run the same crate.

> **Status: in the product on USB** (C6 and S3 silicon and their
> emulators, Studio's Web Serial and emulator-tab providers, `lp-cli`'s
> native serial and `serial:tcp`/`serial:ws`), **on the classic ESP32's
> UART0** (the DOM-Z-102's CH340 link, its emulator, Studio's Web Serial and
> `lp-cli`'s native serial behind a USB-UART bridge — wire proto 32) **and on
> BLE** (the C6's radio links, `fw-esp32-common`'s `radio_link/`, and
> Studio's Web Bluetooth provider — wire proto 37). It was built and measured
> in the investigation `lp2025/2026-09-26-1720-reliable-device-link`, proven
> on a C6 in the `test_comms_lab` firmware, cut over on USB in
> `lp2025/2026-09-27-0215-lp-link-usb-cutover`, on the classic's UART in
> `lp2025/2026-09-28-2015-classic-uart-on-lp-link` (emulator-validated; its
> desk walk is still open), and on BLE in
> `lp2025/2026-09-28-1445-ble-on-lp-link` — Datagram framing (one frame per
> GATT write/notification, `max_payload` capped to the connection's
> negotiated MTU), with no ATT long-write reassembly path. Only `fw-emu`
> still speaks the old `M!`-line framing, until a future milestone brings it
> onto lp-link too. The decision is
> `docs/adr/2026-09-27-lp-link-one-comms-layer.md`; BLE's own is
> `docs/adr/2026-09-24-ble-transport.md`'s dated Amendment.
>
> **The `secure` feature** (Noise NNpsk0 inside the SYN, then every frame
> sealed; [Secure links](#secure-links)) is built and proven against `snow`,
> the simulator, the fuzzer and a host end-to-end login test, and **off on
> every product link** until the Wi-Fi milestones (M6's LAN WebSocket is the
> first). Its decision is `docs/adr/2026-10-01-network-link-security.md`.

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
  `ServerTransport`). The C6 and S3 enable it behind the `usb-link` feature.
- **Board, the classic's UART0:** `lp-fw/fw-esp32-common/src/uart_link/`
  (feature `uart-link`), the same shape: `uart_link_task.rs` owns the `Link`
  on the **thread** executor, `uart_link_transport.rs` (`UartLinkTransport`)
  serializes replies, and `uart_link_config.rs`'s `uart_board_link_config()`
  is the board's cut of the `uart()` preset. The UART itself stays with
  `fw-esp32v3`'s `serial/io_task.rs` on the swi2 interrupt executor, now only
  a byte shuttle between UART0's FIFOs and two static pipes
  (`uart_link_pipes.rs`) — see
  `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`'s
  2026-09-29 amendment. A UART has no cable signal, so the board SYNs into
  the void until a host answers; `syn_backoff` (below) keeps that to one SYN
  every 1.6 s. The backoff covers the handshake only: a host that brings the
  link up and then goes quiet (a closed tab) leaves the board Established,
  sending keepalives and resending what is unacknowledged — ~4–6 frames/s
  for ~19 s at `lp-emu:esp32v3:t1` — until `max_retries` resets the link;
  only then does it back off. A plain serial monitor attached in that
  window sees binary frames, not silence.
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
  tools own the port and poll it. The port's preset comes from its USB
  vendor (`lpa_client::transport_serial::link_config_for_port`): Espressif's
  own `0x303a` is `usb()`, a USB-UART bridge (the classic's CH340, `0x1a86`)
  is `uart()`; a socket carries no vendor and stays on `usb()`, which a
  classic answers just as well because each end sends at most the window the
  other advertised. `lp-cli emu run --chip esp32v3 --host-link` hosts an
  emulated classic's UART0 in process, on the board's own `uart()`. No
  native BLE transport exists (only Studio and `spikes/ble-lab` speak BLE).
- **Studio (wasm):** `lpa-link`'s `LinkPortService` (`device_link/
  link_port_service.rs`) — one per open port, wrapping the same
  `WireLinkPort`, for the Web Serial provider
  (`providers/browser_serial_esp32/`), the emulator-tab provider
  (`emulator_tab_link.rs`), and the Web Bluetooth provider
  (`providers/browser_ble/`, on `LinkConfig::ble()`). The Web Serial
  provider picks the preset by the same vendor rule as native
  (`provider/usb_vendors.rs`, `link_config_for_usb_vendor`). Neither
  `lp-cli emu serve` nor the tab backing holds an emulated classic yet.
- **Tools:** `lpc_wire::WireLinkSniffer` — a passive decoder with no session
  of its own, for `lp-cli wire unpack`, the emulator's wire tap, and
  `lp-cli record timeline`.

Only `fw-emu` still runs the pre-lp-link `M!`-line framing (its own, in
`lp-fw/fw-core`'s `transport/serial.rs`) and its hosts
(`lpc_wire::WireStream`), unaffected by anything below. The classic's
`StreamingMessageRouterTransport` — the `M!` transport only it used — is
gone, and so are `fw-esp32-common`'s `M!` line decoder and loss counters,
which the BLE links read with until they moved.

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
  secure:    (feature `secure`) Noise NNpsk0 in the SYN; header ‖ ctr ‖ sealed body ‖ tag ‖ CRC
pipe:        USB-Serial-JTAG | BLE NUS | UART | UDP | WebSocket   (a TLS socket, where one is used, is under all of it)
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
4. **One design, tuned per transport.** Presets in `link_config.rs`: `usb()`, `uart()`,
   `ble()`, `udp()`, `ws()`. WS/TCP use `NoArq` (channels and lifecycle
   only). A preset is what a **host** runs; a board takes its own cut of one
   (the C6's `UsbLinkShared::config`, the classic's
   `uart_board_link_config`), smaller buffers and a slower resend floor,
   because a host queues upload-sized requests through `send()` and a board
   sends its replies by `send_external`. `uart()` is `usb()` with windows of
   4 (its doc comment says why the 40 ms `min_rto` is a host's floor and the
   classic's board takes 200 ms). `syn_backoff` (0 in every preset) doubles
   the gap between unanswered SYNs up to that many times; the classic's
   board sets 4 — 100, 200, 400, 800 ms, then one every 1.6 s — because a
   UART has no cable signal and a board nobody answers would otherwise SYN
   at 10 Hz forever.
5. **Logs are traffic.** `LogRing` + `link_log!` + a `log` adapter. The ring
   keeps the newest lines while the link is down and reports what it dropped.
6. **Stay readable to a plain serial monitor.** Bytes outside frames arrive as
   `Text`. A panic writes a raw `0xFF` text mark first (it can't occur inside
   a COBS-FF frame).
7. **Small and ours.** No_std + alloc, time injected (`Micros`), no executor.
   Prior art was read as specs only; no code was copied.
8. **A plain receiver reads a SYN's 12-byte prefix and ignores the rest and
   any unknown flag bit.** Of the flags byte it reads only `established`
   (bit 0); every other bit, and every byte after the 12th, belongs to an
   extension it does not know. This is the link's growth path once boards
   update over the air through it (the update protocol's ADR, OTA Part B): a
   new feature arrives the way `secure` did, as a SYN flag and an extension
   the old end ignores, so a fielded board still comes up for a newer host.
   It changes what a link *accepts*, never what it sends: a plain link still
   sends exactly 12 bytes with every other bit zero
   (`tests/plain_bytes_golden.rs`); `tests/plain_syn_tolerance.rs` pins the
   rule. A body shorter than 12 bytes is still not a SYN.

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
| `sim/` (feature `sim`) | the deterministic fault-injecting simulator: USB/BLE/UDP/WS pipe models; drop, corrupt, duplicate, reorder, truncate, stall, reboot; `secure_sim.rs`, `sim_entropy.rs`: secure endpoints with a scripted key table |
| `secure_channel/` (feature `secure`) | the crypto core: HMAC-SHA256 and Noise's HKDF (ours, from the specs), CipherState over ChaCha20-Poly1305, the NNpsk0 initiator and responder, the replay window, `KeyId`/`Psk`/`SecureRole`/`SecureEvent` |
| `link/secure_handshake.rs`, `link/sealed_frames.rs`, `link/secure_state.rs`, `frame/secure_syn.rs` (feature `secure`) | the handshake inside the SYN, the sealed frame, a secure link's state, the SYN extension codec |

## Secure links

The `secure` feature (`docs/adr/2026-10-01-network-link-security.md`) runs
**`Noise_NNpsk0_25519_ChaChaPoly_SHA256`** merged into the session handshake,
then seals every frame. It is for untrusted network links (the LAN
WebSocket, the relay); USB and UART never use it (the cable is the trust).

**The SYN.** Its 12 bytes do not change; flags bit 1 is `SECURE` and bits 2–3
name what follows (`frame/secure_syn.rs`):

| content | sent by | after the 12 bytes | body |
|---|---|---|---|
| presence | responder, nobody heard | — | 12 B |
| msg1 (`psk, e`) | initiator, every SYN while connecting | `key_id[16] ‖ e_i[32] ‖ tag[16]` | 76 B |
| msg2 (`e, ee`) | responder | `e_r[32] ‖ enc(responder nonce)[4] ‖ tag[16]` | 64 B |
| refusal | responder | `reason[1] ‖ retry_after_ms[4]` | 17 B |

**The sealed frame.** `header[4] ‖ ctr[4] ‖ ciphertext ‖ tag[16] ‖ crc`
(`SEAL_OVERHEAD` = 20): ChaCha20-Poly1305 with the header as associated data,
one counter per direction, every transmission re-sealed (resends included).
`max_payload` stays plaintext; `LinkConfig::secured()` takes the overhead off
for a transport with a hard frame size (`ble().secured()`).

**The lifecycle, in five lines.**
1. The initiator (`SecureRole::Initiator { key_id, psk }`) sends msg1 on every
   SYN; one ephemeral per session, so a resend is the same bytes.
2. The responder raises `SecureEvent::KeyLookup`; the edge answers
   `provide_keys(key_id, candidates)` or `refuse(...)` (2 s, then `Busy`).
3. The first candidate whose PSK verifies msg1 gets msg2; the responder is
   half-open (keys split, still connecting).
4. The initiator is up on a msg2 that answers its nonce and sends a sealed
   ACK at once; the responder is up on the first frame that opens
   (`session_auth()` names the key and the candidate).
5. Every reset wipes the keys; the next session is a fresh handshake. An
   established responder resets only for a msg1 that verifies.

**The API** (feature `secure`): `Link::new_secure(cfg, nonce, role, entropy)`,
`poll_secure_event`, `provide_keys`, `refuse`, `retry_with`, `session_auth`,
`is_secure`, `ram_bound_secure`. `LinkEvent` is unchanged. Entropy is a
`fn(&mut [u8])` the edge supplies (32 bytes per handshake per end).

**Replay and failure.** ARQ links keep a 64-frame window: a replay or a bad
tag is dropped and counted (`replays`, `seal_failures`) and ARQ resends. A
no-ARQ link (WebSocket, relay) takes only the next counter: a gap
(`counter_gaps`) or a bad tag resets the session. A CRC failure is still
`bad_frames` (line damage); a tag failure past the CRC is a forgery or a bug.
A plain link hearing a secure peer counts `secure_required` (with the feature
on); a secure link hearing a plain peer raises `PeerNotSecure`. The sniffer
holds no keys and reports a secure capture's frames as `SniffEvent::Sealed`.

**Running its tests:**

```bash
cargo test -p lp-link --features sim,lab,secure   # snow oracle, RFC vectors, handshake legs, sim, fuzz, alloc
cargo test -p lp-link --features sim,lab          # plain: the golden bytes are unchanged
just link-fuzz 20000 && just link-soak 5000       # both include the secure cases
just check-lp-link-targets                        # rv32 + wasm32 with `secure`, no getrandom, no precomputed tables
just link-size                                    # `crypto` and `sr-secure` beside the plain variants
```

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
figures only — **not yet silicon-validated** (the runbook for the silicon
soak is that plan's P5 phase file, "Silicon soak (pending the board)"):

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

Secure links (feature `secure`, plan `lp2025/2026-10-01-1843-secure-link`):

- Flash, `just link-size` (riscv32imac, over the probe's baseline): the
  crypto alone 25,274 B (sha2 included); a selective-repeat link built secure
  54,076 B against the plain link's 21,866 B. The plain variants are
  byte-for-byte what they were.
- On the C6 (`lp-fw/fw-esp32c6` feature `diag_secure_link`, the product image
  with a secure pair run at boot; `lp-emu:esp32c6:t1@f06c77c6d`, where a PMU
  cycle is an instruction — **never time**): ~25 KB of flash for the feature
  (curve25519 9.5 KB, ChaCha20-Poly1305 3.7 KB, `secure_channel` 5–5.8 KB,
  the link's secure paths 5.7 KB, sha2 0.9 KB); +752 B of RAM per link over
  a plain one (`ws()` with the board's cut); 12.07 M instructions per
  handshake (both ends); 34,361 instructions per 64 B message sealed and
  opened, 531,329 per 2 KB; **3,860 B of stack** for a handshake through the
  links (3,204 B for the Noise core alone). Silicon timing: owed (M6's desk
  walk).

## Running it

```bash
cargo test -p lp-link --features sim,lab   # unit tests, scenarios, proptests (CI runs this)
cargo test -p lp-link --features sim,lab,secure   # ...and the secure channel's (CI runs this too)
just link-soak 5000                        # the delivery property at depth
just link-fuzz 20000                       # the decoder fuzzer at depth
just link-bench                            # the tables; `codec` is CPU throughput
just link-size                             # riscv32 code size probe
just check-lp-link-targets                 # riscv32 and wasm32 builds, clippy with every feature
just link-lab-emu                          # the comms lab on the emulated C6
```

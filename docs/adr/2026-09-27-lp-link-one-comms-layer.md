# ADR: lp-link — one reliable comms layer under the wire, on every transport

- **Status:** proposed (2026-09-27; decided at G1 of
  `lp2025/2026-09-26-1720-reliable-device-link`)
- **Deciders:** Yona
- **Evidence:** planning dir `2026-09-26-1720-reliable-device-link/reports/`
  (`REPORT.md`, `m1-loss.md`, `m2-link-design.md`, `m3-on-target.md`);
  `docs/defects/2026-09-26-web-serial-on-macos-drops-bytes-of-packed-frames.md`;
  `docs/defects/2026-09-26-esp-hals-usb-isr-clears-a-tx-edge-it-did-not-handle.md`.

## Context

The Studio ↔ board wire had framing (`M!` lines, `0x00 'L' COBS 0x00` packed
frames) and nothing else: no checksum, no retransmission, no shared notion of
"the other end restarted". Each conversation built its own partial recovery
(idle budgets, re-opt-in on dictionary desync, seq checks on streamed reads,
the not-draining latch, the RESYNC marker, the pause-the-pump rule for
exclusive readers). When bytes went missing, every loss surfaced as an
app-level failure: an ejected editor, a request that never ended, Save that
did nothing.

The 2026-09-26 investigation found:

1. **The board and USB are clean.** 60 MB were read natively with 0 lost,
   and the count matched the board's own.
2. **macOS + Chromium Web Serial drops bytes.** Chromium opens the tty with
   `PARMRK` set and `IGNBRK` clear. The kernel doubles each `0xFF`, and
   IOSerialBSDClient's unsigned free-space count wraps. The tty then drops
   ~1 KB. Any wire that carries `0xFF` is exposed; JSON never was.
3. **Our own edges have had real losses too.** The S3/C6 stale
   `serial_in_empty` race is now gated. esp-hal's ISR clears a TX edge it
   didn't handle (250 ms stalls). There have been BLE long-write and
   chained-ACL losses, and there are open UART interleave and overflow
   defects on the classic.
4. **The learned dictionary (ADR 2026-09-25) is stateful compression without
   a reliable link under it.** One lost frame desyncs it until a re-opt-in.
   The prior art (V.42bis over V.42 LAPM) puts compression on top of an
   error-correcting link that resets both ends together.

We have two transports (USB-Serial-JTAG and BLE NUS) and a third on the
horizon (WiFi: UDP / WebSocket).

## Decision

Design, principles and prior art in one place: `lp-base/lp-link/README.md`.

Put **one sans-IO link layer, `lp-link`** (`lp-base/lp-link`, no_std +
alloc, time injected), between each byte or datagram pipe and the wire
messages, on both ends. The board, Studio (wasm), `lp-cli` and the emulator
host tools run the same crate.

```
wire messages (JSON / learned-dictionary packed; request/response by id)
── lp-link ──  channels · selective-repeat ARQ · session handshake · counters
   framing:   stream  → 0x00 COBS-FF(frame) 0x00   (no 0x00, no 0xFF on the wire)
              datagram → one frame per BLE notification / UDP packet / WS message
pipe:         USB-Serial-JTAG | BLE NUS | UART (classic) | UDP | WebSocket
```

- **Frame:** a 4-byte header (kind + fragment bits + channel, seq, cumulative
  ack, window), the payload, and **CRC-32C keyed with both session nonces**.
  Stale-session frames cannot pass.
- **Channels:** `0 control`, `1 proto` (the wire messages; reliable,
  fragmented up to the message budget), `2 log` (best-effort, sequence-
  counted so losses are counted). Up to 8.
- **Reliability:** selective repeat, with a SACK bitmap, RTO from smoothed RTT
  (Karn, backoff), a tail-loss probe, and receiver-advertised flow control.
  Per-transport presets: USB (256 B payload, window 8, RTO floor 40 ms),
  BLE (236 B, window 8), UDP (1 KB, window 16, reorder allowance 3), and
  WS/TCP (**no-ARQ**: handshake and channels only).
- **Session:** a random nonce per boot or page load, and a SYN exchange.
  A restart, replug, reload or give-up (20 unacked resends) yields `Reset` on
  **both** ends at once. Everything per-link resets on it: the learned
  dictionary, pending requests (they fail immediately with "link reset"), and
  the hello (sent as the first proto message after `Up`).
- **Logs and printf are a link channel.** The board's `log` backend and a
  printf-style macro write into a fixed `LogRing`. It is drained into
  channel 2 while the link is up; while it is down it keeps the newest lines
  and reports a dropped count. Text outside frames stays legible to a plain
  serial monitor (ROM banner, early boot, panics). The panic handler writes a
  raw `0xFF` "text mark" before its text, since `0xFF` can never appear inside
  a COBS-FF frame.
- **Every recovery is counted** (`LinkCounters`: resends, damaged frames,
  stale-session frames, duplicates, datagram losses, resets by reason). The
  counters ride the heartbeat and Studio's recorder, so an edge bug shows up
  as a rising number, not a mystery.
- **One owner per port on the host.** The provider owns the port's `Link`;
  conversations (model pump, lens, push, card feed) share it by request id on
  the proto channel. This retires the exclusive-drainer / pause-the-pump
  convention (ADR 2026-09-01, 2026-09-06 — to be amended).

What this replaces, and removes: the `M!`-line and `0x00 'L'` framing on
serial, `RESYNC_OWED` and the `R` frame, the not-draining latch's reply-dropping
(the link's stall state replaces it), the dictionary's epoch/state
desync-and-re-opt-in path, the per-conversation idle budgets as a loss
detector (they stay only as "board busy" bounds), and the mixing of log text
and frames on one stream.

Migration follows the wire policy (AGENTS.md): no compatibility shims, one
`WIRE_PROTO_VERSION` bump, and the board, Studio and lp-cli land together.
Persisted formats are untouched.

## Consequences

- **+14.2 KB flash** on the C6 product image (measured; the headroom was ~278 KB).
  About 6 KB RAM per link plus a 4 KB log ring. ~1 s CPU per MB moved
  (well under 2 % at Studio's traffic), with a byte-table CRC and run
  copies in COBS-FF as known speedups.
- The Mac Web Serial loss disappears structurally: 0 damaged frames in 30-min
  soaks in headless Brave. Any residual edge loss is resent and counted.
- COBS-FF costs ~1.2 % in size, against 0.4 % for COBS.
- BLE gains end-to-end integrity and retransmission above the radio's own.
  **It is unmeasured on silicon**, and a BLE desk check is required before
  BLE switches.
- UDP across a real network will need congestion control. That is out of
  scope until the WiFi plan.
- `lp-cli wire unpack`, the emulator wire tap and `lp-cli record timeline`
  must learn to decode link frames.
- Chromium (`IGNBRK` when parity is off) and Apple (the unsigned wrap) get
  upstream reports. esp-hal already fixed its ISR clear upstream (esp-hal
  #6089/#6104, released in 1.2.0), and our 1.1.1 fork takes a back-port.
  None of them is relied on.

## Alternatives considered

- **Keep the app wire, add a CRC and per-request retries** (end-to-end only).
  Rejected: every conversation (streamed reads, lens, push, access) would need
  its own resend logic, and the learned dictionary would still desync.
- **Only keep `0xFF` off the wire** (COBS-FF on today's packed frames). This
  fixes the Mac loss and nothing else: no integrity, no recovery from other
  edges, no BLE story. It is kept as an optional first step if the link
  slips.
- **Stop-and-wait / go-back-N.** Measured slower (56 KB/s) or collapsing
  under reordering (UDP), respectively.
- **A third-party ARQ (KCP, MIN) or PPP stack.** Read as specs. We build tiny
  custom code (repo practice), keep it no_std, and control the size.

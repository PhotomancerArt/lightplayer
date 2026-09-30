# ADR: lp-link — one reliable comms layer under the wire, on every transport

- **Status:** implemented on USB (PR #854, 2026-09-27) and on the classic
  ESP32's UART (PR #884, 2026-09-29 — emulator-validated, desk walk
  pending; see the amendment at the end); accepted (2026-09-27, by Yona at
  G1 of `lp2025/2026-09-26-1720-reliable-device-link`)
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

## Status: implemented on USB (PR #854)

Milestone M2 of the rollout plan (with M4, the link-state UX, folded in) cut
USB over to `lp-link` on both ends: C6/S3 silicon, their emulators, Studio's
Web Serial and emulator-tab providers, and `lp-cli`'s native serial and
`serial:tcp`/`serial:ws` transports. BLE, the classic ESP32's UART and
`fw-emu` still run the pre-`lp-link` `M!` framing, on schedule for their own
milestones (M3, M5, and a future one). `WIRE_PROTO_VERSION` moved 29 → 30.
See `lp-base/lp-link/README.md`'s "Where it runs" for exactly what runs
where, and the ADR amendments to `2026-09-01-editor-lens-borrows-the-device-
wire.md`, `2026-09-06-shared-link-conversations-and-the-card-feed.md`,
`2026-09-24-json-pack-wire-encoding.md` and
`2026-09-25-learned-wire-dictionary.md` for what changed under each.

Two deviations from the rollout plan, decided by the implementing agent and
recorded for review in
`~/.photomancer/planning/lp2025/2026-09-27-0215-lp-link-usb-cutover/notes.md`
(D2, D3):

- **D2 — the exclusive-borrow / pause-the-pump convention was kept, not
  retired.** The plan's M2 had asked for it to go; with `lp-link` doing
  reassembly, the torn-partial hazard the borrow rule guarded against is
  gone, but rewriting `device_effects.rs` so conversations share the link
  by request id is a larger Studio-core refactor with no further
  user-visible gain once the link is reliable. Deferred to a future
  milestone, **M2b**.
- **D3 — scope is USB-Serial-JTAG only.** BLE (M3), the classic v3 UART
  (M5) and `fw-emu` (no hello, lossless by construction) keep `M!` until
  their own milestones; `fw-esp32-common` keeps the `M!` serializer for
  them, split into "payload" and "`M!` framing".

## Amendment 2026-09-29: implemented on the classic's UART (M5, PR #884)

Milestone M5 (plan `lp2025/2026-09-28-2015-classic-uart-on-lp-link`) moved
the classic ESP32's UART0 host link onto `lp-link`: `fw-esp32v3` on the
DOM-Z-102's CH340 and its emulator, Studio's Web Serial path and `lp-cli`'s
native serial behind a USB-UART bridge, and `lp-cli emu run --chip esp32v3
--host-link`. `WIRE_PROTO_VERSION` moved 30 → **32** (31 was held for the
Bluetooth milestone, PR #880). After it, BLE and `fw-emu` are the only `M!`
board links. D1 (layering) and D2 (pause-the-pump kept) apply unchanged:
the classic's Web Serial port is one more `LinkPortService`. The lens and
card-feed ADRs' 2026-09-27 amendments describe exactly that mechanism but
name USB, so each gained a two-line 2026-09-29 note extending them to every
`lp-link` port; nothing in them changed.

**The PR is still a draft.** Every claim below is emulated
(`lp-emu:esp32v3:t1@cfac9606f`); none is hardware-validated. The desk
walk's protocol is `hardware-walk-protocol.md` in the plan directory.

What was decided, and what resolved:

- **Where the `Link` lives (ruling DD20).** The classic's io_task keeps its
  shape from `2026-08-25-classic-uart-io-task-executor-isolation.md` (swi2
  interrupt executor, 1 ms pacer) but only moves bytes between UART0's
  FIFOs and two static pipes. The `Link` and the server transport run on
  the **thread** executor, as on the C6. A `Link` shared across the swi2
  preemption boundary would have needed a real lock (a `RefCell` there
  panics), and link work on the interrupt executor would have run on the
  interrupted task's stack.
- **`min_rto`: the plan's 40 ms hypothesis (its D4) did not hold, by
  design rather than by measurement.** D4 assumed the link would inherit
  the interrupt executor's 1 ms service. It does not (DD20): its timers and
  ACKs are serviced between engine ticks, 41–114 ms on a dome-scale project,
  longer than the ~80 ms tick that made the C6 raise its own floor. So the
  board's cut (`uart_board_link_config`) takes the C6's **200 ms**; the
  `uart()` preset keeps 40 ms, which is a *host's* floor. No silicon number
  exists for the classic yet; the C6's two corrections (`frame_abandon`,
  `min_rto`) came from silicon, and so may the classic's.
- **Presets are hosts'; boards cut their own.** `uart()` is `usb()` with
  windows of 4 and `usb()`'s budgets, because hosts queue upload-sized
  requests through `send()`. The board's RAM cuts (send budget 1,280 B,
  two log slots, `keep_reassembly` 512 B) live in its own config, as the
  C6's do. Replies go by `send_external` from the static frame buffer.
  Measured link RAM at rest 7,995 B (`Link::ram_bytes()`); the shipped
  image's idle heap `used` rose 16,636 → 23,944 B, and the load gate still
  reads ~35 KB above its 64 KiB floor with `catalog/projects/zook-dome`.
- **Preset by vendor id (plan D6).** Both hosts pick the preset from the
  port's USB vendor: Espressif `0x303a` → `usb()`, any bridge → `uart()`.
  A socket (`serial:tcp`/`serial:ws`) has no vendor and stays on `usb()`,
  which a classic answers equally, because each end sends at most the
  window the other advertised.
- **A UART has no cable signal (DD27).** A board nobody answers would SYN
  at 10 Hz forever, into a plain serial monitor. New `LinkConfig::
  syn_backoff` (a `u8` count of doublings, 0 in every preset, zero bytes of
  RAM): the classic sets 4, so 100, 200, 400, 800 ms, then one SYN every
  1.6 s; any byte received snaps it back.
- **A host that leaves mid-session (DD34, documented, not changed).** The
  backoff covers the handshake only. After a host that had the link up
  goes quiet, the board stays Established and sends keepalives and resends
  ~4–6 frames/s for ~19 s, until `max_retries` resets the link; then it
  backs off.
- **The nonce across a software reset (DD28).** The classic has no radio
  to seed its RNG, so a software reboot may repeat the random word, which
  would hide `PeerRestarted` from a host that stayed attached. The board's
  nonce is salted with a boot count kept in RTC fast RAM (survives a
  software or watchdog reset, not power-on). The emulator gained a real
  software reset to prove it (its RNG repeats by construction, the worst
  case).
- **UART0 has one writer after boot.** io_task's TX pipe takes only whole
  link frames and whole `[WS281X]` telemetry lines; `[MEM]`/`[JIT]`/`[stack]`
  became log records; a panic writes `0xFF` first. This closes
  `docs/defects/2026-08-02-serial-line-interleaving.md`, and lp-link's ARQ
  closes `2026-08-03-dev-file-sync-drops-on-uart-rx-overflow.md` and
  `2026-08-26-inbound-frames-longer-than-a-tick-lossy.md` structurally (an
  overflow is a counted resend; it can still happen).
- **Found and filed, not fixed:**
  `docs/defects/2026-09-29-the-classics-log-ring-drops-records-under-a-project-load-burst.md`
  — the 4 KiB log ring drops (and counts) records during a multi-output
  project load.

Evidence (all `lp-emu:esp32v3:t1`): `just walk-esp32v3-emu`'s three
readings of the frame agree over the link with 0 damaged frames and 0 or 1
resends (0 in P5's run; 1 on 2026-09-29 at `6134f415f`, counted by the host
with nothing damaged, so a resend timer rather than line damage — most
likely the host's 40 ms floor running out while a long server pass held the
board's link task off, the case P2 described; not traced); a `--uart-faults` soak (new: the C6's `--usb-faults` cut into
64-byte windows over the UART's byte stream) of five project loads at the
C6's 1 % mix, at ~2.5 %, and with 1 KiB damage runs finishes with **0 app
errors** each time, both ends counting the damage that reached them and
resending (`lp-cli/tests/emu_uart_link.rs`).

What the emulator cannot prove, and the desk walk must: real CH340K timing
and its macOS driver; real desk noise; the swi2 pacer and the thread-side
link under silicon's own interrupt latency (and so the 200 ms floor);
silicon's RNG and RTC fast RAM across a real reboot; and byte-level
interleaving on the real line. Two tooling gaps sit beside those:
`lp-cli validate record` cannot host a classic lp-link image yet
(`RunRequest::hosted()` hosts USB-Serial-JTAG only, so the classic's
validation arms stay pinned to pre-lp-link images), and Studio cannot reach
an emulated classic (`lp-cli emu serve` and the tab backing hold C6s only),
so the classic's Studio path is proven by the Web Serial conformance
suite's board double, not the firmware.

## Amendment 2026-09-30: implemented on BLE (M3, PR #880)

Milestone M3 (plan `lp2025/2026-09-28-1445-ble-on-lp-link`) moved the C6's
Bluetooth links onto `lp-link`, on `LinkConfig::ble()`'s Datagram framing
(one frame per GATT write or notification). It was built beside M5 and
merged after it, so it took `WIRE_PROTO_VERSION` 32 → **33**; 31, held for
it above, was never carried by a `main` build. After it, `fw-emu` is the
only `M!` board link. With both in, the `M!` line decoder and loss counters
`fw-esp32-common` kept for BLE and the classic (`transport.rs`,
`serial/link_counters.rs`) had no caller and were deleted, and the classic's
UART transport reads requests through the shared payload decoder the USB and
radio links use. The decisions, the MTU arithmetic and the measured RAM are
`docs/adr/2026-09-24-ble-transport.md`'s 2026-09-29 (D8) Amendment.

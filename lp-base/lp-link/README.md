# lp-link

A small sans-IO link layer for LightPlayer's device transports: USB serial,
BLE, the classic UART, and later UDP and WebSocket. It sits between a raw pipe
and the wire messages, on **both** ends: the board, Studio (wasm), `lp-cli`,
and the emulator tools run the same crate.

> **Status: prototype.** It was built and measured in the investigation
> `lp2025/2026-09-26-1720-reliable-device-link` and proven on a C6 in the
> `test_comms_lab` firmware. It is **not yet wired into the product**. The
> decision is `docs/adr/2026-09-27-lp-link-one-comms-layer.md` (proposed), and
> the rollout is plan `lp2025/2026-09-26-2215-lp-link-comms-layer`.

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

## Where things are

| file | concept |
|---|---|
| `link.rs`, `link_config.rs`, `link_event.rs`, `link_counters.rs` | the `Link<A: Arq>` state machine, presets, events, counters |
| `frame.rs`, `crc.rs`, `cobs.rs`, `deframer.rs` | header and SYN, CRC-32C (keyed with the session), COBS and COBS-FF, stream deframing plus text passthrough |
| `arq/` | `SelectiveRepeat` (chosen), `GoBackN` (`StopAndWait` = window 1), `NoArq` |
| `rtt_estimator.rs`, `seq_num.rs`, `tx_queue.rs`, `inbox.rs` | RFC 6298 timer with Karn's rule, sequence arithmetic, send and reassembly queues |
| `log_ring.rs` | the board-side log ring and `link_log!` |
| `lab/` | the comms-lab soak protocol (`LabBoard`/`LabHost`) |
| `sim/` (feature `sim`) | the deterministic fault-injecting simulator: USB/BLE/UDP/WS pipe models; drop, corrupt, duplicate, reorder, truncate, stall, reboot |

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
- Cost in the C6 product image: +14.2 KB flash, ~6 KB RAM per link plus a
  4 KB log ring, ~1 s CPU per MB moved.

## Running it

```bash
cargo test -p lp-link --features sim   # unit tests + proptests (500 cases per variant)
just link-soak 5000                    # long proptest run
just link-bench                        # the comparison tables
just link-size                         # riscv32 code size probe
just link-lab-emu                      # the comms lab on the emulated C6
```

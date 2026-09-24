# ADR: BLE Transport — the Wire over Nordic UART, One Untrusted Link per Connection

- **Status:** Accepted (firmware half, BLE M4). M5 amends it for Studio.
- **Date:** 2026-09-24
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The BLE remote-control plan (`ble-remote-control`, planning workspace) puts
the PLAYFUL choker — an ESP32-C6 with no cable once worn — under the control
of a phone. The spike (`test_ble`, merged in #790) proved the stack on this
chip: esp-radio 0.18 `ble` → bt-hci 0.8 → trouble-host 0.6, ATT MTU 251, 2M
PHY, a peripheral request for 15 ms / 4 s accepted by macOS and by iOS
Bluefy. M2's desk sitting then measured BLE beside Wi-Fi/ESP-NOW: +22.8 KB of
heap at one connection, and ESP-NOW receive loss of 7–30 % under BLE
*traffic* against ~0 % alone.

M3 (`2026-09-23-ble-access-model.md`) had already made the server
link-aware: every message arrives tagged with a `LinkId` and a `LinkTrust`,
every reply goes back on its link, and an access gate grants an untrusted
link nothing but `Hello` and `Login*` until it logs in. What was left was the
firmware transport that carries the second kind of link.

## Decision

1. **NUS carries the wire unchanged.** One Nordic-UART-shaped GATT service
   (`6E400001`; RX `6E400002` write / write-without-response; TX `6E400003`
   notify) — the spike's UUIDs, so generic BLE terminals and `spikes/ble-lab`
   speak to the product as they did to the spike. The bytes are the same
   `M!{json}\n` lines USB carries. The host writes a line in chunks of at most
   one ATT value; the board re-joins them on newlines (`LineJoiner`, capped at
   the 16 KiB frame budget plus margin, an over-long line dropped whole) and
   notifies each server frame back in `MTU − 3` chunks (`chunk_spans`). No new
   message, no BLE-specific framing, no wire-version bump for the transport.

2. **Trust belongs to the link.** USB is `LinkId::PRIMARY`, trusted (physical
   possession is the recovery path). Each BLE connection is a fresh `LinkId` —
   minted monotonically, never reused — and always `Untrusted`. The firmware
   decides the trust; no message can change it. The gate is M3's.

3. **One link mux, one frame in flight.** `fw_esp32_common::radio_link::
   LinkMuxTransport` wraps the USB transport and serves up to two radio links
   through a `RadioLinkPort` (embassy-sync channels: events, incoming lines,
   one write slot per link). A radio frame is serialized into the **same
   static frame buffer** USB uses, and the send waits for the radio side to
   finish with it, exactly as a USB write does — there is no second 16 KiB
   buffer. The radio side reads the buffer only through
   `RadioLinkPort::copy_frame`, which copies synchronously and only while the
   mux holds a lease on that frame's generation.
   - **A slow link cannot stall the device for long.** The wait is bounded
     (`RADIO_WRITE_DEADLINE_MS` = 5 s, set above the slowest measured central:
     a 16 KiB frame at the Mac's 5 KB/s is ~3.3 s). Past it the lease is
     revoked (so the buffer is safe to reuse at once), the link is closed with
     a logged reason, and the server loop moves on.
   - **A radio link's failure is not the server's.** The mux logs a failed
     radio send at error level and closes the link, and returns `Ok` to the
     server: `tick_and_send` stops answering a whole batch on the first
     transport error, and one dying phone must not cost the USB cable its
     replies. The session drops on the next tick (`take_closed_links`).
   - **Hello and heartbeat go to each link.** A link that opens after boot is
     owed its own hello (`LinkUpkeep::take_opened_links`, sent by the server
     loop); heartbeats are per link, each built for that link's tier.

4. **BLE is off until enabled.** The controller initializes only when the
   device store (`/.lp/access.json`) says `bleEnabled: true`, read once at
   boot. A board without the flag never touches the BT peripheral, and no
   emulated board has it, so no emulator gate meets BLE init — radio is not
   modelled. The `ble` Cargo feature is in the default image; the bytes are in
   every image, the behaviour is not.

5. **A link opens when the central subscribes.** Until the central enables
   notifications on TX, trouble-host silently skips a notification, so no
   frame may be sent: the connection's link is announced to the mux only when
   the CCCD says so, and a central that turns notifications off again is
   disconnected.

6. **Ask for 15 ms / 4 s.** One second after connect the board requests an
   interval of 15 ms (min = max), latency 0, supervision timeout 4 s, accepts
   a central's own request as-is, and three seconds later reads back and logs
   what was granted (the spike never saw an "updated" event on its own).

7. **The notify path queues by construction.** trouble-host 0.6's
   `notify(..).await` returns once the value is queued on the host's outbound
   channel (`L2CAP_TX_QUEUE_SIZE`, 8), not when it is sent; the TX runner
   drains it as the controller grants buffers. So notifications issued back to
   back already keep several in flight per connection event, and the central
   sets the pace (spike: Mac 5–12 KB/s, Bluefy 22–48 KB/s). When the queue is
   full the send waits — backpressure up to the mux's deadline, never a silent
   drop.

8. **At most two connections; ten seconds to log in.** The board advertises
   only while a connection slot is free, so a third central finds nothing to
   connect to (DD12: the second connection is allowed, nothing gates on it).
   A link that holds no tier `LOGIN_DEADLINE_MS` (10 s) after opening is
   closed; the timer lives in the mux, driven from the server loop, which is
   the edge that owns the clock and can ask the server (`link_tier`). A
   connection that never subscribes is dropped by the connection task after
   the same 10 s.

9. **No flashing over BLE.** There is no such request on the wire, and BLE
   adds none. Firmware stays on USB.

10. **The RF switch first, as a type.** On the XIAO C6 the radio's RF switch
    is dead until two pins are driven (M1). `apply_board_quirks` returns a
    `BoardQuirksApplied` token and BLE bring-up takes it by value, so the
    ordering is a compile error to get wrong.

## Consequences

- **Flash:** +359,616 B for the `ble` feature (+8,544 B of it is
  `esp-radio/coex`), headroom 324,320 B of the 3 MB partition, lpfs untouched
  (`2026-07-28-esp32c6-flash-budget.md` ledger).
- **RAM — the cost that bit.** Linking BLE moves 36,000 B of *static* RAM:
  ~21.6 KB of the controller blob's link-layer code, which esp-hal's linker
  script places in IRAM (it must run with the flash cache off), plus the
  blob's statics, trouble-host's packet pool and the task pools. On the C6
  static RAM comes out of the main task's stack, which fell from 71,152 B to
  35,152 B; the meteor example (35.8 KB high-water) overflows it on the
  emulator. Moving the host state to the heap recovered 3,528 B (38,680 B).
  **How the rest is paid — a smaller heap for every board, or a separate BLE
  image — is an open ruling on PR #810**, and this ADR is amended when it is
  made.
- **A BLE-disabled board is not byte-identical at runtime**: +172 B of heap
  at idle and +1.1 KB of main-stack high-water (the mux's send future, and
  coex init), measured on the emulator's heap ratchet.
- **Coex:** Wi-Fi/ESP-NOW and BLE share one radio through esp-radio's `coex`.
  Every image now links it and initializes the arbiter at Wi-Fi init, BLE on
  or off. The C6 emulator maps the arbiter's register block as an
  accept-and-remember stub (`COEX`, `0x600A_F400`).
- **Traffic classes (Yona, 2026-09-24).** ESP-NOW must hold near its
  alone-figure in **steady state** — BLE enabled and a phone connected but
  idle; that is the M4 desk check's stop condition. **Discrete panel work**
  (knobs, brightness, a shader switch) may cost ESP-NOW minor interruptions;
  it is measured and reported, not gated. **Continuous interactive input** —
  drawing on an XY pad, BLE MIDI — is a different, real-time class with its
  own latency and air-time budget; it is out of this slice, and a design that
  puts it on this transport must first measure ESP-NOW loss under that load
  (M2 Run G saw 7–30 % under paced frames and bursts).
- **Big frames stall the loop.** A radio send holds the server loop until the
  frame is queued, so a 16 KiB project-read frame to a Mac central costs the
  render loop up to ~3 s. Authoring over BLE is expected to be slow; Play over
  BLE must stay lean (M5/M6).

## Alternatives Considered

- **A second frame buffer per radio link.** Rejected: 16 KiB of `.bss` per
  link, on a chip whose static RAM is its stack (see Consequences), for a
  concurrency the server loop cannot use — it sends one frame at a time.
- **Returning radio send errors to the server.** Rejected: the server stops
  the batch on the first transport error, so a phone walking out of range
  would drop the USB cable's replies too.
- **A BLE-specific framing (length-prefixed packets).** Rejected: the wire's
  newline framing already survives arbitrary chunking, and one framing keeps
  Studio's link code one code path.
- **Enabling BLE by a build feature instead of the device store.** Rejected
  at G0 (PQ2): a board the user owns decides at runtime; a separate image
  per preference multiplies flash paths.
- **Opening the link at connect.** Rejected: frames sent before the central
  subscribes vanish without an error (trouble-host skips them), including the
  link's hello.

## Follow-ups

- The heap/stack ruling (PR #810) and this ADR's amendment.
- M5: the Studio side (Web Bluetooth transport, chunked writes, "cannot
  flash"), which amends this ADR.
- Continuous-input class (XY pad, BLE MIDI): its own measurement before it
  rides this transport.
- `Identify` (PQ7's replacement) was not built in M4; see the plan's notes.

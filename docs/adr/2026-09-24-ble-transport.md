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
   every image, the behaviour is not. Because the flag is read only at boot,
   **enabling or disabling BLE takes a reboot** (plan DD23).

5. **A link opens when the central subscribes.** Until the central enables
   notifications on TX, trouble-host silently skips a notification, so no
   frame may be sent: the connection's link is announced to the mux only when
   the CCCD says so, and a central that turns notifications off again is
   disconnected.

6. **Ask for 15 ms / 4 s.** One second after connect the board requests an
   interval of 15 ms (min = max), latency 0, supervision timeout 4 s, accepts
   a central's own request as-is, and three seconds later reads back and logs
   what was granted (the spike never saw an "updated" event on its own).

6a. **Advertise every 546.25 ms, not every 160 ms.** An advertising event is
   air time the ESP-NOW receiver loses, and a board with BLE enabled
   advertises whenever a slot is free — which is its steady state. Desk,
   2026-09-24 (one 3-minute window each, two XIAOs ~10 cm apart, no central):
   ESP-NOW RX loss 0.21 % with BLE off, 2.61 % advertising at trouble-host's
   160 ms default, 0.98 % at 546.25 ms, 0.68 % at 1022.5 ms. 546.25 ms is on
   Apple's recommended list; what it costs in discovery and reconnect time is
   not yet measured.

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
  **Ruled (Yona, 2026-09-24): a smaller heap for every board, one image** —
  "a stack overflow crashes; a smaller heap only narrows the compile
  margin". The main heap region went 260,000 → 236,000 B (heap total
  325,536 → 301,536 B), and the stack is 62,664 B, against a meteor
  high-water of 33,896 B on `lp-emu:esp32c6:t1` (28,768 B headroom). The
  rejected alternative was a separate BLE image, which reverses `ble` in
  `default`. See `2026-09-02-esp32c6-ram-split.md`, "Amendment".
- **A BLE-disabled board is not byte-identical at runtime**: +172 B of heap
  at idle and +1.1 KB of main-stack high-water (the mux's send future, and
  coex init), measured on the emulator's heap ratchet.
- **Coex:** Wi-Fi/ESP-NOW and BLE share one radio through esp-radio's `coex`.
  Every image now links it and initializes the arbiter at Wi-Fi init, BLE on
  or off. The C6 emulator maps the arbiter's register block as an
  accept-and-remember stub (`COEX`, `0x600A_F400`).
- **Traffic classes (Yona, 2026-09-24).** ESP-NOW must hold near its
  alone-figure in **steady state**. As first written, steady state included a
  phone connected but idle, and that was the M4 desk check's stop condition;
  the desk runs met it, and Yona re-ruled: **steady state is BLE enabled with
  no phone connected**, and a connected phone is an operating state (see
  Amendment). **Discrete panel work**
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

- ~~The heap/stack ruling (PR #810)~~ — ruled and recorded (Consequences,
  and `2026-09-02-esp32c6-ram-split.md`, Amendment, including the heap
  placement fix).
- M5: the Studio side (Web Bluetooth transport, chunked writes, "cannot
  flash"), which amends this ADR.
- Continuous-input class (XY pad, BLE MIDI): its own measurement before it
  rides this transport.
- `Identify` (PQ7's replacement) was not built in M4; see the plan's notes.
- Whether peripheral latency 4 is steady-state safe (Amendment): a longer run,
  or a longer supervision timeout, before it replaces latency 0.
- The knob round trip on the desk-meter image (Runs K/L, 102–399 ms median at
  15 ms) was slower than on the shipped image (Run J, 93 ms). Unexplained.
- Board-to-Studio idle traffic over BLE is mostly the firmware's heartbeat
  and console lines. Quieter logging on radio links is unbuilt.
- The two-connection heap figure has never been measured on the product
  image (DD12: the second connection is allowed, nothing gates on it).

## Amendment (2026-09-24): steady state re-ruled after the desk runs

**The stop condition was met.** With a central connected, logged in and idle
at the granted 15 ms / latency 0 / 4 s, ESP-NOW receive loss on the BLE board
was about 8–9 %, against ~0.5 % with BLE off and ~1.2 % with BLE enabled and
only advertising. No connection parameter tried brought connected-idle loss
near the BLE-off figure while keeping the link.

Setup, for every number here: two XIAO ESP32-C6 boards on one USB hub ~10 cm
apart on Yona's desk (one BLE board, one ESP-NOW peer), ESP-NOW on channel 11
at 50 Hz each way, the PLAYFUL Choker loaded, and the MacBook's own Chrome 153
as the central, driven over CDP. **Not a phone, not Bluefy.** The loss windows
ran the `desk_espnow_meter` image (never shipped); Runs K and L added
`desk_ble_params`. Raw data and the full tables are in the plan's
`spike-results.md`, Runs J, K and L.

| config (granted) | ESP-NOW in-loss | link drops (`0x08`) |
|---|---|---|
| BLE off | 0.42–0.70 % (Run L pooled 0.57 %) | — |
| BLE on, advertising, no connection | 1.23 % (Run J) | — |
| connected idle, 15 ms / latency 0 / 4 s (shipped) | 8.45 % (J); 8.32 % (K); 8.83 % pooled over 3 windows (L) | 0 in ~15 min over 5 windows (K+L); 2 in Run J |
| connected idle, 15 ms / latency 4 / 4 s | 3.73 % (K); 4.53 % pooled (L) | 3 in ~15 min over 5 windows |
| connected idle, 30–100 ms intervals | 1.7–1.9 % at best (K) | a drop every few minutes |

**Ruled (Yona, 2026-09-24 evening; plan E8):**

- **Steady state is BLE enabled with no phone connected** (~1.2 % against
  ~0.5 % with BLE off). That is the class-1 figure ESP-NOW must hold.
- **A connected phone is an operating state**, in class 2 with discrete panel
  work. Its ~9 % ESP-NOW loss is accepted, measured and reported, not gated.
- **The parameters stay 15 ms / latency 0 / 4 s.** Latency 4 halves the loss,
  but its supervision-timeout drops cluster; link stability is worth more than
  loss once connected is not steady state (director DD28). Latency 4 stays an
  experiment knob: the `desk_ble_params` feature (never shipped) reads
  interval, latency, timeout and an optional active/idle switch from
  `/.lp/ble-exp.txt` at boot, so a desk run changes them with a file write and
  a reboot.

**Flash at the merged head.** The ledger row's 324,320 B headroom was measured
on M4's own branch. After it merged `origin/main` (whose own changes saved
flash), the head `9bfac876d` measured an image of 2,810,960 B and a headroom of
**334,768 B** (`just fw-esp32c6-size-check`, PR #810). The `ble` delta itself
is unchanged.

**The heap cut's placement cost.** The 24,000 B heap cut left a BLE-enabled
board unable to switch from the choker to Zook dome (largest free block
65,534 B < the 64 KiB load gate, with 212 KB free). This was fixed by
placement, not size: the radio blobs' C heap fills the reclaimed `dram2_seg`
region first. See `2026-09-02-esp32c6-ram-split.md` ("Placement") and
`docs/defects/2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut.md`.

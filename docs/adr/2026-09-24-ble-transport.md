# ADR: BLE Transport — the Wire over Nordic UART, One Untrusted Link per Connection

- **Status:** Accepted. The firmware half is BLE M4 (decisions 1–10); the
  Studio half is BLE M5/M6 (decisions S1–S7, folded in 2026-09-25 from
  `2026-09-24-ble-transport-studio.md`, which is now a pointer here).
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
   *Amended 2026-09-25 (#834, below): a long write (Prepare/Execute) is
   reassembled too, and the ATT MTU is 247, not 251.*

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
   *Superseded in part 2026-09-25 (Amendment below): a board with no device
   store now starts BLE on, locked; the flag is still read only at boot.*

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
   *Amended 2026-09-25 (#831, below): a link whose own login challenge is
   outstanding waits for it, never past the challenge's 30 s TTL.*

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
- ~~M5: the Studio side (Web Bluetooth transport, chunked writes, "cannot
  flash")~~ — shipped in #811, decisions S1–S7 below.
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

## Studio side (BLE M5/M6): a control-only `ble:` link, silent reconnect, a lean Play

*Folded in 2026-09-25 from `2026-09-24-ble-transport-studio.md`, which was
written as its own file while this ADR was not yet on `main`. Its decisions
1–7 are S1–S7 here; its text is unchanged except for the numbering.*

### Studio context

M4 carries the wire over a Nordic UART GATT service: the same `M!{json}\n`
lines, RX `6E400002` written by the central in ≤ MTU − 3 chunks, TX
`6E400003` notified back, the link opening when the central subscribes to TX,
and every BLE connection its own untrusted link that must log in within 10 s.
Studio reaches it from the HTTPS page through Web Bluetooth — on iPhone that is
Bluefy (plan D21) — and Studio is a phone tool too (D18).

G1 (M2 desk sitting 1, Run F) measured what Studio can rely on in Bluefy
3.9.3 / iOS 18.7: a held `BluetoothDevice` reconnects with **no gesture**
after a page-caused drop (954 ms) and a board reboot (803 ms);
`navigator.bluetooth.getDevices()` returns the granted board; the device id is
a UUID. And one catch: iOS suspends a hidden page, so a drop while the phone is
locked is delivered only when the page is shown again
(`docs/defects/2026-09-23-bluefy-hidden-page-does-not-see-ble-drops.md`).
G1 Run G measured the cost of a held connection: ESP-NOW receive loss 1–5 %
connected-idle, 7–30 % under traffic.

### Studio decisions

S1. **The endpoint is `ble:<Web Bluetooth device id>`.** The device id is
   opaque and per origin (a UUID in Bluefy, base64 in Chrome) and is never
   identity: the hello's base MAC is. So a board seen over USB and over
   Bluetooth — both at once, which the firmware allows — is one device and one
   registry row (`mac:` key); the row's transport column follows the live link
   ("USB" / "Bluetooth"). The prefix lives in `lpa-devices`
   (`BLE_ENDPOINT_PREFIX`), because the model needs exactly two facts about a
   Bluetooth link and both are read off the endpoint: a re-grant goes through
   the Bluetooth chooser (`Action::AddFromBle`, `Command::RequestBleGrant`;
   `Reconnect` routes by the last endpoint), and firmware cannot be written
   over it.

S2. **The capability fact is `DeviceView::firmware_blocked`.** On a board
   reached over Bluetooth it reads "Firmware updates need USB"; the card draws
   Flash / Update / Factory reset (and the hardware Reset: "Reset needs USB")
   **disabled, with the reason**, never hidden. The transport refuses the same
   effects by name, and `LinkProviderKind::BrowserBle`'s capabilities carry no
   `Reset`, `FlashFirmware`, `EraseDeviceFlash`, `WriteBootControl` or raw
   filesystem operation. Push, project removal and the board-manifest write
   are the ordinary `lpa-client` conversations over the link.

S3. **Reconnect is silent; visibility is "state unknown".** `browser_ble.js`
   holds the `BluetoothDevice`; a drop reports `bluetooth link lost: …` (the
   effects layer reads it as a departure) and starts a bounded reconnect loop
   on the held device (250 ms, then backing off to 30 s, then parking); a
   page load re-connects `getDevices()` boards once. Presence is announced by
   `connect`/`disconnect` edges, the Web Serial hotplug shape, and the effects
   layer re-derives from them. On `visibilitychange → visible` every session
   re-reads `gatt.connected`: a drop the hidden page never heard is handled
   then, and a parked session restarts its reconnect. Every `gatt.connect()` is
   bounded (10 s); every write asks for a response and is awaited, in
   ≤ 244-byte chunks. The one-tap route back is the offline card's Reconnect,
   which opens the Bluetooth chooser.
   *Amended 2026-09-25 (#834, below): writes are 180-byte chunks, and every
   drop calls `gatt.disconnect()`.*

S4. **Play over Bluetooth is lean when idle.** Play is the steady state (a
   phone on a piece's panel) and its air time is shared with ESP-NOW, so while
   a Play surface holds the lens (`PlayViewOp`, a mount lease), an untouched
   lens over `ble:` reads once when it opens, the three verdict-chase reads
   after each accepted knob/fader write, and otherwise once a minute
   (`BLE_PLAY_IDLE_REFRESH_INTERVAL`). The editor over Bluetooth is authoring
   and keeps the device cadence; the device card's live picture feed does not
   run over Bluetooth at all. Measured over `?ble=emu`: see Consequences.
   *Amended 2026-10-08 (below): the card's picture runs over Bluetooth at a
   gentle pace; Play keeps its minute.*

S5. **`?ble=emu` is a polyfill, and it proves the transport — not access.**
   Beside `?emu=`, `navigator.bluetooth` becomes
   `public/lpa-link/virtual_bluetooth.js`: exactly the GATT subset
   `browser_ble.js` calls, over the same `EmulatorPort`s the serial bus holds
   (the banner's cable is the radio; the link opens on subscribe; a hello that
   asks for a login and gets none is dropped after 10 s). The emulated board's
   link is its trusted USB link, so every request is answered at the edit tier:
   it proves the transport, the UI and Play mode, and enforcement stays proven
   by M3's host tests and M4's desk check. The page banner says so.

S6. **Login on connect is Studio's; the board enforces (M6).** When a `ble:`
   link says hello, Studio asks that link's own hello what it holds
   (`auth.required`, `auth.granted`) over the shared wire, and when it holds
   nothing, logs in: the account default password, then the passwords this
   browser remembers (most recently successful first), **at most two
   answers per connect** — each wrong one feeds the board's backoff — and
   then the password sheet. Automatic tries are spent once per device, not
   per connect, so a silent reconnect never burns the backoff the typed
   password is about to need. An open device is connected at play and
   prompted only when an edit is refused (`NotPermitted { needs: Edit }`,
   which `lpa-client` now returns as its own error, worded as the sentence
   the UI shows). The editor lens waits for the link to hold a tier. K is
   derived in Studio's wasm (`lpc-access`'s PBKDF2), cached per session,
   with a yield between derivations; new secrets cost 60 000 iterations.
S7. **The device store is written, never read (M6).** Studio keeps, per device
   (registry key), the whole `/.lp/access.json` it last wrote, and every
   change rewrites the whole file from that record over the link (USB, or a
   Bluetooth login at edit). The panel lists what this browser wrote and says
   the piece may hold others; saving replaces them. `bleEnabled` is read once
   at boot (M4), so a switch shows "turns on when the piece restarts" until a
   newer hello is seen, with Restart now over USB. Remembered passwords
   (`lp.ble.passwords.v1`), these records (`lp.ble.device-access.v1`) and the
   account default (`lp.settings.v1`) are local to the browser (PQ8).

### Studio consequences

- One `?ble=emu` walk (`just walk-ble-emu`, 2026-09-24): an idle Play lens
  put **5.4 B/s** Studio→board on the link (one `projectRead` in 75 s) and
  **107.5 B/s** board→Studio — most of it the board's own heartbeat and
  console lines. The editor over the same link: ~1.0 KB/s up, ~7 KB/s down.
  Emulated, and the emulated board's clock is not silicon's.
- A Bluetooth board after a page reload is reconnected only if the browser
  still grants it (`getDevices()`). A remembered board has no live endpoint
  (endpoints are not persisted); since M6 its record carries
  `last_over_bluetooth` from the registry row's transport column, so its
  Reconnect opens the Bluetooth chooser, not the USB one.
- M6 on the same walk: idle Play over `ble:` put **5.5 B/s** Studio→board
  (411 B in 75 s; one `projectRead`) and **110.5 B/s** board→Studio —
  login-on-connect adds one `hello` per connect and nothing while idle.
- KDF cost (M6, desk M2 Max, `derive_login_key` in wasm with Studio's
  release profile, best of five): 60 000 iterations 46 ms in V8, 44 ms in
  JavaScriptCore; 100 000 took 77 / 73 ms. Bluefy on the phone is
  re-measured at the M7 walk.
- The Web Bluetooth conformance suite (`browser_ble_conformance.rs`) runs in
  `just lpa-link-browser-test` beside the serial one.

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

## Amendment 2026-09-25: S6 and S7 replaced by easy access

`2026-09-24-easy-bluetooth-access.md` replaces two Studio decisions above.
Studio no longer logs in with an account default password (S6): it
unlocks with keys it holds, matched by salt (this browser's, the account's,
the account's optional passwords), then one remembered password, then the
sheet. And the device store is no longer written whole from a local record
(S7): the board merges changes and lists every key (`AccessList`,
`AccessAdd`, `AccessRemove`, `AccessSetSwitches`), and Studio caches only
the last list it read (`lp.access.device-lists.v1`, no keys). The login
protocol, the tiers and the enforcement on the board are unchanged.

## Amendment 2026-09-25: Bluetooth on by default (decision 4)

Yona reversed "off until enabled over USB" (plan DD30, in
`lp2025/2026-09-24-1953-ble-easy-access`, shipped in #821): "worst time to
find out you forgot to turn bluetooth on is … at night at burning man with
just your phone". A board with **no** device store now starts BLE, **locked**
and with no keys (`DeviceAccessFile::fresh()`); a **damaged** store still
keeps BLE off. `bleEnabled` is still read once at boot, so a change still
takes a restart — Studio now does the restart itself after a toggle over USB
(over Bluetooth the toggle is locked, "turn off by USB"). This also
retires decision 4's "no emulator gate meets BLE init": every emulated
board with an empty filesystem now starts the BLE controller beside Wi-Fi.
It came up and advertised under `--strict-bus` with no new override, and
every emulator gate stayed green (easy-access P1; `lp-emu-esp32c6`
README, `COEX` row). The air is still not modelled: nothing connects.
The whole decision, and the generated per-browser and per-account keys that
make "locked" cheap to get past, is `2026-09-24-easy-bluetooth-access.md`;
the board's half is the access-model ADR's 2026-09-24 amendment.

## Amendment 2026-09-25: the knob jump (#831) — an esp-radio fork, and a host restart that recovers

Found on the M7 laptop desk walk (Run M): a Studio knob jump over Bluetooth
killed the C6's BLE host, and the board never advertised again until a USB
reboot. Two faults; both fixed in #831.
Defect: `docs/defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md`.

- **esp-radio 0.18 is vendored and patched** (`third_party/esp-radio`, in
  through the root `[patch.crates-io]`, like `third_party/esp-alloc`;
  `MIT OR Apache-2.0`, with `licenses/Apache-2.0.txt` and the provenance in
  `third_party/esp-radio/README-LP.md`). Upstream's `ble_hs_rx_data` copied
  only the first `os_mbuf` of a received ACL packet, and the C6 controller
  chains exactly the ACL packets of 193–198 B — ATT values of 182–187 B. Any
  single write of that length reached the host short, bt-hci refused it, and
  trouble-host's runner failed. The fork copies every segment, and its parse
  warning names the packet's length and header (the `{:?}` alone prints
  nothing under `-Zfmt-debug=none`). This is AGENTS.md's "fix the
  dependency" path (plan DD32), not a workaround. Both hunks are upstream
  candidates; drop the fork when a release copies chained mbufs.
- **A host-runner restart closes every link and advertises again.**
  trouble-host 0.6 restarts a failed runner by bringing the host up again,
  starting with an HCI `Reset`, and the controller drops every connection on a
  reset without a `Disconnection Complete`. The host's transport now keeps a
  ledger of the connections the controller reported
  (`fw-esp32-common::radio_link::hci_connection_ledger`), and on a `Reset`
  hands the host a `Disconnection Complete` (0x16) for each one still open;
  `ble_task`'s advertiser drops what it was doing and advertises again.
  Silicon: a forced restart came back 5 of 5 on the desk.
- **The 10 s login deadline (decision 8) waits for an outstanding challenge.**
  A person typing a device password on a phone could lose the link mid-sheet
  (Run M: three drops while the sheet was open). While the link's own
  `LoginBegin` challenge is outstanding the deadline waits for it, never past
  the challenge's expiry (`lpc_access::CHALLENGE_TTL_MS`, 30 s); once it
  expires or is refused, the ordinary deadline applies, and a link already past
  it closes. Studio's half (the unlock sheet holds a challenge) shipped in #824.
- **Cost:** +4,160 B of flash.
- **Lesson carried into the desk check:** `spikes/ble-lab/scripts/m4-desk-check.py
  --only-knob-burst` writes every length 178–244 B and forces a restart on a
  `desk_ble_fault` image; `?ble=emu` could not have found either fault, since
  the emulated path goes through neither the controller nor trouble-host.

## Amendment 2026-09-25: editing from the phone (#834) — long writes, MTU 247, teardown on every drop, 180-byte chunks

Found at G4 (Yona, iPhone in Bluefy, the PLAYFUL choker): knobs worked, but
**editing any setting over Bluetooth sent Studio back to `/devices` with no
error**, and a reconnect said the board "never said hello". The board's
console saw no error and no drop. Three mechanisms, all fixed in #834
(merged `97efd6624`). Defect:
`docs/defects/2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md`.

- **Long writes reach the server (decision 1).** A central may send a value
  longer than MTU − 3 as Prepare Write requests plus one Execute Write.
  trouble-host 0.6 answers both itself and never hands either to the
  connection as a write. So a long write whose segments fit RX was
  acknowledged and **dropped**, and one whose segments did not was refused
  with no log line. `fw-esp32-common::radio_link::prepared_write` (host-tested)
  now queues a connection's segments, in order and at most 512 B, and gives
  the whole value to the line joiner on Execute. A bad offset or an oversized
  value is refused with the matching ATT error, and the refusal is logged.
- **ATT MTU 247, not 251.** The host's ATT MTU is its packet size − 4, and a
  255 B packet pool made it 251. The C6 controller's largest ACL packet is
  251 B, so a full-MTU reply (251 + 4 L2CAP) failed trouble-host's send, and
  that failure **restarts the whole BLE host**. The pool is now 251 B
  (`default-packet-pool-mtu-251`). Notifications stay 244 B, so nothing else
  changes size. This corrects the Context's "ATT MTU 251", which was the
  spike's figure.
- **Phantom drops on iOS, and the teardown rule (S3, new rule 5 in
  `browser_ble.js`).** Bluefy told the page its link was gone while iOS kept
  the radio connection up: the board logged the disconnect only when the tab
  closed, ~20 minutes later. Studio handled a drop by reconnecting without
  ever calling `gatt.disconnect()`, so the reconnect rode the old link, the
  board saw no new link, and it never sent the hello it owes one. **Every
  drop, and every failed write, now calls `gatt.disconnect()`**, so a
  reconnect is always a fresh link. A failed write while the browser still
  calls the link up is a drop too: the rest of that line is lost, and a half
  line in the board's joiner would poison the next one.
- **Writes are 180-byte chunks (S3), was 244.** 180 B fits one ATT value at
  iOS's common MTU of 185 as well as the board's 247, so no write depends on
  the long-write path. Cost on the Mac, 2 KB with response at 15 ms: median
  about 3.9 → 3.2 KB/s (runs vary 2.5–6.4).
- **`?ble=emu` (S5)** models the phantom drop (`phantomDrop`), and the
  conformance test `a_phantom_drop_is_torn_down_and_the_reconnect_is_a_fresh_link`
  fails with the teardown removed.
- **Evidence.** Phone (Yona, 2026-09-25): the choker with #834's firmware
  half and the deployed Studio of the time (244 B writes, no teardown): "now
  everything seems to be working", editing held. So the firmware half alone
  cleared it on this phone; the run does not say which of the two firmware
  changes it needed. The Studio half (180 B, teardown) ran on the Mac over
  CDP and in the conformance suite, not yet on the phone. Silicon, Mac Chrome
  over CDP: ten panel writes in a row, each one 300–480 B long write, all
  accepted; a page-side disconnect, a fresh `link2` in 1,077 ms, hello in
  122 ms. Cost: +208 B of flash.
- **Not fixed here:** the board still drops an unparseable request without
  replying, which is how a proto-26 image left Studio waiting on
  `accessList` (plan DD35, deferred to the wire version-skew work).

## Amendment 2026-09-29 (D8): BLE moves onto lp-link

`lp2025/2026-09-28-1445-ble-on-lp-link` (director plan M3 of the lp-link
comms-layer effort) replaces the `M!{json}\n`-line radio transport this ADR
originally specified with lp-link's Datagram framing — the same crate USB
cut over to in `lp2025/2026-09-27-0215-lp-link-usb-cutover`
(`docs/adr/2026-09-27-lp-link-one-comms-layer.md`). This amends decisions 1,
3 and 7 above; decisions 2, 4, 5, 6, 6a, 8, 9 and 10 are unaffected — trust,
enable-at-boot, subscribe-to-open, connection parameters, the slot count and
the no-flashing rule all sit above or beside the framing.

- **Decision 1 (framing) replaced.** NUS still carries the wire (same
  service and characteristic UUIDs, same RX-write/TX-notify shape), but the
  bytes are no longer `M!{json}\n` lines. Each GATT write and each
  notification is exactly one lp-link Datagram frame — a 4-byte header, the
  proto-channel payload (`{`-JSON or an `L` learned-dictionary packed
  message, unchanged from what channel 1 already carried) and a 4-byte
  CRC-32C — capped to `min(180, negotiated_ATT_MTU − 11)` B of payload. GATT
  already delimits one write/notification from the next, so there is no
  byte-stream framing (no COBS, no newline) the way USB's Stream mode needs
  one. `LineJoiner`, `LineChunker` and the ATT long-write (Prepare…Execute)
  reassembly path (`prepared_write.rs`, added by the 2026-09-25 Amendment
  above) are deleted outright — with every frame fixed at or under one ATT
  value, no write can ever need a long write, and the board now actively
  refuses a Prepare Write on RX with `REQUEST_NOT_SUPPORTED`. This closes
  the whole bug class the "editing from the phone" defect above described;
  see the defect's own closure note.
  - **Correction to the 2026-09-25 Amendment's "180 B fits iOS's 185 B
    MTU."** That measured a *write chunk size against a raw ATT value*; a
    180 B lp-link *payload* is a 188 B *frame* (180 + 8 for the header and
    CRC), which does not fit inside iOS/Bluefy's 182-byte usable ATT value
    (185 MTU − 3). The per-connection formula above is what actually makes
    a frame fit any given MTU: 180 B at the board's own ceiling (MTU 247),
    174 B at iOS's common 185, down to 12 B at the Bluetooth minimum of 23
    (below which the link is refused — even a 20-byte SYN would not fit one
    ATT value). Studio no longer needs to guess the MTU at all: lp-link's
    SYN carries the board's `max_payload`, and the host's frames shrink to
    match automatically.
- **Decision 3 (one frame buffer, one in flight) implemented differently,
  ruling still holds.** The ADR's rejection of a second 16 KiB frame buffer
  per link stands. What generalizes is *how* a reply reaches `FRAME_BUF`:
  the mux's existing lease/deadline scheme (`release_frame_buf`,
  `RADIO_WRITE_DEADLINE_MS`) is now reached through lp-link's own
  `send_external` mechanism — a reply serializes once into the buffer the
  firmware already owns, rather than being copied into the link's own send
  ring, mirroring the USB cut-over's own heap follow-up (`lp-link`
  README's "Sending without a copy"). Replies of 1 KiB or less still go
  through the ordinary send ring and hold no buffer at all. A radio link
  that does not release the buffer within its deadline is closed with a
  logged reason, same as before.
- **Decision 7 (queueing backpressures, not drops) unchanged in effect, now
  through lp-link.** trouble-host's own notify-queue backpressure (up to
  the mux's deadline) still composes with lp-link's own send budget and
  selective-repeat window the same way USB's does — these are two layers
  that agree, not two mechanisms in tension: lp-link retransmits what
  trouble-host has not yet drained, and trouble-host's queue absorbs bursts
  lp-link's own window already paces.
- **`WIRE_PROTO_VERSION` 36 → 37** (`lp-core/lpc-wire/src/server/hello.rs`),
  in the same change as the framing switch (built as 30 → 31 beside the
  classic's UART cut-over, PR #884, which merged first and took 32; 33–36
  went to access, the filesystem, the build version and Wi-Fi settings
  while this sat open), per the wire-compatibility
  policy (no shims; every producer/consumer moves together). All four
  firmware `manifest-core.expected.json` goldens moved with it.
- **Packed replies are on by default over BLE now.** The old `M!` radio
  link never asked a board to pack its replies; the new lp-link-based port
  opts in the same way Web Serial's does (`SetEncoding`), unless `?wire=json`.
  This is a behavior change on the air, not just under the hood — the board
  answers it, so no defect follows, but it means Studio's own preference
  now applies to BLE where it previously never did.
- **`?ble=emu` (S5) keeps its original boundary (D9 of the plan), described
  more precisely.** The polyfill still proxies the emulated board's real
  *USB* lp-link **Stream** session — it does not open a genuine BLE
  Datagram session, and building that is explicitly out of scope (see the
  plan's "Out of scope"). What changed is that the polyfill now translates
  the framing instead of piping raw bytes: a page write becomes one
  COBS-FF-wrapped stream chunk into the emulated board's USB byte channel,
  and the board's stream is cut back into one frame per notification on the
  way out (verified byte-identical to `lp_link::frame::wrap_stream` on 200
  vectors). The trust caveat is unchanged: the emulated board sees its
  trusted USB link, so every request is answered at the edit tier, and
  access enforcement still rests on `lpa-server/tests/access_gate.rs` and
  the desk check. One artifact this walk surfaced and did not resolve: the
  emulated board's USB link (`stall_after` 1 s) and Studio's BLE keepalive
  (also 1 s) are two independent timers tuned for two different transports
  now standing in for each other on this one path; an idle hermetic run saw
  a handful of stall edges out of hundreds of polls. This is read as an
  artifact of the emu path's borrowed USB timing, not a BLE framing defect —
  a real board runs BLE's own `ble()` timers (`min_rto` 250 ms, keepalive
  1 s, `stall_after` 3.5 s) on both ends.
- **RAM, measured (host test, not yet silicon).** Per open GATT connection
  at the real firmware config: 7,672 B at rest for one link, 15,344 B for
  two (against the region-1 floor of ~19,500 B this plan's own `notes.md`
  named as already the tightest block in the system, that leaves about
  4.2 KB headroom for two links at rest — worse under a simultaneous large
  upload on each, which peaks at 27,760 B). Full figures and the flash
  delta (image landed 96 B smaller) are in `lp-base/lp-link/README.md`'s
  "Measured" section. These are 64-bit host-test figures; a silicon
  measurement is the soak this plan's own P5 phase still owes (see that
  phase file's "Silicon soak (pending the board)").
- **`RADIO_LINK_SLOTS` stays 2**, per the plan's own R1 ruling: the measured
  two-link-at-rest margin (~4.2 KB) is judged comfortable enough not to
  drop to 1, an improvement over the plan's own pre-firmware preview
  (~1.4 KB) because the single-datagram-slot-per-link design saves about
  1.26 KB per link over the generic `ble()` preset's per-connection cost.
- **The defect this ADR's 2026-09-25 Amendment describes**
  (`docs/defects/2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md`)
  is closed as moot by this change — see the defect file's own closure
  note.
- Plan directory: `lp2025/2026-09-28-1445-ble-on-lp-link` (`plan.md`,
  `notes.md`, phase files `p1`–`p5`).

## Amendment 2026-10-08: the card's picture runs over Bluetooth, gently (S4)

S4 kept the device card's picture feed off Bluetooth entirely, so a board
reached that way never showed a live picture and its card said "No live
picture over Bluetooth — Open in editor to see and control it." Yona,
2026-10-08: "its annoying and seems unnecessary after all the work we did to
get the data rates down". Reversed for the card; Play keeps its minute.

- **The rule.** The card feed runs over Bluetooth at its own completion gap,
  `DEVICE_CARD_FEED_BLE_INTERVAL` = 500 ms, against
  `DEVICE_CARD_FEED_INTERVAL` = 150 ms over USB, the LAN and the relay
  (`card_feed_gap_policy`, `lpa-studio-core/src/app/studio/refresh_cadence.rs`).
  The period is the gap plus the read's own time, so a card sees about one to
  two pictures a second and a bigger frame self-throttles. The rest of the
  feed's rule is unchanged: it pulls only while its card is mounted and the
  page is visible, never under the editor's borrow, and parks after three
  unanswered reads. The card waits for its first picture with the sentence
  every link uses ("No picture yet — the live feed is coming."), and its live
  pill names the pace: "live · 43 fps · shown 1–2/s".
- **What a card read costs, measured.** Off the wire tap of
  `just walk-ble-emu` (`LP_EMU_WIRE_TAP`, `just wire-tap-stat --ledger card`):
  a steady reply is the frame's raw sRGB bytes plus about 87 B — 254 B for
  Peach (1D)'s 56 lamps, 811 B for Logo Sign's 241 — so about 0.5 KB at 128
  lamps and 1.6 KB at 512. The request is 187 B. The first read after a
  connect also carries the geometry once (2,814 B for Logo Sign). These sizes
  are the wire's and do not depend on the link; the walk's read rates do (its
  "Bluetooth" is the emulated board's USB link), so none of its rates is a
  Bluetooth number. For scale on that same emulated link: the card put
  ~0.3 KB/s up and 0.4–1.2 KB/s down, the editor 1.7 KB/s up and 2.8 KB/s
  down.
- **Why M5's reasons no longer hold.**
  - *Size.* M5 sized the card against `M!` JSON lines with base64 pixels.
    Since D8 the link is lp-link and replies are packed: pixels travel raw.
  - *ESP-NOW.* The concern still applies in kind: a card on screen is traffic
    on the air the board shares with ESP-NOW, and a project with a Radio node
    loses some packets while it is watched. But this ADR's 2026-09-24
    Amendment ruled a connected central an operating state whose ESP-NOW loss
    is measured and reported, not gated. Connected and idle already costs
    ~8–9 % against ~0.5 %, and the editor over Bluetooth, which is allowed,
    reads every 75 ms. The card adds well under what the editor does, only
    while a person is looking at the Devices page, and never while the board
    uses Wi‑Fi (ESP-NOW is off then: `fw-esp32-common/src/net/radio_rule.rs`).
    Steady state (Bluetooth on, nothing connected) is untouched.
  - *The board's loop.* A reply of at most `SMALL_REPLY_BYTES` (1 KiB, about
    310 lamps) is copied into the link's send ring and holds nothing; a larger
    one holds the frame buffer only until the link has cut it into frames.
    The board-side ceiling is the radio frame-rate budget's connected row
    (`2026-10-06-radio-frame-rate-budget.md`: ≤ 50 % fps, p99 ≤ 1 s).
- **Play keeps its minute** (`BLE_PLAY_IDLE_REFRESH_INTERVAL`). It is not the
  same case: Play is held for hours on a phone at the piece, its read is the
  lens's whole read (a 407 B request; ~1.2 KB replies for Logo Sign in the
  editor), and its
  surface puts the controls first and the picture in a slim banner. The
  2026-09-24 re-ruling weakens S4's "Play is the steady state", so it is worth
  revisiting, but with the card's desk measurement in hand, not by analogy.
- **Measured on silicon (2026-10-09).** Board `loose-c6` (a XIAO ESP32-C6,
  release 2026.10.08-23), running a copy of Logo Sign (241 lamps, on D10 and
  D9: the catalog's IO13 is a USB data line on the C6), on the desk next to
  the Mac. The central was a background Brave (Chromium 155, macOS 26.5)
  driven over CDP by `spikes/ble-lab/scripts/cdp-central.mjs`; Mac Chrome saw
  no Bluetooth devices at all that night. Fps and frame times are the board's
  own `[perf]` lines over its USB console (`lp-cli link capture`); the bytes
  and reads are counted at the page's GATT characteristics. Windows
  alternated the card off screen (Studio on Projects, still connected) and on
  screen: 2 × 2 minutes each, then 3 × 5 minutes each.

  | | card off screen | card on screen |
  |---|---|---|
  | board fps (median of `[perf]`) | 30 in every window | 29 in every window |
  | p99 frame time | ≤ 50 ms | ≤ 50 ms |
  | slowest frame | 48–50 ms | 51–62 ms |
  | frames over 100 ms | 0 | 0 |
  | card reads | 0 | 0.94–1.24 a second |
  | Bluetooth, page → board / board → page | ~9 / ~45 B/s | ~210–280 / ~800–1,060 B/s |
  | link drops | 0 in ~19 min | 1 in ~19 min |

  The cost is one frame a second of thirty, about 3 % — inside even the
  radio budget's idle row (≤ 10 %), let alone the connected row (≤ 50 %).
  The one drop was a supervision timeout (`0x08`) in a 2-minute window; Studio
  reconnected by itself in about 5 s and the card came back live. The
  15-minute soak that followed (card on and off, 5-minute windows) had none.
  The 2026-09-24 Amendment's desk runs saw the same drop with nothing being
  read (2 in Run J, 0 in ~15 min over K+L), so one drop does not say whether
  the card makes them likelier.
- **On a phone (2026-10-09).** Yona's iPhone in Bluefy, the same board running
  the PLAYFUL Choker (lab rehearsal): the card drew the picture with the
  Bluetooth pill, but only after a reboot. Before it, every read was refused
  for memory: a connected central costs the C6 ~17.5 KB of heap, and the
  choker was left under the 40 KiB read gate. Open defect,
  `docs/defects/2026-10-09-a-phones-bluetooth-link-leaves-the-choker-under-the-read-gate.md`.
  Still owed: a 512-lamp board, and ESP-NOW loss beside a watched card
  (`desk_espnow_meter`).

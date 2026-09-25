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

S4. **Play over Bluetooth is lean when idle.** Play is the steady state (a
   phone on a piece's panel) and its air time is shared with ESP-NOW, so while
   a Play surface holds the lens (`PlayViewOp`, a mount lease), an untouched
   lens over `ble:` reads once when it opens, the three verdict-chase reads
   after each accepted knob/fader write, and otherwise once a minute
   (`BLE_PLAY_IDLE_REFRESH_INTERVAL`). The editor over Bluetooth is authoring
   and keeps the device cadence; the device card's live picture feed does not
   run over Bluetooth at all. Measured over `?ble=emu`: see Consequences.

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

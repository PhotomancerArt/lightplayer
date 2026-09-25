# ADR: BLE Transport, Studio Side — a Control-Only `ble:` Link, Silent Reconnect, a Lean Play

- **Status:** Accepted (Studio half, BLE M5). The Studio amendment to
  `2026-09-24-ble-transport.md` (M4, the firmware half); written as its own
  file because M4's ADR is not on `main` yet, and to be folded into it when
  both have landed.
- **Date:** 2026-09-24
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

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

## Decision

1. **The endpoint is `ble:<Web Bluetooth device id>`.** The device id is
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

2. **The capability fact is `DeviceView::firmware_blocked`.** On a board
   reached over Bluetooth it reads "Firmware updates need USB"; the card draws
   Flash / Update / Factory reset (and the hardware Reset: "Reset needs USB")
   **disabled, with the reason**, never hidden. The transport refuses the same
   effects by name, and `LinkProviderKind::BrowserBle`'s capabilities carry no
   `Reset`, `FlashFirmware`, `EraseDeviceFlash`, `WriteBootControl` or raw
   filesystem operation. Push, project removal and the board-manifest write
   are the ordinary `lpa-client` conversations over the link.

3. **Reconnect is silent; visibility is "state unknown".** `browser_ble.js`
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

4. **Play over Bluetooth is lean when idle.** Play is the steady state (a
   phone on a piece's panel) and its air time is shared with ESP-NOW, so while
   a Play surface holds the lens (`PlayViewOp`, a mount lease), an untouched
   lens over `ble:` reads once when it opens, the three verdict-chase reads
   after each accepted knob/fader write, and otherwise once a minute
   (`BLE_PLAY_IDLE_REFRESH_INTERVAL`). The editor over Bluetooth is authoring
   and keeps the device cadence; the device card's live picture feed does not
   run over Bluetooth at all. Measured over `?ble=emu`: see Consequences.

5. **`?ble=emu` is a polyfill, and it proves the transport — not access.**
   Beside `?emu=`, `navigator.bluetooth` becomes
   `public/lpa-link/virtual_bluetooth.js`: exactly the GATT subset
   `browser_ble.js` calls, over the same `EmulatorPort`s the serial bus holds
   (the banner's cable is the radio; the link opens on subscribe; a hello that
   asks for a login and gets none is dropped after 10 s). The emulated board's
   link is its trusted USB link, so every request is answered at the edit tier:
   it proves the transport, the UI and Play mode, and enforcement stays proven
   by M3's host tests and M4's desk check. The page banner says so.

6. **Login on connect is Studio's; the board enforces (M6).** When a `ble:`
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
7. **The device store is written, never read (M6).** Studio keeps, per device
   (registry key), the whole `/.lp/access.json` it last wrote, and every
   change rewrites the whole file from that record over the link (USB, or a
   Bluetooth login at edit). The panel lists what this browser wrote and says
   the piece may hold others; saving replaces them. `bleEnabled` is read once
   at boot (M4), so a switch shows "turns on when the piece restarts" until a
   newer hello is seen, with Restart now over USB. Remembered passwords
   (`lp.ble.passwords.v1`), these records (`lp.ble.device-access.v1`) and the
   account default (`lp.settings.v1`) are local to the browser (PQ8).

## Consequences

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

## Amendment 2026-09-25: items 6 and 7 replaced

`2026-09-24-easy-bluetooth-access.md` replaces two decisions above.
Studio no longer logs in with an account default password (item 6): it
unlocks with keys it holds, matched by salt (this browser's, the account's,
the account's optional passwords), then one remembered password, then the
sheet. And the device store is no longer written whole from a local record
(item 7): the board merges changes and lists every key (`AccessList`,
`AccessAdd`, `AccessRemove`, `AccessSetSwitches`), and Studio caches only
the last list it read (`lp.access.device-lists.v1`, no keys). The login
protocol, the tiers and the enforcement on the board are unchanged.

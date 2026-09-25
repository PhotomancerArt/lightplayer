---
status: fixed
found: 2026-09-23      # how: hardware-walk (M2 desk sitting 1, spikes/ble-lab, Bluefy on iPhone)
fixed: bcd6e4dbd
area: lpa-link browser_ble (Studio's Web Bluetooth link, BLE M5); spikes/ble-lab/index.html
class: assumed-context
related:
  - lp2025/2026-09-23-1428-ble-remote-control/ (spike-results.md, Run F)
  - docs/defects/2026-09-23-xiao-c6-rf-switch-never-powered.md
---
# In Bluefy, a hidden page does not see its BLE link drop until it is shown again

**Symptom** — in M2 Run F, Bluefy 3.9.3 on iOS 18.7 held a Web Bluetooth
link to the desk XIAO (`LP-BLE-b48c`). The page went hidden (the phone was
carried and its screen slept) at `t=1790231285`. While it was hidden, the
board was reflashed and rebooted (~`1790231300`). That ended the link from
the board's side. The page recorded **nothing** until it was visible again.
Then, in the same millisecond, it got `disconnected` (`upMs: 721921`,
`t=1790231469265`) and `visibility: visible` (`t=1790231469285`). It then
reconnected to the held `BluetoothDevice` with no tap in 803 ms. The lab's
control channel (an SSE stream) closed whenever the page was hidden
(`[server] page SSE disconnected`, `pages_connected: 0`) and reopened on
return.

**Root cause** — iOS suspends a hidden web view's JavaScript. Bluefy
queues `gattserverdisconnected`, and the page's timers, until the page runs
again. The link itself is not the problem. With the page hidden for 30 s
and no board reboot, the BLE connection **survived**: no drop, and `upMs`
ran continuously 162318 → 212484. What goes missing is only the page's
*knowledge* of events while it cannot run.

**Fix** — M5 of the ble-remote-control plan (`bcd6e4dbd`, Studio's Web
Bluetooth link, `lp-app/lpa-link/src/providers/browser_ble/browser_ble.js`)
does each of the four things below: on `visibilitychange → visible` every
session re-reads `gatt.connected` (`recheckAll`), so a drop the hidden page
never heard is handled then, and a lost or parked session restarts its
reconnect on the held `BluetoothDevice`; the link carries no long-lived HTTP
stream. Decision record: `docs/adr/2026-09-24-ble-transport-studio.md` §3.
What the fix had to do:
- treat `visibilitychange → visible` as "state unknown": re-read the link
  before showing it as connected;
- run its reconnect on becoming visible, not only on
  `gattserverdisconnected`;
- never show a hidden period's silence as "connected and idle";
- not rely on a long-lived HTTP stream (SSE/WebSocket) staying open while the
  phone is locked.

Reconnecting itself needs no gesture in Bluefy. `navigator.bluetooth.getDevices()`
exists and returned the granted device, and the held `BluetoothDevice`
reconnected after both a page-initiated drop (954 ms) and a board reboot
(803 ms). So a one-tap "Reconnect" is a fallback, not the main path.

**Regression coverage** — `a_drop_the_page_never_heard_is_found_on_the_recheck`
in `lp-app/lpa-link/tests/browser_ble_conformance.rs` (`just
lpa-link-browser-test`), driving `recheckAll` without a real hide/show. The
phone half — lock the phone, reboot the board, unlock, see the drop and the
reconnect — is a step of the M7 desk walk (G4), not yet run.

**Lesson** — on iOS, "the page is connected" is only true while the page
runs. Any BLE UI on a phone must re-derive link state when it is shown
again, instead of trusting that it heard every event.

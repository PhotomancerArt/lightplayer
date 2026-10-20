---
status: fixed
found: 2026-10-17      # hardware-walk: just walk-ble-phone, Pixel 8a Android 14 Chrome 155 → fixture-c6 and CX2 c6-button (PR #1180's firmware)
fixed: 2026-10-17      # PR #1183 (claude/android-ble-mtu-grace): fw-esp32-common radio_link::mtu_grace + fw-esp32c6 ble_connection
area: fw-esp32c6 ble (ble_connection open_link) × fw-esp32-common radio_link (frame size fixed at open) × Android Chrome's GATT client
class: untested-path   # Mac Chrome and Bluefy exchange the MTU before they subscribe; Android does not always
related:
  - 2026-10-17-an-android-phone-connecting-over-bluetooth-panics-the-c6.md
  - ../adr/2026-09-24-ble-transport.md
---
# An Android reconnect can open the Bluetooth link at ATT MTU 23, and it stays at 12-byte frames

**Symptom** — on the first phone walks (2026-10-17), 2 of the 6 links an
Android phone opened came up at the Bluetooth minimum:

    [ble] link2: notifications on — link open (ATT MTU 23, frames 12 B + 8, serving, link RAM 2704 B, …)
    [ble] link2: parameters updated: interval_us=48750 latency=0 timeout_ms=5000 mtu=247

The other four opened at ATT MTU 247 (frames 180 B). On one of the two, the
page's first hello took ~2 s, and that connection was dropped for no login
within 10 s.

**Why** — the board sizes a link's frames when the central enables
notifications (`ble_connection::open_link` reads `att_mtu()` then, and the
session keeps that size for its life). Mac Chrome and Bluefy exchange the MTU
before they subscribe. Android Chrome, reconnecting to a device whose GATT
table it has cached, can write the CCCD first and exchange the MTU (517)
after, so the link is sized for 23 and never grows.

**Fix** — PR #1183, on the board's side: lp-link's bytes are untouched. A
central that subscribes while the connection is still at the Bluetooth
minimum gets a grace (`fw_esp32_common::radio_link::mtu_grace`, host-tested).
The board polls the MTU every 50 ms and opens the link the moment it rises,
or after 2 s at whatever it is. trouble-host answers the exchange itself and
raises no event for it, hence the poll. A frame written before the link opens
is dropped, as before; the board's own SYN at open starts the handshake.
Resizing a live session was not taken: lp-link fixes the frame size per
session, and its bytes are a never-break pin.

The grace is 2 s, not the 1 s first tried: on the desk the Pixel exchanged
the MTU 1034, 1054 and 1190 ms after its CCCD write, behind its own
connection-parameter request. Studio gives a Bluetooth link 5 s from GATT
open to the hello (`lpa_link`'s `DEFAULT_READY_DEADLINE`), which still holds
the grace, the handshake and the hello for a central that never exchanges.

**Walked** (`just walk-ble-phone`, lightplayer.app at the same base, Pixel 8a
Android 14 Chrome 155, CX1 and CX2, the board's console captured across the
power cuts): 21 of 21 walks passed. Of the 28 links captured (one run of five
lost most of its console to a capture that hung on a power cut), none opened
at ATT MTU 23. Three subscribed at 23 and waited: two on the 1 s build opened
at 247 on the deadline's poll (1034, 1054 ms), and one on the 2 s build
opened at 247 mid-grace (1190 ms), which is the poll catching the rise.

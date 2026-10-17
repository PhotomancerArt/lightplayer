---
status: open
found: 2026-10-17      # hardware-walk: just walk-ble-phone, Pixel 8a Android 14 Chrome 155 → fixture-c6 and CX2 c6-button (PR #1180's firmware)
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

**Not yet done** — choose a fix: wait briefly for the MTU exchange before
opening a link that subscribed at 23, or resize the session when the MTU
grows (an lp-link question: the frame size is fixed per session), or both.
Then walk `just walk-ble-phone` and count the links that opened at 23.

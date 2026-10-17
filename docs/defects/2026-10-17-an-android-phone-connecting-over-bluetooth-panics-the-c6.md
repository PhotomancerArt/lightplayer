---
status: open
found: 2026-10-17      # hardware-walk: the first phone walk (just walk-ble-phone), Pixel 8a Android 14 Chrome 122 → fixture-c6, lightplayer.app
area: fw-esp32c6 ble (trouble-host RxRunner over LpHciTransport, esp-radio 0.18 fork) × Android's Bluetooth stack × lpa-studio-core device_effects (the failure is only a console warning)
class: untested-path   # every Bluetooth walk before this one was Mac Chrome or Bluefy; no Android central had ever connected
related:
  - 2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md   # the same receive path, a chained ACL packet cut short
  - ../adr/2026-09-24-ble-transport.md
  - ../../scripts/phone/README.md
---
# An Android phone connecting over Bluetooth panics the C6, every time

**Symptom** — Studio in a Pixel 8a's Chrome (Android 14, Chrome 122,
lightplayer.app) presses Bluetooth and picks the board in the phone's own
chooser. Nothing else happens: no card, no error on the page. The page's
console has one warning,

    device grant request failed: GATT Server is disconnected. Cannot retrieve services.

Each attempt panics the board and resets it. Reproduced on every attempt
(five of five) on `fixture-c6`, on two firmwares: an old proto-33 image
(commit `30ed7c05e`) and the current release, 2026.10.17-4
(`dd8534867`, proto 43, split image).

**What each end said** (one attempt, 2026-10-17 03:01, the board's USB
console read with `scripts/emu/tty-capture.py`, the phone's `adb logcat`):

- Phone: the LE link opens (`onClientConnectionState status=0`), Chrome asks
  for an ATT MTU of **517** (`configureMTU`), and Android starts primary
  service discovery (`GATTC_Discover … disc_type=1`). Nothing comes back:
  `gatt_rsp_timeout: retry discovery primary service` 5 s later, then the
  link drops with `HCI_ERR_CONNECTION_TOUT` / `GATT_CONN_TIMEOUT`, and
  Chrome reports status 8.
- Board, in between:

      ====================== PANIC ======================
      panicked at <redacted>:0:0:
      range end index 2 out of range for slice of length 0
      frames: 0x42064b44 0x4204dd8e 0x4206b32c 0x4206b2de 0x4206b296 0x420925d4 0x4206185e
              0x42038172 0x420554b4 0x4206ee6a 0x420334a8 0x420333e0 0x42069b12 0x420304ea 0x42030456
      [RECOVERY] crash committed to the RTC ledger
      [RECOVERY] resetting

**Where** — partly decoded. The release carries no ELF, and a local
`just fw-esp32c6-split` at the same commit gave a core 272 B shorter than the
release's (version stamp), so most frames land in unrelated functions. The
coherent middle of the chain is `fw_esp32c6::ble::ble_task` runner_task →
`trouble_host::host::RxRunner<ExternalController<LpHciTransport, 20>, …>`
(+0x860): the panic is in the host's receive path, the code the 2026-09-25
chained-ACL defect fixed one bug in. A slice `[..2]` on an empty buffer fits
an L2CAP or ATT header read on a zero-length payload. Mac Chrome and Bluefy
(MTU ~185) never sent whatever this is; Android's first moves are a
517-byte MTU request and, likely, a data length update to 251-byte packets.

**Studio's half** — the failed connection reaches the person as nothing at
all: the grant request's failure is a console warning, and the Connect a
board section looks as it did before the press.

**Not yet done** — decode against an ELF whose core hashes to the
release's; read the board's HCI traffic around the panic (the phone's
`just phone snoop` holds its side); fix and walk `just walk-ble-phone` green.

# BLE laptop desk walk (M7 S1), 2026-09-25

The laptop walk that comes before the phone walk (G4) in the BLE
remote-control plan. The full record, with screenshots and board consoles,
is Run M in the plan's `spike-results.md`
(`lp2025/2026-09-23-1428-ble-remote-control`, data in `m7-data/runM/`).

**Result: stopped at the knob. Two defects filed. G4 is blocked until the
first one is fixed.**

## Setup

- Board: desk XIAO ESP32-C6 `A0:F2:62:87:B4:8C`, on USB for power, no LEDs
  attached. Firmware: main's shipped image, `commit=23b7db5b3dbd proto=24`.
  Its firmware sources match main `ffc233bd3` and the deployed `5d40f95fb`.
- Bluetooth side: the deployed `https://lightplayer.app` (`/healthz` build
  `5d40f95fb`) in the MacBook's Chrome 153, run in the background and driven
  over CDP. **Not Bluefy and not a phone.**
- USB side: the same Studio code served locally in Brave on the bench port
  block, because the Mac's serial grant covers only localhost. CDP cannot
  answer a Web Serial chooser.
- Distance was not measured. The laptop was on the same desk as the board.

## Steps

| Step | Result |
|---|---|
| Turn on Bluetooth over USB | Studio says it failed, 4 of 4 times (`device did not respond within 5.0s`), but the board had taken the write. BLE comes up after a manual Reset. [Defect](../defects/2026-09-25-turn-on-bluetooth-reports-a-timeout-the-board-answered.md) |
| Add over Bluetooth | Pass |
| Unlock (typed; later remembered) | Pass. The board drops a link that has not unlocked after 10 s, so it dropped the link 3 times while the sheet was open; Studio reconnected each time |
| Play | Pass (through Open in editor) |
| Knob to its end | **Fail, 2 of 2.** `[hci] error parsing packet`, then the BLE host restarts and stops advertising. A USB reboot is the only way back. [Defect](../defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md) |
| One knob step, brightness | Pass. The board read back the stepped knob value |
| Wrong device password | Pass. Refused; after the 4th wrong try, "It will listen again in 2 s"; then the right one unlocks |
| Walk 5 m away | Never run (desk) |

Nothing here is a phone result. Bluefy's timing, its KDF speed and the range
test are still G4's to measure.

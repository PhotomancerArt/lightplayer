# BLE remote control: the laptop desk walk (Run M) and the phone walk (G4), 2026-09-25

The two walks that closed the BLE remote-control plan
(`lp2025/2026-09-23-1428-ble-remote-control`): the laptop desk walk (M7 S1,
Run M) and then Yona's phone walk, the plan's final gate G4.

**Result: G4 passed** (Yona, 2026-09-25). Run M stopped at the knob; that
defect was fixed (#831) before G4. G4 found one more: editing over Bluetooth
from the phone dropped the link. That fix is #834, and the phone confirmed it.

Both walks are **one board, one browser per platform and one room each**.
Nothing here is a claim about other phones, other boards or range.

## G4: the phone walk (Yona)

### Setup

- **People:** Yona, walking it himself.
- **Phone:** his iPhone, in Bluefy (the Web Bluetooth browser Studio names
  for iPhone). The phone's own console was never captured: Bluefy has no dev
  tools.
- **Laptop:** his MacBook, in the desktop browser, for the USB side.
- **Board:** the PLAYFUL choker, a XIAO ESP32-C6, `10:BD:A3:B0:A5:2C`.
- **Studio:** the deployed `https://lightplayer.app`, main `ccd19721a` or
  later (the easy-access Studio, #824).
- **Firmware:** not the image the prep flashed. The prep (director A22)
  flashed `ccd19721a` (proto 27), but the first console capture during the
  walk found `acabd97c1` (proto 26) on the board. The second repro ran on
  `a34cfee65` (proto 27).
- **Flow:** the easy-access product flow (plan DD33). The laptop signed in,
  the choker on USB (Studio adds its keys), then the phone over Bluetooth.

### Result

**Passed**, in Yona's words "mostly worked". The phone connected to the
choker over Bluetooth, and the knobs worked.

One open issue: **editing any setting over Bluetooth from the phone sent
Studio back to `/devices`, with no error on screen**, every time. A
reconnect then said the board "never said hello", and the board seemed to
need a reset. The Mac was fine, over USB and over Bluetooth. Three causes,
all fixed in #834 (merged `97efd6624`):

- trouble-host acknowledged a long ATT write and dropped its bytes;
- an ATT MTU of 251 let a full-size reply overflow the controller's
  packets;
- Studio's drop never disconnected the radio, while iOS kept the link up.

Defect:
`docs/defects/2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md`.

**Phone confirmation of #834:** the choker with #834's firmware half and
the deployed Studio (still 244 B writes, no teardown). Yona: "now
everything seems to be working", and editing held. The first connect of
that session said "no response" and the retry worked. At the time an
agent's lab page held the board's other connection slot and was
reconnecting every ~12 s. **The Studio half of #834 (180 B writes, teardown
on every drop) had not run on the phone when this was written;** it ran on
the Mac over CDP and in the `?ble=emu` conformance suite.

### Not recorded

- The plan's per-step list (a wrong password refused, walking ~5 m away and
  back): no step-by-step record of these was kept for G4.
- Bluefy's KDF time per derivation. Generated keys use one iteration, so
  only a typed password pays PBKDF2 now.

Also found at G4: a board whose saved project is one format behind new
firmware boots with nothing loaded (`unsupported project format 10
(expected 11)`), and only a Studio push upgrades it. It is logged as a
follow-up in the plan's director log, not as a defect.

## Run M: the laptop desk walk (M7 S1)

The full record, with screenshots and board consoles, is Run M in the
plan's `spike-results.md` (data in `m7-data/runM/`).

**Result: stopped at the knob. Two defects filed, both since fixed.**

### Setup

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

### Steps

| Step | Result |
|---|---|
| Turn on Bluetooth over USB | Studio said it failed, 4 of 4 times (`device did not respond within 5.0s`), but the board had taken the write. BLE came up after a manual Reset. [Defect](../defects/2026-09-25-turn-on-bluetooth-reports-a-timeout-the-board-answered.md), fixed in #824 |
| Add over Bluetooth | Pass |
| Unlock (typed; later remembered) | Pass. The board drops a link that has not unlocked after 10 s, so it dropped the link 3 times while the sheet was open; Studio reconnected each time. Since #831 the deadline waits for an outstanding challenge (up to 30 s) |
| Play | Pass (through Open in editor) |
| Knob to its end | **Fail, 2 of 2.** `[hci] error parsing packet`, then the BLE host restarted and stopped advertising. A USB reboot was the only way back. [Defect](../defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md), fixed in #831 |
| One knob step, brightness | Pass. The board read back the stepped knob value |
| Wrong device password | Pass. Refused; after the 4th wrong try, "It will listen again in 2 s"; then the right one unlocks |
| Walk 5 m away | Never run (desk) |

Nothing in Run M is a phone result.

### After the fix (#831), same desk board

Silicon, desk XIAO `A0:F2:62:87:B4:8C`, image `077941dab`, deployed Studio
in Mac Chrome over CDP:

- Play, Reset Scale then `End` (a 186 B write): **10 of 10** with the link
  held, and 10 more earlier (20 of 20).
- A forced host restart (a `desk_ble_fault` build) came back **5 of 5**.

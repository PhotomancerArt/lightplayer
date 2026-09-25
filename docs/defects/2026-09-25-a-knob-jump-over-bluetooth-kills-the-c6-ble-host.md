---
status: open
found: 2026-09-25      # how: hardware-walk (M7 S1 laptop desk walk, Run M: desk XIAO C6 + Mac Chrome 153 over CDP, deployed Studio 5d40f95fb)
area: fw-esp32c6 ble (ble_task host-runner restart, advertising) × esp-radio 0.18 BLE controller HCI × Studio Play panel writes over `ble:`
class: untested-path
related:
  - docs/adr/2026-09-24-ble-transport.md
  - lp2025/2026-09-23-1428-ble-remote-control (spike-results.md, Run M; m7-data/runM/)
---
# A knob jumped to its end over Bluetooth kills the C6's BLE host, and it never advertises again

**Symptom** — in Studio's Play mode over Bluetooth (unlocked at edit), sending
the Scale knob to its maximum (a keyboard `End` on the knob, 1 → 4) dropped
the link within a second or two. The page said
`transport error: Transport error: bluetooth link lost: the board or the radio
ended the connection` and `the board under the editor went away; the editor is
closed`, and went back to `/devices`. The board's console (USB, non-resetting
reader) printed, right after the write and before any disconnect line:

    [WARN] esp_radio::ble::controller: [hci] error parsing packet:
    [ERROR] fw_esp32c6::ble::ble_task: [ble] host runner error — restarting it
    [INFO] trouble_host::host: [host] initialized
    [INFO] trouble_host::host: [host] Device Address A2:F2:62:87:B4:8D

and then nothing more from `[ble]`: no `disconnected` line for the link, and
no `advertising as …` line. The render loop and USB heartbeats carried on. A
Bluetooth chooser then found **no device for 40 s**. Only a reboot (over USB)
brought advertising back.

Reproduced **2 of 2** with `End` (Run M, 09:52 and 09:57 PDT, fresh boot and
fresh unlock each time). In the same session, on the same link, a one-step
knob change (`ArrowUp`, 1 → 1.04, read back from the board) and a brightness
change (0.2 → 0.8) went through with no error. So it is not "any panel write";
what differs about the `End` write is not known.

**Root cause** — not diagnosed. Two separate faults are visible:

1. something in or after that write makes the controller hand the host a
   packet it cannot parse (the `[hci]` line's reason text is empty in this
   build);
2. the firmware's recovery (restart the host runner) comes back without
   advertising, and without reporting the old link as gone, so the board is
   unreachable over Bluetooth until someone with USB reboots it. The address
   the restarted host prints (`A2:F2:62:87:B4:8D`) is not the random static
   address the board advertised before (`9F:F2:64:5D:AB:00`), which suggests
   the restart does not reuse the original setup.

Fault 2 turns any HCI hiccup into a dead remote for a piece on battery, which
is the product's whole BLE use case.

**Fix** — none yet.

**Regression coverage** — none: the host-runner restart path has never run
before this walk, and the emulated `?ble=emu` path does not go through the
controller or `trouble-host`.

**Lesson** — a recovery path that has never run is a guess. The BLE task's
"restart the host runner" branch needs the same bring-up the boot path does
(address, GATT, advertising), and a test that forces it.

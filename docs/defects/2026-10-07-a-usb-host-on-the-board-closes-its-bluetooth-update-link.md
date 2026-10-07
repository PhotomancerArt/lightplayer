---
status: open
found: 2026-10-07      # how: desk run (fixture C6, Mac Chrome via CDP, OTA M7 BLE speed pass)
area: fw-esp32-common `LinkMuxTransport::release_radio_holders` × the update hook's read-back chunks (`RadioLinkPort::send_update`) × a USB host on the same board
class: fixed-budget-over-variable-work
related:
  - docs/defects/2026-10-06-a-stalled-radio-link-holds-the-tick-past-the-watchdog.md (the per-tick wait budget this meets)
  - docs/adr/2026-09-24-ble-transport.md (decision 3, the shared frame buffer)
  - lp2025/2026-10-05-0820-ota-studio-ble-updates (ble-speed-progress.md, surprise 1)
---
# A USB host on the board closes its Bluetooth update link

**Symptom** — fixture C6 (`A0:F2:62:87:B4:8C`), image X
`a0a0a0a0+a3bf56bebe57` (PR #1005 merged with main's Wi-Fi link), Studio
in Mac Chrome over Bluetooth, an update that starts with a backup (the
engine read back over channel 3). With `lp-cli link capture` hosting the
board's USB link at the same time, the backup ran at 1.7 KiB/s for 17 s and
then the board closed the Bluetooth link:

```
radio link link4: a reply still not out of the frame buffer after 1584 ms (1588 B held; 1584 ms of the tick's wait budget left) — closing
[ble] link4: closing at the server's request (reply deadline)
```

Studio reconnected in 2.0 s, the board answered four channel-3 messages on
the new link with `[OTA] refused` (the reason printed empty), and the card
stayed at "Backing up current firmware… 0%" until the run gave up 530 s
after the press. With no USB host, the same build on the same board read
the backup back at a steady 7–11 KiB/s to 99 % (run `a1f`, host load
33–65).

Before main's per-tick budget (PR #1005 at `91ac08361`, 2026-10-06) the
same pairing stalled the backup instead (stuck at 0–2 % with the capture
attached, 9.7 KiB/s without); main's change turned the stall into a closed
link.

**Root cause (likely; one board log line, not yet proven)** — a read-back
chunk (`D`, up to 4 KiB) leaves the board as the Bluetooth link's external
message out of the one shared frame buffer, so the link holds the buffer
until its last fragment is acknowledged. With a USB host up, the server
also writes that host's replies (heartbeats, answers) through the same
buffer, and before each it waits for every radio link to let go
(`release_radio_holders`). That wait is now cut by what is left of the
tick's 5 s budget, and the line above shows the budget nearly spent
(1584 ms left). A link that cannot finish a chunk inside it is closed. The
slow air of a Bluetooth link under a busy central makes that likely.

Not established: why the reconnected link's channel-3 messages were
refused (a session that still holds the closed link as owner, or a login
the reconnect did not redo), and whether the same close explains two
backups that ended at 58–59 % with no USB host at host load 100–180 (runs
`a1d`, `a1e`; no board log was captured there, and the board had not
reset).

**Workaround** — keep the board's USB unplugged from any host (or the
host's link closed) during a Bluetooth update.

**Fix** — none yet. The director decides between, for instance, sizing a
Bluetooth link's wait against the chunk it holds rather than the tick's
budget, or keeping the update hook's chunks out of the shared buffer.
Evidence:
`~/.photomancer/planning/lp2025/2026-10-05-0820-ota-studio-ble-updates/data/ble-speed-2026-10-07/s1-usb-backup/`
(the board's console `usb-010735.txt`, the page's `page-console.log`).

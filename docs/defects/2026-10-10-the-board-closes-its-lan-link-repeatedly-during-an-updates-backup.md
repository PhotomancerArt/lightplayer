---
status: open
found: 2026-10-10      # how: hardware-walk (RAM research V1, CX1 joined to a home Wi-Fi network)
area: fw-esp32c6 `lan_endpoint_task` (the LAN WebSocket link) × an update's backup read-back, on a low heap
class: unclassified   # the board ends the link and why is not known; suspected resource pressure (budget-exhaustion) — reclassify on cause
related:
  - docs/adr/2026-10-06-ota-update-protocol.md
  - docs/adr/2026-10-06-radio-frame-rate-budget.md
  - docs/defects/2026-10-10-a-lan-update-after-a-busy-session-resets-the-board-as-the-backup-begins.md
  - lp2025/2026-10-09-1203-ram-research (experiments/v1-1092-joined-wifi/report.md)
---
# The board closes its LAN link repeatedly during an update's backup

**Symptom** — during the backup of an over-the-air update over the LAN, on
silicon CX1 (ESP32-C6, 14:C1:9F:E6:54:90) joined to a home Wi-Fi network,
the board ends the WebSocket link itself, several times in one update.
`lp-cli link capture` reports each as:

    LAN link lost (the board closed the link (WebSocket close 1000)) at 17.396 s

The release image (2026.10.10-7, `a52b5e0b485e`) did it 3 times in one
update (at 17.4, 32.3 and 52.8 s). The PR image under test (#1092) did it
5 times in one (at 8.2, 48.5, 52.4, 58.0 and 68.0 s) and once refused the
reconnect: `Connection refused` twice, then `LAN link back at 71.688 s
after 3.6 s away, 3 tries`. Memory in those runs: 22–27 KB free, with a
largest block of 5.2–12 KB. The host re-dials each time and the update
completes (UpToDate in 105 s on the release, 124 s on the PR image). The PR
does not cause it: the release image does it too.

**Root cause** — not diagnosed. The close is a normal WebSocket close (code
1000), not a reset or a timeout, so the board's link side chose to end the
session. It happens with the heap short (the backup reads 4 KiB chunks
against a largest block of 5–12 KB), which is the suspect, but no path
from a short heap to a close has been traced. The sibling entry (the reset
as the backup begins) is the same conditions ending in an abort instead.

**Fix** — none yet. First find which branch of the LAN endpoint ends a link
with code 1000 under these conditions (a failed buffer growth, a send-ring
limit, the one-slot busy rule), then make that case a refusal the host can
wait out, not a close.

**Regression coverage** — none yet.

Evidence (RAM research V1,
`experiments/v1-1092-joined-wifi/evidence/ota/` in
`lp2025/2026-10-09-1203-ram-research`): `a-rel-to-x-lpcli.log` (release
control, 3 closes), `y-to-x-lpcli.log` (PR image, 5 closes and the 3.6 s
refusal); the heartbeats in `y-to-x.txt` and `a-rel-to-x.txt` carry the
memory figures.

**Lesson** — `docs/adr/2026-10-06-radio-frame-rate-budget.md` says a
dropped link is a bug, never a hiccup, and the update completing does not
make it less of one: the host's retry hid a fault the update card never
showed. A host that reconnects silently should count what it reconnected
from.

---
status: open
found: 2026-10-06      # how: e2e (`just walk-ota-ble-emu`, step engine-less)
area: lpa-devices `PendingLinkView` × lpa-studio-core `UpdateHost` narration × lpa-studio-web pending card
class: partial-knowledge-loss
related:
  - docs/adr/2026-10-06-studio-updates-over-the-update-channel.md
  - lp2025/2026-10-05-0820-ota-studio-ble-updates (P5, P6, P12)
---
# A restore on a pending link narrates to no terminal

**Symptom** — an engine-less board restores itself on connect with no click,
on both USB and Bluetooth. While it does, its card is a pending link
("Identifying", "Nothing from this board yet.", one status line "Restoring
firmware… 9% · …"), because a core-only board says no hello. When the
restore ends, the board identifies and gets a fresh card whose terminal
starts at "Identifying". The update's own lines (the reconnect times, the
engine's bytes and rate) were never shown. Over Bluetooth the reset in the
middle of the restore is a GATT drop and a reconnect, and its time is not on
any screen.

**Root cause** — `PendingLinkView` carries no terminal. The update host says
its lines to the provisional device's journal, and the card that device
becomes does not show them.

**Fix** — none yet. Options: give the pending card the same terminal as a
settled card, or carry the provisional device's terminal over when its link
is kept or merged.

**Regression coverage** — none: `walk-ota-ble-emu`'s engine-less step
records that the reset dropped and reconnected (`resetDrops`) but cannot
require a timed line (`checkReconnects(…, { timed: false })`).

**Lesson** — a flow that runs with no click before the board has a card
still owes the person its record. A restore should leave the same lines in
a terminal as an update does.

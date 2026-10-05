---
status: open
found: 2026-10-04      # how: code reading while fixing the tab board's update erase
area: lpa-studio-core emu_transport (InspectLayout) / lpa-link emulator_tab_bridge.js
class: stand-in-divergence
related:
  - docs/defects/2026-10-02-updating-a-tab-hosted-board-erases-its-files.md
  - lp2025/2026-10-01-1843-c6-repartition
---
# A tab-hosted board on the legacy layout cannot finish its update

**Symptom** — read from the code, not yet reproduced in a walk. A user's
tab-hosted board (mode A, `EmuDeviceTransport`) whose chip was written
before the C6 repartition keeps its files at `0x310000`. Since the update
stopped erasing the chip, an update to current firmware writes only the
image. The firmware then finds the legacy filesystem, holds it and runs on
a memory FS (`LegacyHeld`). The card offers **Finish update**. On this
transport `InspectLayout` answers "no layout to read" and the flash is plain
again, so the board comes back held every time. Its files are safe and
unusable. Before the fix they were erased.

**Root cause** — `EmuDeviceTransport` has no layout step. The migration
(`lpa-link` `layout_migration`: inspect, `plan_migration`, a backup, a
`FlashPlan`) is run only by the esptool executors. The tab port can read the
whole chip (`getFlash`) and write any region (`writeFlash`), which is all an
executor needs. Nothing wires the two together.

**Fix** — none yet. Answer `InspectLayout` from `getFlash` (the table at
`0x8000`, the legacy region) and run the decided `FlashPlan` over
`writeFlash`, with the same backup rule as a board.

**Regression coverage** — none: no walk drives a mode-A update of a
legacy-layout board.

**Lesson** — fixing a stand-in's write to match a board's makes it match the
board's next step too. Here the next step is a migration this stand-in
never had.

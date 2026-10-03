---
status: open
found: 2026-10-02      # how: code reading while adding the C6 repartition's tab walk (W12)
area: lpa-studio-core emu_transport (FlashFirmware) / lpa-link emulator_tab_bridge.js flashEmuPackage
class: stand-in-divergence
related:
  - lp2025/2026-10-01-1843-c6-repartition
  - docs/adr/2026-09-10-the-c6-emulator-runs-in-the-tab.md
---
# Updating a tab-hosted emulated board erases its files

**Symptom** — not observed in a walk; read from the code, and not yet
reproduced. Studio's Update firmware on an emulated board hosted in the tab
for a user (the ADR's "mode A": `EmuDeviceTransport`, not the `?emu=tab`
polyfill) runs `flashEmuPackage`
(`lp-app/lpa-link/src/providers/emulator_tab/emulator_tab_bridge.js`), which
calls `TabEmulatorPort.putFlash` (`emulator_tab.js`): a `flash-erase` that
the worker runs as `flashEraseChip()` — the **whole chip** — and then the
merged image written at `0x0`. The board's filesystem (at `0x310000` on the
old table, `0x350000` on the new) is erased with everything else, so the
board comes back with its projects, its stamped `/hardware.json` and its
`/.lp/` identity and access files gone. That is the case on main too: the
repartition did not introduce it. The repartition's own `InspectLayout`
answers "no layout to read" on this transport, and its comment claimed the
filesystem "stays where it is"; that comment was wrong and now points here.

**Root cause** — the tab backing's flash verb was built for a blank board
(the ADR's decision 6: "fetches the served package … writes it directly",
and Erase is the same erase), and the Update verb reuses it on a board that
holds files. A real board's update (esptool, the ROM, `--erase-parts` only
where it writes) never erases outside the image; this stand-in does.

**Fix** — none yet. Candidates: write the image without the whole-chip
erase (erase only the sectors the image covers, as a flasher does), or run
the repartition's own layout step over `getFlash`/a sector write so a tab
board migrates like a board.

**Regression coverage** — none: no walk drives a mode-A update of a board
with files. (W12 drives the `?emu=tab` **polyfill** lane, which flashes
through esptool-js and the ROM and keeps every file.)

**Lesson** — a stand-in's write verb that is right for a blank chip is wrong
the first time it is pointed at one that holds data.

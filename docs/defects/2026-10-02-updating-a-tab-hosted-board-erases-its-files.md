---
status: fixed
found: 2026-10-02      # how: code reading while adding the C6 repartition's tab walk (W12)
fixed: this change
area: lpa-studio-core emu_transport (FlashFirmware) / lpa-link emulator_tab_bridge.js flashEmuPackage
class: stand-in-divergence
related:
  - lp2025/2026-10-01-1843-c6-repartition
  - docs/adr/2026-09-10-the-c6-emulator-runs-in-the-tab.md
  - docs/defects/2026-10-04-a-legacy-tab-board-cannot-finish-its-update.md
---
# Updating a tab-hosted emulated board erases its files

**Symptom** — not observed in a walk; read from the code, then reproduced
on 2026-10-04 with the real tab module, the real `TabEmulatorPort` and the
bridge's write (a 2.75 MB image over a board with files at `0x350000`):

```
before update: lpfs@0x350000 = 07 26 45 64 83 a2 c1 e0 6c 69 74 74  intact=true
after update:  lpfs@0x350000 = ff ff ff ff ff ff ff ff ff ff ff ff  intact=false  all-0xFF=true
```

Studio's Update firmware on an emulated board hosted in the tab
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

**Fix** — `TabEmulatorPort.writeFlash(offset, bytes)` is a flasher's write:
it erases only the 4 KiB sectors its range touches (`emu_flash_write`'s own
rule). The bridge's package write (`writeEmuPackage`, which `flashEmuPackage`
calls) uses it. The packaged image is `--skip-padding`, so it ends with the
app and the `lpfs` partition is never touched, as with esptool on a board.
`putFlash` stays the whole-chip verb, for Erase. A board on the **legacy**
table now keeps its files too, held by the firmware at `0x310000`. This
transport cannot migrate them, so its Finish update does not finish (open:
[a-legacy-tab-board-cannot-finish-its-update](2026-10-04-a-legacy-tab-board-cannot-finish-its-update.md)).

**Regression coverage** — `lp-app/lpa-link/tests/js/emulator_tab_update.test.mjs`
(`just lpa-link-js-test`): the real port and bridge over a hub that answers
the worker's flash verbs. It failed with the old write (`0x350000+0 is
0xff`) and passes, for files at `0x350000` and `0x310000`. (W12 drives the
`?emu=tab` **polyfill** lane, which flashes through esptool-js and the ROM.)

**Lesson** — a stand-in's write verb that is right for a blank chip is wrong
the first time it is pointed at one that holds data.

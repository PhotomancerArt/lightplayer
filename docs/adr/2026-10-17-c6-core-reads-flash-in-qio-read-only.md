# ADR: The C6 core reads its flash in QIO, read-only, on parts it has seen

- **Status:** Proposed
- **Date:** 2026-10-17
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Plan:** `lp2025/2026-10-16-0848-tree-store-adoption` (PR #1172; the
  experiment is `docs/reports/2026-10-17-c6-flash-mode-and-first-light.md`,
  PR #1170)

## Context

The C6's image header says DIO, and the mask ROM, the second-stage
bootloader (the stock ESP-IDF one or our own) and the split image's loader
all read in DIO. By the time the core runs, the flash bus is already at
80 MHz; the one step left is the bus width. A QIO header bricks the ROM's own
load on CX1 (it boots nothing until reflashed over USB), so QIO can only come
from code that runs after the boot chain.

QIO needs the part's Quad Enable (QE) bit set. QE is a non-volatile
status-register bit, and a status write is a flash write: a power cut inside
one can leave protection bits behind. The two desk parts read so far ship
with QE already set at the factory.

The ROM's `esp_rom_spiflash_config_readmode(QIO)` is not read-only: in the
C6's mask ROM (`esp32c6-20220919`) it calls `Enable_QMode`, which reads SR2
and **writes** the status register when it reads QE clear.

## Decision

1. **The core switches, nothing else does.** `fw-esp32c6`'s `flash_qio`
   runs once, right after esp-storage's `FlashStorage::new` (whose own `RDID`
   must precede any ROM flash access) and before the boot records, the mount
   and the project load. The header, the bootloaders and the loader stay DIO;
   every reset starts from DIO again.
2. **Read-only.** It reads `RDID` and SR2 (`0x35`) and never writes a status
   register. It switches through the ROM's `SPIMasterReadModeCnfig`, which
   makes `config_readmode`'s controller writes without the QE step, then
   `esp_rom_spiflash_select_qio_pins(0, 0)`.
3. **Only on parts a desk sitting has read.** `fw_esp32_common::flash_qio::
   QIO_PARTS` lists whole JEDEC ids (CX1's XMC `46 40 16`, the XIAO C6's XMC
   `20 40 16`); QE must read set. Anything else stays DIO and says why in the
   boot's one `[flash]` line. A part joins the list after a sitting reads its
   id, SR2 and eFuse flash fields and runs the QIO read-back soak — never by
   family.
4. **Checked before the cache resumes, reverted on doubt.** With the cache
   suspended, from RAM: the controller must have been in DIO and must read
   back as QIO (`0xEB`, a 32-bit address phase, two more dummy cycles than
   DIO had), and 32 bytes at flash `0x0` read over SPI1 in QIO must equal the
   same bytes read in DIO just before. Otherwise every register the switch
   wrote goes back and the core stays DIO.

## Consequences

- First light (fs-tree, `projects/test/basic`, fresh store, medians of 10
  power-on boots, silicon) drops by ~62 ms: CX1 577 → 516 ms and the XIAO
  CX2 589 → 526 ms through the stock IDF door; CX1 reads 378 ms in QIO
  through our own second stage. It reaches fielded boards as a core update;
  nothing persisted changes, nothing on the wire.
- Cost: ~1.5 KB of core flash and the switch's ~330 B of `.rwtext`, which on
  the C6 comes out of the main stack (49,104 → 48,744 B on CX1).
- The emulator names the part a run boots on (`--flash-part`, `FlashPart`);
  its default, the ROM's own chip, stays DIO, so no transcript moves.
- A new board family with another flash part reads in DIO until a sitting adds
  it — the safe failure.
- Found on the way, not changed here: esp-storage's unlock
  (`esp_rom_spiflash_unlock`) writes the status register before the first
  flash write of every boot that writes (QE kept, protection cleared).

## Alternatives Considered

- **A QIO image header.** Bricks the ROM load on CX1 (#1170). Never.
- **A QIO-capable second-stage bootloader.** Only its own few-KB read gains;
  the core still needs the switch. No benefit beyond this.
- **Setting QE when it reads clear.** A persistent status write on every
  board it meets, under a power cut. The gain is not worth that risk.
- **`esp_rom_spiflash_config_readmode`.** Writes the status register when its
  own read says QE is clear (above).
- **A family-wide allowlist (any XMC, any `0x40` type).** The QE location and
  the QIO dummy count are per part; two ids read on silicon are what we know.

## Follow-ups

- Add each new desk part after its sitting (`lp-cli/tests/board_flash_qio.rs`,
  `LP_QIO_*`).
- The switch's RAM could move off the main stack (LP SRAM) if the stack gate
  tightens.

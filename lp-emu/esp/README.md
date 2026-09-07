# `lp-emu/esp/` — the Espressif SoC layer

This directory is the vendor namespace decided in vision D9: architecture
cores live at the root of `lp-emu/`, and anything that assumes a *chip* — a
memory map, MMIO decode, peripherals, a ROM image — lives under the vendor it
belongs to.

- **`lp-emu-esp-common/`** — the SoC substrate every ESP machine shares: the
  bus (`SocBus`), the MMIO decode table, the `Peripheral` trait and its
  `BusCx`, `RegFile` (accept-and-remember with a table of exceptions), the
  bus trace with its spin detector, host byte streams, and the PT_LOAD view
  of an ELF. It holds **no chip numbers** — see its README.

Landing here from later phases of M3 of the 2026-09-06 esp-emulator plan:

- `lp-emu-esp32c6/` — the C6 machine: reset, memory map, ROM loader and
  intercept table, interrupt matrix, the C6's peripheral set, and the
  generated register-name tables (`src/regs/`, from
  `scripts/emu/pac-regnames.py`).
- `roms/` — the vendored ROM ELFs (Apache-2.0, from `esp-rom-elfs`), with
  their LICENSE and checksums (vision D6).

Everything placed here is MIT and inside the fence — see `../README.md` and
`just lint-emu-fence`.

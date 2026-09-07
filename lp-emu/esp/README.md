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

- **`lp-emu-esp32c6/`** — the C6 machine: the memory map, the mask-ROM loader
  and its hook table, direct load, the scheduler run loop, snapshot and the
  CLI, plus the generated register-name tables in `src/regs/` (from
  `scripts/emu/pac-regnames.py`). This is where the chip numbers live — see
  its README. It has no peripherals yet: P4's gate is the first access nothing
  claims, and P5 models the blocks that fault.
- **`roms/`** — the vendored ROM ELFs (Apache-2.0, from `esp-rom-elfs`
  release `20260528`), with their LICENSE and checksums (vision D6). The ROM
  is loaded in **every** configuration, because the application calls into it
  at runtime whatever booted it (plan PD7).

Everything placed here is MIT and inside the fence — see `../README.md` and
`just lint-emu-fence`.

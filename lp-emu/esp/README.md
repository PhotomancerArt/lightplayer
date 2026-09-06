# `lp-emu/esp/` — the Espressif SoC layer

Empty on purpose. This directory reserves the vendor namespace decided in
vision D9: architecture cores live at the root of `lp-emu/`, and anything
that assumes a *chip* — a memory map, MMIO decode, peripherals, a ROM image —
lives under the vendor it belongs to.

Landing here from M3 of the 2026-09-06 esp-emulator plan:

- `lp-emu-esp-common/` — bus, MMIO decode table, peripheral traits shared
  across ESP chips.
- `lp-emu-esp32c6/` — the C6 machine: reset, ROM loader and intercept table,
  interrupt matrix, scheduler, and the C6's peripheral set.
- `roms/` — the vendored ROM ELFs (Apache-2.0, from `esp-rom-elfs`), with
  their LICENSE and checksums (vision D6).

Everything placed here is MIT and inside the fence — see `../README.md` and
`just lint-emu-fence`.

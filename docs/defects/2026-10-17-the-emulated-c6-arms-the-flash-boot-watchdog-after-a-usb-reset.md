---
status: open
found: 2026-10-17      # CX1 (c6-expendable, 14:C1:9F:E6:54:90), own-bootloader parity sitting, V9
area: lp-emu-esp32c6 periph/timg.rs (MWDT0 flash-boot protection) × reset kinds
class: fidelity
related:
  - 2026-10-17-the-c6-boots-through-our-own-bootloader   # ADR, "Silicon (CX1)"
---
# The emulated C6 arms the flash-boot watchdog after a USB reset; silicon did not

**Symptom.** A second stage that spins at its entry (`j .`), flashed at `0x0`:

- **Silicon, after an RTS reset** (`rst:0x15 (USB_UART_HPSYS)`): one ROM
  banner, `entry 0x40850000`, then nothing for 10 s — no `TG0_WDT` reset.
- **Silicon, after a power cycle**: `rst:0x7 (TG0_WDT_HPSYS)` loops, `Saved
  PC:0x40850000`, as the flash-boot protection should.
- **Emulator, after `rst:0x15`**: `TG0_WDT` loops at ~325 ms, like a power-on.

So on silicon MWDT0's flash-boot protection was not armed by the USB-UART
(HP) reset, and the emulator arms it on that reset too. One sample of each;
the reset kinds a requested reboot (`rst:0x3`) and an RWDT reset produce
were not tried.

**Why it matters.** A bring-up that leans on the flash-boot watchdog to end a
hang in the second stage would pass on the emulator and spin on silicon
after a host reset. Our stage does not lean on it: it arms the RWDT before
the jump, so a core that hangs early dies warm on either kind of reset.

**To do.** Measure which reset kinds arm `wdtconfig0.flashboot_mod_en` on
silicon (power-on, RTS, `rst:0x3`, RWDT, brownout) and model that in
`periph/timg.rs`'s reset handling; grade it in `validate.toml`.

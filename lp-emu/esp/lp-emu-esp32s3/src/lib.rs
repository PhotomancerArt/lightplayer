//! The ESP32-S3 (LX7) machine — **not yet a machine**.
//!
//! This crate is where the S3's chip numbers will live, on the same plan as
//! its two siblings: `lp-emu-esp-common` supplies the bus, the peripheral
//! model, the trace and the ELF view and knows nothing about any chip;
//! `lp-xt-emu` supplies the Xtensa hart and knows nothing about MMIO; here
//! they are put together with a memory map, a mask ROM, a reset state and a
//! run loop, and the result takes a `fw-esp32s3` binary.
//!
//! # What is here today (M6 P01), and what is not
//!
//! [`regs`], the generated register-name tables, and **nothing else**. There
//! is no memory map, no bus, no hart, no peripheral, no loader and no boot.
//! P01's product is a measurement — the static inventory at
//! `docs/reports/2026-09-11-esp32s3-firmware-inventory.md` — plus the two
//! things that inventory needed a home for: the vendored
//! `esp32s3_rev0_rom.elf` (`lp-emu/esp/roms/`) and these tables. P03 makes it
//! a machine; see `README.md` for the order.
//!
//! The dependency edges in `Cargo.toml` are declared anyway, for the same
//! reason the classic's were: an edge added with the crate is reviewable, and
//! one added in the same commit as the code that uses it is not.
//!
//! # Three things P01 measured that a reader of the C6's or the classic's
//! code would otherwise assume wrongly
//!
//! - **The shipped image touches no UART.** The S3's console is
//!   `esp-println`'s `jtag-serial` and its only link is USB-Serial-JTAG, so
//!   `regs::UART0` is here for the mask ROM's own console on a ROM-up boot
//!   and for nothing else.
//! - **`EXTMEM`'s cache-enable polarity is inverted relative to the C6's** —
//!   the S3's `icache_ctrl.icache_enable` bit 0 is "0 disable, 1 enable"
//!   where the C6's `l1_icache_ctrl.l1_icache_shut_ibus0` is "0 enable, 1
//!   disable". A cache-off watch copied from the C6 arms backwards.
//! - **SRAM1's two views are both used.** Statically every executable section
//!   is on the I-bus; dynamically the product JIT path writes a shader through
//!   the D-bus view and fetches it through the I-bus alias `+0x6F_0000`. The
//!   report's §5 is the evidence and the one-sentence ruling.

pub mod regs;

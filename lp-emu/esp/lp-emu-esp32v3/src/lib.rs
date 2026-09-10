//! The classic ESP32 (v3, LX6) machine.
//!
//! This crate is where the classic's chip numbers live. `lp-emu-esp-common`
//! supplies the bus, the peripheral model, the trace and the ELF view and
//! knows nothing about any chip; `lp-xt-emu` supplies the Xtensa hart and
//! knows nothing about MMIO. Here they are put together with a memory map, a
//! mask ROM, a reset state and a run loop, and the result takes a
//! `fw-esp32v3` binary.
//!
//! It is `lp-emu-esp32c6`'s twin, deliberately: same shape, same module
//! names, different silicon. Where the two chips genuinely differ — the SRAM1
//! I-bus alias, the cache segment the bootloader executes from, the CH340
//! cable on the wire instead of a USB peripheral inside the chip — the
//! difference is written down where the number is, not left for a reader to
//! infer.
//!
//! # What is here today (M3 P1)
//!
//! [`memmap`], every base cited to `third_party/esp-hal/ld/esp32/memory.x`,
//! the hardware-measured facts in `lp-xt-emu`'s board profiles, the vendored
//! ROM ELF's own program headers or the `esp32` PAC; and [`regs`], the
//! generated register-name tables. The bus, the hart, the loader, the
//! peripherals and the CLI are M3 P2 onward — the crate is split this way so
//! the numbers and the vendored binary can be reviewed and merged on their
//! own, before anything depends on them.

pub mod memmap;
pub mod regs;

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
//! # What is here today (M3 P3)
//!
//! [`memmap`], every base cited to `third_party/esp-hal/ld/esp32/memory.x`,
//! the hardware-measured facts in `lp-xt-emu`'s board profiles, the vendored
//! ROM ELF's own program headers or the `esp32` PAC — and, since P3, the
//! classic's second peripheral window on the AHB bus; [`regs`], the
//! generated register-name tables; [`bus_setup`], which turns the map into
//! a [`lp_emu_esp_common::SocBus`]; [`rom`], which places the mask ROM and
//! seeds the data no program header carries; [`loader`], the direct load
//! and the eleven things it does not reproduce; [`periph`], the accept
//! blocks the strict bring-up pass demanded, in the order it met them;
//! [`machine`], the two-hart skeleton with core 1 stalled and the run loop;
//! [`snapshot`]; and [`test_support`].
//!
//! **No peripheral has behaviour yet.** Every block is an accept-and-remember
//! probe seeded from the PAC, which is what lets a `--strict-bus` boot run
//! until it spins on a register only a model can answer — the flash
//! controller's command word on the direct path, the eFuse read command on
//! the ROM path. Which phase answers which is the P3 report
//! (`docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md`).

pub mod bus_setup;
pub mod cache;
pub mod control;
pub mod intmatrix;
pub mod loader;
pub mod machine;
pub mod memmap;
pub mod periph;
pub mod regs;
pub mod rom;
pub mod snapshot;
pub mod test_support;

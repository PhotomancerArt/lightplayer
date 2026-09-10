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
//! # What is here today (M3 P2)
//!
//! [`memmap`], every base cited to `third_party/esp-hal/ld/esp32/memory.x`,
//! the hardware-measured facts in `lp-xt-emu`'s board profiles, the vendored
//! ROM ELF's own program headers or the `esp32` PAC; [`regs`], the generated
//! register-name tables; [`bus_setup`], which turns the map into a
//! [`lp_emu_esp_common::SocBus`]; [`rom`], which places the mask ROM and
//! seeds the data no program header carries; [`machine`], the two-hart
//! skeleton with core 1 stalled and the run loop; and [`snapshot`].
//!
//! **No peripheral is modelled yet.** The MMIO window is declared and empty
//! on purpose: under `--strict-bus` the first access to any block stops the
//! run and names it, which is the reading P3 takes, in order, before any of
//! them is built.

pub mod bus_setup;
pub mod loader;
pub mod machine;
pub mod memmap;
pub mod periph;
pub mod regs;
pub mod rom;
pub mod snapshot;
pub mod test_support;

//! The ESP32-C6 machine.
//!
//! This crate is where the chip numbers live. `lp-emu-esp-common` supplies
//! the bus, the peripheral model, the trace and the ELF view and knows
//! nothing about any chip; `lp-riscv-emu` supplies the privileged hart and
//! knows nothing about MMIO. Here they are put together with a memory map, a
//! mask ROM, a reset state and a run loop, and the result is something you
//! can hand a `fw-esp32c6` binary to.
//!
//! ```no_run
//! use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, StopCondition};
//!
//! let mut machine = Esp32C6Builder::new()
//!     .app(AppSource::Path("target/.../fw-esp32c6".into()))
//!     .strict(true)
//!     .build()?;
//! let outcome = machine.run_until(&StopCondition::after_micros(100_000));
//! println!("{outcome:?} after {} us", machine.micros());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # The five ideas
//!
//! 1. **The map is one place.** [`memmap`] holds every base and length, each
//!    cited to `esp-hal`'s linker script or `esp-metadata-generated`. Nothing
//!    else in the crate writes an address literal.
//! 2. **The ROM is loaded in every configuration** (plan PD7). The app calls
//!    into the mask ROM at runtime whatever booted it, so the ROM is part of
//!    the map — not an extra for a ROM-up boot. [`rom`].
//! 3. **Direct load reproduces the bootloader, and says what it does not.**
//!    [`loader`] carries the list of everything the real ROM-and-bootloader
//!    path does that this one does not; that list is the seed for M7's
//!    cross-check.
//! 4. **Time is a schedule, never a clock** (plan PD5). [`machine`]'s loop
//!    runs slices between scheduler deadlines and skips idle by moving guest
//!    time, so two runs of the same image are byte-identical. The one
//!    wall-clock input is `--wall-timeout`, which can end a run and cannot
//!    change one.
//! 5. **Unmapped is visible, and strict makes it fatal.** The bus counts and
//!    names every access to an address nothing claims. The bring-up loop is:
//!    run strict, read the first fault, model that block, run again.
//!
//! # The peripherals
//!
//! [`periph`] is the boot set: the interrupt matrix ([`intmatrix`]),
//! SYSTIMER, TIMG0 (the esp-rtos tick), the RWDT, software interrupts,
//! eFuse, the RNG — and every other block the no-radio image touches as an
//! accept-and-remember register file with its spin bits pinned. The README's
//! peripheral table lists each with its grade. What is *not* there is left
//! unmapped on purpose (the radio window, RMT, the flash controller), so a
//! strict run stops on the first block a later milestone owns.

/// THROWAWAY (M5 P1) — never merge.
#[cfg(feature = "block-profile")]
pub mod blockdump;
pub mod cache;
/// THROWAWAY (M5 P1) — never merge.
#[cfg(all(feature = "selfprof", target_os = "macos"))]
pub mod selfprof;
pub mod control;
pub mod flash;
pub mod image;
pub mod intmatrix;
pub mod loader;
pub mod machine;
pub mod memmap;
pub mod periph;
pub mod regs;
pub mod rom;
pub mod snapshot;
pub mod test_support;

pub use cache::CacheMmu;
pub use flash::{FlashBacking, FlashCensus, FlashImage};
pub use loader::{EfuseIdentity, ResetCause};
pub use machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, RomSource, StopCondition, TimeGrade,
    Uart0Sink,
};
pub use rom::{HookResult, HookTable, HostHook};
pub use snapshot::Snapshot;

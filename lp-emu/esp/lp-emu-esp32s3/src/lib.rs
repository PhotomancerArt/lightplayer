//! The ESP32-S3 (LX7) machine.
//!
//! This crate is where the S3's chip numbers live, on the same plan as its
//! two siblings: `lp-emu-esp-common` supplies the bus, the peripheral
//! model, the trace and the ELF view and knows nothing about any chip;
//! `lp-xt-emu` supplies the Xtensa hart and knows nothing about MMIO; here
//! they are put together with a memory map, a mask ROM, a reset state and a
//! run loop, and the result takes a `fw-esp32s3` binary.
//!
//! # What is here today (M6 P04), and what is not
//!
//! **A machine.** [`memmap`], every base cited to
//! `third_party/esp-hal/ld/esp32s3/memory.x`, the `.rwdata_dummy`
//! reservation in `esp32s3.x`, the vendored ROM ELF's own program headers and
//! symbol table, or the `esp32s3-0.35.2` PAC; [`bus_setup`], which turns the
//! map into a [`lp_emu_esp_common::SocBus`] — **including the SRAM1 I-bus
//! view, which is a RAM alias and not a second region** (M6 P02); [`rom`],
//! which places the mask ROM and seeds the data no program header carries;
//! [`loader`], the direct load and the eleven things it does not reproduce;
//! [`machine`], two hart slots with slot 1 held and the quantum run loop;
//! [`snapshot`]; [`test_support`]; and a binary, `lp-emu-esp32s3`.
//!
//! **The blocks before the console** (M6 P04): [`periph`] — the clock gates
//! and the four software interrupts (`SYSTEM`), the reset cause, the stall
//! key and **the RWDT that really runs** (`RTC_CNTL`), two counters per
//! timer group (`TIMG0`/`TIMG1`), the S3's `Instant::now()` (`SYSTIMER`),
//! the MAC and wafer version (`EFUSE`), the analog master, and the accept
//! blocks the mask ROM's cache setup and `esp_hal::init`'s inlined path
//! touch (`SENSITIVE`, `EXTMEM`, `SPI0`/`SPI1`, `APB_CTRL`, `BB`, `NRX`,
//! `FE`, `FE2`) — with [`intmatrix`], the two halves of the interrupt matrix
//! in the **mask form only** (X43). A `--strict-bus` run now gets past
//! `esp_hal::init` and stops at the console (`USB_DEVICE`), which is P05's.
//! There is still no flash cache or ROM-up boot (P06) and no pad fabric
//! (P07).
//!
//! # What P03 found, in four lines
//!
//! - **The first strict stop is the mask ROM's, not the application's**:
//!   `SENSITIVE.cache_dataarray_connect_1` at `0x600C_1004`, read from
//!   `Cache_Occupy_ICache_MEMORY+0xc` at cycle 36. The MMIO census in
//!   `m6/notes.md` §2.4 lists `sensitive` as untouched, and that is true of
//!   the application — the census swept its literals, and the ROM's are not
//!   in it.
//! - **`salt`/`saltu` have no arm in `lp-xt-inst`** (`tests/isa_gaps.rs`).
//!   Every one of this image's six sites is outside a sized code symbol, so
//!   nothing executed is one; the gap is reported, not filled by analogy.
//!   Every other S3-only mnemonic and SR the census named does have an arm.
//! - **A windowed call cannot cross a 1 GiB region.** `retw` rebuilds the
//!   return address as `PC[31:30] ‖ a0[29:0]`, so code in the SRAM1 **D-bus**
//!   view (`0x3FC8_xxxx`) cannot call the mask ROM (`0x4000_xxxx`) and
//!   return. The firmware's own IRAM is at `0x4037_xxxx` for that reason, and
//!   so are this crate's tests.
//! - **`CPENABLE`'s reset value is not `0xff` here.** It is a builder
//!   parameter defaulting to the ISA's generic reset; the classic's `0xff` is
//!   a classic-silicon measurement and the S3 firmware's own `fpu.rs` records
//!   its `0xff` reading as a fact about that boot chain (A4). P09 pins it.
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

pub mod bus_setup;
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

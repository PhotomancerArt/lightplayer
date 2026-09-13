//! The S3's peripheral set — **the blocks before the console** (M6 P04).
//!
//! Filled in one block per commit, in the order the strict bring-up loop
//! met them; `boot_set` and the registration order arrive with the wiring
//! commit once every view exists. See each module for what it models and
//! which sibling it came from.

pub mod accept;
pub mod system;
pub mod systimer;
pub mod rtc_cntl;

/// The crystal: **40 MHz**. esp-hal's S3 clock tree forces it —
/// `ClockConfig::configure` sets `XtalClkConfig::_40` with a `TODO: support
/// multiple crystal frequencies` beside it (`soc/esp32s3/clocks.rs:119-123`)
/// — so on this chip the crystal is not measured by a calibration the way
/// the classic's is; it is what esp-hal assumes.
pub const XTAL_HZ: u64 = 40_000_000;

/// APB, **80 MHz** — `apb_clk` in the generated S3 clock tree
/// (`esp-metadata-generated-0.4.0/src/_generated_esp32s3.rs`), and the
/// TIMG counters' source clock when a group's `use_xtal` bit is clear.
pub const APB_HZ: u64 = 80_000_000;

/// RC_SLOW, **136 kHz** — *modeled*. esp-hal's own S3 clock tree calls it
/// "136k RC_SLOW" and says in the same breath that it is not calibrated
/// there and can only be estimated (`soc/esp32s3/clocks.rs:8-10`). It is
/// the number behind the TIMG calibration's answer and therefore behind the
/// `store1` period the RWDT's timeout is derived from; nothing here measured
/// it.
pub const RC_SLOW_HZ: u64 = 136_000;

/// The watchdog write-protect key, `0x50D8_3AA1` — the same number on the
/// MWDTs and the RWDT (`rtc_cntl/mod.rs:558-563`, `timer/timg.rs`), and the
/// PAC's own reset value for both `TIMG0.wdtwprotect` and
/// `RTC_CNTL.wdtwprotect`, so both come out of reset **unlocked**.
pub const WDT_WKEY: u32 = 0x50D8_3AA1;

/// The super-watchdog's key, **`0x8F1D_312A`** — a different number from the
/// RWDT's on this chip (`rtc_cntl/mod.rs:656-660`: `#[cfg(not(any(esp32c6,
/// esp32h2)))] 0x8F1D_312A`; the C6 reuses the RWDT's), and again the PAC's
/// reset for `swd_wprotect` (`esp32s3-0.35.2/src/rtc_cntl/swd_wprotect.rs:44`),
/// so the SWD too is unlocked at power-on.
pub const SWD_WKEY: u32 = 0x8F1D_312A;

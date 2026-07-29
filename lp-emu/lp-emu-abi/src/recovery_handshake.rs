//! Host↔guest contract for crash-recovery testing on the emulator.
//!
//! The guest firmware (fw-emu) exports a `LP_RECOVERY_AREA` static; the
//! host test harness finds it via the ELF symbol map and reads/writes it
//! directly in guest RAM between runs. This stands in for the ESP32's RTC
//! fast RAM + reset-reason register:
//!
//! - Before (re)starting the guest, the host writes the reset-cause code
//!   and optionally restores the previous run's recovery-region bytes.
//! - Fault injection: the host writes a fault-request word; the guest
//!   checks it once per server-loop frame (and once at boot) and executes
//!   the fault inside recovery frames, writing back a result code.
//! - A guest-side reset request (crash path) surfaces to the host as a
//!   guest panic whose message is [`RESET_REQUEST_SENTINEL`].

/// Symbol name of the guest's recovery area static.
pub const RECOVERY_AREA_SYMBOL: &str = "LP_RECOVERY_AREA";

/// Byte offsets within the area (repr(C): four u32 words, then the
/// 8-aligned recovery region).
pub const RESET_CAUSE_OFFSET: usize = 0;
pub const FAULT_REQUEST_OFFSET: usize = 4;
pub const FAULT_ARG_OFFSET: usize = 8;
pub const FAULT_RESULT_OFFSET: usize = 12;
pub const REGION_OFFSET: usize = 16;

/// Guest panic message that means "the recovery system requested a system
/// reset" (the emulator analog of `software_reset()`).
pub const RESET_REQUEST_SENTINEL: &str = "__LP_RESET_REQUEST__";

/// Reset-cause codes (host → guest), mapped to `lp_recovery::ResetCause`.
pub const CAUSE_POWER_ON: u32 = 0;
pub const CAUSE_USER_RESET: u32 = 1;
pub const CAUSE_SOFTWARE_RESET: u32 = 2;
pub const CAUSE_WATCHDOG_RESET: u32 = 3;
pub const CAUSE_BROWNOUT: u32 = 4;
pub const CAUSE_UNKNOWN: u32 = 5;

/// Fault-request codes (host → guest).
pub const FAULT_NONE: u32 = 0;
/// Panic inside nested recovery frames; caught by the engine boundary.
pub const FAULT_RECOVERED_PANIC: u32 = 1;
/// Panic with an OOM-shaped message inside recovery frames (deterministic
/// stand-in for allocator exhaustion).
pub const FAULT_OOM_PANIC: u32 = 2;
/// Infinite loop inside recovery frames; host sees fuel exhaustion (the
/// emulator analog of a hardware-watchdog reset).
pub const FAULT_HANG: u32 = 3;
/// Panic with live recovery frames and NO catch boundary: exercises the
/// finalize-breadcrumb-and-reset path.
pub const FAULT_HARD_PANIC: u32 = 4;
/// Panic during boot (before the boot-complete milestone), uncaught.
pub const FAULT_BOOT_PANIC: u32 = 5;
/// Run the child frame cleanly (feeds return-to-green accounting).
pub const FAULT_CLEAN_CHILD: u32 = 6;

/// Fault-result codes (guest → host).
pub const FAULT_RESULT_NONE: u32 = 0;
pub const FAULT_RESULT_OK: u32 = 1;
/// The frames ran and returned an error (e.g. a caught panic).
pub const FAULT_RESULT_ERROR: u32 = 2;
/// Entry was denied: the path (or a parent) is gated red.
pub const FAULT_RESULT_GATED: u32 = 3;

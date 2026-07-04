//! Emulator recovery backend: the persistent region + host handshake area.
//!
//! `LP_RECOVERY_AREA` is exported (no_mangle) so the host test harness can
//! locate it via the ELF symbol map and read/write it in guest RAM between
//! runs — the emulator analog of RTC fast RAM + the reset-reason register.
//! Layout matches `lp_riscv_emu_shared::recovery_handshake` offsets.

use lp_recovery::{RecoveryBackend, RecoveryRegion, ResetCause};
use lp_riscv_emu_shared::recovery_handshake as hs;

#[repr(C, align(8))]
pub struct EmuRecoveryArea {
    /// Host → guest: why this boot happened (`hs::CAUSE_*`).
    reset_cause_code: u32,
    /// Host → guest: pending fault injection (`hs::FAULT_*`).
    fault_request: u32,
    /// Host → guest: fault argument (e.g. which synthetic child crashes).
    fault_arg: u32,
    /// Guest → host: result of the last executed fault (`hs::FAULT_RESULT_*`).
    fault_result: u32,
    /// The persistent recovery region (preserved across simulated reboots
    /// by the host harness).
    region: RecoveryRegion,
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".noinit")]
static mut LP_RECOVERY_AREA: EmuRecoveryArea = EmuRecoveryArea {
    reset_cause_code: hs::CAUSE_POWER_ON,
    fault_request: hs::FAULT_NONE,
    fault_arg: 0,
    fault_result: hs::FAULT_RESULT_NONE,
    region: RecoveryRegion::ZEROED,
};

fn area() -> &'static mut EmuRecoveryArea {
    // SAFETY: single-threaded guest; all access is sequential within the
    // firmware loop / boot path.
    unsafe { &mut *&raw mut LP_RECOVERY_AREA }
}

/// The reset cause the host wrote before starting this run.
pub fn boot_reset_cause() -> ResetCause {
    match area().reset_cause_code {
        hs::CAUSE_POWER_ON => ResetCause::PowerOn,
        hs::CAUSE_USER_RESET => ResetCause::UserReset,
        hs::CAUSE_SOFTWARE_RESET => ResetCause::SoftwareReset,
        hs::CAUSE_WATCHDOG_RESET => ResetCause::WatchdogReset,
        hs::CAUSE_BROWNOUT => ResetCause::Brownout,
        _ => ResetCause::Unknown,
    }
}

/// Take (and clear) a pending fault request from the host.
pub fn take_fault() -> Option<(u32, u32)> {
    let area = area();
    if area.fault_request == hs::FAULT_NONE {
        return None;
    }
    let request = (area.fault_request, area.fault_arg);
    area.fault_request = hs::FAULT_NONE;
    Some(request)
}

/// Report the outcome of an executed fault back to the host.
pub fn set_fault_result(result: u32) {
    area().fault_result = result;
}

/// [`RecoveryBackend`] over the handshake area's region.
pub struct EmuRecoveryBackend;

impl RecoveryBackend for EmuRecoveryBackend {
    fn region(&mut self) -> &mut RecoveryRegion {
        &mut area().region
    }

    fn request_reset(&mut self) {
        lp_riscv_emu_guest::reset_request_exit()
    }
}

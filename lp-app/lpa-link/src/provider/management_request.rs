use serde::{Deserialize, Serialize};

use crate::LinkOperation;

/// Provider-neutral request for a low-level link management operation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkManagementRequest {
    /// Reset or reboot the endpoint/runtime without erasing user data.
    ResetRuntime,
    /// Flash a firmware image.
    ///
    /// `build_id` names a firmware build def (`lp-fw/builds/<id>.json`),
    /// which is also the directory the packaged image ships in —
    /// `firmware/<id>/manifest.json`. `None` means "the provider's
    /// deployment default", which is a real answer for an endpoint whose
    /// chip is not known yet; the provider's flash-time chip guard refuses
    /// the default if it turns out to be the wrong ISA.
    ///
    /// Which build a device *should* get is app policy, not provider
    /// policy: `lpa_boards::provisioning_build_id` computes it from the
    /// chip (necessary) and the picked board (refinement).
    FlashFirmware { build_id: Option<String> },
    /// Erase device flash so the endpoint returns to a blank state.
    EraseDeviceFlash,
    /// Erase the raw device filesystem partition below the running server.
    EraseRawFilesystem,
    /// Read the raw device filesystem partition back to the host, verbatim.
    ///
    /// Takes no region: the partition is per board, and the board is
    /// **discovered** by the SYNC handshake the provider performs anyway (see
    /// [`crate::LinkFlashRegion`]). A device that cannot boot cannot be asked
    /// what it is, which is exactly when this operation matters.
    ///
    /// Works from ROM download mode, so it is the one way to get a user's
    /// work off a board whose own project prevents it from running.
    ReadRawFilesystem,
    /// Write the boot-control sector, instructing the device's next boot.
    ///
    /// `flags` are `lp_bootctl::BootFlags` bits, carried as a plain `u32` so
    /// this wire type stays independent of the on-flash format crate;
    /// providers convert with `BootFlags::from_bits` at the point of
    /// encoding. Prefer [`Self::boot_safe_once`] over assembling bits by hand.
    ///
    /// This is a request to the *next* boot, not an immediate effect. The
    /// device applies it when it restarts and consumes it as it does, so the
    /// instruction is one-shot.
    SetBootControl { flags: u32 },
}

impl LinkManagementRequest {
    /// Ask the device to start once in **safe mode**.
    ///
    /// The recovery escape for a project that prevents its own device from
    /// running — too bright, too power-hungry, or hanging the watchdog.
    ///
    /// Sets BOTH the skip-autoload bit and a dim output-clamp level, and the
    /// format's precedence rule (see [`lp_bootctl::BootFlags`]) picks the
    /// best behavior the firmware can deliver: clamp-aware firmware loads
    /// the project dimmed; older firmware ignores the clamp bits and comes
    /// up with nothing loaded. Either way the board is reachable and cannot
    /// brown itself out, and the boot after that is normal again.
    pub fn start_safe_mode() -> Self {
        Self::SetBootControl {
            flags: lp_bootctl::BootFlags::SKIP_PROJECT_AUTOLOAD
                .with_safe_clamp(lp_bootctl::BootFlags::DEFAULT_SAFE_CLAMP)
                .bits(),
        }
    }

    pub fn operation(&self) -> LinkOperation {
        match self {
            Self::ResetRuntime => LinkOperation::Reset,
            Self::FlashFirmware { .. } => LinkOperation::FlashFirmware,
            Self::EraseDeviceFlash => LinkOperation::EraseDeviceFlash,
            Self::EraseRawFilesystem => LinkOperation::WriteRawFilesystem,
            Self::ReadRawFilesystem => LinkOperation::ReadRawFilesystem,
            Self::SetBootControl { .. } => LinkOperation::WriteBootControl,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_safe_mode_sets_the_skip_bit_and_a_dim_clamp() {
        let LinkManagementRequest::SetBootControl { flags } =
            LinkManagementRequest::start_safe_mode()
        else {
            panic!("expected SetBootControl");
        };
        let decoded = lp_bootctl::BootFlags::from_bits(flags);
        // Both halves of the precedence design: the skip for firmware that
        // predates the clamp, the clamp for firmware that has it.
        assert!(decoded.contains(lp_bootctl::BootFlags::SKIP_PROJECT_AUTOLOAD));
        assert_eq!(
            decoded.safe_clamp(),
            Some(lp_bootctl::BootFlags::DEFAULT_SAFE_CLAMP)
        );
        assert!(
            !decoded.has_unknown_bits(),
            "the convenience constructor must not set reserved bits"
        );
    }

    #[test]
    fn set_boot_control_maps_to_the_write_operation() {
        assert_eq!(
            LinkManagementRequest::start_safe_mode().operation(),
            LinkOperation::WriteBootControl
        );
    }
}

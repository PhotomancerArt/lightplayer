use serde::{Deserialize, Serialize};

use crate::LinkOperation;

/// Provider-neutral request for a low-level link management operation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkManagementRequest {
    /// Reset or reboot the endpoint/runtime without erasing user data.
    ResetRuntime,
    /// Flash the provider's configured firmware image.
    FlashFirmware,
    /// Erase device flash so the endpoint returns to a blank state.
    EraseDeviceFlash,
    /// Erase the raw device filesystem partition below the running server.
    EraseRawFilesystem,
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
    /// Ask the device to come up once **without loading a project**.
    ///
    /// The recovery escape for a project that prevents its own device from
    /// running — too bright, too power-hungry, or hanging the watchdog. The
    /// device comes up reachable with nothing loaded, so the project can be
    /// fixed or replaced over the link; the boot after that is normal again.
    pub fn boot_safe_once() -> Self {
        Self::SetBootControl {
            flags: lp_bootctl::BootFlags::SKIP_PROJECT_AUTOLOAD.bits(),
        }
    }

    pub fn operation(&self) -> LinkOperation {
        match self {
            Self::ResetRuntime => LinkOperation::Reset,
            Self::FlashFirmware => LinkOperation::FlashFirmware,
            Self::EraseDeviceFlash => LinkOperation::EraseDeviceFlash,
            Self::EraseRawFilesystem => LinkOperation::WriteRawFilesystem,
            Self::SetBootControl { .. } => LinkOperation::WriteBootControl,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_safe_once_sets_only_the_skip_autoload_bit() {
        let LinkManagementRequest::SetBootControl { flags } =
            LinkManagementRequest::boot_safe_once()
        else {
            panic!("expected SetBootControl");
        };
        let decoded = lp_bootctl::BootFlags::from_bits(flags);
        assert!(decoded.contains(lp_bootctl::BootFlags::SKIP_PROJECT_AUTOLOAD));
        assert!(
            !decoded.has_unknown_bits(),
            "the convenience constructor must not set reserved bits"
        );
    }

    #[test]
    fn set_boot_control_maps_to_the_write_operation() {
        assert_eq!(
            LinkManagementRequest::boot_safe_once().operation(),
            LinkOperation::WriteBootControl
        );
    }
}

//! Inputs to the access controller, riding the studio actor's queue.
//!
//! Gestures (the password sheet, the device access panel, the project's
//! Bluetooth list, Settings' "Forget") and the results of the conversations
//! the controller spawned. Passwords ride some of them, so `Debug` is written
//! by hand and never prints one.

use lpa_devices::identity::DeviceId;
use lpc_access::{DeviceAccessFile, Tier};

use super::access_session::{LoginWindow, TypedPassword};
use super::device_access_record::{DeviceAccessChange, NewSecret};
use super::login_attempt::LoginAttemptOutcome;

#[derive(Clone)]
pub enum AccessCommand {
    /// The browser's stored documents, read at boot (either may be absent).
    MemoryLoaded {
        passwords_json: Option<String>,
        devices_json: Option<String>,
    },
    /// The sheet's Log in: try this password on the device's link.
    SubmitPassword {
        device: DeviceId,
        password: String,
        remember: bool,
    },
    /// The sheet's Not now.
    Dismiss { device: DeviceId },
    /// The card's "Log in" / "Log in for edit": open the sheet.
    LogIn { device: DeviceId },
    /// Settings' "Forget remembered passwords".
    ForgetRememberedPasswords,
    /// A change to the device's store, written over its link (USB, or an
    /// edit-tier Bluetooth login).
    Change {
        device: DeviceId,
        change: DeviceAccessChange,
    },
    /// "Restart now" after turning Bluetooth on or off.
    Restart { device: DeviceId },
    /// Add (or replace, by label) a password in the open project's list.
    ProjectSecretAdd(NewSecret),
    /// Remove one from it.
    ProjectSecretRevoke { label: String },

    // --- results of spawned conversations --------------------------------
    /// The link's hello answered: does it log in, and what does it hold.
    Checked {
        device: DeviceId,
        window: LoginWindow,
        result: Result<(bool, Option<Tier>), String>,
    },
    /// A login conversation ended.
    LoggedIn {
        device: DeviceId,
        window: LoginWindow,
        outcome: LoginAttemptOutcome,
        passwords: Vec<String>,
        typed: Option<TypedPassword>,
    },
    /// A device-store write ended.
    Written {
        device: DeviceId,
        key: String,
        result: Result<DeviceAccessFile, String>,
    },
}

impl core::fmt::Debug for AccessCommand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MemoryLoaded { .. } => f.write_str("MemoryLoaded(..)"),
            Self::SubmitPassword {
                device, remember, ..
            } => f
                .debug_struct("SubmitPassword")
                .field("device", device)
                .field("password", &"<redacted>")
                .field("remember", remember)
                .finish(),
            Self::Dismiss { device } => f.debug_struct("Dismiss").field("device", device).finish(),
            Self::LogIn { device } => f.debug_struct("LogIn").field("device", device).finish(),
            Self::ForgetRememberedPasswords => f.write_str("ForgetRememberedPasswords"),
            Self::Change { device, change } => f
                .debug_struct("Change")
                .field("device", device)
                .field("change", change)
                .finish(),
            Self::Restart { device } => f.debug_struct("Restart").field("device", device).finish(),
            Self::ProjectSecretAdd(secret) => {
                f.debug_tuple("ProjectSecretAdd").field(secret).finish()
            }
            Self::ProjectSecretRevoke { label } => f
                .debug_struct("ProjectSecretRevoke")
                .field("label", label)
                .finish(),
            Self::Checked {
                device,
                window,
                result,
            } => f
                .debug_struct("Checked")
                .field("device", device)
                .field("window", window)
                .field("result", result)
                .finish(),
            Self::LoggedIn {
                device,
                window,
                outcome,
                passwords,
                typed,
            } => f
                .debug_struct("LoggedIn")
                .field("device", device)
                .field("window", window)
                .field("outcome", outcome)
                .field("passwords", &passwords.len())
                .field("typed", typed)
                .finish(),
            Self::Written {
                device,
                key,
                result,
            } => f
                .debug_struct("Written")
                .field("device", device)
                .field("key", key)
                .field("ok", &result.is_ok())
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_a_password() {
        let command = AccessCommand::SubmitPassword {
            device: DeviceId(1),
            password: "hunter2".to_string(),
            remember: true,
        };
        assert!(!format!("{command:?}").contains("hunter2"));
        let command = AccessCommand::ProjectSecretAdd(NewSecret {
            label: "camp".to_string(),
            tier: Tier::Play,
            password: "hunter2".to_string(),
        });
        assert!(!format!("{command:?}").contains("hunter2"));
    }
}

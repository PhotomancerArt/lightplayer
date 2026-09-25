//! Inputs to the access controller, riding the studio actor's queue.
//!
//! Gestures (the Unlock sheet, the access panel, Undo, the project's
//! Bluetooth list, Settings' "Forget"), what the web edge knows and core
//! does not (the account's keys, a name for this browser), and the results
//! of the conversations the controller spawned. Passwords and keys ride some
//! of them, so `Debug` is written by hand and never prints one.

use lpa_devices::identity::DeviceId;
use lpc_access::Tier;

use super::access_session::{LoginWindow, TypedPassword};
use super::account_keys::AccountKeys;
use super::device_access_ops::{AccessListing, AccessSynced};
use super::device_access_record::{DeviceAccessChange, NewSecret};
use super::login_attempt::LoginAttemptOutcome;

#[derive(Clone)]
pub enum AccessCommand {
    /// The browser's stored documents, read at boot (any may be absent).
    /// With no `browser_json`, this browser's key is minted now.
    MemoryLoaded {
        passwords_json: Option<String>,
        devices_json: Option<String>,
        browser_json: Option<String>,
        account_json: Option<String>,
    },
    /// The web edge's default name for this browser's key (`<given name>'s
    /// <platform>` when signed in, else `<Browser> on <platform>`). Applies
    /// until the user renames it; send it again when sign-in changes it.
    BrowserNameDefault(String),
    /// The user renamed this browser's key. Devices re-label it on their
    /// next USB connect.
    RenameBrowser(String),
    /// The signed-in account's keys (after sign-in, or `GetAccountAccess`),
    /// or `None` on sign-out.
    AccountKeys(Option<AccountKeys>),
    /// The sheet's Unlock: try this password on the device's link.
    SubmitPassword {
        device: DeviceId,
        password: String,
        remember: bool,
    },
    /// The sheet's Not now.
    Dismiss { device: DeviceId },
    /// The card's "Unlock" / "Unlock for edit": open the sheet.
    LogIn { device: DeviceId },
    /// Settings' "Forget remembered passwords".
    ForgetRememberedPasswords,
    /// A change to the device's access list, over its link (USB, or a
    /// Bluetooth unlock at edit).
    Change {
        device: DeviceId,
        change: DeviceAccessChange,
    },
    /// The toast's Undo: remove exactly what the last USB connect added to
    /// `device`.
    UndoAutoAdd { device: DeviceId },
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
    /// A connect's sync ended: the device's list, and what was added.
    Synced {
        device: DeviceId,
        window: LoginWindow,
        result: Result<AccessSynced, String>,
    },
    /// A change (or an Undo) ended. `bluetooth` is the Bluetooth switch it
    /// set, if it set one.
    Changed {
        device: DeviceId,
        result: Result<AccessListing, String>,
        bluetooth: Option<bool>,
    },
}

impl core::fmt::Debug for AccessCommand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MemoryLoaded { .. } => f.write_str("MemoryLoaded(..)"),
            Self::BrowserNameDefault(name) => {
                f.debug_tuple("BrowserNameDefault").field(name).finish()
            }
            Self::RenameBrowser(name) => f.debug_tuple("RenameBrowser").field(name).finish(),
            Self::AccountKeys(keys) => f.debug_tuple("AccountKeys").field(keys).finish(),
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
            Self::UndoAutoAdd { device } => f
                .debug_struct("UndoAutoAdd")
                .field("device", device)
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
            Self::Synced {
                device,
                window,
                result,
            } => f
                .debug_struct("Synced")
                .field("device", device)
                .field("window", window)
                .field("result", result)
                .finish(),
            Self::Changed {
                device,
                result,
                bluetooth,
            } => f
                .debug_struct("Changed")
                .field("device", device)
                .field("ok", &result.is_ok())
                .field("bluetooth", bluetooth)
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
        let command = AccessCommand::Change {
            device: DeviceId(1),
            change: DeviceAccessChange::AddPassword {
                label: "friends".to_string(),
                tier: Tier::Play,
                password: "hunter2".to_string(),
            },
        };
        assert!(!format!("{command:?}").contains("hunter2"));
        let mut keys = super::super::account_keys::tests::account(Some("hunter2"));
        keys.edit_password = Some("hunter3".to_string());
        let command = AccessCommand::AccountKeys(Some(keys));
        assert!(!format!("{command:?}").contains("hunter"));
    }
}

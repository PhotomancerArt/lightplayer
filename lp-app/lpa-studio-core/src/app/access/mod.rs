//! Access over Bluetooth, Studio's side (BLE M6): logging in, remembering
//! passwords, and writing who may reach a piece.
//!
//! The board enforces tiers (`lpc-access`, `lpa-server`'s classifier, M4's
//! BLE link); Studio's job is to make logging in effortless and refusals
//! legible:
//!
//! | concept | file |
//! |---|---|
//! | login on connect, per device, pure | [`access_session`] |
//! | one login conversation over a link | [`login_attempt`] |
//! | the client-side KDF and its session cache | [`login_key_cache`] |
//! | passwords this browser remembers | [`remembered_passwords`] |
//! | what this browser wrote to each device store | [`device_access_record`] |
//! | the project sidecar (`<project>/.lp/access.json`) | [`project_access`] |
//! | the controller that runs all of it | [`access_controller`] |
//! | its inputs, and what the UI reads | [`access_command`], [`ui_access_view`] |
//!
//! Decision records: `docs/adr/2026-09-23-ble-access-model.md` (the model),
//! `docs/adr/2026-09-24-ble-transport-studio.md` (the Studio transport and
//! this UX).

pub mod access_command;
pub mod access_controller;
pub mod access_session;
pub mod device_access_record;
pub mod login_attempt;
pub mod login_key_cache;
pub mod project_access;
pub mod remembered_passwords;
pub mod ui_access_view;

#[cfg(test)]
pub(crate) mod test_board;

pub use access_command::AccessCommand;
pub use access_controller::{AccessController, AccessPersist};
pub use access_session::{
    AUTO_LOGIN_ATTEMPTS, AccessPhase, AccessSession, AccessStep, LoginWindow, PromptReason,
    TypedPassword,
};
pub use device_access_record::{
    DeviceAccessChange, DeviceAccessRecord, DeviceAccessRecords, NewSecret,
};
pub use login_attempt::{LoginAttemptOutcome, try_passwords};
pub use login_key_cache::{DEFAULT_KDF_ITERATIONS, LoginKeyCache};
pub use remembered_passwords::{MAX_REMEMBERED_PASSWORDS, RememberedPasswords};
pub use ui_access_view::{
    UiAccessPanel, UiAccessSecret, UiDeviceAccess, UiLoginPrompt, UiProjectAccess,
};

/// The access tier, as the UI names it.
pub use lpc_access::Tier as AccessTier;

/// What an action refused for want of a tier says. One sentence, used by the
/// action error, the card's push outcome and the sheet.
pub fn not_permitted_sentence(needs: lpc_access::Tier) -> &'static str {
    match needs {
        lpc_access::Tier::Edit => "This needs an edit password — log in again with one.",
        lpc_access::Tier::Play => "This needs a password — log in first.",
    }
}

/// A tier's word in the UI ("play", "edit").
pub fn tier_word(tier: lpc_access::Tier) -> &'static str {
    match tier {
        lpc_access::Tier::Play => "play",
        lpc_access::Tier::Edit => "edit",
    }
}

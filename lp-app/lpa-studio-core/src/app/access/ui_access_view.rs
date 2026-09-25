//! What the UI reads about access: a device's login line and its access
//! panel, the password sheet, and the open project's Bluetooth list.
//!
//! Every sentence is decided here, once, so the card, the sheet and the
//! stories say the same thing.

use lpa_devices::identity::DeviceId;
use lpc_access::Tier;

/// One device's access facts, joined onto its card.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiDeviceAccess {
    /// Reached over Bluetooth right now.
    pub over_bluetooth: bool,
    /// The login line ("Unlocked by Yona's MacBook"), for a Bluetooth link.
    pub line: Option<String>,
    /// What the card offers to unlock with, over Bluetooth: the sheet
    /// ("Unlock", nothing granted) or a way to edit (unlocked for play).
    pub unlock: Option<UiUnlockOffer>,
    /// The device access panel, when this link may write the device store.
    pub panel: Option<UiAccessPanel>,
}

/// What a Bluetooth card offers when its unlock is not the whole story.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiUnlockOffer {
    /// Nothing Studio holds unlocked it: "Unlock" opens the sheet.
    Locked,
    /// Unlocked for play only: "Editing needs an edit password, or plug it
    /// in by USB", with "Enter a password".
    PlayOnly,
}

/// The device access panel: "Who has access", read from the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiAccessPanel {
    pub device: DeviceId,
    /// Every entry on the device, in the board's order (the last listing;
    /// empty until the device has answered one).
    pub entries: Vec<UiAccessEntry>,
    /// The "Who has access · N" count: the entries, plus one for "Anyone
    /// nearby" when the device is open.
    pub count: usize,
    /// The device's STORED Bluetooth switch: `None` until it has answered a
    /// listing. A change applies at its next boot.
    pub ble_enabled: Option<bool>,
    /// "Anyone nearby can play".
    pub open: bool,
    /// Bluetooth was switched since the device last restarted.
    pub restart_pending: bool,
    /// "Restart now" works here (a USB link has the reset lines).
    pub can_restart: bool,
    /// Reached over Bluetooth: the Bluetooth switch is locked on ("turn off
    /// by USB").
    pub over_bluetooth: bool,
    /// A change is in flight.
    pub writing: bool,
    /// The last change's failure, in words.
    pub error: Option<String>,
}

/// One row of "Who has access".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiAccessEntry {
    pub label: String,
    pub kind: lpc_access::SecretKind,
    pub tier: Tier,
    /// The entry's identity on the board: what
    /// [`super::DeviceAccessChange::Remove`] names.
    pub salt_id: [u8; lpc_access::SALT_BYTES],
    /// This browser's own key.
    pub is_this_browser: bool,
    /// One of the signed-in account's entries (its key or a password).
    pub is_account: bool,
    /// When it was added (epoch seconds), when the adding client said.
    pub added_at: Option<u64>,
}

/// One password, as the panel and the project list show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiAccessSecret {
    pub label: String,
    pub tier: Tier,
}

/// The password sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiLoginPrompt {
    pub device: DeviceId,
    pub device_name: String,
    /// What happened, in one sentence.
    pub reason: String,
    /// The board's backoff, when a refusal set one.
    pub retry_after_ms: Option<u64>,
    /// A login is running (the button reads "Unlocking…").
    pub busy: bool,
}

/// The open project's Bluetooth list (the sidecar).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiProjectAccess {
    pub secrets: Vec<UiAccessSecret>,
    /// The last change's failure, in words.
    pub error: Option<String>,
}

/// The sheet's sentence for why it is open.
pub fn prompt_sentence(reason: &super::PromptReason, device_name: &str) -> String {
    use super::PromptReason;
    match reason {
        PromptReason::NoPasswordKnown => "This device needs a password to unlock it.".to_string(),
        PromptReason::Refused { retry_after_ms } => match *retry_after_ms {
            0 => format!("That device password didn't unlock {device_name}."),
            ms => format!(
                "That device password didn't unlock {device_name}. It will listen again in {} s.",
                ms.div_ceil(1_000)
            ),
        },
        PromptReason::NeedsEdit => PLAY_ONLY_SENTENCE.to_string(),
        PromptReason::Asked => format!("Unlock {device_name} with another device password."),
    }
}

/// What a device unlocked for play says about editing — on the card and on
/// the sheet it opens.
pub const PLAY_ONLY_SENTENCE: &str = "Editing needs an edit password, or plug it in by USB.";

/// The card's login line for a Bluetooth link.
pub fn access_line(phase: &super::AccessPhase) -> Option<String> {
    use super::AccessPhase;
    Some(match phase {
        AccessPhase::Unknown | AccessPhase::Checking => "Connecting over Bluetooth…".to_string(),
        AccessPhase::LoggingIn => "Unlocking…".to_string(),
        AccessPhase::Granted {
            tier: Tier::Edit,
            label: Some(label),
        } => format!("Unlocked by {label}"),
        AccessPhase::Granted {
            tier: Tier::Play,
            label: Some(label),
        } => format!("Unlocked with {label} · play"),
        AccessPhase::Granted {
            tier: Tier::Play,
            label: None,
        } => "Open — play, no password".to_string(),
        AccessPhase::Granted {
            tier: Tier::Edit,
            label: None,
        } => "Connected — edit".to_string(),
        AccessPhase::Locked => "Needs a device password".to_string(),
        AccessPhase::Unreachable => {
            "Bluetooth has no device password here — connect by USB to set one".to_string()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::super::{AccessPhase, PromptReason};
    use super::*;

    #[test]
    fn the_login_line_names_the_label_and_a_play_tier() {
        assert_eq!(
            access_line(&AccessPhase::Granted {
                tier: Tier::Play,
                label: Some("friends".to_string())
            })
            .as_deref(),
            Some("Unlocked with friends · play")
        );
        assert_eq!(
            access_line(&AccessPhase::Granted {
                tier: Tier::Edit,
                label: Some("Yona's MacBook".to_string())
            })
            .as_deref(),
            Some("Unlocked by Yona's MacBook")
        );
        assert_eq!(
            access_line(&AccessPhase::Granted {
                tier: Tier::Play,
                label: None
            })
            .as_deref(),
            Some("Open — play, no password")
        );
    }

    #[test]
    fn a_refusal_says_when_the_piece_listens_again() {
        let sentence = prompt_sentence(
            &PromptReason::Refused {
                retry_after_ms: 3_500,
            },
            "Choker",
        );
        assert!(sentence.contains("in 4 s"), "{sentence}");
        assert!(!sentence.to_lowercase().contains("failed"));
        assert_eq!(
            prompt_sentence(&PromptReason::NeedsEdit, "Choker"),
            "Editing needs an edit password, or plug it in by USB."
        );
        assert_eq!(
            prompt_sentence(&PromptReason::NoPasswordKnown, "Choker"),
            "This device needs a password to unlock it."
        );
    }

    /// G3 / AC8: the device's door speaks of the device and its password,
    /// never "log in" or the account — those are the cloud account's words,
    /// and its password must not be typed here.
    #[test]
    fn the_sheet_asks_for_a_device_password_never_a_login() {
        for reason in [
            PromptReason::NoPasswordKnown,
            PromptReason::NeedsEdit,
            PromptReason::Asked,
            PromptReason::Refused { retry_after_ms: 0 },
        ] {
            let sentence = prompt_sentence(&reason, "Choker");
            let lower = sentence.to_lowercase();
            assert!(lower.contains("password"), "{sentence}");
            assert!(
                lower.contains("device") || lower.contains("choker") || lower.contains("usb"),
                "{sentence}"
            );
            assert!(!lower.contains("log"), "{sentence}");
            assert!(!lower.contains("account"), "{sentence}");
            assert!(!lower.contains("piece"), "{sentence}");
        }
        for tier in [Tier::Play, Tier::Edit] {
            let sentence = super::super::not_permitted_sentence(tier);
            assert!(sentence.contains("device password"), "{sentence}");
            assert!(!sentence.to_lowercase().contains("log in"), "{sentence}");
        }
    }
}

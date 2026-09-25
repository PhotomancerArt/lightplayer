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
    /// The login line ("Unlocked as camp — play"), for a Bluetooth link.
    pub line: Option<String>,
    /// Offer "Unlock" (nothing granted) or "Unlock for edit" (play).
    pub log_in: Option<String>,
    /// The device access panel, when this link may write the device store.
    pub panel: Option<UiAccessPanel>,
}

/// The device access panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiAccessPanel {
    pub device: DeviceId,
    /// What this browser last wrote: `Some(on)`, or `None` when it never
    /// wrote this device's store (Bluetooth may be anything, most likely
    /// off — the board's default).
    pub ble_enabled: Option<bool>,
    pub open: bool,
    /// The passwords this browser wrote, by label and tier.
    pub secrets: Vec<UiAccessSecret>,
    /// Bluetooth was switched since the board last restarted.
    pub restart_pending: bool,
    /// "Restart now" works here (a USB link has the reset lines).
    pub can_restart: bool,
    /// A write is in flight.
    pub writing: bool,
    /// The last write's failure, in words.
    pub error: Option<String>,
    /// The account default, to pre-fill the password field.
    pub default_password: Option<String>,
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
        PromptReason::NoPasswordKnown => format!("{device_name} asks for its device password."),
        PromptReason::Refused { retry_after_ms } => match *retry_after_ms {
            0 => format!("That device password didn't unlock {device_name}."),
            ms => format!(
                "That device password didn't unlock {device_name}. It will listen again in {} s.",
                ms.div_ceil(1_000)
            ),
        },
        PromptReason::NeedsEdit => {
            format!("This needs an edit device password. {device_name} is unlocked for play only.")
        }
        PromptReason::Asked => format!("Unlock {device_name} with another device password."),
    }
}

/// The card's login line for a Bluetooth link.
pub fn access_line(phase: &super::AccessPhase) -> Option<String> {
    use super::AccessPhase;
    Some(match phase {
        AccessPhase::Unknown | AccessPhase::Checking => "Connecting over Bluetooth…".to_string(),
        AccessPhase::LoggingIn => "Unlocking…".to_string(),
        AccessPhase::Granted {
            tier,
            label: Some(label),
        } => format!("Unlocked as {label} — {}", super::tier_word(*tier)),
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
    fn the_login_line_names_the_label_and_the_tier() {
        assert_eq!(
            access_line(&AccessPhase::Granted {
                tier: Tier::Play,
                label: Some("camp".to_string())
            })
            .as_deref(),
            Some("Unlocked as camp — play")
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
        assert!(
            prompt_sentence(&PromptReason::NeedsEdit, "Choker").contains("edit device password")
        );
    }

    /// G3: the device's door says "Unlock" and "device password", never
    /// "log in" — that is the cloud account's word, and its password must
    /// not be typed here.
    #[test]
    fn the_sheet_asks_for_a_device_password_never_a_login() {
        for reason in [
            PromptReason::NoPasswordKnown,
            PromptReason::NeedsEdit,
            PromptReason::Asked,
            PromptReason::Refused { retry_after_ms: 0 },
        ] {
            let sentence = prompt_sentence(&reason, "Choker");
            assert!(sentence.contains("device password"), "{sentence}");
            assert!(!sentence.to_lowercase().contains("log"), "{sentence}");
        }
        for tier in [Tier::Play, Tier::Edit] {
            let sentence = super::super::not_permitted_sentence(tier);
            assert!(sentence.contains("device password"), "{sentence}");
            assert!(!sentence.to_lowercase().contains("log in"), "{sentence}");
        }
    }
}

//! What the UI reads about access: a device's login line and its access
//! panel, the password sheet, and the open project's Bluetooth list.
//!
//! Every sentence is decided here, once, so the card, the sheet and the
//! stories say the same thing.

use lpa_devices::identity::DeviceId;
use lpc_access::{OpenTo, SecretKind, Tier};

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
    /// The last USB connect could not add the signed-in account's key, so
    /// the board cannot be reached through lightplayer.app: the sentence
    /// saying so, and why ([`account_key_refused_sentence`]). Shown on the
    /// card, not only inside the Access panel.
    pub account_key_refused: Option<String>,
}

/// What the card says when the account's key could not be added over USB,
/// followed by the reason (`why`: the room rule's
/// [`super::device_access_ops::FULL_SENTENCE`], or the board's own refusal).
pub fn account_key_refused_sentence(why: &str) -> String {
    format!(
        "Your account's key couldn't be added, so this board can't be reached through \
         lightplayer.app. {why}"
    )
}

/// What a Bluetooth card offers when its unlock is not the whole story.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiUnlockOffer {
    /// Nothing Studio holds unlocked it: "Unlock" opens the sheet.
    Locked,
    /// Unlocked for play only: [`PLAY_ONLY_SENTENCE`], with "Enter a
    /// password".
    PlayOnly,
}

/// The device access panel, read from the board: Play
/// and Author, then the keys that always get in, folded into one line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiAccessPanel {
    pub device: DeviceId,
    /// Who nearby gets in with no password (nobody until a listing).
    pub open: OpenTo,
    /// The Play line.
    pub play: UiPasswordLine,
    /// The Author line (the edit tier).
    pub author: UiPasswordLine,
    /// "Your browsers & account": every other entry, grouped — this
    /// browser first, then other browsers by name, then the account's.
    pub keys: Vec<UiKeyGroup>,
    /// Entries on the device, of [`Self::capacity`].
    pub used: usize,
    pub capacity: usize,
    /// The device's STORED Bluetooth switch: `None` until it has answered a
    /// listing. A change applies at its next boot.
    pub ble_enabled: Option<bool>,
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
    /// What the last change did on its own ("Author is open now, so play is
    /// too.", a key dropped to make room).
    pub notice: Option<String>,
}

impl UiAccessPanel {
    /// A panel before the device has answered its list: nothing known.
    pub fn reading(device: DeviceId) -> Self {
        Self {
            device,
            open: OpenTo::Nobody,
            play: UiPasswordLine::NotSet,
            author: UiPasswordLine::NotSet,
            keys: Vec::new(),
            used: 0,
            capacity: lpc_access::MAX_SECRETS_PER_FILE,
            ble_enabled: None,
            restart_pending: false,
            can_restart: true,
            over_bluetooth: false,
            writing: false,
            error: None,
            notice: None,
        }
    }
}

/// One line of the panel, Play or Author.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiPasswordLine {
    /// Anyone nearby, no password.
    Anyone,
    /// Play while Author is Anyone: it follows Author.
    FollowsAuthor,
    /// A password this browser set, shown so it can be told or typed over.
    Shown(String),
    /// A password set from another browser: it cannot be shown, only
    /// replaced.
    SetElsewhere,
    /// Password, but none is on the device: only the keys get in.
    NotSet,
}

impl UiPasswordLine {
    /// The line reads Anyone (its own, or following Author).
    pub fn is_anyone(&self) -> bool {
        matches!(self, Self::Anyone | Self::FollowsAuthor)
    }
}

/// One row of "Your browsers & account": entries that share a name and a
/// kind, folded ("Brave on Mac ×11").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiKeyGroup {
    pub label: String,
    pub kind: SecretKind,
    pub tier: Tier,
    /// Every entry in the group: what its trash can removes.
    pub salts: Vec<[u8; lpc_access::SALT_BYTES]>,
    /// This browser's own key (always a group of one).
    pub is_this_browser: bool,
    /// The signed-in account's key or one of its passwords.
    pub is_account: bool,
    /// The earliest and latest `addedAt` in the group, epoch seconds.
    pub first_added: Option<u64>,
    pub last_added: Option<u64>,
}

impl UiKeyGroup {
    pub fn count(&self) -> usize {
        self.salts.len()
    }
}

/// What a change or a sync says about the browser keys it dropped to make
/// room on a full device.
pub fn dropped_sentence(dropped: &[super::DroppedKey]) -> Option<String> {
    match dropped {
        [] => None,
        [one] => Some(format!("To make room, an older {} was dropped.", one.label)),
        many => Some(format!(
            "To make room, {} older browser keys were dropped.",
            many.len()
        )),
    }
}

/// What the card's access row says, decided once. "open" (warning-tinted
/// on the card) is the callout a new board needs.
pub fn open_summary(open: OpenTo) -> &'static str {
    match open {
        OpenTo::Edit => "open",
        OpenTo::Play => "anyone can play",
        OpenTo::Nobody => "password",
    }
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

/// What a device unlocked for play says about authoring — on the card and
/// on the sheet it opens.
pub const PLAY_ONLY_SENTENCE: &str = "Authoring needs an author password, or plug it in by USB.";

/// The card's login line for a link that must be unlocked — Bluetooth, the
/// LAN, or lightplayer.app's relay (`link`, which the words name).
pub fn access_line(phase: &super::AccessPhase, link: crate::UiLinkKind) -> Option<String> {
    use super::AccessPhase;
    Some(match phase {
        AccessPhase::Unknown | AccessPhase::Checking => {
            format!("Connecting over {}…", link.label())
        }
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
        // Unlocked at edit with no name to give (a trusted link, or a
        // re-check that found the link already unlocked): the same word as
        // every other unlocked state, never a second label for it.
        AccessPhase::Granted {
            tier: Tier::Edit,
            label: None,
        } => "Unlocked".to_string(),
        AccessPhase::Locked => "Needs a device password".to_string(),
        AccessPhase::Unreachable => format!(
            "{} has no device password here — connect by USB to set one",
            link.label()
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::super::{AccessPhase, PromptReason};
    use super::*;

    #[test]
    fn the_login_line_names_the_label_and_a_play_tier() {
        assert_eq!(
            access_line(
                &AccessPhase::Granted {
                    tier: Tier::Play,
                    label: Some("friends".to_string())
                },
                crate::UiLinkKind::Bluetooth
            )
            .as_deref(),
            Some("Unlocked with friends · play")
        );
        assert_eq!(
            access_line(
                &AccessPhase::Granted {
                    tier: Tier::Edit,
                    label: Some("Yona's MacBook".to_string())
                },
                crate::UiLinkKind::Bluetooth
            )
            .as_deref(),
            Some("Unlocked by Yona's MacBook")
        );
        assert_eq!(
            access_line(
                &AccessPhase::Granted {
                    tier: Tier::Play,
                    label: None
                },
                crate::UiLinkKind::Bluetooth
            )
            .as_deref(),
            Some("Open — play, no password")
        );
        assert_eq!(
            access_line(
                &AccessPhase::Granted {
                    tier: Tier::Edit,
                    label: None
                },
                crate::UiLinkKind::Bluetooth
            )
            .as_deref(),
            Some("Unlocked"),
            "one word for unlocked, with or without a name"
        );
    }

    /// The line names the link it is about: a board through lightplayer.app
    /// never says "Connecting over Bluetooth…" (PR C).
    #[test]
    fn the_login_line_names_the_link() {
        for (link, words) in [
            (crate::UiLinkKind::Bluetooth, "Connecting over Bluetooth…"),
            (crate::UiLinkKind::Wifi, "Connecting over Wi\u{2011}Fi…"),
            (
                crate::UiLinkKind::Relay,
                "Connecting over Wi\u{2011}Fi via lightplayer.app…",
            ),
        ] {
            assert_eq!(
                access_line(&AccessPhase::Checking, link).as_deref(),
                Some(words)
            );
        }
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
            "Authoring needs an author password, or plug it in by USB."
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

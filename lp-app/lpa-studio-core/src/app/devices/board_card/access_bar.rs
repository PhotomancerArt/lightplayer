//! The access bar: what you can do on this board, and with which key.
//!
//! First match wins:
//!
//! | Board | Summary | Aside | Tone | Action |
//! |---|---|---|---|---|
//! | Offline | "Not known yet" | "when it's back" | Neutral | — |
//! | A trusted link (USB, a stand-in) | "You can edit" | "USB" / "in this tab" | Neutral | — |
//! | Locked | "Locked" | "needs a password" | Neutral | — (Unlock is the primary) |
//! | Play only | "You can play" | the key, or "anyone can play" | Neutral | Unlock (the sheet) |
//! | Unlocked at edit | "You can edit" | the key | Neutral | — |
//! | Checking, unlocking | "Not known yet" (the work shows) | — | Neutral | — |
//! | No password reaches it | "No password here" | "set one over USB" | Warning | — |
//!
//! Over any row: a board open to edit with no password says "anyone can
//! edit", warning-tinted, as today's access row did (I50); and a board
//! whose account key could not be added (#1056) carries that as its
//! notice, in Attention.
//!
//! The bar reads the access facts' typed grant and state
//! ([`crate::UiAccessGrant`], [`crate::UiAccessWait`]), never the login
//! line's words, so it can never name the wrong link: nothing here says
//! "Bluetooth" on a LAN or relay link. Its icon is `lock` in every row; the
//! words say the tier.

use lpc_access::{OpenTo, Tier};

use super::board_card_input::BoardCardInput;
use super::detail_sections::{caution, facts, notice, verbs, without_empty};
use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::access::{
    PLAY_ONLY_SENTENCE, UiAccessPanel, UiAccessWait, UiPasswordLine, UiUnlockOffer, open_summary,
};
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::{RichLine, UiStatusKind};

/// The notice on a board anyone nearby may edit.
pub const ANYONE_CAN_EDIT_SENTENCE: &str = "Anyone nearby can change this board.";
/// The account-key notice's title, ahead of #1056's sentence.
const ACCOUNT_KEY_TITLE: &str = "Your account's key couldn't be added";

/// The access bar.
pub(crate) fn access_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let access = input.access;
    let grant = access.and_then(|access| access.grant.as_ref());
    let waiting = access.and_then(|access| access.waiting);
    let unlock = access.and_then(|access| access.unlock);
    let trusted = input.stand_in() || input.link_kind() == UiLinkKind::Usb;
    let mut work = None;
    let (summary, mut aside, mut tone, action) = if input.offline() {
        (
            "Not known yet",
            Some("when it's back".to_string()),
            UiStatusKind::Neutral,
            None,
        )
    } else if trusted {
        let via = match input.stand_in() {
            true => "in this tab",
            false => "USB",
        };
        (
            "You can edit",
            Some(via.to_string()),
            UiStatusKind::Neutral,
            None,
        )
    } else if unlock == Some(UiUnlockOffer::Locked) {
        (
            "Locked",
            Some("needs a password".to_string()),
            UiStatusKind::Neutral,
            None,
        )
    } else if unlock == Some(UiUnlockOffer::PlayOnly) {
        (
            "You can play",
            Some(
                grant
                    .and_then(|grant| grant.key.clone())
                    .unwrap_or_else(|| open_summary(OpenTo::Play).to_string()),
            ),
            UiStatusKind::Neutral,
            input.offer("unlock").map(|unlock| {
                UiCardAction::press(unlock, "Unlock")
                    .with_icon("lock")
                    .drawn(UiActionDraw::Sheet)
            }),
        )
    } else if let Some(grant) = grant.filter(|grant| grant.tier == Tier::Edit) {
        (
            "You can edit",
            grant.key.clone(),
            UiStatusKind::Neutral,
            None,
        )
    } else if waiting == Some(UiAccessWait::NoPassword) {
        (
            "No password here",
            Some("set one over USB".to_string()),
            UiStatusKind::Warning,
            None,
        )
    } else {
        if let Some(words) = match waiting {
            Some(UiAccessWait::Checking) => Some("Checking access…"),
            Some(UiAccessWait::Unlocking) => Some("Unlocking…"),
            _ => None,
        } {
            work = Some(UiBarWork {
                words: words.to_string(),
                percent: None,
                state: BarWorkState::Running,
                cancel: None,
                other_device: false,
            });
        }
        ("Not known yet", None, UiStatusKind::Neutral, None)
    };
    // Open to edit with no password: anyone nearby can change it. A grant
    // at edit with no key's name is that same board, before its list is
    // read.
    let open_to_edit = !input.offline()
        && (access
            .and_then(|access| access.panel.as_ref())
            .is_some_and(|panel| panel.open == OpenTo::Edit)
            || grant.is_some_and(|grant| grant.tier == Tier::Edit && grant.key.is_none()));
    if open_to_edit {
        aside = Some("anyone can edit".to_string());
        tone = UiStatusKind::Warning;
    }
    let refused = access
        .and_then(|access| access.account_key_refused.as_deref())
        .filter(|_| input.linked());
    if refused.is_some() {
        tone = UiStatusKind::Attention;
    }
    UiStackBar {
        layer: BarLayer::Access,
        icon: "lock".to_string(),
        summary: summary.to_string(),
        details: details(
            input,
            summary,
            aside.as_deref(),
            tone,
            open_to_edit,
            refused,
        ),
        aside,
        aside_icon: None,
        tone: if work.is_some() {
            UiStatusKind::Neutral
        } else {
            tone
        },
        action,
        work,
    }
}

/// A new board's access bar: nothing is known until it says who it is.
pub(crate) fn pending_access_bar() -> UiStackBar {
    UiStackBar {
        layer: BarLayer::Access,
        icon: "lock".to_string(),
        summary: "Not known yet".to_string(),
        aside: None,
        aside_icon: None,
        tone: UiStatusKind::Neutral,
        action: None,
        work: None,
        details: UiBarDetails::default(),
    }
}

/// The access bar's details: the notice, your access, this board's, the
/// play-only sentence with Unlock, and the access panel.
fn details(
    input: &BoardCardInput<'_>,
    summary: &str,
    aside: Option<&str>,
    tone: UiStatusKind,
    open_to_edit: bool,
    refused: Option<&str>,
) -> UiBarDetails {
    let access = input.access;
    let panel = access.and_then(|access| access.panel.as_ref());
    let mut sections = Vec::new();
    if let Some(refused) = refused {
        sections.push(notice(
            ACCOUNT_KEY_TITLE,
            UiStatusKind::Attention,
            refused,
            None,
        ));
    }
    if tone == UiStatusKind::Warning && !open_to_edit {
        sections.push(notice(
            "Access",
            tone,
            format!(
                "{} has no device password here — connect by USB to set one",
                input.link_kind().label()
            ),
            None,
        ));
    }
    if open_to_edit {
        // A tint, not a notice: a board open to edit is how most boards
        // start, and the status corner is for what needs you now.
        sections.push(caution(
            "Access",
            UiStatusKind::Warning,
            ANYONE_CAN_EDIT_SENTENCE,
        ));
    }
    let you_can = match summary {
        "You can edit" => "play and edit",
        "You can play" => "play",
        _ => "nothing yet",
    };
    let with = match (summary, aside) {
        ("You can edit" | "You can play", Some("anyone can edit" | "anyone can play")) => {
            "no password".to_string()
        }
        ("You can edit" | "You can play", Some("in this tab")) => "this tab".to_string(),
        ("You can edit" | "You can play", Some(with)) => with.to_string(),
        _ => "—".to_string(),
    };
    sections.push(facts(
        "Your access",
        vec![
            RichLine::new("You can", you_can),
            RichLine::new("With", with),
        ],
    ));
    if let Some(panel) = panel {
        sections.push(facts("This board", this_board(panel)));
    }
    if access.and_then(|access| access.unlock) == Some(UiUnlockOffer::PlayOnly) {
        let mut play_only = verbs(
            input
                .offer("unlock")
                .map(|unlock| {
                    UiCardAction::press(unlock, "Unlock")
                        .with_icon("lock")
                        .drawn(UiActionDraw::Sheet)
                })
                .into_iter()
                .collect(),
        );
        play_only.sentence = Some(PLAY_ONLY_SENTENCE.to_string());
        sections.push(play_only);
    }
    UiBarDetails {
        sections: without_empty(sections),
        panels: panel
            .cloned()
            .map(UiDetailPanel::Access)
            .into_iter()
            .collect(),
        raised: false,
    }
}

/// "This board": who plays and who edits, from the board's own list.
fn this_board(panel: &UiAccessPanel) -> Vec<RichLine> {
    vec![
        RichLine::new("Open to", open_summary(panel.open)),
        RichLine::new("Play", password_words(&panel.play)),
        RichLine::new("Edit", password_words(&panel.author)),
    ]
}

/// One password line, in a word: never the password itself.
fn password_words(line: &UiPasswordLine) -> &'static str {
    match line {
        UiPasswordLine::Anyone | UiPasswordLine::FollowsAuthor => "anyone",
        UiPasswordLine::Shown(_) | UiPasswordLine::SetElsewhere => "password",
        UiPasswordLine::NotSet => "keys only",
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::DeviceId;

    use super::super::card_fixtures::CardFixture;
    use super::*;
    use crate::{UiAccessGrant, UiDeviceAccess};

    #[test]
    fn an_offline_board_is_not_known_yet_until_its_back() {
        let bar = access_bar(&CardFixture::offline().input());
        assert_eq!(bar.summary, "Not known yet");
        assert_eq!(bar.aside.as_deref(), Some("when it's back"));
        assert_eq!(bar.icon, "lock");
    }

    #[test]
    fn a_trusted_link_can_edit_by_usb_or_in_this_tab() {
        let bar = access_bar(&CardFixture::ready().input());
        assert_eq!(
            (bar.summary.as_str(), bar.aside.as_deref()),
            ("You can edit", Some("USB"))
        );
        let mut sim = CardFixture::ready();
        sim.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        let bar = access_bar(&sim.input());
        assert_eq!(bar.aside.as_deref(), Some("in this tab"));
        assert_eq!(line(&bar, "With").as_deref(), Some("this tab"));
    }

    #[test]
    fn a_locked_board_needs_a_password() {
        let bar = access_bar(
            &CardFixture::ready()
                .over(UiLinkKind::Bluetooth)
                .locked()
                .input(),
        );
        assert_eq!(bar.summary, "Locked");
        assert_eq!(bar.aside.as_deref(), Some("needs a password"));
        assert_eq!(bar.action, None, "Unlock is the primary");
    }

    #[test]
    fn a_play_only_board_offers_unlock_with_the_sheet() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth);
        fixture.access = Some(UiDeviceAccess {
            unlock: Some(UiUnlockOffer::PlayOnly),
            grant: Some(UiAccessGrant {
                tier: Tier::Play,
                key: Some("friends".to_string()),
            }),
            ..UiDeviceAccess::default()
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "You can play");
        assert_eq!(bar.aside.as_deref(), Some("friends"));
        let unlock = bar.action.clone().expect("Unlock");
        assert_eq!(unlock.word, "Unlock");
        assert_eq!(unlock.icon.as_deref(), Some("lock"));
        assert_eq!(unlock.draw, UiActionDraw::Sheet);
        assert!(unlock.offer.to_string().ends_with("/unlock"));
        assert!(bar.details.sections.iter().any(|section| {
            section.sentence.as_deref() == Some(PLAY_ONLY_SENTENCE)
                && section.affordances.first().map(|a| a.word.as_str()) == Some("Unlock")
        }));

        // Open to play: no key, anyone can play.
        fixture.access.as_mut().unwrap().grant = Some(UiAccessGrant {
            tier: Tier::Play,
            key: None,
        });
        assert_eq!(
            access_bar(&fixture.input()).aside.as_deref(),
            Some("anyone can play")
        );
    }

    #[test]
    fn an_edit_grant_names_its_key_and_an_open_board_warns() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Wifi);
        fixture.access = Some(UiDeviceAccess {
            grant: Some(UiAccessGrant {
                tier: Tier::Edit,
                key: Some("Yona's MacBook".to_string()),
            }),
            ..UiDeviceAccess::default()
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "You can edit");
        assert_eq!(bar.aside.as_deref(), Some("Yona's MacBook"));
        assert_eq!(bar.tone, UiStatusKind::Neutral);

        fixture.access.as_mut().unwrap().grant = Some(UiAccessGrant {
            tier: Tier::Edit,
            key: None,
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.aside.as_deref(), Some("anyone can edit"));
        assert_eq!(bar.tone, UiStatusKind::Warning);
        assert!(
            bar.details.notice().is_none(),
            "a tint the corner does not report"
        );
        assert!(
            bar.details
                .sections
                .iter()
                .any(|section| { section.sentence.as_deref() == Some(ANYONE_CAN_EDIT_SENTENCE) })
        );
    }

    #[test]
    fn a_usb_board_open_to_edit_warns_and_shows_its_panel() {
        let mut fixture = CardFixture::ready();
        let mut panel = UiAccessPanel::reading(DeviceId(7));
        panel.open = OpenTo::Edit;
        panel.play = UiPasswordLine::FollowsAuthor;
        panel.author = UiPasswordLine::Anyone;
        fixture.access = Some(UiDeviceAccess {
            panel: Some(panel),
            ..UiDeviceAccess::default()
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "You can edit");
        assert_eq!(bar.aside.as_deref(), Some("anyone can edit"));
        assert_eq!(bar.tone, UiStatusKind::Warning);
        assert_eq!(line(&bar, "Open to").as_deref(), Some("open"));
        assert_eq!(line(&bar, "Edit").as_deref(), Some("anyone"));
        assert!(matches!(
            bar.details.panels.as_slice(),
            [UiDetailPanel::Access(_)]
        ));
    }

    #[test]
    fn checking_and_unlocking_are_the_bars_work() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Relay);
        fixture.access = Some(UiDeviceAccess {
            waiting: Some(UiAccessWait::Checking),
            ..UiDeviceAccess::default()
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(
            bar.work.map(|work| work.words).as_deref(),
            Some("Checking access…")
        );
        fixture.access.as_mut().unwrap().waiting = Some(UiAccessWait::Unlocking);
        let bar = access_bar(&fixture.input());
        assert_eq!(
            bar.work.map(|work| work.words).as_deref(),
            Some("Unlocking…")
        );
    }

    #[test]
    fn a_board_no_password_reaches_says_set_one_over_usb() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth);
        fixture.access = Some(UiDeviceAccess {
            waiting: Some(UiAccessWait::NoPassword),
            ..UiDeviceAccess::default()
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "No password here");
        assert_eq!(bar.aside.as_deref(), Some("set one over USB"));
        assert_eq!(bar.tone, UiStatusKind::Warning);
        assert!(bar.details.notice().is_some());
    }

    /// #1056: the account's key could not be added — said as the bar's
    /// notice, in Attention, with its sentence.
    #[test]
    fn a_refused_account_key_is_the_access_notice() {
        let mut fixture = CardFixture::ready();
        let sentence = crate::account_key_refused_sentence("The board is full.");
        fixture.access = Some(UiDeviceAccess {
            account_key_refused: Some(sentence.clone()),
            ..UiDeviceAccess::default()
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "You can edit");
        assert_eq!(bar.tone, UiStatusKind::Attention);
        let notice = bar.details.notice().expect("the notice");
        assert_eq!(notice.title, "Your account's key couldn't be added");
        assert_eq!(notice.sentence.as_deref(), Some(sentence.as_str()));
    }

    /// The access words over the LAN and the relay never say Bluetooth.
    #[test]
    fn the_access_words_over_lan_and_relay_never_say_bluetooth() {
        for link in [UiLinkKind::Wifi, UiLinkKind::Relay] {
            for waiting in [
                UiAccessWait::Checking,
                UiAccessWait::Unlocking,
                UiAccessWait::NoPassword,
            ] {
                let mut fixture = CardFixture::ready().over(link);
                fixture.access = Some(UiDeviceAccess {
                    waiting: Some(waiting),
                    ..UiDeviceAccess::default()
                });
                let words = format!("{:?}", access_bar(&fixture.input()));
                assert!(
                    !words.contains("Bluetooth"),
                    "{link:?} {waiting:?}: {words}"
                );
            }
            let words = format!(
                "{:?}",
                access_bar(&CardFixture::ready().over(link).locked().input())
            );
            assert!(!words.contains("Bluetooth"), "{words}");
        }
    }

    fn line(bar: &UiStackBar, label: &str) -> Option<String> {
        bar.details
            .sections
            .iter()
            .flat_map(|section| section.lines.iter())
            .find(|line| line.label == label)
            .map(|line| line.value.clone())
    }
}

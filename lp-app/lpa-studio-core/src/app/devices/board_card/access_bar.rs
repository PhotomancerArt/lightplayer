//! The Access row: what you can do on this board
//! (`docs/ux/anatomy/board-card.md`, "Access ○"). **Access · name ·
//! state · action**, the lock icon in every state; the name is the tier and
//! how you hold it.
//!
//! First match wins:
//!
//! | Board | Name | State | Tone | Action |
//! |---|---|---|---|---|
//! | A stand-in (a sim, an in-tab board), on or off | "edit" | — | Neutral | — |
//! | Offline | the tier last known ("edit"), dimmed | — | Neutral | — |
//! | A trusted link (USB) | "edit · USB" | — | Neutral | — |
//! | Locked | "locked" | — | Neutral | — (Unlock is the primary) |
//! | Play only | "play only" | — | Neutral | Unlock (the sheet) |
//! | Unlocked at edit with a key | "edit · password" | — | Neutral | — |
//! | Open to edit, with no password | "edit · no password" | — | Neutral | — |
//! | Checking | "—" | "checking", still | Neutral | — |
//! | Unlocking | "—" | (work: "unlocking") | Neutral | — |
//! | No password reaches it | "USB only · no password" | — | Attention | — (the fix is in the details) |
//!
//! Over any row: a board whose account key could not be added (#1056)
//! says "account key not added" and carries that as its notice, in
//! Attention.
//!
//! An open board is quiet (O4): open is how boards start, and yellow is an
//! unsaved edit's colour (P16). A board no password reaches over this link
//! needs you, orange; no core offer sets a board's password from here, so
//! the row carries no button and the details say what to do (USB) — never
//! a web-built action. The key's own name ("Yona's MacBook") is the
//! details'; the row says only how you hold the tier.
//!
//! The row reads the access facts' typed grant and state
//! ([`crate::UiAccessGrant`], [`crate::UiAccessWait`]), never the login
//! line's words, so it can never name the wrong link: nothing here says
//! "Bluetooth" on a LAN or relay link. A stand-in's access is known whether
//! it runs or not: it lives in this tab, and "simulated" is said once, on
//! the USB row (P2).

use lpc_access::{OpenTo, Tier};

use super::board_card_input::BoardCardInput;
use super::detail_sections::{caution, facts, notice, verbs, without_empty};
use super::ui_bar_work::UiBarWork;
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, NOT_KNOWN_YET, UiBarDetails, UiStackBar};
use crate::app::access::{
    PLAY_ONLY_SENTENCE, UiAccessPanel, UiAccessWait, UiPasswordLine, UiUnlockOffer, open_summary,
};
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::{RichLine, UiStatusKind};

/// The row's name when Studio cannot tell what this link may do.
const ACCESS_UNKNOWN: &str = "unknown";
/// The row's state while the link's hello is read.
const CHECKING: &str = "checking";
/// The notice on a board anyone nearby may edit.
pub const ANYONE_CAN_EDIT_SENTENCE: &str = "Anyone nearby can change this board.";
/// The account-key notice's title, ahead of #1056's sentence.
const ACCOUNT_KEY_TITLE: &str = "Your account's key couldn't be added";
/// The state of a board whose account key could not be added.
const ACCOUNT_KEY_NOT_ADDED: &str = "account key not added";

/// What a link may do, as the row and its details say it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Held {
    /// Play and edit, and how: "edit", "edit · USB", "edit · password",
    /// "edit · no password".
    Edit(Via),
    /// Play only.
    Play,
    /// Nothing: locked.
    Locked,
    /// Nothing reaches it over this link: only USB.
    UsbOnly,
    /// Not known (offline with nothing remembered, or not settled).
    Unknown,
    /// Not known yet: the link's hello is being read.
    Checking,
}

/// How the edit tier is held.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Via {
    /// A stand-in, or a tier only remembered: "edit" alone.
    Alone,
    Usb,
    Password,
    NoPassword,
}

impl Held {
    /// The row's name.
    fn words(self) -> &'static str {
        match self {
            Held::Edit(Via::Alone) => "edit",
            Held::Edit(Via::Usb) => "edit · USB",
            Held::Edit(Via::Password) => "edit · password",
            Held::Edit(Via::NoPassword) => "edit · no password",
            Held::Play => "play only",
            Held::Locked => "locked",
            Held::UsbOnly => "USB only · no password",
            Held::Unknown => ACCESS_UNKNOWN,
            Held::Checking => NOT_KNOWN_YET,
        }
    }

    /// The details' "You can".
    fn you_can(self) -> &'static str {
        match self {
            Held::Edit(_) => "play and edit",
            Held::Play => "play",
            Held::Locked | Held::UsbOnly | Held::Unknown | Held::Checking => "nothing yet",
        }
    }
}

/// The Access row.
pub(crate) fn access_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let access = input.access;
    let grant = access.and_then(|access| access.grant.as_ref());
    let waiting = access.and_then(|access| access.waiting);
    let unlock = access.and_then(|access| access.unlock);
    let usb = input.link_kind() == UiLinkKind::Usb;
    // Open to edit with no password: anyone nearby can change it. A grant
    // at edit with no key's name is that same board, before its list is
    // read.
    let open_to_edit = !input.offline()
        && (access
            .and_then(|access| access.panel.as_ref())
            .is_some_and(|panel| panel.open == OpenTo::Edit)
            || grant.is_some_and(|grant| grant.tier == Tier::Edit && grant.key.is_none()));
    let mut work = None;
    let mut stale = false;
    let held = if input.stand_in() {
        Held::Edit(Via::Alone)
    } else if input.offline() {
        // The tier last known, dimmed (P13): a board reached by its cable
        // edits; a network board's grant, when it is remembered.
        stale = true;
        match grant.map(|grant| grant.tier) {
            Some(Tier::Edit) => Held::Edit(Via::Alone),
            Some(Tier::Play) => Held::Play,
            _ if usb => Held::Edit(Via::Alone),
            _ => Held::Unknown,
        }
    } else if usb {
        Held::Edit(Via::Usb)
    } else if unlock == Some(UiUnlockOffer::Locked) {
        Held::Locked
    } else if unlock == Some(UiUnlockOffer::PlayOnly) {
        Held::Play
    } else if let Some(grant) = grant.filter(|grant| grant.tier == Tier::Edit) {
        match grant.key.is_some() {
            true => Held::Edit(Via::Password),
            false => Held::Edit(Via::NoPassword),
        }
    } else if open_to_edit {
        Held::Edit(Via::NoPassword)
    } else if waiting == Some(UiAccessWait::NoPassword) {
        Held::UsbOnly
    } else if waiting == Some(UiAccessWait::Checking) {
        // The link's own hello, read by itself: no one waits on it, so it
        // stays still ("○ Access  —  checking"), the word in the state's
        // place (P4). Over Bluetooth it can run the length of an update.
        Held::Checking
    } else {
        if waiting == Some(UiAccessWait::Unlocking) {
            // A login someone asked for: the row's work.
            work = Some(UiBarWork::running("unlocking", None));
        }
        Held::Unknown
    };
    let refused = access
        .and_then(|access| access.account_key_refused.as_deref())
        .filter(|_| input.linked());
    let (aside, tone) = match (refused, held) {
        (Some(_), _) => (
            Some(ACCOUNT_KEY_NOT_ADDED.to_string()),
            UiStatusKind::Attention,
        ),
        (None, Held::UsbOnly) => (None, UiStatusKind::Attention),
        (None, Held::Checking) => (Some(CHECKING.to_string()), UiStatusKind::Neutral),
        _ => (None, UiStatusKind::Neutral),
    };
    let action = (held == Held::Play)
        .then(|| input.offer("unlock"))
        .flatten()
        .map(|unlock| {
            UiCardAction::press(unlock, "Unlock")
                .with_icon("lock")
                .drawn(UiActionDraw::Sheet)
        });
    // The name is never empty: a dash while the tier is not known yet,
    // the work's words beside it ("◌ Access  —  unlocking").
    let summary = match work {
        Some(_) => NOT_KNOWN_YET.to_string(),
        None => held.words().to_string(),
    };
    UiStackBar {
        details: details(input, held, tone, open_to_edit, refused),
        aside,
        tone: if work.is_some() {
            UiStatusKind::Neutral
        } else {
            tone
        },
        action,
        stale,
        work,
        ..UiStackBar::new(BarLayer::Access, "lock", summary)
    }
}

/// A new board's Access row: "unknown" while it identifies; "none" once it
/// settles on needing firmware — with no LightPlayer on it there is
/// nothing to unlock.
pub(crate) fn pending_access_bar(pending: &lpa_devices::view::PendingLinkView) -> UiStackBar {
    UiStackBar::new(
        BarLayer::Access,
        "lock",
        match pending.needs_firmware() {
            true => "none",
            false => ACCESS_UNKNOWN,
        },
    )
}

/// The Access row's details: the notice, your access, this board's, the
/// play-only sentence with Unlock, and the access panel.
fn details(
    input: &BoardCardInput<'_>,
    held: Held,
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
    if held == Held::UsbOnly && refused.is_none() {
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
        // A fact, not a notice: a board open to edit is how most boards
        // start (O4), and the status corner is for what needs you now.
        sections.push(caution(
            "Access",
            UiStatusKind::Neutral,
            ANYONE_CAN_EDIT_SENTENCE,
        ));
    }
    let with = match held {
        Held::Edit(Via::NoPassword) => "no password".to_string(),
        Held::Edit(Via::Usb) => "USB".to_string(),
        _ if input.stand_in() => "this tab".to_string(),
        Held::Edit(_) | Held::Play => access
            .and_then(|access| access.grant.as_ref())
            .and_then(|grant| grant.key.clone())
            .unwrap_or_else(|| match held {
                Held::Play => "no password".to_string(),
                _ => "—".to_string(),
            }),
        Held::Locked | Held::UsbOnly | Held::Unknown | Held::Checking => "—".to_string(),
    };
    sections.push(facts(
        "Your access",
        vec![
            RichLine::new("You can", held.you_can()),
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
        switch: None,
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

    /// P13: offline, the tier last known stays, dimmed.
    #[test]
    fn an_offline_board_keeps_the_tier_last_known_dimmed() {
        let bar = access_bar(&CardFixture::offline().input());
        assert_eq!(bar.summary, "edit", "reached by its cable, it edits");
        assert!(bar.stale);
        assert_eq!(bar.kind, Some("Access"));
        assert_eq!(bar.icon, "lock");

        let ble = access_bar(&CardFixture::offline().over(UiLinkKind::Bluetooth).input());
        assert_eq!(ble.summary, "unknown", "nothing remembered over the air");
        assert!(ble.stale);
    }

    /// A stand-in lives in this tab: its access is known even while it is
    /// off, and it says "edit" alone — "simulated" is the USB row's.
    #[test]
    fn a_stand_in_edits_on_or_off() {
        let mut off = CardFixture::offline();
        off.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        let bar = access_bar(&off.input());
        assert_eq!((bar.summary.as_str(), bar.aside.as_deref()), ("edit", None));
        assert!(!bar.stale, "it lives in this tab");
        let mut sim = CardFixture::ready();
        sim.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        let bar = access_bar(&sim.input());
        assert_eq!(bar.summary, "edit");
        assert_eq!(line(&bar, "With").as_deref(), Some("this tab"));
    }

    #[test]
    fn a_trusted_link_edits_by_usb() {
        let bar = access_bar(&CardFixture::ready().input());
        assert_eq!(bar.summary, "edit · USB");
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        assert_eq!(line(&bar, "With").as_deref(), Some("USB"));
    }

    #[test]
    fn a_locked_board_says_locked_and_leaves_unlock_to_the_primary() {
        let bar = access_bar(
            &CardFixture::ready()
                .over(UiLinkKind::Bluetooth)
                .locked()
                .input(),
        );
        assert_eq!(bar.summary, "locked");
        assert_eq!(bar.aside, None);
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
        assert_eq!(bar.summary, "play only");
        assert_eq!(bar.aside, None, "said once");
        let unlock = bar.action.clone().expect("Unlock");
        assert_eq!(unlock.word, "Unlock");
        assert_eq!(unlock.icon.as_deref(), Some("lock"));
        assert_eq!(unlock.draw, UiActionDraw::Sheet);
        assert!(unlock.offer.to_string().ends_with("/unlock"));
        assert_eq!(line(&bar, "With").as_deref(), Some("friends"));
        assert!(bar.details.sections.iter().any(|section| {
            section.sentence.as_deref() == Some(PLAY_ONLY_SENTENCE)
                && section.affordances.first().map(|a| a.word.as_str()) == Some("Unlock")
        }));
    }

    /// A key reads "edit · password", its own name in the details; an open
    /// board reads "edit · no password", quiet (O4).
    #[test]
    fn an_edit_grant_says_password_and_an_open_board_is_quiet() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Wifi);
        fixture.access = Some(UiDeviceAccess {
            grant: Some(UiAccessGrant {
                tier: Tier::Edit,
                key: Some("Yona's MacBook".to_string()),
            }),
            ..UiDeviceAccess::default()
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "edit · password");
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        assert_eq!(line(&bar, "With").as_deref(), Some("Yona's MacBook"));

        fixture.access.as_mut().unwrap().grant = Some(UiAccessGrant {
            tier: Tier::Edit,
            key: None,
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "edit · no password");
        assert_eq!(
            bar.tone,
            UiStatusKind::Neutral,
            "quiet: open is the default"
        );
        assert!(
            bar.details.notice().is_none(),
            "a fact the corner does not report"
        );
        assert!(
            bar.details
                .sections
                .iter()
                .any(
                    |section| section.sentence.as_deref() == Some(ANYONE_CAN_EDIT_SENTENCE)
                        && section.tone == UiStatusKind::Neutral
                )
        );
    }

    #[test]
    fn a_usb_board_open_to_edit_edits_by_usb_and_shows_its_panel() {
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
        assert_eq!(bar.summary, "edit · USB");
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        assert_eq!(line(&bar, "Open to").as_deref(), Some("open"));
        assert_eq!(line(&bar, "Edit").as_deref(), Some("anyone"));
        assert!(matches!(
            bar.details.panels.as_slice(),
            [UiDetailPanel::Access(_)]
        ));
    }

    /// Checking is the link's own hello, read by itself: a still state
    /// beside a dash, never work (it once spun through a whole Bluetooth
    /// update, FS3 2026-10-24). Unlocking is a login someone asked for: the
    /// row's work, beside the same dash.
    #[test]
    fn checking_is_still_and_unlocking_is_the_rows_work() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Relay);
        fixture.access = Some(UiDeviceAccess {
            waiting: Some(UiAccessWait::Checking),
            ..UiDeviceAccess::default()
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "\u{2014}");
        assert_eq!(bar.aside.as_deref(), Some("checking"));
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        assert_eq!(bar.work, None);
        fixture.access.as_mut().unwrap().waiting = Some(UiAccessWait::Unlocking);
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "\u{2014}");
        assert_eq!(
            bar.work.map(|work| work.words).as_deref(),
            Some("unlocking")
        );
    }

    /// A board no password reaches over this link needs you: orange, and no
    /// button — no core offer sets its password from here.
    #[test]
    fn a_board_no_password_reaches_is_usb_only_in_orange() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth);
        fixture.access = Some(UiDeviceAccess {
            waiting: Some(UiAccessWait::NoPassword),
            ..UiDeviceAccess::default()
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "USB only · no password");
        assert_eq!(bar.tone, UiStatusKind::Attention);
        assert_eq!(bar.action, None, "no web-built Set password");
        assert_eq!(
            bar.details
                .notice()
                .and_then(|notice| notice.sentence.as_deref()),
            Some("Bluetooth has no device password here — connect by USB to set one")
        );
    }

    /// #1056: the account's key could not be added — said as the row's
    /// state and its notice, in Attention.
    #[test]
    fn a_refused_account_key_is_the_access_notice() {
        let mut fixture = CardFixture::ready();
        let sentence = crate::account_key_refused_sentence("The board is full.");
        fixture.access = Some(UiDeviceAccess {
            account_key_refused: Some(sentence.clone()),
            ..UiDeviceAccess::default()
        });
        let bar = access_bar(&fixture.input());
        assert_eq!(bar.summary, "edit · USB");
        assert_eq!(bar.aside.as_deref(), Some("account key not added"));
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

    /// A new board's access is unknown while it identifies, and none once
    /// it needs firmware — never "Access not known".
    #[test]
    fn a_new_boards_access_is_unknown_then_none() {
        let mut pending = super::super::primary_action::tests::pending_view();
        assert_eq!(pending_access_bar(&pending).summary, "unknown");
        pending.firmware_face = lpa_devices::view::FirmwareFace::Blank;
        assert_eq!(pending_access_bar(&pending).summary, "none");
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

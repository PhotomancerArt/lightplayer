//! The lightplayer.app row: the board reached through the cloud
//! (`docs/ux/anatomy/board-card.md`, "lightplayer.app"). Its kind is the
//! link's name, "lightplayer.app"; signed in, its name is the account (its
//! email) and its state says whether lightplayer.app can reach the board,
//! and why not. It has its own row for privacy —
//! "lightplayer.app · disabled" is a clean thing to say — and for its
//! journey: sign in, join Wi‑Fi, reached.
//!
//! | Facts | Name · state | Action | Tone |
//! |---|---|---|---|
//! | nothing yet (no firmware, not answered, a stand-in, a board with no relay) | "—" | — | Neutral |
//! | its Cloud relay switch off | "disabled" | — | Neutral |
//! | signed out | "not connected" | Sign in | Neutral |
//! | signed in, the board not on Wi‑Fi (nothing saved, nothing joined, or its Wi‑Fi off) | "river@example.com · no Wi‑Fi" (which, in the details) | — (its fix is the Wi‑Fi row's) | Neutral |
//! | signed in, the board holds no account key | "river@example.com · not added" (why in the details) | — | Neutral |
//! | signed in, joined, lightplayer.app didn't answer | "river@example.com · can't reach" | — | Attention |
//! | signed in, lightplayer.app refused the board | "river@example.com · can't reach" (why in the details) | — | Attention |
//! | the board dialling | "river@example.com · connecting" | — | Neutral |
//! | reachable | "river@example.com · online", and "connected" when this tab's link is the relay | — | Neutral |
//! | no link here, and the account's list says it is online (`ListAccountBoards`) | "river@example.com · online" | — (Connect through lightplayer.app is the card's primary) | Neutral |
//! | no link here, and the account's list holds it, offline | "river@example.com · 2 h ago" (dimmed: last known; the surface says "It was last seen 2 hours ago.") | — | Neutral |
//!
//! An offline card keeps what the board last said about its Wi‑Fi and its
//! key, dimmed, but never "connecting" or "can't reach": this tab can't
//! know how the board stands with lightplayer.app now. It says "online"
//! only when the signed-in account's list says so, and then nothing is
//! dimmed: the list is current.
//!
//! Signed out is a new board's to-do: the row is lit, with Sign in. Hushing
//! it is turning Cloud relay off with the switch at the top of its details
//! (the board's own, `devices/<board>/wifi/cloud-relay`), after which the
//! row says "disabled", for privacy, and the switch turns it back on.
//!
//! The struck cloud mark the old Link row carried is gone: this row says
//! it.
//! The account is the signed-in one, named by its email
//! ([`super::board_links::account_words`]): Yona's phone, 10-19, read
//! "Yona's account" on a board with no Wi‑Fi as "this will work".

use lpc_wire::server::{NetworkStatus, RelayRefusal, RelayState, StationState};

use super::board_card_input::BoardCardInput;
use super::detail_sections::{facts, help};
use super::link_bar::{LinkFacts, connect_verb, link_bar};
use super::link_session::{CONNECTED, DISABLED, NOT_CONNECTED, NOT_KNOWN};
use super::row_switch::{SwitchOffer, row_switch};
use super::ui_card_action::UiCardAction;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::account_boards::AccountBoardNote;
use crate::app::devices::sign_in_offer::SIGN_IN_VERB;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::app::network::WIFI_ENABLED_PARAM;
use crate::app::network::wifi_words::{CLOUD_RELAY_HELP, card, relay};
use crate::core::data_kinds::age_words;
use crate::{RichLine, RichSection, UiStatusKind};

/// The details' line while nobody is signed in.
pub const SIGN_IN_SENTENCE: &str =
    "Sign in to reach this board from any browser, through lightplayer.app.";

/// …and what stays at home until then.
pub const NOTHING_GOES_SENTENCE: &str = "Nothing goes to lightplayer.app until you sign in.";

/// The lightplayer.app row.
pub(crate) fn cloud_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let status = input.network_status().filter(|_| !input.stand_in());
    let mut facts = relay_facts(input, status);
    let listed_offline = input.links.cloud.is_some_and(|cloud| !cloud.online);
    facts.stale = input.offline()
        && !input.online_at_cloud()
        && (status.is_some() || listed_offline)
        && facts.action.is_none();
    facts.details = details(input, status);
    facts.verbs = connect_verb(
        input,
        "connect-relay",
        "Connect through lightplayer.app",
        "cloud",
    )
    .into_iter()
    .collect();
    link_bar(input, BarLayer::Cloud, facts)
}

/// The row's facts from the board's status and who is signed in.
fn relay_facts(input: &BoardCardInput<'_>, status: Option<&NetworkStatus>) -> LinkFacts {
    let row = |summary: &str| LinkFacts::new("cloud", summary);
    let account = input.links.account_words();
    // Reached through lightplayer.app right now: whatever the board last
    // said, the cloud reaches it.
    if input.reached_over(UiLinkKind::Relay) {
        return LinkFacts {
            aside: Some(CONNECTED.to_string()),
            ..row(account.as_deref().unwrap_or(UiLinkKind::Relay.label())).named()
        };
    }
    // No link here, and the account's list says lightplayer.app reaches it
    // now: that is current, whatever the board last said here.
    if let Some(account) = account.as_deref().filter(|_| input.online_at_cloud()) {
        return LinkFacts {
            aside: Some(card::ONLINE.to_string()),
            ..row(account).named()
        };
    }
    // The board holds the account's key and the account's list is full:
    // it is not on it, and the card says so (its details say why).
    if let Some(account) = account.as_deref()
        && matches!(
            input.links.account_note,
            Some(AccountBoardNote::ListFull { .. })
        )
    {
        return LinkFacts {
            aside: Some(card::LIST_FULL.to_string()),
            tone: UiStatusKind::Attention,
            ..row(account).named()
        };
    }
    // No link here, and the account's list holds the board, offline: when
    // anything last saw it, which says more than what it last said here.
    if let Some(account) = account.as_deref().filter(|_| input.offline())
        && let Some(cloud) = input.links.cloud.filter(|cloud| !cloud.online)
    {
        return LinkFacts {
            aside: cloud
                .last_seen
                .map(|seen| card::last_seen(&age_words((input.now - seen).max(0.0)))),
            ..row(account).named()
        };
    }
    let Some(status) = status else {
        return row(NOT_KNOWN);
    };
    if !status.cloud_relay {
        return row(DISABLED);
    }
    if status.relay == RelayState::Off {
        // Cloud relay on, and the board has no relay to dial with.
        return row(NOT_KNOWN);
    }
    // Signed out: a new board's to-do, until signed in or disabled.
    let Some(account) = account else {
        let action = sign_in(input);
        return LinkFacts {
            todo: action.is_some(),
            action,
            ..row(NOT_CONNECTED)
        };
    };
    let named = |aside: &str, tone| LinkFacts {
        aside: Some(aside.to_string()),
        tone,
        ..row(&account).named()
    };
    if no_wifi(status).is_some() {
        return named(card::NO_WIFI, UiStatusKind::Neutral);
    }
    if status.relay == RelayState::NoAccount {
        return named(card::NOT_ADDED, UiStatusKind::Neutral);
    }
    // How it stands with lightplayer.app now, which only a board that
    // answers can say.
    if input.offline() {
        return row(&account).named();
    }
    match status.relay {
        RelayState::WaitingForInternet
        | RelayState::Refused {
            reason: RelayRefusal::UnknownAccount | RelayRefusal::UpdateFirmware,
        } => named(card::CANT_REACH, UiStatusKind::Attention),
        RelayState::Connecting => named(card::CONNECTING, UiStatusKind::Neutral),
        RelayState::Connected
        | RelayState::Off
        | RelayState::NoAccount
        | RelayState::Refused {
            reason: RelayRefusal::Busy,
        } => named(card::ONLINE, UiStatusKind::Neutral),
    }
}

/// Why lightplayer.app can't reach the board because of its Wi‑Fi, as
/// the details say it: its Wi‑Fi off, nothing saved, or nothing joined.
/// `None` once it is joined. The row says "no Wi‑Fi" for each.
fn no_wifi(status: &NetworkStatus) -> Option<&'static str> {
    match () {
        _ if matches!(status.station, StationState::Connected { .. }) => None,
        _ if !status.wifi => Some(relay::WIFI_OFF),
        _ if status.networks.is_empty() => Some(relay::NO_WIFI),
        _ => Some(relay::WIFI_DOWN),
    }
}

/// Sign in, when it is offered here (nobody is signed in).
fn sign_in(input: &BoardCardInput<'_>) -> Option<UiCardAction> {
    input
        .offer(SIGN_IN_VERB)
        .map(|offer| UiCardAction::own_words(offer).with_icon(offer.icon.clone()))
}

/// The row's details: the board's Cloud relay switch at the top while it
/// answers (`devices/<board>/wifi/cloud-relay`, [`super::row_switch`]),
/// and why it is dimmed where this link may not flip it; then, switched
/// off, that it never connects; signed out, what signing in gets you, with
/// Sign in, and what stays at home; signed in, the account and how the
/// board stands with lightplayer.app.
fn details(input: &BoardCardInput<'_>, status: Option<&NetworkStatus>) -> UiBarDetails {
    let mut details = UiBarDetails::default();
    // What lightplayer.app's last answer about the account's list left to
    // say, first.
    if let Some(note) = input.links.account_note {
        details.sections.push(help("", note.sentence()));
    }
    let Some(status) = status else {
        return details;
    };
    details.switch = (!input.offline()).then(|| {
        row_switch(
            input,
            BarLayer::Cloud,
            "cloud",
            status.cloud_relay,
            SwitchOffer {
                under: &["wifi", "cloud-relay"],
                param: WIFI_ENABLED_PARAM,
                turn_off: card::DISABLE_CLOUD,
                turn_on: card::ENABLE_CLOUD,
            },
        )
    });
    if details
        .switch
        .as_ref()
        .is_some_and(|switch| switch.flip.is_none())
    {
        details.sections.extend(cannot_change(input));
    }
    if !status.cloud_relay {
        details.sections.push(help("", card::CLOUD_DISABLED));
        return details;
    }
    let Some(account) = input.links.account_words() else {
        // Not a notice: signing in is a to-do, not news for the corner.
        let mut ask = help("", SIGN_IN_SENTENCE);
        ask.affordances.extend(sign_in(input));
        details.sections.push(ask);
        details
            .sections
            .push(help("Privacy", NOTHING_GOES_SENTENCE));
        return details;
    };
    let mut lines = vec![RichLine::new("Account", account)];
    if let Some((line, _)) = relay::line(status) {
        lines.push(RichLine::new("Now", line));
    }
    // One section: the account, how the board stands, and what the relay
    // is for — or, when the board can't use it, why.
    let mut reach = facts("", lines);
    reach.sentence = Some(
        no_wifi(status)
            .or_else(|| relay::note(status.relay))
            .unwrap_or(CLOUD_RELAY_HELP)
            .to_string(),
    );
    details.sections.push(reach);
    details
}

/// Why the switch isn't here on a board that answers: this link may not
/// change its network settings.
fn cannot_change(input: &BoardCardInput<'_>) -> Option<RichSection<UiCardAction>> {
    input
        .wifi
        .filter(|_| !input.offline())
        .and_then(|wifi| wifi.waiting_line())
        .map(|line| help("", line))
}

#[cfg(test)]
mod tests {
    use lpc_wire::server::SavedNetworkInfo;

    use super::super::card_fixtures::CardFixture;
    use super::*;
    use crate::UiDeviceWifi;

    /// One test per row of the module's table.
    #[test]
    fn the_cloud_row_says_how_lightplayer_app_reaches_the_board() {
        let joined = StationState::Connected {
            ssid: "home".to_string(),
            ip: "10.0.0.5".to_string(),
            rssi: -50,
            host: "lp-b48c.local".to_string(),
        };
        let off = StationState::NotConnected;
        let busy = RelayState::Refused {
            reason: RelayRefusal::Busy,
        };
        let refused = RelayState::Refused {
            reason: RelayRefusal::UnknownAccount,
        };
        // (status: (cloud relay on, Wi‑Fi on, networks saved, station,
        // relay), signed in, summary, aside, tone)
        type Status = (bool, bool, bool, StationState, RelayState);
        let table: Vec<(Option<Status>, bool, &str, Option<&str>, UiStatusKind)> = vec![
            (None, true, "\u{2014}", None, UiStatusKind::Neutral),
            (
                Some((false, true, true, joined.clone(), RelayState::Off)),
                true,
                "disabled",
                None,
                UiStatusKind::Neutral,
            ),
            (
                Some((true, true, true, joined.clone(), RelayState::Off)),
                true,
                "\u{2014}",
                None,
                UiStatusKind::Neutral,
            ),
            (
                Some((
                    true,
                    true,
                    false,
                    off.clone(),
                    RelayState::WaitingForInternet,
                )),
                false,
                "not connected",
                None,
                UiStatusKind::Neutral,
            ),
            (
                Some((
                    true,
                    true,
                    false,
                    off.clone(),
                    RelayState::WaitingForInternet,
                )),
                true,
                EMAIL,
                Some("no Wi\u{2011}Fi"),
                UiStatusKind::Neutral,
            ),
            (
                Some((true, true, false, off.clone(), RelayState::NoAccount)),
                true,
                EMAIL,
                Some("no Wi\u{2011}Fi"),
                UiStatusKind::Neutral,
            ),
            (
                Some((
                    true,
                    true,
                    true,
                    off.clone(),
                    RelayState::WaitingForInternet,
                )),
                true,
                EMAIL,
                Some("no Wi\u{2011}Fi"),
                UiStatusKind::Neutral,
            ),
            (
                Some((
                    true,
                    false,
                    true,
                    off.clone(),
                    RelayState::WaitingForInternet,
                )),
                true,
                EMAIL,
                Some("no Wi\u{2011}Fi"),
                UiStatusKind::Neutral,
            ),
            (
                Some((true, true, true, joined.clone(), RelayState::NoAccount)),
                true,
                EMAIL,
                Some("not added"),
                UiStatusKind::Neutral,
            ),
            (
                Some((
                    true,
                    true,
                    true,
                    joined.clone(),
                    RelayState::WaitingForInternet,
                )),
                true,
                EMAIL,
                Some("can't reach"),
                UiStatusKind::Attention,
            ),
            (
                Some((true, true, true, joined.clone(), refused)),
                true,
                EMAIL,
                Some("can't reach"),
                UiStatusKind::Attention,
            ),
            (
                Some((true, true, true, joined.clone(), RelayState::Connecting)),
                true,
                EMAIL,
                Some("connecting"),
                UiStatusKind::Neutral,
            ),
            (
                Some((true, true, true, joined.clone(), RelayState::Connected)),
                true,
                EMAIL,
                Some("online"),
                UiStatusKind::Neutral,
            ),
            (
                Some((true, true, true, joined.clone(), busy)),
                true,
                EMAIL,
                Some("online"),
                UiStatusKind::Neutral,
            ),
        ];
        for (status, signed_in, summary, aside, tone) in table {
            let mut fixture = CardFixture::ready();
            fixture.account = signed_in.then(|| EMAIL.to_string());
            fixture.wifi = status.clone().map(|(on, wifi_on, saved, station, relay)| {
                let mut wifi = wifi(on, station, relay);
                let status = wifi.status.as_mut().unwrap();
                status.wifi = wifi_on;
                if !saved {
                    status.networks.clear();
                }
                wifi
            });
            let bar = cloud_bar(&fixture.input());
            assert_eq!(bar.kind, Some("lightplayer.app"));
            assert_eq!(
                (bar.summary.as_str(), bar.aside.as_deref(), bar.tone),
                (summary, aside, tone),
                "{status:?} signed in: {signed_in}"
            );
        }
    }

    /// Yona's phone, 10-19: signed in, no Wi‑Fi set up. The row says the
    /// account and why lightplayer.app can't reach the board, and its
    /// details say what to do about it.
    #[test]
    fn signed_in_with_no_wifi_says_it_cant_be_reached() {
        let mut fixture = CardFixture::ready();
        fixture.account = Some(EMAIL.to_string());
        let mut board = wifi(
            true,
            StationState::NotConnected,
            RelayState::WaitingForInternet,
        );
        board.status.as_mut().unwrap().networks.clear();
        fixture.wifi = Some(board);
        let bar = cloud_bar(&fixture.input());
        assert_eq!(
            (bar.summary.as_str(), bar.aside.as_deref()),
            (EMAIL, Some("no Wi\u{2011}Fi"))
        );
        assert!(!bar.todo, "its fix is the Wi\u{2011}Fi row's");
        assert_eq!(line(&bar, "Account").as_deref(), Some(EMAIL));
        let sentences: Vec<&str> = bar
            .details
            .sections
            .iter()
            .filter_map(|section| section.sentence.as_deref())
            .collect();
        assert_eq!(sentences, [relay::NO_WIFI]);
    }

    /// No link here, and the account's list says the board is online at
    /// lightplayer.app: the row says so, not dimmed — for a board this
    /// browser never met too, which has no status of its own to read.
    #[test]
    fn the_accounts_list_says_an_offline_card_is_online() {
        let mut fixture = CardFixture::online_at_cloud();
        let bar = cloud_bar(&fixture.input());
        assert_eq!(
            (bar.summary.as_str(), bar.aside.as_deref()),
            ("river@example.com", Some(card::ONLINE))
        );
        assert!(!bar.stale);
        fixture.view.remembered_firmware = None;
        fixture.remembered_network = None;
        let never_met = cloud_bar(&fixture.input());
        assert_eq!(never_met.aside.as_deref(), Some(card::ONLINE));
    }

    /// Offline, the card keeps the board's last Wi‑Fi facts but never
    /// claims it is online at lightplayer.app on its own say.
    #[test]
    fn an_offline_card_never_says_online() {
        let mut fixture = CardFixture::offline();
        fixture.account = Some(EMAIL.to_string());
        fixture.remembered_network = Some(
            wifi(
                true,
                StationState::Connected {
                    ssid: "home".to_string(),
                    ip: "10.0.0.5".to_string(),
                    rssi: -50,
                    host: "lp-b48c.local".to_string(),
                },
                RelayState::Connected,
            )
            .status
            .unwrap(),
        );
        let bar = cloud_bar(&fixture.input());
        assert_eq!((bar.summary.as_str(), bar.aside.as_deref()), (EMAIL, None));
        assert!(bar.stale);
    }

    /// Signed out, the row says the fact and its button the fix; the
    /// details say what signing in gets you and what stays at home.
    #[test]
    fn signed_out_says_not_connected_and_offers_sign_in() {
        let mut fixture = CardFixture::ready();
        fixture.wifi = Some(wifi(
            true,
            StationState::NotConnected,
            RelayState::NoAccount,
        ));
        let bar = cloud_bar(&fixture.input());
        assert_eq!(bar.summary, "not connected");
        let sign_in = bar.action.clone().expect("Sign in");
        assert_eq!(sign_in.word, "Sign in");
        assert!(sign_in.offer.to_string().ends_with("/sign-in"));
        let sentences: Vec<&str> = bar
            .details
            .sections
            .iter()
            .filter_map(|section| section.sentence.as_deref())
            .collect();
        assert_eq!(sentences, [SIGN_IN_SENTENCE, NOTHING_GOES_SENTENCE]);
    }

    /// The board's Cloud relay switch leads the details: signed out and on,
    /// it flips off and Sign in stands alone below it; off, it flips on
    /// over "lightplayer.app is disabled…".
    #[test]
    fn its_details_lead_with_the_cloud_relay_switch() {
        let mut fixture = CardFixture::ready();
        fixture.wifi = Some(wifi(
            true,
            StationState::NotConnected,
            RelayState::NoAccount,
        ));
        let details = cloud_bar(&fixture.input()).details;
        let switch = details.switch.clone().expect("a switch at the top");
        assert_eq!(
            (switch.label.as_str(), switch.icon.as_str(), switch.on),
            ("lightplayer.app", "cloud", true)
        );
        let flip = switch.flip.expect("this link may flip it");
        assert_eq!(flip.word, card::DISABLE_CLOUD);
        assert!(flip.offer.to_string().ends_with("/wifi/cloud-relay"));
        assert_eq!(flip.args.get(WIFI_ENABLED_PARAM), Some("false"));
        let words: Vec<&str> = details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter())
            .map(|action| action.word.as_str())
            .collect();
        assert_eq!(words, ["Sign in"]);

        fixture.wifi = Some(wifi(false, StationState::NotConnected, RelayState::Off));
        let details = cloud_bar(&fixture.input()).details;
        let switch = details.switch.clone().expect("a switch at the top");
        assert!(!switch.on);
        assert_eq!(switch.flip.expect("flips on").word, card::ENABLE_CLOUD);
        let sentences: Vec<&str> = details
            .sections
            .iter()
            .filter_map(|section| section.sentence.as_deref())
            .collect();
        assert_eq!(sentences, [card::CLOUD_DISABLED]);
    }

    /// Joined and lightplayer.app didn't answer: orange, a problem.
    #[test]
    fn cant_reach_is_orange() {
        let mut fixture = CardFixture::ready();
        fixture.account = Some(EMAIL.to_string());
        fixture.wifi = Some(wifi(
            true,
            StationState::Connected {
                ssid: "home".to_string(),
                ip: "10.0.0.5".to_string(),
                rssi: -50,
                host: "lp-b48c.local".to_string(),
            },
            RelayState::WaitingForInternet,
        ));
        let bar = cloud_bar(&fixture.input());
        assert_eq!(bar.tone, UiStatusKind::Attention);
        assert_eq!(line(&bar, "Account").as_deref(), Some(EMAIL));
        assert_eq!(line(&bar, "Now").as_deref(), Some(relay::NO_INTERNET));
    }

    /// Reached through the relay, the row is the link in use.
    #[test]
    fn a_relay_board_is_the_cloud_rows_and_never_says_bluetooth_or_usb() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Relay);
        fixture.lan = crate::lan_link_for_endpoint("relay:a0f26287b48c");
        fixture.account = Some(EMAIL.to_string());
        let card = super::super::board_card_builder::board_card(&fixture.input());
        let bar = card.bar(BarLayer::Cloud);
        assert_eq!(
            (bar.summary.as_str(), bar.aside.as_deref()),
            (EMAIL, Some("connected"))
        );
        assert_eq!(bar.icon, "cloud");
        assert_eq!(card.bar(BarLayer::Usb).summary, "not connected");
        let words = format!("{bar:?}");
        assert!(!words.contains("Bluetooth"), "{words}");
    }

    const EMAIL: &str = "river@example.com";

    fn wifi(cloud_relay: bool, station: StationState, relay: RelayState) -> UiDeviceWifi {
        UiDeviceWifi {
            status: Some(NetworkStatus {
                wifi: true,
                cloud_relay,
                networks: vec![SavedNetworkInfo {
                    ssid: "home".to_string(),
                    has_password: true,
                    hidden: false,
                    last: None,
                }],
                station,
                relay,
            }),
            ..UiDeviceWifi::new(lpa_devices::DeviceId(7), true)
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

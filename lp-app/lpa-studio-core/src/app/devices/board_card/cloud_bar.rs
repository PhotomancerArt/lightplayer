//! The lightplayer.app row: the board reached through the cloud
//! (`docs/ux/anatomy/board-card.md`, "lightplayer.app"). Its kind is the
//! link's name, "lightplayer.app"; its name is the account it belongs to
//! while lightplayer.app can reach it. It has its own row for privacy —
//! "lightplayer.app · disabled" is a clean thing to say — and for its
//! journey: sign in, join Wi‑Fi, reached.
//!
//! | Facts | Name · state | Action | Tone |
//! |---|---|---|---|
//! | nothing yet (no firmware, not answered, a stand-in, a board with no relay) | "—" | — | Neutral |
//! | its Cloud relay switch off | "disabled" | — | Neutral |
//! | signed out | "not connected" | Sign in | Neutral |
//! | signed in, the board not on Wi‑Fi | "not connected" | — (its fix is the Wi‑Fi row's) | Neutral |
//! | signed in, the board holds no account key | "not connected" (why in the details) | — | Neutral |
//! | signed in, joined, lightplayer.app didn't answer | "Yona's account · can't reach" | — | Attention |
//! | signed in, lightplayer.app refused the board | "Yona's account · can't reach" (why in the details) | — | Attention |
//! | the board dialling | "Yona's account · connecting" | — | Neutral |
//! | reachable | "Yona's account", and "connected" when this tab's link is the relay | — | Neutral |
//!
//! Signed out is a new board's to-do: the row is lit, with Sign in. Hushing
//! it is turning Cloud relay off — "Disable lightplayer.app" beside Sign
//! in, or under the account (the board's own switch,
//! `devices/<board>/wifi/cloud-relay`) — after which the row says
//! "disabled", for privacy, and its details offer "Enable lightplayer.app".
//!
//! The struck cloud mark the old Link row carried is gone: this row says
//! it.
//! The account is the signed-in one, named as its key is on every board
//! ("Yona's account", [`super::board_links::account_words`]).

use lpc_wire::server::{NetworkStatus, RelayRefusal, RelayState, StationState};

use super::board_card_input::BoardCardInput;
use super::detail_sections::{facts, help, verbs};
use super::link_bar::{LinkFacts, connect_verb, link_bar};
use super::link_session::{CONNECTED, DISABLED, NOT_CONNECTED, NOT_KNOWN};
use super::ui_card_action::UiCardAction;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::sign_in_offer::SIGN_IN_VERB;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::app::network::WIFI_ENABLED_PARAM;
use crate::app::network::wifi_words::{CLOUD_RELAY_HELP, card, relay};
use crate::{OfferArgs, RichLine, RichSection, UiStatusKind};

/// The details' line while nobody is signed in.
pub const SIGN_IN_SENTENCE: &str =
    "Sign in to reach this board from any browser, through lightplayer.app.";

/// …and what stays at home until then.
pub const NOTHING_GOES_SENTENCE: &str = "Nothing goes to lightplayer.app until you sign in.";

/// The lightplayer.app row.
pub(crate) fn cloud_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let status = input.network_status().filter(|_| !input.stand_in());
    let mut facts = relay_facts(input, status);
    facts.stale = input.offline() && status.is_some() && facts.action.is_none();
    facts.details = details(input, status);
    facts.verbs = connect_verb(
        input,
        "connect-relay",
        "Connect through lightplayer.app",
        "cloud",
    )
    .into_iter()
    .collect();
    // Signed in and on, the hush sits with the row's other verbs; signed
    // out it is beside Sign in, and off its details offer Enable.
    if input.links.account.is_some() && status.is_some_and(|status| status.cloud_relay) {
        facts
            .verbs
            .extend(relay_switch(input, false, card::DISABLE_CLOUD));
    }
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
    let joined = matches!(status.station, StationState::Connected { .. });
    let named = |aside: Option<&str>, tone| LinkFacts {
        aside: aside.map(str::to_string),
        tone,
        ..row(&account).named()
    };
    match status.relay {
        _ if !joined => row(NOT_CONNECTED),
        RelayState::NoAccount => row(NOT_CONNECTED),
        RelayState::WaitingForInternet
        | RelayState::Refused {
            reason: RelayRefusal::UnknownAccount | RelayRefusal::UpdateFirmware,
        } => named(Some(card::CANT_REACH), UiStatusKind::Attention),
        RelayState::Connecting => named(Some(card::CONNECTING), UiStatusKind::Neutral),
        RelayState::Connected
        | RelayState::Off
        | RelayState::Refused {
            reason: RelayRefusal::Busy,
        } => named(None, UiStatusKind::Neutral),
    }
}

/// Sign in, when it is offered here (nobody is signed in).
fn sign_in(input: &BoardCardInput<'_>) -> Option<UiCardAction> {
    input
        .offer(SIGN_IN_VERB)
        .map(|offer| UiCardAction::own_words(offer).with_icon(offer.icon.clone()))
}

/// The board's Cloud relay switch (`devices/<board>/wifi/cloud-relay`)
/// pressed to `on`, worded `word`, where it is offered (a link at edit).
fn relay_switch(input: &BoardCardInput<'_>, on: bool, word: &str) -> Option<UiCardAction> {
    input.offer_under(&["wifi", "cloud-relay"]).map(|offer| {
        UiCardAction::press_with(
            offer,
            word,
            OfferArgs::new().with(WIFI_ENABLED_PARAM, on.to_string()),
        )
        .with_icon("cloud")
    })
}

/// The row's details: signed out, what signing in gets you and what stays
/// at home, with Sign in and Disable lightplayer.app; signed in, the
/// account and how the board stands with lightplayer.app; disabled, that
/// it never connects, and Enable lightplayer.app.
fn details(input: &BoardCardInput<'_>, status: Option<&NetworkStatus>) -> UiBarDetails {
    let mut details = UiBarDetails::default();
    let Some(status) = status else {
        return details;
    };
    if !status.cloud_relay {
        details
            .sections
            .push(help("lightplayer.app", card::CLOUD_DISABLED));
        let enable = relay_switch(input, true, card::ENABLE_CLOUD);
        if enable.is_none() {
            details.sections.extend(cannot_change(input));
        }
        details.sections.push(verbs(enable.into_iter().collect()));
        return details;
    }
    let Some(account) = input.links.account_words() else {
        // Not a notice: signing in is a to-do, not news for the corner.
        let mut ask = help("lightplayer.app", SIGN_IN_SENTENCE);
        ask.affordances.extend(sign_in(input));
        ask.affordances
            .extend(relay_switch(input, false, card::DISABLE_CLOUD));
        details.sections.push(ask);
        details
            .sections
            .push(help("lightplayer.app", NOTHING_GOES_SENTENCE));
        return details;
    };
    let mut lines = vec![RichLine::new("Account", account)];
    if let Some((line, _)) = relay::line(status) {
        lines.push(RichLine::new("Now", line));
    }
    details.sections.push(facts("lightplayer.app", lines));
    if let Some(note) = relay::note(status.relay) {
        details.sections.push(help("lightplayer.app", note));
    }
    details.sections.push(help("Cloud relay", CLOUD_RELAY_HELP));
    details
}

/// Why the switch isn't here on a board that answers: this link may not
/// change its network settings.
fn cannot_change(input: &BoardCardInput<'_>) -> Option<RichSection<UiCardAction>> {
    input
        .wifi
        .filter(|_| !input.offline())
        .and_then(|wifi| wifi.waiting_line())
        .map(|line| help("lightplayer.app", line))
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
        let table: Vec<(
            Option<(bool, StationState, RelayState)>,
            bool,
            &str,
            Option<&str>,
        )> = vec![
            (None, true, "\u{2014}", None),
            (
                Some((false, joined.clone(), RelayState::Off)),
                true,
                "disabled",
                None,
            ),
            (
                Some((true, joined.clone(), RelayState::Off)),
                true,
                "\u{2014}",
                None,
            ),
            (
                Some((
                    true,
                    StationState::NotConnected,
                    RelayState::WaitingForInternet,
                )),
                false,
                "not connected",
                None,
            ),
            (
                Some((
                    true,
                    StationState::NotConnected,
                    RelayState::WaitingForInternet,
                )),
                true,
                "not connected",
                None,
            ),
            (
                Some((true, joined.clone(), RelayState::NoAccount)),
                true,
                "not connected",
                None,
            ),
            (
                Some((true, joined.clone(), RelayState::WaitingForInternet)),
                true,
                "Yona's account",
                Some("can't reach"),
            ),
            (
                Some((
                    true,
                    joined.clone(),
                    RelayState::Refused {
                        reason: RelayRefusal::UnknownAccount,
                    },
                )),
                true,
                "Yona's account",
                Some("can't reach"),
            ),
            (
                Some((true, joined.clone(), RelayState::Connecting)),
                true,
                "Yona's account",
                Some("connecting"),
            ),
            (
                Some((true, joined.clone(), RelayState::Connected)),
                true,
                "Yona's account",
                None,
            ),
        ];
        for (status, signed_in, summary, aside) in table {
            let mut fixture = CardFixture::ready();
            fixture.account = signed_in.then(|| "Yona".to_string());
            fixture.wifi = status
                .clone()
                .map(|(on, station, relay)| wifi(on, station, relay));
            let bar = cloud_bar(&fixture.input());
            assert_eq!(bar.kind, Some("lightplayer.app"));
            assert_eq!(
                (bar.summary.as_str(), bar.aside.as_deref()),
                (summary, aside),
                "{status:?} signed in: {signed_in}"
            );
        }
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

    /// Joined and lightplayer.app didn't answer: orange, a problem.
    #[test]
    fn cant_reach_is_orange() {
        let mut fixture = CardFixture::ready();
        fixture.account = Some("Yona".to_string());
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
        assert_eq!(line(&bar, "Account").as_deref(), Some("Yona's account"));
        assert_eq!(line(&bar, "Now").as_deref(), Some(relay::NO_INTERNET));
    }

    /// Reached through the relay, the row is the link in use.
    #[test]
    fn a_relay_board_is_the_cloud_rows_and_never_says_bluetooth_or_usb() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Relay);
        fixture.lan = crate::lan_link_for_endpoint("relay:a0f26287b48c");
        fixture.account = Some("Yona".to_string());
        let card = super::super::board_card_builder::board_card(&fixture.input());
        let bar = card.bar(BarLayer::Cloud);
        assert_eq!(
            (bar.summary.as_str(), bar.aside.as_deref()),
            ("Yona's account", Some("connected"))
        );
        assert_eq!(bar.icon, "cloud");
        assert_eq!(card.bar(BarLayer::Usb).summary, "not connected");
        let words = format!("{bar:?}");
        assert!(!words.contains("Bluetooth"), "{words}");
    }

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

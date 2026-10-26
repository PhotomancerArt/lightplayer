//! The Wi‑Fi row: the networks the board joins (`docs/ux/anatomy/board-card.md`,
//! "Wi‑Fi"). Its kind is the link's name, "Wi‑Fi"; its name is the
//! network the board is on, or how its Wi‑Fi stands. Read off the board's
//! own network status — as it answered on this link, else as it last did
//! (an offline board's, dimmed) — never off the link this tab uses: a board
//! on USB says the network it joined.
//!
//! | Board says | Name · state | Action | Tone |
//! |---|---|---|---|
//! | nothing yet (no firmware, not answered, a stand-in, a firmware with no Wi‑Fi and nothing saved) | "—" | — | Neutral |
//! | a firmware that saves networks and can't join yet | "`ssid` · saved" ("2 networks · saved") | — | Neutral |
//! | its Wi‑Fi switch off | "disabled" | — | Neutral |
//! | joining `ssid` | "`ssid`" (work: "joining") | — | Neutral |
//! | joined `ssid` | "`ssid`", and "connected" when this tab's link is Wi‑Fi | — | Neutral |
//! | `ssid` refused its password | "`ssid` · wrong password" | Change (its details) | Attention |
//! | nothing saved | "not connected" | Set up (its details) | Neutral |
//! | saved, none joined (out of range) | "`ssid` · not connected" ("2 networks" for more) | — | Neutral |
//!
//! Nothing saved is a new board's to-do: the row is lit, and Set up opens
//! the details at the network list. Hushing it is turning Wi‑Fi off: the
//! switch at the top of its details (the board's own,
//! `devices/<board>/wifi/enabled`; [`super::row_switch`]), after which the
//! row says "disabled" and wants nothing, and the switch turns it back on.
//!
//! Its details are the Wi‑Fi panel (the board's networks, and adding one),
//! and "Connect over Wi‑Fi" when the primary doesn't already offer it. A
//! Wi‑Fi connect that failed is this row's, red, with Retry
//! ([`super::link_session`]).

use lpc_wire::server::{LastAttempt, NetworkStatus, StationFailure, StationState};

use super::board_card_input::BoardCardInput;
use super::detail_sections::{facts, help};
use super::link_bar::{LinkFacts, connect_verb, link_bar};
use super::link_session::{CONNECTED, DISABLED, NOT_CONNECTED, NOT_KNOWN};
use super::row_switch::{SwitchOffer, row_switch};
use super::ui_bar_work::UiBarWork;
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::app::network::WIFI_ENABLED_PARAM;
use crate::app::network::wifi_words::card;
use crate::{RichLine, UiStatusKind};

/// The Wi‑Fi row.
pub(crate) fn wifi_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let status = input.network_status().filter(|_| !input.stand_in());
    let mut facts = match status {
        Some(status) => station_facts(input, status),
        None => LinkFacts::new("wifi", NOT_KNOWN),
    };
    facts.stale = input.offline() && status.is_some();
    facts.details = details(input, status);
    facts.verbs = connect_verb(input, "connect-wifi", "Connect over Wi\u{2011}Fi", "wifi")
        .into_iter()
        .collect();
    link_bar(input, BarLayer::Wifi, facts)
}

/// The row's facts from the board's status.
fn station_facts(input: &BoardCardInput<'_>, status: &NetworkStatus) -> LinkFacts {
    let row = |summary: &str| LinkFacts::new("wifi", summary);
    let named = |ssid: &str| row(ssid).named();
    // The details' panel can add a network or change a password: the link
    // holds edit, and the board answered on it.
    let can_fix = !input.offline() && input.wifi.is_some_and(|wifi| wifi.can_edit);
    if !status.wifi {
        return row(DISABLED);
    }
    if let Some(ssid) = refused(status) {
        return LinkFacts {
            aside: Some(card::WRONG_PASSWORD.to_string()),
            tone: UiStatusKind::Attention,
            opens_details: can_fix.then_some(card::CHANGE),
            ..named(ssid)
        };
    }
    match &status.station {
        StationState::Off => row(DISABLED),
        // A firmware that saves networks and cannot join them yet: what it
        // holds, "saved".
        StationState::Unsupported => match status.networks.as_slice() {
            [] => row(NOT_KNOWN),
            [only] => LinkFacts {
                aside: Some(card::SAVED.to_string()),
                ..named(&only.ssid)
            },
            many => LinkFacts {
                aside: Some(card::SAVED.to_string()),
                ..named(&card::networks(many.len()))
            },
        },
        StationState::Connecting { ssid, .. } => LinkFacts {
            work: Some(UiBarWork::running(card::JOINING, None)),
            ..named(ssid)
        },
        StationState::Connected { ssid, .. } => LinkFacts {
            aside: input
                .reached_over(UiLinkKind::Wifi)
                .then(|| CONNECTED.to_string()),
            ..named(ssid)
        },
        // Nothing saved: a new board's to-do, until set up or disabled.
        _ if status.networks.is_empty() => LinkFacts {
            opens_details: can_fix.then_some(card::SET_UP),
            todo: can_fix,
            ..row(NOT_CONNECTED)
        },
        StationState::Failed { ssid, .. } => LinkFacts {
            aside: Some(NOT_CONNECTED.to_string()),
            ..named(ssid)
        },
        StationState::NotConnected => match status.networks.as_slice() {
            [only] => LinkFacts {
                aside: Some(NOT_CONNECTED.to_string()),
                ..named(&only.ssid)
            },
            many => LinkFacts {
                aside: Some(NOT_CONNECTED.to_string()),
                ..named(&card::networks(many.len()))
            },
        },
    }
}

/// The saved network that refused its password, now or at its last try.
fn refused(status: &NetworkStatus) -> Option<&str> {
    if let StationState::Failed {
        ssid,
        reason: StationFailure::WrongPassword,
    } = &status.station
    {
        return Some(ssid);
    }
    let joined = matches!(status.station, StationState::Connected { .. });
    status
        .networks
        .iter()
        .find(|network| !joined && network.last == Some(LastAttempt::WrongPassword))
        .map(|network| network.ssid.as_str())
}

/// The row's details: the board's Wi‑Fi switch at the top while it
/// answers, then the Wi‑Fi panel while it is on (on a link that may see
/// it), or "Wi‑Fi is disabled…" while it is off; for a board that is
/// away, what it last said.
fn details(input: &BoardCardInput<'_>, status: Option<&NetworkStatus>) -> UiBarDetails {
    let mut details = UiBarDetails::default();
    let live = input.wifi.filter(|_| !input.offline());
    let on = live.and(status).map(|status| status.wifi);
    details.switch = on.map(|on| {
        row_switch(
            input,
            BarLayer::Wifi,
            "wifi",
            on,
            SwitchOffer {
                under: &["wifi", "enabled"],
                param: WIFI_ENABLED_PARAM,
                turn_off: card::DISABLE_WIFI,
                turn_on: card::ENABLE_WIFI,
            },
        )
        .waiting(input.wifi.is_some_and(|wifi| wifi.writing))
    });
    match (live, on) {
        (Some(_), Some(false)) => {
            details.sections.push(help("", card::WIFI_DISABLED));
        }
        (Some(wifi), _) => {
            details.panels.push(UiDetailPanel::Wifi(wifi.clone()));
        }
        (None, _) => {
            if let Some(StationState::Connected { ssid, ip, .. }) =
                status.map(|status| &status.station)
            {
                details.sections.push(facts(
                    "Last known",
                    vec![
                        RichLine::new("Network", ssid.clone()),
                        RichLine::new("Address", ip.clone()),
                    ],
                ));
            }
        }
    }
    details
}

#[cfg(test)]
mod tests {
    use lpc_wire::server::{ConnectStep, RelayState, SavedNetworkInfo};

    use super::super::card_fixtures::CardFixture;
    use super::*;
    use crate::{UiDeviceWifi, lan_link_for_endpoint};

    /// One test per row of the module's table.
    #[test]
    fn the_wifi_row_says_what_the_board_says() {
        let table: Vec<(Option<StationState>, &[&str], bool, &str, Option<&str>)> = vec![
            (None, &[], true, "\u{2014}", None),
            (Some(StationState::Unsupported), &[], true, "\u{2014}", None),
            (
                Some(StationState::Unsupported),
                &["home", "van"],
                true,
                "2 networks",
                Some("saved"),
            ),
            (Some(StationState::Off), &["home"], false, "disabled", None),
            (
                Some(StationState::Connected {
                    ssid: "home".to_string(),
                    ip: "10.0.0.5".to_string(),
                    rssi: -50,
                    host: "lp-b48c.local".to_string(),
                }),
                &["home"],
                true,
                "home",
                None,
            ),
            (
                Some(StationState::Failed {
                    ssid: "home".to_string(),
                    reason: StationFailure::WrongPassword,
                }),
                &["home"],
                true,
                "home",
                Some("wrong password"),
            ),
            (
                Some(StationState::NotConnected),
                &[],
                true,
                "not connected",
                None,
            ),
            (
                Some(StationState::NotConnected),
                &["home"],
                true,
                "home",
                Some("not connected"),
            ),
            (
                Some(StationState::NotConnected),
                &["home", "van"],
                true,
                "2 networks",
                Some("not connected"),
            ),
        ];
        for (station, saved, on, summary, aside) in table {
            let mut fixture = CardFixture::ready();
            fixture.wifi = Some(wifi(station.clone(), saved, on));
            let bar = wifi_bar(&fixture.input());
            assert_eq!(bar.kind, Some("Wi\u{2011}Fi"));
            assert_eq!(
                (bar.summary.as_str(), bar.aside.as_deref()),
                (summary, aside),
                "{station:?} {saved:?}"
            );
        }
    }

    #[test]
    fn nothing_saved_offers_set_up_and_a_wrong_password_change() {
        let mut fixture = CardFixture::ready();
        fixture.wifi = Some(wifi(Some(StationState::NotConnected), &[], true));
        let bar = wifi_bar(&fixture.input());
        assert_eq!(bar.opens_details, Some("Set up"));
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        assert!(
            bar.details
                .panels
                .iter()
                .any(|panel| matches!(panel, UiDetailPanel::Wifi(_))),
            "the details are the Wi‑Fi panel"
        );

        fixture.wifi = Some(wifi(
            Some(StationState::Failed {
                ssid: "home".to_string(),
                reason: StationFailure::WrongPassword,
            }),
            &["home"],
            true,
        ));
        let bar = wifi_bar(&fixture.input());
        assert_eq!(bar.opens_details, Some("Change"));
        assert_eq!(bar.tone, UiStatusKind::Attention, "a problem: orange");

        // A link that may not change it says the fact and offers no fix.
        let mut play = fixture.clone();
        play.wifi.as_mut().unwrap().can_edit = false;
        assert_eq!(wifi_bar(&play.input()).opens_details, None);
    }

    /// Joining is the board's own work: its network stays the name.
    #[test]
    fn joining_is_the_rows_work_under_the_networks_name() {
        let mut fixture = CardFixture::ready();
        fixture.wifi = Some(wifi(
            Some(StationState::Connecting {
                ssid: "home".to_string(),
                step: ConnectStep::Looking,
            }),
            &["home"],
            true,
        ));
        let bar = wifi_bar(&fixture.input());
        assert_eq!(bar.summary, "home");
        assert_eq!(bar.work, Some(UiBarWork::running("joining", None)));
    }

    /// The link this tab uses is said once: "connected" beside the
    /// network, with how it reaches the board in the details.
    #[test]
    fn the_link_in_use_says_connected_with_its_reach() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Wifi);
        fixture.lan = lan_link_for_endpoint("lan:ws://10.0.0.5/link");
        fixture.wifi = Some(joined());
        let bar = wifi_bar(&fixture.input());
        assert_eq!(
            (bar.summary.as_str(), bar.aside.as_deref()),
            ("home", Some("connected"))
        );
        assert_eq!(line(&bar, "Address").as_deref(), Some("10.0.0.5"));
        assert_eq!(line(&bar, "URL").as_deref(), Some("ws://10.0.0.5/link"));
        let mut usb = CardFixture::ready();
        usb.wifi = Some(joined());
        let bar = wifi_bar(&usb.input());
        assert_eq!(bar.aside, None, "USB is the link in use");
        assert_eq!(line(&bar, "Now"), None);
    }

    /// Offline, the row keeps the network the board last said, dimmed.
    #[test]
    fn an_offline_board_keeps_its_last_network_dimmed() {
        let mut fixture = CardFixture::offline();
        fixture.remembered_network = joined().status;
        fixture.wifi_address = true;
        let bar = wifi_bar(&fixture.input());
        assert_eq!(bar.summary, "home");
        assert!(bar.stale);
        assert_eq!(line(&bar, "Network").as_deref(), Some("home"));
        let words: Vec<&str> = bar
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter().map(|a| a.word.as_str()))
            .collect();
        assert!(
            !words.contains(&"Connect over Wi\u{2011}Fi"),
            "the primary takes Wi‑Fi: {words:?}"
        );
        let mut unknown = CardFixture::offline();
        let bar = wifi_bar(&unknown.input());
        assert_eq!(bar.summary, "\u{2014}");
        assert!(!bar.stale, "nothing to be last known");
    }

    /// Q9: a stand-in has no radio.
    #[test]
    fn a_stand_in_says_a_dash() {
        let mut fixture = CardFixture::ready();
        fixture.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        fixture.wifi = Some(joined());
        assert_eq!(wifi_bar(&fixture.input()).summary, "\u{2014}");
    }

    /// The board's Wi‑Fi switch leads the details: on, over the network
    /// list, it flips off; off, it flips on over "Wi‑Fi is disabled…" and
    /// no list. Offline, there is nothing to flip.
    #[test]
    fn its_details_lead_with_the_wifi_switch() {
        let mut fixture = CardFixture::ready();
        fixture.wifi = Some(joined());
        let details = wifi_bar(&fixture.input()).details;
        let switch = details.switch.expect("a switch at the top");
        assert_eq!(
            (switch.label.as_str(), switch.icon.as_str(), switch.on),
            ("Wi\u{2011}Fi", "wifi", true)
        );
        let flip = switch.flip.expect("this link may flip it");
        assert_eq!(flip.word, card::DISABLE_WIFI);
        assert!(flip.offer.to_string().ends_with("/wifi/enabled"));
        assert_eq!(flip.args.get(WIFI_ENABLED_PARAM), Some("false"));
        assert!(matches!(details.panels[..], [UiDetailPanel::Wifi(_)]));

        fixture.wifi = Some(wifi(Some(StationState::Off), &["home"], false));
        let details = wifi_bar(&fixture.input()).details;
        let switch = details.switch.expect("a switch at the top");
        assert!(!switch.on);
        assert_eq!(switch.flip.expect("flips on").word, card::ENABLE_WIFI);
        let sentences: Vec<&str> = details
            .sections
            .iter()
            .filter_map(|section| section.sentence.as_deref())
            .collect();
        assert_eq!(sentences, [card::WIFI_DISABLED]);
        assert!(details.panels.is_empty());

        let mut fixture = CardFixture::offline();
        fixture.wifi = Some(joined());
        assert!(wifi_bar(&fixture.input()).details.switch.is_none());
    }

    fn joined() -> UiDeviceWifi {
        wifi(
            Some(StationState::Connected {
                ssid: "home".to_string(),
                ip: "10.0.0.5".to_string(),
                rssi: -50,
                host: "lp-b48c.local".to_string(),
            }),
            &["home"],
            true,
        )
    }

    fn wifi(station: Option<StationState>, saved: &[&str], on: bool) -> UiDeviceWifi {
        UiDeviceWifi {
            status: station.map(|station| NetworkStatus {
                wifi: on,
                cloud_relay: true,
                networks: saved
                    .iter()
                    .map(|ssid| SavedNetworkInfo {
                        ssid: ssid.to_string(),
                        has_password: true,
                        hidden: false,
                        last: None,
                    })
                    .collect(),
                station,
                relay: RelayState::Off,
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

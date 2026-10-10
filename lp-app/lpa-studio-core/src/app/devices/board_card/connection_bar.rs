//! The connection bar: how the board is reached, and how that is going.
//!
//! The link is the card's input, read off the roster `Device`'s endpoint
//! (`UiLinkKind::of_endpoint`) — never the `DeviceView`'s over-Bluetooth
//! predicate or `UiDeviceAccess::over_bluetooth`, which have both named a LAN or relay
//! board "Bluetooth" before (the 2026-10-08 defects). Its words are the
//! link's own label ("USB", "Bluetooth", "Wi‑Fi", "Wi‑Fi via
//! lightplayer.app"); its icon `usb`, `bluetooth`, `wifi`, or `cloud` for a
//! board reached through lightplayer.app.
//!
//! First match wins:
//!
//! | Board | Summary | Tone | Action / work |
//! |---|---|---|---|
//! | Identifying, or a Wi‑Fi or relay connect under way | (the work shows) | Neutral | the work's Cancel |
//! | A Wi‑Fi connect turned away busy | "Someone else connected" | Attention | Retry (the same road) |
//! | A take-over under way (asking the other tab, opening the board) | (the work shows) | Neutral | — |
//! | Another tab holds the board | "Open in another tab" ("editor open", or what it is busy with) | Attention | — (Connect, the primary, is the take-over) |
//! | …a tab that let go on request | "Taken by another tab" | Attention | — |
//! | …a take-over that failed | (the failure, striped) | — | Retry (`take-over`) |
//! | A Wi‑Fi connect that failed otherwise | (the failure, striped) | — | Retry |
//! | A stand-in, linked | "In this tab · live" | Neutral | — |
//! | A stand-in, off | "In this tab · off" | Neutral | — (Power on is the primary) |
//! | Not responding | "<link> · not responding" | Warning | Retry |
//! | Port there, closed | "<link> · not connected" | Neutral | — (Connect is the primary) |
//! | Offline | "Offline · <how long>" | Neutral | — (the primary reconnects) |
//! | Linked, the picture live | "<link> · live" | Neutral | — |
//! | Linked | "<link> · connected" | Neutral | — |
//!
//! Work and refusals first (they are news), then what the board is, then
//! how well it answers. "Someone else connected" is for a holder Studio
//! cannot name (a stranger on the network slot); "Open in another tab" is a
//! tab of this browser (`docs/style/language.md`). The held words are
//! [`super::held_board`]'s. The aside says whether the board is also reached
//! through lightplayer.app ("also cloud") or not ("direct only") — once its
//! own Wi‑Fi status has been read (R20), and never on a relay link, which
//! is the cloud.

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::PendingLinkView;

use super::bar_work::bar_work;
use super::board_card_input::{BoardCardInput, link_icon};
use super::detail_sections::{facts, notice, verbs, without_empty};
use super::held_board::{held_aside, held_sentence, held_summary, take_over_work};
use super::primary_action::{offline_words, primary_action};
use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_bluetooth_switch::bluetooth_switch;
use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;
use super::ui_name_bar::UiPrimary;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::age_words::age_words;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::app::devices::wifi_connects::UiWifiConnect;
use crate::app::network::WifiTone;
use crate::{ActionConsequence, RichLine, UiStatusKind};

/// The busy notice's words: another device holds the board's one network
/// connection.
pub const SOMEONE_ELSE_SENTENCE: &str =
    "Another device holds this board's network connection. Try again when it lets go.";

/// The connection bar.
pub(crate) fn connection_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let view = input.view;
    let link = input.link_kind();
    let label = link.label();
    let mut icon = link_icon(link).to_string();
    let busy = input.wifi_connect.filter(|connect| connect.busy);
    let held = input.held();
    let (summary, tone, action) = if let Some(connect) = busy {
        icon = link_icon(connect_link(connect.through_relay)).to_string();
        (
            "Someone else connected".to_string(),
            UiStatusKind::Attention,
            busy_retry(input, connect),
        )
    } else if let Some(held) = held {
        icon = link_icon(input.held_link()).to_string();
        (
            held_summary(held).to_string(),
            UiStatusKind::Attention,
            None,
        )
    } else if input.stand_in() {
        icon = "play".to_string();
        let state = match input.linked() {
            true => "live",
            false => "off",
        };
        (
            format!("In this tab · {state}"),
            UiStatusKind::Neutral,
            None,
        )
    } else {
        match view.status {
            DeviceStatus::NotResponding => (
                format!("{label} · not responding"),
                UiStatusKind::Warning,
                input
                    .offer("retry")
                    .map(|retry| UiCardAction::press(retry, "Retry").with_icon("retry")),
            ),
            DeviceStatus::Attached => (
                format!("{label} · not connected"),
                UiStatusKind::Neutral,
                None,
            ),
            DeviceStatus::Offline => (
                offline_words(input.last_seen_at, input.now),
                UiStatusKind::Neutral,
                None,
            ),
            _ if input.linked() && input.feed.is_some_and(|feed| feed_is_live(feed.liveness)) => {
                (format!("{label} · live"), UiStatusKind::Neutral, None)
            }
            _ if input.linked() => (format!("{label} · connected"), UiStatusKind::Neutral, None),
            _ => (
                format!("{label} · not connected"),
                UiStatusKind::Neutral,
                None,
            ),
        }
    };
    let work = bar_work(input, BarLayer::Connection)
        .or_else(|| take_over_work(input))
        .or_else(|| failed_connect(input));
    let quiet = work.is_some();
    let (aside, aside_icon) = match held {
        Some(held) => (held_aside(held), None),
        None => cloud_aside(input, link),
    };
    UiStackBar {
        layer: BarLayer::Connection,
        icon,
        details: details(input, &summary, tone),
        summary,
        aside,
        aside_icon,
        tone: if quiet { UiStatusKind::Neutral } else { tone },
        action: if quiet { None } else { action },
        work,
    }
}

/// A new board's connection bar: the link it arrived on, "new", and — while
/// it is still saying who it is — that as the bar's work.
pub(crate) fn pending_connection_bar(pending: &PendingLinkView, link: UiLinkKind) -> UiStackBar {
    let state = match pending.detail.as_deref() {
        Some(detail) => format!("{} · {detail}", pending.state_label),
        None => pending.state_label.clone(),
    };
    let identifying = pending.firmware_face == lpa_devices::view::FirmwareFace::Unknown;
    UiStackBar {
        layer: BarLayer::Connection,
        icon: link_icon(link).to_string(),
        summary: format!("{} · new", link.label()),
        aside: None,
        aside_icon: None,
        tone: UiStatusKind::Neutral,
        action: None,
        work: identifying.then(|| UiBarWork {
            words: pending.state_label.clone(),
            percent: None,
            state: BarWorkState::Running,
            cancel: None,
            other_device: false,
        }),
        details: UiBarDetails {
            sections: vec![facts("Reach", vec![RichLine::new("State", state)])],
            panels: Vec::new(),
            raised: false,
        },
    }
}

/// Retry for a connect turned away busy: the same road again. The holder is
/// someone Studio cannot name, so there is no take-over to offer.
fn busy_retry(input: &BoardCardInput<'_>, connect: &UiWifiConnect) -> Option<UiCardAction> {
    let verb = match connect.through_relay {
        true => "connect-relay",
        false => "connect-wifi",
    };
    input
        .offer(verb)
        .map(|offer| UiCardAction::press(offer, "Retry").with_icon("retry"))
}

/// A Wi‑Fi or relay connect that failed (not turned away busy): the bar's
/// failure, striped, with Retry on the same road.
fn failed_connect(input: &BoardCardInput<'_>) -> Option<UiBarWork> {
    let connect = input
        .wifi_connect
        .filter(|connect| !connect.busy && !connect.connecting)?;
    let error = connect.error.clone()?;
    let verb = match connect.through_relay {
        true => "connect-relay",
        false => "connect-wifi",
    };
    Some(UiBarWork {
        words: error,
        percent: None,
        state: BarWorkState::Failed {
            retry: input
                .offer(verb)
                .map(|offer| UiCardAction::press(offer, "Retry").with_icon("retry")),
        },
        cancel: None,
        other_device: false,
    })
}

/// "also cloud" (with the cloud icon) or "direct only", once the board's
/// own Wi‑Fi status says whether Cloud relay is on; never on a relay link.
fn cloud_aside(input: &BoardCardInput<'_>, link: UiLinkKind) -> (Option<String>, Option<String>) {
    if link == UiLinkKind::Relay {
        return (None, None);
    }
    match input.wifi.and_then(|wifi| wifi.status.as_ref()) {
        Some(status) if status.cloud_relay => {
            (Some("also cloud".to_string()), Some("cloud".to_string()))
        }
        Some(_) => (Some("direct only".to_string()), None),
        None => (None, None),
    }
}

/// The connection bar's details: the notice, how the board is reached, its
/// links, its panels, and the other ways to reach it.
fn details(input: &BoardCardInput<'_>, summary: &str, tone: UiStatusKind) -> UiBarDetails {
    let view = input.view;
    let link = input.link_kind();
    let mut sections = Vec::new();
    let held = input.held();
    match tone {
        UiStatusKind::Attention => {
            if let Some(connect) = input.wifi_connect.filter(|connect| connect.busy) {
                sections.push(notice(
                    "Connection",
                    tone,
                    SOMEONE_ELSE_SENTENCE,
                    busy_retry(input, connect),
                ));
            } else if let Some(held) = held {
                let undoable = input
                    .offer("take-over")
                    .is_some_and(|offer| *offer.consequence() == ActionConsequence::Undoable);
                sections.push(notice(
                    "Connection",
                    tone,
                    held_sentence(held, undoable),
                    None,
                ));
            }
        }
        UiStatusKind::Warning => {
            let sentence = match &view.detail {
                Some(detail) => format!("{} — {detail}", view.state_label),
                None => view.state_label.clone(),
            };
            sections.push(notice(
                "Connection",
                tone,
                sentence,
                input
                    .offer("retry")
                    .map(|retry| UiCardAction::press(retry, "Retry").with_icon("retry")),
            ));
        }
        _ => {}
    }

    let mut reach = vec![RichLine::new("Now", summary)];
    if let Some(lan) = input.lan.filter(|lan| !lan.address.is_empty()) {
        reach.push(RichLine::new("Address", lan.address.clone()));
        reach.push(RichLine::new("URL", lan.url.clone()));
    }
    let heard = view
        .freshness_label
        .clone()
        .or_else(|| {
            input
                .last_seen_at
                .filter(|_| input.offline())
                .map(|seen| age_words(input.now - seen))
        })
        .unwrap_or_else(|| "not heard this session".to_string());
    reach.push(RichLine::new("Last heard", heard));
    if let Some(detail) = &view.detail {
        reach.push(RichLine::new("Detail", detail.clone()));
    }
    sections.push(facts("Reach", reach));

    let mut links = Vec::new();
    if link == UiLinkKind::Usb && !input.stand_in() {
        links.push(RichLine::new(
            "USB",
            match input.linked() {
                true => "connected",
                false => "not connected",
            },
        ));
    }
    if let Some(wifi) = input.wifi {
        let value = wifi.row_value();
        if !value.is_empty() {
            links.push(
                RichLine::new("Wi\u{2011}Fi", value).toned(match wifi.row_tone() {
                    WifiTone::Plain => UiStatusKind::Neutral,
                    WifiTone::Good => UiStatusKind::Good,
                    WifiTone::Warn => UiStatusKind::Warning,
                }),
            );
        }
    }
    sections.push(facts("Links", links));

    let mut panels = Vec::new();
    if let Some(access) = input
        .access
        .filter(|access| access.panel.is_some() || link == UiLinkKind::Bluetooth)
    {
        panels.push(UiDetailPanel::Bluetooth(bluetooth_switch(access)));
    }
    if let Some(wifi) = input.wifi {
        panels.push(UiDetailPanel::Wifi(wifi.clone()));
    }
    if let Some(counters) = view.link_counters {
        panels.push(UiDetailPanel::LinkCounters(counters));
    }

    let primary = match primary_action(input, None) {
        Some(UiPrimary::Offer(action)) => Some(action.offer),
        _ => None,
    };
    let mut actions = Vec::new();
    if let Some(identify) = input.offer("identify") {
        actions.push(UiCardAction::press(identify, "Identify again").with_icon("info"));
    }
    for (verb, word, icon) in [
        ("reconnect", "Connect", link_icon(link)),
        ("connect-wifi", "Connect over Wi\u{2011}Fi", "wifi"),
        ("connect-relay", "Connect through lightplayer.app", "cloud"),
    ] {
        // A board another tab holds is reached by the take-over (the
        // primary); each of these would fight that tab for the board.
        if held.is_none()
            && let Some(offer) = input
                .offer(verb)
                .filter(|offer| Some(&offer.path) != primary.as_ref())
        {
            actions.push(UiCardAction::press(offer, word).with_icon(icon));
        }
    }
    if let Some(disconnect) = input.offer("disconnect") {
        actions.push(UiCardAction::own_words(disconnect).with_icon(disconnect.icon.clone()));
    }
    sections.push(verbs(actions));
    UiBarDetails {
        sections: without_empty(sections),
        panels,
        raised: false,
    }
}

/// The link a Wi‑Fi connect takes: the LAN, or lightplayer.app's relay.
fn connect_link(through_relay: bool) -> UiLinkKind {
    match through_relay {
        true => UiLinkKind::Relay,
        false => UiLinkKind::Wifi,
    }
}

/// A picture coming over the link right now (or the editor's lens holding
/// it): the board is live.
fn feed_is_live(liveness: crate::FeedLiveness) -> bool {
    matches!(
        liveness,
        crate::FeedLiveness::Live | crate::FeedLiveness::Lens
    )
}

#[cfg(test)]
mod tests {
    use lpa_devices::view::Escape;
    use lpa_devices::{ActivityKind, HoldLevel, HoldVia};
    use lpc_wire::server::{NetworkStatus, RelayState, StationState};

    use super::super::card_fixtures::{CardFixture, activity, feed};
    use super::*;
    use crate::app::devices::take_over_state::UiTakeOver;
    use crate::{FeedLiveness, UiDeviceWifi, UiWifiConnect, lan_link_for_endpoint};

    #[test]
    fn identifying_is_the_bars_work_with_its_cancel() {
        let mut fixture = CardFixture::ready().with_activity(activity(
            ActivityKind::Identify,
            "Identifying…",
            None,
        ));
        let bar = connection_bar(&fixture.input());
        let work = bar.work.expect("identify");
        assert_eq!(work.words, "Identifying…");
        assert!(work.cancel.is_some());
        assert_eq!(bar.action, None);
    }

    #[test]
    fn a_connect_turned_away_busy_says_someone_else_connected() {
        let mut fixture = CardFixture::offline();
        fixture.wifi_address = true;
        fixture.wifi_connect = Some(failed_connect_words(true));
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.summary, "Someone else connected");
        assert_eq!(bar.tone, UiStatusKind::Attention);
        assert_eq!(bar.work, None);
        // A stranger's busy offers Retry on the same road, and nothing
        // more: taking a board from another person is sharing's question.
        let retry = bar.action.clone().expect("Retry");
        assert_eq!(retry.word, "Retry");
        assert!(retry.offer.to_string().ends_with("/connect-wifi"));
        let notice = bar.details.notice().expect("the notice");
        assert_eq!(notice.sentence.as_deref(), Some(SOMEONE_ELSE_SENTENCE));
        assert_eq!(notice.affordances, vec![retry]);
        assert!(
            fixture
                .offers()
                .iter()
                .all(|offer| !offer.path.to_string().ends_with("/take-over")),
            "no take-over from a stranger"
        );
    }

    #[test]
    fn a_relay_connect_turned_away_busy_retries_through_the_cloud() {
        let mut fixture = CardFixture::offline();
        fixture.relay = true;
        let mut connect = failed_connect_words(true);
        connect.through_relay = true;
        fixture.wifi_connect = Some(connect);
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.summary, "Someone else connected");
        assert_eq!(bar.icon, "cloud");
        let retry = bar.action.expect("Retry");
        assert!(retry.offer.to_string().ends_with("/connect-relay"));
    }

    #[test]
    fn a_board_another_tab_holds_is_open_in_another_tab() {
        for (level, aside) in [
            (HoldLevel::Watching, None),
            (HoldLevel::Open, Some("editor open")),
            (
                HoldLevel::Busy("Updating \u{b7} 42%".to_string()),
                Some("Updating \u{b7} 42%"),
            ),
        ] {
            let mut fixture = CardFixture::held(level.clone());
            let bar = connection_bar(&fixture.input());
            assert_eq!(bar.summary, "Open in another tab", "{level:?}");
            assert_eq!(bar.aside.as_deref(), aside, "{level:?}");
            assert_eq!(bar.aside_icon, None);
            assert_eq!(bar.icon, "usb");
            assert_eq!(bar.tone, UiStatusKind::Attention, "orange: someone has it");
            assert_eq!(bar.work, None);
            assert_eq!(bar.action, None, "the primary carries Connect");
        }
    }

    #[test]
    fn its_details_name_the_facts_and_what_taking_it_costs() {
        let mut watching = CardFixture::held(HoldLevel::Watching);
        let bar = connection_bar(&watching.input());
        let notice = bar.details.notice().expect("a notice");
        assert_eq!(notice.tone, UiStatusKind::Attention);
        assert_eq!(
            notice.sentence.as_deref(),
            Some("Another tab of this browser has this board open.")
        );

        let mut open = CardFixture::held(HoldLevel::Open);
        let bar = connection_bar(&open.input());
        assert_eq!(
            bar.details
                .notice()
                .and_then(|notice| notice.sentence.as_deref()),
            Some(
                "Another tab of this browser has this board open, and its editor is open. \
                 Taking it over closes it there. You can take it back the same way."
            )
        );

        let mut busy = CardFixture::held(HoldLevel::Busy("Pushing".to_string()));
        let bar = connection_bar(&busy.input());
        assert_eq!(
            bar.details
                .notice()
                .and_then(|notice| notice.sentence.as_deref()),
            Some(
                "Another tab of this browser has this board open. Busy in the other tab: Pushing."
            )
        );
    }

    #[test]
    fn a_tab_that_let_go_on_request_says_taken_by_another_tab() {
        let mut fixture = CardFixture::held(HoldLevel::Open);
        fixture
            .view
            .held_elsewhere
            .as_mut()
            .unwrap()
            .taken_from_here = true;
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.summary, "Taken by another tab");
        assert_eq!(bar.aside, None);
        assert_eq!(bar.tone, UiStatusKind::Attention);
        assert_eq!(bar.action, None);
    }

    #[test]
    fn a_take_over_under_way_is_the_bars_work_and_the_summary_stays() {
        for words in ["Asking the other tab\u{2026}", "Opening\u{2026}"] {
            let mut fixture = CardFixture::held(HoldLevel::Watching);
            fixture.take_over = Some(UiTakeOver {
                words: words.to_string(),
                failed: false,
            });
            let bar = connection_bar(&fixture.input());
            let work = bar.work.expect("the take-over is work");
            assert_eq!(work.words, words);
            assert_eq!(work.state, BarWorkState::Running);
            assert_eq!(work.cancel, None);
            assert_eq!(bar.summary, "Open in another tab", "unchanged");
            // The bar reads neutral while it works (`ui.md`, D34).
            assert_eq!(bar.tone, UiStatusKind::Neutral);
            assert_eq!(bar.action, None);
        }
    }

    #[test]
    fn a_take_over_that_failed_is_striped_with_retry_on_the_take_over() {
        let mut fixture = CardFixture::held(HoldLevel::Watching);
        fixture.take_over = Some(UiTakeOver {
            words: "That tab didn't answer".to_string(),
            failed: true,
        });
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.summary, "Open in another tab");
        let work = bar.work.expect("the failure");
        assert_eq!(work.words, "That tab didn't answer");
        let BarWorkState::Failed { retry: Some(retry) } = work.state else {
            panic!("Retry");
        };
        assert_eq!(retry.word, "Retry");
        assert!(retry.offer.to_string().ends_with("/take-over"));
        assert_eq!(retry.refused, None);
    }

    #[test]
    fn a_board_held_on_the_network_reads_the_same_with_the_network_link() {
        let mut fixture = CardFixture::held(HoldLevel::Watching).over(UiLinkKind::Wifi);
        fixture.view.held_elsewhere.as_mut().unwrap().via = HoldVia::Network;
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.summary, "Open in another tab");
        assert_eq!(bar.icon, "wifi");
        assert_eq!(bar.tone, UiStatusKind::Attention);
    }

    #[test]
    fn a_held_board_offers_no_second_way_to_connect() {
        let mut fixture = CardFixture::held(HoldLevel::Watching);
        fixture.wifi_address = true;
        fixture.relay = true;
        let bar = connection_bar(&fixture.input());
        let words: Vec<String> = bar
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter().map(|action| action.word.clone()))
            .collect();
        assert!(
            !words.iter().any(|word| word.starts_with("Connect")),
            "Connect is the take-over: {words:?}"
        );
    }

    #[test]
    fn a_connect_that_failed_is_striped_with_retry() {
        let mut fixture = CardFixture::offline();
        fixture.wifi_address = true;
        fixture.wifi_connect = Some(failed_connect_words(false));
        let bar = connection_bar(&fixture.input());
        let work = bar.work.expect("the failure");
        assert_eq!(
            work.words,
            "Couldn't reach the board at 10.0.0.5. Is it on this network?"
        );
        let BarWorkState::Failed { retry: Some(retry) } = work.state else {
            panic!("Retry");
        };
        assert_eq!(retry.word, "Retry");
        assert!(retry.offer.to_string().ends_with("/connect-wifi"));
    }

    #[test]
    fn a_stand_in_is_in_this_tab_live_or_off() {
        let mut fixture = CardFixture::ready();
        fixture.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.summary, "In this tab · live");
        assert_eq!(bar.icon, "play");
        let mut off = CardFixture::offline();
        off.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        assert_eq!(connection_bar(&off.input()).summary, "In this tab · off");
    }

    #[test]
    fn a_board_not_responding_offers_retry() {
        let mut fixture = CardFixture::ready();
        fixture.view.status = DeviceStatus::NotResponding;
        fixture.view.state_label = "Not responding".to_string();
        fixture.view.detail = Some("no answer after 3 s".to_string());
        fixture.view.firmware_face = lpa_devices::view::FirmwareFace::Unknown;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        fixture.view.loaded_project = lpa_devices::view::LoadedProject::Unknown;
        fixture.view.escapes = vec![Escape::Retry, Escape::Disconnect, Escape::Forget];
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.summary, "USB · not responding");
        assert_eq!(bar.tone, UiStatusKind::Warning);
        let retry = bar.action.expect("Retry");
        assert_eq!(retry.word, "Retry");
        assert!(retry.offer.to_string().ends_with("/retry"));
        assert_eq!(
            bar.details
                .notice()
                .and_then(|notice| notice.sentence.as_deref()),
            Some("Not responding — no answer after 3 s")
        );
    }

    #[test]
    fn a_closed_port_is_not_connected_and_an_offline_board_says_how_long() {
        let mut attached = CardFixture::ready();
        attached.view.status = DeviceStatus::Attached;
        attached.view.can_receive_project = false;
        attached.view.can_remove_project = false;
        assert_eq!(
            connection_bar(&attached.input()).summary,
            "USB · not connected"
        );
        let mut offline = CardFixture::offline();
        offline.last_seen_at = Some(offline.now - 15.0 * 86_400.0);
        let bar = connection_bar(&offline.input());
        assert_eq!(bar.summary, "Offline · 2 weeks");
        assert_eq!(line(&bar, "Last heard").as_deref(), Some("2 weeks ago"));
        offline.last_seen_at = None;
        let bar = connection_bar(&offline.input());
        assert_eq!(bar.summary, "Offline");
        assert_eq!(
            line(&bar, "Last heard").as_deref(),
            Some("not heard this session")
        );
    }

    #[test]
    fn a_linked_board_is_live_with_its_picture_else_connected() {
        let mut fixture = CardFixture::ready();
        assert_eq!(connection_bar(&fixture.input()).summary, "USB · connected");
        fixture.feed = Some(feed(FeedLiveness::Live, true));
        assert_eq!(connection_bar(&fixture.input()).summary, "USB · live");
        let mut ble = CardFixture::ready().over(UiLinkKind::Bluetooth);
        let bar = connection_bar(&ble.input());
        assert_eq!(bar.summary, "Bluetooth · connected");
        assert_eq!(bar.icon, "bluetooth");
    }

    /// Ported: a board on the LAN says how it is reached, and its address
    /// and URL are in details.
    #[test]
    fn a_wifi_board_leads_its_info_line_with_how_it_is_reached() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Wifi);
        fixture.lan = lan_link_for_endpoint("lan:ws://10.0.0.5/link");
        fixture.feed = Some(feed(FeedLiveness::Live, true));
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.summary, "Wi\u{2011}Fi · live");
        assert_eq!(bar.icon, "wifi");
        assert_eq!(line(&bar, "Address").as_deref(), Some("10.0.0.5"));
        assert_eq!(line(&bar, "URL").as_deref(), Some("ws://10.0.0.5/link"));
        assert_eq!(
            line(&bar, "USB"),
            None,
            "the USB row is the USB link's alone"
        );
    }

    #[test]
    fn a_relay_board_never_says_bluetooth_or_usb() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Relay);
        fixture.lan = lan_link_for_endpoint("relay:a0f26287b48c");
        fixture.wifi = Some(wifi_with_relay(true));
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.summary, "Wi\u{2011}Fi via lightplayer.app · connected");
        assert_eq!(bar.icon, "cloud");
        assert_eq!(bar.aside, None, "a relay link is the cloud");
        let words = format!("{bar:?}");
        assert!(!words.contains("Bluetooth"), "{words}");
        assert!(!words.contains("\"USB\""), "{words}");
    }

    /// Ported: the device line read freshness, then the detail; "Last
    /// heard" and "Detail" now.
    #[test]
    fn the_device_line_reads_freshness_then_detail() {
        let mut fixture = CardFixture::ready();
        let bar = connection_bar(&fixture.input());
        assert_eq!(
            line(&bar, "Last heard").as_deref(),
            Some("not heard this session")
        );
        assert_eq!(
            line(&bar, "Detail").as_deref(),
            Some("LightPlayer · seeed/xiao-esp32-c6")
        );
        fixture.view.freshness_label = Some("quiet — last heard 12 s ago".to_string());
        assert_eq!(
            line(&connection_bar(&fixture.input()), "Last heard").as_deref(),
            Some("quiet — last heard 12 s ago")
        );
    }

    #[test]
    fn the_aside_says_also_cloud_or_direct_only_once_the_board_said() {
        let mut fixture = CardFixture::ready();
        fixture.wifi = Some(UiDeviceWifi::new(fixture.view.id, true));
        assert_eq!(connection_bar(&fixture.input()).aside, None, "not read yet");
        fixture.wifi = Some(wifi_with_relay(true));
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.aside.as_deref(), Some("also cloud"));
        assert_eq!(bar.aside_icon.as_deref(), Some("cloud"));
        fixture.wifi = Some(wifi_with_relay(false));
        let bar = connection_bar(&fixture.input());
        assert_eq!(bar.aside.as_deref(), Some("direct only"));
        assert_eq!(bar.aside_icon, None);
        assert_eq!(line(&bar, "Wi\u{2011}Fi").as_deref(), Some("home"));
        assert!(
            bar.details
                .panels
                .iter()
                .any(|panel| matches!(panel, UiDetailPanel::Wifi(_)))
        );
    }

    #[test]
    fn its_details_hold_the_ways_to_reach_it_and_disconnect() {
        let mut fixture = CardFixture::ready();
        let bar = connection_bar(&fixture.input());
        let words: Vec<String> = bar
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter().map(|action| action.word.clone()))
            .collect();
        assert_eq!(words, ["Identify again", "Disconnect"]);
        assert_eq!(line(&bar, "USB").as_deref(), Some("connected"));

        // Offline with both roads: the primary takes Wi‑Fi; the cable and
        // the cloud stay in details.
        let mut offline = CardFixture::offline();
        offline.wifi_address = true;
        offline.relay = true;
        let words: Vec<String> = connection_bar(&offline.input())
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter().map(|action| action.word.clone()))
            .collect();
        assert_eq!(words, ["Connect", "Connect through lightplayer.app"]);
    }

    fn failed_connect_words(busy: bool) -> UiWifiConnect {
        UiWifiConnect {
            host: "10.0.0.5".to_string(),
            through_relay: false,
            connecting: false,
            error: Some(match busy {
                true => crate::WIFI_BUSY_WORDS.to_string(),
                false => "Couldn't reach the board at 10.0.0.5. Is it on this network?".to_string(),
            }),
            busy,
        }
    }

    fn wifi_with_relay(cloud_relay: bool) -> UiDeviceWifi {
        UiDeviceWifi {
            status: Some(NetworkStatus {
                wifi: true,
                cloud_relay,
                networks: Vec::new(),
                station: StationState::Connected {
                    ssid: "home".to_string(),
                    ip: "10.0.0.5".to_string(),
                    rssi: -50,
                    host: "lp-b48c.local".to_string(),
                },
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

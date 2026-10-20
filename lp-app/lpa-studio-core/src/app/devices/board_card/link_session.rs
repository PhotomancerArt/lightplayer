//! This tab's session with the board, said on the row of the link it is
//! about (`docs/ux/anatomy/board-card.md`, the link rows). Four link rows
//! each say their own link; what this tab is doing about reaching the board
//! — connecting, reconnecting, a Connect that failed, a board that stopped
//! answering, another tab or someone else holding it — belongs to the row of
//! the link it happens on, never to USB by default: a Wi‑Fi connect that
//! failed is the Wi‑Fi row's, a board held on its network slot the
//! lightplayer.app or Wi‑Fi row's.
//!
//! The link is the card's input, read off the roster `Device`'s endpoint
//! (`UiLinkKind::of_endpoint`) — never the `DeviceView`'s over-Bluetooth
//! predicate or `UiDeviceAccess::over_bluetooth`, which have both named a
//! LAN or relay board "Bluetooth" before (the 2026-10-08 defects).
//!
//! First match wins, on the row of the link each is about:
//!
//! | Board | Row says | Tone | Action / work |
//! |---|---|---|---|
//! | A Wi‑Fi or relay connect under way | (work: "connecting") | Neutral | — |
//! | A Wi‑Fi or relay connect turned away busy | "Someone else connected" | Attention | Retry (the same road) |
//! | A take-over under way (asking the other tab, opening the board) | (work: its words) | Neutral | — |
//! | A Connect under way | (work: "connecting") | Neutral | — |
//! | The connected session's link dropped, held for the board | (work: "reconnecting") | Neutral | — |
//! | The last Connect failed | (failed: "connection failed"; the reason in the details) | Error | Retry (`connect`) |
//! | Another tab holds the board | "Open in another tab" ("editor open", or what it is busy with) | Attention | — (Connect, the primary, is the take-over) |
//! | …a tab that let go on request | "Taken by another tab" | Attention | — |
//! | …a take-over that failed | (failed: its words) | Error | Retry (`take-over`) |
//! | A Wi‑Fi or relay connect that failed otherwise | (failed: "connection failed"; why in the details) | Error | Retry |
//! | Not responding (a board that said hello went quiet) | "not responding" | Attention | Retry |
//!
//! Work and refusals first (they are news), then who holds the board, then
//! how well it answers. A device that needs you is orange (Attention); a
//! connection that failed is red (Error); nothing here is yellow, the
//! unsaved edit's colour (P16) — and while the card's surface says the
//! row's problem, the row goes quiet ([`super::board_surface::carried_layer`]).
//! "Someone else connected" is for a holder Studio cannot name (a stranger
//! on the network slot); "Open in another tab" is a tab of this browser
//! (`docs/style/language.md`). The held words are [`super::held_board`]'s.

use lpa_devices::device::DeviceStatus;

use super::board_card_input::BoardCardInput;
use super::board_connection::BoardConnection;
use super::detail_sections::{facts, notice};
use super::held_board::{held_aside, held_sentence, held_summary, take_over_work};
use super::primary_action::CONNECTING;
use super::ui_bar_work::UiBarWork;
use super::ui_card_action::UiCardAction;
use super::ui_stack_bar::BarLayer;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::app::devices::wifi_connects::UiWifiConnect;
use crate::core::data_kinds::age_words;
use crate::{ActionConsequence, RichLine, RichSection, UiStatusKind};

/// The busy notice's words: another device holds the board's one network
/// connection.
pub const SOMEONE_ELSE_SENTENCE: &str =
    "Another device holds this board's network connection. Try again when it lets go.";

/// A link row's work while a connected board's dropped link is held for it
/// to come back.
pub const RECONNECTING: &str = "reconnecting";

/// A link row's state when the last Connect did not open, red; the reason
/// is in the row's details.
pub const CONNECTION_FAILED: &str = "connection failed";

/// A link row's state for a link that doesn't reach the board (USB,
/// Wi‑Fi and lightplayer.app alike): the fact, never the fix.
pub const NOT_CONNECTED: &str = "not connected";

/// The USB row's state while this tab reaches the board by its cable, and
/// the aside on the row of the link in use.
pub const CONNECTED: &str = "connected";

/// A link row's state for a board that said hello and went quiet.
pub const NOT_RESPONDING: &str = "not responding";

/// A link row's state while Studio knows nothing of that link yet (no
/// firmware, not answered): a dash, dimmed.
pub const NOT_KNOWN: &str = "\u{2014}";

/// A radio the board has turned off: Wi‑Fi, Bluetooth or lightplayer.app
/// hushed from its row's details.
pub const DISABLED: &str = "disabled";

/// What this tab's session says on a link row, in place of the row's own
/// facts: who holds the board, or that it stopped answering.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SessionLine {
    pub summary: String,
    pub aside: Option<String>,
    pub tone: UiStatusKind,
    pub action: Option<UiCardAction>,
}

/// The row of the link a session is about: a network connect's while one
/// is under way or has failed, else the link in use's (or the last one's).
pub(crate) fn session_layer(input: &BoardCardInput<'_>) -> BarLayer {
    match input.wifi_connect.filter(|connect| connect.connecting) {
        Some(connect) => BarLayer::of_link(connect_link(connect.through_relay)),
        None => BarLayer::of_link(input.link_kind()),
    }
}

/// The row a board another tab holds is said on: the hold's own way in.
pub(crate) fn held_layer(input: &BoardCardInput<'_>) -> BarLayer {
    BarLayer::of_link(input.held_link())
}

/// The row a network connect's news is said on: lightplayer.app's for one
/// through the relay, else Wi‑Fi's.
pub(crate) fn connect_layer(connect: &UiWifiConnect) -> BarLayer {
    BarLayer::of_link(connect_link(connect.through_relay))
}

/// What the session says on `layer`'s row in place of its facts, when it
/// is about that link: someone else holds its network slot, another tab
/// holds the board, or the board stopped answering on it.
pub(crate) fn session_line(input: &BoardCardInput<'_>, layer: BarLayer) -> Option<SessionLine> {
    if let Some(connect) = input
        .wifi_connect
        .filter(|connect| connect.busy && connect_layer(connect) == layer)
    {
        return Some(SessionLine {
            summary: "Someone else connected".to_string(),
            aside: None,
            tone: UiStatusKind::Attention,
            action: busy_retry(input, connect),
        });
    }
    if let Some(held) = input.held().filter(|_| held_layer(input) == layer) {
        return Some(SessionLine {
            summary: held_summary(held).to_string(),
            aside: held_aside(held),
            tone: UiStatusKind::Attention,
            action: None,
        });
    }
    if input.view.status == DeviceStatus::NotResponding
        && BarLayer::of_link(input.link_kind()) == layer
    {
        return Some(SessionLine {
            summary: NOT_RESPONDING.to_string(),
            aside: None,
            tone: UiStatusKind::Attention,
            action: input
                .offer("retry")
                .map(|retry| UiCardAction::press(retry, "Retry").with_icon("retry")),
        });
    }
    None
}

/// The session's work on `layer`'s row, when it is about that link: a
/// network connect under way, a take-over, a Connect opening or a dropped
/// link held, a Connect or a network connect that failed. While it shows,
/// the work's words are the row's state.
pub(crate) fn session_work(input: &BoardCardInput<'_>, layer: BarLayer) -> Option<UiBarWork> {
    take_over_work(input)
        .filter(|_| input.held().is_some() && held_layer(input) == layer)
        .or_else(|| connect_work(input).filter(|_| session_layer(input) == layer))
        .or_else(|| failed_network_connect(input, layer))
}

/// The session's notice on `layer`'s row, first in its details: the
/// board stopped answering (its own words), someone else holds its network
/// slot, or another tab holds it (and what taking it over costs).
pub(crate) fn session_notice(
    input: &BoardCardInput<'_>,
    layer: BarLayer,
) -> Option<RichSection<UiCardAction>> {
    let line = session_line(input, layer)?;
    let view = input.view;
    let sentence = if let Some(connect) = input
        .wifi_connect
        .filter(|connect| connect.busy && connect_layer(connect) == layer)
    {
        return Some(notice(
            "Connection",
            line.tone,
            SOMEONE_ELSE_SENTENCE,
            busy_retry(input, connect),
        ));
    } else if let Some(held) = input.held() {
        let undoable = input
            .offer("take-over")
            .is_some_and(|offer| *offer.consequence() == ActionConsequence::Undoable);
        held_sentence(held, undoable)
    } else {
        match &view.detail {
            Some(detail) => format!("{} — {detail}", view.state_label),
            None => view.state_label.clone(),
        }
    };
    Some(notice("Connection", line.tone, sentence, line.action))
}

/// How the link in use reaches the board, for its row's details: what the
/// row says now, the LAN address, when the board was last heard, and the
/// board's own detail.
pub(crate) fn reach_facts(input: &BoardCardInput<'_>, now: &str) -> RichSection<UiCardAction> {
    let view = input.view;
    let mut reach = vec![RichLine::new("Now", now)];
    if let Some(lan) = input.lan.filter(|lan| !lan.address.is_empty()) {
        reach.push(RichLine::new("Address", lan.address.clone()));
        reach.push(RichLine::new("URL", lan.url.clone()));
    }
    // One age phrasing (P17): a label and a value, `Last heard` · `12 s
    // ago`, never "quiet — last heard …".
    let heard = match input.offline() {
        true => input.last_seen_at.map(|seen| age_words(input.now - seen)),
        false => view.heard_age_ms.map(|ms| age_words(ms as f64 / 1_000.0)),
    }
    .unwrap_or_else(|| "not heard this session".to_string());
    reach.push(RichLine::new("Last heard", heard));
    if let Some(detail) = &view.detail {
        reach.push(RichLine::new("Detail", detail.clone()));
    }
    facts("Reach", reach)
}

/// Retry for a connect turned away busy: the same road again. The holder is
/// someone Studio cannot name, so there is no take-over to offer.
fn busy_retry(input: &BoardCardInput<'_>, connect: &UiWifiConnect) -> Option<UiCardAction> {
    input
        .offer(connect_verb(connect))
        .map(|offer| UiCardAction::press(offer, "Retry").with_icon("retry"))
}

/// The session's state while it is not open on the card (CD7):
/// "connecting" (the board being reached, or its session opening),
/// "reconnecting" (the connected session's link dropped and the lens holds
/// on), or the last Connect's failure — "connection failed", red, with
/// Connect as its Retry and the reason in the details. Connected and
/// watched boards do no session work.
fn connect_work(input: &BoardCardInput<'_>) -> Option<UiBarWork> {
    // A board that stopped answering says so, with its Retry, rather than a
    // spinner over it: a Connect held for it attaches once Retry brings it
    // back.
    let answering = input.view.status != DeviceStatus::NotResponding;
    match input.connection {
        BoardConnection::Connecting => answering.then(|| UiBarWork::running(CONNECTING, None)),
        BoardConnection::Reconnecting => answering.then(|| UiBarWork::running(RECONNECTING, None)),
        BoardConnection::Failed { reason } => Some(UiBarWork::failed(
            CONNECTION_FAILED,
            Some(reason.clone()),
            input
                .offer("connect")
                .map(|connect| UiCardAction::press(connect, "Retry").with_icon("retry")),
        )),
        BoardConnection::Watched | BoardConnection::Connected => None,
    }
}

/// A Wi‑Fi or relay connect under way on its row: "connecting"; or one
/// that failed (not turned away busy): "connection failed", red, why in
/// the details, with Retry on the same road.
fn failed_network_connect(input: &BoardCardInput<'_>, layer: BarLayer) -> Option<UiBarWork> {
    let connect = input
        .wifi_connect
        .filter(|connect| !connect.busy && connect_layer(connect) == layer)?;
    if connect.connecting {
        return Some(UiBarWork::running(CONNECTING, None));
    }
    let error = connect.error.clone()?;
    Some(UiBarWork::failed(
        CONNECTION_FAILED,
        Some(error),
        input
            .offer(connect_verb(connect))
            .map(|offer| UiCardAction::press(offer, "Retry").with_icon("retry")),
    ))
}

/// The verb a network connect takes: `connect-relay` or `connect-wifi`.
fn connect_verb(connect: &UiWifiConnect) -> &'static str {
    match connect.through_relay {
        true => "connect-relay",
        false => "connect-wifi",
    }
}

/// The link a Wi‑Fi connect takes: the LAN, or lightplayer.app's relay.
pub(crate) fn connect_link(through_relay: bool) -> UiLinkKind {
    match through_relay {
        true => UiLinkKind::Relay,
        false => UiLinkKind::Wifi,
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::{HoldLevel, HoldVia};

    use super::super::board_card_builder::board_card;
    use super::super::card_fixtures::CardFixture;
    use super::super::ui_bar_work::BarWorkState;
    use super::*;
    use crate::SurfaceState;

    /// A Wi‑Fi connect that failed is the Wi‑Fi row's, not USB's: the
    /// surface says it, and the Wi‑Fi row goes quiet with its Retry.
    #[test]
    fn a_wifi_connect_that_failed_is_the_wifi_rows_and_goes_quiet() {
        let mut fixture = CardFixture::offline();
        fixture.wifi_address = true;
        fixture.wifi_connect = Some(failed_connect(false, false));
        let card = board_card(&fixture.input());
        assert_eq!(card.surface.state, SurfaceState::NetworkFailed);
        let wifi = card.bar(BarLayer::Wifi);
        let work = wifi.work.clone().expect("the failure");
        assert_eq!(work.words, CONNECTION_FAILED);
        assert_eq!(
            work.detail.as_deref(),
            Some("Couldn't reach the board at 10.0.0.5. Is it on this network?")
        );
        let BarWorkState::Failed { retry: Some(retry) } = work.state else {
            panic!("Retry");
        };
        assert!(retry.offer.to_string().ends_with("/connect-wifi"));
        assert_eq!(
            wifi.tone,
            UiStatusKind::Neutral,
            "the surface is the loud place"
        );
        let usb = card.bar(BarLayer::Usb);
        assert_eq!(usb.work, None, "not USB's");
        assert_eq!(usb.tone, UiStatusKind::Neutral);
    }

    /// A connect turned away busy is said on the row of the road it took,
    /// with Retry on that road and nothing more: taking a board from
    /// another person is sharing's question.
    #[test]
    fn a_connect_turned_away_busy_is_the_rows_of_its_road() {
        for (relay, layer, verb) in [
            (false, BarLayer::Wifi, "/connect-wifi"),
            (true, BarLayer::Cloud, "/connect-relay"),
        ] {
            let mut fixture = CardFixture::offline();
            fixture.wifi_address = !relay;
            fixture.relay = relay;
            fixture.wifi_connect = Some(failed_connect(true, relay));
            let card = board_card(&fixture.input());
            let bar = card.bar(layer);
            assert_eq!(bar.summary, "Someone else connected", "{layer:?}");
            assert_eq!(bar.work, None);
            let retry = bar.action.clone().expect("Retry");
            assert!(retry.offer.to_string().ends_with(verb));
            let notice = bar.details.notice().expect("the notice");
            assert_eq!(notice.sentence.as_deref(), Some(SOMEONE_ELSE_SENTENCE));
            assert_eq!(card.bar(BarLayer::Usb).summary, "not connected");
            assert!(
                fixture
                    .offers()
                    .iter()
                    .all(|offer| !offer.path.to_string().ends_with("/take-over")),
                "no take-over from a stranger"
            );
        }
    }

    /// A board another tab holds on its network slot is said on that way
    /// in's row: Wi‑Fi, or lightplayer.app when it was last reached
    /// through the relay.
    #[test]
    fn a_board_held_on_the_network_is_said_on_its_way_ins_row() {
        for (link, layer) in [
            (UiLinkKind::Wifi, BarLayer::Wifi),
            (UiLinkKind::Relay, BarLayer::Cloud),
        ] {
            let mut fixture = CardFixture::held(HoldLevel::Watching).over(link);
            fixture.view.held_elsewhere.as_mut().unwrap().via = HoldVia::Network;
            let card = board_card(&fixture.input());
            assert_eq!(card.bar(layer).summary, "Open in another tab", "{layer:?}");
            assert_eq!(card.bar(BarLayer::Usb).summary, "not connected");
        }
    }

    /// A network connect under way is its road's row's work.
    #[test]
    fn a_network_connect_under_way_is_its_roads_rows_work() {
        for (relay, layer) in [(false, BarLayer::Wifi), (true, BarLayer::Cloud)] {
            let mut fixture = CardFixture::offline();
            fixture.wifi_connect = Some(UiWifiConnect {
                connecting: true,
                error: None,
                ..failed_connect(false, relay)
            });
            let card = board_card(&fixture.input());
            assert_eq!(
                card.bar(layer).work,
                Some(UiBarWork::running(CONNECTING, None)),
                "{layer:?}"
            );
            assert_eq!(card.bar(BarLayer::Usb).work, None);
        }
    }

    fn failed_connect(busy: bool, through_relay: bool) -> UiWifiConnect {
        UiWifiConnect {
            host: "10.0.0.5".to_string(),
            through_relay,
            connecting: false,
            error: Some(match busy {
                true => crate::WIFI_BUSY_WORDS.to_string(),
                false => "Couldn't reach the board at 10.0.0.5. Is it on this network?".to_string(),
            }),
            busy,
        }
    }
}

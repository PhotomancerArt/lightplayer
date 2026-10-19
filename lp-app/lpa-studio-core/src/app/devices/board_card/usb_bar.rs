//! The USB row: the board's cable (`docs/ux/anatomy/board-card.md`, "USB").
//! Its kind is the link's own name, "USB"; its name says how the cable
//! stands: "connected" while this tab reaches the board by it, else "not
//! connected" — quiet, and normal for a board that lives on a house.
//!
//! A stand-in (a simulated or in-tab emulated board) has no cable: its row
//! keeps its place and says what it is — "Simulated · running", "Emulated ·
//! off" — with the play icon, and this is the one row that says it (P2).
//!
//! What this tab is doing about reaching the board over USB — connecting,
//! reconnecting, a Connect that failed, a board that stopped answering,
//! another tab holding its port — is the session's, laid over these facts
//! ([`super::link_bar`], [`super::link_session`]).
//!
//! | Board | Name | Tone | Action |
//! |---|---|---|---|
//! | A stand-in, linked | "running" (kind "Simulated" / "Emulated", the play icon) | Neutral | — |
//! | A stand-in, off | "off" | Neutral | — (Power on is the primary) |
//! | Reached over USB | "connected" | Neutral | — |
//! | Its port there and closed, unplugged, or reached another way | "not connected" | Neutral | — (Connect is the primary) |
//! | Offline | "not connected", dimmed | Neutral | — (the surface says since when) |
//!
//! Its details, while the cable is the link in use: how it reaches the
//! board, Identify again, Connect, Disconnect ([`super::link_bar`]).

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::PendingLinkView;

use super::board_card_input::{BoardCardInput, link_icon};
use super::detail_sections::facts;
use super::link_bar::{LinkFacts, link_bar};
use super::link_session::{CONNECTED, NOT_CONNECTED, NOT_KNOWN};
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::RichLine;
use crate::app::devices::ui_link_kind::UiLinkKind;

/// The USB row.
pub(crate) fn usb_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let facts = match input.runtime {
        Some(runtime) => LinkFacts {
            kind: Some(runtime.kind_word()),
            ..LinkFacts::new(
                "play",
                match input.linked() {
                    true => "running",
                    false => "off",
                },
            )
        },
        None => LinkFacts {
            stale: input.offline(),
            ..LinkFacts::new(
                "usb",
                match by_cable(input) {
                    true => CONNECTED,
                    false => NOT_CONNECTED,
                },
            )
        },
    };
    link_bar(input, BarLayer::Usb, facts)
}

/// A new board's link rows: the link it arrived on, connected; the others
/// not known yet. Its identifying is the Hardware row's work (it finds out
/// what the board is).
pub(crate) fn pending_link_bars(pending: &PendingLinkView, link: UiLinkKind) -> Vec<UiStackBar> {
    let state = match pending.detail.as_deref() {
        Some(detail) => format!("{} · {detail}", pending.state_label),
        None => pending.state_label.clone(),
    };
    [
        BarLayer::Usb,
        BarLayer::Wifi,
        BarLayer::Bluetooth,
        BarLayer::Cloud,
    ]
    .into_iter()
    .map(|layer| {
        let icon = layer.link().map_or("usb", link_icon);
        match BarLayer::of_link(link) == layer {
            true => UiStackBar {
                details: UiBarDetails {
                    sections: vec![facts("Reach", vec![RichLine::new("State", state.clone())])],
                    panels: Vec::new(),
                    raised: false,
                    foot: Vec::new(),
                },
                ..UiStackBar::new(layer, icon, CONNECTED)
            },
            false => UiStackBar::new(layer, icon, NOT_KNOWN),
        }
    })
    .collect()
}

/// This tab reaches the board by its cable: the link in use is USB, open
/// and answering or not.
fn by_cable(input: &BoardCardInput<'_>) -> bool {
    input.link_kind() == UiLinkKind::Usb
        && input.linked()
        && !matches!(
            input.view.status,
            DeviceStatus::Attached | DeviceStatus::Offline
        )
}

/// The link an offline board's Connect takes (`road_link`), for a test.
#[cfg(test)]
fn offline_link(input: &BoardCardInput<'_>) -> Option<UiLinkKind> {
    super::board_surface::offline_road(input)
        .map(|(road, _)| super::board_surface::road_link(input, road))
}

#[cfg(test)]
mod tests {
    use lpa_devices::view::Escape;
    use lpa_devices::{ActivityKind, HoldLevel};

    use super::super::board_connection::BoardConnection;
    use super::super::card_fixtures::{CardFixture, activity, feed};
    use super::super::link_session::{CONNECTION_FAILED, RECONNECTING};
    use super::super::ui_bar_work::{BarWorkState, UiBarWork};
    use super::*;
    use crate::app::devices::take_over_state::UiTakeOver;
    use crate::{FeedLiveness, UiStatusKind};

    /// The USB row's kind is the link's name; its name says how the cable
    /// stands.
    #[test]
    fn the_usb_row_names_the_cable_and_how_it_stands() {
        let bar = usb_bar(&CardFixture::ready().input());
        assert_eq!(bar.layer, BarLayer::Usb);
        assert_eq!(bar.kind, Some("USB"));
        assert_eq!(bar.icon, "usb");
        assert_eq!(bar.summary, "connected");
        assert_eq!(bar.mark, None, "the struck cloud is gone");
    }

    /// Identifying is the Hardware row's work: the USB row says how the
    /// cable stands.
    #[test]
    fn identifying_is_not_the_usb_rows_work() {
        let mut fixture = CardFixture::ready().with_activity(activity(
            ActivityKind::Identify,
            "Identifying…",
            None,
        ));
        let bar = usb_bar(&fixture.input());
        assert_eq!(bar.work, None);
        assert_eq!(bar.summary, "connected");
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
            let bar = usb_bar(&fixture.input());
            assert_eq!(bar.summary, "Open in another tab", "{level:?}");
            assert_eq!(bar.aside.as_deref(), aside, "{level:?}");
            assert_eq!(bar.icon, "usb");
            assert_eq!(bar.tone, UiStatusKind::Attention, "orange: someone has it");
            assert_eq!(bar.work, None);
            assert_eq!(bar.action, None, "the primary carries Connect");
        }
    }

    #[test]
    fn its_details_name_the_facts_and_what_taking_it_costs() {
        let mut watching = CardFixture::held(HoldLevel::Watching);
        let bar = usb_bar(&watching.input());
        let notice = bar.details.notice().expect("a notice");
        assert_eq!(notice.tone, UiStatusKind::Attention);
        assert_eq!(
            notice.sentence.as_deref(),
            Some("Another tab of this browser has this board open.")
        );

        let mut open = CardFixture::held(HoldLevel::Open);
        let bar = usb_bar(&open.input());
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
        let bar = usb_bar(&busy.input());
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
        let bar = usb_bar(&fixture.input());
        assert_eq!(bar.summary, "Taken by another tab");
        assert_eq!(bar.aside, None);
        assert_eq!(bar.tone, UiStatusKind::Attention);
        assert_eq!(bar.action, None);
    }

    #[test]
    fn a_take_over_under_way_is_the_rows_work_and_the_summary_stays() {
        for words in ["Asking the other tab\u{2026}", "Opening\u{2026}"] {
            let mut fixture = CardFixture::held(HoldLevel::Watching);
            fixture.take_over = Some(UiTakeOver {
                words: words.to_string(),
                failed: false,
            });
            let bar = usb_bar(&fixture.input());
            let work = bar.work.expect("the take-over is work");
            assert_eq!(work.words, words);
            assert_eq!(work.state, BarWorkState::Running);
            assert_eq!(work.cancel, None);
            assert_eq!(bar.summary, "Open in another tab", "unchanged");
            // The row reads neutral while it works (`ui.md`, D34).
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
        let bar = usb_bar(&fixture.input());
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
    fn a_held_board_offers_no_second_way_to_connect() {
        let mut fixture = CardFixture::held(HoldLevel::Watching);
        fixture.wifi_address = true;
        fixture.relay = true;
        let bar = usb_bar(&fixture.input());
        let words = words_of(&bar);
        assert!(
            !words.iter().any(|word| word.starts_with("Connect")),
            "Connect is the take-over: {words:?}"
        );
    }

    /// CD7: a Connect under way is the row's work: "connecting" — the board
    /// being reached, or its session opening — in the state word's place.
    #[test]
    fn a_connect_under_way_is_connecting_work() {
        for session_on_it in [false, true] {
            let mut fixture = CardFixture::ready();
            fixture.editor_holds_it = session_on_it;
            fixture.connection = BoardConnection::Connecting;
            let bar = usb_bar(&fixture.input());
            assert_eq!(
                bar.work,
                Some(UiBarWork::running("connecting", None)),
                "session on it: {session_on_it}"
            );
            assert_eq!(bar.summary, "", "the work says the state");
            assert_eq!(bar.tone, UiStatusKind::Neutral);
            assert_eq!(bar.action, None);
        }
        // A board that stopped answering under a held Connect says so, with
        // its Retry, instead of a spinner over it.
        let mut deaf = CardFixture::ready();
        deaf.view.status = DeviceStatus::NotResponding;
        deaf.view.escapes.insert(0, Escape::Retry);
        deaf.connection = BoardConnection::Connecting;
        let bar = usb_bar(&deaf.input());
        assert_eq!(bar.work, None);
        assert_eq!(bar.summary, "not responding");
        assert_eq!(
            bar.action.map(|retry| retry.word),
            Some("Retry".to_string())
        );
    }

    /// CD7: the connected session's dropped link, held for the board, is
    /// the row's work, "reconnecting".
    #[test]
    fn a_dropped_link_held_is_reconnecting_work() {
        let mut fixture = CardFixture::offline();
        fixture.editor_holds_it = true;
        fixture.connection = BoardConnection::Reconnecting;
        let bar = usb_bar(&fixture.input());
        assert_eq!(bar.work, Some(UiBarWork::running(RECONNECTING, None)));
        assert!(!bar.stale, "news is not last known");
    }

    /// CD7: a Connect that did not open is the row's failure, "connection
    /// failed" in red, with Connect as its Retry and the reason in the
    /// details; the card is otherwise watched.
    #[test]
    fn a_failed_connect_says_connection_failed_with_connect_as_retry() {
        let mut fixture = CardFixture::ready();
        fixture.connection = BoardConnection::Failed {
            reason: "the board did not answer".to_string(),
        };
        let bar = usb_bar(&fixture.input());
        let work = bar.work.clone().expect("the failure");
        assert_eq!(work.words, CONNECTION_FAILED);
        let BarWorkState::Failed { retry: Some(retry) } = work.state else {
            panic!("Retry");
        };
        assert_eq!(retry.word, "Retry");
        assert!(
            retry.offer.to_string().ends_with("/connect"),
            "{}",
            retry.offer
        );
        assert_eq!(retry.refused, None);
        assert_eq!(
            work.detail.as_deref(),
            Some("the board did not answer"),
            "the reason is the details'"
        );
    }

    /// Connected and watched boards do no session work.
    #[test]
    fn a_connected_board_does_no_session_work() {
        let mut fixture = CardFixture::ready();
        fixture.editor_holds_it = true;
        fixture.connection = BoardConnection::Connected;
        fixture.feed = Some(super::super::card_fixtures::lens_feed());
        let bar = usb_bar(&fixture.input());
        assert_eq!(bar.work, None);
        assert_eq!(bar.summary, "connected");
    }

    /// Q9: a stand-in keeps the row and says what it is: simulated or
    /// emulated, running or off.
    #[test]
    fn a_stand_in_says_what_it_is_running_or_off() {
        for (runtime, kind) in [
            (
                crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None),
                "Simulated",
            ),
            (
                crate::UiRuntimeBand::emu("seeed/xiao-esp32-c6", None),
                "Emulated",
            ),
        ] {
            let mut fixture = CardFixture::ready();
            fixture.runtime = Some(runtime.clone());
            let bar = usb_bar(&fixture.input());
            assert_eq!((bar.kind, bar.summary.as_str()), (Some(kind), "running"));
            assert_eq!(bar.icon, "play");
            let mut off = CardFixture::offline();
            off.runtime = Some(runtime);
            let bar = usb_bar(&off.input());
            assert_eq!((bar.kind, bar.summary.as_str()), (Some(kind), "off"));
            assert!(!bar.stale, "a stand-in's state is known");
        }
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
        let bar = usb_bar(&fixture.input());
        assert_eq!(bar.summary, "not responding");
        assert_eq!(
            bar.tone,
            UiStatusKind::Attention,
            "orange: a device needs you"
        );
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

    /// A board unplugged that another way reaches still says only its
    /// cable's fact: "not connected". The surface names the way that works.
    #[test]
    fn an_offline_board_another_way_reaches_says_only_its_cables_fact() {
        let mut fixture = CardFixture::offline();
        fixture.wifi_address = true;
        let bar = usb_bar(&fixture.input());
        assert_eq!(bar.summary, "not connected");
        assert_eq!(bar.icon, "usb");
        assert!(bar.stale);
        assert_eq!(offline_link(&fixture.input()), Some(UiLinkKind::Wifi));
    }

    #[test]
    fn a_closed_port_is_not_connected_and_an_offline_board_is_dimmed() {
        let mut attached = CardFixture::ready();
        attached.view.status = DeviceStatus::Attached;
        attached.view.can_receive_project = false;
        attached.view.can_remove_project = false;
        let bar = usb_bar(&attached.input());
        assert_eq!(bar.summary, "not connected");
        assert!(!bar.stale);
        let mut offline = CardFixture::offline();
        offline.last_seen_at = Some(offline.now - 15.0 * 86_400.0);
        let bar = usb_bar(&offline.input());
        assert_eq!(bar.summary, "not connected");
        assert!(bar.stale, "last known, dimmed");
        assert_eq!(
            line(&bar, "Last heard").as_deref(),
            Some("2 weeks ago"),
            "the surface's sentence says the same age the same way"
        );
        offline.last_seen_at = None;
        let bar = usb_bar(&offline.input());
        assert_eq!(
            line(&bar, "Last heard").as_deref(),
            Some("not heard this session")
        );
    }

    /// A board reached another way is not on USB: the USB row says so.
    #[test]
    fn a_linked_board_is_connected_on_the_row_of_its_link() {
        let mut fixture = CardFixture::ready();
        assert_eq!(usb_bar(&fixture.input()).summary, "connected");
        fixture.feed = Some(feed(FeedLiveness::Live, true));
        assert_eq!(usb_bar(&fixture.input()).summary, "connected");
        let mut ble = CardFixture::ready().over(UiLinkKind::Bluetooth);
        let bar = usb_bar(&ble.input());
        assert_eq!(bar.summary, "not connected");
        assert_eq!(line(&bar, "Now"), None, "the reach is the Bluetooth row's");
    }

    /// "Last heard" is a label and a value in Studio's one age (P17), never
    /// "quiet — last heard …"; then the detail.
    #[test]
    fn the_reach_reads_now_last_heard_then_detail() {
        let mut fixture = CardFixture::ready();
        let bar = usb_bar(&fixture.input());
        assert_eq!(line(&bar, "Now").as_deref(), Some("connected"));
        assert_eq!(
            line(&bar, "Last heard").as_deref(),
            Some("not heard this session")
        );
        assert_eq!(
            line(&bar, "Detail").as_deref(),
            Some("LightPlayer · seeed/xiao-esp32-c6")
        );
        fixture.view.freshness_label = Some("quiet — last heard 12 s ago".to_string());
        fixture.view.heard_age_ms = Some(12_000);
        assert_eq!(
            line(&usb_bar(&fixture.input()), "Last heard").as_deref(),
            Some("12 s ago")
        );
    }

    #[test]
    fn its_details_hold_the_ways_to_reach_it_and_disconnect() {
        let mut fixture = CardFixture::ready();
        let bar = usb_bar(&fixture.input());
        assert_eq!(words_of(&bar), ["Identify again", "Disconnect"]);

        // Offline with both roads: the primary takes Wi‑Fi; the cable stays
        // in the USB row's details, the cloud in lightplayer.app's.
        let mut offline = CardFixture::offline();
        offline.wifi_address = true;
        offline.relay = true;
        assert_eq!(words_of(&usb_bar(&offline.input())), ["Connect"]);
    }

    /// A new board's link rows: the link it arrived on, connected, and the
    /// others not known yet.
    #[test]
    fn a_new_boards_link_rows_name_the_link_it_arrived_on() {
        let pending = super::super::primary_action::tests::pending_view();
        let bars = pending_link_bars(&pending, UiLinkKind::Bluetooth);
        let lines: Vec<(BarLayer, &str)> = bars
            .iter()
            .map(|bar| (bar.layer, bar.summary.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (BarLayer::Usb, "\u{2014}"),
                (BarLayer::Wifi, "\u{2014}"),
                (BarLayer::Bluetooth, "connected"),
                (BarLayer::Cloud, "\u{2014}"),
            ]
        );
    }

    fn words_of(bar: &UiStackBar) -> Vec<String> {
        bar.details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter().map(|action| action.word.clone()))
            .collect()
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

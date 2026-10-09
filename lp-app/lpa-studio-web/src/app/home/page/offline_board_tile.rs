//! An offline board's card: a board Studio remembers and cannot see right
//! now, drawn in the Offline boards grid beside the online ones.
//!
//! This tile used to sit behind a quiet "N remembered boards not
//! connected · show" line under the Devices page's grid (that page's D7,
//! "disconnect → disappear"). The home page reverses that: an offline board
//! is a card in its own section, with its last picture and when it was
//! heard (`docs/adr/2026-10-08-the-board-card-and-one-home-page.md`,
//! section 1). The line, its toggle and its sentence are gone with the
//! Devices page.
//!
//! The tile keeps every verb the old one offered, each the board's own
//! published offer (M3): Reconnect (Power on for a sim), Forget with its
//! inline confirm, "Connect over Wi‑Fi" and "Connect through
//! lightplayer.app" with the line that says how that connect is going.
//! It stays dashed and dimmed until the board card (M2) restyles it.

use dioxus::prelude::*;
use lpa_studio_core::{DeviceEscape, RememberedView, UiAction, UiOffer, escape_verb};

use crate::app::home::play_feed_text::frame_age_label;
use crate::app::home::wifi_address_entry::{
    CONNECTING_LINE_CLASS, FAILED_LINE_CLASS, connect_line,
};
use crate::app::node::lamp_view::LampView;
use crate::core::{ActionButton, ActionButtonVariant, use_device_verbs, verb_named};

/// One offline board: dashed, dimmed, and honest about the fact that
/// nothing here is live.
///
/// The tile carries the same 120px preview slot the cards do so the grid
/// reads as one family. When the board's last picture is known — this
/// session pulled one before the port went, or a sidecar remembered one
/// across a reload — the slot draws it exactly as a card's Offline look
/// does: dimmed, with the neutral "last frame · <age>" pill, the age
/// measured from when the board actually published it. Otherwise the
/// "last seen" sentence: never a stale picture passed off as current, and
/// never an empty box.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn OfflineBoardTile(
    entry: RememberedView,
    on_action: EventHandler<UiAction>,
) -> Element {
    // Its escapes are the device's offers (M3), at the escape's path verb.
    let verbs = use_device_verbs(Some(entry.id))();
    let escapes: Vec<(DeviceEscape, UiOffer)> = entry
        .escapes
        .iter()
        .filter_map(|escape| verb_named(&verbs, escape_verb(*escape)).map(|offer| (*escape, offer)))
        .collect();
    // "Connect over Wi‑Fi": a board this browser remembers an address for
    // (core offers it only then), reached with no cable.
    let wifi = verb_named(&verbs, "connect-wifi");
    // "Connect through lightplayer.app": a board Studio has met, while
    // someone is signed in (core offers it only then), reached from
    // anywhere. Its answer shares the Wi‑Fi line below.
    let relay = verb_named(&verbs, "connect-relay");
    let wifi_line = entry.wifi_connect.as_ref().map(connect_line);
    let wifi_failed = entry
        .wifi_connect
        .as_ref()
        .is_some_and(|connect| connect.error.is_some());
    let meta = remembered_meta_text(&entry);
    let slot = remembered_slot(&entry);

    rsx! {
        div { class: OFFLINE_TILE_CLASS,
            div { class: "ux-armed-dim tw:grid tw:min-w-0 tw:gap-2",
                h3 {
                    class: "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground",
                    title: "{entry.title}",
                    "{entry.title}"
                }
                div { class: "{slot.frame_class}",
                    if let Some(picture) = slot.picture {
                        div { class: "ux-play-lamps",
                            LampView { preview: picture }
                        }
                    }
                    if let Some(sentence) = slot.sentence {
                        div { class: "ux-play-empty",
                            p { class: "tw:m-0", "{sentence}" }
                        }
                    }
                    if let Some(pill) = slot.pill {
                        span { class: "ux-play-pill ux-play-pill-offline",
                            span { class: "ux-play-dot" }
                            "{pill}"
                        }
                    }
                }
                p {
                    class: "tw:m-0 tw:truncate tw:font-mono tw:text-[0.68rem] tw:text-subtle-foreground",
                    title: "{meta}",
                    "{meta}"
                }
                if let Some(line) = wifi_line {
                    p {
                        class: if wifi_failed { FAILED_LINE_CLASS } else { CONNECTING_LINE_CLASS },
                        role: "status",
                        "{line}"
                    }
                }
            }
            // Every escape the projection granted, rendered — the renderer
            // half of invariant I3, exactly as on a card. Reconnect is the
            // tile's one call to action (a grant can die on a replug), so
            // it wears the Outline voice; Forget keeps its inline confirm.
            div { class: "tw:mt-auto tw:flex tw:flex-wrap tw:items-center tw:gap-2 tw:whitespace-nowrap",
                if let Some(wifi) = wifi {
                    ActionButton {
                        key: "{\"connect-wifi\"}",
                        action: wifi.action,
                        running: false,
                        variant: ActionButtonVariant::Outline,
                        on_action,
                    }
                }
                if let Some(relay) = relay {
                    ActionButton {
                        key: "{\"connect-relay\"}",
                        action: relay.action,
                        running: false,
                        variant: ActionButtonVariant::Outline,
                        on_action,
                    }
                }
                for (escape , offer) in escapes {
                    ActionButton {
                        key: "{escape:?}",
                        action: offer.action,
                        running: false,
                        variant: remembered_escape_variant(escape),
                        on_action,
                    }
                }
            }
        }
    }
}

/// An offline board's card: dashed and dimmed, the same width as a card in
/// the grid but with only the rows an absent board can fill.
const OFFLINE_TILE_CLASS: &str = "tw:flex tw:flex-col tw:gap-3 tw:rounded-md tw:border tw:border-dashed tw:border-border tw:bg-card tw:p-4 tw:opacity-75";

/// The tile's second line: the board it is, and when Studio last heard it.
fn remembered_meta_text(entry: &RememberedView) -> String {
    match (entry.board.as_deref(), entry.last_seen_label.as_deref()) {
        (Some(board), Some(last)) => format!("{board} · {last}"),
        (Some(board), None) => board.to_string(),
        (None, Some(last)) => last.to_string(),
        (None, None) => "not heard this session".to_string(),
    }
}

/// What an offline tile's preview slot draws.
#[derive(Debug, PartialEq)]
struct RememberedSlot {
    /// The slot's classes: the fixed frame, dimmed when a picture is in it.
    frame_class: String,
    /// The last picture, when it has geometry to draw.
    picture: Option<lpa_studio_core::UiControlProductPreview>,
    /// "last frame · <age>", beside a picture.
    pill: Option<String>,
    /// The honest sentence when there is no picture to draw.
    sentence: Option<String>,
}

/// The slot's contents: the last picture with its age when the entry
/// carries a frame WITH a layout; a frame without geometry (the board's
/// layout exceeded the wire budget when it was captured) has nothing to
/// draw and keeps the sentence, like the card does.
fn remembered_slot(entry: &RememberedView) -> RememberedSlot {
    let picture = entry
        .feed
        .as_ref()
        .and_then(|feed| feed.frame.as_ref())
        .filter(|frame| frame.display_layout.is_some())
        .cloned();
    match picture {
        Some(picture) => RememberedSlot {
            frame_class: "ux-play-frame ux-play-frame-slot ux-play-frame-dim".to_string(),
            picture: Some(picture),
            pill: Some(format!(
                "last frame · {}",
                frame_age_label(
                    entry
                        .feed
                        .as_ref()
                        .and_then(|feed| feed.frame_age_secs)
                        .unwrap_or_default()
                )
            )),
            sentence: None,
        },
        None => RememberedSlot {
            frame_class: "ux-play-frame ux-play-frame-slot".to_string(),
            picture: None,
            pill: None,
            sentence: Some(remembered_preview_sentence(entry)),
        },
    }
}

/// The preview slot's sentence for an absent board (AC10's honesty rule on
/// a tile): never a stale picture presented as current, and never an empty
/// box either.
fn remembered_preview_sentence(entry: &RememberedView) -> String {
    match entry.last_seen_label.as_deref() {
        Some(last) => format!("Not connected — {last}."),
        None => "Not connected — Studio has not heard this board.".to_string(),
    }
}

/// Reconnect is the tile's call to action and wears the Outline voice; the
/// rest (Forget, and anything the projection adds later) stay quiet chips.
fn remembered_escape_variant(escape: DeviceEscape) -> ActionButtonVariant {
    match escape {
        DeviceEscape::Reconnect => ActionButtonVariant::Outline,
        _ => ActionButtonVariant::Quiet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{
        DeviceRosterConfig, DeviceRosterView, DeviceStatus, DeviceView, RosterView,
        build_home_sections, split_roster,
    };

    /// The remembered board keeps its NAME — the whole point of remembering
    /// one (AC9: replugging brings the card back with its name).
    #[test]
    fn a_cold_registry_row_keeps_its_name_on_its_offline_card() {
        let mut roster = lpa_studio_core::DeviceRoster::new(DeviceRosterConfig::default());
        roster.load_records(&[lpa_studio_core::app::places::RegisteredDevice {
            uid: "dev0000000000000001".to_string(),
            name: "Porch sign".to_string(),
            ..Default::default()
        }]);

        let split = split_roster(&view(
            roster.view(lpa_studio_core::DeviceMillis(0)).roster,
            true,
        ));

        assert!(split.connected.is_empty(), "{split:?}");
        assert_eq!(split.remembered.len(), 1, "{split:?}");
        assert_eq!(split.remembered[0].title, "Porch sign");
        assert!(
            split.remembered[0].escapes.contains(&DeviceEscape::Forget),
            "{:?}",
            split.remembered[0],
        );
    }

    /// Every card offers a way out, in every state — the renderer's half of
    /// invariant I3.
    #[test]
    fn every_rendered_card_has_at_least_one_escape() {
        let cards: Vec<DeviceView> = {
            let mut roster = lpa_studio_core::DeviceRoster::new(DeviceRosterConfig::default());
            roster.load_records(&[lpa_studio_core::app::places::RegisteredDevice {
                uid: "dev0000000000000001".to_string(),
                ..Default::default()
            }]);
            roster.view(lpa_studio_core::DeviceMillis(0)).roster.devices
        };

        for card in cards {
            assert!(!card.escapes.is_empty(), "{card:?}");
        }
    }

    /// A tile says the board it is and when Studio last heard it — and
    /// never invents either half.
    #[test]
    fn a_remembered_tile_names_the_board_and_when_it_was_heard() {
        let mut entry = remembered_fixture();
        assert_eq!(
            remembered_meta_text(&entry),
            "seeed-xiao-esp32c6 · last heard 4 min ago"
        );

        entry.last_seen_label = None;
        assert_eq!(remembered_meta_text(&entry), "seeed-xiao-esp32c6");

        entry.board = None;
        assert_eq!(remembered_meta_text(&entry), "not heard this session");
    }

    /// AC10 on a tile: the preview slot never shows a picture that is not
    /// there, and never sits blank either.
    #[test]
    fn a_remembered_tile_says_why_there_is_no_picture() {
        let mut entry = remembered_fixture();
        assert_eq!(
            remembered_preview_sentence(&entry),
            "Not connected — last heard 4 min ago."
        );

        entry.last_seen_label = None;
        assert_eq!(
            remembered_preview_sentence(&entry),
            "Not connected — Studio has not heard this board."
        );
    }

    /// The home page reverses D7: an offline board is not folded away under
    /// a line, it is a card under Offline boards — and that card draws the
    /// board's last picture with its age. Core puts it in the section; the
    /// tile's slot draws what the roster remembered of it.
    #[test]
    fn an_offline_board_is_a_card_with_its_last_picture_and_its_age() {
        let online = DeviceView {
            id: lpa_studio_core::DeviceId(1),
            status: DeviceStatus::Ready,
            title: "Bench C6".to_string(),
            state_label: "Ready".to_string(),
            ..bare_card()
        };
        let offline = DeviceView {
            id: lpa_studio_core::DeviceId(2),
            status: DeviceStatus::Offline,
            title: "Porch sign".to_string(),
            state_label: "Not connected".to_string(),
            freshness_label: Some("last heard 3 h ago".to_string()),
            ..bare_card()
        };
        let mut devices = view(
            RosterView {
                devices: vec![online, offline],
                pending: Vec::new(),
            },
            true,
        );
        devices
            .feeds
            .insert(lpa_studio_core::DeviceId(2), remembered_feed(true));

        let sections = build_home_sections(&[], &devices);
        assert_eq!(sections.online.len(), 1, "{sections:?}");
        assert_eq!(sections.online[0].title, "Bench C6");
        assert_eq!(sections.offline.len(), 1, "{sections:?}");
        assert_eq!(sections.offline[0].title, "Porch sign");

        // The card the slot finds for that entry, as `BoardCardSlot` does.
        let remembered = split_roster(&devices)
            .remembered
            .into_iter()
            .find(|entry| entry.id == sections.offline[0].id)
            .expect("the offline entry has its remembered view");
        let slot = remembered_slot(&remembered);
        assert!(slot.picture.is_some(), "{slot:?}");
        assert_eq!(slot.pill.as_deref(), Some("last frame · 3 h ago"));
        assert_eq!(slot.sentence, None);
    }

    /// Reconnect is the tile's call to action; Forget stays a quiet chip
    /// with its own inline confirm.
    #[test]
    fn reconnect_is_the_tiles_one_outline_verb() {
        assert_eq!(
            remembered_escape_variant(DeviceEscape::Reconnect),
            ActionButtonVariant::Outline,
        );
        assert_eq!(
            remembered_escape_variant(DeviceEscape::Forget),
            ActionButtonVariant::Quiet,
        );
    }

    /// The tile's slot: the last picture, dimmed and aged from its own
    /// stamp, when the entry carries one with geometry — and the honest
    /// sentence otherwise (no feed, or a frame the board never gave a
    /// layout for).
    #[test]
    fn the_slot_draws_the_last_picture_or_says_why_there_is_none() {
        let entry = remembered_fixture();
        let plain = remembered_slot(&entry);
        assert!(plain.picture.is_none());
        assert_eq!(plain.pill, None);
        assert_eq!(
            plain.sentence.as_deref(),
            Some("Not connected — last heard 4 min ago.")
        );
        assert!(!plain.frame_class.contains("ux-play-frame-dim"));

        let with_picture = remembered_slot(&RememberedView {
            feed: Some(remembered_feed(true)),
            ..remembered_fixture()
        });
        assert!(with_picture.picture.is_some());
        assert_eq!(with_picture.pill.as_deref(), Some("last frame · 3 h ago"));
        assert_eq!(with_picture.sentence, None);
        assert!(with_picture.frame_class.contains("ux-play-frame-dim"));

        let no_layout = remembered_slot(&RememberedView {
            feed: Some(remembered_feed(false)),
            ..remembered_fixture()
        });
        assert!(
            no_layout.picture.is_none(),
            "bytes without geometry draw nothing"
        );
        assert_eq!(no_layout.pill, None);
        assert_eq!(no_layout.sentence, plain.sentence);
    }

    fn view(roster: RosterView, transport_available: bool) -> DeviceRosterView {
        DeviceRosterView {
            roster,
            transport_available,
            usb_available: transport_available,
            ..DeviceRosterView::default()
        }
    }

    fn remembered_fixture() -> RememberedView {
        RememberedView {
            id: lpa_studio_core::DeviceId(7),
            title: "Porch sign".to_string(),
            board: Some("seeed-xiao-esp32c6".to_string()),
            last_seen_label: Some("last heard 4 min ago".to_string()),
            escapes: vec![DeviceEscape::Reconnect, DeviceEscape::Forget],
            face: lpa_studio_core::DeviceFace::Wire,
            feed: None,
            wifi_connect: None,
        }
    }

    fn remembered_feed(with_layout: bool) -> lpa_studio_core::DeviceCardFeedView {
        use std::rc::Rc;
        let layout = with_layout.then(|| {
            Rc::new(lpa_studio_core::ControlDisplayLayout::Layout2d(
                lpa_studio_core::ControlLayout2d::new(
                    lpa_studio_core::Revision::new(7),
                    4,
                    1,
                    Vec::new(),
                ),
            ))
        });
        lpa_studio_core::DeviceCardFeedView {
            frame: Some(lpa_studio_core::UiControlProductPreview {
                revision: 3,
                extent: lpa_studio_core::ControlExtent::new(1, 12),
                sample_format: lpa_studio_core::UiControlSampleFormat::U16,
                sample_layout: lpa_studio_core::ControlSampleLayout { spans: Vec::new() },
                display_layout: layout,
                bytes: Rc::from(vec![0u8; 24]),
            }),
            frame_age_secs: Some(3.0 * 3_600.0),
            engine_fps: None,
            liveness: lpa_studio_core::FeedLiveness::Offline,
        }
    }

    fn bare_card() -> DeviceView {
        DeviceView {
            id: lpa_studio_core::DeviceId(1),
            title: String::new(),
            status: DeviceStatus::Ready,
            state_label: String::new(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: None,
            firmware_face: lpa_studio_core::DeviceFirmwareFace::Unknown,
            remembered_firmware: None,
            degraded: None,
            loaded_project: lpa_studio_core::DeviceLoadedProject::Unknown,
            engine_fps: None,
            link_counters: None,
            can_receive_project: false,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            escapes: vec![DeviceEscape::Forget],
            update_blocked: None,
            last_update_outcome: None,
        }
    }
}

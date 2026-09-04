//! The Devices page (`/devices`, vision D9): the runtime roster.
//!
//! Two sections, from two different places:
//!
//! - **Runtimes** — the live simulator's card, while a sim session exists
//!   (D36). Unchanged by the device-model rebuild; the sim is not a device
//!   (D22) and keeps its own path through round 1.
//! - **Devices** — the `lpa-devices` roster's projection: one card per known
//!   device, one entry per link still being identified, and one button to ask
//!   the browser for another port.
//!
//! The device half renders `RosterView` DIRECTLY. There is no `Ui*` mirror of
//! it, on purpose: the projection is already a pure function of the fold, so
//! this page cannot show a state the model is not in — which is the whole
//! class of bug (two cards for one board, a stale verdict, a vanished danger
//! zone) the rebuild exists to end.
//!
//! Setup, flashing and pushing are round 2. Where they belong, the page says
//! so and disables the control rather than hiding it or, worse, offering a
//! button that does nothing.
//!
//! # Disconnect → disappear (D7, AC9)
//!
//! An unplugged board is NOT a card. [`split_roster`] divides the roster's
//! own projection at [`DeviceStatus::Offline`]: the connected devices are
//! cards in the grid, and everything Studio remembers but cannot currently
//! see collapses into one quiet line beneath it — "N remembered boards not
//! connected · show" — whose expanded tiles carry the two verbs an absent
//! board can honestly offer (Reconnect, Forget). The grid therefore only
//! ever holds boards that are actually there, which is what makes plugging
//! one in read as arrival rather than as a status change on a card that was
//! already sitting there greyed out.
//!
//! The toggle is page-local UI state on purpose: whether a fold is open is
//! not something the device model knows or should learn.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceAction, DeviceEscape, DeviceRosterView, DevicesOp, RememberedView, UiAction, UiHomeView,
    device_escape_action, split_roster,
};

use crate::app::home::device_roster_card::{DeviceRosterCard, PendingLinkCard};
use crate::app::home::sim_card::SimCard;
use crate::app::home::{device_grid_class, section_title_class};
use crate::core::{ActionButton, ActionButtonVariant};

/// The runtime roster page (roadmap M4's gallery top, re-homed).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DevicesPage(
    home: UiHomeView,
    /// Story-only: render the remembered line already expanded. A static
    /// capture cannot click the toggle, and the tiles are the half of D7
    /// worth reviewing. Real surfaces never set this — the line opens
    /// closed and stays where the user leaves it.
    #[props(default)]
    remembered_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let devices = home.devices.clone();
    // D7: the grid draws the boards that are HERE; the ones Studio only
    // remembers become the quiet line under it. The split is the model's
    // own projection filtered by status — the page invents no membership of
    // its own.
    let split = split_roster(&devices);
    let connected = split.connected;
    let remembered = split.remembered;

    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-7",
            if let Some(issue) = home.issue.clone() {
                div { class: "tw:flex tw:items-center tw:gap-3 tw:rounded-md tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-4 tw:py-2.5 tw:text-sm tw:text-status-error-foreground",
                    span { "{issue.message}" }
                }
            }

            // The live simulator, while a session is running (D36: its
            // card exists exactly as long as the session does).
            if let Some(card) = home.sim.clone() {
                section { class: "tw:grid tw:gap-3",
                    header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                        h2 { class: section_title_class(), "Runtimes" }
                    }
                    div { class: device_grid_class(),
                        SimCard { key: "{card.render_key()}", card, on_action }
                    }
                }
            }

            section { class: "tw:grid tw:gap-3",
                header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                    h2 { class: section_title_class(), "Devices" }
                }

                if !devices.transport_available {
                    UnavailableNote {}
                }

                if devices.transport_available {
                    div { class: device_grid_class(),
                        // Pending links come first: a board just plugged in is
                        // what the user is looking at.
                        for pending in devices.roster.pending.iter().cloned() {
                            PendingLinkCard {
                                key: "pending-{pending.link.0}",
                                pending,
                                on_action,
                            }
                        }
                        for card in connected.iter().cloned() {
                            DeviceRosterCard {
                                key: "device-{card.id.0}",
                                // The running face's Open needs the
                                // device's editor address (its registry
                                // uid); a board still identifying has none.
                                open_uid: devices.open_addresses.get(&card.id.0).cloned(),
                                card,
                                // The empty face's picker reads the SAME two
                                // lists the gallery does — there is no
                                // separate device-side project source.
                                projects: home.projects.clone(),
                                examples: home.examples.clone(),
                                on_action,
                            }
                        }
                        // Adding lives IN the roster, at the insertion point
                        // (the house rule: add buttons sit where the new
                        // entry will appear, never in headers).
                        AddDeviceCard { on_action }
                    }
                }

                // The boards Studio knows and cannot see. One line, always
                // below the grid, never a card (D7).
                if !remembered.is_empty() {
                    RememberedLine {
                        remembered,
                        initially_open: remembered_open,
                        on_action,
                    }
                }
            }
        }
    }
}

/// The roster's add slot: a card in the grid where the next device's card
/// will appear. It doubles as the empty state — same slot, same copy, same
/// layout whether it is the first board or the fifth (clear minimalism,
/// G1 ruling) — so there is no separate empty-state block to jump around.
///
/// The CTA wears the default Solid/Primary tier: the Outline override was
/// the round-1 dodge for the too-bold gradient fill, and the spike gate
/// (2026-08-31, "1F for the primary") made Primary the spectrum outline
/// the slot wanted all along.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AddDeviceCard(on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-40 tw:flex-col tw:items-center tw:justify-center tw:gap-3 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:bg-transparent tw:px-5 tw:py-6",
            // The invitation is transport-OPEN: connecting is the goal, and
            // the USB specifics live on the verb below ("It's plugged in"),
            // so a network path can join later as a sibling verb rather
            // than a rewrite.
            p { class: "tw:m-0 tw:max-w-56 tw:text-center tw:text-xs tw:leading-relaxed tw:text-muted-foreground",
                "Connect a LightPlayer board to control\u{a0}it."
            }
            ActionButton {
                action: DevicesOp::action_for(DeviceAction::AddFromUsb),
                running: false,
                on_action,
            }
        }
    }
}

/// No transport: this build (or this browser) cannot reach a USB port at all.
///
/// Said out loud, because an empty roster with no explanation reads as "you
/// have no devices" — a different and wrong claim.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UnavailableNote() -> Element {
    rsx! {
        div { class: note_class(),
            p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-strong-foreground",
                "This browser can't talk to USB devices"
            }
            p { class: "tw:m-0 tw:max-w-prose tw:text-xs tw:leading-relaxed tw:text-subtle-foreground",
                "Studio reaches boards over Web Serial, which Chrome, Edge and \
                 other Chromium browsers support. The simulator works everywhere."
            }
        }
    }
}

/// The quiet line under the grid: the boards Studio remembers but cannot
/// currently see (D7, AC9).
///
/// It is a LINE, not a section: an absent board has nothing to report, and
/// giving it a card would put four boards' worth of grey furniture in front
/// of the one that is actually plugged in. Opening it is a page-local
/// decision (`use_signal`) — the model has no opinion about folds.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RememberedLine(
    remembered: Vec<RememberedView>,
    /// Story-only: start expanded (see [`DevicesPage`]).
    #[props(default)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut open = use_signal(|| initially_open);
    let count = remembered.len();

    rsx! {
        div { class: "tw:grid tw:gap-3",
            p { class: "tw:m-0 tw:flex tw:flex-wrap tw:items-center tw:gap-2 tw:text-xs tw:text-dim-foreground",
                span { "{remembered_line_text(count)}" }
                button {
                    class: remembered_toggle_class(),
                    r#type: "button",
                    title: "Show the boards Studio remembers but cannot see",
                    onclick: move |_| {
                        let was = open();
                        open.set(!was);
                    },
                    if open() { "hide" } else { "show" }
                }
                span { class: "tw:flex-1" }
                // Why they are kept at all, said once rather than on every
                // tile: Forget is the only thing that removes a board, and
                // this is where it lives.
                span { "Studio keeps their names; Forget lives here." }
            }
            if open() {
                div { class: device_grid_class(),
                    for entry in remembered.iter().cloned() {
                        RememberedTile {
                            key: "remembered-{entry.id.0}",
                            entry,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// One remembered board: dashed, dimmed, and honest about the fact that
/// nothing here is live.
///
/// The tile carries the same 120px preview slot the cards do so the row
/// reads as the same family — with the "last seen" sentence in it, because
/// there IS no picture: the feed is a later milestone, and a board that is
/// not connected would have nothing to feed it anyway.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RememberedTile(entry: RememberedView, on_action: EventHandler<UiAction>) -> Element {
    let device = entry.id;
    let meta = remembered_meta_text(&entry);

    rsx! {
        div { class: remembered_tile_class(),
            div { class: "ux-armed-dim tw:grid tw:min-w-0 tw:gap-2",
                h3 {
                    class: "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground",
                    title: "{entry.title}",
                    "{entry.title}"
                }
                div { class: "ux-play-frame ux-play-frame-slot",
                    div { class: "ux-play-empty",
                        p { class: "tw:m-0", "{remembered_preview_sentence(&entry)}" }
                    }
                }
                p {
                    class: "tw:m-0 tw:truncate tw:font-mono tw:text-[0.68rem] tw:text-subtle-foreground",
                    title: "{meta}",
                    "{meta}"
                }
            }
            // Every escape the projection granted, rendered — the renderer
            // half of invariant I3, exactly as on a card. Reconnect is the
            // tile's one call to action (a grant can die on a replug), so
            // it wears the Outline voice; Forget keeps its inline confirm.
            div { class: "tw:mt-auto tw:flex tw:flex-wrap tw:items-center tw:gap-2 tw:whitespace-nowrap",
                for escape in entry.escapes.iter().copied() {
                    ActionButton {
                        key: "{escape:?}",
                        action: device_escape_action(escape, device),
                        running: false,
                        variant: remembered_escape_variant(escape),
                        on_action,
                    }
                }
            }
        }
    }
}

/// "2 remembered boards not connected" — the line's own sentence, singular
/// when there is one of them.
fn remembered_line_text(count: usize) -> String {
    match count {
        1 => "1 remembered board not connected".to_string(),
        other => format!("{other} remembered boards not connected"),
    }
}

/// The tile's second line: the board it is, and when Studio last heard it.
fn remembered_meta_text(entry: &RememberedView) -> String {
    match (entry.board.as_deref(), entry.last_seen_label.as_deref()) {
        (Some(board), Some(last)) => format!("{board} · {last}"),
        (Some(board), None) => board.to_string(),
        (None, Some(last)) => last.to_string(),
        (None, None) => "not heard this session".to_string(),
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

/// The show/hide control: a text affordance, not a button-looking button —
/// the line is chrome, and a chip here would compete with the cards above.
fn remembered_toggle_class() -> &'static str {
    "ux-focus-ring tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:p-0 tw:text-xs tw:text-subtle-foreground tw:underline tw:decoration-dotted"
}

/// A remembered tile: dashed and dimmed, the same width as a card in the
/// grid but with only the rows an absent board can fill.
fn remembered_tile_class() -> &'static str {
    "tw:flex tw:flex-col tw:gap-3 tw:rounded-md tw:border tw:border-dashed tw:border-border tw:bg-card tw:p-4 tw:opacity-75"
}

fn note_class() -> &'static str {
    "tw:grid tw:gap-2 tw:rounded-md tw:border tw:border-dashed tw:border-border tw:px-4 tw:py-5"
}

/// A page-shaped summary of what the roster is showing, for tests and
/// fallback renderers.
///
/// Keeping it here rather than in a test module means the page's own claims
/// ("identifying", "no devices yet") are asserted against the same values the
/// components read — including D7's: an offline board is NOT a card, so it
/// is not a line here either. It is counted by the remembered line instead.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the view-test seam; the page renders the DTOs directly"
    )
)]
pub(crate) fn devices_page_lines(devices: &DeviceRosterView) -> Vec<String> {
    if !devices.transport_available {
        return vec!["This browser can't talk to USB devices".to_string()];
    }
    let split = split_roster(devices);
    if split.connected.is_empty() && devices.roster.pending.is_empty() {
        let mut lines = vec!["No devices yet".to_string()];
        if !split.remembered.is_empty() {
            lines.push(remembered_line_text(split.remembered.len()));
        }
        return lines;
    }
    devices
        .roster
        .pending
        .iter()
        .map(|pending| format!("{} — {}", pending.title, pending.state_label))
        .chain(
            split
                .connected
                .iter()
                .map(|card| format!("{} — {}", card.title, card.state_label)),
        )
        .chain((!split.remembered.is_empty()).then(|| remembered_line_text(split.remembered.len())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{DeviceRosterConfig, DeviceStatus, DeviceView, RosterView};

    fn view(roster: RosterView, transport_available: bool) -> DeviceRosterView {
        DeviceRosterView {
            roster,
            transport_available,
            open_addresses: Default::default(),
        }
    }

    /// A host build (or a Firefox) has no transport, and the page says that
    /// rather than showing an empty roster that reads as "you have none".
    #[test]
    fn no_transport_says_so_instead_of_showing_an_empty_roster() {
        let lines = devices_page_lines(&view(
            RosterView {
                devices: Vec::new(),
                pending: Vec::new(),
            },
            false,
        ));

        assert_eq!(lines, vec!["This browser can't talk to USB devices"]);
    }

    #[test]
    fn a_working_transport_with_nothing_on_it_invites_a_port() {
        let lines = devices_page_lines(&view(
            RosterView {
                devices: Vec::new(),
                pending: Vec::new(),
            },
            true,
        ));

        assert_eq!(lines, vec!["No devices yet"]);
    }

    /// A fresh plug is an "identifying…" entry BEFORE it is a card — and it
    /// is listed first, because it is what the user is looking at. The
    /// registry row rehydrated beside it is COLD (nothing has heard it this
    /// session), so under D7 it is not a card at all: it is counted by the
    /// remembered line under the grid.
    #[test]
    fn a_pending_link_reads_as_identifying_and_comes_first() {
        let mut roster = lpa_studio_core::DeviceRoster::new(DeviceRosterConfig::default());
        roster.load_records(&[lpa_studio_core::app::places::RegisteredDevice {
            uid: "dev0000000000000001".to_string(),
            name: "Porch sign".to_string(),
            ..Default::default()
        }]);
        roster.handle(
            lpa_studio_core::DeviceMillis(0),
            lpa_studio_core::DeviceInput::Event(lpa_devices_event_attach()),
        );

        let lines = devices_page_lines(&view(
            roster.view(lpa_studio_core::DeviceMillis(0)).roster,
            true,
        ));

        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("identifying"), "{lines:?}");
        assert_eq!(lines[1], "1 remembered board not connected", "{lines:?}");
    }

    /// The remembered board keeps its NAME — the whole point of remembering
    /// one (AC9: replugging brings the card back with its name).
    #[test]
    fn a_cold_registry_row_keeps_its_name_on_the_remembered_line() {
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

    /// The line's own sentence (D7): singular when one board is missing,
    /// plural otherwise — a "1 remembered boards" line is the kind of thing
    /// that makes a UI feel unfinished.
    #[test]
    fn the_remembered_line_counts_boards_in_its_own_words() {
        assert_eq!(remembered_line_text(1), "1 remembered board not connected");
        assert_eq!(remembered_line_text(2), "2 remembered boards not connected");
        assert_eq!(
            remembered_line_text(11),
            "11 remembered boards not connected"
        );
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

    /// D7: an offline board leaves the grid entirely and is counted by the
    /// line instead — the page cannot draw a card for a board that is not
    /// there.
    #[test]
    fn an_offline_board_leaves_the_grid_for_the_remembered_line() {
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
            ..bare_card()
        };

        let lines = devices_page_lines(&view(
            RosterView {
                devices: vec![online, offline],
                pending: Vec::new(),
            },
            true,
        ));

        assert_eq!(
            lines,
            vec![
                "Bench C6 — Ready".to_string(),
                "1 remembered board not connected".to_string(),
            ],
        );
    }

    /// A roster of nothing BUT remembered boards still invites a port — the
    /// grid is empty, and the line sits under it.
    #[test]
    fn only_remembered_boards_still_invites_a_port() {
        let offline = DeviceView {
            id: lpa_studio_core::DeviceId(2),
            status: DeviceStatus::Offline,
            title: "Porch sign".to_string(),
            state_label: "Not connected".to_string(),
            ..bare_card()
        };

        let lines = devices_page_lines(&view(
            RosterView {
                devices: vec![offline],
                pending: Vec::new(),
            },
            true,
        ));

        assert_eq!(
            lines,
            vec![
                "No devices yet".to_string(),
                "1 remembered board not connected".to_string(),
            ],
        );
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

    fn remembered_fixture() -> RememberedView {
        RememberedView {
            id: lpa_studio_core::DeviceId(7),
            title: "Porch sign".to_string(),
            board: Some("seeed-xiao-esp32c6".to_string()),
            last_seen_label: Some("last heard 4 min ago".to_string()),
            escapes: vec![DeviceEscape::Reconnect, DeviceEscape::Forget],
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
            degraded: None,
            loaded_project: lpa_studio_core::DeviceLoadedProject::Unknown,
            can_receive_project: false,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            escapes: vec![DeviceEscape::Forget],
        }
    }

    fn lpa_devices_event_attach() -> lpa_studio_core::DeviceEvent {
        lpa_studio_core::DeviceEvent::LinkAttached {
            link: lpa_studio_core::DeviceLinkId(1),
            info: lpa_studio_core::DeviceLinkInfo {
                label: "usb-1".to_string(),
                endpoint: lpa_studio_core::DeviceEndpointKey("usb-1".to_string()),
                usb: None,
                serial_number: None,
            },
        }
    }
}

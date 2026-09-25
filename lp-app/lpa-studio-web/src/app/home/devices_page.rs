//! The Devices page (`/devices`, vision D9): the runtime roster.
//!
//! One section, one source: the `lpa-devices` roster's projection — one
//! card per known device (real, emu, or sim; a sim is a device, same as
//! any board), one entry per link still being identified, and one button
//! to ask the browser for another port.
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
    device_escape_action_for, split_roster,
};

use crate::app::home::ble_reach::{BleReach, use_ble_reach};
use crate::app::home::device_roster_card::{DeviceRosterCard, PendingLinkCard};
use crate::app::home::play_feed_text::frame_age_label;
use crate::app::home::reach_note::{ReachCopy, ReachNote, USB_UNAVAILABLE, this_page_url};
use crate::app::home::target_pick_popover::TargetPickPopover;
use crate::app::home::{device_grid_class, section_title_class};
use crate::app::node::lamp_view::LampView;
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
    /// Story-only: mount the add slot's target menu open, for the same
    /// reason — a capture cannot click a trigger, and the dropdown is the
    /// half of D44 worth reviewing.
    #[props(default)]
    target_pick_open: bool,
    /// Stories only: the access settings section's view (the app reads it
    /// from its access context).
    #[props(default)]
    bluetooth_settings: Option<lpa_studio_core::UiDeviceSettingsView>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let access_ui = super::access_ui_context::use_access_ui();
    let account_ui = crate::cloud::account_access::use_account_access_ui();
    let bluetooth =
        bluetooth_settings.or_else(|| access_ui.map(|ui| ui.device_settings.read().clone()));
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
                                // The board's own picture, joined at the
                                // app view; absent = the slot's sentence.
                                feed: devices.feeds.get(&card.id).cloned(),
                                // The runtime band, for a device that is
                                // not silicon; absent = a real board.
                                runtime: devices.runtime_bands.get(&card.id).cloned(),
                                // Its login line and access panel (BLE M6).
                                access: devices.access.get(&card.id).cloned(),
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
                        AddDeviceCard {
                            pick_open: target_pick_open,
                            usb_available: devices.usb_available,
                            on_action,
                        }
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

            // Unlocking your devices: this browser's name, the account's
            // key and passwords, remembered passwords (spike §6).
            if let Some(settings) = bluetooth {
                super::access_settings_section::AccessSettingsSection {
                    settings,
                    account: account_ui
                        .map(|ui| ui.state.read().clone())
                        .unwrap_or(crate::cloud::account_access::AccountAccessState::SignedOut),
                    platform: super::browser_identity::detect_platform(),
                    browser: super::browser_identity::detect_browser().to_string(),
                    on_access: move |command| {
                        if let Some(ui) = access_ui {
                            ui.on_access.call(command);
                        }
                    },
                    on_set_password: move |change| {
                        if let Some(ui) = account_ui {
                            ui.set_password.call(change);
                        }
                    },
                    on_reset_key: move |_| {
                        if let Some(ui) = account_ui {
                            ui.reset_key.call(());
                        }
                    },
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
/// # "Connect a board", two transports, one detour (D44, M5, G3)
///
/// The heading says the goal; the two buttons say only the path —
/// **via USB** (the spectrum Primary: a board on the desk is the common
/// case) and **via Bluetooth**. Both are ALWAYS drawn (G3, 2026-09-24): a
/// transport this browser cannot drive is DISABLED with its reason under
/// it and a way to continue — the Bluefy link, the Brave flag, this page's
/// address to open where it works — rather than hidden, so a phone visitor
/// learns USB exists and where it works. Below them, **start a board here
/// ▾** is the quiet detour that opens the target menu; its panel floats in
/// the top layer, so the slot is the same height open or shut.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AddDeviceCard(
    /// Stories only: mount the target menu open (a capture cannot click).
    #[props(default = false)]
    pick_open: bool,
    /// Stories only: pin what the Bluetooth half says. Real surfaces ask the
    /// browser (`use_ble_reach`).
    #[props(default = None)]
    ble_reach: Option<BleReach>,
    /// Whether this browser can reach a USB port (Web Serial, or the
    /// `?emu=` shim that polyfills it). Where it cannot — iPhone, Bluefy,
    /// Firefox, Safari — the USB button is drawn disabled with its reason.
    #[props(default = true)]
    usb_available: bool,
    /// Stories only: the address the copy lines show, pinned so a capture
    /// does not print the story server's own URL. Real surfaces read the
    /// page's.
    #[props(default = None)]
    page_url: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let asked = use_ble_reach();
    let ble = ble_reach.unwrap_or_else(|| asked());
    let verbs = add_slot_verbs(usb_available, ble);
    let page_url = page_url.unwrap_or_else(this_page_url);
    let usb_action = transport_action(DeviceAction::AddFromUsb, verbs.usb.enabled, verbs.usb.note);
    let ble_action = transport_action(DeviceAction::AddFromBle, verbs.ble.enabled, verbs.ble.note);
    rsx! {
        div { class: "tw:flex tw:min-h-40 tw:flex-col tw:items-center tw:justify-center tw:gap-3 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:bg-transparent tw:px-5 tw:py-6",
            // The invitation is transport-OPEN: connecting is the goal, and
            // each button names only its path.
            p { class: "tw:m-0 tw:text-center tw:text-sm tw:font-semibold tw:text-strong-foreground",
                "Connect a board"
            }
            // One column, both buttons the full width of it, so the two
            // paths read as a pair whatever each says under it. USB wears
            // the Primary spectrum ring, Bluetooth the Secondary tier.
            div { class: "tw:grid tw:w-full tw:max-w-64 tw:gap-3 tw:text-center",
                TransportOffer {
                    action: usb_action,
                    note: verbs.usb.note,
                    page_url: page_url.clone(),
                    on_action,
                }
                TransportOffer {
                    action: ble_action,
                    note: verbs.ble.note,
                    page_url,
                    on_action,
                }
            }
            span { class: add_slot_or_class(), "or" }
            TargetPickPopover { initially_open: pick_open, on_action }
        }
    }
}

/// One transport's button and, when it is disabled, the way forward under
/// it: the reason (the button's own disabled reason), an optional link
/// out, and the text to select and copy — shown in full, never folded.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn TransportOffer(
    action: UiAction,
    note: Option<ReachNote>,
    page_url: String,
    on_action: EventHandler<UiAction>,
) -> Element {
    let copy = note.and_then(|note| note.copy).map(|copy| match copy {
        ReachCopy::Text(text) => text.to_string(),
        ReachCopy::ThisPage => page_url.clone(),
    });
    rsx! {
        div { class: "tw:grid tw:w-full tw:gap-1",
            ActionButton { action, running: false, on_action }
            if let Some(note) = note {
                if let Some(link) = note.link {
                    a {
                        class: "tw:justify-self-center tw:text-xs tw:font-semibold tw:text-strong-foreground tw:underline tw:underline-offset-2",
                        href: link.href,
                        target: "_blank",
                        rel: "noopener",
                        "{link.label}"
                    }
                }
                if let Some(lead) = note.copy_lead.filter(|_| copy.is_some()) {
                    p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground",
                        "{lead}"
                    }
                }
                if let Some(copy) = copy {
                    code { class: "tw:justify-self-center tw:select-all tw:[overflow-wrap:anywhere] tw:rounded-sm tw:bg-card-muted tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[11px] tw:text-strong-foreground",
                        "{copy}"
                    }
                }
            }
        }
    }
}

/// The add-slot button for one transport: the op's own label and icon,
/// disabled with the note's reason where this browser cannot drive it.
/// Disabled with no note is the Bluetooth answer still on its way — a
/// button that could only fail must not be live for that moment either.
fn transport_action(action: DeviceAction, enabled: bool, note: Option<ReachNote>) -> UiAction {
    let action = DevicesOp::action_for(action);
    if enabled {
        action
    } else {
        action.disabled(note.map_or("", |note| note.reason))
    }
}

/// What the add slot offers, decided apart from the component so it is
/// testable: both buttons, each enabled only where this browser can drive
/// it, each with its note where it cannot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AddSlotVerbs {
    /// "via USB".
    pub usb: TransportVerb,
    /// "via Bluetooth".
    pub ble: TransportVerb,
}

/// One transport's button state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TransportVerb {
    pub enabled: bool,
    /// Said under the disabled button: why, and how to go on.
    pub note: Option<ReachNote>,
}

pub(crate) fn add_slot_verbs(usb_available: bool, ble: BleReach) -> AddSlotVerbs {
    let ble_note = ble.note();
    let mut usb_note = (!usb_available).then_some(USB_UNAVAILABLE);
    // Firefox, desktop Safari: BOTH paths send you to Chrome or Edge, with
    // the same "open this page there" — say the address once, under the
    // lower button, rather than twice in a row.
    if let (Some(usb), Some(ble)) = (usb_note.as_mut(), ble_note)
        && usb.copy == ble.copy
        && usb.copy_lead == ble.copy_lead
    {
        usb.copy_lead = None;
        usb.copy = None;
    }
    AddSlotVerbs {
        usb: TransportVerb {
            enabled: usb_available,
            note: usb_note,
        },
        ble: TransportVerb {
            enabled: ble.offers_verb(),
            note: ble_note,
        },
    }
}

/// The "or" before the slot's detour: the quietest possible separator,
/// because the offers are not equal — connecting a board is the common
/// case and starting one here is the deliberate detour.
fn add_slot_or_class() -> &'static str {
    "tw:text-[10px] tw:tracking-wide tw:text-dim-foreground tw:uppercase"
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
                 other Chromium browsers support. A sim runs anywhere."
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
/// reads as the same family. When the board's last picture is known — this
/// session pulled one before the port went, or a sidecar remembered one
/// across a reload — the slot draws it exactly as a card's Offline look
/// does: dimmed, with the neutral "last frame · <age>" pill, the age
/// measured from when the board actually published it. Otherwise the
/// "last seen" sentence: never a stale picture passed off as current, and
/// never an empty box.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RememberedTile(entry: RememberedView, on_action: EventHandler<UiAction>) -> Element {
    let device = entry.id;
    let meta = remembered_meta_text(&entry);
    let slot = remembered_slot(&entry);

    rsx! {
        div { class: remembered_tile_class(),
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
            }
            // Every escape the projection granted, rendered — the renderer
            // half of invariant I3, exactly as on a card. Reconnect is the
            // tile's one call to action (a grant can die on a replug), so
            // it wears the Outline voice; Forget keeps its inline confirm.
            div { class: "tw:mt-auto tw:flex tw:flex-wrap tw:items-center tw:gap-2 tw:whitespace-nowrap",
                for escape in entry.escapes.iter().copied() {
                    ActionButton {
                        key: "{escape:?}",
                        action: device_escape_action_for(escape, device, entry.face),
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

/// What a remembered tile's preview slot draws.
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
            access: Default::default(),
            roster,
            transport_available,
            usb_available: transport_available,
            open_addresses: Default::default(),
            feeds: Default::default(),
            runtime_bands: Default::default(),
        }
    }

    /// D44 + G3: the slot offers BOTH ways a card can appear, and each
    /// verb's words come from the place that owns them — core's action
    /// vocabulary for the two transports under "Connect a board", the target
    /// menu for the board Studio is about to start.
    #[test]
    fn the_add_slot_offers_both_ways_a_card_can_appear() {
        let usb = DevicesOp::action_for(DeviceAction::AddFromUsb);
        let ble = DevicesOp::action_for(DeviceAction::AddFromBle);

        assert_eq!(usb.meta().label, "via USB");
        assert_eq!(usb.meta().icon.as_deref(), Some("usb"));
        assert_eq!(ble.meta().label, "via Bluetooth");
        assert_eq!(ble.meta().icon.as_deref(), Some("bluetooth"));
        assert_eq!(
            crate::app::home::target_pick_popover::SLOT_VERB_LABEL,
            "start a board here"
        );
    }

    /// G3: both buttons are ALWAYS drawn. Where this browser cannot drive a
    /// transport its button is disabled with the reason, plus a way to go
    /// on — never hidden, never a verb that can only fail.
    #[test]
    fn a_transport_this_browser_cannot_drive_is_disabled_with_a_way_forward() {
        // Chrome/Edge on a computer: both live, nothing to explain.
        let chrome = add_slot_verbs(true, BleReach::Ready);
        assert_eq!(
            chrome,
            AddSlotVerbs {
                usb: TransportVerb {
                    enabled: true,
                    note: None
                },
                ble: TransportVerb {
                    enabled: true,
                    note: None
                },
            }
        );

        // Bluefy: no Web Serial. USB disabled, says where it works and
        // gives this page's address to open there; Bluetooth live.
        let bluefy = add_slot_verbs(false, BleReach::Ready);
        assert!(!bluefy.usb.enabled);
        assert_eq!(bluefy.usb.note, Some(USB_UNAVAILABLE));
        assert_eq!(
            USB_UNAVAILABLE.reason,
            "USB needs Chrome or Edge on a computer."
        );
        assert_eq!(USB_UNAVAILABLE.copy, Some(ReachCopy::ThisPage));
        assert!(bluefy.ble.enabled);
        assert_eq!(bluefy.ble.note, None);

        // iPhone Safari / Chrome on iOS: both disabled; Bluetooth points to
        // Bluefy on the App Store and then this page, USB keeps its own.
        let ios = add_slot_verbs(false, BleReach::Ios);
        assert!(!ios.usb.enabled && !ios.ble.enabled);
        let ble_note = ios.ble.note.expect("the Bluefy path");
        assert_eq!(
            ble_note.link.map(|link| link.href),
            Some(crate::app::home::reach_note::BLUEFY_APP_STORE_URL)
        );
        assert_eq!(ios.usb.note, Some(USB_UNAVAILABLE));

        // Brave on a computer: USB live, Bluetooth disabled with the flag.
        let brave = add_slot_verbs(true, BleReach::Brave);
        assert!(brave.usb.enabled && !brave.ble.enabled);
        assert_eq!(
            brave.ble.note.and_then(|note| note.copy),
            Some(ReachCopy::Text(
                crate::app::home::ble_reach::BRAVE_BLUETOOTH_FLAG
            ))
        );

        // Firefox / desktop Safari: both go to Chrome or Edge — the page's
        // address is said ONCE (under Bluetooth), not twice in a row.
        let firefox = add_slot_verbs(false, BleReach::Firefox);
        let usb_note = firefox.usb.note.expect("USB still says why");
        assert_eq!(usb_note.reason, USB_UNAVAILABLE.reason);
        assert_eq!(usb_note.copy, None, "the address is not repeated");
        assert_eq!(
            firefox.ble.note.and_then(|note| note.copy),
            Some(ReachCopy::ThisPage)
        );

        // The answer still on its way: disabled, nothing said yet.
        let checking = add_slot_verbs(true, BleReach::Checking);
        assert_eq!(
            checking.ble,
            TransportVerb {
                enabled: false,
                note: None
            }
        );
    }

    /// The disabled button carries the note's reason as its own disabled
    /// reason, so it is said directly under the button.
    #[test]
    fn a_disabled_transport_button_carries_its_reason() {
        let action = transport_action(DeviceAction::AddFromUsb, false, Some(USB_UNAVAILABLE));
        assert_eq!(
            action.meta().enablement,
            lpa_studio_core::ActionEnablement::Disabled {
                reason: USB_UNAVAILABLE.reason.to_string()
            }
        );
        assert!(
            transport_action(DeviceAction::AddFromBle, true, None)
                .meta()
                .enablement
                .is_enabled()
        );
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
            face: lpa_studio_core::DeviceFace::Wire,
            feed: None,
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
            can_receive_project: false,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
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

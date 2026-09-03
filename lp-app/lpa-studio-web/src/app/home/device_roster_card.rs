//! Device cards, rendered straight off the model's projection.
//!
//! There is no view model between `lpa-devices` and this file: a
//! [`DeviceView`] already IS the card (title, state line, detail, freshness,
//! activity, escapes), computed as a pure function of intent + evidence +
//! activity. So this renderer makes no decisions about devices — it lays out
//! what the fold concluded, and every affordance it draws comes from the DTO.
//!
//! Design record: `spikes/device-card-v2/index.html` §1 (rounds 1–3, gate
//! 2026-09-02). Production never imports from `spikes/`.
//!
//! # One box, four zones (D3 — full-bleed separators)
//!
//! ```text
//!   header      title + status chip + board · chip · MAC · firmware
//!   ──────────── (full bleed)
//!   state       the fixed rows: what it is, what is happening, its verbs
//!   ────────────
//!   terminal    what the board actually said, and what Studio did to it
//!   ────────────
//!   actions     every escape, plus the device verbs
//! ```
//!
//! The `article` carries **no padding**; each zone is a `section` with its
//! own `px-4` and, for every zone but the first, a `border-t
//! border-border-strong` hairline that runs edge to edge. That is the node
//! cards' own section grammar
//! ([`section_container_class`](crate::app::node::face::node_card_section))
//! and it is what makes the card read as one surface with divisions rather
//! than as a stack of framed widgets. Nothing inside a zone may grow its own
//! frame — a bordered sub-panel puts a box inside the box and the card stops
//! being one thing.
//!
//! # The state zone's rows are FIXED (D4 — AC2)
//!
//! Five rows, in this order, present in every state:
//!
//! | row | height | what it holds |
//! |---|---|---|
//! | state line | 17px | activity (+ %) → fault → freshness → detail, one line, ellipsised, `title` = the full text |
//! | progress slot | 4px | unlit when idle; the activity's fill or sweep otherwise |
//! | preview slot | 120px | the picture, or an honest sentence about why there is none (AC10) |
//! | meta line | 17px | the project name (running), "Nothing loaded", "No firmware" |
//! | verb row | 30px | the PROJECT verbs — empty during an activity (D9) |
//!
//! A board event — a heartbeat, a fault, a lost link, a new terminal line —
//! must never change a card's height. Fixed rows plus a fixed-height
//! terminal are how that is enforced: a box that is always the same size
//! cannot resize the card, and a card that cannot resize cannot make the
//! gallery jump while a flash runs. Content that does not fit a row is
//! TRUNCATED (with the full text on the `title`), never allowed to wrap.
//!
//! # Where the verbs live (AC6)
//!
//! - **The verb row** holds every verb that can need a PICKER, plus the
//!   project verbs: `Open`, `Clear faults` and the re-flash `Flash firmware`
//!   on a running board, `Remove project` on the right; the picker trigger
//!   plus one CTA on the empty and needs-firmware faces. During an activity
//!   it is empty at its height — the escape is the footer's.
//! - **The footer** holds the device verbs that never ask anything: every
//!   escape the DTO carries, then `Reset`, `Factory reset` and `Forget`.
//!
//! The re-flash verb came UP out of the footer (P6) for one reason: on a
//! chip with several fitting boards it has to ask which, and a picker may
//! only open from the verb row — a panel under the footer would change the
//! card's height, which AC2 forbids. Its unresolved case is therefore the
//! same chip re-dressed as [`BoardPickPopover`]'s trigger, and picking a
//! board flashes it.
//!
//! Arming any destructive chip marks the whole card (`.ux-armed-scope:has()`
//! in style.css, D8). Arming dims what the card SAYS (`ux-armed-dim` on the
//! header, the four upper rows and the terminal) and never what it OFFERS —
//! the verb row and the footer keep full contrast, so the chip that is
//! asking is always legible.
//!
//! Two rules it also has to keep:
//!
//! 1. **Every escape the DTO carries is rendered, in every state.** Invariant
//!    I3 lives in the model, but a renderer that dropped an escape would
//!    defeat it from outside, which is exactly how the shipped card lost its
//!    danger zone in the states that needed it.
//! 2. **Nothing is offered that cannot happen.** A pick that does not resolve
//!    leaves its verb honestly disabled rather than guessing a board or a
//!    project.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceAction, DeviceActivityView, DeviceEscape, DeviceLoadedProject, DeviceStatus, DeviceView,
    DevicesOp, PendingLinkView, UiAction, UiExampleCard, UiPackageCard, UiStatus, device_chip,
    device_escape_action, device_identity_line, device_status_kind, pending_escape_action,
    reflash_choice,
};

use super::device_pick_popover::{
    BoardPickMode, BoardPickPopover, ChipSource, ProjectPickPopover, joined_chip,
};
use super::device_terminal::DeviceTerminal;
use crate::core::{ActionButton, ActionButtonVariant, StatusChip};

/// One device card.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceRosterCard(
    card: DeviceView,
    projects: Vec<UiPackageCard>,
    examples: Vec<UiExampleCard>,
    /// Story-only: render the Forget escape already ARMED, so captures can
    /// show the 2K+ armed dress and the card's `:has()` marking. Real
    /// surfaces never set this.
    #[props(default)]
    armed_preview: bool,
    /// The device's editor address (`/device/<uid>`), when it is
    /// registered — the running face's Open (round-2 M5). `None` for a
    /// board that has not earned a registry row yet: no honest address, no
    /// verb.
    #[props(default)]
    open_uid: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let device = card.id;
    let status = UiStatus {
        label: card.state_label.clone(),
        kind: device_status_kind(card.status),
    };
    // The flash face appears on a settled needs-firmware verdict, never
    // while an activity runs (the verb row is withdrawn then).
    let offer_flash = card.needs_firmware && card.activity.is_none();
    // The empty face: a LightPlayer that has REPORTED nothing loaded. A
    // board that simply has not said yet gets neither face — see
    // `DeviceLoadedProject::Unknown`.
    let offer_push = card.can_receive_project && card.loaded_project == DeviceLoadedProject::Empty;
    let running = match &card.loaded_project {
        DeviceLoadedProject::Running { label } => Some(label.clone()),
        DeviceLoadedProject::Empty | DeviceLoadedProject::Unknown => None,
    };
    // "Linked" in projection terms: Disconnect is offered exactly when the
    // model has a link for this device. It gates the terminal zone (an
    // offline card has no wire to show) and the always-actions that need a
    // port, which is the same condition the model's own spawns check.
    let linked = card.escapes.contains(&DeviceEscape::Disconnect);
    let idle = card.activity.is_none();
    // The one condition Clear faults turns on. The status is the derived
    // headline — a board is Degraded exactly when it reported a faulted node
    // or a non-green recovery state — so the verb appears with the attention
    // chip and the fault line, and leaves with them.
    let degraded = card.status == DeviceStatus::Degraded;
    // Re-flash on a RUNNING board (G1 2026-09-02: the only road to newer
    // firmware was Factory reset, which "causes issues sometimes"). The
    // pick is not the user's here — it is the board this card is registered
    // as, resolved against the served catalog for the JOINED chip; when the
    // registry has no board, a chip with exactly one fit still earns the
    // verb. Safe on a live board: the Flash activity parks a native-USB
    // chip in its ROM downloader first, and the merged image ends before
    // the lpfs partition, so the project and the efuse identity survive.
    let chip = joined_chip(&card);
    let reflash = reflash_choice(device_chip(&card).as_deref(), card.board_id.as_deref());
    // When the pick does NOT resolve (a chip with several boards and no
    // registered one — every board flashed before the hello carried its
    // board id), the SAME quiet chip becomes the board popover's trigger
    // instead of guessing, and picking a board flashes it. The popover's
    // panel floats in the top layer, so opening it cannot change the card's
    // height — which is why the verb could come up out of the footer at all.
    let offer_reflash_picker = reflash.is_none()
        && chip.is_some()
        && matches!(card.status, DeviceStatus::Ready | DeviceStatus::Degraded);
    // The running face's ONE Primary: Open — the editor as a lens on this
    // board. Opening is NAVIGATION, so it is a real `<a>` to the device
    // route (the same road the project cards take): a plain click rides the
    // route listener, cmd/middle-click opens a tab natively, and the
    // address bar ends up saying where you are.
    // Only a READY board (identified, port open, idle) can be opened: an
    // attached-but-closed one has no wire to lend, and offering Open there
    // would hold an intent the user did not mean to file. Degraded is a
    // refinement of Ready (the port is open, the show is running, one node
    // faulted) — the editor is exactly where a faulted board wants to be
    // opened, so it keeps the verb.
    let ready = matches!(card.status, DeviceStatus::Ready | DeviceStatus::Degraded);
    let open_href = match (&running, ready && linked && idle, open_uid.as_deref()) {
        (Some(_), true, Some(uid)) => Some(format!("/device/{uid}")),
        _ => None,
    };

    let identity = device_identity_line(&card).display();
    let state_line = state_line_text(&card);
    // The fault takes the state line only when nothing is running: an
    // activity's own narration outranks it (the terminal keeps the fault).
    let state_line_is_fault = idle && card.degraded.is_some();

    rsx! {
        article { class: card_class(),
            // ── zone 1: header ──────────────────────────────────────────
            header { class: zone_class(true),
                div { class: "ux-armed-dim tw:grid tw:min-w-0 tw:gap-1.5",
                    div { class: "tw:flex tw:items-start tw:justify-between tw:gap-3",
                        h3 {
                            class: "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground",
                            title: "{card.title}",
                            "{card.title}"
                        }
                        StatusChip { status }
                    }
                    // board · chip · MAC · firmware (AC3), joined in core so
                    // a hello-only board still names its chip.
                    p { class: mono_line_class(), title: "{identity}", "{identity}" }
                }
            }

            // ── zone 2: state — the five fixed rows (D4) ────────────────
            section { class: zone_class(false),
                div { class: "ux-armed-dim tw:grid tw:min-w-0 tw:gap-2",
                    // 1 · state line (17px, one line, full text on hover)
                    p {
                        class: if state_line_is_fault { fault_line_class() } else { state_line_class() },
                        title: "{state_line}",
                        "{state_line}"
                    }
                    // 2 · progress slot (4px, unlit when idle)
                    div { class: progress_slot_class(card.activity.is_some()),
                        if let Some(activity) = card.activity.clone() {
                            div {
                                class: progress_fill_class(activity.percent),
                                style: progress_fill_style(activity.percent),
                            }
                        }
                    }
                    // 3 · preview slot (120px, AC10). No feed yet — the
                    // honest sentence instead of a fake picture. The
                    // `ux-play-pill` liveness slot belongs top-right inside
                    // this frame; the feed milestone drops it in without
                    // moving anything, because the frame's height is fixed.
                    div { class: preview_frame_class(),
                        div { class: "ux-play-empty",
                            p { class: "tw:m-0", "{preview_sentence(&card)}" }
                        }
                    }
                    // 4 · meta line (17px). The right half is the feed's
                    // fps slot; empty until it lands.
                    div { class: meta_row_class(),
                        if let Some(project) = running.clone() {
                            span { class: meta_name_class(), title: "{project}", "{project}" }
                        } else if card.needs_firmware {
                            span { class: meta_quiet_class(), "No firmware" }
                        } else if card.loaded_project == DeviceLoadedProject::Empty {
                            span { class: meta_quiet_class(), "Nothing loaded" }
                        }
                        span { class: "tw:flex-1" }
                    }
                }
                // 5 · verb row (30px) — outside `ux-armed-dim`: arming dims
                // what the card says, never what it offers.
                div { class: verb_row_class(),
                    if card.activity.is_some() {
                        // D9: the row is kept at its height and withdrawn.
                        // Cancel is the footer's escape.
                    } else if offer_flash {
                        BoardPickPopover { device, chip: chip.clone(), on_action }
                    } else if offer_push {
                        ProjectPickPopover { card: card.clone(), projects, examples, on_action }
                    } else {
                        if let Some(href) = open_href.clone() {
                            a {
                                class: row_cta_class(),
                                href: "{href}",
                                title: "Open this board in the editor",
                                "Open"
                            }
                        }
                        // Only on a board that has SAID it is degraded: a
                        // verb to forget faults offered over a healthy card
                        // would invite a gesture with nothing to do.
                        if degraded && linked {
                            ActionButton {
                                key: "{\"clear-faults\"}",
                                action: DevicesOp::action_for(DeviceAction::ClearFaults { device }),
                                running: false,
                                variant: ActionButtonVariant::Quiet,
                                on_action,
                            }
                        }
                        // Re-flash (#500) lives HERE, not in the footer: it
                        // is the one device verb that needs a picker, and
                        // the verb row is the only place a picker may
                        // appear. A resolved pick dispatches Flash straight
                        // away; an unresolved one turns the same chip into
                        // the board popover's trigger.
                        if let Some(choice) = reflash.clone()
                            && idle
                            && linked
                            && !offer_flash
                        {
                            ActionButton {
                                key: "{\"flash-firmware\"}",
                                action: DevicesOp::action_for(DeviceAction::Flash {
                                    device,
                                    board_id: choice.board_id.clone(),
                                    build_id: choice.build_id.clone(),
                                    park_first: choice.park_first,
                                }),
                                running: false,
                                variant: ActionButtonVariant::Quiet,
                                on_action,
                            }
                        }
                        if offer_reflash_picker && idle && linked && !offer_flash {
                            BoardPickPopover {
                                device,
                                chip: chip.clone(),
                                mode: BoardPickMode::Verb,
                                on_action,
                            }
                        }
                        span { class: "tw:flex-1" }
                        // Only when the board has SAID it is running
                        // something: a delete offered over a board that
                        // never reported one would be a verb aimed at a
                        // guess.
                        if card.can_remove_project {
                            ActionButton {
                                key: "{\"remove-project\"}",
                                action: DevicesOp::action_for(DeviceAction::RemoveProject { device }),
                                running: false,
                                variant: ActionButtonVariant::Quiet,
                                on_action,
                            }
                        }
                    }
                }
            }

            // ── zone 3: terminal ────────────────────────────────────────
            // Always present on a linked card, empty or not: a panel that
            // comes and goes is a panel that resizes the card, which is the
            // churn the fixed rows exist to stop.
            if linked {
                DeviceTerminal {
                    lines: card.terminal.clone(),
                    dropped: card.terminal_dropped,
                    height_class: "tw:h-40",
                }
            }

            // ── zone 4: actions — the DEVICE verbs (AC6) ────────────────
            // Every escape the projection carries, in every state — including
            // Forget mid-activity, which the shipped system could not do.
            // The always-actions join when the board is linked and idle (the
            // same condition the model's spawns check). Project verbs moved
            // up into the verb row.
            footer { class: actions_zone_class(),
                for escape in card.escapes.iter().copied() {
                    ActionButton {
                        key: "{escape:?}",
                        action: device_escape_action(escape, device),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        armed_preview: armed_preview && escape == DeviceEscape::Forget,
                        on_action,
                    }
                }
                // The two device verbs that never ask a question. The
                // re-flash, which does, moved up into the verb row (P6) —
                // its picker could not open from here without changing the
                // card's height.
                if idle && linked {
                    ActionButton {
                        key: "{\"reset-board\"}",
                        action: DevicesOp::action_for(DeviceAction::ResetBoard { device }),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
                if idle && linked {
                    ActionButton {
                        key: "{\"factory-reset\"}",
                        action: DevicesOp::action_for(DeviceAction::Erase { device }),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
            }
        }
    }
}

/// The roster's "new device found, identifying…" entry.
///
/// It is deliberately not a device card: nothing about it is known yet, and
/// promoting it to one before the fold settles is how the shipped system ended
/// up with two cards for one board. Once the verdict settles at
/// needs-firmware, the SAME board pick appears here — the gesture adopts the
/// link, so flashing IS the "keep this one" decision.
///
/// Same zone grammar as [`DeviceRosterCard`] (full bleed, own padding), with
/// the rows a link that has not identified itself can honestly fill: no
/// preview slot and no meta line, because there is no project and no picture
/// to speak of.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PendingLinkCard(
    pending: PendingLinkView,
    on_action: EventHandler<UiAction>,
) -> Element {
    let link = pending.link;
    let status = UiStatus {
        label: "Identifying".to_string(),
        kind: lpa_studio_core::UiStatusKind::Working,
    };
    // The identity a link that has said nothing yet actually has: the chip
    // off the boot banner, and the honest statement that nothing else is
    // knowable until firmware runs on it.
    let identity = match pending.detected_chip.as_deref() {
        Some(chip) => format!("chip: {chip} · no identity until flashed"),
        None => "no identity yet".to_string(),
    };
    let state_line = match pending.detail.as_deref() {
        Some(detail) => format!("{} · {detail}", pending.state_label),
        None => pending.state_label.clone(),
    };

    rsx! {
        article { class: card_class(),
            header { class: zone_class(true),
                div { class: "ux-armed-dim tw:grid tw:min-w-0 tw:gap-1.5",
                    div { class: "tw:flex tw:items-start tw:justify-between tw:gap-3",
                        h3 {
                            class: "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground",
                            title: "{pending.title}",
                            "{pending.title}"
                        }
                        StatusChip { status }
                    }
                    p { class: mono_line_class(), title: "{identity}", "{identity}" }
                }
            }

            section { class: zone_class(false),
                div { class: "ux-armed-dim tw:grid tw:min-w-0 tw:gap-2",
                    p { class: state_line_class(), title: "{state_line}", "{state_line}" }
                    // Identification carries no percentage, so the slot sits
                    // unlit — present so the pending card and the device
                    // card share one row grammar.
                    div { class: progress_slot_class(false) }
                }
                div { class: verb_row_class(),
                    if pending.needs_firmware {
                        // The same popover the device card's firmware face
                        // wears: a blank chip's only chip fact is its ROM
                        // boot banner.
                        BoardPickPopover {
                            device: pending.device,
                            chip: pending
                                .detected_chip
                                .clone()
                                .map(|chip| (chip, ChipSource::BootBanner)),
                            on_action,
                        }
                    } else if pending.can_adopt {
                        // A blank chip may never identify itself, so a user
                        // gesture must be able to keep it. On a
                        // needs-firmware verdict the Flash verb IS that
                        // gesture; here the plain adopt is live.
                        ActionButton {
                            action: DevicesOp::action_for(DeviceAction::AdoptLink { link }),
                            running: false,
                            variant: ActionButtonVariant::Quiet,
                            on_action,
                        }
                    }
                }
            }

            DeviceTerminal { lines: Vec::new(), dropped: 0, height_class: "tw:h-24" }

            footer { class: actions_zone_class(),
                for escape in pending.escapes.iter().copied() {
                    ActionButton {
                        key: "{escape:?}",
                        action: pending_escape_action(escape, link),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
                // The silent-board recovery: a chip parked in ROM
                // download-wait prints nothing, so identify can never
                // settle — a hardware reset reboots it into honest boot
                // output (G1 2026-08-31, the erased C6).
                ActionButton {
                    key: "{\"reset-board\"}",
                    action: DevicesOp::action_for(DeviceAction::ResetBoard {
                        device: pending.device,
                    }),
                    running: false,
                    variant: ActionButtonVariant::Quiet,
                    on_action,
                }
            }
        }
    }
}

/// The verb row's Primary: the same standing spectrum ring the running
/// face's Open wears, sized to the fixed 30px row. It reads its label and
/// its explanation off the action's own meta, so the action model stays the
/// single source for both.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(super) fn RowCta(action: UiAction, on_action: EventHandler<UiAction>) -> Element {
    let meta = action.meta().clone();
    let dispatch = action.clone();

    rsx! {
        button {
            class: row_cta_class(),
            r#type: "button",
            title: "{meta.summary}",
            onclick: move |_| on_action.call(dispatch.clone()),
            "{meta.label}"
        }
    }
}

/// The same verb with nothing behind it yet: the pick did not resolve, so
/// the button says what it is waiting for instead of guessing.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(super) fn RowCtaDisabled(label: String, hint: String) -> Element {
    rsx! {
        button {
            class: row_cta_disabled_class(),
            r#type: "button",
            disabled: true,
            title: "{hint}",
            "{label}"
        }
    }
}

/// The state line's text, by the ruled priority (D4): a running activity
/// outranks everything, then the fault a degraded board reported, then
/// honest staleness, then the evidence detail. One line — the renderer
/// truncates it and hangs the full text on the `title`.
///
/// A finished activity's outcome is deliberately NOT here: it is a line in
/// the terminal (typed `Outcome`/`Failure` by the fold), and giving it a row
/// of its own would be a row that exists in some states and not others.
fn state_line_text(card: &DeviceView) -> String {
    if let Some(activity) = &card.activity {
        return activity_line_text(activity);
    }
    if let Some(fault) = &card.degraded {
        return fault.clone();
    }
    card.freshness_label
        .clone()
        .or_else(|| card.detail.clone())
        .unwrap_or_default()
}

/// An activity as one line: its label, the cancel it has been asked for, and
/// its percentage when it has one.
///
/// A requested cancel is a STATE, not the absence of one: the activity is
/// winding down and will be evicted if it does not. Saying so beats a button
/// that has stopped responding. A flash's cancel can hold for the rest of
/// the write window — esptool cannot stop mid-image cleanly.
fn activity_line_text(activity: &DeviceActivityView) -> String {
    let mut text = match activity.cancel_requested {
        true => format!(
            "{} — cancelling (finishing the current write)",
            activity.label
        ),
        false => activity.label.clone(),
    };
    if let Some(percent) = activity.percent {
        text.push_str(&format!(" · {}%", u32::from(percent).min(100)));
    }
    text
}

/// The preview slot's sentence while there is no feed (AC10): why there is
/// no picture, in this state, in plain words — never a fake picture and
/// never an empty box.
fn preview_sentence(card: &DeviceView) -> String {
    if card.needs_firmware && card.activity.is_none() {
        return "Nothing running — a blank chip has no picture.".to_string();
    }
    if let Some(activity) = &card.activity {
        let label = activity.label.trim_end_matches(['…', '.', ' ']);
        return format!("{label}… the picture returns when the board does.");
    }
    if card.loaded_project == DeviceLoadedProject::Empty {
        return "Nothing loaded — no picture until something runs.".to_string();
    }
    "No picture yet — the live feed is coming.".to_string()
}

/// The card itself: one box, no padding of its own (every zone carries its
/// own), and `overflow-hidden` so the full-bleed hairlines end at the
/// rounded corner instead of crossing it.
///
/// A COLUMN, not a grid: the footer's `mt-auto` has to be able to push it
/// to the bottom edge when the roster's grid stretches a short card to its
/// row's height, and `margin-top: auto` does nothing to a grid item under
/// `content-start`. So the zones stack as flex children, each sized to its
/// own content, and only the gap above the footer grows.
///
/// `ux-armed-scope`: the card is the blast radius of its own armed
/// destructive chips — `:has(.ux-armed)` marks it (style.css).
fn card_class() -> &'static str {
    "ux-armed-scope tw:flex tw:flex-col tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card"
}

/// One zone (D3): its own horizontal padding, and — for every zone but the
/// first — the full-bleed `border-strong` hairline that separates it from
/// the one above. Borrowed verbatim from the node cards' section grammar
/// (`node/face/node_card_section.rs::section_container_class`), which is
/// what makes a card read as one surface with divisions.
fn zone_class(first: bool) -> &'static str {
    if first {
        "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:pt-4 tw:pb-3"
    } else {
        "tw:grid tw:min-w-0 tw:gap-2 tw:border-t tw:border-border-strong tw:px-4 tw:py-3"
    }
}

/// The actions zone: the same full-bleed separator and padding, but the
/// buttons wrap in a row rather than stacking, and it is pushed to the
/// bottom so cards of different heights still line their action rows up.
/// The ROW wraps; a chip's LABEL never does (`whitespace-nowrap` inherits),
/// so a narrow card gets more chip rows rather than taller chips.
fn actions_zone_class() -> &'static str {
    "tw:mt-auto tw:flex tw:flex-wrap tw:items-center tw:gap-2 tw:whitespace-nowrap tw:border-t tw:border-border-strong tw:px-4 tw:py-3"
}

/// Row 1 — the state line: exactly 17px, one line, ellipsised. Never allowed
/// to wrap; the full text rides the `title`.
fn state_line_class() -> &'static str {
    "tw:m-0 tw:h-[17px] tw:truncate tw:text-xs tw:leading-[17px] tw:text-subtle-foreground"
}

/// The state line when a degraded board's fault is what it carries: the
/// Attention tone the status chip already wears for this state, semibold
/// because it is the reason the chip changed. NOT the error voice — an error
/// line is for a failed OUTCOME, and this board is still running.
fn fault_line_class() -> &'static str {
    "tw:m-0 tw:h-[17px] tw:truncate tw:text-xs tw:font-semibold tw:leading-[17px] tw:text-status-attention-foreground"
}

/// Row 2 — the progress slot: 4px, transparent (and therefore invisible)
/// when nothing is running, a track when something is. It occupies its 4px
/// either way, which is the point.
fn progress_slot_class(lit: bool) -> &'static str {
    if lit {
        "tw:h-1 tw:overflow-hidden tw:rounded-pill tw:bg-card-subtle"
    } else {
        "tw:h-1 tw:overflow-hidden tw:rounded-pill tw:bg-transparent"
    }
}

/// The progress fill: a measured percentage, or the indeterminate sweep for
/// an activity that cannot say how far along it is.
fn progress_fill_class(percent: Option<u8>) -> &'static str {
    match percent {
        Some(_) => "tw:h-full tw:rounded-pill tw:bg-status-working-foreground",
        None => {
            "tw:h-full tw:w-[35%] tw:rounded-pill tw:bg-status-working-foreground [animation:ux-progress-sweep_1.2s_ease-in-out_infinite]"
        }
    }
}

fn progress_fill_style(percent: Option<u8>) -> String {
    match percent {
        Some(percent) => format!("width: {}%;", u32::from(percent).min(100)),
        None => String::new(),
    }
}

/// Row 3 — the preview slot: the `ux-play-frame` look (near-black, border,
/// radius) at the card's fixed 120px.
fn preview_frame_class() -> &'static str {
    "ux-play-frame ux-play-frame-slot"
}

/// Row 4 — the meta line: 17px, the project name on the left, the feed's fps
/// slot (empty for now) on the right.
fn meta_row_class() -> &'static str {
    "tw:flex tw:h-[17px] tw:min-w-0 tw:items-center tw:gap-2"
}

fn meta_name_class() -> &'static str {
    "tw:min-w-0 tw:truncate tw:text-xs tw:font-semibold tw:leading-[17px] tw:text-strong-foreground"
}

fn meta_quiet_class() -> &'static str {
    "tw:min-w-0 tw:truncate tw:text-xs tw:leading-[17px] tw:text-subtle-foreground"
}

/// Row 5 — the verb row: 30px, always, whether it holds verbs or (during an
/// activity) nothing. `whitespace-nowrap` INHERITS into the chips, which is
/// what keeps a two-word verb ("Clear faults") on one line in a narrow card
/// instead of wrapping and bursting the row; `overflow-hidden` is the
/// backstop, so a long pick label clips rather than pushing the row taller.
fn verb_row_class() -> &'static str {
    "tw:flex tw:h-[30px] tw:min-w-0 tw:items-center tw:gap-1.5 tw:overflow-hidden tw:whitespace-nowrap"
}

/// The verb row's Primary voice — the standing spectrum ring every surface's
/// one primary verb wears, at the row's height. Shared by the running face's
/// Open anchor and the two pick rows' CTAs.
fn row_cta_class() -> &'static str {
    "ux-spectrum-cta ux-focus-ring tw:inline-flex tw:h-[26px] tw:flex-none tw:cursor-pointer tw:items-center tw:rounded-md tw:border tw:border-transparent tw:bg-transparent tw:px-3 tw:text-xs tw:font-bold tw:text-strong-foreground tw:no-underline"
}

fn row_cta_disabled_class() -> &'static str {
    "tw:inline-flex tw:h-[26px] tw:flex-none tw:cursor-not-allowed tw:items-center tw:rounded-md tw:border tw:border-border tw:bg-transparent tw:px-3 tw:text-xs tw:font-semibold tw:text-subtle-foreground tw:opacity-60"
}

/// A pick row with nothing to offer says why, on the row's one line.
pub(super) fn row_note_class() -> &'static str {
    "tw:m-0 tw:min-w-0 tw:flex-1 tw:truncate tw:text-xs tw:leading-[30px] tw:text-subtle-foreground"
}

fn mono_line_class() -> &'static str {
    "tw:m-0 tw:truncate tw:font-mono tw:text-[0.68rem] tw:text-subtle-foreground"
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{DeviceId, UiStatusKind};

    /// The fault line must wear the tone its own status chip wears, and it
    /// must be a SANCTIONED tone: the accent reckoning removed hue accents,
    /// so Attention is the one this state gets. A line in a colour nothing
    /// else uses is how a design language leaks.
    #[test]
    fn the_fault_line_wears_the_same_tone_as_the_degraded_chip() {
        assert_eq!(
            device_status_kind(lpa_studio_core::DeviceStatus::Degraded),
            UiStatusKind::Attention,
        );
        assert!(
            fault_line_class().contains("tw:text-status-attention-foreground"),
            "{}",
            fault_line_class()
        );
        // Not the plain state voice, and not an error voice either: an error
        // line is for a failed OUTCOME, and this board is running.
        assert_ne!(fault_line_class(), state_line_class());
        assert!(!fault_line_class().contains("status-error"));
    }

    /// Both readings of the state line occupy the SAME fixed row: a board
    /// that starts reporting a fault must not make its card taller (AC2).
    #[test]
    fn the_state_line_is_one_fixed_row_in_both_tones() {
        for class in [state_line_class(), fault_line_class()] {
            assert!(class.contains("tw:h-[17px]"), "{class}");
            assert!(class.contains("tw:leading-[17px]"), "{class}");
            assert!(class.contains("tw:truncate"), "{class}");
        }
    }

    /// D3: the card owns no padding and every zone but the first carries the
    /// full-bleed hairline — the node cards' own section grammar.
    #[test]
    fn only_non_first_zones_carry_the_full_bleed_divider() {
        assert!(!zone_class(true).contains("border-t"));
        assert!(zone_class(false).contains("tw:border-t tw:border-border-strong"));
        assert!(actions_zone_class().contains("tw:border-t tw:border-border-strong"));
        // Each zone pads itself, so the hairline can run edge to edge.
        for class in [zone_class(true), zone_class(false), actions_zone_class()] {
            assert!(class.contains("tw:px-4"), "{class}");
        }
        assert!(
            !card_class().contains("tw:p-4"),
            "the article must carry no padding of its own: {}",
            card_class()
        );
    }

    /// D4: the rows that make the card's height are fixed, in every state.
    #[test]
    fn the_state_zone_rows_are_fixed_height() {
        assert!(progress_slot_class(false).contains("tw:h-1"));
        assert!(progress_slot_class(true).contains("tw:h-1"));
        assert!(meta_row_class().contains("tw:h-[17px]"));
        assert!(verb_row_class().contains("tw:h-[30px]"));
        // The idle progress slot is invisible but still occupies its row.
        assert!(progress_slot_class(false).contains("tw:bg-transparent"));
        // A two-word verb in a narrow card must not wrap and burst the row:
        // `white-space: nowrap` inherits from the row into every chip in it
        // ("Clear faults" broke to two lines before this).
        assert!(verb_row_class().contains("tw:whitespace-nowrap"));
        assert!(verb_row_class().contains("tw:overflow-hidden"));
    }

    /// The activity's own reading: label, the cancel it was asked for, and
    /// the percentage — all on the one line the state row allows.
    #[test]
    fn an_activity_reads_as_one_line_with_its_percentage() {
        let activity = DeviceActivityView {
            kind: lpa_studio_core::DeviceActivityKind::Flash,
            label: "Flashing firmware".to_string(),
            percent: Some(42),
            cancellable: true,
            cancel_requested: false,
        };
        assert_eq!(activity_line_text(&activity), "Flashing firmware · 42%");

        let cancelling = DeviceActivityView {
            cancel_requested: true,
            percent: None,
            ..activity
        };
        assert_eq!(
            activity_line_text(&cancelling),
            "Flashing firmware — cancelling (finishing the current write)"
        );
    }

    fn card_fixture() -> DeviceView {
        DeviceView {
            id: DeviceId(1),
            title: "Bench board".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: Some("LightPlayer · seeed/xiao-esp32-c6".to_string()),
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: None,
            firmware: None,
            needs_firmware: false,
            degraded: None,
            loaded_project: DeviceLoadedProject::Unknown,
            can_receive_project: false,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            escapes: vec![DeviceEscape::Forget],
        }
    }

    /// The ruled priority (D4). An activity outranks a fault, a fault
    /// outranks freshness, freshness outranks the detail — and a card with
    /// none of them still renders its (empty) row rather than dropping it.
    #[test]
    fn the_state_line_follows_the_ruled_priority() {
        let mut card = card_fixture();
        assert_eq!(state_line_text(&card), "LightPlayer · seeed/xiao-esp32-c6");

        card.freshness_label = Some("last heard 12 s ago".to_string());
        assert_eq!(state_line_text(&card), "last heard 12 s ago");

        card.degraded = Some("node /studio.show/s faulted".to_string());
        assert_eq!(state_line_text(&card), "node /studio.show/s faulted");

        card.activity = Some(DeviceActivityView {
            kind: lpa_studio_core::DeviceActivityKind::Identify,
            label: "Identifying".to_string(),
            percent: Some(40),
            cancellable: true,
            cancel_requested: false,
        });
        assert_eq!(state_line_text(&card), "Identifying · 40%");

        let bare = DeviceView {
            detail: None,
            ..card_fixture()
        };
        assert_eq!(state_line_text(&bare), "");
    }

    /// AC10: every state's preview slot says something honest, and no state
    /// falls through to an empty box.
    #[test]
    fn every_state_has_an_honest_preview_sentence() {
        let mut card = card_fixture();
        assert_eq!(
            preview_sentence(&card),
            "No picture yet — the live feed is coming."
        );

        card.loaded_project = DeviceLoadedProject::Empty;
        assert_eq!(
            preview_sentence(&card),
            "Nothing loaded — no picture until something runs."
        );

        card.loaded_project = DeviceLoadedProject::Running {
            label: "porch-sign".to_string(),
        };
        assert_eq!(
            preview_sentence(&card),
            "No picture yet — the live feed is coming."
        );

        card.needs_firmware = true;
        assert_eq!(
            preview_sentence(&card),
            "Nothing running — a blank chip has no picture."
        );

        // An activity outranks the blank-chip reading, and the label's own
        // trailing ellipsis is not doubled.
        card.activity = Some(DeviceActivityView {
            kind: lpa_studio_core::DeviceActivityKind::Flash,
            label: "Flashing firmware…".to_string(),
            percent: None,
            cancellable: true,
            cancel_requested: false,
        });
        assert_eq!(
            preview_sentence(&card),
            "Flashing firmware… the picture returns when the board does."
        );
    }
}

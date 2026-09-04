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
//! # One box, four zones (D3 — full-bleed separators; P9's section identities)
//!
//! ```text
//!   header      title · status chip · board · chip · MAC · firmware
//!   ──────────── (full bleed)
//!   Project     preview slot (120) · info (17) · bar (4) · verbs (30)
//!   ────────────
//!   Firmware    info (17) · bar (4) · verbs (30)
//!               terminal — FLUSH, edge to edge, no hairline above it
//!   ────────────
//!   Device      info (17) · verbs (30)
//! ```
//!
//! The terminal shares the firmware's zone rather than owning one (G1's
//! second ruling, 2026-09-03): it is the same subject said twice — what
//! firmware is on this board, and what that firmware is saying — and a later
//! milestone puts the pair behind one curtain, which only works if they are
//! one element.
//!
//! The `article` carries **no padding**; each zone is a `section` with its
//! own `px-4` and, for every zone but the first, a `border-t
//! border-border-strong` hairline that runs edge to edge. That is the node
//! cards' own section grammar
//! ([`section_container_class`](crate::app::node::face::node_card_section))
//! and it is what makes the card read as one surface with divisions rather
//! than as a stack of framed widgets. Nothing inside a zone may grow its own
//! frame — a bordered sub-panel puts a box inside the box and the card stops
//! being one thing. The firmware zone is the one that pads its ROWS rather
//! than itself ([`zone_rows_class`]), because the terminal underneath them
//! must reach both edges: its dark ground is a band across the whole card,
//! not a panel inside it.
//!
//! # Three subjects, no labels (G1 2026-09-03)
//!
//! The G1 walk read the card as one undifferentiated stack: "Flash firmware
//! sits in the project section". So the middle of the card is split by
//! SUBJECT — the project on the board, the firmware under it, the device
//! itself — and each subject owns its own info line, its own progress bar
//! and its own verbs. Labels were ruled out ("kinda ugly"): a zone is
//! recognised by what it says and what it offers, the way the header is
//! recognised as identity.
//!
//! | zone | info line | verbs |
//! |---|---|---|
//! | Project | the project name, "Nothing loaded", the push's label + %; a degraded board's fault text replaces it | Open · Clear faults … [pick] Put it on the board … Remove |
//! | Firmware | "<firmware> · <board>", "Blank flash — needs firmware", the flash's label + % (the terminal is this zone's second half) | [board pick] Flash firmware … Factory reset |
//! | Device | freshness ("last heard 3 s ago" / "quiet — …"), "Identifying…" | Reset · Retry · Disconnect … Forget |
//!
//! An activity lights the bar of the zone it belongs to and puts its
//! **Cancel** in that zone's verb row ([`activity_zone`]): a push is project
//! work, a flash or an erase is firmware work, identification is the device
//! working out what it is. Every other verb row is withdrawn at its height
//! while an activity runs (D9) — except the Device zone's escapes, which are
//! present in every state (invariant I3: Forget works mid-flash).
//!
//! # The rows are FIXED (D4 — AC2)
//!
//! Every row above is present in every state at the height in the table.
//!
//! | row | height |
//! |---|---|
//! | info line | 17px, one line, ellipsised, `title` = the full text |
//! | bar slot | 4px, unlit when its zone has no activity |
//! | preview slot | 120px, the picture or an honest sentence (AC10) |
//! | verb row | 30px, whether it holds verbs or nothing |
//!
//! A board event — a heartbeat, a fault, a lost link, a new terminal line —
//! must never change a card's height. Fixed rows plus a fixed-height
//! terminal are how that is enforced: a box that is always the same size
//! cannot resize the card, and a card that cannot resize cannot make the
//! gallery jump while a flash runs. Content that does not fit a row is
//! TRUNCATED (with the full text on the `title`), never allowed to wrap.
//!
//! # Where the verbs live (AC6, re-homed by subject in P9)
//!
//! A verb lives in the zone whose subject it acts on, which is what makes
//! the zones legible without labels: `Flash firmware` and `Factory reset`
//! are firmware verbs and sit under the firmware line; `Remove` takes the
//! PROJECT off and sits under the project name; `Reset`, `Disconnect` and
//! `Forget` act on the device.
//!
//! Every verb that can need a PICKER opens it from its own 30px verb row —
//! a picker may never open from a row that is not fixed-height, because a
//! panel that pushes the card taller is exactly what AC2 forbids. The
//! popovers' panels float in the top layer, so opening one cannot change
//! the card's height at all. The re-flash's unresolved case (a chip with
//! several fitting boards, none registered) is the same quiet chip
//! re-dressed as [`BoardPickPopover`]'s trigger, and picking a board
//! flashes it.
//!
//! Arming any destructive chip marks the whole card (`.ux-armed-scope:has()`
//! in style.css, D8). Arming dims what the card SAYS (`ux-armed-dim` on the
//! header, every zone's info line and bar, the preview and the terminal) and
//! never what it OFFERS — every verb row keeps full contrast, so the chip
//! that is asking is always legible.
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
    /// Story-only, the same hook for the OTHER destructive chip: Remove
    /// project in the verb row (D8 — it marks the whole card exactly as
    /// Forget does, and the two live in different zones, so a capture that
    /// proves the marking needs both). Real surfaces never set this.
    #[props(default)]
    armed_remove_preview: bool,
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

    let identity_line = device_identity_line(&card);
    let identity = identity_line.display();
    // Which zone owns the running activity, if any: its bar lights, its
    // verb row holds Cancel, and every other verb row withdraws (D9).
    let busy_zone = card
        .activity
        .as_ref()
        .map(|activity| activity_zone(activity.kind));
    let project_line = project_line_text(&card, busy_zone);
    let firmware_line = firmware_line_text(&card, identity_line.board.as_deref(), busy_zone);
    let device_line = device_line_text(&card, busy_zone);
    // The fault takes the project line only when no project work is
    // running: the push's own narration outranks it (the terminal keeps the
    // fault either way).
    let project_line_is_fault = busy_zone != Some(ZoneKind::Project) && card.degraded.is_some();
    // Cancel is the escape of whatever is RUNNING, so it renders in that
    // activity's own zone rather than in a fixed corner of the card.
    let cancel = card
        .escapes
        .iter()
        .copied()
        .find(|escape| *escape == DeviceEscape::Cancel);

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

            // ── zone 2: PROJECT — what is on the board ──────────────────
            section { class: zone_class(false),
                div { class: "ux-armed-dim tw:grid tw:min-w-0 tw:gap-2",
                    // preview slot (120px, AC10). No feed yet — the honest
                    // sentence instead of a fake picture. The
                    // `ux-play-pill` liveness slot belongs top-right inside
                    // this frame; the feed milestone drops it in without
                    // moving anything, because the frame's height is fixed.
                    div { class: preview_frame_class(),
                        div { class: "ux-play-empty",
                            p { class: "tw:m-0", "{preview_sentence(&card)}" }
                        }
                    }
                    // info line (17px, one line, full text on hover)
                    p {
                        class: if project_line_is_fault { fault_line_class() } else { info_line_class() },
                        title: "{project_line}",
                        "{project_line}"
                    }
                    // bar slot (4px) — lit only for PROJECT work.
                    ZoneBar { activity: card.activity.clone(), lit: busy_zone == Some(ZoneKind::Project) }
                }
                // verb row (30px) — outside `ux-armed-dim`: arming dims what
                // the card says, never what it offers.
                div { class: verb_row_class(),
                    if busy_zone == Some(ZoneKind::Project) {
                        // The push's own way out, on the push's own zone.
                        if let Some(escape) = cancel {
                            ActionButton {
                                key: "{\"cancel-project\"}",
                                action: device_escape_action(escape, device),
                                running: false,
                                variant: ActionButtonVariant::Quiet,
                                on_action,
                            }
                        }
                    } else if card.activity.is_some() {
                        // D9: withdrawn at its height while other work runs.
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
                        span { class: "tw:min-w-0 tw:flex-1" }
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
                                armed_preview: armed_remove_preview,
                                on_action,
                            }
                        }
                    }
                }
            }

            // ── zone 3: FIRMWARE + TERMINAL — one zone (G1 2026-09-03) ──
            // The firmware rows pad themselves; the terminal is the zone's
            // last block, flush to both edges, with no hairline between them.
            section { class: combined_zone_class(),
                div { class: zone_rows_class(),
                div { class: "ux-armed-dim tw:grid tw:min-w-0 tw:gap-2",
                    p { class: info_line_class(), title: "{firmware_line}", "{firmware_line}" }
                    ZoneBar { activity: card.activity.clone(), lit: busy_zone == Some(ZoneKind::Firmware) }
                }
                div { class: verb_row_class(),
                    if busy_zone == Some(ZoneKind::Firmware) {
                        if let Some(escape) = cancel {
                            ActionButton {
                                key: "{\"cancel-firmware\"}",
                                action: device_escape_action(escape, device),
                                running: false,
                                variant: ActionButtonVariant::Quiet,
                                on_action,
                            }
                        }
                    } else if card.activity.is_some() {
                        // Withdrawn at its height while other work runs.
                    } else {
                        // The blank board's face: the chip-filtered board
                        // pick plus its Flash CTA, on one row.
                        if offer_flash {
                            BoardPickPopover { device, chip: chip.clone(), on_action }
                        }
                        // Re-flash (#500) on a board that is already
                        // running: a resolved pick dispatches Flash straight
                        // away; an unresolved one turns the same quiet chip
                        // into the board popover's trigger.
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
                        span { class: "tw:min-w-0 tw:flex-1" }
                        // The other firmware verb: wipe the flash back to a
                        // blank chip. It asks nothing, so it needs no picker.
                        // Not on a board that already IS blank (G2 prep: the
                        // pick trigger needs that row's width, and erasing a
                        // blank flash does nothing).
                        if idle && linked && !offer_flash {
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
                // The terminal, in the same zone as the firmware rows and
                // flush to both edges. Always present on a linked card,
                // empty or not: a panel that comes and goes is a panel that
                // resizes the card, which is the churn the fixed rows exist
                // to stop.
                if linked {
                    DeviceTerminal {
                        lines: card.terminal.clone(),
                        dropped: card.terminal_dropped,
                        height_class: "tw:h-40",
                    }
                }
            }

            // ── zone 4: DEVICE — the board itself ───────────────────────
            // Every escape the projection carries that is not the running
            // activity's Cancel, in every state — including Forget
            // mid-activity, which the shipped system could not do.
            footer { class: device_zone_class(),
                div { class: "ux-armed-dim tw:grid tw:min-w-0",
                    p { class: info_line_class(), title: "{device_line}", "{device_line}" }
                }
                div { class: verb_row_class(),
                    if busy_zone == Some(ZoneKind::Device) {
                        if let Some(escape) = cancel {
                            ActionButton {
                                key: "{\"cancel-device\"}",
                                action: device_escape_action(escape, device),
                                running: false,
                                variant: ActionButtonVariant::Quiet,
                                on_action,
                            }
                        }
                    }
                    // The one device verb that never asks a question.
                    if idle && linked {
                        ActionButton {
                            key: "{\"reset-board\"}",
                            action: DevicesOp::action_for(DeviceAction::ResetBoard { device }),
                            running: false,
                            variant: ActionButtonVariant::Quiet,
                            on_action,
                        }
                    }
                    // Retry, Disconnect, Reconnect — the escapes that act on
                    // the wire. Cancel rode its activity's zone; Forget is
                    // held back for the right-hand end.
                    for escape in card
                        .escapes
                        .iter()
                        .copied()
                        .filter(|escape| !matches!(escape, DeviceEscape::Cancel | DeviceEscape::Forget))
                    {
                        ActionButton {
                            key: "{escape:?}",
                            action: device_escape_action(escape, device),
                            running: false,
                            variant: ActionButtonVariant::Quiet,
                            on_action,
                        }
                    }
                    span { class: "tw:min-w-0 tw:flex-1" }
                    if card.escapes.contains(&DeviceEscape::Forget) {
                        ActionButton {
                            key: "{\"forget\"}",
                            action: device_escape_action(DeviceEscape::Forget, device),
                            running: false,
                            variant: ActionButtonVariant::Quiet,
                            armed_preview,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// One zone's 4px bar slot: present in every state, lit only when the
/// activity running belongs to THIS zone.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ZoneBar(activity: Option<DeviceActivityView>, lit: bool) -> Element {
    rsx! {
        div { class: progress_slot_class(lit),
            if let Some(activity) = activity.filter(|_| lit) {
                div {
                    class: progress_fill_class(activity.percent),
                    style: progress_fill_style(activity.percent),
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
/// the zones a link that has not identified itself can honestly fill: the
/// FIRMWARE zone (its verdict and the pick that answers it), the flush
/// terminal, and the DEVICE zone (what it is doing, and every way out). No
/// project zone at all — there is no project, no picture and no board to
/// speak of until it identifies.
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
    // The firmware zone's line, in the two readings a link that has not
    // identified itself can honestly give: the settled blank verdict, or
    // the honest "nothing is known yet".
    let firmware_line = match pending.needs_firmware {
        true => BLANK_FLASH_LINE.to_string(),
        false => "Firmware unknown until this board identifies".to_string(),
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

            // FIRMWARE + TERMINAL: the only zone a link that has not
            // identified itself can honestly fill — its verdict, the pick
            // that answers it, and whatever it has said so far.
            section { class: combined_zone_class(),
                div { class: zone_rows_class(),
                    div { class: "ux-armed-dim tw:grid tw:min-w-0 tw:gap-2",
                        p { class: info_line_class(), title: "{firmware_line}", "{firmware_line}" }
                        // Identification carries no percentage, so the slot
                        // sits unlit — present so the pending card and the
                        // device card share one row grammar.
                        div { class: progress_slot_class(false) }
                    }
                    div { class: verb_row_class(),
                        if pending.needs_firmware {
                            // The same popover the device card's firmware
                            // zone wears: a blank chip's only chip fact is
                            // its ROM boot banner.
                            BoardPickPopover {
                                device: pending.device,
                                chip: pending
                                    .detected_chip
                                    .clone()
                                    .map(|chip| (chip, ChipSource::BootBanner)),
                                on_action,
                            }
                        }
                    }
                }
                DeviceTerminal { lines: Vec::new(), dropped: 0, height_class: "tw:h-24" }
            }

            // DEVICE: what this link is doing, and every way out of it.
            footer { class: device_zone_class(),
                div { class: "ux-armed-dim tw:grid tw:min-w-0",
                    p { class: info_line_class(), title: "{state_line}", "{state_line}" }
                }
                div { class: verb_row_class(),
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
                    if pending.can_adopt && !pending.needs_firmware {
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
                    span { class: "tw:min-w-0 tw:flex-1" }
                    for escape in pending.escapes.iter().copied() {
                        ActionButton {
                            key: "{escape:?}",
                            action: pending_escape_action(escape, link),
                            running: false,
                            variant: ActionButtonVariant::Quiet,
                            on_action,
                        }
                    }
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

/// The three subjects the middle of the card is divided into (P9). Each
/// owns an info line, a bar slot and a verb row; an activity belongs to
/// exactly one of them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ZoneKind {
    Project,
    Firmware,
    Device,
}

/// Which zone an activity narrates in — the rule that decides which bar
/// lights and where its Cancel appears.
///
/// A push (and a removal) changes what the board RUNS; a flash or an erase
/// changes what is UNDER what it runs; identification is the device working
/// out what it is. Reading the progress bar therefore tells you what kind
/// of work is happening without reading a word.
fn activity_zone(kind: lpa_studio_core::DeviceActivityKind) -> ZoneKind {
    use lpa_studio_core::DeviceActivityKind as Kind;
    match kind {
        Kind::Push | Kind::RemoveProject => ZoneKind::Project,
        Kind::Flash | Kind::Erase => ZoneKind::Firmware,
        Kind::Identify => ZoneKind::Device,
    }
}

/// The blank board's verdict, worded the same in the device card's firmware
/// zone and on a pending link.
const BLANK_FLASH_LINE: &str = "Blank flash — needs firmware";

/// The PROJECT zone's info line: what is on the board, or what is being put
/// on it, or the one line naming why what is on it is not running well.
///
/// A running push outranks the fault (its narration is the news); the fault
/// outranks the project name, because a degraded board's first question is
/// what went wrong — the name is still in the header's neighbourhood and in
/// the terminal. A board that has not SAID what it runs says nothing here:
/// the row stays, empty, rather than guessing.
///
/// A finished activity's outcome is deliberately NOT here: it is a line in
/// the terminal (typed `Outcome`/`Failure` by the fold), and giving it a row
/// of its own would be a row that exists in some states and not others.
fn project_line_text(card: &DeviceView, busy_zone: Option<ZoneKind>) -> String {
    if busy_zone == Some(ZoneKind::Project)
        && let Some(activity) = &card.activity
    {
        return activity_line_text(activity);
    }
    if let Some(fault) = &card.degraded {
        return fault.clone();
    }
    match &card.loaded_project {
        DeviceLoadedProject::Running { label } => label.clone(),
        DeviceLoadedProject::Empty => "Nothing loaded".to_string(),
        DeviceLoadedProject::Unknown => String::new(),
    }
}

/// The FIRMWARE zone's info line: the firmware label joined to the board it
/// was built for, the blank verdict, or the flash's own narration.
///
/// `board` is the identity line's already-resolved board display name, so
/// the zone and the header name the board identically.
fn firmware_line_text(
    card: &DeviceView,
    board: Option<&str>,
    busy_zone: Option<ZoneKind>,
) -> String {
    if busy_zone == Some(ZoneKind::Firmware)
        && let Some(activity) = &card.activity
    {
        return activity_line_text(activity);
    }
    if card.needs_firmware {
        return BLANK_FLASH_LINE.to_string();
    }
    let parts: Vec<String> = card
        .firmware
        .clone()
        .into_iter()
        .chain(board.map(str::to_string))
        .collect();
    if parts.is_empty() {
        return "No firmware reported yet".to_string();
    }
    parts.join(" · ")
}

/// The DEVICE zone's info line: how long ago this board was heard from, or
/// the identification currently running.
///
/// Honest staleness rather than a stuck spinner is the model's own wording
/// ("last heard 3 s ago" / "quiet — last heard 12 s ago"); the evidence
/// detail stands in for a board that has not been heard from at all yet.
fn device_line_text(card: &DeviceView, busy_zone: Option<ZoneKind>) -> String {
    if busy_zone == Some(ZoneKind::Device)
        && let Some(activity) = &card.activity
    {
        return activity_line_text(activity);
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
/// own).
///
/// **No `overflow-hidden`** (G1 2026-09-03): the Primary verb's hover glow
/// is a `box-shadow` bloom that reaches past the button's box, and a
/// clipping ancestor cut it off flat. Nothing needs the clip — the zones
/// are card-coloured at both rounded corners, so no zone ground can poke
/// out of one, and the full-bleed hairlines sit between zones where the
/// card's edges are straight. What kept the rows from growing was never the
/// clip either: it is the fixed row heights plus `whitespace-nowrap`.
///
/// A COLUMN, not a grid: the device zone's `mt-auto` has to be able to push
/// it to the bottom edge when the roster's grid stretches a short card to
/// its row's height, and `margin-top: auto` does nothing to a grid item
/// under `content-start`. So the zones stack as flex children, each sized to
/// its own content, and only the gap above the last zone grows.
///
/// `ux-armed-scope`: the card is the blast radius of its own armed
/// destructive chips — `:has(.ux-armed)` marks it (style.css).
fn card_class() -> &'static str {
    "ux-armed-scope tw:flex tw:flex-col tw:rounded-md tw:border tw:border-border tw:bg-card"
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

/// The FIRMWARE + TERMINAL zone (G1 2026-09-03, second ruling): ONE section
/// holding the firmware's three rows and, under them with no hairline
/// between, the terminal running flush to both card edges.
///
/// It is one `section` on purpose. The two belong together as a subject —
/// what firmware is on this board, and what that firmware is saying — and a
/// later milestone puts the whole block behind a curtain, which is a
/// property of ONE element or it is a property of nothing. So the zone
/// carries the hairline and no padding; its rows pad themselves
/// ([`zone_rows_class`]) and the terminal deliberately does not.
fn combined_zone_class() -> &'static str {
    "tw:grid tw:min-w-0 tw:border-t tw:border-border-strong"
}

/// The padded block inside [`combined_zone_class`]: the same `px-4 py-3` and
/// row gap every other zone carries, applied to the rows rather than to the
/// zone, so the terminal below them can still reach the edges.
fn zone_rows_class() -> &'static str {
    "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3"
}

/// The DEVICE zone: the same grammar as the other zones, pushed to the
/// bottom so cards of different heights still line their last row up.
fn device_zone_class() -> &'static str {
    "tw:mt-auto tw:grid tw:min-w-0 tw:gap-2 tw:border-t tw:border-border-strong tw:px-4 tw:py-3"
}

/// Every zone's info line: exactly 17px, one line, ellipsised. Never allowed
/// to wrap; the full text rides the `title`.
fn info_line_class() -> &'static str {
    "tw:m-0 tw:h-[17px] tw:truncate tw:text-xs tw:leading-[17px] tw:text-subtle-foreground"
}

/// The info line when a degraded board's fault is what it carries: the
/// Attention tone the status chip already wears for this state, semibold
/// because it is the reason the chip changed. NOT the error voice — an error
/// line is for a failed OUTCOME, and this board is still running.
fn fault_line_class() -> &'static str {
    "tw:m-0 tw:h-[17px] tw:truncate tw:text-xs tw:font-semibold tw:leading-[17px] tw:text-status-attention-foreground"
}

/// A zone's bar slot: 4px, transparent (and therefore invisible) when this
/// zone has no activity, a track when it does. It occupies its 4px either
/// way, which is the point.
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

/// The PROJECT zone's preview slot: the `ux-play-frame` look (near-black,
/// border, radius) at the card's fixed 120px.
fn preview_frame_class() -> &'static str {
    "ux-play-frame ux-play-frame-slot"
}

/// A zone's verb row: 30px, always, whether it holds verbs or (during
/// another zone's activity) nothing.
///
/// `whitespace-nowrap` INHERITS into the chips, which is what keeps a
/// two-word verb ("Clear faults") on one line in a narrow card instead of
/// wrapping and bursting the row. There is deliberately **no
/// `overflow-hidden`** (G1 2026-09-03): it clipped the Primary verb's hover
/// glow flat. The row's HEIGHT is what AC2 needs, and that is fixed here;
/// the flexible spacer carries `min-w-0` so it collapses first when a
/// narrow card runs out of room.
fn verb_row_class() -> &'static str {
    "tw:flex tw:h-[30px] tw:min-w-0 tw:items-center tw:gap-1.5 tw:whitespace-nowrap"
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
        assert_ne!(fault_line_class(), info_line_class());
        assert!(!fault_line_class().contains("status-error"));
    }

    /// Both readings of the project line occupy the SAME fixed row: a board
    /// that starts reporting a fault must not make its card taller (AC2).
    #[test]
    fn the_info_line_is_one_fixed_row_in_both_tones() {
        for class in [info_line_class(), fault_line_class()] {
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
        assert!(combined_zone_class().contains("tw:border-t tw:border-border-strong"));
        assert!(device_zone_class().contains("tw:border-t tw:border-border-strong"));
        // Each zone pads itself, so the hairline can run edge to edge. The
        // firmware zone is the exception that proves it: the padding moves
        // to its ROWS so its terminal can reach both card edges.
        for class in [zone_class(true), zone_class(false), device_zone_class()] {
            assert!(class.contains("tw:px-4"), "{class}");
        }
        assert!(!combined_zone_class().contains("tw:px-4"));
        assert!(zone_rows_class().contains("tw:px-4"));
        assert!(
            !card_class().contains("tw:p-4"),
            "the article must carry no padding of its own: {}",
            card_class()
        );
    }

    /// D4: the rows that make the card's height are fixed, in every state.
    #[test]
    fn the_zone_rows_are_fixed_height() {
        assert!(progress_slot_class(false).contains("tw:h-1"));
        assert!(progress_slot_class(true).contains("tw:h-1"));
        assert!(info_line_class().contains("tw:h-[17px]"));
        assert!(verb_row_class().contains("tw:h-[30px]"));
        // The unlit bar slot is invisible but still occupies its row.
        assert!(progress_slot_class(false).contains("tw:bg-transparent"));
        // A two-word verb in a narrow card must not wrap and burst the row:
        // `white-space: nowrap` inherits from the row into every chip in it
        // ("Clear faults" broke to two lines before this).
        assert!(verb_row_class().contains("tw:whitespace-nowrap"));
    }

    /// The Primary verb's hover glow is a box-shadow bloom that reaches past
    /// its button, so NOTHING between it and the page may clip: not the verb
    /// row it sits in, not the card around it (G1 2026-09-03 — it was cut
    /// off flat on Open). The row's height is what AC2 needs, and that is
    /// still nailed down above.
    #[test]
    fn nothing_around_the_primary_verb_clips_its_glow() {
        assert!(
            !verb_row_class().contains("overflow"),
            "{}",
            verb_row_class()
        );
        assert!(!card_class().contains("overflow"), "{}", card_class());
        assert!(row_cta_class().contains("ux-spectrum-cta"));
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

    /// An activity narrates in ONE zone — the one whose subject it changes.
    /// This is what lets a reader tell a flash from a push without reading:
    /// the bar that lights says which.
    #[test]
    fn each_activity_belongs_to_the_zone_whose_subject_it_changes() {
        use lpa_studio_core::DeviceActivityKind as Kind;
        assert_eq!(activity_zone(Kind::Push), ZoneKind::Project);
        assert_eq!(activity_zone(Kind::RemoveProject), ZoneKind::Project);
        assert_eq!(activity_zone(Kind::Flash), ZoneKind::Firmware);
        assert_eq!(activity_zone(Kind::Erase), ZoneKind::Firmware);
        assert_eq!(activity_zone(Kind::Identify), ZoneKind::Device);
    }

    /// The PROJECT line: the push's narration, then the fault, then what the
    /// board says it runs — and an empty (present) row when it has not said.
    #[test]
    fn the_project_line_follows_the_ruled_priority() {
        let mut card = card_fixture();
        assert_eq!(project_line_text(&card, None), "");

        card.loaded_project = DeviceLoadedProject::Empty;
        assert_eq!(project_line_text(&card, None), "Nothing loaded");

        card.loaded_project = DeviceLoadedProject::Running {
            label: "porch-sign".to_string(),
        };
        assert_eq!(project_line_text(&card, None), "porch-sign");

        card.degraded = Some("node /studio.show/s faulted".to_string());
        assert_eq!(
            project_line_text(&card, None),
            "node /studio.show/s faulted",
            "a fault outranks the project name"
        );

        card.activity = Some(DeviceActivityView {
            kind: lpa_studio_core::DeviceActivityKind::Push,
            label: "Sending the project".to_string(),
            percent: Some(40),
            cancellable: true,
            cancel_requested: false,
        });
        assert_eq!(
            project_line_text(&card, Some(ZoneKind::Project)),
            "Sending the project · 40%"
        );
        // A flash runs in the FIRMWARE zone: the project line goes on
        // saying what the fault says, not what the flash is doing.
        assert_eq!(
            project_line_text(&card, Some(ZoneKind::Firmware)),
            "node /studio.show/s faulted"
        );
    }

    /// The FIRMWARE line: the flash's narration, then the blank verdict,
    /// then the firmware label joined to the board it was built for.
    #[test]
    fn the_firmware_line_names_the_firmware_and_its_board() {
        let mut card = card_fixture();
        assert_eq!(
            firmware_line_text(&card, None, None),
            "No firmware reported yet"
        );

        card.firmware = Some("fw-esp32c6 0.9.3".to_string());
        assert_eq!(
            firmware_line_text(&card, Some("XIAO ESP32-C6"), None),
            "fw-esp32c6 0.9.3 · XIAO ESP32-C6"
        );
        // A board that hello'd but reported no firmware label still names
        // the board rather than dropping to the honest-nothing line.
        assert_eq!(
            firmware_line_text(&card_fixture(), Some("XIAO ESP32-C6"), None),
            "XIAO ESP32-C6"
        );

        card.needs_firmware = true;
        assert_eq!(
            firmware_line_text(&card, Some("XIAO ESP32-C6"), None),
            "Blank flash — needs firmware"
        );

        card.activity = Some(DeviceActivityView {
            kind: lpa_studio_core::DeviceActivityKind::Flash,
            label: "Flashing firmware".to_string(),
            percent: Some(62),
            cancellable: true,
            cancel_requested: false,
        });
        assert_eq!(
            firmware_line_text(&card, None, Some(ZoneKind::Firmware)),
            "Flashing firmware · 62%"
        );
        // The same flash, read from the project zone's point of view: not
        // its news.
        assert_eq!(
            firmware_line_text(&card, None, Some(ZoneKind::Project)),
            "Blank flash — needs firmware"
        );
    }

    /// The DEVICE line: honest staleness, the evidence detail while nothing
    /// has been heard yet, and the identification while it runs.
    #[test]
    fn the_device_line_reads_freshness_then_detail() {
        let mut card = card_fixture();
        assert_eq!(
            device_line_text(&card, None),
            "LightPlayer · seeed/xiao-esp32-c6"
        );

        card.freshness_label = Some("quiet — last heard 12 s ago".to_string());
        assert_eq!(device_line_text(&card, None), "quiet — last heard 12 s ago");

        card.activity = Some(DeviceActivityView {
            kind: lpa_studio_core::DeviceActivityKind::Identify,
            label: "Identifying…".to_string(),
            percent: Some(40),
            cancellable: true,
            cancel_requested: false,
        });
        assert_eq!(
            device_line_text(&card, Some(ZoneKind::Device)),
            "Identifying… · 40%"
        );

        let bare = DeviceView {
            detail: None,
            ..card_fixture()
        };
        assert_eq!(device_line_text(&bare, None), "");
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

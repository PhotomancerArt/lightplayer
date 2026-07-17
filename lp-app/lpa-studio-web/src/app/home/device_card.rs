//! Device roster cards for the gallery's *Devices* section — and the
//! story sheet's vocabulary cards (one renderer, both surfaces).
//!
//! Deliberately distinct from package cards: a hardware header (status
//! circle · transport glyph · project chip) instead of a thumbnail — a
//! device must never read as "just another project". Everything shown is
//! read off the core view-model ([`RosterCardState`]): circle, status
//! line, sub-line, and the ≤1 affordance all come from core, so the
//! renderer can never drift from the vocabulary.
//!
//! Anatomy (direction.md "Card grammar"): header row = status circle ·
//! transport glyph+label · project chip right-aligned (identity, not
//! status; muted last-known on offline/error) · the rich-object detail
//! trigger at the right edge (Q1: node-style affordance-following icon —
//! see `device_detail_popover.rs`); device name with pencil-on-hover
//! inline rename (D34); status line (health only); ≤1 sub-line; ≤1
//! affordance button (Q2); offline whole-card fade.

use dioxus::prelude::*;
use lpa_studio_core::{
    BundledFirmware, ControllerId, DEPLOY_NODE_ID, DeployOp, DeviceController, DeviceOp, HomeOp,
    LinkProviderKind, RosterAffordance, RosterCardState, RosterCircle,
    RosterCircleShape as CoreShape, UiAction, UiDeviceCard, UiStatus, UiStatusKind,
};

use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::home::device_detail_popover::DeviceDetailPopover;
use crate::app::home::package_card::home_action;
use crate::base::{StatusCircle, StatusCircleShape, StatusCircleTone, StudioIcon, StudioIconName};
use crate::core::{ActionButton, ActionButtonVariant, StatusChip, quiet_action_class};

/// One roster card. Clicking an offline card reconnects through an
/// already-granted serial port with no chooser (M1); clicking a connected
/// card opens the deploy dialog with the device context (D29's
/// attach-as-runtime click lands in M5); working states are quiet.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceCard(
    card: UiDeviceCard,
    /// Fixed clock for stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    /// D36 sim-runtime presentation (story sheet today): sim glyph
    /// instead of the transport, no connect ceremony.
    #[props(default = false)]
    sim: bool,
    /// Studio's bundled firmware image (packaged manifest), when known —
    /// evidence for the standing amber "firmware update available" chip
    /// and the popover's Technical section.
    #[props(default)]
    bundled_fw: Option<BundledFirmware>,
    /// Open the detail popover on mount (story captures only).
    #[props(default = false)]
    detail_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let now = now_secs.unwrap_or_else(super::package_card::platform_now_secs);
    let (circle_shape, circle_tone) = circle_props(card.state.circle());
    let status_line = card.state.status_line(now);
    let sub_line = card.state.sub_line();
    let faded = matches!(card.state, RosterCardState::Offline { .. });
    // last-known, not current, on offline/error cards (card grammar)
    let chip_muted = faded || matches!(card.state, RosterCardState::NotResponding);
    // Needs-a-name opens the SAME inline form the pencil rename uses —
    // naming is card-anchored, never a dialog trip
    let name_inline = !sim && matches!(card.state, RosterCardState::NeedsAName);
    let click_action = if sim {
        None
    } else {
        card_click_action(&card.state)
    };
    let affordance = card
        .state
        .affordance()
        .and_then(|affordance| device_affordance_action(&card, &affordance));
    let can_rename = card.uid.is_some() && !sim;
    // the standing advisory chip rides an honest comparison of the
    // bundled image against the hello provenance (never a bare flag)
    let fw_update = match (&bundled_fw, &card.fw) {
        (Some(bundled), Some(fw)) => bundled.update_available(fw),
        _ => false,
    };
    let droppable = !sim && !faded;

    let mut renaming = use_signal(|| false);
    let rename_reset = if name_inline {
        String::new()
    } else {
        card.name.clone()
    };
    let mut rename_value = use_signal(|| rename_reset.clone());

    let (glyph, transport_label) = if sim {
        (StudioIconName::Simulator, "Simulator".to_string())
    } else {
        (StudioIconName::Usb, card.transport.clone())
    };

    rsx! {
        article {
            class: device_card_class(faded, click_action.is_some() || name_inline),
            title: "{status_line}",
            // connected cards are drop targets: project card → device card
            // opens the deploy dialog pre-filled (replace preview)
            ondragover: move |event| {
                if droppable {
                    event.prevent_default();
                }
            },
            ondrop: move |event| {
                if !droppable {
                    return;
                }
                event.prevent_default();
                if let Some(key) = super::package_card::take_dragged_project() {
                    on_action.call(UiAction::from_op(
                        ControllerId::new(DEPLOY_NODE_ID),
                        DeployOp::OpenDialog { target_key: Some(key) },
                    ));
                }
            },
            onclick: {
                let click_action = click_action.clone();
                move |_| {
                    if name_inline {
                        renaming.set(true);
                    } else if let Some(action) = &click_action {
                        on_action.call(action.clone());
                    }
                }
            },
            header { class: "tw:flex tw:items-center tw:gap-2 tw:border-b tw:border-border tw:bg-terminal tw:px-3 tw:py-2",
                StatusCircle { shape: circle_shape, tone: circle_tone }
                span { class: "tw:inline-flex tw:items-center tw:text-muted-foreground",
                    StudioIcon { name: glyph, size: 14 }
                }
                span { class: "tw:text-[11px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-muted-foreground",
                    "{transport_label}"
                }
                if let Some(chip) = card.project.clone() {
                    // identity, not status: the project the device holds
                    // (or last ran — muted on offline/error cards)
                    span { class: "tw:ml-auto tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1.5",
                        span {
                            class: "tw:inline-block tw:h-3 tw:w-3 tw:flex-none tw:rounded-[3px]",
                            style: thumb_swatch_style(&chip.uid, chip_muted),
                        }
                        span { class: chip_name_class(chip_muted), "{chip.name}" }
                    }
                }
                if !sim {
                    // the rich-object detail trigger (Q1): the node-style
                    // affordance-following icon at the right edge, riding
                    // the rollup tone; Flash/Erase/Forget live in its
                    // danger zone (the More-menu's rows migrated there)
                    span {
                        class: if card.project.is_some() { "tw:-my-1" } else { "tw:-my-1 tw:ml-auto" },
                        onclick: move |event| event.stop_propagation(),
                        DeviceDetailPopover {
                            card: card.clone(),
                            now_secs: now,
                            bundled_fw: bundled_fw.clone(),
                            initially_open: detail_open,
                            on_action,
                        }
                    }
                }
            }
            div { class: "tw:grid tw:gap-0.5 tw:p-3",
                if renaming() {
                    form {
                        class: "tw:flex tw:gap-2",
                        onclick: move |event| event.stop_propagation(),
                        onsubmit: {
                            let uid = card.uid.clone();
                            move |event: FormEvent| {
                                event.prevent_default();
                                let name = rename_value.read().trim().to_string();
                                if !name.is_empty() {
                                    // stamped devices rename; an anonymous
                                    // device (Needs a name) stamps
                                    let op = match &uid {
                                        Some(uid) => HomeOp::RenameDevice {
                                            uid: uid.clone(),
                                            name,
                                        },
                                        None => HomeOp::NameDevice { name },
                                    };
                                    on_action.call(home_action(op));
                                }
                                renaming.set(false);
                            }
                        },
                        input {
                            class: "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-0.5 tw:text-sm tw:text-strong-foreground",
                            autofocus: true,
                            placeholder: if name_inline { "e.g. Porch sign" } else { "" },
                            value: "{rename_value}",
                            oninput: move |event| rename_value.set(event.value()),
                            onkeydown: {
                                let reset = rename_reset.clone();
                                move |event: KeyboardEvent| {
                                    if event.key() == Key::Escape {
                                        rename_value.set(reset.clone());
                                        renaming.set(false);
                                    }
                                }
                            },
                        }
                        button {
                            class: quiet_action_class(),
                            r#type: "submit",
                            if name_inline { "Name" } else { "Rename" }
                        }
                    }
                } else {
                    span { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
                        p { class: device_name_class(faded), "{card.name}" }
                        if can_rename {
                            // pencil-on-hover → inline rename (D34)
                            button {
                                class: "tw:invisible tw:inline-flex tw:cursor-pointer tw:items-center tw:rounded tw:border-0 tw:bg-transparent tw:p-0.5 tw:text-muted-foreground tw:group-hover:visible tw:hover:text-strong-foreground",
                                r#type: "button",
                                title: "Rename this device",
                                aria_label: "Rename {card.name}",
                                onclick: {
                                    let name = card.name.clone();
                                    move |event: MouseEvent| {
                                        event.stop_propagation();
                                        rename_value.set(name.clone());
                                        renaming.set(true);
                                    }
                                },
                                StudioIcon { name: StudioIconName::Edited, size: 12 }
                            }
                        }
                    }
                }
                p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-dim-foreground", "{status_line}" }
                if let Some(sub_line) = sub_line {
                    p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-subtle-foreground", "{sub_line}" }
                }
                if fw_update {
                    div { class: "tw:mt-1",
                        StatusChip { status: UiStatus::warning("Firmware update available") }
                    }
                }
                if let Some(action) = affordance {
                    div { class: "tw:mt-1",
                        span {
                            onclick: move |event| event.stop_propagation(),
                            ActionButton {
                                action,
                                running: false,
                                variant: ActionButtonVariant::Quiet,
                                on_action,
                            }
                        }
                    }
                }
                if name_inline && !renaming() {
                    // the Needs-a-name affordance: opens the inline form
                    // above (card-anchored naming, never a dialog)
                    div { class: "tw:mt-1",
                        button {
                            class: quiet_action_class(),
                            r#type: "button",
                            onclick: move |event| {
                                event.stop_propagation();
                                renaming.set(true);
                            },
                            "Name this device…"
                        }
                    }
                }
            }
        }
    }
}

/// The dashed "Connect a device" affordance card. Copy comes from the
/// action's own metadata so the card and the toolbar chip never drift.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ConnectDeviceCard(on_action: EventHandler<UiAction>) -> Element {
    let action = connect_device_action();
    let meta = action.meta().clone();
    rsx! {
        button {
            class: "tw:grid tw:min-h-24 tw:cursor-pointer tw:place-items-center tw:gap-1 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:bg-transparent tw:p-3 tw:text-muted-foreground tw:transition-colors tw:hover:border-accent tw:hover:text-strong-foreground",
            r#type: "button",
            title: "{meta.summary}",
            onclick: move |_| on_action.call(action.clone()),
            span { class: "tw:inline-flex tw:items-center tw:gap-2",
                StudioIcon { name: StudioIconName::Usb, size: 16 }
                span { class: "tw:text-sm tw:font-semibold", "{meta.label}" }
            }
            span { class: "tw:text-xs tw:text-dim-foreground", "{meta.summary}" }
        }
    }
}

/// One-click reconnect for an offline/remembered device (M1): connect a
/// granted serial port directly; the browser chooser only appears when no
/// grant exists.
pub(crate) fn reconnect_device_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(DeviceController::NODE_ID),
        DeviceOp::ReconnectDevice,
    )
}

/// Connect = the VID-filtered browser chooser, directly (D32's filter
/// rides `requestPort`). The deploy dialog is no longer a connect
/// surface: it opens only WITH a device context.
pub(crate) fn connect_device_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(DeviceController::NODE_ID),
        DeviceOp::OpenProvider {
            provider_id: LinkProviderKind::BrowserSerialEsp32,
        },
    )
    .with_label("Connect a device")
    .with_summary("Connect a LightPlayer device over USB.")
    .with_icon("usb")
}

/// "Flash firmware…", the quiet secondary affordance (D33 demotion): with
/// a device connected it opens the dialog's firmware flows (device
/// context present — never `NeedsDevice`); with nothing connected it
/// opens the recovery chooser (link-only open, no app attach), after
/// which the card's own state carries the flow.
pub(crate) fn flash_device_action(device_connected: bool) -> UiAction {
    let action = if device_connected {
        UiAction::from_op(
            ControllerId::new(DEPLOY_NODE_ID),
            DeployOp::OpenDialog { target_key: None },
        )
    } else {
        UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::OpenProviderForRecovery {
                provider_id: LinkProviderKind::BrowserSerialEsp32,
            },
        )
    };
    action
        .with_label("Flash firmware…")
        .with_summary("Install or repair LightPlayer firmware on an ESP32.")
        .with_icon("zap")
}

/// What clicking the card body does, per state: offline reconnects (M1);
/// connected states open the deploy dialog WITH the device context (the
/// D29 attach-editor click is M5); self-healing/working states are quiet.
fn card_click_action(state: &RosterCardState) -> Option<UiAction> {
    match state {
        RosterCardState::Offline { .. } => Some(reconnect_device_action()),
        RosterCardState::ConnectingRetrying { .. }
        | RosterCardState::OperationInFlight { .. }
        | RosterCardState::InUseElsewhere => None,
        // handled in the renderer: click opens the inline name form
        RosterCardState::NeedsAName => None,
        _ => Some(UiAction::from_op(
            ControllerId::new(DEPLOY_NODE_ID),
            DeployOp::OpenDialog { target_key: None },
        )),
    }
}

/// The ≤1 affordance button, wired to what exists TODAY: flows the
/// vocabulary anticipates but that land later (Set up popup = M8,
/// troubleshoot popup = M6, D30 drift popup = M5) route to the deploy
/// dialog with the device context — never a dead button. Click-through
/// affordances (open editor) render no button; the card body carries the
/// action.
pub(super) fn device_affordance_action(
    card: &UiDeviceCard,
    affordance: &RosterAffordance,
) -> Option<UiAction> {
    let dialog = |target_key: Option<String>| {
        UiAction::from_op(
            ControllerId::new(DEPLOY_NODE_ID),
            DeployOp::OpenDialog { target_key },
        )
    };
    let action = match affordance {
        // click-through: the card click is the action (deploy dialog now,
        // editor-on-device in M5)
        RosterAffordance::OpenEditor => return None,
        // the dialog's Reviewing state carries push + the diverged verbs
        // until the in-card push / D30 popup land
        RosterAffordance::PushVersion { .. } => {
            dialog(card.project.as_ref().map(|chip| chip.uid.clone()))
                .with_summary("Review and push your newest version to this device.")
                .with_icon("upload")
        }
        RosterAffordance::ResolveDrift => {
            dialog(card.project.as_ref().map(|chip| chip.uid.clone()))
                .with_summary("Review the device's edited copy against your version.")
                .with_icon("edit")
        }
        RosterAffordance::Troubleshoot => dialog(None)
            .with_summary("Repair or reflash this device.")
            .with_icon("zap"),
        RosterAffordance::ChooseProject => dialog(None)
            .with_summary("Choose a project to put on this device.")
            .with_icon("play"),
        RosterAffordance::SetUp => dialog(None)
            .with_summary("Install LightPlayer firmware on this board.")
            .with_icon("zap"),
        RosterAffordance::UpdateFirmware => dialog(None)
            .with_summary("Install this build's firmware on the device.")
            .with_icon("zap"),
        // rendered as the card's inline name form, not an action button
        RosterAffordance::NameDevice => return None,
        RosterAffordance::Reconnect => reconnect_device_action()
            .with_summary("Reconnect over the granted serial port.")
            .with_icon("usb"),
    };
    Some(action.with_label(affordance.label()))
}

/// The danger zone's flash row: [`flash_device_action`] with a live
/// device context, wearing the destructive treatment the zone's rows
/// share (Q5 — the inline-tinted zone reads uniformly red).
pub(super) fn flash_device_action_destructive() -> UiAction {
    let action = flash_device_action(true);
    let meta = action.meta().clone().destructive();
    action.with_meta(meta)
}

/// Erase the device's flash entirely, from the danger zone.
/// Confirmation states the honest facts: full wipe; anything Studio could
/// read was banked at connect (D8) — unreadable content is gone for good.
pub(super) fn erase_device_action(name: String) -> UiAction {
    UiAction::from_op(ControllerId::new(DEPLOY_NODE_ID), DeployOp::EraseDevice).with_confirmation(
        lpa_studio_core::ActionConfirmation::new(
            "Erase device",
            format!(
                "Erase everything on \"{name}\"? Its flash is wiped clean; \
                 anything Studio could read was already saved to your library."
            ),
            "Erase",
        ),
    )
}

/// The forget action (D34 hygiene) for the offline card's danger zone.
pub(super) fn forget_device_action(uid: String, name: String) -> UiAction {
    home_action(HomeOp::ForgetDevice { uid }).with_confirmation(
        lpa_studio_core::ActionConfirmation::new(
            "Forget device",
            format!("Forget \"{name}\"? Connecting it again adds it back."),
            "Forget",
        ),
    )
}

/// Core circle spec → base component props (the one consumer-side hop —
/// base primitives stay independent of `lpa-studio-core`).
pub(crate) fn circle_props(circle: RosterCircle) -> (StatusCircleShape, StatusCircleTone) {
    let shape = match circle.shape {
        CoreShape::Solid => StatusCircleShape::Solid,
        CoreShape::Hollow => StatusCircleShape::Hollow,
        CoreShape::Pulsing => StatusCircleShape::Pulsing,
    };
    let tone = match circle.tone {
        UiStatusKind::Neutral => StatusCircleTone::Neutral,
        UiStatusKind::Working => StatusCircleTone::Working,
        UiStatusKind::Good => StatusCircleTone::Good,
        UiStatusKind::Warning => StatusCircleTone::Warning,
        UiStatusKind::Error => StatusCircleTone::Error,
    };
    (shape, tone)
}

fn device_card_class(faded: bool, clickable: bool) -> &'static str {
    // tw:group anchors the pencil's hover reveal
    match (faded, clickable) {
        (true, _) => {
            "tw:group tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:opacity-70 tw:transition-opacity tw:hover:opacity-100"
        }
        (false, true) => {
            "tw:group tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:transition-colors tw:hover:border-border-strong"
        }
        (false, false) => {
            "tw:group tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card"
        }
    }
}

fn chip_name_class(muted: bool) -> &'static str {
    if muted {
        "tw:truncate tw:text-[11px] tw:text-dim-foreground"
    } else {
        "tw:truncate tw:text-[11px] tw:text-muted-foreground"
    }
}

fn device_name_class(faded: bool) -> &'static str {
    if faded {
        "tw:m-0 tw:truncate tw:text-sm tw:font-semibold tw:text-muted-foreground"
    } else {
        "tw:m-0 tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground"
    }
}

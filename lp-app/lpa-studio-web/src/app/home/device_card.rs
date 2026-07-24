//! Device roster cards for the gallery's *Devices* section — and the
//! story sheet's vocabulary cards (one renderer, both surfaces).
//!
//! M7′ (card-as-control-panel, D39–D43): the card IS the device's control
//! panel. Anatomy, per the ratified spike (`spikes/device-card-panel/`):
//!
//! - **Tint left edge** carries state — tone from the rich-object rollup
//!   (worst-actionable section), treatment from the retired circle's
//!   shape grammar (filled = live, double/faded = remembered, pulsing =
//!   working), so state reads without color. No status circle.
//! - **D40 title bar**: kind glyph LEFT of the name (device/sim),
//!   inline-editable name (the D34 rename and the name-stamping flow,
//!   re-homed here), transport text label right, always-visible GROW ⤢ —
//!   the ONE editor entry (`OpenDeviceProject`/`OpenSimProject`); the
//!   old whole-card editor click is retired, body clicks are quiet.
//! - **Icon-tab row** below the title renders the rich-object sections
//!   per the ratified mapping ([`device_card_tabs`]); tab badges derive
//!   from the rollup families. The detail popovers are no longer
//!   reachable from cards (deletion lands with M7′ P3).
//!
//! Everything shown still reads off the core view-model
//! ([`RosterCardState`] → [`device_rich_object`]/[`sim_rich_object`]),
//! so the renderer can never drift from the vocabulary.

use dioxus::prelude::*;
use lpa_studio_core::{
    BundledFirmware, CardTabView, ControllerId, DEPLOY_NODE_ID, DeployOp, DeviceCardTab,
    DeviceController, DeviceDetailAffordance, DeviceOp, DeviceRichInput, HomeOp, LinkProviderKind,
    ProjectController, ProjectOp, RichObjectView, RichSection, RosterAffordance, RosterCardState,
    RosterCircleShape as CoreShape, SimDetailAffordance, SimRichInput, UiAction, UiDeviceCard,
    UiStatusKind, device_card_tabs, device_rich_object, sim_rich_object,
};
// The circle→component mapping (`circle_props`) survives only for the
// exploration story sheet; its imports ride the same `stories` gate so
// host clippy (no stories) does not see them as unused.
#[cfg(feature = "stories")]
use lpa_studio_core::RosterCircle;

use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::home::package_card::home_action;
use crate::base::{NodeKindIcon, StudioIcon, StudioIconName};
#[cfg(feature = "stories")]
use crate::base::{StatusCircleShape, StatusCircleTone};
use crate::core::{ActionButton, ActionButtonVariant, StatusChip, chip_status, quiet_action_class};

/// One roster card: the device (or live sim session) as a tabbed control
/// panel. The grow control is the editor entry; the tabs carry status,
/// project, settings, console (P2), and the danger zone; body clicks are
/// quiet (drop targets stay live).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceCard(
    card: UiDeviceCard,
    /// Fixed clock for stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    /// D36 sim-runtime presentation: sim glyph, no rename/stamp flows,
    /// sim rich-object sections.
    #[props(default = false)]
    sim: bool,
    /// Studio's bundled firmware image (packaged manifest), when known —
    /// evidence for the advisory "firmware update available" chip on the
    /// Settings tab (and its badge).
    #[props(default)]
    bundled_fw: Option<BundledFirmware>,
    /// Open with this tab selected (story captures only).
    #[props(default)]
    initial_tab: Option<DeviceCardTab>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let now = now_secs.unwrap_or_else(super::package_card::platform_now_secs);
    let status_line = card.state.status_line(now);
    let faded = matches!(card.state, RosterCardState::Offline { .. });
    // last-known, not current, on offline/error cards (card grammar)
    let chip_muted = faded || matches!(card.state, RosterCardState::NotResponding);
    // Needs-a-name opens the SAME inline form the pencil rename uses —
    // naming is card-anchored, never a dialog trip
    let name_inline = !sim && matches!(card.state, RosterCardState::NeedsAName);
    let can_rename = card.uid.is_some() && !sim;
    let droppable = !sim && !faded;

    // The rich-object view: sections wired to concrete actions here (the
    // one identity→action hop), rollup tone for the edge, then the
    // ratified sections→tabs grouping.
    let sections: Vec<RichSection<UiAction>> = if sim {
        sim_rich_object(&SimRichInput {
            state: &card.state,
            project_name: card.project.as_ref().map(|chip| chip.name.as_str()),
            now_secs: now,
        })
        .sections
        .into_iter()
        .map(wire_sim_section)
        .collect()
    } else {
        device_rich_object(&DeviceRichInput {
            state: &card.state,
            uid: card.uid.as_deref(),
            transport: &card.transport,
            project_name: card.project.as_ref().map(|chip| chip.name.as_str()),
            fw: card.fw.as_ref(),
            bundled_fw: bundled_fw.as_ref(),
            now_secs: now,
        })
        .sections
        .into_iter()
        .map(|section| wire_section(&card, section))
        .collect()
    };
    let view = RichObjectView::new(sections);
    let edge_tone = view.rollup().tone;
    let tabs = device_card_tabs(view);
    let edge_shape = card.state.circle().shape;

    // The grow control dispatches the existing editor-attach ops exactly
    // where the old click-arms fired; elsewhere it renders disabled (the
    // control is always VISIBLE — D40 — never a dead click).
    let grow_action = if sim {
        card.project.is_some().then(open_sim_project_action)
    } else {
        matches!(
            card.state,
            RosterCardState::RunningUpToDate
                | RosterCardState::RunningBehind { .. }
                | RosterCardState::EditedOnDevice
        )
        .then(open_device_project_action)
    };

    let selected = use_signal(move || initial_tab.unwrap_or(DeviceCardTab::Status));
    // a state change may drop the selected tab (e.g. Danger during an
    // operation): fall back to Status rather than a blank body
    let active_tab = tabs
        .iter()
        .find(|tab| tab.tab == selected())
        .map_or(DeviceCardTab::Status, |tab| tab.tab);

    let mut renaming = use_signal(|| false);
    let rename_reset = if name_inline {
        String::new()
    } else {
        card.name.clone()
    };
    let mut rename_value = use_signal(|| rename_reset.clone());

    let edge_style = format!(
        "--edge-tint: var(--studio-status-{}-text);",
        status_family(edge_tone)
    );
    let glyph = if sim {
        StudioIconName::Simulator
    } else {
        StudioIconName::Usb
    };
    let transport_label = if sim {
        "Simulator".to_string()
    } else {
        card.transport.clone()
    };

    rsx! {
        article {
            class: device_card_class(faded, edge_shape),
            style: "{edge_style}",
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
            // D40 title bar: kind glyph · inline-editable name · transport
            // label · the always-visible grow control.
            header { class: "tw:flex tw:min-h-9 tw:items-center tw:gap-2 tw:border-b tw:border-border tw:bg-terminal tw:py-1.5 tw:pl-3 tw:pr-1.5",
                span { class: "tw:inline-flex tw:flex-none tw:items-center tw:text-muted-foreground",
                    title: if sim { "Simulator" } else { "Device" },
                    StudioIcon { name: glyph, size: 14 }
                }
                if renaming() {
                    form {
                        class: "tw:flex tw:min-w-0 tw:flex-1 tw:gap-2",
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
                            class: "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border tw:bg-card tw:px-2 tw:py-0.5 tw:text-sm tw:text-strong-foreground",
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
                    p {
                        class: device_name_class(faded, can_rename || name_inline),
                        title: if can_rename || name_inline { "Rename this device" } else { "" },
                        onclick: move |_| {
                            if can_rename || name_inline {
                                renaming.set(true);
                            }
                        },
                        "{card.name}"
                    }
                    if can_rename {
                        // pencil-on-hover → inline rename (D34)
                        button {
                            class: "tw:invisible tw:inline-flex tw:cursor-pointer tw:items-center tw:rounded tw:border-0 tw:bg-transparent tw:p-0.5 tw:text-muted-foreground tw:group-hover:visible tw:hover:text-strong-foreground",
                            r#type: "button",
                            title: "Rename this device",
                            aria_label: "Rename {card.name}",
                            onclick: {
                                let name = card.name.clone();
                                move |_| {
                                    rename_value.set(name.clone());
                                    renaming.set(true);
                                }
                            },
                            StudioIcon { name: StudioIconName::Edited, size: 12 }
                        }
                    }
                    if !transport_label.is_empty() {
                        span { class: "tw:ml-auto tw:flex-none tw:text-[11px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-dim-foreground",
                            "{transport_label}"
                        }
                    }
                    button {
                        class: grow_button_class(transport_label.is_empty()),
                        r#type: "button",
                        disabled: grow_action.is_none(),
                        title: if grow_action.is_some() { "Open in the editor" } else { "Nothing to open in the editor yet" },
                        aria_label: "Open {card.name} in the editor",
                        onclick: {
                            let grow_action = grow_action.clone();
                            move |_| {
                                if let Some(action) = &grow_action {
                                    on_action.call(action.clone());
                                }
                            }
                        },
                        StudioIcon { name: StudioIconName::Grow, size: 14 }
                    }
                }
            }
            // the icon-tab row (below the title bar — spike anatomy)
            div {
                class: "tw:flex tw:gap-0.5 tw:border-b tw:border-border tw:bg-terminal tw:px-1.5 tw:py-1",
                role: "tablist",
                for tab_view in tabs.iter() {
                    {tab_button(tab_view, active_tab, selected)}
                }
            }
            div { class: "tw:grid tw:content-start tw:gap-1.5 tw:p-3",
                match active_tab {
                    DeviceCardTab::Status => rsx! {
                        {status_tab_body(&card, &tabs, chip_muted, name_inline, renaming, on_action)}
                    },
                    DeviceCardTab::Console => rsx! {
                        // D42's strip + per-session log plumbing land in
                        // M7′ P2; the tab keeps its learned position.
                        p { class: "tw:m-0 tw:font-mono tw:text-xs tw:text-dim-foreground",
                            "The per-device console lands here soon."
                        }
                    },
                    _ => rsx! {
                        {sections_tab_body(&tabs, active_tab, on_action)}
                    },
                }
            }
        }
    }
}

/// One icon tab: selection wears the card tint (`.ux-device-tab` — the
/// Danger tab the error family), the badge dot the per-tab announcement.
fn tab_button(
    tab_view: &CardTabView<UiAction>,
    active_tab: DeviceCardTab,
    mut selected: Signal<DeviceCardTab>,
) -> Element {
    let tab = tab_view.tab;
    let label = tab.label();
    let badge_style = tab_view.badge.map(|badge| {
        format!(
            "background: var(--studio-status-{}-text);",
            status_family(badge)
        )
    });
    rsx! {
        button {
            class: if tab == DeviceCardTab::Danger { "ux-device-tab ux-device-tab-danger" } else { "ux-device-tab" },
            r#type: "button",
            role: "tab",
            aria_selected: tab == active_tab,
            title: "{label}",
            aria_label: "{label}",
            onclick: move |event| {
                event.stop_propagation();
                selected.set(tab);
            },
            StudioIcon { name: tab_icon(tab), size: 14 }
            if let Some(badge_style) = badge_style {
                span { class: "ux-device-tab-badge", style: "{badge_style}" }
            }
        }
    }
}

/// The Status tab: the Health section with the status line up front, the
/// project chip as identity, and the state-table affordance — today's
/// card body, re-homed.
fn status_tab_body(
    card: &UiDeviceCard,
    tabs: &[CardTabView<UiAction>],
    chip_muted: bool,
    name_inline: bool,
    mut renaming: Signal<bool>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let health = tabs
        .iter()
        .find(|tab| tab.tab == DeviceCardTab::Status)
        .map(|tab| tab.sections.as_slice())
        .unwrap_or_default();
    rsx! {
        for section in health {
            for line in section.lines.iter() {
                if line.label == "status" {
                    // the headline: tinted like the edge (the spike's
                    // status line), never a bare kv row
                    p { class: "tw:m-0 tw:truncate tw:text-xs tw:font-semibold",
                        style: "color: var(--edge-tint);",
                        "{line.value}"
                    }
                } else {
                    p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-subtle-foreground",
                        "{line.value}"
                    }
                }
            }
            if let Some(chip) = section.chip.as_ref() {
                div { class: "tw:mt-1",
                    StatusChip { status: chip_status(chip) }
                }
            }
        }
        if let Some(chip) = card.project.clone() {
            // identity, not status: the project the device holds (or last
            // ran — muted on offline/error cards); the drift facts live on
            // the Project tab
            span { class: "tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1.5",
                span {
                    class: "tw:inline-block tw:h-3 tw:w-3 tw:flex-none tw:rounded-[3px]",
                    style: thumb_swatch_style(&chip.uid, chip_muted),
                }
                span { class: chip_name_class(chip_muted), "{chip.name}" }
            }
        }
        for section in health {
            for action in section.affordances.iter() {
                div { class: "tw:mt-1",
                    ActionButton {
                        action: action.clone(),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
            }
        }
        if name_inline && !renaming() {
            // the Needs-a-name affordance: opens the title bar's inline
            // form (card-anchored naming, never a dialog)
            div { class: "tw:mt-1",
                button {
                    class: quiet_action_class(),
                    r#type: "button",
                    onclick: move |_| renaming.set(true),
                    "Name this device…"
                }
            }
        }
    }
}

/// A non-Status tab's body: the tab's sections as compact fact rows +
/// advisory chip + affordances. Danger rows render as destructive menu
/// rows (inspector-row convention); other affordances as quiet chips.
fn sections_tab_body(
    tabs: &[CardTabView<UiAction>],
    active_tab: DeviceCardTab,
    on_action: EventHandler<UiAction>,
) -> Element {
    let sections = tabs
        .iter()
        .find(|tab| tab.tab == active_tab)
        .map(|tab| tab.sections.as_slice())
        .unwrap_or_default();
    let menu_rows = active_tab == DeviceCardTab::Danger;
    rsx! {
        for section in sections {
            if !section.lines.is_empty() {
                dl { class: "tw:m-0 tw:grid tw:min-w-0 tw:gap-1 tw:text-xs",
                    for line in section.lines.iter() {
                        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[72px_minmax(0,1fr)] tw:gap-2",
                            dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                                "{line.label}"
                            }
                            dd { class: "tw:m-0 tw:min-w-0 tw:font-mono tw:text-muted-foreground tw:break-words",
                                "{line.value}"
                            }
                        }
                    }
                }
            }
            if let Some(chip) = section.chip.as_ref() {
                div { StatusChip { status: chip_status(chip) } }
            }
            for action in section.affordances.iter() {
                div {
                    ActionButton {
                        action: action.clone(),
                        running: false,
                        variant: if menu_rows { ActionButtonVariant::MenuItem } else { ActionButtonVariant::Quiet },
                        on_action,
                    }
                }
            }
        }
    }
}

/// The tab's icon (icon tabs at card scale; labels arrive in pane mode).
fn tab_icon(tab: DeviceCardTab) -> StudioIconName {
    match tab {
        DeviceCardTab::Status => StudioIconName::Play,
        DeviceCardTab::Project => StudioIconName::NodeKind(NodeKindIcon::Project),
        DeviceCardTab::Settings => StudioIconName::Settings,
        DeviceCardTab::Performance => StudioIconName::Performance,
        DeviceCardTab::Console => StudioIconName::Console,
        DeviceCardTab::Danger => StudioIconName::Danger,
    }
}

/// The status family's token name — the edge tint and badge dots ride the
/// shared `--studio-status-*` families (attention-orange for health;
/// yellow/purple stay node meanings, never borrowed here).
fn status_family(tone: UiStatusKind) -> &'static str {
    match tone {
        UiStatusKind::Neutral => "neutral",
        UiStatusKind::Working => "working",
        UiStatusKind::Good => "good",
        UiStatusKind::Warning => "warning",
        UiStatusKind::Attention => "attention",
        UiStatusKind::Error => "error",
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
pub(crate) fn reconnect_device_action(uid: Option<String>) -> UiAction {
    UiAction::from_op(
        ControllerId::new(DeviceController::NODE_ID),
        DeviceOp::ReconnectDevice { uid },
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

/// The D29 editor entry, now dispatched from the grow control (⤢): move
/// the editor lens onto the device session and open its running project.
/// The card targets the attached session (`uid: None`); the
/// `#/device/<uid>` route passes the uid.
fn open_device_project_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        ProjectOp::OpenDeviceProject { uid: None },
    )
}

/// The sim card's grow (the D29 grammar's sim arm, runtime-pool P4):
/// re-attach the editor lens to the sim session and open what it runs.
fn open_sim_project_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        ProjectOp::OpenSimProject,
    )
}

/// Stop the simulator, from the sim card's Danger tab (runtime-pool P3's
/// destroy op). Confirmation states the honest cost: the worker dies, and
/// applied-but-unsaved edits live on it — anything not saved to the
/// library is gone.
pub(super) fn stop_simulator_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(DeviceController::NODE_ID),
        DeviceOp::StopSimulator,
    )
    .with_confirmation(lpa_studio_core::ActionConfirmation::new(
        "Stop simulator",
        "Stop the simulator? Anything not saved to your library is discarded.",
        "Stop",
    ))
}

/// The ≤1 affordance, wired to what exists TODAY: Push runs the in-card
/// push directly (M5 — the button is the D11 consent); flows the
/// vocabulary anticipates but that land later (Set up = M8′,
/// troubleshoot = M6, D30 rich drift resolution = the M7′ P2 sheet)
/// route to the deploy dialog with the device context — never a dead
/// button. The editor identity renders no button — the grow control is
/// the editor entry.
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
        // the grow control (⤢) is the editor entry — no row
        RosterAffordance::OpenEditor => return None,
        // The in-card push (M5): the button IS the D11 consent — the push
        // dispatches directly and its progress folds into the card's
        // Operation-in-flight state. No dialog. (Defensive fallback to the
        // dialog when the chip is somehow absent — RunningBehind derives
        // from Known content, so it never should be.)
        RosterAffordance::PushVersion { .. } => match card.project.as_ref() {
            Some(chip) => UiAction::from_op(
                ControllerId::new(DEPLOY_NODE_ID),
                DeployOp::PushProject {
                    key: chip.uid.clone(),
                },
            )
            .with_summary("Push your newest version to this device.")
            .with_icon("upload"),
            None => dialog(None)
                .with_summary("Review and push your newest version to this device.")
                .with_icon("upload"),
        },
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
        RosterAffordance::Reconnect => reconnect_device_action(card.uid.clone())
            .with_summary("Reconnect over the granted serial port.")
            .with_icon("usb"),
    };
    Some(action.with_label(affordance.label()))
}

/// Map one device section's affordance identities onto concrete
/// `UiAction`s for this card. Identities without a live flow render no
/// row (the name-device flow lives in the title bar).
pub(super) fn wire_section(
    card: &UiDeviceCard,
    section: RichSection<DeviceDetailAffordance>,
) -> RichSection<UiAction> {
    RichSection {
        title: section.title,
        tone: section.tone,
        lines: section.lines,
        chip: section.chip,
        affordances: section
            .affordances
            .iter()
            .filter_map(|affordance| wire_affordance(card, affordance))
            .collect(),
        weight: section.weight,
    }
}

fn wire_affordance(card: &UiDeviceCard, affordance: &DeviceDetailAffordance) -> Option<UiAction> {
    match affordance {
        DeviceDetailAffordance::Roster(affordance) => device_affordance_action(card, affordance),
        DeviceDetailAffordance::FlashFirmware => Some(flash_device_action_destructive()),
        DeviceDetailAffordance::EraseDevice => Some(erase_device_action(card.name.clone())),
        DeviceDetailAffordance::ForgetDevice => card
            .uid
            .clone()
            .map(|uid| forget_device_action(uid, card.name.clone())),
    }
}

/// Map one sim section's affordance identities onto concrete `UiAction`s.
pub(super) fn wire_sim_section(section: RichSection<SimDetailAffordance>) -> RichSection<UiAction> {
    RichSection {
        title: section.title,
        tone: section.tone,
        lines: section.lines,
        chip: section.chip,
        affordances: section
            .affordances
            .iter()
            .map(|affordance| match affordance {
                SimDetailAffordance::StopSimulator => stop_simulator_action(),
            })
            .collect(),
        weight: section.weight,
    }
}

/// The Danger tab's flash row: [`flash_device_action`] with a live
/// device context, wearing the destructive treatment the tab's rows
/// share (the inline red zone reads uniformly red).
pub(super) fn flash_device_action_destructive() -> UiAction {
    let action = flash_device_action(true);
    let meta = action.meta().clone().destructive();
    action.with_meta(meta)
}

/// Erase the device's flash entirely, from the Danger tab. Confirmation
/// states the honest facts: full wipe; anything Studio could read was
/// banked at connect (D8) — unreadable content is gone for good.
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

/// The forget action (D34 hygiene) for the offline card's Danger tab.
pub(super) fn forget_device_action(uid: String, name: String) -> UiAction {
    home_action(HomeOp::ForgetDevice { uid }).with_confirmation(
        lpa_studio_core::ActionConfirmation::new(
            "Forget device",
            format!("Forget \"{name}\"? Connecting it again adds it back."),
            "Forget",
        ),
    )
}

/// Core circle spec → base component props (kept for the exploration
/// stories' vocabulary sheet; cards themselves render the edge chrome —
/// `StatusCircle`'s deletion lands with M7′ P3).
#[cfg(feature = "stories")]
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
        UiStatusKind::Attention => StatusCircleTone::Attention,
        UiStatusKind::Error => StatusCircleTone::Error,
    };
    (shape, tone)
}

/// The card's chrome: the tint edge class per the shape grammar plus the
/// offline whole-card fade. Body clicks are quiet (no pointer cursor —
/// the interactive surfaces carry their own).
fn device_card_class(faded: bool, shape: CoreShape) -> String {
    let edge = match shape {
        CoreShape::Solid => "ux-device-edge",
        CoreShape::Hollow => "ux-device-edge ux-device-edge-remembered",
        CoreShape::Pulsing => "ux-device-edge ux-device-edge-working",
    };
    let fade = if faded {
        " tw:opacity-70 tw:transition-opacity tw:hover:opacity-100"
    } else {
        ""
    };
    format!(
        // tw:group anchors the pencil's hover reveal
        "tw:group tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card {edge}{fade}"
    )
}

fn chip_name_class(muted: bool) -> &'static str {
    if muted {
        "tw:truncate tw:text-[11px] tw:text-dim-foreground"
    } else {
        "tw:truncate tw:text-[11px] tw:text-muted-foreground"
    }
}

fn device_name_class(faded: bool, editable: bool) -> &'static str {
    match (faded, editable) {
        (true, true) => {
            "tw:m-0 tw:min-w-0 tw:cursor-text tw:truncate tw:text-sm tw:font-semibold tw:text-muted-foreground"
        }
        (true, false) => {
            "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-semibold tw:text-muted-foreground"
        }
        (false, true) => {
            "tw:m-0 tw:min-w-0 tw:cursor-text tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground"
        }
        (false, false) => {
            "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground"
        }
    }
}

/// The grow control's chrome; hugs the header's right edge when no
/// transport label sits beside it.
fn grow_button_class(push_right: bool) -> &'static str {
    if push_right {
        "tw:ml-auto tw:inline-flex tw:h-6 tw:w-7 tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded tw:border-0 tw:bg-transparent tw:text-dim-foreground tw:hover:text-strong-foreground tw:disabled:cursor-default tw:disabled:opacity-40"
    } else {
        "tw:inline-flex tw:h-6 tw:w-7 tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded tw:border-0 tw:bg-transparent tw:text-dim-foreground tw:hover:text-strong-foreground tw:disabled:cursor-default tw:disabled:opacity-40"
    }
}

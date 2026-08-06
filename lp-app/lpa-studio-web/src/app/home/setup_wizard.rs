//! The setup wizard: one component per machine state.
//!
//! Design: `docs/design/device-setup-flow.md`; the visual contract is
//! `spikes/device-setup-flow/` (four gate rounds). Placement is F5b as the
//! G2 gate amended it (2026-08-05) — **the wizard is a state of the device
//! card**, and it renders in one of two frames:
//!
//! * [`SetupWizardCard`] — the standalone card in the entry-cards slot,
//!   for the states where no device exists yet to be the body of
//!   (CONNECT_INTRO, BOARD_FIRST, PORT_PICKING, and the whole sim path up
//!   to the simulator starting).
//! * [`setup_takeover_body`] — the same rail, the same per-state
//!   components, rendered as the BODY of the bound device's own roster
//!   card from the port grant onward. Same card, same key, same grid
//!   slot: the handoff at DEVICE_HOME is a body swap and nothing else.
//!
//! The two frames share every state component below, so a state cannot
//! look like two different steps depending on where it is drawn.
//!
//! **No flow logic lives here.** Every button dispatches a
//! [`SetupGesture`]; the reducer decides what it means, and a gesture a
//! state has no arm for is inert by design. Nothing in this file branches
//! on what the flow will do next — the closest it comes is arming a
//! forward verb from the state's own data (a board has been picked), which
//! is the state describing itself.
//!
//! Two grammars are borrowed rather than re-drawn: the **board picker** is
//! the shipped setup-form component ([`BoardPicker`]), and the **flash
//! activity view** is the card-owned op flow's own
//! ([`card_op_activity`]) — the wizard runs the same op on the same card.

use dioxus::prelude::*;
use lpa_studio_core::{
    CardOp, ConnectHint, HomeOp, ProvisionPhase, SetupGesture, SetupState, UiAction, UiLogEntry,
    UiSetupProject, UiSetupRailPhase, UiSetupWizard,
};

use crate::app::home::card_sheet::{
    CardSheet, CardSheetButton, CardSheetButtons, CardSheetMessage, CardSheetTitle, SheetButtonTone,
};
use crate::app::home::device_card::{BoardPicker, card_op_activity};
use crate::app::home::package_card::home_action;

/// Dispatch one wizard gesture through the normal action path.
fn gesture(gesture: SetupGesture) -> UiAction {
    home_action(HomeOp::Setup(gesture))
}

/// The wizard as the BODY of the bound device's card (G2 ruling): the
/// rail (carrying the ✕ the card's own title bar has no room for), the
/// state's component, and the abandon guard — everything the standalone
/// card draws except its frame and title, which the DEVICE card owns.
///
/// The caller hands over a wizard whose `flash`/`console_tail` are the
/// CARD's live ones, so the flash step narrates from the same progressive
/// patch the card op overlay would have used.
pub(crate) fn setup_takeover_body(
    wizard: &UiSetupWizard,
    on_action: EventHandler<UiAction>,
) -> Element {
    let guard = matches!(wizard.state, SetupState::AbandonGuard { .. });
    rsx! {
        div { class: "tw:relative tw:flex tw:min-h-0 tw:flex-col",
            {steps_rail(wizard, Some(on_action))}
            div { class: "tw:grid tw:content-start tw:gap-3 tw:px-3 tw:py-3",
                if let Some(error) = wizard.error.clone() {
                    {failure_note(error)}
                }
                {wizard_body(wizard, on_action)}
            }
        }
        if guard {
            {abandon_guard_sheet(on_action)}
        }
    }
}

/// The wizard, as a card in the devices grid.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SetupWizardCard(wizard: UiSetupWizard, on_action: EventHandler<UiAction>) -> Element {
    // The sim path wears the violet (bound) family the studio reserves for
    // "this is the simulator"; hardware wears the working amber until it
    // becomes a real device card.
    let edge = if wizard.sim {
        "tw:border-l-[3px] tw:border-l-[var(--studio-status-bound-text)]"
    } else {
        "tw:border-l-[3px] tw:border-l-[var(--studio-status-warning-text)]"
    };
    let kind_chip = if wizard.sim {
        "tw:rounded-full tw:border tw:border-[var(--studio-status-bound-border)] tw:bg-[var(--studio-status-bound-bg)] tw:px-2 tw:py-0.5 tw:font-mono tw:text-[0.6rem] tw:font-bold tw:uppercase tw:tracking-wider tw:text-[var(--studio-status-bound-text)]"
    } else {
        "tw:rounded-full tw:border tw:border-[var(--studio-status-good-border)] tw:bg-[var(--studio-status-good-bg)] tw:px-2 tw:py-0.5 tw:font-mono tw:text-[0.6rem] tw:font-bold tw:uppercase tw:tracking-wider tw:text-[var(--studio-status-good-text)]"
    };
    let kind_label = if wizard.sim { "SIM" } else { "HW" };
    // The abandon guard is a card-resident SHEET over the flash that never
    // paused (design §7.8), which is why it is drawn around the body
    // rather than instead of it.
    let guard = matches!(wizard.state, SetupState::AbandonGuard { .. });

    rsx! {
        section {
            class: "tw:relative tw:grid tw:grid-rows-[auto_auto_1fr] tw:overflow-hidden tw:rounded-md tw:border tw:border-border-strong tw:bg-surface {edge}",
            aria_label: "{wizard.title}",
            header { class: "tw:flex tw:items-center tw:gap-2 tw:border-b tw:border-border-muted tw:bg-terminal tw:px-2.5 tw:py-1.5",
                span { class: "tw:min-w-0 tw:flex-1 tw:truncate tw:text-sm tw:font-bold tw:text-heading",
                    "{wizard.title}"
                }
                span { class: kind_chip, "{kind_label}" }
                button {
                    class: "tw:cursor-pointer tw:rounded tw:border-0 tw:bg-transparent tw:px-1.5 tw:text-sm tw:text-dim-foreground tw:hover:text-strong-foreground",
                    r#type: "button",
                    title: "Close",
                    aria_label: "Close setup",
                    onclick: move |_| on_action.call(gesture(SetupGesture::CloseRequested)),
                    "✕"
                }
            }
            {steps_rail(&wizard, None)}
            div { class: "tw:grid tw:content-start tw:gap-3 tw:px-3 tw:py-3",
                if let Some(error) = wizard.error.clone() {
                    {failure_note(error)}
                }
                {wizard_body(&wizard, on_action)}
            }
            if guard {
                {abandon_guard_sheet(on_action)}
            }
        }
    }
}

/// ABANDON_GUARD's card-resident sheet — the same one in both frames.
fn abandon_guard_sheet(on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        CardSheet {
            // The ✕ IS the question here — dismissing the sheet by
            // clicking away must not answer it, so the backdrop
            // repeats the (inert) close rather than abandoning.
            on_dismiss: move |()| on_action.call(gesture(SetupGesture::CloseRequested)),
            CardSheetTitle { text: "Flash in progress".to_string() }
            CardSheetMessage {
                text: "Leaving now abandons the write. The board keeps its bootloader \
                       (nothing bricks), but it will not run until a flash completes."
                    .to_string(),
            }
            CardSheetButtons {
                CardSheetButton {
                    label: "Keep flashing".to_string(),
                    tone: SheetButtonTone::Primary,
                    onclick: move |()| on_action.call(gesture(SetupGesture::KeepFlashing)),
                }
                CardSheetButton {
                    label: "Abandon".to_string(),
                    tone: SheetButtonTone::Destructive,
                    onclick: move |()| on_action.call(gesture(SetupGesture::Abandon)),
                }
            }
        }
    }
}

/// `Connect › Flash › Project › Done` — derived in core (the rail is a
/// property of the machine's position, not of this layout).
///
/// `close` is `Some` in the takeover frame ONLY: the device card's title
/// bar is the device's, so the flow's ✕ moves down to the rail rather
/// than displacing a card control (or, worse, disappearing — abandoning
/// mid-flash has to stay one click away).
fn steps_rail(wizard: &UiSetupWizard, close: Option<EventHandler<UiAction>>) -> Element {
    rsx! {
        div { class: "tw:flex tw:items-center tw:gap-1 tw:border-b tw:border-border-muted tw:bg-terminal tw:px-2.5 tw:py-1.5",
            for (index , step) in wizard.steps.iter().enumerate() {
                if index > 0 {
                    span { class: "tw:text-[0.6rem] tw:text-dim-foreground", "›" }
                }
                span {
                    class: match step.phase {
                        UiSetupRailPhase::Now => {
                            "tw:inline-flex tw:items-center tw:gap-1 tw:rounded-full tw:bg-surface-raised tw:px-1.5 tw:py-0.5 tw:text-[0.65rem] tw:font-bold tw:text-strong-foreground"
                        }
                        UiSetupRailPhase::Done => {
                            "tw:inline-flex tw:items-center tw:gap-1 tw:px-1 tw:text-[0.65rem] tw:font-bold tw:text-heading"
                        }
                        UiSetupRailPhase::Todo => {
                            "tw:inline-flex tw:items-center tw:gap-1 tw:px-1 tw:text-[0.65rem] tw:font-bold tw:text-dim-foreground"
                        }
                    },
                    span {
                        class: match step.phase {
                            UiSetupRailPhase::Todo => {
                                "tw:inline-flex tw:h-3.5 tw:w-3.5 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-border-strong tw:text-[0.55rem] tw:text-subtle-foreground"
                            }
                            _ => {
                                "tw:inline-flex tw:h-3.5 tw:w-3.5 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-accent tw:text-[0.55rem] tw:text-accent"
                            }
                        },
                        if step.phase == UiSetupRailPhase::Done {
                            "✓"
                        } else {
                            "{index + 1}"
                        }
                    }
                    "{step.label}"
                }
            }
            if let Some(on_action) = close {
                button {
                    class: "tw:ml-auto tw:cursor-pointer tw:rounded tw:border-0 tw:bg-transparent tw:px-1.5 tw:text-sm tw:text-dim-foreground tw:hover:text-strong-foreground",
                    r#type: "button",
                    title: "Close",
                    aria_label: "Close setup",
                    onclick: move |_| on_action.call(gesture(SetupGesture::CloseRequested)),
                    "✕"
                }
            }
        }
    }
}

/// One component per machine state. The match is exhaustive: a state with
/// no rendering would be a compile error here, which is the point.
fn wizard_body(wizard: &UiSetupWizard, on_action: EventHandler<UiAction>) -> Element {
    match &wizard.state {
        SetupState::ConnectIntro { hint } => connect_intro(*hint, on_action),
        SetupState::BoardFirst { chosen } => board_first(chosen.clone(), on_action),
        SetupState::PortPicking { .. } => spinner(
            "The browser is asking which port…",
            "Pick your board's port in the chooser.",
        ),
        SetupState::Probing { .. } => spinner(
            "Talking to the board…",
            "Reading the chip, and what is already on it.",
        ),
        SetupState::BoardPick(pick) => board_pick(wizard, pick.clone(), on_action),
        SetupState::WledFound { .. } => wled_found(wizard, on_action),
        SetupState::AlreadyLp { .. } => already_lp(wizard, on_action),
        SetupState::ProbeFailed { .. } => probe_failed(on_action),
        SetupState::Flashing { attempt, .. } | SetupState::AbandonGuard { attempt, .. } => {
            flashing(wizard.flash.as_ref(), &wizard.console_tail, *attempt)
        }
        SetupState::FlashFailed {
            attempt, detail, ..
        } => flash_failed(detail.clone(), *attempt, on_action),
        SetupState::Provision(provision) => {
            provision_step(wizard, provision.name.clone(), provision.phase, on_action)
        }
        SetupState::DeviceHome { adopted, .. } => device_home(wizard, *adopted),
        // The card is removed the moment a flow closes, so this is the
        // frame between the last command and the next view build.
        SetupState::Closed { .. } => rsx! {
            p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "Setup closed." }
        },
    }
}

/// CONNECT_INTRO: two stacked full-width CTAs. The hint escalates toward
/// the secondary — driver help is usually the real answer.
fn connect_intro(hint: ConnectHint, on_action: EventHandler<UiAction>) -> Element {
    let hint_line = match hint {
        ConnectHint::None => None,
        ConnectHint::PickerClosed => Some((
            false,
            "Picker closed without a pick — no rush, press it again when the board is plugged in."
                .to_string(),
        )),
        ConnectHint::NoPortsSeen => Some((
            true,
            "Nothing in the list? Some boards need a driver before the port shows up — \
             start from the board instead."
                .to_string(),
        )),
        ConnectHint::Disconnected => Some((
            true,
            "The board disconnected. Plug it back in, then try again.".to_string(),
        )),
    };
    rsx! {
        p { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground",
            "Connect your device now"
        }
        p { class: "tw:m-0 tw:text-xs tw:leading-normal tw:text-subtle-foreground",
            "Plug the board into this computer over USB, then pick its port."
        }
        if let Some((warned, line)) = hint_line {
            p {
                class: if warned {
                    "tw:m-0 tw:text-xs tw:text-[var(--studio-status-warning-text)]"
                } else {
                    "tw:m-0 tw:text-xs tw:text-dim-foreground"
                },
                "{line}"
            }
        }
        button {
            class: primary_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::ItsConnected)),
            "It's connected →"
        }
        button {
            class: secondary_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::PickBoardFirst)),
            "Pick my board first"
        }
        p { class: "tw:m-0 tw:text-[0.7rem] tw:text-dim-foreground",
            "Know the board already? Start there — drivers and setup help live on that path."
        }
    }
}

/// BOARD_FIRST: the full catalog, then board-specific connect guidance —
/// the CH340 driver steps for the boards that need them, cable and port
/// tips otherwise.
fn board_first(chosen: Option<String>, on_action: EventHandler<UiAction>) -> Element {
    let board = chosen
        .as_deref()
        .and_then(lpa_boards::board_by_id)
        .map(|board| (board.display_name.clone(), needs_usb_serial_driver(board)));
    rsx! {
        p { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground", "Select your board" }
        BoardPicker {
            selected: chosen.clone(),
            detected_chip: None,
            offer_generic: false,
            on_pick: move |board_id: Option<String>| {
                if let Some(board_id) = board_id {
                    on_action.call(gesture(SetupGesture::BoardChosen { board_id }));
                }
            },
        }
        if let Some((name, needs_driver)) = board {
            div { class: "tw:grid tw:gap-1.5 tw:rounded-lg tw:border tw:border-border tw:bg-surface-muted tw:p-2.5 tw:text-xs tw:text-muted-foreground",
                span { class: "tw:font-bold tw:text-strong-foreground", "{name}" }
                if needs_driver {
                    span {
                        "This board uses a CH340-class USB chip — it looks completely dead \
                         without the WCH driver."
                    }
                    span { class: "tw:font-mono tw:text-[0.7rem] tw:text-subtle-foreground",
                        "1 · Install the WCH CH34x driver (wch-ic.com/downloads)"
                    }
                    span { class: "tw:font-mono tw:text-[0.7rem] tw:text-subtle-foreground",
                        "2 · Approve it in System Settings › Privacy & Security"
                    }
                    span { class: "tw:font-mono tw:text-[0.7rem] tw:text-subtle-foreground",
                        "3 · Replug the board, then pick the port"
                    }
                } else {
                    span {
                        "USB-serial is native on this board — no driver needed. Not showing up? \
                         Try another cable (charge-only cables are silent killers) or another port."
                    }
                }
            }
            button {
                class: primary_cta_class(),
                r#type: "button",
                onclick: move |_| on_action.call(gesture(SetupGesture::ItsPluggedIn)),
                "It's plugged in — pick the port →"
            }
        }
        {back_link(on_action)}
    }
}

/// BOARD_PICK: the shipped picker, filtered to the detected chip, plus the
/// step's forward verb. Recognition rides above it when the probed MAC
/// matched a remembered row.
fn board_pick(
    wizard: &UiSetupWizard,
    pick: lpa_studio_core::BoardPickState,
    on_action: EventHandler<UiAction>,
) -> Element {
    let detected = wizard.detected_chip().map(str::to_string);
    let selected = pick.selected.clone();
    let armed = selected.is_some();
    let verb = if wizard.sim {
        "Continue →"
    } else if pick.replaces_firmware {
        "⚡ Replace with LightPlayer →"
    } else {
        "⚡ Flash LightPlayer →"
    };
    let sub = match (&detected, wizard.sim) {
        (_, true) => "The sim takes on the board — pins, wires, and limits follow it.".to_string(),
        (Some(chip), _) => format!("Blank board · chip {chip} detected — showing matches."),
        (None, _) => {
            "The chip did not identify itself — every board Studio can flash is listed.".to_string()
        }
    };
    let picked_line = selected
        .as_deref()
        .and_then(lpa_boards::board_by_id)
        .map(board_bio);
    rsx! {
        p { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground", "Select your board" }
        {recognition_line(wizard)}
        p { class: "tw:m-0 tw:text-[0.7rem] tw:text-dim-foreground", "{sub}" }
        if pick.replaces_firmware {
            p { class: "tw:m-0 tw:rounded-md tw:border tw:border-[var(--studio-status-warning-border)] tw:bg-[var(--studio-status-warning-bg)] tw:px-2.5 tw:py-1.5 tw:text-xs tw:text-[var(--studio-status-warning-text)]",
                "Flashing replaces the firmware already on this board."
            }
        }
        BoardPicker {
            selected: selected.clone(),
            detected_chip: detected,
            offer_generic: !wizard.sim,
            on_pick: move |board_id: Option<String>| {
                if let Some(board_id) = board_id {
                    on_action.call(gesture(SetupGesture::BoardChosen { board_id }));
                }
            },
        }
        if let Some(line) = picked_line {
            p { class: "tw:m-0 tw:font-mono tw:text-[0.65rem] tw:text-dim-foreground", "{line}" }
        }
        button {
            class: primary_cta_class(),
            r#type: "button",
            disabled: !armed,
            onclick: move |_| on_action.call(gesture(SetupGesture::Confirm)),
            "{verb}"
        }
        {back_link(on_action)}
    }
}

/// WLED_FOUND: the wipe offer. Migration is future work and the copy says
/// so — presets live in WLED's own backups, not here.
fn wled_found(wizard: &UiSetupWizard, on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground",
            "This board runs WLED."
        }
        {recognition_line(wizard)}
        div { class: "tw:grid tw:gap-2 tw:rounded-lg tw:border tw:border-[var(--studio-status-warning-border)] tw:bg-[var(--studio-status-warning-bg)] tw:p-2.5 tw:text-xs tw:leading-normal tw:text-[var(--studio-status-warning-text)]",
            span {
                "Setting it up for LightPlayer replaces WLED on the board — its presets stay \
                 in your WLED backups, not here. Importing WLED settings is on the roadmap; \
                 today is wipe-and-set-up."
            }
        }
        button {
            class: warn_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::WipeAndSetUp)),
            "Wipe and set up →"
        }
        button {
            class: secondary_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::Back)),
            "Keep WLED (cancel)"
        }
    }
}

/// ALREADY_LP: adopt is one click and writes nothing but the sighting.
///
/// **Done does not navigate** (G2 follow-up, 2026-08-05). Adopting a board
/// that was already glowing is not a setup; being thrown into the editor
/// for it read as one. Done ends the flow where the user already is, the
/// card goes back to its own body on the roster, and the editor is the
/// SECONDARY verb for whoever actually wanted it.
fn already_lp(wizard: &UiSetupWizard, on_action: EventHandler<UiAction>) -> Element {
    let name = wizard.recognised_name().unwrap_or("This board").to_string();
    let chip = wizard.detected_chip().unwrap_or("").to_string();
    rsx! {
        p { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground",
            "Already running LightPlayer"
        }
        div { class: "tw:grid tw:gap-0.5 tw:rounded-lg tw:border tw:border-border tw:border-l-[3px] tw:border-l-[var(--studio-status-good-text)] tw:bg-surface tw:p-2.5",
            span { class: "tw:text-xs tw:font-bold tw:text-strong-foreground", "{name}" }
            if !chip.is_empty() {
                span { class: "tw:font-mono tw:text-[0.65rem] tw:text-subtle-foreground", "{chip}" }
            }
        }
        p { class: "tw:m-0 tw:text-[0.7rem] tw:text-dim-foreground",
            "Done = it joins your roster as it is, and nothing is written — you stay right here, \
             and it keeps glowing. Re-flash, wipe, and rename live on its card afterwards."
        }
        button {
            class: primary_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::AdoptDone)),
            "Done"
        }
        button {
            class: secondary_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::AdoptAndOpen)),
            "Open in the editor →"
        }
        button {
            class: secondary_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::SetUpFresh)),
            "Set it up fresh…"
        }
    }
}

/// PROBE_FAILED: retry, replug hint, driver help. Never a dead end.
fn probe_failed(on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2 tw:rounded-lg tw:border tw:border-[var(--studio-status-error-border)] tw:bg-[var(--studio-status-error-bg)] tw:p-2.5 tw:text-xs tw:leading-normal tw:text-[var(--studio-status-error-text)]",
            span { class: "tw:font-bold",
                "The port opened, but nothing intelligible answered."
            }
            span { class: "tw:text-[0.7rem] tw:text-dim-foreground",
                "Try: hold BOOT while plugging in (that puts the chip in download mode) · \
                 another cable · another port."
            }
        }
        button {
            class: primary_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::Retry)),
            "Probe again"
        }
        button {
            class: secondary_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::PickBoardFirst)),
            "Driver help — start from the board"
        }
        {back_link(on_action)}
    }
}

/// FLASHING (and the guard sheet's body underneath it): the card-owned op
/// flow's own activity view, unchanged.
fn flashing(flash: Option<&CardOp>, tail: &[UiLogEntry], attempt: u32) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground",
            "Flashing firmware"
        }
        p { class: "tw:m-0 tw:text-xs tw:text-subtle-foreground",
            "Keep the device connected while Studio writes the image."
        }
        if attempt > 1 {
            p { class: "tw:m-0 tw:text-[0.7rem] tw:text-dim-foreground", "Attempt {attempt}." }
        }
        if let Some(flash) = flash {
            {card_op_activity(flash, tail)}
        } else {
            div { class: "tw:ux-card-op", role: "status",
                p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "Starting the write…" }
            }
        }
    }
}

/// FLASH_FAILED: retry re-runs FROM ERASE, and the second failure adds the
/// replug guidance — a wedged serial state survives a soft reset.
fn flash_failed(detail: String, attempt: u32, on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2 tw:rounded-lg tw:border tw:border-[var(--studio-status-error-border)] tw:bg-[var(--studio-status-error-bg)] tw:p-2.5 tw:text-xs tw:leading-normal tw:text-[var(--studio-status-error-text)]",
            span { class: "tw:font-bold", "The flash did not finish." }
            span { class: "tw:font-mono tw:text-[0.65rem]", "{detail}" }
            span { class: "tw:text-[0.7rem] tw:text-dim-foreground",
                "An incomplete image is never kept; retrying re-runs from erase."
            }
            if attempt > 1 {
                span { class: "tw:text-[0.7rem] tw:text-[var(--studio-status-warning-text)]",
                    "Still failing: unplug the board, plug it back in, then retry — a wedged \
                     serial state survives soft resets. Third strike: another cable or port."
                }
            }
        }
        button {
            class: primary_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::Retry)),
            "Retry flash"
        }
        button {
            class: secondary_cta_class(),
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::Abandon)),
            "Abandon"
        }
    }
}

/// PROVISION: the generated project (compact line + ⓘ) and, on hardware
/// only, the derived-then-editable device name.
fn provision_step(
    wizard: &UiSetupWizard,
    name: String,
    phase: ProvisionPhase,
    on_action: EventHandler<UiAction>,
) -> Element {
    let working = phase != ProvisionPhase::Editing;
    let verb = match (phase, wizard.sim) {
        (ProvisionPhase::Generating, _) => "Making your project…",
        (ProvisionPhase::Pushing, false) => "Sending it to the board…",
        (ProvisionPhase::Pushing, true) => "Starting the simulator…",
        (ProvisionPhase::Editing, false) => "Run it →",
        (ProvisionPhase::Editing, true) => "Start the simulator →",
    };
    let show_name = !wizard.sim;
    rsx! {
        if let Some(project) = wizard.project.clone() {
            {project_box(project)}
        }
        if show_name {
            div { class: "tw:grid tw:gap-1",
                label { class: "tw:text-[0.65rem] tw:font-extrabold tw:uppercase tw:tracking-wider tw:text-muted-foreground",
                    "Device name"
                }
                input {
                    class: "tw:w-full tw:rounded-md tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1.5 tw:text-sm tw:text-strong-foreground tw:outline-none tw:focus:border-accent",
                    value: "{name}",
                    disabled: working,
                    oninput: move |event| {
                        on_action.call(gesture(SetupGesture::NameEdited { name: event.value() }));
                    },
                }
                span { class: "tw:text-[0.65rem] tw:text-dim-foreground",
                    "Derived from the board and today's date — edit it only if you feel like it."
                }
            }
        }
        button {
            class: primary_cta_class(),
            r#type: "button",
            disabled: working,
            onclick: move |_| on_action.call(gesture(SetupGesture::Confirm)),
            "{verb}"
        }
    }
}

/// The generated project's box: one compact line, the rest behind ⓘ.
fn project_box(project: UiSetupProject) -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-2 tw:rounded-lg tw:border tw:border-accent tw:bg-surface-muted tw:p-2.5",
            span { class: "tw:text-xs tw:font-bold tw:text-strong-foreground",
                "Your first project"
                span { class: "tw:ml-1.5 tw:font-normal tw:text-dim-foreground",
                    "made for the {project.board_label}"
                }
            }
            div {
                class: "tw:flex tw:flex-wrap tw:items-center tw:gap-2 tw:rounded-md tw:border tw:border-border-muted tw:bg-terminal tw:px-2.5 tw:py-2 tw:font-mono tw:text-[0.7rem] tw:font-semibold tw:text-muted-foreground",
                span { "{project.summary}" }
                span {
                    class: "tw:cursor-help tw:border-b tw:border-dotted tw:border-dim-foreground tw:text-dim-foreground",
                    title: "{project.detail}",
                    "ⓘ what's inside"
                }
            }
            span { class: "tw:text-[0.65rem] tw:text-dim-foreground",
                "Add, swap, and remix effects in the editor — this just gets pixels moving."
            }
        }
    }
}

/// DEVICE_HOME: the frame between the last push and the device card. The
/// editor is already lensed to the target by the time this draws.
fn device_home(wizard: &UiSetupWizard, adopted: bool) -> Element {
    let headline = match (adopted, wizard.sim) {
        (true, _) => "Adopted — it never stopped glowing.",
        (false, false) => "It's glowing.",
        (false, true) => "The simulator is running.",
    };
    rsx! {
        p { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground", "{headline}" }
        div { class: "tw:h-6 tw:rounded-md tw:border tw:border-border-muted tw:bg-[linear-gradient(90deg,#14323f,#1d5c50,#7be0b2,#1d5c50,#14323f)]" }
        p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "Taking you to it…" }
    }
}

/// "was Porch sign" — the recognition copy the three verdict states carry
/// when the probed MAC matched a remembered row.
fn recognition_line(wizard: &UiSetupWizard) -> Element {
    let Some(name) = wizard.recognised_name() else {
        return rsx! {};
    };
    rsx! {
        p { class: "tw:m-0 tw:text-xs tw:text-heading", "You know this board — it was \"{name}\"." }
    }
}

fn spinner(title: &str, detail: &str) -> Element {
    rsx! {
        div { class: "tw:flex tw:items-center tw:gap-3 tw:py-4",
            span { class: "ux-card-op-bar is-indeterminate tw:w-10", span {} }
            div { class: "tw:grid tw:gap-0.5",
                span { class: "tw:text-xs tw:font-bold tw:text-strong-foreground", "{title}" }
                span { class: "tw:text-[0.7rem] tw:text-dim-foreground", "{detail}" }
            }
        }
    }
}

fn back_link(on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        button {
            class: "tw:cursor-pointer tw:justify-self-start tw:border-0 tw:bg-transparent tw:p-0 tw:text-[0.7rem] tw:font-bold tw:text-heading tw:hover:text-strong-foreground",
            r#type: "button",
            onclick: move |_| on_action.call(gesture(SetupGesture::Back)),
            "← Back"
        }
    }
}

/// The generate/push failure the machine has no edge for (design §7.10).
fn failure_note(error: String) -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-1 tw:rounded-lg tw:border tw:border-[var(--studio-status-error-border)] tw:bg-[var(--studio-status-error-bg)] tw:p-2.5 tw:text-xs tw:text-[var(--studio-status-error-text)]",
            span { class: "tw:font-bold", "That step did not finish." }
            span { class: "tw:font-mono tw:text-[0.65rem]", "{error}" }
            span { class: "tw:text-[0.7rem] tw:text-dim-foreground",
                "The board keeps whatever already landed on it — closing here leaves it on the \
                 roster, where a project can be pushed from the gallery."
            }
        }
    }
}

/// `<name> — <chip> · N LED wire(s) · default <pin>`.
fn board_bio(board: &lpa_boards::BoardDisplayFile) -> String {
    let wires = board.default_led_wires.len().max(1);
    let plural = if wires == 1 { "" } else { "s" };
    let pin = board.default_led_wire().unwrap_or("—");
    format!(
        "{} — {} · {wires} LED wire{plural} · default {pin}",
        board.display_name, board.family
    )
}

/// Whether this board's USB-serial bridge needs a driver before the port
/// appears — the CH340/CH34x class, which looks completely dead without
/// one (`classic-esp32-ch340k-macos-driver`).
fn needs_usb_serial_driver(board: &lpa_boards::BoardDisplayFile) -> bool {
    let haystack = format!("{} {}", board.board_id, board.display_name).to_ascii_lowercase();
    ["quinled", "dig-uno", "dig-quad", "nodemcu", "devkitc v4"]
        .iter()
        .any(|marker| haystack.contains(marker))
}

fn primary_cta_class() -> &'static str {
    "tw:w-full tw:cursor-pointer tw:rounded-md tw:border tw:border-[var(--studio-status-good-text)] tw:bg-transparent tw:px-3 tw:py-2 tw:text-sm tw:font-semibold tw:text-[var(--studio-status-good-text)] tw:disabled:cursor-not-allowed tw:disabled:opacity-40"
}

fn secondary_cta_class() -> &'static str {
    "tw:w-full tw:cursor-pointer tw:rounded-md tw:border tw:border-border-strong tw:bg-transparent tw:px-3 tw:py-2 tw:text-sm tw:font-semibold tw:text-muted-foreground tw:hover:text-strong-foreground"
}

fn warn_cta_class() -> &'static str {
    "tw:w-full tw:cursor-pointer tw:rounded-md tw:border tw:border-[var(--studio-status-warning-border)] tw:bg-transparent tw:px-3 tw:py-2 tw:text-sm tw:font-semibold tw:text-[var(--studio-status-warning-text)]"
}

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
    BootloaderEntryFlow, BundledFirmware, CardSheet as CardSheetState, CardTabView, CardUiOp,
    CardVerb, ControllerId, DEPLOY_NODE_ID, DeployOp, DeviceCardTab, DeviceController,
    DeviceDetailAffordance, DeviceOp, DeviceRichInput, DeviceTarget, HomeOp, LinkProviderKind,
    ProjectController, ProjectOp, RecoveryInstructions, RichObjectView, RichSection,
    RosterAffordance, RosterCardState, RosterTreatment, SimDetailAffordance, SimRichInput,
    UiAction, UiDeviceCard, UiDeviceProjectChip, UiStatusKind, device_card_tabs,
    device_rich_object, sim_rich_object,
};
use lpa_studio_core::{UiLogEntry, UiLogLevel};

use crate::app::home::card_sheet::{
    CardSheet, CardSheetButton, CardSheetButtons, CardSheetMessage, CardSheetTitle, SheetButtonTone,
};
use crate::app::home::device_play_tab::PlayTabBody;
use crate::app::home::package_card::home_action;
use crate::base::{NodeKindIcon, StudioIcon, StudioIconName};
use crate::core::{ActionButton, ActionButtonVariant, StatusChip, chip_status, quiet_action_class};

/// A card-resident sheet (D41) the card can open: THE destructive-confirm
/// pattern for card actions and the name-stamping sheet. One at a time;
/// the title bar is always spared. (The D30 drift sheet is GONE — §3c-2:
/// the diverged card's verbs live on its face.)
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DeviceCardSheet {
    /// A destructive confirm: title/message/verb come from the action's
    /// own [`lpa_studio_core::ActionConfirmation`]; Confirm dispatches it.
    Confirm(UiAction),
    /// The name-stamping sheet (NeedsAName — ratified spike round 3).
    /// Supersedes the title-bar-form entry for the unstamped board;
    /// renaming a STAMPED device stays the title bar's inline edit.
    Name,
    /// The troubleshooting sheet on the Not-responding card (M6 — the
    /// ladder's honest ending): concrete basic instructions plus the
    /// Reconnect and recovery-flash escapes. Card-resident per D41
    /// (supersedes the contract-era "merged-outline popup" language).
    Troubleshoot,
    /// The bootloader-entry ritual (M5): steps, waiting, confirmation.
    BootloaderEntry(BootloaderEntryFlow),
}

/// What a rendered affordance row does. Sheet and tab rows carry a
/// display-only action (label/icon/destructive chrome) that never
/// dispatches — the sheet's own confirm (or the opened tab's content)
/// carries the flow. A sheet row now carries the CORE sheet-state
/// ([`CardSheetState`]) it opens: clicking dispatches
/// `HomeOp::CardUi(OpenSheet)` so the open sheet lives in core, survives
/// the card ⇄ pane growth, and is e2e-drivable (device-lifecycle P2b).
#[derive(Clone, PartialEq)]
enum CardRowAction {
    Dispatch(UiAction),
    Sheet(CardSheetState, UiAction),
    /// Select a card tab (M8′: "Choose a project" jumps to the
    /// Project-tab picker).
    SelectTab(DeviceCardTab, UiAction),
}

impl CardRowAction {
    /// A row that just runs its action. With the confirm sheets now wired
    /// explicitly (each to its [`CardVerb`]) and provisioning one-click,
    /// nothing reaching here still carries a confirmation gate.
    fn from_action(action: UiAction) -> Self {
        Self::Dispatch(action)
    }
}

/// Reconstruct a confirm sheet's wired action from its core [`CardVerb`]
/// at render (the plan's "web maps verb→action" — core state stays a pure
/// value, never a boxed op). Each verb names exactly one card action; the
/// confirmation copy rides the action itself.
fn verb_to_action(verb: &CardVerb, card: &UiDeviceCard) -> UiAction {
    let card_key = card.identity_key();
    match verb {
        CardVerb::Erase => erase_device_action(card_key, card.name.clone()),
        CardVerb::Forget => {
            forget_device_action(card.uid.clone().unwrap_or_default(), card.name.clone())
        }
        CardVerb::StopSim => stop_simulator_action(),
        // Base meta carries the confirm copy ("can't be backed up…").
        CardVerb::WipeProject => UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::WipeProject {
                target: DeviceTarget::card(card_key),
            },
        ),
        CardVerb::Flash => flash_device_action_destructive(card_key),
        CardVerb::PushDrop { key } => push_project_action(card_key, key.clone()).with_confirmation(
            lpa_studio_core::ActionConfirmation::new(
                "Push this project",
                format!(
                    "Push the dropped project to \"{}\"? It replaces what the \
                     device runs now.",
                    card.name
                ),
                "Push",
            ),
        ),
    }
}

/// Project the core sheet-state onto the web render enum (the confirm arm
/// reconstructs its action via [`verb_to_action`]).
fn sheet_to_web(sheet: &CardSheetState, card: &UiDeviceCard) -> DeviceCardSheet {
    match sheet {
        CardSheetState::Confirm(verb) => DeviceCardSheet::Confirm(verb_to_action(verb, card)),
        CardSheetState::Name => DeviceCardSheet::Name,
        CardSheetState::Troubleshoot => DeviceCardSheet::Troubleshoot,
        CardSheetState::BootloaderEntry(flow) => DeviceCardSheet::BootloaderEntry(flow.clone()),
    }
}

/// Dispatch a tab selection through core (`HomeOp::CardUi`), keyed by the
/// card's identity so it lands on the right device in any module mode.
fn select_tab_action(card_key: &str, tab: DeviceCardTab) -> UiAction {
    home_action(HomeOp::CardUi(CardUiOp::SelectTab {
        card: card_key.to_string(),
        tab,
    }))
}

/// Dispatch opening a card-resident sheet through core.
fn open_sheet_action(card_key: &str, sheet: CardSheetState) -> UiAction {
    home_action(HomeOp::CardUi(CardUiOp::OpenSheet {
        card: card_key.to_string(),
        sheet,
    }))
}

/// Dispatch closing the open sheet through core.
fn close_sheet_action(card_key: &str) -> UiAction {
    home_action(HomeOp::CardUi(CardUiOp::CloseSheet {
        card: card_key.to_string(),
    }))
}

/// The setup form's date-default device name (state-flow model §1-A):
/// `YYYY-MM-DD HH:MM LightPlayer`. A fixed story clock derives a
/// deterministic UTC name (baselines never drift); live rendering uses
/// the platform's local clock.
/// The clamp's user-facing brightness, 0–100. 255 is "no reduction", so
/// the wire's 26 renders as the ~10% the boot log promises.
fn clamp_percent(clamp: u8) -> u32 {
    (u32::from(clamp) * 100 + 127) / 255
}

/// The setup form's default device name: `YYYY-MM-DD - <what it is>`.
///
/// Sortable date first (the spike's round-2 call: date is how people
/// organize devices), then the most specific thing we know — the picked
/// board's product slug, else the detected chip, else a bare fallback.
/// The name re-derives as the pick changes until the user types.
fn default_setup_name(now_secs: Option<f64>, discriminator: &str) -> String {
    format!("{} - {discriminator}", today_stamp(now_secs))
}

fn today_stamp(now_secs: Option<f64>) -> String {
    #[cfg(target_arch = "wasm32")]
    if now_secs.is_none() {
        let now = js_sys::Date::new_0();
        return format!(
            "{:04}-{:02}-{:02}",
            now.get_full_year(),
            now.get_month() + 1,
            now.get_date(),
        );
    }
    let secs = now_secs.unwrap_or(0.0) as i64;
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

/// The `vendor/product` id's product half — what a board contributes to a
/// device name (`seeed/xiao-esp32-c6` → `xiao-esp32-c6`).
fn board_slug(board_id: &str) -> &str {
    board_id.rsplit('/').next().unwrap_or(board_id)
}

/// Days-since-epoch → civil (y, m, d), Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The Recovery-mode face: the state's two exit verbs, each with a
/// one-line summary that lets the user self-select their case. Studio
/// cannot tell the cases apart — a board in download mode sends no hello,
/// so it cannot be linked to a registered device — which is why the copy
/// disambiguates instead of a wizard.
///
/// Order: Install first (the common new-board arrival), the rescue verb
/// second. Neither endangers the other case: a flash does not touch the
/// project partition, and a boot-control record on a non-LightPlayer
/// board is inert.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RecoveryFace(card_key: String, on_action: EventHandler<UiAction>) -> Element {
    let recovery_key = card_key.clone();
    rsx! {
        div { class: "tw:mt-1 tw:grid tw:gap-3",
            div { class: "tw:grid tw:gap-1",
                div {
                    CardSheetButton {
                        label: "⚡ Install firmware",
                        tone: SheetButtonTone::Primary,
                        onclick: move |_| {
                            on_action.call(UiAction::from_op(
                                ControllerId::new(DeviceController::NODE_ID),
                                DeviceOp::ProvisionFirmware {
                                    target: DeviceTarget::card(&recovery_key),
                                    setup_name: None,
                                    board_id: None,
                                },
                            ));
                        },
                    }
                }
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-subtle-foreground",
                    "Put LightPlayer on this board. New boards and boards running "
                    "other firmware start here. Projects on the board survive."
                }
            }
            div { class: "tw:grid tw:gap-1",
                div {
                    CardSheetButton {
                        label: "Start in safe mode",
                        tone: SheetButtonTone::Quiet,
                        onclick: {
                            let action = boot_safe_once_action(&card_key);
                            move |_| on_action.call(action.clone())
                        },
                    }
                }
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-subtle-foreground",
                    "Already runs LightPlayer, but its project stops it from "
                    "starting? Start once with the LEDs dimmed so you can "
                    "connect and fix the project."
                }
            }
            div { class: "tw:grid tw:gap-1",
                div {
                    CardSheetButton {
                        label: "Download a backup",
                        tone: SheetButtonTone::Quiet,
                        onclick: {
                            let action = back_up_filesystem_action(&card_key);
                            move |_| on_action.call(action.clone())
                        },
                    }
                }
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-subtle-foreground",
                    "About to try something drastic? Save everything on the "
                    "board to a ZIP on your computer first. This works even "
                    "though it will not start."
                }
            }
            // Wayfinding, not another verb: everything else recovery-shaped
            // (wipe, erase, and — once it lands — restore) lives in the
            // danger zone, and a user on this face should not have to know
            // that.
            button {
                // No underline: at this size a dotted underline through two
                // wrapped lines reads as STRIKETHROUGH — struck-out "wipe,
                // erase" is exactly the wrong signal. The arrow + hover
                // carry the clickability.
                class: "tw:cursor-pointer tw:border-0 tw:bg-transparent tw:p-0 tw:text-left tw:text-xs tw:text-muted-foreground tw:hover:text-strong-foreground tw:hover:underline",
                r#type: "button",
                onclick: {
                    let card_key = card_key.clone();
                    move |_| {
                        on_action.call(home_action(HomeOp::CardUi(CardUiOp::SelectTab {
                            card: card_key.clone(),
                            tab: DeviceCardTab::Danger,
                        })));
                    }
                },
                "More options — wipe, erase, or troubleshoot →"
            }
        }
    }
}

/// The blank board's SETUP FORM (state-flow model §1-A): the Settings
/// front door
/// IS the form — the board picker (M5), a prefilled date-default name,
/// and ONE Install button; no confirm, no separate naming dialog. The
/// name rides the provision op and stamps at first post-flash contact;
/// the board choice rides the same op and lands as `/hardware.json`.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SetupForm(
    card_key: String,
    /// Wall clock for the derived default name.
    now_secs: Option<f64>,
    /// OtherFirmware wears replacing copy; a blank board installing copy.
    replaces: bool,
    /// The core-owned board pick (`CardUiState::setup_board`); `None` =
    /// the generic fallback, which is also the untouched default.
    setup_board: Option<String>,
    /// Chip identity from passive/probe evidence — matching boards lead
    /// and the headline names the chip.
    detected_chip: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    // `None` until the user types: the name keeps deriving from the pick
    // (spike round 2), and the first keystroke freezes it.
    let mut typed_name = use_signal(|| None::<String>);
    let label = if replaces {
        "Replace with LightPlayer"
    } else {
        "Install firmware"
    };

    let boards = provisionable_boards();
    let picked = setup_board
        .as_deref()
        .and_then(|id| boards.iter().find(|board| board.board_id == id).copied());
    let discriminator = match (&picked, &detected_chip) {
        (Some(board), _) => board_slug(&board.board_id).to_string(),
        // The chip ID, not the raw report: a probe answer would otherwise
        // name the device `…-esp32c6qfn32revisionv02`.
        (None, Some(chip)) => chip_id(chip)
            .map(str::to_string)
            .unwrap_or_else(|| lpa_link::normalize_chip_name(chip)),
        (None, None) => "lightplayer".to_string(),
    };
    let derived_name = default_setup_name(now_secs, &discriminator);
    let name_value = typed_name().unwrap_or(derived_name);
    let submit_board = setup_board.clone();
    let submit_name = name_value.clone();
    let submit_card_key = card_key.clone();

    rsx! {
        div { class: "tw:mt-1 tw:grid tw:gap-3.5",
            // ONE line, and it is the ask (recovered from the parked
            // card-labels branch, d7a881424). The card TITLE already names
            // the chip (M1's chip-based title), so a "Set up ESP32-C6
            // device" headline directly beneath it said the same thing
            // twice. What went with it was the "or install generic and
            // choose later" half: picking the board is not a decision to
            // defer — generic writes no `/hardware.json`, so a device set
            // up that way has no pin map until someone comes back for it.
            // The offer is still there in the dashed cell, without the
            // copy inviting people to take it.
            p { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground",
                "Select your board"
            }
            BoardPicker {
                selected: setup_board.clone(),
                detected_chip: detected_chip.clone(),
                // Nothing picked yet is the IMPLICIT generic install:
                // marked quietly, never as a green choice.
                generic_is_implicit: true,
                on_pick: move |board_id| {
                    on_action.call(home_action(HomeOp::CardUi(CardUiOp::SelectSetupBoard {
                        card: card_key.clone(),
                        board_id,
                    })));
                },
            }
            div {
                // Same voice and same weight as "Select your board": the
                // form is two asks in a row, and a quiet field LABEL beside
                // a bold instruction read as two different kinds of thing.
                label { class: "tw:mb-1 tw:block tw:text-sm tw:font-bold tw:text-strong-foreground",
                    "Name your device"
                }
                input {
                    class: "tw:w-full tw:rounded-md tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1.5 tw:font-mono tw:text-sm tw:text-strong-foreground tw:outline-none",
                    value: "{name_value}",
                    oninput: move |event| typed_name.set(Some(event.value())),
                }
            }
            CardSheetButton {
                label: "⚡ {label}",
                tone: SheetButtonTone::Primary,
                onclick: move |_| {
                    let setup_name = Some(submit_name.trim().to_string())
                        .filter(|value| !value.is_empty());
                    on_action.call(UiAction::from_op(
                        ControllerId::new(DeviceController::NODE_ID),
                        DeviceOp::ProvisionFirmware {
                            target: DeviceTarget::card(&submit_card_key),
                            setup_name,
                            board_id: submit_board.clone(),
                        },
                    ));
                },
            }
        }
    }
}

/// The grid's ceiling: 4 rows of 3, one of which is Generic and — when
/// boards overflow — one is the "+N more" cell (gate round 3: the card is
/// allowed to grow, but not without bound).
const SETUP_TILE_LIMIT: usize = 11;

/// THE board picker: thumbnail tiles, chip-filtered, Generic first —
/// shipped as the setup form's grid (board-selection M5, gate rounds 2–3)
/// and reused verbatim by the setup wizard's BOARD_PICK and BOARD_FIRST
/// steps. There is one picker in this app; `on_pick` is the only thing
/// that differs between its callers (`None` = the generic fallback).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BoardPicker(
    /// The board picked so far; `None` = generic.
    selected: Option<String>,
    /// Chip identity from passive/probe evidence — matching boards lead.
    /// `None` shows the whole provisionable catalog (the wizard's sim
    /// entry, and any board whose chip nothing has named).
    detected_chip: Option<String>,
    /// Whether an unpicked state means "Generic is what will happen"
    /// (the setup form's implicit install) or simply "nothing picked yet"
    /// (the wizard, whose forward verb is not armed until a pick).
    #[props(default = false)]
    generic_is_implicit: bool,
    /// Whether the Generic tile is offered at all. The wizard's sim entry
    /// takes on a REAL board (pins, wires, limits follow it), so there is
    /// no no-decision cell there.
    #[props(default = true)]
    offer_generic: bool,
    on_pick: EventHandler<Option<String>>,
) -> Element {
    let mut show_all = use_signal(|| false);
    let boards = provisionable_boards();
    let detected = detected_chip.as_deref().and_then(chip_id);
    // A board for another chip CANNOT be flashed onto this device — the
    // guard refuses it — so when the chip is known, the other-chip boards
    // are not offered AT ALL. The 2026-08-02 walk collapsed them behind a
    // "+N other boards" disclosure; the 2026-08-03 gate-1 sitting judged
    // even that wrong — a disclosure reads as "more applicable choices",
    // and everything behind it was a dead end. When the chip is UNKNOWN
    // (no banner, unresolved report), the full list stands: detection, not
    // the catalog, is the uncertain half then.
    let applicable: Vec<&'static lpa_boards::BoardDisplayFile> = match detected {
        Some(chip) => boards
            .iter()
            .filter(|board| chip_id(&board.family) == Some(chip))
            .copied()
            .collect(),
        None => boards.clone(),
    };
    // Generic occupies the first cell, so the board capacity is one less;
    // the overflow cell fills the 12th (4 rows of 3). Plain grid
    // truncation — every tile behind it IS applicable.
    let capacity = if offer_generic {
        SETUP_TILE_LIMIT - 1
    } else {
        SETUP_TILE_LIMIT
    };
    let overflow = if show_all() {
        0
    } else {
        applicable.len().saturating_sub(capacity)
    };
    let shown: Vec<_> = if overflow == 0 {
        applicable
    } else {
        applicable.into_iter().take(capacity).collect()
    };
    let overflow_label = format!("+{overflow} more");

    rsx! {
        div { class: "tw:grid tw:grid-cols-3 tw:gap-2",
            // Generic leads: it is the no-decision outcome, and it has to
            // stay findable as the grid grows (gate round 3).
            if offer_generic {
                BoardTile {
                    board: None,
                    caption: "Generic".to_string(),
                    selected: selected.is_none(),
                    implicit: generic_is_implicit && selected.is_none(),
                    on_pick,
                }
            }
            for board in shown {
                BoardTile {
                    board: Some(board.clone()),
                    caption: board.display_name.clone(),
                    selected: selected.as_deref() == Some(board.board_id.as_str()),
                    implicit: false,
                    on_pick,
                }
            }
            if overflow > 0 && !show_all() {
                button {
                    class: "tw:flex tw:min-h-[4.5rem] tw:items-center tw:justify-center tw:rounded-lg tw:border tw:border-dashed tw:border-border tw:bg-transparent tw:text-xs tw:text-subtle-foreground tw:hover:border-strong tw:hover:text-strong-foreground",
                    r#type: "button",
                    onclick: move |_| show_all.set(true),
                    "{overflow_label}"
                }
            }
        }
    }
}

/// One tile in the board grid: the board's own drawing over its name.
/// `board: None` is the generic fallback, drawn as a dashed placeholder.
///
/// Selection is core-owned card state, so it survives re-renders and tab
/// switches. Three visual states, deliberately distinct (gate rounds 2–3):
/// picked (accent outline + check), the IMPLICIT generic default (quiet
/// dashed outline — "this is what plain Install does", not a choice), and
/// unpicked (no chrome until hover).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BoardTile(
    board: Option<lpa_boards::BoardDisplayFile>,
    caption: String,
    selected: bool,
    /// The unpicked generic cell: outlined quietly, never as a green pick.
    implicit: bool,
    /// What a click means. The tile knows the board, not the flow.
    on_pick: EventHandler<Option<String>>,
) -> Element {
    let board_id = board.as_ref().map(|board| board.board_id.clone());
    let class = match (selected, implicit) {
        (true, false) => {
            "tw:relative tw:grid tw:justify-items-center tw:gap-1.5 tw:rounded-lg tw:border tw:border-accent tw:bg-transparent tw:px-1 tw:py-2"
        }
        (true, true) => {
            "tw:relative tw:grid tw:justify-items-center tw:gap-1.5 tw:rounded-lg tw:border tw:border-dashed tw:border-border tw:bg-transparent tw:px-1 tw:py-2"
        }
        _ => {
            "tw:relative tw:grid tw:justify-items-center tw:gap-1.5 tw:rounded-lg tw:border tw:border-transparent tw:bg-transparent tw:px-1 tw:py-2 tw:hover:border-strong"
        }
    };
    rsx! {
        button {
            class,
            r#type: "button",
            onclick: move |_| on_pick.call(board_id.clone()),
            if selected && !implicit {
                span { class: "tw:absolute tw:right-1 tw:top-0.5 tw:text-[0.6rem] tw:font-bold tw:text-accent",
                    "✓"
                }
            }
            span { class: "lpb-setup-tile-art",
                if let Some(board) = board {
                    lpa_boards::BoardDiagram {
                        board,
                        mode: lpa_boards::DiagramMode::Plain,
                        scale: 0.26,
                        labels: false,
                    }
                } else {
                    span { class: "tw:flex tw:h-8 tw:w-10 tw:items-center tw:justify-center tw:rounded tw:border tw:border-dashed tw:border-border tw:font-mono tw:text-[0.6rem] tw:text-subtle-foreground",
                        "?"
                    }
                }
            }
            span {
                class: if selected && !implicit {
                    "tw:text-center tw:text-[0.65rem] tw:leading-tight tw:text-strong-foreground"
                } else {
                    "tw:text-center tw:text-[0.65rem] tw:leading-tight tw:text-subtle-foreground"
                },
                "{caption}"
            }
        }
    }
}

/// Resolve a chip name to its canonical id for family matching: probe
/// answers say `ESP32-C6 (QFN32) (revision v0.2)`, boot banners say
/// `esp32c6`, board families say `esp32c6`.
///
/// Deliberately the SAME resolution the flash guard and the build selector
/// use (`lpa_link::chip_id_from_reported`). A picker that matched boards by
/// one rule while the guard refused images by another would offer a board
/// and then refuse to flash it.
fn chip_id(name: &str) -> Option<&'static str> {
    lpa_link::chip_id_from_reported(name)
}

/// The boards the picker offers: a build this deployment SERVES exists for
/// the board (`lp-fw/builds/served.json` — flashing is real, not
/// aspirational) AND its runtime manifest is checked in (the
/// `/hardware.json` write needs the bytes). Display-only catalog boards stay
/// out until both land.
fn provisionable_boards() -> Vec<&'static lpa_boards::BoardDisplayFile> {
    lpa_boards::all_boards()
        .iter()
        .filter(|board| {
            lpa_boards::runtime_manifest_json(&board.board_id).is_some()
                && lpa_boards::compatible_builds_for(board)
                    .iter()
                    .any(|candidate| lpa_boards::is_served(&candidate.build.id))
        })
        .collect()
}

/// One roster card: the device (or live sim session) as a tabbed control
/// panel. The grow control is the editor entry; the tabs carry the ▶
/// picture, the Settings front door (health + technical — the Status tab
/// folded into it at the honest-preview G1), project, console (P2), and
/// the danger zone; body clicks are quiet (drop targets stay live).
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
    /// D43 pane mode: the SAME card grown into the editor's right-side
    /// column — tall body, ⇲ shrinks back to the gallery. The console is
    /// the Console tab here exactly as at card scale (G1b ruling 7
    /// retired round 3.5's permanent bottom region and the ambient
    /// strip).
    #[props(default = false)]
    pane: bool,
    /// Studio's bundled firmware image (packaged manifest), when known —
    /// evidence for the advisory "firmware update available" chip on the
    /// Settings tab (and its badge).
    #[props(default)]
    bundled_fw: Option<BundledFirmware>,
    /// The library projects the Project-tab picker offers on an empty
    /// device (M8′: "choose a project" — list → push; no dialog). The
    /// gallery passes its hydrated project list; empty hides the picker.
    #[props(default)]
    project_choices: Vec<UiDeviceProjectChip>,
    /// The setup flow bound to THIS card, rendered as a **body takeover**
    /// (G2 ruling, 2026-08-05): the header stays the device's and grows
    /// real facts as they land, while the tabs, hero strip, and op overlay
    /// stand aside for the wizard until the flow hands back. The gallery
    /// passes it to the one card whose key the wizard names; every other
    /// card gets `None` and renders normally.
    #[props(default)]
    setup: Option<lpa_studio_core::UiSetupWizard>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let now = now_secs.unwrap_or_else(super::package_card::platform_now_secs);
    let status_line = card.state.status_line(now);
    let faded = matches!(card.state, RosterCardState::Offline { .. });
    // (the "last-known, not current" dimming moved onto the ▶ tab with the
    // hero strip's removal — `PlayTabBody` owns it now)
    // The bound flow owns the body while it runs (G2 ruling). It borrows
    // the CARD's live op and console tail rather than the snapshot the
    // view build handed the wizard: those two are patched progressively
    // between builds, so this is what keeps the flash step's bar and
    // terminal moving at the same rate the card's own overlay would.
    let takeover = setup.map(|mut wizard| {
        if let Some(op) = card.ui.op.clone() {
            wizard.flash = Some(op);
        }
        if !card.console_tail.is_empty() {
            wizard.console_tail = card.console_tail.clone();
        }
        wizard
    });
    let in_setup = takeover.is_some();
    // Needs-a-name opens the name-stamping SHEET (D41 — ratified spike
    // round 3); renaming a stamped device stays the title-bar inline edit.
    //
    // Mid-setup the card has exactly one surface: the flow. Renaming
    // (PROVISION owns the name), dropping a project onto a board that is
    // still being flashed, and the hero strip's leftover project all
    // stand aside until the flow hands back.
    let name_inline = !sim && !in_setup && matches!(card.state, RosterCardState::NeedsAName);
    let can_rename = card.uid.is_some() && !sim && !in_setup;
    let droppable = !sim && !faded && !in_setup;

    // The rich-object view: sections wired to concrete rows here (the
    // one identity→action hop — a row dispatches or opens a card sheet),
    // rollup tone for the edge, then the ratified sections→tabs grouping.
    let sections: Vec<RichSection<CardRowAction>> = if sim {
        sim_rich_object(&SimRichInput {
            state: &card.state,
            project_name: card.project.as_ref().map(|chip| chip.name.as_str()),
            // D4: "as ESP32-S3 DevKitC-1" under the status line
            board_id: card.board_id.as_deref(),
            now_secs: now,
        })
        .sections
        .into_iter()
        .map(wire_sim_card_section)
        .collect()
    } else {
        device_rich_object(&DeviceRichInput {
            state: &card.state,
            uid: card.uid.as_deref(),
            transport: &card.transport,
            project_name: card.project.as_ref().map(|chip| chip.name.as_str()),
            fw: card.fw.as_ref(),
            hardware: card.hardware.as_ref(),
            bundled_fw: bundled_fw.as_ref(),
            detected_chip: card.detected_chip.as_deref(),
            port_label: card.port_label.as_deref(),
            board_id: card.board_id.as_deref(),
            now_secs: now,
        })
        .sections
        .into_iter()
        .map(|section| wire_card_section(&card, section))
        .collect()
    };
    let view = RichObjectView::new(sections);
    let edge_tone = view.rollup().tone;
    // The ▶ tab exists exactly when the card holds a project: that is the
    // one condition under which there is something honest to draw (the
    // device's frames, or the sim's own re-simulation). A board with
    // nothing on it gets no picture tab — its front-door body says so instead.
    let mut tabs = device_card_tabs(view, card.project.is_some());
    // M8′: the Connected-empty device offers the Project-tab PICKER —
    // the tab exists exactly when there is something honest to offer
    // (choices in the library), mirroring the data-adaptive rule.
    let picker_mode = !sim
        && matches!(card.state, RosterCardState::ConnectedEmpty)
        && !project_choices.is_empty()
        && !tabs.iter().any(|tab| tab.tab == DeviceCardTab::Project);
    if picker_mode {
        // Straight after the front door (Settings), wherever it sits —
        // the ▶ tab now leads the row on cards that have one, so index 1
        // is no longer a synonym for "after the front door".
        let after_front_door = tabs
            .iter()
            .position(|tab| tab.tab == DeviceCardTab::Details)
            .map_or(0, |index| index + 1);
        tabs.insert(
            after_front_door,
            CardTabView {
                tab: DeviceCardTab::Project,
                sections: Vec::new(),
                badge: None,
            },
        );
    }
    let edge_treatment = card.state.spec().treatment;

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
                | RosterCardState::EditedOnDevice { .. }
        )
        .then(open_device_project_action)
    };

    // Tab + open sheet are CORE-OWNED view-state now (device-lifecycle
    // P2b): they ride `card.ui`, keyed by identity, so they survive the
    // card ⇄ pane growth and are e2e-drivable. The card key threads every
    // interaction back to core via `HomeOp::CardUi`.
    let card_key = card.identity_key().to_string();
    // a state change may drop the selected tab (e.g. Danger during an
    // operation): fall back to Settings (the front door) rather than a
    // blank body. Console is an ordinary tab in BOTH modes (G1b ruling
    // 7 reversed round 3.5's pane exclusion).
    let active_tab = tabs
        .iter()
        .find(|tab| tab.tab == card.ui.tab)
        .map_or(DeviceCardTab::Details, |tab| tab.tab);
    // The open sheet projected to the render enum (confirm arm rebuilt
    // from its verb). `None` = no sheet.
    let active_sheet = card.ui.sheet.as_ref().map(|s| sheet_to_web(s, &card));

    let mut renaming = use_signal(|| false);
    let rename_reset = card.name.clone();
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
            class: device_card_class(faded, edge_treatment, pane),
            style: "{edge_style}",
            title: "{status_line}",
            // connected cards are drop targets: project card → device card
            // opens the push-confirm sheet (M8′ — the drop aims, the
            // sheet's verb consents)
            ondragover: move |event| {
                if droppable {
                    event.prevent_default();
                }
            },
            ondrop: {
                let card_key = card_key.clone();
                move |event: DragEvent| {
                    if !droppable {
                        return;
                    }
                    event.prevent_default();
                    // M8′: the drop opens the push-confirm SHEET (D41) —
                    // dropping aims, the sheet's verb is the D11 consent.
                    // The verb (with the dropped key) rides core state; the
                    // confirm copy is rebuilt from it at render.
                    if let Some(key) = super::package_card::take_dragged_project() {
                        on_action.call(open_sheet_action(
                            &card_key,
                            CardSheetState::Confirm(CardVerb::PushDrop { key }),
                        ));
                    }
                }
            },
            // D40 title bar: kind glyph · inline-editable name · transport
            // label · the always-visible grow control.
            header { class: "tw:flex tw:min-h-9 tw:flex-none tw:items-center tw:gap-2 tw:border-b tw:border-border tw:bg-terminal tw:py-1.5 tw:pl-3 tw:pr-1.5",
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
                                if !name.is_empty()
                                    && let Some(uid) = &uid
                                {
                                    // stamped devices rename inline; the
                                    // unstamped board's naming is the D41
                                    // sheet, never this form
                                    on_action.call(home_action(HomeOp::RenameDevice {
                                        uid: uid.clone(),
                                        name,
                                    }));
                                }
                                renaming.set(false);
                            }
                        },
                        input {
                            class: "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border tw:bg-card tw:px-2 tw:py-0.5 tw:text-sm tw:text-strong-foreground",
                            autofocus: true,
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
                            "Rename"
                        }
                    }
                } else {
                    p {
                        class: device_name_class(faded, can_rename || name_inline),
                        title: if can_rename { "Rename this device" } else if name_inline { "Name this device" } else { "" },
                        onclick: {
                            let card_key = card_key.clone();
                            move |_| {
                                if can_rename {
                                    renaming.set(true);
                                } else if name_inline {
                                    on_action.call(open_sheet_action(&card_key, CardSheetState::Name));
                                }
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
                    if pane {
                        // D43: the grown pane's ⇲ shrinks back to the
                        // gallery — the same route + detach pair the
                        // shell wordmark uses (the hash may not change).
                        a {
                            class: "{grow_button_class(transport_label.is_empty())} tw:no-underline",
                            href: "/devices",
                            title: "Back to the gallery",
                            aria_label: "Shrink {card.name} back to the gallery",
                            onclick: move |_| {
                                on_action.call(UiAction::from_op(
                                    ProjectController::NODE_ID,
                                    ProjectOp::DetachLens,
                                ));
                            },
                            StudioIcon { name: StudioIconName::Shrink, size: 14 }
                        }
                    } else {
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
            }
            // NO hero strip. The D12/P05 strip re-simulated the project in
            // the browser and presented the result as the board's face —
            // which the 2026-08-05 G2 ruling called what it was:
            // dishonest, and letterboxed besides. What the device is
            // actually doing lives on the ▶ tab now, drawn from frames the
            // board published; the project's identity rides that tab's
            // meta row.
            //
            // Everything below the title bar shares one wrapper: a D41
            // sheet dims exactly this region, so the name above it stays
            // readable (spike round 3: sheets spare the title bar).
            //
            // GRID STACK, not an absolute overlay (2026-07-31). The tab
            // content, the sheet, and the op overlay all occupy the same
            // grid cell, so the region is as tall as the TALLEST of them —
            // a sheet can grow the card. The previous scheme positioned
            // overlays absolutely and compensated with a hand-maintained
            // per-sheet min-height table, which produced a recurring class
            // of clipping bugs (Name at 210px, the bootloader sheet, then
            // the troubleshoot sheet leaving the user stuck with no
            // scroll). Content-driven height deletes the class.
            div { class: if pane { "ux-card-stack tw:min-h-0 tw:flex-1" } else { "ux-card-stack" },
                // The flow, wearing this card's body: the rail (with the
                // flow's ✕), the state's own component, and the abandon
                // guard. No tabs, no console strip, no op overlay — the
                // wizard IS the narration until it hands back, and at
                // DEVICE_HOME the card's own body below simply returns,
                // without the card ever leaving the grid.
                if let Some(wizard) = takeover.as_ref() {
                    {crate::app::home::setup_wizard::setup_takeover_body(wizard, on_action)}
                }
                // The card's own body: exactly when no flow has taken it.
                if !in_setup {
                div { class: if pane { "tw:relative tw:flex tw:min-h-0 tw:flex-col" } else { "tw:relative tw:flex tw:flex-col" },
                    // the icon-tab row (below the title bar — spike anatomy)
                    div {
                        class: "tw:flex tw:flex-none tw:gap-0.5 tw:border-b tw:border-border tw:bg-terminal tw:px-1.5 tw:py-1",
                        role: "tablist",
                        for tab_view in tabs.iter() {
                            {tab_button(tab_view, active_tab, &card_key, on_action)}
                        }
                    }
                    // Safe-mode callout: above the tab body so it is visible
                    // on EVERY tab — a clamped board looks broken (dim), and
                    // the only exit is a power cycle the user must be told
                    // about. Evidence: live heartbeat only (core clears it
                    // the moment the link drops).
                    if let Some(clamp) = card.safe_clamp {
                        div { class: "ux-safe-mode-callout",
                            p { class: "tw:m-0 tw:text-xs tw:font-semibold",
                                "Safe mode — output limited to {clamp_percent(clamp)}%"
                            }
                            p { class: "tw:m-0 tw:text-xs tw:leading-snug",
                                "This boot is intentionally dimmed for recovery. Fix the project, then unplug the board and plug it back in to restore full brightness."
                            }
                        }
                    }
                    div { class: if pane { "tw:grid tw:min-h-0 tw:flex-1 tw:content-start tw:gap-1.5 tw:overflow-y-auto tw:p-3" } else { "tw:grid tw:content-start tw:gap-1.5 tw:p-3" },
                        match active_tab {
                            DeviceCardTab::Play => rsx! {
                                PlayTabBody {
                                    card: card.clone(),
                                    sim,
                                    now,
                                    // The same editor entry the title-bar ⤢
                                    // dispatches (G1: the picture carries its
                                    // own way in).
                                    open_action: grow_action.clone(),
                                    // The gone device's way back, IN the box
                                    // (G1b ruling 8).
                                    reconnect_action: (!sim
                                        && matches!(
                                            card.state,
                                            RosterCardState::Offline { .. }
                                                | RosterCardState::NotResponding
                                        ))
                                    .then(|| reconnect_device_action(card.uid.clone())),
                                    on_action,
                                }
                            },
                            DeviceCardTab::Details => rsx! {
                                {details_tab_body(&card, &tabs, on_action, &card_key, now_secs)}
                            },
                            DeviceCardTab::Console => rsx! {
                                {console_tab_body(&card.console_tail)}
                            },
                            DeviceCardTab::Project if picker_mode => rsx! {
                                {project_picker_body(&card_key, &project_choices, on_action)}
                            },
                            _ => rsx! {
                                {sections_tab_body(&tabs, active_tab, on_action, &card_key)}
                            },
                        }
                    }
                    // No permanent pane console region and no ambient
                    // strip (G1b ruling 7, amending D42/round 3.5): the
                    // console is the Console TAB in both modes — the strip
                    // and the always-open region earned their pixels on
                    // neither surface.
                }
                }
                if let Some(active_sheet) = active_sheet.as_ref().filter(|_| !in_setup) {
                    {device_card_sheet_view(active_sheet, &card, &card_key, on_action)}
                }
                // In-place op progress (device-lifecycle P2): the LAST
                // child of the body wrapper, so it covers the tab row and
                // body (the title bar above is spared) — a heavy op takes
                // over the card here, never an app-level modal. Mid-setup
                // the wizard's own flash step is already showing this op
                // (same value, same activity view), so the overlay stands
                // down rather than double-narrating it.
                if let Some(op) = card.ui.op.as_ref().filter(|_| !in_setup) {
                    {card_op_overlay(op, &card, &card_key, on_action)}
                }
            }
        }
    }
}

/// "Copy details" on a failed op: the whole failure context — error,
/// device state, chip, board choice, running build, console tail — as one
/// pasteable block, for handing to an agent or a bug report.
///
/// The failure overlay already SHOWS this material, but showing it and
/// being able to relay it are different problems: transcribing an error
/// plus a dozen console lines by hand is where bug reports lose their
/// evidence.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CopyDetailsButton(report: String) -> Element {
    let mut copied = use_signal(|| false);
    rsx! {
        button {
            class: "ux-card-op-copy",
            r#type: "button",
            title: "Copy the error, device state, and console tail",
            onclick: move |_| {
                crate::clipboard::write_text(&report);
                copied.set(true);
            },
            if copied() { "Copied ✓" } else { "Copy details" }
        }
    }
}

/// The in-place op overlay (device-lifecycle P2 + state-flow model §2):
/// the card's own progress while a heavy op flow runs on it. Three
/// phases: Running (label + bar + terminal), AwaitingDevice (the op's
/// EXPECTED disconnect — indeterminate, overlay stays up), and Failed
/// (error in the terminal + the ONE exit to the nearest stable state,
/// I4 — dispatches `CardUi(ClearOp)` so the card re-derives). Covers the
/// tabs and blurs the body; never an elevated dialog.
fn card_op_overlay(
    op: &lpa_studio_core::CardOp,
    card: &UiDeviceCard,
    card_key: &str,
    on_action: EventHandler<UiAction>,
) -> Element {
    use lpa_studio_core::CardOpPhase;
    let tail = &card.console_tail;
    if let CardOpPhase::Failed { error, exit_label } = &op.phase {
        let card_key = card_key.to_string();
        let exit_label = exit_label.clone();
        // Built EAGERLY, while the failure's context is still on the card:
        // the report has to describe the moment it failed, not whatever
        // the card has drifted to by the time someone clicks.
        let report = crate::app::home::failure_report::failure_report(card, op);
        return rsx! {
            div { class: "ux-card-op", role: "alert",
                span { class: "ux-card-op-label ux-card-op-label-failed", "{op.label}" }
                details { class: "ux-card-op-term", open: true,
                    summary { "Technical details" }
                    div { class: "ux-console-line-error", "{error}" }
                    for entry in tail.iter() {
                        div { class: console_line_class(entry.level), "{entry.message}" }
                    }
                }
                div { class: "tw:flex tw:items-center tw:justify-between tw:gap-2",
                    if let Some(report) = report {
                        CopyDetailsButton { report }
                    }
                    button {
                        class: "ux-card-op-exit",
                        r#type: "button",
                        onclick: move |_| {
                            on_action.call(home_action(HomeOp::CardUi(CardUiOp::ClearOp {
                                card: card_key.clone(),
                            })));
                        },
                        "{exit_label}"
                    }
                }
            }
        };
    }
    card_op_activity(op, tail)
}

/// The WORKING half of the card's op overlay — label, bar, terminal — with
/// no card behind it. The setup wizard's FLASHING step renders this
/// verbatim: the flash is the same op, narrated by the same card-owned
/// flow, so the two surfaces cannot drift.
pub(crate) fn card_op_activity(op: &lpa_studio_core::CardOp, tail: &[UiLogEntry]) -> Element {
    use lpa_studio_core::CardOpPhase;
    let indeterminate = op.percent.is_none() || matches!(op.phase, CardOpPhase::AwaitingDevice);
    let bar_class = if indeterminate {
        "ux-card-op-bar is-indeterminate"
    } else {
        "ux-card-op-bar"
    };
    let fill_style = op
        .percent
        .map(|pct| format!("width: {}%;", pct.min(100)))
        .unwrap_or_default();
    rsx! {
        div { class: "ux-card-op", role: "status", aria_busy: "true",
            span { class: "ux-card-op-label", "{op.label}" }
            div { class: bar_class,
                span { style: "{fill_style}" }
            }
            {card_op_terminal(tail)}
        }
    }
}

/// The op overlay's TERMINAL alone — the log tail under a "Technical
/// details" disclosure. Split out for the setup wizard's waiting steps
/// (CONNECTING, PROBING), which have link output to show but no op to draw
/// a labelled bar for; one definition so the two cannot drift.
pub(crate) fn card_op_terminal(tail: &[UiLogEntry]) -> Element {
    rsx! {
        details { class: "ux-card-op-term", open: true,
            summary { "Technical details" }
            if tail.is_empty() {
                div { "Waiting for device output…" }
            } else {
                for entry in tail.iter() {
                    div { class: console_line_class(entry.level), "{entry.message}" }
                }
            }
        }
    }
}

/// The Console tab (D42, card mode): the session's tail, read-only.
/// Level/filtering controls are display-only in P2 — severity shows as
/// line tint, nothing else.
fn console_tab_body(tail: &[UiLogEntry]) -> Element {
    if tail.is_empty() {
        return rsx! {
            p { class: "tw:m-0 tw:font-mono tw:text-xs tw:text-dim-foreground",
                "No console output yet."
            }
            {trace_footer()}
        };
    }
    rsx! {
        div { class: "ux-console-lines",
            for entry in tail {
                div { class: console_line_class(entry.level), "{entry.message}" }
            }
        }
        {trace_footer()}
    }
}

/// The device-trace export affordance (M0): copies the lifecycle event
/// trace — including the PREVIOUS session's, which survives the refresh
/// that "fixed" the jank — as JSONL to the clipboard.
fn trace_footer() -> Element {
    rsx! {
        div { class: "tw:mt-1.5 tw:flex tw:justify-end",
            button {
                class: "ux-card-op-copy",
                r#type: "button",
                title: "Copy the device lifecycle event trace (this session and the previous one) as JSONL",
                onclick: move |_| crate::device_events_io::copy_trace(),
                "Copy device trace"
            }
        }
    }
}

/// Severity tint for a console line (the shared status families).
fn console_line_class(level: UiLogLevel) -> &'static str {
    match level {
        UiLogLevel::Trace | UiLogLevel::Debug => "ux-console-line-dim",
        UiLogLevel::Info => "",
        UiLogLevel::Warn => "ux-console-line-warn",
        UiLogLevel::Error => "ux-console-line-error",
    }
}

/// One icon tab: selection wears the card tint (`.ux-device-tab` — the
/// Danger tab the error family), the badge dot the per-tab announcement.
fn tab_button<A>(
    tab_view: &CardTabView<A>,
    active_tab: DeviceCardTab,
    card_key: &str,
    on_action: EventHandler<UiAction>,
) -> Element {
    let tab = tab_view.tab;
    let label = tab.label();
    let card_key = card_key.to_string();
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
                on_action.call(select_tab_action(&card_key, tab));
            },
            StudioIcon { name: tab_icon(tab), size: 14 }
            if let Some(badge_style) = badge_style {
                span { class: "ux-device-tab-badge", style: "{badge_style}" }
            }
        }
    }
}

/// The Details front door: the Health section with the status line up front, and
/// the state-table affordance — today's card body, re-homed. The project's
/// identity rides the ▶ tab's meta row (honest-device preview P3) rather
/// than a row here.
fn details_tab_body(
    card: &UiDeviceCard,
    tabs: &[CardTabView<CardRowAction>],
    on_action: EventHandler<UiAction>,
    card_key: &str,
    now_secs: Option<f64>,
) -> Element {
    // The folded front door (G1: Status retired; G1b: renamed Details): health
    // sections keep their narrative treatment and the forms; Technical
    // sections follow as plain fact rows.
    let sections = tabs
        .iter()
        .find(|tab| tab.tab == DeviceCardTab::Details)
        .map(|tab| tab.sections.as_slice())
        .unwrap_or_default();
    let (technical, health): (Vec<_>, Vec<_>) = sections
        .iter()
        .partition(|section| section.title == "Technical");
    // The blank board's front door IS the setup form (model §1-A): the
    // form replaces the affordance rows (SetUp came through them).
    let setup_form = !card.sim
        && matches!(
            card.state,
            RosterCardState::ReadyToSetUp | RosterCardState::OtherFirmware
        );
    // Recovery mode's front door carries the exit verbs DIRECTLY (bench
    // feedback 2026-07-31): a user here has already done the hard part,
    // and routing them through Troubleshoot — a sheet that mostly explains
    // how to get INTO this state — was backwards.
    let recovery_face = !card.sim && card.state == RosterCardState::RecoveryMode;
    rsx! {
        for section in health.iter() {
            for line in section.lines.iter() {
                if line.label == "status" {
                    // the headline: tinted like the edge (the spike's
                    // status line), never a bare kv row. The setup form
                    // brings its OWN headline naming the chip ("Set up
                    // ESP32-C6 device"), so a generic "Ready to set up"
                    // above it would be two lines saying one thing.
                    if !setup_form {
                        p { class: "tw:m-0 tw:truncate tw:text-xs tw:font-semibold",
                            style: "color: var(--edge-tint);",
                            "{line.value}"
                        }
                    }
                } else {
                    // the §3a explain-the-situation line WRAPS — truncating
                    // an explanation defeats it (2026-07-26: the drift
                    // story clipped mid-sentence)
                    p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-subtle-foreground",
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
        if setup_form {
            SetupForm {
                card_key: card_key.to_string(),
                now_secs,
                replaces: matches!(card.state, RosterCardState::OtherFirmware),
                setup_board: card.ui.setup_board.clone(),
                detected_chip: card.detected_chip.clone(),
                on_action,
            }
        } else if !card.sim && card.state == RosterCardState::NeedsAName {
            // The flashed-but-unstamped board names itself ON the card —
            // the setup form's naming half, reused. The "Name it" button →
            // sheet detour predates the on-card form (sitting feedback,
            // 2026-08-03: "we moved mostly away from the little dialogs").
            NameForm {
                card_key: card_key.to_string(),
                board_id: card.hardware.as_ref().and_then(|hw| hw.board_id.clone()),
                detected_chip: card.detected_chip.clone(),
                now_secs,
                on_action,
            }
        } else if recovery_face {
            RecoveryFace { card_key: card_key.to_string(), on_action }
        } else {
            for section in health.iter() {
                for row in section.affordances.iter() {
                    div { class: "tw:mt-1",
                        {row_button(row, ActionButtonVariant::Quiet, on_action, card_key)}
                    }
                }
            }
        }
        // Technical facts follow the health story — the same rows the
        // standalone Settings tab always drew.
        for section in technical.iter() {
            {fact_section(section, false, on_action, card_key)}
        }
    }
}

/// A plain tab's body: the tab's sections as compact fact rows +
/// advisory chip + affordances. Danger rows render as destructive menu
/// rows (inspector-row convention); other affordances as quiet chips.
fn sections_tab_body(
    tabs: &[CardTabView<CardRowAction>],
    active_tab: DeviceCardTab,
    on_action: EventHandler<UiAction>,
    card_key: &str,
) -> Element {
    let sections = tabs
        .iter()
        .find(|tab| tab.tab == active_tab)
        .map(|tab| tab.sections.as_slice())
        .unwrap_or_default();
    let menu_rows = active_tab == DeviceCardTab::Danger;
    rsx! {
        for section in sections {
            {fact_section(section, menu_rows, on_action, card_key)}
        }
    }
}

/// One section as compact fact rows + advisory chip + affordance rows —
/// the plain-tab treatment, also reused for the Technical facts at the
/// bottom of the folded Settings front door.
fn fact_section(
    section: &RichSection<CardRowAction>,
    menu_rows: bool,
    on_action: EventHandler<UiAction>,
    card_key: &str,
) -> Element {
    rsx! {
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
        for row in section.affordances.iter() {
            div {
                {row_button(
                    row,
                    if menu_rows { ActionButtonVariant::MenuItem } else { ActionButtonVariant::Quiet },
                    on_action,
                    card_key,
                )}
            }
        }
    }
}

/// One affordance row: dispatch rows fire `on_action`; sheet rows open
/// their card sheet, tab rows select their tab — both now dispatched
/// through core (`HomeOp::CardUi`), keyed by the card's identity. The
/// display action carries only the button's label/icon chrome.
fn row_button(
    row: &CardRowAction,
    variant: ActionButtonVariant,
    on_action: EventHandler<UiAction>,
    card_key: &str,
) -> Element {
    match row {
        CardRowAction::Dispatch(action) => rsx! {
            ActionButton { action: action.clone(), running: false, variant, on_action }
        },
        CardRowAction::Sheet(kind, display) => {
            let kind = kind.clone();
            let card_key = card_key.to_string();
            rsx! {
                ActionButton {
                    action: display.clone(),
                    running: false,
                    variant,
                    on_action: move |_| on_action.call(open_sheet_action(&card_key, kind.clone())),
                }
            }
        }
        CardRowAction::SelectTab(tab, display) => {
            let tab = *tab;
            let card_key = card_key.to_string();
            rsx! {
                ActionButton {
                    action: display.clone(),
                    running: false,
                    variant,
                    on_action: move |_| on_action.call(select_tab_action(&card_key, tab)),
                }
            }
        }
    }
}

/// Render the active card sheet (D41).
fn device_card_sheet_view(
    active: &DeviceCardSheet,
    card: &UiDeviceCard,
    card_key: &str,
    on_action: EventHandler<UiAction>,
) -> Element {
    let card_key = card_key.to_string();
    match active {
        DeviceCardSheet::Confirm(action) => rsx! {
            ConfirmSheet { action: action.clone(), card_key, on_action }
        },
        DeviceCardSheet::Name => rsx! {
            NameDeviceSheet { card_key, on_action }
        },
        DeviceCardSheet::Troubleshoot => rsx! {
            TroubleshootSheet {
                uid: card.uid.clone(),
                card_key,
                firmware_package: card.fw.as_ref().map(|fw| fw.package.clone()),
                on_action,
            }
        },
        DeviceCardSheet::BootloaderEntry(flow) => rsx! {
            BootloaderEntrySheet { flow: flow.clone(), card_key, on_action }
        },
    }
}

/// The same action without its confirmation gate — the confirm sheet WAS
/// the gate, so Confirm dispatches through [`ActionButton`]/directly
/// without the native `confirm()` firing on top.
fn strip_confirmation(action: UiAction) -> UiAction {
    let meta = lpa_studio_core::ActionMeta {
        confirmation: None,
        ..action.meta().clone()
    };
    action.with_meta(meta)
}

/// The troubleshooting sheet (M6): what to try when the ladder ended on
/// Not-responding. Instructions stay concrete and short (the MVP bar);
/// Reconnect re-runs the ladder, the recovery flash opens the chooser
/// without attaching.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TroubleshootSheet(
    uid: Option<String>,
    card_key: String,
    firmware_package: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let reconnect = reconnect_device_action(uid);
    let recovery_flash = flash_device_action(&card_key, false);
    let boot_safe_once = boot_safe_once_action(&card_key);
    // The firmware package names the chip it was built for ("fw-esp32c6"),
    // which is the only chip source available before the user reaches
    // bootloader mode — a device that will not boot cannot tell us its board.
    // `None` yields generic steps that say they are generic.
    let instructions = RecoveryInstructions::for_chip(firmware_package.as_deref());
    rsx! {
        CardSheet {
            on_dismiss: {
                let card_key = card_key.clone();
                move |_| on_action.call(close_sheet_action(&card_key))
            },
            CardSheetTitle { text: "Device not responding" }
            ul { class: "tw:m-0 tw:mb-3 tw:grid tw:list-disc tw:gap-1 tw:pl-4 tw:text-xs tw:leading-normal tw:text-muted-foreground",
                li { "Check the USB cable — charge-only cables never carry data." }
                li { "Unplug the device, plug it back in, then Reconnect." }
                li {
                    "If it was fine until you changed the project, the project may be "
                    "stopping it from starting — start it once in safe mode."
                }
            }
            div { class: "tw:mb-3 tw:rounded tw:border tw:border-line tw:p-2",
                div { class: "tw:mb-1 tw:text-xs tw:font-medium",
                    "Still stuck? Put {instructions.subject} into recovery mode:"
                }
                ol { class: "tw:m-0 tw:grid tw:list-decimal tw:gap-1 tw:pl-4 tw:text-xs tw:leading-normal tw:text-muted-foreground",
                    for step in instructions.steps.iter() {
                        li { "{step.text}" }
                    }
                }
                if instructions.is_generic {
                    div { class: "tw:mt-1 tw:text-xs tw:text-muted-foreground",
                        "These are the usual ESP32 steps — this board may differ."
                    }
                }
            }
            div { class: "tw:grid tw:justify-end tw:gap-2",
                CardSheetButton {
                    label: "Reconnect",
                    tone: SheetButtonTone::Primary,
                    onclick: {
                        let card_key = card_key.clone();
                        move |_| {
                            on_action.call(close_sheet_action(&card_key));
                            on_action.call(reconnect.clone());
                        }
                    },
                }
                CardSheetButton {
                    label: "Start in safe mode",
                    tone: SheetButtonTone::Quiet,
                    onclick: {
                        let card_key = card_key.clone();
                        move |_| {
                            on_action.call(close_sheet_action(&card_key));
                            on_action.call(boot_safe_once.clone());
                        }
                    },
                }
                CardSheetButton {
                    label: "Flash firmware…",
                    tone: SheetButtonTone::Quiet,
                    onclick: {
                        let card_key = card_key.clone();
                        move |_| {
                            on_action.call(close_sheet_action(&card_key));
                            on_action.call(recovery_flash.clone());
                        }
                    },
                }
                CardSheetButton {
                    label: "Close",
                    tone: SheetButtonTone::Quiet,
                    onclick: move |_| on_action.call(close_sheet_action(&card_key)),
                }
            }
        }
    }
}

/// The bootloader-entry sheet (M5): the ritual, and — the whole point —
/// the confirmation that it worked.
///
/// Without feedback a failed attempt and a dead board look identical, so
/// people repeat the wrong motion and conclude the device is bricked. The
/// `Confirmed` arm is what makes the ritual learnable.
///
/// Advancing is re-opening the sheet with the next flow value, so the
/// renderer holds no state of its own.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BootloaderEntrySheet(
    flow: BootloaderEntryFlow,
    card_key: String,
    on_action: EventHandler<UiAction>,
) -> Element {
    // "I've done that" must PROBE, not merely change state. Advancing the
    // sheet without asking the device would show a waiting spinner that
    // never resolves — worse than nothing, because the user still cannot
    // tell a failed attempt from a dead board. The probe reboots the
    // device, and the button press is exactly the signal that a replug just
    // happened, so this is the one honest place to run it.
    let probe = |card_key: &str, flow: BootloaderEntryFlow| {
        UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::ProbeBootloaderMode {
                card_key: card_key.to_string(),
                flow,
            },
        )
        .with_label("Check the device")
    };
    rsx! {
        CardSheet {
            on_dismiss: {
                let card_key = card_key.clone();
                move |_| on_action.call(close_sheet_action(&card_key))
            },
            match &flow {
                BootloaderEntryFlow::Confirmed { chip_name } => {
                    let subject = chip_name.clone().unwrap_or_else(|| "The device".to_string());
                    rsx! {
                        CardSheetTitle { text: "Recovery mode — ready" }
                        p { class: "tw:m-0 tw:mb-3 tw:text-xs tw:leading-normal tw:text-muted-foreground",
                            "{subject} is listening. You can flash firmware, back it up, or "
                            "have it start once without its project."
                        }
                    }
                }
                _ => {
                    let instructions = flow.instructions().expect("non-confirmed states carry steps");
                    let waiting = flow.should_probe_on_arrival();
                    rsx! {
                        CardSheetTitle { text: "Put {instructions.subject} into recovery mode" }
                        if matches!(flow, BootloaderEntryFlow::NotYet { .. }) {
                            p { class: "tw:m-0 tw:mb-2 tw:text-xs tw:leading-normal tw:text-status-attention-foreground",
                                "That attempt didn't land — the device answered as if it were "
                                "running normally. Worth another go; the timing is fiddly."
                            }
                        }
                        ol { class: "tw:m-0 tw:mb-3 tw:grid tw:list-decimal tw:gap-1 tw:pl-4 tw:text-xs tw:leading-normal tw:text-muted-foreground",
                            for step in instructions.steps.iter() {
                                li { "{step.text}" }
                            }
                        }
                        if instructions.is_generic {
                            p { class: "tw:m-0 tw:mb-3 tw:text-xs tw:text-muted-foreground",
                                "These are the usual ESP32 steps — this board may differ."
                            }
                        }
                        if waiting {
                            p { class: "tw:m-0 tw:mb-3 tw:text-xs tw:leading-normal",
                                "Waiting for the device to reappear…"
                            }
                        }
                    }
                }
            }
            div { class: "tw:grid tw:justify-end tw:gap-2",
                if !flow.is_confirmed() && !flow.should_probe_on_arrival() {
                    CardSheetButton {
                        label: "I've done that",
                        tone: SheetButtonTone::Primary,
                        onclick: {
                            let card_key = card_key.clone();
                            let flow = flow.clone();
                            move |_| on_action.call(probe(&card_key, flow.clone()))
                        },
                    }
                }
                CardSheetButton {
                    label: "Close",
                    tone: SheetButtonTone::Quiet,
                    onclick: move |_| on_action.call(close_sheet_action(&card_key)),
                }
            }
        }
    }
}

/// The destructive-confirm sheet: copy from the action's own
/// [`lpa_studio_core::ActionConfirmation`] (title · message · verb);
/// Confirm dispatches the action with the gate stripped (the sheet WAS
/// the gate).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ConfirmSheet(action: UiAction, card_key: String, on_action: EventHandler<UiAction>) -> Element {
    let meta = action.meta().clone();
    let confirmation = meta.confirmation.clone().unwrap_or_else(|| {
        // defensive: a confirm sheet over an unconfirmed action still
        // reads sensibly (label as title, summary as body)
        lpa_studio_core::ActionConfirmation::new(
            meta.label.clone(),
            meta.summary.clone(),
            meta.label.clone(),
        )
    });
    let tone = if meta.destructive {
        SheetButtonTone::Destructive
    } else {
        SheetButtonTone::Primary
    };
    let confirmed = strip_confirmation(action);
    rsx! {
        CardSheet {
            on_dismiss: {
                let card_key = card_key.clone();
                move |_| on_action.call(close_sheet_action(&card_key))
            },
            CardSheetTitle { text: confirmation.title.clone() }
            CardSheetMessage { text: confirmation.message.clone() }
            CardSheetButtons {
                CardSheetButton {
                    label: "Cancel",
                    tone: SheetButtonTone::Quiet,
                    onclick: {
                        let card_key = card_key.clone();
                        move |_| on_action.call(close_sheet_action(&card_key))
                    },
                }
                CardSheetButton {
                    label: confirmation.confirm_label.clone(),
                    tone,
                    onclick: move |_| {
                        on_action.call(close_sheet_action(&card_key));
                        on_action.call(confirmed.clone());
                    },
                }
            }
        }
    }
}

/// The flashed-but-unstamped board's inline naming (NeedsAName): the
/// SETUP FORM's naming half, reused — same voice ("Name your device"),
/// same derived date + board/chip default, one primary verb, no dialog.
/// The discriminator prefers what the board itself reports (`board_id`
/// from the hello — the setup form's pick was stamped into
/// /hardware.json), then the detected chip, then the bare fallback.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NameForm(
    card_key: String,
    board_id: Option<String>,
    detected_chip: Option<String>,
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let mut typed_name = use_signal(|| None::<String>);
    let discriminator = board_id
        .as_deref()
        .map(|id| board_slug(id).to_string())
        .or_else(|| {
            detected_chip
                .as_deref()
                .and_then(|chip| chip_id(chip).map(str::to_string))
        })
        .unwrap_or_else(|| "lightplayer".to_string());
    let derived = default_setup_name(now_secs, &discriminator);
    let name_value = typed_name().unwrap_or(derived);
    let submit_name = name_value.clone();
    let name_card_key = card_key.clone();
    rsx! {
        div { class: "tw:mt-1 tw:grid tw:gap-3.5",
            div {
                label { class: "tw:mb-1 tw:block tw:text-sm tw:font-bold tw:text-strong-foreground",
                    "Name your device"
                }
                input {
                    class: "tw:w-full tw:rounded-md tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1.5 tw:font-mono tw:text-sm tw:text-strong-foreground tw:outline-none",
                    value: "{name_value}",
                    oninput: move |event| typed_name.set(Some(event.value())),
                }
            }
            CardSheetButton {
                label: "Name device",
                tone: SheetButtonTone::Primary,
                onclick: move |_| {
                    let value = submit_name.trim().to_string();
                    if !value.is_empty() {
                        on_action.call(home_action(HomeOp::NameDevice {
                            target: DeviceTarget::card(&name_card_key),
                            name: value,
                        }));
                    }
                },
            }
        }
    }
}

/// The name-stamping sheet (retained for the title-bar naming path;
/// the NeedsAName FACE now names inline via [`NameForm`]): input +
/// Enter-to-save; naming stamps the uid and the card returns to the front door.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NameDeviceSheet(card_key: String, on_action: EventHandler<UiAction>) -> Element {
    let mut name = use_signal(String::new);
    rsx! {
        CardSheet {
            on_dismiss: {
                let card_key = card_key.clone();
                move |_| on_action.call(close_sheet_action(&card_key))
            },
            CardSheetTitle { text: "Name this device" }
            CardSheetMessage { text: "Studio remembers it by name; the name is stamped onto the board." }
            form {
                onsubmit: {
                    let card_key = card_key.clone();
                    move |event: FormEvent| {
                        event.prevent_default();
                        save_device_name(name, &card_key, on_action);
                    }
                },
                input {
                    class: "tw:mb-3 tw:w-full tw:rounded-md tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-1.5 tw:text-sm tw:text-strong-foreground tw:outline-none",
                    autofocus: true,
                    placeholder: "e.g. Porch sign",
                    value: "{name}",
                    oninput: move |event| name.set(event.value()),
                    onkeydown: {
                        let card_key = card_key.clone();
                        move |event: KeyboardEvent| {
                            if event.key() == Key::Escape {
                                on_action.call(close_sheet_action(&card_key));
                            }
                        }
                    },
                }
                CardSheetButtons {
                    CardSheetButton {
                        label: "Cancel",
                        tone: SheetButtonTone::Quiet,
                        onclick: {
                            let card_key = card_key.clone();
                            move |_| on_action.call(close_sheet_action(&card_key))
                        },
                    }
                    CardSheetButton {
                        label: "Name device",
                        tone: SheetButtonTone::Primary,
                        onclick: {
                            let card_key = card_key.clone();
                            move |_| save_device_name(name, &card_key, on_action)
                        },
                    }
                }
            }
        }
    }
}

/// The name sheet's save: a non-empty name dispatches the stamp op and
/// closes the sheet. Naming mints the device's uid, so its identity (and
/// thus its `CardUiState` key) changes — the newly-stamped card comes up
/// fresh on the front door; no explicit tab reset needed.
fn save_device_name(name: Signal<String>, card_key: &str, on_action: EventHandler<UiAction>) {
    let value = name.read().trim().to_string();
    if value.is_empty() {
        return;
    }
    on_action.call(home_action(HomeOp::NameDevice {
        target: DeviceTarget::card(card_key),
        name: value,
    }));
    on_action.call(close_sheet_action(card_key));
}

/// The tab's icon (icon tabs at card scale; labels arrive in pane mode).
fn tab_icon(tab: DeviceCardTab) -> StudioIconName {
    match tab {
        // ▶ goes to the tab that actually plays something. Details wears
        // the info glyph (G1b rename — the front door is what we KNOW
        // about the device, not its settings).
        DeviceCardTab::Play => StudioIconName::Play,
        DeviceCardTab::Project => StudioIconName::NodeKind(NodeKindIcon::Module),
        DeviceCardTab::Details => StudioIconName::Info,
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

/// The two dashed entry cards, stacked in ONE grid cell: **connect a
/// device** and **simulate a device**, each half height (flow design F5b).
/// Both open the setup wizard — the first on a board at the end of a
/// cable, the second on THE simulator, which takes on a real board and
/// needs no hardware. Copy comes from each op's own metadata so the cards
/// and any toolbar chip never drift.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ConnectDeviceCard(on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        div { class: "tw:grid tw:grid-rows-2 tw:gap-3.5",
            SetupEntryCard { sim: false, on_action }
            SetupEntryCard { sim: true, on_action }
        }
    }
}

/// One entry card. The sim wears the violet (bound) family the studio
/// reserves for "this is the simulator", never the accent green.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SetupEntryCard(sim: bool, on_action: EventHandler<UiAction>) -> Element {
    let action = start_setup_action(sim);
    let meta = action.meta().clone();
    let class = if sim {
        "tw:grid tw:min-h-24 tw:cursor-pointer tw:place-items-center tw:gap-1 tw:rounded-md tw:border tw:border-dashed tw:border-[var(--studio-status-bound-border)] tw:bg-transparent tw:p-3 tw:text-muted-foreground tw:transition-colors tw:hover:border-[var(--studio-status-bound-text)] tw:hover:text-strong-foreground"
    } else {
        "tw:grid tw:min-h-24 tw:cursor-pointer tw:place-items-center tw:gap-1 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:bg-transparent tw:p-3 tw:text-muted-foreground tw:transition-colors tw:hover:border-dim-foreground tw:hover:text-strong-foreground ux-focus-ring"
    };
    rsx! {
        button {
            class,
            r#type: "button",
            title: "{meta.summary}",
            onclick: move |_| on_action.call(action.clone()),
            span { class: "tw:inline-flex tw:items-center tw:gap-2",
                if sim {
                    StudioIcon { name: StudioIconName::Simulator, size: 16 }
                } else {
                    StudioIcon { name: StudioIconName::Usb, size: 16 }
                }
                span { class: "tw:text-sm tw:font-semibold", "{meta.label}" }
            }
            span { class: "tw:text-center tw:text-xs tw:text-dim-foreground", "{meta.summary}" }
        }
    }
}

/// Open the setup wizard on a target — the entry cards' action.
pub(crate) fn start_setup_action(sim: bool) -> UiAction {
    home_action(HomeOp::StartSetup { sim })
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

// `connect_device_action` — the bare "open the VID-filtered chooser"
// action — is GONE (P06). Connecting is now the first step of the setup
// flow rather than a gesture of its own: the wizard's CONNECT_INTRO asks
// for the port through the machine's `RequestPort`, so there is exactly
// one place a port grant can start and exactly one thing that knows what
// to do with the board on the other end.

/// "Flash firmware…", the quiet secondary affordance (D33 demotion,
/// dialog-free since M8′): with a device connected it runs the flash
/// directly (the card's Operation-in-flight lane narrates; the
/// confirmation renders as the D41 sheet); with nothing connected it
/// opens the recovery chooser (link-only open, no app attach), after
/// which the card's own state carries the flow.
/// Write the boot-control record so the device's next restart skips its
/// project. Non-destructive and one-shot — see `DeviceOp::BootSafeOnce`.
pub(crate) fn boot_safe_once_action(card_key: &str) -> UiAction {
    UiAction::from_op(
        ControllerId::new(DeviceController::NODE_ID),
        DeviceOp::BootSafeOnce {
            target: DeviceTarget::card(card_key),
        },
    )
    .with_label("Start in safe mode")
}

/// Read the device's storage over the bootloader and download it as a ZIP.
///
/// No confirmation: nothing is written. It is deliberately the row people
/// meet BEFORE the destructive verbs, because it is what makes them
/// survivable — see `DeviceOp::BackUpFilesystem`.
pub(crate) fn back_up_filesystem_action(card_key: &str) -> UiAction {
    UiAction::from_op(
        ControllerId::new(DeviceController::NODE_ID),
        DeviceOp::BackUpFilesystem {
            target: DeviceTarget::card(card_key),
        },
    )
    .with_label("Download a backup")
    .with_summary(
        "Save everything on this device to a ZIP on your computer — this \
         works even if the board will not start.",
    )
    .with_icon("download")
}

pub(crate) fn flash_device_action(card_key: &str, device_connected: bool) -> UiAction {
    let action = if device_connected {
        UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::ProvisionFirmware {
                target: DeviceTarget::card(card_key),
                setup_name: None,
                board_id: None,
            },
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

/// The in-card push op for one library project (M5's lane; M8′ reuses
/// it for the picker rows and the drop-confirm sheet).
fn push_project_action(card_key: &str, key: String) -> UiAction {
    UiAction::from_op(
        ControllerId::new(DEPLOY_NODE_ID),
        DeployOp::PushProject {
            key,
            target: DeviceTarget::card(card_key),
        },
    )
    .with_label("Push")
    .with_summary("Push this project to the device.")
    .with_icon("upload")
}

/// The Project-tab PICKER (M8′, contract §3): the library projects as
/// menu rows — one click pushes (the click is the D11 consent, exactly
/// like the in-card Push). Rendered only on the Connected-empty card.
fn project_picker_body(
    card_key: &str,
    choices: &[UiDeviceProjectChip],
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:text-xs tw:text-subtle-foreground",
            "Put a project on this device:"
        }
        div { class: "tw:grid",
            for chip in choices.iter() {
                {
                    let action = push_project_action(card_key, chip.uid.clone())
                        .with_label(chip.name.clone())
                        .with_summary(format!("Push {} to this device.", chip.name))
                        .with_icon("play");
                    rsx! {
                        ActionButton {
                            action,
                            running: false,
                            variant: ActionButtonVariant::MenuItem,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// The D29 editor entry, now dispatched from the grow control (⤢): move
/// the editor lens onto the device session and open its running project.
/// The card targets the attached session (`uid: None`); the
/// `/device/<uid>` route passes the uid.
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
pub(crate) fn stop_simulator_action() -> UiAction {
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

/// The ≤1 affordance, fully card-native (M8′ — the deploy dialog is
/// gone): Push runs the in-card push (M5, the button is the D11
/// consent); Set-up and Update run the ONE flash effect
/// (`DeviceOp::ProvisionFirmware` — the card's Operation-in-flight lane
/// narrates, the confirmation renders as the D41 sheet); sheet-homed
/// flows (drift, name, troubleshoot) and the Project-tab picker are
/// intercepted in `wire_card_affordance` before this runs. The editor
/// identity renders no button — the grow control is the editor entry.
pub(super) fn device_affordance_action(
    card: &UiDeviceCard,
    affordance: &RosterAffordance,
) -> Option<UiAction> {
    // Every device verb below acts on THIS card's board (M4).
    let card_key = card.identity_key();
    let action = match affordance {
        // the grow control (⤢) is the editor entry — no row
        // The visible editor CTA on the running card's face (2026-07-26
        // walk) — same op as the grow ⤢, said out loud.
        RosterAffordance::OpenEditor => open_device_project_action()
            .with_summary("Open this device's project in the editor.")
            .with_icon("grow"),
        // The in-card push (M5): the button IS the D11 consent — the push
        // dispatches directly and its progress folds into the card's
        // Operation-in-flight state. A missing chip means no honest push
        // target — no row (RunningBehind derives from Known content, so
        // it never should be missing).
        RosterAffordance::PushVersion { .. } => {
            let chip = card.project.as_ref()?;
            UiAction::from_op(
                ControllerId::new(DEPLOY_NODE_ID),
                DeployOp::PushProject {
                    key: chip.uid.clone(),
                    target: DeviceTarget::card(card_key),
                },
            )
            .with_summary("Push your newest version to this device.")
            .with_icon("upload")
        }
        // §3c-2: the diverged card's verbs live ON the face and dispatch
        // directly — both are non-destructive by construction (adopt keeps
        // the old head in history; keep-both forks), so neither needs a
        // gate. The copy SAYS so, which is what makes one click feel safe.
        RosterAffordance::UseBoardCopy => UiAction::from_op(
            ControllerId::new(DEPLOY_NODE_ID),
            DeployOp::AdoptDeviceCopy {
                target: DeviceTarget::card(card_key),
            },
        )
        .with_summary(
            "Make the board's copy your newest version — your local \
                     changes stay in your project history.",
        )
        .with_icon("download"),
        // P5: the format verb dispatches from the face like the drift
        // verbs, and for the same reason — it is non-destructive by
        // construction (the pre-upgrade copy stays in project history), so
        // a confirm gate would be theatre. The summary says where the old
        // copy went, which is what makes one click feel safe.
        RosterAffordance::UpgradeProject => UiAction::from_op(
            ControllerId::new(DEPLOY_NODE_ID),
            DeployOp::UpgradeDeviceProject {
                target: DeviceTarget::card(card_key),
            },
        )
        .with_summary(
            "Upgrade this board's project to the format Studio uses and put \
             it back on the board — the copy that's there now is already \
             saved in your library.",
        )
        .with_icon("upload"),
        RosterAffordance::KeepBoth => UiAction::from_op(
            ControllerId::new(DEPLOY_NODE_ID),
            DeployOp::KeepBothFork {
                target: DeviceTarget::card(card_key),
            },
        )
        .with_summary("Save the board's copy as its own project.")
        .with_icon("copy"),
        // intercepted upstream: the troubleshoot sheet / the Project-tab
        // picker / the wipe confirm (`wire_card_affordance`)
        RosterAffordance::Troubleshoot
        | RosterAffordance::ChooseProject
        | RosterAffordance::WipeProject => return None,
        // M8′ + device-lifecycle P2: provisioning IS the flash, worn per
        // context — and it is ONE CLICK (no confirm gate). A blank board
        // has nothing to lose; a firmware update warns via its summary
        // ("the device restarts after") rather than a modal. The op takes
        // over the card in place (the P2 overlay), so the click is honest
        // without a dialog. Destructive re-flash/erase keep their confirm.
        RosterAffordance::SetUp => UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::ProvisionFirmware {
                target: DeviceTarget::card(card_key),
                setup_name: None,
                board_id: None,
            },
        )
        .with_summary(
            "Install LightPlayer firmware on this board — about a minute; \
             the card shows progress and you can walk away.",
        )
        .with_icon("zap"),
        RosterAffordance::UpdateFirmware => UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::ProvisionFirmware {
                target: DeviceTarget::card(card_key),
                setup_name: None,
                board_id: None,
            },
        )
        .with_summary(
            "Install this build's firmware over the device's older build — \
             the card shows progress; the device restarts after.",
        )
        .with_icon("zap"),
        // rendered as the card's naming flows, not an action button
        RosterAffordance::NameDevice => return None,
        RosterAffordance::Reconnect => reconnect_device_action(card.uid.clone())
            .with_summary("Reconnect over the granted serial port.")
            .with_icon("usb"),
    };
    Some(action.with_label(affordance.label()))
}

/// Map one device section's affordance identities onto card ROWS (M7′
/// P2): confirm-carrying actions and the name/drift flows become sheet
/// rows (D41); everything else dispatches. (`wire_section` below keeps
/// the popover-era `UiAction` wiring compiling until P3 deletes the
/// popover.)
fn wire_card_section(
    card: &UiDeviceCard,
    section: RichSection<DeviceDetailAffordance>,
) -> RichSection<CardRowAction> {
    RichSection {
        title: section.title,
        tone: section.tone,
        lines: section.lines,
        chip: section.chip,
        affordances: section
            .affordances
            .iter()
            .filter_map(|affordance| wire_card_affordance(card, affordance))
            .collect(),
        weight: section.weight,
    }
}

fn wire_card_affordance(
    card: &UiDeviceCard,
    affordance: &DeviceDetailAffordance,
) -> Option<CardRowAction> {
    match affordance {
        // M8′: "Choose a project" jumps to the Project-tab picker (the
        // list of library projects → push) — no popup, no dialog.
        DeviceDetailAffordance::Roster(RosterAffordance::ChooseProject) => {
            let display = home_action(HomeOp::OpenPackage { key: String::new() })
                .with_label(RosterAffordance::ChooseProject.label())
                .with_summary("Choose a project to put on this device.")
                .with_icon("play");
            Some(CardRowAction::SelectTab(DeviceCardTab::Project, display))
        }
        // The name-stamping sheet (an unstamped board's one naming flow).
        DeviceDetailAffordance::Roster(RosterAffordance::NameDevice) => {
            let display = home_action(HomeOp::NameDevice {
                target: DeviceTarget::card(card.identity_key()),
                name: String::new(),
            })
            .with_label("Name this device…")
            .with_summary("Stamp a name onto this board so Studio remembers it.")
            .with_icon("edit");
            Some(CardRowAction::Sheet(CardSheetState::Name, display))
        }
        // Opens the troubleshooting sheet. The action here is meta-only —
        // it supplies the row's label/summary/icon and is never dispatched;
        // the sheet is the effect.
        //
        // `strip_confirmation` is load-bearing, not tidiness. This row
        // borrowed ProvisionFirmware purely as a carrier, and inherited its
        // DESTRUCTIVE confirmation with it — so opening a read-only help
        // sheet asked "This will write LightPlayer firmware to the selected
        // ESP32. Continue?" (reported from the bench 2026-07-31; latent
        // since M6, and exposed once Troubleshoot became reachable from
        // every state). A meta-only carrier must never inherit a gate for
        // an effect it does not have.
        DeviceDetailAffordance::Roster(RosterAffordance::Troubleshoot) => {
            let display = strip_confirmation(
                UiAction::from_op(
                    ControllerId::new(DeviceController::NODE_ID),
                    DeviceOp::ProvisionFirmware {
                        target: DeviceTarget::card(card.identity_key()),
                        setup_name: None,
                        board_id: None,
                    },
                )
                .with_label(RosterAffordance::Troubleshoot.label())
                .with_summary("Steps to try when the device is not responding.")
                .with_icon("zap"),
            );
            Some(CardRowAction::Sheet(CardSheetState::Troubleshoot, display))
        }
        // The unreadable card's wipe: destructive → the D41 confirm sheet
        // gates it; the verb rides core state (model rev 2026-07-26).
        DeviceDetailAffordance::Roster(RosterAffordance::WipeProject) => {
            let action = UiAction::from_op(
                ControllerId::new(DeviceController::NODE_ID),
                DeviceOp::WipeProject {
                    target: DeviceTarget::card(card.identity_key()),
                },
            )
            .with_label(RosterAffordance::WipeProject.label())
            .with_summary("Remove the project from this device — back to blank.")
            .with_icon("revert");
            Some(CardRowAction::Sheet(
                CardSheetState::Confirm(CardVerb::WipeProject),
                strip_confirmation(action),
            ))
        }
        // The firmware UPDATE keeps its gate, but as the card's own sheet
        // — every other confirmed verb on this card does (D41), and the
        // generic ActionButton path put a BROWSER confirm() in the middle
        // of an otherwise card-resident flow (gate-1 sitting, 2026-08-03:
        // "I got a browser confirm, which is weird").
        DeviceDetailAffordance::Roster(affordance @ RosterAffordance::UpdateFirmware) => {
            device_affordance_action(card, affordance).map(|action| {
                CardRowAction::Sheet(
                    CardSheetState::Confirm(CardVerb::Flash),
                    strip_confirmation(action),
                )
            })
        }
        DeviceDetailAffordance::Roster(affordance) => {
            device_affordance_action(card, affordance).map(CardRowAction::from_action)
        }
        // The destructive reflash of an already-provisioned board keeps its
        // D41 confirm; the verb rides core state, its display is the same
        // action with the gate stripped (the sheet IS the gate).
        DeviceDetailAffordance::FlashFirmware => Some(CardRowAction::Sheet(
            CardSheetState::Confirm(CardVerb::Flash),
            strip_confirmation(flash_device_action_destructive(card.identity_key())),
        )),
        // Non-destructive, so it dispatches straight from the row — a
        // confirm gate on "save a copy of your work" would be theatre.
        DeviceDetailAffordance::BackUpFilesystem => Some(CardRowAction::from_action(
            back_up_filesystem_action(card.identity_key()),
        )),
        DeviceDetailAffordance::EraseDevice => Some(CardRowAction::Sheet(
            CardSheetState::Confirm(CardVerb::Erase),
            strip_confirmation(erase_device_action(card.identity_key(), card.name.clone())),
        )),
        DeviceDetailAffordance::ForgetDevice => card.uid.clone().map(|uid| {
            CardRowAction::Sheet(
                CardSheetState::Confirm(CardVerb::Forget),
                strip_confirmation(forget_device_action(uid, card.name.clone())),
            )
        }),
        // Non-destructive (the board keeps running; reconnecting adds it
        // back), so it dispatches straight from the row — no confirm
        // theatre. Only live cards carry a session key to target.
        DeviceDetailAffordance::DisconnectDevice => card
            .session_key
            .clone()
            .map(|session_key| CardRowAction::from_action(disconnect_device_action(&session_key))),
    }
}

/// The Danger tab's disconnect row (multi-device M3): close THIS board's
/// session, targeted by the card's session key so a second attached board
/// is untouched.
pub(crate) fn disconnect_device_action(card_key: &str) -> UiAction {
    UiAction::from_op(
        ControllerId::new(DeviceController::NODE_ID),
        DeviceOp::DisconnectDevice {
            target: DeviceTarget::card(card_key),
        },
    )
    .with_label("Disconnect")
    .with_summary(
        "Close this board's session and remove its card. The board keeps \
         running; connecting it again adds it back.",
    )
    .with_icon("remove")
}

/// Map one sim section's affordance identities onto card rows (the
/// stop-simulator confirm rides the D41 sheet).
fn wire_sim_card_section(section: RichSection<SimDetailAffordance>) -> RichSection<CardRowAction> {
    RichSection {
        title: section.title,
        tone: section.tone,
        lines: section.lines,
        chip: section.chip,
        affordances: section
            .affordances
            .iter()
            .map(|affordance| match affordance {
                SimDetailAffordance::OpenEditor => CardRowAction::Dispatch(
                    open_sim_project_action()
                        .with_label("Open in editor")
                        .with_summary("Open the loaded project in the editor.")
                        .with_icon("grow"),
                ),
                SimDetailAffordance::StopSimulator => CardRowAction::Sheet(
                    CardSheetState::Confirm(CardVerb::StopSim),
                    strip_confirmation(stop_simulator_action()),
                ),
            })
            .collect(),
        weight: section.weight,
    }
}

/// The Danger tab's flash row: [`flash_device_action`] with a live
/// device context, wearing the destructive treatment the tab's rows
/// share (the inline red zone reads uniformly red).
pub(super) fn flash_device_action_destructive(card_key: &str) -> UiAction {
    let action = flash_device_action(card_key, true);
    let meta = action.meta().clone().destructive();
    action.with_meta(meta)
}

/// Erase the device's flash entirely, from the Danger tab. Confirmation
/// states the honest facts: full wipe; anything Studio could read was
/// banked at connect (D8) — unreadable content is gone for good.
///
/// What it no longer costs is the BOARD (device identity design §6): an
/// erase takes the projects, not the identity, because the identity is
/// the chip's own factory MAC. The board comes back as itself, under its
/// own name, with its history intact.
pub(crate) fn erase_device_action(card_key: &str, name: String) -> UiAction {
    UiAction::from_op(
        ControllerId::new(DEPLOY_NODE_ID),
        DeployOp::EraseDevice {
            target: DeviceTarget::card(card_key),
        },
    )
    .with_confirmation(lpa_studio_core::ActionConfirmation::new(
        "Erase device",
        format!(
            "Erase everything on \"{name}\"? Its flash is wiped clean; \
                 anything Studio could read was already saved to your library. \
                 The board itself stays remembered — its identity is in its silicon."
        ),
        "Erase",
    ))
}

/// The forget action (D34 hygiene) for the offline card's Danger tab.
pub(crate) fn forget_device_action(uid: String, name: String) -> UiAction {
    home_action(HomeOp::ForgetDevice { uid }).with_confirmation(
        lpa_studio_core::ActionConfirmation::new(
            "Forget device",
            format!("Forget \"{name}\"? Connecting it again adds it back."),
            "Forget",
        ),
    )
}

/// The card's chrome: the tint edge class per the shape grammar plus the
/// offline whole-card fade; pane mode (D43) makes the card a tall flex
/// column. Body clicks are quiet (no pointer cursor — the interactive
/// surfaces carry their own).
fn device_card_class(faded: bool, treatment: RosterTreatment, pane: bool) -> String {
    let edge = match treatment {
        RosterTreatment::Filled => "ux-device-edge",
        RosterTreatment::Remembered => "ux-device-edge ux-device-edge-remembered",
        RosterTreatment::Working => "ux-device-edge ux-device-edge-working",
    };
    let fade = if faded {
        " tw:opacity-70 tw:transition-opacity tw:hover:opacity-100"
    } else {
        ""
    };
    let grown = if pane {
        " tw:flex tw:min-h-[560px] tw:flex-col"
    } else {
        ""
    };
    format!(
        // tw:group anchors the pencil's hover reveal
        "tw:group tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card {edge}{fade}{grown}"
    )
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

#[cfg(test)]
mod setup_name_tests {
    use super::{board_slug, default_setup_name};

    /// Sortable date first (gate round 2), then the most specific thing
    /// known about the device.
    #[test]
    fn a_fixed_story_clock_derives_a_deterministic_utc_name() {
        assert_eq!(
            default_setup_name(Some(1_800_000_000.0), "esp32c6"),
            "2027-01-15 - esp32c6"
        );
        assert_eq!(
            default_setup_name(Some(1_800_000_000.0), "xiao-esp32-c6"),
            "2027-01-15 - xiao-esp32-c6"
        );
    }

    /// A board contributes its PRODUCT half — the vendor is noise in a
    /// device name.
    #[test]
    fn board_slug_drops_the_vendor() {
        assert_eq!(board_slug("seeed/xiao-esp32-c6"), "xiao-esp32-c6");
        assert_eq!(board_slug("dom-z-102"), "dom-z-102");
    }
}

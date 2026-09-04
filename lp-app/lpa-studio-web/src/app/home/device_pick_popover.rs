//! The device card's two pickers, as POPOVERS (D6, AC4, AC8).
//!
//! # Why a popover and not an inline list
//!
//! Both picks used to render their options inside the card. That made the
//! card's height a function of the LIBRARY: one example and three projects
//! was a short card, forty projects was a card taller than the viewport, and
//! installing a project silently re-laid out the whole roster. The reflow
//! rule (AC2 — a board event never changes a card's height, and only a user
//! action may) cannot survive a control that grows with data, so the options
//! moved into a panel that floats in the browser's top layer:
//! [`PopoverButton`] anchors it, and the card only ever holds the 30px
//! trigger. The verb row is `overflow-hidden` at exactly 30px, which is also
//! why the panel may never be an in-flow sibling — it would be clipped to
//! nothing.
//!
//! # The three sources, and who decides
//!
//! The gallery popover offers exactly what
//! [`push_offer`](lpa_studio_core::push_offer) grouped: **Examples** (the
//! bundled gallery), **My projects** (the library), and **New** (a starter
//! generated for this board — or the honest reason there is none). Core owns
//! the grouping, the preselect and the copy; this file only lays them out
//! and dispatches the op the offer handed back. The board popover is the
//! same shape over [`flash_offer`](lpa_studio_core::flash_offer).
//!
//! A pick is **ephemeral UI state** — a `use_signal` holding one key. It is
//! journaled by nothing: the decision reaches the model as a parameter on the
//! Push (or Flash) action the CTA dispatches, which is the card ruling ("no
//! wizard state anywhere"). A pick the offer no longer contains (the library
//! changed under it, or the chip filter was re-applied) falls back to the
//! offer's own preselect rather than being dispatched — the stale-pick guard.
//!
//! # The chip filter, stated and escapable (AC4)
//!
//! The board list is filtered to the JOINED chip
//! ([`device_chip`](lpa_studio_core::device_chip)) whenever either source
//! knows it — the ROM boot banner, or the board id its firmware's hello
//! carried. The panel says which source answered and how many boards fit,
//! and offers "show all" as the escape; in show-all mode it says that the
//! flash preflight's chip guard is what makes a wrong pick fail safely, and
//! offers the way back. The filter is convenience; the guard is the safety.
//!
//! # The board's own picture (P10)
//!
//! A board tile used to be three lines of text, which asked the user to pick
//! hardware by reading part numbers — the one thing they cannot check against
//! the thing in their hand. `lpa-boards` already draws every catalog board
//! from its display sidecar (the boards page shows the same drawings full
//! size), so the tile leads with that drawing and the trigger's swatch
//! carries the picked board's. The rendering is the same one metadata source
//! and the same renderer: nothing here is hand-drawn or board-specific.
//!
//! Boards are portrait and differ in height by three to one, so the tiles ask
//! [`BoardDiagram`] to FIT a box rather than passing a scale — one multiplier
//! would make the S3 devkit three times the XIAO. They also ask for it TURNED:
//! a devkit standing upright in a 145px band is a 20px sliver in a field of
//! air, and on its side the same board fills the band, so the drawing is the
//! tile's face rather than a mark on it. The quinled pair are nearly square
//! and gain nothing from the turn; they are turned anyway, because one
//! orientation across the grid beats a few pixels on two tiles. Labels are
//! off: pin names at this size would be a grey smear (and would be sideways),
//! and the pick is between silhouettes.
//!
//! # Keyboard
//!
//! [`PopoverButton`] gives Escape-to-close, a dismissing backdrop, and focus
//! on the trigger. It does NOT give arrow-key roving between the cards in a
//! panel — noted as a follow-up, deliberately outside P6's scope.

use dioxus::prelude::*;
use lpa_boards::{BoardDiagram, DiagramMode};
use lpa_studio_core::{
    DeviceAction, DeviceId, DevicePushOp, DeviceView, DevicesOp, FlashBoardChoice, PreviewSource,
    PushOffer, PushSource, PushSourceChoice, PushSourceGroup, UiAction, UiExampleCard,
    UiPackageCard, device_chip, flash_offer, push_offer,
};

use super::card_thumb::{CardThumb, thumb_swatch_style};
use super::device_roster_card::{RowCta, RowCtaDisabled, row_note_class};
use super::thumb_poster::cached_poster;
use crate::base::{
    OPTION_CARD_CHECK_CLASS, PopoverButton, PopoverCloseHandle, PopoverPlacement, StudioIcon,
    StudioIconName,
};
use crate::core::quiet_action_class;

/// Where a board's chip fact came from — the panel says so, because "the
/// list was filtered" only earns trust when the user can see what it was
/// filtered ON.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChipSource {
    /// The ROM printed it at boot (a blank board, or one just reset).
    BootBanner,
    /// The board id its hello carried did — the board's own firmware.
    Firmware,
}

impl ChipSource {
    fn phrase(self) -> &'static str {
        match self {
            Self::BootBanner => "in the boot banner",
            Self::Firmware => "from its firmware",
        }
    }
}

/// The chip a card's board pick is filtered by, and which source answered.
///
/// Mirrors [`device_chip`]'s own priority: the boot banner first, then the
/// catalog family of the board id the hello carried.
pub(crate) fn joined_chip(card: &DeviceView) -> Option<(String, ChipSource)> {
    if let Some(chip) = card.detected_chip.clone() {
        return Some((chip, ChipSource::BootBanner));
    }
    device_chip(card).map(|chip| (chip, ChipSource::Firmware))
}

/// Which half of the gallery popover is showing. One tab per
/// [`PushSourceGroup`], and never a fourth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PickTab {
    Examples,
    Mine,
    New,
}

impl PickTab {
    fn label(self) -> &'static str {
        match self {
            Self::Examples => "Examples",
            Self::Mine => "My projects",
            Self::New => "New",
        }
    }

    fn group(self) -> PushSourceGroup {
        match self {
            Self::Examples => PushSourceGroup::Example,
            Self::Mine => PushSourceGroup::Library,
            Self::New => PushSourceGroup::New,
        }
    }

    fn for_group(group: PushSourceGroup) -> Self {
        match group {
            PushSourceGroup::Example => Self::Examples,
            PushSourceGroup::Library => Self::Mine,
            PushSourceGroup::New => Self::New,
        }
    }
}

/// The empty face's verb row: the gallery popover's trigger plus the one
/// primary verb, on the row's 30px line.
///
/// Renders BOTH halves because they share one pick — the trigger shows it,
/// the CTA dispatches it — and nothing is journaled in between.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ProjectPickPopover(
    card: DeviceView,
    projects: Vec<UiPackageCard>,
    examples: Vec<UiExampleCard>,
    /// Stories only: mount the panel open (capture cannot click).
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    // Every hook first: the "nothing to offer" row below is an early
    // return, and a hook behind one would shift the hook order the frame a
    // library appeared.
    let mut pick = use_signal(|| None::<String>);

    let device = card.id;
    let offer = push_offer(&card, &projects, &examples);

    // Nothing at all to offer: the row says why, in the words core chose.
    if let Some(unavailable) = offer.unavailable.clone() {
        return rsx! {
            p { class: row_note_class(), title: "{unavailable}", "{unavailable}" }
        };
    }

    // The stale-pick guard: a key the offer no longer carries (the library
    // changed while the card sat there) falls back to the offer's own
    // preselect rather than dispatching something that is gone.
    let selected_key = pick()
        .filter(|key| offer.choices.iter().any(|choice| &choice.key == key))
        .or_else(|| offer.preselect.clone());
    let chosen = selected_key
        .as_deref()
        .and_then(|key| offer.choices.iter().find(|choice| choice.key == key))
        .cloned();

    let trigger_title = chosen
        .as_ref()
        .map(|choice| choice.title.clone())
        .unwrap_or_else(|| format!("{} to choose from", offer.choices.len()));
    let trigger_tag = chosen
        .as_ref()
        .map(|choice| provenance_tag(choice.group).to_string())
        .unwrap_or_default();
    let trigger_poster = chosen.as_ref().and_then(choice_poster);
    let trigger_seed = chosen
        .as_ref()
        .map(|choice| choice.key.clone())
        .unwrap_or_else(|| "no-pick".to_string());
    let source = chosen.map(|choice| choice.source);

    rsx! {
        div { class: trigger_slot_class(),
            PopoverButton {
                class: pick_trigger_class().to_string(),
                open_class: pick_trigger_class().to_string(),
                trigger: rsx! {
                    ProjectSwatch { seed: trigger_seed, poster: trigger_poster }
                    span { class: trigger_label_class(), "{trigger_title}" }
                    if !trigger_tag.is_empty() {
                        span { class: trigger_tag_class(), "{trigger_tag}" }
                    }
                    span { class: trigger_caret_class(), "\u{25be}" }
                },
                label: "Choose what to put on this board".to_string(),
                title: "Choose what to put on this board".to_string(),
                popup_class: GALLERY_POPUP_CLASS.to_string(),
                chrome_class: "ux-popover-chrome-neutral".to_string(),
                placement: PopoverPlacement::BottomStart,
                layer_keeps_layout: true,
                initially_open,
                ProjectPickPanel {
                    offer,
                    selected: selected_key,
                    on_pick: move |key: String| pick.set(Some(key)),
                }
            }
        }
        match source {
            Some(source) => rsx! {
                RowCta { action: DevicePushOp::action_for(device, source.clone()), on_action }
            },
            // Several things to choose from and none preselected: the verb
            // waits rather than guessing which project the user meant.
            None => rsx! {
                RowCtaDisabled {
                    label: "Put it on the board".to_string(),
                    hint: "Pick what to put on the board first.".to_string(),
                }
            },
        }
    }
}

/// The gallery panel: three tabs with counts, a title filter, a card grid,
/// and the line saying these are the same cards the library pages show.
///
/// A component (rather than an inline fragment) so it has a scope of its own
/// to read the enclosing popover's [`PopoverCloseHandle`] from — picking is a
/// completed gesture, so it closes (the add-node picker's rule).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectPickPanel(
    offer: PushOffer,
    selected: Option<String>,
    on_pick: EventHandler<String>,
) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    // Open on the tab the current pick already lives in — the palette
    // chooser's rule ("the chooser opens on the tab its config already
    // is"). No pick means the gallery, which is where a first plug goes.
    let opening_tab = selected
        .as_deref()
        .and_then(|key| offer.choices.iter().find(|choice| choice.key == key))
        .map_or(PickTab::Examples, |choice| PickTab::for_group(choice.group));
    let mut tab = use_signal(|| opening_tab);
    let mut query = use_signal(String::new);

    let current = tab();
    let filter = query().trim().to_lowercase();
    let visible: Vec<PushSourceChoice> = offer
        .choices
        .iter()
        .filter(|choice| choice.group == current.group())
        .filter(|choice| filter.is_empty() || choice.title.to_lowercase().contains(&filter))
        .cloned()
        .collect();
    let counts: Vec<(PickTab, usize)> = [PickTab::Examples, PickTab::Mine, PickTab::New]
        .into_iter()
        .map(|candidate| {
            let count = offer
                .choices
                .iter()
                .filter(|choice| choice.group == candidate.group())
                .count();
            (candidate, count)
        })
        .collect();
    let new_unavailable = offer.new_project_unavailable.clone();

    rsx! {
        div { class: "tw:grid tw:min-w-0",
            div { class: panel_top_class(),
                div { class: "tw:flex tw:min-w-0 tw:gap-0.5",
                    for (candidate , count) in counts {
                        button {
                            key: "{candidate:?}",
                            class: tab_class(candidate == current),
                            r#type: "button",
                            onclick: move |event: MouseEvent| {
                                event.stop_propagation();
                                tab.set(candidate);
                            },
                            "{candidate.label()}"
                            span { class: tab_count_class(), "{count}" }
                        }
                    }
                }
                input {
                    class: panel_search_class(),
                    r#type: "search",
                    placeholder: "Search\u{2026}",
                    value: "{query()}",
                    oninput: move |event| query.set(event.value()),
                }
            }
            div { class: panel_body_class(),
                // The New tab is one card or one honest reason — never an
                // empty grid, which would read as a bug rather than as "this
                // board has not said which board it is".
                if visible.is_empty() {
                    p { class: panel_note_class(),
                        if current == PickTab::New {
                            {
                                new_unavailable
                                    .clone()
                                    .unwrap_or_else(|| "No starter fits this board.".to_string())
                            }
                        } else {
                            "Nothing here matches that."
                        }
                    }
                } else {
                    div { class: pick_grid_class(),
                        for choice in visible {
                            {
                                let picked = selected.as_deref() == Some(choice.key.as_str());
                                let key = choice.key.clone();
                                rsx! {
                                    PickCard {
                                        key: "{choice.key}",
                                        choice: choice.clone(),
                                        selected: picked,
                                        on_pick: move |_| {
                                            on_pick.call(key.clone());
                                            if let Some(mut close) = close {
                                                close.close();
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
            p { class: panel_foot_class(), "Same cards as the Explore and Projects pages." }
        }
    }
}

/// One thing the board can be given: its poster (or identity swatch), its
/// title, and where it came from. Selected wears the option-card grammar —
/// the static spectrum ring, the selection wash, and the check badge.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PickCard(choice: PushSourceChoice, selected: bool, on_pick: EventHandler<()>) -> Element {
    let poster = choice_poster(&choice);
    let is_new = choice.group == PushSourceGroup::New;
    let sub = format!("{} \u{b7} {}", provenance_tag(choice.group), choice.blurb);

    rsx! {
        button {
            class: pick_card_class(selected),
            r#type: "button",
            title: "{choice.blurb}",
            onclick: move |event: MouseEvent| {
                event.stop_propagation();
                on_pick.call(());
            },
            if selected {
                span { class: OPTION_CARD_CHECK_CLASS, aria_hidden: "true",
                    StudioIcon { name: StudioIconName::StepComplete, size: 10 }
                }
            }
            // The starter has no picture to show — it does not exist yet —
            // so it wears the dashed "make one" face rather than a thumb
            // that would be a lie.
            if is_new {
                span { class: pick_new_face_class(), aria_hidden: "true", "+" }
            } else {
                span { class: "tw:block tw:overflow-hidden tw:rounded-xs",
                    CardThumb {
                        seed: choice.key.clone(),
                        label: choice.title.clone(),
                        static_poster: poster,
                    }
                }
            }
            span { class: pick_card_title_class(), title: "{choice.title}", "{choice.title}" }
            span { class: pick_card_sub_class(), "{sub}" }
        }
    }
}

/// How the board pick is being asked for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoardPickMode {
    /// The needs-firmware verb row: a picker trigger that fills the row,
    /// with the Flash CTA beside it. Picking only updates the trigger.
    Row,
    /// The re-flash verb on a board that is already running (the P4
    /// amendment): the quiet "Flash firmware" chip IS the trigger, and
    /// picking a board flashes it straight away — the verb was already
    /// pressed, so the pick completes the gesture rather than arming a
    /// second one.
    Verb,
}

/// The board pick, as a popover (AC4).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BoardPickPopover(
    device: DeviceId,
    /// The joined chip and the source that answered — `None` when neither
    /// source knows it, which widens the offer to every served board.
    #[props(default = None)]
    chip: Option<(String, ChipSource)>,
    #[props(default = BoardPickMode::Row)] mode: BoardPickMode,
    /// Stories only: mount the panel open (capture cannot click).
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    // Hooks before the early return, for the same reason the gallery's are.
    let mut show_all = use_signal(|| false);
    let mut pick = use_signal(|| None::<String>);

    let chip_name = chip.as_ref().map(|(name, _)| name.clone());
    // Show-all is local UI state: the escape from the chip filter, never a
    // fact about the device.
    let filter_chip = match show_all() {
        true => None,
        false => chip_name.as_deref(),
    };
    let offer = flash_offer(filter_chip);

    if let Some(unavailable) = offer.unavailable.clone() {
        return rsx! {
            p { class: row_note_class(), title: "{unavailable}", "{unavailable}" }
        };
    }

    // The same stale-pick guard as the gallery: a board id the (possibly
    // re-filtered) offer no longer carries falls back to its preselect.
    let selected_id = pick()
        .filter(|id| {
            offer
                .candidates
                .iter()
                .any(|candidate| &candidate.board_id == id)
        })
        .or_else(|| offer.preselect.clone());
    let chosen = selected_id
        .as_deref()
        .and_then(|id| {
            offer
                .candidates
                .iter()
                .find(|candidate| candidate.board_id == id)
        })
        .cloned();

    let flash_action = move |choice: &FlashBoardChoice| {
        DevicesOp::action_for(DeviceAction::Flash {
            device,
            board_id: choice.board_id.clone(),
            build_id: choice.build_id.clone(),
            park_first: choice.park_first,
        })
    };

    let lead = board_filter_lead(
        chip.as_ref().map(|(name, source)| (name.as_str(), *source)),
        offer.candidates.len(),
        show_all(),
    );
    let escape = board_filter_escape(chip_name.as_deref(), show_all());
    let candidates = offer.candidates.clone();
    let candidates_for_pick = offer.candidates.clone();
    let panel = rsx! {
        BoardPickPanel {
            candidates,
            chip: chip_name.clone(),
            selected: selected_id,
            lead,
            escape_label: escape.as_ref().map(|escape| escape.label.clone()),
            escape_show_all: escape.map(|escape| escape.show_all).unwrap_or_default(),
            on_show_all: move |next: bool| show_all.set(next),
            on_pick: move |board_id: String| {
                match mode {
                    BoardPickMode::Row => pick.set(Some(board_id)),
                    // The verb was already pressed: picking IS the flash.
                    BoardPickMode::Verb => {
                        if let Some(choice) = candidates_for_pick
                            .iter()
                            .find(|candidate| candidate.board_id == board_id)
                        {
                            on_action.call(flash_action(choice));
                        }
                    }
                }
            },
        }
    };

    match mode {
        BoardPickMode::Verb => rsx! {
            PopoverButton {
                class: quiet_action_class().to_string(),
                open_class: quiet_action_class().to_string(),
                trigger: rsx! {
                    span { "Flash firmware" }
                },
                label: "Flash firmware".to_string(),
                title: "Write the firmware this Studio serves onto the board; the project and identity stay. Several boards fit this chip, so say which one it is."
                    .to_string(),
                popup_class: BOARD_POPUP_CLASS.to_string(),
                chrome_class: "ux-popover-chrome-neutral".to_string(),
                placement: PopoverPlacement::BottomStart,
                layer_keeps_layout: true,
                initially_open,
                {panel}
            }
        },
        BoardPickMode::Row => rsx! {
            div { class: trigger_slot_class(),
                PopoverButton {
                    class: pick_trigger_class().to_string(),
                    open_class: pick_trigger_class().to_string(),
                    trigger: rsx! {
                        BoardSwatch {
                            board_id: chosen.as_ref().map(|choice| choice.board_id.clone()),
                        }
                        span { class: trigger_label_class(), "{board_trigger_label(&chosen, offer.candidates.len())}" }
                        if let Some(family) = chosen.as_ref().and_then(|choice| board_family(&choice.board_id)) {
                            span { class: trigger_tag_class(), "{family}" }
                        }
                        span { class: trigger_caret_class(), "\u{25be}" }
                    },
                    label: "Choose this board".to_string(),
                    title: "Choose which board this is — the pin map is written to the device."
                        .to_string(),
                    popup_class: BOARD_POPUP_CLASS.to_string(),
                    chrome_class: "ux-popover-chrome-neutral".to_string(),
                    placement: PopoverPlacement::BottomStart,
                    layer_keeps_layout: true,
                    initially_open,
                    {panel}
                }
            }
            match chosen {
                Some(choice) => rsx! {
                    RowCta { action: flash_action(&choice), on_action }
                },
                // The pin map is written to the device, so an unresolved
                // pick leaves the verb waiting rather than guessing.
                None => rsx! {
                    RowCtaDisabled {
                        label: "Flash firmware".to_string(),
                        hint: "Pick the board first — the pin map is written to the device."
                            .to_string(),
                    }
                },
            }
        },
    }
}

/// The board panel: the filter line and its escape, the board tiles, and the
/// sentence that says why the pick matters and why a wrong one is survivable.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BoardPickPanel(
    candidates: Vec<FlashBoardChoice>,
    /// The chip the list was filtered by, for the tiles' "matches" mark.
    chip: Option<String>,
    selected: Option<String>,
    lead: String,
    escape_label: Option<String>,
    escape_show_all: bool,
    on_show_all: EventHandler<bool>,
    on_pick: EventHandler<String>,
) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();

    rsx! {
        div { class: "tw:grid tw:min-w-0",
            div { class: panel_top_class(),
                p { class: filter_line_class(),
                    "{lead}"
                    if let Some(label) = escape_label.clone() {
                        " \u{b7} "
                        button {
                            class: filter_escape_class(),
                            r#type: "button",
                            onclick: move |event: MouseEvent| {
                                event.stop_propagation();
                                on_show_all.call(escape_show_all);
                            },
                            "{label}"
                        }
                    }
                }
            }
            div { class: panel_body_class(),
                div { class: board_grid_class(),
                    for candidate in candidates {
                        {
                            let picked = selected.as_deref() == Some(candidate.board_id.as_str());
                            let board_id = candidate.board_id.clone();
                            let family = board_family(&candidate.board_id);
                            let matches = family.is_some() && family.as_deref() == chip.as_deref();
                            rsx! {
                                button {
                                    key: "{candidate.board_id}",
                                    class: board_tile_class(picked),
                                    r#type: "button",
                                    onclick: move |event: MouseEvent| {
                                        event.stop_propagation();
                                        on_pick.call(board_id.clone());
                                        if let Some(mut close) = close {
                                            close.close();
                                        }
                                    },
                                    if picked {
                                        span { class: OPTION_CARD_CHECK_CLASS, aria_hidden: "true",
                                            StudioIcon { name: StudioIconName::StepComplete, size: 10 }
                                        }
                                    }
                                    BoardFigure { board_id: candidate.board_id.clone() }
                                    span { class: board_tile_title_class(), "{candidate.title}" }
                                    span { class: board_tile_sub_class(), "{candidate.blurb}" }
                                    if let Some(family) = family {
                                        span { class: board_tile_family_class(matches),
                                            "{family_tag_text(&family, matches)}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            p { class: panel_foot_class(),
                "The pin map is written to the device, so the board must be right; the chip guard makes a wrong pick fail safely."
            }
        }
    }
}

/// The board panel's escape from (or back into) the chip filter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BoardFilterEscape {
    pub(crate) label: String,
    /// What pressing it sets show-all to.
    pub(crate) show_all: bool,
}

/// The filter line's sentence (AC4): what narrowed the list, and how many
/// boards survived it.
///
/// Three readings, and the test pins all three:
///
/// - a known chip, filtered — names the chip AND the source that answered
///   it, because "the list was filtered" only earns trust when the user can
///   see what it was filtered on;
/// - show-all — says the list is now everything, and that the flash
///   preflight is the real safety;
/// - an unknown chip — the list was never narrowed, so there is nothing to
///   escape from, and the preflight is again what is being leaned on.
pub(crate) fn board_filter_lead(
    chip: Option<(&str, ChipSource)>,
    fits: usize,
    show_all: bool,
) -> String {
    match (chip, show_all) {
        (_, true) => "Every served board \u{2014} the flash preflight checks the chip".to_string(),
        (Some((chip, source)), false) => format!(
            "Detected {chip} {} \u{b7} {}",
            source.phrase(),
            board_count(fits)
        ),
        (None, false) => format!(
            "No boot banner named the chip \u{2014} every served board is offered ({}); \
             the flash preflight checks the pick against the silicon",
            board_count(fits)
        ),
    }
}

/// The filter line's escape link, when there is one to offer. An unknown
/// chip never narrowed anything, so it has nothing to escape.
pub(crate) fn board_filter_escape(chip: Option<&str>, show_all: bool) -> Option<BoardFilterEscape> {
    let chip = chip?;
    Some(match show_all {
        true => BoardFilterEscape {
            label: format!("only {chip}"),
            show_all: false,
        },
        false => BoardFilterEscape {
            label: "show all".to_string(),
            show_all: true,
        },
    })
}

/// "1 board fits" / "N boards fit" — the unit-awareness principle applied to
/// the smallest number Studio ever prints.
fn board_count(fits: usize) -> String {
    match fits {
        1 => "1 board fits".to_string(),
        n => format!("{n} boards fit"),
    }
}

/// The board trigger's label: the pick, or how many are waiting for one.
fn board_trigger_label(chosen: &Option<FlashBoardChoice>, candidates: usize) -> String {
    match chosen {
        Some(choice) => choice.title.clone(),
        None => board_count(candidates),
    }
}

/// The tile's family tag, with the "matches" mark that is the whole reason
/// the tag is there.
fn family_tag_text(family: &str, matches: bool) -> String {
    match matches {
        true => format!("{family} \u{b7} matches"),
        false => family.to_string(),
    }
}

/// The catalog family for a board id ("esp32c6"), for the tile's family tag.
/// A display join only — the pick's real safety is the flash preflight's
/// chip guard, which runs against the silicon.
fn board_family(board_id: &str) -> Option<String> {
    lpa_boards::board_by_id(board_id).map(|board| board.family.clone())
}

/// The trigger's provenance tag, in the user's words rather than the enum's.
fn provenance_tag(group: PushSourceGroup) -> &'static str {
    match group {
        PushSourceGroup::Example => "example",
        PushSourceGroup::Library => "my project",
        PushSourceGroup::New => "new",
    }
}

/// This session's captured poster for a choice, when the gallery pages
/// already made one. Deliberately the CACHE and not a lease: a popover that
/// leased forty preview slots to draw forty thumbs would spend the whole
/// preview pool on a control that is open for two seconds.
fn choice_poster(choice: &PushSourceChoice) -> Option<String> {
    let source = match &choice.source {
        PushSource::Example { example_id } => PreviewSource::Example(example_id.clone()),
        PushSource::Library { project_uid } => PreviewSource::ProjectUid(project_uid.clone()),
        PushSource::NewForBoard { .. } => return None,
    };
    cached_poster(&source)
}

/// The pick's own swatch on the trigger: its poster when the session has
/// one, else the identity gradient every card thumb falls back to.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProjectSwatch(seed: String, poster: Option<String>) -> Element {
    rsx! {
        span {
            class: "tw:relative tw:block tw:h-4 tw:w-[22px] tw:flex-none tw:overflow-hidden tw:rounded-xs",
            style: thumb_swatch_style(&seed, false),
            aria_hidden: "true",
            if let Some(poster) = poster {
                img {
                    class: "tw:absolute tw:inset-0 tw:h-full tw:w-full tw:object-cover",
                    src: "{poster}",
                    alt: "",
                }
            }
        }
    }
}

/// The tile's face: the board as `lpa-boards` draws it, fitted to the tile's
/// figure band.
///
/// A board the catalog cannot draw still gets a tile — the flash offer is
/// built from the firmware join, not from the display sidecars, so the two
/// lists could in principle disagree — and the band holds its place empty
/// rather than collapsing, which would make one tile shorter than its
/// neighbours for a reason the user cannot see.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BoardFigure(board_id: String) -> Element {
    rsx! {
        span { class: board_figure_class(), aria_hidden: "true",
            if let Some(board) = lpa_boards::board_by_id(&board_id) {
                BoardDiagram {
                    board: board.clone(),
                    mode: DiagramMode::Plain,
                    labels: false,
                    landscape: true,
                    fit: (BOARD_FIGURE_W, BOARD_FIGURE_H),
                }
            }
        }
    }
}

/// The board pick's swatch on the trigger: the picked board at the project
/// swatch's own 22×16, on the raised chip that stands in for it before a
/// pick is made. Turned like the tiles, so the board lies along the swatch's
/// long axis instead of standing as a 5px thread down it: what the swatch
/// carries is the silhouette — a long devkit against a stub of a XIAO — with
/// the trigger's label there for the name.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BoardSwatch(board_id: Option<String>) -> Element {
    let board = board_id.as_deref().and_then(lpa_boards::board_by_id);
    rsx! {
        span { class: board_swatch_class(), aria_hidden: "true",
            if let Some(board) = board {
                BoardDiagram {
                    board: board.clone(),
                    mode: DiagramMode::Plain,
                    labels: false,
                    landscape: true,
                    fit: (BOARD_SWATCH_W, BOARD_SWATCH_H),
                }
            }
        }
    }
}

/// The verb row's slot for a picker trigger: a GRID so [`PopoverButton`]'s
/// own inline-grid wrapper stretches into it, and `flex-1` so the trigger
/// takes the row's free width while the CTA keeps its own. (The palette
/// swatch field's arrangement — the only one that survives the popover's
/// open-state placeholder pinning, which freezes the trigger at its
/// measured width.)
fn trigger_slot_class() -> &'static str {
    "tw:grid tw:min-w-0 tw:flex-1"
}

/// The picker trigger itself. Keeps the P4 placeholder's exact metrics —
/// 26px tall, the full width of its slot, bordered, `px-2`, `gap-1.5` — so
/// wiring the popover changed behaviour and not layout.
fn pick_trigger_class() -> &'static str {
    "tw:inline-flex tw:h-[26px] tw:w-full tw:min-w-0 tw:cursor-pointer tw:items-center tw:gap-1.5 tw:rounded-md tw:border tw:border-border tw:bg-transparent tw:px-2 tw:text-left tw:text-xs tw:font-semibold tw:text-muted-foreground tw:hover:border-border-strong tw:hover:text-strong-foreground"
}

fn trigger_label_class() -> &'static str {
    "tw:min-w-0 tw:flex-1 tw:truncate"
}

fn trigger_tag_class() -> &'static str {
    "tw:flex-none tw:text-[10px] tw:font-medium tw:text-subtle-foreground"
}

fn trigger_caret_class() -> &'static str {
    "tw:flex-none tw:text-[10px] tw:text-subtle-foreground"
}

/// The board pick's swatch: the spike's raised chip, now a FRAME the picked
/// board is drawn inside (`place-items-center`, so an unpicked trigger is the
/// bare chip and nothing shifts when a pick fills it).
fn board_swatch_class() -> &'static str {
    "tw:grid tw:h-4 tw:w-[22px] tw:flex-none tw:place-items-center tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-raised-strong"
}

/// The swatch's drawing box, inside its 22×16 chip and its 1px border.
const BOARD_SWATCH_W: f32 = 20.0;
const BOARD_SWATCH_H: f32 = 14.0;

/// The tile's figure band: 56px, the project pick cards' own thumb height, on
/// the terminal ground the boards page draws its figures on. A FIXED height,
/// so eight boards that differ three to one in drawing height still line their
/// names up on one row — and, with the boards turned on their side, one the
/// drawing fills rather than rattles around in.
fn board_figure_class() -> &'static str {
    "tw:mb-1 tw:grid tw:h-14 tw:min-w-0 tw:place-items-center tw:overflow-hidden tw:rounded-xs tw:bg-terminal"
}

/// The figure band's drawing box: the NARROWEST tile the grid can make (its
/// 150px minimum, less the border and `p-2`) rather than the one the 520px
/// panel usually gives, because the band clips and a wider box would crop the
/// board's ends at a viewport-clamped width. With the drawing turned it is
/// the width that binds for every board but the near-square quinled pair.
const BOARD_FIGURE_W: f32 = 132.0;
const BOARD_FIGURE_H: f32 = 56.0;

/// The gallery panel: the spike's 520px, never past the viewport.
///
/// `whitespace-normal` is load-bearing, not decoration. The verb row sets
/// `white-space: nowrap` so a two-word verb cannot wrap and burst its fixed
/// 30px — and that INHERITS down the DOM into the panel, which lives in the
/// top layer visually but is still a descendant of the trigger. Without the
/// reset the panel's prose becomes one unwrappable line and its own
/// `overflow-hidden` clips the sentence in half.
const GALLERY_POPUP_CLASS: &str = "tw:grid tw:w-[520px] tw:max-w-[calc(100vw-80px)] tw:min-w-0 tw:overflow-hidden tw:whitespace-normal tw:rounded-md tw:border tw:text-sm tw:text-muted-foreground";

/// The board panel: the gallery's own 520px since the tiles carry drawings
/// (at 440 the grid fell to two columns, so the eight served boards of
/// show-all became a four-row scroll). Same `whitespace-normal` reset as the
/// gallery, for the same inherited reason.
const BOARD_POPUP_CLASS: &str = "tw:grid tw:w-[520px] tw:max-w-[calc(100vw-80px)] tw:min-w-0 tw:overflow-hidden tw:whitespace-normal tw:rounded-md tw:border tw:text-sm tw:text-muted-foreground";

fn panel_top_class() -> &'static str {
    "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:border-b tw:border-border-muted tw:px-2.5 tw:py-2"
}

/// The body scrolls; the panel never grows past a screenful.
fn panel_body_class() -> &'static str {
    "tw:max-h-[340px] tw:min-w-0 tw:overflow-y-auto tw:overflow-x-hidden tw:p-2.5"
}

fn panel_foot_class() -> &'static str {
    "tw:m-0 tw:border-t tw:border-border-muted tw:px-2.5 tw:py-2 tw:text-[11px] tw:leading-snug tw:text-dim-foreground"
}

fn panel_note_class() -> &'static str {
    "tw:m-0 tw:text-[11.5px] tw:leading-snug tw:text-subtle-foreground"
}

fn panel_search_class() -> &'static str {
    "tw:ml-auto tw:w-[170px] tw:min-w-0 tw:appearance-none tw:rounded-xs tw:border tw:border-border tw:bg-card tw:px-2 tw:py-1 tw:text-[11.5px] tw:text-strong-foreground"
}

fn tab_class(active: bool) -> String {
    let base = "tw:inline-flex tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1 tw:rounded-xs tw:border tw:px-2 tw:py-1 tw:text-[11.5px] tw:font-semibold";
    if active {
        format!("{base} tw:border-border-muted tw:bg-card tw:text-strong-foreground")
    } else {
        format!(
            "{base} tw:border-transparent tw:bg-transparent tw:text-subtle-foreground tw:hover:text-soft-foreground"
        )
    }
}

fn tab_count_class() -> &'static str {
    "tw:text-[10px] tw:font-medium tw:text-dim-foreground"
}

/// The spike's card grid: a 112px minimum, so a 520px panel shows four
/// across and a viewport-clamped one still shows two.
fn pick_grid_class() -> &'static str {
    "tw:grid tw:min-w-0 tw:grid-cols-[repeat(auto-fill,minmax(112px,1fr))] tw:gap-2"
}

/// One pick card. Selected wears the option-card grammar verbatim
/// (`ux-sel-ring` + selection wash + check badge) — a picked card IS a
/// selection, and selection is one language app-wide.
fn pick_card_class(selected: bool) -> &'static str {
    if selected {
        "ux-sel-ring tw:relative tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:content-start tw:gap-1 tw:rounded-sm tw:border tw:border-transparent tw:bg-selection-bg tw:p-1.5 tw:text-left tw:text-strong-foreground"
    } else {
        "tw:relative tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:content-start tw:gap-1 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-transparent tw:p-1.5 tw:text-left tw:text-muted-foreground tw:hover:border-border-strong tw:hover:text-strong-foreground"
    }
}

fn pick_card_title_class() -> &'static str {
    "tw:min-w-0 tw:truncate tw:text-[11.5px] tw:font-semibold"
}

fn pick_card_sub_class() -> &'static str {
    "tw:min-w-0 tw:truncate tw:text-[10px] tw:text-dim-foreground"
}

/// The starter's face: a dashed frame at the thumb's own aspect, so the New
/// tab's card is exactly the size of every other card.
fn pick_new_face_class() -> &'static str {
    "tw:grid tw:aspect-[4/3] tw:w-full tw:place-items-center tw:rounded-xs tw:border tw:border-dashed tw:border-border-strong tw:text-lg tw:text-subtle-foreground"
}

fn filter_line_class() -> &'static str {
    "tw:m-0 tw:min-w-0 tw:text-[11px] tw:leading-snug tw:text-subtle-foreground"
}

fn filter_escape_class() -> &'static str {
    "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:p-0 tw:text-[11px] tw:font-semibold tw:text-muted-foreground tw:underline tw:hover:text-strong-foreground"
}

/// Board tiles are wider than pick cards: a board's name plus its
/// manufacturer is the whole face, and 150px is what keeps them off a
/// second line.
fn board_grid_class() -> &'static str {
    "tw:grid tw:min-w-0 tw:grid-cols-[repeat(auto-fill,minmax(150px,1fr))] tw:gap-2"
}

fn board_tile_class(selected: bool) -> &'static str {
    if selected {
        "ux-sel-ring tw:relative tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:content-start tw:gap-0.5 tw:rounded-sm tw:border tw:border-transparent tw:bg-selection-bg tw:p-2 tw:text-left tw:text-strong-foreground"
    } else {
        "tw:relative tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:content-start tw:gap-0.5 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-transparent tw:p-2 tw:text-left tw:text-muted-foreground tw:hover:border-border-strong tw:hover:text-strong-foreground"
    }
}

fn board_tile_title_class() -> &'static str {
    "tw:min-w-0 tw:truncate tw:text-xs tw:font-semibold"
}

fn board_tile_sub_class() -> &'static str {
    "tw:min-w-0 tw:truncate tw:text-[10.5px] tw:text-dim-foreground"
}

/// The family tag. Green ONLY where it equals the detected chip: the point
/// of the mark is "this one matches the silicon", and a tag that were always
/// green would say nothing.
fn board_tile_family_class(matches: bool) -> &'static str {
    if matches {
        "tw:min-w-0 tw:truncate tw:font-mono tw:text-[9.5px] tw:text-status-good-foreground"
    } else {
        "tw:min-w-0 tw:truncate tw:font-mono tw:text-[9.5px] tw:text-dim-foreground"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC4: the filter line states the filter AND the source that answered
    /// it, says how many boards survived, and reads honestly in the two
    /// states that have no filter to state.
    #[test]
    fn the_filter_line_states_the_chip_its_source_and_the_escape() {
        let filtered = board_filter_lead(Some(("esp32c6", ChipSource::BootBanner)), 2, false);
        assert_eq!(
            filtered,
            "Detected esp32c6 in the boot banner \u{b7} 2 boards fit"
        );
        assert_eq!(
            board_filter_escape(Some("esp32c6"), false),
            Some(BoardFilterEscape {
                label: "show all".to_string(),
                show_all: true,
            })
        );

        // The hello-only board: the chip came from its firmware's board id,
        // not from a boot banner it never printed.
        let joined = board_filter_lead(Some(("esp32c6", ChipSource::Firmware)), 1, false);
        assert_eq!(
            joined,
            "Detected esp32c6 from its firmware \u{b7} 1 board fits"
        );

        // Show all: the preflight is what is being leaned on, and the way
        // back is offered by name.
        let all = board_filter_lead(Some(("esp32c6", ChipSource::BootBanner)), 9, true);
        assert_eq!(
            all,
            "Every served board \u{2014} the flash preflight checks the chip"
        );
        assert_eq!(
            board_filter_escape(Some("esp32c6"), true).map(|escape| escape.label),
            Some("only esp32c6".to_string()),
        );

        // No chip at all: nothing was narrowed, so nothing is escapable.
        let unknown = board_filter_lead(None, 9, false);
        assert!(
            unknown.contains("No boot banner named the chip"),
            "{unknown}"
        );
        assert!(unknown.contains("9 boards fit"), "{unknown}");
        assert!(unknown.contains("preflight"), "{unknown}");
        assert_eq!(board_filter_escape(None, false), None);
        assert_eq!(board_filter_escape(None, true), None);
    }

    /// The trigger must keep the P4 placeholder's metrics exactly — the row
    /// is a fixed 30px and the card's height may not move when a popover
    /// opens, so a trigger that grew by a pixel would break AC2.
    #[test]
    fn the_trigger_keeps_the_fixed_rows_metrics() {
        let trigger = pick_trigger_class();
        assert!(trigger.contains("tw:h-[26px]"), "{trigger}");
        assert!(trigger.contains("tw:min-w-0"), "{trigger}");
        // The SLOT grows, not the button: PopoverButton wraps the trigger in
        // its own inline-grid span, which only stretches inside a grid.
        assert!(trigger.contains("tw:w-full"), "{trigger}");
        let slot = trigger_slot_class();
        assert!(slot.contains("tw:flex-1"), "{slot}");
        assert!(slot.contains("tw:grid"), "{slot}");
        // No fixed height anywhere on a panel: it scrolls instead, so the
        // library's size cannot reach the card.
        assert!(panel_body_class().contains("tw:overflow-y-auto"));
        assert!(panel_body_class().contains("tw:max-h-[340px]"));
        // Both panels undo the verb row's INHERITED `nowrap` (which is there
        // so a verb cannot burst the 30px row). Without the reset the panels'
        // prose is one unwrappable line and their own overflow clips it —
        // the board panel's foot sentence lost its second half.
        for panel in [GALLERY_POPUP_CLASS, BOARD_POPUP_CLASS] {
            assert!(panel.contains("tw:whitespace-normal"), "{panel}");
        }
    }

    /// The selection grammar is the option cards' own — one language for
    /// "this is the chosen one", app-wide (the accent reckoning: never a
    /// hue).
    #[test]
    fn a_picked_card_wears_the_selection_family() {
        for picked in [pick_card_class(true), board_tile_class(true)] {
            assert!(picked.contains("ux-sel-ring"), "{picked}");
            assert!(picked.contains("tw:bg-selection-bg"), "{picked}");
            assert!(!picked.contains("accent"), "{picked}");
        }
        for plain in [pick_card_class(false), board_tile_class(false)] {
            assert!(!plain.contains("ux-sel-ring"), "{plain}");
            assert!(!plain.contains("tw:bg-selection-bg"), "{plain}");
        }
        assert!(OPTION_CARD_CHECK_CLASS.contains("tw:absolute"));
    }

    /// Every board the flash offer can put on a tile is one the catalog can
    /// DRAW, and both drawing boxes fit inside the frames that hold them —
    /// the figure band is a fixed height, so the tiles' names stay on one row
    /// however tall the board is.
    #[test]
    fn every_offered_board_has_a_drawing_that_fits_its_frame() {
        let offered = flash_offer(None).candidates;
        assert!(offered.len() > 1, "the catalog serves several boards");
        for candidate in &offered {
            assert!(
                lpa_boards::board_by_id(&candidate.board_id).is_some(),
                "{} has no display sidecar to draw",
                candidate.board_id,
            );
        }
        // The swatch draws inside a 22×16 chip with a 1px border; the figure
        // band inside a 56px row.
        assert!(BOARD_SWATCH_W <= 20.0 && BOARD_SWATCH_H <= 14.0);
        assert_eq!(BOARD_FIGURE_H, 56.0);
        assert!(board_figure_class().contains("tw:h-14"));
        assert!(board_swatch_class().contains("tw:h-4"));
        assert!(board_swatch_class().contains("tw:w-[22px]"));
    }

    /// The family tag is green only where it means something.
    #[test]
    fn the_family_tag_is_green_only_when_it_matches_the_chip() {
        assert!(board_tile_family_class(true).contains("status-good"));
        assert!(!board_tile_family_class(false).contains("status-good"));
        assert_eq!(family_tag_text("esp32c6", true), "esp32c6 \u{b7} matches");
        assert_eq!(family_tag_text("esp32", false), "esp32");
    }

    /// The tabs are the offer's own groups, and the trigger's tag speaks the
    /// same vocabulary the tabs do.
    #[test]
    fn every_tab_maps_to_one_push_group() {
        for tab in [PickTab::Examples, PickTab::Mine, PickTab::New] {
            assert_eq!(PickTab::for_group(tab.group()), tab);
        }
        assert_eq!(provenance_tag(PushSourceGroup::Example), "example");
        assert_eq!(provenance_tag(PushSourceGroup::Library), "my project");
        assert_eq!(provenance_tag(PushSourceGroup::New), "new");
    }

    /// The joined chip names its own source: the boot banner when the ROM
    /// printed one, the firmware's board id otherwise (P2's join).
    #[test]
    fn the_chip_source_says_which_fact_answered() {
        let mut card = card_fixture();
        assert_eq!(joined_chip(&card), None);

        card.board_id = Some("seeed/xiao-esp32-c6".to_string());
        assert_eq!(
            joined_chip(&card),
            Some(("esp32c6".to_string(), ChipSource::Firmware))
        );

        card.detected_chip = Some("esp32c6".to_string());
        assert_eq!(
            joined_chip(&card),
            Some(("esp32c6".to_string(), ChipSource::BootBanner))
        );
    }

    /// The board trigger says what is picked, or how many are waiting for a
    /// pick — never an empty chip.
    #[test]
    fn the_board_trigger_names_the_pick_or_the_count() {
        assert_eq!(board_trigger_label(&None, 2), "2 boards fit");
        assert_eq!(board_trigger_label(&None, 1), "1 board fits");
        let choice = flash_offer(Some("esp32c6"))
            .candidates
            .first()
            .cloned()
            .expect("the catalog ships a C6 board");
        let title = choice.title.clone();
        assert_eq!(board_trigger_label(&Some(choice), 2), title);
    }

    fn card_fixture() -> DeviceView {
        use lpa_studio_core::{DeviceEscape, DeviceLoadedProject, DeviceStatus};
        DeviceView {
            id: DeviceId(1),
            title: "Bench board".to_string(),
            status: DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: None,
            firmware: None,
            needs_firmware: false,
            degraded: None,
            loaded_project: DeviceLoadedProject::Empty,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            escapes: vec![DeviceEscape::Forget],
        }
    }
}

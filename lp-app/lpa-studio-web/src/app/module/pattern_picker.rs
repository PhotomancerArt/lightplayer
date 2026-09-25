//! The Play-mode **Pattern** instrument (multi-pattern vision D15/D21).
//!
//! It sits between the shared knobs and the playing pattern's own knobs,
//! and it does four things, top to bottom:
//!
//! 1. **Prev · now playing · next.** The playing pattern's name, between
//!    the two step buttons. The step targets are chosen in core (the
//!    neighbouring enabled pattern, wrapping), so a button with nowhere to
//!    go is simply absent.
//! 2. **Tour.** A switch and, while it runs, the step time with shorter /
//!    longer buttons. Step times are a short ladder (`20 s`, `30 s`, …), so
//!    they are squared blocks, not a drag.
//! 3. **The set.** Every pattern's name in authored order — a scrolling
//!    list at phone width, as many columns as fit when there is room. Tap a name to
//!    play it. The playing one wears the live family (the playlist strip's
//!    ACTIVE colour); a failed one says so in the error family; a skipped
//!    one is dimmed and dashed.
//! 4. **On/off per pattern.** A squared block at the end of each row: off
//!    means the tour, next and prev pass it by. A tap still plays it.
//!
//! Everything a gesture sends is a ready [`UiAction`] on the picker — this
//! component decides nothing. While the panel holds the tour or the switch
//! set, those controls wear the engaged gold and a reset glyph releases
//! them (panel.md P2: clear is always one obvious gesture).
//!
//! State changes are class swaps, never inline `style` writes (Dioxus
//! whole-string style writes never remove properties).

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiPanelTarget, UiPatternEntryState, UiPatternPicker, UiPatternPickerEntry,
};

use crate::app::module::PanelGesture;
use crate::base::{StudioIcon, StudioIconName};

/// A squared block button: the discrete-control language (panel.md; the
/// stepped knob's blocks).
const BLOCK_CLASS: &str = "tw:inline-flex tw:h-8 tw:min-w-8 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-raised tw:px-2 tw:font-mono tw:text-xs tw:text-strong-foreground tw:hover:border-muted-foreground";
/// The same block with nothing to do: kept in place so the row never
/// reflows, but inert and quiet.
const BLOCK_INERT_CLASS: &str = "tw:inline-flex tw:h-8 tw:min-w-8 tw:flex-none tw:cursor-default tw:appearance-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-muted tw:bg-transparent tw:px-2 tw:font-mono tw:text-xs tw:text-dim-foreground";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PatternPicker(
    picker: UiPatternPicker,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    #[props(default)] on_panel: Option<EventHandler<PanelGesture>>,
) -> Element {
    let playing = picker
        .playing()
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| "—".to_string());
    let tour_held = picker
        .tour_target
        .as_ref()
        .is_some_and(|target| target.engaged);
    let skip_held = picker
        .skip_target
        .as_ref()
        .is_some_and(|target| target.engaged);

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2.5",
            // 1. prev · now playing · next
            div { class: "tw:flex tw:min-w-0 tw:items-stretch tw:gap-1.5",
                ActionBlock {
                    label: "◀",
                    title: "Previous pattern",
                    action: picker.prev.clone(),
                    on_action,
                }
                div {
                    class: "tw:flex tw:min-w-0 tw:flex-1 tw:items-center tw:gap-2 tw:rounded-xs tw:border tw:border-status-live-border tw:bg-status-live-bg tw:px-2.5",
                    span { class: "tw:flex-none tw:text-[0.55rem] tw:font-bold tw:uppercase tw:tracking-[0.14em] tw:text-status-live-foreground",
                        "playing"
                    }
                    span {
                        class: "tw:min-w-0 tw:truncate tw:text-sm tw:text-strong-foreground",
                        title: "{playing}",
                        "{playing}"
                    }
                }
                ActionBlock {
                    label: "▶",
                    title: "Next pattern",
                    action: picker.next.clone(),
                    on_action,
                }
            }
            // 2. tour
            TourRow { picker: picker.clone(), held: tour_held, on_action, on_panel }
            // 3 + 4. the set, with its on/off blocks
            div { class: "tw:grid tw:min-w-0 tw:gap-1",
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
                    span { class: "tw:text-[0.6rem] tw:font-bold tw:uppercase tw:tracking-[0.12em] tw:text-subtle-foreground",
                        "{picker.entries.len()} patterns"
                    }
                    span { class: "tw:ml-auto tw:text-[0.6rem] tw:text-dim-foreground", "in tour" }
                    if skip_held && let Some(target) = picker.skip_target.clone() {
                        ResetGlyph {
                            target,
                            title: "Reset the on/off switches — the playlist's own list plays again",
                            on_panel,
                        }
                    }
                }
                ul { class: "tw:m-0 tw:grid tw:max-h-[24rem] tw:min-w-0 tw:list-none tw:grid-cols-[repeat(auto-fill,minmax(12rem,1fr))] tw:gap-1 tw:overflow-y-auto tw:p-0",
                    for entry in picker.entries.clone() {
                        PatternRow {
                            key: "{entry.key}",
                            entry,
                            held: skip_held,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// The tour switch, and the step while it runs.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TourRow(
    picker: UiPatternPicker,
    held: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    #[props(default)] on_panel: Option<EventHandler<PanelGesture>>,
) -> Element {
    let touring = picker.touring();
    let switch_class = match (touring, held) {
        (true, true) => {
            "tw:inline-flex tw:h-8 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:rounded-xs tw:border tw:border-status-engaged-border tw:bg-status-engaged-bg tw:px-2.5 tw:text-xs tw:font-bold tw:text-status-engaged-foreground"
        }
        (true, false) => {
            "tw:inline-flex tw:h-8 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:rounded-xs tw:border tw:border-border-strong tw:bg-card-raised tw:px-2.5 tw:text-xs tw:font-bold tw:text-strong-foreground"
        }
        (false, true) => {
            "tw:inline-flex tw:h-8 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:rounded-xs tw:border tw:border-status-engaged-border tw:bg-transparent tw:px-2.5 tw:text-xs tw:text-status-engaged-foreground"
        }
        (false, false) => {
            "tw:inline-flex tw:h-8 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:rounded-xs tw:border tw:border-border tw:bg-transparent tw:px-2.5 tw:text-xs tw:text-muted-foreground"
        }
    };
    let pip_class = if touring {
        "tw:h-2.5 tw:w-2.5 tw:flex-none tw:rounded-[1px] tw:bg-muted-foreground"
    } else {
        "tw:h-2.5 tw:w-2.5 tw:flex-none tw:rounded-[1px] tw:border tw:border-dim-foreground"
    };
    let toggle = picker.tour_toggle.clone();
    let step = picker
        .step_seconds()
        .map(lpa_studio_core::app::project::node::pattern_picker_derivation::format_step_seconds);

    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-1.5",
            button {
                class: switch_class,
                r#type: "button",
                role: "switch",
                aria_checked: "{touring}",
                disabled: toggle.is_none(),
                title: if touring { "Touring — tap to stay on the playing pattern" } else { "Holding — tap to tour the patterns" },
                onclick: move |event| {
                    event.stop_propagation();
                    if let (Some(action), Some(handler)) = (toggle.clone(), on_action) {
                        handler.call(action);
                    }
                },
                span { class: pip_class }
                "Tour"
            }
            if let Some(step) = step {
                ActionBlock {
                    label: "−",
                    title: "Shorter step",
                    action: picker.step_shorter.clone(),
                    on_action,
                }
                span { class: "tw:inline-flex tw:h-8 tw:min-w-[4.5rem] tw:items-center tw:justify-center tw:font-mono tw:text-xs tw:tabular-nums tw:text-strong-foreground",
                    "every {step}"
                }
                ActionBlock {
                    label: "+",
                    title: "Longer step",
                    action: picker.step_longer.clone(),
                    on_action,
                }
            } else {
                span { class: "tw:text-xs tw:text-dim-foreground", "staying on the playing pattern" }
            }
            if held && let Some(target) = picker.tour_target.clone() {
                ResetGlyph {
                    target,
                    title: "Reset the tour — the playlist's own setting plays again",
                    on_panel,
                }
            }
        }
    }
}

/// One pattern: its name (tap to play) and its on/off block.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PatternRow(
    entry: UiPatternPickerEntry,
    /// The panel holds the switch set: the blocks wear the engaged gold.
    held: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let row_class = match entry.state {
        UiPatternEntryState::Playing => {
            "tw:flex tw:min-w-0 tw:items-stretch tw:gap-1 tw:rounded-xs tw:border tw:border-status-live-border tw:bg-status-live-bg"
        }
        UiPatternEntryState::Failed => {
            "tw:flex tw:min-w-0 tw:items-stretch tw:gap-1 tw:rounded-xs tw:border tw:border-status-error-border tw:bg-transparent"
        }
        UiPatternEntryState::Skipped => {
            "tw:flex tw:min-w-0 tw:items-stretch tw:gap-1 tw:rounded-xs tw:border tw:border-dashed tw:border-border tw:bg-transparent"
        }
        UiPatternEntryState::Available => {
            "tw:flex tw:min-w-0 tw:items-stretch tw:gap-1 tw:rounded-xs tw:border tw:border-border tw:bg-card"
        }
    };
    let name_class = match entry.state {
        UiPatternEntryState::Playing => {
            "tw:min-w-0 tw:truncate tw:text-sm tw:font-bold tw:text-status-live-foreground"
        }
        UiPatternEntryState::Failed => "tw:min-w-0 tw:truncate tw:text-sm tw:text-muted-foreground",
        UiPatternEntryState::Skipped => "tw:min-w-0 tw:truncate tw:text-sm tw:text-dim-foreground",
        UiPatternEntryState::Available => {
            "tw:min-w-0 tw:truncate tw:text-sm tw:text-strong-foreground"
        }
    };
    let tag = match entry.state {
        UiPatternEntryState::Playing => Some((
            "playing",
            "tw:flex-none tw:text-[0.55rem] tw:font-bold tw:uppercase tw:tracking-[0.12em] tw:text-status-live-foreground",
        )),
        UiPatternEntryState::Failed => Some((
            "failed",
            "tw:flex-none tw:text-[0.55rem] tw:font-bold tw:uppercase tw:tracking-[0.12em] tw:text-status-error-foreground",
        )),
        UiPatternEntryState::Skipped | UiPatternEntryState::Available => None,
    };
    let title = match entry.state {
        UiPatternEntryState::Playing => format!("{} is playing", entry.name),
        UiPatternEntryState::Failed => format!(
            "{} failed to load on the device — tap to try it again",
            entry.name
        ),
        UiPatternEntryState::Skipped => format!("Play {} (it is off in the tour)", entry.name),
        UiPatternEntryState::Available => format!("Play {}", entry.name),
    };
    let play = entry.play.clone();
    let toggle = entry.toggle.clone();
    let block_class = "tw:inline-flex tw:w-9 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:border-0 tw:border-l tw:border-solid tw:border-border-muted tw:bg-transparent tw:p-0";
    let square_class = match (entry.enabled, held) {
        (true, true) => "tw:h-3.5 tw:w-3.5 tw:rounded-[1px] tw:bg-status-engaged-foreground",
        (true, false) => "tw:h-3.5 tw:w-3.5 tw:rounded-[1px] tw:bg-muted-foreground",
        (false, true) => {
            "tw:h-3.5 tw:w-3.5 tw:rounded-[1px] tw:border tw:border-status-engaged-foreground"
        }
        (false, false) => "tw:h-3.5 tw:w-3.5 tw:rounded-[1px] tw:border tw:border-dim-foreground",
    };
    let switch_label = if entry.enabled {
        format!("{} is on in the tour — tap to skip it", entry.name)
    } else {
        format!("{} is off in the tour — tap to include it", entry.name)
    };

    rsx! {
        li { class: row_class,
            button {
                class: "tw:flex tw:min-h-9 tw:min-w-0 tw:flex-1 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2 tw:border-0 tw:bg-transparent tw:px-2.5 tw:py-1 tw:text-left",
                r#type: "button",
                title: "{title}",
                aria_label: "{title}",
                aria_current: if entry.state == UiPatternEntryState::Playing { "true" } else { "false" },
                onclick: move |event| {
                    event.stop_propagation();
                    if let (Some(action), Some(handler)) = (play.clone(), on_action) {
                        handler.call(action);
                    }
                },
                span { class: name_class, "{entry.name}" }
                if let Some((text, class)) = tag {
                    span { class, "{text}" }
                }
            }
            button {
                class: block_class,
                r#type: "button",
                role: "switch",
                aria_checked: "{entry.enabled}",
                aria_label: "{switch_label}",
                title: "{switch_label}",
                disabled: toggle.is_none(),
                onclick: move |event| {
                    event.stop_propagation();
                    if let (Some(action), Some(handler)) = (toggle.clone(), on_action) {
                        handler.call(action);
                    }
                },
                span { class: square_class }
            }
        }
    }
}

/// A squared block that dispatches one ready action, or sits inert when
/// there is none.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ActionBlock(
    label: &'static str,
    title: &'static str,
    action: Option<UiAction>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let class = if action.is_some() {
        BLOCK_CLASS
    } else {
        BLOCK_INERT_CLASS
    };
    rsx! {
        button {
            class,
            r#type: "button",
            title,
            aria_label: title,
            disabled: action.is_none(),
            onclick: move |event| {
                event.stop_propagation();
                if let (Some(action), Some(handler)) = (action.clone(), on_action) {
                    handler.call(action);
                }
            },
            "{label}"
        }
    }
}

/// Release one held channel (panel.md P2): the gold revert glyph.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ResetGlyph(
    target: UiPanelTarget,
    title: &'static str,
    #[props(default)] on_panel: Option<EventHandler<PanelGesture>>,
) -> Element {
    rsx! {
        button {
            class: "tw:inline-flex tw:h-5 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:px-1 tw:text-status-engaged-foreground tw:opacity-70 tw:hover:opacity-100",
            r#type: "button",
            title,
            aria_label: title,
            onclick: move |event| {
                event.stop_propagation();
                if let Some(handler) = on_panel {
                    handler.call(PanelGesture::ClearControl {
                        target: target.clone(),
                    });
                }
            },
            StudioIcon { name: StudioIconName::Revert, size: 11 }
        }
    }
}

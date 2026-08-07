//! The palette chooser: the swatch control's popover (M4 P4).
//!
//! Spike §9 round 3c, rebuilt in the real popover system. Two tabs —
//! **Palette** and **↻ Cycle** — that map 1:1 onto
//! [`GradientConfig::Static`] and [`GradientConfig::Cycle`], which is what
//! dissolves the ambiguous list-click of the earlier rounds: in the Palette
//! tab a click SELECTS, in the Cycle tab a click ADDS. The chooser opens on
//! the tab its current config already is.
//!
//! Every gesture emits the WHOLE config down P3's
//! [`palette_write_action`](super::palette_swatch_field::palette_write_action)
//! path — a panel write when the control is public, a slot edit when it is
//! not. A cycle mutation is no different: the set, the step, and the fade
//! ride together, because they are one value, and the studio actor coalesces
//! the stream a slider produces.
//!
//! A pick in the Palette tab closes the popover (a selection is a completed
//! gesture, the add-node picker's rule); cycle edits do not, because
//! building a set is several gestures in a row.

use dioxus::prelude::*;
use lpa_studio_core::{ProjectSlotAddress, UiAction, UiPanelTarget};
use lpc_model::{Gradient, GradientConfig, MAX_CYCLE_SET};

use crate::base::{GradientStripCanvas, PopoverCloseHandle, StudioIcon, StudioIconName};

use super::palette_catalog::{
    PaletteChoice, PaletteGroup, filter_choices, group_choices, use_palette_catalog,
};
use super::palette_editor::{PaletteEditor, PaletteOrigin};
use super::palette_swatch_field::palette_write_action;

/// Step and fade a static palette is promoted with when the first extra
/// member turns it into a cycle: the same 20 s / 0.5 s a hand-authored cycle
/// starts at, so "add a second palette" produces something that visibly
/// moves without asking for two more decisions first.
const PROMOTED_STEP_SECONDS: f32 = 20.0;
const PROMOTED_FADE_SECONDS: f32 = 0.5;

/// The longest step the speed slider offers — beyond a couple of minutes a
/// cycle is scenery, and the frozen end (0) is the "hold this one" gesture.
const MAX_STEP_SECONDS: f32 = 120.0;

/// The fade presets, spike §9's segmented row.
const FADE_PRESETS: [(&str, f32); 3] = [("cut", 0.0), ("0.3 s", 0.3), ("0.9 s", 0.9)];

/// Which half of the chooser is showing — one tab per [`GradientConfig`]
/// variant, never a third state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaletteChooserTab {
    Palette,
    Cycle,
}

/// Where an edited palette LANDS when the editor says done — always the
/// place the ✎ was pressed, never a new one (P5 scope 2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaletteEditTarget {
    /// The control's single held palette: done writes `Static`.
    Static,
    /// One member of the cycle set: done replaces it in place.
    Member(usize),
}

impl PaletteChooserTab {
    /// The tab a config opens on: the kind it already is.
    #[must_use]
    pub fn for_config(config: &GradientConfig) -> Self {
        match config {
            GradientConfig::Static(_) => Self::Palette,
            GradientConfig::Cycle { .. } => Self::Cycle,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Palette => "Palette",
            Self::Cycle => "\u{21bb} Cycle",
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PaletteChooser(
    /// The EFFECTIVE config the control is showing — every gesture is
    /// expressed as a whole replacement of this, so a set built here grows
    /// from what is playing rather than from the authored default.
    config: GradientConfig,
    /// Backing slot for the slot-local write path.
    #[props(default = None)]
    address: Option<ProjectSlotAddress>,
    /// Panel channel for the public write path.
    #[props(default = None)]
    panel_target: Option<UiPanelTarget>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// Open on a specific tab regardless of the config's kind (stories).
    #[props(default = None)]
    initial_tab: Option<PaletteChooserTab>,
    /// Open straight into the editor on this target (stories) — the state
    /// a ✎ press produces.
    #[props(default = None)]
    initial_edit: Option<PaletteEditTarget>,
) -> Element {
    let mut tab =
        use_signal(|| initial_tab.unwrap_or_else(|| PaletteChooserTab::for_config(&config)));
    let mut query = use_signal(String::new);
    let catalog = use_palette_catalog();
    let choices = catalog.choices();
    let close = try_consume_context::<PopoverCloseHandle>();

    // The takeover: `Some(session)` means the popover shows the editor
    // INSTEAD of the lists, never beside them.
    let mut editing = use_signal(|| {
        initial_edit.and_then(|target| {
            edit_source(&config, target).map(|gradient| PaletteEditSession { target, gradient })
        })
    });

    let emit = {
        let address = address.clone();
        let panel_target = panel_target.clone();
        move |next: GradientConfig| {
            let (Some(address), Some(handler)) = (address.clone(), on_action) else {
                return;
            };
            handler.call(palette_write_action(&panel_target, address, &next));
        }
    };

    // The editor takes the WHOLE popover — spike §9's `ddview-edit`. It
    // writes nothing until done, and done writes exactly once.
    if let Some(session) = editing() {
        let (name, origin) = palette_identity(&session.gradient, &choices);
        let edited_config = config.clone();
        let target = session.target;
        let emit_done = emit;
        return rsx! {
            PaletteEditor {
                gradient: session.gradient,
                name,
                origin,
                on_done: move |gradient: Gradient| {
                    emit_done(with_palette_edited(&edited_config, target, &gradient));
                    editing.set(None);
                },
                on_cancel: move |_| editing.set(None),
            }
        };
    }

    let current = tab();
    let visible = filter_choices(&choices, &query());

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-0",
            // Tabs: the config's two kinds, and nothing else.
            div { class: "tw:flex tw:min-w-0 tw:gap-1 tw:border-b tw:border-border-muted tw:px-2 tw:pt-2",
                for candidate in [PaletteChooserTab::Palette, PaletteChooserTab::Cycle] {
                    button {
                        key: "{candidate:?}",
                        class: tab_class(candidate == current),
                        r#type: "button",
                        title: match candidate {
                            PaletteChooserTab::Palette => "Hold one palette",
                            PaletteChooserTab::Cycle => "Walk a set of palettes in turn",
                        },
                        onclick: move |event| {
                            event.stop_propagation();
                            tab.set(candidate);
                        },
                        "{candidate.label()}"
                    }
                }
            }
            // One search box serves both tabs: the same catalog is being
            // read, only the click's meaning differs.
            div { class: "tw:px-2 tw:py-1.5",
                input {
                    class: "tw:w-full tw:min-w-0 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-xs tw:text-strong-foreground",
                    r#type: "search",
                    placeholder: "Search palettes",
                    value: "{query()}",
                    oninput: move |event| query.set(event.value()),
                }
            }
            if current == PaletteChooserTab::Palette {
                PaletteTabBody {
                    choices: visible,
                    on_pick: move |gradient: Gradient| {
                        emit(GradientConfig::Static(gradient));
                        if let Some(mut close) = close {
                            close.close();
                        }
                    },
                    // A project row's ✎ edits THAT row's palette, and the
                    // result lands as the held value — the Palette tab's
                    // click already means "this is the palette", and the
                    // editor does not change what a tab means.
                    on_edit: move |gradient: Gradient| {
                        editing
                            .set(
                                Some(PaletteEditSession {
                                    target: PaletteEditTarget::Static,
                                    gradient,
                                }),
                            );
                    },
                }
            } else {
                CycleTabBody {
                    config: config.clone(),
                    choices: visible,
                    named: choices.clone(),
                    on_change: emit,
                    on_edit: move |(index, gradient): (usize, Gradient)| {
                        editing
                            .set(
                                Some(PaletteEditSession {
                                    target: PaletteEditTarget::Member(index),
                                    gradient,
                                }),
                            );
                    },
                }
            }
        }
    }
}

/// The Palette tab: the whole catalog, grouped, where a click SELECTS.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PaletteTabBody(
    choices: Vec<PaletteChoice>,
    on_pick: EventHandler<Gradient>,
    on_edit: EventHandler<Gradient>,
) -> Element {
    let groups = group_choices(&choices);
    rsx! {
        div { class: PALETTE_LIST_CLASS,
            if groups.is_empty() {
                p { class: "tw:m-0 tw:px-1 tw:py-2 tw:text-xs tw:text-subtle-foreground",
                    "No palette matches that search."
                }
            }
            for (group , rows) in groups {
                section { key: "{group:?}", class: "tw:grid tw:gap-0.5 tw:pb-1",
                    h4 { class: GROUP_HEADING_CLASS, "{group.label()}" }
                    for choice in rows {
                        PaletteRow {
                            key: "{choice.id}",
                            choice: choice.clone(),
                            // Only a palette this project already owns is
                            // edited from a ROW; a built-in is forked from
                            // wherever it is in use (the ✎ on its chip), so
                            // the catalog list stays a catalog.
                            editable: choice.group == PaletteGroup::ThisProject,
                            on_press: {
                                let gradient = choice.gradient.clone();
                                move |_| on_pick.call(gradient.clone())
                            },
                            on_edit: {
                                let gradient = choice.gradient.clone();
                                move |_| on_edit.call(gradient.clone())
                            },
                        }
                    }
                }
            }
        }
    }
}

/// The Cycle tab: the member set as chips, the same catalog as an add-list,
/// and the two timings that make a set a cycle.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CycleTabBody(
    config: GradientConfig,
    /// The catalog filtered by the search box — the add-list.
    choices: Vec<PaletteChoice>,
    /// The UNFILTERED catalog, used to name the members.
    named: Vec<PaletteChoice>,
    on_change: EventHandler<GradientConfig>,
    /// A member's ✎: the set position and the palette sitting in it.
    on_edit: EventHandler<(usize, Gradient)>,
) -> Element {
    let members: Vec<Gradient> = config.gradients().to_vec();
    let full = members.len() >= MAX_CYCLE_SET as usize;
    let groups = group_choices(&choices);
    let step_seconds = cycle_step_seconds(&config);
    let fade_seconds = cycle_fade_seconds(&config);

    rsx! {
        // The set, in order. A static config shows its single palette here
        // too — adding the second member is what makes it a cycle. Capped
        // and scrollable for the same reason the catalog list is: a full
        // set plus a catalog must not grow the popover past the viewport.
        div { class: "tw:grid tw:max-h-[168px] tw:gap-0.5 tw:overflow-y-auto tw:px-2 tw:pb-1.5",
            for (index , gradient) in members.iter().cloned().enumerate() {
                CycleMemberChip {
                    key: "{index}",
                    name: member_name(&gradient, &named, index),
                    gradient: gradient.clone(),
                    // A one-member "set" has nothing to remove down to.
                    removable: members.len() > 1,
                    on_remove: {
                        let config = config.clone();
                        move |_| on_change.call(with_member_removed(&config, index))
                    },
                    on_edit: {
                        let gradient = gradient.clone();
                        move |_| on_edit.call((index, gradient.clone()))
                    },
                }
            }
        }
        // Timings: the rate in BOTH voices (the P6 gate picks one), and the
        // hand-off.
        div { class: "tw:grid tw:gap-1 tw:border-t tw:border-border-muted tw:px-2 tw:py-1.5",
            div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:justify-between tw:gap-2",
                span { class: CONTROL_LABEL_CLASS, "Step" }
                span { class: "tw:font-mono tw:text-[0.7rem] tw:tabular-nums tw:text-muted-foreground",
                    "{step_readout(step_seconds)}"
                }
            }
            input {
                class: "tw:w-full tw:min-w-0 tw:cursor-pointer",
                r#type: "range",
                min: "0",
                max: "{MAX_STEP_SECONDS}",
                step: "0.5",
                value: "{step_seconds}",
                title: "Seconds each palette holds — drag to 0 to hold the current one",
                oninput: {
                    let config = config.clone();
                    move |event: FormEvent| {
                        if let Ok(next) = event.value().parse::<f32>() {
                            on_change.call(with_step_seconds(&config, next));
                        }
                    }
                },
            }
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:pt-0.5",
                span { class: CONTROL_LABEL_CLASS, "Fade" }
                div { class: "tw:inline-flex tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle",
                    for (label , seconds) in FADE_PRESETS {
                        button {
                            key: "{label}",
                            class: seg_button_class(approx_eq(fade_seconds, seconds)),
                            r#type: "button",
                            title: "Cross-fade at each hand-off",
                            onclick: {
                                let config = config.clone();
                                move |event: MouseEvent| {
                                    event.stop_propagation();
                                    on_change.call(with_fade_seconds(&config, seconds));
                                }
                            },
                            "{label}"
                        }
                    }
                }
            }
        }
        // The add-list. Same rows as the Palette tab; here a click ADDS.
        div { class: "tw:border-t tw:border-border-muted",
            if full {
                p { class: "tw:m-0 tw:px-3 tw:py-1.5 tw:text-[11px] tw:text-status-attention-foreground",
                    "Full set — {MAX_CYCLE_SET} palettes is the most a cycle holds. Remove one to add another."
                }
            }
            div { class: PALETTE_LIST_CLASS,
                for (group , rows) in groups {
                    section { key: "{group:?}", class: "tw:grid tw:gap-0.5 tw:pb-1",
                        h4 { class: GROUP_HEADING_CLASS, "{group.label()}" }
                        for choice in rows {
                            PaletteRow {
                                key: "{choice.id}",
                                choice: choice.clone(),
                                adding: true,
                                disabled: full,
                                // No ✎ in the add-list: here a row means
                                // "put this in the set", and the chip it
                                // becomes is where it is edited.
                                on_edit: move |_| {},
                                on_press: {
                                    let config = config.clone();
                                    let gradient = choice.gradient.clone();
                                    move |_| {
                                        if let Some(next) = with_member_added(&config, &gradient) {
                                            on_change.call(next);
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One catalog row: mini strip, name, and — for a third-party palette — the
/// credit on a second line beneath it, with the full attribution in the row's
/// tooltip.
///
/// The licence used to be a tag pinned to the row's right edge. It moved
/// under the name at the M4 follow-up gate: a licence is not something the
/// person choosing a palette is deciding on, and it took the width a name
/// needs. Nothing legal rides on this row — attribution is pinned to
/// `assets/palettes/third-party/COPYING.md` by ADR
/// `2026-08-04-palette-catalog-licensing-and-isolation` and enforced by
/// `tests/license_manifest.rs` — so the second line is a deliberate credit,
/// not a compliance sticker.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PaletteRow(
    choice: PaletteChoice,
    /// Cycle tab: the row's affordance is "add to the set".
    #[props(default = false)]
    adding: bool,
    #[props(default = false)] disabled: bool,
    /// Whether the row carries a ✎ — true only where editing means editing
    /// THIS palette (a project one), never a catalog entry.
    #[props(default = false)]
    editable: bool,
    on_press: EventHandler<()>,
    on_edit: EventHandler<()>,
) -> Element {
    // Credit, when there is anyone to credit: author and licence on their own
    // dim line under the name. A LightPlayer original or a project palette
    // has nobody to attribute and must not grow a blank second line.
    let credit = choice
        .license
        .as_ref()
        .map(|license| format!("{} · {}", credit_author(&license.author), license.spdx));
    let name = choice.name.clone();
    rsx! {
        // A row with a ✎ is two gestures, so the affordance is a sibling of
        // the row button rather than a button inside one (nested buttons are
        // not a thing the DOM has).
        div { class: "tw:flex tw:min-w-0 tw:items-center",
            button {
                class: palette_row_class(disabled),
                r#type: "button",
                disabled,
                title: choice.title(),
                onclick: move |event| {
                    event.stop_propagation();
                    if !disabled {
                        on_press.call(());
                    }
                },
                span { class: "tw:w-14 tw:flex-none",
                    GradientStripCanvas { gradient: choice.gradient.clone() }
                }
                // The text column is the grower, so every row's strip, name
                // and credit share one left edge and the add affordance sits
                // flush right — the button itself is `w-full` for the same
                // reason, or it would shrink-wrap and take the right edge
                // with it.
                span { class: "tw:grid tw:min-w-0 tw:grow tw:text-left",
                    span { class: "tw:min-w-0 tw:truncate", "{choice.name}" }
                    if let Some(credit) = credit {
                        span { class: "tw:min-w-0 tw:truncate tw:text-[10px] tw:leading-tight tw:text-dim-foreground",
                            "{credit}"
                        }
                    }
                }
                if adding {
                    span { class: "tw:flex tw:flex-none tw:items-center tw:text-subtle-foreground", aria_hidden: "true",
                        StudioIcon { name: StudioIconName::Add, size: 12 }
                    }
                }
            }
            if editable {
                EditPaletteButton {
                    name: name.clone(),
                    disabled,
                    on_edit: move |_| on_edit.call(()),
                }
            }
        }
    }
}

/// The ✎ that swaps the popover for the editor. One component, because the
/// gesture means the same thing from a chip and from a row: take THIS
/// palette into the editor, and land the result where it came from.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditPaletteButton(
    name: String,
    #[props(default = false)] disabled: bool,
    on_edit: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: "tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:p-1 tw:text-subtle-foreground tw:hover:text-strong-foreground tw:disabled:cursor-default tw:disabled:opacity-50",
            r#type: "button",
            disabled,
            title: "Edit this palette",
            aria_label: "Edit {name}",
            onclick: move |event: MouseEvent| {
                event.stop_propagation();
                on_edit.call(());
            },
            StudioIcon { name: StudioIconName::Edited, size: 12 }
        }
    }
}

/// One member of the cycle set: its strip, its name, and the remove that
/// takes it back out.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CycleMemberChip(
    name: String,
    gradient: Gradient,
    removable: bool,
    on_remove: EventHandler<()>,
    on_edit: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:rounded-xs tw:border tw:border-border-subtle tw:px-1.5 tw:py-1",
            span { class: "tw:w-14 tw:flex-none",
                GradientStripCanvas { gradient }
            }
            span { class: "tw:min-w-0 tw:grow tw:truncate tw:text-xs", "{name}" }
            EditPaletteButton { name: name.clone(), on_edit: move |_| on_edit.call(()) }
            if removable {
                button {
                    class: "tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:hover:text-status-error-foreground",
                    r#type: "button",
                    title: "Remove this palette from the cycle",
                    aria_label: "Remove {name}",
                    onclick: move |event| {
                        event.stop_propagation();
                        on_remove.call(());
                    },
                    StudioIcon { name: StudioIconName::Remove, size: 12 }
                }
            }
        }
    }
}

/// Scroll containment: the list is the only thing in the popover that grows
/// with the catalog, so the cap lives here rather than on the panel.
/// Taller than the 210px it was: catalog rows carry a credit line now, so the
/// same cap showed barely half as many palettes. The list still has to leave
/// the popover shorter than a phone viewport.
const PALETTE_LIST_CLASS: &str =
    "tw:grid tw:max-h-[264px] tw:min-w-0 tw:gap-0.5 tw:overflow-y-auto tw:px-2 tw:py-1";

const GROUP_HEADING_CLASS: &str = "tw:m-0 tw:px-1 tw:pt-1 tw:text-[10px] tw:font-bold tw:uppercase tw:tracking-[0.08em] tw:text-subtle-foreground";

const CONTROL_LABEL_CLASS: &str = "tw:text-[0.66rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.08em] tw:text-subtle-foreground";

fn tab_class(active: bool) -> String {
    let base = "tw:cursor-pointer tw:appearance-none tw:border-0 tw:border-b-2 tw:bg-transparent tw:px-2 tw:pb-1.5 tw:text-xs tw:font-bold";
    if active {
        format!("{base} tw:border-border-strong tw:text-strong-foreground")
    } else {
        format!(
            "{base} tw:border-transparent tw:text-subtle-foreground tw:hover:text-soft-foreground"
        )
    }
}

fn palette_row_class(disabled: bool) -> String {
    // `w-full`: the button is a flex ITEM inside the row wrapper, so without
    // it the button shrink-wraps its content and every row ends at a
    // different x — which is what used to leave the licence tags ragged.
    let base = "tw:flex tw:w-full tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2 tw:rounded-xs tw:border-0 tw:bg-transparent tw:px-1 tw:py-1 tw:text-xs tw:text-muted-foreground tw:hover:bg-card-muted tw:hover:text-strong-foreground";
    if disabled {
        format!("{base} tw:cursor-default tw:opacity-55 tw:hover:bg-transparent")
    } else {
        base.to_string()
    }
}

/// The author as a row's second line spells it: the name before any
/// parenthetical. Upstream credits run long ("FastLED (Daniel Garcia, Mark
/// Kriegsman et al.)") and the row has one line to give; the tooltip and
/// `COPYING.md` carry the full text.
fn credit_author(author: &str) -> &str {
    author
        .split_once(" (")
        .map_or(author, |(lead, _)| lead)
        .trim()
}

/// One button of a segmented row (the fade presets, and the editor's space
/// and method segments): active reads as pressed, the rest as quiet text.
pub(crate) fn seg_button_class(active: bool) -> String {
    let base = "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:px-2 tw:py-0.5 tw:text-[11px]";
    if active {
        format!("{base} tw:bg-card-muted tw:font-bold tw:text-strong-foreground")
    } else {
        format!("{base} tw:text-subtle-foreground tw:hover:text-soft-foreground")
    }
}

/// The step a cycle holds each member for; a static palette answers with the
/// promotion default, so the slider has somewhere to start.
fn cycle_step_seconds(config: &GradientConfig) -> f32 {
    match config {
        GradientConfig::Cycle { step_seconds, .. } => *step_seconds,
        GradientConfig::Static(_) => PROMOTED_STEP_SECONDS,
    }
}

fn cycle_fade_seconds(config: &GradientConfig) -> f32 {
    match config {
        GradientConfig::Cycle { fade_seconds, .. } => *fade_seconds,
        GradientConfig::Static(_) => PROMOTED_FADE_SECONDS,
    }
}

/// The step in the P6 gate's Step voice: the plain seconds the slider
/// actually moves through (`every 4 s`), never the reciprocal rate idiom.
/// A frozen step says `held`, because it has no rate.
#[must_use]
pub fn step_readout(step_seconds: f32) -> String {
    if !step_seconds.is_finite() || step_seconds <= 0.0 {
        return "held".to_string();
    }
    format!("every {}", format_seconds(step_seconds))
}

/// Seconds, trimmed: `4 s`, `0.5 s`, `20 s`.
fn format_seconds(seconds: f32) -> String {
    if (seconds - seconds.round()).abs() < 0.05 {
        format!("{} s", seconds.round() as i64)
    } else {
        format!("{seconds:.1} s")
    }
}

/// The member name a chip prints: the catalog's name when the gradient is a
/// catalog (or project) palette, and its position otherwise — an edited or
/// imported ramp still has to be identifiable in the set.
fn member_name(gradient: &Gradient, choices: &[PaletteChoice], index: usize) -> String {
    choices
        .iter()
        .find(|choice| &choice.gradient == gradient)
        .map(|choice| choice.name.clone())
        .unwrap_or_else(|| format!("Palette {}", index + 1))
}

/// Add `gradient` to the set. A static config is PROMOTED to a two-member
/// cycle (that is what "add" means on the Cycle tab); a full set refuses
/// with `None`, which is what the full-set affordance reports.
#[must_use]
pub fn with_member_added(config: &GradientConfig, gradient: &Gradient) -> Option<GradientConfig> {
    match config {
        GradientConfig::Static(current) => Some(GradientConfig::Cycle {
            set: vec![current.clone(), gradient.clone()],
            step_seconds: PROMOTED_STEP_SECONDS,
            fade_seconds: PROMOTED_FADE_SECONDS,
        }),
        GradientConfig::Cycle {
            set,
            step_seconds,
            fade_seconds,
        } => {
            if set.len() >= MAX_CYCLE_SET as usize {
                return None;
            }
            let mut set = set.clone();
            set.push(gradient.clone());
            Some(GradientConfig::Cycle {
                set,
                step_seconds: *step_seconds,
                fade_seconds: *fade_seconds,
            })
        }
    }
}

/// Take the member at `index` out. Removing down to ONE member is not a
/// one-entry cycle — the model's floor is two — so it becomes a static hold
/// of the survivor, which is also what the gesture means.
#[must_use]
pub fn with_member_removed(config: &GradientConfig, index: usize) -> GradientConfig {
    let GradientConfig::Cycle {
        set,
        step_seconds,
        fade_seconds,
    } = config
    else {
        return config.clone();
    };
    let mut set = set.clone();
    if index >= set.len() {
        return config.clone();
    }
    set.remove(index);
    match set.len() {
        0 => config.clone(),
        1 => GradientConfig::Static(set.remove(0)),
        _ => GradientConfig::Cycle {
            set,
            step_seconds: *step_seconds,
            fade_seconds: *fade_seconds,
        },
    }
}

/// One open editor takeover: which palette is on the bar, and where its
/// result lands. Held as a whole so nothing has to re-derive the source from
/// a config that the edit is about to replace.
#[derive(Clone, Debug, PartialEq)]
struct PaletteEditSession {
    target: PaletteEditTarget,
    gradient: Gradient,
}

/// The palette a target currently holds — the editor's starting value when
/// the edit came from the CONFIG (a cycle chip, or the story-opened static
/// value) rather than from a catalog row.
#[must_use]
pub fn edit_source(config: &GradientConfig, target: PaletteEditTarget) -> Option<Gradient> {
    let gradients = config.gradients();
    match target {
        PaletteEditTarget::Static => gradients.first().cloned(),
        PaletteEditTarget::Member(index) => gradients.get(index).cloned(),
    }
}

/// Land an edited palette back where the ✎ was pressed.
///
/// The set's SHAPE never changes here: replacing a member leaves a cycle a
/// cycle with the same timings, and a static edit stays static. Copy-on-use
/// forking needs no branch of its own — the copy simply becomes the authored
/// value at that position, and the catalog is not something this function can
/// reach.
#[must_use]
pub fn with_palette_edited(
    config: &GradientConfig,
    target: PaletteEditTarget,
    gradient: &Gradient,
) -> GradientConfig {
    match (config, target) {
        (_, PaletteEditTarget::Static) | (GradientConfig::Static(_), _) => {
            GradientConfig::Static(gradient.clone())
        }
        (
            GradientConfig::Cycle {
                set,
                step_seconds,
                fade_seconds,
            },
            PaletteEditTarget::Member(index),
        ) => {
            let mut set = set.clone();
            if index >= set.len() {
                // A stale ✎ (the set shrank underneath it) changes nothing
                // rather than appending a member nobody asked for.
                return config.clone();
            }
            set[index] = gradient.clone();
            GradientConfig::Cycle {
                set,
                step_seconds: *step_seconds,
                fade_seconds: *fade_seconds,
            }
        }
    }
}

/// What the editor's title row says about a palette: its name, and whether
/// done will FORK it.
///
/// Provenance is decided by VALUE, not by which list the ✎ was pressed in: a
/// cycle chip holding the shipped `Ocean` is a built-in wherever it sits, and
/// the moment a stop moves it stops being one. Anything the catalog has never
/// heard of — an edited ramp, an import — is already this project's.
#[must_use]
pub fn palette_identity(gradient: &Gradient, choices: &[PaletteChoice]) -> (String, PaletteOrigin) {
    let matched = choices.iter().find(|choice| &choice.gradient == gradient);
    match matched {
        Some(choice) if choice.group == PaletteGroup::ThisProject => {
            (choice.name.clone(), PaletteOrigin::ProjectCustom)
        }
        Some(choice) => (
            choice.name.clone(),
            PaletteOrigin::BuiltinCopy(choice.name.clone()),
        ),
        None => ("Custom palette".to_string(), PaletteOrigin::ProjectCustom),
    }
}

/// Retime the cycle. A static palette has no step to set, so it is returned
/// untouched — the slider only appears with a set behind it.
#[must_use]
pub fn with_step_seconds(config: &GradientConfig, seconds: f32) -> GradientConfig {
    match config {
        GradientConfig::Cycle {
            set, fade_seconds, ..
        } => GradientConfig::Cycle {
            set: set.clone(),
            step_seconds: seconds.max(0.0),
            fade_seconds: *fade_seconds,
        },
        GradientConfig::Static(_) => config.clone(),
    }
}

#[must_use]
pub fn with_fade_seconds(config: &GradientConfig, seconds: f32) -> GradientConfig {
    match config {
        GradientConfig::Cycle {
            set, step_seconds, ..
        } => GradientConfig::Cycle {
            set: set.clone(),
            step_seconds: *step_seconds,
            fade_seconds: seconds.max(0.0),
        },
        GradientConfig::Static(_) => config.clone(),
    }
}

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[cfg(test)]
mod tests {
    use lpc_model::{Colorspace, GradientStop, InterpMethod};

    use super::super::palette_catalog::PaletteGroup;
    use super::*;

    fn ramp(shade: f32) -> Gradient {
        Gradient {
            space: Colorspace::Oklab,
            method: InterpMethod::Linear,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: [0.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [shade, 0.1, 0.1],
                },
            ],
        }
    }

    fn cycle(count: usize) -> GradientConfig {
        GradientConfig::Cycle {
            set: (0..count).map(|i| ramp(i as f32 / 10.0)).collect(),
            step_seconds: 20.0,
            fade_seconds: 0.5,
        }
    }

    #[test]
    fn the_chooser_opens_on_the_kind_the_config_already_is() {
        assert_eq!(
            PaletteChooserTab::for_config(&GradientConfig::Static(ramp(0.5))),
            PaletteChooserTab::Palette
        );
        assert_eq!(
            PaletteChooserTab::for_config(&cycle(3)),
            PaletteChooserTab::Cycle
        );
    }

    #[test]
    fn adding_to_a_static_palette_promotes_it_to_a_two_member_cycle() {
        let promoted = with_member_added(&GradientConfig::Static(ramp(0.1)), &ramp(0.9))
            .expect("a static palette always has room");
        let GradientConfig::Cycle {
            set,
            step_seconds,
            fade_seconds,
        } = promoted
        else {
            panic!("adding a member makes a cycle");
        };
        assert_eq!(set, vec![ramp(0.1), ramp(0.9)]);
        assert_eq!(step_seconds, PROMOTED_STEP_SECONDS);
        assert_eq!(fade_seconds, PROMOTED_FADE_SECONDS);
    }

    #[test]
    fn a_full_set_refuses_the_add() {
        let full = cycle(MAX_CYCLE_SET as usize);
        assert!(with_member_added(&full, &ramp(0.99)).is_none());
        // One under the cap still takes one.
        let room = cycle(MAX_CYCLE_SET as usize - 1);
        assert!(with_member_added(&room, &ramp(0.99)).is_some());
    }

    #[test]
    fn removing_down_to_one_member_becomes_a_static_hold_of_the_survivor() {
        let two = GradientConfig::Cycle {
            set: vec![ramp(0.2), ramp(0.8)],
            step_seconds: 20.0,
            fade_seconds: 0.5,
        };
        assert_eq!(
            with_member_removed(&two, 0),
            GradientConfig::Static(ramp(0.8)),
            "the model's floor is two members, so one member is a held palette"
        );

        // Above the floor the set just shrinks, timings intact.
        let three = cycle(3);
        let GradientConfig::Cycle {
            set, step_seconds, ..
        } = with_member_removed(&three, 1)
        else {
            panic!("three minus one is still a cycle");
        };
        assert_eq!(set.len(), 2);
        assert_eq!(step_seconds, 20.0);

        // An out-of-range index (a stale click) changes nothing.
        assert_eq!(with_member_removed(&three, 9), three);
    }

    #[test]
    fn a_row_credits_the_author_without_the_parenthetical() {
        assert_eq!(
            credit_author("FastLED (Daniel Garcia, Mark Kriegsman et al.)"),
            "FastLED",
            "the row has one line to give; the full credit is in the tooltip"
        );
        assert_eq!(credit_author("Blackheartedwolf"), "Blackheartedwolf");
        assert_eq!(credit_author(""), "");
    }

    #[test]
    fn timing_edits_carry_the_whole_config() {
        let retimed = with_step_seconds(&cycle(3), 4.0);
        let GradientConfig::Cycle {
            set,
            step_seconds,
            fade_seconds,
        } = retimed
        else {
            panic!("retiming keeps the kind");
        };
        assert_eq!(set.len(), 3, "the set rides along with every mutation");
        assert_eq!(step_seconds, 4.0);
        assert_eq!(fade_seconds, 0.5);

        let faded = with_fade_seconds(&cycle(3), 0.9);
        assert_eq!(cycle_fade_seconds(&faded), 0.9);
        // Negative input (a bad parse) is clamped, never stored.
        assert_eq!(cycle_step_seconds(&with_step_seconds(&cycle(2), -3.0)), 0.0);
    }

    #[test]
    fn the_step_readout_speaks_plain_seconds() {
        assert_eq!(step_readout(4.0), "every 4 s");
        assert_eq!(step_readout(0.5), "every 0.5 s");
        assert_eq!(step_readout(20.0), "every 20 s");
        // Frozen has no rate to state.
        assert_eq!(step_readout(0.0), "held");
        assert_eq!(step_readout(f32::NAN), "held");
    }

    #[test]
    fn the_editor_opens_on_the_palette_the_target_holds() {
        let held = GradientConfig::Static(ramp(0.3));
        assert_eq!(
            edit_source(&held, PaletteEditTarget::Static),
            Some(ramp(0.3))
        );

        let three = cycle(3);
        assert_eq!(
            edit_source(&three, PaletteEditTarget::Member(1)),
            Some(ramp(0.1))
        );
        // A stale ✎ (the set shrank underneath it) opens nothing.
        assert_eq!(edit_source(&three, PaletteEditTarget::Member(9)), None);
    }

    #[test]
    fn done_lands_the_edit_where_the_pencil_was_pressed() {
        // A static edit stays static.
        let held = GradientConfig::Static(ramp(0.3));
        assert_eq!(
            with_palette_edited(&held, PaletteEditTarget::Static, &ramp(0.9)),
            GradientConfig::Static(ramp(0.9))
        );

        // A member edit replaces exactly that member, timings intact.
        let three = cycle(3);
        let GradientConfig::Cycle {
            set,
            step_seconds,
            fade_seconds,
        } = with_palette_edited(&three, PaletteEditTarget::Member(1), &ramp(0.9))
        else {
            panic!("replacing a member keeps the cycle a cycle");
        };
        assert_eq!(set, vec![ramp(0.0), ramp(0.9), ramp(0.2)]);
        assert_eq!(step_seconds, 20.0);
        assert_eq!(fade_seconds, 0.5);

        // A stale target changes nothing rather than appending.
        assert_eq!(
            with_palette_edited(&three, PaletteEditTarget::Member(9), &ramp(0.9)),
            three
        );
    }

    #[test]
    fn provenance_is_decided_by_value_not_by_list() {
        let choices = vec![
            PaletteChoice {
                id: "ocean".to_string(),
                name: "Ocean".to_string(),
                group: PaletteGroup::FastledStock,
                license: None,
                gradient: ramp(0.4),
            },
            PaletteChoice {
                id: "mine".to_string(),
                name: "Mine".to_string(),
                group: PaletteGroup::ThisProject,
                license: None,
                gradient: ramp(0.6),
            },
        ];

        // A shipped palette forks — wherever the ✎ was pressed.
        assert_eq!(
            palette_identity(&ramp(0.4), &choices),
            (
                "Ocean".to_string(),
                PaletteOrigin::BuiltinCopy("Ocean".to_string())
            )
        );
        // A project palette edits in place.
        assert_eq!(
            palette_identity(&ramp(0.6), &choices),
            ("Mine".to_string(), PaletteOrigin::ProjectCustom)
        );
        // A value the catalog never heard of is already this project's.
        assert_eq!(
            palette_identity(&ramp(0.9), &choices),
            ("Custom palette".to_string(), PaletteOrigin::ProjectCustom)
        );
    }

    #[test]
    fn members_take_their_names_from_the_catalog_when_they_are_in_it() {
        let choices = vec![PaletteChoice {
            id: "ocean".to_string(),
            name: "Ocean".to_string(),
            group: PaletteGroup::FastledStock,
            license: None,
            gradient: ramp(0.4),
        }];
        assert_eq!(member_name(&ramp(0.4), &choices, 0), "Ocean");
        assert_eq!(member_name(&ramp(0.7), &choices, 2), "Palette 3");
    }
}

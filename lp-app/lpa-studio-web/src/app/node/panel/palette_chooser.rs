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
use lpa_studio_core::{ProjectSlotAddress, UiAction, UiPanelTarget, phasor_rate_display};
use lpc_model::{Gradient, GradientConfig, MAX_CYCLE_SET};

use crate::base::{GradientStripCanvas, PopoverCloseHandle, StudioIcon, StudioIconName};

use super::palette_catalog::{PaletteChoice, filter_choices, group_choices, use_palette_catalog};
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
    /// The config the control currently holds — every gesture is expressed
    /// as a whole replacement of this.
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
) -> Element {
    let mut tab =
        use_signal(|| initial_tab.unwrap_or_else(|| PaletteChooserTab::for_config(&config)));
    let mut query = use_signal(String::new);
    let catalog = use_palette_catalog();
    let choices = catalog.choices();
    let close = try_consume_context::<PopoverCloseHandle>();

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
                }
            } else {
                CycleTabBody {
                    config: config.clone(),
                    choices: visible,
                    named: choices.clone(),
                    on_change: emit,
                }
            }
        }
    }
}

/// The Palette tab: the whole catalog, grouped, where a click SELECTS.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PaletteTabBody(choices: Vec<PaletteChoice>, on_pick: EventHandler<Gradient>) -> Element {
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
                            on_press: move |_| on_pick.call(choice.gradient.clone()),
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
                }
            }
        }
        // Timings: the rate in BOTH voices (the P6 gate picks one), and the
        // hand-off.
        div { class: "tw:grid tw:gap-1 tw:border-t tw:border-border-muted tw:px-2 tw:py-1.5",
            div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:justify-between tw:gap-2",
                span { class: CONTROL_LABEL_CLASS, "Speed" }
                span { class: "tw:font-mono tw:text-[0.7rem] tw:tabular-nums tw:text-muted-foreground",
                    "{speed_readout(step_seconds)}"
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
                            class: fade_preset_class(approx_eq(fade_seconds, seconds)),
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

/// One catalog row: mini strip, name, and — for a third-party palette — its
/// license tag, with author and source in the row's tooltip.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PaletteRow(
    choice: PaletteChoice,
    /// Cycle tab: the row's affordance is "add to the set".
    #[props(default = false)]
    adding: bool,
    #[props(default = false)] disabled: bool,
    on_press: EventHandler<()>,
) -> Element {
    let spdx = choice.license.as_ref().map(|license| license.spdx.clone());
    rsx! {
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
            span { class: "tw:min-w-0 tw:truncate tw:text-left", "{choice.name}" }
            if let Some(spdx) = spdx {
                span { class: "tw:ml-auto tw:flex-none tw:rounded-xs tw:border tw:border-border-muted tw:px-1 tw:text-[10px] tw:leading-tight tw:text-dim-foreground",
                    "{spdx}"
                }
            }
            if adding {
                span { class: "tw:ml-auto tw:flex tw:flex-none tw:items-center tw:text-subtle-foreground", aria_hidden: "true",
                    StudioIcon { name: StudioIconName::Add, size: 12 }
                }
            }
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
) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:rounded-xs tw:border tw:border-border-subtle tw:px-1.5 tw:py-1",
            span { class: "tw:w-14 tw:flex-none",
                GradientStripCanvas { gradient }
            }
            span { class: "tw:min-w-0 tw:grow tw:truncate tw:text-xs", "{name}" }
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
const PALETTE_LIST_CLASS: &str =
    "tw:grid tw:max-h-[210px] tw:min-w-0 tw:gap-0.5 tw:overflow-y-auto tw:px-2 tw:py-1";

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
    let base = "tw:flex tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2 tw:rounded-xs tw:border-0 tw:bg-transparent tw:px-1 tw:py-1 tw:text-xs tw:text-muted-foreground tw:hover:bg-card-muted tw:hover:text-strong-foreground";
    if disabled {
        format!("{base} tw:cursor-default tw:opacity-55 tw:hover:bg-transparent")
    } else {
        base.to_string()
    }
}

fn fade_preset_class(active: bool) -> String {
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

/// The step rate in BOTH voices — the unit-aware rate every other periodic
/// reading in Studio uses, and the raw seconds it is derived from
/// (`15/min · 4 s`). The P6 gate picks one; until then a reader can check
/// one against the other. A frozen step says `held`, because it has no rate.
#[must_use]
pub fn speed_readout(step_seconds: f32) -> String {
    if !step_seconds.is_finite() || step_seconds <= 0.0 {
        return "held".to_string();
    }
    format!(
        "{} \u{b7} {}",
        phasor_rate_display(step_seconds),
        format_seconds(step_seconds)
    )
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
    fn the_speed_readout_speaks_both_voices() {
        assert_eq!(speed_readout(4.0), "15/min \u{b7} 4 s");
        assert_eq!(speed_readout(0.5), "2/s \u{b7} 0.5 s");
        assert_eq!(speed_readout(20.0), "3/min \u{b7} 20 s");
        // Frozen has no rate to state.
        assert_eq!(speed_readout(0.0), "held");
        assert_eq!(speed_readout(f32::NAN), "held");
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

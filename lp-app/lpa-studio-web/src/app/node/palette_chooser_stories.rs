//! Stories for the palette chooser — the swatch control's popover (M4 P4).
//!
//! The design questions these answer: does the anchored popover really read
//! as "diving into the control" (one merged outline joining band and panel),
//! do the two tabs make the config's two kinds obvious, and does the Cycle
//! tab's tray — chips, timings, add-list — stay legible at a full set.
//!
//! The catalog is FAKED here (context-injected, the binding picker's
//! precedent), so a capture never moves when a palette is added to
//! `lpa-palettes`, and the "This project" section can be shown both full and
//! empty on demand.

use dioxus::prelude::*;
use lpa_studio_core::UiSlotFieldState;
use lpa_studio_web_story_macros::story;
use lpc_model::{Colorspace, Gradient, GradientConfig, GradientStop, InterpMethod};

use crate::app::node::face_story_fixtures::palette_swatch_control;
use crate::app::node::{
    PaletteCatalog, PaletteChoice, PaletteChooserTab, PaletteEditTarget, PaletteGroup,
    PaletteSwatchField,
};

/// A two-stop Oklab ramp — enough to be visually distinct in a mini strip
/// without dragging real catalog data into a story fixture.
fn ramp(lightness: f32, a: f32, b: f32) -> Gradient {
    Gradient {
        space: Colorspace::Oklab,
        method: InterpMethod::Linear,
        stops: vec![
            GradientStop {
                at: 0.0,
                c: [0.08, a * 0.2, b * 0.2],
            },
            GradientStop {
                at: 1.0,
                c: [lightness, a, b],
            },
        ],
    }
}

fn choice(
    name: &str,
    group: PaletteGroup,
    gradient: Gradient,
    spdx: Option<&str>,
) -> PaletteChoice {
    PaletteChoice {
        id: name.to_lowercase().replace(' ', "_"),
        name: name.to_string(),
        group,
        license: spdx.map(|spdx| lpa_palettes::PaletteLicense {
            spdx: spdx.to_string(),
            author: "FastLED".to_string(),
            source_url: "https://github.com/FastLED/FastLED".to_string(),
        }),
        gradient,
    }
}

/// The faked catalog every story here reads: a project section, the two
/// third-party groups (so the SPDX tag is on screen), and an original.
fn story_catalog(project: Vec<PaletteChoice>) -> PaletteCatalog {
    PaletteCatalog {
        project,
        catalog: Some(vec![
            choice(
                "Ocean",
                PaletteGroup::FastledStock,
                ramp(0.72, -0.09, -0.12),
                Some("MIT"),
            ),
            choice(
                "Lava",
                PaletteGroup::FastledStock,
                ramp(0.68, 0.16, 0.12),
                Some("MIT"),
            ),
            choice(
                "Forest",
                PaletteGroup::FastledStock,
                ramp(0.66, -0.13, 0.09),
                Some("MIT"),
            ),
            choice(
                "Rainfall on a very long afternoon",
                PaletteGroup::CptCity,
                ramp(0.70, -0.04, -0.10),
                Some("CC-BY-3.0"),
            ),
            choice(
                "Dusk",
                PaletteGroup::LightplayerOriginal,
                ramp(0.60, 0.05, -0.14),
                None,
            ),
            choice(
                "Ember",
                PaletteGroup::LightplayerOriginal,
                ramp(0.64, 0.13, 0.10),
                None,
            ),
        ]),
    }
}

fn project_rows() -> Vec<PaletteChoice> {
    vec![
        choice(
            "Dome wash",
            PaletteGroup::ThisProject,
            ramp(0.74, 0.02, 0.15),
            None,
        ),
        choice(
            "Aurora 1",
            PaletteGroup::ThisProject,
            ramp(0.70, -0.12, 0.02),
            None,
        ),
    ]
}

/// The story frame: room below the control for the popover to grow into,
/// and the card width a panel row really has.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChooserStoryCard(catalog: PaletteCatalog, children: Element) -> Element {
    let mut provided = use_context_provider(|| Signal::new(catalog.clone()));
    if *provided.peek() != catalog {
        provided.set(catalog);
    }
    rsx! {
        div { class: "tw:grid tw:min-h-[560px] tw:w-full tw:max-w-[420px] tw:content-start tw:gap-2 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6",
            {children}
        }
    }
}

/// One swatch control wired for the chooser, with the panel row's label
/// above it (the label's own detail popover is P3's story, not this one).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChooserRow(
    config: GradientConfig,
    #[props(default = None)] tab: Option<PaletteChooserTab>,
    /// Open straight into the editor takeover — the state a ✎ press
    /// produces.
    #[props(default = None)]
    edit: Option<PaletteEditTarget>,
) -> Element {
    let control = palette_swatch_control("Palette", &config, UiSlotFieldState::editable(), false);
    rsx! {
        span { class: "tw:text-[0.66rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.08em] tw:text-subtle-foreground",
            "Palette"
        }
        PaletteSwatchField {
            config,
            state: UiSlotFieldState::editable(),
            address: control.address.clone(),
            on_action: move |_| {},
            chooser_initially_open: true,
            chooser_initial_tab: tab,
            chooser_initial_edit: edit,
        }
    }
}

fn cycle_of(count: usize) -> GradientConfig {
    GradientConfig::Cycle {
        set: (0..count)
            .map(|index| {
                let t = index as f32 / count as f32;
                ramp(0.6 + t * 0.15, 0.16 - t * 0.28, 0.12 - t * 0.24)
            })
            .collect(),
        step_seconds: 4.0,
        fade_seconds: 0.3,
    }
}

#[story(
    description = "The chooser open on the PALETTE tab, anchored on the swatch: one merged outline welds the band to the panel below it, so opening reads as diving into the control rather than a menu appearing near it. The list leads with `This project` — the distinct palettes already authored in this project's graph — then the shipped catalog by provenance, where a third-party row carries its license tag and keeps author and source in its tooltip. A click here SELECTS: it writes `GradientConfig::Static` and closes, because a selection is a completed gesture."
)]
fn palette_tab() -> Element {
    rsx! {
        ChooserStoryCard { catalog: story_catalog(project_rows()),
            ChooserRow {
                config: GradientConfig::Static(ramp(0.72, -0.09, -0.12)),
                tab: PaletteChooserTab::Palette,
            }
        }
    }
}

#[story(
    description = "The CYCLE tab: the member set as chips (each removable — removing down to one member is not a one-entry cycle, it becomes a held palette of the survivor), the two timings that make a set a cycle, and the same catalog as an add-list where a click ADDS instead of selecting. The speed readout deliberately says BOTH voices — `15/min · 4 s` — until the P6 gate picks one. Every gesture emits the whole config; the actor coalesces what a slider produces."
)]
fn cycle_tab() -> Element {
    rsx! {
        ChooserStoryCard { catalog: story_catalog(project_rows()),
            ChooserRow { config: cycle_of(3), tab: PaletteChooserTab::Cycle }
        }
    }
}

#[story(
    description = "A full set: eight palettes is the most a cycle holds, so the add-list states the ceiling and every row goes inert rather than silently refusing the click. Long names truncate in their row and stay whole in the tooltip; the list scrolls inside the popover instead of growing it past the viewport."
)]
fn full_set() -> Element {
    rsx! {
        ChooserStoryCard { catalog: story_catalog(project_rows()),
            ChooserRow { config: cycle_of(8), tab: PaletteChooserTab::Cycle }
        }
    }
}

#[story(
    description = "The editor TAKEOVER on a shipped built-in (M4-P5): the ✎ swapped the whole popover for the gradient editor — one popover, two views, never a popup over a popup. The provenance line says `copy of built-in \u{201c}Ocean\u{201d}` because done lands a COPY as the slot's authored value and the catalog entry is untouched; the first stop is selected, so its handle rings and the stop row below speaks display sRGB for exactly that stop. Nothing is written until done — a stop drag must not put half-built ramps on a live channel."
)]
fn editor_takeover_builtin_copy() -> Element {
    rsx! {
        ChooserStoryCard { catalog: story_catalog(project_rows()),
            ChooserRow {
                config: GradientConfig::Static(ramp(0.72, -0.09, -0.12)),
                edit: PaletteEditTarget::Static,
            }
        }
    }
}

#[story(
    description = "The same takeover on a palette this project already owns: the provenance line says `project custom` and done edits it in place — no fork, because there is no shipped entry to protect. Provenance is decided by VALUE, not by which list the ✎ was pressed in. (A true mid-drag frame is not capturable — the drag lives in pointer capture — so the selected-stop state above stands in for the editor's working anatomy.)"
)]
fn editor_takeover_project_custom() -> Element {
    rsx! {
        ChooserStoryCard { catalog: story_catalog(project_rows()),
            ChooserRow {
                config: GradientConfig::Static(ramp(0.74, 0.02, 0.15)),
                edit: PaletteEditTarget::Static,
            }
        }
    }
}

#[story(
    description = "Editing one MEMBER of a cycle: the ✎ on the second chip opened its palette, and done replaces exactly that member — same set order, same timings, still a cycle. These generated ramps are values the catalog has never heard of, so the title falls back to `Custom palette` / `project custom` — the identity a hand-built or imported member gets; a catalog member would carry its own name, as the built-in-copy story shows."
)]
fn editor_takeover_cycle_member() -> Element {
    rsx! {
        ChooserStoryCard { catalog: story_catalog(project_rows()),
            ChooserRow { config: cycle_of(3), edit: PaletteEditTarget::Member(1) }
        }
    }
}

/// One voice's column in the rate-language decision render: the timing row
/// as the Cycle tab would wear it, at the three telling rates.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RateVoiceColumn(heading: String, label: String, readouts: Vec<(String, String)>) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-2 tw:rounded-sm tw:border tw:border-border-muted tw:p-2",
            span { class: "tw:text-[10px] tw:font-bold tw:uppercase tw:tracking-[0.08em] tw:text-subtle-foreground",
                "{heading}"
            }
            for (case , readout) in readouts {
                div { class: "tw:grid tw:gap-0.5",
                    span { class: "tw:text-[9px] tw:uppercase tw:tracking-[0.06em] tw:text-dim-foreground",
                        "{case}"
                    }
                    div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:justify-between tw:gap-2",
                        span { class: "tw:text-[0.66rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.08em] tw:text-subtle-foreground",
                            "{label}"
                        }
                        span { class: "tw:font-mono tw:text-[0.7rem] tw:tabular-nums tw:text-muted-foreground",
                            "{readout}"
                        }
                    }
                    input {
                        class: "tw:w-full tw:min-w-0 tw:cursor-pointer",
                        r#type: "range",
                        disabled: true,
                    }
                }
            }
        }
    }
}

#[story(
    description = "GATE DECISION — the cycle rate's language, both voices side by side on the same three rates (a quick 0.5 s step, the 4 s default, a slow 20 s, and frozen). SPEED speaks the auto-denominated rate idiom every periodic reading in Studio uses (`2/s`, `15/min`) — same knob language as the phasor, whose Speed wording is still PROVISIONAL and is settled by this same answer. STEP speaks the seconds the slider actually moves through (`every 4 s`). The shipped control currently hedges with both (`15/min · 4 s`); the gate picks the one that leads."
)]
fn rate_language() -> Element {
    let speed_cases = [
        (0.5f32, "quick"),
        (4.0, "default"),
        (20.0, "slow"),
        (0.0, "frozen"),
    ];
    let speed: Vec<(String, String)> = speed_cases
        .iter()
        .map(|&(seconds, case)| {
            let readout = if seconds > 0.0 {
                lpa_studio_core::phasor_rate_display(seconds)
            } else {
                "held".to_string()
            };
            (format!("{case} ({seconds} s step)"), readout)
        })
        .collect();
    let step: Vec<(String, String)> = speed_cases
        .iter()
        .map(|&(seconds, case)| {
            let readout = if seconds > 0.0 {
                if seconds < 1.0 {
                    format!("every {seconds} s")
                } else {
                    format!("every {} s", seconds as i64)
                }
            } else {
                "held".to_string()
            };
            (format!("{case} ({seconds} s step)"), readout)
        })
        .collect();
    rsx! {
        div { class: "tw:grid tw:w-full tw:max-w-[560px] tw:grid-cols-2 tw:gap-3 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6",
            RateVoiceColumn {
                heading: "A — Speed (rate idiom)".to_string(),
                label: "Speed".to_string(),
                readouts: speed,
            }
            RateVoiceColumn {
                heading: "B — Step (plain seconds)".to_string(),
                label: "Step".to_string(),
                readouts: step,
            }
        }
    }
}

#[story(
    description = "A project nobody has authored a palette in yet: the `This project` heading is simply absent rather than an empty section, and the catalog carries the whole list."
)]
fn empty_project_section() -> Element {
    rsx! {
        ChooserStoryCard { catalog: story_catalog(Vec::new()),
            ChooserRow {
                config: GradientConfig::Static(ramp(0.64, 0.13, 0.10)),
                tab: PaletteChooserTab::Palette,
            }
        }
    }
}

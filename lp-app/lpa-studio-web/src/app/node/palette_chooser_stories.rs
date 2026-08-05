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
    PaletteCatalog, PaletteChoice, PaletteChooserTab, PaletteGroup, PaletteSwatchField,
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

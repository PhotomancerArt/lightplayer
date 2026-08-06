//! `GradientStripCanvas` stories: interpolation methods and the
//! `second`/`mix` cross-fade blend.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;
use lpc_model::{Colorspace, Gradient, GradientStop, InterpMethod};

use crate::base::GradientStripCanvas;

fn sunset() -> Gradient {
    Gradient {
        space: Colorspace::Oklab,
        method: InterpMethod::Linear,
        stops: vec![
            GradientStop {
                at: 0.0,
                c: [0.15, 0.05, -0.1],
            },
            GradientStop {
                at: 0.5,
                c: [0.65, 0.15, 0.1],
            },
            GradientStop {
                at: 1.0,
                c: [0.95, -0.02, 0.12],
            },
        ],
    }
}

fn ocean() -> Gradient {
    Gradient {
        space: Colorspace::Oklab,
        method: InterpMethod::Linear,
        stops: vec![
            GradientStop {
                at: 0.0,
                c: [0.1, -0.02, -0.12],
            },
            GradientStop {
                at: 1.0,
                c: [0.85, -0.1, -0.02],
            },
        ],
    }
}

fn swatches() -> Gradient {
    Gradient {
        space: Colorspace::Srgb,
        method: InterpMethod::Step,
        stops: vec![
            GradientStop {
                at: 0.0,
                c: [0.9, 0.2, 0.2],
            },
            GradientStop {
                at: 0.33,
                c: [0.95, 0.85, 0.1],
            },
            GradientStop {
                at: 0.66,
                c: [0.15, 0.55, 0.9],
            },
        ],
    }
}

#[story(
    description = "GradientStripCanvas across interpolation methods: a smooth Oklab ramp renders as a soft gradient, a Step gradient renders as hard pixelated bands."
)]
fn methods() -> Element {
    rsx! {
        div { class: "tw:flex tw:w-80 tw:flex-col tw:gap-3 tw:p-3",
            div { class: "tw:flex tw:flex-col tw:gap-1",
                span { class: "tw:text-xs tw:text-subtle-foreground", "Linear (Oklab sunset)" }
                GradientStripCanvas { gradient: sunset() }
            }
            div { class: "tw:flex tw:flex-col tw:gap-1",
                span { class: "tw:text-xs tw:text-subtle-foreground", "Step (3 swatches)" }
                GradientStripCanvas { gradient: swatches() }
            }
        }
    }
}

#[story(
    description = "GradientStripCanvas cross-fading from one gradient toward a second by `mix` — the from→to blend later widgets will scrub."
)]
fn blend() -> Element {
    let steps = [0.0_f32, 0.25, 0.5, 0.75, 1.0];
    rsx! {
        div { class: "tw:flex tw:w-80 tw:flex-col tw:gap-3 tw:p-3",
            for mix in steps {
                div { class: "tw:flex tw:flex-col tw:gap-1",
                    span { class: "tw:text-xs tw:text-subtle-foreground", "mix {mix}" }
                    GradientStripCanvas { gradient: sunset(), second: ocean(), mix }
                }
            }
        }
    }
}

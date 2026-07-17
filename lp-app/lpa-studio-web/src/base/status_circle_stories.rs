//! Status-circle stories: the full shape × tone grammar.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{StatusCircle, StatusCircleShape, StatusCircleTone};

#[story(description = "Every shape × tone pairing of the roster status circle.")]
fn shape_grammar() -> Element {
    let shapes = [
        ("Solid (live)", StatusCircleShape::Solid),
        ("Hollow (remembered)", StatusCircleShape::Hollow),
        ("Pulsing (working)", StatusCircleShape::Pulsing),
    ];
    let tones = [
        ("Neutral", StatusCircleTone::Neutral),
        ("Working", StatusCircleTone::Working),
        ("Good", StatusCircleTone::Good),
        ("Warning", StatusCircleTone::Warning),
        ("Error", StatusCircleTone::Error),
    ];

    rsx! {
        div { class: "tw:grid tw:gap-2 tw:p-4",
            div { class: "tw:grid tw:grid-cols-[160px_repeat(5,72px)] tw:items-center tw:gap-2",
                span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "Shape" }
                for (tone_label, _) in tones {
                    span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{tone_label}" }
                }
            }
            for (shape_label, shape) in shapes {
                div { class: "tw:grid tw:grid-cols-[160px_repeat(5,72px)] tw:items-center tw:gap-2",
                    span { class: "tw:text-xs tw:font-bold tw:text-strong-foreground", "{shape_label}" }
                    for (_, tone) in tones {
                        span { class: "tw:inline-flex tw:justify-center",
                            StatusCircle { shape, tone }
                        }
                    }
                }
            }
        }
    }
}

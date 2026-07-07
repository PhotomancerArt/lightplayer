//! Stories for the shared detail-popover base.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{DetailPopover, DetailSection, DetailSectionTint, IconMenuTone, StudioIconName};

#[story(
    description = "The shared detail card open over its trigger: `DetailSection`s in every status tint — an affordance-tinted section wears its color on the TITLE (with a right-aligned meta cell), while the untitled identity-style section and the plain titled section keep the standard heading treatment."
)]
pub(crate) fn open_sections() -> Element {
    let tints = [
        ("Good", DetailSectionTint::Good),
        ("Working", DetailSectionTint::Working),
        ("Warning", DetailSectionTint::Warning),
        ("Error", DetailSectionTint::Error),
        ("Live", DetailSectionTint::Live),
    ];

    rsx! {
        div { class: "tw:flex tw:min-h-[620px] tw:justify-end",
            DetailPopover {
                icon: StudioIconName::Info,
                label: "Detail popover",
                tone: IconMenuTone::Neutral,
                active: true,
                initially_open: true,
                DetailSection {
                    p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-muted-foreground",
                        "An untitled section: padding and divider only."
                    }
                }
                DetailSection { title: "Plain",
                    p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-muted-foreground",
                        "A titled section without an affordance keeps the heading color."
                    }
                }
                for (label, tint) in tints {
                    DetailSection { title: "{label}", meta: "2", tint,
                        p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-muted-foreground",
                            "The {label} tint on the title, with the section wash."
                        }
                    }
                }
            }
        }
    }
}

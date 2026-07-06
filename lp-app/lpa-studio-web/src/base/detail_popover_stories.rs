//! Stories for the shared detail-popover base.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{
    DetailPopover, DetailSectionTint, IconMenuTone, StudioIconName, detail_popover_section_class,
};

#[story(
    description = "The shared detail card open over its trigger: standard sections in every status tint."
)]
pub(crate) fn open_sections() -> Element {
    let tints = [
        ("Plain", DetailSectionTint::None),
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
                for (label, tint) in tints {
                    section { class: detail_popover_section_class(tint),
                        h3 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "{label}" }
                        p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-muted-foreground",
                            "Section content on the shared detail card."
                        }
                    }
                }
            }
        }
    }
}

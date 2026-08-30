//! Inline-button stories: the tone × shape × state matrix for the one
//! shared small-action-button family.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{InlineButton, InlineButtonTone, StudioIconName};

#[story(
    description = "The shared small inline action button: colored tone border, toned glyph, dark background — every tone in icon, icon+text, text, and disabled shapes."
)]
fn tones() -> Element {
    let tones = [
        (
            "Action (default)",
            InlineButtonTone::Action,
            StudioIconName::Add,
        ),
        ("Neutral", InlineButtonTone::Neutral, StudioIconName::Cancel),
        ("Bound", InlineButtonTone::Bound, StudioIconName::Bus),
        ("Warning", InlineButtonTone::Warning, StudioIconName::Revert),
        (
            "Attention",
            InlineButtonTone::Attention,
            StudioIconName::Revert,
        ),
        (
            "Error",
            InlineButtonTone::Error,
            StudioIconName::StatusError,
        ),
        ("Good", InlineButtonTone::Good, StudioIconName::StepComplete),
        (
            "Working",
            InlineButtonTone::Working,
            StudioIconName::StatusRunning,
        ),
        ("Live", InlineButtonTone::Live, StudioIconName::MapLive),
    ];

    rsx! {
        div { class: "tw:grid tw:w-fit tw:grid-cols-[auto_auto_auto_auto_auto] tw:items-center tw:gap-x-4 tw:gap-y-2",
            for (name , tone , icon) in tones {
                span { class: "tw:text-xs tw:text-subtle-foreground", "{name}" }
                InlineButton {
                    label: "{name} action",
                    icon,
                    tone,
                    on_press: |_| {},
                }
                InlineButton {
                    label: "{name} action",
                    icon,
                    text: "label",
                    tone,
                    on_press: |_| {},
                }
                InlineButton {
                    label: "{name} action",
                    text: "text only",
                    tone,
                    on_press: |_| {},
                }
                InlineButton {
                    label: "{name} action (disabled)",
                    icon,
                    tone,
                    disabled: true,
                    on_press: |_| {},
                }
            }
        }
    }
}

#[story(description = "Inline buttons in row context: the gesture cluster a map slot row renders.")]
fn in_a_row() -> Element {
    rsx! {
        div { class: "tw:grid tw:w-80 tw:gap-2 tw:rounded-sm tw:border tw:border-border tw:bg-card tw:p-3",
            div { class: "tw:flex tw:items-center tw:gap-1.5",
                span { class: "tw:min-w-0 tw:flex-1 tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground",
                    "paths"
                }
                InlineButton {
                    label: "Add entry at key 3",
                    icon: StudioIconName::Add,
                    on_press: |_| {},
                }
                InlineButton {
                    label: "Add entry at a chosen key",
                    text: "at key\u{2026}",
                    on_press: |_| {},
                }
            }
            div { class: "tw:flex tw:items-center tw:gap-1.5",
                span { class: "tw:min-w-0 tw:flex-1 tw:truncate tw:text-sm tw:text-muted-foreground",
                    "brightness"
                }
                InlineButton {
                    label: "Revert",
                    icon: StudioIconName::Revert,
                    tone: InlineButtonTone::Warning,
                    on_press: |_| {},
                }
                InlineButton {
                    label: "Clear the optional value",
                    icon: StudioIconName::Remove,
                    on_press: |_| {},
                }
            }
            div { class: "tw:flex tw:items-center tw:gap-1.5",
                span { class: "tw:min-w-0 tw:flex-1 tw:truncate tw:text-sm tw:text-muted-foreground",
                    "speed"
                }
                InlineButton {
                    label: "Author a binding from this slot to a bus channel",
                    text: "Bind\u{2026}",
                    tone: InlineButtonTone::Bound,
                    on_press: |_| {},
                }
                InlineButton {
                    label: "Set the optional value (unavailable)",
                    icon: StudioIconName::Add,
                    disabled: true,
                    on_press: |_| {},
                }
            }
        }
    }
}

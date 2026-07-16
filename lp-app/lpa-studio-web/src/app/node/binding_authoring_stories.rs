//! Stories for the binding authoring section (roadmap M4).

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectNodeAddress, ProjectSlotAddress, ProjectSlotRoot, SlotPath, UiBindingAuthoring,
    UiBindingAuthoringDirection, UiBindingEndpoint, UiChannelChoice, UiSlotAffordance,
    UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow,
};
use lpa_studio_web_story_macros::story;

use super::binding_authoring_section::BindingAuthoringSection;

/// The slot's current Binding aspect as the project controller would build
/// it: the endpoint row plus, for default wiring, the origin explanation.
fn bound_aspect(label: &str, endpoint: &str, default_origin: bool) -> UiSlotAspect {
    let mut aspect = UiSlotAspect::new(UiSlotAspectKind::Binding, "Binding")
        .with_row(UiSlotAspectRow::new(label.to_string(), endpoint))
        .with_affordance(UiSlotAffordance::Bound);
    if default_origin {
        aspect = aspect.with_row(
            UiSlotAspectRow::new("Origin", "default binding")
                .with_detail("Declared by the slot itself; authoring a binding overrides it."),
        );
    }
    aspect
}

fn bindings_map() -> ProjectSlotAddress {
    ProjectSlotAddress::new(
        ProjectNodeAddress::parse("/demo.project/playlist.playlist")
            .expect("valid story node address"),
        ProjectSlotRoot::def(),
        SlotPath::parse("bindings").expect("valid story slot path"),
    )
}

fn authoring(authored: Option<&str>) -> UiBindingAuthoring {
    UiBindingAuthoring {
        key: "time".to_string(),
        direction: UiBindingAuthoringDirection::Source,
        bindings_map: bindings_map(),
        authored: authored.map(UiBindingEndpoint::new),
    }
}

fn story_choices() -> Vec<UiChannelChoice> {
    vec![
        UiChannelChoice {
            name: "time".to_string(),
            kind: Some("Instant".to_string()),
            doc: Some("Project clock in seconds; the clock publishes it by default."),
            well_known: true,
            observed: true,
        },
        UiChannelChoice {
            name: "trigger".to_string(),
            kind: Some("Instant".to_string()),
            doc: Some("Control events; map readers merge by message id."),
            well_known: true,
            observed: true,
        },
        UiChannelChoice {
            name: "visual.out".to_string(),
            kind: Some("Color".to_string()),
            doc: Some("The project's primary visual output."),
            well_known: true,
            observed: false,
        },
        UiChannelChoice {
            name: "wobble".to_string(),
            kind: None,
            doc: None,
            well_known: false,
            observed: true,
        },
    ]
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StoryFrame(children: Element) -> Element {
    use_context_provider(|| Signal::new(story_choices()));
    rsx! {
        div { class: "tw:max-w-72 tw:rounded-sm tw:border tw:border-border tw:bg-card-subtle tw:p-2",
            {children}
        }
    }
}

#[story(
    label = "Unbound",
    description = "A bindable slot with no wiring at all: a single Bind… affordance."
)]
pub(crate) fn unbound() -> Element {
    rsx! {
        StoryFrame {
            BindingAuthoringSection { authoring: authoring(None), on_action: |_| {} }
        }
    }
}

#[story(
    label = "Authored",
    description = "A slot with an authored binding: the current wiring heads the block; Edit… points it elsewhere, Unbind removes the entry (re-enabling any slot-declared default)."
)]
pub(crate) fn authored() -> Element {
    rsx! {
        StoryFrame {
            BindingAuthoringSection {
                authoring: authoring(Some("bus:time")),
                on_action: |_| {},
                current: bound_aspect("Bound", "bus:time", false),
            }
        }
    }
}

#[story(
    label = "Default",
    description = "A slot wired by its declared default: the origin story explains the wiring; Edit… authors a binding that overrides it (no Unbind — there is no authored entry to remove)."
)]
pub(crate) fn default_bound() -> Element {
    rsx! {
        StoryFrame {
            BindingAuthoringSection {
                authoring: authoring(None),
                on_action: |_| {},
                current: bound_aspect("Published", "bus:time", true),
            }
        }
    }
}

#[story(
    label = "Picker",
    description = "The channel picker: observed ∪ well-known channels with kind tags and registry docs, kind-mismatch hints against the current channel, and validated free-text entry for new names — the picker teaches the naming norm, it does not gate. Enter in the text field submits."
)]
pub(crate) fn picker() -> Element {
    rsx! {
        StoryFrame {
            BindingAuthoringSection {
                authoring: authoring(Some("bus:time")),
                on_action: |_| {},
                current: bound_aspect("Bound", "bus:time", false),
                initially_picking: true,
            }
        }
    }
}

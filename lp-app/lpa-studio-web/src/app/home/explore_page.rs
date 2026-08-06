//! The Explore page (`#/explore`, vision D10): the example grid under
//! its new name — exactly the old gallery's Examples section, with
//! today's open-example behavior. No kinds, no provenance, no remix UI
//! (the content system is future M4 / wled-compat material, D6/D7).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiExampleCard, UiHomeView};

use crate::app::home::example_card::ExampleCard;
use crate::app::home::{card_grid_class, section_title_class};

/// The example grid. `home` is `None` while a project is open (the view
/// only builds the gallery slice when the shell would show it); the
/// examples are compiled-in content, so the page derives its own cards
/// then — only the transient opening state needs the view.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ExplorePage(
    #[props(default)] home: Option<UiHomeView>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let (examples, opening, busy) = match &home {
        Some(home) => (
            home.examples.clone(),
            home.opening.clone(),
            home.opening.is_some(),
        ),
        None => (embedded_example_cards(), None, false),
    };
    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-7",
            section { class: "tw:grid tw:gap-3",
                header { class: "tw:flex tw:items-center tw:gap-3",
                    h2 { class: section_title_class(), "Examples" }
                    // kind filter chips: Modules stays hidden while no module
                    // examples exist (M6 grows this)
                    span { class: "tw:rounded-full tw:border tw:border-border tw:px-2.5 tw:py-0.5 tw:text-xs tw:font-semibold tw:text-muted-foreground",
                        "Projects"
                    }
                }
                div { class: card_grid_class(),
                    for card in examples {
                        ExampleCard {
                            key: "{card.id}",
                            opening: opening.as_deref() == Some(card.id.as_str()),
                            busy,
                            card,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// The compiled-in examples as cards — the same projection the home view
/// builder makes, for the no-gallery-slice mounts.
fn embedded_example_cards() -> Vec<UiExampleCard> {
    lpa_studio_core::app::home::embedded_examples()
        .iter()
        .map(|example| UiExampleCard {
            id: example.id.to_string(),
            name: example.name.to_string(),
            kind: example.kind.to_string(),
        })
        .collect()
}

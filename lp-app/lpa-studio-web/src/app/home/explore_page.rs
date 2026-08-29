//! The Explore page (`#/explore`, vision D10): the example grid under
//! its new name — exactly the old gallery's Examples section, with
//! today's open-example behavior. No kinds, no provenance, no remix UI
//! (the content system is future M4 / wled-compat material, D6/D7).

use dioxus::prelude::*;

use crate::base::HelpLink;
use lpa_studio_core::{UiAction, UiHomeView};

use crate::app::home::example_card::{ExampleCard, embedded_example_cards};
use crate::app::home::gallery_preview::HoveredCard;
use crate::app::home::project_opening_frame::OpenFailureNotice;
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
    // Hover-to-play is page-scoped: one signal names one hovered card, so
    // the whole grid holds at most one live lease at a time.
    use_context_provider(|| HoveredCard(Signal::new(None)));
    let (examples, opening, busy) = match &home {
        Some(home) => (
            home.examples.clone(),
            home.opening.clone(),
            home.opening.is_some(),
        ),
        None => (embedded_example_cards(), None, false),
    };
    // An example open never reaches a `/p/` route, so it has no opening
    // frame to fail inside — and before P6 a failed example open left the
    // card back to normal and the error in the console only. Read at
    // render: the terminal failure is a page-thread signal, and this page
    // re-renders on the very emission that clears `home.opening`.
    let failure = match lpa_studio_core::open_stage() {
        lpa_studio_core::OpenStage::Failed(failure) => Some(failure),
        _ => None,
    };
    rsx! {
        div { class: "tw:grid tw:content-start tw:gap-7",
            if let Some(failure) = failure {
                OpenFailureNotice {
                    message: failure.message,
                    retry: failure.retry,
                    on_action: Some(on_action),
                }
            }
            section { class: "tw:grid tw:gap-3",
                header { class: "tw:flex tw:items-center tw:gap-3",
                    h2 { class: section_title_class(), "Examples" }
                    // kind filter chips: Modules stays hidden while no module
                    // examples exist (M6 grows this)
                    span { class: "tw:rounded-full tw:border tw:border-border tw:px-2.5 tw:py-0.5 tw:text-xs tw:font-semibold tw:text-muted-foreground",
                        "Projects"
                    }
                    // Where a WLED person goes looking for "the effects
                    // list" — the exact spot the shader question arises.
                    HelpLink {
                        href: crate::app::docs::docs_links::what_is_a_shader::HREF,
                        title: "What's a shader?",
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



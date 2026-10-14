//! The Explore page (`#/explore`, vision D10): the catalog, grouped by
//! kind (catalog content tree D17) — real pieces first, then patterns,
//! the same sections the home page shows as its examples
//! ([`ExampleGroups`], drawn here with titles), with today's open-example
//! behavior. No provenance, no remix UI, no filter chrome (T4 material).

use dioxus::prelude::*;

use lpa_studio_core::{UiAction, UiHomeView, example_groups};

use crate::app::home::example_card::embedded_example_cards;
use crate::app::home::gallery_preview::HoveredCard;
use crate::app::home::page::example_groups::{ExampleGroups, ExampleHeading};
use crate::app::home::project_opening_frame::OpenFailureNotice;

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
            ExampleGroups {
                groups: example_groups(&examples),
                opening,
                busy,
                heading: ExampleHeading::Title,
                on_action,
            }
        }
    }
}

//! Example cards: the window-shopper path.

use dioxus::prelude::*;
use lpa_studio_core::{HomeOp, PreviewSource, UiAction, UiExampleCard};

use crate::app::home::card_footer::CardGlassFooter;
use crate::app::home::card_thumb::CardThumb;
use crate::app::home::gallery_preview::{ThumbMode, card_hover_handlers};
use crate::app::home::package_card::home_action;
use crate::app::home::project_opening_frame::OpeningProgressLine;

/// One example. Click → running simulator, zero choices; the copy becomes
/// yours in the library (seed-once) and forks on first divergent save.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ExampleCard(
    card: UiExampleCard,
    /// This card's open is in flight.
    #[props(default = false)]
    opening: bool,
    /// Some other open is in flight. The card reads busy — but it still
    /// takes clicks: the newest click wins (D4), and a click that looks
    /// ignored is the failure this whole change exists to remove.
    #[props(default = false)]
    busy: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let open_id = card.id.clone();
    let source = PreviewSource::Example(card.id.clone());
    // Hover-to-play: pointing at a card is what buys live rendering. Touch
    // devices never send these, so a tap still just opens the example.
    let (hover_enter, hover_leave) = card_hover_handlers(Some(&source));

    rsx! {
        article {
            class: example_card_class(opening, busy),
            onclick: move |_| {
                // `busy` no longer swallows the click: an open already in
                // flight is SUPERSEDED by this one (D4). Only the card
                // whose own open is running stays inert — there is nothing
                // for it to supersede but itself.
                if !opening {
                    on_action
                        .call(home_action(HomeOp::OpenExample {
                            id: open_id.clone(),
                        }));
                }
            },
            onmouseenter: hover_enter,
            onmouseleave: hover_leave,
            CardThumb {
                seed: card.id.clone(),
                label: card.name.clone(),
                source: Some(source),
                // A shelf of examples is a shelf of pictures: each card
                // renders just long enough to take one, then lets its slot
                // go. Explore shows a dozen of these at once.
                mode: ThumbMode::PosterFirst,
            }
            // The face is the art; the words are one shallow glass bar
            // (card-overlay redesign). Title ONLY at rest — no menu, no
            // glyphs, and no "Example" label: the shelf's own section
            // header already says it, and repeating it on every card
            // read as noise at the G1 gate (2026-08-26). Hover slides
            // the bar up over the example's fixture blurb, in step with
            // the live preview the same hover starts.
            CardGlassFooter {
                title: card.name.clone(),
                reveal: (!opening && !card.blurb.is_empty()).then(|| rsx! {
                    p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "{card.blurb}" }
                }),
                if opening {
                    // The live pipeline, not a static "Opening…": an example
                    // open never routes to the full opening frame, so on a
                    // slow connection this line is the only honest indicator
                    // of the engine download it is waiting on.
                    OpeningProgressLine {}
                }
            }
        }
    }
}

/// The card's treatment while an open runs. Busy is a DIMMING, not a
/// disabling: the cursor stays a pointer because the card still acts.
fn example_card_class(opening: bool, busy: bool) -> &'static str {
    // tw:relative anchors the glass footer over the art box
    match (opening, busy) {
        (true, _) => {
            "tw:group tw:relative tw:cursor-wait tw:overflow-hidden tw:rounded-md tw:border tw:border-status-working-border tw:bg-card"
        }
        (false, true) => {
            "tw:group tw:relative tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:opacity-60 tw:transition-opacity"
        }
        (false, false) => {
            "tw:group tw:relative tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:transition-colors tw:hover:border-border-strong"
        }
    }
}

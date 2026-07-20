//! Example cards: the window-shopper path.

use dioxus::prelude::*;
use lpa_studio_core::{HomeOp, PreviewSource, UiAction, UiExampleCard};

use crate::app::home::card_thumb::CardThumb;
use crate::app::home::package_card::home_action;

/// One example. Click → running simulator, zero choices; the copy becomes
/// yours in the library (seed-once) and forks on first divergent save.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ExampleCard(
    card: UiExampleCard,
    /// This card's open is in flight.
    #[props(default = false)]
    opening: bool,
    /// Some other open is in flight — clicks are ignored.
    #[props(default = false)]
    busy: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let open_id = card.id.clone();

    rsx! {
        article {
            class: example_card_class(opening),
            onclick: move |_| {
                if !busy && !opening {
                    on_action.call(home_action(HomeOp::OpenExample {
                        id: open_id.clone(),
                    }));
                }
            },
            CardThumb {
                seed: card.id.clone(),
                label: card.name.clone(),
                source: Some(PreviewSource::Example(card.id.clone())),
            }
            div { class: "tw:grid tw:gap-0.5 tw:p-3",
                p { class: "tw:m-0 tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground",
                    "{card.name}"
                }
                if opening {
                    p { class: "tw:m-0 tw:text-xs tw:text-status-working-foreground", "Opening…" }
                } else {
                    p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "Example" }
                }
            }
        }
    }
}

fn example_card_class(opening: bool) -> &'static str {
    if opening {
        "tw:cursor-wait tw:overflow-hidden tw:rounded-md tw:border tw:border-status-working-border tw:bg-card"
    } else {
        "tw:cursor-pointer tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:transition-colors tw:hover:border-border-strong"
    }
}

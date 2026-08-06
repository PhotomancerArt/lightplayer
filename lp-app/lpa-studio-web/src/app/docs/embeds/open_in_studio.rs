//! `open-in-studio`: the escape hatch out of the article and into the real
//! editor (tier 3, D6).
//!
//! Deliberately **not** an embedded mini-editor. The reader has been
//! playing with the real components for four beats; the honest next step
//! is the real app, with the same example, in their own gallery. Seeding
//! it there is correct here — unlike every other docs surface, this one is
//! the user's deliberate act.
//!
//! It dispatches the **main app's** `HomeOp::OpenExample`, not a docs
//! host: docs sims are leased and disposable, and this project is meant to
//! be kept. The app's own view→URL sync moves the route to the editor once
//! the open lands, so there is no navigation to perform here.
//!
//! Without a dispatcher (the story book, host builds) the button renders
//! inert with a tooltip that says why, matching the settings-chip
//! precedent.

use dioxus::prelude::*;
use lpa_studio_core::{HOME_NODE_ID, HomeOp, UiAction};

use crate::base::{StudioIcon, StudioIconName};

use super::docs_sims::DocsStudioActions;

/// The article's "make it yours" button.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn OpenInStudioButton(
    /// Embedded example id (`examples/plasma`).
    example_id: String,
    /// Button text; the fence's `label=` overrides the default.
    #[props(default)]
    label: Option<String>,
) -> Element {
    let actions = try_consume_context::<DocsStudioActions>();
    let live = actions.as_ref().is_some_and(DocsStudioActions::is_live);
    let label = label.unwrap_or_else(|| "Open in Studio".to_string());
    let title = if live {
        "Opens this example in the editor and keeps it in your projects".to_string()
    } else {
        "Only available in the running app".to_string()
    };

    rsx! {
        div { class: "tw:mb-1.5 tw:flex tw:justify-center tw:py-1 tw:last:mb-0",
            button {
                class: open_button_class(live),
                r#type: "button",
                disabled: !live,
                title: "{title}",
                onclick: move |_| {
                    if let Some(actions) = &actions {
                        actions
                            .dispatch(
                                UiAction::from_op(
                                    HOME_NODE_ID,
                                    HomeOp::OpenExample {
                                        id: example_id.clone(),
                                    },
                                ),
                            );
                    }
                },
                span { "{label}" }
                StudioIcon { name: StudioIconName::ExternalLink, size: 16 }
            }
        }
    }
}

/// Prominent and friendly: the accent-filled shape, big enough to read as
/// the page's call to action rather than another inline chip. Inert keeps
/// the identical footprint on the muted surface.
fn open_button_class(live: bool) -> &'static str {
    if live {
        "tw:inline-flex tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded-md tw:border tw:border-accent-border tw:bg-accent tw:px-4 tw:py-2 tw:text-sm tw:font-bold tw:text-accent-foreground tw:transition-colors tw:hover:bg-accent-hover"
    } else {
        "tw:inline-flex tw:cursor-not-allowed tw:items-center tw:gap-2 tw:rounded-md tw:border tw:border-border-muted tw:bg-card-muted tw:px-4 tw:py-2 tw:text-sm tw:font-bold tw:text-dim-foreground"
    }
}

//! The project-shaped opening frame.
//!
//! Shown while the route says a project and the actor's view hasn't
//! reached it yet (boot reopen, forward-button reopen): the URL's intent
//! picks the frame, so the gallery never flashes on a project reload.
//! Deliberately generic ("Opening project…") — the library may not be
//! attached yet when this first renders; upgrading the copy with the
//! project's name once the snapshot has it is recorded future polish.

use dioxus::prelude::*;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectOpeningFrame() -> Element {
    rsx! {
        section { class: "tw:grid tw:gap-3.5",
            div { class: "tw:flex tw:items-center tw:gap-3",
                span { class: "tw:h-2.5 tw:w-2.5 tw:animate-pulse tw:rounded-full tw:bg-status-working-foreground" }
                p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-muted-foreground", "Opening project…" }
            }
            // a rough silhouette of the editor's three-column layout
            div { class: "tw:grid tw:animate-pulse tw:grid-cols-[minmax(220px,280px)_minmax(0,1fr)_minmax(300px,360px)] tw:gap-3.5 tw:max-[960px]:grid-cols-1",
                div { class: skeleton_class(), style: "height: 180px;" }
                div { class: "tw:grid tw:content-start tw:gap-3.5",
                    div { class: skeleton_class(), style: "height: 120px;" }
                    div { class: skeleton_class(), style: "height: 220px;" }
                }
                div { class: skeleton_class(), style: "height: 180px;" }
            }
        }
    }
}

fn skeleton_class() -> &'static str {
    "tw:rounded-md tw:border tw:border-border tw:bg-card"
}

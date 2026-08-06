//! [`DocsPage`]: sidebar + article for the docs section.
//!
//! A section of the running app, not a standalone page: `web_app`'s route
//! signal drives `page`, so article switches and section switches are both
//! plain re-renders — the actor and every open runtime session survive
//! them. That is also what makes live, running examples inside articles
//! possible (the interactive-docs initiative builds on it).

use dioxus::prelude::*;

use super::embeds::render_embed;
use super::{DocPage, PAGES, page_for};
use crate::base::MarkdownDocs;
use crate::base::markdown_text::MdEmbedRef;

/// The docs section body. `page` is the route's article slug; unknown and
/// missing slugs resolve to the guide's landing article.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DocsPage(#[props(default)] page: Option<String>) -> Element {
    let page = page_for(page.as_deref());
    // The one place `embed` fences become components. Unknown names are an
    // authoring mistake the generated checks catch before merge; if one ever
    // reaches a reader, it says so out loud rather than vanishing.
    let embeds = use_callback(|embed: MdEmbedRef| match render_embed(&embed) {
        Some(element) => element,
        None => rsx! {
            div { class: "tw:mb-1.5 tw:rounded-md tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-4 tw:text-xs tw:text-status-error-foreground tw:last:mb-0",
                "Unknown docs embed `{embed.name}` — no such directive is registered."
            }
        },
    });
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-start tw:gap-7 tw:max-[720px]:flex-col",
            nav { class: "tw:flex tw:w-[200px] tw:flex-none tw:flex-col tw:gap-0.5 tw:max-[720px]:w-full",
                span { class: "tw:mb-1 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                    "Guide"
                }
                for entry in PAGES {
                    SidebarLink { entry, active: entry.slug == page.slug }
                }
            }
            article { class: "tw:min-w-0 tw:flex-1",
                MarkdownDocs { text: page.markdown.to_string(), embeds: Some(embeds) }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SidebarLink(entry: &'static DocPage, active: bool) -> Element {
    let class = if active {
        "tw:rounded-sm tw:bg-card-subtle tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:text-heading tw:no-underline"
    } else {
        "tw:rounded-sm tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:text-subtle-foreground tw:no-underline tw:hover:bg-background-wash tw:hover:text-strong-foreground"
    };
    rsx! {
        a { class: "{class}", href: "#/docs/{entry.slug}", "{entry.title}" }
    }
}

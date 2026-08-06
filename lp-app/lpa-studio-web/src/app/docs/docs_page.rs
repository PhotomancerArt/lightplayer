//! [`DocsPage`]: sidebar + article for the docs section.
//!
//! A section of the running app, not a standalone page: `web_app`'s route
//! signal drives `page`, so article switches and section switches are both
//! plain re-renders — the actor and every open runtime session survive
//! them. That is also what makes the live examples inside articles work
//! (see `docs/adr/2026-08-06-interactive-docs-architecture.md`).

use dioxus::prelude::*;
use lpa_studio_core::UiAction;

use super::embeds::render_embed;
use super::embeds::{DocsSimProvider, DocsStudioActions};
use super::{DocPage, PAGES, page_for};
use crate::base::MarkdownDocs;
use crate::base::markdown_text::MdEmbedRef;

/// The docs section body. `page` is the route's article slug; unknown and
/// missing slugs resolve to the guide's landing article.
///
/// `on_studio_action` is the running app's dispatcher, lent to the section
/// so the `open-in-studio` embed can run the real open flow. It is
/// deliberately optional: the story book and any other host renders the
/// section without one, and the button goes inert there.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DocsPage(
    #[props(default)] page: Option<String>,
    /// Heading anchor from the route (`#/docs/<slug>#<anchor>`, help
    /// links): scrolled to after render. The whole string lives inside
    /// `location.hash`, so the browser's native fragment scroll never
    /// fires — this effect is the mechanism, not a progressive touch-up.
    #[props(default)]
    anchor: Option<String>,
    #[props(default)] on_studio_action: Option<EventHandler<UiAction>>,
) -> Element {
    let page = page_for(page.as_deref());
    // Re-runs on article and anchor changes alike; MarkdownDocs renders
    // heading ids synchronously with the article, so the target exists by
    // the time the effect fires. A missing id (stale link that somehow
    // escaped the build check) is a silent no-scroll, not an error.
    let scroll_slug = page.slug;
    let scroll_anchor = anchor.clone();
    use_effect(use_reactive!(|(scroll_slug, scroll_anchor)| {
        let _ = scroll_slug;
        if let Some(anchor) = scroll_anchor
            && let Some(document) = web_sys::window().and_then(|window| window.document())
            && let Some(target) = document.get_element_by_id(&anchor)
        {
            target.scroll_into_view();
        }
    }));
    // Refreshed rather than captured: `web_app` builds a fresh
    // `EventHandler` every render, and a click must ride the live one.
    // Writing through the cell is not reactive, so this cannot loop.
    let studio_actions = use_context_provider(DocsStudioActions::empty);
    *studio_actions.0.borrow_mut() = on_studio_action;
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
                // KEYED BY SLUG on purpose: switching articles must remount
                // the provider, because that is what runs the docs sims'
                // teardown (`DocsSimProvider`'s `use_drop` → the enqueued
                // StopSimulator that terminates the Worker). A shared,
                // un-keyed provider would carry one article's sims into the
                // next and leak them.
                DocsSimProvider { key: "{page.slug}", sims: page.sims,
                    MarkdownDocs { text: page.markdown.to_string(), embeds: Some(embeds) }
                }
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

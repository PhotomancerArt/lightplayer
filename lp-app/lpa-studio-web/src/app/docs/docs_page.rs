//! [`DocsPage`]: sidebar + article for the docs section.
//!
//! A standalone page like the boards catalog (early return in
//! `web_app.rs`, no studio actor). Article switches within the section
//! navigate by hash and re-render through a `hashchange` listener — no
//! reload; the studio's route listener is never installed on standalone
//! pages, so owning `onhashchange` here clobbers nothing.

use dioxus::prelude::*;

use super::{DocPage, PAGES, page_for};
use crate::base::MarkdownDocs;

/// The docs section body. `initial_page` is the deep-linked slug from the
/// URL at mount; later in-section navigation is handled internally.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DocsPage(#[props(default)] initial_page: Option<String>) -> Element {
    let mut current = use_signal(|| page_for(initial_page.as_deref()).slug);

    // Follow hash changes (sidebar clicks, back/forward within docs).
    // Guarded on the current route so a story-book mount never installs
    // this into the book's own hash navigation.
    #[cfg(target_arch = "wasm32")]
    use_hook(move || {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;
        if !matches!(
            crate::router::current_route(),
            crate::router::StudioRoute::Docs { .. }
        ) {
            return;
        }
        let closure = Closure::<dyn FnMut()>::new(move || {
            let hash = web_sys::window()
                .map(|window| window.location().hash().unwrap_or_default())
                .unwrap_or_default();
            match crate::router::StudioRoute::parse(&hash) {
                crate::router::StudioRoute::Docs { page } => {
                    current.set(page_for(page.as_deref()).slug);
                }
                // Leaving the section (a chrome tab, back/forward): the
                // studio app mounts on fresh page loads only — reload,
                // mirroring the studio-side route listener.
                _ => crate::router::hard_reload(),
            }
        });
        if let Some(window) = web_sys::window() {
            // addEventListener, not `onhashchange = …`: the SiteChrome
            // wrapper installs its own leave-section listener on the same
            // event, and the two must coexist.
            let _ = window
                .add_event_listener_with_callback("hashchange", closure.as_ref().unchecked_ref());
        }
        // Leak deliberately: the page lives for the document's lifetime
        // (route changes out of standalone pages hard-reload).
        closure.forget();
    });

    let page = page_for(Some(current()));
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
                MarkdownDocs { text: page.markdown.to_string() }
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

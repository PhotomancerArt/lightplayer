//! The "?" help affordance: a quiet icon link from a confusing surface to
//! the docs page that answers it.
//!
//! The flywheel this exists for: a question keeps coming up → a docs page
//! answers it (named after the question, per the style guide) → a
//! [`HelpLink`] is planted exactly where people hit the question → the
//! question stops needing a human. Plant one wherever a concept first
//! confronts a user, not in every corner — a page of "?"s is noise.
//!
//! `href` takes a **generated docs-link constant**
//! (`crate::app::docs::docs_links::…`, emitted by the build script from
//! the real articles), so a help link to a page or anchor that stops
//! existing is a compile error, never a dead "?" in the field.
//!
//! Visually it is deliberately quieter than an action: no border chip, a
//! subtle glyph that brightens to the strong neutral on hover (links are
//! neutral at rest — accent reckoning D1, 2026-08-30). It must be
//! discoverable without competing with the surface's real controls.

use dioxus::prelude::*;

use crate::base::{StudioIcon, StudioIconName};

/// Glyph size for the fixed `h-6 w-6` footprint.
const HELP_ICON_SIZE: u32 = 14;

/// A quiet "?" linking to a docs page. `href` must be a generated
/// `docs_links` constant; `title` is the question the page answers, in
/// the page's own words (e.g. "What's a shader?").
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HelpLink(
    href: &'static str,
    title: String,
    /// Extra layout-only classes (e.g. `tw:ml-auto`) — never restyling.
    #[props(default = None)]
    class: Option<String>,
) -> Element {
    let mut link_class = "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:items-center \
         tw:justify-center tw:rounded-xs tw:text-subtle-foreground \
         tw:transition-colors tw:hover:text-strong-foreground"
        .to_string();
    if let Some(extra) = class {
        link_class = format!("{link_class} {extra}");
    }
    rsx! {
        a {
            class: link_class,
            href: "{href}",
            aria_label: "{title}",
            title: "{title}",
            // Help links live inside clickable rows and headers; opening
            // the docs must never double as the row's own click.
            onclick: move |event| event.stop_propagation(),
            StudioIcon { name: StudioIconName::Help, size: HELP_ICON_SIZE }
        }
    }
}

#[cfg(test)]
mod tests {
    // The component is prop-plumbing over an anchor; the load-bearing
    // contract — help hrefs exist — lives in the generated docs checks
    // (`app/docs/docs_checks.rs`) and in `href` being `&'static str`
    // resolved from generated constants at compile time.
}

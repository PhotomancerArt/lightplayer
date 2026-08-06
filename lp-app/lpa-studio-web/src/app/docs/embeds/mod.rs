//! The docs embed registry: the closed set of ` ```embed <name> ... ` fence
//! directives articles may use, and how each one renders.
//!
//! [`EMBED_NAMES`] is the single source of truth for "which directives
//! exist". `build.rs` scans `docs/user-guide/` and generates a check
//! (`super::docs_checks`) asserting every fence name in every registered
//! article appears here — a typo in an article is a failing test, not a
//! mystery box in the rendered page.
//!
//! The bodies are placeholders for now: each known name renders a labeled,
//! obviously-unfinished box that states the directive and its arguments, so
//! an article author can see the fence was recognized and where the real
//! surface will sit. Wiring those to live Studio surfaces is the next
//! phase's work; nothing here boots a runtime.

use dioxus::prelude::*;

use crate::base::markdown_text::MdEmbedRef;

/// Every embed directive an article may use. Adding a directive means
/// adding it here *and* to [`render_embed`]'s match — the
/// `every_registered_name_renders` test keeps the two in step.
pub const EMBED_NAMES: &[&str] = &[
    "hero-preview",
    "sim-canvas",
    "panel",
    "code-figure",
    "open-in-studio",
];

/// Render a parsed `embed` fence, or `None` when the name is not
/// registered. Unknown names are an authoring mistake: the caller decides
/// how loud to be about them (see `docs_page`, which renders a visible
/// error) — this registry just declines.
pub(crate) fn render_embed(embed: &MdEmbedRef) -> Option<Element> {
    if !EMBED_NAMES.contains(&embed.name.as_str()) {
        return None;
    }
    // One placeholder shape for every registered name until the real
    // surfaces land; the name and args are what an author needs to see.
    Some(embed_placeholder(embed))
}

/// The unfinished-embed box: bordered, muted, dashed (Studio's "nothing
/// real here yet" convention), and tall enough to hold the space the live
/// surface will occupy so article layout doesn't lurch when it arrives.
fn embed_placeholder(embed: &MdEmbedRef) -> Element {
    let args = format_args_line(&embed.args);
    rsx! {
        div { class: "tw:mb-1.5 tw:grid tw:min-h-24 tw:place-items-center tw:gap-1 tw:rounded-md tw:border tw:border-dashed tw:border-border-subtle tw:bg-card-subtle tw:p-4 tw:text-center tw:last:mb-0",
            span { class: "tw:font-mono tw:text-xs tw:text-subtle-foreground", "embed {embed.name}" }
            if !args.is_empty() {
                span { class: "tw:font-mono tw:text-xs tw:text-muted-foreground", "{args}" }
            }
            span { class: "tw:text-xs tw:text-muted-foreground", "placeholder — the live surface lands in a later phase" }
        }
    }
}

/// Directive arguments as they were written, for the placeholder label.
fn format_args_line(args: &[(String, String)]) -> String {
    args.iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embed(name: &str) -> MdEmbedRef {
        MdEmbedRef {
            name: name.to_string(),
            args: vec![("sim".to_string(), "disc".to_string())],
            body: String::new(),
        }
    }

    #[test]
    fn every_registered_name_renders() {
        for name in EMBED_NAMES {
            assert!(
                render_embed(&embed(name)).is_some(),
                "`{name}` is registered in EMBED_NAMES but render_embed declined it"
            );
        }
    }

    #[test]
    fn unknown_names_are_declined_not_guessed_at() {
        assert!(render_embed(&embed("hero-previews")).is_none());
        assert!(render_embed(&embed("")).is_none());
    }

    #[test]
    fn registered_names_are_unique() {
        for (index, name) in EMBED_NAMES.iter().enumerate() {
            assert!(
                !EMBED_NAMES[..index].contains(name),
                "`{name}` is listed twice in EMBED_NAMES"
            );
        }
    }

    #[test]
    fn args_line_reconstructs_what_the_author_wrote() {
        assert_eq!(
            format_args_line(&[
                ("sim".to_string(), "disc".to_string()),
                ("mode".to_string(), "interactive".to_string()),
            ]),
            "sim=disc mode=interactive"
        );
        assert_eq!(format_args_line(&[]), "");
    }
}

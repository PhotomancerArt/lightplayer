//! The docs embed registry: the closed set of ` ```embed <name> ... ` fence
//! directives articles may use, and how each one renders.
//!
//! [`EMBED_NAMES`] is the single source of truth for "which directives
//! exist". `build.rs` scans `docs/user-guide/` and generates a check
//! (`super::docs_checks`) asserting every fence name in every registered
//! article appears here — a typo in an article is a failing test, not a
//! mystery box in the rendered page.
//!
//! # The contract
//!
//! Arguments are `key=value`, space separated, on the fence's info line.
//! Unknown keys are ignored; a *missing required* key, an unknown `sim=`
//! handle, and an unparseable enum value all render a loud problem box
//! rather than a guess.
//!
//! | directive | arguments | renders |
//! |---|---|---|
//! | `hero-preview` | `example=<id>` | a `PreviewHost` lease, article-wide, 16:9 |
//! | `sim-canvas` | `sim=<name>`, `view=map\|product` (default `map`) | the real `ProductPreview` over that sim's output |
//! | `panel` | `sim=<name>[,<name>…]`, `mode=interactive\|readonly` (default `interactive`) | the real `ModulePanel`, fanning gestures out to every named sim |
//! | `code-figure` | `src=<figure id>` | the registered listing from `super::code_figures` |
//! | `open-in-studio` | `example=<id>`, `label=<text>` | the main app's real open flow |
//!
//! # Where the live state comes from
//!
//! Everything with a `sim=` reads [`docs_sims::DocsSimRegistry`] out of
//! page-local context, provided by [`DocsSimProvider`] around the article
//! (see its module docs for the lifecycle). `open-in-studio` reads
//! [`docs_sims::DocsStudioActions`], the main app's dispatcher lent to the
//! docs section. Both are **absent by design** in the story book and on
//! host builds, and every embed has a defined inert rendering for that:
//! sim embeds show their loading state, the button renders disabled. No
//! embed ever boots a worker just by being rendered.
//!
//! # Deliberate gaps
//!
//! - `controls=` (filtering a panel to named channels) is specified in the
//!   plan but unimplemented: no article asks for it, and the panel these
//!   embeds show is two knobs wide. It is not in [`EMBED_NAMES`]' argument
//!   surface, so an article using it gets the whole panel silently — add
//!   it with a real caller.
//! - Visibility pause is document-level only (see [`docs_sims`]); a sim
//!   scrolled off screen keeps pulling its view while the tab is focused.

use dioxus::prelude::*;

use crate::base::markdown_text::MdEmbedRef;

pub(crate) mod docs_sims;
mod embed_frame;
mod hero_preview;
mod open_in_studio;
mod panel_embed;
mod sim_canvas;

pub(crate) use docs_sims::{DocsSimProvider, DocsStudioActions};
pub(crate) use open_in_studio::OpenInStudioButton;
#[cfg(feature = "stories")]
pub(crate) use panel_embed::DocsPanelSurface;
pub(crate) use panel_embed::PanelMode;
#[cfg(feature = "stories")]
pub(crate) use sim_canvas::DocsSimCanvas;
pub(crate) use sim_canvas::SimCanvasView;

use embed_frame::EmbedProblem;
use hero_preview::DocsHeroPreview;
use panel_embed::PanelEmbed;
use sim_canvas::SimCanvasEmbed;

/// Every embed directive an article may use. Adding a directive means
/// adding it here *and* to [`render_embed`]'s match — the
/// `every_registered_name_renders` test keeps the two in step.
pub const EMBED_NAMES: &[&str] = &[
    "hero-preview",
    "sim-canvas",
    "panel",
    "code-figure",
    "open-in-studio",
    "editor",
];

/// Render a parsed `embed` fence, or `None` when the name is not
/// registered. Unknown names are an authoring mistake: the caller decides
/// how loud to be about them (see `docs_page`, which renders a visible
/// error) — this registry just declines.
///
/// This runs inside the article's render, so it stays a pure function of
/// the fence: everything that needs hooks or context lives in the
/// components it returns.
pub(crate) fn render_embed(embed: &MdEmbedRef) -> Option<Element> {
    match embed.name.as_str() {
        "hero-preview" => Some(match arg(embed, "example") {
            Some(example_id) => rsx! {
                DocsHeroPreview { example_id: example_id.to_string() }
            },
            None => problem("`hero-preview` needs an `example=<id>` argument."),
        }),
        "sim-canvas" => Some(render_sim_canvas(embed)),
        // G1-round-2 (R3): the editable GLSL editor against the docs sim.
        // Placeholder until the embed component lands in this round.
        "editor" => Some(match arg(embed, "sim") {
            Some(name) => rsx! {
                div { class: "tw:mb-1.5 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-4 tw:text-xs tw:text-muted-foreground tw:last:mb-0",
                    "editor embed (sim={name}) — landing in this revision"
                }
            },
            None => problem("`editor` needs a `sim=<name>` argument."),
        }),
        "panel" => Some(render_panel(embed)),
        "code-figure" => Some(render_code_figure(embed)),
        "open-in-studio" => Some(match arg(embed, "example") {
            Some(example_id) => {
                let label = arg(embed, "label").map(str::to_string);
                rsx! {
                    OpenInStudioButton { example_id: example_id.to_string(), label }
                }
            }
            None => problem("`open-in-studio` needs an `example=<id>` argument."),
        }),
        _ => None,
    }
}

/// `sim-canvas`: one named sim's output through the real preview path.
///
/// Only the fence's *syntax* is decided here; resolving the sim needs page
/// context, which is [`SimCanvasEmbed`]'s own render's job.
fn render_sim_canvas(embed: &MdEmbedRef) -> Element {
    let Some(name) = arg(embed, "sim") else {
        return problem("`sim-canvas` needs a `sim=<name>` argument.");
    };
    let view = match arg(embed, "view") {
        None => SimCanvasView::default(),
        Some(value) => match SimCanvasView::parse(value) {
            Some(view) => view,
            None => {
                return problem(&format!(
                    "`sim-canvas` got `view={value}`; the views are `map` and `product`."
                ));
            }
        },
    };
    rsx! {
        SimCanvasEmbed { sim: name.to_string(), view }
    }
}

/// `panel`: the real panel surface over one or more named sims.
fn render_panel(embed: &MdEmbedRef) -> Element {
    let Some(names) = arg(embed, "sim") else {
        return problem("`panel` needs a `sim=<name>[,<name>…]` argument.");
    };
    let mode = match arg(embed, "mode") {
        None => PanelMode::default(),
        Some(value) => match PanelMode::parse(value) {
            Some(mode) => mode,
            None => {
                return problem(&format!(
                    "`panel` got `mode={value}`; the modes are `interactive` and `readonly`."
                ));
            }
        },
    };
    rsx! {
        PanelEmbed { sims: names.to_string(), mode }
    }
}

/// `code-figure`: a registered listing, byte-identical to the file the
/// running sim compiles.
fn render_code_figure(embed: &MdEmbedRef) -> Element {
    let Some(id) = arg(embed, "src") else {
        return problem("`code-figure` needs a `src=<id>` argument.");
    };
    let Some(figure) = super::code_figures::code_figure(id) else {
        return problem(&format!(
            "`code-figure` names figure `{id}`, which is not registered."
        ));
    };
    rsx! {
        div { class: "tw:mb-1.5 tw:last:mb-0",
            crate::base::CodeFigure {
                code: figure.code.to_string(),
                title: figure.title.to_string(),
                highlights: figure.highlights(),
            }
        }
    }
}

/// One `key=value` argument off the fence's info line.
fn arg<'a>(embed: &'a MdEmbedRef, key: &str) -> Option<&'a str> {
    embed
        .args
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

/// An authoring mistake, said out loud in the article.
fn problem(message: &str) -> Element {
    rsx! {
        EmbedProblem { message: message.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embed(name: &str) -> MdEmbedRef {
        MdEmbedRef {
            name: name.to_string(),
            args: vec![
                ("sim".to_string(), "disc".to_string()),
                ("example".to_string(), "examples/plasma".to_string()),
                ("src".to_string(), "plasma-shader".to_string()),
            ],
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
    fn arguments_resolve_by_key() {
        let embed = embed("panel");
        assert_eq!(arg(&embed, "sim"), Some("disc"));
        assert_eq!(arg(&embed, "mode"), None);
    }

    /// A directive missing its required argument still renders (loudly)
    /// rather than vanishing or panicking mid-article.
    #[test]
    fn missing_required_arguments_render_a_problem_not_a_panic() {
        for name in EMBED_NAMES {
            let bare = MdEmbedRef {
                name: (*name).to_string(),
                args: Vec::new(),
                body: String::new(),
            };
            assert!(
                render_embed(&bare).is_some(),
                "`{name}` declined a bare fence"
            );
        }
    }

    /// The article's own fences, verbatim: the exact argument spellings
    /// `docs/user-guide/what-is-a-shader.md` uses must all resolve.
    #[test]
    fn the_articles_own_fences_all_resolve() {
        let fences = [
            ("hero-preview", vec![("example", "examples/plasma")]),
            ("panel", vec![("sim", "disc,grid"), ("mode", "interactive")]),
            ("code-figure", vec![("src", "plasma-shader")]),
            ("sim-canvas", vec![("sim", "disc"), ("view", "map")]),
            ("sim-canvas", vec![("sim", "grid"), ("view", "map")]),
            ("open-in-studio", vec![("example", "examples/plasma")]),
        ];
        for (name, args) in fences {
            let embed = MdEmbedRef {
                name: name.to_string(),
                args: args
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
                body: String::new(),
            };
            assert!(
                render_embed(&embed).is_some(),
                "the article's `{name}` fence did not render"
            );
        }
    }
}

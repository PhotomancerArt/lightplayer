//! The in-app docs section: a minimal static surface over the repo's
//! `docs/user-guide/` articles, rendered at `/docs[/<slug>]`.
//!
//! Deliberately simple — a compiled-in manifest and a sidebar+article
//! layout. Adding an article is one file in `docs/user-guide/`, one
//! [`PAGES`] entry, and — when the article embeds live sims — that entry's
//! [`DocPage::sims`] declarations.
//!
//! Interactive docs (live Studio surfaces embedded in articles) is now this
//! module's job rather than a future initiative: [`PAGES`] declares each
//! article's sims, [`embeds`] registers the `embed` directive names articles
//! may use, and `build.rs` scans `docs/user-guide/` to generate the checks in
//! [`docs_checks`] — an unknown embed name, an undeclared `sim=`, or a dead
//! `#/docs` link fails validation rather than shipping a broken page.

pub mod docs_checks;
pub mod docs_page;
pub mod embeds;
#[cfg(feature = "stories")]
pub(crate) mod embeds_stories;

pub use docs_checks::docs_links;
pub use docs_page::DocsPage;

/// One live sim an article may embed, declared by the page that uses it.
///
/// `name` is the handle articles reference (`sim=disc`); `example_id` names
/// the embedded example it boots from (see
/// `lpa_studio_core::app::home::embedded_example`).
#[derive(Debug, PartialEq)]
pub struct DocsSimSpec {
    /// The article-facing handle, e.g. `main` in ` ```embed panel sim=main `.
    pub name: &'static str,
    /// The embedded example this sim runs, e.g. `examples/plasma-duo`.
    pub example_id: &'static str,
}

/// One compiled-in article.
#[derive(Debug, PartialEq)]
pub struct DocPage {
    /// URL segment: `/docs/<slug>`. The landing page's slug never appears
    /// in URLs it emits, but deep links to it still resolve.
    pub slug: &'static str,
    /// Sidebar label.
    pub title: &'static str,
    /// The article body, compiled in from `docs/user-guide/`.
    pub markdown: &'static str,
    /// Sims this article's embeds may reference, empty for static pages.
    /// Every `sim=` argument in the article must name one of these — the
    /// generated checks in [`docs_checks`] enforce it, and
    /// `embeds::DocsSimProvider` boots exactly this list when the page
    /// mounts.
    pub sims: &'static [DocsSimSpec],
}

/// Every article, in sidebar order. The first entry is the landing page.
pub const PAGES: &[DocPage] = &[
    DocPage {
        slug: "guide",
        title: "Welcome",
        markdown: include_str!("../../../../../docs/user-guide/README.md"),
        sims: &[DocsSimSpec {
            name: "main",
            example_id: "examples/plasma-duo",
        }],
    },
    DocPage {
        slug: "what-is-a-shader",
        title: "What's a shader?",
        markdown: include_str!("../../../../../docs/user-guide/what-is-a-shader.md"),
        sims: &[DocsSimSpec {
            name: "main",
            example_id: "examples/plasma-duo",
        }],
    },
    DocPage {
        slug: "the-peach",
        title: "The peach",
        markdown: include_str!("../../../../../docs/user-guide/the-peach.md"),
        // Two sims, because the article's argument IS the pair: the same
        // artwork, the same wire, the same patch files, declared 1D and 2D.
        // A reader who cannot see them side by side has to take it on faith.
        sims: &[
            DocsSimSpec {
                name: "twod",
                example_id: "examples/peach-2d",
            },
            DocsSimSpec {
                name: "oned",
                example_id: "examples/peach-1d",
            },
        ],
    },
    DocPage {
        slug: "patching-the-dome",
        title: "Patching the dome",
        markdown: include_str!("../../../../../docs/user-guide/patching-the-dome.md"),
        sims: &[DocsSimSpec {
            name: "main",
            example_id: "examples/small-dome",
        }],
    },
    DocPage {
        slug: "brightness-and-smooth-fades",
        title: "Brightness & smooth fades",
        markdown: include_str!("../../../../../docs/user-guide/brightness-and-smooth-fades.md"),
        sims: &[],
    },
];

/// Resolve a route slug to an article, landing-page fallback included —
/// unknown slugs are user input (typed URLs), not errors.
pub fn page_for(slug: Option<&str>) -> &'static DocPage {
    slug.and_then(|slug| PAGES.iter().find(|page| page.slug == slug))
        .unwrap_or(&PAGES[0])
}

/// Code listings an article can show by id (the `code-figure` embed).
pub(crate) mod code_figures;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_missing_slugs_land_on_the_guide() {
        assert_eq!(page_for(None).slug, "guide");
        assert_eq!(page_for(Some("no-such-page")).slug, "guide");
        assert_eq!(
            page_for(Some("brightness-and-smooth-fades")).slug,
            "brightness-and-smooth-fades"
        );
    }

    /// A declared sim names an embedded example that must actually exist —
    /// otherwise the article's hero boots nothing and the failure only shows
    /// up in a browser.
    #[test]
    fn declared_sims_name_real_embedded_examples() {
        for page in PAGES {
            for sim in page.sims {
                assert!(
                    lpa_studio_core::app::home::embedded_example(sim.example_id).is_some(),
                    "docs page `{}` declares sim `{}` on unknown example `{}`; known ids: {:?}",
                    page.slug,
                    sim.name,
                    sim.example_id,
                    lpa_studio_core::app::home::embedded_examples()
                        .iter()
                        .map(|example| example.id)
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    /// Sim handles are how articles address a page's sims; two sims with the
    /// same handle on one page makes `sim=` ambiguous.
    #[test]
    fn sim_handles_are_unique_per_page() {
        for page in PAGES {
            for (index, sim) in page.sims.iter().enumerate() {
                assert!(
                    !page.sims[..index]
                        .iter()
                        .any(|other| other.name == sim.name),
                    "docs page `{}` declares sim `{}` twice",
                    page.slug,
                    sim.name
                );
            }
        }
    }

    #[test]
    fn compiled_in_articles_are_nonempty() {
        for page in PAGES {
            assert!(
                !page.markdown.trim().is_empty(),
                "{} compiled in empty",
                page.slug
            );
        }
    }
}

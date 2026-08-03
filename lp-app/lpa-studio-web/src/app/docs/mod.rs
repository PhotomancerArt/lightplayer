//! The in-app docs section: a minimal static surface over the repo's
//! `docs/user-guide/` articles, rendered at `#/docs[/<slug>]`.
//!
//! Deliberately simple — a compiled-in manifest and a sidebar+article
//! layout. Adding an article is one file in `docs/user-guide/` plus one
//! [`PAGES`] entry. The interactive-docs vision (live nodes embedded in
//! articles) is a separate future initiative; nothing here should grow
//! toward it ahead of that plan.

pub mod docs_page;

pub use docs_page::DocsPage;

/// One compiled-in article.
#[derive(Debug, PartialEq)]
pub struct DocPage {
    /// URL segment: `#/docs/<slug>`. The landing page's slug never appears
    /// in URLs it emits, but deep links to it still resolve.
    pub slug: &'static str,
    /// Sidebar label.
    pub title: &'static str,
    /// The article body, compiled in from `docs/user-guide/`.
    pub markdown: &'static str,
}

/// Every article, in sidebar order. The first entry is the landing page.
pub const PAGES: &[DocPage] = &[
    DocPage {
        slug: "guide",
        title: "User guide",
        markdown: include_str!("../../../../../docs/user-guide/README.md"),
    },
    DocPage {
        slug: "brightness-and-smooth-fades",
        title: "Brightness & smooth fades",
        markdown: include_str!("../../../../../docs/user-guide/brightness-and-smooth-fades.md"),
    },
];

/// Resolve a route slug to an article, landing-page fallback included —
/// unknown slugs are user input (typed URLs), not errors.
pub fn page_for(slug: Option<&str>) -> &'static DocPage {
    slug.and_then(|slug| PAGES.iter().find(|page| page.slug == slug))
        .unwrap_or(&PAGES[0])
}

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

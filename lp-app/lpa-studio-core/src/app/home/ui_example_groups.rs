//! Catalog cards grouped by kind — the one derivation the home landing,
//! Explore and the device card's project picker all render (catalog
//! content vision D17: "grouped by category" IS the product of the work;
//! the home page read as jumbled because pieces and effects shared one
//! grid).
//!
//! Core owns the grouping and the words; the web lays the sections out.
//! Order is the catalog's bucket rank — real pieces first, then patterns —
//! and templates (D14) are excluded from every grid surface once the kind
//! exists. A `featured` manifest flag is the declared hook for a curated
//! home once Explore returns as the full list; it is not built.

use lpc_model::ProjectKind;

use super::ui_example_card::UiExampleCard;

/// One labelled section of catalog cards.
#[derive(Clone, Debug, PartialEq)]
pub struct UiExampleGroup {
    /// Stable key for the section (`projects` | `patterns`).
    pub key: &'static str,
    /// The section heading.
    pub label: &'static str,
    /// One line under the heading saying what the section holds.
    pub lede: &'static str,
    pub cards: Vec<UiExampleCard>,
}

/// The sections, in display order; empty sections are omitted.
pub fn example_groups(cards: &[UiExampleCard]) -> Vec<UiExampleGroup> {
    let mut projects = Vec::new();
    let mut patterns = Vec::new();
    for card in cards {
        match card.kind {
            ProjectKind::Pattern { .. } => patterns.push(card.clone()),
            ProjectKind::General | ProjectKind::Show | ProjectKind::Rig { .. } => {
                projects.push(card.clone());
            }
        }
    }
    [
        UiExampleGroup {
            key: "projects",
            label: PROJECTS_LABEL,
            lede: PROJECTS_LEDE,
            cards: projects,
        },
        UiExampleGroup {
            key: "patterns",
            label: PATTERNS_LABEL,
            lede: PATTERNS_LEDE,
            cards: patterns,
        },
    ]
    .into_iter()
    .filter(|group| !group.cards.is_empty())
    .collect()
}

/// The heading over the real pieces.
pub const PROJECTS_LABEL: &str = "Projects";
/// The heading over the single-effect patterns.
pub const PATTERNS_LABEL: &str = "Patterns";
pub const PROJECTS_LEDE: &str =
    "Real pieces, wired the way they were built. Open one in the simulator, then make it yours.";
pub const PATTERNS_LEDE: &str =
    "Single effects on a test rig. Each exports an effect you can import into your own project.";

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, kind: ProjectKind) -> UiExampleCard {
        UiExampleCard {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            description: String::new(),
        }
    }

    fn pattern() -> ProjectKind {
        ProjectKind::Pattern {
            exports: vec!["effect".to_string()],
        }
    }

    #[test]
    fn projects_come_first_then_patterns_in_the_given_order() {
        let groups = example_groups(&[
            card("catalog/comet", pattern()),
            card("catalog/fyeah-sign", ProjectKind::General),
            card("catalog/pulse", pattern()),
            card("catalog/zook-dome", ProjectKind::General),
        ]);
        let shape: Vec<(&str, Vec<&str>)> = groups
            .iter()
            .map(|g| (g.key, g.cards.iter().map(|c| c.id.as_str()).collect()))
            .collect();
        assert_eq!(
            shape,
            vec![
                ("projects", vec!["catalog/fyeah-sign", "catalog/zook-dome"]),
                ("patterns", vec!["catalog/comet", "catalog/pulse"]),
            ]
        );
        assert_eq!(groups[0].label, "Projects");
        assert_eq!(groups[1].label, "Patterns");
    }

    #[test]
    fn empty_sections_are_omitted() {
        let groups = example_groups(&[card("catalog/pulse", pattern())]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key, "patterns");
        assert!(example_groups(&[]).is_empty());
    }

    /// The catalog itself: fifteen entries, eight pieces and seven patterns
    /// (fault-demo is a test rig: its unbounded loop has no fuel meter on a
    /// GPU tier — `docs/defects/2026-09-06-gpu-tier-executes-unbounded-shaders.md`).
    #[test]
    fn the_catalog_splits_eight_and_seven() {
        let cards: Vec<UiExampleCard> = crate::app::home::embedded_examples()
            .iter()
            .map(UiExampleCard::from_embedded)
            .collect();
        let groups = example_groups(&cards);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].cards.len(), 8, "{:?}", groups[0]);
        assert_eq!(groups[1].cards.len(), 7, "{:?}", groups[1]);
    }
}

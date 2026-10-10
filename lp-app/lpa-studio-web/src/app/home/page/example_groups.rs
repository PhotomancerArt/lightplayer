//! The catalog's sections, drawn once for both pages that show them: the
//! home page (Example projects, Example patterns, between hairline rules)
//! and Explore (Projects, Patterns, as titles).
//!
//! Core owns the grouping and the words (`example_groups`,
//! `example_page_label`); this lays the cards out. The page filters the
//! groups by the selected tab (`UiHomeTab::shows`) before it hands them
//! over, so what is drawn is exactly what the tab shows.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiExampleGroup, example_page_label};

use super::page_section::PageSection;
use crate::app::home::example_card::ExampleCard;
use crate::app::home::{card_grid_class, section_title_class};
use crate::base::HelpLink;

/// How each group's heading is drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExampleHeading {
    /// The home page's: the page word ("Example projects") between rules,
    /// in a [`PageSection`].
    Rule,
    /// Explore's: the group's own label as a title, with the patterns'
    /// "What's a shader?" help link beside it.
    Title,
}

/// The groups' sections, in the order given. `first_id` goes on the first
/// group's section as its element id (the Connect a board hint scrolls
/// to it).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ExampleGroups(
    groups: Vec<UiExampleGroup>,
    /// The card whose open is in flight, if any (`UiHomeView::opening`).
    opening: Option<String>,
    busy: bool,
    heading: ExampleHeading,
    #[props(default)] first_id: Option<&'static str>,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        for (index , group) in groups.into_iter().enumerate() {
            match heading {
                ExampleHeading::Rule => rsx! {
                    PageSection {
                        key: "{group.key}",
                        title: group_heading(heading, &group),
                        id: first_id.filter(|_| index == 0),
                        {group_body(group, opening.clone(), busy, on_action)}
                    }
                },
                ExampleHeading::Title => rsx! {
                    section { key: "{group.key}", class: "tw:grid tw:gap-3",
                        header { class: "tw:flex tw:items-center tw:gap-3",
                            h2 { class: section_title_class(), "{group_heading(heading, &group)}" }
                            // Where a WLED person goes looking for "the
                            // effects list" — the patterns section is the
                            // exact spot the shader question arises.
                            if group.key == "patterns" {
                                HelpLink {
                                    href: crate::app::docs::docs_links::what_is_a_shader::HREF,
                                    title: "What's a shader?",
                                }
                            }
                        }
                        {group_body(group, opening.clone(), busy, on_action)}
                    }
                },
            }
        }
    }
}

/// A group's heading in this style: the page word on the home page, the
/// group's own label on Explore.
pub(crate) fn group_heading(heading: ExampleHeading, group: &UiExampleGroup) -> &'static str {
    match heading {
        ExampleHeading::Rule => example_page_label(group.key),
        ExampleHeading::Title => group.label,
    }
}

/// The lede and the cards.
fn group_body(
    group: UiExampleGroup,
    opening: Option<String>,
    busy: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-muted-foreground", "{group.lede}" }
        div { class: card_grid_class(),
            for card in group.cards {
                ExampleCard {
                    key: "{card.id}",
                    opening: opening.as_deref() == Some(card.id.as_str()),
                    busy,
                    card,
                    on_action,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiExampleCard, example_groups};
    use lpc_model::ProjectKind;

    use super::*;

    #[test]
    fn the_home_page_words_the_groups_as_examples_and_explore_keeps_its_titles() {
        let card = |id: &str, kind| UiExampleCard {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            description: String::new(),
        };
        let groups = example_groups(&[
            card("catalog/fyeah-sign", ProjectKind::General),
            card(
                "catalog/pulse",
                ProjectKind::Pattern {
                    exports: vec!["effect".to_string()],
                },
            ),
        ]);
        let words = |heading| -> Vec<&'static str> {
            groups
                .iter()
                .map(|group| group_heading(heading, group))
                .collect()
        };
        assert_eq!(
            words(ExampleHeading::Rule),
            ["Example projects", "Example patterns"]
        );
        assert_eq!(words(ExampleHeading::Title), ["Projects", "Patterns"]);
    }
}

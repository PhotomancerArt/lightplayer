//! What the home page draws, in order, for the sections a tab shows.
//!
//! Core says which sections a tab shows and what each holds
//! ([`UiHomeSections::visible`](lpa_studio_core::UiHomeSections::visible));
//! these are the page's own drawing rules on top of that, kept in one pure
//! function so they are tested rather than buried in markup:
//!
//! - a first visit has no board to unlock, so no "Unlocking your boards";
//! - a browser whose library did not mount draws one short line in place of
//!   the projects sections (Other projects, Projects, Your patterns);
//! - the catalog's sections are drawn after the loop, by the examples
//!   component the page shares with Explore, so they are not parts here.

use lpa_studio_core::UiHomeSection;

/// One thing the page draws between its controls and the catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HomePart {
    /// A section, drawn as itself.
    Section(UiHomeSection),
    /// "Your project library is not available in this browser. Examples
    /// still run."
    LibraryUnavailable,
}

/// The line a browser without a library reads, once.
pub(crate) const LIBRARY_UNAVAILABLE_LINE: &str =
    "Your project library is not available in this browser. Examples still run.";

/// The parts for `visible` (core's order), on a page whose library is (or
/// is not) available, for a newcomer or not.
pub(crate) fn home_parts(
    visible: &[UiHomeSection],
    library_available: bool,
    newcomer: bool,
) -> Vec<HomePart> {
    let mut parts = Vec::new();
    for section in visible {
        let part = match section {
            UiHomeSection::ExampleProjects | UiHomeSection::ExamplePatterns => None,
            UiHomeSection::UnlockingYourBoards if newcomer => None,
            UiHomeSection::OtherProjects
            | UiHomeSection::Projects
            | UiHomeSection::YourPatterns
                if !library_available =>
            {
                Some(HomePart::LibraryUnavailable)
            }
            other => Some(HomePart::Section(*other)),
        };
        if let Some(part) = part
            && !parts.contains(&part)
        {
            parts.push(part);
        }
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use UiHomeSection::*;
    use lpa_studio_core::{UiHomeSections, UiHomeTab};

    fn visible(tab: UiHomeTab) -> Vec<UiHomeSection> {
        UiHomeSections::default().visible(tab)
    }

    #[test]
    fn the_all_tab_draws_boards_then_projects_then_the_archive() {
        assert_eq!(
            home_parts(&visible(UiHomeTab::All), true, false),
            [
                OnlineBoards,
                ConnectBoard,
                OfflineBoards,
                UnlockingYourBoards,
                OtherProjects,
                YourPatterns,
                ArchivedProjects,
            ]
            .map(HomePart::Section)
        );
    }

    #[test]
    fn a_newcomer_has_no_keys_fold() {
        let parts = home_parts(&visible(UiHomeTab::All), true, true);
        assert!(!parts.contains(&HomePart::Section(UnlockingYourBoards)));
        assert!(parts.contains(&HomePart::Section(OtherProjects)));
    }

    #[test]
    fn without_a_library_one_line_stands_in_for_the_projects_sections() {
        assert_eq!(
            home_parts(&visible(UiHomeTab::All), false, false),
            vec![
                HomePart::Section(OnlineBoards),
                HomePart::Section(ConnectBoard),
                HomePart::Section(OfflineBoards),
                HomePart::Section(UnlockingYourBoards),
                HomePart::LibraryUnavailable,
                HomePart::Section(ArchivedProjects),
            ]
        );
        assert_eq!(
            home_parts(&visible(UiHomeTab::Projects), false, false),
            vec![
                HomePart::LibraryUnavailable,
                HomePart::Section(ArchivedProjects)
            ]
        );
        assert_eq!(
            home_parts(&visible(UiHomeTab::Patterns), false, false),
            vec![HomePart::LibraryUnavailable]
        );
    }

    #[test]
    fn the_projects_tab_lists_the_whole_library_then_the_archive() {
        assert_eq!(
            home_parts(&visible(UiHomeTab::Projects), true, false),
            vec![
                HomePart::Section(Projects),
                HomePart::Section(ArchivedProjects)
            ]
        );
    }
}

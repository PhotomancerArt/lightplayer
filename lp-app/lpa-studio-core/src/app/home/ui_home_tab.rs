//! The home page's tabs, and which sections each one shows (PD5).
//!
//! A tab is view state: the web keeps the selected one in a signal and no
//! `UiAction` presses it. What core owns is the table — which tab shows
//! which section — so the web decides nothing about membership and the app
//! agent can say "the Projects tab" and mean the same thing a person sees.

use super::ui_home_section::UiHomeSection;

/// One tab of the home page's strip.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum UiHomeTab {
    /// Everything: boards, then projects, then the examples.
    #[default]
    All,
    /// The boards half.
    Boards,
    /// The library's projects.
    Projects,
    /// The patterns, yours and the catalog's.
    Patterns,
}

impl UiHomeTab {
    /// The strip, left to right.
    pub const ALL: [Self; 4] = [Self::All, Self::Boards, Self::Projects, Self::Patterns];

    /// The tab's label, exactly as the strip words it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Boards => "Boards",
            Self::Projects => "Projects",
            Self::Patterns => "Patterns",
        }
    }

    /// A stable lowercase key, for the web's tab strip.
    pub const fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Boards => "boards",
            Self::Projects => "projects",
            Self::Patterns => "patterns",
        }
    }

    /// The tab a [`Self::key`] names; `None` for anything else.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.key() == key)
    }

    /// Whether this tab shows `section`. The table of `plan.md` ("Which tab
    /// shows which section"), the only place it is written down:
    ///
    /// | Section | All | Boards | Projects | Patterns |
    /// |---|:-:|:-:|:-:|:-:|
    /// | Online boards | ● | ● | | |
    /// | Connect a board | ● | ● | | |
    /// | Offline boards | ● | ● | | |
    /// | Unlocking your boards | ● | ● | | |
    /// | Other projects | ● | | | |
    /// | Projects | | | ● | |
    /// | Your patterns | ● | | | ● |
    /// | Archived projects | ● | | ● | |
    /// | Example projects | ● | | ● | |
    /// | Example patterns | ● | | | ● |
    pub const fn shows(self, section: UiHomeSection) -> bool {
        use UiHomeSection::*;
        match (self, section) {
            (Self::All, Projects) => false,
            (Self::All, _) => true,
            (Self::Boards, OnlineBoards | ConnectBoard | OfflineBoards | UnlockingYourBoards) => {
                true
            }
            (Self::Projects, Projects | ArchivedProjects | ExampleProjects) => true,
            (Self::Patterns, YourPatterns | ExamplePatterns) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use UiHomeSection::*;

    /// The plan's table, one row per section: which of All, Boards,
    /// Projects, Patterns show it.
    const TABLE: [(UiHomeSection, [bool; 4]); 10] = [
        (OnlineBoards, [true, true, false, false]),
        (ConnectBoard, [true, true, false, false]),
        (OfflineBoards, [true, true, false, false]),
        (UnlockingYourBoards, [true, true, false, false]),
        (OtherProjects, [true, false, false, false]),
        (Projects, [false, false, true, false]),
        (YourPatterns, [true, false, false, true]),
        (ArchivedProjects, [true, false, true, false]),
        (ExampleProjects, [true, false, true, false]),
        (ExamplePatterns, [true, false, false, true]),
    ];

    #[test]
    fn every_tab_shows_exactly_the_sections_of_the_table() {
        assert_eq!(TABLE.len(), UiHomeSection::ALL.len());
        for (section, shown) in TABLE {
            for (tab, expected) in UiHomeTab::ALL.into_iter().zip(shown) {
                assert_eq!(
                    tab.shows(section),
                    expected,
                    "{} on the {} tab",
                    section.label(),
                    tab.label()
                );
            }
        }
    }

    #[test]
    fn the_strip_reads_all_boards_projects_patterns() {
        let labels: Vec<&str> = UiHomeTab::ALL.iter().map(|tab| tab.label()).collect();
        assert_eq!(labels, ["All", "Boards", "Projects", "Patterns"]);
        assert_eq!(UiHomeTab::default(), UiHomeTab::All);
    }

    #[test]
    fn from_key_round_trips_every_key_and_refuses_the_rest() {
        for tab in UiHomeTab::ALL {
            assert_eq!(UiHomeTab::from_key(tab.key()), Some(tab));
        }
        assert_eq!(UiHomeTab::from_key("All"), None);
        assert_eq!(UiHomeTab::from_key(""), None);
        assert_eq!(UiHomeTab::from_key("devices"), None);
    }
}

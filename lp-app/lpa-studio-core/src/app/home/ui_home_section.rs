//! The sections the home page can hold, named once.
//!
//! The page is one column of sections; which of them a tab shows is
//! [`UiHomeTab::shows`](super::ui_home_tab::UiHomeTab::shows), and what each
//! holds is [`UiHomeSections`](super::ui_home_sections::UiHomeSections). The
//! words here are the page's own (`docs/style/language.md`, "Boards and the
//! home page"), so the web draws no heading core did not name.

/// One section of the home page.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiHomeSection {
    /// Boards Studio can see right now, pending links first.
    OnlineBoards,
    /// The three ways a board comes in, and "start a board here".
    ConnectBoard,
    /// Boards Studio remembers and cannot see.
    OfflineBoards,
    /// This browser's name, the account key and the passwords.
    UnlockingYourBoards,
    /// Library projects no board plays.
    OtherProjects,
    /// Every library project that is not a pattern (the Projects tab).
    Projects,
    /// Library projects of kind pattern.
    YourPatterns,
    /// The archive drawer.
    ArchivedProjects,
    /// The catalog's real pieces.
    ExampleProjects,
    /// The catalog's single effects.
    ExamplePatterns,
}

impl UiHomeSection {
    /// Every section, in the order the page draws them. A tab draws the
    /// ones it shows in this order ([`UiHomeSections::visible`]).
    ///
    /// [`UiHomeSections::visible`]: super::ui_home_sections::UiHomeSections::visible
    pub const ALL: [Self; 10] = [
        Self::OnlineBoards,
        Self::ConnectBoard,
        Self::OfflineBoards,
        Self::UnlockingYourBoards,
        Self::OtherProjects,
        Self::Projects,
        Self::YourPatterns,
        Self::ArchivedProjects,
        Self::ExampleProjects,
        Self::ExamplePatterns,
    ];

    /// The section's heading, exactly as the page words it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::OnlineBoards => "Online boards",
            Self::ConnectBoard => "Connect a board",
            Self::OfflineBoards => "Offline boards",
            Self::UnlockingYourBoards => "Unlocking your boards",
            Self::OtherProjects => "Other projects",
            Self::Projects => "Projects",
            Self::YourPatterns => "Your patterns",
            Self::ArchivedProjects => "Archived projects",
            Self::ExampleProjects => "Example projects",
            Self::ExamplePatterns => "Example patterns",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_headings_are_the_pages_own_words() {
        let words: Vec<&str> = UiHomeSection::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(
            words,
            [
                "Online boards",
                "Connect a board",
                "Offline boards",
                "Unlocking your boards",
                "Other projects",
                "Projects",
                "Your patterns",
                "Archived projects",
                "Example projects",
                "Example patterns",
            ]
        );
    }

    #[test]
    fn every_section_is_listed_once() {
        for (index, section) in UiHomeSection::ALL.iter().enumerate() {
            assert!(
                !UiHomeSection::ALL[index + 1..].contains(section),
                "{section:?} is listed twice"
            );
        }
    }
}

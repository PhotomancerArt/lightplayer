//! [`UiPanel`]: a drawer or panel the web has open over the page.

/// One drawer or panel open over the page — web chrome whose open state
/// lives in the page (like a popover's), reported so core knows it is
/// there. A node card's own sections are not here: core already owns
/// those (`NodeCardUiState`).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiPanel {
    /// The app chat drawer.
    AppChat,
    /// The ⌘K command palette.
    CommandPalette,
    /// The header session control's panel, at one section.
    Session(UiSessionSection),
}

/// Which section the header session control's panel shows.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiSessionSection {
    /// What is running: the device.
    Device,
    /// What document is open: the project.
    Project,
    /// What is in flight: unsaved changes.
    Changes,
    /// The project's history.
    History,
}

impl UiPanel {
    /// The panel in a few plain words, for the agent's readout.
    pub fn describe(self) -> &'static str {
        match self {
            Self::AppChat => "the assistant chat",
            Self::CommandPalette => "the command palette",
            Self::Session(UiSessionSection::Device) => "the session panel's device section",
            Self::Session(UiSessionSection::Project) => "the session panel's project section",
            Self::Session(UiSessionSection::Changes) => "the session panel's changes section",
            Self::Session(UiSessionSection::History) => "the session panel's history section",
        }
    }
}

//! [`UiPlace`]: where the user is, as the web reports it.

use super::{UiPage, UiPanel};

/// Where the user is: the page, and the drawers and panels open over it.
///
/// The web sends it on `StudioCommand::Place` whenever the route or a
/// panel changes, and core keeps the latest. It is a fact core reads,
/// never a request: core does not navigate, open or close anything because
/// of it, and nothing persists it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPlace {
    /// The page the route shows.
    pub page: UiPage,
    /// The drawers and panels open over it, in a stable order (sorted, no
    /// repeats), so the same place always compares equal.
    pub panels: Vec<UiPanel>,
}

impl UiPlace {
    /// `page` with nothing open over it.
    pub fn new(page: UiPage) -> Self {
        Self {
            page,
            panels: Vec::new(),
        }
    }

    /// This place with `panel` open too.
    #[must_use]
    pub fn with_panel(mut self, panel: UiPanel) -> Self {
        if let Err(at) = self.panels.binary_search(&panel) {
            self.panels.insert(at, panel);
        }
        self
    }

    /// Whether `panel` is open.
    pub fn has_panel(&self, panel: UiPanel) -> bool {
        self.panels.contains(&panel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::studio::place::UiSessionSection;

    #[test]
    fn panels_are_kept_sorted_and_once_so_one_place_compares_equal() {
        let one = UiPlace::new(UiPage::Devices)
            .with_panel(UiPanel::Session(UiSessionSection::Device))
            .with_panel(UiPanel::AppChat)
            .with_panel(UiPanel::AppChat);
        let other = UiPlace::new(UiPage::Devices)
            .with_panel(UiPanel::AppChat)
            .with_panel(UiPanel::Session(UiSessionSection::Device));

        assert_eq!(one, other);
        assert_eq!(one.panels.len(), 2);
        assert!(one.has_panel(UiPanel::AppChat));
        assert!(!one.has_panel(UiPanel::CommandPalette));
    }
}

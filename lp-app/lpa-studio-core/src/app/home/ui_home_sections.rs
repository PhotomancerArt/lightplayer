//! What the home page holds, in what order — as data (PD2).
//!
//! The web draws these lists and decides nothing about which board or which
//! project sits in which section. The app agent reads the same facts
//! (`app_agent_readout::home_lines`), so it sees the page the way a person
//! does. [`build_home_sections`](super::home_sections_builder::build_home_sections)
//! fills it from the library and the roster; the board↔project join
//! ([`BoardProjects`](crate::BoardProjects)) decides which boards play which
//! project, and this module never decides that again.

use crate::{DeviceId, OfferPath};

use super::ui_home_section::UiHomeSection;
use super::ui_home_tab::UiHomeTab;

/// The home page's sections, filled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiHomeSections {
    /// Pending links first (a board just plugged in is what the user is
    /// looking at), then the connected boards in the roster's order.
    pub online: Vec<UiHomeBoard>,
    /// Boards Studio remembers and cannot see, in the roster's last-seen
    /// order.
    pub offline: Vec<UiHomeBoard>,
    /// The Connect a board section's offers.
    pub connect: UiHomeConnect,
    /// The `prj…` uids of the library projects no board plays (and that are
    /// not patterns), newest saved first.
    pub other_projects: Vec<String>,
    /// The `prj…` uids of every library project that is not a pattern,
    /// newest saved first. A project a board plays is here too: the
    /// Projects tab is where an offline board's project keeps its Rename,
    /// Duplicate, Download and Delete.
    pub projects: Vec<String>,
    /// The `prj…` uids of the library's pattern projects, newest saved
    /// first.
    pub patterns: Vec<String>,
    /// No board of any kind and no library project of any kind: a first
    /// visit. The page then shows no tabs and no switch, and Connect a
    /// board comes first.
    pub newcomer: bool,
}

impl UiHomeSections {
    /// The number a tab's chip shows; `None` for All, which has none.
    pub fn count(&self, tab: UiHomeTab) -> Option<usize> {
        match tab {
            UiHomeTab::All => None,
            UiHomeTab::Boards => Some(self.online.len() + self.offline.len()),
            UiHomeTab::Projects => Some(self.projects.len()),
            UiHomeTab::Patterns => Some(self.patterns.len()),
        }
    }

    /// The sections `tab` shows, in the order the page draws them. Whether
    /// an empty one is drawn is the web's call, by the page's rules (Online
    /// and Offline boards hide when empty; Connect a board never does).
    pub fn visible(&self, tab: UiHomeTab) -> Vec<UiHomeSection> {
        UiHomeSection::ALL
            .into_iter()
            .filter(|section| tab.shows(*section))
            .collect()
    }
}

/// One board in Online or Offline boards.
#[derive(Clone, Debug, PartialEq)]
pub struct UiHomeBoard {
    /// The roster handle (a pending link has one too). The web finds the
    /// board's views in the roster by it.
    pub id: DeviceId,
    pub kind: UiHomeBoardKind,
    /// What the board is called.
    pub title: String,
    /// The roster's own words: its state ("Ready", "Identifying…"), or for
    /// a remembered board how long ago it was heard.
    pub status: String,
    /// The library project it plays, by name (the join's answer, J2), else
    /// the label the board itself reports; `None` when neither is known.
    pub project: Option<String>,
    /// The verbs a list row may draw, by their last path segment, first
    /// published wins. The web resolves each against the board's published
    /// offers; core never puts an action here.
    pub row_verbs: Vec<&'static str>,
}

/// Where a board stands on the page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiHomeBoardKind {
    /// A link still being identified.
    Pending,
    /// On the bus (or reached some other way) right now.
    Connected,
    /// Remembered, not reachable now.
    Remembered,
}

/// The Connect a board section.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiHomeConnect {
    /// The offers the section draws, by path, in the order it draws them:
    /// `devices/connect-usb`, `devices/connect-ble`,
    /// `devices/connect-wifi-address`, and `devices/new-sim` (only when the
    /// page has a transport to start a board on). Each is published in the
    /// offer tree; the web presses what the tree holds.
    pub offers: Vec<OfferPath>,
    /// Add the one hint line for a first visit (the same as
    /// [`UiHomeSections::newcomer`]).
    pub welcome: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(id: u64, kind: UiHomeBoardKind) -> UiHomeBoard {
        UiHomeBoard {
            id: DeviceId(id),
            kind,
            title: format!("board {id}"),
            status: "Ready".to_string(),
            project: None,
            row_verbs: Vec::new(),
        }
    }

    fn sections() -> UiHomeSections {
        UiHomeSections {
            online: vec![
                board(1, UiHomeBoardKind::Pending),
                board(2, UiHomeBoardKind::Connected),
            ],
            offline: vec![board(3, UiHomeBoardKind::Remembered)],
            projects: vec!["prja".to_string(), "prjb".to_string(), "prjc".to_string()],
            patterns: vec!["prjp".to_string()],
            ..UiHomeSections::default()
        }
    }

    #[test]
    fn counts_follow_the_tab() {
        let sections = sections();
        assert_eq!(sections.count(UiHomeTab::All), None);
        assert_eq!(sections.count(UiHomeTab::Boards), Some(3));
        assert_eq!(sections.count(UiHomeTab::Projects), Some(3));
        assert_eq!(sections.count(UiHomeTab::Patterns), Some(1));
        assert_eq!(
            UiHomeSections::default().count(UiHomeTab::Boards),
            Some(0),
            "a tab with nothing in it counts zero, not nothing"
        );
    }

    #[test]
    fn visible_lists_the_tabs_sections_in_page_order() {
        use UiHomeSection::*;
        let sections = sections();
        assert_eq!(
            sections.visible(UiHomeTab::All),
            [
                OnlineBoards,
                ConnectBoard,
                OfflineBoards,
                UnlockingYourBoards,
                OtherProjects,
                YourPatterns,
                ArchivedProjects,
                ExampleProjects,
                ExamplePatterns,
            ]
        );
        assert_eq!(
            sections.visible(UiHomeTab::Boards),
            [
                OnlineBoards,
                ConnectBoard,
                OfflineBoards,
                UnlockingYourBoards
            ]
        );
        assert_eq!(
            sections.visible(UiHomeTab::Projects),
            [Projects, ArchivedProjects, ExampleProjects]
        );
        assert_eq!(
            sections.visible(UiHomeTab::Patterns),
            [YourPatterns, ExamplePatterns]
        );
    }
}

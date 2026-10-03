//! [`UiPage`]: which page the user is on — the web router's route, said in
//! core's words.

use crate::OfferPath;

/// Which page the user is on: the route's kind plus the ids it carries.
///
/// One variant per route the web router knows (`router.rs`), so a new
/// route is a new variant here, and the web's mapping fails to compile
/// until it says which page it is.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiPage {
    /// The landing page (`/`).
    Home,
    /// The devices gallery (`/devices`).
    Devices,
    /// The projects library (`/projects`).
    Projects,
    /// The explore section (`/explore`).
    Explore,
    /// The signed-in account's page (`/account`).
    Account,
    /// Where a shared device password lands (`/unlock`).
    Unlock,
    /// A library project in the editor (`/p/<slug>-prj…`), by its uid.
    Project { uid: String, view: UiProjectView },
    /// An embedded example in the editor (`/p/<slug>`), by its slug.
    Example { slug: String, view: UiProjectView },
    /// A device's session in the editor (`/device/<dev-uid>`).
    Device { uid: String, view: UiProjectView },
    /// The in-app docs, at one article or the guide's landing.
    Docs { article: Option<String> },
    /// The boards catalog, at one board or the list.
    Boards { board: Option<String> },
    /// The board display editor.
    BoardEditor,
    /// The story book (dev).
    Stories,
}

/// Which surface of the project editor the page shows: the `/play`,
/// `/patch` and `/mapping` suffixes, or none.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UiProjectView {
    /// The node workspace (no suffix).
    #[default]
    Nodes,
    /// Play mode: the root module's panel, nothing else.
    Play,
    /// The patching view.
    Patch,
    /// The mapping view.
    Mapping,
}

impl UiPage {
    /// Whether the page is the project editor (any of its views).
    pub fn is_editor(&self) -> bool {
        matches!(
            self,
            Self::Project { .. } | Self::Example { .. } | Self::Device { .. }
        )
    }

    /// The offer area this page is about: `project` in the editor,
    /// `devices` on the gallery pages, `None` where Studio offers nothing
    /// (docs, the boards catalog, the board editor, the story book).
    pub fn offer_area(&self) -> Option<OfferPath> {
        match self {
            Self::Project { .. } | Self::Example { .. } | Self::Device { .. } => {
                Some(OfferPath::project())
            }
            Self::Home
            | Self::Devices
            | Self::Projects
            | Self::Explore
            | Self::Account
            | Self::Unlock => Some(OfferPath::devices()),
            Self::Docs { .. } | Self::Boards { .. } | Self::BoardEditor | Self::Stories => None,
        }
    }

    /// The page in a few plain words, for the agent's readout:
    /// `project editor, patch view`, `devices page`, `docs: wiring`.
    pub fn describe(&self) -> String {
        match self {
            Self::Home => "home".to_string(),
            Self::Devices => "devices page".to_string(),
            Self::Projects => "projects library".to_string(),
            Self::Explore => "explore page".to_string(),
            Self::Account => "account page".to_string(),
            Self::Unlock => "unlock page (a shared device password)".to_string(),
            Self::Project { view, .. } => format!("project editor, {}", view.describe()),
            Self::Example { slug, view } => {
                format!(
                    "project editor on the example {slug:?}, {}",
                    view.describe()
                )
            }
            Self::Device { view, .. } => {
                format!("project editor on a device's session, {}", view.describe())
            }
            Self::Docs { article: None } => "docs".to_string(),
            Self::Docs {
                article: Some(article),
            } => format!("docs: {article}"),
            Self::Boards { board: None } => "boards catalog".to_string(),
            Self::Boards { board: Some(board) } => format!("boards catalog: {board}"),
            Self::BoardEditor => "board editor".to_string(),
            Self::Stories => "story book".to_string(),
        }
    }
}

impl UiProjectView {
    /// `nodes view`, `play mode`, `patch view`, `mapping view`.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Nodes => "nodes view",
            Self::Play => "play mode",
            Self::Patch => "patch view",
            Self::Mapping => "mapping view",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editor_is_about_the_project_and_the_galleries_about_devices() {
        let editor = UiPage::Project {
            uid: "prj123".to_string(),
            view: UiProjectView::Patch,
        };
        assert!(editor.is_editor());
        assert_eq!(editor.offer_area(), Some(OfferPath::project()));
        assert_eq!(editor.describe(), "project editor, patch view");
        assert_eq!(UiPage::Devices.offer_area(), Some(OfferPath::devices()));
        assert_eq!(UiPage::Home.offer_area(), Some(OfferPath::devices()));
        assert_eq!(
            UiPage::Docs {
                article: Some("wiring".to_string())
            }
            .offer_area(),
            None
        );
    }
}

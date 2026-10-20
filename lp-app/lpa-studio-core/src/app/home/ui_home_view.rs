//! The home page's view model.

use crate::UiIssue;

use super::ui_example_card::UiExampleCard;
use super::ui_home_sections::UiHomeSections;
use super::ui_package_card::UiPackageCard;

/// Everything the home page renders. Present on
/// [`UiStudioView`](crate::UiStudioView) when the shell should show the
/// page instead of the pane layout.
///
/// The page is [`Self::sections`]: which board or project sits in which
/// section, in what order, and whether this is a first visit. The other
/// fields are the things those sections point at (the library's cards, the
/// roster's views, the catalog).
#[derive(Clone, Debug, PartialEq)]
pub struct UiHomeView {
    /// The library's projects, name-sorted like the library lists them.
    /// The sections name them by uid; the web resolves a uid here.
    pub projects: Vec<UiPackageCard>,
    /// The examples (embedded packages until M6).
    pub examples: Vec<UiExampleCard>,
    /// The device roster: the `lpa-devices` projection, verbatim.
    ///
    /// Not a `Ui*` mirror on purpose (M3 of the device-model rebuild). The
    /// model's `RosterView`/`DeviceView` ARE the view model — every card,
    /// label, freshness line and escape is a pure function of the fold — so
    /// there is nowhere for the page and the model to disagree.
    pub devices: crate::DeviceRosterView,
    /// What the page holds, in what order: Online and Offline boards,
    /// Connect a board, Other projects, Projects, Your patterns, and
    /// whether this is a newcomer. Filled by `StudioController::home_view`
    /// once the roster is in.
    pub sections: UiHomeSections,
    /// Whether the local library mounted; when `false` the projects
    /// sections explain instead of listing (the store banner carries the
    /// details).
    pub library_available: bool,
    /// The card key (`prj…` uid or example id) whose open is in flight, so
    /// the renderer can show it busy.
    pub opening: Option<String>,
    /// A library problem to surface on the home page.
    pub issue: Option<UiIssue>,
}

impl UiHomeView {
    /// Render as plain text lines for fallback renderers and tests.
    pub fn render_text_lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "Home: {} projects, {} examples",
            self.projects.len(),
            self.examples.len(),
        )];
        if !self.devices.roster.devices.is_empty() || !self.devices.roster.pending.is_empty() {
            lines.push(format!(
                "  devices: {} cards, {} identifying",
                self.devices.roster.devices.len(),
                self.devices.roster.pending.len()
            ));
        }
        let sections = &self.sections;
        if sections.newcomer {
            lines.push("  sections: a first visit (no boards, no projects)".to_string());
        } else {
            lines.push(format!(
                "  sections: {} online, {} offline, {} other projects, {} projects, {} patterns",
                sections.online.len(),
                sections.offline.len(),
                sections.other_projects.len(),
                sections.projects.len(),
                sections.patterns.len(),
            ));
        }
        if let Some(opening) = &self.opening {
            lines.push(format!("  opening {opening}"));
        }
        if let Some(issue) = &self.issue {
            lines.push(format!("  issue: {}", issue.message));
        }
        lines
    }
}

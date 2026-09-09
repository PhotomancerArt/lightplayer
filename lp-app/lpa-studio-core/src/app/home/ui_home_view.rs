//! The home gallery view model.

use crate::UiIssue;

use super::ui_example_card::UiExampleCard;
use super::ui_package_card::UiPackageCard;

/// Everything the home screen renders. Present on
/// [`UiStudioView`](crate::UiStudioView) when the shell should show the
/// gallery instead of the pane layout.
#[derive(Clone, Debug, PartialEq)]
pub struct UiHomeView {
    /// *Your projects* section, name-sorted like the library lists them.
    pub projects: Vec<UiPackageCard>,
    /// *Examples* section (embedded packages until M6).
    pub examples: Vec<UiExampleCard>,
    /// The device roster: the `lpa-devices` projection, verbatim.
    ///
    /// Not a `Ui*` mirror on purpose (M3 of the device-model rebuild). The
    /// model's `RosterView`/`DeviceView` ARE the view model — every card,
    /// label, freshness line and escape is a pure function of the fold — so
    /// there is nowhere for the page and the model to disagree.
    pub devices: crate::DeviceRosterView,
    /// Whether the local library mounted; when `false` the projects section
    /// explains instead of listing (the store banner carries the details).
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
        if let Some(opening) = &self.opening {
            lines.push(format!("  opening {opening}"));
        }
        if let Some(issue) = &self.issue {
            lines.push(format!("  issue: {}", issue.message));
        }
        lines
    }
}

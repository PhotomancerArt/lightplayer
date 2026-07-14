//! The home gallery view model.

use crate::UiIssue;

use super::ui_device_card::UiDeviceCard;
use super::ui_example_card::UiExampleCard;
use super::ui_package_card::UiPackageCard;

/// Everything the home screen renders. Present on
/// [`UiStudioView`](crate::UiStudioView) when the shell should show the
/// gallery instead of the pane layout.
#[derive(Clone, Debug, PartialEq)]
pub struct UiHomeView {
    /// *Connected* section: live and remembered devices. The dashed
    /// "Connect a device" card is the renderer's.
    pub devices: Vec<UiDeviceCard>,
    /// *Your projects* section, name-sorted like the library lists them.
    pub projects: Vec<UiPackageCard>,
    /// *Examples* section (embedded packages until M6).
    pub examples: Vec<UiExampleCard>,
    /// Whether the local library mounted; when `false` the projects section
    /// explains instead of listing (the store banner carries the details).
    pub library_available: bool,
    /// The card key (`prj_…` uid or example id) whose open is in flight, so
    /// the renderer can show it busy.
    pub opening: Option<String>,
    /// A provider-selection or library problem to surface on the home page.
    pub issue: Option<UiIssue>,
}

impl UiHomeView {
    /// Render as plain text lines for fallback renderers and tests.
    pub fn render_text_lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "Home: {} devices, {} projects, {} examples",
            self.devices.len(),
            self.projects.len(),
            self.examples.len()
        )];
        if let Some(opening) = &self.opening {
            lines.push(format!("  opening {opening}"));
        }
        if let Some(issue) = &self.issue {
            lines.push(format!("  issue: {}", issue.message));
        }
        lines
    }
}

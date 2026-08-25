//! The home gallery view model.

use crate::UiIssue;

use super::ui_example_card::UiExampleCard;
use super::ui_package_card::UiPackageCard;
use super::ui_sim_card::UiSimCard;

/// Everything the home screen renders. Present on
/// [`UiStudioView`](crate::UiStudioView) when the shell should show the
/// gallery instead of the pane layout.
#[derive(Clone, Debug, PartialEq)]
pub struct UiHomeView {
    /// The live simulator's card, while a sim session lives. Device cards
    /// were the rest of this section until the device system was torn down
    /// (M2 of the device-model rebuild); the rebuilt model re-adds them.
    pub sim: Option<UiSimCard>,
    /// *Your projects* section, name-sorted like the library lists them.
    pub projects: Vec<UiPackageCard>,
    /// *Examples* section (embedded packages until M6).
    pub examples: Vec<UiExampleCard>,
    /// The boards Studio still REMEMBERS, by registry name — read straight
    /// off the surviving `DeviceRegistry` (R3's keep).
    ///
    /// Names only, and read-only: while device support is being rebuilt
    /// (M2 of the device-model rebuild) the Devices page says so and lists
    /// what it remembers, so a user whose boards vanished from the roster
    /// can see that the records did not. Nothing acts on them.
    pub remembered: Vec<String>,
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
            "Home: {} runtimes, {} projects, {} examples, {} remembered",
            usize::from(self.sim.is_some()),
            self.projects.len(),
            self.examples.len(),
            self.remembered.len()
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

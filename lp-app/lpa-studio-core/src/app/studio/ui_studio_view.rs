use core::fmt::Write;

use crate::app::studio::ui_console_view::UiConsoleView;
use crate::{
    ActionPriority, UiActivityView, UiPaneView, UiStatus, UiStepState, UiViewContent,
    UxActivityTarget,
};

#[derive(Clone, Debug, PartialEq)]
pub struct UiStudioView {
    pub panes: Vec<UiPaneView>,
    /// The console slice: filtered log entries plus the filter state that
    /// produced them.
    pub console: UiConsoleView,
    /// The home gallery, when the shell should render it instead of the
    /// pane layout (no project open, no device flow engaged — M4).
    pub home: Option<Box<crate::app::home::UiHomeView>>,
    /// The `prj_…` uid of the open library package, when one backs the
    /// running project (identity for route↔view comparisons).
    pub open_project_uid: Option<String>,
    /// The open package's slug — the user-facing identifier the web shell
    /// mirrors into `#/project/<slug>` (URL follows the view, covering
    /// example opens and clearing on disconnect without action plumbing).
    pub open_project_slug: Option<String>,
    /// Connect-as-pull result for the attached DEVICE (never the sim —
    /// D22): identity + content classification. Feeds the device pane,
    /// gallery cards, and the deploy dialog (M5).
    pub device_sync: Option<crate::app::places::DeviceSyncState>,
    /// The deploy dialog, when open — rendered as a modal overlay over
    /// whatever the shell shows (M5).
    pub deploy: Option<Box<crate::app::device::UiDeployView>>,
}

impl UiStudioView {
    pub fn new(panes: Vec<UiPaneView>, console: UiConsoleView) -> Self {
        Self {
            panes,
            console,
            home: None,
            open_project_uid: None,
            open_project_slug: None,
            device_sync: None,
            deploy: None,
        }
    }

    pub fn with_home(mut self, home: Option<crate::app::home::UiHomeView>) -> Self {
        self.home = home.map(Box::new);
        self
    }

    pub fn with_open_project(mut self, uid: Option<String>, slug: Option<String>) -> Self {
        self.open_project_uid = uid;
        self.open_project_slug = slug;
        self
    }

    pub fn with_device_sync(
        mut self,
        device_sync: Option<crate::app::places::DeviceSyncState>,
    ) -> Self {
        self.device_sync = device_sync;
        self
    }

    pub fn with_deploy(mut self, deploy: Option<crate::app::device::UiDeployView>) -> Self {
        self.deploy = deploy.map(Box::new);
        self
    }

    /// An empty view with no panes and an empty default-filtered console. The
    /// web shell seeds its `Signal<UiStudioView>` with this before the actor
    /// emits its first change-gated snapshot.
    pub fn empty() -> Self {
        Self::new(Vec::new(), UiConsoleView::empty())
    }

    /// Apply a progressive activity update in place, so live pane/section
    /// activity emitted mid-action (before the next full snapshot) reaches the
    /// UI. This is the core-owned form of the retired web `apply_activity_update`
    /// (P4/Q5): the actor calls it on the latest snapshot when a
    /// [`UxUpdate::Activity`](crate::UxUpdate::Activity) arrives, then republishes
    /// the mutated view.
    pub fn apply_activity(
        &mut self,
        target: &UxActivityTarget,
        status: UiStatus,
        activity: UiActivityView,
    ) {
        let Some(pane) = self
            .panes
            .iter_mut()
            .find(|pane| pane.node_id.as_str() == target.pane_node_id().as_str())
        else {
            return;
        };
        pane.status = status;

        match target {
            UxActivityTarget::Pane { .. } => {
                pane.body = UiViewContent::Activity(activity);
            }
            UxActivityTarget::StackSection { section_id, .. } => {
                if let UiViewContent::Stack(stack) = &mut pane.body {
                    if let Some(section) = stack
                        .sections
                        .iter_mut()
                        .find(|section| &section.id == section_id)
                    {
                        section.state = UiStepState::Active;
                        section.body = UiViewContent::Activity(activity);
                        section.actions.clear();
                        return;
                    }
                }
                pane.body = UiViewContent::Activity(activity);
            }
        }
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        if let Some(home) = &self.home {
            for line in home.render_text_lines() {
                let _ = writeln!(output, "{line}");
            }
            output.push('\n');
        }
        for pane in &self.panes {
            let _ = writeln!(output, "{}", pane.title);
            let _ = writeln!(output, "  node: {}", pane.node_id);
            let _ = writeln!(output, "  status: {}", pane.status.label);
            for line in pane.body.render_text_lines() {
                let _ = writeln!(output, "  {line}");
            }
            if !pane.actions.is_empty() {
                let _ = writeln!(output, "  actions:");
                for action in &pane.actions {
                    let meta = action.meta();
                    let _ = writeln!(
                        output,
                        "    - [{}] {}",
                        priority_label(meta.priority),
                        meta.label
                    );
                }
            }
            output.push('\n');
        }
        if !self.console.entries.is_empty() {
            let _ = writeln!(output, "Runtime");
            for log in self.console.entries.iter().rev().take(8) {
                let _ = writeln!(output, "  {:?} {}: {}", log.level, log.source, log.message);
            }
        }
        output
    }
}

fn priority_label(priority: ActionPriority) -> &'static str {
    match priority {
        ActionPriority::Primary => "primary",
        ActionPriority::Secondary => "secondary",
        ActionPriority::Tertiary => "tertiary",
    }
}

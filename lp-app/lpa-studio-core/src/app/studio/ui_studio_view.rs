use core::fmt::Write;

use crate::app::runtime_pool::CONSOLE_TAIL_LEN;
use crate::app::studio::ui_console_view::UiConsoleView;
use crate::{
    ActionPriority, UiActivityView, UiPaneView, UiStatus, UiViewContent, UxActivityTarget,
};

/// Which runtime session the editor lens is bound to (D35/D37 — the SDI
/// record: one lens shown at a time, and **the URL is the focused
/// document**). The web shell's route reconciliation binds `#/sim/<key>`
/// and `#/device/<uid>` to this, never to raw project identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiLensRuntime {
    /// The lens is on THE sim session. A sim runtime's identity is its
    /// project: `project_key` is the loaded project's slug (the D37 route
    /// key), read from the session's loaded-project record so re-attach
    /// flows (the sim-card click) address the same document; `None` while
    /// nothing library-backed is loaded (the storeless demo path).
    Sim { project_key: Option<String> },
    /// The lens is on the hardware device session. `uid` is the stamped
    /// `dev_…` identity (the D37 route key) once the hello or the
    /// connect-as-pull carried it; `None` for a not-yet-identified device
    /// (no honest address exists — the URL stays put).
    Device { uid: Option<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiStudioView {
    pub panes: Vec<UiPaneView>,
    /// The console slice: filtered log entries plus the filter state that
    /// produced them.
    pub console: UiConsoleView,
    /// The home gallery, when the shell should render it instead of the
    /// pane layout (no project open, no device flow engaged — M4).
    pub home: Option<Box<crate::app::home::UiHomeView>>,
    /// The editor lens's runtime binding, when a session holds the lens
    /// (see [`UiLensRuntime`]); `None` while the editor is detached.
    pub lens: Option<UiLensRuntime>,
    /// The `prj_…` uid of the open library package, when one backs the
    /// running project (identity for route↔view comparisons).
    pub open_project_uid: Option<String>,
    /// The open package's slug — the user-facing identifier the web shell
    /// mirrors into `#/sim/<slug>` (URL follows the view, covering
    /// example opens and clearing on disconnect without action plumbing).
    pub open_project_slug: Option<String>,
    /// Connect-as-pull result for the attached DEVICE (never the sim —
    /// D22): identity + content classification. Feeds the device pane,
    /// gallery cards, and the device-push verbs (M5/M8′).
    pub device_sync: Option<crate::app::places::DeviceSyncState>,
    /// The LENS session's card, for the editor layout (D43: the grown
    /// device card IS the editor's right-side pane). Same construction
    /// as the gallery roster's live cards; `None` while no lens session
    /// exists (the shell falls back to the device pane surface).
    pub lens_card: Option<Box<crate::app::home::UiDeviceCard>>,
    /// The layered-settings slice (effective values, provenance, override
    /// state) for the shell's settings popover.
    pub settings: crate::app::settings::UiSettingsView,
    /// The open project's edit state, hoisted to the shell so the web edge
    /// can arm the unload gate without reaching into the pane tree. Clean
    /// when no project is open. See
    /// [`has_unsaved_work`](crate::app::studio::has_unsaved_work) for which
    /// buckets actually mean "you would lose work".
    pub dirty: crate::DirtySummary,
}

impl UiStudioView {
    pub fn new(panes: Vec<UiPaneView>, console: UiConsoleView) -> Self {
        Self {
            panes,
            console,
            home: None,
            lens: None,
            open_project_uid: None,
            open_project_slug: None,
            device_sync: None,
            lens_card: None,
            settings: crate::app::settings::UiSettingsView::default(),
            dirty: crate::DirtySummary::clean(),
        }
    }

    pub fn with_home(mut self, home: Option<crate::app::home::UiHomeView>) -> Self {
        self.home = home.map(Box::new);
        self
    }

    pub fn with_lens(mut self, lens: Option<UiLensRuntime>) -> Self {
        self.lens = lens;
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

    pub fn with_lens_card(mut self, card: Option<crate::app::home::UiDeviceCard>) -> Self {
        self.lens_card = card.map(Box::new);
        self
    }

    pub fn with_settings(mut self, settings: crate::app::settings::UiSettingsView) -> Self {
        self.settings = settings;
        self
    }

    pub fn with_dirty(mut self, dirty: crate::DirtySummary) -> Self {
        self.dirty = dirty;
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
        pane.body = UiViewContent::Activity(activity);
    }

    /// Apply the CARD-OWNED op flow's live state in place, so a long
    /// management op (flash / erase / reset) narrates on its card while
    /// it runs — the counterpart of [`Self::apply_activity`] for the
    /// card overlay rather than a pane section.
    ///
    /// The matching rule is [`UiDeviceCard::takes_card_op`], the same one
    /// the controller's view build uses, so the mid-flight card and the
    /// snapshot that replaces it never disagree. The lens card is the
    /// same card grown into the editor (D43), so it takes the op too.
    pub fn apply_card_op(&mut self, uid: Option<&str>, op: crate::CardOp) {
        for card in self.device_cards_mut() {
            if card.takes_card_op(uid) {
                card.ui.op = Some(op.clone());
            }
        }
    }

    /// Append one stamped entry to the console tail of whichever card is
    /// mid-op, so the op overlay's technical-details region streams the
    /// work's own output (esptool's writes, the device's boot lines)
    /// rather than sitting on "Waiting for device output…".
    ///
    /// Scoped to cards already wearing an op: action dispatch is
    /// serialized through the actor, so at most one card carries an op
    /// flow at a time and there is no one else to smear onto.
    ///
    /// Capped at [`CONSOLE_TAIL_LEN`] like the session-fed tail this
    /// stands in for — a firmware flash emits hundreds of lines and
    /// every progressive publish clones the view.
    pub fn push_card_op_console(&mut self, entry: crate::UiLogEntry) {
        for card in self.device_cards_mut() {
            if card.ui.op.is_some() {
                card.console_tail.push(entry.clone());
                let overflow = card.console_tail.len().saturating_sub(CONSOLE_TAIL_LEN);
                card.console_tail.drain(..overflow);
            }
        }
    }

    /// Every device card the view carries: the gallery roster plus the
    /// lens card (the same card grown into the editor pane, D43).
    fn device_cards_mut(&mut self) -> impl Iterator<Item = &mut crate::app::home::UiDeviceCard> {
        self.home
            .iter_mut()
            .flat_map(|home| home.devices.iter_mut())
            .chain(self.lens_card.iter_mut().map(|card| card.as_mut()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::home::{CardUiState, UiDeviceCard, UiHomeView};
    use crate::app::roster::RosterCardState;
    use crate::{CardOp, UiLogEntry, UiLogLevel, UiLogOrigin};

    fn card(uid: Option<&str>, state: RosterCardState) -> UiDeviceCard {
        UiDeviceCard {
            uid: uid.map(str::to_string),
            name: "Board".to_string(),
            transport: "USB".to_string(),
            state,
            project: None,
            fw: None,
            hardware: None,
            safe_clamp: None,
            sim: false,
            console_tail: Vec::new(),
            ui: CardUiState::default(),
        }
    }

    fn view_with(devices: Vec<UiDeviceCard>) -> UiStudioView {
        UiStudioView::empty().with_home(Some(UiHomeView {
            devices,
            projects: Vec::new(),
            examples: Vec::new(),
            library_available: true,
            opening: None,
            issue: None,
            backup: None,
        }))
    }

    fn line(message: &str) -> UiLogEntry {
        UiLogEntry::new(1.0, UiLogLevel::Info, UiLogOrigin::Link, message)
    }

    #[test]
    fn a_stamped_op_lands_on_its_own_card_only() {
        let mut view = view_with(vec![
            card(Some("dev_a"), RosterCardState::RunningUpToDate),
            card(Some("dev_b"), RosterCardState::RunningUpToDate),
        ]);

        view.apply_card_op(Some("dev_a"), CardOp::new("Writing…", Some(42)));

        let devices = &view.home.as_ref().unwrap().devices;
        assert_eq!(devices[0].ui.op, Some(CardOp::new("Writing…", Some(42))));
        assert_eq!(devices[1].ui.op, None, "the op must not smear across cards");
    }

    #[test]
    fn an_unstamped_op_rides_the_live_card_never_a_remembered_one() {
        let mut view = view_with(vec![
            card(
                Some("dev_offline"),
                RosterCardState::Offline { last_seen_at: None },
            ),
            card(None, RosterCardState::ConnectedEmpty),
        ]);

        view.apply_card_op(None, CardOp::new("Installing…", Some(10)));

        let devices = &view.home.as_ref().unwrap().devices;
        assert_eq!(devices[0].ui.op, None, "a remembered card is not live");
        assert!(devices[1].ui.op.is_some(), "the blank live board takes it");
    }

    #[test]
    fn op_console_lines_reach_the_card_mid_op_and_stay_bounded() {
        let mut view = view_with(vec![
            card(Some("dev_a"), RosterCardState::RunningUpToDate),
            card(Some("dev_b"), RosterCardState::RunningUpToDate),
        ]);
        view.apply_card_op(Some("dev_a"), CardOp::new("Writing…", None));

        for index in 0..CONSOLE_TAIL_LEN + 5 {
            view.push_card_op_console(line(&format!("Writing at {index:#x}")));
        }

        let devices = &view.home.as_ref().unwrap().devices;
        assert_eq!(
            devices[0].console_tail.len(),
            CONSOLE_TAIL_LEN,
            "a long flash must not grow the tail without bound"
        );
        assert_eq!(
            devices[0].console_tail.last().unwrap().message,
            format!("Writing at {:#x}", CONSOLE_TAIL_LEN + 4),
            "the newest line survives the trim"
        );
        assert!(
            devices[1].console_tail.is_empty(),
            "only the card mid-op shows the op's output"
        );
    }

    #[test]
    fn the_lens_card_tracks_the_op_it_is_the_same_card_grown() {
        let mut view = view_with(Vec::new())
            .with_lens_card(Some(card(Some("dev_a"), RosterCardState::RunningUpToDate)));

        view.apply_card_op(Some("dev_a"), CardOp::new("Writing…", Some(7)));
        view.push_card_op_console(line("Writing at 0x0"));

        let lens = view.lens_card.as_ref().unwrap();
        assert_eq!(lens.ui.op, Some(CardOp::new("Writing…", Some(7))));
        assert_eq!(lens.console_tail.len(), 1);
    }
}

use core::fmt::Write;

use crate::app::runtime_pool::CONSOLE_TAIL_LEN;
use crate::app::studio::ui_console_view::UiConsoleView;
use crate::{
    ActionPriority, UiActivityView, UiPaneView, UiStatus, UiViewContent, UxActivityTarget,
};

/// Which runtime session the editor lens is bound to (D35/D37 — the SDI
/// record: one lens shown at a time, and **the URL is the focused
/// document**). The web shell's route reconciliation binds
/// `/p/<slug>-<project-uid>` and `/device/<uid>` to this, never to raw
/// project identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiLensRuntime {
    /// The lens is on THE sim session. A sim runtime's identity is its
    /// project: `project_uid` is the loaded project's `prj…` uid — the
    /// whole of the project route's identity — read from the session's
    /// loaded-project record so re-attach flows (the sim-card click)
    /// address the same document; `None` while nothing library-backed is
    /// loaded (the storeless demo path). The slug that decorates the
    /// address is cosmetic and comes from
    /// [`UiStudioView::open_project_name`], which tracks renames live.
    Sim { project_uid: Option<String> },
    /// The lens is on the hardware device session. `uid` is the stamped
    /// `dev…` identity (the D37 route key) once the hello or the
    /// connect-as-pull carried it; `None` for a not-yet-identified device
    /// (no honest address exists — the URL stays put).
    Device { uid: Option<String> },
}

/// The header session·project control's three-dot status vocabulary
/// (D16), collapsed from
/// [`crate::app::roster::RosterCardState`]: accent = running clean,
/// amber = anything needing attention, hollow = connected with nothing
/// running.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiChromeSessionStatus {
    /// Running the project cleanly (accent dot).
    Run,
    /// Behind / edited-on-device / connecting / any attention state
    /// (amber dot). The dot only marks that attention is due — the
    /// card carries the story.
    Attention,
    /// Connected with nothing running (hollow dot).
    Empty,
}

/// The tab's ONE runtime session, projected for the header
/// session·project control (single-session web policy). Same card
/// derivation as the gallery roster, so the control and the gallery can
/// never disagree about a session — but a CONTROL's projection, not
/// wayfinding: it carries the facts its panel needs (the board it runs,
/// whether an operation is in flight, the device-zone stat line) instead
/// of a route target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiChromeSessionControl {
    /// The underlying card's identity key
    /// ([`UiDeviceCard::identity_key`](crate::UiDeviceCard::identity_key)),
    /// which is also what a teardown of THIS session has to name:
    /// hardware disconnects through
    /// [`DeviceTarget::card`](crate::DeviceTarget::card), exactly as the
    /// card's own danger-zone row does, so the header's leave-the-studio
    /// teardown and the card can never end up pointing at different
    /// boards. (The sim needs no target — `StopSimulator` is unique by
    /// construction — so for it this is only the render key.)
    pub key: String,
    /// The sim session (violet sim glyph); hardware wears its own.
    pub sim: bool,
    /// "Sim" for the simulator (the control renders the board as a
    /// suffix), the registry name for hardware.
    pub name: String,
    /// Human board name via
    /// [`board_display_name`](crate::app::roster::board_display_name) —
    /// sim only, `None` when the project names no board (the control
    /// shows a bare "Sim").
    pub board: Option<String>,
    /// The same three-dot vocabulary [`UiChromeSessionStatus`] defines.
    pub status: UiChromeSessionStatus,
    /// The in-flight operation's label, the nav guard's evidence: while
    /// this is `Some`, leaving (or opening anything else) is refused
    /// rather than silently tearing the work down.
    pub busy: Option<String>,
    /// The panel's device-zone stat line, assembled core-side (e.g.
    /// "60 fps · USB"); `None` while nothing is known — a session that
    /// has published no frame and named no transport has nothing honest
    /// to say here.
    pub stat_line: Option<String>,
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
    /// The `prj…` uid of the open library package, when one backs the
    /// running project (identity for route↔view comparisons).
    pub open_project_uid: Option<String>,
    /// The open package's user-facing display name (manifest `name`,
    /// falling back to the library slug) — the web shell slugifies it into
    /// the cosmetic half of `/p/<slug>-<uid>`, so the address bar agrees
    /// with the cloud sidecar's name and the service's canonical URL. URL
    /// follows the view, covering example opens and clearing on disconnect
    /// without action plumbing.
    pub open_project_name: Option<String>,
    /// `Some(example id)` while the open session is a TRANSIENT view
    /// session over an embedded example (examples vision D2): nothing was
    /// installed, [`Self::open_project_uid`] carries an ephemeral RAM uid
    /// that must never reach a URL or the cloud, and the honest address is
    /// the bare `/p/<slug>`. Cleared the moment an explicit save forks the
    /// session into the library.
    pub open_project_transient: Option<String>,
    /// Connect-as-pull result for the attached DEVICE (never the sim —
    /// D22): identity + content classification. Feeds the device pane,
    /// gallery cards, and the device-push verbs (M5/M8′).
    pub device_sync: Option<crate::app::places::DeviceSyncState>,
    /// The LENS session's card, for the editor layout (D43: the grown
    /// device card IS the editor's right-side pane). Same construction
    /// as the gallery roster's live cards; `None` while no lens session
    /// exists (the shell falls back to the device pane surface).
    pub lens_card: Option<Box<crate::app::home::UiDeviceCard>>,
    /// THE session this tab runs, for the header session·project control
    /// (single-session web policy). `None` with nothing attached.
    pub session: Option<UiChromeSessionControl>,
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
            open_project_name: None,
            open_project_transient: None,
            device_sync: None,
            lens_card: None,
            session: None,
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
        self.open_project_name = slug;
        self
    }

    /// Mark the open session as a transient view of `example_id` (see
    /// [`Self::open_project_transient`]).
    pub fn with_transient(mut self, example_id: Option<String>) -> Self {
        self.open_project_transient = example_id;
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

    pub fn with_session(mut self, session: Option<UiChromeSessionControl>) -> Self {
        self.session = session;
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
    pub fn apply_card_op(&mut self, session_key: &str, op: crate::CardOp) {
        for card in self.device_cards_mut() {
            if card.takes_card_op(session_key) {
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

    /// A LIVE card: `session_key` is what a card-owned op addresses (M4),
    /// so a fixture without one models a registry card, not a board. Two
    /// cards never share a session, so the key derives from the uid.
    fn card(uid: Option<&str>, state: RosterCardState) -> UiDeviceCard {
        live_card(&format!("runtime-{}", uid.unwrap_or("blank")), uid, state)
    }

    fn live_card(session_key: &str, uid: Option<&str>, state: RosterCardState) -> UiDeviceCard {
        UiDeviceCard {
            frame_preview: None,
            frame_age_secs: None,
            frame_fps: None,
            port_label: None,
            session_key: Some(session_key.to_string()),
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
            detected_chip: None,
            board_id: None,
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
            setup: None,
        }))
    }

    fn line(message: &str) -> UiLogEntry {
        UiLogEntry::new(1.0, UiLogLevel::Info, UiLogOrigin::Link, message)
    }

    #[test]
    fn an_op_lands_on_its_own_session_card_only() {
        let mut view = view_with(vec![
            live_card("runtime-1", Some("deva"), RosterCardState::RunningUpToDate),
            live_card("runtime-2", Some("devb"), RosterCardState::RunningUpToDate),
        ]);

        view.apply_card_op("runtime-1", CardOp::new("Writing…", Some(42)));

        let devices = &view.home.as_ref().unwrap().devices;
        assert_eq!(devices[0].ui.op, Some(CardOp::new("Writing…", Some(42))));
        assert_eq!(devices[1].ui.op, None, "the op must not smear across cards");
    }

    /// Two ANONYMOUS boards: the pre-M4 rule rode "any uid-less live
    /// card", so one flash narrated on both. The session key is what
    /// tells them apart.
    #[test]
    fn an_op_on_one_blank_board_does_not_narrate_on_the_other() {
        let mut view = view_with(vec![
            live_card("runtime-1", None, RosterCardState::ConnectedEmpty),
            live_card("runtime-2", None, RosterCardState::ConnectedEmpty),
        ]);

        view.apply_card_op("runtime-2", CardOp::new("Installing…", Some(10)));

        let devices = &view.home.as_ref().unwrap().devices;
        assert_eq!(devices[0].ui.op, None, "the other blank board is not it");
        assert!(
            devices[1].ui.op.is_some(),
            "the board being flashed takes it"
        );
    }

    #[test]
    fn a_remembered_card_never_adopts_an_op() {
        let mut view = view_with(vec![
            UiDeviceCard {
                session_key: None,
                ..card(
                    Some("devoffline"),
                    RosterCardState::Offline { last_seen_at: None },
                )
            },
            live_card("runtime-1", None, RosterCardState::ConnectedEmpty),
        ]);

        view.apply_card_op("runtime-1", CardOp::new("Installing…", Some(10)));

        let devices = &view.home.as_ref().unwrap().devices;
        assert_eq!(devices[0].ui.op, None, "a remembered card has no session");
        assert!(devices[1].ui.op.is_some(), "the live board takes it");
    }

    #[test]
    fn op_console_lines_reach_the_card_mid_op_and_stay_bounded() {
        let mut view = view_with(vec![
            card(Some("deva"), RosterCardState::RunningUpToDate),
            card(Some("devb"), RosterCardState::RunningUpToDate),
        ]);
        view.apply_card_op("runtime-deva", CardOp::new("Writing…", None));

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
            .with_lens_card(Some(card(Some("deva"), RosterCardState::RunningUpToDate)));

        view.apply_card_op("runtime-deva", CardOp::new("Writing…", Some(7)));
        view.push_card_op_console(line("Writing at 0x0"));

        let lens = view.lens_card.as_ref().unwrap();
        assert_eq!(lens.ui.op, Some(CardOp::new("Writing…", Some(7))));
        assert_eq!(lens.console_tail.len(), 1);
    }
}

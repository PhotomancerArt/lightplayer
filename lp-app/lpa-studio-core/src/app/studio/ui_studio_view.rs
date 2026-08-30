use core::fmt::Write;

use crate::app::studio::ui_console_view::UiConsoleView;
use crate::{
    ActionPriority, UiActivityView, UiPaneView, UiStatus, UiViewContent, UxActivityTarget,
};

/// Which runtime session the editor lens is bound to (D35/D37 — the SDI
/// record: one lens shown at a time, and **the URL is the focused
/// document**). The web shell's route reconciliation binds
/// `/p/<slug>-<project-uid>` to this, never to raw project identity.
///
/// ⚠️ The `Device` arm — and the `/device/<uid>` route it fed — went with
/// M2 of the device-model rebuild; the rebuilt model re-adds it.
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
}

/// The header session·project control's three-dot status vocabulary
/// (D16), collapsed from the card state: accent = running clean, amber =
/// anything needing attention, hollow = connected with nothing running.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiChromeSessionStatus {
    /// Running the project cleanly (accent dot).
    Run,
    /// Any attention state (amber dot). The dot only marks that attention
    /// is due — the card carries the story. Unreachable while the sim is
    /// the only runtime; the rebuilt device model produces it again.
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
    /// ([`UiSimCard::identity_key`](crate::UiSimCard::identity_key)) —
    /// the render key. The sim needs no teardown target:
    /// `StopSimulator` is unique by construction.
    pub key: String,
    /// "Sim" for the simulator (the control renders the board as a
    /// suffix).
    pub name: String,
    /// Human board name via
    /// [`board_display_name`](crate::app::roster::board_display_name) —
    /// sim only, `None` when the project names no board (the control
    /// shows a bare "Sim").
    pub board: Option<String>,
    /// The same three-dot vocabulary [`UiChromeSessionStatus`] defines.
    pub status: UiChromeSessionStatus,
    /// The panel's runtime stat line, assembled core-side (e.g. "60 fps");
    /// `None` while nothing is known — a session that has published no
    /// frame has nothing honest to say here.
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
    /// The open session is a TRANSIENT view session (examples vision D2 /
    /// P5): memory stores, nothing installed — an explicit save forks it.
    /// Covers BOTH kinds (embedded example and shared View link); the web
    /// edge keys the leave-discards-work confirm off this.
    pub open_project_transient: bool,
    /// `Some(example id)` when the transient session views an embedded
    /// example: [`Self::open_project_uid`] then carries a RAM-minted uid
    /// that must never reach a URL, and the honest address is the bare
    /// `/p/<slug>`. `None` for shared-view sessions (their uid is the
    /// cloud document's own and the Project route is honest). Cleared the
    /// moment an explicit save forks the session into the library.
    pub open_transient_example: Option<String>,
    /// Completed fork-at-save count for this tab (monotonic). The web
    /// shell watches it to raise the fork toast — a state transition
    /// alone cannot distinguish "forked" from "opened another project".
    pub transient_fork_generation: u64,
    /// The LENS session's card, for the editor layout (D43: the grown
    /// runtime card IS the editor's right-side pane). Same construction
    /// as the gallery roster's live card; `None` while no lens session
    /// exists.
    pub lens_card: Option<Box<crate::app::home::UiSimCard>>,
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
            open_project_transient: false,
            open_transient_example: None,
            transient_fork_generation: 0,
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

    /// Mark the open session's transient state (see
    /// [`Self::open_project_transient`] / [`Self::open_transient_example`]).
    pub fn with_transient(
        mut self,
        transient: bool,
        example_id: Option<String>,
        fork_generation: u64,
    ) -> Self {
        self.open_project_transient = transient;
        self.open_transient_example = example_id;
        self.transient_fork_generation = fork_generation;
        self
    }

    pub fn with_lens_card(mut self, card: Option<crate::app::home::UiSimCard>) -> Self {
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

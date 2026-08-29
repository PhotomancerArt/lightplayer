//! What the auto-publish driver last did, per project — the diagnostic
//! ledger behind the `/account` page's "Cloud sync" rows.
//!
//! The engine's design rule is that sync never speaks
//! ([`sync_engine`](super::sync_engine) module docs): a working driver is
//! invisible and a failing one only lags. That rule survived until every
//! failure in the path was a `log::warn!` nobody reads, and "my projects
//! never published" had no first question to ask
//! (`docs/defects/2026-08-28-auto-publish-outcomes-invisible.md`). This
//! module is the smallest correction: the driver records what each trip
//! concluded — including the conclusions that produce **no network traffic
//! at all** (nothing saved yet, skipped, refused local state) — and one
//! diagnostic surface renders the ledger. Nothing here schedules, retries,
//! or renders; it is a notebook, not a nervous system.
//!
//! Like the queue, the ledger is per-tab and never persisted: it describes
//! this tab's driver, and a fresh page load re-derives everything the next
//! sweep concludes.

use std::collections::BTreeMap;

/// How one project's last trip concluded, coarse enough to badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcomeKind {
    /// The cloud record was created or restated; content went up.
    Published,
    /// Already published; new content went up (or was already there).
    Pushed,
    /// The project has no saved version — nothing to hold, no failure. The
    /// silent branch that most needs a name: it produces zero network
    /// traffic and the queue forgets it.
    NothingSaved,
    /// This tab did not run the trip (open in another tab, no library
    /// host). Another actor or a later pass owns it.
    Skipped,
    /// A retryable failure (offline, gateway); the coarse timer will come
    /// back to it.
    Retrying,
    /// A terminal refusal — dropped until the next save or sign-in.
    Refused,
    /// The denied latch (P6): the server said "not yours to write" and the
    /// queue has stopped asking.
    Denied,
}

impl SyncOutcomeKind {
    /// The badge word.
    pub fn label(self) -> &'static str {
        match self {
            SyncOutcomeKind::Published => "published",
            SyncOutcomeKind::Pushed => "pushed",
            SyncOutcomeKind::NothingSaved => "no save yet",
            SyncOutcomeKind::Skipped => "skipped",
            SyncOutcomeKind::Retrying => "retrying",
            SyncOutcomeKind::Refused => "refused",
            SyncOutcomeKind::Denied => "denied",
        }
    }

    /// Whether the outcome is the kind a user should read as trouble.
    pub fn is_failure(self) -> bool {
        matches!(
            self,
            SyncOutcomeKind::Retrying | SyncOutcomeKind::Refused | SyncOutcomeKind::Denied
        )
    }
}

/// One project's row in the ledger.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSyncStatus {
    pub uid: String,
    /// The display name the driver had in hand (roster name, or the uid
    /// when the roster did not know it).
    pub name: String,
    pub kind: SyncOutcomeKind,
    /// One human sentence: the report, or the error, verbatim.
    pub detail: String,
    /// Milliseconds since epoch (`js_sys::Date::now` domain).
    pub at_ms: f64,
}

/// What the driver itself has been through — the facts that explain an
/// EMPTY project list ("signed out", "the sweep never found a library").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineSyncStatus {
    /// Whether the driver believes there is an account to sync with.
    pub signed_in: bool,
    /// The last sign-in sweep: when, and how many projects it offered.
    pub last_sweep: Option<SweepSyncStatus>,
}

/// One sweep's summary.
#[derive(Debug, Clone, PartialEq)]
pub struct SweepSyncStatus {
    pub at_ms: f64,
    /// Projects offered to the queue. Zero with `host_missing` unset means
    /// the library really was empty.
    pub offered: usize,
    /// The sweep gave up waiting for the OPFS library host — nothing was
    /// offered and nothing will be until the next page load or save.
    pub host_missing: bool,
}

/// The ledger: engine facts plus the newest conclusion per project.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SyncStatusBoard {
    pub engine: EngineSyncStatus,
    projects: BTreeMap<String, ProjectSyncStatus>,
}

impl SyncStatusBoard {
    pub fn record_signed_in(&mut self, signed_in: bool) {
        self.engine.signed_in = signed_in;
    }

    pub fn record_sweep(&mut self, offered: usize, host_missing: bool, at_ms: f64) {
        self.engine.last_sweep = Some(SweepSyncStatus {
            at_ms,
            offered,
            host_missing,
        });
    }

    /// Record a project's newest conclusion, replacing the previous one.
    pub fn record_project(
        &mut self,
        uid: &str,
        name: &str,
        kind: SyncOutcomeKind,
        detail: impl Into<String>,
        at_ms: f64,
    ) {
        self.projects.insert(
            uid.to_string(),
            ProjectSyncStatus {
                uid: uid.to_string(),
                name: name.to_string(),
                kind,
                detail: detail.into(),
                at_ms,
            },
        );
    }

    /// Every row, newest first — failures do not float; recency is the
    /// honest ordering for "what just happened".
    pub fn rows(&self) -> Vec<ProjectSyncStatus> {
        let mut rows: Vec<_> = self.projects.values().cloned().collect();
        rows.sort_by(|a, b| b.at_ms.total_cmp(&a.at_ms).then(a.uid.cmp(&b.uid)));
        rows
    }

    /// How many rows currently read as trouble.
    pub fn failures(&self) -> usize {
        self.projects
            .values()
            .filter(|row| row.kind.is_failure())
            .count()
    }
}

/// A read-only copy for one render.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SyncStatusSnapshot {
    pub engine: EngineSyncStatus,
    pub rows: Vec<ProjectSyncStatus>,
}

// ---------------------------------------------------------------------------
// The tab's ledger. Thread-local like the engine it describes; compiled on
// every target so the pure board stays natively testable, while only the
// wasm driver ever writes it.

use std::cell::RefCell;

thread_local! {
    static BOARD: RefCell<SyncStatusBoard> = RefCell::new(SyncStatusBoard::default());
}

/// Run one mutation against the tab's ledger.
pub fn record(mutate: impl FnOnce(&mut SyncStatusBoard)) {
    BOARD.with(|board| mutate(&mut board.borrow_mut()));
}

/// The tab's ledger, copied for one render.
pub fn snapshot() -> SyncStatusSnapshot {
    BOARD.with(|board| {
        let board = board.borrow();
        SyncStatusSnapshot {
            engine: board.engine.clone(),
            rows: board.rows(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_conclusion_per_project_wins() {
        let mut board = SyncStatusBoard::default();
        board.record_project("prj1", "Dome", SyncOutcomeKind::Retrying, "offline", 1.0);
        board.record_project("prj1", "Dome", SyncOutcomeKind::Published, "v1 up", 2.0);
        let rows = board.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, SyncOutcomeKind::Published);
        assert_eq!(board.failures(), 0);
    }

    #[test]
    fn rows_come_newest_first() {
        let mut board = SyncStatusBoard::default();
        board.record_project("prj1", "A", SyncOutcomeKind::Published, "", 1.0);
        board.record_project("prj2", "B", SyncOutcomeKind::Refused, "no history", 5.0);
        let rows = board.rows();
        assert_eq!(rows[0].uid, "prj2");
        assert_eq!(board.failures(), 1);
    }

    /// The whole point of the ledger: the branches that produce no network
    /// traffic still leave a row a person can read.
    #[test]
    fn silent_branches_have_names() {
        assert_eq!(SyncOutcomeKind::NothingSaved.label(), "no save yet");
        assert!(!SyncOutcomeKind::NothingSaved.is_failure());
        assert!(SyncOutcomeKind::Refused.is_failure());
    }

    #[test]
    fn a_sweep_that_found_no_host_says_so() {
        let mut board = SyncStatusBoard::default();
        board.record_signed_in(true);
        board.record_sweep(0, true, 3.0);
        let sweep = board.engine.last_sweep.clone().unwrap();
        assert!(sweep.host_missing);
        assert_eq!(sweep.offered, 0);
    }
}

//! **The project stage**: one board's stored projects brought to the format
//! its engine reads, by the host, over channel 1.
//!
//! ```text
//! ProjectStage::new(engine_format, tier)          (the engine's hello says its format)
//!   ├─ engine_format past this host's chain → Done(HostTooOld), nothing read
//!   → ListProjects → ReadStartupChoice
//!   → ReadManifest per project (the startup project first)
//!   ├─ none older                            → Done(Finished)
//!   ├─ older, and less than edit             → Done(NeedsEdit), nothing written
//!   → for each older project, the startup project first:
//!       ReadProject → lpa_upgrade::upgrade_with_chain(engine_format)
//!         ├─ refused → named with the upgrader's sentence, left alone; next
//!         → WriteBatch (one atomic batch: changed + added written, removed deleted)
//!             ├─ NotAtomic → Done(NotAtomic), nothing written
//!             ├─ Failed    → Done(Interrupted)
//!             → (the startup project) LoadProject
//!   → Done(Finished)
//! ```
//!
//! **The dark corner** (DD27): a link that drops after the startup
//! project's batch commits and before its load leaves a board whose startup
//! project is current and not running, and a stage run again finds nothing
//! older. When the board says it refused its startup project for its format
//! ([`ProjectStage::startup_refused`]), the stage loads it — once it is
//! current, before any other project is rewritten — and never otherwise, so
//! a board someone stopped on purpose stays stopped. Loading needs edit.
//!
//! **It resumes from the board, never from its own memory** (DM9, the
//! driver's rule): it keeps nothing across a link. Every run lists and reads
//! the board afresh, and a project already at the engine's format is left
//! alone, so a run after a cut or a dropped link only finishes what is left.
//! Each project's rewrite is one batch, which the tree store (the C6's and
//! the S3's, the boards that update over the air) commits as one
//! transaction, so after a cut each project is wholly old or wholly new.
//!
//! **The device never upgrades** (ADR 2026-07-05): the board says its format
//! and refuses an older project cleanly; the host transforms; channel 1
//! carries bytes.
//!
//! Sans-IO, like the rest of the crate: inputs in, [`ProjectStageEffect`]s
//! out, one effect at a time; no clock, no IO.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use lpa_upgrade::{
    JsonNode, ProjectFiles, UpgradeError, UpgradeStep, chain_tip_of, upgrade_steps,
    upgrade_with_chain,
};
use lpc_access::Tier;

use crate::projects::project_stage_effect::{BatchOutcome, ProjectStageEffect};
use crate::projects::project_stage_report::{
    ProjectStageReport, RefusedProject, StageEnd, UpgradedProject,
};
use crate::projects::stored_project::{StoredDisposition, StoredProject, project_name};

/// The key of the startup choice in the board's `/lightplayer.json`
/// (`lpc_model::server::server_config::ServerConfig`).
const STARTUP_KEY: &str = "startup_project";

/// One run of the project stage over one board's link.
pub struct ProjectStage {
    engine_format: u32,
    tier: Option<Tier>,
    chain: Vec<UpgradeStep>,
    phase: Phase,
    /// Every listed project, the startup project first, then by path.
    order: Vec<String>,
    /// The older projects, in `order`'s order.
    todo: Vec<String>,
    /// The board said it refused its startup project for its format (DD27).
    startup_refused: bool,
    report: ProjectStageReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    NotStarted,
    Listing,
    ReadingStartup,
    /// Waiting for `order[at]`'s manifest.
    ReadingManifest {
        at: usize,
    },
    /// Waiting for `todo[at]`'s files.
    ReadingProject {
        at: usize,
    },
    /// Waiting for `todo[at]`'s batch.
    Writing {
        at: usize,
        report: lpa_upgrade::UpgradeReport,
    },
    /// Waiting for the startup project's load; then `todo[next]` follows.
    Loading {
        project: String,
        next: usize,
    },
    Done,
}

impl ProjectStage {
    /// A stage for a board whose engine reads `engine_format` (its hello's
    /// `build.projectFormat`), over a link holding `tier`, upgrading with
    /// this host's own chain (`lpa_upgrade::upgrade_steps`).
    pub fn new(engine_format: u32, tier: Option<Tier>) -> Self {
        Self::with_chain(engine_format, tier, upgrade_steps().to_vec())
    }

    /// The same over a caller's chain — a test's, with a step past this
    /// build's own. The stage does nothing differently for it: that is the
    /// point.
    pub fn with_chain(engine_format: u32, tier: Option<Tier>, chain: Vec<UpgradeStep>) -> Self {
        Self {
            engine_format,
            tier,
            chain,
            phase: Phase::NotStarted,
            order: Vec::new(),
            todo: Vec::new(),
            startup_refused: false,
            report: ProjectStageReport {
                engine_format,
                ..ProjectStageReport::default()
            },
        }
    }

    /// The board's own report says it refused its startup project for its
    /// format (DD27): once that project is current, the stage loads it even
    /// when this run rewrote nothing. Without this report a current startup
    /// project is never loaded by the stage, so a board someone stopped on
    /// purpose stays stopped.
    pub fn startup_refused(mut self, refused: bool) -> Self {
        self.startup_refused = refused;
        self
    }

    /// How far the rewriting is: `(done, total)` counted in projects older
    /// than the engine's format, once every manifest is read; `None` before
    /// that and once the stage is done.
    pub fn progress(&self) -> Option<(usize, usize)> {
        let total = self.todo.len();
        match self.phase {
            Phase::ReadingProject { at } | Phase::Writing { at, .. } => Some((at, total)),
            Phase::Loading { next, .. } => Some((next.min(total), total)),
            Phase::NotStarted
            | Phase::Listing
            | Phase::ReadingStartup
            | Phase::ReadingManifest { .. }
            | Phase::Done => None,
        }
    }

    /// Begin: the first effect, or `Done(HostTooOld)` when this host's
    /// upgrader cannot reach the engine's format (Q105), before anything is
    /// read.
    pub fn start(&mut self) -> Vec<ProjectStageEffect> {
        if self.phase != Phase::NotStarted {
            return Vec::new();
        }
        let tip = chain_tip_of(&self.chain);
        if self.engine_format > tip {
            return self.finish(StageEnd::HostTooOld(UpgradeError::TargetBeyondChain {
                target: self.engine_format,
                tip,
            }));
        }
        self.phase = Phase::Listing;
        vec![ProjectStageEffect::ListProjects]
    }

    /// The board's project listing (`ListAvailableProjects`' paths).
    pub fn projects_listed(&mut self, mut paths: Vec<String>) -> Vec<ProjectStageEffect> {
        if self.phase != Phase::Listing {
            return Vec::new();
        }
        paths.sort();
        paths.dedup();
        if paths.is_empty() {
            return self.finish(StageEnd::Finished);
        }
        self.order = paths;
        self.phase = Phase::ReadingStartup;
        vec![ProjectStageEffect::ReadStartupChoice]
    }

    /// The board's `/lightplayer.json`, or `None` when it has none. The
    /// startup project is the one it names, or — as the board's own boot
    /// picks when it names none that exists — the first project by path.
    pub fn startup_choice_read(&mut self, config: Option<Vec<u8>>) -> Vec<ProjectStageEffect> {
        if self.phase != Phase::ReadingStartup {
            return Vec::new();
        }
        let named = config
            .as_deref()
            .and_then(|bytes| core::str::from_utf8(bytes).ok())
            .and_then(|text| JsonNode::parse(text).ok())
            .and_then(|config| config.get(STARTUP_KEY).and_then(JsonNode::as_str));
        let startup = named
            .and_then(|name| {
                self.order
                    .iter()
                    .position(|path| project_name(path) == name)
            })
            .unwrap_or(0);
        let first = self.order.remove(startup);
        self.order.insert(0, first.clone());
        self.report.startup = Some(first.clone());
        self.phase = Phase::ReadingManifest { at: 0 };
        vec![ProjectStageEffect::ReadManifest { project: first }]
    }

    /// A project's `project.json`, or `None` when the board has none.
    pub fn manifest_read(
        &mut self,
        project: &str,
        manifest: Option<Vec<u8>>,
    ) -> Vec<ProjectStageEffect> {
        let Phase::ReadingManifest { at } = self.phase else {
            return Vec::new();
        };
        if self.order[at] != project {
            return Vec::new();
        }
        let stored = StoredProject::judge(
            String::from(project),
            manifest.as_deref(),
            self.engine_format,
        );
        match stored.disposition {
            StoredDisposition::Current => self.report.untouched.push(stored.path),
            StoredDisposition::Older { .. } => self.todo.push(stored.path),
            StoredDisposition::Refused { why } => self.report.refused.push(RefusedProject {
                project: stored.path,
                why,
            }),
        }
        if let Some(next) = self.order.get(at + 1) {
            let project = next.clone();
            self.phase = Phase::ReadingManifest { at: at + 1 };
            return vec![ProjectStageEffect::ReadManifest { project }];
        }
        if self.tier != Some(Tier::Edit) {
            if self.todo.is_empty() {
                return self.finish(StageEnd::Finished);
            }
            self.report.pending = self.todo.clone();
            return self.finish(StageEnd::NeedsEdit);
        }
        // DD27: a current startup project the board refused for its format
        // is loaded first, before anything else is rewritten.
        if let Some(startup) = self.refused_current_startup() {
            self.phase = Phase::Loading {
                project: startup.clone(),
                next: 0,
            };
            return vec![ProjectStageEffect::LoadProject { project: startup }];
        }
        if self.todo.is_empty() {
            return self.finish(StageEnd::Finished);
        }
        self.read_project(0)
    }

    /// Every file of the project the stage asked for, keyed by its path in
    /// the project. Upgraded here; a refusal is named and the stage moves
    /// on, an upgrade becomes one batch.
    pub fn project_read(
        &mut self,
        project: &str,
        mut files: ProjectFiles,
    ) -> Vec<ProjectStageEffect> {
        let Phase::ReadingProject { at } = self.phase else {
            return Vec::new();
        };
        if self.todo[at] != project {
            return Vec::new();
        }
        let report = match upgrade_with_chain(&mut files, self.engine_format, &self.chain) {
            Ok(report) => report,
            Err(error) => {
                self.report.refused.push(RefusedProject {
                    project: String::from(project),
                    why: error.to_string(),
                });
                return self.next_project(at);
            }
        };
        let writes: Vec<(String, Vec<u8>)> = report
            .changed_files
            .iter()
            .chain(&report.added_files)
            .filter_map(|path| files.get(path).map(|bytes| (path.clone(), bytes.to_vec())))
            .collect();
        let deletes = report.removed_files.clone();
        self.phase = Phase::Writing { at, report };
        vec![ProjectStageEffect::WriteBatch {
            project: String::from(project),
            writes,
            deletes,
        }]
    }

    /// How the project's batch ended.
    pub fn batch_done(&mut self, project: &str, outcome: BatchOutcome) -> Vec<ProjectStageEffect> {
        let Phase::Writing { at, ref report } = self.phase else {
            return Vec::new();
        };
        if self.todo[at] != project {
            return Vec::new();
        }
        match outcome {
            BatchOutcome::Committed => {
                self.report.upgraded.push(UpgradedProject {
                    project: String::from(project),
                    report: report.clone(),
                });
                if self.report.startup.as_deref() == Some(project) {
                    self.phase = Phase::Loading {
                        project: String::from(project),
                        next: at + 1,
                    };
                    return vec![ProjectStageEffect::LoadProject {
                        project: String::from(project),
                    }];
                }
                self.next_project(at)
            }
            BatchOutcome::NotAtomic => {
                self.report.pending = self.todo[at..].to_vec();
                self.finish(StageEnd::NotAtomic)
            }
            BatchOutcome::Failed(why) => {
                self.report.pending = self.todo[at..].to_vec();
                self.finish(StageEnd::Interrupted(why))
            }
        }
    }

    /// How the startup project's load ended: `Ok`, or the board's words.
    /// Either way the other projects follow.
    pub fn load_done(
        &mut self,
        project: &str,
        result: Result<(), String>,
    ) -> Vec<ProjectStageEffect> {
        let Phase::Loading {
            project: ref loading,
            next,
        } = self.phase
        else {
            return Vec::new();
        };
        if loading != project {
            return Vec::new();
        }
        self.report.load = Some(result);
        if next < self.todo.len() {
            self.read_project(next)
        } else {
            self.finish(StageEnd::Finished)
        }
    }

    /// A read the stage asked for failed: it ends, and a run on the next
    /// link starts over from the board.
    pub fn read_failed(&mut self, why: String) -> Vec<ProjectStageEffect> {
        self.interrupted(why)
    }

    /// The link dropped. Whatever batch was open did not land (the board
    /// drops a batch whose link closes), so every project is wholly old or
    /// wholly new; a stage on the next link finishes the job.
    pub fn link_lost(&mut self) -> Vec<ProjectStageEffect> {
        self.interrupted(String::from("the link dropped"))
    }

    /// Whether the stage has said `Done`.
    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// The startup project, when the board refused it for its format and it
    /// is current now (DD27).
    fn refused_current_startup(&self) -> Option<String> {
        let startup = self.report.startup.as_ref()?;
        (self.startup_refused && self.report.untouched.contains(startup)).then(|| startup.clone())
    }

    fn read_project(&mut self, at: usize) -> Vec<ProjectStageEffect> {
        self.phase = Phase::ReadingProject { at };
        vec![ProjectStageEffect::ReadProject {
            project: self.todo[at].clone(),
        }]
    }

    fn next_project(&mut self, at: usize) -> Vec<ProjectStageEffect> {
        if at + 1 < self.todo.len() {
            self.read_project(at + 1)
        } else {
            self.finish(StageEnd::Finished)
        }
    }

    fn interrupted(&mut self, why: String) -> Vec<ProjectStageEffect> {
        let first_pending = match self.phase {
            Phase::NotStarted | Phase::Done => return Vec::new(),
            Phase::ReadingProject { at } | Phase::Writing { at, .. } => Some(at),
            Phase::Loading { next, .. } => Some(next),
            Phase::Listing | Phase::ReadingStartup | Phase::ReadingManifest { .. } => None,
        };
        if let Some(at) = first_pending {
            self.report.pending = self.todo.get(at..).unwrap_or_default().to_vec();
        }
        self.finish(StageEnd::Interrupted(why))
    }

    fn finish(&mut self, end: StageEnd) -> Vec<ProjectStageEffect> {
        self.phase = Phase::Done;
        self.report.end = end;
        vec![ProjectStageEffect::Done(core::mem::take(&mut self.report))]
    }
}

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
    /// Waiting for the startup project's (`todo[at]`) load.
    Loading {
        at: usize,
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
            report: ProjectStageReport {
                engine_format,
                ..ProjectStageReport::default()
            },
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
        if self.todo.is_empty() {
            return self.finish(StageEnd::Finished);
        }
        if self.tier != Some(Tier::Edit) {
            self.report.pending = self.todo.clone();
            return self.finish(StageEnd::NeedsEdit);
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
                    self.phase = Phase::Loading { at };
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
        let Phase::Loading { at } = self.phase else {
            return Vec::new();
        };
        if self.todo[at] != project {
            return Vec::new();
        }
        self.report.load = Some(result);
        self.next_project(at)
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
            Phase::Loading { at } => Some(at + 1),
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

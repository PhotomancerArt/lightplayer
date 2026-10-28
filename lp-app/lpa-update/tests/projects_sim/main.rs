//! **The project stage, simulated** (M3 P10): a board whose stored projects
//! live in a real tree store on simulated NOR flash (`lp-nor-sim`), three
//! real format-11 projects from `lpa-upgrade`'s v11 corpus on it —
//! `shader-oracle-6` the startup project (not the first by name),
//! `eaves-wired` with a patch file (so a file is removed) and
//! `fiber-headband` with hand-placed lamps (so a file is added) — and the
//! stage driven over a minimal channel-1 responder (`board.rs`) by the edge
//! P11's hosts will mirror (`edge.rs`).
//!
//! The statements say what the stage does; the sweeps cut the power after
//! **every** flash program and erase of an upgrade (under the calibrated
//! tears, and again under every other model `lp-nor-sim` names), remount, and run the stage again
//! from scratch: every project is wholly at 11 or wholly at 12 after every
//! cut, a cut before the first commit leaves every byte as it was, and the
//! rerun ends with the bytes an uncut run writes. A link dropped before any
//! request, and a fresh stage on a new link, finish the job too.
//!
//! Simulator only: no real flash timing, no radio.
//!
//! ```bash
//! cargo test -p lpa-update --test projects_sim -- --nocapture
//! ```

mod board;
mod edge;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lp_nor_sim::{FaultPlan, TearModel};
use lpa_update::{ProjectStage, StageEnd};
use lpa_upgrade::{ProjectFiles, UpgradeError, UpgradeReport, UpgradeStep, upgrade_steps};
use lpc_access::Tier;

use board::{Board, CONFIG, project_of};

const ENGINE: u32 = 12;
const STARTUP: &str = "shader-oracle-6";
const PATCHED: &str = "eaves-wired";
const PLACED: &str = "fiber-headband";
const PROJECTS: [&str; 3] = [STARTUP, PATCHED, PLACED];

// --- the statements --------------------------------------------------------------

/// The stage upgrades every older project on the board, the startup project
/// first, and then loads it; the others follow.
#[test]
fn the_stage_upgrades_every_older_project_startup_first_and_loads_it() {
    let mut board = Board::tree_with(&old_board());
    let (report, trace) = edge::run(&mut ProjectStage::new(ENGINE, Some(Tier::Edit)), &mut board);

    assert_eq!(report.end, StageEnd::Finished);
    assert_eq!(report.startup.as_deref(), Some("projects/shader-oracle-6"));
    assert_eq!(
        trace,
        [
            "read projects/shader-oracle-6",
            "write projects/shader-oracle-6",
            "load projects/shader-oracle-6",
            "read projects/eaves-wired",
            "write projects/eaves-wired",
            "read projects/fiber-headband",
            "write projects/fiber-headband",
        ]
    );
    assert_eq!(board.loads, ["/projects/shader-oracle-6"]);
    assert_eq!(report.load, Some(Ok(())));
    assert_eq!(
        report
            .upgraded
            .iter()
            .map(|u| (u.project.as_str(), u.report.from, u.report.to))
            .collect::<Vec<_>>(),
        [
            ("projects/shader-oracle-6", 11, 12),
            ("projects/eaves-wired", 11, 12),
            ("projects/fiber-headband", 11, 12),
        ]
    );
    assert!(report.refused.is_empty() && report.untouched.is_empty() && report.pending.is_empty());

    // On the flash: each project is the upgrader's own output, the patch
    // file gone and the placed lamps' map2d added.
    let snapshot = board.snapshot();
    for name in PROJECTS {
        assert_eq!(project_of(&snapshot, name), upgraded(name), "{name}");
    }
    assert!(!snapshot.contains_key("/projects/eaves-wired/fixture.patch.json"));
    assert!(snapshot.contains_key("/projects/fiber-headband/fixture.map2d.json"));
    assert_eq!(
        snapshot.get(CONFIG),
        old_board().get(CONFIG),
        "the startup choice is kept"
    );
}

/// A stage run again after it finished changes nothing: every project is
/// at the engine's format, so nothing is read, written or loaded.
#[test]
fn a_stage_run_again_changes_nothing_that_is_done() {
    let mut board = Board::tree_with(&old_board());
    edge::run(&mut ProjectStage::new(ENGINE, Some(Tier::Edit)), &mut board);
    let done = board.snapshot();
    let ops = board.ops_since_plan();

    let (report, trace) = edge::run(&mut ProjectStage::new(ENGINE, Some(Tier::Edit)), &mut board);
    assert_eq!(report.end, StageEnd::Finished);
    assert!(trace.is_empty(), "{trace:?}");
    assert_eq!(report.untouched.len(), 3);
    assert!(report.upgraded.is_empty() && report.load.is_none());
    assert_eq!(board.snapshot(), done);
    assert_eq!(board.ops_since_plan(), ops, "not one flash operation");
}

/// A project the step refuses is named with the step's own sentence and
/// left as it is; a project saved by a newer LightPlayer too; the others
/// are upgraded.
#[test]
fn a_project_the_step_refuses_is_named_and_left_alone_and_the_others_finish() {
    let mut files = old_board();
    insert_project(
        &mut files,
        "quad-wire-oracle",
        &upgrade_tests().join("refused/quad-wire-oracle"),
    );
    files.insert(
        String::from("/projects/from-the-future/project.json"),
        b"{\n  \"format\": 13\n}\n".to_vec(),
    );
    let mut board = Board::tree_with(&files);
    let (report, _) = edge::run(&mut ProjectStage::new(ENGINE, Some(Tier::Edit)), &mut board);

    assert_eq!(report.end, StageEnd::Finished);
    assert_eq!(report.upgraded.len(), 3);
    let refused: BTreeMap<&str, &str> = report
        .refused
        .iter()
        .map(|r| (r.project.as_str(), r.why.as_str()))
        .collect();
    assert!(
        refused["projects/quad-wire-oracle"].contains("`panel` is cut"),
        "the step's own sentence: {refused:?}"
    );
    assert!(
        refused["projects/from-the-future"].contains("saved by a newer LightPlayer"),
        "{refused:?}"
    );
    let snapshot = board.snapshot();
    for name in ["quad-wire-oracle", "from-the-future"] {
        assert_eq!(
            project_of(&snapshot, name),
            project_of(&files, name),
            "{name} untouched"
        );
    }
    for name in PROJECTS {
        assert_eq!(project_of(&snapshot, name), upgraded(name), "{name}");
    }
}

/// With less than edit the stage reads, finds the older projects, says so,
/// and writes nothing.
#[test]
fn with_less_than_edit_the_stage_writes_nothing_and_says_why() {
    for tier in [Some(Tier::Play), None] {
        let mut board = Board::tree_with(&old_board());
        let before = board.snapshot();
        let ops = board.ops_since_plan();
        let (report, trace) = edge::run(&mut ProjectStage::new(ENGINE, tier), &mut board);
        assert_eq!(report.end, StageEnd::NeedsEdit, "{tier:?}");
        assert_eq!(
            report.pending,
            [
                "projects/shader-oracle-6",
                "projects/eaves-wired",
                "projects/fiber-headband"
            ]
        );
        assert!(trace.is_empty(), "{tier:?}: {trace:?}");
        assert!(board.loads.is_empty());
        assert_eq!(board.snapshot(), before);
        assert_eq!(
            board.ops_since_plan(),
            ops,
            "{tier:?}: not one flash operation"
        );
    }
}

/// A board whose files cannot be rewritten in one transaction (memory
/// here; the classic's littlefs on a device) is not touched.
#[test]
fn a_board_whose_files_cannot_be_rewritten_atomically_is_not_touched() {
    let mut board = Board::memory_with(&old_board());
    let before = board.snapshot();
    let (report, trace) = edge::run(&mut ProjectStage::new(ENGINE, Some(Tier::Edit)), &mut board);
    assert_eq!(report.end, StageEnd::NotAtomic);
    assert_eq!(report.pending.len(), 3);
    assert!(report.upgraded.is_empty());
    assert_eq!(
        trace,
        [
            "read projects/shader-oracle-6",
            "write projects/shader-oracle-6"
        ]
    );
    assert!(board.loads.is_empty());
    assert_eq!(board.snapshot(), before);
}

/// A host whose upgrader stops short of the engine's format refuses the
/// whole stage, saying to update the host, before reading anything.
#[test]
fn a_host_older_than_the_engine_refuses_before_reading_anything() {
    let mut board = Board::tree_with(&old_board());
    let (report, trace) = edge::run(
        &mut ProjectStage::new(ENGINE + 1, Some(Tier::Edit)),
        &mut board,
    );
    let StageEnd::HostTooOld(error) = &report.end else {
        panic!("{report:?}");
    };
    assert_eq!(
        *error,
        UpgradeError::TargetBeyondChain {
            target: ENGINE + 1,
            tip: ENGINE
        }
    );
    assert!(error.to_string().contains("Update lp-cli or reload Studio"));
    assert!(trace.is_empty());
    assert_eq!(board.requests, 0, "not one request");
}

/// The stage knows nothing about mapping: an engine at a toy format 13,
/// with a toy v12→v13 step after this build's chain (written here and
/// nowhere else), takes the format-11 projects through the shipped v11→v12
/// and the toy, and a project already at 12 through the toy alone — over
/// the same effects, batches and load.
#[test]
fn a_later_format_rides_the_same_stage_with_no_new_work() {
    let mut files = old_board();
    for (path, bytes) in upgraded(PATCHED) {
        files.insert(path.replace(PATCHED, "at-twelve"), bytes);
    }
    let mut board = Board::tree_with(&files);
    let mut chain = upgrade_steps().to_vec();
    chain.push(UpgradeStep::new(12, 13, toy_v12_to_v13));
    let (report, trace) = edge::run(
        &mut ProjectStage::with_chain(13, Some(Tier::Edit), chain),
        &mut board,
    );

    assert_eq!(report.end, StageEnd::Finished);
    assert_eq!(
        trace[..3],
        [
            "read projects/shader-oracle-6",
            "write projects/shader-oracle-6",
            "load projects/shader-oracle-6"
        ]
    );
    let formats: BTreeMap<&str, (u32, u32)> = report
        .upgraded
        .iter()
        .map(|u| (u.project.as_str(), (u.report.from, u.report.to)))
        .collect();
    assert_eq!(formats["projects/at-twelve"], (12, 13));
    assert_eq!(formats["projects/eaves-wired"], (11, 13));
    assert_eq!(formats.len(), 4);
    let snapshot = board.snapshot();
    for name in [
        "shader-oracle-6",
        "eaves-wired",
        "fiber-headband",
        "at-twelve",
    ] {
        let manifest = &snapshot[&format!("/projects/{name}/project.json")];
        assert!(
            String::from_utf8_lossy(manifest).contains("\"format\": 13"),
            "{name}"
        );
        assert_eq!(
            snapshot
                .get(&format!("/projects/{name}/toy-13.txt"))
                .map(Vec::as_slice),
            Some(&b"format 13\n"[..]),
            "{name}"
        );
    }
    assert!(!snapshot.contains_key("/projects/eaves-wired/fixture.patch.json"));
}

// --- the sweeps ------------------------------------------------------------------

/// Cut the power after every flash program and erase of the stage (the
/// calibrated tears), remount, check every project is wholly old or wholly
/// new, and run the stage again from scratch: it ends with the uncut bytes.
#[test]
fn every_cut_of_the_stage_leaves_each_project_wholly_old_or_wholly_new() {
    let uncut = Uncut::run();
    let mut outcomes = Outcomes::default();
    for k in 0..uncut.ops {
        outcomes.add(uncut.cut_and_finish(k, TearModel::Calibrated));
    }
    println!(
        "project stage cut sweep (lp-nor-sim simulator, host): {} flash ops, every one cut \
         under the calibrated tears: {outcomes}",
        uncut.ops
    );
    assert_eq!(outcomes.cuts, uncut.ops);
    // The last op is the last batch's commit, so no cut leaves all three
    // new; the reruns are what finish them.
    assert!(outcomes.all_old > 0 && outcomes.mixed > 0, "{outcomes}");
}

/// The same at every op under every other model `lp-nor-sim` names (the
/// guessed tears and the calibrated one's forced shapes).
#[test]
fn every_cut_under_every_named_tear_model_leaves_each_project_wholly_old_or_wholly_new() {
    let uncut = Uncut::run();
    let mut outcomes = Outcomes::default();
    for tear in TearModel::NAMED {
        for k in 0..uncut.ops {
            outcomes.add(uncut.cut_and_finish(k, tear));
        }
    }
    println!(
        "project stage cut sweep (lp-nor-sim simulator, host): {} flash ops x {} tear models: \
         {outcomes}",
        uncut.ops,
        TearModel::NAMED.len()
    );
    assert_eq!(outcomes.cuts, uncut.ops * TearModel::NAMED.len() as u64);
}

/// A link dropped at any request of the stage, then a fresh stage on a new
/// link: every project is wholly old or new in between, and the second
/// stage finishes the job with the uncut bytes.
#[test]
fn a_link_dropped_anywhere_and_a_fresh_stage_on_a_new_link_finish_the_job() {
    let uncut = Uncut::run();
    for n in 0..uncut.requests {
        let mut board = Board::tree(uncut.before.clone());
        board.drop_at = Some(n);
        let (report, _) = edge::run(&mut ProjectStage::new(ENGINE, Some(Tier::Edit)), &mut board);
        assert!(
            matches!(report.end, StageEnd::Interrupted(_)),
            "dropped at request {n}: {report:?}"
        );
        board.new_link();
        uncut.check_whole(&board.snapshot(), &format!("dropped at request {n}"));

        let (report, _) = edge::run(&mut ProjectStage::new(ENGINE, Some(Tier::Edit)), &mut board);
        assert_eq!(report.end, StageEnd::Finished, "dropped at request {n}");
        assert_eq!(board.snapshot(), uncut.after, "dropped at request {n}");
    }
    println!(
        "project stage link drops: every one of {} requests dropped once, every rerun finished",
        uncut.requests
    );
}

// --- the uncut run, and what a cut must leave --------------------------------------

struct Uncut {
    /// The flash before the stage: three format-11 projects.
    before: lp_nor_sim::NorFlashSim,
    old: BTreeMap<String, Vec<u8>>,
    after: BTreeMap<String, Vec<u8>>,
    /// Flash programs and erases the stage made.
    ops: u64,
    /// The op count when the first batch committed.
    first_commit: u64,
    /// Channel-1 requests the stage made.
    requests: usize,
}

impl Uncut {
    fn run() -> Self {
        let board = Board::tree_with(&old_board());
        let before = board.flash();
        let old = board.snapshot();

        let mut board = Board::tree(before.clone());
        board.set_plan(FaultPlan::none());
        let (report, _) = edge::run(&mut ProjectStage::new(ENGINE, Some(Tier::Edit)), &mut board);
        assert_eq!(report.end, StageEnd::Finished);
        let after = board.snapshot();
        assert_ne!(old, after);
        Self {
            before,
            old,
            after,
            ops: board.ops_since_plan(),
            first_commit: board.commits_at[0],
            requests: board.requests,
        }
    }

    /// Cut at op `k` under `tear`, remount, check, run again, check.
    fn cut_and_finish(&self, k: u64, tear: TearModel) -> Outcome {
        let what = format!("cut at op {k}/{} {tear:?}", self.ops);
        let mut board = Board::tree(self.before.clone());
        board.set_plan(FaultPlan::cut(k, tear, k << 5 | tear as u64));
        let (report, _) = edge::run(&mut ProjectStage::new(ENGINE, Some(Tier::Edit)), &mut board);
        let mut flash = board.flash();
        flash.power_cycle(FaultPlan::none());

        let mut board = Board::tree(flash);
        let cut = board.snapshot();
        let outcome = self.check_whole(&cut, &what);
        if k < self.first_commit {
            assert_eq!(
                cut, self.old,
                "{what}: before the first commit, every byte as it was"
            );
        }

        let (rerun, _) = edge::run(&mut ProjectStage::new(ENGINE, Some(Tier::Edit)), &mut board);
        assert_eq!(rerun.end, StageEnd::Finished, "{what}: the rerun");
        assert_eq!(
            board.snapshot(),
            self.after,
            "{what}: the rerun's bytes are the uncut run's"
        );
        let _ = report;
        outcome
    }

    /// Every project in `snapshot` is wholly old or wholly new.
    fn check_whole(&self, snapshot: &BTreeMap<String, Vec<u8>>, what: &str) -> Outcome {
        let mut old = 0;
        let mut new = 0;
        for name in PROJECTS {
            let got = project_of(snapshot, name);
            if got == project_of(&self.after, name) {
                new += 1;
            } else {
                assert_eq!(
                    got,
                    project_of(&self.old, name),
                    "{what}: {name} is neither wholly 11 nor wholly 12"
                );
                old += 1;
            }
        }
        assert_eq!(
            snapshot.get(CONFIG),
            self.old.get(CONFIG),
            "{what}: the startup choice"
        );
        Outcome { old, new }
    }
}

struct Outcome {
    old: usize,
    new: usize,
}

#[derive(Default)]
struct Outcomes {
    cuts: u64,
    all_old: u64,
    mixed: u64,
    all_new: u64,
}

impl Outcomes {
    fn add(&mut self, outcome: Outcome) {
        self.cuts += 1;
        match (outcome.old, outcome.new) {
            (_, 0) => self.all_old += 1,
            (0, _) => self.all_new += 1,
            _ => self.mixed += 1,
        }
    }
}

impl std::fmt::Display for Outcomes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} cuts: {} left every project at 11, {} some at 12 (each whole), {} every project \
             at 12; every rerun finished with the uncut bytes",
            self.cuts, self.all_old, self.mixed, self.all_new
        )
    }
}

// --- fixtures --------------------------------------------------------------------

/// The board after an update to a format-12 engine: three format-11
/// projects and the startup choice naming one that is not first by name.
fn old_board() -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for name in PROJECTS {
        insert_project(
            &mut files,
            name,
            &upgrade_tests().join("corpus/v11").join(name),
        );
    }
    files.insert(
        String::from(CONFIG),
        format!("{{\"startup_project\":\"{STARTUP}\"}}").into_bytes(),
    );
    files
}

/// A project as the upgrader writes it at 12, under `/projects/<name>/`.
fn upgraded(name: &str) -> BTreeMap<String, Vec<u8>> {
    let mut files: ProjectFiles = read_dir(&upgrade_tests().join("corpus/v11").join(name))
        .into_iter()
        .collect();
    lpa_upgrade::upgrade_to(&mut files, ENGINE).expect("the corpus upgrades");
    files
        .into_pairs()
        .into_iter()
        .map(|(path, bytes)| (format!("/projects/{name}/{path}"), bytes))
        .collect()
}

fn insert_project(files: &mut BTreeMap<String, Vec<u8>>, name: &str, dir: &Path) {
    for (path, bytes) in read_dir(dir) {
        files.insert(format!("/projects/{name}/{path}"), bytes);
    }
}

fn upgrade_tests() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../lpa-upgrade/tests")
}

/// Every file under `dir`, keyed by its path inside it.
fn read_dir(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    collect(dir, dir, &mut out);
    assert!(!out.is_empty(), "{} is missing", dir.display());
    out.sort();
    out
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, root, out);
        } else {
            let key = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((key, std::fs::read(&path).unwrap()));
        }
    }
}

/// The toy step: stamps format 13 and adds one file. It knows nothing of
/// mapping, which is the point.
fn toy_v12_to_v13(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    let manifest = String::from_utf8_lossy(files.manifest().expect("manifest")).into_owned();
    let stamped = manifest.replacen("\"format\": 12", "\"format\": 13", 1);
    if stamped == manifest {
        return Err(UpgradeError::Refused {
            file: String::from("project.json"),
            reason: String::from("no format 12 to stamp"),
        });
    }
    files.insert("project.json", stamped.into_bytes());
    report.record_changed("project.json");
    files.insert("toy-13.txt", b"format 13\n".to_vec());
    report.record_added("toy-13.txt");
    Ok(())
}

//! The stage's edge in the simulation: each effect becomes the channel-1
//! requests it names, and each answer the matching input — the shape P11's
//! Studio and lp-cli edges take over the real wire. A link that goes is
//! `link_lost`.

use lpa_update::{BatchOutcome, ProjectStage, ProjectStageEffect, ProjectStageReport};
use lpa_upgrade::ProjectFiles;

use crate::board::{Answer, Board, CONFIG};

/// What the edge did, in order: `read <project>`, `write <project>`,
/// `load <project>` (the other requests are not logged).
pub type Trace = Vec<String>;

/// Run `stage` to its end over `board`'s link.
pub fn run(stage: &mut ProjectStage, board: &mut Board) -> (ProjectStageReport, Trace) {
    let mut trace = Trace::new();
    let mut effects = stage.start();
    loop {
        let effect = effects
            .pop()
            .expect("the stage always asks for something until it is done");
        assert!(effects.is_empty(), "one effect at a time");
        effects = match effect {
            ProjectStageEffect::Done(report) => return (report, trace),
            ProjectStageEffect::ListProjects => match board.list_projects() {
                None => stage.link_lost(),
                Some(Ok(paths)) => stage.projects_listed(paths),
                Some(Err(why)) => stage.read_failed(why),
            },
            ProjectStageEffect::ReadStartupChoice => match board.read(CONFIG) {
                None => stage.link_lost(),
                Some(answer) => stage.startup_choice_read(answer.ok()),
            },
            ProjectStageEffect::ReadManifest { project } => {
                match board.read(&format!("{}/project.json", absolute(&project))) {
                    None => stage.link_lost(),
                    Some(answer) => stage.manifest_read(&project, answer.ok()),
                }
            }
            ProjectStageEffect::ReadProject { project } => {
                trace.push(format!("read {project}"));
                match read_project(board, &project) {
                    None => stage.link_lost(),
                    Some(Ok(files)) => stage.project_read(&project, files),
                    Some(Err(why)) => stage.read_failed(why),
                }
            }
            ProjectStageEffect::WriteBatch {
                project,
                writes,
                deletes,
            } => {
                trace.push(format!("write {project}"));
                match write_batch(board, &project, &writes, &deletes) {
                    None => stage.link_lost(),
                    Some(outcome) => stage.batch_done(&project, outcome),
                }
            }
            ProjectStageEffect::LoadProject { project } => {
                trace.push(format!("load {project}"));
                match board.load(&absolute(&project)) {
                    None => stage.link_lost(),
                    Some(result) => stage.load_done(&project, result),
                }
            }
        };
    }
}

/// `ListDir` recursive, then a `Read` of every entry that is not a
/// directory (an entry another entry lies under), keyed inside the project.
fn read_project(board: &mut Board, project: &str) -> Answer<ProjectFiles> {
    let root = absolute(project);
    let entries = match board.list_dir(&root)? {
        Ok(entries) => entries,
        Err(why) => return Some(Err(why)),
    };
    let mut files = ProjectFiles::new();
    for entry in &entries {
        let is_dir = entries
            .iter()
            .any(|other| other.starts_with(&format!("{}/", entry.trim_end_matches('/'))));
        if is_dir {
            continue;
        }
        let bytes = match board.read(entry)? {
            Ok(bytes) => bytes,
            Err(why) => return Some(Err(why)),
        };
        let key = entry
            .strip_prefix(&format!("{root}/"))
            .expect("listed under the project");
        files.insert(key, bytes);
    }
    Some(Ok(files))
}

/// One batch: begin (and stop, writing nothing, on `atomic: false`),
/// write, delete, commit. A failure aborts; the link going is `None`.
fn write_batch(
    board: &mut Board,
    project: &str,
    writes: &[(String, Vec<u8>)],
    deletes: &[String],
) -> Option<BatchOutcome> {
    let root = absolute(project);
    match board.begin_batch()? {
        Ok(true) => {}
        Ok(false) => return Some(BatchOutcome::NotAtomic),
        Err(why) => return Some(BatchOutcome::Failed(why)),
    }
    let mut failed = None;
    for (path, bytes) in writes {
        if let Err(why) = board.write(&format!("{root}/{path}"), bytes)? {
            failed = Some(why);
            break;
        }
    }
    if failed.is_none() {
        for path in deletes {
            if let Err(why) = board.delete(&format!("{root}/{path}"))? {
                failed = Some(why);
                break;
            }
        }
    }
    if let Some(why) = failed {
        board.abort_batch()?.ok();
        return Some(BatchOutcome::Failed(why));
    }
    Some(match board.commit_batch()? {
        Ok(()) => BatchOutcome::Committed,
        Err(why) => BatchOutcome::Failed(why),
    })
}

/// A listed project path (`projects/<name>`) as an absolute fs path.
fn absolute(project: &str) -> String {
    format!("/{}", project.trim_start_matches('/').trim_end_matches('/'))
}

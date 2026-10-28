//! The Graph on every project in the repo: it opens each one, and each
//! lamp lands on the Graph where the engine lands it.
//!
//! The second statement reads the engine's landing goldens
//! (`lp-app/lpa-upgrade/tests/landing/<project path>.landing.json`, read
//! by `lp-core/lpc-engine/tests/upgrade_landing.rs`): where the live
//! engine landed every fixture lamp, recorded from the output's own
//! fragments. The Graph opens the project as Studio does (`Opened::open`,
//! then the port tables core builds with the engine's walk,
//! `graph_port_tables.rs`), and its effective patch must say the same, lamp
//! by lamp. The third does it again after a save: an unedited save writes
//! nothing, and every file the Graph owns, written and opened again, lands
//! every lamp where the golden says (mapping rebuild M4 P05). The goldens
//! are never re-blessed here: a difference is the Graph's to fix.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use lab_mapping_files::{DirFiles, Opened, ProductProject, Saved, Source};
use lab_mapping_model::{NodeId, Project};

use super::graph_port_tables::port_tables;

/// Where the engine's landing goldens live, from the workspace root.
const GOLDEN_ROOT: &str = "lp-app/lpa-upgrade/tests/landing";

/// The trees whose projects the Graph must open, from the workspace root.
const PROJECT_TREES: [&str; 3] = ["catalog", "projects", "lab/mapping/projects"];

/// The project format the Graph reads.
const PROJECT_FORMAT: u64 = 12;

/// The Graph opens every format-6 project in the repo: every project under
/// `catalog/`, `projects/` and `lab/mapping/projects/` opens, and every
/// fixture's mapping renders.
#[test]
fn the_graph_opens_every_format_6_project_in_the_repo() {
    let workspace = workspace_dir();
    let mut dirs = Vec::new();
    for tree in PROJECT_TREES {
        project_dirs(&workspace.join(tree), &mut dirs);
    }
    dirs.sort();
    let mut failures = Vec::new();
    let mut opened = 0usize;
    let mut fixtures = 0usize;
    for dir in &dirs {
        let name = relative(&workspace, dir);
        let result = Opened::open(&DirFiles::new(dir));
        let project = match result {
            Ok(o) => o.project,
            Err(why) => {
                failures.push(format!("{name}: doesn't open: {why}"));
                continue;
            }
        };
        opened += 1;
        let placed = project.placed();
        for n in project.fixtures() {
            fixtures += 1;
            if let Some(why) = placed.refusal(&n.id) {
                failures.push(format!("{name}: fixture {}'s mapping: {why}", n.id));
            }
        }
    }
    eprintln!(
        "the Graph opened {opened} of {} projects, {fixtures} fixtures",
        dirs.len()
    );
    assert!(
        failures.is_empty(),
        "the Graph can't open these:\n{}",
        failures.join("\n")
    );
    assert!(opened > 50, "only {opened} projects were found");
}

/// Every lamp lands on the Graph where the engine's landing golden says:
/// for every golden whose project is at format 12, each fixture's lamps,
/// in the fixture's lamp order, land on the same output, port and channel
/// (dark stays dark).
#[test]
fn every_lamp_lands_where_the_engines_landing_golden_says() {
    let workspace = workspace_dir();
    let mut totals = Totals::default();
    let mut failures = Vec::new();
    for (relative, text) in goldens(&workspace) {
        let dir = workspace.join(&relative);
        if !dir.join("project.json").is_file() {
            totals.skip(&relative, "its golden names a project that is not there");
            continue;
        }
        let format = project_format(&dir);
        if format != Some(PROJECT_FORMAT) {
            totals.skip(&relative, &format!("at project format {format:?}"));
            continue;
        }
        let golden = Golden::read(&text).unwrap_or_else(|why| panic!("{relative}: {why}"));
        if let Some(why) = &golden.refused {
            totals.skip(&relative, &format!("the engine refused it: {why}"));
            continue;
        }
        let Opened { mut project, files } = match Opened::open(&DirFiles::new(&dir)) {
            Ok(o) => o,
            Err(why) => {
                failures.push(format!("{relative}: doesn't open: {why}"));
                continue;
            }
        };
        project.set_port_tables(port_tables(&project, &files));
        let graph = graph_landing(&project, &files);
        totals.projects += 1;
        for (path, recorded) in &golden.fixtures {
            totals.fixtures += 1;
            totals.lamps += recorded.len() as u64;
            totals.dark += recorded.iter().filter(|l| l.is_empty()).count() as u64;
            let Some(now) = graph.get(path) else {
                totals.differences += recorded.len() as u64;
                failures.push(format!("{relative}: no fixture {path} on the Graph"));
                continue;
            };
            let differ = recorded
                .iter()
                .zip(now)
                .filter(|(was, is)| was != is)
                .count()
                + recorded.len().abs_diff(now.len());
            totals.differences += differ as u64;
            if differ > 0 {
                failures.push(first_difference(&relative, path, recorded, now));
            }
        }
        for path in graph.keys().filter(|p| !golden.fixtures.contains_key(*p)) {
            if !graph[path].is_empty() {
                failures.push(format!(
                    "{relative}: fixture {path} has lamps on the Graph and none in its golden"
                ));
            }
        }
    }
    eprintln!("{totals}");
    assert!(
        failures.is_empty(),
        "these lamps land on the Graph where the engine doesn't land them:\n{}",
        failures.join("\n")
    );
    assert!(
        totals.projects > 50,
        "only {} goldens were compared",
        totals.projects
    );
}

/// The Graph lands every lamp where the engine did after a save, too: for
/// every golden's project, an unedited save writes nothing, and every file
/// the Graph owns written out (each mapping by `lpc-mapping`'s format-6
/// writer) and opened again — port tables built from the written files —
/// lands each lamp where the golden says. What the Graph writes says all
/// the patching there is.
#[test]
fn every_lamp_lands_where_the_engines_golden_says_after_a_save() {
    let workspace = workspace_dir();
    let mut totals = Totals::default();
    let mut failures = Vec::new();
    for (relative, text) in goldens(&workspace) {
        let dir = workspace.join(&relative);
        if !dir.join("project.json").is_file() || project_format(&dir) != Some(PROJECT_FORMAT) {
            totals.skip(&relative, "not a format-12 project in the tree");
            continue;
        }
        let golden = Golden::read(&text).unwrap_or_else(|why| panic!("{relative}: {why}"));
        if let Some(why) = &golden.refused {
            totals.skip(&relative, &format!("the engine refused it: {why}"));
            continue;
        }
        let Ok(opened) = Opened::open(&DirFiles::new(&dir)) else {
            failures.push(format!("{relative}: doesn't open"));
            continue;
        };
        let unedited = opened
            .files
            .save(&opened.project)
            .unwrap_or_else(|e| panic!("{relative}: {e}"));
        if unedited != Saved::default() {
            failures.push(format!(
                "{relative}: an unedited save writes {:?}",
                unedited.writes.iter().map(|(p, _)| p).collect::<Vec<_>>()
            ));
        }
        let saved = opened
            .files
            .save_all(&opened.project)
            .unwrap_or_else(|e| panic!("{relative}: {e}"));
        totals.written += saved.writes.len();
        let overlay = Overlay {
            dir: DirFiles::new(&dir),
            saved: &saved,
        };
        let Opened { mut project, files } = match Opened::open(&overlay) {
            Ok(o) => o,
            Err(why) => {
                failures.push(format!("{relative}: what was written doesn't open: {why}"));
                continue;
            }
        };
        project.set_port_tables(port_tables(&project, &files));
        let graph = graph_landing(&project, &files);
        totals.projects += 1;
        for (path, recorded) in &golden.fixtures {
            totals.fixtures += 1;
            totals.lamps += recorded.len() as u64;
            totals.dark += recorded.iter().filter(|l| l.is_empty()).count() as u64;
            let Some(now) = graph.get(path) else {
                totals.differences += recorded.len() as u64;
                failures.push(format!("{relative}: no fixture {path} after a save"));
                continue;
            };
            let differ = recorded
                .iter()
                .zip(now)
                .filter(|(was, is)| was != is)
                .count()
                + recorded.len().abs_diff(now.len());
            totals.differences += differ as u64;
            if differ > 0 {
                failures.push(first_difference(&relative, path, recorded, now));
            }
        }
    }
    eprintln!("after a save, {totals}");
    assert!(
        failures.is_empty(),
        "after a save, these lamps land where the engine doesn't land them:\n{}",
        failures.join("\n")
    );
    assert!(
        totals.projects > 50,
        "only {} goldens were compared",
        totals.projects
    );
}

/// A project's directory with a save's writes over it.
struct Overlay<'a> {
    dir: DirFiles,
    saved: &'a Saved,
}

impl Source for Overlay<'_> {
    fn read(&self, path: &str) -> Option<String> {
        self.saved
            .writes
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, text)| text.clone())
            .or_else(|| self.dir.read(path))
    }
}

/// One lamp on one wire: the output's definition file, the port key and
/// the lamp's first channel on that port.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Landing {
    output: String,
    port: u32,
    channel: u32,
}

/// Every place each lamp lands, sorted, per lamp in fixture order.
type FixtureLanding = Vec<Vec<Landing>>;

/// A landing golden, read.
struct Golden {
    refused: Option<String>,
    /// By the fixture's definition file (`/fixture.json`).
    fixtures: BTreeMap<String, FixtureLanding>,
}

impl Golden {
    /// Read a golden as `upgrade_landing.rs`'s `from_golden` does: a run is
    /// `[first lamp, count, output def path, port key, first channel,
    /// channel step]`.
    fn read(text: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let refused = value
            .get("refused")
            .map(|why| why.as_str().map(String::from).ok_or("refused isn't text"))
            .transpose()?;
        let mut fixtures = BTreeMap::new();
        let listed = value["fixtures"].as_object().ok_or("no fixtures")?;
        for (path, fixture) in listed {
            let lamps = fixture["lamps"].as_u64().ok_or("no lamps")? as usize;
            let mut landings: FixtureLanding = vec![Vec::new(); lamps];
            for run in fixture["runs"].as_array().ok_or("no runs")? {
                let run = run.as_array().ok_or("a run isn't a list")?;
                let number = |i: usize| run.get(i).and_then(serde_json::Value::as_i64);
                let (Some(first), Some(count), Some(channel), Some(step)) =
                    (number(0), number(1), number(4), number(5))
                else {
                    return Err(format!("bad run {run:?}"));
                };
                let output = run[2].as_str().ok_or("a run's output")?;
                let port: u32 = run[3]
                    .as_str()
                    .and_then(|k| k.parse().ok())
                    .ok_or("a run's port")?;
                for k in 0..count {
                    landings
                        .get_mut((first + k) as usize)
                        .ok_or("a run past the fixture's lamps")?
                        .push(Landing {
                            output: output.to_string(),
                            port,
                            channel: (channel + step * k) as u32,
                        });
                }
            }
            for lands in &mut landings {
                lands.sort();
            }
            fixtures.insert(path.clone(), landings);
        }
        Ok(Self { refused, fixtures })
    }
}

/// Where the Graph lands every fixture's lamps, by the fixture's
/// definition file: the fixture's segments in its lamp order, each
/// segment's lamps in data order, each lamp on every run that holds it.
fn graph_landing(project: &Project, files: &ProductProject) -> BTreeMap<String, FixtureLanding> {
    let patch = project.effective_patch();
    let def_path = |id: &NodeId| {
        files
            .nodes
            .get(id)
            .map(|f| format!("/{}", f.path))
            .unwrap_or_else(|| format!("<no file for {id}>"))
    };
    let mut out = BTreeMap::new();
    for n in project.fixtures() {
        let mut lamps: FixtureLanding = Vec::new();
        for segment in project.segments_in(&n.id) {
            let runs: Vec<_> = patch.pixel_runs_of(&segment.id).collect();
            for lamp in 0..segment.lamps.len() as u32 {
                let mut lands: Vec<Landing> = runs
                    .iter()
                    .filter(|run| run.lamps.contains(&lamp))
                    .map(|run| Landing {
                        output: def_path(&run.output),
                        port: run.port.parse().unwrap_or(u32::MAX),
                        channel: run.pixel(lamp) * 3,
                    })
                    .collect();
                lands.sort();
                lamps.push(lands);
            }
        }
        out.insert(def_path(&n.id), lamps);
    }
    out
}

/// The first lamp the Graph lands differently, in words.
fn first_difference(
    project: &str,
    fixture: &str,
    recorded: &FixtureLanding,
    now: &FixtureLanding,
) -> String {
    if recorded.len() != now.len() {
        return format!(
            "{project} {fixture}: the engine has {} lamps, the Graph {}",
            recorded.len(),
            now.len()
        );
    }
    let differing = recorded.iter().zip(now).filter(|(a, b)| a != b).count();
    let (lamp, (was, is)) = recorded
        .iter()
        .zip(now)
        .enumerate()
        .find(|(_, (a, b))| a != b)
        .expect("a difference");
    format!(
        "{project} {fixture}: {differing} lamps differ; first, lamp {lamp}: the engine {was:?}, \
         the Graph {is:?}"
    )
}

/// Totals over every golden compared.
#[derive(Default)]
struct Totals {
    projects: usize,
    fixtures: usize,
    lamps: u64,
    dark: u64,
    differences: u64,
    /// Files a save wrote (after-a-save only).
    written: usize,
    skipped: Vec<String>,
}

impl Totals {
    fn skip(&mut self, project: &str, why: &str) {
        self.skipped.push(format!("{project}: {why}"));
    }
}

impl fmt::Display for Totals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "landing on the Graph: {} projects, {} fixtures, {} lamps compared, {} dark, \
             {} lamps landing differently",
            self.projects, self.fixtures, self.lamps, self.dark, self.differences
        )?;
        if self.written > 0 {
            write!(f, ", {} files written", self.written)?;
        }
        for skipped in &self.skipped {
            write!(f, "\n  skipped {skipped}")?;
        }
        Ok(())
    }
}

/// Every landing golden, by the project path it names (from the workspace
/// root), with its text; sorted.
fn goldens(workspace: &Path) -> Vec<(String, String)> {
    let root = workspace.join(GOLDEN_ROOT);
    let mut files = Vec::new();
    golden_files(&root, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let relative = relative(&root, &path);
            let project = relative
                .strip_suffix(".landing.json")
                .unwrap_or_else(|| panic!("{relative}: not a landing golden"))
                .to_string();
            let text = std::fs::read_to_string(&path).expect("a golden");
            (project, text)
        })
        .collect()
}

fn golden_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for path in entries(dir) {
        if path.is_dir() {
            golden_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Every directory under `dir` holding a `project.json`.
fn project_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.join("project.json").is_file() {
        out.push(dir.to_path_buf());
    }
    for path in entries(dir) {
        if path.is_dir() {
            project_dirs(&path, out);
        }
    }
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    read.filter_map(Result::ok).map(|e| e.path()).collect()
}

/// A project's format, from its `project.json`.
fn project_format(dir: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(dir.join("project.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value["format"].as_u64()
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("lpa-studio-core lives two levels under the workspace root")
        .to_path_buf()
}

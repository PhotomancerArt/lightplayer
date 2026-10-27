//! Reading projects and the landing goldens from the workspace, for the
//! tests that compare against what the live engine recorded (M3 P1).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lpa_upgrade::ProjectFiles;

/// `(output definition file, port key, channel)` of one lamp on one wire.
pub type Place = (String, u32, u32);

/// One recorded golden, read lamp by lamp.
pub struct Golden {
    /// The engine's words, when it refused the project.
    pub refused: Option<String>,
    /// Fixture node → each lamp's places.
    pub fixtures: BTreeMap<String, Vec<Vec<Place>>>,
}

/// Every golden under `tests/landing/`, with the project path it mirrors.
pub fn goldens(workspace: &Path) -> Vec<(String, Golden)> {
    let root = workspace.join("lp-app/lpa-upgrade/tests/landing");
    let mut paths = Vec::new();
    collect_files(&root, &mut paths);
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let Some(project) = path
            .strip_prefix(&root)
            .ok()
            .and_then(|relative| relative.to_str())
            .and_then(|relative| relative.strip_suffix(".landing.json"))
        else {
            continue;
        };
        let text = std::fs::read_to_string(&path).expect("golden");
        out.push((project.replace('\\', "/"), read_golden(&text)));
    }
    assert!(
        !out.is_empty(),
        "no landing goldens under {}",
        root.display()
    );
    out
}

fn read_golden(text: &str) -> Golden {
    let value: serde_json::Value = serde_json::from_str(text).expect("golden parses");
    let refused = value
        .get("refused")
        .and_then(|why| why.as_str())
        .map(String::from);
    let mut fixtures = BTreeMap::new();
    for (node, fixture) in value["fixtures"].as_object().expect("fixtures") {
        let lamps = fixture["lamps"].as_u64().expect("lamps") as usize;
        let mut places: Vec<Vec<Place>> = vec![Vec::new(); lamps];
        for run in fixture["runs"].as_array().expect("runs") {
            let first = run[0].as_i64().expect("first");
            let count = run[1].as_i64().expect("count");
            let output = run[2].as_str().expect("output");
            let port: u32 = run[3].as_str().expect("port").parse().expect("port key");
            let channel = run[4].as_i64().expect("channel");
            let step = run[5].as_i64().expect("step");
            for k in 0..count {
                places[(first + k) as usize].push((
                    String::from(output),
                    port,
                    (channel + step * k) as u32,
                ));
            }
        }
        for lamp in &mut places {
            lamp.sort();
        }
        fixtures.insert(node.clone(), places);
    }
    Golden { refused, fixtures }
}

/// Every file of the project at `dir`, keyed by its path in the project.
pub fn read_project(dir: &Path) -> ProjectFiles {
    let mut paths = Vec::new();
    collect_files(dir, &mut paths);
    paths
        .into_iter()
        .map(|path| {
            let key = path
                .strip_prefix(dir)
                .expect("under the project")
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(&path).expect("project file");
            (key, bytes)
        })
        .collect()
}

pub fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

pub fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("lpa-upgrade lives two levels under the workspace root")
        .to_path_buf()
}

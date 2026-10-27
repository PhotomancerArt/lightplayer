//! The frozen format-11 landing agrees with the live engine, lamp for lamp,
//! on every project in the repo.
//!
//! The "before" side is the landing goldens under `tests/landing/`, recorded
//! by `lpc-engine`'s `upgrade_landing` test from the live engine at project
//! format 11. [`lpa_upgrade::landing`] computes the same answer from the
//! files alone — the arithmetic the v11→v12 step is built on. The two share
//! no code, so a bug in the frozen copy cannot hide behind itself.
//!
//! ```bash
//! cargo test -p lpa-upgrade --test frozen_landing
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lpa_upgrade::{FixtureLayout, Landing, ProjectFiles, SegmentSource, landing};

/// For every project with a landing golden whose files are at format 11,
/// the frozen landing lands every lamp exactly where the engine did.
#[test]
fn the_frozen_landing_lands_every_lamp_where_the_engine_did() {
    let workspace = workspace_dir();
    let mut compared = 0usize;
    let mut skipped = Vec::new();
    let mut failures = Vec::new();
    for (project, golden) in goldens(&workspace) {
        if let Some(why) = golden.refused {
            skipped.push(format!("{project}: the engine refused it ({why})"));
            continue;
        }
        let files = read_project(&workspace.join(&project));
        let landed = match landing(&files) {
            Ok(landed) => landed,
            Err(error) => {
                failures.push(format!("{project}: the frozen landing refused it: {error}"));
                continue;
            }
        };
        compared += 1;
        let frozen = lamp_landings(&landed);
        // A map2d already at format 6 is not the frozen landing's to read
        // (the step passes it through byte for byte); the engine's parity
        // check covers it.
        let mut recorded = golden.fixtures;
        for fixture in &landed.fixtures {
            if fixture.layout == FixtureLayout::AlreadyFormat6 {
                recorded.remove(&fixture.node);
                skipped.push(format!(
                    "{project} {}: its map2d is already format 6",
                    fixture.node
                ));
            }
        }
        if let Some(difference) = first_difference(&recorded, &frozen) {
            failures.push(format!("{project}: {difference}"));
        }
    }
    eprintln!("frozen landing: compared {compared} projects");
    for skip in &skipped {
        eprintln!("  skipped {skip}");
    }
    assert!(
        failures.is_empty(),
        "the frozen landing disagrees with the engine:\n{}",
        failures.join("\n")
    );
    assert!(compared > 90, "only {compared} projects were compared");
}

/// The only segment in the repo that does not land as one window on one
/// port is quad-wire-oracle's 64-lamp grid, which its four 16-lamp ports
/// cut into four.
#[test]
fn the_only_cut_segment_is_quad_wire_oracles_grid() {
    let workspace = workspace_dir();
    let mut cuts = Vec::new();
    for (project, golden) in goldens(&workspace) {
        if golden.refused.is_some() {
            continue;
        }
        let Ok(landed) = landing(&read_project(&workspace.join(&project))) else {
            continue;
        };
        for fixture in &landed.fixtures {
            let FixtureLayout::Segments(segments) = &fixture.layout else {
                continue;
            };
            for segment in segments {
                if let Landing::Cut { why, pieces } = &segment.landing {
                    cuts.push(format!(
                        "{project} {} {}: {why} ({} pieces)",
                        fixture.node,
                        source_name(&segment.source),
                        pieces.len()
                    ));
                }
            }
        }
    }
    assert_eq!(
        cuts,
        vec![String::from(
            "projects/test/quad-wire-oracle /fixture.json panel: a port boundary falls inside \
             it (4 pieces)"
        )]
    );
}

// --- the golden side --------------------------------------------------------

/// `(output, port, channel)` of one lamp on one wire.
type Place = (String, u32, u32);

/// One recorded golden, read lamp by lamp.
struct Golden {
    refused: Option<String>,
    /// Fixture node → each lamp's places.
    fixtures: BTreeMap<String, Vec<Vec<Place>>>,
}

/// Every golden under `tests/landing/`, with the project path it mirrors.
fn goldens(workspace: &Path) -> Vec<(String, Golden)> {
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

// --- the frozen side --------------------------------------------------------

/// The frozen landing, lamp by lamp, in the golden's shape.
fn lamp_landings(landed: &lpa_upgrade::ProjectLanding) -> BTreeMap<String, Vec<Vec<Place>>> {
    let mut fixtures = BTreeMap::new();
    for fixture in &landed.fixtures {
        let mut places: Vec<Vec<Place>> = vec![Vec::new(); fixture.lamps as usize];
        if let FixtureLayout::Segments(segments) = &fixture.layout {
            for segment in segments {
                match &segment.landing {
                    Landing::Landed {
                        output,
                        port,
                        first_channel,
                        reversed,
                        rotation,
                    } => {
                        let n = segment.lamps;
                        for j in 0..n {
                            let flipped = if *reversed { n - 1 - j } else { j };
                            let slot = (flipped + rotation) % n;
                            places[(segment.first_lamp + j) as usize].push((
                                output.clone(),
                                *port,
                                first_channel + 3 * slot,
                            ));
                        }
                    }
                    Landing::Dark { .. } => {}
                    Landing::Cut { pieces, .. } => {
                        for piece in pieces {
                            for k in 0..piece.lamps {
                                let channel = i64::from(piece.first_channel)
                                    + i64::from(piece.step) * i64::from(k);
                                places[(piece.first_lamp + k) as usize].push((
                                    piece.output.clone(),
                                    piece.port,
                                    channel as u32,
                                ));
                            }
                        }
                    }
                }
            }
        }
        if fixture.layout != FixtureLayout::AlreadyFormat6 {
            fixtures.insert(fixture.node.clone(), places);
        }
    }
    fixtures
}

/// The first lamp the two sides disagree on, in words.
fn first_difference(
    golden: &BTreeMap<String, Vec<Vec<Place>>>,
    frozen: &BTreeMap<String, Vec<Vec<Place>>>,
) -> Option<String> {
    let recorded: Vec<&String> = golden.keys().collect();
    let computed: Vec<&String> = frozen.keys().collect();
    if recorded != computed {
        return Some(format!(
            "fixtures: the engine has {recorded:?}, the frozen landing {computed:?}"
        ));
    }
    for (node, lamps) in golden {
        let other = &frozen[node];
        if lamps.len() != other.len() {
            return Some(format!(
                "{node}: the engine has {} lamps, the frozen landing {}",
                lamps.len(),
                other.len()
            ));
        }
        for (lamp, (engine, computed)) in lamps.iter().zip(other).enumerate() {
            if engine != computed {
                return Some(format!(
                    "{node} lamp {lamp}: the engine lands it at {engine:?}, the frozen landing \
                     at {computed:?}"
                ));
            }
        }
    }
    None
}

fn source_name(source: &SegmentSource) -> String {
    match source {
        SegmentSource::Object {
            index,
            id,
            name,
            instances,
        } => {
            let mut text = id.clone().unwrap_or_else(|| {
                if name.is_empty() {
                    format!("object {index}")
                } else {
                    name.clone()
                }
            });
            for instance in instances {
                text.push_str(&format!("@{instance}"));
            }
            text
        }
        SegmentSource::PointList { key, .. } => format!("path {key}"),
    }
}

// --- helpers ----------------------------------------------------------------

/// Every file of the project at `dir`, keyed by its path in the project.
fn read_project(dir: &Path) -> ProjectFiles {
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

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
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

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("lpa-upgrade lives two levels under the workspace root")
        .to_path_buf()
}

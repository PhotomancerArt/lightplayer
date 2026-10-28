//! Saving map2d format 6 (mapping rebuild M4 P05): the Graph saves exactly
//! what a person made, with `lpc-mapping`'s own writer, and nothing when
//! they made nothing.
//!
//! The repo-wide statements walk every project under `catalog/`,
//! `projects/` and `lab/mapping/projects/` at project format 12.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::{fixture, node, open_files, part, project_around};
use lab_mapping_files::embedded::{CHOKER, EAVES, ZOOK_DOME};
use lab_mapping_files::{DirFiles, MemFiles, Opened, Saved, Source};
use lab_mapping_model::{NodeId, Project, Vec2};
use lpc_mapping::edit::{self, OverrideField};
use lpc_mapping::map2d6;
use lpc_mapping::part::{PartKind, Path as PathPart, SegmentKey, Shape, TreeId};
use lpc_mapping::patching::{Direction, PortKey, PortOp};

/// Saving a project nobody edited writes nothing, and removes nothing, on
/// every project in the repo.
#[test]
fn saving_a_project_nobody_edited_writes_nothing() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for (name, dir) in repo_projects() {
        let o = open_dir(&dir);
        let saved = o
            .files
            .save(&o.project)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        if saved != Saved::default() {
            failures.push(format!(
                "{name}: writes {:?}, removes {:?}",
                paths(&saved),
                saved.removed
            ));
        }
        checked += 1;
    }
    eprintln!(
        "unedited saves: {checked} projects, {} wrote something",
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(checked > 50, "only {checked} projects were found");
}

/// Every mapping in the repo is the writer's own form: read and written
/// back, it is the same bytes (so the first edit's diff is the edit, not a
/// reformat).
#[test]
fn every_mapping_in_the_repo_is_the_writers_own_form() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for (name, dir) in repo_projects() {
        let o = open_dir(&dir);
        for f in o.files.nodes.values() {
            let Some(map) = &f.map else { continue };
            checked += 1;
            let again = map2d6::write(&map.mapping).expect("it writes");
            if again != map.text {
                failures.push(format!("{name}/{}", map.path));
            }
        }
    }
    eprintln!(
        "mappings written back: {checked}, {} differ",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "not in the writer's form:\n{}",
        failures.join("\n")
    );
}

/// Saving after an edit writes only that fixture's map2d, and a saved
/// mapping reopens equal — ids, counters, ring keys, starts, flows and
/// overrides — on every project in the repo. The edit: a new part minted
/// into each fixture, and an override (reversed, one lamp along) on its
/// first segment. No edit removes a file.
#[test]
fn an_edit_writes_only_its_mapping_and_reopens_equal_on_every_project() {
    let mut edited = 0;
    let mut failures = Vec::new();
    for (name, dir) in repo_projects() {
        let o = open_dir(&dir);
        let mut p = o.project.clone();
        let mut want: BTreeMap<NodeId, String> = BTreeMap::new();
        for id in fixture_ids(&p) {
            let Some(map) = o.files.nodes.get(&id).and_then(|f| f.map.as_ref()) else {
                continue;
            };
            let first = p.segments_in(&id).first().map(|s| s.id.id.clone());
            let new = p.add(&id, a_little_path()).expect("a part");
            let f = p.fixture_mut(&id).unwrap();
            let target = first.unwrap_or_else(|| TreeId::of(&new.part));
            edit::set_override_field(
                &mut f.mapping,
                &target,
                OverrideField::Direction(Direction::Reverse),
            )
            .unwrap();
            edit::set_override_field(&mut f.mapping, &target, OverrideField::Offset(1)).unwrap();
            want.insert(id, map.path.clone());
        }
        if want.is_empty() {
            continue;
        }
        edited += 1;
        let saved = o.files.save(&p).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut expected: Vec<String> = want.values().cloned().collect();
        expected.sort();
        if paths(&saved) != expected || !saved.removed.is_empty() {
            failures.push(format!(
                "{name}: wrote {:?} (want {expected:?}), removed {:?}",
                paths(&saved),
                saved.removed
            ));
            continue;
        }
        let again = reopen_dir(&dir, &saved);
        for id in want.keys() {
            let was = &p.fixture(id).unwrap().mapping;
            let is = &again.project.fixture(id).unwrap().mapping;
            if was != is {
                failures.push(format!("{name}: {id} reopens as another mapping"));
            }
        }
        // Saved once: nothing is left to write.
        let twice = again.files.save(&p).unwrap();
        if twice != Saved::default() {
            failures.push(format!("{name}: a second save writes {:?}", paths(&twice)));
        }
    }
    eprintln!(
        "edited and saved: {edited} projects, {} failed",
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(edited > 50, "only {edited} projects were edited");
}

/// A circle's rings keep their keys across a save (F17): with ring r2
/// deleted and a ring added, the override on `circle1/r3` still finds
/// ring r3 (radius 22, 12 lamps) after the mapping is saved and opened
/// again, and the new ring has a key never used before (r6), its counter
/// saved with it.
#[test]
fn a_circles_rings_keep_their_keys_across_a_save() {
    let dir = workspace().join("projects/test/circular-display-6");
    let o = open_dir(&dir);
    let fixture = NodeId::new("fixture");
    let mut p = o.project.clone();
    let circle = lpc_mapping::part::PartId::new("circle1").unwrap();
    let f = p.fixture_mut(&fixture).unwrap();
    edit::remove_ring(&mut f.mapping, &circle, &SegmentKey::new("r2").unwrap()).unwrap();
    let added = edit::add_ring(&mut f.mapping, &circle, 37.0, 20).unwrap();
    assert_eq!(added.to_string(), "r6");
    let saved = o.files.save(&p).unwrap();
    assert_eq!(paths(&saved), ["fixture.map2d.json"]);

    let again = reopen_dir(&dir, &saved).project;
    let m = &again.fixture(&fixture).unwrap().mapping;
    assert_eq!(m, &p.fixture(&fixture).unwrap().mapping);
    let r3 = TreeId::parse("circle1/r3").unwrap();
    assert_eq!(
        m.overrides[&r3].port,
        Some(PortOp::Set {
            key: PortKey(1),
            plus: 0
        })
    );
    let ring = again
        .segments_in(&fixture)
        .into_iter()
        .find(|s| s.id.id == r3)
        .expect("ring r3 is still there");
    assert_eq!(ring.lamps.len(), 12);
    assert_eq!(
        again.effective_patch().landing(&ring.id, 0).map(|l| l.port),
        Some("1".to_string()),
        "and its override still puts it on port 1"
    );
    let PartKind::Shape(Shape::Circle(c)) = &m.part(&circle).unwrap().kind else {
        panic!("a circle");
    };
    let keys: Vec<String> = c.rings.iter().map(|r| r.key.to_string()).collect();
    assert_eq!(keys, ["r1", "r3", "r4", "r5", "r6"]);
    assert_eq!(c.next_ring, 7, "r2 and r6 are never minted again");
}

/// Undo on the Graph puts back the mapping it opened with, and saving that
/// writes the file's own bytes back.
#[test]
fn saving_what_undo_put_back_writes_the_files_own_bytes() {
    let o = common::open(&CHOKER);
    let fixture = NodeId::new("fixture");
    let mut e = lab_mapping_model::Editor::new(o.project.clone());
    let first = e.project.fixture(&fixture).unwrap().parts()[0].id.clone();
    e.select(vec![lab_mapping_model::Target::Part(
        lab_mapping_model::PartRef::new(&fixture, &first),
    )]);
    assert!(e.press(lab_mapping_model::GraphVerb::More));
    let saved = o.files.save(&e.project).unwrap();
    assert_eq!(paths(&saved), ["playful.map2d.json"]);
    let again = reopen(&CHOKER, &saved);
    assert_eq!(again.project.lamp_count(), 74);

    assert!(e.press(lab_mapping_model::GraphVerb::Undo));
    let back = again.files.save(&e.project).unwrap();
    assert_eq!(paths(&back), ["playful.map2d.json"]);
    let original = CHOKER.files().read("playful.map2d.json").unwrap();
    assert_eq!(back.writes[0].1, original, "the file's own bytes");
}

/// Moving the zook dome rewrites its mapping and nothing else, and the
/// mapping reads back with the move.
#[test]
fn moving_a_part_writes_only_its_mapping() {
    let o = common::open(&ZOOK_DOME);
    let mut p = o.project.clone();
    let fixture = NodeId::new("fixture");
    let repeat = p.fixture(&fixture).unwrap().parts()[0].id.clone();
    let f = p.fixture_mut(&fixture).unwrap();
    let moved = f.part_mut(&repeat).unwrap();
    lab_mapping_model::domain::part_geometry::translate(&mut moved.kind, Vec2::new(2.0, 0.0));
    let saved = o.files.save(&p).unwrap();
    assert_eq!(paths(&saved), ["fixture.map2d.json"]);
    assert!(saved.removed.is_empty());
    let again = reopen(&ZOOK_DOME, &saved).project;
    assert_eq!(
        again.fixture(&fixture).unwrap().mapping,
        p.fixture(&fixture).unwrap().mapping
    );
}

/// Resizing a fixture's box writes its canvas from the box's corner, and
/// the box opens again that size.
#[test]
fn resizing_a_fixture_writes_its_canvas() {
    let o = common::open(&CHOKER);
    let fixture = NodeId::new("fixture");
    let mut p = o.project.clone();
    p.fixture_mut(&fixture).unwrap().size = Vec2::new(140.0, 40.0);
    let saved = o.files.save(&p).unwrap();
    assert_eq!(paths(&saved), ["playful.map2d.json"]);
    let again = reopen(&CHOKER, &saved).project;
    assert_eq!(
        again.fixture(&fixture).unwrap().mapping.canvas,
        Some([0.0, 0.0, 140.0, 40.0])
    );
    assert_eq!(
        again.fixture(&fixture).unwrap().size,
        Vec2::new(140.0, 40.0)
    );
}

/// Deleting the choker's shader removes it from the module and reports its
/// file as no longer used; no mapping is written.
#[test]
fn deleting_a_node_drops_it_from_its_module() {
    let o = common::open(&CHOKER);
    let mut p = o.project.clone();
    p.remove_node(&NodeId::new("shader"));
    let saved = o.files.save(&p).unwrap();
    assert_eq!(paths(&saved), ["module.json"]);
    assert_eq!(saved.removed, ["shader.json"]);
}

/// A port count edited on an output writes the output's file, and only it.
#[test]
fn a_port_count_writes_its_output() {
    let o = common::open(&CHOKER);
    let mut p = o.project.clone();
    let out = NodeId::new("output");
    p.output_mut(&out).unwrap().ports[0].pixels = Some(80);
    let saved = o.files.save(&p).unwrap();
    assert_eq!(paths(&saved), ["output.json"]);
    let again = reopen(&CHOKER, &saved).project;
    assert_eq!(again.output(&out).unwrap().ports[0].pixels, Some(80));
}

/// The circular display, saved after a journey's edits (ring 3 reversed on
/// another port, a flow on the circle), opens landing every lamp where it
/// did: the patching a person made is all in the file.
#[test]
fn a_journeys_patching_reopens_landing_the_same() {
    let files = project_around(
        &common::format6_doc("circular_display"),
        "main",
        &[("ws281x:local:D0", Some(64)), ("ws281x:local:D1", Some(64))],
    );
    let o = open_files(&files);
    let mut p = o.project.clone();
    let f = p.fixture_mut(&fixture()).unwrap();
    let r3 = node("circle1/r3").id;
    edit::set_override_field(
        &mut f.mapping,
        &r3,
        OverrideField::Direction(Direction::Reverse),
    )
    .unwrap();
    edit::set_flow(
        &mut f.mapping,
        lpc_mapping::patching::PatchTarget::Part(part("circle1").part),
        lpc_mapping::patching::Flow::OnePerPort { from: PortKey(0) },
    )
    .unwrap();
    let saved = o.files.save(&p).unwrap();
    assert_eq!(paths(&saved), ["fixture.map2d.json"]);
    let mut again_files = files.clone();
    for (path, text) in &saved.writes {
        again_files.files.insert(path.clone(), text.clone());
    }
    let again = open_files(&again_files).project;
    assert_eq!(
        again.effective_patch().pixel_runs,
        p.effective_patch().pixel_runs
    );
    assert_eq!(
        again.fixture(&fixture()).unwrap().mapping,
        p.fixture(&fixture()).unwrap().mapping
    );
}

/// The desk (the lab's own seed, every node new) saves as a new project:
/// module, fixtures, outputs, a stand-in shader and a format-6 mapping for
/// each fixture; it opens again with the same lamps landing on the same
/// pixels.
#[test]
fn the_desk_saves_as_a_new_project() {
    let o = common::open(&CHOKER);
    let empty = lab_mapping_files::ProductProject {
        nodes: Default::default(),
        root: serde_json::json!({"kind": "Module", "nodes": {}}),
        ..o.files.clone()
    };
    let desk = Project::seed();
    let saved = empty.save(&desk).unwrap();
    for (path, text) in &saved.writes {
        if path.ends_with(".map2d.json") {
            assert_eq!(
                map2d6::peek_format(text),
                Ok(map2d6::MAP2D6_FORMAT),
                "{path}"
            );
        }
    }
    let mut files = MemFiles::new(saved.writes.clone());
    files.files.insert(
        "project.json".into(),
        r#"{"format": 12, "name": "desk"}"#.into(),
    );
    let again = Opened::open(&files).unwrap().project;
    assert_eq!(again.lamp_count(), desk.lamp_count());
    assert_eq!(wire(&again), wire(&desk));
}

/// J2's ending (Sean's eaves): what the patching journey made, saved and
/// opened again, lands the same.
#[test]
fn the_eaves_patched_by_hand_save_and_reopen_the_same() {
    let o = common::open(&EAVES);
    let mut p = o.project.clone();
    let fixture = p.fixtures()[0].id.clone();
    let segments: Vec<_> = p
        .segments_in(&fixture)
        .iter()
        .map(|s| s.id.clone())
        .collect();
    let f = p.fixture_mut(&fixture).unwrap();
    edit::set_override_field(
        &mut f.mapping,
        &segments[0].id,
        OverrideField::Direction(Direction::Reverse),
    )
    .unwrap();
    edit::set_override_field(
        &mut f.mapping,
        &segments[1].id,
        OverrideField::Port(PortOp::Set {
            key: PortKey(1),
            plus: 0,
        }),
    )
    .unwrap();
    let saved = o.files.save(&p).unwrap();
    assert_eq!(paths(&saved), ["fixture.map2d.json"]);
    let again = reopen(&EAVES, &saved).project;
    assert_eq!(
        again.effective_patch().pixel_runs,
        p.effective_patch().pixel_runs
    );
}

// ---- helpers --------------------------------------------------------------

/// A three-lamp path, for an edit that mints a part.
fn a_little_path() -> PartKind {
    PartKind::Shape(Shape::Path(PathPart::through(&[[1.0, 1.0], [5.0, 5.0]], 3)))
}

/// Every lamp on every pixel: (output, port, pixel) → (fixture, where the
/// lamp is in its fixture, to a thousandth). Names as the product spells
/// them.
fn wire(p: &Project) -> Vec<(String, String, u32, String, i64, i64)> {
    let mut w = Vec::new();
    for s in &p.effective_patch().pixel_runs {
        let Some(segment) = p.segment(&s.segment) else {
            continue;
        };
        for lamp in s.lamps.clone() {
            let at = segment.lamps[lamp as usize].pos;
            w.push((
                p.node_name(&s.output).replace(' ', "_"),
                s.port.clone(),
                s.pixel(lamp),
                p.node_name(&s.segment.fixture).replace(' ', "_"),
                (at.x * 1000.0).round() as i64,
                (at.y * 1000.0).round() as i64,
            ));
        }
    }
    w.sort();
    w
}

fn paths(saved: &Saved) -> Vec<String> {
    saved.writes.iter().map(|(p, _)| p.clone()).collect()
}

fn fixture_ids(p: &Project) -> Vec<NodeId> {
    p.fixtures().iter().map(|n| n.id.clone()).collect()
}

fn reopen(e: &lab_mapping_files::embedded::Embedded, saved: &Saved) -> Opened {
    let mut files = e.files();
    for (path, text) in &saved.writes {
        files.files.insert(path.clone(), text.clone());
    }
    Opened::open(&files).unwrap_or_else(|err| panic!("{}: {err}", e.key))
}

/// A project directory with `saved`'s writes over it.
struct Overlay<'a> {
    dir: DirFiles,
    writes: &'a Saved,
}

impl Source for Overlay<'_> {
    fn read(&self, path: &str) -> Option<String> {
        self.writes
            .writes
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, t)| t.clone())
            .or_else(|| self.dir.read(path))
    }
}

fn reopen_dir(dir: &Path, saved: &Saved) -> Opened {
    let overlay = Overlay {
        dir: DirFiles::new(dir),
        writes: saved,
    };
    Opened::open(&overlay).unwrap_or_else(|err| panic!("{}: {err}", dir.display()))
}

fn open_dir(dir: &Path) -> Opened {
    Opened::open(&DirFiles::new(dir)).unwrap_or_else(|err| panic!("{}: {err}", dir.display()))
}

/// Every project in the repo at project format 12, by its path from the
/// workspace root.
fn repo_projects() -> Vec<(String, PathBuf)> {
    let root = workspace();
    let mut dirs = Vec::new();
    for tree in ["catalog", "projects", "lab/mapping/projects"] {
        project_dirs(&root.join(tree), &mut dirs);
    }
    dirs.sort();
    dirs.into_iter()
        .filter(|d| project_format(d) == Some(12))
        .map(|d| {
            let name = d.strip_prefix(&root).unwrap_or(&d).display().to_string();
            (name, d)
        })
        .collect()
}

fn project_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.join("project.json").is_file() {
        out.push(dir.to_path_buf());
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for e in read.filter_map(Result::ok) {
        if e.path().is_dir() {
            project_dirs(&e.path(), out);
        }
    }
}

fn project_format(dir: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(dir.join("project.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v["format"].as_u64()
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

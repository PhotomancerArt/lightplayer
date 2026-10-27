//! The v11→v12 step: every lamp of every fixture lands where it landed
//! before, read back by the real format-6 reader and placed by the real
//! patcher (`lpc-mapping`, a dev-dependency only), never by the step's own
//! arithmetic.
//!
//! "Before" is the live engine's landing goldens (`tests/landing/`, M3 P1)
//! for every project in the repo, and the frozen landing
//! ([`lpa_upgrade::landing`], itself proved against those goldens) for the
//! synthetic cases. "After" is the step's files: each fixture's map2d read
//! by `map2d6::read`, rendered, and patched against the project's named
//! outputs, lamp by lamp. Positions are compared too: every lamp's position
//! bit for bit, through the fit for hand-placed lamps.
//!
//! ```bash
//! cargo test -p lpa-upgrade --test step_v11_to_v12 -- --nocapture
//! ```

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{Place, goldens, read_project, workspace_dir};
use lpa_upgrade::{
    FixtureLayout, Landing, ProjectFiles, ProjectLanding, landing, upgrade_v11_to_v12,
};
use lpc_mapping::map2d6;
use lpc_mapping::part::render;
use lpc_mapping::patching::{OutputName, OutputPorts, Port, PortKey, Problem, patch_mapping};
use lpc_mapping::{Bounds2d, Map2dDoc, fit_points, fit_scale, resolve};

// --- the gate: every project in the repo --------------------------------------

/// For every project with a landing golden (every project in the repo at
/// format 11), the step's files land every lamp where the live engine did,
/// keep every lamp's position bit for bit, write the format-6 writer's own
/// bytes, report exactly the files it changed, added and removed, and come
/// out the same twice. The one project refused is quad-wire-oracle, whose
/// grid its ports cut (hand-migrated in P4, lean Q70).
#[test]
fn every_repo_project_lands_every_lamp_where_the_engine_did() {
    let workspace = workspace_dir();
    let mut migrated = 0usize;
    let mut skipped = Vec::new();
    let mut refused = Vec::new();
    let mut failures = Vec::new();
    let mut summaries = Vec::new();
    for (project, golden) in goldens(&workspace) {
        if let Some(why) = golden.refused {
            skipped.push(format!(
                "{project}: the engine refused it at format 11 ({why})"
            ));
            continue;
        }
        let before = read_project(&workspace.join(&project));
        let mut after = before.clone();
        let report = match upgrade_v11_to_v12(&mut after) {
            Ok(report) => report,
            Err(error) => {
                if after != before {
                    failures.push(format!("{project}: a refusal changed the files"));
                }
                refused.push(format!("{project}: {error}"));
                continue;
            }
        };
        migrated += 1;
        let nodes: Vec<String> = golden.fixtures.keys().cloned().collect();
        let checked = after_landing(&after, &nodes)
            .and_then(|lands| compare(&golden.fixtures, &lands.places).map(|()| lands))
            .and_then(|lands| {
                positions_kept(&before, &after, &nodes)?;
                files_reported(&before, &after, &report)?;
                deterministic(&before, &after)?;
                Ok(lands)
            });
        match checked {
            Ok(lands) => summaries.push(format!(
                "{project}: {}; outputs by name {:?}; patch files removed {:?}, map2d added {:?}",
                lands.summary, lands.named, report.removed_files, report.added_files
            )),
            Err(error) => failures.push(format!("{project}: {error}")),
        }
    }
    eprintln!("v11→v12: {migrated} projects migrated, every lamp where the engine put it");
    for line in &summaries {
        eprintln!("  {line}");
    }
    for line in &skipped {
        eprintln!("  skipped {line}");
    }
    for line in &refused {
        eprintln!("  REFUSED {line}");
    }
    assert!(
        failures.is_empty(),
        "the step moved lamps:\n{}",
        failures.join("\n")
    );
    assert_eq!(
        refused.len(),
        1,
        "only quad-wire-oracle is refused:\n{}",
        refused.join("\n")
    );
    assert!(
        refused[0].starts_with("projects/test/quad-wire-oracle: "),
        "{}",
        refused[0]
    );
    assert!(migrated > 90, "only {migrated} projects were migrated");
}

/// The two goldens the engine recorded as refused (old-uid-sign's
/// `module.json` carries a `uid` the engine's module reader rejects;
/// fault-demo's shader runs out of fuel every frame) have no lamps to
/// compare with. The step still migrates them, and lands every lamp where
/// the frozen landing says (the frozen landing being the engine's on every
/// other project).
#[test]
fn the_projects_the_engine_refused_land_as_the_frozen_landing_says() {
    let workspace = workspace_dir();
    let mut checked = Vec::new();
    for (project, golden) in goldens(&workspace) {
        if golden.refused.is_none() {
            continue;
        }
        let before = read_project(&workspace.join(&project));
        let mut after = before.clone();
        upgrade_v11_to_v12(&mut after).unwrap_or_else(|e| panic!("{project}: {e}"));
        lands_as_today(&before, &after);
        checked.push(project);
    }
    eprintln!("checked against the frozen landing: {checked:?}");
    assert_eq!(checked.len(), 2, "{checked:?}");
}

// --- statements, on small projects ----------------------------------------------

/// A fixture that only flows gets one start and no overrides.
#[test]
fn a_fixture_that_only_flows_gets_one_start_and_no_overrides() {
    let after = step(&fixture_project(
        &paths(&[("a", 4), ("b", 6)]),
        None,
        &[output("o", None, r#"{"0":{"count":100}}"#)],
    ));
    let doc = map2d(&after, "f.map2d.json");
    assert_eq!(
        doc["start"],
        serde_json::json!({"output": {"set": "1"}, "port": {"set": 0}, "channel": {"set": {"lamps": 0}}})
    );
    assert!(doc.get("overrides").is_none(), "{doc}");
}

/// A segment a row puts on the next port's first lamp, where the patcher
/// would wrap it anyway, needs no override.
#[test]
fn a_segment_that_wraps_as_the_patcher_would_needs_no_override() {
    let after = step(&fixture_project(
        &paths(&[("a", 7), ("b", 6)]),
        Some(r#"{"format":2,"entries":[["/a",-1,0],["/b",-1,10]]}"#),
        &[output("o", None, r#"{"0":{"count":10},"1":{"count":10}}"#)],
    ));
    assert!(
        map2d(&after, "f.map2d.json").get("overrides").is_none(),
        "{}",
        map2d(&after, "f.map2d.json")
    );
}

/// A reversed row becomes a reversed override; a rotation becomes an
/// offset.
#[test]
fn a_reversed_rotated_row_becomes_direction_and_offset() {
    let after = step(&fixture_project(
        &paths(&[("a", 3), ("b", 5)]),
        Some(r#"{"format":2,"entries":[["/a",-1,0],["/b",-1,3,"r",2]]}"#),
        &[output("o", None, r#"{"0":{"count":100}}"#)],
    ));
    assert_eq!(
        map2d(&after, "f.map2d.json")["overrides"],
        serde_json::json!({"b": {"direction": "reverse", "offset": 2}})
    );
}

/// Under manual, a segment no row claims is off, and the cursor does not
/// move past it.
#[test]
fn a_segment_no_row_claims_under_manual_is_off() {
    let after = step(&fixture_project(
        &paths(&[("a", 3), ("b", 5)]),
        Some(r#"{"format":3,"flow":"manual","entries":[["/b",-1,0]]}"#),
        &[output("o", None, r#"{"0":{"count":100}}"#)],
    ));
    let doc = map2d(&after, "f.map2d.json");
    assert_eq!(doc["overrides"], serde_json::json!({"a": {"off": true}}));
    assert_eq!(
        doc["start"]["channel"],
        serde_json::json!({"set": {"lamps": 0}})
    );
}

/// A fixture whose channel no output reads has no start and lights
/// nothing; the report says so.
#[test]
fn a_fixture_no_output_reads_has_no_start_and_lights_nothing() {
    let before = project(&[
        ("project.json", r#"{"format":11,"name":"t"}"#),
        (
            "module.json",
            r#"{"kind":"Module","nodes":{"f":{"ref":"./f.json"},"o":{"ref":"./o.json"}}}"#,
        ),
        (
            "f.json",
            r#"{"kind":"Fixture","bindings":{"output":{"target":"bus:control.other"}},"mapping":{"kind":"Map2d","source":"f.map2d.json"}}"#,
        ),
        ("f.map2d.json", &paths(&[("a", 4)])),
        ("o.json", r#"{"kind":"Output","ports":{"0":{"count":10}}}"#),
    ]);
    let mut after = before.clone();
    let report = upgrade_v11_to_v12(&mut after).unwrap();
    lands_as_today(&before, &after);
    let doc = map2d(&after, "f.map2d.json");
    assert!(doc.get("start").is_none(), "{doc}");
    assert!(doc.get("overrides").is_none(), "{doc}");
    assert!(
        report.warnings.iter().any(|w| w.contains("lights nothing")),
        "{:?}",
        report.warnings
    );
    // Nothing names the output, so it stays unnamed and untouched.
    assert_eq!(after.get("o.json"), before.get("o.json"));
}

/// An unnamed output a start names is named "1" in its own file, and
/// nothing else in that file moves.
#[test]
fn an_unnamed_output_a_start_names_is_named_1_and_nothing_else_moves() {
    let output_text = "{\n  \"kind\": \"Output\",\n  \"ports\": {\n    \"0\": {\n      \"endpoint\": \"ws281x:local:IO18\",\n      \"count\": 10\n    }\n  },\n  \"options\": {\n    \"white_point\": [\n      1.0e0,\n      1,\n      1.00\n    ]\n  }\n}\n";
    let before = fixture_project(
        &paths(&[("a", 4)]),
        None,
        &[("o.json".to_string(), output_text.to_string())],
    );
    let after = step(&before);
    let expected = output_text.replace(
        "\"kind\": \"Output\",\n",
        "\"kind\": \"Output\",\n  \"name\": \"1\",\n",
    );
    assert_eq!(
        std::str::from_utf8(after.get("o.json").unwrap()).unwrap(),
        expected
    );
}

/// An object without an id takes its name's slug, or kind and number when
/// the slug collides (logo-sign's `L` and `l`).
#[test]
fn an_object_without_an_id_takes_its_slug_or_kind_and_number() {
    let after = step(&fixture_project(
        r#"{"format":1,"objects":[
            {"name":"L","shape":{"path":{"points":[[0,0],[1,0]],"count":2}}},
            {"name":"l","shape":{"path":{"points":[[0,1],[1,1]],"count":2}}},
            {"name":"F-lower","shape":{"grid":{"origin":[0,2],"cols":2,"rows":1,"pitch":1}}}
        ]}"#,
        None,
        &[output("o", None, r#"{"0":{}}"#)],
    ));
    let doc = map2d(&after, "f.map2d.json");
    let ids: Vec<&str> = doc["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| part["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["path1", "path2", "f-lower"]);
    assert_eq!(doc["next"], serde_json::json!({"path": 3}));
}

/// A repeat's copies keep today's row names: `/rim-a/2` becomes `rim-a@2`.
#[test]
fn a_repeats_copies_keep_todays_row_names() {
    let after = step(&fixture_project(
        r#"{"format":3,"objects":[{"id":"rim-a","name":"rim a","shape":{"repeat":{
            "center":[0,0],"count":3,"shape":{"path":{"points":[[1,0],[2,0]],"count":4}}}}}]}"#,
        Some(r#"{"format":2,"entries":[["/rim-a/2",-1,40,"r"]]}"#),
        &[output("o", None, r#"{"0":{}}"#)],
    ));
    let doc = map2d(&after, "f.map2d.json");
    assert_eq!(doc["parts"][0]["id"], "repeat1");
    assert_eq!(doc["parts"][0]["kind"]["repeat"]["part"]["id"], "rim-a");
    let keys: Vec<&String> = doc["overrides"].as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["rim-a@2"]);
}

/// Nested repeat copies are named innermost first: today's `/x/1/0`
/// (outer copy 1, inner copy 0) is `x@0@1`, and `/x/0/2` is `x@2`.
#[test]
fn nested_repeat_copies_are_named_innermost_first() {
    let after = step(&fixture_project(
        r#"{"format":3,"objects":[{"id":"x","shape":{"repeat":{"center":[0,0],"count":2,
            "shape":{"repeat":{"center":[0,0],"count":3,
                "shape":{"path":{"points":[[1,0],[2,0]],"count":2}}}}}}}]}"#,
        Some(r#"{"format":2,"entries":[["/x/1/0",-1,50,"r"],["/x/0/2",-1,60,"r"]]}"#),
        &[output("o", None, r#"{"0":{}}"#)],
    ));
    let doc = map2d(&after, "f.map2d.json");
    let overrides = doc["overrides"].as_object().unwrap();
    assert_eq!(
        overrides["x@0@1"],
        serde_json::json!({"port": {"set": 0}, "channel": {"set": {"lamps": 50}}, "direction": "reverse"})
    );
    assert_eq!(
        overrides["x@2"],
        serde_json::json!({"port": {"set": 0}, "channel": {"set": {"lamps": 60}}, "direction": "reverse"})
    );
}

/// A path with jumpers becomes one `d` with a move at each jumper, and the
/// same lamps (positions checked bit for bit by `lands_as_today`).
#[test]
fn a_path_with_jumpers_becomes_one_d_and_keeps_its_lamps() {
    let after = step(&fixture_project(
        r#"{"format":2,"objects":[{"name":"run","shape":{"path":{
            "points":[[0,0],[10,0.5],[12,0],[20,0],[25,3]],"count":17,"gaps":[1,3],"reversed":true}}}]}"#,
        None,
        &[output("o", None, r#"{"0":{}}"#)],
    ));
    let path = &map2d(&after, "f.map2d.json")["parts"][0]["kind"]["path"];
    assert_eq!(path["d"], "M 0 0 L 10 0.5 M 12 0 L 20 0 M 25 3");
    assert_eq!(path["reversed"], true);
}

/// A ring set becomes one circle with one segment for all its rings, and
/// every lamp keeps its position bit for bit.
#[test]
fn a_ring_set_becomes_one_circle_and_keeps_every_lamp() {
    let after = step(&fixture_project(
        r#"{"format":1,"objects":[{"name":"display","shape":{"ring":{"center":[50,50],
            "radius":42.5,"outer_count":24,"rings":4,"counts":[24,0,12],
            "order":"inner_first","start_angle_deg":-33.3,"dir":"ccw"}}}]}"#,
        None,
        &[output("o", None, r#"{"0":{}}"#)],
    ));
    let circle = &map2d(&after, "f.map2d.json")["parts"][0]["kind"]["circle"];
    assert_eq!(circle["split"], "one");
    assert_eq!(circle["drawn_from"], "inside");
    assert_eq!(circle["rings"].as_array().unwrap().len(), 4);
}

/// A ring set whose innermost ring has one lamp keeps that lamp where it is
/// today (at the start angle on its radius, not the centre).
#[test]
fn a_ring_set_with_a_one_lamp_ring_keeps_that_lamp() {
    let after = step(&fixture_project(
        r#"{"format":1,"objects":[{"shape":{"ring":{"center":[0,0],"radius":10,
            "outer_count":6,"rings":6}}}]}"#,
        None,
        &[output("o", None, r#"{"0":{}}"#)],
    ));
    let rings = map2d(&after, "f.map2d.json")["parts"][0]["kind"]["circle"]["rings"].clone();
    let last = rings.as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["count"], 1);
    assert!(last["radius"].as_f64().unwrap() > 0.0);
}

/// A segment a port boundary cuts is refused, naming the project, the
/// fixture, the object and where it is cut, and every file stays as it was.
#[test]
fn a_segment_a_port_boundary_cuts_is_refused_by_name() {
    let before = fixture_project(
        r#"{"format":1,"objects":[{"name":"panel","shape":{"grid":{"origin":[0,0],"cols":8,"rows":8,"pitch":1}}}]}"#,
        None,
        &[output(
            "o",
            None,
            r#"{"0":{"count":16},"1":{"count":16},"2":{"count":16},"3":{}}"#,
        )],
    );
    let mut after = before.clone();
    let error = upgrade_v11_to_v12(&mut after).unwrap_err().to_string();
    assert_eq!(after, before, "a refusal leaves every file as it was");
    for words in [
        "project \"t\"",
        "fixture f.json",
        "`panel`",
        "a port boundary falls inside it",
        "lamps 16, 32 and 48",
        "f.map2d.json",
        "export the project",
    ] {
        assert!(
            error.to_lowercase().contains(&words.to_lowercase()),
            "{words:?} missing from: {error}"
        );
    }
}

/// A patch file is removed, the fixture's patch slot is gone, and the
/// report names the removed file.
#[test]
fn a_patch_file_is_removed_and_its_slot_is_gone() {
    let before = fixture_project(
        &paths(&[("a", 3), ("b", 5)]),
        Some(r#"{"format":2,"entries":[["/b",-1,30]]}"#),
        &[output("o", None, r#"{"0":{}}"#)],
    );
    let mut after = before.clone();
    let report = upgrade_v11_to_v12(&mut after).unwrap();
    assert!(!after.contains("f.patch.json"));
    assert_eq!(report.removed_files, vec!["f.patch.json"]);
    let def: serde_json::Value = serde_json::from_slice(after.get("f.json").unwrap()).unwrap();
    assert!(def.get("patch").is_none(), "{def}");
    files_reported(&before, &after, &report).unwrap();
}

/// A patch today's engine cannot read freezes today's auto-flow, and says
/// so (DD8).
#[test]
fn an_unreadable_patch_freezes_auto_flow_and_says_so() {
    let before = fixture_project(
        &paths(&[("a", 3), ("b", 5)]),
        Some(r#"{"format":7,"entries":[]}"#),
        &[output("o", None, r#"{"0":{}}"#)],
    );
    let mut after = before.clone();
    let report = upgrade_v11_to_v12(&mut after).unwrap();
    lands_as_today(&before, &after);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("cannot read this patch")),
        "{:?}",
        report.warnings
    );
}

/// A `PathPoints` fixture becomes a map2d of `points` parts, its def names
/// it, and every lamp keeps its position bit for bit through the fit.
#[test]
fn hand_placed_lamps_become_a_map2d_of_points() {
    let before = project(&[
        ("project.json", r#"{"format":11,"name":"t"}"#),
        (
            "module.json",
            r#"{"kind":"Module","nodes":{"f":{"ref":"./fixture.json"},"o":{"ref":"./o.json"}}}"#,
        ),
        (
            "fixture.json",
            r#"{"kind":"Fixture","render_size":{"width":16,"height":5},"mapping":{"kind":"PathPoints",
            "paths":{"0":{"kind":"PointList","first_channel":2,"points":{"0":[0.1,0.9],"1":[0.2,0.7]}},
                     "1":{"kind":"PointList","first_channel":0,"points":{"0":[0.0312,0.1],"1":[1.5,0.5]}}},
            "sample_diameter":1.0}}"#,
        ),
        ("o.json", r#"{"kind":"Output","ports":{"0":{}}}"#),
    ]);
    let mut after = before.clone();
    let report = upgrade_v11_to_v12(&mut after).unwrap();
    lands_as_today(&before, &after);
    assert_eq!(report.added_files, vec!["fixture.map2d.json"]);
    let def: serde_json::Value =
        serde_json::from_slice(after.get("fixture.json").unwrap()).unwrap();
    assert_eq!(
        def["mapping"],
        serde_json::json!({"kind": "Map2d", "source": "fixture.map2d.json"})
    );
    let doc = map2d(&after, "fixture.map2d.json");
    let ids: Vec<&str> = doc["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| part["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["points1", "points2"]);
}

/// A map2d already at format 6 passes through byte for byte.
#[test]
fn a_format_6_map2d_passes_through_byte_for_byte() {
    let text = "{\n  \"format\": 6,\n  \"sample_diameter\": 2.0,\n  \"parts\": []\n}\n";
    let before = fixture_project(text, None, &[output("o", None, r#"{"0":{}}"#)]);
    let after = step(&before);
    assert_eq!(after.get("f.map2d.json"), before.get("f.map2d.json"));
    assert_eq!(after.get("f.json"), before.get("f.json"));
}

/// Two fixtures sharing one map2d but patched differently: the later one
/// gets its own copy.
#[test]
fn a_shared_map2d_patched_two_ways_is_copied_for_the_later_fixture() {
    let before = project(&[
        ("project.json", r#"{"format":11,"name":"t"}"#),
        (
            "module.json",
            r#"{"kind":"Module","nodes":{"a":{"ref":"./a.json"},"b":{"ref":"./b.json"},"o":{"ref":"./o.json"},"p":{"ref":"./p.json"}}}"#,
        ),
        (
            "a.json",
            r#"{"kind":"Fixture","bindings":{"output":{"target":"bus:control.a"}},"mapping":{"kind":"Map2d","source":"m.map2d.json"}}"#,
        ),
        (
            "b.json",
            r#"{"kind":"Fixture","bindings":{"output":{"target":"bus:control.b"}},"mapping":{"kind":"Map2d","source":"m.map2d.json"},"patch":{"kind":"File","source":"b.patch.json"}}"#,
        ),
        ("m.map2d.json", &paths(&[("a", 3)])),
        ("b.patch.json", r#"{"format":2,"entries":[["/a",-1,7]]}"#),
        (
            "o.json",
            r#"{"kind":"Output","bindings":{"input":{"source":"bus:control.a"}},"ports":{"0":{}}}"#,
        ),
        (
            "p.json",
            r#"{"kind":"Output","bindings":{"input":{"source":"bus:control.b"}},"ports":{"0":{}}}"#,
        ),
    ]);
    let mut after = before.clone();
    let report = upgrade_v11_to_v12(&mut after).unwrap();
    lands_as_today(&before, &after);
    assert_eq!(report.added_files, vec!["m-2.map2d.json"]);
    let b: serde_json::Value = serde_json::from_slice(after.get("b.json").unwrap()).unwrap();
    assert_eq!(b["mapping"]["source"], "m-2.map2d.json");
}

/// The written map2d reads back through the format-6 reader, and the
/// writer writes it back byte for byte.
#[test]
fn the_written_map2d_reads_back_to_itself() {
    let after = step(&fixture_project(
        &paths(&[("a", 3), ("b", 5)]),
        Some(r#"{"format":3,"flow":"manual","entries":[["/b",-1,9,"r",1]]}"#),
        &[output("o", Some("Main"), r#"{"0":{"count":4},"1":{}}"#)],
    ));
    let text = std::str::from_utf8(after.get("f.map2d.json").unwrap()).unwrap();
    let mapping = map2d6::read(text).unwrap();
    assert_eq!(map2d6::write(&mapping).unwrap(), text);
}

// --- the after side: the patcher on the step's files ----------------------------

/// Where the patcher lands each fixture's lamps in a format-12 project.
struct AfterLanding {
    /// Fixture node → each lamp's places.
    places: BTreeMap<String, Vec<Vec<Place>>>,
    /// The freeze in words, for the gate's report.
    summary: String,
    /// The outputs the step named, `(file, name)`.
    named: Vec<(String, String)>,
}

fn after_landing(files: &ProjectFiles, nodes: &[String]) -> Result<AfterLanding, String> {
    let outputs = outputs_of(files);
    let ports: Vec<OutputPorts> = outputs
        .iter()
        .filter_map(|(_, name, ports)| {
            Some(OutputPorts {
                output: OutputName::new(name.as_deref()?),
                ports: ports
                    .iter()
                    .map(|(key, count)| Port {
                        key: PortKey(*key),
                        channels: count.map(|lamps| lamps * 3),
                    })
                    .collect(),
            })
        })
        .collect();
    let output_file = |name: &OutputName| -> String {
        outputs
            .iter()
            .find(|(_, other, _)| other.as_deref() == Some(name.as_str()))
            .map(|(file, ..)| file.clone())
            .unwrap_or_default()
    };
    let mut places = BTreeMap::new();
    let (mut segments, mut overrides, mut off, mut starts) = (0, 0, 0, 0);
    for node in nodes {
        let Some(source) = map2d_source(files, node)? else {
            places.insert(node.clone(), Vec::new());
            continue;
        };
        let text = text_of(files, &source)?;
        let mapping = map2d6::read(&text).map_err(|e| format!("{source}: {e:?}"))?;
        let rewritten = map2d6::write(&mapping).map_err(|e| format!("{source}: {e:?}"))?;
        if rewritten != text {
            return Err(format!("{source}: not the writer's canonical bytes"));
        }
        let tree = render(&mapping).map_err(|e| format!("{source}: {e:?}"))?;
        let patching = patch_mapping(&mapping, &tree, &ports);
        let mut lamps: Vec<Vec<Place>> = vec![Vec::new(); tree.positions().len()];
        for segment in tree.segments() {
            for j in 0..segment.count() {
                if let Some(address) = patching.landing(&segment.id, j) {
                    lamps[(segment.lamps.start + j) as usize].push((
                        output_file(&address.output),
                        address.port.0,
                        address.channel,
                    ));
                }
            }
        }
        let landed_any = lamps.iter().any(|lamp| !lamp.is_empty());
        for problem in &patching.problems {
            if !(matches!(problem, Problem::NoStart) && !landed_any) {
                return Err(format!("{node}: the patcher reports {problem:?}"));
            }
        }
        segments += tree.segments().len();
        overrides += mapping.overrides.len();
        off += mapping.overrides.values().filter(|o| o.off).count();
        starts += usize::from(!mapping.start.is_keep());
        places.insert(node.clone(), lamps);
    }
    Ok(AfterLanding {
        places,
        summary: format!(
            "{} fixture(s), {segments} segment(s), {starts} start(s), {overrides} \
             override(s), {off} off",
            nodes.len()
        ),
        named: Vec::new(),
    })
    .map(|mut landing| {
        landing.named = outputs
            .into_iter()
            .filter_map(|(file, name, _)| Some((file, name?)))
            .collect();
        landing
    })
}

/// The first lamp the two sides disagree on, in words.
fn compare(
    before: &BTreeMap<String, Vec<Vec<Place>>>,
    after: &BTreeMap<String, Vec<Vec<Place>>>,
) -> Result<(), String> {
    for (node, lamps) in before {
        let other = after
            .get(node)
            .ok_or_else(|| format!("{node}: no fixture after the step"))?;
        if lamps.len() != other.len() {
            return Err(format!(
                "{node}: {} lamps before, {} after",
                lamps.len(),
                other.len()
            ));
        }
        for (lamp, (was, is)) in lamps.iter().zip(other).enumerate() {
            if was != is {
                return Err(format!(
                    "{node} lamp {lamp}: landed at {was:?}, the step's files land it at {is:?}"
                ));
            }
        }
    }
    Ok(())
}

/// Before: the frozen landing of `before`; after: the patcher on `after`;
/// and positions, reports and determinism, for a synthetic project.
fn lands_as_today(before: &ProjectFiles, after: &ProjectFiles) {
    let today = frozen_places(&landing(before).expect("today's landing"));
    let nodes: Vec<String> = today.keys().cloned().collect();
    let lands = after_landing(after, &nodes).unwrap();
    compare(&today, &lands.places).unwrap();
    positions_kept(before, after, &nodes).unwrap();
    deterministic(before, after).unwrap();
}

/// The frozen landing, lamp by lamp.
fn frozen_places(landed: &ProjectLanding) -> BTreeMap<String, Vec<Vec<Place>>> {
    let mut fixtures = BTreeMap::new();
    for fixture in &landed.fixtures {
        let mut places: Vec<Vec<Place>> = vec![Vec::new(); fixture.lamps as usize];
        if let FixtureLayout::Segments(segments) = &fixture.layout {
            for segment in segments {
                if let Landing::Landed {
                    output,
                    port,
                    first_channel,
                    reversed,
                    rotation,
                } = &segment.landing
                {
                    let n = segment.lamps;
                    for j in 0..n {
                        let flipped = if *reversed { n - 1 - j } else { j };
                        places[(segment.first_lamp + j) as usize].push((
                            output.clone(),
                            *port,
                            first_channel + 3 * ((flipped + rotation) % n),
                        ));
                    }
                }
            }
        }
        fixtures.insert(fixture.node.clone(), places);
    }
    fixtures
}

// --- positions, reports, determinism ---------------------------------------------

/// Every lamp's position, before and after, bit for bit: a format 1–5 map2d
/// through today's resolver against the format-6 render; hand-placed lamps
/// as the sampler clamps them against the render through the fit.
fn positions_kept(
    before: &ProjectFiles,
    after: &ProjectFiles,
    nodes: &[String],
) -> Result<(), String> {
    for node in nodes {
        let def = json_of(before, node)?;
        let Some(new_source) = map2d_source(after, node)? else {
            continue;
        };
        let new_text = text_of(after, &new_source)?;
        let mapping = map2d6::read(&new_text).map_err(|e| format!("{new_source}: {e:?}"))?;
        let new_positions = render(&mapping)
            .map_err(|e| format!("{new_source}: {e:?}"))?
            .positions()
            .to_vec();
        match def["mapping"]["kind"].as_str() {
            Some("Map2d") => {
                let old_source = map2d_source(before, node)?.unwrap_or_default();
                let old_text = text_of(before, &old_source)?;
                if map2d6::peek_format(&old_text).unwrap_or(0) >= 6 {
                    if old_text != new_text {
                        return Err(format!("{old_source}: a format-6 map2d was rewritten"));
                    }
                    continue;
                }
                let old = Map2dDoc::from_json(&old_text).map_err(|e| format!("{e:?}"))?;
                let old_positions = resolve(&old).map_err(|e| format!("{e:?}"))?.positions();
                same_positions(node, &old_positions, &new_positions)?;
                if old.sample_diameter.to_bits() != mapping.sample_diameter.to_bits()
                    || old.canvas.map(|c| c.map(f32::to_bits))
                        != mapping.canvas.map(|c| c.map(f32::to_bits))
                {
                    return Err(format!("{node}: the sample diameter or canvas moved"));
                }
            }
            Some("PathPoints") => {
                let (width, height) = (
                    def["render_size"]["width"].as_u64().unwrap_or(16) as u32,
                    def["render_size"]["height"].as_u64().unwrap_or(16) as u32,
                );
                let today = hand_placed(&def);
                let frame = mapping.canvas.and_then(|[min_x, min_y, w, h]| {
                    (w > f32::EPSILON && h > f32::EPSILON).then_some(Bounds2d {
                        min_x,
                        min_y,
                        width: w,
                        height: h,
                    })
                });
                let fitted = fit_points(&new_positions, frame, width, height)
                    .map_err(|e| format!("{e:?}"))?;
                same_positions(node, &today, &fitted)?;
                let scale = fit_scale(&new_positions, frame, width, height)
                    .map_err(|e| format!("{e:?}"))?;
                let old_diameter = def["mapping"]["sample_diameter"].as_f64().unwrap() as f32;
                if mapping.sample_diameter * scale != old_diameter {
                    return Err(format!(
                        "{node}: the sample diameter reaches the sampler as {} where it was \
                         {old_diameter}",
                        mapping.sample_diameter * scale
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn same_positions(node: &str, was: &[[f32; 2]], is: &[[f32; 2]]) -> Result<(), String> {
    if was.len() != is.len() {
        return Err(format!(
            "{node}: {} lamps before, {} after",
            was.len(),
            is.len()
        ));
    }
    for (lamp, (a, b)) in was.iter().zip(is).enumerate() {
        // `==`: the same number (a zero of either sign is zero).
        if a[0] != b[0] || a[1] != b[1] {
            return Err(format!("{node} lamp {lamp}: at {a:?} before, {b:?} after"));
        }
    }
    Ok(())
}

/// Hand-placed lamps by channel, as the sampler clamps them.
fn hand_placed(def: &serde_json::Value) -> Vec<[f32; 2]> {
    let mut lamps: BTreeMap<u64, [f32; 2]> = BTreeMap::new();
    for path in def["mapping"]["paths"].as_object().unwrap().values() {
        let first = path["first_channel"].as_u64().unwrap();
        for (key, xy) in path["points"].as_object().unwrap() {
            let at = |i: usize| (xy[i].as_f64().unwrap() as f32).clamp(0.0, 1.0);
            lamps.insert(first + key.parse::<u64>().unwrap(), [at(0), at(1)]);
        }
    }
    lamps.into_values().collect()
}

/// The report names exactly the files whose bytes changed, were added and
/// were removed.
fn files_reported(
    before: &ProjectFiles,
    after: &ProjectFiles,
    report: &lpa_upgrade::UpgradeReport,
) -> Result<(), String> {
    let was: BTreeSet<&str> = before.paths().collect();
    let is: BTreeSet<&str> = after.paths().collect();
    let changed: Vec<String> = was
        .intersection(&is)
        .filter(|path| before.get(path) != after.get(path))
        .map(|path| path.to_string())
        .collect();
    let added: Vec<String> = is.difference(&was).map(|p| p.to_string()).collect();
    let removed: Vec<String> = was.difference(&is).map(|p| p.to_string()).collect();
    if report.changed_files != changed
        || report.added_files != added
        || report.removed_files != removed
    {
        return Err(format!(
            "the report says changed {:?} added {:?} removed {:?}; the files say {changed:?} \
             {added:?} {removed:?}",
            report.changed_files, report.added_files, report.removed_files
        ));
    }
    Ok(())
}

/// The same input gives the same bytes.
fn deterministic(before: &ProjectFiles, after: &ProjectFiles) -> Result<(), String> {
    let mut again = before.clone();
    upgrade_v11_to_v12(&mut again).map_err(|e| e.to_string())?;
    if &again != after {
        return Err(String::from("a second run wrote different bytes"));
    }
    Ok(())
}

// --- reading the step's files ----------------------------------------------------

/// Every output: its file, its name, its ports `(key, count)`.
type OutputDef = (String, Option<String>, Vec<(u32, Option<u32>)>);

fn outputs_of(files: &ProjectFiles) -> Vec<OutputDef> {
    let mut out = Vec::new();
    for (path, bytes) in files.iter() {
        if !path.ends_with(".json") {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            continue;
        };
        if value["kind"] != "Output" {
            continue;
        }
        let mut ports: Vec<(u32, Option<u32>)> = value["ports"]
            .as_object()
            .map(|ports| {
                ports
                    .iter()
                    .map(|(key, port)| {
                        (
                            key.parse().unwrap(),
                            port.get("count").and_then(|c| c.as_f64()).map(|c| c as u32),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        ports.sort_by_key(|(key, _)| *key);
        out.push((
            format!("/{}", path.trim_start_matches('/')),
            value["name"].as_str().map(String::from),
            ports,
        ));
    }
    out
}

/// The map2d a fixture's def names, from the project root.
fn map2d_source(files: &ProjectFiles, node: &str) -> Result<Option<String>, String> {
    let def = json_of(files, node)?;
    if def["mapping"]["kind"] != "Map2d" {
        return Ok(None);
    }
    let source = def["mapping"]["source"].as_str().unwrap_or_default();
    Ok(Some(resolve_from(node, source)))
}

fn resolve_from(from: &str, relative: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    if !relative.starts_with('/') {
        segments.extend(from.split('/').filter(|s| !s.is_empty()));
        segments.pop();
    }
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    segments.join("/")
}

fn text_of(files: &ProjectFiles, path: &str) -> Result<String, String> {
    let bare = path.trim_start_matches('/');
    let bytes = files
        .get(bare)
        .or_else(|| files.get(&format!("/{bare}")))
        .ok_or_else(|| format!("{bare} is not in the project"))?;
    String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())
}

fn json_of(files: &ProjectFiles, path: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(&text_of(files, path)?).map_err(|e| format!("{path}: {e}"))
}

fn map2d(files: &ProjectFiles, path: &str) -> serde_json::Value {
    json_of(files, path).unwrap()
}

// --- building small projects -------------------------------------------------------

/// Run the step and check it lands as today.
fn step(before: &ProjectFiles) -> ProjectFiles {
    let mut after = before.clone();
    upgrade_v11_to_v12(&mut after).unwrap();
    lands_as_today(before, &after);
    after
}

/// One fixture `f` (its map2d `f.map2d.json`, its patch `f.patch.json`
/// when given) and the outputs, all on `control.out`.
fn fixture_project(map2d: &str, patch: Option<&str>, outputs: &[(String, String)]) -> ProjectFiles {
    let mut nodes = vec![String::from(r#""f":{"ref":"./f.json"}"#)];
    for (index, (file, _)) in outputs.iter().enumerate() {
        nodes.push(format!(r#""o{index}":{{"ref":"./{file}"}}"#));
    }
    let module = format!(r#"{{"kind":"Module","nodes":{{{}}}}}"#, nodes.join(","));
    let fixture = match patch {
        Some(_) => {
            r#"{"kind":"Fixture","mapping":{"kind":"Map2d","source":"f.map2d.json"},"patch":{"kind":"File","source":"f.patch.json"}}"#
        }
        None => r#"{"kind":"Fixture","mapping":{"kind":"Map2d","source":"f.map2d.json"}}"#,
    };
    let mut files = vec![
        (
            "project.json".to_string(),
            r#"{"format":11,"name":"t"}"#.to_string(),
        ),
        ("module.json".to_string(), module),
        ("f.json".to_string(), fixture.to_string()),
        ("f.map2d.json".to_string(), map2d.to_string()),
    ];
    if let Some(patch) = patch {
        files.push(("f.patch.json".to_string(), patch.to_string()));
    }
    files.extend(outputs.iter().cloned());
    files
        .into_iter()
        .map(|(path, text)| (path, text.into_bytes()))
        .collect()
}

/// An output file `<key>.json`, named or not, with these ports.
fn output(key: &str, name: Option<&str>, ports: &str) -> (String, String) {
    let name = name
        .map(|n| format!(r#""name":"{n}","#))
        .unwrap_or_default();
    (
        format!("{key}.json"),
        format!(r#"{{"kind":"Output",{name}"ports":{ports}}}"#),
    )
}

/// A format-3 map2d of straight paths, ids as given, in order.
fn paths(objects: &[(&str, u32)]) -> String {
    let objects: Vec<String> = objects
        .iter()
        .enumerate()
        .map(|(row, (id, count))| {
            format!(
                r#"{{"id":"{id}","shape":{{"path":{{"points":[[0,{row}],[10,{row}]],"count":{count}}}}}}}"#
            )
        })
        .collect();
    format!(r#"{{"format":3,"objects":[{}]}}"#, objects.join(","))
}

fn project(files: &[(&str, &str)]) -> ProjectFiles {
    files
        .iter()
        .map(|(path, text)| (path.to_string(), text.as_bytes().to_vec()))
        .collect()
}

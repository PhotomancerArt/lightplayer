//! Opening the three test projects and saving them back, without the
//! product: what each opens as, and that saving an unedited project writes
//! nothing. (`lab-mapping-fit-tests` holds the proofs against the product.)

use lab_mapping_files::embedded::{CHOKER, Embedded, RADIANCE_DOME, ZOOK_DOME};
use lab_mapping_files::{MemFiles, Opened};
use lab_mapping_model::{ColorOrder, ComponentKind, NodeId, NodeKind, PatchRule, Project, Target};

/// The choker opens with all five nodes, 12 paths and 73 lamps, its one
/// port taking the rest, and its ids minted from its object names.
#[test]
fn the_choker_opens() {
    let o = open(&CHOKER);
    let p = &o.project;
    assert_eq!(p.name, "PLAYFUL Choker");
    let names: Vec<&str> = p.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["clock", "fixture", "output", "power", "shader"]);
    let fixture = NodeId::new("fixture");
    let f = p.fixture(&fixture).unwrap();
    assert_eq!(f.components.len(), 12);
    assert_eq!(f.components[0].id.as_str(), "p");
    assert_eq!(f.components[3].id.as_str(), "a-bar");
    assert_eq!(f.color_order, ColorOrder::Rgb);
    assert_eq!(p.lamp_count(), 73);
    let out = NodeId::new("output");
    assert_eq!(p.output(&out).unwrap().ports[0].pixels, None);
    assert_eq!(
        f.rules,
        vec![PatchRule::Auto {
            output: out.clone()
        }]
    );
    assert_eq!(p.port_fill(&out, "0"), (73, 73));
    assert!(matches!(
        p.node(&NodeId::new("shader")).unwrap().kind,
        NodeKind::Other(_)
    ));
}

/// The zook dome opens as one repeat of a zigzag path with a jumper: five
/// copies of 300 lamps, copy k on port k.
#[test]
fn the_zook_dome_opens() {
    let o = open(&ZOOK_DOME);
    let p = &o.project;
    let f = p.fixture(&NodeId::new("fixture")).unwrap();
    let ComponentKind::Repeat { shape, count, .. } = &f.components[0].kind else {
        panic!("a repeat");
    };
    assert_eq!(*count, 5);
    assert!(matches!(**shape, ComponentKind::Path { lamps: 300, ref gaps, .. } if gaps == &[6]));
    assert_eq!(p.lamp_count(), 1500);
    let out = NodeId::new("output");
    for k in 0..5 {
        assert_eq!(p.port_fill(&out, &k.to_string()), (300, 300), "port {k}");
    }
    let sector = Target::Component(f.components[0].id.clone());
    assert_eq!(p.children(Some(&sector)).len(), 5);
}

/// The radiance dome opens: 55 repeats of 5 copies, 22,000 lamps, ten
/// named outputs of 32 ports, every panel placed by hand (no auto rule),
/// its two quirks kept.
#[test]
fn the_radiance_dome_opens() {
    let o = open(&RADIANCE_DOME);
    let p = &o.project;
    let dome = NodeId::new("dome");
    let f = p.fixture(&dome).unwrap();
    assert_eq!(f.components.len(), 55);
    assert_eq!(p.lamp_count(), 22_000);
    assert_eq!(p.outputs().len(), 10);
    assert!(
        p.outputs()
            .iter()
            .all(|n| n.output().unwrap().ports.len() == 32)
    );
    assert_eq!(f.rules.len(), 275);
    assert!(f.rules.iter().all(|r| matches!(r, PatchRule::Place { .. })));
    assert_eq!(
        f.rules
            .iter()
            .filter(|r| matches!(r, PatchRule::Place { reversed: true, .. }))
            .count(),
        1
    );
    assert_eq!(
        f.rules
            .iter()
            .filter(|r| matches!(r, PatchRule::Place { rotation, .. } if *rotation != 0))
            .count(),
        1
    );
    let patch = p.effective_patch();
    assert!(patch.unpatched.is_empty() && patch.problems.is_empty());
    assert_eq!(
        p.output(&NodeId::new("out_3")).unwrap().label.as_deref(),
        Some("3")
    );
}

/// Saving a project nobody edited writes nothing: every file stays byte
/// for byte as it was.
#[test]
fn saving_an_unedited_project_writes_nothing() {
    for e in [&CHOKER, &ZOOK_DOME, &RADIANCE_DOME] {
        let o = open(e);
        let saved = o.files.save(&o.project).unwrap();
        assert_eq!(saved.writes, vec![], "{}", e.key);
        assert!(saved.removed.is_empty(), "{}", e.key);
    }
}

/// Saving everything and opening what was written gives the same project
/// back.
#[test]
fn what_saving_writes_opens_as_the_same_project() {
    for e in [&CHOKER, &ZOOK_DOME, &RADIANCE_DOME] {
        let o = open(e);
        let again = reopen(e, &o.files.save_all(&o.project).unwrap().writes);
        assert_eq!(again.project, o.project, "{}", e.key);
    }
}

/// An edit writes only the files it touches: a new count on the zook dome
/// rewrites its mapping, and nothing else.
#[test]
fn an_edit_writes_only_what_it_touches() {
    let o = open(&ZOOK_DOME);
    let mut p = o.project.clone();
    let sector = p.fixture(&NodeId::new("fixture")).unwrap().components[0]
        .id
        .clone();
    assert!(p.edit_prop(
        &Target::Component(sector),
        "lamps",
        lab_mapping_model::PropEdit::SetCount(299),
        2.4
    ));
    let saved = o.files.save(&p).unwrap();
    let paths: Vec<&str> = saved.writes.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["fixture.map2d.json"]);
    assert_eq!(reopen(&ZOOK_DOME, &saved.writes).project.lamp_count(), 1495);
}

/// Deleting the choker's shader removes it from the module and reports its
/// file as no longer used.
#[test]
fn deleting_a_node_drops_it_from_its_module() {
    let o = open(&CHOKER);
    let mut p = o.project.clone();
    p.remove_node(&NodeId::new("shader"));
    let saved = o.files.save(&p).unwrap();
    let paths: Vec<&str> = saved.writes.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["module.json"]);
    assert_eq!(saved.removed, ["shader.json"]);
}

/// A new place rule writes a patch file for the fixture, naming the
/// object by a stable id it now writes into the mapping.
#[test]
fn a_new_place_rule_writes_a_patch() {
    let o = open(&CHOKER);
    let mut p = o.project.clone();
    let out = NodeId::new("output");
    let fixture = NodeId::new("fixture");
    let a = p.fixture(&fixture).unwrap().components[2].id.clone();
    p.fixture_mut(&fixture).unwrap().rules.insert(
        0,
        PatchRule::Place {
            object: lab_mapping_model::ObjectId::new(&a, "path"),
            output: out.clone(),
            port: "0".into(),
            pixel: 100,
            reversed: true,
            rotation: 0,
        },
    );
    let saved = o.files.save(&p).unwrap();
    let paths: Vec<&str> = saved.writes.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        ["fixture.json", "fixture.patch.json", "playful.map2d.json"]
    );
    let again = reopen(&CHOKER, &saved.writes);
    assert_eq!(
        again.project.fixture(&fixture).unwrap().rules,
        p.fixture(&fixture).unwrap().rules
    );
}

/// The desk (the lab's own seed) saves as a new product project, and opens
/// again with the same lamps and the same wire.
#[test]
fn the_desk_saves_as_a_new_project() {
    let o = open(&CHOKER);
    let empty = lab_mapping_files::ProductProject {
        nodes: Default::default(),
        root: serde_json::json!({"kind": "Module", "nodes": {}}),
        ..o.files.clone()
    };
    let desk = Project::seed();
    let saved = empty.save(&desk).unwrap();
    let mut files = MemFiles::new(saved.writes.clone());
    files.files.insert(
        "project.json".into(),
        r#"{"format": 11, "name": "desk"}"#.into(),
    );
    let again = Opened::open(&files).unwrap().project;
    assert_eq!(again.lamp_count(), desk.lamp_count());
    // Every pixel carries the same lamp: (output, port, pixel) → (fixture,
    // where the lamp is in its fixture). Names as the product spells them.
    let wire = |p: &Project| {
        let mut w: Vec<(String, String, u32, String, i64, i64)> = Vec::new();
        for s in p.effective_patch().spans {
            let origin = p.origin(&s.fixture);
            let lamps = p.object(&s.object).unwrap().lamps;
            for lamp in s.lamps.clone() {
                let at = lamps[lamp as usize].pos - origin;
                w.push((
                    p.node_name(&s.output).replace(' ', "_"),
                    s.port.clone(),
                    s.pixel(lamp),
                    p.node_name(&s.fixture).replace(' ', "_"),
                    (at.x * 1000.0).round() as i64,
                    (at.y * 1000.0).round() as i64,
                ));
            }
        }
        w.sort();
        w
    };
    assert_eq!(wire(&again), wire(&desk));
}

fn open(e: &Embedded) -> Opened {
    Opened::open(&e.files()).unwrap_or_else(|err| panic!("{}: {err}", e.key))
}

/// The project as it would be with `writes` written over its files.
fn reopen(e: &Embedded, writes: &[(String, String)]) -> Opened {
    let mut files = e.files();
    for (path, text) in writes {
        files.files.insert(path.clone(), text.clone());
    }
    Opened::open(&files).unwrap_or_else(|err| panic!("{}: {err}", e.key))
}

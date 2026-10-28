//! Opening the three test projects and saving them back, without the
//! product: what each opens as, and that saving an unedited project writes
//! nothing. They open the format-11 copies in `lab/mapping/fixtures-v11/`.

use lab_mapping_files::embedded::{CHOKER, Embedded, RADIANCE_DOME, ZOOK_DOME};
use lab_mapping_files::{DirFiles, MemFiles, Opened};
use lab_mapping_model::{
    Channel, ColorOrder, ComponentKind, Direction, NodeId, NodeKind, PatchPlace, PatchTarget,
    Project, Target, Wire,
};

/// The choker opens with all five nodes, 12 paths and 73 lamps, its one
/// port taking the rest, and its ids minted from its object names; its
/// shader opens as a render source on `visual.out`, keeping its GLSL, and
/// its fixture reads that channel.
#[test]
fn the_choker_opens() {
    let o = open(&CHOKER);
    let p = &o.project;
    assert_eq!(p.name, "PLAYFUL Choker");
    let names: Vec<&str> = p.nodes().iter().map(|n| n.name.as_str()).collect();
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
        f.places,
        vec![PatchPlace {
            wire: Some(Wire::Auto {
                output: out.clone()
            }),
            ..PatchPlace::on(PatchTarget::Fixture)
        }]
    );
    assert_eq!(p.port_fill(&out, "0"), (73, 73));
    let shader = p.node(&NodeId::new("shader")).unwrap();
    let NodeKind::Source(src) = &shader.kind else {
        panic!("a render source");
    };
    assert_eq!(src.kind, "Shader");
    assert_eq!(src.writes, Channel::visual_out());
    assert!(src.glsl.as_deref().unwrap().contains("render_2d"));
    assert_eq!(f.reads, Channel::visual_out());
    assert_eq!(
        p.render_product(&fixture).unwrap().source,
        NodeId::new("shader")
    );
    assert!(matches!(
        p.node(&NodeId::new("clock")).unwrap().kind,
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
    assert_eq!(f.places.len(), 275);
    assert!(f.places.iter().all(|r| matches!(
        (&r.on, &r.wire),
        (PatchTarget::Object(_), Some(Wire::At { .. }))
    )));
    assert_eq!(
        f.places
            .iter()
            .filter(|r| r.direction == Some(Direction::Reversed))
            .count(),
        1
    );
    assert_eq!(
        f.places
            .iter()
            .filter(|r| r.turn.is_some_and(|t| t != 0))
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

/// A mapping that is written gives every object an id, stamped at the
/// lowest format that holds ids (3), and reopens with the same component
/// ids it was saved with — so a second save compares like with like.
#[test]
fn a_written_mapping_gives_every_object_an_id() {
    let o = open(&CHOKER);
    let mut p = o.project.clone();
    let first = p.fixture(&NodeId::new("fixture")).unwrap().components[0]
        .id
        .clone();
    assert!(p.edit_prop(
        &Target::Component(first),
        "lamps",
        lab_mapping_model::PropEdit::Nudge(1),
        2.4
    ));
    let saved = o.files.save(&p).unwrap();
    let (_, map) = saved
        .writes
        .iter()
        .find(|(path, _)| path.ends_with(".map2d.json"))
        .expect("the mapping is written");
    let doc: serde_json::Value = serde_json::from_str(map).unwrap();
    assert_eq!(doc["format"], 3);
    let objects = doc["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 12);
    assert!(objects.iter().all(|o| o.get("id").is_some()));
    let again = reopen(&CHOKER, &saved.writes);
    let ids = |p: &Project| -> Vec<String> {
        p.fixture(&NodeId::new("fixture"))
            .unwrap()
            .components
            .iter()
            .map(|c| c.id.to_string())
            .collect()
    };
    assert_eq!(ids(&again.project), ids(&p));
    assert!(
        again.files.save(&p).unwrap().writes.is_empty(),
        "nothing left to write"
    );
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
    p.fixture_mut(&fixture).unwrap().places.insert(
        0,
        PatchPlace {
            on: PatchTarget::Object(lab_mapping_model::ObjectId::new(&a, "path")),
            wire: Some(Wire::At {
                output: out.clone(),
                port: "0".into(),
                pixel: 100,
            }),
            direction: Some(Direction::Reversed),
            turn: None,
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
        again.project.fixture(&fixture).unwrap().places,
        p.fixture(&fixture).unwrap().places
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
        for s in &p.effective_patch().pixel_runs {
            let origin = p.origin(&s.fixture);
            let lamps = &p.object(&s.object).unwrap().lamps;
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

    // A circle placed whole, reversed, with one ring turned back the way it
    // was drawn: today's rows can't say that (plan D14), so it saves as each
    // ring at its address, and opens landing exactly as it did.
    let mut placed = Project::seed();
    let ring_display = placed.named("ring display").unwrap().id.clone();
    let main = placed.named("main").unwrap().id.clone();
    let circle = lab_mapping_model::ComponentId::new("circle1");
    let target = PatchTarget::Component(circle.clone());
    placed.set_wire(
        &ring_display,
        &target,
        Some(Wire::At {
            output: main.clone(),
            port: "0".into(),
            pixel: 5,
        }),
    );
    placed.set_direction(&ring_display, &target, Some(Direction::Reversed));
    placed.set_direction(
        &ring_display,
        &PatchTarget::Object(lab_mapping_model::ObjectId::new(&circle, "r2")),
        Some(Direction::AsDrawn),
    );
    let saved = empty.save(&placed).unwrap();
    let mut files = MemFiles::new(saved.writes.clone());
    files.files.insert(
        "project.json".into(),
        r#"{"format": 11, "name": "desk"}"#.into(),
    );
    let again = Opened::open(&files).unwrap().project;
    assert_eq!(wire(&again), wire(&placed));
}

/// A grid and a ring of several rings open whole (plan M4): the plasma
/// pattern's fixture is a one-lamp grid and an eight-ring disc on one
/// strand, every lamp where the product puts it. An unedited save writes
/// nothing; an edit to the grid rewrites only its numbers, and the disc's
/// object is written back as it was.
#[test]
fn a_grid_and_a_ring_set_open_whole() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures-v11/plasma");
    let o = Opened::open(&DirFiles::new(root)).unwrap();
    let fixture = o.project.fixture(&NodeId::new("fixture")).unwrap();
    assert_eq!(fixture.unsupported, None);
    let kinds: Vec<&str> = fixture.components.iter().map(|c| c.kind_word()).collect();
    assert_eq!(kinds, ["grid", "rings"]);
    // 1 + (60 + 48 + 40 + 32 + 24 + 16 + 12 + 8).
    assert_eq!(o.project.lamp_count(), 241);
    let saved = o.files.save(&o.project).unwrap();
    assert!(saved.writes.is_empty(), "{:?}", saved.writes);

    let mut p = o.project.clone();
    let grid = fixture.components[0].id.clone();
    assert!(p.edit_prop(
        &Target::Component(grid),
        "columns",
        lab_mapping_model::PropEdit::SetCount(3),
        2.0
    ));
    assert_eq!(p.lamp_count(), 243);
    let saved = o.files.save(&p).unwrap();
    let paths: Vec<&str> = saved.writes.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["fixture.map2d.json"]);
    let written: serde_json::Value = serde_json::from_str(&saved.writes[0].1).unwrap();
    let original: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{root}/fixture.map2d.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(written["objects"][0]["shape"]["grid"]["cols"], 3);
    assert_eq!(
        written["objects"][1]["shape"], original["objects"][1]["shape"],
        "the disc is written back as it was"
    );
}

/// A project in the new mapping format (project format 12, map2d format 6)
/// is refused by name: the lab says it doesn't read the new mapping format,
/// rather than naming a version only.
#[test]
fn a_project_in_the_new_mapping_format_is_refused_by_name() {
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../catalog/projects/playful-choker"
    );
    let Err(why) = Opened::open(&DirFiles::new(root)) else {
        panic!("the lab opened a format-12 project");
    };
    let why = why.to_string();
    assert!(
        why.contains("map2d format 6, the new mapping format, which the lab doesn't read"),
        "{why}"
    );
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

//! Opening real projects without the product: the repo's own projects at
//! project format 12, each fixture's mapping read by `lpc-mapping`'s
//! map2d format 6 reader into its parts and patching, and what each opens
//! as. A project at another format, or a mapping in an older map2d format,
//! is refused by name. Saving is `save.rs`'s.

mod common;

use common::{format6_doc, project_around};
use lab_mapping_files::embedded::{CHOKER, Embedded, PLASMA, RADIANCE_DOME, ZOOK_DOME};
use lab_mapping_files::{FilesError, MemFiles, Opened};
use lab_mapping_model::{Channel, ColorOrder, NodeId, NodeKind, Target, Vec2};
use lpc_mapping::map2d6;
use lpc_mapping::part::{PartKind, PathCommand, Shape, TreeId};
use lpc_mapping::patching::{Amount, Direction, Start};

/// The choker opens with all five nodes, 12 paths and 73 lamps, its one
/// port taking the rest, and its part ids as its mapping writes them; it
/// starts at its output's port 0, lamp 0 with nothing overridden (the
/// letters run end to end from there); its shader opens as a render source
/// on `visual.out`, keeping its GLSL, and its fixture reads that channel.
#[test]
fn the_choker_opens() {
    let o = open(&CHOKER);
    let p = &o.project;
    assert_eq!(p.name, "PLAYFUL Choker");
    let names: Vec<&str> = p.nodes().iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["clock", "fixture", "output", "power", "shader"]);
    let fixture = NodeId::new("fixture");
    let f = p.fixture(&fixture).unwrap();
    assert_eq!(f.parts().len(), 12);
    assert_eq!(f.parts()[0].id.as_str(), "p");
    assert_eq!(f.parts()[3].id.as_str(), "a-bar");
    assert_eq!(f.color_order, ColorOrder::Rgb);
    assert_eq!(p.lamp_count(), 73);
    let out = NodeId::new("output");
    assert_eq!(p.output(&out).unwrap().ports[0].pixels, None);
    assert_eq!(p.output(&out).unwrap().label.as_deref(), Some("1"));
    assert_eq!(f.mapping.start, Start::at("1", 0, Amount::lamps(0)));
    assert!(f.mapping.overrides.is_empty());
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
/// copies of 300 lamps, copy k on port k. The repeat (`repeat1`) holds the
/// path as a part of its own, `sector`: copy 0 is `sector`, copy 2 is
/// `sector@2`.
#[test]
fn the_zook_dome_opens() {
    let o = open(&ZOOK_DOME);
    let p = &o.project;
    let fixture = NodeId::new("fixture");
    let f = p.fixture(&fixture).unwrap();
    let PartKind::Repeat(repeat) = &f.parts()[0].kind else {
        panic!("a repeat");
    };
    assert_eq!(repeat.count, 5);
    assert_eq!(f.parts()[0].id.as_str(), "repeat1");
    assert_eq!(repeat.part.id.as_str(), "sector");
    let PartKind::Shape(Shape::Path(path)) = &repeat.part.kind else {
        panic!("a path");
    };
    assert_eq!(path.count, 300);
    let moves: Vec<usize> = path
        .commands
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c, PathCommand::Move(_)))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(moves, [0, 7], "one jumper, after the seventh point");
    assert_eq!(p.lamp_count(), 1500);
    let out = NodeId::new("output");
    for k in 0..5 {
        assert_eq!(p.port_fill(&out, &k.to_string()), (300, 300), "port {k}");
    }
    let copy2 = TreeId::parse("sector@2").unwrap();
    assert!(p.segments_in(&fixture).iter().any(|s| s.id.id == copy2));
    let repeat = Target::Part(lab_mapping_model::PartRef::new(&fixture, &f.parts()[0].id));
    assert_eq!(p.children(Some(&repeat)).len(), 5);
}

/// The radiance dome opens: 55 repeats of 5 copies, 22,000 lamps, ten
/// named outputs of 32 ports, every panel placed by hand — a start and the
/// overrides the patcher needs to land every panel where the product does,
/// its two quirks kept (one panel reversed, one entered off its first
/// lamp) — with nothing unpatched and no problem.
#[test]
fn the_radiance_dome_opens() {
    let o = open(&RADIANCE_DOME);
    let p = &o.project;
    let dome = NodeId::new("dome");
    let f = p.fixture(&dome).unwrap();
    assert_eq!(f.parts().len(), 55);
    assert_eq!(p.lamp_count(), 22_000);
    assert_eq!(p.outputs().len(), 10);
    assert!(
        p.outputs()
            .iter()
            .all(|n| n.output().unwrap().ports.len() == 32)
    );
    assert!(!f.mapping.start.is_keep());
    assert!(f.mapping.overrides.len() <= 275);
    assert_eq!(
        f.mapping
            .overrides
            .values()
            .filter(|o| o.direction == Some(Direction::Reverse))
            .count(),
        1
    );
    assert_eq!(
        f.mapping
            .overrides
            .values()
            .filter(|o| o.offset.is_some_and(|t| t != 0))
            .count(),
        1
    );
    let patch = p.effective_patch();
    assert!(patch.unpatched.is_empty(), "{:?}", patch.unpatched);
    assert!(patch.problems.is_empty(), "{:?}", patch.problems);
    assert_eq!(
        p.output(&NodeId::new("out_3")).unwrap().label.as_deref(),
        Some("3")
    );
}

/// A grid and a ring set open whole: the plasma pattern's fixture is a
/// one-lamp grid and an eight-ring disc on one strand, every lamp where the
/// product puts it.
#[test]
fn a_grid_and_a_ring_set_open_whole() {
    let o = open(&PLASMA);
    let fixture = o.project.fixture(&NodeId::new("fixture")).unwrap();
    assert_eq!(fixture.unsupported, None);
    let kinds: Vec<&str> = fixture
        .parts()
        .iter()
        .map(|p| lab_mapping_model::domain::target::part_word(&p.kind))
        .collect();
    assert_eq!(kinds, ["grid", "circle"]);
    // 1 + (60 + 48 + 40 + 32 + 24 + 16 + 12 + 8).
    assert_eq!(o.project.lamp_count(), 241);
    assert!(o.project.effective_patch().unpatched.is_empty());
}

/// A format-12 project opens with its fixture's mapping as `lpc-mapping`
/// reads it — its parts, start, flows and overrides, exactly what
/// `map2d6::read` gives — and its box is the document's canvas.
#[test]
fn a_format_12_project_opens_with_its_fixtures_as_lpc_mapping_parts() {
    let doc = format6_doc("circular_display");
    let ports = [
        ("ws281x:local:D0", Some(64)),
        ("ws281x:local:D1", Some(64)),
        ("ws281x:local:D2", Some(64)),
        ("ws281x:local:D3", Some(64)),
        ("ws281x:local:D4", Some(64)),
    ];
    let o = Opened::open(&project_around(&doc, "main", &ports)).unwrap();
    let f = o.project.fixture(&NodeId::new("fixture")).unwrap();
    assert_eq!(f.mapping, map2d6::read(&doc).unwrap());
    assert!(matches!(
        f.parts()[0].kind,
        PartKind::Shape(Shape::Circle(_))
    ));
    assert_eq!(f.mapping.overrides.len(), 1);
    assert_eq!(f.size, Vec2::new(100.0, 100.0));
    assert_eq!(o.project.lamp_count(), 61);
    let map = o.files.nodes[&NodeId::new("fixture")].map.as_ref().unwrap();
    assert_eq!(map.path, "fixture.map2d.json");
    assert_eq!(map.text, doc);
}

/// A project at an older project format is refused by name: it says the
/// format it is at, and the one the Graph reads.
#[test]
fn a_project_at_an_older_format_is_refused_by_name() {
    let why = refused(&with_manifest(r#"{"format": 11, "name": "old"}"#));
    assert_eq!(why.path, "project.json");
    assert!(
        why.what
            .contains("project format 11, from an older LightPlayer")
            && why.what.contains("reads project format 12"),
        "{why}"
    );
}

/// A project at a newer project format is refused by name.
#[test]
fn a_project_at_a_newer_format_is_refused_by_name() {
    let why = refused(&with_manifest(r#"{"format": 13, "name": "new"}"#));
    assert_eq!(why.path, "project.json");
    assert!(
        why.what
            .contains("project format 13, from a newer LightPlayer"),
        "{why}"
    );
}

/// A project that says no format is refused, saying so.
#[test]
fn a_project_with_no_format_is_refused_by_name() {
    let why = refused(&with_manifest(r#"{"name": "none"}"#));
    assert!(why.what.contains("says no format"), "{why}");
}

/// A mapping in an older map2d format inside a format-12 project is
/// refused by name: the mapping's file, its format and the one the Graph
/// reads.
#[test]
fn a_mapping_at_an_older_map2d_format_is_refused_by_name() {
    for older in 1..=5 {
        let map = format!(
            r#"{{"format": {older}, "sample_diameter": 2.0, "objects": [{{"name": "p", "shape": {{"path": {{"points": [[0, 0], [10, 0]], "count": 3}}}}}}]}}"#
        );
        let why = refused(&project_around(&map, "out", &[("ws281x:local:D10", None)]));
        assert_eq!(why.path, "fixture.map2d.json");
        assert!(
            why.what
                .contains(&format!("map2d format {older}, from an older LightPlayer"))
                && why.what.contains("reads map2d format 6"),
            "{why}"
        );
    }
}

/// A mapping in a newer map2d format is refused by name too, as from a
/// newer LightPlayer.
#[test]
fn a_mapping_at_a_newer_map2d_format_is_refused_by_name() {
    let map = format6_doc("eaves").replacen("\"format\": 6", "\"format\": 7", 1);
    let why = refused(&project_around(&map, "out", &[("ws281x:local:D10", None)]));
    assert_eq!(why.path, "fixture.map2d.json");
    assert!(
        why.what
            .contains("map2d format 7, from a newer LightPlayer"),
        "{why}"
    );
}

// ---- helpers --------------------------------------------------------------

fn open(e: &Embedded) -> Opened {
    Opened::open(&e.files()).unwrap_or_else(|err| panic!("{}: {err}", e.key))
}

/// A project around the eaves document, with this `project.json`.
fn with_manifest(manifest: &str) -> MemFiles {
    let mut files = project_around(&format6_doc("eaves"), "out", &[("ws281x:local:D10", None)]);
    files
        .files
        .insert("project.json".into(), manifest.to_string());
    files
}

/// Why these files don't open; they must not.
fn refused(files: &MemFiles) -> FilesError {
    match Opened::open(files) {
        Ok(_) => panic!("it opened"),
        Err(why) => why,
    }
}

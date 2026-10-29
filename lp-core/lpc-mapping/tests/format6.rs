//! Statements: map2d format 6, the one file that holds a fixture's layout
//! and patching (G1, 2026-10-27). One sentence per test; the doc comment is
//! the statement.
//!
//! The four journeys' documents live in `tests/format6/`: the choker (the
//! catalog's own paths), the circular display as soldered, a dome slice
//! (zook's own sector) one per port, and Sean's eaves as wired. Each reads,
//! patches as its journey ends, and writes back byte for byte.

mod support;

use std::path::PathBuf;

use lpc_mapping::map2d6::{self, Map2d6Error};
use lpc_mapping::part::{
    Circle, CircleSplit, Colors, Mapping, Part, PartId, PartKind, Path, PathCommand, PathSplit,
    Points, Repeat, Shape, TreeId, render,
};
use lpc_mapping::patching::{
    Amount, ChannelOp, Direction, Flow, OutputName, OutputOp, OutputPorts, Override, Patching,
    PortKey, PortOp, Problem, Start, patch_mapping,
};
use lpc_mapping::{
    GridCorner, GridRouting, GridShape, Map2dDoc, Map2dError, PathAlign, PolygonShape, resolve,
};
use support::{bits, today_as_mapping};

// ---- reading and writing ----------------------------------------------------

/// A format-6 document reads back to the mapping it was written from, byte
/// for byte: every journey's document, and a mapping with every construct.
#[test]
fn a_document_reads_back_to_the_mapping_it_was_written_from() {
    for name in JOURNEYS {
        let text = example(name);
        let m = map2d6::read(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(map2d6::write(&m).unwrap(), text, "{name}: written back");
    }

    let m = every_construct();
    let text = map2d6::write(&m).unwrap();
    assert_eq!(map2d6::read(&text).unwrap(), m, "the mapping back");
    assert_eq!(map2d6::write(&map2d6::read(&text).unwrap()).unwrap(), text);
}

/// A document newer than this build reads is refused, naming the format it
/// reads up to; so is format 0.
#[test]
fn a_newer_document_is_refused_naming_what_this_build_reads() {
    let error = map2d6::read(r#"{"format": 7, "parts": [{"what": "is this"}]}"#).unwrap_err();
    assert_eq!(error, Map2d6Error::UnsupportedFormat { found: 7 });
    assert!(error.to_string().contains("reads up to 6"), "{error}");
    assert_eq!(
        map2d6::read(r#"{"format": 0}"#),
        Err(Map2d6Error::UnsupportedFormat { found: 0 })
    );
}

/// Today's reader still refuses format 6, and still reads formats 1 to 5;
/// this reader reads 6 and sends 1 to 5 back to today's.
#[test]
fn todays_reader_refuses_6_and_still_reads_1_to_5() {
    let six = example("circular_display");
    assert_eq!(
        Map2dDoc::from_json(&six),
        Err(Map2dError::UnsupportedFormat {
            found: 6,
            supported: 5
        })
    );
    for format in 1..=5 {
        let text = format!(r#"{{"format": {format}, "objects": []}}"#);
        assert!(Map2dDoc::from_json(&text).is_ok(), "format {format}");
        assert_eq!(map2d6::peek_format(&text), Ok(format));
        assert_eq!(
            map2d6::read(&text),
            Err(Map2d6Error::OlderFormat { found: format })
        );
    }
}

/// A path's `d` with a move inside keeps the jumper: the same lamps as the
/// points-and-gaps form.
#[test]
fn a_move_inside_a_path_is_the_jumper() {
    let m = map2d6::read(&example("eaves")).unwrap();
    let tree = render(&m).unwrap();
    let today = resolve(&today_doc(
        "lab/mapping/projects/eaves-wired/fixture.map2d.json",
    ))
    .unwrap();
    assert_eq!(bits(tree.positions()), bits(&today.positions()));
}

/// Path data reads moves and lines, `H` and `V` as lines and relative
/// commands as absolute; it is written absolute, one space between tokens,
/// each number the shortest that reads back the same.
#[test]
fn path_data_reads_lines_and_writes_them_absolute() {
    let m = read_part_kind(r#"{"path": {"d": "m10,10 h30 v5.5 L 0.1 -0 M1e1 2", "count": 3}}"#);
    let PartKind::Shape(Shape::Path(path)) = &m.parts[0].kind else {
        panic!("not a path");
    };
    assert_eq!(
        path.commands,
        [
            PathCommand::Move([10.0, 10.0]),
            PathCommand::Line([40.0, 10.0]),
            PathCommand::Line([40.0, 15.5]),
            PathCommand::Line([0.1, -0.0]),
            PathCommand::Move([10.0, 2.0]),
        ]
    );
    let text = map2d6::write(&m).unwrap();
    assert!(
        text.contains(r#""d": "M 10 10 L 40 10 L 40 15.5 L 0.1 -0 M 10 2""#),
        "{text}"
    );
}

/// A close, a curve or an arc in a path is refused by name: this build
/// draws straight lines only.
#[test]
fn a_curve_an_arc_or_a_close_is_refused_by_name() {
    for command in ['Z', 'z', 'C', 'c', 'S', 'Q', 'T', 'A', 'a'] {
        let args = if command.eq_ignore_ascii_case(&'z') {
            ""
        } else {
            " 1 2 3 4 5 6 7"
        };
        let json = part_kind_json(&format!(
            r#"{{"path": {{"d": "M 0 0 L 5 5 {command}{args}", "count": 2}}}}"#
        ));
        let error = map2d6::read(&json).unwrap_err();
        assert_eq!(
            error,
            Map2d6Error::PathCommandNotBuilt {
                part: "path1".to_string(),
                command
            }
        );
        assert!(error.to_string().contains("straight lines only"), "{error}");
    }
}

/// A path that doesn't start with a move, or holds what isn't path data, is
/// refused, naming the part.
#[test]
fn path_data_that_isnt_a_path_is_refused() {
    for d in ["L 1 2", "", "M 1", "M 1 2 X 3 4", "M 1 2 L 3 four"] {
        let json = part_kind_json(&format!(r#"{{"path": {{"d": "{d}", "count": 2}}}}"#));
        let error = map2d6::read(&json).unwrap_err();
        assert!(
            matches!(&error, Map2d6Error::PathData { part, .. } if part == "path1"),
            "{d:?}: {error:?}"
        );
    }
}

/// An absent start field is Keep, in a part's start and the fixture's.
#[test]
fn an_absent_start_field_is_keep() {
    let m = map2d6::read(
        r#"{"format": 6, "next": {"path": 2}, "start": {"port": {"set": 1}},
            "parts": [{"id": "path1", "start": {"channel": {"add": {"lamps": 2}}},
                       "kind": {"path": {"d": "M 0 0 L 9 0", "count": 2}}}]}"#,
    )
    .unwrap();
    assert_eq!(
        m.start,
        Start {
            output: OutputOp::Keep,
            port: PortOp::set(1),
            channel: ChannelOp::Keep,
        }
    );
    assert_eq!(m.parts[0].start.output, OutputOp::Keep);
    assert_eq!(m.parts[0].start.port, PortOp::Keep);
    assert_eq!(m.parts[0].start.channel, ChannelOp::Add(Amount::lamps(2)));
}

/// An amount says its unit: `{"lamps": 4}` and `{"channels": 12}` land the
/// same RGB segment.
#[test]
fn an_amount_says_its_unit() {
    let landing = |amount: &str| {
        let m = map2d6::read(&format!(
            r#"{{"format": 6, "next": {{"path": 2}},
                "start": {{"output": {{"set": "out"}}, "port": {{"set": 0}},
                          "channel": {{"set": {amount}}}}},
                "parts": [{{"id": "path1", "kind": {{"path": {{"d": "M 0 0 L 9 0", "count": 3}}}}}}]}}"#
        ))
        .unwrap();
        let p = patch(&m, &[OutputPorts::of_lamps("out", &[Some(60)], 3)]);
        p.landing(&TreeId::parse("path1").unwrap(), 0).unwrap()
    };
    assert_eq!(landing(r#"{"lamps": 4}"#), landing(r#"{"channels": 12}"#));
    assert_eq!(landing(r#"{"lamps": 4}"#).channel, 12);
}

/// An override's `"keep"` is read and written back: it holds the cursor,
/// where an absent field takes what the node would get.
#[test]
fn an_overrides_keep_is_not_absent() {
    let text = map2d6::write(&{
        let mut m = Mapping::new();
        let arm = m.new_part(straight(4));
        let mut repeat = m.new_part(PartKind::Repeat(Repeat::new(arm, [0.0, 0.0], 3)));
        repeat.flow = Flow::OnePerPort { from: PortKey(0) };
        m.parts.push(repeat);
        m.start = Start::at("out", 0, Amount::lamps(0));
        m.overrides.insert(
            TreeId::parse("path1@2").unwrap(),
            Override {
                port: Some(PortOp::Keep),
                channel: Some(ChannelOp::Keep),
                ..Override::default()
            },
        );
        m
    })
    .unwrap();
    assert!(text.contains(r#""port": "keep""#), "{text}");
    let m = map2d6::read(&text).unwrap();
    let p = patch(&m, &[OutputPorts::of_lamps("out", &[Some(60); 3], 3)]);
    let copy2 = p.landing(&TreeId::parse("path1@2").unwrap(), 0).unwrap();
    assert_eq!(
        (copy2.port, copy2.channel),
        (PortKey(1), 12),
        "follows copy 1"
    );
}

// ---- refusals ----------------------------------------------------------------

/// A repeated id, an `@` or `/` in a part id, or a counter below an
/// existing id is refused, naming it.
#[test]
fn a_bad_id_or_counter_is_refused_naming_it() {
    let two = r#"{"format": 6, "parts": [
        {"id": "front", "kind": {"path": {"d": "M 0 0 L 1 0", "count": 2}}},
        {"id": "g", "kind": {"group": {"parts": [
            {"id": "front", "kind": {"path": {"d": "M 0 0 L 1 0", "count": 2}}}]}}}]}"#;
    assert_eq!(
        map2d6::read(two),
        Err(Map2d6Error::DuplicatePartId {
            id: "front".to_string()
        })
    );

    for (id, mark) in [("panel@2", "'@'"), ("circle1/r3", "'/'")] {
        let json = format!(
            r#"{{"format": 6, "parts": [{{"id": "{id}", "kind": {{"path": {{"d": "M 0 0 L 1 0", "count": 2}}}}}}]}}"#
        );
        let error = map2d6::read(&json).unwrap_err();
        assert!(
            matches!(&error, Map2d6Error::InvalidPartId { id: named, reason } if named == id && reason.contains(mark)),
            "{error:?}"
        );
    }

    let low = r#"{"format": 6, "next": {"path": 3}, "parts": [
        {"id": "path3", "kind": {"path": {"d": "M 0 0 L 1 0", "count": 2}}}]}"#;
    assert_eq!(
        map2d6::read(low),
        Err(Map2d6Error::CounterBelowId {
            word: "path".to_string(),
            next: 3,
            id: "path3".to_string()
        })
    );
    let absent = low.replace(r#""next": {"path": 3}, "#, "");
    assert!(matches!(
        map2d6::read(&absent),
        Err(Map2d6Error::CounterBelowId { next: 1, .. })
    ));
    let unknown = low.replace(r#"{"path": 3}"#, r#"{"path": 4, "ringset": 1}"#);
    assert_eq!(
        map2d6::read(&unknown),
        Err(Map2d6Error::UnknownCounter {
            word: "ringset".to_string()
        })
    );
}

/// A ring key repeated in its circle, or a `next_ring` that would mint a
/// key a ring has, is refused, naming the circle and the key.
#[test]
fn a_bad_ring_key_is_refused_naming_it() {
    let circle = |next_ring: u32, keys: [&str; 2]| {
        part_kind_json(&format!(
            r#"{{"circle": {{"center": [0, 0], "next_ring": {next_ring}, "rings": [
                {{"key": "{}", "radius": 10, "count": 6}}, {{"key": "{}", "radius": 5, "count": 3}}]}}}}"#,
            keys[0], keys[1]
        ))
        .replace("path1", "circle1")
        .replace(r#""path": 2"#, r#""circle": 2"#)
    };
    assert!(map2d6::read(&circle(3, ["r1", "r2"])).is_ok());
    assert!(matches!(
        map2d6::read(&circle(3, ["r1", "r1"])),
        Err(Map2d6Error::InvalidRingKey { part, key, .. }) if part == "circle1" && key == "r1"
    ));
    assert!(matches!(
        map2d6::read(&circle(3, ["r1", "R2"])),
        Err(Map2d6Error::InvalidRingKey { key, .. }) if key == "R2"
    ));
    assert_eq!(
        map2d6::read(&circle(2, ["r1", "r2"])),
        Err(Map2d6Error::RingCounterBelowKey {
            part: "circle1".to_string(),
            next: 2,
            key: "r2".to_string()
        })
    );
}

/// An override keyed by text that is not a node id is refused, naming it.
#[test]
fn a_malformed_override_id_is_refused() {
    for bad in ["path1@0", "Path1", "path1/", "path1@x"] {
        let json = part_kind_json(r#"{"path": {"d": "M 0 0 L 1 0", "count": 2}}"#).replace(
            r#""parts""#,
            &format!(r#""overrides": {{"{bad}": {{"off": true}}}}, "parts""#),
        );
        assert_eq!(
            map2d6::read(&json),
            Err(Map2d6Error::MalformedOverrideId {
                id: bad.to_string()
            }),
            "{bad}"
        );
    }
}

/// An override naming a node that isn't there is read and kept, and the
/// patcher reports it.
#[test]
fn an_override_naming_no_node_is_kept() {
    let json = part_kind_json(r#"{"path": {"d": "M 0 0 L 1 0", "count": 2}}"#).replace(
        r#""parts""#,
        r#""overrides": {"gone@4": {"port": {"set": 2}}}, "parts""#,
    );
    let m = map2d6::read(&json).unwrap();
    let gone = TreeId::parse("gone@4").unwrap();
    assert!(m.overrides.contains_key(&gone));
    let p = patch(&m, &[OutputPorts::of_lamps("out", &[Some(60)], 3)]);
    assert!(p.problems.contains(&Problem::GoneNode(gone)));
    assert!(map2d6::write(&m).unwrap().contains(r#""gone@4""#));
}

/// A repeat of more copies than an editor may author is refused, naming it.
#[test]
fn a_repeat_of_too_many_copies_is_refused() {
    let json = part_kind_json(
        r#"{"repeat": {"center": [0, 0], "count": 65, "part": {"id": "path2",
            "kind": {"path": {"d": "M 0 0 L 1 0", "count": 2}}}}}"#,
    )
    .replace(r#""path": 2"#, r#""path": 3"#);
    assert_eq!(
        map2d6::read(&json),
        Err(Map2d6Error::TooManyCopies {
            part: "path1".to_string(),
            count: 65
        })
    );
}

/// Lamp facts, and a port step only a flow renders, are refused on write:
/// format 6 has no place for them.
#[test]
fn what_format_6_has_no_place_for_is_refused_on_write() {
    let mut m = Mapping::new();
    m.lamp.colors = Some(Colors::Rgbw);
    assert!(matches!(
        map2d6::write(&m),
        Err(Map2d6Error::NotWritable { reason }) if reason.contains("lamp facts")
    ));

    let mut m = Mapping::new();
    m.start.port = PortOp::Set {
        key: PortKey(2),
        plus: 1,
    };
    assert!(matches!(
        map2d6::write(&m),
        Err(Map2d6Error::NotWritable { reason }) if reason.contains("flow")
    ));
}

/// Counters are written above every id that reads as minted, even when the
/// mapping's own counter is lower.
#[test]
fn counters_are_written_honest() {
    let mut m = Mapping::new();
    m.parts
        .push(Part::new(PartId::new("path7").unwrap(), straight(2)));
    let text = map2d6::write(&m).unwrap();
    assert!(text.contains(r#""path": 8"#), "{text}");
    assert!(map2d6::read(&text).is_ok());
}

// ---- parity with today's documents (A2, A4) ----------------------------------

/// Every layout in the catalog, the test projects and the lab's projects,
/// written as format 6 and read back, renders today's lamps bit for bit:
/// ring sets as circles split one, gaps as moves.
#[test]
fn every_layout_through_format_6_renders_todays_lamps_bit_for_bit() {
    let mut files = Vec::new();
    for dir in ["catalog", "projects", "lab/mapping/projects"] {
        collect_map2d(&root().join(dir), &mut files);
    }
    files.sort();
    assert!(files.len() >= 100, "found {} documents", files.len());
    for file in files {
        let doc = Map2dDoc::from_json(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let today = resolve(&doc).unwrap();
        let text = map2d6::write(&today_as_mapping(&doc)).unwrap();
        let m = map2d6::read(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let tree = render(&m).unwrap();
        assert_eq!(
            bits(tree.positions()),
            bits(&today.positions()),
            "{}",
            file.display()
        );
        assert_eq!(m.canvas, doc.canvas);
        assert_eq!(m.sample_diameter.to_bits(), doc.sample_diameter.to_bits());
    }
}

/// Zook's dome (the scale a microcontroller drives) is smaller in format 6
/// than today, and stays well under the on-device 32 KB read trigger even
/// with an override on every copy.
#[test]
fn zooks_dome_is_small_in_format_6() {
    let file = "catalog/projects/zook-dome/fixture.map2d.json";
    let today_bytes = std::fs::read_to_string(root().join(file)).unwrap().len();
    let as_flowing = example("dome_slice");
    let mut every_copy = map2d6::read(&as_flowing).unwrap();
    every_copy.parts[0].flow = Flow::EndToEnd;
    for k in 0..5 {
        every_copy.overrides.insert(
            TreeId::parse("path1").unwrap().in_copy(k).canonical(),
            Override {
                port: Some(PortOp::set(k)),
                channel: Some(ChannelOp::Set(Amount::lamps(0))),
                ..Override::default()
            },
        );
    }
    let every_copy = map2d6::write(&every_copy).unwrap();
    let sizes = format!(
        "zook dome: today {today_bytes} B; format 6 one per port {} B; an override on every copy {} B",
        as_flowing.len(),
        every_copy.len()
    );
    assert!(as_flowing.len() < today_bytes, "{sizes}");
    assert!(every_copy.len() < 32 * 1024, "{sizes}");
}

// ---- the journeys' documents -----------------------------------------------

/// The choker's document (the catalog's own paths) patches as its journey
/// ends: the letters end to end from lamp 0, F's lower stroke at lamp 42,
/// U at 52, the last L at 65; its lamps are today's.
#[test]
fn the_chokers_document_patches_as_its_journey_ends() {
    let m = map2d6::read(&example("choker")).unwrap();
    let p = patch(&m, &[OutputPorts::of_lamps("out", &[None], 3)]);
    let lamp = |id: &str| p.landing(&node(id), 0).map(|a| (a.port.0, a.channel / 3));
    assert_eq!(lamp("p"), Some((0, 0)));
    assert_eq!(lamp("f-lower"), Some((0, 42)));
    assert_eq!(lamp("u"), Some((0, 52)));
    assert_eq!(lamp("l2"), Some((0, 65)));
    assert!(p.problems.is_empty(), "{:?}", p.problems);
    let today = resolve(&today_doc(
        "catalog/projects/playful-choker/playful.map2d.json",
    ))
    .unwrap();
    assert_eq!(
        bits(render(&m).unwrap().positions()),
        bits(&today.positions())
    );
}

/// The circular display's document, as soldered, patches as its journey
/// ends: r1 and r2 on port 0, ring 3 reversed from the start of port 1, r4
/// and the centre after it.
#[test]
fn the_circular_displays_document_patches_as_soldered() {
    let m = map2d6::read(&example("circular_display")).unwrap();
    let p = patch(&m, &[OutputPorts::of_lamps("main", &[Some(64); 5], 3)]);
    let ring = |key: &str| span(&p, &format!("circle1/{key}"));
    assert_eq!(ring("r1"), Some((0, 0, 23)));
    assert_eq!(ring("r2"), Some((0, 24, 39)));
    assert_eq!(ring("r3"), Some((1, 11, 0)));
    assert_eq!(ring("r4"), Some((1, 12, 19)));
    assert_eq!(ring("r5"), Some((1, 20, 20)));
    assert!(p.problems.is_empty(), "{:?}", p.problems);
    let tree = render(&m).unwrap();
    let centre = tree.segment(&node("circle1/r5")).unwrap();
    assert_eq!(tree.lamps_of(centre), [[50.0, 50.0]]);
}

/// The dome slice's document (zook's own sector) patches as its journey
/// ends: one per port, copy k on port k at its first lamp; its lamps are
/// today's.
#[test]
fn the_dome_slices_document_patches_one_per_port() {
    let m = map2d6::read(&example("dome_slice")).unwrap();
    let p = patch(
        &m,
        &[OutputPorts::of_lamps(
            "out",
            &[Some(300), Some(300), Some(300), Some(300), None],
            3,
        )],
    );
    for k in 0..5 {
        let copy = TreeId::parse("path1").unwrap().in_copy(k).canonical();
        let first = p.landing(&copy, 0).unwrap();
        assert_eq!((first.port.0, first.channel), (k, 0), "copy {k}");
    }
    assert!(p.problems.is_empty(), "{:?}", p.problems);
    let today = resolve(&today_doc("catalog/projects/zook-dome/fixture.map2d.json")).unwrap();
    assert_eq!(
        bits(render(&m).unwrap().positions()),
        bits(&today.positions())
    );
}

/// The eaves' document, as wired, patches as its journey ends: the front
/// fed from its far end on port 0, the side on port 1.
#[test]
fn the_eaves_document_patches_as_wired() {
    let m = map2d6::read(&example("eaves")).unwrap();
    let p = patch(&m, &[OutputPorts::of_lamps("out", &[Some(60), None], 3)]);
    assert_eq!(span(&p, "front"), Some((0, 59, 0)));
    assert_eq!(span(&p, "side"), Some((1, 0, 49)));
    assert!(p.segment(&node("side")).unwrap().wrapped);
}

// ---- helpers --------------------------------------------------------------

const JOURNEYS: [&str; 4] = ["choker", "circular_display", "dome_slice", "eaves"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn example(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/format6")
        .join(format!("{name}.map2d.json"));
    std::fs::read_to_string(path).unwrap()
}

fn today_doc(rel: &str) -> Map2dDoc {
    Map2dDoc::from_json(&std::fs::read_to_string(root().join(rel)).unwrap()).unwrap()
}

/// A document of one part, `path1` (or whatever the kind's id is set to
/// after), holding this kind.
fn part_kind_json(kind: &str) -> String {
    format!(
        r#"{{"format": 6, "next": {{"path": 2}}, "parts": [{{"id": "path1", "kind": {kind}}}]}}"#
    )
}

fn read_part_kind(kind: &str) -> Mapping {
    map2d6::read(&part_kind_json(kind)).unwrap()
}

fn straight(count: u32) -> PartKind {
    PartKind::Shape(Shape::Path(Path::through(
        &[[0.0, 0.0], [10.0, 0.0]],
        count,
    )))
}

fn node(text: &str) -> TreeId {
    TreeId::parse(text).unwrap()
}

fn patch(m: &Mapping, outputs: &[OutputPorts]) -> Patching {
    patch_mapping(m, &render(m).unwrap(), outputs)
}

/// A segment's span as (port, the lamp its first lamp lands on, the lamp
/// its last lamp lands on), in RGB lamps.
fn span(p: &Patching, id: &str) -> Option<(u32, u32, u32)> {
    let id = node(id);
    let n = p.segment(&id)?.lamps;
    let first = p.landing(&id, 0)?;
    let last = p.landing(&id, n - 1)?;
    assert_eq!(first.port, last.port);
    Some((first.port.0, first.channel / 3, last.channel / 3))
}

/// A mapping with every construct format 6 has: each shape, a split path
/// and circle, points, a group, a repeat, part starts and flows, and every
/// kind of override field.
fn every_construct() -> Mapping {
    let mut m = Mapping::new();
    m.sample_diameter = 1.25;
    m.canvas = Some([-1.5, 0.0, 300.0, 120.25]);

    let mut split = Path::of(
        vec![
            PathCommand::Move([0.0, 0.0]),
            PathCommand::Line([10.0, 0.0]),
            PathCommand::Move([20.0, 0.0]),
            PathCommand::Line([30.5, 0.0]),
        ],
        6,
    );
    split.split = PathSplit::Subpaths;
    split.reversed = true;
    split.align = PathAlign::Inside;
    let split = m.new_part(PartKind::Shape(Shape::Path(split)));
    m.parts.push(split);

    let mut circle = Circle::new([50.0, 50.0], &[(40.0, 12), (20.0, 6), (0.0, 1)]);
    circle.start_angle_deg = 12.5;
    circle.drawn_from = lpc_mapping::RingOrder::InnerFirst;
    circle.dir = lpc_mapping::RingDir::Ccw;
    let mut circle = m.new_part(PartKind::Shape(Shape::Circle(circle)));
    circle.flow = Flow::OnePerPort { from: PortKey(2) };
    circle.name = "the display".to_string();
    m.parts.push(circle);

    let mut one = Circle::new([10.0, 10.0], &[(5.0, 4)]);
    one.split = CircleSplit::One;
    let one = m.new_part(PartKind::Shape(Shape::Circle(one)));

    let polygon = m.new_part(PartKind::Shape(Shape::Polygon(PolygonShape {
        points: vec![[0.0, 0.0], [30.0, 0.0], [15.0, 26.0]],
        count: 9,
        align: PathAlign::Outside,
    })));
    let grid = m.new_part(PartKind::Shape(Shape::Grid(GridShape {
        origin: [3.5, 7.25],
        cols: 4,
        rows: 2,
        pitch: 6.5,
        routing: GridRouting::Raster,
        start_corner: GridCorner::Br,
    })));
    let filled = m.new_part(PartKind::Shape(Shape::FilledPolygon(
        lpc_mapping::FilledPolygonShape {
            points: vec![[0.0, 0.0], [40.0, 0.0], [40.0, 30.0], [0.0, 30.0]],
            pitch: 7.0,
            angle_deg: 20.0,
            origin: [1.0, 0.5],
            routing: GridRouting::Snake,
            start_corner: GridCorner::Tl,
        },
    )));
    let points = m.new_part(PartKind::Shape(Shape::Points(Points {
        at: vec![[1.0, 2.0], [3.0, 4.0]],
    })));
    let mut group = m.new_part(PartKind::Group {
        parts: vec![one, polygon, grid, filled, points],
    });
    group.start = Start::next_port();
    m.parts.push(group);

    let arm = m.new_part(straight(5));
    let mut repeat = m.new_part(PartKind::Repeat(Repeat::new(arm, [40.0, 40.0], 4)));
    repeat.start.channel = ChannelOp::Add(Amount::channels(13));
    m.parts.push(repeat);

    m.start = Start::at("main", 0, Amount::lamps(0));
    m.flow = Flow::OnePerPort { from: PortKey(0) };
    m.overrides.insert(
        node("circle1/r2"),
        Override {
            output: Some(OutputOp::Set(OutputName::new("side"))),
            port: Some(PortOp::Add(1)),
            channel: Some(ChannelOp::Add(Amount::lamps(2))),
            direction: Some(Direction::Reverse),
            offset: Some(3),
            ..Override::default()
        },
    );
    m.overrides.insert(
        node("path2@3"),
        Override {
            output: Some(OutputOp::Keep),
            port: Some(PortOp::Keep),
            channel: Some(ChannelOp::Keep),
            off: true,
            direction: Some(Direction::Forward),
            ..Override::default()
        },
    );
    m.overrides.insert(node("path1/s2"), Override::default());
    m
}

fn collect_map2d(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_map2d(&path, out);
        } else if path.to_string_lossy().ends_with(".map2d.json") {
            out.push(path);
        }
    }
}

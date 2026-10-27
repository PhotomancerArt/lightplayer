//! Statements: the part tree, its ids and its segments (names.html draft 3,
//! "Parts", "Segments", "Ids"). One sentence per test; the doc comment is
//! the statement. Ported from the lab spike's `object_tree.rs` in the
//! settled words, plus the parity statements (A2): every segment carries
//! the lamps today's `resolve()` gives the same geometry, bit for bit.

use std::path::{Path as FsPath, PathBuf};

use lpc_mapping::part::{
    Circle, Colors, Mapping, NodeKind, Order, Part, PartId, PartKind, Path, PathCommand, PathSplit,
    RenderError, Repeat, SegmentKey, Shape, Tree, TreeId, render,
};
use lpc_mapping::{
    GridCorner, GridRouting, GridShape, Map2dDoc, Map2dShape, PathAlign, PathShape, RingDir,
    RingOrder, RingShape, path_gap_breaks, resolve,
};

/// A path is its own segment: its segment's id is the path's id, and its
/// node is that segment.
#[test]
fn a_path_is_its_own_segment() {
    let mut m = Mapping::new();
    let path = add(&mut m, straight(5));
    let tree = render(&m).unwrap();
    assert_eq!(tree.nodes().len(), 1, "one node: the path and its segment");
    assert_eq!(tree.segments()[0].id, TreeId::of(&path));
    assert_eq!(tree.node(0).segment, Some(0));
    assert_eq!(tree.segments()[0].count(), 5);
}

/// A circle renders one segment per ring, `circle1/r1`, `circle1/r2`,
/// generated under the circle's own node.
#[test]
fn a_circle_renders_one_segment_per_ring() {
    let mut m = Mapping::new();
    add(&mut m, circle(&[(40.0, 24), (20.0, 12)]));
    let tree = render(&m).unwrap();
    assert!(
        !tree.node(0).is_segment(),
        "the circle holds no lamps itself"
    );
    let rings: Vec<String> = tree
        .children(0)
        .map(|i| tree.node(i).id.to_string())
        .collect();
    assert_eq!(rings, ["circle1/r1", "circle1/r2"]);
    assert!(
        tree.children(0)
            .all(|i| tree.node(i).kind() == NodeKind::GeneratedSegment)
    );
    assert_eq!(ids(&tree), ["circle1/r1", "circle1/r2"]);
}

/// A ring of one lamp is one lamp at the circle's centre, whatever its
/// radius.
#[test]
fn a_ring_of_one_lamp_sits_at_the_centre() {
    let mut m = Mapping::new();
    add(&mut m, circle(&[(40.0, 24), (12.0, 1)]));
    let tree = render(&m).unwrap();
    let r2 = tree.segment(&id("circle1/r2")).unwrap();
    assert_eq!(tree.lamps_of(r2), [[50.0, 50.0]]);
}

/// A ring set renders one segment, as today: its rings are one run.
#[test]
fn a_ring_set_renders_one_segment() {
    let mut m = Mapping::new();
    add(&mut m, PartKind::Shape(Shape::RingSet(ring_set(2))));
    let tree = render(&m).unwrap();
    assert_eq!(ids(&tree), ["ringset1"]);
    assert_eq!(tree.segments()[0].count(), 16 + 8);
}

/// A path split by subpaths renders one segment per subpath, `path1/s1`,
/// `path1/s2`; the lamps are the unsplit path's.
#[test]
fn a_split_path_renders_one_segment_per_subpath() {
    let mut whole = Mapping::new();
    add(&mut whole, PartKind::Shape(Shape::Path(jumpered(6))));
    let mut split = Mapping::new();
    let mut path = jumpered(6);
    path.split = PathSplit::Subpaths;
    add(&mut split, PartKind::Shape(Shape::Path(path)));

    let whole = render(&whole).unwrap();
    let split = render(&split).unwrap();
    assert_eq!(ids(&split), ["path1/s1", "path1/s2"]);
    assert_eq!(bits(split.positions()), bits(whole.positions()));
    let counts: Vec<u32> = split.segments().iter().map(|s| s.count()).collect();
    assert_eq!(counts, [3, 3]);
}

/// A split path is cut where today's `path_gap_breaks` says, and a
/// reversed one lists its last-drawn subpath first, keeping its key.
#[test]
fn a_split_path_is_cut_where_todays_jumpers_break_it() {
    for reversed in [false, true] {
        let mut path = Path::of(
            vec![
                PathCommand::Move([0.0, 0.0]),
                PathCommand::Line([10.0, 0.0]),
                PathCommand::Move([20.0, 0.0]),
                PathCommand::Line([50.0, 0.0]),
                PathCommand::Move([60.0, 0.0]),
                PathCommand::Line([70.0, 0.0]),
            ],
            11,
        );
        path.reversed = reversed;
        let breaks = path_gap_breaks(&path.to_path_shape().unwrap());
        path.split = PathSplit::Subpaths;
        let mut m = Mapping::new();
        add(&mut m, PartKind::Shape(Shape::Path(path)));
        let tree = render(&m).unwrap();
        let starts: Vec<u32> = tree.segments()[1..].iter().map(|s| s.lamps.start).collect();
        assert_eq!(starts, breaks, "reversed: {reversed}");
        let expected = if reversed {
            ["path1/s3", "path1/s2", "path1/s1"]
        } else {
            ["path1/s1", "path1/s2", "path1/s3"]
        };
        assert_eq!(ids(&tree), expected);
    }
}

/// A subpath with no lamps makes no segment, and the others keep their
/// keys.
#[test]
fn a_subpath_with_no_lamps_makes_no_segment() {
    let mut path = Path::of(
        vec![
            PathCommand::Move([0.0, 0.0]),
            PathCommand::Move([5.0, 0.0]),
            PathCommand::Line([15.0, 0.0]),
        ],
        4,
    );
    path.split = PathSplit::Subpaths;
    let mut m = Mapping::new();
    add(&mut m, PartKind::Shape(Shape::Path(path)));
    let tree = render(&m).unwrap();
    assert_eq!(ids(&tree), ["path1/s2"]);
}

/// A move inside a path is a jumper: it carries no lamps, and the pitch is
/// the same either side.
#[test]
fn a_move_inside_a_path_is_a_jumper() {
    let mut m = Mapping::new();
    add(&mut m, PartKind::Shape(Shape::Path(jumpered(6))));
    let tree = render(&m).unwrap();
    let xs: Vec<f32> = tree.positions().iter().map(|p| p[0]).collect();
    assert_eq!(xs, [0.0, 4.0, 8.0, 22.0, 26.0, 30.0]);
    assert!(
        xs.iter().all(|x| !(10.0 < *x && *x < 20.0)),
        "none on the jumper"
    );
}

/// A repeat holds a part with its own id: copy 0 has the original's ids and
/// copy k is `<id>@k`.
#[test]
fn a_repeat_holds_a_part_with_its_own_id() {
    let mut m = Mapping::new();
    let arm = m.new_part(straight(3));
    let repeat = add(&mut m, PartKind::Repeat(Repeat::new(arm, [50.0, 50.0], 3)));
    let tree = render(&m).unwrap();
    assert_eq!(tree.node(0).id, TreeId::of(&repeat));
    let copies: Vec<String> = tree
        .children(0)
        .map(|i| tree.node(i).id.to_string())
        .collect();
    assert_eq!(copies, ["path1", "path1@1", "path1@2"]);
    let kinds: Vec<NodeKind> = tree.children(0).map(|i| tree.node(i).kind()).collect();
    assert_eq!(
        kinds,
        [
            NodeKind::AuthoredPart,
            NodeKind::GeneratedPart,
            NodeKind::GeneratedPart
        ]
    );
}

/// A repeated group's copies hold copies of each part: `path3@2`.
#[test]
fn a_repeated_groups_copies_hold_copies_of_each_part() {
    let mut m = Mapping::new();
    let _unrelated = add(&mut m, straight(1));
    let _also = add(&mut m, straight(1));
    let a = m.new_part(straight(2));
    let b = m.new_part(straight(2));
    let group = m.new_part(PartKind::Group { parts: vec![a, b] });
    add(
        &mut m,
        PartKind::Repeat(Repeat::new(group, [50.0, 50.0], 3)),
    );
    let tree = render(&m).unwrap();
    let copy2 = tree.find(&id("group1@2")).unwrap();
    let inner: Vec<String> = tree
        .children(copy2)
        .map(|i| tree.node(i).id.to_string())
        .collect();
    assert_eq!(inner, ["path3@2", "path4@2"]);
}

/// Nested repeats stack copies innermost first, and only trailing zeros are
/// dropped: `x@3`, `x@0@3`.
#[test]
fn nested_repeats_stack_copies_innermost_first() {
    let mut m = Mapping::new();
    let x = Part::new(pid("x"), straight(2));
    let inner = m.new_part(PartKind::Repeat(Repeat::new(x, [50.0, 50.0], 4)));
    add(
        &mut m,
        PartKind::Repeat(Repeat::new(inner, [50.0, 50.0], 4)),
    );
    let tree = render(&m).unwrap();
    let all = ids(&tree);
    assert_eq!(&all[..4], ["x", "x@1", "x@2", "x@3"]);
    assert_eq!(all[12], "x@0@3");
    assert_eq!(all[15], "x@3@3");
}

/// Ids round-trip through their text, and a trailing `@0` is not an id.
#[test]
fn ids_round_trip_through_their_text() {
    for text in [
        "path1",
        "circle2/r1",
        "panel@2",
        "circle2/r1@2",
        "x@0@3",
        "sector",
    ] {
        assert_eq!(TreeId::parse(text).unwrap().to_string(), text);
    }
    for not_one in [
        "panel@0", "x@3@0", "panel@", "panel@03", "panel@+3", "/r1", "a/b/c", "A1",
    ] {
        assert!(TreeId::parse(not_one).is_none(), "{not_one}");
    }
}

/// A part id never holds `@` or `/`.
#[test]
fn a_part_id_never_holds_at_or_slash() {
    assert!(PartId::new("panel@2").is_err());
    assert!(PartId::new("circle1/r2").is_err());
    assert!(PartId::new("f-lower").is_ok());
}

/// A minted id is never one the mapping already has, and deleting a part
/// never lowers a counter.
#[test]
fn a_minted_id_is_never_reused() {
    let mut m = Mapping::new();
    m.parts.push(Part::new(pid("path1"), straight(1)));
    m.parts.push(Part::new(pid("path2"), straight(1)));
    let third = add(&mut m, straight(1));
    assert_eq!(
        third.as_str(),
        "path3",
        "a hand-written path1 and path2 are skipped"
    );
    m.parts.retain(|p| p.id != third);
    let fourth = add(&mut m, straight(1));
    assert_eq!(
        fourth.as_str(),
        "path4",
        "path3 is gone, and never comes back"
    );
    assert_eq!(add(&mut m, circle(&[(10.0, 4)])).as_str(), "circle1");
}

/// A circle's ring keys are kept: removing ring 2 leaves ring 3 as `r3`,
/// and a new ring is `r4`.
#[test]
fn a_rings_key_survives_edits_to_the_others() {
    let mut c = Circle::new([50.0, 50.0], &[(40.0, 8), (30.0, 6), (20.0, 4)]);
    c.rings.remove(1);
    assert_eq!(c.add_ring(10.0, 2), SegmentKey::ring(4));
    let mut m = Mapping::new();
    add(&mut m, PartKind::Shape(Shape::Circle(c)));
    assert_eq!(
        ids(&render(&m).unwrap()),
        ["circle1/r1", "circle1/r3", "circle1/r4"]
    );
}

/// Regrouping parts changes no id.
#[test]
fn regrouping_parts_changes_no_id() {
    let mut m = Mapping::new();
    add(&mut m, straight(2));
    add(&mut m, circle(&[(20.0, 4), (10.0, 2)]));
    let before = ids(&render(&m).unwrap());
    let parts = std::mem::take(&mut m.parts);
    let group = m.new_part(PartKind::Group { parts });
    m.parts.push(group);
    let after = render(&m).unwrap();
    assert_eq!(ids(&after), before);
    assert_eq!(after.node(0).id.to_string(), "group1");
}

/// The segments in tree order are the order the fixture lists its lamps.
#[test]
fn the_segments_in_tree_order_are_the_lamp_order() {
    let mut m = Mapping::new();
    let a = m.new_part(straight(2));
    let b = m.new_part(straight(3));
    add(&mut m, PartKind::Group { parts: vec![a, b] });
    add(&mut m, circle(&[(20.0, 8), (10.0, 4)]));
    let tree = render(&m).unwrap();
    assert_eq!(ids(&tree), ["path1", "path2", "circle1/r1", "circle1/r2"]);
    let mut next = 0;
    for segment in tree.segments() {
        assert_eq!(segment.lamps.start, next);
        next = segment.lamps.end;
    }
    assert_eq!(next as usize, tree.positions().len());
}

/// A node knows its parent, the segments under it, and the part it came
/// from.
#[test]
fn a_node_knows_its_parent_its_segments_and_its_part() {
    let mut m = Mapping::new();
    let ring = m.new_part(circle(&[(20.0, 8), (10.0, 4)]));
    add(&mut m, PartKind::Repeat(Repeat::new(ring, [50.0, 50.0], 2)));
    let tree = render(&m).unwrap();
    let r2 = tree.find(&id("circle1/r2@1")).unwrap();
    let copy = tree.parent(r2).unwrap();
    assert_eq!(tree.node(copy).id.to_string(), "circle1@1");
    assert_eq!(tree.parent(copy), Some(0));
    assert_eq!(tree.part_of(r2).as_str(), "circle1");
    let under: Vec<String> = tree
        .segments_under(copy)
        .iter()
        .map(|s| s.id.to_string())
        .collect();
    assert_eq!(under, ["circle1/r1@1", "circle1/r2@1"]);
    assert_eq!(tree.segments_under(0).len(), 4);
}

/// A segment's colors come from the nearest part that says, else the
/// mapping's, else RGB.
#[test]
fn a_segments_colors_come_from_the_nearest_part_that_says() {
    let mut m = Mapping::new();
    add(&mut m, straight(1));
    let mut inner = m.new_part(straight(1));
    inner.lamp.colors = Some(Colors::Cct);
    let mut group = m.new_part(PartKind::Group {
        parts: vec![inner, Part::new(pid("plain"), straight(1))],
    });
    group.lamp.colors = Some(Colors::Rgbw);
    m.parts.push(group);
    let colors = |m: &Mapping| -> Vec<Colors> {
        render(m)
            .unwrap()
            .segments()
            .iter()
            .map(|s| s.lamp.colors_or_rgb())
            .collect()
    };
    assert_eq!(colors(&m), [Colors::Rgb, Colors::Cct, Colors::Rgbw]);
    m.lamp.colors = Some(Colors::Mono);
    assert_eq!(colors(&m), [Colors::Mono, Colors::Cct, Colors::Rgbw]);
    assert_eq!(Colors::Rgbw.channels(), 4);
}

/// An order whose length isn't its colors' channel count is refused.
#[test]
fn an_order_that_doesnt_fit_its_colors_is_refused() {
    let mut m = Mapping::new();
    m.lamp.order = Some(Order::parse("GRB").unwrap());
    let mut rgbw = m.new_part(straight(1));
    rgbw.lamp.colors = Some(Colors::Rgbw);
    m.parts.push(rgbw);
    add(&mut m, straight(1));
    let tree = render(&m).unwrap();
    assert!(
        tree.segments()[0].lamp.checked_order().is_err(),
        "GRB on RGBW"
    );
    assert!(tree.segments()[1].lamp.checked_order().is_ok());
    assert!(Order::parse("GRBW").unwrap().fits(Colors::Rgbw));
    assert!(Order::parse("GGB").is_err(), "a colour twice");
    assert!(Order::parse("GRX").is_err(), "not a colour");
    assert_eq!(Order::parse("grbw").unwrap().to_string(), "GRBW");
    assert_eq!(
        Order::parse("GRB").unwrap().packed(),
        2 | (1 << 3) | (3 << 6)
    );
}

/// Two parts with one id are refused, not rendered.
#[test]
fn two_parts_with_one_id_are_refused() {
    let mut m = Mapping::new();
    m.parts.push(Part::new(pid("same"), straight(1)));
    m.parts.push(Part::new(pid("same"), straight(1)));
    assert_eq!(render(&m), Err(RenderError::DuplicateId(pid("same"))));
}

/// A mapping too big for memory is refused, never aborted.
#[test]
fn a_mapping_too_big_for_memory_is_refused() {
    let mut m = Mapping::new();
    let huge = m.new_part(straight(u32::MAX));
    add(
        &mut m,
        PartKind::Repeat(Repeat::new(huge, [0.0, 0.0], u32::MAX)),
    );
    assert!(matches!(
        render(&m),
        Err(RenderError::TooBigForMemory { .. })
    ));
}

// ---- parity with today's resolver (A2) ---------------------------------

/// Every corpus document's parts carry exactly the lamps today's
/// `resolve()` gives, bit for bit, one segment per strand.
#[test]
fn the_corpus_renders_todays_lamps_bit_for_bit() {
    let corpus = [
        ("basic_button", lpc_mapping::corpus::basic_button()),
        ("cat_ears", lpc_mapping::corpus::cat_ears()),
        ("panel_16x16", lpc_mapping::corpus::panel_16x16()),
        ("gapped_path", lpc_mapping::corpus::gapped_path()),
        ("repeated_sector", lpc_mapping::corpus::repeated_sector()),
        ("fyeah", lpc_mapping::corpus::fyeah()),
    ];
    for (name, doc) in corpus {
        assert_parity(name, &doc);
    }
}

/// Every map2d document in the catalog renders today's lamps, bit for bit.
#[test]
fn the_catalog_renders_todays_lamps_bit_for_bit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../catalog");
    let mut files = Vec::new();
    collect_map2d(&root, &mut files);
    files.sort();
    assert!(files.len() >= 30, "found {} catalog documents", files.len());
    for file in files {
        let json = std::fs::read_to_string(&file).unwrap();
        let doc = Map2dDoc::from_json(&json).unwrap();
        assert_parity(&file.display().to_string(), &doc);
    }
}

/// Every grid routing and corner, reversed paths with and without jumpers,
/// polygons, ring sets of every order and direction, a filled polygon, and
/// nested repeats render today's lamps, bit for bit.
#[test]
fn every_shape_kind_renders_todays_lamps_bit_for_bit() {
    let mut shapes = Vec::new();
    for routing in [GridRouting::Snake, GridRouting::Raster] {
        for start_corner in [
            GridCorner::Tl,
            GridCorner::Tr,
            GridCorner::Bl,
            GridCorner::Br,
        ] {
            shapes.push(Map2dShape::Grid(GridShape {
                origin: [3.5, 7.25],
                cols: 5,
                rows: 4,
                pitch: 6.5,
                routing,
                start_corner,
            }));
        }
    }
    for reversed in [false, true] {
        for gaps in [vec![], vec![1], vec![0, 2]] {
            shapes.push(Map2dShape::Path(PathShape {
                points: vec![[1.0, 2.0], [30.0, 5.0], [40.0, 40.0], [3.0, 33.0]],
                count: 17,
                reversed,
                gaps,
                align: PathAlign::On,
            }));
        }
    }
    for order in [RingOrder::OuterFirst, RingOrder::InnerFirst] {
        for dir in [RingDir::Cw, RingDir::Ccw] {
            let mut ring = ring_set(3);
            ring.order = order;
            ring.dir = dir;
            ring.counts = vec![0, 9];
            shapes.push(Map2dShape::Ring(ring));
        }
    }
    shapes.push(Map2dShape::Polygon(lpc_mapping::PolygonShape {
        points: vec![[0.0, 0.0], [30.0, 0.0], [15.0, 26.0]],
        count: 9,
        align: PathAlign::On,
    }));
    shapes.push(Map2dShape::FilledPolygon(lpc_mapping::FilledPolygonShape {
        points: vec![[0.0, 0.0], [40.0, 0.0], [40.0, 30.0], [0.0, 30.0]],
        pitch: 7.0,
        angle_deg: 20.0,
        origin: [1.0, 0.5],
        routing: GridRouting::Snake,
        start_corner: GridCorner::Br,
    }));
    let inner = Map2dShape::Repeat(lpc_mapping::RepeatShape {
        shape: Box::new(shapes[9].clone()),
        center: [20.0, 20.0],
        count: 3,
    });
    shapes.push(Map2dShape::Repeat(lpc_mapping::RepeatShape {
        shape: Box::new(inner),
        center: [50.0, 45.0],
        count: 7,
    }));
    let mut doc = Map2dDoc::new();
    for (i, shape) in shapes.into_iter().enumerate() {
        doc.objects.push(lpc_mapping::Map2dObject {
            name: format!("shape {i}"),
            id: None,
            stride: None,
            shape,
        });
    }
    assert_parity("every shape kind", &doc);
}

/// A circle's ring k carries the lamps of a single-ring ring set of the same
/// radius and count, except a ring of one lamp.
#[test]
fn a_circles_ring_is_a_single_ring_ring_set() {
    let mut c = Circle::new([40.0, 60.0], &[(30.0, 24), (20.0, 13), (5.0, 1)]);
    c.start_angle_deg = 17.0;
    c.dir = RingDir::Ccw;
    let mut m = Mapping::new();
    add(&mut m, PartKind::Shape(Shape::Circle(c.clone())));
    let tree = render(&m).unwrap();
    for (ring, segment) in c.rings.iter().zip(tree.segments()) {
        if ring.count == 1 {
            assert_eq!(tree.lamps_of(segment), [c.center]);
            continue;
        }
        let mut doc = Map2dDoc::new();
        doc.objects.push(lpc_mapping::Map2dObject {
            name: String::new(),
            id: None,
            stride: None,
            shape: Map2dShape::Ring(RingShape {
                center: c.center,
                radius: ring.radius,
                outer_count: ring.count,
                rings: 1,
                counts: vec![],
                order: RingOrder::OuterFirst,
                start_angle_deg: c.start_angle_deg,
                dir: c.dir,
            }),
        });
        let today = resolve(&doc).unwrap().positions();
        assert_eq!(bits(tree.lamps_of(segment)), bits(&today), "{}", segment.id);
    }
}

// ---- helpers --------------------------------------------------------------

fn add(m: &mut Mapping, kind: PartKind) -> PartId {
    let part = m.new_part(kind);
    let id = part.id.clone();
    m.parts.push(part);
    id
}

fn pid(text: &str) -> PartId {
    PartId::new(text).unwrap()
}

fn id(text: &str) -> TreeId {
    TreeId::parse(text).unwrap()
}

fn ids(tree: &Tree) -> Vec<String> {
    tree.segments().iter().map(|s| s.id.to_string()).collect()
}

fn straight(count: u32) -> PartKind {
    PartKind::Shape(Shape::Path(Path::through(
        &[[10.0, 10.0], [80.0, 10.0]],
        count,
    )))
}

fn circle(rings: &[(f32, u32)]) -> PartKind {
    PartKind::Shape(Shape::Circle(Circle::new([50.0, 50.0], rings)))
}

/// A 10-unit line, a 10-unit jumper, a 10-unit line.
fn jumpered(count: u32) -> Path {
    Path::of(
        vec![
            PathCommand::Move([0.0, 0.0]),
            PathCommand::Line([10.0, 0.0]),
            PathCommand::Move([20.0, 0.0]),
            PathCommand::Line([30.0, 0.0]),
        ],
        count,
    )
}

fn ring_set(rings: u32) -> RingShape {
    RingShape {
        center: [50.0, 50.0],
        radius: 40.0,
        outer_count: 16,
        rings,
        counts: vec![],
        order: RingOrder::OuterFirst,
        start_angle_deg: -90.0,
        dir: RingDir::Cw,
    }
}

fn bits(positions: &[[f32; 2]]) -> Vec<[u32; 2]> {
    positions
        .iter()
        .map(|p| [p[0].to_bits(), p[1].to_bits()])
        .collect()
}

/// The document's objects as parts, rendered, against today's resolver:
/// the same lamps bit for bit, and one segment per strand.
fn assert_parity(name: &str, doc: &Map2dDoc) {
    let today = resolve(doc).unwrap_or_else(|e| panic!("{name}: {e}"));
    let mut m = Mapping::new();
    for object in &doc.objects {
        let kind = kind_of(&object.shape, &mut m);
        let part = match &object.id {
            Some(id) => Part::new(pid(id.as_str()), kind),
            None => m.new_part(kind),
        };
        m.parts.push(part);
    }
    let tree = render(&m).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(
        bits(tree.positions()),
        bits(&today.positions()),
        "{name}: lamps"
    );
    let strands: Vec<(u32, u32)> = today.spans.iter().map(|s| (s.start, s.count)).collect();
    let segments: Vec<(u32, u32)> = tree
        .segments()
        .iter()
        .map(|s| (s.lamps.start, s.count()))
        .collect();
    assert_eq!(segments, strands, "{name}: one segment per strand");
}

/// Test-only: today's shape as a part kind (a repeat's shape becomes a part
/// with an id of its own).
fn kind_of(shape: &Map2dShape, m: &mut Mapping) -> PartKind {
    match shape {
        Map2dShape::Grid(grid) => PartKind::Shape(Shape::Grid(grid.clone())),
        Map2dShape::Ring(ring) => PartKind::Shape(Shape::RingSet(ring.clone())),
        Map2dShape::Polygon(polygon) => PartKind::Shape(Shape::Polygon(polygon.clone())),
        Map2dShape::FilledPolygon(filled) => PartKind::Shape(Shape::FilledPolygon(filled.clone())),
        Map2dShape::Path(path) => {
            let commands = path
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if i == 0 || path.gaps.contains(&(i as u32 - 1)) {
                        PathCommand::Move(*p)
                    } else {
                        PathCommand::Line(*p)
                    }
                })
                .collect();
            let mut new = Path::of(commands, path.count);
            new.reversed = path.reversed;
            new.align = path.align;
            PartKind::Shape(Shape::Path(new))
        }
        Map2dShape::Repeat(repeat) => {
            let inner = kind_of(&repeat.shape, m);
            let part = m.new_part(inner);
            PartKind::Repeat(Repeat::new(part, repeat.center, repeat.count))
        }
    }
}

fn collect_map2d(dir: &FsPath, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_map2d(&path, out);
        } else if path.to_string_lossy().ends_with(".map2d.json") {
            out.push(path);
        }
    }
}

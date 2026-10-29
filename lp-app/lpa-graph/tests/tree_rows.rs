//! Statements: the tree the Graph shows (D18) — the tree pane's rows and
//! the targets you walk with the keys. One sentence per test; the doc
//! comment is the statement.
//!
//! The rendered tree and its ids are `lpc-mapping`'s (`Tree`, `TreeId`,
//! tested in its `tests/part_tree.rs`); these say what the Graph makes of
//! them. Each test builds its own small project: a fixture `f` and an
//! output `out` with one port, D0.

use lab_mapping_model::*;
use lpc_mapping::part::{Circle, PartKind, Path, Repeat, Shape};

/// A path is one row, its own segment: its lamps sit directly under it as
/// targets, and the tree pane lists nothing below it.
#[test]
fn a_path_is_one_row_with_its_lamps_under_it() {
    let (mut p, f) = bare();
    let path = p.add(&f, straight(10.0, 5)).unwrap();
    let t = Target::Part(path.clone());
    assert_eq!(p.children(Some(&Target::Node(f.clone()))), vec![t.clone()]);
    let lamps: Vec<Target> = (0..5)
        .map(|index| Target::Lamp {
            segment: path.node(),
            index,
        })
        .collect();
    assert_eq!(p.children(Some(&t)), lamps);
    assert_eq!(p.parent(&lamps[3]), Some(t.clone()));
    assert_eq!(
        rows(&p),
        [(0, "f"), (1, "path1"), (0, "out"), (1, "D0")].map(owned)
    );
    assert_eq!(row(&p, &t).detail, "5 lamps");
}

/// A circle's rings are rows under it, made by the circle (drawn lighter),
/// and each ring's parent is its circle.
#[test]
fn a_circles_rings_are_rows_under_it() {
    let (mut p, f) = bare();
    let circle = p.add(&f, circle_kind(&[(40.0, 24), (20.0, 12)])).unwrap();
    let ring = |k: &str| Target::Generated(TreeRef::parse(&f, &format!("circle1/{k}")).unwrap());
    let t = Target::Part(circle);
    assert_eq!(p.children(Some(&t)), vec![ring("r1"), ring("r2")]);
    assert_eq!(p.parent(&ring("r2")), Some(t.clone()));
    assert_eq!(
        rows(&p),
        [
            (0, "f"),
            (1, "circle1"),
            (2, "ring r1"),
            (2, "ring r2"),
            (0, "out"),
            (1, "D0"),
        ]
        .map(owned)
    );
    assert_eq!(row(&p, &t).detail, "2 rings · 36 lamps");
    assert!(!row(&p, &t).derived);
    assert!(row(&p, &ring("r1")).derived);
}

/// A repeat's copies are rows under it: copy 0 is its part, as authored
/// (`path1`), and copy k is generated (`path1@k`), named "copy k".
#[test]
fn a_repeats_copies_are_rows_under_it() {
    let (mut p, f) = bare();
    let arm = p.new_part(&f, straight(10.0, 3)).unwrap();
    let repeat = p
        .add(&f, PartKind::Repeat(Repeat::new(arm, [50.0, 50.0], 3)))
        .unwrap();
    let copy = |text: &str| Target::Generated(TreeRef::parse(&f, text).unwrap());
    let path1 = Target::Part(PartRef::parse(&f, "path1").unwrap());
    let t = Target::Part(repeat);
    assert_eq!(
        p.children(Some(&t)),
        vec![path1.clone(), copy("path1@1"), copy("path1@2")]
    );
    assert_eq!(p.parent(&copy("path1@2")), Some(t.clone()));
    assert_eq!(p.parent(&path1), Some(t));
    assert_eq!(
        rows(&p),
        [
            (0, "f"),
            (1, "repeat1"),
            (2, "path1"),
            (2, "copy 1"),
            (2, "copy 2"),
            (0, "out"),
            (1, "D0"),
        ]
        .map(owned)
    );
}

/// Copy 2 of a repeated group holds copy 2 of each of its parts, and each
/// of them leads back up to it.
#[test]
fn a_repeated_groups_copy_holds_copies_of_its_parts() {
    let (mut p, f) = bare();
    let a = p.new_part(&f, straight(10.0, 2)).unwrap();
    let b = p.new_part(&f, straight(20.0, 2)).unwrap();
    let group = p
        .new_part(&f, PartKind::Group { parts: vec![a, b] })
        .unwrap();
    p.add(&f, PartKind::Repeat(Repeat::new(group, [50.0, 50.0], 3)))
        .unwrap();
    let node = |text: &str| Target::Generated(TreeRef::parse(&f, text).unwrap());
    assert_eq!(
        p.children(Some(&node("group1@2"))),
        vec![node("path1@2"), node("path2@2")]
    );
    assert_eq!(p.parent(&node("path2@2")), Some(node("group1@2")));
}

/// A tree node's ref reads back from its text in its fixture and names that
/// fixture; a part's own node in copy 0 is the part as a target, anything
/// else a generated one.
#[test]
fn a_tree_ref_reads_back_from_its_text() {
    let (p, f) = bare();
    for text in [
        "path1",
        "circle2/r1",
        "panel@2",
        "circle2/r1@2",
        "path1@0@1",
    ] {
        let o = TreeRef::parse(&f, text).unwrap();
        assert_eq!(o.to_string(), text);
        assert_eq!(o.fixture, f);
    }
    assert!(
        TreeRef::parse(&f, "panel@0").is_none(),
        "copy 0 is never written"
    );
    let panel = TreeRef::parse(&f, "panel").unwrap();
    assert_eq!(
        p.tree_target(&panel),
        Target::Part(PartRef::parse(&f, "panel").unwrap())
    );
    let copy = TreeRef::parse(&f, "panel@2").unwrap();
    assert_eq!(p.tree_target(&copy), Target::Generated(copy.clone()));
    assert_eq!(copy.original(), panel);
}

/// A ring of radius 0 is one lamp at the centre: the circular display's
/// centre is its innermost ring.
#[test]
fn a_ring_of_radius_0_is_the_centre() {
    let p = Project::circular_display();
    let display = p.named("display").unwrap().id.clone();
    let centre = TreeRef::parse(&display, "circle1/r5").unwrap();
    let lamps = p.children(Some(&Target::Generated(centre.clone())));
    assert_eq!(lamps.len(), 1);
    let at = p.segment(&centre).unwrap().lamps[0].pos - p.origin(&display);
    assert!(at.distance(Vec2::new(50.0, 50.0)) < 1e-6, "{at:?}");
}

/// The segments in tree order are the order a fixture lists its lamps: a
/// group's parts in order, then what follows it.
#[test]
fn the_segments_in_tree_order_are_the_lamp_order() {
    let (mut p, f) = bare();
    let a = p.new_part(&f, straight(10.0, 2)).unwrap();
    let b = p.new_part(&f, straight(20.0, 2)).unwrap();
    p.add(&f, PartKind::Group { parts: vec![a, b] }).unwrap();
    p.add(&f, circle_kind(&[(20.0, 8)])).unwrap();
    let segments = p.segments_in(&f);
    let ids: Vec<String> = segments.iter().map(|s| s.id.to_string()).collect();
    assert_eq!(ids, ["path1", "path2", "circle1/r1"]);
    let lamps: Vec<Vec2> = segments
        .iter()
        .flat_map(|s| s.lamps.iter().map(|l| l.pos))
        .collect();
    assert_eq!(p.lamps_under(&Target::Node(f.clone())), lamps);
}

// ---- helpers --------------------------------------------------------------

fn bare() -> (Project, NodeId) {
    let mut p = Project::new("t");
    let f = p.add_fixture(None, "f").unwrap();
    p.add_output(None, "out", &[("D0", 60)]).unwrap();
    (p, f)
}

/// A straight path of `n` lamps across the fixture at height `y`.
fn straight(y: f32, n: u32) -> PartKind {
    PartKind::Shape(Shape::Path(Path::through(&[[10.0, y], [80.0, y]], n)))
}

/// A circle at the fixture's middle with these rings, outside in.
fn circle_kind(rings: &[(f32, u32)]) -> PartKind {
    PartKind::Shape(Shape::Circle(Circle::new([50.0, 50.0], rings)))
}

/// The tree pane's rows, as (depth, label).
fn rows(p: &Project) -> Vec<(usize, String)> {
    p.tree_rows()
        .into_iter()
        .map(|r| (r.depth, r.label))
        .collect()
}

/// The tree pane's row for one target.
fn row(p: &Project, t: &Target) -> TreeRow {
    p.tree_rows().into_iter().find(|r| &r.target == t).unwrap()
}

fn owned((depth, label): (usize, &str)) -> (usize, String) {
    (depth, label.to_string())
}

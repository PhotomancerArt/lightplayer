//! Statements: the object tree and its ids (theory "Components and objects",
//! "Ids"). One sentence per test; the doc comment is the statement.
//!
//! Each test builds its own small project: a fixture `f` and an output `out`.

use lab_mapping_model::domain::object_tree::ObjectNodeKind;
use lab_mapping_model::*;

/// A path is its own object: its object's id is the path's id, and it is a
/// leaf holding the path's lamps.
#[test]
fn a_path_is_its_own_object() {
    let (mut p, f) = bare();
    let path = add(&mut p, &f, ComponentKind::path(pts(&[(10.0, 10.0), (50.0, 10.0)]), 5));
    let tree = p.placed().tree(&f);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].id.to_string(), path.to_string());
    assert_eq!(tree[0].leaf.as_ref().unwrap().lamps.len(), 5);
}

/// A circle renders one node, a container, with a leaf per ring under it:
/// `circle1/r1`, `circle1/r2`, made by the circle (generated).
#[test]
fn a_circles_rings_are_objects_under_it() {
    let (mut p, f) = bare();
    let circle = add(&mut p, &f, ComponentKind::circle(Vec2::new(50.0, 50.0), &[(40.0, 24), (20.0, 12)]));
    let node = &p.placed().tree(&f)[0];
    assert_eq!(node.id, ObjectId::of(&circle));
    assert!(!node.is_leaf());
    let rings: Vec<String> = node.children.iter().map(|c| c.id.to_string()).collect();
    assert_eq!(rings, ["circle1/r1", "circle1/r2"]);
    assert!(node.children.iter().all(|c| c.kind == ObjectNodeKind::Generated));
}

/// A repeat holds a component with its own id: copy 0 has the original's
/// ids (`path1`), copy k is `path1@k`.
#[test]
fn a_repeats_copies_carry_the_originals_ids() {
    let (mut p, f) = bare();
    let child = p.mint(ComponentKind::path(pts(&[(50.0, 10.0), (50.0, 30.0)]), 3));
    let repeat = add(&mut p, &f, ComponentKind::repeat(child, Vec2::new(50.0, 50.0), 3));
    let node = &p.placed().tree(&f)[0];
    assert_eq!(node.id, ObjectId::of(&repeat));
    let copies: Vec<String> = node.children.iter().map(|c| c.id.to_string()).collect();
    assert_eq!(copies, ["path1", "path1@1", "path1@2"]);
}

/// A repeated group's copies hold copies of each of its children: copy 2 of
/// the group is `group1@2`, holding `path1@2` and `path2@2`. The group's
/// nesting is never in an id.
#[test]
fn a_repeated_groups_copies_hold_copies_of_its_children() {
    let (mut p, f) = bare();
    let a = p.mint(ComponentKind::path(pts(&[(50.0, 10.0), (50.0, 20.0)]), 2));
    let b = p.mint(ComponentKind::path(pts(&[(55.0, 10.0), (55.0, 20.0)]), 2));
    let group = p.mint(ComponentKind::group(vec![a, b]));
    add(&mut p, &f, ComponentKind::repeat(group, Vec2::new(50.0, 50.0), 3));
    let copy2 = &p.placed().tree(&f)[0].children[2];
    assert_eq!(copy2.id.to_string(), "group1@2");
    let inner: Vec<String> = copy2.children.iter().map(|c| c.id.to_string()).collect();
    assert_eq!(inner, ["path1@2", "path2@2"]);
}

/// Nested repeats stack their copies, innermost first; copy 0 of the outer
/// repeat is dropped from the end, never from the middle: inner copy 1 in
/// outer copy 0 is `@1`, inner copy 0 in outer copy 1 is `@0@1`.
#[test]
fn nested_repeats_stack_copies_inner_first() {
    let (mut p, f) = bare();
    let leaf = p.mint(ComponentKind::path(pts(&[(50.0, 10.0), (50.0, 20.0)]), 2));
    let inner = p.mint(ComponentKind::repeat(leaf, Vec2::new(50.0, 50.0), 2));
    add(&mut p, &f, ComponentKind::repeat(inner, Vec2::new(50.0, 50.0), 2));
    let ids: Vec<String> = p
        .placed_objects()
        .iter()
        .map(|(_, o)| o.id.to_string())
        .collect();
    assert_eq!(ids, ["path1", "path1@1", "path1@0@1", "path1@1@1"]);
}

/// An object id reads back from its text, and `@` can't be part of a
/// component id, so the text is never ambiguous.
#[test]
fn an_object_id_reads_back_from_its_text() {
    for text in ["path1", "circle2/r1", "panel@2", "circle2/r1@2", "path1@0@1"] {
        assert_eq!(ObjectId::parse(text).unwrap().to_string(), text);
    }
    assert!(ObjectId::parse("panel@0").is_none(), "copy 0 is never written");
}

/// A ring with one lamp is one lamp at the centre, whatever its radius: the
/// circular display's centre is its innermost ring.
#[test]
fn a_ring_of_one_lamp_sits_at_the_centre() {
    let (mut p, f) = bare();
    let centre = Vec2::new(50.0, 50.0);
    let circle = add(&mut p, &f, ComponentKind::circle(centre, &[(40.0, 24), (12.0, 1)]));
    let r2 = p.object(&ObjectId::new(&circle, "r2")).unwrap();
    assert_eq!(r2.lamps.len(), 1);
    let origin = p.origin(&f);
    assert!((r2.lamps[0].pos - origin).distance(centre) < 1e-6);
}

/// The leaves in tree order are the order a fixture lists its lamps: a
/// group's children in order, then what follows it.
#[test]
fn the_leaves_in_tree_order_are_the_lamp_order() {
    let (mut p, f) = bare();
    let a = p.mint(ComponentKind::path(pts(&[(10.0, 10.0), (20.0, 10.0)]), 2));
    let b = p.mint(ComponentKind::path(pts(&[(10.0, 20.0), (20.0, 20.0)]), 2));
    add(&mut p, &f, ComponentKind::group(vec![a, b]));
    add(&mut p, &f, ComponentKind::circle(Vec2::new(50.0, 50.0), &[(20.0, 8)]));
    let ids: Vec<String> = p
        .placed_objects()
        .iter()
        .map(|(_, o)| o.id.to_string())
        .collect();
    assert_eq!(ids, ["path1", "path2", "circle1/r1"]);
}

// ---- helpers --------------------------------------------------------------

fn bare() -> (Project, NodeId) {
    let mut p = Project::new("t");
    let f = p.add_fixture(None, "f").unwrap();
    p.add_output(None, "out", &[("D0", 60)]).unwrap();
    (p, f)
}

fn add(p: &mut Project, fixture: &NodeId, kind: ComponentKind) -> ComponentId {
    p.add(fixture, kind).unwrap()
}

fn pts(points: &[(f64, f64)]) -> Vec<Vec2> {
    points.iter().map(|&(x, y)| Vec2::new(x, y)).collect()
}

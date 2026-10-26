//! Statements: the walker (theory "Patching"). One sentence per test; the
//! doc comment is the statement. Worded to carry over to `lpc-mapping` as
//! M1's walker tests: a fixture, its components, an output's ports, and
//! where each lamp lands.
//!
//! Each test builds its own project: a fixture `f`, an output `out` with
//! the ports named (60 pixels each unless it says), RGB lamps (3 channels).

use lab_mapping_model::*;

/// A fixture with no start patches nothing, and says so: nothing flows in
/// from another fixture.
#[test]
fn a_fixture_with_no_start_patches_nothing() {
    let (mut p, f, _) = bare(&[("D0", 60)]);
    path(&mut p, &f, 5);
    let patch = p.effective_patch();
    assert!(patch.pixel_runs.is_empty());
    assert_eq!(patch.problems[0].kind, ProblemKind::NoStart);
}

/// A path at D0 from pixel 0 lands on pixels 0 to n−1.
#[test]
fn a_path_lands_from_its_start() {
    let (mut p, f, out) = bare(&[("D0", 60)]);
    let a = path(&mut p, &f, 5);
    p.start_at(&f, &out, "0", 0);
    assert_eq!(pixels(&p, &a), vec![0, 1, 2, 3, 4]);
}

/// The cursor continues: the next thing starts where the last one ended.
#[test]
fn the_cursor_continues() {
    let (mut p, f, out) = bare(&[("D0", 60)]);
    let _a = path(&mut p, &f, 5);
    let b = path(&mut p, &f, 3);
    p.start_at(&f, &out, "0", 10);
    assert_eq!(pixels(&p, &b), vec![15, 16, 17]);
}

/// Leaving a container doesn't move the cursor: what follows a circle starts
/// where its last ring ended.
#[test]
fn leaving_a_container_doesnt_move_the_cursor() {
    let (mut p, f, out) = bare(&[("D0", 60)]);
    let circle = p
        .add(&f, ComponentKind::circle(Vec2::new(50.0, 50.0), &[(30.0, 8), (15.0, 4)]))
        .unwrap();
    let after = path(&mut p, &f, 2);
    p.start_at(&f, &out, "0", 0);
    let _ = circle;
    assert_eq!(pixels(&p, &after), vec![12, 13]);
}

/// A ring set to D9 at channel 0 takes the rings after it along (D1b: "if I
/// say ring 3 is on d9, ch0 then we assume the rest follow after").
#[test]
fn a_ring_set_elsewhere_takes_the_rest_along() {
    let (mut p, f, out) = bare(&[("D0", 60), ("D9", 60)]);
    let circle = p
        .add(
            &f,
            ComponentKind::circle(Vec2::new(50.0, 50.0), &[(40.0, 6), (30.0, 5), (20.0, 4), (10.0, 3)]),
        )
        .unwrap();
    p.start_at(&f, &out, "0", 0);
    let ring = |k: &str| ObjectId::new(&circle, k);
    p.patch_object(&ring("r3"), &out, "1", 0).unwrap();
    assert_eq!(landing(&p, &ring("r2"), 0), Some(("0".into(), 6)));
    assert_eq!(landing(&p, &ring("r3"), 0), Some(("1".into(), 0)));
    assert_eq!(landing(&p, &ring("r4"), 0), Some(("1".into(), 4)), "follows r3");
}

/// One per port from D1: ring k starts on the k-th port from D1, at its
/// first pixel.
#[test]
fn one_per_port_puts_each_child_on_its_own_port() {
    let (mut p, f, out) = bare(&[("D0", 60), ("D1", 60), ("D2", 60), ("D3", 60)]);
    let circle = p
        .add(
            &f,
            ComponentKind::circle(Vec2::new(50.0, 50.0), &[(40.0, 6), (30.0, 5), (20.0, 4)]),
        )
        .unwrap();
    p.start_at(&f, &out, "0", 0);
    p.set_children(
        &f,
        &PatchTarget::Component(circle.clone()),
        Children::OnePerPort { from: "1".into() },
    );
    for (k, port) in [("r1", "1"), ("r2", "2"), ("r3", "3")] {
        assert_eq!(landing(&p, &ObjectId::new(&circle, k), 0), Some((port.into(), 0)));
    }
}

/// Anchor copy 3 of a one-per-port repeat elsewhere, and copy 4 keeps its
/// port: the slots are absolute, as jacks are physical.
#[test]
fn anchoring_one_copy_leaves_the_others_in_their_slots() {
    let ports: Vec<(String, u32)> = (0..8).map(|k| (format!("D{k}"), 60)).collect();
    let ports: Vec<(&str, u32)> = ports.iter().map(|(n, c)| (n.as_str(), *c)).collect();
    let (mut p, f, out) = bare(&ports);
    let child = p.mint(ComponentKind::path(
        vec![Vec2::new(50.0, 10.0), Vec2::new(50.0, 30.0)],
        10,
    ));
    let arm = child.id.clone();
    let repeat = p
        .add(&f, ComponentKind::repeat(child, Vec2::new(50.0, 50.0), 5))
        .unwrap();
    p.start_at(&f, &out, "0", 0);
    p.set_children(
        &f,
        &PatchTarget::Component(repeat),
        Children::OnePerPort { from: "0".into() },
    );
    let copy = |k: u32| ObjectId::of(&arm).in_copy(k).canonical();
    p.patch_object(&copy(3), &out, "7", 0).unwrap();
    assert_eq!(landing(&p, &copy(3), 0), Some(("7".into(), 0)));
    assert_eq!(landing(&p, &copy(4), 0), Some(("4".into(), 0)), "keeps its slot");
}

/// A leaf that doesn't fit what is left of its port starts on the next
/// port, at its first pixel (universe wrapping).
#[test]
fn a_leaf_that_doesnt_fit_starts_on_the_next_port() {
    let (mut p, f, out) = bare(&[("D0", 10), ("D1", 10)]);
    let _a = path(&mut p, &f, 6);
    let b = path(&mut p, &f, 6);
    p.start_at(&f, &out, "0", 0);
    assert_eq!(landing(&p, &b, 0), Some(("1".into(), 0)));
    let w = p.effective_patch().walked(&b).unwrap();
    assert!(w.wrapped);
}

/// A start that was set and doesn't fit is reported, not moved: the object
/// lands as far as its port reaches.
#[test]
fn a_set_start_that_doesnt_fit_is_reported_not_moved() {
    let (mut p, f, out) = bare(&[("D0", 10), ("D1", 10)]);
    let a = path(&mut p, &f, 6);
    p.start_at(&f, &out, "0", 0);
    p.patch_object(&a, &out, "0", 7).unwrap();
    assert_eq!(landing(&p, &a, 0), Some(("0".into(), 7)));
    assert_eq!(landing(&p, &a, 3), None, "past the port's end");
    assert!(matches!(
        p.effective_patch().problems[0].kind,
        ProblemKind::DoesntFit { .. }
    ));
}

/// Past the last port is an error: it never moves on to another output, and
/// what follows on lands nowhere until something sets a new start.
#[test]
fn past_the_last_port_is_an_error() {
    let (mut p, f, out) = bare(&[("D0", 10)]);
    let _a = path(&mut p, &f, 8);
    let b = path(&mut p, &f, 4);
    let c = path(&mut p, &f, 4);
    p.start_at(&f, &out, "0", 0);
    assert!(!p.effective_patch().lands(&b));
    assert!(!p.effective_patch().lands(&c), "follows on: nowhere");
    assert!(matches!(
        p.effective_patch().problems[0].kind,
        ProblemKind::PastLastPort { .. }
    ));
    p.patch_object(&c, &out, "0", 0).unwrap();
    assert!(p.effective_patch().lands(&c), "a new start lands again");
}

/// Off: an object makes no span, and the next one starts where it would
/// have.
#[test]
fn off_makes_no_span_and_doesnt_move_the_cursor() {
    let (mut p, f, out) = bare(&[("D0", 60)]);
    let a = path(&mut p, &f, 5);
    let b = path(&mut p, &f, 3);
    p.start_at(&f, &out, "0", 0);
    p.edit_override(&a, |o| o.off = true).unwrap();
    assert!(!p.effective_patch().lands(&a));
    assert_eq!(pixels(&p, &b), vec![0, 1, 2]);
}

/// Next port: a component starts on the next port key after where the walk
/// is, at its first pixel, whatever the gaps between the keys.
#[test]
fn next_port_is_the_next_key() {
    let (mut p, f, out) = bare(&[("D0", 60), ("D1", 60), ("D2", 60)]);
    p.output_mut(&out).unwrap().ports.remove(1);
    let _a = path(&mut p, &f, 5);
    let b = path(&mut p, &f, 3);
    p.start_at(&f, &out, "0", 0);
    p.set_start(&f, &PatchTarget::Component(b.component.clone()), Start::NextPort);
    assert_eq!(landing(&p, &b, 0), Some(("2".into(), 0)));
}

/// Values keep the unit they were written in: 6 channels is pixel 2; 4
/// channels is part-way into a pixel, reported, and lands nowhere yet.
#[test]
fn values_keep_their_unit() {
    let (mut p, f, out) = bare(&[("D0", 60)]);
    let a = path(&mut p, &f, 3);
    p.start_at(&f, &out, "0", 0);
    p.edit_override(&a, |o| o.channel = Some(ChannelOp::Set(Amount::ch(6))))
        .unwrap();
    assert_eq!(pixels(&p, &a), vec![2, 3, 4]);
    p.edit_override(&a, |o| o.channel = Some(ChannelOp::Set(Amount::ch(4))))
        .unwrap();
    assert!(!p.effective_patch().lands(&a));
    assert!(matches!(
        p.effective_patch().problems[0].kind,
        ProblemKind::MidPixel { .. }
    ));
}

/// Reversed and offset land as the product's arithmetic does: orientation
/// first, then rotation.
#[test]
fn reversed_and_offset_land_as_the_product_does() {
    let (mut p, f, out) = bare(&[("D0", 60)]);
    let a = path(&mut p, &f, 5);
    p.start_at(&f, &out, "0", 10);
    p.edit_override(&a, |o| {
        o.direction = Some(Direction::Reversed);
        o.offset = Some(2);
    })
    .unwrap();
    assert_eq!(pixels(&p, &a), vec![11, 10, 14, 13, 12]);
}

/// An override naming an object that's gone is a problem: kept, never moved
/// to another object.
#[test]
fn an_override_for_a_gone_object_is_kept_and_reported() {
    let (mut p, f, out) = bare(&[("D0", 60)]);
    let a = path(&mut p, &f, 5);
    p.start_at(&f, &out, "0", 0);
    p.edit_override(&a, |o| o.off = true).unwrap();
    p.fixture_mut(&f).unwrap().remove(&a.component);
    assert!(p.fixture(&f).unwrap().overrides.contains_key(&a));
    assert!(matches!(
        &p.effective_patch().problems[0].kind,
        ProblemKind::GoneObject(o) if *o == a
    ));
}

/// Nothing flows across fixtures: two fixtures on one output each land
/// where their own start says, and one with none lands nowhere.
#[test]
fn nothing_flows_across_fixtures() {
    let (mut p, f, out) = bare(&[("D0", 60)]);
    let a = path(&mut p, &f, 5);
    let g = p.add_fixture(None, "g").unwrap();
    let b = path(&mut p, &g, 3);
    p.start_at(&f, &out, "0", 0);
    assert_eq!(pixels(&p, &a), vec![0, 1, 2, 3, 4]);
    assert!(!p.effective_patch().lands(&b), "g starts nowhere");
    p.start_at(&g, &out, "0", 40);
    assert_eq!(pixels(&p, &b), vec![40, 41, 42]);
}

// ---- helpers --------------------------------------------------------------

fn bare(ports: &[(&str, u32)]) -> (Project, NodeId, NodeId) {
    let mut p = Project::new("t");
    let f = p.add_fixture(None, "f").unwrap();
    let out = p.add_output(None, "out", ports).unwrap();
    (p, f, out)
}

/// A straight path of `n` lamps in `f`: its object.
fn path(p: &mut Project, f: &NodeId, n: u32) -> ObjectId {
    let k = p.fixture(f).unwrap().components.len() as f64;
    let y = 10.0 + 5.0 * k;
    let id = p
        .add(f, ComponentKind::path(vec![Vec2::new(10.0, y), Vec2::new(80.0, y)], n))
        .unwrap();
    ObjectId::of(&id)
}

fn landing(p: &Project, object: &ObjectId, lamp: u32) -> Option<(String, u32)> {
    p.effective_patch()
        .landing(object, lamp)
        .map(|l| (l.port, l.pixel))
}

fn pixels(p: &Project, object: &ObjectId) -> Vec<u32> {
    let n = p.object(object).unwrap().lamps.len() as u32;
    (0..n)
        .map(|j| landing(p, object, j).expect("on the wire").1)
        .collect()
}

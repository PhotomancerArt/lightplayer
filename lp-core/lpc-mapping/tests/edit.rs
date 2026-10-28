//! Statements: edits to a mapping, and the rules for the ones that rewrite
//! references (plan D6, D12). One sentence per test; the doc comment is the
//! statement.
//!
//! "Every lamp lands where it did" means: patching before and after the
//! edit gives every lamp of every segment the same landing
//! ([`Patching::landing`]), each old segment id mapped to its new one, and
//! no segment more or fewer. It is checked on the four journeys' mappings
//! (`support/journey_mappings.rs`) wherever the edit applies to them.

#[path = "support/journey_mappings.rs"]
mod journey_mappings;

use journey_mappings::{Journey, choker, circular_display, dome_slice, eaves, journeys};
use lpc_mapping::edit::{
    DrawnFrom, EditError, OverrideField, OverrideFieldKind, add_part, add_ring,
    clear_override_field, expand_repeat, insert_part, move_part, overrides_past_count, remove_part,
    remove_ring, rename_part, set_circle_split, set_drawn_from, set_flow, set_lamp_count,
    set_override_field, set_path_split, set_repeat_count, set_start, unrepeat, wrap_in_repeat,
};
use lpc_mapping::part::{
    Circle, CircleSplit, Mapping, Part, PartId, PartKind, Path, PathCommand, PathSplit, Repeat,
    SegmentKey, Shape, TreeId, render,
};
use lpc_mapping::patching::{
    Amount, ChannelOp, Direction, Flow, OutputPorts, Override, PatchTarget, Patching, PortKey,
    PortOp, Problem, Start, patch_mapping,
};
use lpc_mapping::{GridShape, MAX_REPEAT_COUNT, map2d6};

/// Inserting a part refuses an id in use, and a place past the end.
#[test]
fn inserting_refuses_an_id_in_use() {
    let mut m = choker().mapping;
    assert_eq!(
        insert_part(&mut m, None, 0, Part::new(part("u"), straight(2))),
        Err(EditError::IdTaken(part("u")))
    );
    assert_eq!(
        insert_part(&mut m, None, 13, Part::new(part("v"), straight(2))),
        Err(EditError::IndexOutOfRange { index: 13, len: 12 })
    );
    assert_eq!(
        insert_part(
            &mut m,
            Some(&part("u")),
            0,
            Part::new(part("v"), straight(2))
        ),
        Err(EditError::NotAGroup(part("u")))
    );
    insert_part(&mut m, None, 12, Part::new(part("path7"), straight(2))).unwrap();
    assert_eq!(
        m.new_part(straight(2)).id,
        part("path8"),
        "never minted again"
    );
}

/// Removing a part keeps its overrides; the patcher reports each as gone,
/// and putting the part back finds them again.
#[test]
fn removing_a_part_keeps_its_overrides() {
    let journey = circular_display();
    let mut m = journey.mapping.clone();
    let removed = remove_part(&mut m, &part("circle1")).unwrap();
    assert_eq!(removed.kept_overrides, 1);
    assert!(m.overrides.contains_key(&node("circle1/r3")));
    let p = patch(&m, &journey.outputs);
    assert!(p.problems.contains(&Problem::GoneNode(node("circle1/r3"))));

    insert_part(&mut m, removed.parent.as_ref(), removed.index, removed.part).unwrap();
    lands_where_it_did(&journey, &m, Clone::clone);
}

/// Moving a part changes where its segments come, never its ids: its
/// overrides go with it.
#[test]
fn moving_a_part_changes_where_its_lamps_come() {
    let journey = choker();
    let mut m = journey.mapping.clone();
    move_part(&mut m, &part("l2"), None, 0).unwrap();
    let p = patch(&m, &journey.outputs);
    assert_eq!(first_lamp(&p, "l2"), Some((0, 0)));
    assert_eq!(first_lamp(&p, "p"), Some((0, 8)));
    assert_eq!(
        first_lamp(&p, "f-lower"),
        Some((0, 60)),
        "its override went too"
    );

    assert_eq!(
        move_part(&mut m, &part("l2"), None, 12),
        Err(EditError::IndexOutOfRange { index: 12, len: 11 })
    );
    assert_eq!(
        move_part(&mut m, &part("l2"), Some(&part("p")), 0),
        Err(EditError::NotAGroup(part("p")))
    );
}

/// Moving a part out of the repeat round it is refused while an override
/// names one of its copies: it would name a different node.
#[test]
fn moving_a_part_out_of_its_repeat_keeps_its_copies_overrides_honest() {
    let mut m = Mapping::new();
    let a = m.new_part(straight(3));
    let b = m.new_part(straight(3));
    let group = m.new_part(PartKind::Group { parts: vec![a, b] });
    let repeat = m.new_part(PartKind::Repeat(Repeat::new(group, [0.0, 0.0], 3)));
    m.parts.push(repeat);
    m.overrides.insert(node("path2@2"), reversed());
    m.overrides.insert(node("path2"), reversed());

    let before = m.clone();
    assert_eq!(
        move_part(&mut m, &part("path2"), None, 1),
        Err(EditError::WouldRetarget {
            part: part("path2"),
            overrides: vec![node("path2@2")],
        })
    );
    assert_eq!(m, before);
    assert_eq!(
        move_part(&mut m, &part("group1"), None, 0),
        Err(EditError::HeldByRepeat(part("group1")))
    );
    assert_eq!(
        remove_part(&mut m, &part("group1")),
        Err(EditError::HeldByRepeat(part("group1")))
    );

    m.overrides.remove(&node("path2@2"));
    move_part(&mut m, &part("path2"), None, 1).unwrap();
    assert!(
        m.overrides.contains_key(&node("path2")),
        "its own copy's went with it"
    );
}

/// A part can't go inside itself.
#[test]
fn a_part_cant_move_into_itself() {
    let mut m = Mapping::new();
    let inner = m.new_part(PartKind::Group { parts: Vec::new() });
    let outer = m.new_part(PartKind::Group { parts: vec![inner] });
    m.parts.push(outer);
    assert_eq!(
        move_part(&mut m, &part("group2"), Some(&part("group1")), 0),
        Err(EditError::IntoItself(part("group2")))
    );
}

/// Renaming a part moves its overrides, ring keys and copies included, and
/// every lamp lands where it did.
#[test]
fn renaming_a_part_moves_its_overrides() {
    for (journey, old, new, moved) in [
        (choker(), "f-lower", "f-low", "f-low"),
        (circular_display(), "circle1", "display", "display/r3"),
        (dome_slice(), "path1", "arm", "arm@3"),
        (eaves(), "front", "front-run", "front-run"),
    ] {
        let mut m = journey.mapping.clone();
        rename_part(&mut m, &part(old), part(new)).unwrap();
        assert!(m.part(&part(old)).is_none(), "{}", journey.name);
        assert!(m.overrides.contains_key(&node(moved)), "{}", journey.name);
        assert_eq!(m.overrides.len(), journey.mapping.overrides.len());
        lands_where_it_did(&journey, &m, |id| {
            if id.part == part(old) {
                TreeId {
                    part: part(new),
                    ..id.clone()
                }
            } else {
                id.clone()
            }
        });
    }
}

/// A part can't take an id another part has, or one used before: minted
/// already, or still named by a removed part's override.
#[test]
fn a_part_cant_take_an_id_another_part_has_or_one_used_before() {
    let mut m = Mapping::new();
    let (path1, path2) = (m.new_part(straight(5)), m.new_part(straight(5)));
    let (a, b) = (path1.id.clone(), path2.id.clone());
    m.parts.push(path1);
    m.parts.push(path2);
    assert_eq!(
        rename_part(&mut m, &a, b.clone()),
        Err(EditError::IdTaken(b.clone()))
    );

    // path3 was minted and removed: it is never a part's again.
    let c = m.new_part(straight(5));
    let c_id = c.id.clone();
    insert_part(&mut m, None, 2, c).unwrap();
    remove_part(&mut m, &c_id).unwrap();
    assert_eq!(c_id, part("path3"));
    assert_eq!(
        rename_part(&mut m, &a, c_id.clone()),
        Err(EditError::IdUsedBefore(c_id))
    );

    // `front` is gone, but an override still names it.
    let mut eaves = eaves().mapping;
    remove_part(&mut eaves, &part("front")).unwrap();
    let before = eaves.clone();
    assert_eq!(
        rename_part(&mut eaves, &part("side"), part("front")),
        Err(EditError::IdUsedBefore(part("front")))
    );
    assert_eq!(eaves, before, "a refused edit changes nothing");
}

/// Wrapping a part in a repeat keeps copy 0 where the part was: wrapped in
/// a repeat of one copy, every lamp of every journey lands where it did
/// (inside a repeat already, `path1@3` becomes `path1@0@3`).
#[test]
fn wrapping_a_part_in_a_repeat_keeps_copy_0_where_the_part_was() {
    for (journey, wrapped) in [
        (choker(), "f-lower"),
        (circular_display(), "circle1"),
        (dome_slice(), "path1"),
        (eaves(), "front"),
    ] {
        let mut m = journey.mapping.clone();
        let repeat = wrap_in_repeat(&mut m, &part(wrapped), 1, [0.0, 0.0]).unwrap();
        assert!(repeat.as_str().starts_with("repeat"), "{}", journey.name);
        let nested = journey.name == "dome slice";
        lands_where_it_did(&journey, &m, |id| {
            if nested && id.part == part(wrapped) {
                let mut copies = vec![0];
                copies.extend(&id.copies);
                TreeId {
                    copies,
                    ..id.clone()
                }
                .canonical()
            } else {
                id.clone()
            }
        });
    }
    let mut m = dome_slice().mapping;
    wrap_in_repeat(&mut m, &part("path1"), 1, [0.0, 0.0]).unwrap();
    assert!(m.overrides.contains_key(&node("path1@0@3")));
}

/// Wrapped in a repeat of three, copy 0 is where the part was and the
/// copies follow it end to end: the repeat takes the part's own start, so
/// they don't all start where the part did.
#[test]
fn a_wrapped_parts_copies_follow_copy_0() {
    let mut m = Mapping::new();
    let mut a = m.new_part(straight(4));
    a.start = Start::at("out", 1, Amount::lamps(2));
    m.parts.push(a);
    let outputs = [OutputPorts::of_lamps("out", &[Some(60); 2], 3)];
    assert_eq!(first_lamp(&patch(&m, &outputs), "path1"), Some((1, 2)));

    wrap_in_repeat(&mut m, &part("path1"), 3, [0.0, 0.0]).unwrap();
    let p = patch(&m, &outputs);
    assert_eq!(first_lamp(&p, "path1"), Some((1, 2)));
    assert_eq!(first_lamp(&p, "path1@1"), Some((1, 6)));
    assert_eq!(first_lamp(&p, "path1@2"), Some((1, 10)));
    assert!(p.problems.is_empty(), "{:?}", p.problems);
}

/// Where a part's own override adds to the cursor, wrapping it doesn't add
/// the flow's step too: copy 0 still lands where the part did.
#[test]
fn wrapping_keeps_an_override_that_adds() {
    let mut m = Mapping::new();
    for _ in 0..2 {
        let p = m.new_part(straight(4));
        m.parts.push(p);
    }
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.flow = Flow::OnePerPort { from: PortKey(0) };
    m.overrides.insert(
        node("path2"),
        Override {
            port: Some(PortOp::Add(2)),
            ..Override::default()
        },
    );
    let journey = Journey {
        name: "adds",
        mapping: m.clone(),
        outputs: vec![OutputPorts::of_lamps("out", &[Some(60); 4], 3)],
    };
    assert_eq!(
        first_lamp(&patch(&m, &journey.outputs), "path2"),
        Some((2, 0))
    );
    wrap_in_repeat(&mut m, &part("path2"), 1, [0.0, 0.0]).unwrap();
    lands_where_it_did(&journey, &m, Clone::clone);
}

/// Wrapping refuses a repeat of no copies.
#[test]
fn wrapping_refuses_a_repeat_of_no_copies() {
    let mut m = eaves().mapping;
    assert_eq!(
        wrap_in_repeat(&mut m, &part("front"), 0, [0.0, 0.0]),
        Err(EditError::BadCount(0))
    );
}

/// Unrepeating keeps copy 0 where it was, with the step the repeat gave it,
/// and keeps the overrides of the other copies, reported as gone.
#[test]
fn unrepeating_keeps_copy_0_and_the_copies_overrides() {
    let journey = dome_slice();
    let mut m = journey.mapping.clone();
    m.parts[0].flow = Flow::OnePerPort { from: PortKey(2) };
    let outputs = &journey.outputs;
    assert_eq!(first_lamp(&patch(&m, outputs), "path1"), Some((2, 0)));

    assert_eq!(unrepeat(&mut m, &part("sector")), Ok(1));
    assert_eq!(m.parts[0].id, part("path1"));
    let p = patch(&m, outputs);
    assert_eq!(first_lamp(&p, "path1"), Some((2, 0)), "the flow's step");
    assert!(m.overrides.contains_key(&node("path1@3")));
    assert!(p.problems.contains(&Problem::GoneNode(node("path1@3"))));
}

/// Unrepeating a repeat inside a repeat is refused when an override on one
/// of its copies would come to name a copy of the outer repeat.
#[test]
fn unrepeating_refuses_to_retarget_a_copys_override() {
    let mut m = wheel_of_spokes().mapping;
    let before = m.clone();
    assert!(matches!(
        unrepeat(&mut m, &part("spoke")),
        Err(EditError::WouldRetarget { .. })
    ));
    assert_eq!(m, before);

    // Without the override on inner copy 1 of outer copy 0 (`arm@1`), the
    // others are carried (`arm@0@2` → `arm@2`) or stay gone (`arm@2@3`).
    m.overrides.remove(&node("arm@1"));
    assert_eq!(unrepeat(&mut m, &part("spoke")), Ok(1));
    assert!(m.overrides.contains_key(&node("arm@2")));
    assert!(m.overrides.contains_key(&node("arm@2@3")));
}

/// Expanding a repeat makes copy k a part `x-k`, and every override on
/// `x@k` now names `x-k`; every lamp lands where it did, and stays where it
/// was drawn.
#[test]
fn expanding_a_repeat_makes_each_copy_a_part() {
    let journey = dome_slice();
    let mut m = journey.mapping.clone();
    let copies = expand_repeat(&mut m, &part("sector")).unwrap();
    let ids: Vec<&str> = copies.iter().map(PartId::as_str).collect();
    assert_eq!(ids, ["path1", "path1-1", "path1-2", "path1-3", "path1-4"]);
    assert!(matches!(m.parts[0].kind, PartKind::Group { .. }));
    assert_eq!(
        m.parts[0].id,
        part("sector"),
        "the group keeps the repeat's id"
    );
    assert!(m.overrides.contains_key(&node("path1-3")));
    assert!(!m.overrides.contains_key(&node("path1@3")));
    lands_where_it_did(&journey, &m, |id| match id.copies.as_slice() {
        [] => id.clone(),
        [k] => node(&format!("{}-{k}", id.part)),
        _ => unreachable!(),
    });
    drawn_where_it_was(&journey.mapping, &m, 1e-3, |id| {
        match id.copies.as_slice() {
            [] => id.clone(),
            [k] => node(&format!("{}-{k}", id.part)),
            _ => unreachable!(),
        }
    });
}

/// A grid can't carry a turn: expanding a repeat of one makes each copy
/// points at exactly the lamps the repeat drew.
#[test]
fn expanding_a_repeat_of_a_grid_keeps_every_lamp_exactly() {
    let mut m = Mapping::new();
    let grid = m.new_part(PartKind::Shape(Shape::Grid(GridShape {
        origin: [10.0, 10.0],
        cols: 3,
        rows: 2,
        pitch: 4.0,
        routing: Default::default(),
        start_corner: Default::default(),
    })));
    let repeat = m.new_part(PartKind::Repeat(Repeat::new(grid, [0.0, 0.0], 3)));
    m.parts.push(repeat);
    let before = m.clone();
    expand_repeat(&mut m, &part("repeat1")).unwrap();
    assert!(matches!(
        m.part(&part("grid1-2")).unwrap().kind,
        PartKind::Shape(Shape::Points(_))
    ));
    drawn_where_it_was(&before, &m, 0.0, |id| match id.copies.as_slice() {
        [] => id.clone(),
        [k] => node(&format!("{}-{k}", id.part)),
        _ => unreachable!(),
    });
}

/// Expanding a repeat that holds a repeat rewrites the inner copies first,
/// as ids list them: expanding the outer, `arm@2@3` (inner copy 2 of outer
/// copy 3) becomes `arm-3@2`; expanding the inner, `arm-2@3`. Every lamp
/// lands where it did either way.
#[test]
fn expanding_a_nested_repeat_rewrites_inner_copies_first() {
    let journey = wheel_of_spokes();

    let mut m = journey.mapping.clone();
    expand_repeat(&mut m, &part("wheel")).unwrap();
    assert!(m.overrides.contains_key(&node("arm-3@2")));
    assert!(
        m.overrides.contains_key(&node("arm@1")),
        "outer copy 0 stays"
    );
    assert!(m.overrides.contains_key(&node("arm-2")), "inner copy 0");
    assert!(
        m.part(&part("spoke-3")).is_some(),
        "the inner repeat is copied"
    );
    lands_where_it_did(&journey, &m, |id| {
        let [inner, outer] = padded(id);
        let name = if outer == 0 {
            id.part.to_string()
        } else {
            format!("{}-{outer}", id.part)
        };
        TreeId {
            part: part(&name),
            key: None,
            copies: vec![inner],
        }
        .canonical()
    });

    let mut m = journey.mapping.clone();
    expand_repeat(&mut m, &part("spoke")).unwrap();
    assert!(m.overrides.contains_key(&node("arm-2@3")));
    assert!(m.overrides.contains_key(&node("arm-1")));
    assert!(m.overrides.contains_key(&node("arm@2")), "inner copy 0");
    lands_where_it_did(&journey, &m, |id| {
        let [inner, outer] = padded(id);
        let name = if inner == 0 {
            id.part.to_string()
        } else {
            format!("{}-{inner}", id.part)
        };
        TreeId {
            part: part(&name),
            key: None,
            copies: vec![outer],
        }
        .canonical()
    });
}

/// Expanding refuses a copy id another part has, and changes nothing.
#[test]
fn expanding_refuses_a_copy_id_in_use() {
    let mut m = dome_slice().mapping;
    m.parts.push(Part::new(part("path1-2"), straight(4)));
    let before = m.clone();
    assert_eq!(
        expand_repeat(&mut m, &part("sector")),
        Err(EditError::IdTaken(part("path1-2")))
    );
    assert_eq!(m, before);
}

/// Counting a repeat down keeps the overrides of the copies it drops (the
/// patcher reports them); counting it back up finds them again.
#[test]
fn counting_a_repeat_down_keeps_its_dropped_copies_overrides() {
    let journey = dome_slice();
    let mut m = journey.mapping.clone();
    assert_eq!(overrides_past_count(&m, &part("sector"), 4), 0);
    assert_eq!(overrides_past_count(&m, &part("sector"), 3), 1);
    set_repeat_count(&mut m, &part("sector"), 3).unwrap();
    assert!(m.overrides.contains_key(&node("path1@3")));
    let p = patch(&m, &journey.outputs);
    assert!(p.problems.contains(&Problem::GoneNode(node("path1@3"))));

    set_repeat_count(&mut m, &part("sector"), 5).unwrap();
    lands_where_it_did(&journey, &m, Clone::clone);

    assert_eq!(
        set_repeat_count(&mut m, &part("sector"), 0),
        Err(EditError::BadCount(0))
    );
    assert_eq!(
        set_repeat_count(&mut m, &part("sector"), MAX_REPEAT_COUNT + 1),
        Err(EditError::BadCount(MAX_REPEAT_COUNT + 1))
    );
}

/// A ring added to a circle gets a key never used before, even after one
/// was removed, and goes where its radius puts it; the removed ring's
/// override is kept and reported.
#[test]
fn a_ring_added_gets_a_key_never_used_before() {
    let journey = circular_display();
    let mut m = journey.mapping.clone();
    let circle = part("circle1");
    let gone = remove_ring(&mut m, &circle, &key("r3")).unwrap();
    assert_eq!(gone.key, key("r3"));
    assert_eq!(add_ring(&mut m, &circle, 5.0, 4), Ok(key("r6")));
    remove_ring(&mut m, &circle, &key("r6")).unwrap();
    assert_eq!(add_ring(&mut m, &circle, 5.0, 4), Ok(key("r7")));
    let order = |m: &Mapping| -> Vec<String> {
        let PartKind::Shape(Shape::Circle(c)) = &m.part(&circle).unwrap().kind else {
            panic!("a circle");
        };
        c.rings.iter().map(|r| r.key.to_string()).collect()
    };
    assert_eq!(
        order(&m),
        ["r1", "r2", "r4", "r7", "r5"],
        "listed outside in: it goes where its radius puts it"
    );
    assert_eq!(add_ring(&mut m, &circle, 50.0, 30), Ok(key("r8")));
    assert_eq!(order(&m)[0], "r8", "a larger ring goes first");
    let p = patch(&m, &journey.outputs);
    assert!(p.problems.contains(&Problem::GoneNode(node("circle1/r3"))));
    assert!(p.segment(&node("circle1/r7")).is_some());

    assert_eq!(
        remove_ring(&mut m, &circle, &key("r3")),
        Err(EditError::NoSuchRing {
            part: circle.clone(),
            key: key("r3")
        })
    );
    assert!(matches!(
        add_ring(&mut m, &circle, 0.0, 2),
        Err(EditError::BadRing(_))
    ));
    assert_eq!(
        add_ring(&mut m, &part("nope"), 1.0, 2),
        Err(EditError::NoSuchPart(part("nope")))
    );
}

/// Splitting a path into its subpaths finds the overrides on them again;
/// making it whole leaves them kept and reported.
#[test]
fn splitting_a_path_finds_its_subpaths_overrides() {
    let journey = eaves();
    let mut m = journey.mapping.clone();
    m.overrides.insert(node("side/s2"), reversed());
    assert!(
        patch(&m, &journey.outputs)
            .problems
            .contains(&Problem::GoneNode(node("side/s2")))
    );
    set_path_split(&mut m, &part("side"), PathSplit::Subpaths).unwrap();
    let p = patch(&m, &journey.outputs);
    assert!(p.problems.is_empty(), "{:?}", p.problems);
    assert!(p.segment(&node("side/s2")).unwrap().direction.is_reverse());
    assert_eq!(
        set_path_split(&mut m, &part("sector"), PathSplit::One),
        Err(EditError::NoSuchPart(part("sector")))
    );
}

/// A new part never takes an id a kept override names: minted that way, it
/// would quietly pick the override up.
#[test]
fn a_new_part_never_takes_an_id_a_kept_override_names() {
    let journey = circular_display();
    let mut m = journey.mapping.clone();
    // An override left on a ring of a `circle2` the counter never passed
    // (a hand-authored file: the reader checks counters against parts only).
    m.overrides.insert(node("circle2/r1"), reversed());
    let circle = || PartKind::Shape(Shape::Circle(Circle::new([10.0, 10.0], &[(5.0, 4)])));
    assert_eq!(m.clone().new_part(circle()).id, part("circle2"), "the trap");

    let id = add_part(&mut m, None, 1, circle()).unwrap();
    assert_eq!(id, part("circle3"));
    assert_eq!(m.parts[1].id, id, "at the place asked for");
    let p = patch(&m, &journey.outputs);
    assert!(p.problems.contains(&Problem::GoneNode(node("circle2/r1"))));
    assert!(
        !p.segment(&node("circle3/r1"))
            .unwrap()
            .direction
            .is_reverse()
    );
    assert_eq!(m.clone().new_part(circle()).id, part("circle4"), "counted");

    let next = m.next.clone();
    assert_eq!(
        add_part(&mut m, Some(&part("circle1")), 0, circle()),
        Err(EditError::NotAGroup(part("circle1")))
    );
    assert_eq!(m.next, next, "a refused part mints nothing");
}

/// A circle made one segment loses its rings' overrides to "gone", and
/// split into rings again finds them: every lamp lands where it did.
#[test]
fn a_circle_made_one_segment_finds_its_rings_overrides_again() {
    let journey = circular_display();
    let mut m = journey.mapping.clone();
    set_circle_split(&mut m, &part("circle1"), CircleSplit::One).unwrap();
    let p = patch(&m, &journey.outputs);
    assert_eq!(p.segment(&node("circle1")).unwrap().lamps, 61);
    assert!(p.problems.contains(&Problem::GoneNode(node("circle1/r3"))));

    set_circle_split(&mut m, &part("circle1"), CircleSplit::Rings).unwrap();
    lands_where_it_did(&journey, &m, Clone::clone);
    assert_eq!(
        set_circle_split(&mut choker().mapping, &part("u"), CircleSplit::One),
        Err(EditError::NotACircle(part("u")))
    );
}

/// A lamp count is a path's, a polygon's or one ring's: the lamps are
/// spread again, and ids and overrides stay.
#[test]
fn a_lamp_count_is_a_paths_or_one_rings() {
    let journey = circular_display();
    let mut m = journey.mapping.clone();
    let circle = part("circle1");
    set_lamp_count(&mut m, &circle, Some(&key("r1")), 30).unwrap();
    let p = patch(&m, &journey.outputs);
    assert_eq!(p.segment(&node("circle1/r1")).unwrap().lamps, 30);
    assert_eq!(
        first_lamp(&p, "circle1/r2"),
        Some((0, 30)),
        "ring 2 follows"
    );
    assert_eq!(m.overrides, journey.mapping.overrides);
    assert!(matches!(
        set_lamp_count(&mut m, &circle, Some(&key("r5")), 2),
        Err(EditError::BadRing(_))
    ));
    assert_eq!(
        set_lamp_count(&mut m, &circle, None, 9),
        Err(EditError::NoLampCount(circle.clone()))
    );
    assert_eq!(
        set_lamp_count(&mut m, &circle, Some(&key("r1")), 0),
        Err(EditError::NoLamps(circle.clone()))
    );

    let mut choker = choker().mapping;
    set_lamp_count(&mut choker, &part("u"), None, 9).unwrap();
    let tree = render(&choker).unwrap();
    assert_eq!(tree.segment(&node("u")).unwrap().lamps.len(), 9);
}

/// Drawn from the inside, a circle's rings come inner first, each keeping
/// its key and its override; a path drawn from its last point starts there.
/// "Drawn from" is layout: no override changes.
#[test]
fn drawn_from_orders_a_circles_rings_and_starts_a_path() {
    let journey = circular_display();
    let mut m = journey.mapping.clone();
    set_drawn_from(&mut m, &part("circle1"), DrawnFrom::Inside).unwrap();
    let order: Vec<String> = render(&m)
        .unwrap()
        .segments()
        .iter()
        .map(|s| s.id.to_string())
        .collect();
    assert_eq!(
        order,
        [
            "circle1/r5",
            "circle1/r4",
            "circle1/r3",
            "circle1/r2",
            "circle1/r1"
        ]
    );
    assert_eq!(m.overrides, journey.mapping.overrides);
    let p = patch(&m, &journey.outputs);
    assert!(
        p.segment(&node("circle1/r3"))
            .unwrap()
            .direction
            .is_reverse()
    );
    set_drawn_from(&mut m, &part("circle1"), DrawnFrom::Outside).unwrap();
    lands_where_it_did(&journey, &m, Clone::clone);

    let mut choker = choker().mapping;
    let first = |m: &Mapping| {
        let tree = render(m).unwrap();
        tree.lamps_of(tree.segment(&node("u")).unwrap())[0]
    };
    let from_first = first(&choker);
    set_drawn_from(&mut choker, &part("u"), DrawnFrom::LastPoint).unwrap();
    assert_eq!(from_first, [100.0, 0.0]);
    assert_eq!(first(&choker), [100.0, 20.0], "lamp 0 at its last point");
    assert_eq!(
        set_drawn_from(&mut choker, &part("u"), DrawnFrom::Inside),
        Err(EditError::NotACircle(part("u")))
    );
    assert_eq!(
        set_drawn_from(&mut m, &part("circle1"), DrawnFrom::LastPoint),
        Err(EditError::NotAPath(part("circle1")))
    );
}

/// Clearing the last field of an override removes it.
#[test]
fn clearing_the_last_field_of_an_override_removes_it() {
    let mut m = eaves().mapping;
    let side = node("side");
    set_override_field(&mut m, &side, OverrideField::Direction(Direction::Reverse)).unwrap();
    set_override_field(&mut m, &side, OverrideField::Port(PortOp::set(0))).unwrap();
    clear_override_field(&mut m, &side, OverrideFieldKind::Port).unwrap();
    assert_eq!(m.overrides[&side], reversed());
    clear_override_field(&mut m, &side, OverrideFieldKind::Direction).unwrap();
    assert!(!m.overrides.contains_key(&side));

    set_override_field(&mut m, &side, OverrideField::Off(false)).unwrap();
    assert!(
        !m.overrides.contains_key(&side),
        "nothing set is no override"
    );
    assert_eq!(
        set_override_field(&mut m, &node("nope"), OverrideField::Offset(1)),
        Err(EditError::NoSuchPart(part("nope")))
    );
}

/// Setting a start on a part applies to every copy; setting one on a
/// copy's node applies to that copy, and setting all Keep there takes it
/// back to the flow.
#[test]
fn a_start_on_a_part_applies_to_every_copy_and_on_a_copy_to_that_copy() {
    let mut m = Mapping::new();
    let arm = m.new_part(straight(10));
    let repeat = m.new_part(PartKind::Repeat(Repeat::new(arm, [0.0, 0.0], 3)));
    m.parts.push(repeat);
    set_start(
        &mut m,
        PatchTarget::Fixture,
        Start::at("out", 0, Amount::lamps(0)),
    )
    .unwrap();
    let outputs = [OutputPorts::of_lamps("out", &[Some(60); 8], 3)];

    set_start(&mut m, PatchTarget::Part(part("path1")), Start::next_port()).unwrap();
    let p = patch(&m, &outputs);
    for (copy, port) in [("path1", 1), ("path1@1", 2), ("path1@2", 3)] {
        assert_eq!(first_lamp(&p, copy), Some((port, 0)), "{copy}");
    }

    let copy1 = node("path1@1");
    set_start(
        &mut m,
        PatchTarget::Node(copy1.clone()),
        Start::at("out", 5, Amount::lamps(0)),
    )
    .unwrap();
    let p = patch(&m, &outputs);
    assert_eq!(first_lamp(&p, "path1"), Some((1, 0)));
    assert_eq!(first_lamp(&p, "path1@1"), Some((5, 0)));
    assert_eq!(first_lamp(&p, "path1@2"), Some((6, 0)), "follows copy 1");

    set_start(&mut m, PatchTarget::Node(copy1.clone()), Start::KEEP).unwrap();
    assert!(!m.overrides.contains_key(&copy1));
    assert_eq!(first_lamp(&patch(&m, &outputs), "path1@1"), Some((2, 0)));
}

/// A flow is set on the fixture or a part, never on one node of it.
#[test]
fn a_flow_belongs_to_a_part() {
    let journey = circular_display();
    let mut m = journey.mapping.clone();
    m.overrides.clear();
    set_flow(
        &mut m,
        PatchTarget::Part(part("circle1")),
        Flow::OnePerPort { from: PortKey(0) },
    )
    .unwrap();
    let p = patch(&m, &journey.outputs);
    assert_eq!(first_lamp(&p, "circle1/r4"), Some((3, 0)));
    assert_eq!(
        set_flow(
            &mut m,
            PatchTarget::Node(node("circle1/r4")),
            Flow::EndToEnd
        ),
        Err(EditError::FlowOnNode(node("circle1/r4")))
    );
}

/// What the edits make saves as map2d format 6 and reads back the same:
/// renamed parts, a wrapped part (with the repeat's explicit Keep), an
/// expanded repeat (its copies turned, a grid baked to points), an unrepeat,
/// rings added and removed, and kept overrides of gone nodes.
#[test]
fn what_the_edits_make_saves_and_reads_back() {
    for journey in journeys() {
        let mut m = journey.mapping.clone();
        let first = m.parts[0].id.clone();
        let repeat = wrap_in_repeat(&mut m, &first, 3, [5.0, 5.0]).unwrap();
        round_trips(&m, journey.name);
        expand_repeat(&mut m, &repeat).unwrap();
        round_trips(&m, journey.name);
        rename_part(&mut m, &first, part("renamed")).unwrap();
        round_trips(&m, journey.name);
    }

    let mut m = dome_slice().mapping;
    unrepeat(&mut m, &part("sector")).unwrap();
    round_trips(&m, "unrepeated dome");

    let mut m = circular_display().mapping;
    remove_ring(&mut m, &part("circle1"), &key("r3")).unwrap();
    add_ring(&mut m, &part("circle1"), 5.0, 4).unwrap();
    round_trips(&m, "display");

    let mut m = Mapping::new();
    for _ in 0..2 {
        let p = m.new_part(straight(4));
        m.parts.push(p);
    }
    m.overrides.insert(
        node("path2"),
        Override {
            port: Some(PortOp::Add(2)),
            ..Override::default()
        },
    );
    wrap_in_repeat(&mut m, &part("path2"), 1, [0.0, 0.0]).unwrap();
    assert_eq!(m.overrides[&node("repeat1")].port, Some(PortOp::Keep));
    round_trips(&m, "explicit keep");

    let mut m = Mapping::new();
    let grid = m.new_part(PartKind::Shape(Shape::Grid(GridShape {
        origin: [10.0, 10.0],
        cols: 3,
        rows: 2,
        pitch: 4.0,
        routing: Default::default(),
        start_corner: Default::default(),
    })));
    let repeat = m.new_part(PartKind::Repeat(Repeat::new(grid, [0.0, 0.0], 3)));
    m.parts.push(repeat);
    expand_repeat(&mut m, &part("repeat1")).unwrap();
    round_trips(&m, "baked grid");
}

/// Every journey still patches as it did when nothing is edited: the
/// statements' oracle agrees with itself.
#[test]
fn the_oracle_agrees_with_itself() {
    for journey in journeys() {
        lands_where_it_did(&journey, &journey.mapping, Clone::clone);
    }
}

// ---- helpers --------------------------------------------------------------

fn patch(m: &Mapping, outputs: &[OutputPorts]) -> Patching {
    patch_mapping(m, &render(m).unwrap(), outputs)
}

/// Every lamp of every segment of the journey lands, after the edit, where
/// it did, through `new_id`; and there are as many segments as before.
fn lands_where_it_did(journey: &Journey, after: &Mapping, new_id: impl Fn(&TreeId) -> TreeId) {
    let before_tree = render(&journey.mapping).unwrap();
    let after_tree = render(after).unwrap();
    let before = patch_mapping(&journey.mapping, &before_tree, &journey.outputs);
    let after_patch = patch_mapping(after, &after_tree, &journey.outputs);
    assert_eq!(
        before_tree.segments().len(),
        after_tree.segments().len(),
        "{}: segments",
        journey.name
    );
    for segment in before_tree.segments() {
        let new = new_id(&segment.id);
        let moved = after_tree.segment(&new).unwrap_or_else(|| {
            panic!(
                "{}: {} became {new}, which isn't there",
                journey.name, segment.id
            )
        });
        assert_eq!(
            moved.lamps.len(),
            segment.lamps.len(),
            "{}: {new}",
            journey.name
        );
        for lamp in 0..segment.lamps.len() as u32 {
            assert_eq!(
                after_patch.landing(&new, lamp),
                before.landing(&segment.id, lamp),
                "{}: lamp {lamp} of {} (now {new})",
                journey.name,
                segment.id
            );
        }
    }
    assert_eq!(
        after_patch.problems.len(),
        before.problems.len(),
        "{}: {:?}",
        journey.name,
        after_patch.problems
    );
}

/// Written as format 6 and read back, it is the same mapping.
fn round_trips(m: &Mapping, what: &str) {
    let json = map2d6::write(m).unwrap_or_else(|e| panic!("{what}: {e}"));
    let back = map2d6::read(&json).unwrap_or_else(|e| panic!("{what}: {e}\n{json}"));
    assert_eq!(&back, m, "{what}");
}

/// Every lamp is drawn within `tolerance` of where it was, through `new_id`.
fn drawn_where_it_was(
    before: &Mapping,
    after: &Mapping,
    tolerance: f32,
    new_id: impl Fn(&TreeId) -> TreeId,
) {
    let before = render(before).unwrap();
    let after = render(after).unwrap();
    for segment in before.segments() {
        let moved = after.segment(&new_id(&segment.id)).unwrap();
        for (a, b) in before.lamps_of(segment).iter().zip(after.lamps_of(moved)) {
            let far = (a[0] - b[0]).abs().max((a[1] - b[1]).abs());
            assert!(far <= tolerance, "{}: {a:?} → {b:?}", segment.id);
        }
    }
}

/// A wheel of four spokes, each spoke three copies of a 4-lamp arm turned
/// about its own hub: `wheel` (outer) holds `spoke` (inner) holds `arm`.
/// Overrides on `arm@2@3` (a port of its own), `arm@1` and `arm@0@2`.
fn wheel_of_spokes() -> Journey {
    let arm = Part::new(
        part("arm"),
        PartKind::Shape(Shape::Path(Path::of(
            vec![
                PathCommand::Move([30.0, 0.0]),
                PathCommand::Line([40.0, 0.0]),
            ],
            4,
        ))),
    );
    let spoke = Part::new(
        part("spoke"),
        PartKind::Repeat(Repeat::new(arm, [30.0, 5.0], 3)),
    );
    let wheel = Part::new(
        part("wheel"),
        PartKind::Repeat(Repeat::new(spoke, [0.0, 0.0], 4)),
    );
    let mut m = Mapping::new();
    m.parts.push(wheel);
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(
        node("arm@2@3"),
        Override {
            port: Some(PortOp::set(1)),
            channel: Some(ChannelOp::Set(Amount::lamps(0))),
            ..Override::default()
        },
    );
    m.overrides.insert(node("arm@1"), reversed());
    m.overrides.insert(
        node("arm@0@2"),
        Override {
            offset: Some(1),
            ..Override::default()
        },
    );
    Journey {
        name: "wheel of spokes",
        mapping: m,
        outputs: vec![OutputPorts::of_lamps("out", &[Some(60), Some(60)], 3)],
    }
}

/// A two-level id's copies, zeros put back: [inner, outer].
fn padded(id: &TreeId) -> [u32; 2] {
    [
        id.copies.first().copied().unwrap_or(0),
        id.copies.get(1).copied().unwrap_or(0),
    ]
}

/// Where a node's first lamp lands, as (port, RGB lamp on that port).
fn first_lamp(p: &Patching, id: &str) -> Option<(u32, u32)> {
    p.landing(&node(id), 0).map(|a| (a.port.0, a.channel / 3))
}

fn straight(count: u32) -> PartKind {
    PartKind::Shape(Shape::Path(Path::through(
        &[[0.0, 0.0], [10.0, 0.0]],
        count,
    )))
}

fn reversed() -> Override {
    Override {
        direction: Some(Direction::Reverse),
        ..Override::default()
    }
}

fn part(id: &str) -> PartId {
    PartId::new(id).unwrap()
}

fn node(text: &str) -> TreeId {
    TreeId::parse(text).unwrap_or_else(|| panic!("{text:?} is no id"))
}

fn key(text: &str) -> SegmentKey {
    SegmentKey::new(text).unwrap()
}

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
use lpc_mapping::edit::{EditError, insert_part, move_part, remove_part, rename_part};
use lpc_mapping::part::{Mapping, Part, PartId, PartKind, Path, Repeat, Shape, TreeId, render};
use lpc_mapping::patching::{Direction, OutputPorts, Override, Patching, Problem, patch_mapping};

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

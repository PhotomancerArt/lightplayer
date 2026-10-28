//! Statements: the layout verbs that take values (mapping rebuild M4 P07),
//! made the way an offer, a row and a key make them, through
//! `Editor::edit`. One sentence per test; the doc comment is the
//! statement.
//!
//! "Every lamp lands where it did" means every lamp of every segment lands
//! on the same output, port and pixel after the edit, each old segment id
//! taken to its new one.

use lab_mapping_model::*;
use lpc_mapping::part::{Circle, PartKind, Path, Repeat, Shape};
use lpc_mapping::patching::Direction;

/// Renaming a part keeps every lamp's landing: every override on it, its
/// rings' included, follows it to the new id (D6). An id that breaks the
/// rule, one another part has and one minted before are refused, saying
/// why, and change nothing.
#[test]
fn renaming_a_part_keeps_every_lamps_landing() {
    let mut e = Editor::new(Project::circular_display_wired());
    let circle = circle(&e.project);
    let before = landings(&e.project);

    e.edit(&[GraphEdit::Rename {
        part: circle.clone(),
        id: "display".to_string(),
    }])
    .unwrap();
    let after = landings(&e.project);
    let renamed: Vec<_> = before
        .iter()
        .map(|(id, lamps)| (id.replace("circle1", "display"), lamps.clone()))
        .collect();
    assert_eq!(after, renamed);
    assert!(
        e.project
            .fixture(&circle.fixture)
            .unwrap()
            .mapping
            .overrides
            .keys()
            .all(|k| k.part.as_str() == "display"),
        "every override follows"
    );

    let renamed = PartRef::parse(&circle.fixture, "display").unwrap();
    for (id, why) in [
        ("Display", "can't be an id"),
        ("circle1", "was used before"),
    ] {
        let refused = e.edit(&[GraphEdit::Rename {
            part: renamed.clone(),
            id: id.to_string(),
        }]);
        assert!(refused.is_err_and(|w| w.contains(why)), "{id}");
        assert_eq!(landings(&e.project), after, "nothing changed");
    }
}

/// Expanding a repeat keeps every lamp's landing and moves the overrides
/// on its copies to the parts they become (`arm@2` → `arm-2`).
#[test]
fn expanding_a_repeat_keeps_every_lamps_landing_and_moves_its_overrides() {
    let (mut e, f) = wheel();
    let arm2 = TreeRef::parse(&f, "arm@2").unwrap();
    e.edit(&[GraphEdit::SetDirection {
        subject: arm2,
        direction: Direction::Reverse,
    }])
    .unwrap();
    let before = landings(&e.project);

    e.edit(&[GraphEdit::Expand {
        part: PartRef::parse(&f, "repeat1").unwrap(),
    }])
    .unwrap();
    let after = landings(&e.project);
    let expanded: Vec<_> = before
        .iter()
        .map(|(id, lamps)| (id.replace('@', "-"), lamps.clone()))
        .collect();
    assert_eq!(after, expanded);
    let overrides = &e.project.fixture(&f).unwrap().mapping.overrides;
    let keys: Vec<String> = overrides.keys().map(ToString::to_string).collect();
    assert_eq!(keys, ["arm-2"], "the override moved with its copy");
}

/// A repeat counted down keeps the overrides of the copies it drops,
/// kept and gone (D12), and says so; counted back up, they land again. A
/// count that would drop an overridden copy is known before it is pressed.
#[test]
fn a_count_down_keeps_the_dropped_copies_overrides() {
    let (mut e, f) = wheel();
    let arm3 = TreeRef::parse(&f, "arm@3").unwrap();
    e.edit(&[GraphEdit::SetDirection {
        subject: arm3.clone(),
        direction: Direction::Reverse,
    }])
    .unwrap();
    let before = landings(&e.project);
    let repeat = PartRef::parse(&f, "repeat1").unwrap();
    assert_eq!(
        e.counts_that_keep(),
        [(node_value(&repeat.node()), 4)],
        "a count below 4 drops copy 3, which an override is on"
    );

    e.edit(&[GraphEdit::SetCount {
        subject: repeat.node(),
        count: 2,
    }])
    .unwrap();
    let mapping = &e.project.fixture(&f).unwrap().mapping;
    assert!(mapping.overrides.contains_key(&arm3.id), "kept");
    assert!(
        e.project
            .effective_patch()
            .problems
            .iter()
            .any(|x| x.kind == ProblemKind::GoneNode(arm3.clone())),
        "and reported gone"
    );
    assert_eq!(
        e.notice.as_ref().and_then(|n| n.why.clone()).as_deref(),
        Some("1 override on repeat1's copies is kept; it shows as a problem")
    );
    assert!(e.counts_that_keep().is_empty(), "nothing more to drop");

    e.edit(&[GraphEdit::SetCount {
        subject: repeat.node(),
        count: 4,
    }])
    .unwrap();
    assert_eq!(landings(&e.project), before, "found again");
}

/// A centre lamp is a one-lamp ring at the circle's centre (Q94, DD16),
/// added once: a circle that has one isn't offered another.
#[test]
fn a_centre_lamp_sits_at_the_centre() {
    let mut p = Project::new("t");
    let f = p
        .add_fixture_sized(None, "f", Vec2::new(100.0, 100.0))
        .unwrap();
    let out = p.add_output(None, "out", &[("D0", 64)]).unwrap();
    let circle = p
        .add(
            &f,
            PartKind::Shape(Shape::Circle(Circle::new(
                [50.0, 50.0],
                &[(42.0, 24), (32.0, 16)],
            ))),
        )
        .unwrap();
    p.start_at(&f, &out, "0", 0);
    let mut e = Editor::new(p);
    let offered = |e: &Editor| e.subjects(GraphVerb::AddCentreLamp).len();
    assert_eq!(offered(&e), 1);

    e.edit(&[GraphEdit::AddCentreLamp {
        circle: circle.clone(),
    }])
    .unwrap();
    let centre = TreeRef::parse(&f, "circle1/r3").unwrap();
    let lamps = &e.project.segment(&centre).unwrap().lamps;
    assert_eq!(lamps.len(), 1, "one lamp");
    let at = e.project.frame(&f).to_world(Vec2::new(50.0, 50.0));
    assert!(lamps[0].pos.distance(at) < 1e-4, "at the centre");
    assert_eq!(
        landing(&e.project, &centre, 0),
        Some(("0".to_string(), 40)),
        "last, after both rings"
    );
    assert_eq!(offered(&e), 0, "a circle has one centre");
    assert!(e.edit(&[GraphEdit::AddCentreLamp { circle }]).is_err());
}

/// Choosing a port of an output with no name names it, with the lowest
/// number no output carries, in the same undo step (lean Q96); the port's
/// option says what it will be named.
#[test]
fn naming_an_output_on_a_port_choice_writes_the_outputs_name() {
    let mut p = Project::circular_display();
    let spare = p
        .add_node(None, "spare", NodeKind::Output(Output::new(&[("D5", 10)])))
        .unwrap();
    let mut e = Editor::new(p);
    let option = e
        .port_choices(GraphVerb::SetPort)
        .into_iter()
        .find(|c| c.value == port_value(&spare, "0"))
        .unwrap();
    assert_eq!(option.label, "D5 of spare (names it output 1)");
    assert_eq!(option.disabled, None, "on offer");

    let r3 = TreeRef::parse(&circle(&e.project).fixture, "circle1/r3").unwrap();
    e.edit(&[GraphEdit::SetPort {
        subject: r3.clone(),
        port: PortChoice::At {
            output: spare.clone(),
            port: "0".to_string(),
        },
    }])
    .unwrap();
    assert_eq!(
        e.project.output(&spare).unwrap().label.as_deref(),
        Some("1")
    );
    let set = e.project.override_of(&r3).unwrap().output.clone();
    assert_eq!(
        set,
        Some(lpc_mapping::patching::OutputOp::Set(
            lpc_mapping::patching::OutputName::new("1")
        ))
    );
    e.undo();
    assert_eq!(e.project.output(&spare).unwrap().label, None, "one step");
}

/// A layout verb is offered whenever something can take it, whatever is
/// selected (ruling DD37); the selection only preselects its subject.
#[test]
fn a_layout_verb_is_offered_whatever_is_selected() {
    let mut e = Editor::new(Project::circular_display());
    assert!(e.selection.is_empty());
    let subjects = e.subjects(GraphVerb::RemoveRing);
    assert_eq!(subjects.len(), 5, "every ring of a circle of five");
    assert!(e.subjects(GraphVerb::Unrepeat).is_empty(), "no repeat");
    assert_eq!(e.selected_subject(GraphVerb::RemoveRing, &subjects), None);

    let r2 = TreeRef::parse(&circle(&e.project).fixture, "circle1/r2").unwrap();
    e.select(vec![Target::Generated(r2.clone())]);
    assert_eq!(
        e.selected_subject(GraphVerb::RemoveRing, &subjects),
        Some(node_value(&r2))
    );
    let circles = e.subjects(GraphVerb::AddRing);
    assert_eq!(
        e.selected_subject(GraphVerb::AddRing, &circles),
        Some(part_value(&circle(&e.project))),
        "a ring selected preselects its circle"
    );
}

/// The inspector's "A segment per ring" row presses `set-split`: off, the
/// circle is one segment and ring 3's override is kept and gone; one undo
/// splits it again and ring 3 lands where it did.
#[test]
fn the_split_row_presses_set_split() {
    let mut e = Editor::new(Project::circular_display_wired());
    let circle = circle(&e.project);
    let before = landings(&e.project);
    e.input(GraphInput::Select(vec![Target::Part(circle.clone())]));
    e.input(GraphInput::EditProp {
        id: "split-rings",
        edit: PropEdit::SetFlag(false),
    });
    let ids: Vec<String> = landings(&e.project).into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, ["circle1"], "one segment");
    let r3 = TreeRef::parse(&circle.fixture, "circle1/r3").unwrap();
    assert!(
        e.project
            .effective_patch()
            .problems
            .iter()
            .any(|x| x.kind == ProblemKind::GoneNode(r3.clone()))
    );
    e.undo();
    assert_eq!(landings(&e.project), before);
}

// ---- helpers ------------------------------------------------------------------

/// Every segment's id and where each of its lamps lands, in tree order.
fn landings(p: &Project) -> Vec<(String, Vec<Option<Landing>>)> {
    p.placed_segments()
        .iter()
        .map(|(_, s)| {
            let lamps = (0..s.lamps.len() as u32)
                .map(|lamp| p.effective_patch().landing(&s.id, lamp))
                .collect();
            (s.id.id.to_string(), lamps)
        })
        .collect()
}

fn landing(p: &Project, segment: &TreeRef, lamp: u32) -> Option<(String, u32)> {
    p.effective_patch()
        .landing(segment, lamp)
        .map(|l| (l.port, l.pixel))
}

/// The circular display's circle.
fn circle(p: &Project) -> PartRef {
    let display = p.named("display").unwrap().id.clone();
    PartRef::parse(&display, "circle1").unwrap()
}

/// A wheel: a repeat of four copies of a 3-lamp arm about the box's
/// middle, end to end on one 64-lamp port.
fn wheel() -> (Editor, NodeId) {
    let mut p = Project::new("t");
    let f = p
        .add_fixture_sized(None, "f", Vec2::new(100.0, 100.0))
        .unwrap();
    let out = p.add_output(None, "out", &[("D0", 64)]).unwrap();
    let arm = lpc_mapping::part::Part::new(
        lpc_mapping::part::PartId::new("arm").unwrap(),
        PartKind::Shape(Shape::Path(Path::through(&[[50.0, 10.0], [50.0, 30.0]], 3))),
    );
    p.add(&f, PartKind::Repeat(Repeat::new(arm, [50.0, 50.0], 4)))
        .unwrap();
    p.start_at(&f, &out, "0", 0);
    (Editor::new(p), f)
}

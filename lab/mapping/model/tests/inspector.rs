//! Statements: the one tree, the inspector, ports as rows of spans, and
//! problems in plain words (M4 P04). One sentence per test; the doc
//! comment is the statement.
//!
//! Most use the circular display (`Project::circular_display`): a circle
//! of four rings and a one-lamp centre (ring r5, radius 0) on output
//! `main`, five ports of 64 lamps, starting at D0's first lamp, end to end.

use lab_mapping_model::*;
use lpc_mapping::part::{PartKind, Path, Repeat, Shape};
use lpc_mapping::patching::{Direction, Flow, PatchTarget, PortKey};

// ---- the one tree ------------------------------------------------------------

/// The tree lists a circle's rings under it, lighter, and they can be
/// selected.
#[test]
fn the_tree_lists_a_circles_rings_under_it_lighter_and_selectable() {
    let mut e = Editor::new(Project::circular_display());
    let rows = e.view_tree_pane().rows;
    let circle = rows.iter().position(|r| r.label == "circle1").unwrap();
    let under: Vec<(&str, bool, usize)> = rows[circle + 1..circle + 6]
        .iter()
        .map(|r| (r.label.as_str(), r.derived, r.depth))
        .collect();
    let depth = rows[circle].depth + 1;
    assert_eq!(
        under,
        vec![
            ("ring r1", true, depth),
            ("ring r2", true, depth),
            ("ring r3", true, depth),
            ("ring r4", true, depth),
            ("the centre", true, depth),
        ]
    );
    assert!(!rows[circle].derived, "the circle is a part: primary");
    let r3 = rows[circle + 3].target.clone();
    e.input(GraphInput::Select(vec![r3.clone()]));
    assert_eq!(e.selection, vec![r3.clone()]);
    assert!(
        e.view_tree_pane()
            .rows
            .iter()
            .any(|r| r.target == r3 && r.selected)
    );
}

/// A repeat's copy 0 is its part's row; copies 1…n are listed lighter,
/// their insides are not.
#[test]
fn a_repeats_copy_0_is_its_parts_row_and_copies_are_lighter_leaves() {
    let (mut p, f) = bare();
    let a = p.new_part(&f, straight(10.0, 2)).unwrap();
    let b = p.new_part(&f, straight(20.0, 2)).unwrap();
    let group = p
        .new_part(&f, PartKind::Group { parts: vec![a, b] })
        .unwrap();
    p.add(&f, PartKind::Repeat(Repeat::new(group, [50.0, 50.0], 3)))
        .unwrap();
    let rows: Vec<(String, usize, bool)> = p
        .tree_rows()
        .into_iter()
        .map(|r| (r.label, r.depth, r.derived))
        .collect();
    let at = |label: &str| rows.iter().position(|r| r.0 == label).unwrap();
    let repeat = at("repeat1");
    assert_eq!(
        rows[repeat + 1..repeat + 6],
        [
            ("group1".to_string(), 2, false),
            ("path1".to_string(), 3, false),
            ("path2".to_string(), 3, false),
            ("copy 1".to_string(), 2, true),
            ("copy 2".to_string(), 2, true),
        ]
    );
    assert_eq!(
        rows.get(repeat + 6).map(|r| r.1),
        Some(0),
        "nothing under copy 2: the canvas and Enter reach a copy's insides"
    );
}

/// A path with one segment is one row.
#[test]
fn a_path_with_one_segment_is_one_row() {
    let (mut p, f) = bare();
    p.add(&f, straight(10.0, 5)).unwrap();
    let rows: Vec<(String, usize)> = p
        .tree_rows()
        .into_iter()
        .map(|r| (r.label, r.depth))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("f".to_string(), 0),
            ("path1".to_string(), 1),
            ("out".to_string(), 0),
            ("D0".to_string(), 1),
        ]
    );
}

// ---- the inspector -------------------------------------------------------------

/// The inspector shows the part's section before the segment's.
#[test]
fn the_inspector_shows_the_parts_section_before_the_segments() {
    let mut e = Editor::new(Project::circular_display());
    let r3 = ring(&e.project, "r3");
    e.select(vec![Target::Generated(r3)]);
    let headings: Vec<String> = e
        .view_inspector()
        .sections
        .iter()
        .map(|s| s.heading())
        .filter(|h| !h.is_empty())
        .collect();
    assert_eq!(headings[0], "Part · circle1");
    assert_eq!(headings[1], "Segment · circle1/r3");
    let part = &e.view_inspector().sections[0];
    assert!(
        part.items.iter().any(|i| matches!(
            i,
            UiItem::Hint(h) if h == "Changes here change every ring of circle1."
        )),
        "{part:?}"
    );
    assert_eq!(part.id.as_deref(), Some("circle1"), "the id, monospaced");
}

/// A ring's port that comes from the circle's flow says so, and selecting
/// the words selects the circle.
#[test]
fn a_rings_port_from_the_circles_flow_says_so_and_the_words_select_the_circle() {
    let mut p = Project::circular_display();
    let circle = circle(&p);
    p.set_flow(
        &circle.fixture,
        &PatchTarget::Part(circle.part.clone()),
        Flow::OnePerPort { from: PortKey(0) },
    );
    let mut e = Editor::new(p);
    e.select(vec![Target::Generated(ring(&e.project, "r3"))]);
    let port = field(&e, "Port");
    assert_eq!(port.value_words(), "D2 of main");
    let Some(UiSource::From {
        words,
        target: Some(target),
    }) = port.source
    else {
        panic!("{port:?}");
    };
    assert_eq!(words, "from circle1's flow: one per port");
    e.input(GraphInput::Select(vec![target]));
    assert_eq!(e.selection, vec![Target::Part(circle)]);
}

/// A port set here says "set here" and offers Back to the flow.
#[test]
fn a_port_set_here_says_set_here_and_offers_back_to_the_flow() {
    let mut e = Editor::new(Project::circular_display_wired());
    e.select(vec![Target::Generated(ring(&e.project, "r3"))]);
    let port = field(&e, "Port");
    assert_eq!(port.source, Some(UiSource::SetHere));
    assert_eq!(port.source_words(), "set here");
    assert_eq!(port.value_words(), "D1 of main");
    // Back to the flow: the field's Reset.
    e.input(GraphInput::Patch(PatchInput::Reset {
        field: PatchField::Port,
    }));
    let port = field(&e, "Port");
    assert_eq!(port.value_words(), "D0 of main");
    assert_eq!(port.source_words(), "follows ring r2");
}

/// Setting ring 3's port says what follows it.
#[test]
fn setting_ring_3s_port_says_what_follows_it() {
    let mut e = Editor::new(Project::circular_display());
    e.select(vec![Target::Generated(ring(&e.project, "r3"))]);
    choose(&mut e, PatchField::Port, "Port", "D1 of main");
    let port = field(&e, "Port");
    assert_eq!(port.source, Some(UiSource::SetHere));
    assert_eq!(
        port.note.as_deref(),
        Some("ring r4 and the centre follow it")
    );
}

/// Turning ring 3 off says what moves back into its place.
#[test]
fn turning_ring_3_off_says_what_moves_back_into_its_place() {
    let mut e = Editor::new(Project::circular_display());
    e.select(vec![Target::Generated(ring(&e.project, "r3"))]);
    choose(&mut e, PatchField::OnWire, "On the line", "Off");
    let line = field(&e, "On the line");
    assert_eq!(line.value_words(), "Off");
    assert_eq!(line.source, Some(UiSource::SetHere));
    assert_eq!(
        line.note.as_deref(),
        Some("ring r4 and the centre move back into its place")
    );
}

/// A direction set on the circle reads "from circle1" on each ring.
#[test]
fn a_direction_set_on_the_circle_reads_from_circle1_on_each_ring() {
    let mut p = Project::circular_display();
    let circle = circle(&p);
    p.edit_override(&circle.node(), |o| o.direction = Some(Direction::Reverse))
        .unwrap();
    let mut e = Editor::new(p);
    for key in ["r1", "r2", "r3", "r4", "r5"] {
        e.select(vec![Target::Generated(ring(&e.project, key))]);
        let d = field(&e, "Direction");
        assert_eq!(d.value_words(), "Reverse", "{key}");
        assert_eq!(
            d.source,
            Some(UiSource::From {
                words: "from circle1".into(),
                target: Some(Target::Part(circle.clone())),
            }),
            "{key}"
        );
    }
    e.select(vec![Target::Part(circle)]);
    assert_eq!(field(&e, "Direction").source, Some(UiSource::SetHere));
}

/// Presentation B shows only what is set here, under the flow's start.
#[test]
fn presentation_b_shows_only_what_is_set_here_under_the_flows_start() {
    let mut p = Project::circular_display();
    let circle = circle(&p);
    p.set_flow(
        &circle.fixture,
        &PatchTarget::Part(circle.part.clone()),
        Flow::OnePerPort { from: PortKey(0) },
    );
    let r3 = ring(&p, "r3");
    p.edit_override(&r3, |o| o.direction = Some(Direction::Reverse))
        .unwrap();
    let mut e = Editor::new(p);
    e.input(GraphInput::Presentation(Presentation::B));
    e.select(vec![Target::Generated(r3)]);
    let segment = e
        .view_inspector()
        .sections
        .into_iter()
        .find(|s| s.title.as_deref() == Some("Segment"))
        .unwrap();
    // After the ring's own properties (its lamps, its radius).
    let mut items = segment
        .items
        .into_iter()
        .skip_while(|i| matches!(i, UiItem::Prop { .. }));
    assert_eq!(
        items.next(),
        Some(UiItem::Hint(
            "circle1's flow: port D2, from its first lamp".into()
        ))
    );
    let fields: Vec<(&str, UiSource)> = segment_fields(&e)
        .into_iter()
        .map(|f| (f.label, f.source.unwrap()))
        .collect();
    assert_eq!(fields, vec![("Direction", UiSource::SetHere)]);
    // The rest can be set here from one list.
    assert!(items.any(|i| matches!(
        i,
        UiItem::SetHere { options } if options.iter().any(|(w, _)| w == "Port")
    )));
    // A shows every row.
    e.input(GraphInput::Presentation(Presentation::A));
    assert!(segment_fields(&e).len() > 1);
}

// ---- ports as rows of spans ----------------------------------------------------

/// A port row lists its segments and the gaps between them, across every
/// fixture on that output.
#[test]
fn a_port_row_lists_segments_and_gaps_across_every_fixture_on_the_output() {
    let (p, f1, f2, out) = two_fixtures(10);
    let row = p.port_row(&out, "0");
    let spans: Vec<(std::ops::Range<u32>, u32, Option<NodeId>)> = row
        .spans
        .iter()
        .map(|s| {
            (
                s.channels.clone(),
                s.lamps,
                s.segment.as_ref().map(|t| t.fixture.clone()),
            )
        })
        .collect();
    assert_eq!(
        spans,
        vec![
            (0..15, 5, Some(f1)),
            (15..30, 5, None),
            (30..42, 4, Some(f2)),
            (42..120, 26, None),
        ]
    );
    assert!(row.overlaps.is_empty());
    assert_eq!(row.spans[1].words(&p), "gap · 5 lamps");
}

/// Two segments on the same channels are marked on the port and listed as
/// a problem on both fixtures.
#[test]
fn an_overlap_is_marked_on_the_port_and_listed_on_both_fixtures() {
    let (p, f1, f2, out) = two_fixtures(2);
    let row = p.port_row(&out, "0");
    assert_eq!(row.overlaps.len(), 1);
    assert_eq!(row.overlaps[0].channels, 6..15);
    let words =
        "path1 of f1 and path1 of f2 share 3 lamps on port D0 of out: the board shows only one";
    for f in [&f1, &f2] {
        let texts: Vec<String> = p
            .problems_of_fixture(f)
            .into_iter()
            .map(|x| x.text)
            .collect();
        assert_eq!(texts, vec![words.to_string()], "{f:?}");
        assert_eq!(p.problem_count_words(f).as_deref(), Some("1 problem"));
    }
    let e = Editor::new(p);
    let face = e
        .view_tree()
        .nodes
        .into_iter()
        .find_map(|n| match n.face {
            Some(UiNodeFace::Output(o)) if o.output == out => Some(o),
            _ => None,
        })
        .unwrap();
    assert_eq!(face.ports[0].overlap.as_deref(), Some("3 lamps overlap"));
    assert!(!face.ports[0].overlaps.is_empty(), "outlined on the row");
}

// ---- problems --------------------------------------------------------------------

/// Each problem reads as one sentence naming its segment or port.
#[test]
fn each_problem_reads_as_one_sentence_naming_its_segment_or_port() {
    // One ring per port from D2: rings r1–r3 take D2–D4; ring r4 and the
    // centre are each past the last port (each is given a port of its own
    // by the flow, so each is reported).
    let mut p = Project::circular_display();
    let circle = circle(&p);
    p.set_flow(
        &circle.fixture,
        &PatchTarget::Part(circle.part.clone()),
        Flow::OnePerPort { from: PortKey(2) },
    );
    // An override on a ring that isn't there.
    let gone = TreeRef::parse(&circle.fixture, "circle1/r9").unwrap();
    p.edit_override(&gone, |o| o.off = true).unwrap();
    let mut e = Editor::new(p);
    let texts: Vec<String> = e
        .project
        .problems_of_fixture(&circle.fixture)
        .into_iter()
        .map(|x| x.text)
        .collect();
    assert_eq!(
        texts,
        vec![
            "an override on circle1/r9, which isn't there any more".to_string(),
            "ring r4 is past the last port of main: it isn't lit".to_string(),
            "the centre is past the last port of main: it isn't lit".to_string(),
        ]
    );
    for t in &texts {
        assert!(
            !t.contains('\n') && !t.trim_end_matches('.').contains(". "),
            "{t}"
        );
    }
    // A segment's own problem shows in its section too.
    e.select(vec![Target::Generated(ring(&e.project, "r4"))]);
    assert!(rows(&e).iter().any(|i| matches!(
        i,
        UiItem::Problem(x) if x.text == texts[1]
    )));
    // The gone override's problem carries Remove, and Remove takes it.
    let fixture = e
        .project
        .problems_of_fixture(&circle.fixture)
        .into_iter()
        .find(|x| x.remove.is_some())
        .unwrap();
    e.input(GraphInput::RemoveOverride(fixture.remove.unwrap()));
    assert!(e.project.override_of(&gone).is_none());
    // A segment set where it doesn't fit: one sentence naming the port.
    let mut p = Project::circular_display();
    let main = p.named("main").unwrap().id.clone();
    let r1 = ring(&p, "r1");
    p.patch_segment(&r1, &main, "0", 50).unwrap();
    let texts: Vec<String> = p
        .problems_of_fixture(&circle.fixture)
        .into_iter()
        .map(|x| x.text)
        .collect();
    assert!(
        texts.contains(
            &"ring r1 doesn't fit on port D0: it lands as far as the port goes".to_string()
        ),
        "{texts:?}"
    );
}

// ---- helpers -----------------------------------------------------------------------

/// A bare project: a fixture `f` and an output `out` with one port, D0.
fn bare() -> (Project, NodeId) {
    let mut p = Project::new("t");
    let f = p.add_fixture(None, "f").unwrap();
    p.add_output(None, "out", &[("D0", 40)]).unwrap();
    (p, f)
}

/// Two fixtures, each one path, both on output `out`'s D0 (40 lamps):
/// `f1`'s 5 lamps from its first lamp, `f2`'s 4 from lamp `second_at`.
fn two_fixtures(second_at: u32) -> (Project, NodeId, NodeId, NodeId) {
    let mut p = Project::new("t");
    let f1 = p.add_fixture(None, "f1").unwrap();
    let f2 = p.add_fixture(None, "f2").unwrap();
    let out = p.add_output(None, "out", &[("D0", 40)]).unwrap();
    p.add(&f1, straight(10.0, 5)).unwrap();
    p.add(&f2, straight(10.0, 4)).unwrap();
    p.start_at(&f1, &out, "0", 0);
    p.start_at(&f2, &out, "0", second_at);
    (p, f1, f2, out)
}

fn straight(y: f32, lamps: u32) -> PartKind {
    PartKind::Shape(Shape::Path(Path::through(&[[10.0, y], [80.0, y]], lamps)))
}

/// The circular display's circle.
fn circle(p: &Project) -> PartRef {
    let display = p.named("display").unwrap().id.clone();
    PartRef::parse(&display, "circle1").unwrap()
}

/// One ring of the circular display: `r1` … `r5` (the centre).
fn ring(p: &Project, key: &str) -> TreeRef {
    let display = p.named("display").unwrap().id.clone();
    TreeRef::parse(&display, &format!("circle1/{key}")).unwrap()
}

fn rows(e: &Editor) -> Vec<UiItem> {
    e.view_inspector()
        .sections
        .into_iter()
        .flat_map(|s| s.items)
        .collect()
}

/// The last patching row with this label (the segment's, after its
/// part's).
fn field(e: &Editor, label: &str) -> UiField {
    rows(e)
        .into_iter()
        .rev()
        .find_map(|i| match i {
            UiItem::Field(f) if f.label == label => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {label} row: {:?}", rows(e)))
}

/// The Segment section's patching rows.
fn segment_fields(e: &Editor) -> Vec<UiField> {
    e.view_inspector()
        .sections
        .into_iter()
        .filter(|s| s.title.as_deref() == Some("Segment"))
        .flat_map(|s| s.items)
        .filter_map(|i| match i {
            UiItem::Field(f) => Some(f),
            _ => None,
        })
        .collect()
}

/// Choose an option of a choice row by its words.
fn choose(e: &mut Editor, field: PatchField, label: &str, words: &str) {
    let UiFieldValue::Choice { options, .. } = self::field(e, label).value else {
        panic!("{label} isn't a choice");
    };
    let option = options
        .iter()
        .position(|o| o == words)
        .unwrap_or_else(|| panic!("{label} has no {words}: {options:?}"));
    e.input(GraphInput::Patch(PatchInput::Choose { field, option }));
}

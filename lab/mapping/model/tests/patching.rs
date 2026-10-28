//! Statements: patching by hand (the lab roadmap's stage 4). One sentence
//! per test, as in `statements.rs`: each test's doc comment IS the
//! statement.
//!
//! The projects here are bare: a rainbow shader, a fixture `f` of paths,
//! and an output `out` with the ports named.

use lab_mapping_model::*;
use lpc_mapping::part::{Circle, PartKind, Path, Shape};
use lpc_mapping::patching::{Direction, PatchTarget, StepSource};

// ---- the patch, by hand ----------------------------------------------------

/// Patching a segment puts its first lamp on the pixel named.
#[test]
fn patching_a_segment_puts_its_first_lamp_on_the_pixel() {
    let (mut p, _, out, segments) = paths(&[("D0", 40), ("D1", 40)], &[5]);
    let a = &segments[0];
    p.patch_segment(a, &out, "1", 7).unwrap();
    assert_eq!(landing(&p, a, 0), Some(("1".into(), 7)));
    assert_eq!(landing(&p, a, 4), Some(("1".into(), 11)));
}

/// Moving a patched segment keeps its direction and turn.
#[test]
fn moving_a_patched_segment_keeps_its_direction_and_turn() {
    let (mut p, _, out, segments) = paths(&[("D0", 40)], &[5]);
    let a = &segments[0];
    p.patch_segment(a, &out, "0", 0).unwrap();
    p.reverse_placement(a).unwrap();
    p.turn_placement(a, 2).unwrap();
    let pixels_before = pixels(&p, a);
    p.patch_segment(a, &out, "0", 20).unwrap();
    let after: Vec<u32> = pixels(&p, a).iter().map(|x| x - 20).collect();
    assert_eq!(after, pixels_before);
}

/// An edit takes what follows along, and leaves what comes before alone:
/// the walk carries on from wherever a segment starts (no snapping, D1b).
#[test]
fn an_edit_takes_what_follows_along() {
    let (mut p, f, out, segments) = paths(&[("D0", 40)], &[3, 4, 5]);
    flow(&mut p, &f, &out);
    let [a, b, c] = [&segments[0], &segments[1], &segments[2]];
    let was_a = pixels(&p, a);
    let edit = p.patch_segment(b, &out, "0", 30).unwrap();
    assert_eq!(pixels(&p, a), was_a, "before it: where it was");
    assert_eq!(pixels(&p, b), vec![30, 31, 32, 33]);
    assert_eq!(
        pixels(&p, c),
        (34..39).collect::<Vec<_>>(),
        "after it: along"
    );
    assert_eq!(edit.pinned, 0, "nothing is pinned behind your back");
}

/// Pinning a flowing segment where it is moves nothing at all.
#[test]
fn pinning_moves_nothing() {
    let (mut p, f, out, segments) = paths(&[("D0", 4), ("D1", 40)], &[3, 4]);
    flow(&mut p, &f, &out);
    let before = p.effective_patch().pixel_runs.clone();
    p.pin_segment(&segments[1]).unwrap();
    assert_eq!(p.effective_patch().pixel_runs, before);
    assert!(matches!(p.on_wire(&segments[1]), OnWire::Placed { .. }));
}

/// Reversing a segment lays it the other way along the same pixels.
#[test]
fn reversing_lays_a_segment_the_other_way() {
    let (mut p, f, out, segments) = paths(&[("D0", 40)], &[5]);
    flow(&mut p, &f, &out);
    let a = &segments[0];
    p.reverse_placement(a).unwrap();
    assert_eq!(pixels(&p, a), vec![4, 3, 2, 1, 0]);
}

/// Turning one lamp moves where the data enters back one lamp, the same
/// way whether the segment runs forward or reversed: the same key turns the
/// same way on the piece after a reverse (lp2014's keys).
#[test]
fn turning_is_the_same_way_after_a_reverse() {
    for reversed in [false, true] {
        let (mut p, _, out, segments) = paths(&[("D0", 40)], &[5]);
        let a = &segments[0];
        p.patch_segment(a, &out, "0", 0).unwrap();
        if reversed {
            p.reverse_placement(a).unwrap();
        }
        let enters = |p: &Project| (0..5).find(|&j| landing(p, a, j) == Some(("0".into(), 0)));
        let before = enters(&p).unwrap();
        p.turn_placement(a, 1).unwrap();
        assert_eq!(enters(&p), Some((before + 4) % 5), "reversed: {reversed}");
    }
}

/// The big turn is a third of the segment: 40 lamps of a 119-lamp panel,
/// one of its three sides.
#[test]
fn the_big_turn_is_a_third() {
    let (p, _, _, segments) = paths(&[("D0", 200)], &[119]);
    assert_eq!(p.coarse_turn(&segments[0]), 40);
}

/// Shifting a segment moves its start along its port, and never off it.
#[test]
fn shifting_moves_along_the_port_and_stays_on_it() {
    let (mut p, _, out, segments) = paths(&[("D0", 10), ("D1", 10)], &[3]);
    let a = &segments[0];
    p.patch_segment(a, &out, "1", 0).unwrap();
    p.shift_placement(a, 2).unwrap();
    assert_eq!(landing(&p, a, 0), Some(("1".into(), 2)));
    assert_eq!(p.shift_placement(a, -3), Err(PortEdge::Start));
    assert_eq!(p.shift_placement(a, 8), Err(PortEdge::End));
    assert_eq!(
        landing(&p, a, 0),
        Some(("1".into(), 2)),
        "refused: no change"
    );
}

/// Swapping two patched segments trades their places, and each keeps its
/// own direction.
#[test]
fn swapping_trades_places_and_keeps_direction() {
    let (mut p, _, out, segments) = paths(&[("D0", 40)], &[3, 3]);
    let (a, b) = (&segments[0], &segments[1]);
    p.patch_segment(a, &out, "0", 0).unwrap();
    p.patch_segment(b, &out, "0", 10).unwrap();
    p.reverse_placement(a).unwrap();
    p.swap_placements(a, b).unwrap();
    assert_eq!(pixels(&p, a), vec![12, 11, 10]);
    assert_eq!(pixels(&p, b), vec![0, 1, 2]);
}

/// Unpatch all takes every segment off the wire; patching one puts it back
/// on alone.
#[test]
fn unpatch_all_then_patching_one_puts_it_back_alone() {
    let (mut p, f, out, segments) = paths(&[("D0", 40)], &[3, 4]);
    p.patch_segment(&segments[0], &out, "0", 5).unwrap();
    p.patch_segment(&segments[1], &out, "0", 20).unwrap();
    assert_eq!(p.unpatch_all(&f), 2);
    assert!(p.effective_patch().pixel_runs.is_empty());
    p.patch_segment(&segments[1], &out, "0", 0).unwrap();
    assert_eq!(pixels(&p, &segments[1]), vec![0, 1, 2, 3]);
    assert!(!p.effective_patch().lands(&segments[0]));
}

/// Two segments on one pixel are allowed, reported as an overlap, and the
/// board shows the later one.
#[test]
fn an_overlap_is_reported_and_the_later_one_shows() {
    let (mut p, f, out, segments) = paths(&[("D0", 40)], &[4, 4]);
    let (a, b) = (&segments[0], &segments[1]);
    p.patch_segment(a, &out, "0", 0).unwrap();
    p.patch_segment(b, &out, "0", 2).unwrap();
    let overlaps = &p.effective_patch().overlaps;
    assert_eq!(overlaps.len(), 1);
    assert_eq!(overlaps[0].pixels, 2..4);
    assert_eq!(overlaps[0].segments, [a.clone(), b.clone()]);
    // The board shows b's lamp 0 on pixel 2: select b and its first lamp
    // is blue on the board (lp2014's start), on pixel 2.
    let p = laid_out(p);
    let frame = p.board_frame(&[Target::Generated(b.clone())], 0.0);
    let d0 = frame.port(&out, "0").unwrap();
    let b0 = frame.lamp(b, 0);
    assert!(b0.is_some());
    assert_eq!(Some(d0[2]), b0);
    let _ = f;
}

/// A port's free segments are the pixels no segment covers, cut to the
/// size asked for; the next segment to patch is the first with no lamp on
/// the wire.
#[test]
fn free_segments_and_the_next_segment() {
    let (mut p, f, out, segments) = paths(&[("D0", 10)], &[3, 2]);
    p.patch_segment(&segments[0], &out, "0", 2).unwrap();
    assert_eq!(p.free_segments(&out, "0", 3), vec![0..2, 5..8, 8..10]);
    assert_eq!(p.next_unplaced(&f), Some(segments[1].clone()));
}

// ---- the editor: the arm and the keys ---------------------------------------

/// Plain clicks never patch: selecting a pixel, then a segment, writes
/// nothing.
#[test]
fn plain_clicks_never_patch() {
    let (p, _, out, segments) = paths(&[("D0", 40)], &[3]);
    let mut e = editor_of(p);
    let before = e.project.clone();
    e.select(vec![pixel(&out, "0", 4)]);
    e.select(vec![Target::Part(segments[0].part())]);
    assert_eq!(e.project, before);
}

/// Wire first: A on a free segment arms; the arrows start on the first
/// segment not on the wire; Enter patches it there, and selects it.
#[test]
fn wire_first_a_then_enter_patches() {
    let (p, _, out, segments) = paths(&[("D0", 40)], &[3, 4]);
    let mut e = editor_of(p);
    e.select(vec![port(&out, "0")]);
    key(&mut e, Key::Char('a'), none());
    assert!(e.armed.is_some());
    assert_eq!(
        e.focused_candidate(),
        Some(PairEnd::Object(segments[0].clone()))
    );
    // The bar names the pair in focus, for whoever isn't looking at the
    // cursor.
    assert!(
        e.hint_bar().lead.starts_with("Patch "),
        "{}",
        e.hint_bar().lead
    );
    key(&mut e, Key::Right, none());
    assert_eq!(
        e.focused_candidate(),
        Some(PairEnd::Object(segments[1].clone()))
    );
    key(&mut e, Key::Enter, none());
    assert!(e.armed.is_none());
    assert_eq!(landing(&e.project, &segments[1], 0), Some(("0".into(), 0)));
    assert_eq!(e.selected_object(), Some(segments[1].clone()));
}

/// Segment first: A on a segment arms; the arrows walk the free segments in
/// wire order, across ports; Enter patches it on the one in focus.
#[test]
fn segment_first_arrows_walk_free_segments() {
    let (p, _, out, segments) = paths(&[("D0", 4), ("D1", 10)], &[3]);
    let mut e = editor_of(p);
    e.select(vec![Target::Part(segments[0].part())]);
    key(&mut e, Key::Char('a'), none());
    let walked: Vec<(String, u32)> = e
        .candidates()
        .iter()
        .map(|c| match c {
            PairEnd::Segment { port, pixels, .. } => (port.clone(), pixels.start),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        walked,
        vec![
            ("0".into(), 0),
            ("0".into(), 3),
            ("1".into(), 0),
            ("1".into(), 3),
            ("1".into(), 6),
            ("1".into(), 9)
        ]
    );
    key(&mut e, Key::Right, none());
    key(&mut e, Key::Right, none());
    key(&mut e, Key::Enter, none());
    assert_eq!(landing(&e.project, &segments[0], 0), Some(("1".into(), 0)));
    let _ = out;
}

/// M patches, then selects the next free segment and stays armed: the
/// loop is one key and one look per segment.
#[test]
fn m_patches_and_moves_on_armed() {
    let (p, _, out, segments) = paths(&[("D0", 3), ("D1", 3), ("D2", 3)], &[3, 3, 3]);
    let mut e = editor_of(p);
    e.select(vec![port(&out, "0")]);
    key(&mut e, Key::Char('m'), none());
    assert!(e.armed.is_some(), "unarmed, M arms");
    for (i, o) in segments.iter().enumerate() {
        assert_eq!(e.focused_candidate(), Some(PairEnd::Object(o.clone())));
        key(&mut e, Key::Char('m'), none());
        assert_eq!(landing(&e.project, o, 0), Some((i.to_string(), 0)));
    }
    assert!(
        e.armed.is_none(),
        "every port is full: nothing left to arm from"
    );
}

/// Esc disarms first; only the next Esc climbs.
#[test]
fn esc_disarms_before_it_climbs() {
    let (p, _, _, segments) = paths(&[("D0", 40)], &[3]);
    let mut e = editor_of(p);
    let t = Target::Part(segments[0].part());
    e.select(vec![t.clone()]);
    key(&mut e, Key::Char('a'), none());
    key(&mut e, Key::Escape, none());
    assert!(e.armed.is_none());
    assert_eq!(e.selection, vec![t]);
}

/// While armed, the board shows the pair as if it were made: the segment's
/// first lamp blue on the candidate's first pixel. Unarmed, that pixel is
/// as the patch has it.
#[test]
fn armed_the_board_previews_the_pair() {
    let (p, _, out, segments) = paths(&[("D0", 40)], &[3]);
    let mut e = editor_of(p);
    e.select(vec![Target::Part(segments[0].part())]);
    let unarmed = e.board_frame(0.0).port(&out, "0").unwrap()[0];
    key(&mut e, Key::Char('a'), none());
    let armed = e.board_frame(0.0).port(&out, "0").unwrap()[0];
    assert_eq!(armed, Rgb::new(0, 0, 255), "lp2014's blue start");
    assert_ne!(unarmed, armed);
}

/// R reverses the placement while Move & draw is off: the segment keeps
/// its window, still flowing on auto, and runs the other way inside it,
/// and the path's own direction (its points and flag) stays as drawn. With
/// Move & draw on, R reverses the path itself.
#[test]
fn r_reverses_the_placement_with_move_and_draw_off() {
    let (mut p, f, out, segments) = paths(&[("D0", 40)], &[3]);
    flow(&mut p, &f, &out);
    let mut e = editor_of(p);
    let c = segments[0].part();
    e.select(vec![Target::Part(c.clone())]);
    key(&mut e, Key::Char('d'), none());
    let path = e.project.part(&c).cloned();
    key(&mut e, Key::Char('r'), none());
    assert_eq!(e.project.part(&c).cloned(), path, "the path is as drawn");
    assert_eq!(pixels(&e.project, &segments[0]), vec![2, 1, 0]);
    assert_eq!(e.project.on_wire(&segments[0]), OnWire::Flowing);
    key(&mut e, Key::Char('d'), none());
    key(&mut e, Key::Char('r'), none());
    assert_ne!(e.project.part(&c).cloned(), path, "now the path reverses");
}

/// U (Back to the flow) on a segment that already follows its flow is
/// refused, and says how to take it off the line.
#[test]
fn unpatching_what_already_follows_says_how_to_take_it_off() {
    let (mut p, f, out, segments) = paths(&[("D0", 40)], &[3]);
    flow(&mut p, &f, &out);
    let mut e = editor_of(p);
    e.select(vec![Target::Part(segments[0].part())]);
    key(&mut e, Key::Char('u'), none());
    let n = e.notice.clone().unwrap();
    assert_eq!(n.kind, NoticeKind::Refused);
    assert!(n.help.unwrap().contains("On the line"));
}

/// ⇧→ walks the wire span by span: the segments on it and the gaps
/// between them, in pixel order, port after port; . jumps to the next
/// port's first gap.
#[test]
fn shift_arrows_walk_the_wire() {
    let (mut p, _, out, segments) = paths(&[("D0", 10), ("D1", 10)], &[3, 4]);
    p.patch_segment(&segments[1], &out, "0", 3).unwrap();
    let mut e = editor_of(p);
    e.select(vec![port(&out, "0")]);
    key(&mut e, Key::Right, shift());
    assert_eq!(e.selected_end(), Some(free(&out, "0", 0..3)));
    key(&mut e, Key::Right, shift());
    assert_eq!(e.selected_object(), Some(segments[1].clone()));
    key(&mut e, Key::Right, shift());
    assert_eq!(e.selected_end(), Some(free(&out, "0", 7..10)));
    key(&mut e, Key::Char('.'), none());
    assert_eq!(e.selected_end(), Some(free(&out, "1", 0..10)));
    assert!(matches!(
        e.selection.as_slice(),
        [Target::Span { segment: None, .. }]
    ));
}

/// Every patching verb the hint bar offers does something, for a segment
/// on the wire, one off it, a port, a free segment and a fixture.
#[test]
fn every_offered_patching_verb_does_something() {
    let (mut p, f, out, segments) = paths(&[("D0", 10), ("D1", 10)], &[3, 4]);
    p.patch_segment(&segments[0], &out, "0", 2).unwrap();
    let starts = vec![
        vec![Target::Part(segments[0].part())],
        vec![Target::Part(segments[1].part())],
        vec![port(&out, "1")],
        vec![pixel(&out, "1", 0), pixel(&out, "1", 1)],
        vec![Target::Node(f.clone())],
    ];
    let patching = [
        GraphVerb::Patch,
        GraphVerb::PatchNext,
        GraphVerb::Unpatch,
        GraphVerb::Swap,
        GraphVerb::Turn,
        GraphVerb::TurnBack,
        GraphVerb::TurnThird,
        GraphVerb::TurnThirdBack,
        GraphVerb::Shift,
        GraphVerb::ShiftBack,
        GraphVerb::WireNext,
        GraphVerb::WirePrevious,
        GraphVerb::NextPort,
        GraphVerb::UnpatchAll,
    ];
    let mut e = editor_of(p);
    key(&mut e, Key::Char('d'), none());
    for start in starts {
        e.select(start.clone());
        for verb in e.verbs().into_iter().filter(|v| patching.contains(v)) {
            let mut probe = e.clone();
            let before = (probe.project.clone(), probe.selection.clone());
            assert!(probe.press(verb), "{verb:?} listened for {start:?}");
            let changed =
                (probe.project.clone(), probe.selection.clone()) != before || probe.armed.is_some();
            assert!(changed, "{verb:?} did something for {start:?}");
            assert!(
                probe
                    .notice
                    .as_ref()
                    .is_none_or(|n| n.kind != NoticeKind::Refused),
                "{verb:?} was offered but refused for {start:?}: {:?}",
                probe.notice
            );
        }
    }
}

// ---- rules and overrides on the walk (theory "Patching") ---------------------

/// A fixture's segments run end to end from its start unless something says
/// otherwise: nothing set their steps, the walk carried on.
#[test]
fn segments_follow_the_fixtures_start() {
    let (mut p, f, out, segments) = paths(&[("D0", 40)], &[3, 4]);
    flow(&mut p, &f, &out);
    assert_eq!(pixels(&p, &segments[0]), vec![0, 1, 2]);
    assert_eq!(pixels(&p, &segments[1]), vec![3, 4, 5, 6]);
    let w = p.effective_patch().walked(&segments[1]).unwrap();
    assert!(w.sources.iter().all(|s| *s == StepSource::Follows));
}

/// A circle's Start at a pixel puts its rings there end to end, in order.
#[test]
fn placing_a_circle_puts_its_rings_there_in_order() {
    let (mut p, f, out, circle) = circle(&[("D0", 40)]);
    place(
        &mut p,
        &f,
        &PatchTarget::Part(circle.part.clone()),
        &out,
        "0",
        10,
    );
    let (r1, r2) = (ring(&circle, "r1"), ring(&circle, "r2"));
    assert_eq!(pixels(&p, &r1), (10..18).collect::<Vec<_>>());
    assert_eq!(pixels(&p, &r2), (18..22).collect::<Vec<_>>());
}

/// Reversing a circle's own node runs each ring the other way in its own
/// slot; the order the rings go in is the circle's layout, not its patch
/// (theory "Direction").
#[test]
fn reversing_a_circle_reverses_each_ring_in_its_slot() {
    let (mut p, f, out, circle) = circle(&[("D0", 40)]);
    place(
        &mut p,
        &f,
        &PatchTarget::Part(circle.part.clone()),
        &out,
        "0",
        10,
    );
    p.edit_override(&circle.node(), |o| o.direction = Some(Direction::Reverse))
        .unwrap();
    let (r1, r2) = (ring(&circle, "r1"), ring(&circle, "r2"));
    assert_eq!(pixels(&p, &r1), (10..18).rev().collect::<Vec<_>>());
    assert_eq!(pixels(&p, &r2), vec![21, 20, 19, 18]);
}

/// I can reverse the direction of the leds on one ring without manually
/// choosing its address: it keeps its slot in the circle's run.
#[test]
fn one_ring_reverses_without_an_address() {
    let (mut p, f, out, circle) = circle(&[("D0", 40)]);
    place(
        &mut p,
        &f,
        &PatchTarget::Part(circle.part.clone()),
        &out,
        "0",
        10,
    );
    let r2 = ring(&circle, "r2");
    p.reverse_placement(&r2).unwrap();
    assert_eq!(pixels(&p, &r2), vec![21, 20, 19, 18]);
    let w = p.effective_patch().walked(&r2).unwrap();
    assert!(
        w.sources.iter().all(|s| *s == StepSource::Follows),
        "no address"
    );
    assert_eq!(
        p.override_of(&r2).unwrap().direction,
        Some(Direction::Reverse)
    );
}

/// A ring set somewhere of its own takes the rings after it along (the
/// cursor continues); taking its start away puts them all back.
#[test]
fn a_ring_set_elsewhere_takes_the_rest_along() {
    let (mut p, f, out, circle) = circle(&[("D0", 40), ("D1", 40)]);
    place(
        &mut p,
        &f,
        &PatchTarget::Part(circle.part.clone()),
        &out,
        "0",
        0,
    );
    let (r1, r2) = (ring(&circle, "r1"), ring(&circle, "r2"));
    p.patch_segment(&r1, &out, "1", 5).unwrap();
    assert_eq!(landing(&p, &r1, 0), Some(("1".into(), 5)));
    assert_eq!(landing(&p, &r2, 0), Some(("1".into(), 13)), "r2 follows r1");
    assert!(p.unpatch_segment(&r1));
    assert_eq!(pixels(&p, &r1), (0..8).collect::<Vec<_>>());
    assert_eq!(pixels(&p, &r2), vec![8, 9, 10, 11]);
}

/// An offset is a leaf's alone: an offset on a container offsets nothing.
#[test]
fn an_offset_is_a_leafs_alone() {
    let (mut p, f, out, circle) = circle(&[("D0", 40)]);
    place(
        &mut p,
        &f,
        &PatchTarget::Part(circle.part.clone()),
        &out,
        "0",
        0,
    );
    p.edit_override(&circle.node(), |o| o.offset = Some(3))
        .unwrap();
    let r1 = ring(&circle, "r1");
    assert_eq!(pixels(&p, &r1), (0..8).collect::<Vec<_>>());
}

// ---- the patching rows ------------------------------------------------------

/// The inspector shows the part, then its segment: a ring's shows the
/// circle's section (its Flow and Start: every ring changes with it), then
/// the ring's own rows. A circle, which has no segment of its own, is one
/// section: its Flow and Start, then its own node's Direction and On the
/// line. A fixture has its Flow and Start. A ring of a circle that starts
/// nowhere lands nowhere, and offers its ports.
#[test]
fn the_inspector_shows_the_part_then_its_segment() {
    let (p, f, _, circle) = circle(&[("D0", 40)]);
    let mut e = editor_of(p);
    e.select(vec![Target::Generated(ring(&circle, "r2"))]);
    let sections = patch_labels(&e);
    assert_eq!(sections[0].0, "Part · circle1");
    assert!(sections[0].1.ends_with(&["Flow", "Start"]));
    assert_eq!(sections[1].0, "Segment · circle1/r2");
    assert!(sections[1].1.ends_with(&[
        "Output",
        "Port",
        "Starts at",
        "Direction",
        "Offset",
        "On the line"
    ]));
    assert_eq!(choice(&e, "Port").1, None, "nowhere yet: choose one");
    e.select(vec![Target::Part(circle.clone())]);
    let sections = patch_labels(&e);
    assert_eq!(sections[0].0, "Part · circle1");
    assert!(
        sections[0]
            .1
            .ends_with(&["Flow", "Start", "Direction", "On the line"]),
        "{sections:?}"
    );
    assert!(!sections.iter().any(|s| s.0.starts_with("Segment")));
    e.select(vec![Target::Node(f.clone())]);
    assert_eq!(e.rule_rows().map(|r| r.len()), Some(2), "Flow and Start");
}

/// Choosing a port puts the segment at that port's first lamp ("ring 3 is
/// on D9, from its first lamp"); Starts at's − + move its start.
#[test]
fn choosing_a_port_places_it_and_pixel_moves_it() {
    let (p, _, out, circle) = circle(&[("D0", 40)]);
    let mut e = editor_of(p);
    let r1 = ring(&circle, "r1");
    e.select(vec![Target::Generated(r1.clone())]);
    let (options, _) = choice(&e, "Port");
    let d0 = options.iter().position(|o| o.starts_with("D0")).unwrap();
    e.patch_input(PatchInput::Choose {
        field: PatchField::Port,
        option: d0,
    });
    assert_eq!(landing(&e.project, &r1, 0), Some(("0".into(), 0)));
    e.patch_input(PatchInput::Nudge {
        field: PatchField::Pixel,
        by: 3,
    });
    assert_eq!(landing(&e.project, &r1, 0), Some(("0".into(), 3)));
    let _ = out;
}

/// A ring given a port of its own says what moves with it: the rings after
/// it that only follow on.
#[test]
fn a_ring_with_a_port_of_its_own_says_what_follows_it() {
    let (mut p, f, out, circle) = circle(&[("D0", 40), ("D1", 40)]);
    place(&mut p, &f, &PatchTarget::Fixture, &out, "0", 0);
    let mut e = editor_of(p);
    e.select(vec![Target::Generated(ring(&circle, "r1"))]);
    e.patch_input(PatchInput::Override {
        field: PatchField::Port,
    });
    let (options, _) = choice(&e, "Port");
    let d1 = options.iter().position(|o| o.starts_with("D1")).unwrap();
    e.patch_input(PatchInput::Choose {
        field: PatchField::Port,
        option: d1,
    });
    let notes: Vec<String> = rows(&e)
        .into_iter()
        .filter_map(|i| match i {
            UiItem::Field(f) => f.note,
            _ => None,
        })
        .collect();
    assert!(
        notes.contains(&"ring r2 follows it".to_string()),
        "{notes:?}"
    );
}

/// Direction "Reversed" on one ring, chosen in its Patch section, leaves its
/// address alone.
#[test]
fn reversing_one_ring_from_the_section_keeps_its_address() {
    let (mut p, f, out, circle) = circle(&[("D0", 40)]);
    place(
        &mut p,
        &f,
        &PatchTarget::Part(circle.part.clone()),
        &out,
        "0",
        10,
    );
    let mut e = editor_of(p);
    let r2 = ring(&circle, "r2");
    e.select(vec![Target::Generated(r2.clone())]);
    e.patch_input(PatchInput::Override {
        field: PatchField::Direction,
    });
    let (options, _) = choice(&e, "Direction");
    let reversed = options.iter().position(|o| o == "Reverse").unwrap();
    e.patch_input(PatchInput::Choose {
        field: PatchField::Direction,
        option: reversed,
    });
    assert_eq!(pixels(&e.project, &r2), vec![21, 20, 19, 18]);
}

/// Two segments selected edit together; a Port they don't share is mixed.
#[test]
fn two_segments_edit_together_and_say_mixed() {
    let (mut p, f, out, segments) = paths(&[("D0", 40), ("D1", 40)], &[3, 3]);
    flow(&mut p, &f, &out);
    p.patch_segment(&segments[1], &out, "1", 20).unwrap();
    let mut e = editor_of(p);
    e.select(segments.iter().map(|o| Target::Part(o.part())).collect());
    assert_eq!(inherited(&e, "Port").0, "mixed");
    e.patch_input(PatchInput::Override {
        field: PatchField::Direction,
    });
    let (options, _) = choice(&e, "Direction");
    let reversed = options.iter().position(|o| o == "Reverse").unwrap();
    e.patch_input(PatchInput::Choose {
        field: PatchField::Direction,
        option: reversed,
    });
    assert_eq!(pixels(&e.project, &segments[0]), vec![2, 1, 0]);
    assert_eq!(pixels(&e.project, &segments[1]), vec![22, 21, 20]);
    assert_eq!(landing(&e.project, &segments[1], 0), Some(("1".into(), 22)));
}

/// Offset's − + do what L and K do: one undo step each.
#[test]
fn turn_in_the_section_is_the_turn_key() {
    let (mut p, f, out, segments) = paths(&[("D0", 40)], &[5]);
    flow(&mut p, &f, &out);
    let mut by_key = editor_of(p.clone());
    let mut by_row = editor_of(p);
    let t = Target::Part(segments[0].part());
    by_key.select(vec![t.clone()]);
    by_row.select(vec![t]);
    key(&mut by_key, Key::Char('k'), none());
    by_row.patch_input(PatchInput::Nudge {
        field: PatchField::Offset,
        by: 1,
    });
    assert_eq!(
        by_key.project.effective_patch().pixel_runs,
        by_row.project.effective_patch().pixel_runs
    );
}

/// A value from the walk says what it is and where it came from — the
/// fixture's start for the first thing laid, the thing before it after
/// that — and the link selects it (G2 of the last spike: "follows ring
/// display" said neither).
#[test]
fn a_walked_value_names_where_it_came_from() {
    let (mut p, f, out, circle) = circle(&[("D0", 40)]);
    place(&mut p, &f, &PatchTarget::Fixture, &out, "0", 10);
    let mut e = editor_of(p);
    let r1 = ring(&circle, "r1");
    e.select(vec![Target::Generated(r1.clone())]);
    let (value, from) = inherited(&e, "Port");
    assert_eq!(value, "D0 of out");
    let (words, target) = from.unwrap();
    assert_eq!(words, "from the fixture's start");
    assert_eq!(target, Target::Node(f.clone()));
    e.select(vec![Target::Generated(ring(&circle, "r2"))]);
    assert_eq!(inherited(&e, "Starts at").0, "18 lamps");
    let (words, target) = inherited(&e, "Starts at").1.unwrap();
    assert_eq!(words, "follows ring r1");
    assert_eq!(target, Target::Generated(r1));
}

/// Override makes an inherited value the thing's own and moves nothing;
/// Reset hands it back.
#[test]
fn override_moves_nothing_and_reset_follows_again() {
    let (mut p, f, out, rings) = circle(&[("D0", 40)]);
    place(&mut p, &f, &PatchTarget::Fixture, &out, "0", 10);
    let mut e = editor_of(p);
    let before = e.project.effective_patch().pixel_runs.clone();
    let r2 = Target::Generated(ring(&rings, "r2"));
    e.select(vec![r2]);
    for field in [PatchField::Port, PatchField::Direction] {
        e.patch_input(PatchInput::Override { field });
    }
    assert_eq!(e.project.effective_patch().pixel_runs, before);
    let (options, chosen) = choice(&e, "Port");
    assert_eq!(options[chosen.unwrap()], "D0 of out");
    assert_eq!(
        choice(&e, "Direction").0[choice(&e, "Direction").1.unwrap()],
        "Forward"
    );
    for field in [PatchField::Port, PatchField::Direction] {
        e.patch_input(PatchInput::Reset { field });
    }
    assert_eq!(e.project.effective_patch().pixel_runs, before);
    assert_eq!(inherited(&e, "Port").1.unwrap().0, "follows ring r1");

    // From the fixture's own start too: each ring taking its place as its
    // own keeps the order.
    let (mut p, f, out, rings) = circle(&[("D0", 40)]);
    flow(&mut p, &f, &out);
    let mut e = editor_of(p);
    let before = e.project.effective_patch().pixel_runs.clone();
    for key in ["r2", "r1"] {
        e.select(vec![Target::Generated(ring(&rings, key))]);
        e.patch_input(PatchInput::Override {
            field: PatchField::Port,
        });
        assert_eq!(e.project.effective_patch().pixel_runs, before, "{key}");
    }
}

// ---- spans (plan P7, D12, D13) -----------------------------------------------

/// A port's spans are each segment's pixels on it and the gaps between; two
/// overlapping segments make two spans, each leading back to its own segment.
#[test]
fn spans_are_projections_and_gaps_and_may_overlap() {
    let (mut p, _, out, segments) = paths(&[("D0", 20)], &[4, 4]);
    p.patch_segment(&segments[0], &out, "0", 2).unwrap();
    p.patch_segment(&segments[1], &out, "0", 4).unwrap();
    let spans: Vec<(std::ops::Range<u32>, Option<TreeRef>)> = p
        .spans(&out, "0")
        .into_iter()
        .map(|s| (s.pixels, s.segment))
        .collect();
    assert_eq!(
        spans,
        vec![
            (0..2, None),
            (2..6, Some(segments[0].clone())),
            (4..8, Some(segments[1].clone())),
            (8..20, None),
        ]
    );
}

/// A gap's Patch here puts the chosen segment at the gap's start, and
/// selects it.
#[test]
fn patch_here_puts_a_segment_at_the_gap() {
    let (mut p, _, out, segments) = paths(&[("D0", 20)], &[3, 4]);
    p.patch_segment(&segments[0], &out, "0", 0).unwrap();
    let mut e = editor_of(p);
    let gap = e.project.spans(&out, "0")[1].target();
    e.select(vec![gap]);
    let options = e.patch_here_options();
    assert_eq!(options, vec![segments[1].clone()]);
    e.patch_input(PatchInput::PatchHere { option: 0 });
    assert_eq!(pixels(&e.project, &segments[1]), vec![3, 4, 5, 6]);
    assert_eq!(e.selected_object(), Some(segments[1].clone()));
}

/// A span's Patch section is its segment's: reversing from the span
/// reverses the segment.
#[test]
fn a_spans_patch_section_edits_its_segment() {
    let (mut p, f, out, segments) = paths(&[("D0", 20)], &[3]);
    flow(&mut p, &f, &out);
    let mut e = editor_of(p);
    e.select(vec![e.project.spans(&out, "0")[0].target()]);
    e.patch_input(PatchInput::Override {
        field: PatchField::Direction,
    });
    let (options, _) = choice(&e, "Direction");
    let reversed = options.iter().position(|o| o == "Reverse").unwrap();
    e.patch_input(PatchInput::Choose {
        field: PatchField::Direction,
        option: reversed,
    });
    assert_eq!(pixels(&e.project, &segments[0]), vec![2, 1, 0]);
}

// ---- the piece ------------------------------------------------------------

/// The piece shows the board's pixels through its true wiring, not the
/// model's: a segment patched to the wrong pixels shows what is really on
/// its own.
#[test]
fn the_piece_shows_the_truth() {
    let (mut p, f, out, segments) = paths(&[("D0", 40)], &[3, 3]);
    let (a, b) = (&segments[0], &segments[1]);
    p.patch_segment(a, &out, "0", 0).unwrap();
    p.patch_segment(b, &out, "0", 10).unwrap();
    let piece = Piece::of(&p, "the test");
    // The model now has them the wrong way round.
    p.swap_placements(a, b).unwrap();
    // Select a: on the board, a's run (pixels 10–12 now) runs its start
    // colour. On the piece that is b's lamps, not a's.
    let p = laid_out(p);
    let frame = p.board_frame(&[Target::Generated(a.clone())], 0.0);
    assert_eq!(piece.lamp(&frame, b, 0), frame.lamp(a, 0));
    assert_eq!(
        piece
            .differences(&p)
            .iter()
            .map(|d| d.kind)
            .collect::<Vec<_>>(),
        vec![DifferenceKind::OtherPlace, DifferenceKind::OtherPlace]
    );
    let _ = f;
}

/// After unpatching everything and patching each segment as the piece is
/// wired, the patch matches the piece; until then it says what differs.
#[test]
fn a_hand_patch_matches_the_piece_when_done() {
    let (mut p, f, out, segments) = paths(&[("D0", 40)], &[5, 5]);
    let (a, b) = (&segments[0], &segments[1]);
    p.patch_segment(a, &out, "0", 10).unwrap();
    p.patch_segment(b, &out, "0", 0).unwrap();
    p.reverse_placement(b).unwrap();
    p.turn_placement(b, 2).unwrap();
    let piece = Piece::of(&p, "the test");
    p.unpatch_all(&f);
    let kinds = |p: &Project| {
        piece
            .differences(p)
            .iter()
            .map(|d| d.kind)
            .collect::<Vec<_>>()
    };
    assert_eq!(kinds(&p), vec![DifferenceKind::NotPatched; 2]);
    // Unpatching takes wires away; each segment keeps its own direction and
    // turn, so patching it back is all it takes.
    p.patch_segment(a, &out, "0", 10).unwrap();
    p.patch_segment(b, &out, "0", 0).unwrap();
    assert!(piece.matches(&p));
    // Reversed and turned wrong, it says which.
    p.reverse_placement(b).unwrap();
    assert_eq!(kinds(&p), vec![DifferenceKind::Reversed]);
    p.reverse_placement(b).unwrap();
    p.turn_placement(b, 1).unwrap();
    assert_eq!(kinds(&p), vec![DifferenceKind::Turned]);
    p.turn_placement(b, -1).unwrap();
    assert!(piece.matches(&p));
}

// ---- helpers --------------------------------------------------------------

/// A bare project: one fixture `f` with a straight path per count, and an
/// output `out` with these ports. Nothing is on the wire yet: the fixture
/// starts nowhere and every segment is off, for patching by hand.
fn paths(ports: &[(&str, u32)], counts: &[u32]) -> (Project, NodeId, NodeId, Vec<TreeRef>) {
    let mut p = Project::new("t");
    p.add_shader(None, "rainbow", Pattern::Rainbow);
    let f = p.add_fixture(None, "f").unwrap();
    let out = p.add_output(None, "out", ports).unwrap();
    let segments = counts
        .iter()
        .enumerate()
        .map(|(i, &n)| {
            let y = 10.0 + 10.0 * i as f32;
            let path = Path::through(&[[10.0, y], [80.0, y]], n);
            let id = p.add(&f, PartKind::Shape(Shape::Path(path))).unwrap();
            id.node()
        })
        .collect();
    p.unpatch_all(&f);
    (p, f, out, segments)
}

/// Put every segment of `f` back on, end to end from D0's first pixel.
fn flow(p: &mut Project, f: &NodeId, out: &NodeId) {
    p.fixture_mut(f).unwrap().mapping.overrides.clear();
    p.start_at(f, out, "0", 0);
}

/// An editor on `p`, laid out the way the page does it.
fn editor_of(p: Project) -> Editor {
    let mut e = Editor::new(p);
    let placement = lab_layout::canvas_layout(&e.layout_input());
    e.set_placement(&placement);
    e.resize(Vec2::new(1200.0, 800.0));
    e
}

fn key(e: &mut Editor, k: Key, mods: Mods) {
    e.key_down(k.clone(), mods);
    e.key_up(k, mods);
}

fn none() -> Mods {
    Mods::default()
}

fn shift() -> Mods {
    Mods {
        shift: true,
        ..Mods::default()
    }
}

fn port(output: &NodeId, key: &str) -> Target {
    Target::Port {
        output: output.clone(),
        key: key.into(),
    }
}

fn pixel(output: &NodeId, key: &str, index: u32) -> Target {
    Target::Pixel {
        output: output.clone(),
        key: key.into(),
        index,
    }
}

fn free(output: &NodeId, port: &str, pixels: std::ops::Range<u32>) -> PairEnd {
    PairEnd::Segment {
        output: output.clone(),
        port: port.into(),
        pixels,
    }
}

/// A bare project with one circle `circle` of two rings, r1 (8 lamps)
/// then r2 (4), in a fixture `f` with nothing placed.
fn circle(ports: &[(&str, u32)]) -> (Project, NodeId, NodeId, PartRef) {
    let mut p = Project::new("t");
    p.add_shader(None, "rainbow", Pattern::Rainbow);
    let f = p.add_fixture(None, "f").unwrap();
    let out = p.add_output(None, "out", ports).unwrap();
    let rings = Circle::new([50.0, 50.0], &[(30.0, 8), (15.0, 4)]);
    let c = p.add(&f, PartKind::Shape(Shape::Circle(rings))).unwrap();
    (p, f, out, c)
}

/// One ring of a circle: `r1`, `r2`.
fn ring(circle: &PartRef, key: &str) -> TreeRef {
    TreeRef::parse(&circle.fixture, &format!("{}/{key}", circle.part)).unwrap()
}

/// Start the fixture, or a part, at a port's pixel.
fn place(p: &mut Project, f: &NodeId, t: &PatchTarget, out: &NodeId, port: &str, pixel: u32) {
    let start = p.start_ops_at(out, port, pixel).unwrap();
    assert!(p.set_start(f, t, start));
}

/// The inspector's sections, each title with its rows' labels.
fn patch_labels(e: &Editor) -> Vec<(String, Vec<&'static str>)> {
    e.view_inspector()
        .sections
        .into_iter()
        .map(|s| {
            let labels = s
                .items
                .iter()
                .filter_map(|i| match i {
                    UiItem::Field(f) => Some(f.label),
                    _ => None,
                })
                .collect();
            (s.heading(), labels)
        })
        .collect()
}

/// Every patching row the inspector shows for the selection.
fn rows(e: &Editor) -> Vec<UiItem> {
    e.view_inspector()
        .sections
        .into_iter()
        .flat_map(|s| s.items)
        .collect()
}

/// A Patch choice row's options and what is chosen (the last one with
/// that label: the segment's, after the part's).
fn choice(e: &Editor, label: &str) -> (Vec<String>, Option<usize>) {
    rows(e)
        .into_iter()
        .rev()
        .find_map(|i| match i {
            UiItem::Field(UiField {
                label: l,
                value: UiFieldValue::Choice { options, chosen },
                ..
            }) if l == label => Some((options, chosen)),
            _ => None,
        })
        .unwrap()
}

/// A patching row's value in words, and where it came from (the words and
/// what they select).
fn inherited(e: &Editor, label: &str) -> (String, Option<(String, Target)>) {
    rows(e)
        .into_iter()
        .rev()
        .find_map(|i| match i {
            UiItem::Field(f) if f.label == label => Some((
                f.value_words(),
                match f.source {
                    Some(UiSource::From {
                        words,
                        target: Some(t),
                    }) => Some((words, t)),
                    _ => None,
                },
            )),
            _ => None,
        })
        .unwrap()
}

/// The project as the page has it: laid out, so its lamps sit in their
/// fixture's box on the canvas and the simulated board lights them.
fn laid_out(p: Project) -> Project {
    let mut e = Editor::new(p);
    let placement = lab_layout::canvas_layout(&e.layout_input());
    e.set_placement(&placement);
    e.project
}

fn landing(p: &Project, segment: &TreeRef, lamp: u32) -> Option<(String, u32)> {
    p.effective_patch()
        .landing(segment, lamp)
        .map(|l| (l.port, l.pixel))
}

/// Every lamp's pixel, lamp 0 first (all on one port).
fn pixels(p: &Project, segment: &TreeRef) -> Vec<u32> {
    let n = p.segment(segment).unwrap().lamps.len() as u32;
    (0..n)
        .map(|j| landing(p, segment, j).expect("on the wire").1)
        .collect()
}

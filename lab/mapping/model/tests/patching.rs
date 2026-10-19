//! Statements: patching by hand (the lab roadmap's stage 4). One sentence
//! per test, as in `statements.rs`: each test's doc comment IS the
//! statement.
//!
//! The projects here are bare: a rainbow shader, a fixture `f` of paths,
//! and an output `out` with the ports named.

use lab_mapping_model::*;

// ---- the patch, by hand ----------------------------------------------------

/// Patching an object puts its first lamp on the pixel named.
#[test]
fn patching_an_object_puts_its_first_lamp_on_the_pixel() {
    let (mut p, _, out, objects) = paths(&[("D0", 40), ("D1", 40)], &[5]);
    let a = &objects[0];
    p.patch_object(a, &out, "1", 7).unwrap();
    assert_eq!(landing(&p, a, 0), Some(("1".into(), 7)));
    assert_eq!(landing(&p, a, 4), Some(("1".into(), 11)));
}

/// Moving a patched object keeps its direction and turn.
#[test]
fn moving_a_patched_object_keeps_its_direction_and_turn() {
    let (mut p, _, out, objects) = paths(&[("D0", 40)], &[5]);
    let a = &objects[0];
    p.patch_object(a, &out, "0", 0).unwrap();
    p.reverse_placement(a).unwrap();
    p.turn_placement(a, 2).unwrap();
    let pixels_before = pixels(&p, a);
    p.patch_object(a, &out, "0", 20).unwrap();
    let after: Vec<u32> = pixels(&p, a).iter().map(|x| x - 20).collect();
    assert_eq!(after, pixels_before);
}

/// A wiring edit moves only what it edits: pinning, reversing or placing
/// one auto-flowed object leaves every other lamp where it was.
#[test]
fn a_wiring_edit_moves_only_what_it_edits() {
    let (mut p, f, out, objects) = paths(&[("D0", 40)], &[3, 4, 5]);
    p.auto_patch(&f, &out);
    let [a, b, c] = [&objects[0], &objects[1], &objects[2]];
    let (was_a, was_c) = (pixels(&p, a), pixels(&p, c));
    // b is in the middle of the flow: placing it far away would reflow a.
    let edit = p.patch_object(b, &out, "0", 30).unwrap();
    assert_eq!(pixels(&p, a), was_a);
    assert_eq!(pixels(&p, c), was_c);
    assert_eq!(edit.pinned, 2, "a and c are pinned where they were");
    assert_eq!(pixels(&p, b), vec![30, 31, 32, 33]);
}

/// Pinning a flowing object where it is moves nothing at all.
#[test]
fn pinning_moves_nothing() {
    let (mut p, f, out, objects) = paths(&[("D0", 4), ("D1", 40)], &[3, 4]);
    p.auto_patch(&f, &out);
    let before = p.effective_patch().spans.clone();
    p.pin_object(&objects[1]).unwrap();
    assert_eq!(p.effective_patch().spans, before);
    assert!(matches!(p.on_wire(&objects[1]), OnWire::Placed { .. }));
}

/// Reversing an object lays it the other way along the same pixels.
#[test]
fn reversing_lays_an_object_the_other_way() {
    let (mut p, f, out, objects) = paths(&[("D0", 40)], &[5]);
    p.auto_patch(&f, &out);
    let a = &objects[0];
    p.reverse_placement(a).unwrap();
    assert_eq!(pixels(&p, a), vec![4, 3, 2, 1, 0]);
}

/// Turning one lamp moves where the data enters back one lamp, the same
/// way whether the object runs forward or reversed: the same key turns the
/// same way on the piece after a reverse (lp2014's keys).
#[test]
fn turning_is_the_same_way_after_a_reverse() {
    for reversed in [false, true] {
        let (mut p, _, out, objects) = paths(&[("D0", 40)], &[5]);
        let a = &objects[0];
        p.patch_object(a, &out, "0", 0).unwrap();
        if reversed {
            p.reverse_placement(a).unwrap();
        }
        let enters = |p: &Project| (0..5).find(|&j| landing(p, a, j) == Some(("0".into(), 0)));
        let before = enters(&p).unwrap();
        p.turn_placement(a, 1).unwrap();
        assert_eq!(enters(&p), Some((before + 4) % 5), "reversed: {reversed}");
    }
}

/// The big turn is a third of the object: 40 lamps of a 119-lamp panel,
/// one of its three sides.
#[test]
fn the_big_turn_is_a_third() {
    let (p, _, _, objects) = paths(&[("D0", 200)], &[119]);
    assert_eq!(p.coarse_turn(&objects[0]), 40);
}

/// Shifting an object moves its start along its port, and never off it.
#[test]
fn shifting_moves_along_the_port_and_stays_on_it() {
    let (mut p, _, out, objects) = paths(&[("D0", 10), ("D1", 10)], &[3]);
    let a = &objects[0];
    p.patch_object(a, &out, "1", 0).unwrap();
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

/// Swapping two patched objects trades their places, and each keeps its
/// own direction.
#[test]
fn swapping_trades_places_and_keeps_direction() {
    let (mut p, _, out, objects) = paths(&[("D0", 40)], &[3, 3]);
    let (a, b) = (&objects[0], &objects[1]);
    p.patch_object(a, &out, "0", 0).unwrap();
    p.patch_object(b, &out, "0", 10).unwrap();
    p.reverse_placement(a).unwrap();
    p.swap_placements(a, b).unwrap();
    assert_eq!(pixels(&p, a), vec![12, 11, 10]);
    assert_eq!(pixels(&p, b), vec![0, 1, 2]);
}

/// Unpatch all leaves a manual fixture dark, and turning auto back on
/// flows it all on again.
#[test]
fn unpatch_all_then_auto() {
    let (mut p, f, out, objects) = paths(&[("D0", 40)], &[3, 4]);
    p.patch_object(&objects[0], &out, "0", 5).unwrap();
    p.patch_object(&objects[1], &out, "0", 20).unwrap();
    assert_eq!(p.unpatch_all(&f), 2);
    assert!(p.effective_patch().spans.is_empty());
    assert!(!p.is_auto(&f));
    assert_eq!(p.set_auto(&f, true), Some(out.clone()));
    assert_eq!(pixels(&p, &objects[1]), vec![3, 4, 5, 6]);
}

/// Two objects on one pixel are allowed, reported as an overlap, and the
/// board shows the later one.
#[test]
fn an_overlap_is_reported_and_the_later_one_shows() {
    let (mut p, f, out, objects) = paths(&[("D0", 40)], &[4, 4]);
    let (a, b) = (&objects[0], &objects[1]);
    p.patch_object(a, &out, "0", 0).unwrap();
    p.patch_object(b, &out, "0", 2).unwrap();
    let overlaps = &p.effective_patch().overlaps;
    assert_eq!(overlaps.len(), 1);
    assert_eq!(overlaps[0].pixels, 2..4);
    assert_eq!(overlaps[0].objects, [a.clone(), b.clone()]);
    // The board shows b's lamp 0 on pixel 2: select b and its first lamp
    // is blue on the board (lp2014's start), on pixel 2.
    let p = laid_out(p);
    let frame = p.board_frame(&[Target::Object(b.clone())], 0.0);
    let d0 = frame.port(&out, "0").unwrap();
    let b0 = frame.lamp(b, 0);
    assert!(b0.is_some());
    assert_eq!(Some(d0[2]), b0);
    let _ = f;
}

/// A port's free segments are the pixels no object covers, cut to the
/// size asked for; the next object to patch is the first with no lamp on
/// the wire.
#[test]
fn free_segments_and_the_next_object() {
    let (mut p, f, out, objects) = paths(&[("D0", 10)], &[3, 2]);
    p.patch_object(&objects[0], &out, "0", 2).unwrap();
    assert_eq!(p.free_segments(&out, "0", 3), vec![0..2, 5..8, 8..10]);
    assert_eq!(p.next_unplaced(&f), Some(objects[1].clone()));
}

// ---- the editor: the arm and the keys ---------------------------------------

/// Plain clicks never patch: selecting a pixel, then an object, writes
/// nothing.
#[test]
fn plain_clicks_never_patch() {
    let (p, _, out, objects) = paths(&[("D0", 40)], &[3]);
    let mut e = editor_of(p);
    let before = e.project.clone();
    e.select(vec![pixel(&out, "0", 4)]);
    e.select(vec![Target::Component(objects[0].component.clone())]);
    assert_eq!(e.project, before);
}

/// Wire first: A on a free segment arms; the arrows start on the first
/// object not on the wire; Enter patches it there, and selects it.
#[test]
fn wire_first_a_then_enter_patches() {
    let (p, _, out, objects) = paths(&[("D0", 40)], &[3, 4]);
    let mut e = editor_of(p);
    e.select(vec![port(&out, "0")]);
    key(&mut e, Key::Char('a'), none());
    assert!(e.armed.is_some());
    assert_eq!(
        e.focused_candidate(),
        Some(PairEnd::Object(objects[0].clone()))
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
        Some(PairEnd::Object(objects[1].clone()))
    );
    key(&mut e, Key::Enter, none());
    assert!(e.armed.is_none());
    assert_eq!(landing(&e.project, &objects[1], 0), Some(("0".into(), 0)));
    assert_eq!(e.selected_object(), Some(objects[1].clone()));
}

/// Object first: A on an object arms; the arrows walk the free segments in
/// wire order, across ports; Enter patches it on the one in focus.
#[test]
fn object_first_arrows_walk_free_segments() {
    let (p, _, out, objects) = paths(&[("D0", 4), ("D1", 10)], &[3]);
    let mut e = editor_of(p);
    e.select(vec![Target::Component(objects[0].component.clone())]);
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
    assert_eq!(landing(&e.project, &objects[0], 0), Some(("1".into(), 0)));
    let _ = out;
}

/// M patches, then selects the next free segment and stays armed: the
/// loop is one key and one look per object.
#[test]
fn m_patches_and_moves_on_armed() {
    let (p, _, out, objects) = paths(&[("D0", 3), ("D1", 3), ("D2", 3)], &[3, 3, 3]);
    let mut e = editor_of(p);
    e.select(vec![port(&out, "0")]);
    key(&mut e, Key::Char('m'), none());
    assert!(e.armed.is_some(), "unarmed, M arms");
    for (i, o) in objects.iter().enumerate() {
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
    let (p, _, _, objects) = paths(&[("D0", 40)], &[3]);
    let mut e = editor_of(p);
    let t = Target::Component(objects[0].component.clone());
    e.select(vec![t.clone()]);
    key(&mut e, Key::Char('a'), none());
    key(&mut e, Key::Escape, none());
    assert!(e.armed.is_none());
    assert_eq!(e.selection, vec![t]);
}

/// While armed, the board shows the pair as if it were made: the object's
/// first lamp blue on the candidate's first pixel. Unarmed, that pixel is
/// as the patch has it.
#[test]
fn armed_the_board_previews_the_pair() {
    let (p, _, out, objects) = paths(&[("D0", 40)], &[3]);
    let mut e = editor_of(p);
    e.select(vec![Target::Component(objects[0].component.clone())]);
    let unarmed = e.board_frame(0.0).port(&out, "0").unwrap()[0];
    key(&mut e, Key::Char('a'), none());
    let armed = e.board_frame(0.0).port(&out, "0").unwrap()[0];
    assert_eq!(armed, Rgb::new(0, 0, 255), "lp2014's blue start");
    assert_ne!(unarmed, armed);
}

/// R reverses the placement while Move & draw is off, and the path's own
/// direction (its points and flag) stays as drawn; with Move & draw on, R
/// reverses the path itself.
#[test]
fn r_reverses_the_placement_with_move_and_draw_off() {
    let (mut p, f, out, objects) = paths(&[("D0", 40)], &[3]);
    p.auto_patch(&f, &out);
    let mut e = editor_of(p);
    let c = objects[0].component.clone();
    e.select(vec![Target::Component(c.clone())]);
    key(&mut e, Key::Char('d'), none());
    let path = e.project.component(&c).cloned();
    key(&mut e, Key::Char('r'), none());
    assert_eq!(
        e.project.component(&c).cloned(),
        path,
        "the path is as drawn"
    );
    assert!(matches!(
        e.project.on_wire(&objects[0]),
        OnWire::Placed { reversed: true, .. }
    ));
    key(&mut e, Key::Char('d'), none());
    key(&mut e, Key::Char('r'), none());
    assert_ne!(
        e.project.component(&c).cloned(),
        path,
        "now the path reverses"
    );
}

/// U on an object auto flows on is refused, and says to turn auto off.
#[test]
fn unpatching_a_flowing_object_says_turn_auto_off() {
    let (mut p, f, out, objects) = paths(&[("D0", 40)], &[3]);
    p.auto_patch(&f, &out);
    let mut e = editor_of(p);
    e.select(vec![Target::Component(objects[0].component.clone())]);
    key(&mut e, Key::Char('u'), none());
    let n = e.notice.clone().unwrap();
    assert_eq!(n.kind, NoticeKind::Refused);
    assert!(n.help.unwrap().contains("⇧A"));
}

/// ⇧→ walks the wire: the objects on it and its free segments, in pixel
/// order, port after port; . jumps to the next port's first free segment.
#[test]
fn shift_arrows_walk_the_wire() {
    let (mut p, _, out, objects) = paths(&[("D0", 10), ("D1", 10)], &[3, 4]);
    p.patch_object(&objects[1], &out, "0", 3).unwrap();
    let mut e = editor_of(p);
    e.select(vec![port(&out, "0")]);
    key(&mut e, Key::Right, shift());
    assert_eq!(e.selected_end(), Some(segment(&out, "0", 0..3)));
    key(&mut e, Key::Right, shift());
    assert_eq!(e.selected_object(), Some(objects[1].clone()));
    key(&mut e, Key::Right, shift());
    assert_eq!(e.selected_end(), Some(segment(&out, "0", 7..10)));
    key(&mut e, Key::Char('.'), none());
    assert_eq!(e.selected_end(), Some(segment(&out, "1", 0..3)));
}

/// Every patching verb the hint bar offers does something, for an object
/// on the wire, one off it, a port, a free segment and a fixture.
#[test]
fn every_offered_patching_verb_does_something() {
    let (mut p, f, out, objects) = paths(&[("D0", 10), ("D1", 10)], &[3, 4]);
    p.patch_object(&objects[0], &out, "0", 2).unwrap();
    let starts = vec![
        vec![Target::Component(objects[0].component.clone())],
        vec![Target::Component(objects[1].component.clone())],
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
        GraphVerb::Auto,
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

// ---- the piece ------------------------------------------------------------

/// The piece shows the board's pixels through its true wiring, not the
/// model's: an object patched to the wrong pixels shows what is really on
/// its own.
#[test]
fn the_piece_shows_the_truth() {
    let (mut p, f, out, objects) = paths(&[("D0", 40)], &[3, 3]);
    let (a, b) = (&objects[0], &objects[1]);
    p.patch_object(a, &out, "0", 0).unwrap();
    p.patch_object(b, &out, "0", 10).unwrap();
    let piece = Piece::of(&p, "the test");
    // The model now has them the wrong way round.
    p.swap_placements(a, b).unwrap();
    // Select a: on the board, a's run (pixels 10–12 now) runs its start
    // colour. On the piece that is b's lamps, not a's.
    let p = laid_out(p);
    let frame = p.board_frame(&[Target::Object(a.clone())], 0.0);
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

/// After unpatching everything and patching each object as the piece is
/// wired, the patch matches the piece; until then it says what differs.
#[test]
fn a_hand_patch_matches_the_piece_when_done() {
    let (mut p, f, out, objects) = paths(&[("D0", 40)], &[5, 5]);
    let (a, b) = (&objects[0], &objects[1]);
    p.patch_object(a, &out, "0", 10).unwrap();
    p.patch_object(b, &out, "0", 0).unwrap();
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
    p.patch_object(a, &out, "0", 10).unwrap();
    p.patch_object(b, &out, "0", 0).unwrap();
    assert_eq!(kinds(&p), vec![DifferenceKind::Reversed]);
    p.reverse_placement(b).unwrap();
    assert_eq!(kinds(&p), vec![DifferenceKind::Turned]);
    p.turn_placement(b, 2).unwrap();
    assert!(piece.matches(&p));
}

// ---- helpers --------------------------------------------------------------

/// A bare project: one fixture `f` with a straight path per count, and an
/// output `out` with these ports. No rules: the fixture is manual.
fn paths(ports: &[(&str, u32)], counts: &[u32]) -> (Project, NodeId, NodeId, Vec<ObjectId>) {
    let mut p = Project::new("t");
    p.add_shader(None, "rainbow", Pattern::Rainbow);
    let f = p.add_fixture(None, "f").unwrap();
    let out = p.add_output(None, "out", ports).unwrap();
    let objects = counts
        .iter()
        .enumerate()
        .map(|(i, &n)| {
            let y = 10.0 + 10.0 * i as f64;
            let id = p
                .add(
                    &f,
                    ComponentKind::path(vec![Vec2::new(10.0, y), Vec2::new(80.0, y)], n),
                )
                .unwrap();
            ObjectId::new(&id, "path")
        })
        .collect();
    (p, f, out, objects)
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

fn segment(output: &NodeId, port: &str, pixels: std::ops::Range<u32>) -> PairEnd {
    PairEnd::Segment {
        output: output.clone(),
        port: port.into(),
        pixels,
    }
}

/// The project as the page has it: laid out, so its lamps sit in their
/// fixture's box on the canvas and the simulated board lights them.
fn laid_out(p: Project) -> Project {
    let mut e = Editor::new(p);
    let placement = lab_layout::canvas_layout(&e.layout_input());
    e.set_placement(&placement);
    e.project
}

fn landing(p: &Project, object: &ObjectId, lamp: u32) -> Option<(String, u32)> {
    p.effective_patch()
        .landing(object, lamp)
        .map(|l| (l.port, l.pixel))
}

/// Every lamp's pixel, lamp 0 first (all on one port).
fn pixels(p: &Project, object: &ObjectId) -> Vec<u32> {
    let n = p.object(object).unwrap().lamps.len() as u32;
    (0..n)
        .map(|j| landing(p, object, j).expect("on the wire").1)
        .collect()
}

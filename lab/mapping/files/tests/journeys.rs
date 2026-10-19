//! The hand-patching journeys (plan `lp2025/2026-10-18-2337-explicit-
//! patching-spike`), as tests: each opens a real project, wires its piece
//! (the install as it really is), and drives the editor by keys alone —
//! the way a person at the gate does — until the patch is the piece's.
//!
//! "Looking at the piece" is done the way a person does it: light the
//! selection on the board, and see which object on the piece lights up.

use lab_mapping_files::Opened;
use lab_mapping_files::embedded::{EAVES, EAVES_WIRED, Embedded, RADIANCE_DOME};
use lab_mapping_model::domain::breath::BREATH_SECONDS;
use lab_mapping_model::*;

/// J1: the dome, one object at a time. The first box's panels are
/// scrambled across its ports, each a random way round and entering at a
/// random lamp; everything is unpatched; then, port by port, the piece
/// shows which panel is on it, A pairs it, R and the turn keys line its
/// chase up with the piece's, and M moves on. Two panels go the wrong way
/// round on purpose and S swaps them back. It ends with the box's patch
/// exactly the piece's.
#[test]
fn j1_the_dome_one_panel_at_a_time() {
    let mut e = editor_of(open(&RADIANCE_DOME).project);
    let box1 = e.project.outputs()[0].id.clone();
    let piece = Piece::scrambled(&e.project, &box1, 7, "the first box, scrambled");
    let panels = truly_on(&piece, &e.project, &box1);
    let ports = e.project.output(&box1).unwrap().ports.len();
    assert!(panels.len() >= 19, "the box carries its panels");

    key(&mut e, Key::Char('d'), none()); // Move & draw off: patching only.
    e.select(vec![Target::Node(e.project.fixtures()[0].id.clone())]);
    key(&mut e, Key::Char('u'), shift()); // ⇧U: every panel off the wire.
    assert!(panels.iter().all(|o| !e.project.effective_patch().lands(o)));

    e.select(vec![Target::Port {
        output: box1.clone(),
        key: "0".into(),
    }]);
    let mut keys_per_panel = Vec::new();
    let mut walk_per_panel = Vec::new();
    let mut crossed: Vec<ObjectId> = Vec::new();
    for port in 0..ports {
        let mut keys = 0;
        let mut walk = 0;
        let lit = look(&e, &piece);
        if e.armed.is_none() {
            key(&mut e, Key::Char('a'), none());
            keys += 1;
        }
        // Ports 8 and 9: pair each with the other's panel, by mistake.
        let want = match port {
            8 => truly_at(&piece, &e.project, &box1, "9"),
            9 => truly_at(&piece, &e.project, &box1, "8"),
            _ => lit.clone(),
        };
        if want != lit {
            crossed.push(want.clone());
        }
        if port == 3 {
            // Object first, for once: the panel, then where it goes.
            let segment = e.selected_end().unwrap();
            key(&mut e, Key::Escape, none());
            e.select(vec![e.project.object_target(&want)]);
            key(&mut e, Key::Char('a'), none());
            keys += 2;
            while e.focused_candidate().as_ref() != Some(&segment) {
                key(&mut e, Key::Right, none());
                walk += 1;
                keys += 1;
                assert!(keys < 400, "port {port}: the segment is a candidate");
            }
        } else {
            while e.focused_candidate() != Some(PairEnd::Object(want.clone())) {
                key(&mut e, Key::Right, none());
                walk += 1;
                keys += 1;
                assert!(keys < 400, "port {port}: the panel is a candidate");
            }
        }
        key(&mut e, Key::Enter, none());
        keys += 1;
        keys += line_up(&mut e, &piece, &want);
        key(&mut e, Key::Char('m'), none());
        keys += 1;
        keys_per_panel.push(keys);
        walk_per_panel.push(walk);
    }

    // The two crossed panels: select one, S, walk to the other, Enter.
    let [a, b] = [crossed[0].clone(), crossed[1].clone()];
    e.select(vec![e.project.object_target(&a)]);
    key(&mut e, Key::Char('s'), none());
    while e.focused_candidate() != Some(PairEnd::Object(b.clone())) {
        key(&mut e, Key::Right, none());
    }
    key(&mut e, Key::Enter, none());
    for o in [&a, &b] {
        e.select(vec![e.project.object_target(o)]);
        line_up(&mut e, &piece, o);
    }

    let left: Vec<_> = panels
        .iter()
        .filter_map(|o| piece.difference(&e.project, o))
        .collect();
    assert!(left.is_empty(), "the box is patched as wired: {left:?}");
    // What the gate asks: how many keys a panel took, and how many of
    // them were arrows walking to the panel (a click picks it in one).
    let mean = |v: &[usize]| v.iter().sum::<usize>() as f64 / v.len() as f64;
    println!(
        "J1: {} panels, {:.1} keys a panel ({:.1} of them arrows to the panel, {:.1} the rest)",
        keys_per_panel.len(),
        mean(&keys_per_panel),
        mean(&walk_per_panel),
        mean(&keys_per_panel) - mean(&walk_per_panel),
    );
}

/// J2: Sean's eaves across two ports. The house is wired from the corner
/// where the runs meet: D10 feeds the front run from its far end, D9 the
/// side run. The eaves open on auto, so the front runs backwards on the
/// piece; R (Move & draw off) reverses its placement, the side run is
/// patched onto D9 by hand, and then D10 can grow without moving it. Saved
/// and opened again, the patch is the same.
#[test]
fn j2_seans_eaves_across_two_ports() {
    let opened = open(&EAVES);
    let piece = Piece::of(&open(&EAVES_WIRED).project, "the house");
    let mut e = editor_of(opened.project.clone());
    let fixture = e.project.fixtures()[0].id.clone();
    let output = e.project.outputs()[0].id.clone();
    let objects: Vec<ObjectId> = e
        .project
        .objects_in(&fixture)
        .iter()
        .map(|o| o.id.clone())
        .collect();
    let (front, side) = (objects[0].clone(), objects[1].clone());
    assert_eq!(
        kinds(&piece, &e.project),
        vec![(front.clone(), DifferenceKind::Reversed)],
        "on auto, the front runs backwards"
    );

    key(&mut e, Key::Char('d'), none());
    e.select(vec![e.project.object_target(&front)]);
    key(&mut e, Key::Char('r'), none());
    assert!(piece.matches(&e.project), "{:?}", kinds(&piece, &e.project));

    // The side run onto D9 by hand: A, then the first candidate is D9's
    // first pixel.
    e.select(vec![e.project.object_target(&side)]);
    key(&mut e, Key::Char('a'), none());
    let d9 = PairEnd::Segment {
        output: output.clone(),
        port: "1".into(),
        pixels: 0..50,
    };
    while e.focused_candidate() != Some(d9.clone()) {
        key(&mut e, Key::Right, none());
    }
    key(&mut e, Key::Enter, none());
    assert!(matches!(e.project.on_wire(&side), OnWire::Placed { .. }));

    // D10 has spare pixels at its far end: ten more, and the side run
    // stays on D9.
    e.select(vec![Target::Port {
        output: output.clone(),
        key: "0".into(),
    }]);
    for _ in 0..10 {
        key(&mut e, Key::Char('='), none());
    }
    assert_eq!(e.project.port_pixels(&output, "0"), 70);
    assert!(piece.matches(&e.project), "{:?}", kinds(&piece, &e.project));

    // Saved and opened again: the same patch, in today's rows.
    let saved = opened.files.save(&e.project).unwrap();
    let patch = saved
        .writes
        .iter()
        .find(|(path, _)| path.ends_with(".patch.json"))
        .expect("a patch file");
    assert!(
        patch.1.contains("\"format\": 2") || patch.1.contains("\"format\": 3"),
        "today's patch rows (format 3 at most): {}",
        patch.1
    );
    let again = reopen(&EAVES, &saved.writes).project;
    assert_eq!(
        again.effective_patch().spans,
        e.project.effective_patch().spans
    );
}

// ---- helpers --------------------------------------------------------------

/// What a person sees on the piece with the selection lit on the board: the
/// object whose lamps shine brightest.
fn look(e: &Editor, piece: &Piece) -> ObjectId {
    let frame = e.project.board_frame(&e.selection, BREATH_SECONDS / 2.0);
    let mut best: Option<(f64, ObjectId)> = None;
    for (_, o) in e.project.placed_objects() {
        let n = o.lamps.len().max(1) as f64;
        let light: f64 = (0..o.lamps.len() as u32)
            .filter_map(|j| piece.lamp(&frame, &o.id, j))
            .map(|c| c.r as f64 + c.g as f64 + c.b as f64)
            .sum::<f64>()
            / n;
        if best.as_ref().is_none_or(|(b, _)| light > *b) {
            best = Some((light, o.id.clone()));
        }
    }
    best.expect("the piece has objects").1
}

/// R and the turn keys until the object's chase on the canvas runs as it
/// does on the piece. Returns how many keys it took.
fn line_up(e: &mut Editor, piece: &Piece, o: &ObjectId) -> usize {
    let mut keys = 0;
    loop {
        let kind = piece.difference(&e.project, o).map(|d| d.kind);
        match kind {
            None | Some(DifferenceKind::OtherPlace) | Some(DifferenceKind::NotPatched) => {
                return keys;
            }
            Some(DifferenceKind::Reversed) => key(e, Key::Char('r'), none()),
            Some(DifferenceKind::Turned) => {
                // A third at a time while that's closer, then a lamp.
                let third = e.project.coarse_turn(o) as i64;
                let off = turn_off(e, piece, o);
                let k = if off >= third { ';' } else { 'k' };
                key(e, Key::Char(k), none());
            }
        }
        keys += 1;
        assert!(keys < 200, "{o} lines up");
    }
}

/// How many single-lamp turns (K) bring an object's entry to the piece's.
fn turn_off(e: &Editor, piece: &Piece, o: &ObjectId) -> i64 {
    let n = e.project.object(o).unwrap().lamps.len() as i64;
    let first = piece.landing(o, 0).unwrap();
    // Which lamp lands where the piece's lamp 0 is, now.
    let now = (0..n as u32)
        .find(|&j| e.project.effective_patch().landing(o, j).as_ref() == Some(&first))
        .unwrap() as i64;
    // One K moves where the data enters back one lamp: lamp `now` moves to
    // lamp 0 in `now` presses.
    now.rem_euclid(n)
}

/// The objects the piece has on an output.
fn truly_on(piece: &Piece, p: &Project, output: &NodeId) -> Vec<ObjectId> {
    p.placed_objects()
        .iter()
        .filter(|(_, o)| piece.landing(&o.id, 0).is_some_and(|l| &l.output == output))
        .map(|(_, o)| o.id.clone())
        .collect()
}

/// The object the piece has on one port.
fn truly_at(piece: &Piece, p: &Project, output: &NodeId, port: &str) -> ObjectId {
    p.placed_objects()
        .iter()
        .find(|(_, o)| {
            (0..o.lamps.len() as u32).any(|j| {
                piece
                    .landing(&o.id, j)
                    .is_some_and(|l| &l.output == output && l.port == port)
            })
        })
        .map(|(_, o)| o.id.clone())
        .unwrap()
}

fn kinds(piece: &Piece, p: &Project) -> Vec<(ObjectId, DifferenceKind)> {
    piece
        .differences(p)
        .into_iter()
        .map(|d| (d.object, d.kind))
        .collect()
}

fn open(e: &Embedded) -> Opened {
    Opened::open(&e.files()).unwrap_or_else(|err| panic!("{}: {err}", e.key))
}

/// The project as it would be with `writes` written over its files.
fn reopen(e: &Embedded, writes: &[(String, String)]) -> Opened {
    let mut files = e.files();
    for (path, text) in writes {
        files.files.insert(path.clone(), text.clone());
    }
    Opened::open(&files).unwrap_or_else(|err| panic!("{}: {err}", e.key))
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

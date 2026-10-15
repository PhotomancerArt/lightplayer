//! Statements: how the editor behaves, one sentence per test.
//!
//! Each test's doc comment IS the statement. Read this file as the list.
//! When a statement changes, change the sentence first, then the code.
//!
//! The project is `Project::seed()`, "desk", laid out with each output
//! beside the fixture that feeds it:
//! - module `tail rig`, holding
//!   - fixture `tail` (82 square): group1 (path1 out, path2 back),
//!     straight paths at x = 37 and 45 from y = 12 to 70, auto-patched to
//!     `tail out`;
//!   - output `tail out`: one port, GPIO 10, 60 px;
//! - fixture `ring display` (70 square): circle1 at (35, 35), rings r1
//!   (outer, 40 lamps) and r2 (inner, 24 lamps), auto-patched to `main`;
//! - fixture `strip` (80 square): path3, (6, 40) → (74, 40), 30 lamps,
//!   auto-patched to `main` after the ring display;
//! - output `main`: ports D0 (64 px) and D1 (30 px);
//! - fixture `scarf` (100 square, as every new fixture is): empty, to draw
//!   into, auto-patched to
//! - output `scarf out`: one port, D10, 60 px.
//!
//! The board is simulated (`device_frame`): times are in seconds, and
//! `CREST` is the top of a selected lamp's breath.
//!
//! Positions are in each fixture's own coordinates; `place(e, "tail", x, y)`
//! turns them into the canvas's.

use lab_mapping_model::*;

// ---- deleting ------------------------------------------------------------

/// Select everything and press delete, and everything is gone.
#[test]
fn select_all_then_delete_deletes_everything() {
    let mut e = editor();
    key(&mut e, Key::Char('a'), cmd());
    key(&mut e, Key::Delete, none());
    assert_eq!(e.project.lamp_count(), 0);
    assert!(
        e.project
            .fixtures()
            .iter()
            .all(|n| n.fixture().unwrap().components.is_empty())
    );
    assert_eq!(
        e.project.fixtures().len(),
        4,
        "the fixtures themselves stay"
    );
    assert_eq!(e.project.outputs().len(), 3, "and the outputs");
}

/// ⌘A climbs a level each time: from a lamp, every lamp of its ring, then
/// every ring, then everything in its fixture, then everything you drew in
/// every fixture, then the nodes themselves.
#[test]
fn select_all_climbs_to_everything_from_anywhere() {
    let mut e = editor();
    e.select(vec![lamp("circle1", "r2", 3)]);
    key(&mut e, Key::Char('a'), cmd());
    assert_eq!(e.selection.len(), 24, "every lamp of r2");
    key(&mut e, Key::Char('a'), cmd());
    assert_eq!(
        e.selection,
        vec![ring("circle1", "r1"), ring("circle1", "r2")]
    );
    key(&mut e, Key::Char('a'), cmd());
    assert_eq!(
        e.selection,
        vec![comp("circle1")],
        "everything in ring display"
    );
    key(&mut e, Key::Char('a'), cmd());
    assert_eq!(
        e.selection,
        vec![comp("group1"), comp("circle1"), comp("path3")]
    );
    key(&mut e, Key::Char('a'), cmd());
    assert_eq!(e.selection, e.project.children(None), "the nodes");
    key(&mut e, Key::Delete, none());
    assert!(e.project.nodes().is_empty());
}

/// Deleting a lamp is refused, and the refusal says why and what to do.
#[test]
fn deleting_a_lamp_is_refused_with_a_reason() {
    let mut e = editor();
    e.select(vec![lamp("path3", "path", 4)]);
    key(&mut e, Key::Delete, none());
    let n = e.notice.clone().expect("a notice");
    assert_eq!(n.kind, NoticeKind::Refused);
    assert!(n.why.unwrap().contains("path3"));
    assert!(n.help.unwrap().contains("−"));
    assert_eq!(
        e.project.lamp_count(),
        25 + 25 + 40 + 24 + 30,
        "nothing changed"
    );
}

/// Deleting a ring goes through its circle, and the other rings keep
/// their ids.
#[test]
fn deleting_a_ring_keeps_the_other_rings_ids() {
    let mut e = editor();
    e.select(vec![ring("circle1", "r1")]);
    key(&mut e, Key::Delete, none());
    assert!(!e.project.exists(&ring("circle1", "r1")));
    assert!(e.project.exists(&ring("circle1", "r2")));
}

/// A circle's last ring can't be deleted on its own; the refusal points at
/// the circle.
#[test]
fn a_circles_last_ring_is_refused() {
    let mut e = editor();
    e.select(vec![ring("circle1", "r1"), ring("circle1", "r2")]);
    key(&mut e, Key::Delete, none());
    let n = e.notice.clone().unwrap();
    assert_eq!(n.kind, NoticeKind::Refused);
    assert!(n.help.unwrap().contains("circle1"));
    assert!(e.project.exists(&comp("circle1")));
}

/// Undo brings back what delete took.
#[test]
fn undo_restores_a_delete() {
    let mut e = editor();
    let before = e.project.clone();
    key(&mut e, Key::Char('a'), cmd());
    key(&mut e, Key::Delete, none());
    key(&mut e, Key::Char('z'), cmd());
    assert_eq!(e.project, before);
}

// ---- selecting: the drill-down -------------------------------------------

/// With nothing selected, clicking a lamp selects the top-level thing it
/// belongs to.
#[test]
fn first_click_selects_the_top_level_thing() {
    let mut e = editor();
    click_lamp(&mut e, "circle1", "r2", 5, none());
    assert_eq!(e.selection, vec![comp("circle1")]);
}

/// Clicking the same place again goes one level in each time: circle, ring,
/// lamp.
#[test]
fn clicking_again_goes_in_one_level() {
    let mut e = editor();
    click_lamp(&mut e, "circle1", "r2", 5, none());
    click_lamp(&mut e, "circle1", "r2", 5, none());
    assert_eq!(e.selection, vec![ring("circle1", "r2")]);
    click_lamp(&mut e, "circle1", "r2", 5, none());
    assert_eq!(e.selection, vec![lamp("circle1", "r2", 5)]);
}

/// Inside a group, clicking a sibling selects the sibling, not the group.
#[test]
fn inside_a_group_a_click_selects_a_sibling() {
    let mut e = editor();
    e.select(vec![comp("path1")]);
    click_lamp(&mut e, "path2", "path", 10, none());
    assert_eq!(e.selection, vec![comp("path2")]);
}

/// ⌘-click selects the lamp directly, from anywhere.
#[test]
fn command_click_selects_the_lamp_directly() {
    let mut e = editor();
    click_lamp(&mut e, "path2", "path", 10, cmd());
    assert_eq!(e.selection, vec![lamp("path2", "path", 10)]);
}

/// Clicking empty space clears the selection.
#[test]
fn clicking_empty_space_deselects() {
    let mut e = editor();
    e.select(vec![comp("circle1")]);
    let empty = e.camera.to_screen(place(&e, "scarf", 50.0, 15.0));
    e.pointer_down(empty, Button::Primary, none());
    e.pointer_up(empty, none());
    assert!(e.selection.is_empty());
}

/// What the hover outline shows is exactly what a click there selects.
#[test]
fn hover_shows_what_a_click_selects() {
    let starts: Vec<Vec<Target>> = vec![
        vec![],
        vec![comp("circle1")],
        vec![ring("circle1", "r1")],
        vec![lamp("circle1", "r1", 2)],
        vec![comp("path1")],
    ];
    let probes = [
        ("circle1", "r2", 5),
        ("circle1", "r1", 2),
        ("path1", "path", 3),
        ("path3", "path", 0),
    ];
    for start in starts {
        for (c, k, i) in probes {
            for mods in [none(), cmd()] {
                let mut e = editor();
                e.select(start.clone());
                let at = lamp_screen(&e, c, k, i);
                e.pointer_move(at, mods);
                let shown = e.hover_target();
                e.pointer_down(at, Button::Primary, mods);
                e.pointer_up(at, mods);
                assert_eq!(
                    shown,
                    e.selection.first().cloned(),
                    "start {start:?}, probe {c}/{k}/{i}, {mods:?}"
                );
            }
        }
    }
}

/// Esc goes up one level; at the top it deselects.
#[test]
fn escape_goes_up_a_level() {
    let mut e = editor();
    e.select(vec![lamp("circle1", "r2", 5)]);
    key(&mut e, Key::Escape, none());
    assert_eq!(e.selection, vec![ring("circle1", "r2")]);
    key(&mut e, Key::Escape, none());
    assert_eq!(e.selection, vec![comp("circle1")]);
    key(&mut e, Key::Escape, none());
    assert_eq!(e.selection, vec![fixture(&e, "ring display")]);
    key(&mut e, Key::Escape, none());
    assert!(e.selection.is_empty());
}

/// Arrow keys walk the lamps of a path one at a time.
#[test]
fn arrows_walk_the_lamps() {
    let mut e = editor();
    e.select(vec![lamp("path3", "path", 0)]);
    key(&mut e, Key::Right, none());
    key(&mut e, Key::Right, none());
    assert_eq!(e.selection, vec![lamp("path3", "path", 2)]);
    key(&mut e, Key::Left, none());
    assert_eq!(e.selection, vec![lamp("path3", "path", 1)]);
}

/// A box selects everything at the current level that it touches; with ⌥
/// held, only what's fully inside.
#[test]
fn box_touches_and_option_encloses() {
    // Around the top half of the tail: it touches group1 but does not hold
    // all of it.
    for (mods, want) in [(none(), vec![comp("group1")]), (alt(), vec![])] {
        let mut e = editor();
        let a = e.camera.to_screen(place(&e, "tail", 28.0, 5.0));
        let b = e.camera.to_screen(place(&e, "tail", 57.0, 40.0));
        e.pointer_down(a, Button::Primary, none());
        e.pointer_move(a + Vec2::new(10.0, 10.0), mods);
        e.pointer_move(b, mods);
        e.pointer_up(b, mods);
        assert_eq!(e.selection, want, "{mods:?}");
    }
}

// ---- areas and overlaps --------------------------------------------------

/// Clicking empty space inside a group selects the group.
#[test]
fn empty_space_inside_a_group_selects_it() {
    let mut e = editor();
    // Between the tail's two paths, x = 37 and x = 45.
    let p = place(&e, "tail", 41.0, 40.0);
    click_at(&mut e, p, none());
    assert_eq!(e.selection, vec![comp("group1")]);
}

/// Clicking the empty middle of a circle selects the circle.
#[test]
fn the_middle_of_a_circle_selects_it() {
    let mut e = editor();
    let p = place(&e, "ring display", 35.0, 35.0);
    click_at(&mut e, p, none());
    assert_eq!(e.selection, vec![comp("circle1")]);
}

/// What's outlined is exactly what a click hits: wherever the hover shows
/// something, that thing's area holds the cursor, and where no area holds
/// it, nothing is hovered.
#[test]
fn the_outline_is_the_area_a_click_hits() {
    for start in [vec![], vec![comp("circle1")], vec![comp("path1")]] {
        let mut e = editor();
        e.select(start.clone());
        let x = e.project.extent();
        for gx in 0..70 {
            for gy in 0..50 {
                let w = Vec2::new(
                    x.min.x - 10.0 + gx as f64 * (x.width() + 20.0) / 70.0,
                    x.min.y - 10.0 + gy as f64 * (x.height() + 20.0) / 50.0,
                );
                let p = e.camera.to_screen(w);
                e.pointer_move(p, none());
                let under = e.project.things_at(&e.camera, p);
                match e.hover_target() {
                    Some(t) => {
                        let area = e
                            .project
                            .area(&e.camera, &t)
                            .expect("hovered things have an area");
                        assert!(area.contains(p), "{t:?} hovered outside its area at {p:?}");
                    }
                    None => assert!(under.is_empty(), "nothing hovered over {under:?}"),
                }
            }
        }
    }
}

/// Where two things at the same level overlap, ⌥-click selects the next
/// one, and keeps going round.
#[test]
fn option_click_cycles_through_overlaps() {
    let mut e = editor();
    // A path crossing path3 at (40, 40).
    cross_path3(&mut e);
    let at = place(&e, "strip", 40.0, 40.0);
    click_at(&mut e, at, alt());
    let first = e.selection.clone();
    click_at(&mut e, at, alt());
    let second = e.selection.clone();
    click_at(&mut e, at, alt());
    assert_ne!(first, second);
    assert_eq!(e.selection, first, "round again");
    let both = [comp("path3"), comp("path4")];
    assert!(both.contains(&first[0]) && both.contains(&second[0]));
}

/// When more than one thing at the click's level is under the cursor, the
/// cursor note says so and how to reach the others.
#[test]
fn the_cursor_note_names_overlaps() {
    let mut e = editor();
    cross_path3(&mut e);
    e.pointer_move(e.camera.to_screen(place(&e, "strip", 40.0, 40.0)), none());
    let note = e.cursor_note().expect("a note");
    assert!(
        note.contains("2 paths here") && note.contains("⌥ click") && note.contains("right-click"),
        "{note}"
    );
}

/// Right-click lists everything under the cursor, every level, in tree
/// order; choosing an item selects it; Esc closes the list and changes
/// nothing. The list opens only when asked for.
#[test]
fn right_click_lists_whats_here() {
    let mut e = editor();
    // Close enough that r2's lamps don't overlap on screen.
    e.select(vec![fixture(&e, "ring display")]);
    key(&mut e, Key::Char('2'), shift());
    e.select(vec![]);
    let at = lamp_screen(&e, "circle1", "r2", 5);
    e.pointer_down(at, Button::Secondary, none());
    let menu = e.menu.clone().expect("a menu");
    let targets: Vec<Target> = menu.items.iter().map(|i| i.target.clone()).collect();
    assert_eq!(
        targets[..3],
        [
            comp("circle1"),
            ring("circle1", "r2"),
            lamp("circle1", "r2", 5)
        ]
    );
    assert_eq!(
        menu.items
            .iter()
            .map(|i| i.depth)
            .take(3)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(e.selection.is_empty(), "opening it selects nothing");

    key(&mut e, Key::Escape, none());
    assert!(e.menu.is_none() && e.selection.is_empty());

    e.pointer_down(at, Button::Secondary, none());
    e.menu_choose(1, false);
    assert_eq!(e.selection, vec![ring("circle1", "r2")]);
    assert!(e.menu.is_none());
}

// ---- fixtures -------------------------------------------------------------

/// Clicking a fixture's title selects the fixture; clicking inside it
/// selects what's drawn there.
#[test]
fn a_fixtures_title_selects_it() {
    let mut e = editor();
    let corner = e.camera.to_screen(place(&e, "tail", 0.0, 0.0));
    let title = corner + Vec2::new(12.0, -10.0);
    e.pointer_move(title, none());
    e.pointer_down(title, Button::Primary, none());
    e.pointer_up(title, none());
    assert_eq!(e.selection, vec![fixture(&e, "tail")]);
}

/// Dragging a node pins it where it's dropped; unpinning returns it to the
/// layout; nodes never overlap.
#[test]
fn dragging_a_node_pins_it() {
    let mut e = editor();
    let ring = node_id(&e, "ring display");
    let before = e.project.node_rect(&ring).unwrap();
    let lamp_before = lamp_pos(&e, "circle1", "r1", 0);
    // By its title, 40 units right.
    let title = e.camera.to_screen(before.min) + Vec2::new(12.0, -10.0);
    let to = title + Vec2::new(40.0, 0.0) * e.camera.scale;
    drag(&mut e, title, to, none());
    assert_eq!(e.selection, vec![fixture(&e, "ring display")]);
    let moved = before.min + Vec2::new(40.0, 0.0);
    let after = e.project.node_rect(&ring).unwrap();
    assert!(after.min.distance(moved) < 1e-9, "{after:?}");
    assert!(e.project.node(&ring).unwrap().pinned.is_some(), "pinned");
    assert!(
        lamp_pos(&e, "circle1", "r1", 0).distance(lamp_before + Vec2::new(40.0, 0.0)) < 1e-9,
        "what's drawn in it comes along"
    );
    assert_eq!(overlaps(&e.project), Vec::<String>::new());
    // Undo puts it back under the layout; redo pins it again.
    key(&mut e, Key::Char('z'), cmd());
    assert!(e.project.node(&ring).unwrap().pinned.is_none());
    assert_eq!(e.project.node_rect(&ring).unwrap(), before);
    key(
        &mut e,
        Key::Char('z'),
        Mods {
            shift: true,
            ..cmd()
        },
    );
    assert!(e.project.node_rect(&ring).unwrap().min.distance(moved) < 1e-9);
    // U gives it back to the layout.
    key(&mut e, Key::Char('u'), none());
    assert!(e.project.node(&ring).unwrap().pinned.is_none());
    assert_eq!(e.project.node_rect(&ring).unwrap(), before);

    // An output drags by its card. Dropped right on top of the strip, it
    // stays where it was dropped and the strip makes room.
    let scarf_out = node_id(&e, "scarf out");
    let strip = node_id(&e, "strip");
    let card = e.project.node_rect(&scarf_out).unwrap();
    let grab = card.min + Vec2::new(20.0, 3.0);
    let onto = e.project.node_rect(&strip).unwrap().min + Vec2::new(20.0, 3.0);
    e.select(vec![]);
    let (from, to) = (e.camera.to_screen(grab), e.camera.to_screen(onto));
    drag(&mut e, from, to, none());
    assert_eq!(e.selection, vec![Target::Node(scarf_out.clone())]);
    let card = e.project.node_rect(&scarf_out).unwrap();
    assert!(
        card.min.distance(onto - Vec2::new(20.0, 3.0)) < 1e-9,
        "{card:?}"
    );
    assert_eq!(overlaps(&e.project), Vec::<String>::new());
    // The right-click menu offers to unpin it.
    let at = e.camera.to_screen(onto);
    e.pointer_down(at, Button::Secondary, none());
    let menu = e.menu.clone().unwrap();
    assert_eq!(menu.actions[0].label, "Unpin scarf out");
    e.menu_act(0);
    assert!(e.project.node(&scarf_out).unwrap().pinned.is_none());
}

/// A fixture's size is a property like any other.
#[test]
fn a_fixtures_size_is_a_property() {
    let mut e = editor();
    e.select(vec![fixture(&e, "scarf")]);
    e.edit_selection_prop("width", PropEdit::SetNumber(140.0));
    let scarf = node_id(&e, "scarf");
    assert_eq!(e.project.fixture(&scarf).unwrap().size.x, 140.0);
}

/// A box drag that leaves the canvas keeps going, and finishes where the
/// pointer is let go.
#[test]
fn a_drag_survives_leaving_the_canvas() {
    let mut e = editor();
    // From the tail's bottom-left, up past the top of the canvas.
    let a = e.camera.to_screen(place(&e, "tail", 28.0, 77.0));
    e.pointer_down(a, Button::Primary, none());
    e.pointer_move(a + Vec2::new(10.0, -10.0), none());
    e.pointer_leave();
    let outside = Vec2::new(e.camera.to_screen(place(&e, "tail", 55.0, 0.0)).x, -500.0);
    e.pointer_move(outside, none());
    assert!(e.selection_box().is_some(), "the box is still there");
    e.pointer_up(outside, none());
    assert_eq!(e.selection, vec![comp("group1")]);
}

// ---- nodes, outputs, edges -------------------------------------------------

/// Layout never overlaps nodes, at any level.
#[test]
fn layout_never_overlaps_nodes() {
    let mut e = editor();
    // A second module, nested in the first, so there are three levels.
    let rig = node_id(&e, "tail rig");
    let inner = e.project.add_module(Some(&rig), "spare").unwrap();
    e.project
        .add_fixture_sized(Some(&inner), "spare strip", Vec2::new(30.0, 10.0));
    let p = &e.project;
    assert_eq!(overlaps(p), Vec::<String>::new());
    assert!(
        p.all_nodes()
            .iter()
            .any(|n| n.children().iter().any(|c| !c.children().is_empty())),
        "three levels"
    );
    // And a module's frame holds everything in it.
    for n in p.all_nodes() {
        let frame = p.node_rect(&n.id).unwrap();
        for c in n.children() {
            let r = p.node_rect(&c.id).unwrap();
            assert!(
                frame.contains(r.min) && frame.contains(r.max),
                "{} in {}",
                c.name,
                n.name
            );
        }
    }
}

/// An output sits beside the fixture that feeds it — the first one, when
/// several do — a fixed gap to its right, level with its top.
#[test]
fn an_output_sits_beside_the_fixture_that_feeds_it() {
    let mut e = editor();
    let p = &e.project;
    let rect = |name: &str| p.node_rect(&node_id(&e, name)).unwrap();
    for (output, fixture) in [
        ("tail out", "tail"),
        ("main", "ring display"),
        ("scarf out", "scarf"),
    ] {
        let (o, f) = (rect(output), rect(fixture));
        assert_eq!(
            o.min,
            Vec2::new(f.max.x + layout::OUTPUT_GAP, f.min.y),
            "{output} beside {fixture}"
        );
    }
    // One that nothing feeds goes to the right of everything.
    let spare = e.project.add_output(None, "spare", &[("D5", 10)]).unwrap();
    let r = e.project.node_rect(&spare).unwrap();
    let others = e
        .project
        .nodes()
        .iter()
        .filter(|n| n.id != spare)
        .map(|n| e.project.node_rect(&n.id).unwrap().max.x)
        .fold(0.0, f64::max);
    assert!(r.min.x > others, "{r:?}");
    assert_eq!(overlaps(&e.project), Vec::<String>::new());
}

/// A node of any kind (module, fixture, output) selects, deletes and
/// climbs (Esc) the same way.
#[test]
fn every_kind_of_node_selects_deletes_and_climbs_alike() {
    for name in ["tail rig", "tail", "tail out", "ring display", "main"] {
        let mut e = editor();
        let t = fixture(&e, name);
        e.select(vec![t.clone()]);
        key(&mut e, Key::Escape, none());
        assert_eq!(
            e.selection,
            e.project.parent(&t).into_iter().collect::<Vec<_>>(),
            "Esc climbs from {name} to its frame"
        );
        e.select(vec![t.clone()]);
        key(&mut e, Key::Delete, none());
        assert!(!e.project.exists(&t), "{name} deleted");
        key(&mut e, Key::Char('z'), cmd());
        assert!(e.project.exists(&t), "{name} back");
    }
    // A child of a module climbs to it.
    let mut e = editor();
    e.select(vec![fixture(&e, "tail out")]);
    key(&mut e, Key::Escape, none());
    assert_eq!(e.selection, vec![fixture(&e, "tail rig")]);
}

/// Clicking inside a module selects what's inside it; the module is picked
/// by its title or border.
#[test]
fn a_module_is_see_through() {
    let mut e = editor();
    // Between the tail's two paths: group1, through the module and the
    // fixture.
    let between = place(&e, "tail", 41.0, 40.0);
    click_at(&mut e, between, none());
    assert_eq!(e.selection, vec![comp("group1")]);
    // The tail out card, inside the module.
    let card = e.project.node_rect(&node_id(&e, "tail out")).unwrap();
    e.select(vec![]);
    click_at(&mut e, card.min + Vec2::new(20.0, 3.0), none());
    assert_eq!(e.selection, vec![fixture(&e, "tail out")]);
    // The module's title tab.
    let corner = e.camera.to_screen(place(&e, "tail rig", 0.0, 0.0));
    let title = corner + Vec2::new(12.0, -10.0);
    e.pointer_move(title, none());
    e.pointer_down(title, Button::Primary, none());
    e.pointer_up(title, none());
    assert_eq!(e.selection, vec![fixture(&e, "tail rig")]);
}

/// Clicking an output selects it; clicking it again selects the port under
/// the cursor.
#[test]
fn clicking_an_output_then_again_selects_a_port() {
    let mut e = editor();
    let main = node_id(&e, "main");
    let d1 = e.project.port_rect(&main, "1").unwrap();
    let at = d1.min.lerp(d1.max, 0.5);
    click_at(&mut e, at, none());
    assert_eq!(e.selection, vec![Target::Node(main.clone())]);
    click_at(&mut e, at, none());
    assert_eq!(e.selection, vec![port(&main, "1")]);
    assert_eq!(e.project.label(&e.selection[0]), "D1");
}

/// The auto rule fills an output's ports in order: ring display fills D0
/// (64/64), strip fills D1 (30/30); the tail fills GPIO 10 with 50 of 60.
#[test]
fn the_auto_rule_fills_ports_in_order() {
    let e = editor();
    let p = &e.project;
    let main = node_id(&e, "main");
    let tail_out = node_id(&e, "tail out");
    assert_eq!(p.port_fill(&main, "0"), (64, 64));
    assert_eq!(p.port_fill(&main, "1"), (30, 30));
    assert_eq!(p.port_fill(&tail_out, "0"), (50, 60));
    let patch = p.effective_patch();
    assert!(patch.unpatched.is_empty() && patch.problems.is_empty());
    // The ring display's lamps: r1 then r2, in data order, from pixel 0.
    let d0: Vec<(String, std::ops::Range<u32>, u32)> = patch
        .spans
        .iter()
        .filter(|s| s.output == main && s.port == "0")
        .map(|s| (s.object.key.clone(), s.lamps.clone(), s.start_pixel))
        .collect();
    assert_eq!(d0, vec![("r1".into(), 0..40, 0), ("r2".into(), 0..24, 40)]);
    // One more lamp on the strip has nowhere to go: it is unpatched, not
    // squeezed in.
    let mut e = editor();
    e.select(vec![comp("path3")]);
    key(&mut e, Key::Char('='), none());
    let patch = e.project.effective_patch();
    assert_eq!(patch.unpatched.len(), 1);
    assert_eq!(patch.unpatched[0].lamps, 30..31);
}

/// Patch edges match the effective patch: one per object (one per span,
/// where an object runs on into the next port), in patch order.
#[test]
fn patch_edges_match_the_effective_patch() {
    let e = editor();
    let edges = e.edges();
    let spans = e.project.effective_patch().spans.clone();
    assert_eq!(edges.len(), spans.len());
    let got: Vec<(Target, Target, String)> = edges
        .iter()
        .map(|x| (x.from.clone(), x.to.clone(), x.label.clone()))
        .collect();
    let tail_out = node_id(&e, "tail out");
    let main = node_id(&e, "main");
    assert_eq!(
        got,
        vec![
            (
                comp("path1"),
                port(&tail_out, "0"),
                "lamps 0–24 → GPIO 10 @0".into()
            ),
            (
                comp("path2"),
                port(&tail_out, "0"),
                "lamps 0–24 → GPIO 10 @25".into()
            ),
            (
                ring("circle1", "r1"),
                port(&main, "0"),
                "lamps 0–39 → D0 @0".into()
            ),
            (
                ring("circle1", "r2"),
                port(&main, "0"),
                "lamps 0–23 → D0 @40".into()
            ),
            (comp("path3"), port(&main, "1"), "lamps 0–29 → D1 @0".into()),
        ]
    );
    assert!(edges.iter().all(|x| !x.bundled && x.active));
}

/// A per-object edge leaves from the object's first lamp and lands on the
/// pixel where its span starts: it leaves heading toward the output and
/// arrives from the left.
#[test]
fn an_edge_runs_from_the_first_lamp_to_its_pixel() {
    let mut e = editor();
    let tail_out = node_id(&e, "tail out");
    let main = node_id(&e, "main");
    let left_of = |e: &Editor, out: &NodeId, key: &str, i: u32| {
        let r = e.project.pixel_rect(out, key, i).unwrap();
        Vec2::new(r.min.x, (r.min.y + r.max.y) / 2.0)
    };
    let edge = |e: &Editor, from: Target| e.edges().into_iter().find(|x| x.from == from).unwrap();
    // path2 enters at its top (its first point) and lands on pixel 25.
    let x = edge(&e, comp("path2"));
    assert_eq!(x.from_at, lamp_pos(&e, "path2", "path", 0));
    assert_eq!(x.to_at, left_of(&e, &tail_out, "0", 25));
    // r2 from its lamp 0, onto pixel 40 of D0, after r1's 40.
    let x = edge(&e, ring("circle1", "r2"));
    assert_eq!(x.from_at, lamp_pos(&e, "circle1", "r2", 0));
    assert_eq!(x.to_at, left_of(&e, &main, "0", 40));
    // The curve heads for the output and comes in from the left.
    let [a, c1, c2, b] = x.curve(&e.camera);
    assert!((c1 - a).x > 0.0, "leaves toward the output, on its right");
    assert!(c2.y == b.y && c2.x < b.x, "arrives from the left");
    // Reverse path2, and its edge leaves from the other end.
    e.select(vec![comp("path2")]);
    key(&mut e, Key::Char('r'), none());
    let x = edge(&e, comp("path2"));
    assert_eq!(x.from_at, lamp_pos(&e, "path2", "path", 0));
    assert!(
        x.from_at.distance(place(&e, "tail", 45.0, 70.0)) < 1e-9,
        "the bottom now"
    );
}

/// Edges don't change when the selection changes; a fixture with many
/// objects on one output bundles, and so does one zoomed out too far to
/// tell its edges apart.
#[test]
fn edges_bundle_by_density_not_selection() {
    let mut e = editor();
    let shape = |e: &Editor| {
        e.edges()
            .into_iter()
            .map(|x| (x.from, x.to, x.from_at, x.to_at, x.bundled))
            .collect::<Vec<_>>()
    };
    let before = shape(&e);
    let main = node_id(&e, "main");
    for selection in [
        vec![fixture(&e, "tail")],
        vec![comp("path2")],
        vec![lamp("circle1", "r2", 4)],
        vec![port(&main, "0")],
        vec![pixel(&main, "1", 7)],
    ] {
        e.select(selection.clone());
        assert_eq!(shape(&e), before, "{selection:?}");
    }

    // Twenty-five short paths in the scarf: more than 24 objects on one
    // output, so one bundled edge, from the scarf's frame to pixel 0.
    let scarf = node_id(&e, "scarf");
    let scarf_out = node_id(&e, "scarf out");
    for i in 0..25 {
        let y = 4.0 + i as f64 * 3.6;
        e.project.add(
            &scarf,
            ComponentKind::path(vec![Vec2::new(10.0, y), Vec2::new(20.0, y)], 2),
        );
    }
    let from_scarf: Vec<Edge> = e
        .edges()
        .into_iter()
        .filter(|x| {
            x.from == Target::Node(scarf.clone()) || {
                e.project.fixture_of_target(&x.from) == Some(scarf.clone())
            }
        })
        .collect();
    assert_eq!(from_scarf.len(), 1, "one bundle");
    let x = &from_scarf[0];
    assert!(x.bundled && x.to == Target::Node(scarf_out.clone()));
    assert_eq!(x.label, "50 lamps");
    let frame = e.project.node_rect(&scarf).unwrap();
    assert!(
        (x.from_at.x - frame.max.x).abs() < 1e-9,
        "leaves the frame's side facing the output"
    );

    // Ten paths in the scarf stay ten edges — until zoomed far out (half
    // a pixel a unit), where the scarf is too small on screen to tell them
    // apart.
    let mut e = editor();
    let scarf = node_id(&e, "scarf");
    for i in 0..10 {
        let y = 5.0 + i as f64 * 9.0;
        e.project.add(
            &scarf,
            ComponentKind::path(vec![Vec2::new(10.0, y), Vec2::new(30.0, y)], 4),
        );
    }
    let bundled = |e: &Editor| {
        e.edges()
            .into_iter()
            .filter(|x| x.bundled)
            .map(|x| x.from)
            .collect::<Vec<_>>()
    };
    assert!(bundled(&e).is_empty());
    let middle = e.viewport * 0.5;
    while e.camera.scale > 0.5 {
        e.wheel(middle, Vec2::new(0.0, 5.0), cmd());
    }
    assert_eq!(
        bundled(&e),
        vec![Target::Node(scarf)],
        "only the crowded one"
    );
}

/// Deleting an output leaves its fixtures' rules as visible problems;
/// nothing is retargeted.
#[test]
fn deleting_an_output_leaves_problems_not_retargets() {
    let mut e = editor();
    let main = node_id(&e, "main");
    e.select(vec![Target::Node(main.clone())]);
    key(&mut e, Key::Delete, none());
    let p = &e.project;
    let patch = p.effective_patch();
    let broken: Vec<String> = patch
        .problems
        .iter()
        .map(|x| p.node_name(&x.fixture))
        .collect();
    assert_eq!(broken, vec!["ring display", "strip"]);
    assert!(patch.problems.iter().all(|x| x.missing == main));
    let ring = node_id(&e, "ring display");
    assert_eq!(
        p.fixture(&ring).unwrap().rules,
        vec![PatchRule::Auto {
            output: main.clone()
        }],
        "the rule still names main"
    );
    assert!(patch.spans.iter().all(|s| s.fixture == node_id(&e, "tail")));
    // The notice says so.
    let n = e.notice.clone().unwrap();
    assert!(n.why.unwrap().contains("ring display and strip"));
    // And so does the problem, in rustc style.
    let pn = patch.problems[0].notice(p);
    assert!(pn.text.contains("no longer exists") && pn.help.is_some());
}

/// Selecting a port highlights exactly the lamps it carries.
#[test]
fn a_selected_port_highlights_its_lamps() {
    let e = editor();
    let main = node_id(&e, "main");
    let d1 = e.project.lamps_on_port(&main, "1");
    let path3 = ObjectId::new(&ComponentId::new("path3"), "path");
    assert_eq!(d1, (0..30).map(|i| (path3.clone(), i)).collect::<Vec<_>>());
    let d0 = e.project.lamps_on_port(&main, "0");
    assert_eq!(d0.len(), 64);
    assert!(d0.iter().all(|(o, _)| o.component.as_str() == "circle1"));
    // And the other way: the ring display lands on D0 only.
    assert_eq!(
        e.project.ports_of(&fixture(&e, "ring display")),
        vec![port(&main, "0")]
    );
}

/// Edges stay quiet: with nothing selected none shows a label, and with
/// something selected only the edges attached to that thing itself do.
#[test]
fn only_edges_on_the_selection_are_labelled() {
    let mut e = editor();
    assert!(
        e.edges().iter().all(|x| !x.labelled),
        "nothing selected, no labels"
    );

    let main = node_id(&e, "main");
    e.select(vec![port(&main, "1")]);
    let labelled: Vec<Edge> = e.edges().into_iter().filter(|x| x.labelled).collect();
    assert_eq!(labelled.len(), 1, "only the strip's edge lands on D1");
    assert_eq!(labelled[0].from, comp("path3"));

    // Inside the ring display, the circle's per-ring edges aren't attached
    // to a selected ring, so only the selected ring's edge is labelled.
    e.select(vec![ring("circle1", "r2")]);
    let labelled: Vec<Edge> = e.edges().into_iter().filter(|x| x.labelled).collect();
    assert!(
        labelled.iter().all(|x| x.from == ring("circle1", "r2")),
        "{labelled:?}"
    );
}

/// Enter on a fixture frames the fixture together with its outputs.
#[test]
fn enter_on_a_fixture_frames_it_with_its_outputs() {
    let mut e = editor();
    let overview = e.camera.scale;
    e.select(vec![fixture(&e, "ring display")]);
    key(&mut e, Key::Enter, none());
    assert_eq!(e.selection, vec![comp("circle1")]);
    assert!(e.camera.scale > overview, "zoomed in");
    for name in ["ring display", "main"] {
        let r = e.project.node_rect(&node_id(&e, name)).unwrap();
        for corner in [r.min, r.max] {
            let s = e.camera.to_screen(corner);
            assert!(
                s.x >= 0.0 && s.y >= 0.0 && s.x <= e.viewport.x && s.y <= e.viewport.y,
                "{name} is on screen"
            );
        }
    }
    // ⇧2 frames a selection the same way; ⇧1 fits everything again.
    let framed = e.camera;
    e.select(vec![fixture(&e, "ring display")]);
    key(&mut e, Key::Char('1'), shift());
    assert!(e.camera.scale < framed.scale);
    key(&mut e, Key::Char('2'), shift());
    assert_eq!(e.camera, framed);
}

// ---- editing ---------------------------------------------------------------

/// Changing a path's count keeps the selected lamp selected.
#[test]
fn selection_survives_a_count_change() {
    let mut e = editor();
    e.select(vec![lamp("path3", "path", 7)]);
    key(&mut e, Key::Char('='), none());
    assert_eq!(e.selection, vec![lamp("path3", "path", 7)]);
    assert_eq!(
        e.project
            .object(&ObjectId::new(&ComponentId::new("path3"), "path"))
            .unwrap()
            .lamps
            .len(),
        31
    );
}

/// Shrinking a path past the selected lamp moves the selection to the new
/// last lamp; it never vanishes.
#[test]
fn shrinking_past_the_selected_lamp_keeps_the_last_one() {
    let mut e = editor();
    e.select(vec![lamp("path3", "path", 29)]);
    key(&mut e, Key::Char('-'), none());
    assert_eq!(e.selection, vec![lamp("path3", "path", 28)]);
}

/// With many things selected, the inspector shows what they share, mixed
/// values say so, and an edit lands on every one.
#[test]
fn inspector_edits_everything_selected() {
    let mut e = editor();
    e.select(vec![comp("path1"), comp("path3"), ring("circle1", "r2")]);
    let props = common_props(&e.project, &e.selection);
    let lamps = props
        .iter()
        .find(|p| p.desc.id == "lamps")
        .expect("lamps is common");
    assert_eq!(lamps.value.describe(), "mixed 24–30");
    assert!(
        props.iter().all(|p| p.desc.id != "radius"),
        "paths have no radius"
    );

    e.edit_selection_prop("lamps", PropEdit::SetCount(12));
    let props = common_props(&e.project, &e.selection);
    assert_eq!(
        props
            .iter()
            .find(|p| p.desc.id == "lamps")
            .unwrap()
            .value
            .describe(),
        "12"
    );
}

/// A ring's properties are edited with the ring selected and stored on its
/// circle.
#[test]
fn ring_edits_land_in_the_circle() {
    let mut e = editor();
    e.select(vec![ring("circle1", "r2")]);
    e.edit_selection_prop("ccw", PropEdit::SetFlag(true));
    assert!(
        e.project
            .ring(&ComponentId::new("circle1"), "r2")
            .unwrap()
            .reversed
    );
    assert!(
        !e.project
            .ring(&ComponentId::new("circle1"), "r1")
            .unwrap()
            .reversed
    );
}

/// Reversing a path keeps the selected lamp on the same physical lamp.
#[test]
fn reverse_keeps_the_same_physical_lamp() {
    let mut e = editor();
    e.select(vec![lamp("path3", "path", 2)]);
    let before = lamp_pos(&e, "path3", "path", 2);
    key(&mut e, Key::Char('r'), none());
    let Target::Lamp { index, .. } = e.selection[0].clone() else {
        panic!()
    };
    assert_eq!(index, 27);
    assert!(lamp_pos(&e, "path3", "path", index).distance(before) < 1e-9);
}

/// Dragging a ring moves its whole circle.
#[test]
fn dragging_a_ring_moves_its_circle() {
    let mut e = editor();
    e.select(vec![ring("circle1", "r2")]);
    let at = lamp_screen(&e, "circle1", "r2", 0);
    let by = Vec2::new(5.0, 0.0) * e.camera.scale;
    e.pointer_down(at, Button::Primary, none());
    e.pointer_move(at + by * 0.5, none());
    e.pointer_move(at + by, none());
    e.pointer_up(at + by, none());
    let ComponentKind::Circle { center, .. } = &e
        .project
        .component(&ComponentId::new("circle1"))
        .unwrap()
        .kind
    else {
        panic!()
    };
    assert!((center.x - 40.0).abs() < 1e-6, "{center:?}");
}

/// Drawing a path puts it in the fixture its first point is in, gives it
/// one lamp every `spacing` units of its length, selects it, and goes back
/// to the select tool.
#[test]
fn drawing_a_path_counts_lamps_from_its_length() {
    let mut e = editor();
    draw_path(&mut e, "scarf", &[(10.0, 15.0), (34.0, 15.0)]);
    let Target::Component(id) = e.selection[0].clone() else {
        panic!()
    };
    assert_eq!(id.as_str(), "path4", "ids are never reused");
    assert_eq!(e.project.lamps_under(&e.selection[0]).len(), 11); // 24 / 2.4 + 1
    assert_eq!(e.tool, Tool::Select);
    assert_eq!(e.project.fixture_of(&id).unwrap().name, "scarf");
}

/// Clicking points with the path tool builds a path; double-click (or
/// Enter) finishes it; Esc takes back one point.
#[test]
fn clicking_points_builds_a_path() {
    let mut e = editor();
    key(&mut e, Key::Char('p'), none());
    assert_eq!(e.tool, Tool::Path);
    for (x, y) in [(10.0, 10.0), (50.0, 10.0), (50.0, 50.0), (90.0, 90.0)] {
        let at = place(&e, "scarf", x, y);
        click_at(&mut e, at, none());
    }
    assert_eq!(e.path_draft.len(), 4);
    // Esc takes back the last point; the rest stay.
    key(&mut e, Key::Escape, none());
    assert_eq!(e.path_draft.len(), 3);
    assert_eq!(e.tool, Tool::Path, "still drawing");
    // A double-click on the last point finishes: its two presses are one
    // point.
    let last = e.camera.to_screen(place(&e, "scarf", 50.0, 50.0));
    e.pointer_down(last, Button::Primary, none());
    e.pointer_up(last, none());
    assert!(e.double_click(last, none()));
    let Target::Component(id) = e.selection[0].clone() else {
        panic!("the new path is selected")
    };
    let points = e.project.path_points(&id).unwrap();
    let want = [(10.0, 10.0), (50.0, 10.0), (50.0, 50.0)];
    assert_eq!(points.len(), want.len(), "{points:?}");
    for (p, (x, y)) in points.iter().zip(want) {
        assert!(p.distance(Vec2::new(x, y)) < 1e-9, "{points:?}");
    }
    assert_eq!(e.tool, Tool::Select);
    assert!(e.path_draft.is_empty());

    // Enter finishes too; L is the same tool as P.
    key(&mut e, Key::Char('l'), none());
    assert_eq!(e.tool, Tool::Path);
    let at = place(&e, "scarf", 10.0, 80.0);
    click_at(&mut e, at, none());
    let at = place(&e, "scarf", 40.0, 80.0);
    click_at(&mut e, at, none());
    key(&mut e, Key::Enter, none());
    assert_eq!(e.selection, vec![comp("path5")]);

    // One point is no path: refused, and the draft is kept to go on from.
    key(&mut e, Key::Char('p'), none());
    let at = e.camera.to_screen(place(&e, "scarf", 70.0, 70.0));
    e.pointer_down(at, Button::Primary, none());
    e.pointer_down(at, Button::Primary, none());
    e.double_click(at, none());
    assert_eq!(e.notice.clone().unwrap().kind, NoticeKind::Refused);
    assert_eq!(e.path_draft.len(), 1);
    assert_eq!(e.tool, Tool::Path);
    // Esc takes back that point, and Esc again leaves the tool.
    key(&mut e, Key::Escape, none());
    assert!(e.path_draft.is_empty());
    key(&mut e, Key::Escape, none());
    assert_eq!(e.tool, Tool::Select);
}

/// A path's lamps are spread evenly along its length, first on its first
/// point and last on its last.
#[test]
fn a_paths_lamps_are_spread_along_its_length() {
    let mut e = editor();
    // An L: 30 across, then 18 down — 48 long, so 21 lamps 2.4 apart.
    draw_path(&mut e, "scarf", &[(10.0, 10.0), (40.0, 10.0), (40.0, 28.0)]);
    let Target::Component(id) = e.selection[0].clone() else {
        panic!()
    };
    let lamps = e
        .project
        .object(&ObjectId::new(&id, "path"))
        .unwrap()
        .lamps
        .iter()
        .map(|l| l.pos)
        .collect::<Vec<_>>();
    assert_eq!(lamps.len(), 21);
    let origin = place(&e, "scarf", 0.0, 0.0);
    assert!(lamps[0].distance(origin + Vec2::new(10.0, 10.0)) < 1e-9);
    assert!(lamps[20].distance(origin + Vec2::new(40.0, 28.0)) < 1e-9);
    // Evenly along the path: lamp 12 is 28.8 along, on the first leg;
    // lamp 13 is 31.2 along, 1.2 down the second. (To f32: the product's
    // arithmetic.)
    assert!(lamps[12].distance(origin + Vec2::new(38.8, 10.0)) < 1e-4);
    assert!(lamps[13].distance(origin + Vec2::new(40.0, 11.2)) < 1e-4);
}

/// Drawing that starts outside every fixture is refused, with where to
/// start instead.
#[test]
fn drawing_outside_every_fixture_is_refused() {
    let mut e = editor();
    key(&mut e, Key::Char('p'), none());
    let x = e.project.extent();
    click_at(&mut e, Vec2::new(x.max.x + 20.0, x.min.y), none());
    assert_eq!(e.notice.clone().unwrap().kind, NoticeKind::Refused);
    assert!(e.path_draft.is_empty(), "no point was added");
    assert_eq!(e.project.lamp_count(), 144);
}

/// Dragging a path's end point moves it and keeps its lamp count; one undo
/// puts it back.
#[test]
fn dragging_a_paths_end_keeps_its_count() {
    let mut e = editor();
    e.select(vec![comp("path3")]);
    let end = handle_at(
        &e,
        |k| matches!(k, HandleKind::PathPoint { path, index: 1 } if path.as_str() == "path3"),
    );
    e.pointer_move(end, none());
    let hint = e.hint_bar();
    assert_eq!(hint.hints[0].keys, "drag");
    assert!(
        hint.hints[0].does.contains("move this end"),
        "{:?}",
        hint.hints[0]
    );
    let before = e.project.clone();
    let by = Vec2::new(0.0, 20.0) * e.camera.scale;
    e.pointer_down(end, Button::Primary, none());
    e.pointer_move(end + by * 0.5, none());
    e.pointer_move(end + by, none());
    assert!(
        e.cursor_note()
            .unwrap()
            .starts_with("Dragging an end keeps 30 lamps"),
        "{:?}",
        e.cursor_note()
    );
    e.pointer_up(end + by, none());
    let points = e.project.path_points(&ComponentId::new("path3")).unwrap();
    assert!(
        points[1].distance(Vec2::new(74.0, 60.0)) < 1e-9,
        "{points:?}"
    );
    assert_eq!(points[0], Vec2::new(6.0, 40.0), "the other end stays");
    assert_eq!(e.project.lamps_under(&comp("path3")).len(), 30);
    assert_eq!(e.selection, vec![comp("path3")], "still selected");
    key(&mut e, Key::Char('z'), cmd());
    assert_eq!(e.project, before, "one drag, one undo");
}

/// A click on a handle is a click on what's under it: clicking a selected
/// path's end goes into the path, like clicking its first lamp anywhere.
#[test]
fn a_click_on_a_handle_clicks_whats_under_it() {
    let mut e = editor();
    e.select(vec![comp("path3")]);
    let start = handle_at(&e, |k| matches!(k, HandleKind::PathPoint { index: 0, .. }));
    let before = e.project.clone();
    e.pointer_down(start, Button::Primary, none());
    e.pointer_up(start, none());
    assert_eq!(e.selection, vec![lamp("path3", "path", 0)]);
    assert_eq!(e.project, before);
}

/// A path's handles show while one of its lamps is selected, and dragging
/// from a handle moves the point — even from under a selected lamp. A plain
/// click there still selects the lamp.
#[test]
fn a_paths_handles_show_while_a_lamp_of_it_is_selected() {
    let mut e = editor();
    e.select(vec![lamp("path3", "path", 0)]);
    let shown: Vec<HandleKind> = e.handles().into_iter().map(|h| h.kind).collect();
    let point = |index| HandleKind::PathPoint {
        path: ComponentId::new("path3"),
        index,
    };
    assert_eq!(shown, vec![point(0), point(1)]);
    // Lamp 0 sits on point 0, and is selected: a drag from there moves the
    // point, not the lamp's path.
    let start = handle_at(&e, |k| *k == point(0));
    let to = start + Vec2::new(0.0, 15.0) * e.camera.scale;
    drag(&mut e, start, to, none());
    let points = e.project.path_points(&ComponentId::new("path3")).unwrap();
    assert!(
        points[0].distance(Vec2::new(6.0, 55.0)) < 1e-9,
        "{points:?}"
    );
    assert_eq!(points[1], Vec2::new(74.0, 40.0), "the other end stays");
    assert_eq!(e.selection, vec![lamp("path3", "path", 0)]);
    key(&mut e, Key::Char('z'), cmd());
    // A click on the far end's handle selects the lamp there.
    let end = handle_at(&e, |k| *k == point(1));
    e.pointer_down(end, Button::Primary, none());
    e.pointer_up(end, none());
    assert_eq!(e.selection, vec![lamp("path3", "path", 29)]);
    // A ring's lamp shows its ring's handle.
    e.select(vec![lamp("circle1", "r2", 3)]);
    let shown: Vec<HandleKind> = e.handles().into_iter().map(|h| h.kind).collect();
    assert_eq!(
        shown,
        vec![HandleKind::RingRadius {
            circle: ComponentId::new("circle1"),
            ring: "r2".into()
        }]
    );
}

/// Adding a point from the menu or by double-clicking a segment adds one
/// point there and keeps the lamps; ⌥-click or the menu takes one away;
/// removing one below two points is refused.
#[test]
fn adding_and_removing_a_paths_points() {
    let mut e = editor();
    let path3 = ComponentId::new("path3");
    let points = |e: &Editor| e.project.path_points(&path3).unwrap().to_vec();
    let near = |a: &[Vec2], b: &[(f64, f64)]| {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(p, (x, y))| p.distance(Vec2::new(*x, *y)) < 1e-9)
    };
    // A double-click on an unshaped path only selects, as two clicks do.
    let at = e.camera.to_screen(place(&e, "strip", 30.0, 40.0));
    double_click(&mut e, at);
    assert_eq!(points(&e).len(), 2);

    // On the selected path's segment, it adds a point there.
    e.select(vec![comp("path3")]);
    double_click(&mut e, at);
    assert!(
        near(&points(&e), &[(6.0, 40.0), (30.0, 40.0), (74.0, 40.0)]),
        "{:?}",
        points(&e)
    );
    assert_eq!(e.selection, vec![comp("path3")]);
    assert_eq!(
        e.project.lamps_under(&comp("path3")).len(),
        30,
        "same lamps"
    );

    // Right-click on the second segment: "Add a point to path3 here".
    let at = e.camera.to_screen(place(&e, "strip", 50.0, 40.0));
    e.pointer_down(at, Button::Secondary, none());
    let menu = e.menu.clone().unwrap();
    let add = menu
        .actions
        .iter()
        .position(|a| a.label == "Add a point to path3 here")
        .expect("offered");
    e.menu_act(add);
    assert!(near(
        &points(&e),
        &[(6.0, 40.0), (30.0, 40.0), (50.0, 40.0), (74.0, 40.0)]
    ));

    // Right-click on a point's handle: "Remove point 2 of path3".
    let second = handle_at(&e, |k| matches!(k, HandleKind::PathPoint { index: 1, .. }));
    e.pointer_down(second, Button::Secondary, none());
    let menu = e.menu.clone().unwrap();
    assert_eq!(menu.actions[0].label, "Remove point 2 of path3");
    e.menu_act(0);
    assert!(near(
        &points(&e),
        &[(6.0, 40.0), (50.0, 40.0), (74.0, 40.0)]
    ));

    // ⌥-click on a point's handle takes it away…
    let middle = handle_at(&e, |k| matches!(k, HandleKind::PathPoint { index: 1, .. }));
    let w = e.camera.to_world(middle);
    click_at(&mut e, w, alt());
    assert!(near(&points(&e), &[(6.0, 40.0), (74.0, 40.0)]));
    // …but never below two.
    let end = handle_at(&e, |k| matches!(k, HandleKind::PathPoint { index: 1, .. }));
    let w = e.camera.to_world(end);
    click_at(&mut e, w, alt());
    assert_eq!(points(&e).len(), 2);
    let n = e.notice.clone().unwrap();
    assert_eq!(n.kind, NoticeKind::Refused);
    assert!(n.why.unwrap().contains("at least two points"));
}

/// Dragging a ring's handle changes its radius, and the ring keeps its
/// lamps.
#[test]
fn dragging_a_rings_handle_changes_its_radius() {
    let mut e = editor();
    e.select(vec![comp("circle1")]);
    let r2 = handle_at(
        &e,
        |k| matches!(k, HandleKind::RingRadius { ring, .. } if ring == "r2"),
    );
    let by = Vec2::new(-6.0, 0.0) * e.camera.scale;
    e.pointer_down(r2, Button::Primary, none());
    e.pointer_move(r2 + by * 0.5, none());
    e.pointer_move(r2 + by, none());
    assert_eq!(
        e.hint_bar().lead,
        "Changing the radius of ring r2 of circle1"
    );
    e.pointer_up(r2 + by, none());
    let circle = ComponentId::new("circle1");
    let ring = e.project.ring(&circle, "r2").unwrap();
    assert!((ring.radius - 10.0).abs() < 1e-9, "{}", ring.radius);
    assert_eq!(ring.lamps, 24);
    assert_eq!(e.project.ring(&circle, "r1").unwrap().radius, 26.0);
}

/// A new fixture is square; dragging its corner handle resizes it; ⇧ keeps
/// it square.
#[test]
fn a_new_fixture_is_square_and_its_corner_resizes_it() {
    let mut e = editor();
    let id = e.project.add_fixture(None, "new").unwrap();
    let size = e.project.fixture(&id).unwrap().size;
    assert_eq!(size.x, size.y, "square");
    assert_eq!(size.x, Fixture::DEFAULT_SIDE);

    let scarf = node_id(&e, "scarf");
    e.select(vec![Target::Node(scarf.clone())]);
    let corner = handle_at(&e, |k| {
        matches!(
            k,
            HandleKind::FixtureSize {
                edge: SizeEdge::Corner,
                ..
            }
        )
    });
    let by = Vec2::new(20.0, 10.0) * e.camera.scale;
    for (mods, want) in [(none(), (120.0, 110.0)), (shift(), (120.0, 120.0))] {
        e.pointer_down(corner, Button::Primary, mods);
        e.pointer_move(corner + by * 0.5, mods);
        e.pointer_move(corner + by, mods);
        e.pointer_up(corner + by, mods);
        let size = e.project.fixture(&scarf).unwrap().size;
        assert!(
            (size.x - want.0).abs() < 1e-9 && (size.y - want.1).abs() < 1e-9,
            "{mods:?}: {size:?}"
        );
        key(&mut e, Key::Char('z'), cmd());
        assert_eq!(
            e.project.fixture(&scarf).unwrap().size,
            Vec2::new(100.0, 100.0)
        );
    }
}

// ---- teaching ----------------------------------------------------------------

/// Hovering a lamp of the selected circle, the hint bar says a click goes
/// into it.
#[test]
fn hint_bar_says_a_click_goes_in() {
    let mut e = editor();
    e.select(vec![comp("circle1")]);
    e.pointer_move(lamp_screen(&e, "circle1", "r1", 3), none());
    let bar = e.hint_bar();
    let click = bar.hints.iter().find(|h| h.keys == "click").unwrap();
    assert!(click.does.contains("go into circle1"), "{}", click.does);
}

/// Holding ⌘ changes what the hint bar says a click does.
#[test]
fn holding_command_changes_the_click_hint() {
    let mut e = editor();
    e.pointer_move(lamp_screen(&e, "circle1", "r1", 3), cmd());
    let bar = e.hint_bar();
    assert!(
        bar.hints[0].keys == "⌘ click" && bar.hints[0].does.contains("lamp 3"),
        "{:?}",
        bar.hints[0]
    );
}

/// Every key the hint bar offers does something — for a selected ring, and
/// for a port, an output, a module and a fixture alike.
#[test]
fn every_offered_key_does_something() {
    let e0 = editor();
    let main = node_id(&e0, "main");
    let starts = [
        ring("circle1", "r2"),
        port(&main, "0"),
        Target::Node(main.clone()),
        fixture(&e0, "tail rig"),
        fixture(&e0, "strip"),
    ];
    for start in starts {
        let mut e = editor();
        e.select(vec![start.clone()]);
        let bar = e.hint_bar();
        if matches!(start, Target::Object(_)) {
            for keys in ["Esc", "Enter", "r", "− ="] {
                assert!(
                    bar.hints.iter().any(|h| h.keys == keys),
                    "{keys} is offered"
                );
            }
        }
        for (keys, key_) in [
            ("Esc", Key::Escape),
            ("Enter", Key::Enter),
            ("r", Key::Char('r')),
            ("− =", Key::Char('=')),
            ("⌫", Key::Delete),
        ] {
            if !bar.hints.iter().any(|h| h.keys == keys) {
                continue;
            }
            let mut probe = e.clone();
            let before = (probe.project.clone(), probe.selection.clone());
            probe.key_down(key_.clone(), none());
            assert_ne!(
                (probe.project, probe.selection),
                before,
                "{keys} changed something for {start:?}"
            );
        }
    }
}

// ---- the simulated board ----------------------------------------------------

/// With nothing selected, the board shows the pattern on every patched pixel
/// and black on unpatched ones.
#[test]
fn the_board_shows_the_pattern_on_patched_pixels_only() {
    let e = editor();
    let frame = e.device_frame(0.0);
    let tail_out = node_id(&e, "tail out");
    let gpio10 = frame.port(&tail_out, "0").unwrap();
    assert_eq!(gpio10.len(), 60);
    assert!(
        gpio10[..50].iter().all(|px| !px.is_black()),
        "the tail's 50 lamps"
    );
    assert!(
        gpio10[50..].iter().all(|px| px.is_black()),
        "10 spare pixels"
    );
    let main = node_id(&e, "main");
    for key in ["0", "1"] {
        assert!(
            frame
                .port(&main, key)
                .unwrap()
                .iter()
                .all(|px| !px.is_black())
        );
    }
    let scarf_out = node_id(&e, "scarf out");
    assert!(
        frame
            .port(&scarf_out, "0")
            .unwrap()
            .iter()
            .all(|px| px.is_black()),
        "nothing is drawn in the scarf yet"
    );
    assert!(!e.board_animates(), "nothing moves on its own");
}

/// Selecting a lamp lights exactly its pixel; every other patched pixel
/// dims.
#[test]
fn selecting_a_lamp_lights_exactly_its_pixel() {
    let mut e = editor();
    let plain = e.device_frame(CREST);
    e.select(vec![lamp("path3", "path", 4)]);
    assert!(e.board_animates());
    let frame = e.device_frame(CREST);
    let main = node_id(&e, "main");
    let d1 = frame.port(&main, "1").unwrap();
    assert_eq!(d1[4], Rgb::WHITE);
    for (port, plain_port) in frame.ports.iter().zip(&plain.ports) {
        for (i, (px, was)) in port.pixels.iter().zip(&plain_port.pixels).enumerate() {
            if port.output == main && port.key == "1" && i == 4 {
                continue;
            }
            assert!(
                !bright(*px),
                "{} {} pixel {i} stays dim",
                port.output,
                port.key
            );
            if !was.is_black() {
                assert!(
                    !px.is_black() && px.level() < was.level(),
                    "{} {} pixel {i} dims, not off",
                    port.output,
                    port.key
                );
            }
        }
    }
    // It breathes: dimmer at the start of a breath than at its crest, but
    // even then brighter than anything dimmed.
    let low = e.device_frame(0.0);
    let lowest = low.port(&main, "1").unwrap()[4];
    assert!(lowest.level() < Rgb::WHITE.level());
    let dimmed = low
        .ports
        .iter()
        .flat_map(|p| p.pixels.iter())
        .filter(|px| **px != lowest)
        .map(|px| px.level())
        .max()
        .unwrap();
    assert!(lowest.level() > dimmed, "never as dark as the rest");
}

/// A lamp outside its fixture's box is never lit, even when it is
/// selected.
#[test]
fn a_lamp_outside_its_box_is_never_lit() {
    let mut e = editor();
    // A path in the tail running out past its right side (the tail is 82
    // wide): lamps at x = 74, 79 inside; 84, 89, 94 outside. They land on
    // GPIO 10 at pixels 50–54.
    let tail = node_id(&e, "tail");
    let id = e
        .project
        .add(
            &tail,
            ComponentKind::path(vec![Vec2::new(74.0, 40.0), Vec2::new(94.0, 40.0)], 5),
        )
        .unwrap();
    let outside: Vec<u32> = e
        .project
        .lamps_outside()
        .into_iter()
        .filter(|(o, _)| o.component == id)
        .map(|(_, i)| i)
        .collect();
    assert_eq!(outside, vec![2, 3, 4]);
    let tail_out = node_id(&e, "tail out");
    let object = ObjectId::new(&id, "path");
    for selection in [
        vec![],
        vec![Target::Component(id.clone())],
        vec![Target::Lamp {
            object: object.clone(),
            index: 3,
        }],
    ] {
        e.select(selection.clone());
        let frame = e.device_frame(CREST);
        let gpio10 = frame.port(&tail_out, "0").unwrap();
        assert!(
            !gpio10[50].is_black() && !gpio10[51].is_black(),
            "{selection:?}"
        );
        for px in 52..55 {
            assert!(gpio10[px].is_black(), "pixel {px} dark with {selection:?}");
        }
    }
}

/// While drawing a path into the scarf, the scarf's port shows exactly the
/// draft's lamps, lit, at the right pixels — the ones the path takes when
/// it is finished — and everything else is off.
#[test]
fn drawing_into_the_scarf_lights_the_drafts_pixels() {
    let mut e = editor();
    let scarf_out = node_id(&e, "scarf out");
    // A first path, 11 lamps, takes pixels 0–10.
    draw_path(&mut e, "scarf", &[(10.0, 15.0), (34.0, 15.0)]);
    // The second, one point clicked and the cursor 24 units on: 11 more
    // lamps.
    key(&mut e, Key::Char('p'), none());
    let at = place(&e, "scarf", 40.0, 15.0);
    click_at(&mut e, at, none());
    let b = e.camera.to_screen(place(&e, "scarf", 64.0, 15.0));
    e.pointer_move(b, none());
    assert!(e.board_animates());
    let frame = e.device_frame(0.0);
    let d10 = frame.port(&scarf_out, "0").unwrap();
    let lit: Vec<usize> = (0..d10.len()).filter(|i| !d10[*i].is_black()).collect();
    assert_eq!(lit, (11..22).collect::<Vec<_>>());
    assert!(lit.iter().all(|i| d10[*i] == Rgb::WHITE));
    for port in &frame.ports {
        if port.output != scarf_out {
            assert!(
                port.pixels.iter().all(|px| px.is_black()),
                "{} is off",
                port.output
            );
        }
    }
    // Click there and double-click: the path lands exactly where the draft
    // showed.
    e.pointer_down(b, Button::Primary, none());
    e.pointer_up(b, none());
    e.pointer_down(b, Button::Primary, none());
    e.pointer_up(b, none());
    e.double_click(b, none());
    let Target::Component(id) = e.selection[0].clone() else {
        panic!()
    };
    let landed: Vec<usize> = e
        .project
        .effective_patch()
        .spans
        .iter()
        .filter(|s| s.object.component == id)
        .flat_map(|s| s.start_pixel as usize..(s.start_pixel + s.lamps.len() as u32) as usize)
        .collect();
    assert_eq!(landed, lit);
}

/// Pressing = on a selected path lights one more pixel on its port at once.
#[test]
fn pressing_equals_lights_one_more_pixel() {
    let mut e = editor();
    let tail_out = node_id(&e, "tail out");
    e.select(vec![comp("path1")]);
    // path1's red lead-out ends on its last pixel; path2's dimmed lamps
    // start on the next.
    let ends = |e: &Editor| {
        let frame = e.device_frame(CREST);
        let gpio10 = frame.port(&tail_out, "0").unwrap().to_vec();
        let last_red = gpio10
            .iter()
            .rposition(|px| *px == device_frame::END_COLOUR)
            .unwrap();
        let lit = gpio10.iter().rposition(|px| !px.is_black()).unwrap() + 1;
        (last_red, lit)
    };
    assert_eq!(ends(&e), (24, 50));
    key(&mut e, Key::Char('='), none());
    assert_eq!(ends(&e), (25, 51), "path1 has 26 lamps, path2 moved up one");
}

/// A selected object shows lp2014's pattern on the board: blue lead-in,
/// red lead-out, a white band sweeping start to end.
#[test]
fn a_selected_object_shows_lp2014s_pattern() {
    let mut e = editor();
    let main = node_id(&e, "main");
    // path3: 30 lamps on D1, so 3 lamps (an eighth, at least one, at most
    // 15) at each end.
    e.select(vec![comp("path3")]);
    let band = |t: f64| {
        let frame = e.device_frame(t);
        let d1 = frame.port(&main, "1").unwrap().to_vec();
        assert!(d1[..3].iter().all(|px| *px == device_frame::START_COLOUR));
        assert!(d1[27..].iter().all(|px| *px == device_frame::END_COLOUR));
        let middle = &d1[3..27];
        assert!(
            middle.iter().all(|px| px.r == px.g && px.g == px.b),
            "black and white between the ends at {t}"
        );
        let lit: Vec<usize> = (0..middle.len())
            .filter(|i| !middle[*i].is_black())
            .map(|i| i + 3)
            .collect();
        assert!(
            !lit.is_empty() && lit.len() <= 9,
            "a band, not the whole object, at {t}: {lit:?}"
        );
        assert!(
            lit.windows(2).all(|w| w[1] == w[0] + 1),
            "one band at {t}: {lit:?}"
        );
        let brightest = (3..27).max_by_key(|i| d1[*i].level()).unwrap();
        (lit[0], brightest)
    };
    let (a, a_peak) = band(0.5);
    let (b, b_peak) = band(1.0);
    let (c, c_peak) = band(1.5);
    assert!(a < b && b < c, "the band sweeps start → end");
    assert!(a_peak < b_peak && b_peak < c_peak);
    // The lead scales with the object: the tail's group, two paths of 25,
    // shows it on each — 3 at each end of both.
    e.select(vec![comp("group1")]);
    let frame = e.device_frame(0.5);
    let gpio10 = frame.port(&node_id(&e, "tail out"), "0").unwrap().to_vec();
    for start in [0, 25] {
        assert!(
            gpio10[start..start + 3]
                .iter()
                .all(|px| *px == device_frame::START_COLOUR)
        );
        assert!(
            gpio10[start + 22..start + 25]
                .iter()
                .all(|px| *px == device_frame::END_COLOUR)
        );
    }
    // And with lp2014's panels of 120, 15.
    assert_eq!(device_frame::lead_lamps(120), 15);
    assert_eq!(device_frame::lead_lamps(400), 15);
    assert_eq!(device_frame::lead_lamps(4), 1);
}

/// Every patched lamp on the canvas shows the same colour as its pixel on
/// the board; a lamp that lights nothing has no colour.
#[test]
fn every_patched_lamp_shows_its_pixels_colour() {
    let mut e = editor();
    // A path in the tail whose last lamps run outside its box.
    let tail = node_id(&e, "tail");
    let out = e
        .project
        .add(
            &tail,
            ComponentKind::path(vec![Vec2::new(74.0, 40.0), Vec2::new(94.0, 40.0)], 5),
        )
        .unwrap();
    // And 20 more lamps on the strip than D1 has pixels for.
    e.select(vec![comp("path3")]);
    e.edit_selection_prop("lamps", PropEdit::SetCount(50));
    let main = node_id(&e, "main");
    for (selection, t) in [
        (vec![], 0.0),
        (vec![comp("path3")], 0.7),
        (vec![lamp("circle1", "r2", 3)], CREST),
        (vec![port(&main, "0")], CREST),
    ] {
        e.select(selection.clone());
        let frame = e.device_frame(t);
        let patch = e.project.effective_patch();
        for s in &patch.spans {
            let pixels = frame.port(&s.output, &s.port).unwrap();
            for lamp in s.lamps.clone() {
                let pixel = pixels[(s.start_pixel + lamp - s.lamps.start) as usize];
                let shown = frame.lamp(&s.object, lamp);
                if s.object.component == out && lamp >= 2 {
                    assert_eq!(shown, None, "outside its box: no colour");
                } else {
                    assert_eq!(shown, Some(pixel), "{selection:?} {} {lamp}", s.object);
                }
            }
        }
        let strip = ObjectId::new(&ComponentId::new("path3"), "path");
        assert_eq!(frame.lamp(&strip, 30), None, "unpatched: no colour");
    }
}

/// Inside a fixture, the data path runs from the last lamp of each object
/// to the first lamp of the next, in patch order; data comes in where a
/// port starts.
#[test]
fn the_data_path_runs_object_to_object_in_patch_order() {
    let e = editor();
    let pos = |c: &str, k: &str, i: u32| lamp_pos(&e, c, k, i);
    let tail = e.project.data_path(&node_id(&e, "tail"));
    assert_eq!(
        tail.links,
        vec![(pos("path1", "path", 24), pos("path2", "path", 0))]
    );
    assert_eq!(tail.entries.len(), 1);
    assert_eq!(tail.entries[0].at, pos("path1", "path", 0));
    assert!(
        tail.entries[0].along.distance(Vec2::new(0.0, -1.0)) < 1e-9,
        "path1 runs up"
    );
    let ring = e.project.data_path(&node_id(&e, "ring display"));
    assert_eq!(
        ring.links,
        vec![(pos("circle1", "r1", 39), pos("circle1", "r2", 0))]
    );
    // The strip starts D1 on its own: data comes in, and goes nowhere else.
    let strip = e.project.data_path(&node_id(&e, "strip"));
    assert!(strip.links.is_empty());
    assert_eq!(strip.entries.len(), 1);

    // Reverse path1, and the wire runs from its new last lamp.
    let mut e = editor();
    e.select(vec![comp("path1")]);
    key(&mut e, Key::Char('r'), none());
    let tail = e.project.data_path(&node_id(&e, "tail"));
    assert_eq!(
        tail.links,
        vec![(
            lamp_pos(&e, "path1", "path", 24),
            lamp_pos(&e, "path2", "path", 0)
        )]
    );
    assert!(
        tail.links[0].0.y > tail.links[0].1.y,
        "from the bottom of path1 now"
    );
}

/// Deleting a path takes its pixels off the board at once; what came after
/// it moves up.
#[test]
fn deleting_a_path_changes_the_next_frame() {
    let mut e = editor();
    let tail_out = node_id(&e, "tail out");
    let before = e.device_frame(0.0).port(&tail_out, "0").unwrap().to_vec();
    e.select(vec![comp("path1")]);
    key(&mut e, Key::Delete, none());
    let after = e.device_frame(0.0).port(&tail_out, "0").unwrap().to_vec();
    assert_eq!(
        after[..25],
        before[25..50],
        "path2's lamps, now from pixel 0"
    );
    assert!(after[25..].iter().all(|px| px.is_black()));
}

/// Selecting a port lights its whole range, patched or not.
#[test]
fn selecting_a_port_lights_its_whole_range() {
    for (name, unpatched) in [("tail out", 10), ("scarf out", 60)] {
        let mut e = editor();
        let out = node_id(&e, name);
        e.select(vec![port(&out, "0")]);
        let frame = e.device_frame(CREST);
        let pixels = frame.port(&out, "0").unwrap();
        assert_eq!(pixels.len(), 60);
        assert!(
            pixels.iter().all(|px| bright(*px)),
            "all of {name}, {unpatched} unpatched"
        );
        for p in frame.ports.iter().filter(|p| p.output != out) {
            assert!(p.pixels.iter().all(|px| !bright(*px)), "{} dims", p.output);
        }
    }
}

/// Hovering an edge says what it carries, lights that edge and selects
/// nothing.
#[test]
fn hovering_an_edge_says_what_it_carries() {
    let mut e = editor();
    let at = clear_point_on_edge(&e, &comp("path1"));
    e.pointer_move(at, none());
    assert_eq!(
        e.cursor_note().as_deref(),
        Some("path1 → GPIO 10 · lamps 0–24 at pixel 0")
    );
    let hovered: Vec<Target> = e
        .edges()
        .into_iter()
        .filter(|x| x.hovered)
        .map(|x| x.from)
        .collect();
    assert_eq!(hovered, vec![comp("path1")]);
    assert!(e.selection.is_empty(), "hovering selects nothing");

    // With something selected, the same.
    e.select(vec![comp("path2")]);
    let at = clear_point_on_edge(&e, &ring("circle1", "r2"));
    e.pointer_move(at, none());
    assert_eq!(
        e.cursor_note().as_deref(),
        Some("ring r2 → D0 · lamps 0–23 at pixel 40")
    );
    assert_eq!(e.selection, vec![comp("path2")]);

    // Away from every edge, no note.
    e.pointer_move(e.camera.to_screen(place(&e, "scarf", 50.0, 15.0)), none());
    assert!(e.edges().iter().all(|x| !x.hovered));
    assert_eq!(e.cursor_note(), None);
}

/// The bar's lead says what you're doing, never what's selected: empty
/// while you are plainly selecting.
#[test]
fn the_bar_lead_says_what_youre_doing() {
    let mut e = editor();
    for selection in [
        vec![],
        vec![comp("circle1")],
        vec![lamp("path3", "path", 2)],
    ] {
        e.select(selection.clone());
        assert_eq!(e.hint_bar().lead, "", "{selection:?}");
    }
    // Moving circle1.
    e.select(vec![comp("circle1")]);
    let at = lamp_screen(&e, "circle1", "r1", 0);
    e.pointer_down(at, Button::Primary, none());
    e.pointer_move(at + Vec2::new(20.0, 0.0), none());
    assert_eq!(e.hint_bar().lead, "Moving circle1");
    key(&mut e, Key::Escape, none());
    // Box-selecting.
    let empty = e.camera.to_screen(place(&e, "scarf", 50.0, 15.0));
    e.pointer_down(empty, Button::Primary, none());
    e.pointer_move(empty + Vec2::new(20.0, 20.0), none());
    assert_eq!(e.hint_bar().lead, "Box-selecting");
    e.pointer_up(empty + Vec2::new(20.0, 20.0), none());
    // Choosing from the right-click list.
    e.pointer_down(
        lamp_screen(&e, "path3", "path", 3),
        Button::Secondary,
        none(),
    );
    assert_eq!(e.hint_bar().lead, "Choosing what's under the cursor");
    key(&mut e, Key::Escape, none());
    // Drawing a path.
    key(&mut e, Key::Char('p'), none());
    assert_eq!(e.hint_bar().lead, "Path tool");
    e.pointer_down(empty, Button::Primary, none());
    e.pointer_up(empty, none());
    e.pointer_move(empty + Vec2::new(30.0, 0.0), none());
    assert_eq!(e.hint_bar().lead, "Drawing a path");
    key(&mut e, Key::Escape, none());
    key(&mut e, Key::Escape, none());
    // Resizing a fixture by its corner.
    e.select(vec![fixture(&e, "scarf")]);
    let corner = handle_at(&e, |k| {
        matches!(
            k,
            HandleKind::FixtureSize {
                edge: SizeEdge::Corner,
                ..
            }
        )
    });
    e.pointer_down(corner, Button::Primary, none());
    e.pointer_move(corner + Vec2::new(20.0, 20.0), none());
    assert_eq!(e.hint_bar().lead, "Resizing scarf");
}

// ---- Move & draw -------------------------------------------------------------

/// With Move & draw off, nothing moves, draws or reshapes, and each try
/// says the toggle is off and how to turn it on; selection, pixels and the
/// board still work.
#[test]
fn with_move_and_draw_off_nothing_moves() {
    let mut e = editor();
    key(&mut e, Key::Char('m'), none());
    assert!(!e.move_draw);
    let before = e.project.clone();
    let refused = |e: &Editor| {
        let n = e.notice.clone().expect("a notice");
        n.kind == NoticeKind::Refused
            && n.why.as_deref() == Some("Move & draw is off")
            && n.help.unwrap_or_default().starts_with("M turns it on")
    };

    // Selecting works: a click selects circle1, and again its ring.
    click_lamp(&mut e, "circle1", "r1", 3, none());
    assert_eq!(e.selection, vec![comp("circle1")]);
    assert!(e.handles().is_empty(), "no handles");
    // Dragging it moves nothing.
    let at = lamp_screen(&e, "circle1", "r1", 3);
    drag(&mut e, at, at + Vec2::new(40.0, 0.0), none());
    assert!(refused(&e), "{:?}", e.notice);
    assert_eq!(e.project, before);
    // Nor does dragging a node by its title: nothing is pinned.
    let title = e.camera.to_screen(place(&e, "strip", 0.0, 0.0)) + Vec2::new(12.0, -10.0);
    drag(&mut e, title, title + Vec2::new(0.0, 60.0), none());
    assert!(refused(&e));
    assert_eq!(e.project, before);
    // No drawing tools.
    key(&mut e, Key::Char('p'), none());
    assert_eq!(e.tool, Tool::Select);
    assert!(refused(&e));
    // No count or size that moves lamps.
    e.select(vec![comp("path3")]);
    key(&mut e, Key::Char('='), none());
    assert!(refused(&e));
    e.select(vec![fixture(&e, "scarf")]);
    e.edit_selection_prop("width", PropEdit::SetNumber(140.0));
    assert!(refused(&e));
    // No points added by double-click.
    e.select(vec![comp("path3")]);
    let at = e.camera.to_screen(place(&e, "strip", 30.0, 40.0));
    double_click(&mut e, at);
    assert!(refused(&e));
    assert_eq!(e.project, before, "nothing changed");
    let bar = e.hint_bar();
    assert!(
        bar.hints
            .iter()
            .all(|h| h.keys != "drag" && h.keys != "P · O"),
        "the bar offers no moving: {:?}",
        bar.hints
    );
    assert!(bar.hints.iter().any(|h| h.keys == "M"));

    // A pixel still lights; a port's own count is patching, not drawing.
    let main = node_id(&e, "main");
    e.select(vec![pixel(&main, "1", 3)]);
    assert!(bright(e.device_frame(CREST).port(&main, "1").unwrap()[3]));
    e.select(vec![port(&main, "1")]);
    key(&mut e, Key::Char('='), none());
    assert_eq!(e.project.port_fill(&main, "1"), (30, 31));

    // M turns it back on, and a drag moves again.
    key(&mut e, Key::Char('m'), none());
    e.select(vec![comp("circle1")]);
    let at = lamp_screen(&e, "circle1", "r1", 3);
    drag(&mut e, at, at + Vec2::new(40.0, 0.0), none());
    assert_ne!(
        e.project.component(&ComponentId::new("circle1")),
        before.component(&ComponentId::new("circle1"))
    );
}

// ---- pixels --------------------------------------------------------------------

/// Clicking into a port selects one of its pixels; the arrow keys walk the
/// pixels, and ⇧-click selects the run from the first one picked.
#[test]
fn clicking_into_a_port_selects_a_pixel() {
    let mut e = editor();
    let main = node_id(&e, "main");
    let at = pixel_centre(&e, &main, "1", 3);
    click_at(&mut e, at, none());
    assert_eq!(e.selection, vec![Target::Node(main.clone())]);
    click_at(&mut e, at, none());
    assert_eq!(e.selection, vec![port(&main, "1")]);
    click_at(&mut e, at, none());
    assert_eq!(e.selection, vec![pixel(&main, "1", 3)]);
    assert_eq!(
        e.project.long_label(&e.selection[0]),
        "pixel 3 of D1 of main"
    );
    key(&mut e, Key::Right, none());
    assert_eq!(e.selection, vec![pixel(&main, "1", 4)]);
    let at = pixel_centre(&e, &main, "1", 8);
    click_at(&mut e, at, shift());
    assert_eq!(
        e.selection,
        (4..=8).map(|i| pixel(&main, "1", i)).collect::<Vec<_>>()
    );
    // A pixel comes from its port's count: it can't be deleted on its own.
    key(&mut e, Key::Delete, none());
    assert_eq!(e.notice.clone().unwrap().kind, NoticeKind::Refused);
}

/// Selecting a port pixel lights it on the board, even with nothing patched
/// to it; the lamp patched to it is marked, and the other way round.
#[test]
fn a_selected_pixel_lights_and_marks_its_lamp() {
    let mut e = editor();
    // Nothing is drawn in the scarf: pixel 5 of D10 is patched to nothing.
    let scarf_out = node_id(&e, "scarf out");
    e.select(vec![pixel(&scarf_out, "0", 5)]);
    let frame = e.device_frame(CREST);
    let d10 = frame.port(&scarf_out, "0").unwrap();
    assert!(bright(d10[5]), "it lights");
    for (i, px) in d10.iter().enumerate() {
        assert!(i == 5 || !bright(*px), "only pixel 5 of D10, not {i}");
    }
    assert!(e.marked_lamps().is_empty(), "no lamp to mark");

    // Pixel 3 of D1 carries lamp 3 of path3: that lamp is marked.
    let main = node_id(&e, "main");
    e.select(vec![pixel(&main, "1", 3)]);
    assert_eq!(e.marked_lamps(), vec![lamp("path3", "path", 3)]);
    let frame = e.device_frame(CREST);
    assert!(bright(frame.port(&main, "1").unwrap()[3]));
    // And the other way round: select the lamp, and its pixel is marked.
    e.select(vec![lamp("path3", "path", 3)]);
    assert_eq!(e.marked_pixels(), vec![pixel(&main, "1", 3)]);
    // r2's first lamp lands on D0 after r1's 40.
    e.select(vec![lamp("circle1", "r2", 0)]);
    assert_eq!(e.marked_pixels(), vec![pixel(&main, "0", 40)]);
}

// ---- the product's shapes and rules (real projects) ---------------------------

/// A path's gaps are jumper wire: they carry no lamps, and the lamps keep
/// one pitch across the rest, as on the device.
#[test]
fn a_paths_gaps_carry_no_lamps() {
    let (mut p, f, _) = bare(&[("D0", 60)]);
    // Out along the top, a jumper down (segment 1), on along the bottom:
    // 20 units that carry lamps, so 5 lamps sit 5 apart.
    let path = add(
        &mut p,
        &f,
        ComponentKind::Path {
            points: pts(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (20.0, 10.0)]),
            lamps: 5,
            reversed: false,
            gaps: vec![1],
        },
    );
    assert_eq!(
        local_lamps(&p, &path, 0),
        pts(&[
            (0.0, 0.0),
            (5.0, 0.0),
            (10.0, 0.0),
            (15.0, 10.0),
            (20.0, 10.0)
        ])
    );
}

/// Reversing a path flips which end its data enters, and never moves a
/// point: its lamp 0 is now on its last point.
#[test]
fn reversing_a_path_never_moves_a_point() {
    let mut e = editor();
    let path3 = ComponentId::new("path3");
    let before = e.project.path_points(&path3).unwrap().to_vec();
    e.select(vec![comp("path3")]);
    key(&mut e, Key::Char('r'), none());
    assert_eq!(e.project.path_points(&path3).unwrap(), before.as_slice());
    assert!(matches!(
        e.project.component(&path3).unwrap().kind,
        ComponentKind::Path { reversed: true, .. }
    ));
    assert!(lamp_pos(&e, "path3", "path", 0).distance(place(&e, "strip", 74.0, 40.0)) < 1e-9);
}

/// A one-lamp path's lamp sits on its first point, where the device puts
/// it.
#[test]
fn a_one_lamp_paths_lamp_sits_on_its_start() {
    let (mut p, f, _) = bare(&[("D0", 60)]);
    let path = add(
        &mut p,
        &f,
        ComponentKind::path(pts(&[(10.0, 10.0), (30.0, 10.0)]), 1),
    );
    assert_eq!(local_lamps(&p, &path, 0), pts(&[(10.0, 10.0)]));
}

/// A polygon is closed: lamp 0 sits on its first point and the rest go
/// evenly round it, the side back to the start included — a square of 8
/// has 2 a side, one on each corner.
#[test]
fn a_polygon_is_closed_and_starts_on_its_first_point() {
    let (mut p, f, _) = bare(&[("D0", 60)]);
    let square = add(
        &mut p,
        &f,
        ComponentKind::polygon(
            pts(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
            8,
        ),
    );
    assert_eq!(
        local_lamps(&p, &square, 0),
        pts(&[
            (0.0, 0.0),
            (5.0, 0.0),
            (10.0, 0.0),
            (10.0, 5.0),
            (10.0, 10.0),
            (5.0, 10.0),
            (0.0, 10.0),
            (0.0, 5.0)
        ])
    );
    assert_eq!(p.kind_word(&Target::Component(square)), "polygon");
}

/// A repeat makes one object per copy, keyed by its number (the product's
/// `/sector/2`): copy k is its shape turned k · 360° / count, clockwise,
/// about its centre. Each copy is its own strand.
#[test]
fn a_repeat_makes_one_copy_per_turn() {
    let (mut p, f, _) = bare(&[("D0", 60)]);
    let sector = add(
        &mut p,
        &f,
        ComponentKind::repeat(
            ComponentKind::path(pts(&[(50.0, 10.0), (50.0, 30.0)]), 3),
            Vec2::new(50.0, 50.0),
            4,
        ),
    );
    let t = Target::Component(sector.clone());
    let copies = p.children(Some(&t));
    let keys: Vec<String> = copies
        .iter()
        .map(|c| match c {
            Target::Object(o) => o.key.clone(),
            _ => panic!("a copy is an object"),
        })
        .collect();
    assert_eq!(keys, ["0", "1", "2", "3"]);
    assert_eq!(p.label(&copies[2]), "copy 2");
    // Copy 1 is turned a quarter clockwise: from above the centre to its
    // right.
    let first = local_lamps(&p, &sector, 1)[0];
    assert!(first.distance(Vec2::new(90.0, 50.0)) < 1e-4, "{first:?}");
    assert_eq!(p.lamps_under(&t).len(), 12);
}

/// A repeat's area is its copies' outlines, not the space between them:
/// round a dome, the middle belongs to whatever is there.
#[test]
fn a_repeat_is_picked_by_its_copies() {
    let (mut p, f, _) = bare(&[("D0", 60)]);
    let sector = add(
        &mut p,
        &f,
        ComponentKind::repeat(
            ComponentKind::path(pts(&[(50.0, 10.0), (50.0, 30.0)]), 3),
            Vec2::new(50.0, 50.0),
            4,
        ),
    );
    let mut e = Editor::new(p);
    e.resize(Vec2::new(1200.0, 800.0));
    e.fit();
    let origin = e.project.origin(&f);
    let t = Target::Component(sector);
    let on_copy = e.camera.to_screen(origin + Vec2::new(50.0, 20.0));
    let middle = e.camera.to_screen(origin + Vec2::new(50.0, 50.0));
    assert!(e.project.area(&e.camera, &t).unwrap().contains(on_copy));
    assert!(!e.project.area(&e.camera, &t).unwrap().contains(middle));
}

/// A copy's lamps live in its repeat (D11): changing them on one copy
/// changes every copy.
#[test]
fn a_copys_lamps_live_in_its_repeat() {
    let (mut p, f, _) = bare(&[("D0", 60)]);
    let sector = add(
        &mut p,
        &f,
        ComponentKind::repeat(
            ComponentKind::path(pts(&[(50.0, 10.0), (50.0, 30.0)]), 3),
            Vec2::new(50.0, 50.0),
            4,
        ),
    );
    let copy = Target::Object(ObjectId::new(&sector, "2"));
    assert!(p.edit_prop(&copy, "lamps", PropEdit::SetCount(7), 2.4));
    for k in 0..4 {
        assert_eq!(local_lamps(&p, &sector, k).len(), 7);
    }
}

/// A copy can't be deleted on its own: ⌫ says it comes from its repeat's
/// count, and how to change that.
#[test]
fn a_copy_cant_be_deleted_on_its_own() {
    let (mut p, f, _) = bare(&[("D0", 60)]);
    let sector = add(
        &mut p,
        &f,
        ComponentKind::repeat(
            ComponentKind::path(pts(&[(50.0, 10.0), (50.0, 30.0)]), 3),
            Vec2::new(50.0, 50.0),
            4,
        ),
    );
    let mut e = Editor::new(p);
    e.select(vec![Target::Object(ObjectId::new(&sector, "2"))]);
    key(&mut e, Key::Delete, none());
    assert_eq!(
        e.project.children(Some(&Target::Component(sector))).len(),
        4
    );
    let n = e.notice.clone().unwrap();
    assert!(n.why.unwrap().contains("copies from a count"));
}

/// A place rule puts one object's lamps on a port, starting on a pixel.
/// Reversed lays them end first, and a turn rotates them inside their own
/// window: orientation first, then rotation, as on the device — five
/// lamps at pixel 10 turned 2 land on 12, 13, 14, 10, 11; reversed too, on
/// 11, 10, 14, 13, 12.
#[test]
fn a_place_rule_puts_an_object_on_a_pixel() {
    for (reversed, want) in [(false, [12, 13, 14, 10, 11]), (true, [11, 10, 14, 13, 12])] {
        let (mut p, f, out) = bare(&[("D0", 40)]);
        let path = add(
            &mut p,
            &f,
            ComponentKind::path(pts(&[(10.0, 10.0), (50.0, 10.0)]), 5),
        );
        let object = ObjectId::new(&path, "path");
        p.fixture_mut(&f).unwrap().rules.push(PatchRule::Place {
            object: object.clone(),
            output: out.clone(),
            port: "0".into(),
            pixel: 10,
            reversed,
            rotation: 2,
        });
        let got: Vec<u32> = (0..5)
            .map(|i| match p.pixel_of_lamp(&object, i) {
                Some(Target::Pixel { index, .. }) => index,
                other => panic!("lamp {i}: {other:?}"),
            })
            .collect();
        assert_eq!(got, want, "reversed: {reversed}");
    }
}

/// An output's ports make one flat wire, in key order: a run that passes
/// the end of one port carries on into the next.
#[test]
fn a_run_carries_on_into_the_next_port() {
    let (mut p, f, out) = bare(&[("D0", 4), ("D1", 10)]);
    add(
        &mut p,
        &f,
        ComponentKind::path(pts(&[(10.0, 10.0), (50.0, 10.0)]), 6),
    );
    p.auto_patch(&f, &out);
    let spans: Vec<(String, std::ops::Range<u32>, u32)> = p
        .effective_patch()
        .spans
        .iter()
        .map(|s| (s.port.clone(), s.lamps.clone(), s.start_pixel))
        .collect();
    assert_eq!(spans, vec![("0".into(), 0..4, 0), ("1".into(), 4..6, 0)]);
}

/// The last port may have no count: it takes the rest — as many pixels as
/// land on it.
#[test]
fn a_last_port_without_a_count_takes_the_rest() {
    let (mut p, f, out) = bare(&[("D0", 4)]);
    let key = p.output_mut(&out).unwrap().add_port("D1", None);
    add(
        &mut p,
        &f,
        ComponentKind::path(pts(&[(10.0, 10.0), (50.0, 10.0)]), 10),
    );
    p.auto_patch(&f, &out);
    assert_eq!(p.port_fill(&out, &key), (6, 6));
    assert!(p.effective_patch().unpatched.is_empty());
}

/// Auto flows on after every placed run on its output. A fixture with
/// place rules too flows its other lamps on after its own runs; a fixture
/// with only an auto rule, after everything.
#[test]
fn auto_flows_after_placed_runs() {
    let (mut p, a, out) = bare(&[("D0", 40)]);
    let a1 = add(
        &mut p,
        &a,
        ComponentKind::path(pts(&[(0.0, 0.0), (9.0, 0.0)]), 3),
    );
    let a2 = add(
        &mut p,
        &a,
        ComponentKind::path(pts(&[(0.0, 5.0), (9.0, 5.0)]), 2),
    );
    p.fixture_mut(&a).unwrap().rules = vec![
        PatchRule::Place {
            object: ObjectId::new(&a2, "path"),
            output: out.clone(),
            port: "0".into(),
            pixel: 10,
            reversed: false,
            rotation: 0,
        },
        PatchRule::Auto {
            output: out.clone(),
        },
    ];
    let b = p.add_fixture(None, "b").unwrap();
    let b1 = add(
        &mut p,
        &b,
        ComponentKind::path(pts(&[(0.0, 0.0), (9.0, 0.0)]), 2),
    );
    p.auto_patch(&b, &out);
    let at = |c: &ComponentId| {
        p.effective_patch()
            .spans
            .iter()
            .find(|s| &s.object.component == c)
            .map(|s| s.start_pixel)
    };
    assert_eq!(at(&a2), Some(10));
    assert_eq!(at(&a1), Some(12), "after a's own run");
    assert_eq!(at(&b1), Some(15), "after every run");
}

/// A fixture with no auto rule is manual: a lamp no rule places is dark.
#[test]
fn without_an_auto_rule_unplaced_lamps_are_dark() {
    let (mut p, f, out) = bare(&[("D0", 40)]);
    let a1 = add(
        &mut p,
        &f,
        ComponentKind::path(pts(&[(0.0, 0.0), (9.0, 0.0)]), 3),
    );
    let a2 = add(
        &mut p,
        &f,
        ComponentKind::path(pts(&[(0.0, 5.0), (9.0, 5.0)]), 2),
    );
    p.fixture_mut(&f).unwrap().rules = vec![PatchRule::Place {
        object: ObjectId::new(&a2, "path"),
        output: out.clone(),
        port: "0".into(),
        pixel: 0,
        reversed: false,
        rotation: 0,
    }];
    let patch = p.effective_patch();
    assert_eq!(patch.spans.len(), 1);
    assert_eq!(patch.unpatched.len(), 1);
    assert_eq!(patch.unpatched[0].object, ObjectId::new(&a1, "path"));
}

/// Fixtures sharing an output fill it in the product's order — each
/// module's nodes by name, depth first — not the order they were added in.
#[test]
fn fixtures_share_an_output_in_name_order() {
    let mut p = Project::new("t");
    let zebra = p.add_fixture(None, "zebra").unwrap();
    let apple = p.add_fixture(None, "apple").unwrap();
    let out = p.add_output(None, "out", &[("D0", 40)]).unwrap();
    for f in [&zebra, &apple] {
        add(
            &mut p,
            f,
            ComponentKind::path(pts(&[(0.0, 0.0), (9.0, 0.0)]), 3),
        );
        p.auto_patch(f, &out);
    }
    let start = |f: &NodeId| {
        p.effective_patch()
            .spans
            .iter()
            .find(|s| &s.fixture == f)
            .map(|s| s.start_pixel)
    };
    assert_eq!(start(&apple), Some(0));
    assert_eq!(start(&zebra), Some(3));
}

/// A rule placing lamps on a port that's gone is a problem; it is never
/// moved to another port, and its lamps fall to the next rule.
#[test]
fn a_rule_on_a_gone_port_is_a_problem() {
    let (mut p, f, out) = bare(&[("D0", 40)]);
    let a1 = add(
        &mut p,
        &f,
        ComponentKind::path(pts(&[(0.0, 0.0), (9.0, 0.0)]), 3),
    );
    p.fixture_mut(&f).unwrap().rules = vec![
        PatchRule::Place {
            object: ObjectId::new(&a1, "path"),
            output: out.clone(),
            port: "7".into(),
            pixel: 0,
            reversed: false,
            rotation: 0,
        },
        PatchRule::Auto {
            output: out.clone(),
        },
    ];
    let patch = p.effective_patch();
    assert_eq!(patch.problems.len(), 1);
    assert_eq!(patch.problems[0].gone, Gone::Port("7".into()));
    assert_eq!(patch.spans.len(), 1, "auto still places it");
    assert!(patch.problems[0].notice(&p).why.is_some());
}

/// The colour order belongs to the fixture, as in the product; a port
/// shows every order that lands on it.
#[test]
fn colour_order_belongs_to_the_fixture() {
    let mut p = Project::new("t");
    let a = p.add_fixture(None, "a").unwrap();
    let b = p.add_fixture(None, "b").unwrap();
    let out = p.add_output(None, "out", &[("D0", 40)]).unwrap();
    for f in [&a, &b] {
        add(
            &mut p,
            f,
            ComponentKind::path(pts(&[(0.0, 0.0), (9.0, 0.0)]), 3),
        );
        p.auto_patch(f, &out);
    }
    assert_eq!(
        p.fixture(&a).unwrap().color_order,
        ColorOrder::Grb,
        "the default"
    );
    p.fixture_mut(&b).unwrap().color_order = ColorOrder::Rgb;
    assert_eq!(
        p.color_orders_on(&out, "0"),
        vec![ColorOrder::Grb, ColorOrder::Rgb]
    );
}

/// A node the lab doesn't edit (a shader, a clock) is a plain card at the
/// left of what it feeds: it selects and deletes like any node, and holds
/// nothing.
#[test]
fn a_node_the_lab_doesnt_edit_is_a_plain_card() {
    let mut e = editor();
    let shader = e
        .project
        .add_node(None, "shader", NodeKind::Other(OtherNode::new("Shader")))
        .unwrap();
    let t = Target::Node(shader.clone());
    assert!(!e.project.is_frame(&t));
    assert!(e.project.children(Some(&t)).is_empty());
    let card = e.project.node_rect(&shader).unwrap();
    for n in e.project.nodes().iter().filter(|n| !n.is_card()) {
        assert!(
            card.max.x < e.project.node_rect(&n.id).unwrap().min.x,
            "{}",
            n.name
        );
    }
    e.select(vec![t]);
    key(&mut e, Key::Delete, none());
    assert!(e.project.node(&shader).is_none());
}

/// Minting never hands out an id already in the project — one opened from
/// a file, say.
#[test]
fn minting_skips_ids_already_taken() {
    let (mut p, f, _) = bare(&[("D0", 40)]);
    p.fixture_mut(&f).unwrap().components.push(Component {
        id: ComponentId::new("path1"),
        kind: ComponentKind::path(pts(&[(0.0, 0.0), (9.0, 0.0)]), 3),
    });
    let next = add(
        &mut p,
        &f,
        ComponentKind::path(pts(&[(0.0, 5.0), (9.0, 5.0)]), 3),
    );
    assert_eq!(next.as_str(), "path2");
}

// ---- kept answers --------------------------------------------------------------

/// What the model works out about a project — where every node sits, every
/// lamp, the patch, which pixel each lamp lights — is kept between
/// questions, and an edit shows at once: after a count change, a resize, a
/// move, a deleted output and an undo, every answer is what working it out
/// afresh gives.
#[test]
fn kept_answers_are_never_stale() {
    let mut e = Editor::new(Project::seed());
    let main = node_id(&e, "main");
    let ring_display = node_id(&e, "ring display");
    let same_as_afresh = |e: &Editor| {
        let mut afresh = e.project.clone();
        afresh.nodes_mut(); // a `&mut` way in forgets what was kept
        assert_eq!(e.project.layout(), afresh.layout());
        assert_eq!(e.project.objects(), afresh.objects());
        assert_eq!(e.project.effective_patch(), afresh.effective_patch());
        assert_eq!(
            e.project.device_frame(&[], 0.5),
            afresh.device_frame(&[], 0.5)
        );
        let ring = ObjectId::new(&ComponentId::new("circle1"), "r1");
        for i in 0..44 {
            assert_eq!(
                e.project.device_frame(&[], 0.5).lamp(&ring, i),
                afresh.device_frame(&[], 0.5).lamp(&ring, i)
            );
        }
    };
    same_as_afresh(&e);
    let lamps = e.project.lamp_count();
    let patch = e.project.effective_patch().clone();

    // A count: four more lamps on the outer ring, on the patch at once (the
    // strip after it moves along the wire).
    e.select(vec![ring("circle1", "r1")]);
    e.edit_selection_prop("lamps", PropEdit::Nudge(4));
    assert_eq!(e.project.lamp_count(), lamps + 4);
    assert_ne!(*e.project.effective_patch(), patch);
    same_as_afresh(&e);

    // A resize: the fixture's box, and the output beside it, move.
    let card = e.project.node_rect(&main).unwrap();
    e.select(vec![Target::Node(ring_display.clone())]);
    e.edit_selection_prop("width", PropEdit::SetNumber(120.0));
    assert_eq!(e.project.node_rect(&ring_display).unwrap().width(), 120.0);
    assert!(e.project.node_rect(&main).unwrap().min.x > card.min.x);
    same_as_afresh(&e);

    // A move: the circle's lamps go with it.
    let before = lamp_pos(&e, "circle1", "r1", 0);
    e.select(vec![comp("circle1")]);
    let at = e.camera.to_screen(before);
    e.pointer_move(at, none());
    e.pointer_down(at, Button::Primary, none());
    e.pointer_move(at + Vec2::new(30.0, 0.0), none());
    e.pointer_up(at + Vec2::new(30.0, 0.0), none());
    assert!(lamp_pos(&e, "circle1", "r1", 0).x > before.x);
    same_as_afresh(&e);

    // A deleted output: its patch is a problem, its pixels gone.
    e.select(vec![Target::Node(main.clone())]);
    key(&mut e, Key::Delete, none());
    assert!(e.project.output(&main).is_none());
    assert!(!e.project.effective_patch().problems.is_empty());
    same_as_afresh(&e);

    // Undo all the way back: the first answers again.
    while e.can_undo() {
        e.undo();
        same_as_afresh(&e);
    }
    assert_eq!(e.project.lamp_count(), lamps);
    assert_eq!(*e.project.effective_patch(), patch);
}

// ---- helpers -------------------------------------------------------------------

/// Every pair of nodes at one level whose boxes overlap, in words: empty
/// when the layout is clean.
fn overlaps(p: &Project) -> Vec<String> {
    let mut levels: Vec<Vec<&Node>> = vec![p.nodes().iter().collect()];
    for n in p.all_nodes() {
        if !n.children().is_empty() {
            levels.push(n.children().iter().collect());
        }
    }
    let mut out = Vec::new();
    for level in levels {
        let boxes: Vec<(String, Rect)> = level
            .iter()
            .map(|n| (n.name.clone(), p.node_rect(&n.id).unwrap()))
            .collect();
        for (i, (na, a)) in boxes.iter().enumerate() {
            for (nb, b) in &boxes[i + 1..] {
                let apart = a.max.x <= b.min.x
                    || b.max.x <= a.min.x
                    || a.max.y <= b.min.y
                    || b.max.y <= a.min.y;
                if !apart {
                    out.push(format!("{na} {a:?} overlaps {nb} {b:?}"));
                }
            }
        }
    }
    out
}

/// A double-click as the browser sends it: two clicks, then the event.
fn double_click(e: &mut Editor, at: Vec2) {
    for _ in 0..2 {
        e.pointer_move(at, none());
        e.pointer_down(at, Button::Primary, none());
        e.pointer_up(at, none());
    }
    e.double_click(at, none());
}

/// Press at `from`, drag to `to` (screen), let go.
fn drag(e: &mut Editor, from: Vec2, to: Vec2, mods: Mods) {
    e.pointer_move(from, mods);
    e.pointer_down(from, Button::Primary, mods);
    e.pointer_move(from.lerp(to, 0.5), mods);
    e.pointer_move(to, mods);
    e.pointer_up(to, mods);
}

/// The top of the selection's breath.
const CREST: f64 = device_frame::BREATH_SECONDS / 2.0;

/// Lit by the selection (at the crest), as opposed to dimmed or off.
fn bright(px: Rgb) -> bool {
    px.level() > 100
}

/// Draw a path with the path tool, clicking each point (a fixture's own
/// coordinates), then finishing with Enter.
fn draw_path(e: &mut Editor, fixture: &str, points: &[(f64, f64)]) {
    key(e, Key::Char('p'), none());
    for (x, y) in points {
        let at = e.camera.to_screen(place(e, fixture, *x, *y));
        e.pointer_move(at, none());
        e.pointer_down(at, Button::Primary, none());
        e.pointer_up(at, none());
    }
    key(e, Key::Enter, none());
}

/// A point on the edge from `from`, on screen, with nothing under it (an
/// edge starts among lamps, and things win over edges).
fn clear_point_on_edge(e: &Editor, from: &Target) -> Vec2 {
    let edge = e.edges().into_iter().find(|x| &x.from == from).unwrap();
    (10..=90)
        .map(|i| edge.point_at(&e.camera, i as f64 / 100.0))
        .find(|p| e.project.things_at(&e.camera, *p).is_empty())
        .expect("somewhere clear along it")
}

/// Where the handle `which` picks sits on screen.
fn handle_at(e: &Editor, which: impl Fn(&HandleKind) -> bool) -> Vec2 {
    let h = e
        .handles()
        .into_iter()
        .find(|h| which(&h.kind))
        .expect("the handle is shown");
    e.camera.to_screen(h.at)
}

fn editor() -> Editor {
    let mut e = Editor::new(Project::seed());
    e.resize(Vec2::new(1200.0, 800.0));
    e
}

fn none() -> Mods {
    Mods::default()
}

fn cmd() -> Mods {
    Mods {
        command: true,
        ..Mods::default()
    }
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

/// The middle of a pixel's square on its card, in world units.
fn pixel_centre(e: &Editor, output: &NodeId, key: &str, index: u32) -> Vec2 {
    let r = e.project.pixel_rect(output, key, index).unwrap();
    r.min.lerp(r.max, 0.5)
}

fn alt() -> Mods {
    Mods {
        alt: true,
        ..Mods::default()
    }
}

/// A point in a node's own coordinates (from its top-left), on the canvas.
fn place(e: &Editor, name: &str, x: f64, y: f64) -> Vec2 {
    e.project.origin(&node_id(e, name)) + Vec2::new(x, y)
}

fn node_id(e: &Editor, name: &str) -> NodeId {
    e.project.named(name).unwrap().id.clone()
}

/// A node, by name: a fixture, a module, an output.
fn fixture(e: &Editor, name: &str) -> Target {
    Target::Node(node_id(e, name))
}

/// A short path in `strip` crossing path3 at (40, 40).
fn cross_path3(e: &mut Editor) {
    let strip = node_id(e, "strip");
    e.project.add(
        &strip,
        ComponentKind::path(vec![Vec2::new(40.0, 32.0), Vec2::new(40.0, 48.0)], 6),
    );
}

fn comp(id: &str) -> Target {
    Target::Component(ComponentId::new(id))
}

fn ring(circle: &str, key: &str) -> Target {
    Target::Object(ObjectId::new(&ComponentId::new(circle), key))
}

fn lamp(component: &str, key: &str, index: u32) -> Target {
    Target::Lamp {
        object: ObjectId::new(&ComponentId::new(component), key),
        index,
    }
}

fn lamp_pos(e: &Editor, component: &str, key: &str, index: u32) -> Vec2 {
    e.project
        .object(&ObjectId::new(&ComponentId::new(component), key))
        .unwrap()
        .lamps[index as usize]
        .pos
}

fn lamp_screen(e: &Editor, component: &str, key: &str, index: u32) -> Vec2 {
    e.camera.to_screen(lamp_pos(e, component, key, index))
}

fn click_lamp(e: &mut Editor, component: &str, key: &str, index: u32, mods: Mods) {
    let at = lamp_screen(e, component, key, index);
    e.pointer_move(at, mods);
    e.pointer_down(at, Button::Primary, mods);
    e.pointer_up(at, mods);
}

fn click_at(e: &mut Editor, world: Vec2, mods: Mods) {
    let at = e.camera.to_screen(world);
    e.pointer_move(at, mods);
    e.pointer_down(at, Button::Primary, mods);
    e.pointer_up(at, mods);
}

fn key(e: &mut Editor, k: Key, mods: Mods) {
    e.key_down(k.clone(), mods);
    e.key_up(k, mods);
}

/// A bare project: one 100-square fixture `f` and one output `out` with
/// these ports.
fn bare(ports: &[(&str, u32)]) -> (Project, NodeId, NodeId) {
    let mut p = Project::new("t");
    let f = p.add_fixture(None, "f").unwrap();
    let out = p.add_output(None, "out", ports).unwrap();
    (p, f, out)
}

fn add(p: &mut Project, fixture: &NodeId, kind: ComponentKind) -> ComponentId {
    p.add(fixture, kind).unwrap()
}

fn pts(points: &[(f64, f64)]) -> Vec<Vec2> {
    points.iter().map(|&(x, y)| Vec2::new(x, y)).collect()
}

/// A component's `k`th object's lamps, in its fixture's own coordinates.
fn local_lamps(p: &Project, id: &ComponentId, k: usize) -> Vec<Vec2> {
    object::objects_of(p.component(id).unwrap())[k]
        .lamps
        .iter()
        .map(|l| l.pos)
        .collect()
}

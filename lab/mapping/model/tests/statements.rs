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
//! The board is simulated (`board`): times are in seconds, and `CREST` is
//! the top of a selected lamp's breath. A shader `rainbow` at the root
//! feeds every fixture there through `visual.out`; the tail rig has its
//! own shader, `glow`, which the tail reads instead (the nearer writer).
//!
//! Positions are in each fixture's own coordinates; `place(e, "tail", x, y)`
//! turns them into the canvas's.

use lab_mapping_model::card_geometry::*;
use lab_mapping_model::domain::*;
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
    let empty = e
        .camera
        .to_screen(e.project.extent().min - Vec2::new(20.0, 20.0));
    e.pointer_down(empty, Button::Primary, none());
    e.pointer_up(empty, none());
    assert!(e.selection.is_empty());
}

/// A click on a fixture's empty inside selects the fixture, and the hover
/// says so; a drag from there still draws a selection box.
#[test]
fn a_fixtures_inside_selects_it_and_a_drag_there_boxes() {
    let mut e = editor();
    e.select(vec![comp("circle1")]);
    let inside = e.camera.to_screen(place(&e, "scarf", 50.0, 15.0));
    e.pointer_move(inside, none());
    assert_eq!(e.hover_target(), Some(fixture(&e, "scarf")));
    e.pointer_down(inside, Button::Primary, none());
    e.pointer_up(inside, none());
    assert_eq!(e.selection, vec![fixture(&e, "scarf")]);
    e.pointer_down(inside, Button::Primary, none());
    e.pointer_move(inside + Vec2::new(20.0, 20.0), none());
    assert_eq!(e.hint_bar().lead, "Box-selecting");
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

/// The layout places every node by itself (D64): dragging a node by its
/// title selects it and moves nothing, U unpins nothing, and the hint bar
/// says pinning is under Debug.
#[test]
fn the_layout_places_every_node_while_pinning_is_off() {
    let mut e = editor();
    assert!(!e.debug.pinning, "off by default");
    let ring = node_id(&e, "ring display");
    let before = e.project.node_rect(&ring).unwrap();
    let title = e.camera.to_screen(before.min) + Vec2::new(12.0, -10.0);
    let to = title + Vec2::new(40.0, 0.0) * e.camera.scale;
    e.pointer_down(title, Button::Primary, none());
    e.pointer_move(title + Vec2::new(10.0, 0.0), none());
    assert!(
        e.notice
            .as_ref()
            .is_some_and(|n| n.text.contains("pinning is under Debug")),
        "{:?}",
        e.notice
    );
    e.pointer_move(to, none());
    e.pointer_up(to, none());
    lay_out(&mut e);
    assert_eq!(e.selection, vec![fixture(&e, "ring display")]);
    assert_eq!(e.project.node_rect(&ring).unwrap(), before, "nothing moved");
    assert!(
        e.project.node(&ring).unwrap().pinned.is_none(),
        "nothing pinned"
    );
    assert!(
        e.hint_bar().hints.iter().all(|h| h.keys != "drag"),
        "the bar offers no dragging"
    );
    key(&mut e, Key::Char('u'), alt());
    assert!(
        e.notice
            .as_ref()
            .is_some_and(|n| n.text == "Nothing to unpin")
    );
}

/// With pinning on (under Debug), dragging a node pins it where it's
/// dropped; unpinning returns it to the layout; nodes never overlap. The
/// layout ignores pins again when pinning goes off.
#[test]
fn dragging_a_node_pins_it() {
    let mut e = editor();
    e.set_pinning(true);
    let ring = node_id(&e, "ring display");
    let before = e.project.node_rect(&ring).unwrap();
    let lamp_before = lamp_pos(&e, "circle1", "r1", 0);
    // By its title, 40 units right.
    let title = e.camera.to_screen(before.min) + Vec2::new(12.0, -10.0);
    let to = title + Vec2::new(40.0, 0.0) * e.camera.scale;
    drag(&mut e, title, to, none());
    lay_out(&mut e);
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
    lay_out(&mut e);
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
    lay_out(&mut e);
    assert!(e.project.node_rect(&ring).unwrap().min.distance(moved) < 1e-9);
    // ⌥U gives it back to the layout.
    key(&mut e, Key::Char('u'), alt());
    lay_out(&mut e);
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
    lay_out(&mut e);
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

    // Pinned again, then pinning off: the layout places it as if it weren't.
    lay_out(&mut e);
    let card = e.project.node_rect(&scarf_out).unwrap();
    let from = e.camera.to_screen(card.min + Vec2::new(20.0, 3.0));
    drag(&mut e, from, from + Vec2::new(0.0, 40.0), none());
    lay_out(&mut e);
    assert!(e.project.node(&scarf_out).unwrap().pinned.is_some());
    e.set_pinning(false);
    assert!(e.layout_input().pins.is_empty());
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
    // From the tail's top-left, down past the bottom of the canvas.
    let a = e.camera.to_screen(place(&e, "tail", 28.0, 5.0));
    e.pointer_down(a, Button::Primary, none());
    e.pointer_move(a + Vec2::new(10.0, 10.0), none());
    e.pointer_leave();
    let outside = Vec2::new(
        e.camera.to_screen(place(&e, "tail", 55.0, 0.0)).x,
        e.viewport.y + 500.0,
    );
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
    lay_out(&mut e);
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

/// Core says what there is to lay out, never where (D63): each node with
/// its module, its role from its kind, the size it declares (a fixture's
/// box; an output's card, a row per port), and the dataflow from the
/// bindings and the patch. The page runs the layout.
#[test]
fn the_layout_input_says_what_there_is_never_where() {
    let e = editor();
    let input = e.layout_input();
    let node = |name: &str| {
        let id = node_id(&e, name).to_string();
        input.nodes.iter().find(|n| n.id == id).unwrap().clone()
    };
    let rig = node_id(&e, "tail rig").to_string();
    assert_eq!(node("tail rig").role, lab_layout::LayoutRole::Module);
    assert_eq!(node("rainbow").role, lab_layout::LayoutRole::Source);
    assert_eq!(node("tail").role, lab_layout::LayoutRole::Fixture);
    assert_eq!(node("tail").parent, Some(rig));
    assert_eq!(node("tail").size, lab_layout::Vec2::new(82.0, 82.0));
    assert_eq!(node("main").role, lab_layout::LayoutRole::Output);
    assert_eq!(
        node("main").size,
        lab_layout::Vec2::new(CARD_WIDTH, CARD_HEADER + 2.0 * CARD_ROW + CARD_FOOT),
        "a header and a row per port"
    );
    let edge = |a: &str, b: &str| (node_id(&e, a).to_string(), node_id(&e, b).to_string());
    for (a, b) in [
        ("rainbow", "ring display"),
        ("glow", "tail"),
        ("ring display", "main"),
        ("strip", "main"),
        ("tail", "tail out"),
    ] {
        assert!(input.edges.contains(&edge(a, b)), "{a} → {b}");
    }
    assert!(input.pins.is_empty(), "nothing is pinned");
}

/// To the layout, a clock, a power button, a button and a radio are
/// controls, and a playlist makes a picture, like a shader.
#[test]
fn controls_and_playlists_have_their_roles() {
    let mut p = Project::new("t");
    for kind in [
        "Clock",
        "PowerButton",
        "Button",
        "ControlRadio",
        "Playlist",
        "Fluid",
    ] {
        p.add_node(None, kind, NodeKind::Other(OtherNode::new(kind)));
    }
    let e = editor_of(p);
    let roles: Vec<lab_layout::LayoutRole> =
        e.layout_input().nodes.iter().map(|n| n.role).collect();
    use lab_layout::LayoutRole::*;
    assert_eq!(
        roles,
        vec![Control, Control, Control, Control, Source, Other]
    );
}

/// Where nodes sit is a fact the view hands over: before it does, no node
/// has a place; the first placement gives every node one and frames them
/// all.
#[test]
fn where_nodes_sit_is_a_fact_the_view_hands_over() {
    let mut e = Editor::new(Project::seed());
    e.resize(Vec2::new(1200.0, 800.0));
    let tail = node_id(&e, "tail");
    assert_eq!(e.project.node_rect(&tail), None);
    let unplaced = e.camera;
    lay_out(&mut e);
    for n in e.project.all_nodes() {
        assert!(
            e.project.node_rect(&n.id).is_some(),
            "{} has a place",
            n.name
        );
    }
    assert_ne!(e.camera, unplaced, "framed");
    let framed = e.camera;
    e.fit();
    assert_eq!(e.camera, framed, "everything fits");
}

/// Until you move the view, the canvas keeps everything framed as its size
/// changes (the page mounts it again when the size class settles); once
/// you pan, a new size leaves the view where you put it.
#[test]
fn the_canvas_keeps_everything_framed_until_you_move_the_view() {
    let mut e = editor_of(Project::seed());
    e.resize(Vec2::new(600.0, 800.0));
    lay_out(&mut e);
    e.resize(Vec2::new(1200.0, 800.0));
    let wide = e.camera;
    e.fit();
    assert_eq!(e.camera, wide, "framed for the new size");
    e.wheel(Vec2::new(300.0, 300.0), Vec2::new(0.0, 40.0), none());
    let moved = e.camera;
    e.resize(Vec2::new(900.0, 800.0));
    assert_eq!(e.camera, moved, "where you put it");
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

/// On an output, a span is its object: a click on the strip's span on D1
/// selects path3, as a click on it in the strip would, and a press there
/// never drags the drawing. The output itself is picked by its title, and
/// Esc from a span's pixel climbs to its port.
#[test]
fn a_span_on_an_output_selects_its_object() {
    let mut e = editor();
    let main = node_id(&e, "main");
    let at = pixel_centre(&e, &main, "1", 3);
    e.pointer_move(e.camera.to_screen(at), none());
    assert_eq!(e.hover_target(), Some(comp("path3")), "the hover says so");
    assert_eq!(e.cursor_note(), None, "the card behind is no overlap");
    click_at(&mut e, at, none());
    assert_eq!(e.selection, vec![comp("path3")]);
    let spans = e
        .view_inspector()
        .sections
        .into_iter()
        .find(|s| s.title.as_deref() == Some("Spans"))
        .expect("the inspector says where it lands");
    assert_eq!(
        spans.items,
        vec![UiItem::Fact("Pixels 0–29 of D1 of main".into())]
    );
    let before = e.project.clone();
    let from = e.camera.to_screen(at);
    drag(&mut e, from, from + Vec2::new(30.0, 0.0), none());
    assert_eq!(e.project, before, "nothing moved");

    let title = e.project.node_rect(&main).unwrap().min + Vec2::new(5.0, 3.0);
    click_at(&mut e, title, none());
    assert_eq!(e.selection, vec![Target::Node(main.clone())]);

    click_at(&mut e, at, none());
    click_at(&mut e, at, none());
    assert_eq!(e.selection, vec![pixel(&main, "1", 3)]);
    key(&mut e, Key::Escape, none());
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

/// Edges are node to node: one per pair — a render source into a fixture,
/// a fixture into an output — never one per object or per port. Where the
/// objects land shows on the output itself, as runs of pixels.
#[test]
fn edges_run_node_to_node_one_per_pair() {
    let e = editor();
    let p = &e.project;
    let edges = e.edges();
    assert!(
        edges
            .iter()
            .all(|x| matches!(x.from, Target::Node(_)) && matches!(x.to, Target::Node(_))),
        "{edges:?}"
    );
    let patch: Vec<(String, String, String)> = edges
        .iter()
        .filter(|x| p.output(&node_of_target(&x.to)).is_some())
        .map(|x| {
            (
                p.node_name(&node_of_target(&x.from)),
                p.node_name(&node_of_target(&x.to)),
                x.label.clone(),
            )
        })
        .collect();
    let mut patch = patch;
    patch.sort();
    let want: Vec<(String, String, String)> = [
        ("ring display", "main", "64 lamps on D0"),
        ("scarf", "scarf out", ""),
        ("strip", "main", "30 lamps on D1"),
        ("tail", "tail out", "50 lamps on GPIO 10"),
    ]
    .iter()
    .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
    .collect();
    assert_eq!(patch, want);
    assert_eq!(patch.len(), 4, "one edge per fixture and output");
    assert!(edges.iter().all(|x| x.active));
}

/// An edge into an output says which of its port rows it lands beside, so
/// the view brings it in level with them and two fixtures on one output
/// arrive apart. An edge into anything else lands beside no row.
#[test]
fn an_edge_into_an_output_says_which_ports_it_lands_beside() {
    let e = editor();
    let main = node_id(&e, "main");
    let edge = |from: &str, to: &NodeId| {
        e.edges()
            .into_iter()
            .find(|x| x.from == fixture(&e, from) && x.to == Target::Node(to.clone()))
            .unwrap()
    };
    assert_eq!(edge("ring display", &main).lands_on, vec!["0".to_string()]);
    assert_eq!(edge("strip", &main).lands_on, vec!["1".to_string()]);
    let ring = node_id(&e, "ring display");
    assert_eq!(edge("rainbow", &ring).lands_on, Vec::<String>::new());
}

/// Edges don't change when the selection changes — only how they are drawn
/// — and a fixture with many objects on one output is still one edge.
#[test]
fn edges_dont_change_with_selection_or_density() {
    let mut e = editor();
    let shape = |e: &Editor| {
        e.edges()
            .into_iter()
            .map(|x| (x.from, x.to, x.lands_on))
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
    // Twenty-five short paths in the scarf: one edge to its output.
    let scarf = node_id(&e, "scarf");
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
        .filter(|x| x.from == Target::Node(scarf.clone()))
        .collect();
    assert_eq!(from_scarf.len(), 1);
    assert_eq!(from_scarf[0].label, "50 lamps on D10");
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
    assert!(
        patch
            .problems
            .iter()
            .all(|x| x.gone == Gone::Output(main.clone()))
    );
    let ring = node_id(&e, "ring display");
    assert_eq!(
        p.fixture(&ring).unwrap().places,
        vec![auto_onto(&main)],
        "the placement still names main"
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

    // Enter finishes too.
    key(&mut e, Key::Char('p'), none());
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
    let bar = e.hint_bar();
    let drag = bar.hints.iter().find(|h| h.keys == "drag").unwrap();
    assert_eq!(drag.does, "move point");
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
    assert_eq!(e.hint_bar().lead, "Changing a radius");
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

/// The hint bar names each action in a word or two, never what it acts
/// on (the breadcrumb says what's selected), and it stays put while you
/// point around or hold ⌘.
#[test]
fn the_hint_bar_names_actions_and_stays_put() {
    let mut e = editor();
    e.select(vec![comp("circle1")]);
    e.pointer_move(Vec2::new(1.0, 1.0), none());
    let still = e.hint_bar();
    for h in &still.hints {
        assert!(!h.does.contains("circle1"), "{h:?}");
        assert!(h.does.split_whitespace().count() <= 3, "{h:?}");
    }
    e.pointer_move(lamp_screen(&e, "circle1", "r1", 3), none());
    assert_eq!(e.hint_bar(), still, "hovering a lamp");
    e.pointer_move(lamp_screen(&e, "circle1", "r1", 3), cmd());
    assert_eq!(e.hint_bar(), still, "holding ⌘");
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
            for keys in ["Esc", "Enter", "R", "− ="] {
                assert!(
                    bar.hints.iter().any(|h| h.keys == keys),
                    "{keys} is offered"
                );
            }
        }
        for (keys, key_) in [
            ("Esc", Key::Escape),
            ("Enter", Key::Enter),
            ("R", Key::Char('r')),
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

// ---- corner jumpers (plan M5) ---------------------------------------------------

/// An editor with an eave in its first fixture: a path of 20 lamps
/// turning one corner on a short segment, three segments in all.
fn eave() -> (Editor, ComponentId) {
    let mut p = Project::seed();
    let fixture = p.fixtures()[0].id.clone();
    let c = p.mint(ComponentKind::path(
        vec![
            Vec2::new(5.0, 40.0),
            Vec2::new(5.0, 10.0),
            Vec2::new(7.0, 8.0),
            Vec2::new(40.0, 8.0),
        ],
        20,
    ));
    let id = c.id.clone();
    p.fixture_mut(&fixture).unwrap().components.push(c);
    (editor_of(p), id)
}

/// A segment made a jumper carries no lamps: the path keeps its count, and
/// every lamp lands on its lit segments. Lit again, they spread back.
#[test]
fn a_jumper_carries_no_lamps_and_keeps_the_count() {
    let (mut e, path) = eave();
    let before: Vec<Vec2> = e
        .project
        .objects_of(&path)
        .flat_map(|o| o.lamps.iter().map(|l| l.pos))
        .collect();
    e.toggle_jumper(&path, 0);
    assert_eq!(e.project.path_jumpers(&path), &[0]);
    let points = e.project.path_points(&path).unwrap().to_vec();
    let after: Vec<Vec2> = e
        .project
        .objects_of(&path)
        .flat_map(|o| o.lamps.iter().map(|l| l.pos))
        .collect();
    assert_eq!(after.len(), before.len(), "the count stays");
    // The first lamp sits where the first lit segment starts, not on the
    // jumper's start.
    let frame = e.project.component_frame(&path);
    let start = frame.to_world(points[1]);
    assert!(
        after[0].distance(start) < 1e-3,
        "{:?} vs {start:?}",
        after[0]
    );
    assert_ne!(after, before);
    e.toggle_jumper(&path, 0);
    assert!(e.project.path_jumpers(&path).is_empty());
}

/// A path keeps at least one lit segment.
#[test]
fn a_path_keeps_one_lit_segment() {
    let (mut e, path) = eave();
    let segments = e.project.path_points(&path).unwrap().len() - 1;
    for s in 0..segments {
        e.toggle_jumper(&path, s);
    }
    assert_eq!(e.project.path_jumpers(&path).len(), segments - 1);
}

/// Adding a point keeps a jumper where it was: a point added before it
/// moves it along one, and one added inside it makes it two.
#[test]
fn adding_a_point_keeps_the_jumpers_in_place() {
    let (mut e, path) = eave();
    e.toggle_jumper(&path, 1);
    let points = e.project.path_points(&path).unwrap().to_vec();
    let frame = e.project.component_frame(&path);
    let mid0 = frame.to_world(points[0].lerp(points[1], 0.5));
    assert!(e.project.insert_path_point(&path, 0, mid0));
    assert_eq!(e.project.path_jumpers(&path), &[2]);
    let points = e.project.path_points(&path).unwrap().to_vec();
    let mid2 = frame.to_world(points[2].lerp(points[3], 0.5));
    assert!(e.project.insert_path_point(&path, 2, mid2));
    assert_eq!(e.project.path_jumpers(&path), &[2, 3]);
}

/// A fixture's colour order is one of the product's six, picked in the
/// inspector: what a WS2811 string needs is found by picking and looking.
#[test]
fn a_fixtures_colour_order_is_picked_from_six() {
    let mut e = editor();
    let fixture = e.project.fixtures()[0].id.clone();
    e.selection = vec![Target::Node(fixture.clone())];
    let props = e.project.props_of(&Target::Node(fixture.clone()));
    let (desc, _) = props
        .iter()
        .find(|(d, _)| d.id == "colour-order")
        .expect("a colour order");
    assert!(matches!(desc.kind, PropKind::Choice { options } if options.len() == 6));
    e.input(GraphInput::EditProp {
        id: "colour-order",
        edit: PropEdit::SetChoice(4),
    });
    assert_eq!(
        e.project.fixture(&fixture).unwrap().color_order,
        ColorOrder::Brg
    );
}

// ---- verbs by name -------------------------------------------------------------

/// Every verb the hint bar names is on offer by name, and pressing one
/// does what its key does: ⌘A then ⌫, pressed, empty every fixture as the
/// keys do. Undo is on offer once there is something to undo.
#[test]
fn pressing_a_verb_does_what_its_key_does() {
    let mut pressed = editor();
    let mut keyed = editor();
    assert!(!pressed.verbs().contains(&GraphVerb::Undo));
    assert!(pressed.verbs().contains(&GraphVerb::SelectAll));
    pressed.press(GraphVerb::SelectAll);
    keyed.key_down(
        Key::Char('a'),
        Mods {
            command: true,
            ..Mods::default()
        },
    );
    assert_eq!(pressed.selection, keyed.selection);
    let bar: Vec<GraphVerb> = pressed
        .hint_bar()
        .hints
        .iter()
        .flat_map(|h| h.verbs.clone())
        .collect();
    assert!(bar.contains(&GraphVerb::Delete));
    assert!(pressed.verbs().contains(&GraphVerb::Delete));
    pressed.press(GraphVerb::Delete);
    keyed.key_down(Key::Delete, Mods::default());
    assert_eq!(pressed.project.lamp_count(), 0);
    assert_eq!(keyed.project.lamp_count(), 0);
    assert!(pressed.verbs().contains(&GraphVerb::Undo));
}

/// A verb's name is a path segment: lowercase words, never a dot, and
/// each names one verb.
#[test]
fn every_verb_has_its_own_name() {
    for v in GraphVerb::ALL {
        assert!(!v.name().contains('.'), "{}", v.name());
        assert_eq!(GraphVerb::from_name(v.name()), Some(v));
    }
}

// ---- a real board ------------------------------------------------------------

/// A real board's pixels are what the board shows: each port as the board
/// sent it, and each lamp in the colour of the pixel it lands on. A port
/// the board said nothing for is dark.
#[test]
fn a_real_boards_pixels_colour_the_lamps_on_them() {
    let e = editor();
    let tail_out = node_id(&e, "tail out");
    let red = Rgb::new(255, 0, 0);
    let frame = e.project.board_frame_from_ports(|output, key| {
        (output == &tail_out && key == "0").then(|| vec![red; 60])
    });
    assert_eq!(frame.port(&tail_out, "0").unwrap(), &[red; 60][..]);
    let tail = e
        .project
        .placed_objects()
        .iter()
        .find(|(f, _)| f == &node_id(&e, "tail"))
        .map(|(_, o)| o.id.clone())
        .unwrap();
    assert_eq!(frame.lamp(&tail, 0), Some(red));
    let main = node_id(&e, "main");
    assert!(
        frame
            .port(&main, "0")
            .unwrap()
            .iter()
            .all(|px| px.is_black())
    );
}

// ---- the simulated board ----------------------------------------------------

/// With nothing selected, the board shows the pattern on every patched pixel
/// and black on unpatched ones.
#[test]
fn the_board_shows_the_pattern_on_patched_pixels_only() {
    let e = editor();
    let frame = e.board_frame(0.0);
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
    let plain = e.board_frame(CREST);
    e.select(vec![lamp("path3", "path", 4)]);
    assert!(e.board_animates());
    let frame = e.board_frame(CREST);
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
    let low = e.board_frame(0.0);
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
        let frame = e.board_frame(CREST);
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
    let frame = e.board_frame(0.0);
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
        let frame = e.board_frame(CREST);
        let gpio10 = frame.port(&tail_out, "0").unwrap().to_vec();
        let last_red = gpio10
            .iter()
            .rposition(|px| *px == fixture_overrides::END_COLOUR)
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
        let frame = e.board_frame(t);
        let d1 = frame.port(&main, "1").unwrap().to_vec();
        assert!(
            d1[..3]
                .iter()
                .all(|px| *px == fixture_overrides::START_COLOUR)
        );
        assert!(
            d1[27..]
                .iter()
                .all(|px| *px == fixture_overrides::END_COLOUR)
        );
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
    let frame = e.board_frame(0.5);
    let gpio10 = frame.port(&node_id(&e, "tail out"), "0").unwrap().to_vec();
    for start in [0, 25] {
        assert!(
            gpio10[start..start + 3]
                .iter()
                .all(|px| *px == fixture_overrides::START_COLOUR)
        );
        assert!(
            gpio10[start + 22..start + 25]
                .iter()
                .all(|px| *px == fixture_overrides::END_COLOUR)
        );
    }
    // And with lp2014's panels of 120, 15.
    assert_eq!(fixture_overrides::lead_lamps(120), 15);
    assert_eq!(fixture_overrides::lead_lamps(400), 15);
    assert_eq!(fixture_overrides::lead_lamps(4), 1);
}

/// Every lamp on the canvas shows its colour in its fixture's control
/// product, and a patched lamp's pixel shows the same colour while no
/// output override paints over it; a lamp outside its box has no colour,
/// and an unpatched lamp keeps its fixture's colour.
#[test]
fn every_lamp_shows_its_control_products_colour() {
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
    for (selection, t) in [
        (vec![], 0.0),
        (vec![comp("path3")], 0.7),
        (vec![lamp("circle1", "r2", 3)], CREST),
    ] {
        e.select(selection.clone());
        let frame = e.board_frame(t);
        let patch = e.project.effective_patch();
        for s in &patch.spans {
            let pixels = frame.port(&s.output, &s.port).unwrap();
            let product = frame.control_product(&s.fixture).unwrap();
            for lamp in s.lamps.clone() {
                let pixel = pixels[(s.start_pixel + lamp - s.lamps.start) as usize];
                let shown = frame.lamp(&s.object, lamp);
                if s.object.component == out && lamp >= 2 {
                    assert_eq!(shown, None, "outside its box: no colour");
                    assert!(pixel.is_black(), "and a dark pixel");
                } else {
                    assert_eq!(shown, Some(pixel), "{selection:?} {} {lamp}", s.object);
                    assert!(product.lamps.contains(&shown));
                }
            }
        }
        let strip = ObjectId::new(&ComponentId::new("path3"), "path");
        assert!(
            !frame.lands(&strip, 30),
            "lamp 30 of the strip is unpatched"
        );
        assert!(
            frame.lamp(&strip, 30).is_some(),
            "unpatched, it keeps its fixture's colour"
        );
    }
}

// ---- the engine: render products, control products, overrides ----------------

/// A fixture's lamps take their colours from its render product, sampled
/// at their positions.
#[test]
fn a_fixtures_lamps_sample_its_render_product() {
    let e = editor();
    let frame = e.board_frame(0.0);
    for name in ["ring display", "strip", "tail"] {
        let f = node_id(&e, name);
        let product = e.project.render_product(&f).expect("a shader feeds it");
        let fx = e.project.fixture(&f).unwrap();
        let origin = e.project.origin(&f);
        for o in e.project.objects_in(&f) {
            for (i, l) in o.lamps.iter().enumerate() {
                assert_eq!(
                    frame.lamp(&o.id, i as u32),
                    Some(product.sample(fx.uv(l.pos - origin), 0.0)),
                    "{name} {} {i}",
                    o.id
                );
            }
        }
    }
    // Lamps in different places differ: it's a picture, not a fill.
    let r1 = ObjectId::new(&ComponentId::new("circle1"), "r1");
    assert_ne!(frame.lamp(&r1, 0), frame.lamp(&r1, 20));
    // The ring display reads the root's shader; the tail reads its own
    // rig's, the nearer writer of `visual.out` — and the two look
    // different.
    let source = |name: &str| e.project.render_product(&node_id(&e, name)).unwrap().source;
    assert_eq!(source("ring display"), node_id(&e, "rainbow"));
    assert_eq!(source("tail"), node_id(&e, "glow"));
    let pattern = |name: &str| {
        e.project
            .node(&node_id(&e, name))
            .unwrap()
            .source()
            .unwrap()
            .pattern
    };
    assert_ne!(pattern("rainbow"), pattern("glow"));
    // With no shader at all, nothing renders: lamps have no colour.
    let mut e = editor();
    for name in ["rainbow", "tail rig"] {
        let id = node_id(&e, name);
        e.project.remove_node(&id);
    }
    let frame = e.board_frame(0.0);
    assert_eq!(frame.lamp(&r1, 0), None);
}

/// Selecting something inside a fixture changes its control product, and
/// so its lamps on the canvas and its pixels on the output.
#[test]
fn a_selection_inside_a_fixture_changes_its_control_product() {
    let mut e = editor();
    let main = node_id(&e, "main");
    let strip = node_id(&e, "strip");
    let path3 = ObjectId::new(&ComponentId::new("path3"), "path");
    let plain = e.board_frame(CREST);
    e.select(vec![comp("path3")]);
    let frame = e.board_frame(CREST);
    assert_ne!(
        plain.control_product(&strip).unwrap().lamps,
        frame.control_product(&strip).unwrap().lamps
    );
    // path3's lamp 0 leads in blue: in the product, on the canvas, on D1.
    assert_eq!(
        frame.control_product(&strip).unwrap().lamps[0],
        Some(fixture_overrides::START_COLOUR)
    );
    assert_eq!(frame.lamp(&path3, 0), Some(fixture_overrides::START_COLOUR));
    assert_eq!(
        frame.port(&main, "1").unwrap()[0],
        fixture_overrides::START_COLOUR
    );
    // A lamp on its own breathes, and its pixel with it: r2's lamp 3 is
    // pixel 43 of D0.
    e.select(vec![lamp("circle1", "r2", 3)]);
    let frame = e.board_frame(CREST);
    let r2 = ObjectId::new(&ComponentId::new("circle1"), "r2");
    assert_eq!(frame.lamp(&r2, 3), Some(Rgb::WHITE));
    assert_eq!(frame.port(&main, "0").unwrap()[43], Rgb::WHITE);
    assert!(frame.lamp(&path3, 0).unwrap().level() < plain.lamp(&path3, 0).unwrap().level());
}

/// Selecting an output, port, run or pixel changes only the output's
/// pixels; no lamp's colour on the canvas changes.
#[test]
fn an_output_selection_changes_only_the_outputs_pixels() {
    let mut e = editor();
    let main = node_id(&e, "main");
    let tail_out = node_id(&e, "tail out");
    let plain = e.board_frame(CREST);
    for selection in [
        vec![Target::Node(main.clone())],
        vec![port(&main, "0")],
        (4..=8).map(|i| pixel(&main, "1", i)).collect(),
        vec![pixel(&tail_out, "0", 3)],
    ] {
        e.select(selection.clone());
        let frame = e.board_frame(CREST);
        for n in e.project.fixtures() {
            assert_eq!(
                frame.control_product(&n.id),
                plain.control_product(&n.id),
                "{} with {selection:?}",
                n.name
            );
        }
        for o in e.project.objects() {
            for i in 0..o.lamps.len() as u32 {
                assert_eq!(frame.lamp(&o.id, i), plain.lamp(&o.id, i));
            }
        }
        assert_ne!(
            frame.ports, plain.ports,
            "{selection:?} shows on the pixels"
        );
    }
    // The selected pixels breathe; the rest of the outputs dim.
    e.select(vec![port(&main, "0")]);
    let frame = e.board_frame(CREST);
    assert!(frame.port(&main, "0").unwrap().iter().all(|px| bright(*px)));
    assert!(
        frame
            .port(&main, "1")
            .unwrap()
            .iter()
            .all(|px| !bright(*px))
    );
}

/// A selected pixel's lamp is marked on the canvas, not recoloured.
#[test]
fn a_selected_pixels_lamp_is_marked_not_recoloured() {
    let mut e = editor();
    let main = node_id(&e, "main");
    let path3 = ObjectId::new(&ComponentId::new("path3"), "path");
    let plain = e.board_frame(CREST);
    e.select(vec![pixel(&main, "1", 3)]);
    let frame = e.board_frame(CREST);
    assert_eq!(e.marked_lamps(), vec![lamp("path3", "path", 3)]);
    assert_eq!(
        frame.lamp(&path3, 3),
        plain.lamp(&path3, 3),
        "its own colour"
    );
    assert_eq!(
        frame.port(&main, "1").unwrap()[3],
        Rgb::WHITE,
        "the pixel breathes"
    );
    assert_ne!(frame.lamp(&path3, 3), Some(Rgb::WHITE));
}

/// Every edge between nodes comes from a channel binding and is labelled
/// with its channel's name.
#[test]
fn every_edge_is_a_channel_binding_with_its_name() {
    let e = editor();
    let p = &e.project;
    let bindings = p.bindings();
    let node_of = |t: &Target| match t {
        Target::Node(id) => id.clone(),
        Target::Port { output, .. } | Target::Pixel { output, .. } => output.clone(),
        _ => p.fixture_of_target(t).unwrap(),
    };
    let edges = e.edges();
    for x in &edges {
        let (from, to) = (node_of(&x.from), node_of(&x.to));
        let b = bindings
            .iter()
            .find(|b| b.from == from && b.to == to)
            .unwrap_or_else(|| panic!("{x:?} is no binding"));
        assert_eq!(x.channel, b.channel.as_str());
    }
    // Every binding is drawn, once, with its name.
    let named: Vec<(String, String, String)> = edges
        .iter()
        .map(|x| {
            (
                p.node_name(&node_of(&x.from)),
                p.node_name(&node_of(&x.to)),
                x.channel.clone(),
            )
        })
        .collect();
    let mut want: Vec<(String, String, String)> = [
        ("rainbow", "ring display", "visual.out"),
        ("rainbow", "scarf", "visual.out"),
        ("rainbow", "strip", "visual.out"),
        ("glow", "tail", "visual.out"),
        ("ring display", "main", "control.out"),
        ("scarf", "scarf out", "control.scarf_out"),
        ("strip", "main", "control.out"),
        ("tail", "tail out", "control.out"),
    ]
    .iter()
    .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
    .collect();
    let mut got = named.clone();
    got.sort();
    want.sort();
    assert_eq!(got, want);
    assert_eq!(bindings.len(), want.len());
    // The empty scarf has nothing placed yet: its binding is still an edge.
    let scarf = edges
        .iter()
        .find(|x| x.from == fixture(&e, "scarf"))
        .unwrap();
    assert!(scarf.label.is_empty() && scarf.to == fixture(&e, "scarf out"));
}

/// Two fixtures on one output each arrive as their own control product,
/// placed by the patch.
#[test]
fn two_fixtures_on_one_output_arrive_as_their_own_products() {
    let e = editor();
    let main = node_id(&e, "main");
    let ring = node_id(&e, "ring display");
    let strip = node_id(&e, "strip");
    let frame = e.board_frame(0.0);
    assert_eq!(frame.inputs(&main), [ring.clone(), strip.clone()]);
    // The ring display's 64 lamps fill D0, in its data order; the strip's
    // 30 fill D1.
    let dark = |c: &Option<Rgb>| c.unwrap_or(Rgb::BLACK);
    let ring_lamps: Vec<Rgb> = frame
        .control_product(&ring)
        .unwrap()
        .lamps
        .iter()
        .map(dark)
        .collect();
    let strip_lamps: Vec<Rgb> = frame
        .control_product(&strip)
        .unwrap()
        .lamps
        .iter()
        .map(dark)
        .collect();
    assert_eq!(frame.port(&main, "0").unwrap(), ring_lamps.as_slice());
    assert_eq!(frame.port(&main, "1").unwrap(), strip_lamps.as_slice());
    assert_ne!(ring_lamps[0], strip_lamps[0], "two products, two pictures");
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
    let before = e.board_frame(0.0).port(&tail_out, "0").unwrap().to_vec();
    e.select(vec![comp("path1")]);
    key(&mut e, Key::Delete, none());
    let after = e.board_frame(0.0).port(&tail_out, "0").unwrap().to_vec();
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
        let frame = e.board_frame(CREST);
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

/// Hovering an edge, as the view reports it, says what the edge carries,
/// lights it and selects nothing. A thing under the pointer wins over an
/// edge.
#[test]
fn hovering_an_edge_says_what_it_carries() {
    let mut e = editor();
    let tail = node_id(&e, "tail");
    let tail_out = node_id(&e, "tail out");
    // Somewhere clear of everything, as an edge between cards is.
    let clear = e.camera.to_screen(Vec2::new(-40.0, -40.0));
    assert!(e.project.things_at(&e.camera, clear).is_empty());
    e.pointer_move(clear, none());
    e.set_hovered_edge(Some((tail.clone(), tail_out.clone())));
    assert_eq!(
        e.cursor_note().as_deref(),
        Some("tail → tail out · 50 lamps on GPIO 10 · control.out")
    );
    let hovered: Vec<Target> = e
        .edges()
        .into_iter()
        .filter(|x| x.hovered)
        .map(|x| x.from)
        .collect();
    assert_eq!(hovered, vec![fixture(&e, "tail")]);
    assert!(e.selection.is_empty(), "hovering selects nothing");

    // With something selected, the same.
    e.select(vec![comp("path2")]);
    let ring = node_id(&e, "ring display");
    let main = node_id(&e, "main");
    e.set_hovered_edge(Some((ring, main)));
    assert_eq!(
        e.cursor_note().as_deref(),
        Some("ring display → main · 64 lamps on D0 · control.out")
    );
    assert_eq!(e.selection, vec![comp("path2")]);

    // Over a thing, the thing wins.
    e.pointer_move(e.camera.to_screen(lamp_pos(&e, "circle1", "r1", 0)), none());
    assert!(e.hover_target().is_some());
    assert!(e.edges().iter().all(|x| !x.hovered));

    // Away from every edge, no note.
    e.pointer_move(clear, none());
    e.set_hovered_edge(None);
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
    assert_eq!(e.hint_bar().lead, "Moving");
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
    assert_eq!(e.hint_bar().lead, "Choosing");
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
    assert_eq!(e.hint_bar().lead, "Resizing");
}

// ---- Move & draw -------------------------------------------------------------

/// With Move & draw off, nothing moves, draws or reshapes, and each try
/// says the toggle is off and how to turn it on; selection, pixels and the
/// board still work.
#[test]
fn with_move_and_draw_off_nothing_moves() {
    let mut e = editor();
    key(&mut e, Key::Char('d'), none());
    assert!(!e.move_draw);
    let before = e.project.clone();
    let refused = |e: &Editor| {
        let n = e.notice.clone().expect("a notice");
        n.kind == NoticeKind::Refused
            && n.why.as_deref() == Some("Move & draw is off")
            && n.help.unwrap_or_default().starts_with("D turns it on")
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
    assert!(bar.hints.iter().any(|h| h.keys == "D"));

    // A pixel still lights; a port's own count is patching, not drawing.
    let main = node_id(&e, "main");
    e.select(vec![pixel(&main, "1", 3)]);
    assert!(bright(e.board_frame(CREST).port(&main, "1").unwrap()[3]));
    e.select(vec![port(&main, "1")]);
    key(&mut e, Key::Char('='), none());
    assert_eq!(e.project.port_fill(&main, "1"), (30, 31));

    // D turns it back on, and a drag moves again.
    key(&mut e, Key::Char('d'), none());
    e.select(vec![comp("circle1")]);
    let at = lamp_screen(&e, "circle1", "r1", 3);
    drag(&mut e, at, at + Vec2::new(40.0, 0.0), none());
    assert_ne!(
        e.project.component(&ComponentId::new("circle1")),
        before.component(&ComponentId::new("circle1"))
    );
}

// ---- pixels --------------------------------------------------------------------

/// Clicking a span selects its object; clicking again selects the pixel
/// under the cursor. The arrow keys walk the pixels, and ⇧-click selects
/// the run from the first one picked.
#[test]
fn clicking_into_a_span_selects_a_pixel() {
    let mut e = editor();
    let main = node_id(&e, "main");
    let at = pixel_centre(&e, &main, "1", 3);
    click_at(&mut e, at, none());
    assert_eq!(e.selection, vec![comp("path3")]);
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
    let frame = e.board_frame(CREST);
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
    let frame = e.board_frame(CREST);
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
    let mut e = editor_of(p);
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
    let mut e = editor_of(p);
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
        p.fixture_mut(&f)
            .unwrap()
            .places
            .push(placed_at(&object, &out, "0", 10, reversed, 2));
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
    p.fixture_mut(&a).unwrap().places = vec![
        placed_at(&ObjectId::new(&a2, "path"), &out, "0", 10, false, 0),
        auto_onto(&out),
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
    p.fixture_mut(&f).unwrap().places = vec![placed_at(
        &ObjectId::new(&a2, "path"),
        &out,
        "0",
        0,
        false,
        0,
    )];
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
    p.fixture_mut(&f).unwrap().places = vec![
        placed_at(&ObjectId::new(&a1, "path"), &out, "7", 0, false, 0),
        auto_onto(&out),
    ];
    let patch = p.effective_patch();
    assert_eq!(patch.problems.len(), 1);
    assert_eq!(
        patch.problems[0].gone,
        Gone::Port {
            output: out.clone(),
            port: "7".into()
        }
    );
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

/// A node the lab doesn't edit (a fluid, a clock) is a plain card: it
/// selects and deletes like any node, and holds nothing.
#[test]
fn a_node_the_lab_doesnt_edit_is_a_plain_card() {
    let mut e = editor();
    let shader = e
        .project
        .add_node(None, "shader", NodeKind::Other(OtherNode::new("Shader")))
        .unwrap();
    lay_out(&mut e);
    let t = Target::Node(shader.clone());
    assert!(!e.project.is_frame(&t));
    assert!(e.project.children(Some(&t)).is_empty());
    assert!(
        e.project.node_rect(&shader).is_some(),
        "it is on the canvas"
    );
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
/// afresh gives. The project's revision — what a page keeps its drawing by —
/// moves with every edit, and with nothing else.
#[test]
fn kept_answers_are_never_stale() {
    let mut e = editor();
    let main = node_id(&e, "main");
    let ring_display = node_id(&e, "ring display");
    let same_as_afresh = |e: &Editor| {
        let mut afresh = e.project.clone();
        afresh.nodes_mut(); // a `&mut` way in forgets what was kept
        assert_eq!(e.project.placement(), afresh.placement());
        assert_eq!(e.project.objects(), afresh.objects());
        assert_eq!(e.project.effective_patch(), afresh.effective_patch());
        assert_eq!(
            e.project.board_frame(&[], 0.5),
            afresh.board_frame(&[], 0.5)
        );
        let ring = ObjectId::new(&ComponentId::new("circle1"), "r1");
        for i in 0..44 {
            assert_eq!(
                e.project.board_frame(&[], 0.5).lamp(&ring, i),
                afresh.board_frame(&[], 0.5).lamp(&ring, i)
            );
        }
    };
    let start = e.project.revision();
    same_as_afresh(&e);
    let _ = e.hint_bar();
    let _ = e.edges();
    assert_eq!(e.project.revision(), start, "questions are not edits");
    let lamps = e.project.lamp_count();
    let patch = e.project.effective_patch().clone();

    // A count: four more lamps on the outer ring, on the patch at once (the
    // strip after it moves along the wire).
    e.select(vec![ring("circle1", "r1")]);
    e.edit_selection_prop("lamps", PropEdit::Nudge(4));
    assert_eq!(e.project.lamp_count(), lamps + 4);
    assert_ne!(*e.project.effective_patch(), patch);
    let counted = e.project.revision();
    assert_ne!(counted, start);
    same_as_afresh(&e);

    // A resize: the fixture's box, and the output beside it, move.
    let card = e.project.node_rect(&main).unwrap();
    e.select(vec![Target::Node(ring_display.clone())]);
    e.edit_selection_prop("width", PropEdit::SetNumber(120.0));
    let edited = e.project.revision();
    lay_out(&mut e);
    assert_eq!(
        e.project.revision(),
        edited,
        "a new placement is not an edit"
    );
    assert_eq!(e.project.node_rect(&ring_display).unwrap().width(), 120.0);
    assert!(e.project.node_rect(&main).unwrap().min.x > card.min.x);
    assert_ne!(e.project.revision(), counted);
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
    assert_eq!(e.project.revision(), start, "undo gives back what it held");
}

// ---- port runs ------------------------------------------------------------

/// A port's pixels split into runs, one per object patched to it, with the
/// unpatched pixels as runs of their own; every pixel is in exactly one run.
#[test]
fn a_ports_pixels_split_into_runs_per_object() {
    let e = editor();
    let out = node_id(&e, "tail out");
    let runs = e.project.port_runs(&out, "0");
    // The tail's two paths, 25 lamps each, then 10 spare pixels.
    let sizes: Vec<(u32, bool)> = runs
        .iter()
        .map(|r| (r.pixels.len() as u32, r.object.is_some()))
        .collect();
    assert_eq!(sizes, vec![(25, true), (25, true), (10, false)]);
    let mut covered: Vec<u32> = runs.iter().flat_map(|r| r.pixels.clone()).collect();
    covered.sort();
    assert_eq!(covered, (0..60).collect::<Vec<_>>());
}

/// Each run starts on a fresh square: on its own line when a port has few
/// runs, or after an empty square when runs share a line.
#[test]
fn each_run_starts_on_a_fresh_square() {
    let e = editor();
    let out = node_id(&e, "tail out");
    let grid = e.project.pixel_grid(&out, "0").unwrap();
    let last_of_first = grid.rect(24).unwrap();
    let first_of_second = grid.rect(25).unwrap();
    if (first_of_second.min.y - last_of_first.min.y).abs() < 1e-9 {
        assert!(
            first_of_second.min.x - last_of_first.max.x > grid.side * 0.9,
            "a gap between runs"
        );
    } else {
        assert!(
            first_of_second.min.x <= grid.rect(0).unwrap().min.x + 1e-9,
            "a new line starts at the left"
        );
    }
}

// ---- the view tree -------------------------------------------------------------

/// The view tree carries no positions: every node, in tree order, with its
/// module, its role and the size core declares — the same the layout is
/// given — and the page places the cards.
#[test]
fn the_view_tree_says_what_there_is_never_where() {
    let e = editor();
    let view = e.view_tree();
    let input = e.layout_input();
    assert_eq!(view.nodes.len(), input.nodes.len());
    for (n, l) in view.nodes.iter().zip(&input.nodes) {
        assert_eq!(n.id.to_string(), l.id);
        assert_eq!(n.parent.as_ref().map(|p| p.to_string()), l.parent);
        assert_eq!(n.role, l.role);
        assert_eq!((n.size.x, n.size.y), (l.size.x, l.size.y));
    }
    let frame = |name: &str| {
        view.nodes
            .iter()
            .find(|n| n.header.name == name)
            .unwrap()
            .frame
    };
    assert_eq!(frame("tail rig"), FrameKind::Module);
    assert_eq!(frame("tail"), FrameKind::Fixture);
    assert_eq!(frame("main"), FrameKind::Card);
}

/// Outside the level you are working in, a node is dimmed — except an
/// output fed by a fixture you are working in, which stays as it is. A
/// node is marked selected when it is.
#[test]
fn outside_the_level_you_work_in_nodes_are_dimmed() {
    let mut e = editor();
    let emphasis = |e: &Editor, name: &str| {
        e.view_nodes()
            .into_iter()
            .find(|n| n.header.name == name)
            .unwrap()
            .emphasis
    };
    assert!(
        e.view_nodes().iter().all(|n| n.emphasis == Emphasis::Full),
        "nothing selected, nothing dimmed"
    );
    e.select(vec![comp("circle1")]);
    assert_eq!(emphasis(&e, "ring display"), Emphasis::Full);
    assert_eq!(emphasis(&e, "strip"), Emphasis::Dimmed);
    assert_eq!(emphasis(&e, "main"), Emphasis::Full, "the ring feeds it");
    assert_eq!(emphasis(&e, "scarf out"), Emphasis::Dimmed);
    e.select(vec![fixture(&e, "strip")]);
    assert!(
        e.view_nodes()
            .iter()
            .any(|n| n.header.name == "strip" && n.selected)
    );
}

/// In a fixture's face, an object in the selection is lit and one outside
/// the level you are in is dimmed; the face is drawn in the fixture's own
/// units, wherever the fixture landed.
#[test]
fn a_fixtures_face_lights_the_selection_in_its_own_units() {
    let mut e = editor();
    e.select(vec![ring("circle1", "r1")]);
    let view = e.view_tree();
    let face = |name: &str| match view
        .nodes
        .iter()
        .find(|n| n.header.name == name)
        .and_then(|n| n.face.clone())
    {
        Some(UiNodeFace::Fixture(f)) => f,
        _ => panic!("{name} has a fixture's face"),
    };
    let ring_face = face("ring display");
    let emphasis = |key: &str| {
        ring_face
            .objects
            .iter()
            .find(|o| o.id.key == key)
            .unwrap()
            .emphasis
    };
    assert_eq!(emphasis("r1"), Emphasis::Lit);
    assert_eq!(emphasis("r2"), Emphasis::Full, "in the level you are in");
    assert!(
        face("strip")
            .objects
            .iter()
            .all(|o| o.emphasis == Emphasis::Dimmed),
        "another fixture is outside it"
    );
    let Some(UiShape::Ring { center, .. }) = ring_face.objects.first().map(|o| &o.shape) else {
        panic!("a ring")
    };
    assert_eq!(*center, Vec2::new(35.0, 35.0), "the ring's own centre");
    assert_eq!(ring_face.look, FixtureLook::Editing, "you work inside it");
    assert_eq!(
        face("strip").look,
        FixtureLook::Live,
        "drawn as it looks lit"
    );
    assert!(face("strip").wires.is_empty());
}

/// A card shows its words only when they would be at least 4.5 px on
/// screen; much smaller it shows its face without them, and smaller still
/// only its box.
#[test]
fn a_card_shows_as_much_as_fits() {
    let mut e = editor();
    let detail = |e: &Editor| {
        e.view_nodes()
            .into_iter()
            .find(|n| n.header.name == "main")
            .unwrap()
            .detail
    };
    e.camera.scale = 4.5 / 3.9 + 0.01;
    assert_eq!(detail(&e), DetailLevel::Full);
    e.camera.scale = 4.5 / 3.9 - 0.01;
    assert_eq!(detail(&e), DetailLevel::Face);
    e.camera.scale = 0.1;
    assert_eq!(detail(&e), DetailLevel::Dot);
}

/// Selecting a fixture tints the port rows it lands on, and selecting a
/// lamp rings its pixel on its port's row; the run its object is on is
/// lit.
#[test]
fn an_outputs_face_shows_where_the_selection_lands() {
    let mut e = editor();
    let port_row = |e: &Editor, output: &str, key: &str| {
        e.view_nodes()
            .into_iter()
            .find(|n| n.header.name == output)
            .and_then(|n| match n.face {
                Some(UiNodeFace::Output(f)) => f.ports.into_iter().find(|p| p.key == key),
                _ => None,
            })
            .unwrap()
    };
    e.select(vec![fixture(&e, "strip")]);
    assert!(port_row(&e, "main", "1").tinted, "the strip lands on D1");
    assert!(!port_row(&e, "main", "0").tinted);
    e.select(vec![lamp("circle1", "r1", 3)]);
    let d0 = port_row(&e, "main", "0");
    assert_eq!(d0.marked, vec![3], "its pixel, ringed");
    assert!(d0.runs.iter().any(|r| r.emphasis == Emphasis::Lit));
}

/// An edge touching the selection is lit and the others step back while
/// anything is selected. Every edge says its channel and nothing more: what
/// one carries is the note beside the cursor while you hover it.
#[test]
fn the_selections_edges_are_lit_and_say_their_channel() {
    let mut e = editor();
    assert!(e.view_edges().iter().all(|x| x.emphasis == Emphasis::Full));
    e.select(vec![fixture(&e, "tail")]);
    let tail = node_id(&e, "tail");
    for x in e.view_edges() {
        if x.from == tail || x.to == tail {
            assert_eq!(x.emphasis, Emphasis::Lit);
        } else {
            assert_eq!(x.emphasis, Emphasis::Quiet);
        }
        assert!(x.shown);
    }
    let out = e
        .view_edges()
        .into_iter()
        .find(|x| x.from == tail && e.project.output(&x.to).is_some())
        .unwrap();
    assert_eq!(out.channel, "control.out");
    assert!(
        out.carries.contains("50 lamps on GPIO 10"),
        "{}",
        out.carries
    );
}

/// The overlay outlines the selection, the level it is in, and what a
/// click would select, each in the shape a click hits, on screen.
#[test]
fn the_overlay_outlines_selection_context_and_hover() {
    let mut e = editor();
    e.select(vec![lamp("circle1", "r1", 3)]);
    e.pointer_move(lamp_screen(&e, "circle1", "r1", 10), none());
    let kinds: Vec<OutlineKind> = e.view_overlay().outlines.iter().map(|o| o.kind).collect();
    // The ring is outlined in its fixture and on its span on D0.
    assert_eq!(
        kinds,
        vec![
            OutlineKind::Context,
            OutlineKind::Context,
            OutlineKind::Selected,
            OutlineKind::Hover
        ]
    );
    let hover = e.hover_target().unwrap();
    let area = e.project.area(&e.camera, &hover).unwrap();
    assert_eq!(e.view_overlay().outlines[3].path, area.outline_path());
}

/// With nothing selected the inspector lists the project's facts; with a
/// fixture selected, its patch and its properties, under the crumbs down
/// to it.
#[test]
fn the_inspector_says_whats_true_of_the_selection() {
    let mut e = editor();
    let titles = |e: &Editor| -> Vec<Option<String>> {
        e.view_inspector()
            .sections
            .iter()
            .map(|s| s.title.clone())
            .collect()
    };
    assert_eq!(titles(&e)[0].as_deref(), Some("Project"));
    e.select(vec![fixture(&e, "tail")]);
    let inspector = e.view_inspector();
    let crumbs: Vec<&str> = inspector.crumbs.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(crumbs, vec!["desk", "tail rig", "tail"]);
    assert!(titles(&e).contains(&Some("Patch".into())));
    assert!(titles(&e).contains(&Some("Properties".into())));
    let patch = &inspector
        .sections
        .iter()
        .find(|s| s.title.as_deref() == Some("Patch"))
        .unwrap()
        .items;
    assert!(
        patch
            .iter()
            .any(|i| matches!(i, UiItem::Rule(r) if r.starts_with("Lands on")))
    );
}

/// The tree pane marks the selection and the level it is in, and a
/// selected lamp lights its path's row.
#[test]
fn the_tree_pane_marks_selection_and_context() {
    let mut e = editor();
    e.select(vec![lamp("path2", "", 3)]);
    let tree = e.view_tree_pane();
    let row = |t: &Target| tree.rows.iter().find(|r| &r.target == t).unwrap();
    assert!(row(&comp("path2")).lit);
    assert!(row(&comp("path2")).context, "the lamp's level is its path");
    e.select(vec![fixture(&e, "strip")]);
    let tree = e.view_tree_pane();
    assert!(tree.rows.iter().any(|r| r.selected && r.label == "strip"));
}

/// Panning builds nothing again but the overlay: the keys of the nodes, the
/// edges and the panels stay the same, so a page keeps what it drew. A
/// selection or an edit changes them.
#[test]
fn panning_changes_no_part_of_the_view_but_the_overlay() {
    let mut e = editor();
    let keys = |e: &Editor| (e.view_nodes_key(), e.view_edges_key(), e.view_panels_key());
    let before = keys(&e);
    e.camera.pan(Vec2::new(40.0, -25.0));
    assert!(keys(&e) == before, "a pan");
    e.select(vec![fixture(&e, "strip")]);
    let selected = keys(&e);
    assert!(selected.0 != before.0 && selected.1 != before.1 && selected.2 != before.2);
    e.edit_selection_prop("width", PropEdit::SetNumber(90.0));
    assert!(keys(&e).0 != selected.0, "an edit");
}

// ---- the two layouts and the screen ----------------------------------------------

/// On a phone-sized screen the canvas starts in the stack layout; on a
/// tablet or a desk, in the canvas layout.
#[test]
fn a_phone_starts_in_the_stack() {
    let mut e = editor();
    assert_eq!(e.layout(), LayoutKind::Canvas, "a desk");
    e.report_size_class(SizeClass::of_width(390.0));
    assert_eq!(e.size_class(), SizeClass::Sm);
    assert_eq!(e.layout(), LayoutKind::Stack);
    e.report_size_class(SizeClass::of_width(820.0));
    assert_eq!(e.size_class(), SizeClass::Md);
    assert_eq!(e.layout(), LayoutKind::Canvas);
}

/// A layout you choose wins over the screen size's default, until you give
/// the choice back. Debug can force a size class, as if the screen were
/// that size.
#[test]
fn a_layout_you_choose_wins_over_the_screen() {
    let mut e = editor();
    e.set_layout(Some(LayoutKind::Stack));
    assert_eq!(e.layout(), LayoutKind::Stack, "on a desk");
    e.report_size_class(SizeClass::Sm);
    e.set_layout(Some(LayoutKind::Canvas));
    assert_eq!(e.layout(), LayoutKind::Canvas, "on a phone");
    e.set_layout(None);
    assert_eq!(e.layout(), LayoutKind::Stack, "the phone's own again");
    e.report_size_class(SizeClass::Lg);
    e.force_size_class(Some(SizeClass::Sm));
    assert_eq!(e.size_class(), SizeClass::Sm);
    assert_eq!(e.layout(), LayoutKind::Stack);
    assert_eq!(e.view_toolbar().forced_size_class, Some(SizeClass::Sm));
}

/// In the stack layout only the selected node's edges show (in and out);
/// with nothing selected, none do. On the canvas every edge shows.
#[test]
fn in_the_stack_only_the_selected_nodes_edges_show() {
    let mut e = editor();
    e.set_layout(Some(LayoutKind::Stack));
    assert!(e.view_edges().iter().all(|x| !x.shown));
    e.select(vec![fixture(&e, "strip")]);
    let strip = node_id(&e, "strip");
    for x in e.view_edges() {
        assert_eq!(
            x.shown,
            x.from == strip || x.to == strip,
            "{} → {}",
            x.from,
            x.to
        );
    }
    e.set_layout(Some(LayoutKind::Canvas));
    assert!(e.view_edges().iter().all(|x| x.shown));
}

/// The node the selection is in is the one the stack brings into view: a
/// selected node, or the fixture or output holding what's selected.
#[test]
fn the_selection_has_a_node_it_is_in() {
    let mut e = editor();
    assert_eq!(e.focus_node(), None);
    e.select(vec![lamp("circle1", "r1", 3)]);
    assert_eq!(e.focus_node(), Some(node_id(&e, "ring display")));
    let main = node_id(&e, "main");
    e.select(vec![pixel(&main, "0", 2)]);
    assert_eq!(e.focus_node(), Some(main));
    e.select(vec![fixture(&e, "strip")]);
    assert_eq!(e.focus_node(), Some(node_id(&e, "strip")));
}

/// In the stack a fixture fills the column, drawn bigger than its own box,
/// and what's drawn in it comes along: its lamps land scaled with it, a
/// click on a lamp picks that lamp, and a drag moves its object by the
/// same amount in the fixture's own units.
#[test]
fn a_fixture_drawn_bigger_in_the_stack_still_picks_and_drags() {
    let mut e = editor();
    e.set_layout(Some(LayoutKind::Stack));
    let placement = lab_layout::stack_layout(&e.layout_input(), 300.0);
    e.set_placement(&placement);
    e.fit();
    let ring = node_id(&e, "ring display");
    let box_ = e.project.node_rect(&ring).unwrap();
    let k = box_.width() / 70.0;
    assert!(k > 3.0, "the ring display is drawn {k}× its box");
    let lamp0 = lamp_pos(&e, "circle1", "r1", 0);
    let local = local_lamps(&e.project, &ComponentId::new("circle1"), 0)[0];
    assert!((lamp0.x - (box_.min.x + local.x * k)).abs() < 1e-9);
    click_lamp(&mut e, "circle1", "r1", 0, cmd());
    assert_eq!(e.selection, vec![lamp("circle1", "r1", 0)]);
    // Drag the circle 30 world units right: 30 / k in its own units.
    e.select(vec![comp("circle1")]);
    let at = e.camera.to_screen(lamp0);
    let to = at + Vec2::new(30.0 * e.camera.scale, 0.0);
    drag(&mut e, at, to, none());
    let Some(ComponentKind::Circle { center, .. }) = e
        .project
        .component(&ComponentId::new("circle1"))
        .map(|c| c.kind.clone())
    else {
        panic!("a circle")
    };
    assert!((center.x - (35.0 + 30.0 / k)).abs() < 1e-6, "{center:?}");
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
const CREST: f64 = breath::BREATH_SECONDS / 2.0;

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

/// Where the handle `which` picks sits on screen.
fn handle_at(e: &Editor, which: impl Fn(&HandleKind) -> bool) -> Vec2 {
    let h = e
        .handles()
        .into_iter()
        .find(|h| which(&h.kind))
        .expect("the handle is shown");
    e.camera.to_screen(h.at)
}

/// The node a target names (edges only ever end on nodes now).
fn node_of_target(t: &Target) -> NodeId {
    match t {
        Target::Node(id) => id.clone(),
        other => panic!("not a node: {other:?}"),
    }
}

fn editor() -> Editor {
    editor_of(Project::seed())
}

/// An editor on `p`, laid out the way the page does it.
fn editor_of(p: Project) -> Editor {
    let mut e = Editor::new(p);
    lay_out(&mut e);
    e.resize(Vec2::new(1200.0, 800.0));
    e
}

/// What the page does after every change: build the layout input, run the
/// layout, and hand the editor where everything landed.
fn lay_out(e: &mut Editor) {
    let placement = lab_layout::canvas_layout(&e.layout_input());
    e.set_placement(&placement);
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

/// An object on `pixel` of `port`, reversed and turned: its own placement.
fn placed_at(
    object: &ObjectId,
    output: &NodeId,
    port: &str,
    pixel: u32,
    reversed: bool,
    rotation: u32,
) -> PatchPlace {
    PatchPlace {
        on: PatchTarget::Object(object.clone()),
        wire: Some(Wire::At {
            output: output.clone(),
            port: port.into(),
            pixel,
        }),
        direction: reversed.then_some(Direction::Reversed),
        turn: (rotation != 0).then_some(rotation),
    }
}

/// The fixture's own wire, auto onto `output`.
fn auto_onto(output: &NodeId) -> PatchPlace {
    PatchPlace {
        wire: Some(Wire::Auto {
            output: output.clone(),
        }),
        ..PatchPlace::on(PatchTarget::Fixture)
    }
}

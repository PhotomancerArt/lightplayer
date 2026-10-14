//! Statements: how the editor behaves, one sentence per test.
//!
//! Each test's doc comment IS the statement. Read this file as the list.
//! When a statement changes, change the sentence first, then the code.
//!
//! The project is `Project::seed()`, "desk", laid out in columns:
//! - module `tail rig`, holding
//!   - fixture `tail`: group1 (line1 out, line2 back), lines at x = 14 and
//!     22, auto-patched to `tail out`;
//!   - output `tail out`: one port, GPIO 10, 60 px;
//! - fixture `ring display`: circle1 at (35, 35), rings r1 (outer, 40
//!   lamps) and r2 (inner, 24 lamps), auto-patched to `main`;
//! - fixture `strip`: line3, (6, 10) → (74, 10), 30 lamps, auto-patched to
//!   `main` after the ring display;
//! - output `main`: ports D0 (64 px) and D1 (30 px);
//! - fixture `scarf`: empty, no rule, to draw into.
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
    assert_eq!(e.project.outputs().len(), 2, "and the outputs");
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
        vec![comp("group1"), comp("circle1"), comp("line3")]
    );
    key(&mut e, Key::Char('a'), cmd());
    assert_eq!(e.selection, e.project.children(None), "the nodes");
    key(&mut e, Key::Delete, none());
    assert!(e.project.nodes.is_empty());
}

/// Deleting a lamp is refused, and the refusal says why and what to do.
#[test]
fn deleting_a_lamp_is_refused_with_a_reason() {
    let mut e = editor();
    e.select(vec![lamp("line3", "line", 4)]);
    key(&mut e, Key::Delete, none());
    let n = e.notice.clone().expect("a notice");
    assert_eq!(n.kind, NoticeKind::Refused);
    assert!(n.why.unwrap().contains("line3"));
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
    e.select(vec![comp("line1")]);
    click_lamp(&mut e, "line2", "line", 10, none());
    assert_eq!(e.selection, vec![comp("line2")]);
}

/// ⌘-click selects the lamp directly, from anywhere.
#[test]
fn command_click_selects_the_lamp_directly() {
    let mut e = editor();
    click_lamp(&mut e, "line2", "line", 10, cmd());
    assert_eq!(e.selection, vec![lamp("line2", "line", 10)]);
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
        vec![comp("line1")],
    ];
    let probes = [
        ("circle1", "r2", 5),
        ("circle1", "r1", 2),
        ("line1", "line", 3),
        ("line3", "line", 0),
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

/// Arrow keys walk the lamps of a line one at a time.
#[test]
fn arrows_walk_the_lamps() {
    let mut e = editor();
    e.select(vec![lamp("line3", "line", 0)]);
    key(&mut e, Key::Right, none());
    key(&mut e, Key::Right, none());
    assert_eq!(e.selection, vec![lamp("line3", "line", 2)]);
    key(&mut e, Key::Left, none());
    assert_eq!(e.selection, vec![lamp("line3", "line", 1)]);
}

/// A box selects everything at the current level that it touches; with ⌥
/// held, only what's fully inside.
#[test]
fn box_touches_and_option_encloses() {
    // Around the top half of the tail: it touches group1 but does not hold
    // all of it.
    for (mods, want) in [(none(), vec![comp("group1")]), (alt(), vec![])] {
        let mut e = editor();
        let a = e.camera.to_screen(place(&e, "tail", 5.0, 5.0));
        let b = e.camera.to_screen(place(&e, "tail", 34.0, 40.0));
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
    // Between the tail's two lines, x = 14 and x = 22.
    let p = place(&e, "tail", 18.0, 40.0);
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
    for start in [vec![], vec![comp("circle1")], vec![comp("line1")]] {
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
    // A line crossing line3 at (40, 10).
    cross_line3(&mut e);
    let at = place(&e, "strip", 40.0, 10.0);
    click_at(&mut e, at, alt());
    let first = e.selection.clone();
    click_at(&mut e, at, alt());
    let second = e.selection.clone();
    click_at(&mut e, at, alt());
    assert_ne!(first, second);
    assert_eq!(e.selection, first, "round again");
    let both = [comp("line3"), comp("line4")];
    assert!(both.contains(&first[0]) && both.contains(&second[0]));
}

/// When more than one thing at the click's level is under the cursor, the
/// cursor note says so and how to reach the others.
#[test]
fn the_cursor_note_names_overlaps() {
    let mut e = editor();
    cross_line3(&mut e);
    e.pointer_move(e.camera.to_screen(place(&e, "strip", 40.0, 10.0)), none());
    let note = e.cursor_note().expect("a note");
    assert!(
        note.contains("2 lines here") && note.contains("⌥ click") && note.contains("right-click"),
        "{note}"
    );
}

/// Right-click lists everything under the cursor, every level, in tree
/// order; choosing an item selects it; Esc closes the list and changes
/// nothing. The list opens only when asked for.
#[test]
fn right_click_lists_whats_here() {
    let mut e = editor();
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

/// Dragging a fixture moves nothing and says why.
#[test]
fn dragging_a_fixture_is_refused() {
    let mut e = editor();
    let before = e.project.clone();
    let corner = e.camera.to_screen(place(&e, "tail", 0.0, 0.0));
    let title = corner + Vec2::new(12.0, -10.0);
    e.pointer_down(title, Button::Primary, none());
    e.pointer_move(title + Vec2::new(40.0, 0.0), none());
    e.pointer_up(title + Vec2::new(40.0, 0.0), none());
    assert_eq!(e.project, before);
    assert_eq!(e.notice.unwrap().kind, NoticeKind::Refused);
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
    let a = e.camera.to_screen(place(&e, "tail", 5.0, 77.0));
    e.pointer_down(a, Button::Primary, none());
    e.pointer_move(a + Vec2::new(10.0, -10.0), none());
    e.pointer_leave();
    let outside = Vec2::new(e.camera.to_screen(place(&e, "tail", 30.0, 0.0)).x, -500.0);
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
        .add_fixture(Some(&inner), "spare strip", Vec2::new(30.0, 10.0));
    let p = &e.project;
    let mut levels: Vec<Vec<&Node>> = vec![p.nodes.iter().collect()];
    for n in p.all_nodes() {
        if !n.children().is_empty() {
            levels.push(n.children().iter().collect());
        }
    }
    assert!(levels.len() >= 3);
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
                assert!(apart, "{na} {a:?} overlaps {nb} {b:?}");
            }
        }
    }
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
    // Between the tail's two lines: group1, through the module and the
    // fixture.
    let between = place(&e, "tail", 18.0, 40.0);
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
    let d1 = e.project.port_rect(&main, "p2").unwrap();
    let at = d1.min.lerp(d1.max, 0.5);
    click_at(&mut e, at, none());
    assert_eq!(e.selection, vec![Target::Node(main.clone())]);
    click_at(&mut e, at, none());
    assert_eq!(e.selection, vec![port(&main, "p2")]);
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
    assert_eq!(p.port_fill(&main, "p1"), (64, 64));
    assert_eq!(p.port_fill(&main, "p2"), (30, 30));
    assert_eq!(p.port_fill(&tail_out, "p1"), (50, 60));
    let patch = p.effective_patch();
    assert!(patch.unpatched.is_empty() && patch.problems.is_empty());
    // The ring display's lamps: r1 then r2, in data order, from pixel 0.
    let d0: Vec<(String, std::ops::Range<u32>, u32)> = patch
        .spans
        .iter()
        .filter(|s| s.output == main && s.port == "p1")
        .map(|s| (s.object.key.clone(), s.lamps.clone(), s.start_pixel))
        .collect();
    assert_eq!(d0, vec![("r1".into(), 0..40, 0), ("r2".into(), 0..24, 40)]);
    // One more lamp on the strip has nowhere to go: it is unpatched, not
    // squeezed in.
    let mut e = editor();
    e.select(vec![comp("line3")]);
    key(&mut e, Key::Char('='), none());
    let patch = e.project.effective_patch();
    assert_eq!(patch.unpatched.len(), 1);
    assert_eq!(patch.unpatched[0].lamps, 30..31);
}

/// Patch edges match the effective patch: bundled per (fixture, output)
/// from outside, one per span from inside a fixture.
#[test]
fn patch_edges_match_the_effective_patch() {
    let mut e = editor();
    let edges = e.edges();
    let bundled: Vec<(Target, Target, String)> = edges
        .iter()
        .map(|x| (x.from.clone(), x.to.clone(), x.label.clone()))
        .collect();
    assert_eq!(
        bundled,
        vec![
            (
                fixture(&e, "tail"),
                fixture(&e, "tail out"),
                "50 lamps".into()
            ),
            (
                fixture(&e, "ring display"),
                fixture(&e, "main"),
                "64 lamps".into()
            ),
            (fixture(&e, "strip"), fixture(&e, "main"), "30 lamps".into()),
        ]
    );
    assert!(edges.iter().all(|x| x.bundled && x.active));

    // Inside the tail: one edge per span, from each line to its port.
    e.select(vec![comp("line2")]);
    let spans = e.project.effective_patch().spans;
    let tail = node_id(&e, "tail");
    let tail_out = node_id(&e, "tail out");
    let all = e.edges();
    let inside: Vec<&Edge> = all.iter().filter(|x| !x.bundled).collect();
    let tail_spans: Vec<_> = spans.iter().filter(|s| s.fixture == tail).collect();
    assert_eq!(inside.len(), tail_spans.len());
    assert_eq!(inside[0].from, comp("line1"));
    assert_eq!(inside[0].to, port(&tail_out, "p1"));
    assert_eq!(inside[1].label, "lamps 0–24 → GPIO 10 @25");
    // The other fixtures stay bundled.
    assert_eq!(e.edges().iter().filter(|x| x.bundled).count(), 2);
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
    let d1 = e.project.lamps_on_port(&main, "p2");
    let line3 = ObjectId::new(&ComponentId::new("line3"), "line");
    assert_eq!(d1, (0..30).map(|i| (line3.clone(), i)).collect::<Vec<_>>());
    let d0 = e.project.lamps_on_port(&main, "p1");
    assert_eq!(d0.len(), 64);
    assert!(d0.iter().all(|(o, _)| o.component.as_str() == "circle1"));
    // And the other way: the ring display lands on D0 only.
    assert_eq!(
        e.project.ports_of(&fixture(&e, "ring display")),
        vec![port(&main, "p1")]
    );
}

/// Edges stay quiet: with nothing selected none shows a label, and with
/// something selected only the edges attached to that thing itself do.
#[test]
fn only_edges_on_the_selection_are_labelled() {
    let mut e = editor();
    assert!(e.edges().iter().all(|x| !x.labelled), "nothing selected, no labels");

    let main = node_id(&e, "main");
    e.select(vec![port(&main, "p2")]);
    let labelled: Vec<Edge> = e.edges().into_iter().filter(|x| x.labelled).collect();
    assert_eq!(labelled.len(), 1, "only the strip's edge lands on D1");
    assert_eq!(labelled[0].from, fixture(&e, "strip"));

    // Inside the ring display, the circle's per-ring edges aren't attached
    // to a selected ring, so only the selected ring's edge is labelled.
    e.select(vec![ring("circle1", "r2")]);
    let labelled: Vec<Edge> = e.edges().into_iter().filter(|x| x.labelled).collect();
    assert!(labelled.iter().all(|x| x.from == ring("circle1", "r2")), "{labelled:?}");
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

/// Changing a line's count keeps the selected lamp selected.
#[test]
fn selection_survives_a_count_change() {
    let mut e = editor();
    e.select(vec![lamp("line3", "line", 7)]);
    key(&mut e, Key::Char('='), none());
    assert_eq!(e.selection, vec![lamp("line3", "line", 7)]);
    assert_eq!(
        e.project
            .object(&ObjectId::new(&ComponentId::new("line3"), "line"))
            .unwrap()
            .lamps
            .len(),
        31
    );
}

/// Shrinking a line past the selected lamp moves the selection to the new
/// last lamp; it never vanishes.
#[test]
fn shrinking_past_the_selected_lamp_keeps_the_last_one() {
    let mut e = editor();
    e.select(vec![lamp("line3", "line", 29)]);
    key(&mut e, Key::Char('-'), none());
    assert_eq!(e.selection, vec![lamp("line3", "line", 28)]);
}

/// With many things selected, the inspector shows what they share, mixed
/// values say so, and an edit lands on every one.
#[test]
fn inspector_edits_everything_selected() {
    let mut e = editor();
    e.select(vec![comp("line1"), comp("line3"), ring("circle1", "r2")]);
    let props = common_props(&e.project, &e.selection);
    let lamps = props
        .iter()
        .find(|p| p.desc.id == "lamps")
        .expect("lamps is common");
    assert_eq!(lamps.value.describe(), "mixed 24–30");
    assert!(
        props.iter().all(|p| p.desc.id != "radius"),
        "lines have no radius"
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

/// Reversing a line keeps the selected lamp on the same physical lamp.
#[test]
fn reverse_keeps_the_same_physical_lamp() {
    let mut e = editor();
    e.select(vec![lamp("line3", "line", 2)]);
    let before = lamp_pos(&e, "line3", "line", 2);
    key(&mut e, Key::Char('r'), none());
    let Target::Lamp { index, .. } = e.selection[0].clone() else {
        panic!()
    };
    assert_eq!(index, 27);
    assert!(lamp_pos(&e, "line3", "line", index).distance(before) < 1e-9);
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

/// Drawing a line puts it in the fixture you start in, gives it one lamp
/// every `spacing` units, selects it, and goes back to the select tool.
#[test]
fn drawing_a_line_counts_lamps_from_its_length() {
    let mut e = editor();
    key(&mut e, Key::Char('l'), none());
    let a = e.camera.to_screen(place(&e, "scarf", 10.0, 15.0));
    let b = e.camera.to_screen(place(&e, "scarf", 34.0, 15.0));
    e.pointer_down(a, Button::Primary, none());
    e.pointer_move(b, none());
    e.pointer_up(b, none());
    let Target::Component(id) = e.selection[0].clone() else {
        panic!()
    };
    assert_eq!(id.as_str(), "line4", "ids are never reused");
    assert_eq!(e.project.lamps_under(&e.selection[0]).len(), 11); // 24 / 2.4 + 1
    assert_eq!(e.tool, Tool::Select);
    assert_eq!(e.project.fixture_of(&id).unwrap().name, "scarf");
}

/// Drawing that starts outside every fixture is refused, with where to
/// start instead.
#[test]
fn drawing_outside_every_fixture_is_refused() {
    let mut e = editor();
    key(&mut e, Key::Char('l'), none());
    let x = e.project.extent();
    let a = e.camera.to_screen(Vec2::new(x.max.x + 20.0, x.min.y));
    e.pointer_down(a, Button::Primary, none());
    e.pointer_move(a + Vec2::new(80.0, 0.0), none());
    e.pointer_up(a + Vec2::new(80.0, 0.0), none());
    assert_eq!(e.notice.unwrap().kind, NoticeKind::Refused);
    assert_eq!(e.project.lamp_count(), 144);
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
        port(&main, "p1"),
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

// ---- helpers -------------------------------------------------------------------

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

/// A short line in `strip` crossing line3 at (40, 10).
fn cross_line3(e: &mut Editor) {
    let strip = node_id(e, "strip");
    e.project.add(
        &strip,
        ComponentKind::line(Vec2::new(40.0, 2.0), Vec2::new(40.0, 18.0), 6),
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

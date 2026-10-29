//! Statements: how nodes lay out, one sentence per test.
//!
//! Each test's doc comment IS the statement; read this file as the list.
//! When a statement changes, change the sentence first, then the code.
//!
//! Cards are the lab's: 60 wide, 27 tall (a header and one row), an output
//! 14 taller per port.

use lab_layout::spacing::*;
use lab_layout::*;

// ---- stability -------------------------------------------------------------

/// The same input always lays out the same way.
#[test]
fn the_same_input_always_lays_out_the_same_way() {
    for input in [desk(), choker(), fyeah_sign(), two_modules()] {
        assert_eq!(canvas_layout(&input), canvas_layout(&input.clone()));
        assert_eq!(stack_layout(&input, 300.0), stack_layout(&input, 300.0));
    }
}

/// Nothing laid out overlaps anything else at its level.
#[test]
fn nothing_overlaps_anything_else_at_its_level() {
    for input in [
        desk(),
        choker(),
        fyeah_sign(),
        two_modules(),
        many_outputs(),
    ] {
        for p in [canvas_layout(&input), stack_layout(&input, 300.0)] {
            for a in &input.nodes {
                for b in &input.nodes {
                    if a.id < b.id && a.parent == b.parent {
                        let (ra, rb) = (rect(&p, &a.id), rect(&p, &b.id));
                        assert!(
                            !ra.overlaps(&rb),
                            "{} {ra:?} overlaps {} {rb:?}",
                            a.id,
                            b.id
                        );
                    }
                }
            }
        }
    }
}

/// Adding a node inside a module moves nothing in the pipeline outside it,
/// and what sits below the module moves down together, keeping its shape.
#[test]
fn adding_a_node_inside_a_module_moves_only_what_is_below_it_and_as_one() {
    let before = two_modules();
    let mut after = before.clone();
    after
        .nodes
        .push(fixture("rig a extra", 50.0, 40.0).in_module("rig a"));
    let (b, a) = (canvas_layout(&before), canvas_layout(&after));

    for id in ["shader", "strip", "strip out", "rig a"] {
        assert_eq!(rect(&b, id).min, rect(&a, id).min, "{id} moved");
    }
    for id in ["rig a shader", "rig a fixture", "rig a out"] {
        assert_eq!(rect(&b, id), rect(&a, id), "{id}, inside the module, moved");
    }
    let shift = rect(&a, "rig b").min - rect(&b, "rig b").min;
    assert!(
        shift.y > 0.0 && shift.x == 0.0,
        "rig b moves down: {shift:?}"
    );
    for id in ["rig b fixture", "rig b out"] {
        assert_eq!(
            rect(&a, id).min - rect(&b, id).min,
            shift,
            "{id} moves with rig b"
        );
    }
}

/// Adding a fixture to a column moves nothing in the columns to its left.
#[test]
fn adding_a_fixture_moves_nothing_to_its_left() {
    let before = desk();
    let mut after = before.clone();
    after.nodes.push(fixture("new", 70.0, 70.0));
    after.edges.push(("rainbow".into(), "new".into()));
    let (b, a) = (canvas_layout(&before), canvas_layout(&after));
    assert_eq!(rect(&b, "rainbow"), rect(&a, "rainbow"));
    for id in ["ring", "strip", "scarf"] {
        assert_eq!(
            rect(&b, id),
            rect(&a, id),
            "{id}, above it in its own column, moved"
        );
    }
}

// ---- the canvas layout, by role -------------------------------------------

/// In a pipeline, a source sits left of the fixture it feeds, and the
/// fixture left of the output it feeds.
#[test]
fn a_pipeline_reads_left_to_right() {
    for input in [desk(), choker(), fyeah_sign()] {
        let p = canvas_layout(&input);
        for (from, to) in &input.edges {
            let (role_from, role_to) = (role(&input, from), role(&input, to));
            if matches!(
                (role_from, role_to),
                (LayoutRole::Source, LayoutRole::Fixture)
                    | (LayoutRole::Fixture, LayoutRole::Output)
            ) {
                assert!(
                    rect(&p, from).max.x <= rect(&p, to).min.x - COLUMN_GAP,
                    "{from} should sit left of {to}"
                );
            }
        }
    }
}

/// An output sits level with the fixture feeding it; one fed by two sits
/// level with their middle.
#[test]
fn an_output_sits_level_with_what_feeds_it() {
    let p = canvas_layout(&desk());
    let middle = |r: Rect| r.centre().y;
    assert_eq!(middle(rect(&p, "scarf out")), middle(rect(&p, "scarf")));
    let (ring, strip) = (rect(&p, "ring"), rect(&p, "strip"));
    assert_eq!(
        middle(rect(&p, "main")),
        (ring.min.y + strip.max.y) / 2.0,
        "main, fed by the ring and the strip, sits level with their middle"
    );
}

/// Outputs keep the order of the fixtures feeding them, pushing down to
/// clear one another rather than overlapping.
#[test]
fn outputs_keep_the_order_of_what_feeds_them() {
    let input = many_outputs();
    let p = canvas_layout(&input);
    let tops: Vec<f64> = (1..=6)
        .map(|i| rect(&p, &format!("out {i}")).min.y)
        .collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "in order: {tops:?}");
}

/// Controls sit in a row above the pipeline.
#[test]
fn controls_sit_in_a_row_above_the_pipeline() {
    let input = fyeah_sign();
    let p = canvas_layout(&input);
    let controls = ["clock", "button", "radio"].map(|id| rect(&p, id));
    assert!(
        controls.iter().all(|r| r.min.y == controls[0].min.y),
        "one row"
    );
    assert!(
        controls.windows(2).all(|w| w[0].max.x < w[1].min.x),
        "in order"
    );
    let row_bottom = controls.iter().map(|r| r.max.y).fold(0.0, f64::max);
    for id in ["shader a", "shader b", "playlist", "sign", "sign out"] {
        assert!(
            rect(&p, id).min.y >= row_bottom + NODE_GAP,
            "{id} is below the row"
        );
    }
}

/// A module wraps its children with room for their titles.
#[test]
fn a_module_wraps_its_children_with_room_for_their_titles() {
    let input = choker();
    let p = canvas_layout(&input);
    let module = rect(&p, "playful");
    for n in input
        .nodes
        .iter()
        .filter(|n| n.parent.as_deref() == Some("playful"))
    {
        let r = rect(&p, &n.id);
        assert!(r.min.x >= module.min.x + MODULE_PAD, "{} left", n.id);
        assert!(r.min.y >= module.min.y + MODULE_PAD_TOP, "{} top", n.id);
        assert!(r.max.x <= module.max.x - MODULE_PAD, "{} right", n.id);
        assert!(r.max.y <= module.max.y - MODULE_PAD, "{} bottom", n.id);
    }
    assert_eq!(
        p.inside(Some("playful")),
        module.min + Vec2::new(MODULE_PAD, MODULE_PAD_TOP)
    );
}

/// Modules stack below the pipeline of the container holding them.
#[test]
fn modules_stack_below_the_pipeline() {
    let p = canvas_layout(&two_modules());
    let pipeline_bottom = ["shader", "strip", "strip out"]
        .map(|id| rect(&p, id).max.y)
        .into_iter()
        .fold(0.0, f64::max);
    let (a, b) = (rect(&p, "rig a"), rect(&p, "rig b"));
    assert!(a.min.y >= pipeline_bottom + NODE_GAP);
    assert!(b.min.y >= a.max.y + NODE_GAP);
}

/// A node that is none of the pipeline's roles goes in a column of its own
/// at the right.
#[test]
fn free_nodes_go_in_a_column_at_the_right() {
    let mut input = desk();
    input.nodes.push(card("fluid sim", LayoutRole::Other));
    let p = canvas_layout(&input);
    let free = rect(&p, "fluid sim");
    for n in input
        .nodes
        .iter()
        .filter(|n| n.parent.is_none() && n.id != "fluid sim")
    {
        assert!(free.min.x >= rect(&p, &n.id).max.x, "right of {}", n.id);
    }
}

// ---- pins -------------------------------------------------------------------

/// A pinned node stays where it was pinned, and the rest clear it.
#[test]
fn a_pinned_node_stays_put_and_the_rest_clear_it() {
    // Pinned right where the shader would go.
    let input = desk().pin("strip", Vec2::new(0.0, 0.0));
    let p = canvas_layout(&input);
    assert_eq!(rect(&p, "strip").min, Vec2::new(0.0, 0.0));
    assert!(!rect(&p, "rainbow").overlaps(&rect(&p, "strip")));
}

/// A pin inside a module never sits above or left of its inside.
#[test]
fn a_pin_inside_a_module_stays_inside() {
    let input = choker().pin("fixture", Vec2::new(-40.0, -40.0));
    let p = canvas_layout(&input);
    assert_eq!(rect(&p, "fixture").min, p.inside(Some("playful")));
}

// ---- the stack layout -------------------------------------------------------

/// The stack layout lists nodes in tree order, a module's children right
/// after it and inset under it.
#[test]
fn the_stack_lists_nodes_in_tree_order_children_inset() {
    let input = two_modules();
    let p = stack_layout(&input, 300.0);
    let order = [
        "shader",
        "strip",
        "strip out",
        "rig a",
        "rig a shader",
        "rig a fixture",
        "rig a out",
        "rig b",
        "rig b fixture",
        "rig b out",
    ];
    let tops: Vec<f64> = order.iter().map(|id| rect(&p, id).min.y).collect();
    assert!(
        tops.windows(2).all(|w| w[0] < w[1]),
        "in tree order: {tops:?}"
    );
    assert_eq!(rect(&p, "shader").min.x, 0.0);
    assert_eq!(rect(&p, "rig a fixture").min.x, STACK_INSET);
    assert_eq!(rect(&p, "rig a fixture").width(), 300.0 - STACK_INSET);
    let rig = rect(&p, "rig a");
    assert!(
        rig.max.y >= rect(&p, "rig a out").max.y,
        "the module covers its children"
    );
}

/// In the stack, a fixture keeps its box's shape at the column's width,
/// and every other card keeps its height.
#[test]
fn in_the_stack_a_fixture_keeps_its_shape() {
    let p = stack_layout(&desk(), 300.0);
    let ring = rect(&p, "ring");
    assert_eq!(ring.width(), 300.0);
    assert_eq!(ring.height(), 300.0, "the ring's box is square");
    assert_eq!(rect(&p, "rainbow").height(), 27.0);
}

// ---- the catalog's shapes ---------------------------------------------------

/// The PLAYFUL choker lays out by role: its clock and power button in a row
/// over the shader, the fixture and the output, all in one module.
#[test]
fn the_choker_lays_out_by_role() {
    let p = canvas_layout(&choker());
    let (clock, power) = (rect(&p, "clock"), rect(&p, "power"));
    let (shader, fixture, output) = (rect(&p, "shader"), rect(&p, "fixture"), rect(&p, "output"));
    assert_eq!(clock.min.y, power.min.y);
    assert!(shader.min.y > clock.max.y);
    assert!(shader.max.x < fixture.min.x && fixture.max.x < output.min.x);
    assert_eq!(output.centre().y, fixture.centre().y);
}

/// The zook dome lays out by role: clock above, then shader, fixture and
/// output left to right, the output level with the dome.
#[test]
fn the_zook_dome_lays_out_by_role() {
    let input = LayoutInput::new(vec![
        LayoutNode::new("module", LayoutRole::Module, Vec2::ZERO),
        card("clock", LayoutRole::Control).in_module("module"),
        card("shader", LayoutRole::Source).in_module("module"),
        fixture("fixture", 200.0, 200.0).in_module("module"),
        output("output", 5).in_module("module"),
    ])
    .edge("clock", "shader")
    .edge("shader", "fixture")
    .edge("fixture", "output");
    let p = canvas_layout(&input);
    let (clock, shader) = (rect(&p, "clock"), rect(&p, "shader"));
    let (fixture, output) = (rect(&p, "fixture"), rect(&p, "output"));
    assert!(clock.max.y < shader.min.y && clock.max.y < fixture.min.y);
    assert!(shader.max.x < fixture.min.x && fixture.max.x < output.min.x);
    assert_eq!(output.centre().y, fixture.centre().y);
}

/// The fyeah sign lays out by role: clock, button and radio in a row, the
/// playlist and its shaders in the source column, then the sign, then its
/// output.
#[test]
fn the_fyeah_sign_lays_out_by_role() {
    let p = canvas_layout(&fyeah_sign());
    let sign = rect(&p, "sign");
    for id in ["shader a", "shader b", "playlist"] {
        assert_eq!(rect(&p, id).min.x, 0.0, "{id} in the source column");
        assert!(rect(&p, id).max.x < sign.min.x);
    }
    let out = rect(&p, "sign out");
    assert!(sign.max.x < out.min.x);
    assert_eq!(out.centre().y, sign.centre().y);
}

// ---- helpers ----------------------------------------------------------------

fn card(id: &str, role: LayoutRole) -> LayoutNode {
    LayoutNode::new(id, role, Vec2::new(60.0, 27.0))
}

fn output(id: &str, ports: usize) -> LayoutNode {
    LayoutNode::new(
        id,
        LayoutRole::Output,
        Vec2::new(60.0, 13.0 + 14.0 * ports as f64),
    )
}

fn fixture(id: &str, w: f64, h: f64) -> LayoutNode {
    LayoutNode::new(id, LayoutRole::Fixture, Vec2::new(w, h))
}

fn module(id: &str) -> LayoutNode {
    LayoutNode::new(id, LayoutRole::Module, Vec2::ZERO)
}

fn rect(p: &Placement, id: &str) -> Rect {
    p.rect(id).unwrap_or_else(|| panic!("{id} was not placed"))
}

fn role(input: &LayoutInput, id: &str) -> LayoutRole {
    input
        .nodes
        .iter()
        .find(|n| n.id == id)
        .map(|n| n.role)
        .unwrap()
}

/// The lab's own desk: a shader, three fixtures and two outputs at the
/// root (one output fed by two fixtures), and a rig module.
fn desk() -> LayoutInput {
    LayoutInput::new(vec![
        card("rainbow", LayoutRole::Source),
        module("tail rig"),
        card("glow", LayoutRole::Source).in_module("tail rig"),
        fixture("tail", 82.0, 82.0).in_module("tail rig"),
        output("tail out", 1).in_module("tail rig"),
        fixture("ring", 70.0, 70.0),
        fixture("strip", 80.0, 80.0),
        output("main", 2),
        fixture("scarf", 100.0, 100.0),
        output("scarf out", 1),
    ])
    .edge("glow", "tail")
    .edge("rainbow", "ring")
    .edge("rainbow", "strip")
    .edge("rainbow", "scarf")
    .edge("tail", "tail out")
    .edge("ring", "main")
    .edge("strip", "main")
    .edge("scarf", "scarf out")
}

/// The PLAYFUL choker: everything in one module.
fn choker() -> LayoutInput {
    LayoutInput::new(vec![
        module("playful"),
        card("clock", LayoutRole::Control).in_module("playful"),
        card("power", LayoutRole::Control).in_module("playful"),
        card("shader", LayoutRole::Source).in_module("playful"),
        fixture("fixture", 120.0, 40.0).in_module("playful"),
        output("output", 1).in_module("playful"),
    ])
    .edge("clock", "shader")
    .edge("shader", "fixture")
    .edge("fixture", "output")
}

/// The fyeah sign, the catalog's largest: three controls, a playlist of two
/// shaders, the sign and its output.
fn fyeah_sign() -> LayoutInput {
    LayoutInput::new(vec![
        card("clock", LayoutRole::Control),
        card("button", LayoutRole::Control),
        card("radio", LayoutRole::Control),
        card("shader a", LayoutRole::Source),
        card("shader b", LayoutRole::Source),
        card("playlist", LayoutRole::Source),
        fixture("sign", 160.0, 60.0),
        output("sign out", 1),
    ])
    .edge("clock", "shader a")
    .edge("clock", "shader b")
    .edge("shader a", "playlist")
    .edge("shader b", "playlist")
    .edge("button", "playlist")
    .edge("playlist", "sign")
    .edge("sign", "sign out")
}

/// A pipeline at the root and two modules under it.
fn two_modules() -> LayoutInput {
    LayoutInput::new(vec![
        card("shader", LayoutRole::Source),
        fixture("strip", 80.0, 30.0),
        output("strip out", 1),
        module("rig a"),
        card("rig a shader", LayoutRole::Source).in_module("rig a"),
        fixture("rig a fixture", 60.0, 60.0).in_module("rig a"),
        output("rig a out", 2).in_module("rig a"),
        module("rig b"),
        fixture("rig b fixture", 90.0, 50.0).in_module("rig b"),
        output("rig b out", 1).in_module("rig b"),
    ])
    .edge("shader", "strip")
    .edge("strip", "strip out")
    .edge("rig a shader", "rig a fixture")
    .edge("rig a fixture", "rig a out")
    .edge("shader", "rig b fixture")
    .edge("rig b fixture", "rig b out")
}

/// One small fixture feeding six tall outputs, the dome's shape in
/// miniature: they cannot all sit level with it.
fn many_outputs() -> LayoutInput {
    let mut nodes = vec![fixture("dome", 100.0, 100.0)];
    let mut input = LayoutInput::default();
    for i in 1..=6 {
        nodes.push(output(&format!("out {i}"), 4));
        input.edges.push(("dome".into(), format!("out {i}")));
    }
    input.nodes = nodes;
    input
}

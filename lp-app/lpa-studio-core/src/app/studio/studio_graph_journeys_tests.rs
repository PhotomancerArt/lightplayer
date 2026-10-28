//! The mapping rebuild's four journeys (A8), pressed on Studio's Graph by
//! their offers' paths, the way the app agent presses them (mapping
//! rebuild M4 P06; `lpc-mapping/tests/journey_*.rs` are the same journeys
//! as model edits).
//!
//! Each opens a format-6 project on a running in-process board, presses
//! `project/graph/<verb>` with its values (`OfferArgs`), and checks two
//! things after every step: where the lamps land (`lpc_mapping`'s patcher
//! over the Graph's mapping and the port tables Studio built from the
//! outputs' files) and what Studio saved (the fixture's map2d format 6, as
//! `lpc-mapping`'s writer writes the Graph's mapping, read back by the
//! engine's reader). Each also shows one verb it does not offer, and that
//! choosing a port of an output with no name names it (lean Q96).
//!
//! The layout verbs (mapping rebuild M4 P07) are pressed the same way:
//! `the_circular_display_from_nothing` maps A8's lead journey from a new
//! board's project, and the saved file is checked against a golden.

use std::path::{Path, PathBuf};

use lab_mapping_model::{
    Button, GraphInput, GraphVerb, Mods, NodeId, NodeKind, Output, Target, Vec2, port_value,
};
use lpc_mapping::part::{Mapping, TreeId, render};
use lpc_mapping::patching::{
    Amount, ChannelOp, Direction, Flow, OutputName, OutputOp, Override, Patching, PortKey, PortOp,
    Problem, patch_mapping,
};

use crate::app::studio::offer_press_test_api::{OfferPressTestApi, actor_clicks};
use crate::app::studio::studio_graph_e2e_tests::Graph;
use crate::{ActionConsequence, OfferArgs, OfferParamKind};

/// The PLAYFUL choker: F's lower stroke set to start at lamp 60 takes F's
/// bar, U and the last L along after it, and the letters before keep their
/// lamps; back to the flow, it follows Y again at lamp 42 and the file is
/// its own bytes again.
#[test]
fn the_playful_choker() {
    let mut j = Journey::catalog("catalog/playful-choker", "/playful.map2d.json");
    let before_f = ["p", "l1", "a", "a-bar", "y-stem", "y-left", "y-right"];

    // 1. End to end on its one port: F's lower stroke at 42, U at 52, the
    //    last L at 65.
    assert_eq!(j.first_lamp("f-lower"), Some((0, 42)));
    assert_eq!(j.first_lamp("u"), Some((0, 52)));
    assert_eq!(j.first_lamp("l2"), Some((0, 65)));
    let letters: Vec<_> = before_f.iter().map(|id| j.first_lamp(id)).collect();
    j.not_offered("turn-on"); // nothing is off
    j.not_offered("remove-override"); // no override is gone

    // 2. F's lower stroke starts at lamp 60: what follows it flows on
    //    after it, and the letters before keep their lamps.
    j.press(
        "set-starts-at",
        OfferArgs::new()
            .with("subject", "fixture:f-lower")
            .with("lamps", "60"),
    );
    assert_eq!(j.first_lamp("f-lower"), Some((0, 60)));
    assert_eq!(j.first_lamp("f-bar"), Some((0, 63)));
    assert_eq!(j.first_lamp("u"), Some((0, 70)));
    assert_eq!(j.first_lamp("l2"), Some((0, 83)));
    let after: Vec<_> = before_f.iter().map(|id| j.first_lamp(id)).collect();
    assert_eq!(after, letters, "the letters before F keep their lamps");
    let saved = j.saved();
    assert_eq!(
        saved.overrides.get(&id("f-lower")),
        Some(&Override {
            channel: Some(ChannelOp::Set(Amount::lamps(60))),
            ..Override::default()
        }),
        "the file says F's lower stroke starts at lamp 60, and nothing else"
    );
    assert_eq!(saved.overrides.len(), 1);

    // 3. Back to the flow: it follows Y again, and the file is the
    //    catalog's own bytes.
    j.press(
        "back-to-the-flow",
        OfferArgs::new()
            .with("subject", "fixture:f-lower")
            .with("field", "starts-at"),
    );
    assert_eq!(j.first_lamp("f-lower"), Some((0, 42)));
    assert_eq!(j.saved_text(), j.original, "the file's own bytes again");

    // A port of an output with no name is offered; choosing it names it.
    j.choosing_a_port_of_an_unnamed_output_names_it("fixture:f-lower");
}

/// The circular display as soldered (`projects/test/circular-display-6`:
/// rings of 24, 16, 12, 8 and the centre on `main`, two ports of 64):
/// ring 3 reversed, then on port D1 from its first lamp, ring 4 and the
/// centre following it there; then one ring per port from D0 instead,
/// which this display's two ports can't hold past ring 2.
#[test]
fn the_circular_display() {
    let mut j = Journey::repo("projects/test/circular-display-6", "/fixture.map2d.json");
    let ring = |k: &str| format!("fixture:circle1/{k}");

    // As soldered: ring 3 reversed on D1. Everything back to the flow
    // first: end to end on D0, outside in.
    assert_eq!(j.span("circle1/r3"), Some((1, 11, 0)), "as soldered");
    j.press(
        "back-to-the-flow",
        OfferArgs::new()
            .with("subject", ring("r3"))
            .with("field", "all"),
    );
    assert_eq!(j.span("circle1/r1"), Some((0, 0, 23)));
    assert_eq!(j.span("circle1/r2"), Some((0, 24, 39)));
    assert_eq!(j.span("circle1/r3"), Some((0, 40, 51)));
    assert_eq!(j.span("circle1/r4"), Some((0, 52, 59)));
    assert_eq!(j.span("circle1/r5"), Some((0, 60, 60)));
    assert!(j.saved().overrides.is_empty(), "nothing set here any more");

    // 1. Ring 3 reversed: it runs 51 down to 40 in its slot; ring 4 stays.
    j.press(
        "set-direction",
        OfferArgs::new()
            .with("subject", ring("r3"))
            .with("direction", "reverse"),
    );
    assert_eq!(j.span("circle1/r3"), Some((0, 51, 40)));
    assert_eq!(j.span("circle1/r4"), Some((0, 52, 59)));

    // 2. On port D1 from its first lamp: ring 4 and the centre follow it.
    let port = j.offered_option("set-port", "port", "D1 of main");
    j.press(
        "set-port",
        OfferArgs::new()
            .with("subject", ring("r3"))
            .with("port", port),
    );
    assert_eq!(j.span("circle1/r2"), Some((0, 24, 39)));
    assert_eq!(j.span("circle1/r3"), Some((1, 11, 0)));
    assert_eq!(j.span("circle1/r4"), Some((1, 12, 19)));
    assert_eq!(j.span("circle1/r5"), Some((1, 20, 20)));
    assert!(j.patching().problems.is_empty());
    let saved = j.saved();
    assert_eq!(
        saved.overrides.get(&id("circle1/r3")),
        Some(&Override {
            output: Some(OutputOp::Set(OutputName::new("main"))),
            port: Some(PortOp::set(1)),
            channel: Some(ChannelOp::Set(Amount::lamps(0))),
            direction: Some(Direction::Reverse),
            ..Override::default()
        }),
        "the port names its output (Q17), from its first lamp, reversed"
    );

    // 3. One ring per port from D0 instead: ring 3's port goes back to the
    //    flow (its direction stays), and the circle's flow is one per port.
    j.press(
        "back-to-the-flow",
        OfferArgs::new()
            .with("subject", ring("r3"))
            .with("field", "start"),
    );
    j.press(
        "set-flow",
        OfferArgs::new()
            .with("subject", "fixture:circle1")
            .with("flow", "one-per-port:0"),
    );
    assert_eq!(j.span("circle1/r1"), Some((0, 0, 23)));
    assert_eq!(j.span("circle1/r2"), Some((1, 0, 15)));
    // Two ports: ring 3, ring 4 and the centre have none, and say so.
    for k in ["r3", "r4", "r5"] {
        assert_eq!(j.span(&format!("circle1/{k}")), None, "{k} isn't lit");
    }
    let past: Vec<String> = j
        .patching()
        .problems
        .iter()
        .filter_map(|x| match x {
            Problem::PastLastPort { segment } => Some(segment.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(past, ["circle1/r3", "circle1/r4", "circle1/r5"]);
    let saved = j.saved();
    assert_eq!(saved.parts[0].flow, Flow::OnePerPort { from: PortKey(0) });
    assert_eq!(
        saved.overrides.get(&id("circle1/r3")),
        Some(&Override {
            direction: Some(Direction::Reverse),
            ..Override::default()
        }),
        "still reversed"
    );

    j.not_offered("remove-override");
    j.choosing_a_port_of_an_unnamed_output_names_it(&ring("r1"));
}

/// A dome slice (`catalog/zook-dome`: five copies of the sector's 300-lamp
/// path, ports 0–3 of 300 and port 4 taking the rest): one per port; copy
/// 3 moved to port 4, where copy 4 keeps its own slot and the overlap is
/// reported; copy 3 back to the flow.
#[test]
fn a_dome_slice() {
    let mut j = Journey::catalog("catalog/zook-dome", "/fixture.map2d.json");
    let copy = |k: u32| match k {
        0 => "sector".to_string(),
        k => format!("sector@{k}"),
    };

    // End to end, each copy fills its port and the next wraps on.
    for k in 0..5 {
        assert_eq!(j.first_lamp(&copy(k)), Some((k, 0)), "copy {k}");
    }

    // 1. One per port from port 0: each copy keeps its slot.
    j.press(
        "set-flow",
        OfferArgs::new()
            .with("subject", "fixture:repeat1")
            .with("flow", "one-per-port:0"),
    );
    for k in 0..5 {
        assert_eq!(j.first_lamp(&copy(k)), Some((k, 0)), "copy {k}");
    }
    assert!(j.patching().problems.is_empty());

    // 2. Copy 3 to port 4: copy 4 keeps its own slot there (jacks are
    //    physical), and the two overlap.
    let port = j.offered_option("set-port", "port", "IO16 of output 1");
    j.press(
        "set-port",
        OfferArgs::new()
            .with("subject", format!("fixture:{}", copy(3)))
            .with("port", port),
    );
    assert_eq!(j.first_lamp(&copy(3)), Some((4, 0)));
    assert_eq!(j.first_lamp(&copy(4)), Some((4, 0)), "keeps its slot");
    assert!(j.patching().problems.contains(&Problem::Overlap {
        a: id("sector@3"),
        b: id("sector@4"),
        output: OutputName::new("1"),
        port: PortKey(4),
    }));
    assert!(
        j.saved().overrides.contains_key(&id("sector@3")),
        "the copy's override is saved by its id"
    );

    // 3. Copy 3 back to the flow: on port 3 again, nothing overlaps, and
    //    the file holds the flow and no override.
    j.press(
        "back-to-the-flow",
        OfferArgs::new()
            .with("subject", format!("fixture:{}", copy(3)))
            .with("field", "all"),
    );
    assert_eq!(j.first_lamp(&copy(3)), Some((3, 0)));
    assert!(j.patching().problems.is_empty());
    let saved = j.saved();
    assert!(saved.overrides.is_empty());
    assert_eq!(saved.parts[0].flow, Flow::OnePerPort { from: PortKey(0) });

    j.not_offered("turn-on");
    j.choosing_a_port_of_an_unnamed_output_names_it(&format!("fixture:{}", copy(1)));
}

/// Sean's eaves (`lab/mapping/projects/eaves`: the front's 60 lamps on D10,
/// which holds 60, and the side's 50 after it): the side that doesn't fit
/// what is left wraps to the next port; the front reversed, as the house
/// feeds it from its far end.
#[test]
fn seans_eaves() {
    let mut j = Journey::repo("lab/mapping/projects/eaves", "/fixture.map2d.json");

    // 1. The front fills D10; the side doesn't fit, so it starts D9.
    assert_eq!(j.span("front"), Some((0, 0, 59)));
    assert_eq!(j.span("side"), Some((1, 0, 49)));
    let p = j.patching();
    assert!(p.segment(&id("side")).expect("the side").wrapped);
    assert!(!p.segment(&id("front")).expect("the front").wrapped);

    // 2. The front reversed: 59 down to 0; the side where it was.
    j.press(
        "set-direction",
        OfferArgs::new()
            .with("subject", "fixture:front")
            .with("direction", "reverse"),
    );
    assert_eq!(j.span("front"), Some((0, 59, 0)));
    assert_eq!(j.span("side"), Some((1, 0, 49)));
    let saved = j.saved();
    assert_eq!(
        saved.overrides.get(&id("front")),
        Some(&Override {
            direction: Some(Direction::Reverse),
            ..Override::default()
        })
    );
    assert_eq!(saved.overrides.len(), 1);

    j.not_offered("remove-override");
    j.choosing_a_port_of_an_unnamed_output_names_it("fixture:side");
}

/// An override whose node is gone — a copy counted away — is offered for
/// removal (lean Q85, Undoable), named by its id; pressing it takes the
/// override out of the saved file.
#[test]
fn a_gone_overrides_remove_is_an_offer() {
    let mut j = Journey::catalog("catalog/zook-dome", "/fixture.map2d.json");
    j.not_offered("remove-override");
    j.press(
        "set-direction",
        OfferArgs::new()
            .with("subject", "fixture:sector@4")
            .with("direction", "reverse"),
    );
    // Five copies down to four by `set-count`: copy 4 is gone, its override
    // kept, and so the press is Undoable (it takes copy 4 away).
    let offer = actor_clicks(&mut j.graph.actor, &j.graph.tx).offered("project/graph/set-count");
    assert_eq!(offer.action.meta().consequence, ActionConsequence::Routine);
    let args = OfferArgs::new()
        .with("subject", "fixture:repeat1")
        .with("count", "4");
    let action = offer.press(&args).expect("it binds");
    assert_eq!(
        action.meta().consequence,
        ActionConsequence::Undoable,
        "a count that drops an overridden copy takes something away"
    );
    j.press("set-count", args);
    assert!(j.saved().overrides.contains_key(&id("sector@4")), "kept");
    let notice = j.graph.handle.editor().borrow().notice.clone();
    assert_eq!(
        notice.and_then(|n| n.why),
        Some("1 override on repeat1's copies is kept; it shows as a problem".to_string())
    );
    let offer =
        actor_clicks(&mut j.graph.actor, &j.graph.tx).offered("project/graph/remove-override");
    assert_eq!(offer.action.meta().consequence, ActionConsequence::Undoable);
    let value = j.offered_option(
        "remove-override",
        "subject",
        "sector@4, which isn't there any more",
    );
    j.press("remove-override", OfferArgs::new().with("subject", value));
    assert!(j.saved().overrides.is_empty());
    j.not_offered("remove-override");
}

/// Every patching verb is offered at `project/graph/<verb>` with typed
/// parameters, a `subject` first, and at its level (lean Q78): what takes
/// something away is Undoable, the rest Routine, nothing Lasting.
#[test]
fn the_patching_verbs_are_offered_with_their_levels() {
    let mut j = Journey::repo("projects/test/circular-display-6", "/fixture.map2d.json");
    // Turn ring 3 off so Turn on has something to act on.
    j.press(
        "turn-off",
        OfferArgs::new().with("subject", "fixture:circle1/r3"),
    );
    let mut listed = Vec::new();
    for verb in GraphVerb::PATCHING {
        let path = format!("project/graph/{}", verb.name());
        if verb == GraphVerb::RemoveOverride {
            j.not_offered(verb.name());
            continue;
        }
        let offer = actor_clicks(&mut j.graph.actor, &j.graph.tx).offered(&path);
        let params: Vec<String> = offer
            .params()
            .iter()
            .map(|p| {
                let kind = match &p.kind {
                    OfferParamKind::Choice { options, .. } => {
                        format!("choice of {}", options.len())
                    }
                    OfferParamKind::Text { .. } => "text".into(),
                    OfferParamKind::Toggle { .. } => "toggle".into(),
                };
                format!("{} ({kind})", p.name)
            })
            .collect();
        assert_eq!(offer.params()[0].name, "subject", "{path}");
        let consequence = offer.action.meta().consequence.clone();
        let undoable = matches!(
            verb,
            GraphVerb::TurnOff | GraphVerb::BackToTheFlow | GraphVerb::RemoveOverride
        );
        assert_eq!(
            consequence,
            if undoable {
                ActionConsequence::Undoable
            } else {
                ActionConsequence::Routine
            },
            "{path}"
        );
        listed.push(format!("{path} · {consequence:?} · {}", params.join(", ")));
    }
    println!("{}", listed.join("\n"));
}

/// The circular display from nothing (A8's lead journey, lean Q97), on a
/// new board's project as Studio generates it (M3 P5): its strip deleted,
/// a circle drawn with the circle tool, its outer ring set to 24 lamps,
/// rings of 16, 12 and 8 and a centre lamp added, the fixture started on
/// D0 of its output (which has no name, so choosing its port names it
/// `1`), ring 3 reversed and moved to D1 from its first lamp. Every step
/// but the drawing is an offer pressed by its path; the drawing is the
/// pointer input the page reports. It ends where M1's journey does: ring 3
/// runs 11 → 0 on port 1, ring 4 12–19, the centre 20, no problems; and
/// what Studio saved is the golden next to this test.
#[test]
fn the_circular_display_from_nothing() {
    let mut j = Journey::new_board_with_display_output();
    let ring = |k: &str| format!("fixture:circle1/{k}");

    // The starter's strip, deleted by its offer (Undoable).
    let delete = actor_clicks(&mut j.graph.actor, &j.graph.tx).offered("project/graph/delete");
    assert_eq!(
        delete.action.meta().consequence,
        ActionConsequence::Undoable
    );
    j.press("delete", OfferArgs::new().with("subject", "fixture:strip"));
    assert!(j.mapping().parts.is_empty(), "nothing left to map");

    // The box made room for a circle (the offer the inspector's Width and
    // Height rows press).
    j.press(
        "set-fixture-size",
        OfferArgs::new()
            .with("subject", "fixture:")
            .with("width", "100")
            .with("height", "100"),
    );
    // The page lays the canvas out again and says where everything went.
    let placement = {
        let e = j.graph.handle.editor().borrow();
        lab_layout::canvas_layout(&e.layout_input())
    };
    // (Where things sit is not an edit: nothing to send the actor.)
    j.graph.handle.input(GraphInput::Placement(placement));
    j.graph.refresh();

    // The circle tool (an offer), and a circle drawn from the box's middle
    // out 42 (the pointer, as the page reports it).
    j.press("circle-tool", OfferArgs::new());
    j.draw_circle([50.0, 50.0], 42.0);
    let circle = &j.mapping().parts[0];
    assert_eq!(circle.id.as_str(), "circle1");

    // Its ring 24 lamps; rings of 16, 12 and 8 inside it; a centre lamp.
    j.press(
        "set-count",
        OfferArgs::new()
            .with("subject", ring("r1"))
            .with("count", "24"),
    );
    for (count, radius) in [("16", "32"), ("12", "22"), ("8", "12")] {
        j.press(
            "add-ring",
            OfferArgs::new()
                .with("subject", "fixture:circle1")
                .with("count", count)
                .with("radius", radius),
        );
    }
    j.press(
        "add-centre-lamp",
        OfferArgs::new().with("subject", "fixture:circle1"),
    );
    j.not_offered("add-centre-lamp"); // it has one now

    // Started on D0 of its output, which has no name: choosing its port
    // names it `1`, in the same step, and the fixture's start (the
    // starter's, naming `1` already) finds it.
    assert!(
        j.graph
            .handle
            .editor()
            .borrow()
            .project
            .port_tables(&j.fixture)
            .is_empty(),
        "an output with no name has no ports to land on"
    );
    let d0 = j.offered_option(
        "set-fixture-start",
        "port",
        "D0 of output (names it output 1)",
    );
    j.press(
        "set-fixture-start",
        OfferArgs::new()
            .with("subject", "fixture:")
            .with("port", d0)
            .with("lamps", "0"),
    );
    let output: serde_json::Value =
        serde_json::from_str(&j.graph.file_text("/output.json")).expect("json");
    assert_eq!(output["name"], "1", "the output's file carries its name");
    assert_eq!(j.span("circle1/r1"), Some((0, 0, 23)));
    assert_eq!(j.span("circle1/r2"), Some((0, 24, 39)));
    assert_eq!(j.span("circle1/r3"), Some((0, 40, 51)));
    assert_eq!(j.span("circle1/r4"), Some((0, 52, 59)));
    assert_eq!(j.span("circle1/r5"), Some((0, 60, 60)));
    assert!(j.patching().problems.is_empty());

    // Ring 3 reversed, then on D1 from its first lamp.
    j.press(
        "set-direction",
        OfferArgs::new()
            .with("subject", ring("r3"))
            .with("direction", "reverse"),
    );
    let d1 = j.offered_option("set-port", "port", "D1 of output 1");
    j.press(
        "set-port",
        OfferArgs::new()
            .with("subject", ring("r3"))
            .with("port", d1),
    );

    // Where M1's circular-display journey ends.
    assert_eq!(j.span("circle1/r1"), Some((0, 0, 23)));
    assert_eq!(j.span("circle1/r2"), Some((0, 24, 39)));
    assert_eq!(j.span("circle1/r3"), Some((1, 11, 0)));
    assert_eq!(j.span("circle1/r4"), Some((1, 12, 19)));
    assert_eq!(j.span("circle1/r5"), Some((1, 20, 20)));
    let p = j.patching();
    assert!(p.problems.is_empty(), "{:?}", p.problems);
    assert!(
        p.segment(&id("circle1/r3"))
            .expect("r3")
            .direction
            .is_reverse()
    );
    for k in ["r1", "r2", "r4", "r5"] {
        let segment = p.segment(&id(&format!("circle1/{k}"))).expect("a ring");
        assert!(!segment.direction.is_reverse(), "{k} runs forward");
    }

    // What Studio saved: the writer's bytes for the Graph's mapping, with
    // the canvas its resized box gives (the Graph's in-memory canvas is the
    // one it opened with: P05's finding), read back by the engine's
    // reader; and the golden next to this test.
    let text = j.saved_text();
    let read = lpc_mapping::map2d6::read(&text).expect("the engine's reader reads it");
    let mut held = j.mapping();
    held.canvas = Some([0.0, 0.0, 100.0, 100.0]);
    assert_eq!(read, held);
    assert_eq!(text, lpc_mapping::map2d6::write(&held).expect("it writes"));
    let golden = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/app/studio/circular_display_from_nothing.map2d.json"
    );
    if std::env::var_os("LPA_STUDIO_CORE_BLESS").is_some() {
        std::fs::write(golden, &text).expect("the golden writes");
    }
    let expected = std::fs::read_to_string(golden).expect("the golden");
    assert_eq!(text, expected, "the saved map2d is the golden");
}

/// The output the Graph names keeps its name on the board when it is then
/// selected, and Save writes it (docs/defects/2026-10-28-the-graph-names-
/// an-output-the-board-keeps-it-unnamed.md). Selecting an output writes its
/// `highlight` Debug slot; on the board that slot edit replaced the output
/// file's pending body (the name) and clearing it dropped the file's
/// overlay, so Save wrote nothing for it. What Studio showed kept the name,
/// which is why the journey above never saw it: this asks the board. Once
/// saved, the output breathes on the board again when selected; and a body
/// the Graph writes while it breathes (a port's lamps) is kept and saved
/// too.
#[test]
fn a_named_output_keeps_its_name_on_the_board_when_selected_and_saved() {
    let mut j = Journey::new_board_with_display_output();
    let output = NodeId::new("output");
    let select = |j: &mut Journey, targets: Vec<Target>| {
        j.graph.handle.input(GraphInput::Select(targets));
        j.graph.batch();
        j.graph.refresh();
    };
    let field = |text: Option<String>, pointer: &str| -> Option<serde_json::Value> {
        let def: serde_json::Value =
            serde_json::from_str(&text.expect("the output's file")).expect("json");
        def.pointer(pointer).cloned()
    };
    let name = |text: Option<String>| field(text, "/name");
    let one = Some(serde_json::Value::from("1"));
    // Breathing on the board: its overlay holds the output's `highlight`.
    let breathes = |j: &Journey| {
        use lpc_model::project::overlay::ArtifactOverlay;
        let server = j.graph.server.borrow();
        let project = server.project_manager().first_loaded().expect("a project");
        let overlay = project.registry().overlay().get();
        matches!(
            overlay.artifact(&lpc_model::ArtifactLocation::file("/output.json")),
            Some(ArtifactOverlay::Slot { .. })
        )
    };

    // Named by choosing its port.
    let d0 = j.offered_option(
        "set-fixture-start",
        "port",
        "D0 of output (names it output 1)",
    );
    j.press(
        "set-fixture-start",
        OfferArgs::new()
            .with("subject", "fixture:")
            .with("port", d0)
            .with("lamps", "0"),
    );
    assert_eq!(name(Some(j.graph.file_text("/output.json"))), one);
    assert_eq!(
        name(j.graph.board_text("/output.json")),
        one,
        "the board holds the name"
    );

    // The output selected, then nothing, then the output, then nothing.
    for (targets, what) in [
        (vec![Target::Node(output.clone())], "the output selected"),
        (Vec::new(), "nothing selected"),
        (
            vec![Target::Node(output.clone())],
            "the output selected again",
        ),
        (Vec::new(), "nothing selected again"),
    ] {
        select(&mut j, targets);
        assert_eq!(
            name(j.graph.board_text("/output.json")),
            one,
            "{what}, the board still holds the name"
        );
    }

    // Saved: the board's own file carries it.
    actor_clicks(&mut j.graph.actor, &j.graph.tx).press("project/save", OfferArgs::new());
    j.graph.refresh();
    assert_eq!(
        name(j.graph.board_saved_text("/output.json")),
        one,
        "saved, the board's output.json is named"
    );

    // Selected again, it breathes; a port's lamps written while it does
    // are kept when it stops, and saved.
    select(&mut j, vec![Target::Node(output.clone())]);
    assert!(breathes(&j), "saved, the selected output breathes again");
    j.press(
        "set-port-lamps",
        OfferArgs::new()
            .with("subject", port_value(&output, "4"))
            .with("lamps", "50"),
    );
    let fifty = Some(serde_json::Value::from(50));
    assert_eq!(
        field(j.graph.board_text("/output.json"), "/ports/4/count"),
        fifty
    );
    select(&mut j, Vec::new());
    assert_eq!(
        field(j.graph.board_text("/output.json"), "/ports/4/count"),
        fifty,
        "nothing selected, the board still holds the port's lamps"
    );
    actor_clicks(&mut j.graph.actor, &j.graph.tx).press("project/save", OfferArgs::new());
    j.graph.refresh();
    let saved = j.graph.board_saved_text("/output.json");
    assert_eq!(field(saved.clone(), "/ports/4/count"), fifty, "saved");
    assert_eq!(name(saved), one, "and still named");
}

/// The same for a fixture: the Graph writes its file as a body (its
/// colour order), and selecting it writes its `selection` Debug slot,
/// which must not take the colour order off the board.
#[test]
fn a_fixtures_colour_order_stays_on_the_board_when_selected() {
    let mut j = Journey::new_board_with_display_output();
    j.press(
        "set-colour-order",
        OfferArgs::new()
            .with("subject", "fixture:")
            .with("order", "grb"),
    );
    let order = |j: &Journey| -> Option<serde_json::Value> {
        let text = j
            .graph
            .board_text("/fixture.json")
            .expect("the fixture's file");
        let def: serde_json::Value = serde_json::from_str(&text).expect("json");
        def.get("color_order").cloned()
    };
    let grb = Some(serde_json::Value::from("grb"));
    assert_eq!(order(&j), grb, "the board holds the colour order");
    for targets in [vec![Target::Node(j.fixture.clone())], Vec::new()] {
        j.graph.handle.input(GraphInput::Select(targets));
        j.graph.batch();
        j.graph.refresh();
        assert_eq!(order(&j), grb, "selected or not, the board keeps it");
    }
}

/// `projects/test/circular-display-walk`, the project G3's and M5's walks
/// map the circular display in by hand, is exactly what the journey above
/// starts from: a new board's project as Studio generates it, with the
/// display's output. Written with `LPA_STUDIO_CORE_BLESS=1`; its README is
/// its own.
#[test]
fn the_walk_project_is_the_from_nothing_journeys() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../projects/test/circular-display-walk");
    let files = display_walk_files();
    if std::env::var_os("LPA_STUDIO_CORE_BLESS").is_some() {
        for (path, bytes) in &files {
            let to = dir.join(path);
            std::fs::create_dir_all(to.parent().expect("a parent")).expect("the folder");
            std::fs::write(&to, bytes).expect("it writes");
        }
    }
    let mut on_disk = Vec::new();
    collect(&dir, &dir, &mut on_disk);
    on_disk.retain(|(path, _)| path != "README.md");
    let mut want = files;
    want.sort();
    on_disk.sort();
    let names = |f: &[(String, Vec<u8>)]| f.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
    assert_eq!(names(&on_disk), names(&want), "the same files");
    for ((path, have), (_, want)) in on_disk.iter().zip(&want) {
        assert!(have == want, "{path} is what Studio generates");
    }
}

/// Every layout verb is offered at `project/graph/<verb>` with typed
/// parameters, a `subject` first, whenever something can take it (ruling
/// DD37), at its level (lean Q78): delete, unrepeat and remove-ring take
/// something away (Undoable), the rest Routine, nothing Lasting. A count
/// is priced by its press (`a_gone_overrides_remove_is_an_offer`).
#[test]
fn the_layout_verbs_are_offered_with_their_levels() {
    let mut j = Journey::repo("projects/test/circular-display-6", "/fixture.map2d.json");
    // A repeat, so unrepeat and expand have something to take; then the
    // circle has its centre, so add-centre-lamp has nothing.
    j.press(
        "repeat",
        OfferArgs::new()
            .with("subject", "fixture:circle1")
            .with("count", "2"),
    );
    let mut listed = Vec::new();
    for verb in GraphVerb::LAYOUT {
        let path = format!("project/graph/{}", verb.name());
        // Nothing to take them: the circle has its centre, and the
        // display holds no grid or filled polygon.
        if matches!(
            verb,
            GraphVerb::AddCentreLamp | GraphVerb::SetLattice | GraphVerb::SetRouting
        ) {
            j.not_offered(verb.name());
            continue;
        }
        let offer = actor_clicks(&mut j.graph.actor, &j.graph.tx).offered(&path);
        let params: Vec<String> = offer
            .params()
            .iter()
            .map(|p| {
                let kind = match &p.kind {
                    OfferParamKind::Choice { options, .. } => {
                        format!("choice of {}", options.len())
                    }
                    OfferParamKind::Text { .. } => "text".into(),
                    OfferParamKind::Toggle { .. } => "toggle".into(),
                };
                format!("{} ({kind})", p.name)
            })
            .collect();
        assert_eq!(offer.params()[0].name, "subject", "{path}");
        let consequence = offer.action.meta().consequence.clone();
        let undoable = matches!(
            verb,
            GraphVerb::Delete | GraphVerb::Unrepeat | GraphVerb::RemoveRing
        );
        assert_eq!(
            consequence,
            if undoable {
                ActionConsequence::Undoable
            } else {
                ActionConsequence::Routine
            },
            "{path}"
        );
        listed.push(format!("{path} · {consequence:?} · {}", params.join(", ")));
    }
    println!("{}", listed.join("\n"));

    // Rename refuses an id that breaks the rule, and says why.
    let rings = ["r1", "r2", "r3", "r4", "r5"];
    let ids = |part: &str| -> Vec<String> {
        rings
            .iter()
            .flat_map(|k| [format!("{part}/{k}"), format!("{part}/{k}@1")])
            .collect()
    };
    let before: Vec<_> = ids("circle1").iter().map(|x| j.span(x)).collect();
    j.press(
        "rename",
        OfferArgs::new()
            .with("subject", "fixture:circle1")
            .with("id", "Display"),
    );
    let notice = j
        .graph
        .handle
        .editor()
        .borrow()
        .notice
        .clone()
        .expect("a notice");
    assert!(
        notice
            .why
            .as_deref()
            .is_some_and(|why| why.contains("can't be an id")),
        "{notice:?}"
    );
    // Renamed, every override on it follows (D6): every lamp lands where
    // it did, copies included.
    j.press(
        "rename",
        OfferArgs::new()
            .with("subject", "fixture:circle1")
            .with("id", "display"),
    );
    let after: Vec<_> = ids("display").iter().map(|x| j.span(x)).collect();
    assert_eq!(after, before);
    assert_eq!(j.span("display/r3"), Some((1, 11, 0)), "ring 3 of copy 0");
    assert!(j.saved().overrides.contains_key(&id("display/r3")));
}

// ---- the bench ------------------------------------------------------------

/// One journey: the Graph on a running project, its fixture `fixture`, and
/// the mapping file Studio saves it to.
struct Journey {
    graph: Graph,
    fixture: NodeId,
    map: &'static str,
    /// The mapping file as the project holds it before any press.
    original: String,
}

impl Journey {
    /// A catalog project, embedded in Studio.
    fn catalog(example: &str, map: &'static str) -> Self {
        Self::open(Graph::new(example), map)
    }

    /// A project from the repo, by its directory.
    fn repo(dir: &str, map: &'static str) -> Self {
        Self::open(Graph::on_server(repo_server(dir)), map)
    }

    /// A new board's project as Studio generates it (the setup flow's,
    /// `generate_board_project`), its one output given the display's five
    /// ports of 64 lamps (D0 to D4) and no name: the starter's output has
    /// one port and is named `1`, and no verb adds ports.
    fn new_board_with_display_output() -> Self {
        let server = crate::app::studio::studio_face_e2e_tests::files_e2e_server(
            "circular-display-from-nothing",
            &display_walk_files(),
        );
        Self::open(Graph::on_server(server), "/fixture.map2d.json")
    }

    /// Draw a circle with the circle tool: the pointer pressed at `centre`
    /// (the fixture's own coordinates) and dragged out `radius`, as the
    /// page reports it; then let Studio save.
    fn draw_circle(&mut self, centre: [f64; 2], radius: f64) {
        let (from, to) = {
            let e = self.graph.handle.editor().borrow();
            let frame = e.project.frame(&self.fixture);
            let screen = |x: f64, y: f64| e.camera.to_screen(frame.to_world(Vec2::new(x, y)));
            (
                screen(centre[0], centre[1]),
                screen(centre[0] + radius, centre[1]),
            )
        };
        let mods = Mods::default();
        let input = |graph: &mut Graph, i: GraphInput| {
            graph.handle.input(i);
        };
        input(
            &mut self.graph,
            GraphInput::PointerDown {
                at: from,
                button: Button::Primary,
                mods,
            },
        );
        input(&mut self.graph, GraphInput::PointerMove { at: to, mods });
        input(&mut self.graph, GraphInput::PointerUp { at: to, mods });
        self.graph.batch();
        self.graph.refresh();
    }

    /// The fixture's mapping as the Graph holds it.
    fn mapping(&self) -> Mapping {
        let e = self.graph.handle.editor().borrow();
        e.project
            .fixture(&self.fixture)
            .expect("the fixture")
            .mapping
            .clone()
    }

    fn open(mut graph: Graph, map: &'static str) -> Self {
        graph.go_to_graph();
        graph.refresh();
        let original = graph.file_text(map);
        Self {
            graph,
            fixture: NodeId::new("fixture"),
            map,
            original,
        }
    }

    /// Press `project/graph/<verb>` with `args`, as a click or the agent
    /// does, and let Studio save what it changed.
    #[track_caller]
    fn press(&mut self, verb: &str, args: OfferArgs) {
        actor_clicks(&mut self.graph.actor, &self.graph.tx)
            .press(format!("project/graph/{verb}"), args);
        self.graph.refresh();
    }

    #[track_caller]
    fn not_offered(&mut self, verb: &str) {
        actor_clicks(&mut self.graph.actor, &self.graph.tx)
            .not_offered(format!("project/graph/{verb}"));
    }

    /// The value of `verb`'s `param` option labelled `label`.
    #[track_caller]
    fn offered_option(&mut self, verb: &str, param: &str, label: &str) -> String {
        let offer = actor_clicks(&mut self.graph.actor, &self.graph.tx)
            .offered(format!("project/graph/{verb}"));
        let OfferParamKind::Choice { options, .. } = &offer
            .params()
            .iter()
            .find(|p| p.name == param)
            .unwrap_or_else(|| panic!("{verb} takes a {param}"))
            .kind
        else {
            panic!("{param} is a choice");
        };
        options
            .iter()
            .find(|o| o.label == label)
            .unwrap_or_else(|| {
                let labels: Vec<_> = options.iter().map(|o| o.label.as_str()).collect();
                panic!("no {param} labelled {label}: {labels:?}")
            })
            .value
            .clone()
    }

    /// An output with no name, added to the Graph's project (none of the
    /// journeys' projects has one; Studio names every output it makes):
    /// `set-port` lists its port, saying what choosing it names it, and a
    /// press choosing it names it with the lowest free number in the same
    /// step (lean Q96). Saved, the output's file carries the name and the
    /// mapping names it. Saving again writes that output once: its file is
    /// neither written under a second name nor deleted.
    #[track_caller]
    fn choosing_a_port_of_an_unnamed_output_names_it(&mut self, subject: &str) {
        let (spare, name) = {
            let handle = self.graph.handle.clone();
            let mut e = handle.editor().borrow_mut();
            let mut output = Output::new(&[("ws281x:local:D5", 10)]);
            output.label = None;
            let spare = e
                .project
                .add_node(None, "spare", NodeKind::Output(output))
                .expect("a spare output");
            (spare, e.project.next_output_name())
        };
        let value = port_value(&spare, "0");
        let label = self.option_label("set-port", "port", &value);
        assert!(
            label.ends_with(&format!("(names it output {name})")),
            "{label}"
        );
        self.press(
            "set-port",
            OfferArgs::new()
                .with("subject", subject)
                .with("port", value),
        );
        let named = {
            let e = self.graph.handle.editor().borrow();
            e.project.output(&spare).and_then(|o| o.label.clone())
        };
        assert_eq!(named.as_deref(), Some(name.as_str()), "named");
        let file: serde_json::Value =
            serde_json::from_str(&self.graph.file_text("/spare.json")).expect("json");
        assert_eq!(file["name"], name.as_str(), "its file carries the name");
        let node =
            TreeId::parse(subject.rsplit_once(':').expect("<fixture>:<id>").1).expect("an id");
        assert_eq!(
            self.saved()
                .overrides
                .get(&node)
                .and_then(|o| o.output.clone()),
            Some(OutputOp::Set(OutputName::new(&name))),
            "the mapping names it"
        );

        // Saved again (another edit): one output, its file kept.
        self.press(
            "set-starts-at",
            OfferArgs::new().with("subject", subject).with("lamps", "1"),
        );
        assert!(
            self.graph
                .file_text("/spare.json")
                .contains(&format!("\"{name}\"")),
            "its file is still there"
        );
        let module = self.graph.file_text("/module.json");
        assert_eq!(module.matches("spare").count(), 2, "{module}"); // key and ref
        assert!(!module.contains("spare_2"), "{module}");
    }

    /// The label of `verb`'s `param` option whose value is `value`.
    #[track_caller]
    fn option_label(&mut self, verb: &str, param: &str, value: &str) -> String {
        let offer = actor_clicks(&mut self.graph.actor, &self.graph.tx)
            .offered(format!("project/graph/{verb}"));
        let OfferParamKind::Choice { options, .. } = &offer
            .params()
            .iter()
            .find(|p| p.name == param)
            .unwrap_or_else(|| panic!("{verb} takes a {param}"))
            .kind
        else {
            panic!("{param} is a choice");
        };
        options
            .iter()
            .find(|o| o.value == value)
            .unwrap_or_else(|| panic!("no {param} {value}"))
            .label
            .clone()
    }

    /// Where `node`'s lamp 0 lands, as (port, RGB lamp on that port).
    fn first_lamp(&self, node: &str) -> Option<(u32, u32)> {
        self.patching()
            .landing(&id(node), 0)
            .map(|a| (a.port.0, a.channel / 3))
    }

    /// A segment's span: (port, the lamp its first lamp lands on, the lamp
    /// its last lamp lands on), in RGB lamps.
    fn span(&self, node: &str) -> Option<(u32, u32, u32)> {
        let p = self.patching();
        let id = id(node);
        let n = p.segment(&id)?.lamps;
        let first = p.landing(&id, 0)?;
        let last = p.landing(&id, n - 1)?;
        assert_eq!(first.port, last.port);
        Some((first.port.0, first.channel / 3, last.channel / 3))
    }

    /// The Graph's fixture patched by `lpc-mapping`'s patcher against the
    /// port tables Studio built from the outputs' files.
    fn patching(&self) -> Patching {
        let e = self.graph.handle.editor().borrow();
        let mapping = &e
            .project
            .fixture(&self.fixture)
            .expect("the fixture")
            .mapping;
        let tables = e.project.port_tables(&self.fixture);
        assert!(!tables.is_empty(), "Studio handed over port tables");
        patch_mapping(mapping, &render(mapping).expect("it renders"), &tables)
    }

    /// The mapping file as Studio saved it: `lpc-mapping`'s writer's bytes
    /// for the Graph's mapping, which the engine's reader reads back.
    #[track_caller]
    fn saved(&mut self) -> Mapping {
        let text = self.saved_text();
        let e = self.graph.handle.editor().borrow();
        let mapping = &e
            .project
            .fixture(&self.fixture)
            .expect("the fixture")
            .mapping;
        assert_eq!(
            text,
            lpc_mapping::map2d6::write(mapping).expect("it writes"),
            "Studio saved the writer's bytes for the Graph's mapping"
        );
        let read = lpc_mapping::map2d6::read(&text).expect("the engine's reader reads it");
        assert_eq!(&read, mapping);
        read
    }

    fn saved_text(&mut self) -> String {
        self.graph.file_text(self.map)
    }
}

/// A new board's project as Studio generates it, its output given the
/// display's five ports of 64 and no name (`new_board_with_display_output`,
/// and `projects/test/circular-display-walk`).
fn display_walk_files() -> Vec<(String, Vec<u8>)> {
    let mut project =
        crate::app::home::generate_board_project("seeed/xiao-esp32-c6", Some("Circular display"))
            .expect("the board project");
    let output = project
        .files
        .iter_mut()
        .find(|(path, _)| path == "output.json")
        .expect("its output");
    // Edited as text: a node file's keys keep their authored order (`kind`
    // first), which the engine's slot reader needs; serde_json's map would
    // sort them.
    let text = String::from_utf8(output.1.clone()).expect("utf-8");
    let lines: Vec<&str> = text.lines().collect();
    let name = lines
        .iter()
        .position(|l| l.trim_start().starts_with("\"name\""))
        .expect("the starter's output is named");
    let ports = lines
        .iter()
        .position(|l| l.trim_start().starts_with("\"ports\""))
        .expect("its ports");
    let bindings = lines
        .iter()
        .position(|l| l.trim_start().starts_with("\"bindings\""))
        .expect("its bindings");
    let five: Vec<String> = (0..5)
        .map(|k| {
            let comma = if k < 4 { "," } else { "" };
            format!(
                "    \"{k}\": {{\n      \"endpoint\": \"ws281x:local:D{k}\",\n      \"count\": 64\n    }}{comma}"
            )
        })
        .collect();
    let mut out: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == name || (i > ports && i < bindings - 1) {
            continue;
        }
        out.push(line.to_string());
        if i == ports {
            out.extend(five.iter().cloned());
        }
    }
    output.1 = (out.join("\n") + "\n").into_bytes();
    project.files
}

fn id(text: &str) -> TreeId {
    TreeId::parse(text).unwrap_or_else(|| panic!("{text} is an id"))
}

/// A board running the repo project in `dir` (relative to the repo root),
/// as `example_e2e_server` runs a catalog one.
fn repo_server(dir: &str) -> lpa_server::LpServer {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(dir);
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    let name = dir.replace('/', "-");
    crate::app::studio::studio_face_e2e_tests::files_e2e_server(&name, &files)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("an entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(root, &path, out);
        } else {
            let rel = path.strip_prefix(root).expect("under the root");
            out.push((
                rel.to_string_lossy().replace('\\', "/"),
                std::fs::read(&path).expect("readable"),
            ));
        }
    }
}

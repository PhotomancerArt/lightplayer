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
//! engine's reader). Each also shows one choice the Graph offers disabled,
//! with its reason, and one verb it does not offer.

use std::path::{Path, PathBuf};

use lab_mapping_model::{
    GraphInput, GraphVerb, NodeId, NodeKind, Output, PartRef, PropEdit, Target, port_value,
};
use lpc_mapping::part::{Mapping, PartId, TreeId, render};
use lpc_mapping::patching::{
    Amount, ChannelOp, Direction, Flow, OutputName, OutputOp, Override, Patching, PortKey, PortOp,
    Problem, patch_mapping,
};

use crate::app::studio::offer_press_test_api::{OfferPressTestApi, actor_clicks};
use crate::app::studio::studio_graph_e2e_tests::Graph;
use crate::{ActionConsequence, OfferArgError, OfferArgs, OfferParamKind};

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

    // A port of an output with no name is offered, disabled.
    j.a_port_of_an_unnamed_output_is_disabled("fixture:f-lower");
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
    j.a_port_of_an_unnamed_output_is_disabled(&ring("r1"));
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
    j.a_port_of_an_unnamed_output_is_disabled(&format!("fixture:{}", copy(1)));
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
    j.a_port_of_an_unnamed_output_is_disabled("fixture:side");
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
    // Five copies down to four, as the inspector's Copies row does it (a
    // count is a layout verb of P07's, not an offer yet): copy 4 is gone,
    // its override kept.
    let repeat = PartRef::new(&j.fixture, &PartId::new("repeat1").expect("an id"));
    j.graph
        .handle
        .input(GraphInput::Select(vec![Target::Part(repeat)]));
    j.graph.handle.input(GraphInput::MoveDraw(true));
    j.graph.handle.input(GraphInput::EditProp {
        id: "copies",
        edit: PropEdit::SetCount(4),
    });
    // The page's inputs told the actor; it saves.
    j.graph.batch();
    j.graph.refresh();
    assert!(j.saved().overrides.contains_key(&id("sector@4")), "kept");
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
    for verb in GraphVerb::WITH_VALUES {
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
    /// journeys' projects has one): `set-port` lists its port, disabled
    /// until it has a name (Q96), and a press choosing it is refused with
    /// that reason.
    #[track_caller]
    fn a_port_of_an_unnamed_output_is_disabled(&mut self, subject: &str) {
        let spare = {
            let handle = self.graph.handle.clone();
            let mut e = handle.editor().borrow_mut();
            let mut output = Output::new(&[("ws281x:local:D5", 10)]);
            output.label = None;
            e.project
                .add_node(None, "spare", NodeKind::Output(output))
                .expect("a spare output")
        };
        let offer =
            actor_clicks(&mut self.graph.actor, &self.graph.tx).offered("project/graph/set-port");
        let value = port_value(&spare, "0");
        let refused = offer.press(
            &OfferArgs::new()
                .with("subject", subject)
                .with("port", value.clone()),
        );
        assert_eq!(
            refused.err(),
            Some(OfferArgError::OptionDisabled {
                name: "port".to_string(),
                value,
                reason: "this output has no name yet".to_string(),
            })
        );
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

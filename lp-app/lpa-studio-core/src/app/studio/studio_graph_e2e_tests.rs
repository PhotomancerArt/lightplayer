//! The Graph tab on a running project, end to end: the actor opens the
//! project's own files (project format 12, every mapping map2d format 6)
//! on the lab's model while the Graph is the place, the page's handle reads
//! the same editor, and the colours are the board's own published frames;
//! the selection breathes or chases on the board through its `highlight`
//! and `selection` slots (plans `lp2025/2026-10-17-0202-mapping-lab-into-studio`
//! and `lp2025/2026-10-27-0114-mapping-rebuild`). An edit pressed by its
//! offer's path saves map2d format 6 through Studio's asset edits, and an
//! unedited Graph writes nothing (mapping rebuild M4 P05).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use lab_mapping_model::{GraphInput, GraphVerb, NodeId, PartRef, Rgb, Target, UiNodeFace};

use crate::app::graph::{GraphHandle, UiGraphState, graph_verb_path};
use crate::app::studio::offer_press_test_api::{OfferPressTestApi, actor_clicks};
use crate::app::studio::studio_edit_e2e_tests::{InProcessServerIo, drive};
use crate::app::studio::studio_face_e2e_tests::{connect_running_project, example_e2e_server};
use crate::{
    ArtifactLocation, OfferArgs, OfferPath, StudioActor, StudioCommand, StudioController,
    StudioServerClient, UiPage, UiPlace, UiProjectView, UiStudioView,
};

/// The Graph opens a format-6 fixture: the choker's state is open, its
/// fixture's parts are the editor's, and the Graph's verbs are offered
/// under `project/graph`.
#[test]
fn the_graph_opens_a_format_6_fixture() {
    let mut graph = Graph::new("catalog/playful-choker");
    let view = graph.go_to_graph();
    let state = view.graph.as_ref().map(|g| g.state.clone());
    assert!(
        matches!(state, Some(UiGraphState::Open { .. })),
        "the Graph opens the choker: {state:?}"
    );
    let parts: Vec<String> = {
        let e = graph.handle.editor().borrow();
        let fixture = e
            .project
            .fixture(&NodeId::new("fixture"))
            .expect("the choker's fixture");
        fixture
            .mapping
            .parts
            .iter()
            .map(|p| p.id.to_string())
            .collect()
    };
    assert_eq!(
        parts,
        [
            "p", "l1", "a", "a-bar", "y-stem", "y-left", "y-right", "f-lower", "f-bar", "f-upper",
            "u", "l2"
        ]
    );
    let graph_verbs = OfferPath::project().child(crate::app::graph::GRAPH_GROUP);
    let offered: Vec<String> = view
        .offers
        .verbs_of(&graph_verbs)
        .map(|offer| offer.path.to_string())
        .collect();
    assert!(
        !offered.is_empty(),
        "the Graph's verbs are offered under project/graph"
    );
}

/// The choker opens on the Graph as the lab opens it: its five nodes,
/// twelve paths and 73 lamps, laid out, its shader feeding its fixture and
/// its fixture its output. Until the Graph is the place nothing is read.
#[test]
fn the_choker_opens_on_the_graph() {
    let mut graph = Graph::new("catalog/playful-choker");
    assert!(
        graph.view().graph.is_none(),
        "nothing until the Graph is asked for"
    );
    let view = graph.go_to_graph();
    let state = view.graph.as_ref().map(|g| g.state.clone());
    assert!(
        matches!(state, Some(UiGraphState::Open { .. })),
        "the Graph opens the choker: {state:?}"
    );
    let e = graph.handle.editor().borrow();
    assert_eq!(e.project.name, "PLAYFUL Choker");
    assert_eq!(e.project.lamp_count(), 73);
    let names: Vec<&str> = e.project.nodes().iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["clock", "fixture", "output", "power", "shader"]);
    let fixture = NodeId::new("fixture");
    assert_eq!(e.project.segments_in(&fixture).len(), 12, "twelve paths");
    // Laid out: every node has its place on the canvas.
    let nodes = e.view_nodes();
    assert_eq!(nodes.len(), 5);
    assert!(
        nodes
            .iter()
            .all(|n| e.project.placement().rects.contains_key(&n.id))
    );
    let edges = e.view_edges();
    let channels: Vec<&str> = edges.iter().map(|x| x.channel.as_str()).collect();
    assert!(channels.contains(&"visual.out"), "{channels:?}");
    assert!(channels.contains(&"control.out"), "{channels:?}");
}

/// The colours are the board's: once it has published a frame, the
/// fixture's lamps take the colours of the pixels they land on, and the
/// output's port shows the board's pixels — never a stand-in pattern.
#[test]
fn the_graph_shows_the_boards_own_colours() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let view = graph.refresh();
    let g = view.graph.expect("the Graph's slice");
    assert_eq!(g.state, UiGraphState::Open { live: true });
    let frame = &g.frame.0;
    let output = NodeId::new("output");
    let pixels = frame.port(&output, "0").expect("the output's port");
    assert_eq!(pixels.len(), 73);
    assert!(
        pixels.iter().any(|px| !px.is_black()),
        "the shader lights the choker"
    );
    let e = graph.handle.editor().borrow();
    let (_, first) = &e.project.placed_segments()[0];
    assert!(
        frame.lamp(&first.id, 0).is_some(),
        "a lamp takes its pixel's colour"
    );
}

/// The page's input reaches core's editor at once, through the handle,
/// and a selection made there is the editor's.
#[test]
fn the_pages_input_is_cores_editor() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let fixture = Target::Node(NodeId::new("fixture"));
    assert!(
        graph
            .handle
            .input(GraphInput::Select(vec![fixture.clone()]))
    );
    assert_eq!(graph.handle.editor().borrow().selection, vec![fixture]);
    let nodes = graph.handle.editor().borrow().view_nodes();
    let face = nodes
        .iter()
        .find(|n| n.id == NodeId::new("fixture"))
        .and_then(|n| n.face.clone());
    assert!(
        matches!(face, Some(UiNodeFace::Fixture(f)) if f.look == lab_mapping_model::FixtureLook::Editing),
        "a selected fixture is its editing view"
    );
}

/// A catalog pattern's grid and its ring of eight rings open whole on the
/// Graph: every lamp the mapping renders, nothing held back.
#[test]
fn a_patterns_grid_and_rings_open_whole() {
    let mut graph = Graph::new("catalog/plasma");
    let view = graph.go_to_graph();
    assert!(matches!(
        view.graph.map(|g| g.state),
        Some(UiGraphState::Open { .. })
    ));
    let e = graph.handle.editor().borrow();
    let face = fixture_face(&e);
    assert_eq!(face.not_shown, None);
    assert_eq!(e.project.lamp_count(), 241);
}

/// A fixture mapped as a points part opens whole: the headband's two
/// fibres are its two lamps, drawn where the mapping puts them, and
/// nothing is held back.
#[test]
fn a_points_fixture_opens_whole() {
    let mut graph = Graph::new("catalog/fiber-headband");
    let view = graph.go_to_graph();
    assert!(matches!(
        view.graph.map(|g| g.state),
        Some(UiGraphState::Open { .. })
    ));
    let e = graph.handle.editor().borrow();
    let face = fixture_face(&e);
    assert_eq!(face.not_shown, None);
    assert_eq!(e.project.lamp_count(), 2);
    let fixture = NodeId::new("fixture");
    let segments = e.project.segments_in(&fixture);
    assert_eq!(segments.len(), 1, "one points part, one segment");
    assert_eq!(segments[0].id.to_string(), "points1");
}

/// A selected segment chases on the board: its first lamp's pixel is the
/// chase's blue head, its last the red tail — read back from the board's
/// own frame. Deselecting stops it.
#[test]
fn a_selected_segment_chases_on_the_board() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let first = graph.first_part();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Part(first)]));
    graph.batch();
    let (head, tail) = graph.first_segment_pixels();
    let view = graph.refresh();
    let frame = view.graph.expect("the Graph").frame.0;
    let pixels = frame.port(&NodeId::new("output"), "0").unwrap();
    assert_eq!(pixels[head], Rgb::new(0, 0, 255), "the chase's blue head");
    assert_eq!(pixels[tail], Rgb::new(255, 0, 0), "the chase's red tail");

    graph.handle.input(GraphInput::Select(Vec::new()));
    graph.batch();
    let view = graph.refresh();
    let frame = view.graph.expect("the Graph").frame.0;
    let pixels = frame.port(&NodeId::new("output"), "0").unwrap();
    assert_ne!(
        (pixels[head], pixels[tail]),
        (Rgb::new(0, 0, 255), Rgb::new(255, 0, 0)),
        "deselected, the board shows its pattern again"
    );
}

/// A selected port breathes on the board, every pixel of it one grey.
#[test]
fn a_selected_port_breathes_on_the_board() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    graph.handle.input(GraphInput::Select(vec![Target::Port {
        output: NodeId::new("output"),
        key: "0".to_string(),
    }]));
    graph.batch();
    let view = graph.refresh();
    let frame = view.graph.expect("the Graph").frame.0;
    let pixels = frame.port(&NodeId::new("output"), "0").unwrap();
    assert!(
        pixels.iter().all(|p| p.r == p.g && p.g == p.b),
        "{pixels:?}"
    );
}

/// Leaving the Graph stops the board's highlight, as leaving Patch did.
#[test]
fn leaving_the_graph_stops_the_highlight() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let first = graph.first_part();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Part(first)]));
    graph.batch();
    let (head, tail) = graph.first_segment_pixels();
    graph
        .tx
        .send(StudioCommand::Place(UiPlace::new(UiPage::Project {
            uid: "prj".to_string(),
            view: UiProjectView::Nodes,
        })));
    graph.batch();
    // Away from the Graph, the board's frame no longer chases (the Graph
    // still holds its selection, for when it comes back).
    let view = graph.refresh();
    let frame = view.graph.expect("the Graph's slice").frame.0;
    let pixels = frame.port(&NodeId::new("output"), "0").unwrap();
    assert_ne!(pixels[head], Rgb::new(0, 0, 255));
    assert_ne!(pixels[tail], Rgb::new(255, 0, 0));
    assert!(!graph.handle.editor().borrow().selection.is_empty());
}

/// Over a device link the Graph asks for a shader's picture only when the
/// selection does: nothing selected, no picture; the shader selected, its
/// live picture; a path in the fixture it feeds selected, the same
/// picture, which the fixture's face draws behind its lamps.
#[test]
fn a_shaders_picture_follows_the_selection_over_a_device_link() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let shader = NodeId::new("shader");
    let picture = |graph: &mut Graph| {
        let view = graph.refresh();
        view.graph
            .expect("the Graph's slice")
            .frame
            .0
            .picture(&shader)
            .map(|p| (p.width, p.height, p.rgb.iter().any(|b| *b != 0)))
    };
    assert_eq!(
        picture(&mut graph),
        None,
        "nothing selected, nothing asked for"
    );

    graph
        .handle
        .input(GraphInput::Select(vec![Target::Node(shader.clone())]));
    graph.batch();
    let (w, h, lit) = picture(&mut graph).expect("the selected shader's picture");
    assert!(w > 0 && h > 0 && lit, "{w}×{h}, lit: {lit}");

    let first = graph.first_part();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Part(first)]));
    graph.batch();
    assert!(
        picture(&mut graph).is_some(),
        "a path's fixture samples the shader"
    );
    let e = graph.handle.editor().borrow();
    let face = fixture_face(&e);
    assert_eq!(face.samples, Some(shader));
    assert_eq!(face.render_size, Some((104, 28)));
}

/// With the board's own fixture rules (the `selection` slot): a selected
/// fixture chases EVERY segment in it, each from its own blue head to its
/// own red tail — what the outputs' single-segment chase could never say —
/// and its lamps reach the wire that way.
#[test]
fn a_selected_fixture_chases_every_segment_in_it() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Node(NodeId::new(
            "fixture",
        ))]));
    graph.batch();
    let view = graph.refresh();
    let frame = view.graph.expect("the Graph's slice").frame.0;
    let pixels = frame.port(&NodeId::new("output"), "0").unwrap();
    // The choker runs end to end in part order: wire lamp = fixture lamp.
    let e = graph.handle.editor().borrow();
    let mut start = 0usize;
    for (_, o) in e.project.placed_segments() {
        let n = o.lamps.len();
        assert_eq!(pixels[start], Rgb::new(0, 0, 255), "{} starts blue", o.id);
        assert_eq!(
            pixels[start + n - 1],
            Rgb::new(255, 0, 0),
            "{} ends red",
            o.id
        );
        start += n;
    }
}

/// Opening the Graph and selecting writes nothing: no file of the project
/// becomes an edit, and the mapping is the file's own bytes.
#[test]
fn the_graph_writes_nothing_when_nothing_is_edited() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let first = graph.first_part();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Part(first)]));
    graph.batch();
    graph.refresh();
    let project = graph.actor.controller_mut_for_test().project_for_test();
    for path in [MAP, "/fixture.json", "/output.json", "/module.json"] {
        let content = project.asset_content_cached(&ArtifactLocation::file(path));
        assert!(
            content.as_ref().is_none_or(|c| !c.dirty),
            "{path} is not an edit"
        );
    }
    assert_eq!(graph.mapping_text(), choker_map(), "the file's own bytes");
}

/// An edit pressed on the Graph by its offer's path saves through Studio's
/// own asset edit: the choker's mapping is the project's pending edit, in
/// map2d format 6 exactly as `lpc-mapping`'s writer writes the edited
/// mapping, which the engine's reader reads back with the lamp the edit
/// added. Only the mapping is written.
#[test]
fn an_edit_on_the_graph_saves_through_the_asset_path() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let first = graph.first_part();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Part(first.clone())]));
    graph.batch();
    graph.press(GraphVerb::More);
    graph.refresh();
    assert_eq!(graph.handle.editor().borrow().project.lamp_count(), 74);
    let project = graph.actor.controller_mut_for_test().project_for_test();
    let content = project
        .asset_content_cached(&ArtifactLocation::file(MAP))
        .expect("the mapping is Studio's to show");
    assert!(content.dirty, "the edit is the project's pending edit");
    for path in ["/fixture.json", "/output.json", "/module.json"] {
        let content = project.asset_content_cached(&ArtifactLocation::file(path));
        assert!(
            content.as_ref().is_none_or(|c| !c.dirty),
            "{path} is not written"
        );
    }
    let text = graph.mapping_text();
    let mapping = lpc_mapping::map2d6::read(&text).expect("the engine's reader reads it");
    let editor_mapping = graph.fixture_mapping();
    assert_eq!(mapping, editor_mapping, "the Graph's mapping, all of it");
    assert_eq!(
        text,
        lpc_mapping::map2d6::write(&editor_mapping).unwrap(),
        "the writer's canonical bytes"
    );
    let tree = lpc_mapping::part::render(&mapping).expect("it renders");
    assert_eq!(tree.positions().len(), 74);
    // Saved once: the next reads write nothing more, and the Graph keeps
    // its editor (its own edit is no change made elsewhere).
    graph.refresh();
    assert_eq!(graph.handle.editor().borrow().project.lamp_count(), 74);
    assert!(graph.handle.editor().borrow().can_undo());
    assert_eq!(graph.mapping_text(), text);
}

/// Undo on the Graph is the Graph's own, and what it puts back is saved
/// the same way: the mapping is the file's own bytes again.
#[test]
fn undo_on_the_graph_saves_what_it_puts_back() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let first = graph.first_part();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Part(first)]));
    graph.batch();
    graph.press(GraphVerb::More);
    graph.refresh();
    assert_ne!(graph.mapping_text(), choker_map());
    graph.press(GraphVerb::Undo);
    graph.refresh();
    assert_eq!(graph.handle.editor().borrow().project.lamp_count(), 73);
    assert_eq!(graph.mapping_text(), choker_map(), "the file's own bytes");
}

/// A port's count edited on the Graph re-patches as soon as it is saved:
/// the output's file is written, and the port tables are built again from
/// it, so the dome's first copy no longer fits its port — without opening
/// the project again.
#[test]
fn a_port_count_edited_on_the_graph_re_patches_once_saved() {
    let mut graph = Graph::new("catalog/zook-dome");
    graph.go_to_graph();
    let fixture = NodeId::new("fixture");
    let capacity = |graph: &Graph| {
        graph.handle.editor().borrow().project.port_tables(&fixture)[0].ports[0].channels
    };
    assert_eq!(capacity(&graph), Some(900), "300 lamps of three channels");
    graph.handle.input(GraphInput::Select(vec![Target::Port {
        output: NodeId::new("output"),
        key: "0".to_string(),
    }]));
    graph.batch();
    graph.press(GraphVerb::Fewer);
    graph.refresh();
    let output = graph
        .actor
        .controller_mut_for_test()
        .project_for_test()
        .asset_content_cached(&ArtifactLocation::file("/output.json"))
        .and_then(|c| c.text().map(str::to_string))
        .expect("the output");
    assert!(output.contains("\"count\": 299"), "{output}");
    assert_eq!(capacity(&graph), Some(897), "the tables follow the file");
    assert!(
        !graph
            .handle
            .editor()
            .borrow()
            .project
            .effective_patch()
            .problems
            .is_empty(),
        "copy 0's 300 lamps no longer fit port 0"
    );
}

/// Deleting a node on the Graph deletes its files through Studio's asset
/// path in the same step (ruling DD36): the module stops naming it and its
/// file reads as deleted, so Save leaves nothing on disk that nothing
/// names. Undo on the Graph writes them back: the node's file byte for
/// byte, the module naming it again.
#[test]
fn deleting_a_node_deletes_its_files_and_undo_writes_them_back() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    graph.refresh();
    let repo = |file: &str| {
        let path = format!(
            "{}/../../catalog/projects/playful-choker/{file}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).expect("the choker's file")
    };
    let (module, power) = (repo("module.json"), repo("power.json"));
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Node(NodeId::new("power"))]));
    graph.batch();
    let delete =
        actor_clicks(&mut graph.actor, &graph.tx).offered(graph_verb_path(GraphVerb::Delete));
    assert_eq!(
        delete.action.meta().consequence,
        crate::ActionConsequence::Undoable
    );
    graph.press(GraphVerb::Delete);
    graph.refresh();
    let content = |graph: &mut Graph, path: &str| {
        graph
            .actor
            .controller_mut_for_test()
            .project_for_test()
            .asset_content_cached(&ArtifactLocation::file(path))
            .expect("Studio holds it")
    };
    let gone = content(&mut graph, "/power.json");
    assert_eq!(gone.body, crate::UiAssetContentBody::Deleted, "deleted");
    assert!(gone.dirty, "a pending edit, for Save");
    assert!(
        !graph.file_text("/module.json").contains("power"),
        "the module no longer names it"
    );

    graph.press(GraphVerb::Undo);
    graph.refresh();
    assert_eq!(graph.file_text("/power.json"), power, "its own bytes again");
    // The module names it again. (Its other keys come back in the files
    // crate's sorted order, as any module the Graph writes does.)
    let json = |text: &str| serde_json::from_str::<serde_json::Value>(text).expect("json");
    assert_eq!(json(&graph.file_text("/module.json")), json(&module));
}

/// The choker's mapping file in the repo.
const MAP: &str = "/playful.map2d.json";

fn choker_map() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../catalog/projects/playful-choker/playful.map2d.json"
    );
    std::fs::read_to_string(path).expect("the choker's mapping")
}

/// The fixture node's face, as the canvas draws it.
fn fixture_face(e: &lab_mapping_model::Editor) -> lab_mapping_model::UiFixtureFace {
    let face = e
        .view_nodes()
        .into_iter()
        .find(|n| n.id == NodeId::new("fixture"))
        .and_then(|n| n.face);
    let Some(UiNodeFace::Fixture(face)) = face else {
        panic!("the fixture's face");
    };
    face
}

/// A studio actor on a running project, and the Graph's handle (also the
/// journeys' bench, `studio_graph_journeys_tests.rs`).
pub(super) struct Graph {
    pub(super) actor: StudioActor<fn(core::time::Duration) -> core::future::Ready<()>>,
    pub(super) tx: crate::app::studio::studio_view_channel::CommandSender,
    view: crate::app::studio::studio_view_channel::StudioViewReceiver,
    pub(super) handle: GraphHandle,
    last: Option<UiStudioView>,
}

impl Graph {
    /// On a running catalog project.
    pub(super) fn new(example: &str) -> Self {
        let example = crate::app::home::embedded_example(example).expect("embedded");
        Self::on_server(example_e2e_server(&example))
    }

    /// On whatever project `server` runs.
    pub(super) fn on_server(server: lpa_server::LpServer) -> Self {
        let server = Rc::new(RefCell::new(server));
        let io = InProcessServerIo {
            server,
            inbox: Rc::new(RefCell::new(VecDeque::new())),
            sent: Rc::new(RefCell::new(Vec::new())),
        };
        let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
        let controller = StudioController::connected_with_client_for_test(client);
        let handle = controller.graph_handle();
        let timer: fn(core::time::Duration) -> core::future::Ready<()> =
            |_| core::future::ready(());
        let (actor, studio) = StudioActor::new(controller, timer);
        let mut graph = Self {
            actor,
            tx: studio.tx,
            view: studio.view,
            handle,
            last: None,
        };
        connect_running_project(&graph.tx);
        graph.batch();
        graph.refresh();
        graph
    }

    pub(super) fn batch(&mut self) {
        drive(self.actor.run_one_batch_for_test());
        if let Some(next) = self.view.try_recv() {
            self.last = Some(next);
        }
    }

    pub(super) fn refresh(&mut self) -> UiStudioView {
        // The lens reads the project and the board's frames, as the page's
        // refresh timer has it do.
        for _ in 0..4 {
            drive(
                self.actor
                    .controller_mut_for_test()
                    .refresh_project_for_test(),
            );
        }
        let view = self.actor.controller_mut_for_test().view();
        self.last = Some(view.clone());
        view
    }

    fn view(&self) -> UiStudioView {
        self.last.clone().expect("a snapshot")
    }

    /// Press the Graph's verb by its offer's path, as a click does.
    fn press(&mut self, verb: GraphVerb) {
        actor_clicks(&mut self.actor, &self.tx).press(graph_verb_path(verb), OfferArgs::default());
        if let Some(next) = self.view.try_recv() {
            self.last = Some(next);
        }
    }

    /// The choker's mapping as Studio holds it now.
    fn mapping_text(&mut self) -> String {
        self.file_text(MAP)
    }

    /// A project file as Studio holds it now (an edit included).
    pub(super) fn file_text(&mut self, path: &str) -> String {
        self.actor
            .controller_mut_for_test()
            .project_for_test()
            .asset_content_cached(&ArtifactLocation::file(path))
            .and_then(|c| c.text().map(str::to_string))
            .unwrap_or_else(|| panic!("{path}"))
    }

    /// The fixture's mapping, as the Graph holds it.
    fn fixture_mapping(&self) -> lpc_mapping::part::Mapping {
        self.handle
            .editor()
            .borrow()
            .project
            .fixture(&NodeId::new("fixture"))
            .expect("the fixture")
            .mapping
            .clone()
    }

    /// The choker's first drawn part.
    fn first_part(&self) -> PartRef {
        let fixture = NodeId::new("fixture");
        let e = self.handle.editor().borrow();
        let part = &e
            .project
            .fixture(&fixture)
            .expect("the choker's fixture")
            .mapping
            .parts[0];
        PartRef::new(&fixture, &part.id)
    }

    /// The pixels the choker's first segment's first and last lamps land
    /// on: its one port takes every lamp in order (end to end), so lamp `i`
    /// of the first segment is pixel `i`.
    fn first_segment_pixels(&self) -> (usize, usize) {
        let e = self.handle.editor().borrow();
        let (_, o) = &e.project.placed_segments()[0];
        (0, o.lamps.len() - 1)
    }

    /// Say the page is the Graph, and let the actor open it.
    pub(super) fn go_to_graph(&mut self) -> UiStudioView {
        self.tx
            .send(StudioCommand::Place(UiPlace::new(UiPage::Project {
                uid: "prj".to_string(),
                view: UiProjectView::Graph,
            })));
        self.batch();
        self.view()
    }
}

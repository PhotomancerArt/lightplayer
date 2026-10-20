//! The Graph tab on a running project, end to end: the actor opens the
//! project's own files on the lab's model while the Graph is the place,
//! the page's handle reads the same editor, and the colours are the
//! board's own published frames; an edit pressed by its offer's path saves
//! through Studio's asset edits, and the selection breathes or chases on
//! the board through its `highlight` slots (plan
//! `lp2025/2026-10-17-0202-mapping-lab-into-studio`, M1–M3).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use lab_mapping_model::{GraphInput, GraphVerb, NodeId, Rgb, Target, UiNodeFace};

use crate::app::graph::{GraphHandle, UiGraphState, graph_verb_path};
use crate::app::studio::offer_press_test_api::{OfferPressTestApi, actor_clicks};
use crate::app::studio::studio_edit_e2e_tests::{InProcessServerIo, drive};
use crate::app::studio::studio_face_e2e_tests::{connect_running_project, example_e2e_server};
use crate::{
    ArtifactLocation, OfferArgs, StudioActor, StudioCommand, StudioController, StudioServerClient,
    UiPage, UiPlace, UiProjectView, UiStudioView,
};

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
    let (_, first) = &e.project.placed_objects()[0];
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
/// Graph (plan M4): every lamp the product resolves, nothing held back.
#[test]
fn a_patterns_grid_and_rings_open_whole() {
    let mut graph = Graph::new("catalog/plasma");
    let view = graph.go_to_graph();
    assert!(matches!(
        view.graph.map(|g| g.state),
        Some(UiGraphState::Open { .. })
    ));
    let e = graph.handle.editor().borrow();
    let face = e
        .view_nodes()
        .into_iter()
        .find(|n| n.id == NodeId::new("fixture"))
        .and_then(|n| n.face);
    let Some(UiNodeFace::Fixture(face)) = face else {
        panic!("the fixture's face");
    };
    assert_eq!(face.not_shown, None);
    assert_eq!(e.project.lamp_count(), 241);
}

/// A fixture mapped with the product's older point lists opens as its box,
/// saying where to edit it, and the project opens round it: the Graph
/// refuses the fixture, never the project (plan M4).
#[test]
fn a_point_list_fixture_opens_as_its_box() {
    let mut graph = Graph::new("catalog/fiber-headband");
    let view = graph.go_to_graph();
    assert!(matches!(
        view.graph.map(|g| g.state),
        Some(UiGraphState::Open { .. })
    ));
    let e = graph.handle.editor().borrow();
    let face = e
        .view_nodes()
        .into_iter()
        .find(|n| n.id == NodeId::new("fixture"))
        .and_then(|n| n.face);
    let Some(UiNodeFace::Fixture(face)) = face else {
        panic!("the fixture's face");
    };
    assert_eq!(
        face.not_shown.as_deref(),
        Some("The Graph can't show its point-list mapping yet. Edit it in Nodes.")
    );
}

/// An edit pressed on the Graph by its offer's path saves through Studio's
/// own asset edit: the choker's mapping is the project's pending edit, in
/// today's format with an id on every object, and the product's own
/// resolver reads it back with the lamp the edit added.
#[test]
fn an_edit_on_the_graph_saves_through_the_asset_path() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let first = graph.first_component();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Component(first)]));
    graph.batch();
    graph.press(GraphVerb::More);
    graph.refresh();
    assert_eq!(graph.handle.editor().borrow().project.lamp_count(), 74);
    let content = graph
        .actor
        .controller_mut_for_test()
        .project_for_test()
        .asset_content_cached(&ArtifactLocation::file("/playful.map2d.json"))
        .expect("the mapping is Studio's to show");
    assert!(content.dirty, "the edit is the project's pending edit");
    let text = content.text().expect("text").to_string();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["format"], 3, "the lowest format that holds ids");
    let objects = doc["objects"].as_array().unwrap();
    assert!(objects.iter().all(|o| o.get("id").is_some()));
    let product = lpc_mapping::Map2dDoc::from_json(&text).expect("the product reads it");
    let resolved = lpc_mapping::resolve(&product).expect("and resolves it");
    assert_eq!(resolved.lamps.len(), 74);
    // Saved once: the next reads write nothing more, and the Graph keeps
    // its editor (its own edit is no change made elsewhere).
    graph.refresh();
    assert_eq!(graph.handle.editor().borrow().project.lamp_count(), 74);
    assert!(graph.handle.editor().borrow().can_undo());
}

/// Undo on the Graph is the Graph's own, and what it puts back is saved
/// the same way: the mapping is as it was opened.
#[test]
fn undo_on_the_graph_saves_what_it_puts_back() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let first = graph.first_component();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Component(first)]));
    graph.press(GraphVerb::More);
    graph.refresh();
    graph.press(GraphVerb::Undo);
    graph.refresh();
    let text = graph
        .actor
        .controller_mut_for_test()
        .project_for_test()
        .asset_content_cached(&ArtifactLocation::file("/playful.map2d.json"))
        .and_then(|c| c.text().map(str::to_string))
        .expect("the mapping");
    let product = lpc_mapping::Map2dDoc::from_json(&text).expect("the product reads it");
    assert_eq!(lpc_mapping::resolve(&product).unwrap().lamps.len(), 73);
}

/// A selected object chases on the board: its first lamp's pixel is the
/// chase's blue head, its last the red tail — read back from the board's
/// own frame. Deselecting stops it.
#[test]
fn a_selected_object_chases_on_the_board() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let first = graph.first_component();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Component(first)]));
    graph.batch();
    let (head, tail) = graph.first_object_pixels();
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

/// Leaving the Graph stops the board's highlight, as leaving Patch does.
#[test]
fn leaving_the_graph_stops_the_highlight() {
    let mut graph = Graph::new("catalog/playful-choker");
    graph.go_to_graph();
    let first = graph.first_component();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Component(first)]));
    graph.batch();
    let (head, tail) = graph.first_object_pixels();
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
/// selection does (plan D11): nothing selected, no picture; the shader
/// selected, its live picture; a path in the fixture it feeds selected,
/// the same picture, which the fixture's face draws behind its lamps.
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

    let first = graph.first_component();
    graph
        .handle
        .input(GraphInput::Select(vec![Target::Component(first)]));
    graph.batch();
    assert!(
        picture(&mut graph).is_some(),
        "a path's fixture samples the shader"
    );
    let e = graph.handle.editor().borrow();
    let face = e
        .view_nodes()
        .into_iter()
        .find(|n| n.id == NodeId::new("fixture"))
        .and_then(|n| n.face);
    let Some(UiNodeFace::Fixture(face)) = face else {
        panic!("the fixture's face");
    };
    assert_eq!(face.samples, Some(shader));
    assert_eq!(face.render_size, Some((104, 28)));
}

/// With the board's own fixture rules (plan M4b, the `selection` slot):
/// a selected fixture chases EVERY object in it, each from its own blue
/// head to its own red tail — what the outputs' single-object chase could
/// never say — and its lamps reach the wire that way.
#[test]
fn a_selected_fixture_chases_every_object_in_it() {
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
    // The choker auto-flows in object order: wire lamp = fixture lamp.
    let e = graph.handle.editor().borrow();
    let mut start = 0usize;
    for (_, o) in e.project.placed_objects() {
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

/// A studio actor on a running catalog project, and the Graph's handle.
struct Graph {
    actor: StudioActor<fn(core::time::Duration) -> core::future::Ready<()>>,
    tx: crate::app::studio::studio_view_channel::CommandSender,
    view: crate::app::studio::studio_view_channel::StudioViewReceiver,
    handle: GraphHandle,
    last: Option<UiStudioView>,
}

impl Graph {
    fn new(example: &str) -> Self {
        let example = crate::app::home::embedded_example(example).expect("embedded");
        let server = Rc::new(RefCell::new(example_e2e_server(&example)));
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

    fn batch(&mut self) {
        drive(self.actor.run_one_batch_for_test());
        if let Some(next) = self.view.try_recv() {
            self.last = Some(next);
        }
    }

    fn refresh(&mut self) -> UiStudioView {
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

    /// The choker's first drawn component.
    fn first_component(&self) -> lab_mapping_model::ComponentId {
        self.handle
            .editor()
            .borrow()
            .project
            .fixture(&NodeId::new("fixture"))
            .unwrap()
            .components[0]
            .id
            .clone()
    }

    /// The pixels the choker's first object's first and last lamps land
    /// on: its one port takes every lamp in order (auto), so lamp `i` of the
    /// first object is pixel `i`.
    fn first_object_pixels(&self) -> (usize, usize) {
        let e = self.handle.editor().borrow();
        let (_, o) = &e.project.placed_objects()[0];
        (0, o.lamps.len() - 1)
    }

    /// Say the page is the Graph, and let the actor open it.
    fn go_to_graph(&mut self) -> UiStudioView {
        self.tx
            .send(StudioCommand::Place(UiPlace::new(UiPage::Project {
                uid: "prj".to_string(),
                view: UiProjectView::Graph,
            })));
        self.batch();
        self.view()
    }
}

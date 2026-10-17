//! The Graph tab on a running project, end to end: the actor opens the
//! project's own files on the lab's model while the Graph is the place,
//! the page's handle reads the same editor, and the colours are the
//! board's own published frames (plan
//! `lp2025/2026-10-17-0202-mapping-lab-into-studio`, M1).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use lab_mapping_model::{GraphInput, NodeId, Target, UiNodeFace};

use crate::app::graph::{GraphHandle, UiGraphState};
use crate::app::studio::studio_edit_e2e_tests::{InProcessServerIo, drive, project_action};
use crate::app::studio::studio_face_e2e_tests::example_e2e_server;
use crate::{
    ProjectOp, StudioActor, StudioCommand, StudioController, StudioServerClient, UiPage, UiPlace,
    UiProjectView, UiStudioView,
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

/// A catalog pattern's grid fixture opens as its box on the Graph, saying
/// why, and the rest of the project opens around it.
#[test]
fn a_grid_fixture_opens_as_its_box() {
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
    assert_eq!(
        face.not_shown.as_deref(),
        Some("This fixture's grid isn't on the Graph yet — use Map")
    );
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
        graph
            .tx
            .send(project_action(ProjectOp::ConnectRunningProject));
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
        for _ in 0..4 {
            self.tx.send(project_action(ProjectOp::RefreshProject));
            self.batch();
        }
        self.view()
    }

    fn view(&self) -> UiStudioView {
        self.last.clone().expect("a snapshot")
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

//! The Graph tab on a running project, end to end, between project format
//! 12 and the new Graph (mapping rebuild M3 P6): every repo project's
//! mapping is map2d format 6, which the lab's reader behind today's Graph
//! does not read, so the Graph says plainly that the new editor is coming
//! and offers nothing that saves. M4's Graph replaces this file's journeys
//! (plan `lp2025/2026-10-27-0114-mapping-rebuild`).

use std::cell::RefCell;
use std::collections::VecDeque;

use std::rc::Rc;

use crate::app::graph::{NEW_MAPPING_FORMAT_SENTENCE, UiGraphState};
use crate::app::studio::studio_edit_e2e_tests::{InProcessServerIo, drive};
use crate::app::studio::studio_face_e2e_tests::{connect_running_project, example_e2e_server};
use crate::{
    OfferPath, StudioActor, StudioCommand, StudioController, StudioServerClient, UiPage, UiPlace,
    UiProjectView, UiStudioView,
};

/// The Graph on a project whose fixture's mapping is map2d format 6 says
/// the new editor is coming, in one plain sentence, and offers nothing that
/// saves (no verb under `project/graph`).
#[test]
fn the_graph_on_a_format_6_fixture_says_the_new_editor_is_coming() {
    let mut graph = Graph::new("catalog/playful-choker");
    assert!(
        graph.view().graph.is_none(),
        "nothing until the Graph is asked for"
    );
    let view = graph.go_to_graph();
    let state = view.graph.as_ref().map(|g| g.state.clone());
    assert_eq!(
        state,
        Some(UiGraphState::NewMappingFormat {
            sentence: NEW_MAPPING_FORMAT_SENTENCE.to_string()
        })
    );
    assert_eq!(
        NEW_MAPPING_FORMAT_SENTENCE,
        "This fixture uses the new mapping format. Its editor is coming soon."
    );
    let graph_verbs = OfferPath::project().child(crate::app::graph::GRAPH_GROUP);
    let offered: Vec<String> = view
        .offers
        .verbs_of(&graph_verbs)
        .map(|offer| offer.path.to_string())
        .collect();
    assert!(
        offered.is_empty(),
        "nothing on the Graph saves: {offered:?}"
    );
}

/// A studio actor on a running catalog project.
struct Graph {
    actor: StudioActor<fn(core::time::Duration) -> core::future::Ready<()>>,
    tx: crate::app::studio::studio_view_channel::CommandSender,
    view: crate::app::studio::studio_view_channel::StudioViewReceiver,
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
        let timer: fn(core::time::Duration) -> core::future::Ready<()> =
            |_| core::future::ready(());
        let (actor, studio) = StudioActor::new(controller, timer);
        let mut graph = Self {
            actor,
            tx: studio.tx,
            view: studio.view,
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

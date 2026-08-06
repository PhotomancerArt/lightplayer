//! Interactive-docs P1 e2e rows: the docs-sim bootstrap
//! ([`ProjectOp::OpenDocsExample`]) and the [`DocsSimHost`] lease
//! lifecycle, driven against the in-process real server.
//!
//! What these rows pin:
//! - the docs deploy goes straight to the runtime (`deploy_project_files`
//!   into a `docs-…` storage dir) and needs **no library** — the library
//!   open path would have errored on these controllers;
//! - re-dispatch is the docs "reset": same op, same sim session, a
//!   pristine re-deploy;
//! - `DocsSimHost::shutdown` is a complete, ordered teardown the actor
//!   finishes on its own (StopSimulator empties the pool, Shutdown ends
//!   the loop);
//! - a `drain_logs: false` actor never steals the global log sink's
//!   records from the main actor (the docs-host log-theft hazard).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Wake, Waker};

use log::Log as _;
use lpc_wire::ClientMessage;

use super::studio_actor::poll_now;
use super::studio_edit_e2e_tests::{InProcessServerIo, drive, edit_e2e_server};
use crate::{
    ControllerId, DocsSimHost, ProjectOp, StudioActor, StudioActorOptions, StudioCommand,
    StudioController, StudioServerClient, UiAction,
};

/// The example the rows deploy: the demo project, whose files are known
/// to load on the host `LpServer` (the storeless demo path is the host
/// harness's own).
const DOCS_EXAMPLE: &str = crate::STUDIO_DEMO_PROJECT_ID;

fn docs_open_action(example_id: &str) -> UiAction {
    UiAction::from_op(
        ControllerId::new(crate::ProjectController::NODE_ID),
        ProjectOp::OpenDocsExample {
            example_id: example_id.to_string(),
        },
    )
}

/// A controller with a stub sim whose client talks to an in-process real
/// server — the host stand-in for a connected browser-worker sim. NO
/// library is attached, deliberately: the docs path must not need one.
fn docs_studio_with_stub_sim() -> (StudioController, Rc<RefCell<Vec<ClientMessage>>>) {
    let server = Rc::new(RefCell::new(edit_e2e_server()));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let io = InProcessServerIo {
        server,
        inbox: Rc::new(RefCell::new(VecDeque::new())),
        sent: Rc::clone(&sent),
    };
    let mut studio = StudioController::new(|| 1.0);
    studio.install_stub_sim_with_client_for_test(StudioServerClient::from_io_for_test(
        "in-process",
        Box::new(io),
    ));
    (studio, sent)
}

/// The storage ids of every deploy's `LoadProject` request — the deploy
/// path is per-file `Filesystem(Write…)` messages followed by one
/// `LoadProject { path: "projects/<storage_id>" }`.
fn sent_deploy_storage_ids(sent: &Rc<RefCell<Vec<ClientMessage>>>) -> Vec<String> {
    sent.borrow()
        .iter()
        .filter_map(|message| {
            let debug = format!("{message:?}");
            if !debug.contains("LoadProject") {
                return None;
            }
            debug.find("projects/").map(|at| {
                debug[at + "projects/".len()..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '-')
                    .collect()
            })
        })
        .collect()
}

/// The docs bootstrap deploys directly — no library attached, no library
/// needed, files land in a `docs-…` storage dir, the mirror opens.
#[test]
fn open_docs_example_deploys_directly_without_a_library() {
    let (mut studio, sent) = docs_studio_with_stub_sim();

    let notices = drive(studio.dispatch(docs_open_action(DOCS_EXAMPLE)))
        .expect("the docs open succeeds with no library");
    assert!(
        notices
            .notices
            .iter()
            .any(|notice| notice.message.contains("Example running")),
        "the op reports the running example, got {:?}",
        notices.notices
    );

    let ids = sent_deploy_storage_ids(&sent);
    assert!(
        ids.iter().any(|id| id == "docs-fyeah-sign"),
        "the deploy targets the docs storage dir, saw {ids:?}"
    );
    let view = studio.view();
    assert!(
        !view.panes.is_empty(),
        "the editor mirror opened on the deployed example"
    );
    assert!(
        view.open_project_uid.is_none(),
        "a direct deploy carries no library identity"
    );
}

/// Re-dispatch is the docs reset: same sim session, second pristine
/// deploy, no session churn.
#[test]
fn re_dispatching_open_docs_example_resets_on_the_live_sim() {
    let (mut studio, sent) = docs_studio_with_stub_sim();

    drive(studio.dispatch(docs_open_action(DOCS_EXAMPLE))).expect("first open");
    let sim_id = studio
        .runtime_pool_for_test()
        .sim_session()
        .expect("a sim session")
        .id();
    drive(studio.dispatch(docs_open_action(DOCS_EXAMPLE))).expect("reset open");

    let pool = studio.runtime_pool_for_test();
    let sim = pool.sim_session().expect("still exactly one sim session");
    assert_eq!(sim.id(), sim_id, "the reset reuses the running session");
    assert!(
        sent_deploy_storage_ids(&sent).len() >= 2,
        "the reset re-deploys the files"
    );
}

/// Boot → first batch deploys and publishes a view → `shutdown()` alone
/// completes the actor (StopSimulator then Shutdown, one enqueue): the
/// lease's whole lifecycle without the page driving anything after the
/// shutdown call.
#[test]
fn docs_sim_host_boots_deploys_and_shutdown_completes_the_actor() {
    let (studio, sent) = docs_studio_with_stub_sim();

    let spawned: Rc<RefCell<Option<std::pin::Pin<Box<dyn Future<Output = ()>>>>>> =
        Rc::new(RefCell::new(None));
    let slot = Rc::clone(&spawned);
    let mut host = DocsSimHost::boot(
        DOCS_EXAMPLE,
        studio,
        move |future| *slot.borrow_mut() = Some(future),
        |_| std::future::ready(()),
    );
    let mut view_rx = host.take_view().expect("the view stream is takeable once");
    assert!(host.take_view().is_none(), "and only once");

    let mut actor = spawned.borrow_mut().take().expect("boot spawned the actor");

    // Drive the actor through the bootstrap batch (all inner awaits are
    // immediate on the in-process harness, so a bounded poll loop is a
    // scheduler).
    let waker = noop_waker();
    let mut context = Context::from_waker(&waker);
    for _ in 0..1_000 {
        if actor.as_mut().poll(&mut context).is_ready() {
            panic!("the actor must not stop before shutdown");
        }
        if !sent_deploy_storage_ids(&sent).is_empty() {
            break;
        }
    }
    assert!(
        sent_deploy_storage_ids(&sent)
            .iter()
            .any(|id| id == "docs-fyeah-sign"),
        "the bootstrap deployed the example"
    );
    let mut latest = None;
    while let Some(Some(view)) = poll_now(view_rx.recv()) {
        latest = Some(view);
    }
    let view = latest.expect("the actor published a change-gated view");
    assert!(!view.panes.is_empty(), "the published view carries the mirror");

    host.shutdown();
    host.shutdown(); // idempotent

    let mut completed = false;
    for _ in 0..1_000 {
        if actor.as_mut().poll(&mut context).is_ready() {
            completed = true;
            break;
        }
    }
    assert!(completed, "shutdown alone ends the actor loop");
}

/// The log-theft hazard, pinned: a `drain_logs: false` actor leaves the
/// global sink's pending records for the main actor.
#[test]
fn docs_actor_never_drains_the_global_log_sink() {
    // Queue a record on this thread's pending queue through the real
    // `Log::log` entry point, as the macros would.
    crate::STUDIO_LOG_SINK.log(
        &log::Record::builder()
            .level(log::Level::Info)
            .target("docs_e2e")
            .args(format_args!("docs isolation probe"))
            .build(),
    );

    let docs_controller = StudioController::new(|| 1.0);
    let (mut docs_actor, docs_handle) = StudioActor::new_with_options(
        docs_controller,
        |_| std::future::ready(()),
        StudioActorOptions { drain_logs: false },
    );
    docs_handle.tx.send(StudioCommand::RefreshTick);
    drive(docs_actor.run_one_batch_for_test());
    assert!(
        !docs_actor
            .controller_mut_for_test()
            .logs()
            .iter()
            .any(|entry| entry.message.contains("docs isolation probe")),
        "the docs actor must not steal sink records"
    );

    let main_controller = StudioController::new(|| 1.0);
    let (mut main_actor, main_handle) =
        StudioActor::new(main_controller, |_| std::future::ready(()));
    main_handle.tx.send(StudioCommand::RefreshTick);
    drive(main_actor.run_one_batch_for_test());
    assert!(
        main_actor
            .controller_mut_for_test()
            .logs()
            .iter()
            .any(|entry| entry.message.contains("docs isolation probe")),
        "the record is still there for the main actor"
    );
}

fn noop_waker() -> Waker {
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
    Waker::from(Arc::new(NoopWake))
}

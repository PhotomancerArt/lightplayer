//! End-to-end agent-flow tests: a SCRIPTED fake `ModelProvider` drives the
//! real path — actor command queue → `AgentController` → `lpa_agent`
//! session loop → `iterate` tool (real compile + probes on `lps-probe`) →
//! host bridge → `AssetEditOp::ApplyBody` overlay mutation against an
//! in-process `LpServer`. Only the model is fake.
//!
//! Reuses the edit-e2e harness (`asset_e2e_server`, `InProcessServerIo`,
//! `drive`) from `studio_edit_e2e_tests`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use lpa_agent::provider::model_provider::{
    BoxStream, ModelProvider, StopReason, TokenUsage, TurnEvent, TurnRequest,
};
use serde_json::json;

use crate::app::studio::studio_edit_e2e_tests::{
    ASSET_SHADER_V1, InProcessServerIo, asset_e2e_server, drive, editor_dirty, find_asset_editor,
    project_action,
};
use crate::{
    AgentTaskFuture, ProjectOp, SettingsCommand, StudioActor, StudioCommand, StudioController,
    StudioServerClient, UiAgentAvailability, UiAgentStatus, UiAgentTurn, UiAgentView, UiStudioView,
};

/// The staged edit. `layout(binding = N)` is required by the probe
/// dialect (naga `glsl-in`), exactly as the agent's system prompt says.
const GREEN_SHADER: &str = "layout(binding = 0) uniform float time;\n\nvec4 render(vec2 pos) {\n    return vec4(0.0, 1.0, 0.0, 1.0);\n}\n";

#[test]
fn agent_turn_stages_an_overlay_edit_end_to_end() {
    // Script: turn 1 stages the green shader through `iterate`; turn 2
    // wraps up with text. The provider streams the tool input in two
    // fragments like the real SSE path.
    let input = json!({ "source": GREEN_SHADER, "note": "go green" }).to_string();
    let mid = input.len() / 2;
    let scripts = vec![vec![
        vec![
            TurnEvent::TextDelta("Trying green.".into()),
            TurnEvent::ToolUseStart {
                id: "tu_1".into(),
                name: "iterate".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_1".into(),
                json_fragment: input[..mid].into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_1".into(),
                json_fragment: input[mid..].into(),
            },
            turn_done(StopReason::ToolUse, 20, 30),
        ],
        vec![
            TurnEvent::TextDelta("The lights are green now.".into()),
            turn_done(StopReason::EndTurn, 40, 10),
        ],
    ]];
    let harness = AgentHarness::new(scripts);
    let (mut actor, handle) = StudioActor::new(harness.controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");

    // The GLSL editor is decorated with an empty, Ready agent chat.
    let agent = agent_view(&snapshot);
    assert_eq!(agent.availability, UiAgentAvailability::Ready);
    assert_eq!(agent.status, UiAgentStatus::Idle);
    assert!(agent.turns.is_empty());
    // The shader card's FACE carries the same chat (node-card P3: the
    // agent section relocated onto the face decorates from the same
    // session state).
    let face_agent = face_agent_view(&snapshot);
    assert_eq!(face_agent.availability, UiAgentAvailability::Ready);
    assert!(face_agent.turns.is_empty());

    // Send. The dispatch resolves context, fetches the source body, and
    // spawns exactly one run future.
    handle
        .tx
        .send(StudioCommand::Action(agent.send_action("make it green")));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("send emits a snapshot");
    let agent = agent_view(&snapshot);
    assert!(agent.busy(), "run started: {:?}", agent.status);
    assert!(matches!(
        agent.turns.first(),
        Some(UiAgentTurn::User { text }) if text == "make it green"
    ));
    assert_eq!(harness.tasks.borrow().len(), 1, "one spawned run future");

    // Drive the run to completion (the scripted provider is synchronous):
    // it streams both turns, executes the tool (real compile + staging
    // through the command queue), and parks the session runtime.
    let run = harness.tasks.borrow_mut().remove(0);
    drive(run);

    // The queued feedback + the staged ApplyBody action land in one batch.
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("agent batch emits a snapshot");

    // Transcript: user → assistant → tool row (done, staged, compiled) →
    // assistant; status back to Idle; usage totalled across both turns.
    let agent = agent_view(&snapshot);
    assert_eq!(agent.status, UiAgentStatus::Idle);
    assert_eq!(agent.turns.len(), 4, "turns: {:?}", agent.turns);
    assert!(matches!(
        &agent.turns[1],
        UiAgentTurn::Assistant { text } if text == "Trying green."
    ));
    let UiAgentTurn::Tool(row) = &agent.turns[2] else {
        panic!("expected a tool row, got {:?}", agent.turns[2]);
    };
    assert!(row.done);
    assert!(row.staged);
    assert_eq!(row.note.as_deref(), Some("go green"));
    assert_eq!(row.shader_ok, Some(true));
    assert!(matches!(
        &agent.turns[3],
        UiAgentTurn::Assistant { text } if text == "The lights are green now."
    ));
    assert_eq!(
        (agent.usage.input_tokens, agent.usage.output_tokens),
        (60, 40)
    );
    // The face's chat sees the SAME transcript — one session, two hosts.
    assert_eq!(face_agent_view(&snapshot).turns.len(), agent.turns.len());

    // The staged edit went through the ONE overlay write path: the editor
    // content shadows to the agent's source, dirty, and the save panel
    // counts it as a persisted-class pending edit.
    let editor = find_asset_editor(&snapshot);
    let content = editor.content.as_ref().expect("resolved content");
    assert!(content.dirty, "agent edit is overlay-dirty");
    assert_eq!(content.text(), Some(GREEN_SHADER));
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    // The system prompts saw the real context: current source (turn 1 =
    // saved body, turn 2 = the staged edit), node identity, and bindings.
    let requests = harness.requests.borrow();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].system.contains(ASSET_SHADER_V1));
    assert!(requests[0].system.contains("Shader node: Shader"));
    assert!(requests[0].system.contains("`time` (float)"));
    assert!(
        requests[1].system.contains(GREEN_SHADER),
        "turn 2 sees the staged source"
    );
}

/// Stages a shader that compiles fine in the probe world but declares a
/// uniform (`speed`) the node def has no record for — the engine then fails
/// at RENDER time ("missing uniform field"), the exact class the probe
/// harness cannot see.
const SPEED_SHADER: &str = "layout(binding = 0) uniform float time;\nlayout(binding = 1) uniform float speed;\n\nvec4 render(vec2 pos) {\n    return vec4(fract(time * speed), 0.0, 0.0, 1.0);\n}\n";

#[test]
fn staged_engine_render_error_reaches_the_iterate_result() {
    let input = json!({ "source": SPEED_SHADER, "note": "add a speed control" }).to_string();
    let scripts = vec![vec![
        vec![
            TurnEvent::ToolUseStart {
                id: "tu_1".into(),
                name: "iterate".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_1".into(),
                json_fragment: input,
            },
            turn_done(StopReason::ToolUse, 20, 30),
        ],
        vec![
            TurnEvent::TextDelta("I need a def record for speed.".into()),
            turn_done(StopReason::EndTurn, 40, 10),
        ],
    ]];
    let harness = AgentHarness::new(scripts);
    // Yield-once timers: every engine-verdict poll (and every tool-stage
    // yield) suspends the run exactly once, handing control back to the
    // test so it can pump actor batches between polls — the interleaving a
    // live browser gets from real timers.
    let (mut actor, handle) = StudioActor::new(harness.controller, |_| {
        let mut yielded = false;
        core::future::poll_fn(move |cx| {
            if yielded {
                core::task::Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                core::task::Poll::Pending
            }
        })
    });
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let agent = agent_view(&snapshot);

    handle
        .tx
        .send(StudioCommand::Action(agent.send_action("faster please")));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();

    // Interleave: poll the run future; while it suspends (stage yields and
    // verdict-wait timer steps), pump one actor batch with a forced project
    // refresh — the pull that carries the engine's post-apply status back.
    let mut run = harness.tasks.borrow_mut().remove(0);
    let mut live_phases: Vec<String> = Vec::new();
    {
        use core::task::{Context, Poll};
        use std::sync::Arc;
        use std::task::{Wake, Waker};
        struct NoopWake;
        impl Wake for NoopWake {
            fn wake(self: Arc<Self>) {}
        }
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut pumps = 0;
        loop {
            match run.as_mut().poll(&mut context) {
                Poll::Ready(()) => break,
                Poll::Pending => {
                    pumps += 1;
                    assert!(pumps < 200, "agent run did not complete");
                    handle.tx.send(project_action(ProjectOp::RefreshProject));
                    drive(actor.run_one_batch_for_test());
                    if let Some(snapshot) = view.try_recv() {
                        // Record the running row's live activity as the UI
                        // would render it mid-run.
                        if let Some(UiAgentTurn::Tool(row)) = agent_view(&snapshot)
                            .turns
                            .iter()
                            .find(|turn| matches!(turn, UiAgentTurn::Tool(_)))
                            && !row.done
                        {
                            live_phases.push(row.summary_line());
                        }
                    }
                }
            }
        }
    }
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("final agent batch emits a snapshot");

    // The running row showed live phase text (note + activity) mid-run.
    assert!(
        live_phases
            .iter()
            .any(|line| line.starts_with("add a speed control — ")),
        "live phases: {live_phases:?}"
    );

    // The tool result the MODEL sees (turn 2's request) carries the engine
    // section with the render error — message prefix-stripped, no
    // "shader render: render:" chain.
    let requests = harness.requests.borrow();
    assert_eq!(requests.len(), 2);
    let tool_result = requests[1]
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .find_map(|block| match block {
            lpa_agent::ContentBlock::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        })
        .expect("turn 2 carries the tool result");
    let content: serde_json::Value = serde_json::from_str(&tool_result).expect("result json");
    assert_eq!(content["shader"], "ok", "probe-world compile succeeds");
    assert_eq!(content["engine"]["status"], "error", "{content}");
    let message = content["engine"]["message"].as_str().expect("message");
    assert!(
        message.contains("missing uniform field"),
        "engine message: {message}"
    );
    assert!(
        !message.contains("shader render:") && !message.starts_with("render:"),
        "prefix chain must be stripped: {message}"
    );

    // The finished row keeps the engine outcome in its summary detail.
    let agent = agent_view(&snapshot);
    let UiAgentTurn::Tool(row) = &agent.turns[1] else {
        panic!("expected a tool row, got {:?}", agent.turns);
    };
    assert!(row.done);
    assert!(row.detail.contains("\"engine\""), "{}", row.detail);
}

#[test]
fn provider_error_surfaces_sanely_and_a_retry_recovers() {
    // First run: the provider fails like a bad API key (401, not
    // retryable). Second run: a clean text turn succeeds — same session.
    let scripts = vec![
        vec![vec![TurnEvent::ProviderError {
            message: "401 authentication_error: invalid x-api-key".into(),
            retryable: false,
        }]],
        vec![vec![
            TurnEvent::TextDelta("Back on track.".into()),
            turn_done(StopReason::EndTurn, 5, 5),
        ]],
    ];
    let harness = AgentHarness::new(scripts);
    let (mut actor, handle) = StudioActor::new(harness.controller, |_| core::future::ready(()));
    let mut view = handle.view;

    handle
        .tx
        .send(project_action(ProjectOp::ConnectRunningProject));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("connect emits a snapshot");
    let agent = agent_view(&snapshot);

    handle
        .tx
        .send(StudioCommand::Action(agent.send_action("hello")));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    let run = harness.tasks.borrow_mut().remove(0);
    drive(run);
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("error batch emits a snapshot");
    let agent = agent_view(&snapshot);

    // The failure is surfaced (status + transcript notice), nothing
    // crashed, and the chat is not stuck busy.
    assert_eq!(
        agent.status,
        UiAgentStatus::Error {
            message: "401 authentication_error: invalid x-api-key".into(),
            retryable: false,
        }
    );
    assert!(!agent.busy(), "retry must be possible");
    assert!(matches!(
        agent.turns.last(),
        Some(UiAgentTurn::Notice { text }) if text.contains("401")
    ));

    // Retry with the next scripted provider: the run completes and the
    // transcript keeps the whole history.
    handle
        .tx
        .send(StudioCommand::Action(agent.send_action("try again")));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    let run = harness.tasks.borrow_mut().remove(0);
    drive(run);
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("retry batch emits a snapshot");
    let agent = agent_view(&snapshot);
    assert_eq!(agent.status, UiAgentStatus::Idle);
    assert!(matches!(
        agent.turns.last(),
        Some(UiAgentTurn::Assistant { text }) if text == "Back on track."
    ));
}

// -- harness ---------------------------------------------------------------

/// A connected controller with scripted providers, a task-collecting
/// spawner, and a user-layer API key installed.
struct AgentHarness {
    controller: StudioController,
    /// Futures handed to the injected spawner (drive them manually).
    tasks: Rc<RefCell<Vec<AgentTaskFuture>>>,
    /// Every `TurnRequest` any scripted provider received.
    requests: Rc<RefCell<Vec<TurnRequest>>>,
}

impl AgentHarness {
    /// `scripts[n]` is the turn script of the provider built for run `n`
    /// (the factory is called once per Send).
    fn new(scripts: Vec<Vec<Vec<TurnEvent>>>) -> Self {
        let server = Rc::new(RefCell::new(asset_e2e_server()));
        let io = InProcessServerIo {
            server,
            inbox: Rc::new(RefCell::new(VecDeque::new())),
            sent: Rc::new(RefCell::new(Vec::new())),
        };
        let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
        let mut controller = StudioController::connected_with_client_for_test(client);
        controller.apply_settings_command(SettingsCommand::SetAgentAnthropicApiKey(Some(
            "sk-test-fake".to_string(),
        )));

        let tasks: Rc<RefCell<Vec<AgentTaskFuture>>> = Rc::new(RefCell::new(Vec::new()));
        let spawner_tasks = Rc::clone(&tasks);
        controller.set_agent_spawner(move |future| spawner_tasks.borrow_mut().push(future));

        let requests: Rc<RefCell<Vec<TurnRequest>>> = Rc::new(RefCell::new(Vec::new()));
        let factory_requests = Rc::clone(&requests);
        let remaining = RefCell::new(VecDeque::from(scripts));
        controller.set_agent_provider_factory(move |config| {
            let crate::AgentProviderConfig::Anthropic(config) = config else {
                panic!("expected the anthropic config for the default provider");
            };
            assert_eq!(config.api_key, "sk-test-fake");
            let turns = remaining.borrow_mut().pop_front().expect("script left");
            Box::new(ScriptedProvider {
                turns: RefCell::new(turns.into()),
                requests: Rc::clone(&factory_requests),
            })
        });

        Self {
            controller,
            tasks,
            requests,
        }
    }
}

/// One-turn-script provider (the `lpa-agent` FakeProvider pattern, shared
/// request log so assertions survive the factory rebuilds).
struct ScriptedProvider {
    turns: RefCell<VecDeque<Vec<TurnEvent>>>,
    requests: Rc<RefCell<Vec<TurnRequest>>>,
}

impl ModelProvider for ScriptedProvider {
    fn run_turn(&self, req: TurnRequest) -> BoxStream<'_, TurnEvent> {
        self.requests.borrow_mut().push(req);
        let events = self.turns.borrow_mut().pop_front().unwrap_or_default();
        Box::pin(futures_util::stream::iter(events))
    }
}

fn turn_done(stop_reason: StopReason, input: u32, output: u32) -> TurnEvent {
    TurnEvent::TurnDone {
        stop_reason,
        usage: TokenUsage {
            input_tokens: input,
            output_tokens: output,
            ..TokenUsage::default()
        },
    }
}

/// The decorated agent chat DTO on the shader node's inline editor.
fn agent_view(view: &UiStudioView) -> UiAgentView {
    find_asset_editor(view)
        .agent
        .expect("GLSL editor carries the agent chat DTO")
}

/// The decorated agent chat DTO on the shader node's card face (node-card
/// P3: the face's agent section).
fn face_agent_view(view: &UiStudioView) -> UiAgentView {
    let editor = view
        .panes
        .iter()
        .find_map(|pane| match &pane.body {
            crate::UiViewContent::ProjectEditor(editor) => Some(editor),
            _ => None,
        })
        .expect("project editor pane");
    editor
        .nodes
        .iter()
        .find_map(|node| match &node.face {
            Some(crate::UiNodeFace::Shader(face)) => face.agent.clone(),
            _ => None,
        })
        .expect("shader face carries the agent chat DTO")
}

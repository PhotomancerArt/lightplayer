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
    project_action, workspace_cards,
};
use crate::{
    AgentTaskFuture, ProjectOp, SettingsCommand, StudioActor, StudioCommand, StudioController,
    StudioServerClient, UiAgentAvailability, UiAgentStatus, UiAgentTurn, UiAgentView, UiStudioView,
};

/// The staged edit. `layout(binding = N)` is required by the probe
/// dialect (naga `glsl-in`), exactly as the agent's system prompt says.
const GREEN_SHADER: &str = "layout(binding = 0) uniform float time;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(0.0, 1.0, 0.0, 1.0);\n}\n";

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

/// A second staged edit for the history/revert flow.
const BLUE_SHADER: &str = "layout(binding = 0) uniform float time;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(0.0, 0.0, 1.0, 1.0);\n}\n";

/// P4: two staged edits build the session history; reverting to the first
/// restages its source through the real overlay path — editor content and
/// dirty state show v1 again, the transcript says so, and the history
/// records themselves stay intact.
#[test]
fn history_revert_restages_an_earlier_edit_end_to_end() {
    let stage = |id: &str, source: &str, note: &str| {
        vec![
            TurnEvent::ToolUseStart {
                id: id.into(),
                name: "iterate".into(),
            },
            TurnEvent::ToolInputDelta {
                id: id.into(),
                json_fragment: json!({ "source": source, "note": note }).to_string(),
            },
            turn_done(StopReason::ToolUse, 10, 10),
        ]
    };
    let scripts = vec![
        vec![
            stage("tu_1", GREEN_SHADER, "go green"),
            stage("tu_2", BLUE_SHADER, "try blue"),
            vec![
                TurnEvent::TextDelta("Blue it is.".into()),
                turn_done(StopReason::EndTurn, 10, 10),
            ],
        ],
        // Run 2, after the revert: a text-only turn whose system prompt
        // must carry the reverted source.
        vec![vec![
            TurnEvent::TextDelta("Yes — green.".into()),
            turn_done(StopReason::EndTurn, 10, 10),
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
        .send(StudioCommand::Action(agent.send_action("green then blue")));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    let run = harness.tasks.borrow_mut().remove(0);
    drive(run);
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("agent batch emits a snapshot");

    // Both staged calls became history entries, in order; the editor holds
    // the LAST staged source.
    let agent = agent_view(&snapshot);
    assert_eq!(agent.history.len(), 2, "history: {:?}", agent.history);
    assert_eq!(agent.history[0].turn, 1);
    assert_eq!(agent.history[0].note.as_deref(), Some("go green"));
    assert_eq!(agent.history[1].turn, 2);
    assert_eq!(agent.history[1].note.as_deref(), Some("try blue"));
    assert_eq!(agent.history_dropped, 0);
    let editor = find_asset_editor(&snapshot);
    assert_eq!(
        editor.content.as_ref().expect("content").text(),
        Some(BLUE_SHADER)
    );

    // Revert to turn 1 through the DTO's action builder (the filmstrip's
    // click): the recorded v1 source restages through ApplyBody.
    handle
        .tx
        .send(StudioCommand::Action(agent.revert_action(1)));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("revert emits a snapshot");

    let editor = find_asset_editor(&snapshot);
    let content = editor.content.as_ref().expect("resolved content");
    assert!(content.dirty, "revert stages, never saves");
    assert_eq!(content.text(), Some(GREEN_SHADER));
    assert_eq!(editor_dirty(&snapshot), (1, 0));

    // The transcript tells the user (and the next run's context reflects
    // the overlay anyway); history keeps both records for further hops.
    let agent = agent_view(&snapshot);
    assert!(
        matches!(
            agent.turns.last(),
            Some(UiAgentTurn::Notice { text, .. }) if text.contains("Reverted to turn 1")
        ),
        "turns: {:?}",
        agent.turns
    );
    assert_eq!(agent.history.len(), 2);

    // The NEXT run's current_source reflects the revert (the run-start
    // refresh reads the overlay): its system prompt carries v1, not v2.
    handle
        .tx
        .send(StudioCommand::Action(agent.send_action("still green?")));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    let run = harness.tasks.borrow_mut().remove(0);
    drive(run);
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();
    let requests = harness.requests.borrow();
    assert_eq!(requests.len(), 4);
    assert!(
        requests[3].system.contains(GREEN_SHADER),
        "the follow-up run sees the reverted source"
    );
    assert!(
        !requests[3].system.contains(BLUE_SHADER),
        "v2 is no longer the working source"
    );
}

/// Stages a shader that compiles fine in the probe world but declares a
/// uniform (`speed`) the node def has no record for — the engine then fails
/// at RENDER time ("missing uniform field"), the exact class the probe
/// harness cannot see.
const SPEED_SHADER: &str = "layout(binding = 0) uniform float time;\nlayout(binding = 1) uniform float speed;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(fract(time * speed), 0.0, 0.0, 1.0);\n}\n";

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

/// The full detect→report→repair loop on the REAL in-process server, all in
/// the staged (Save-less) overlay state: stage a shader declaring a uniform
/// with no def record → `iterate` reports the `declared_only` orphan AND the
/// engine's render error → `upsert_param` dispatches the `PutSlotEdit` batch
/// → the def change recompiles the node → a follow-up `iterate` shows params
/// clean and the engine ok — and the panel knob appears on the shader face
/// through the existing derivation, no UI change involved.
#[test]
fn declared_orphan_is_repaired_by_upsert_param_end_to_end() {
    let iterate_input =
        json!({ "source": SPEED_SHADER, "note": "add a speed control" }).to_string();
    let upsert_input = json!({
        "name": "speed", "label": "Speed", "default": 1.0,
        "min": 0.0, "max": 4.0,
    })
    .to_string();
    let verify_input = json!({ "note": "verify params" }).to_string();
    let scripts = vec![vec![
        vec![
            TurnEvent::ToolUseStart {
                id: "tu_1".into(),
                name: "iterate".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_1".into(),
                json_fragment: iterate_input,
            },
            turn_done(StopReason::ToolUse, 10, 10),
        ],
        vec![
            TurnEvent::ToolUseStart {
                id: "tu_2".into(),
                name: "upsert_param".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_2".into(),
                json_fragment: upsert_input,
            },
            turn_done(StopReason::ToolUse, 10, 10),
        ],
        vec![
            TurnEvent::ToolUseStart {
                id: "tu_3".into(),
                name: "iterate".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_3".into(),
                json_fragment: verify_input,
            },
            turn_done(StopReason::ToolUse, 10, 10),
        ],
        vec![
            TurnEvent::TextDelta("Speed knob ready.".into()),
            turn_done(StopReason::EndTurn, 10, 10),
        ],
    ]];
    let harness = AgentHarness::new(scripts);
    // Yield-once timers, exactly like the render-error e2e: every bridge
    // poll (upsert ack + engine verdict) hands control back to the test so
    // it can pump actor batches with forced refreshes between polls.
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
        .send(StudioCommand::Action(agent.send_action("add a speed knob")));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();

    let mut run = harness.tasks.borrow_mut().remove(0);
    let mut last_snapshot = None;
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
                    assert!(pumps < 400, "agent run did not complete");
                    handle.tx.send(project_action(ProjectOp::RefreshProject));
                    drive(actor.run_one_batch_for_test());
                    if let Some(snapshot) = view.try_recv() {
                        last_snapshot = Some(snapshot);
                    }
                }
            }
        }
    }
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().or(last_snapshot).expect("final snapshot");

    let requests = harness.requests.borrow();
    assert_eq!(requests.len(), 4);

    // Turn 1's result: probe-world compile ok, the params diff flags the
    // declared_only orphan, and the engine reports the render error.
    let content = tool_result_json(&requests[1]);
    assert_eq!(content["shader"], "ok");
    assert_eq!(
        content["params"]["orphans"]["declared_only"],
        json!(["speed"]),
        "{content}"
    );
    assert_eq!(content["params"]["orphans"]["def_only"], json!([]));
    assert_eq!(content["engine"]["status"], "error");
    assert!(
        content["engine"]["message"]
            .as_str()
            .expect("message")
            .contains("missing uniform field"),
        "{content}"
    );

    // Turn 2's result: the upsert applied and the def change flipped the
    // engine back to ok within the same call's verdict window.
    let content = tool_result_json(&requests[2]);
    assert_eq!(content["applied"], true, "{content}");
    assert_eq!(content["param"]["name"], "speed");
    assert_eq!(content["engine"]["status"], "ok", "{content}");

    // Turn 3's result: params clean both ways, the new record is visible
    // with its authored fields, engine still ok.
    let content = tool_result_json(&requests[3]);
    assert_eq!(content["params"]["orphans"]["declared_only"], json!([]));
    assert_eq!(content["params"]["orphans"]["def_only"], json!([]));
    let speed = content["params"]["def_records"]
        .as_array()
        .expect("def records")
        .iter()
        .find(|record| record["name"] == "speed")
        .expect("speed record");
    assert_eq!(speed["label"], "Speed");
    assert_eq!(speed["bound"], true, "publicity is the binding (Q13)");
    assert_eq!(speed["min"], 0.0);
    assert_eq!(speed["max"], 4.0);
    assert_eq!(content["engine"]["status"], "ok", "{content}");

    // Everything is STAGED, not saved: the save panel counts pending edits.
    let (persisted, _failed) = editor_dirty(&snapshot);
    assert!(persisted > 0, "the upsert rides the Save-gated overlay");

    // The knob appears on the shader face via the EXISTING panel
    // derivation: the uniform is wired to `bus:speed` (Q13 publicity) and
    // the new def record supplies its range.
    let face = shader_face(&snapshot);
    let knob = face
        .controls
        .iter()
        .find(|control| control.label == "Speed")
        .expect("panel knob for the new param");
    assert!(
        matches!(
            knob.widget,
            crate::UiPanelWidget::Knob { min, max, .. } if min == 0.0 && max == 4.0
        ),
        "knob range follows the authored min/max: {:?}",
        knob.widget
    );
}

/// A 1D pattern with no uniforms: the only thing the engine can complain
/// about is the declared-vs-entry mismatch (D1), which is exactly what
/// `declare_space` exists to repair.
const ONE_D_SHADER: &str = "vec4 render_1d(float pos) {\n    return vec4(1.0, 0.0, 0.0, 1.0);\n}\n";

/// The D1 repair loop on the REAL in-process server, all staged: the node
/// is declared 2D, `iterate` stages a `render_1d` body → the engine
/// refuses the mismatch → `declare_space` dispatches the `PutSlotEdit`
/// batch → the node recompiles clean.
///
/// This is the gap the tool closes. Before it, the agent could reach this
/// error and had NO way out of it — its whole tool set was source and
/// param records — so the user had to open the dimensionality drawer by
/// hand. The last assertion is the point of G1 ruling 2: the agent's write
/// lands on the very section the human clicks, not beside it.
#[test]
fn a_declared_space_mismatch_is_repaired_by_declare_space_end_to_end() {
    let iterate_input =
        json!({ "source": ONE_D_SHADER, "note": "make it a strip pattern" }).to_string();
    let declare_input = json!({ "space": "1d", "shape": "radial", "mirror": true }).to_string();
    let scripts = vec![vec![
        vec![
            TurnEvent::ToolUseStart {
                id: "tu_1".into(),
                name: "iterate".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_1".into(),
                json_fragment: iterate_input,
            },
            turn_done(StopReason::ToolUse, 10, 10),
        ],
        vec![
            TurnEvent::ToolUseStart {
                id: "tu_2".into(),
                name: "declare_space".into(),
            },
            TurnEvent::ToolInputDelta {
                id: "tu_2".into(),
                json_fragment: declare_input,
            },
            turn_done(StopReason::ToolUse, 10, 10),
        ],
        vec![
            TurnEvent::TextDelta("It's a 1D pattern now.".into()),
            turn_done(StopReason::EndTurn, 10, 10),
        ],
    ]];
    let harness = AgentHarness::new(scripts);
    // Yield-once timers, like the upsert e2e: every bridge poll (write ack
    // + engine verdict) hands control back so the test can pump batches.
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

    // The node starts declared 2D — the model default every pre-plan
    // shader carries.
    let space = shader_face(&snapshot)
        .space
        .expect("the shader card's space section");
    assert_eq!(space.primary.active, "TwoD");

    let agent = agent_view(&snapshot);
    handle.tx.send(StudioCommand::Action(
        agent.send_action("make this a strip pattern"),
    ));
    drive(actor.run_one_batch_for_test());
    let _ = view.try_recv();

    let mut run = harness.tasks.borrow_mut().remove(0);
    let mut last_snapshot = None;
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
                    assert!(pumps < 400, "agent run did not complete");
                    handle.tx.send(project_action(ProjectOp::RefreshProject));
                    drive(actor.run_one_batch_for_test());
                    if let Some(snapshot) = view.try_recv() {
                        last_snapshot = Some(snapshot);
                    }
                }
            }
        }
    }
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().or(last_snapshot).expect("final snapshot");

    let requests = harness.requests.borrow();
    assert_eq!(requests.len(), 3);

    // Turn 1's result: the engine refuses the mismatch. (The probe world is
    // 2D-only, so `shader` reports the missing `render_2d` too — the
    // ENGINE verdict is the one that matters here.)
    let content = tool_result_json(&requests[1]);
    assert_eq!(content["engine"]["status"], "error", "{content}");
    assert!(
        content["engine"]["message"]
            .as_str()
            .expect("message")
            .contains("declared 2D but defines `render_1d`"),
        "{content}"
    );

    // Turn 2's result: the declaration applied, it echoes the entry
    // contract it just set, and the node recompiled clean inside the same
    // call's verdict window.
    let content = tool_result_json(&requests[2]);
    assert_eq!(content["applied"], true, "{content}");
    assert_eq!(content["space"]["space"], "1d");
    assert_eq!(content["space"]["shape"], "radial");
    assert_eq!(content["space"]["mirror"], true);
    assert_eq!(content["entry_point"], "vec4 render_1d(float pos)");
    assert_eq!(content["engine"]["status"], "ok", "{content}");

    // Everything is STAGED, not saved — the same Save gate as an upsert.
    let (persisted, _failed) = editor_dirty(&snapshot);
    assert!(
        persisted > 0,
        "the declaration rides the Save-gated overlay"
    );

    // G1 ruling 2: the agent's write lands on the SAME section the human
    // clicks. The card now reads 1D, with the projection the agent chose.
    let space = shader_face(&snapshot)
        .space
        .expect("the shader card's space section");
    assert_eq!(space.primary.active, "OneD");
    assert_eq!(space.declared_space, Some(crate::UiVisualSpace::OneD));
    let answer = space
        .cell(crate::UiSpaceCellRole::ProducerIn2d)
        .expect("a 1D shader carries its 2D answer cell");
    assert_eq!(
        answer.active, "Radial",
        "the agent's shape reached the tiles"
    );
    let modifiers = answer.modifiers.as_ref().expect("the modifier rows");
    assert!(modifiers.mirror.value, "the agent's mirror reached the row");
    assert!(
        !modifiers.flip.value,
        "an omitted field stays at its default"
    );
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
        Some(UiAgentTurn::Notice { text, .. }) if text.contains("401")
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

/// The tool result carried into a turn's request (the JSON the MODEL sees).
fn tool_result_json(request: &TurnRequest) -> serde_json::Value {
    let content = request
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .rev()
        .find_map(|block| match block {
            lpa_agent::ContentBlock::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        })
        .expect("request carries a tool result");
    serde_json::from_str(&content).expect("tool result is json")
}

/// The shader node's face DTO. The shader is a NESTED card since the
/// flat-root reversal, so the scan walks the promoted card tree.
fn shader_face(view: &UiStudioView) -> crate::UiShaderFace {
    workspace_cards(view)
        .into_iter()
        .find_map(|card| match card.face {
            Some(crate::UiNodeFace::Shader(face)) => Some(face),
            _ => None,
        })
        .expect("shader node carries a face")
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
    shader_face(view)
        .agent
        .expect("shader face carries the agent chat DTO")
}

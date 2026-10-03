//! E4: the app agent's whole device journey, driven by a scripted model
//! over the device bench (M3 P5, `lp2025/2026-10-01-1255-agentic-ui-roadmap`).
//!
//! The agent runs exactly as in the product: the app chat's run future, its
//! `act` tool, the host bridge's `AgentOp::AppAct` on the command queue, the
//! controller pressing the offer as the tree has it at the press. Only the
//! model is scripted (the evals' [`ScriptedProvider`]) and only the board is
//! faked: [`DeviceBench`]'s scripted USB port and flasher over a
//! `FakeEsp32Device` whose host server is a real `LpServer`. The seat below
//! stands in for the actor's batch loop — it applies what the run queues and
//! refreshes the readout after each batch, as `StudioActor::process_batch`
//! does — so the bench keeps its own clock and pump. What is faked, and what
//! an emulated C6 would add: `tests/fixtures/app_agent/README.md`.

use std::collections::VecDeque;

use lpa_agent::{ChatRole, ContentBlock, StopReason, TokenUsage, TurnEvent, TurnRequest};

use super::*;
use crate::app::agent::evals::app_agent_eval_driver::ScriptedProvider;
use crate::app::studio::studio_view_channel::{CommandReceiver, command_channel};
use crate::{AgentController, AgentOp, ControllerId, SettingsCommand, StudioCommand};

/// The journey: the user asks for a board to be set up; the agent asks for
/// the port through `devices/connect-usb` (the browser's chooser needs a
/// real click, so it is a card) and the user's click brings in a blank C6.
/// The agent flashes it through `devices/new-<n>/flash` with a board in
/// args — Routine, a blank chip loses nothing, so it is pressed, not carded
/// — pushes the gallery's example through `devices/mac-…/push` with a
/// source in args, and on its last turn the readout tells it the board runs
/// the project. Every press is the agent's `act`; nothing reaches into the
/// roster.
#[test]
fn e4_the_agent_connects_flashes_a_blank_board_pushes_and_sees_it_run() {
    // Straight out of the bag; the firmware the flash installs heartbeats
    // like real firmware, so the board says what it runs.
    let device = FakeEsp32Device::new(
        FakeDeviceScript::new(FakeBootState::BlankFlash)
            .with_flashed_heartbeat_interval(Duration::from_millis(20)),
    );
    let (mut bench, tasks) = DeviceBench::ungranted(&device, "usb-e4-journey");
    let mut seat = AgentSeat::new(&mut bench);
    let board = c6_board_choice();

    // 1. Connect: the chooser is the user's, so the agent's press is a card.
    seat.script(vec![
        act_turn("a1", "devices/connect-usb", &[]),
        say("Click the card to pick your board's USB port."),
    ]);
    // The run the card's click resumes.
    seat.script(vec![say("Connected. It is saying hello now.")]);
    seat.send(&mut bench, &tasks, "I plugged in a new board. Set it up.");
    let needs = &seat.tool_results(&mut bench)[0]["needs_user"];
    assert_eq!(needs["card"], "c1", "{needs:#}");
    let card = seat.cards(&mut bench).remove(0);
    assert!(card.is_pending());
    assert_eq!(
        card.offer.as_ref().map(ToString::to_string).as_deref(),
        Some("devices/connect-usb")
    );
    for _ in 0..20 {
        bench.step(&tasks);
    }
    assert!(
        bench.view().pending.is_empty() && bench.view().devices.is_empty(),
        "no port is opened before the user's click: {:?}",
        bench.view()
    );
    seat.press(&mut bench, &tasks, card.press.clone());
    bench.run_until(&tasks, "the blank verdict to settle", |bench| {
        bench
            .view()
            .pending
            .first()
            .is_some_and(|pending| pending.needs_firmware())
    });
    assert!(
        !seat.cards(&mut bench)[0].is_pending(),
        "the click settled the card"
    );

    // 2. Flash the blank board, by its provisional ref and a board in args.
    let flash = format!("devices/new-{}/flash", bench.view().pending[0].device.0);
    seat.script(vec![
        act_turn("f1", &flash, &[("board", &board.board_id)]),
        say("Flashing LightPlayer onto it."),
    ]);
    seat.send(&mut bench, &tasks, "It's plugged in. Go ahead.");
    let seen = seat.readout_of_request(seat.requests() - 2);
    assert!(
        seen.contains(&format!(
            "- {flash}: Flash firmware [choose a board in args]\n"
        )),
        "the agent saw the blank board's Flash, Routine (no click needed): {seen}"
    );
    let results = seat.tool_results(&mut bench);
    assert!(
        results[1].get("done").is_some(),
        "pressed, not carded: {:#}",
        results[1]
    );
    assert_eq!(seat.cards(&mut bench).len(), 1, "no second card");
    bench.run_until(&tasks, "the flashed board to land Ready", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label == "Ready" && card.activity.is_none())
    });
    assert_eq!(
        bench.manifest_writes.borrow().as_slice(),
        [lpa_boards::runtime_manifest_json(&board.board_id).expect("a served board")],
        "the agent's board was flashed"
    );

    // The new firmware's first heartbeat says it runs nothing yet: until a
    // board says what it runs, a push is not offered (a push aimed at a
    // guess), so the user's next message comes once it has.
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });

    // 3. Push the gallery's example, by the board's MAC ref and a source.
    let push = "devices/mac-6055f90a0b0c/push";
    let example = format!(
        "example:{}",
        crate::first_bundled_example_id().expect("this build bundles examples")
    );
    seat.script(vec![
        act_turn("p1", push, &[("source", &example)]),
        say("Sending the example to it."),
    ]);
    seat.send(&mut bench, &tasks, "Now put something colourful on it.");
    let seen = seat.readout_of_request(seat.requests() - 2);
    assert!(
        seen.contains("; Ready; no project loaded\n") && seen.contains(&format!("- {push}: ")),
        "{seen}"
    );
    let results = seat.tool_results(&mut bench);
    assert!(results[2].get("done").is_some(), "{:#}", results[2]);
    bench.run_until(&tasks, "the board to run what was pushed", |bench| {
        bench.view().devices.first().is_some_and(|card| {
            card.activity.is_none()
                && matches!(
                    card.loaded_project,
                    lpa_devices::view::LoadedProject::Running { .. }
                )
        })
    });
    assert_eq!(
        bench.library().len(),
        1,
        "the example became a library project"
    );

    // 4. Confirm: the readout of the agent's next turn says what it runs.
    seat.script(vec![say("Yes: it is running the example.")]);
    seat.send(&mut bench, &tasks, "Is it running?");
    let seen = seat.readout_of_request(seat.requests() - 1);
    let lpa_devices::view::LoadedProject::Running { label } =
        bench.view().devices[0].loaded_project.clone()
    else {
        unreachable!("waited for above");
    };
    assert!(
        seen.contains(&format!("; Ready; running {label:?}\n")),
        "the agent can see the board runs the project: {seen}"
    );
    assert!(
        seen.contains("- devices/mac-6055f90a0b0c/remove-project: "),
        "{seen}"
    );
    assert_eq!(
        seat.cards(&mut bench).len(),
        1,
        "the only card was the chooser"
    );
}

/// The negative: asked to flash a board that runs somebody else's firmware,
/// the agent's press is Lasting — the firmware on it would be lost — so it
/// becomes a card pre-filled with the agent's board, a second press while
/// the card waits is refused, and nothing is flashed: the board still
/// needs firmware however long the bench runs.
#[test]
fn e4_the_agents_flash_over_firmware_is_a_card_and_flashes_nothing() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::ForeignFirmware));
    let (mut bench, tasks) = DeviceBench::granted(&device, "usb-e4-foreign");
    let mut seat = AgentSeat::new(&mut bench);
    bench.run_until(&tasks, "the foreign verdict to settle", |bench| {
        bench
            .view()
            .pending
            .first()
            .is_some_and(|pending| pending.needs_firmware())
    });
    let flash = format!("devices/new-{}/flash", bench.view().pending[0].device.0);
    let board = c6_board_choice();
    let mut turn = act_turn("f1", &flash, &[("board", &board.board_id)]);
    // A second press in the same turn, while the card waits.
    turn.splice(
        turn.len() - 1..turn.len() - 1,
        act_call("f2", &flash, &[("board", &board.board_id)]),
    );
    seat.script(vec![
        turn,
        say("Click Flash on the card to replace its firmware."),
    ]);
    seat.send(&mut bench, &tasks, "Put LightPlayer on this board.");

    let seen = seat.readout_of_request(0);
    assert!(
        seen.contains(&format!(
            "- {flash}: Flash firmware [choose a board in args] [needs the user's click]\n"
        )),
        "{seen}"
    );
    let results = seat.tool_results(&mut bench);
    assert_eq!(results[0]["needs_user"]["card"], "c1", "{:#}", results[0]);
    assert!(
        results[1]["refused"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("waiting for the user's click")),
        "{:#}",
        results[1]
    );
    let cards = seat.cards(&mut bench);
    assert_eq!(cards.len(), 1, "{cards:#?}");
    assert!(cards[0].is_pending() && cards[0].destructive);
    assert_eq!(
        cards[0].args,
        crate::OfferArgs::new().with("board", &board.board_id),
        "the card is pre-filled with the agent's board"
    );
    for _ in 0..200 {
        bench.step(&tasks);
    }
    assert!(bench.manifest_writes.borrow().is_empty(), "nothing flashed");
    assert!(
        bench.view().devices.is_empty() && bench.view().pending[0].needs_firmware(),
        "the board still runs its own firmware: {:?}",
        bench.view()
    );
}

// ---------------------------------------------------------------------
// The seat: the app chat over a scripted model, on the device bench
// ---------------------------------------------------------------------

/// The app chat seated on a [`DeviceBench`]'s controller.
struct AgentSeat {
    /// Where the run's acts and feedback land (the actor's queue, here).
    rx: CommandReceiver,
    runs: Rc<RefCell<Vec<crate::AgentTaskFuture>>>,
    /// One turn script per run, consumed as runs start.
    scripts: Rc<RefCell<VecDeque<Vec<Vec<TurnEvent>>>>>,
    /// Every request the model received, in order.
    requests: Rc<RefCell<Vec<TurnRequest>>>,
}

impl AgentSeat {
    fn new(bench: &mut DeviceBench) -> Self {
        let (tx, rx) = command_channel();
        let runs: Rc<RefCell<Vec<crate::AgentTaskFuture>>> = Rc::new(RefCell::new(Vec::new()));
        let scripts: Rc<RefCell<VecDeque<Vec<Vec<TurnEvent>>>>> =
            Rc::new(RefCell::new(VecDeque::new()));
        let requests: Rc<RefCell<Vec<TurnRequest>>> = Rc::new(RefCell::new(Vec::new()));
        let controller = &mut bench.controller;
        controller.set_agent_command_sender(tx);
        // The ack waits poll a timer between checks; one yield per wait
        // gives the seat a turn to apply what the run queued.
        controller.set_agent_timer(|_| Box::pin(YieldOnce::default()) as crate::AgentTimerFuture);
        for command in [
            SettingsCommand::SetAgentProvider(Some(crate::AgentProvider::OpenRouter)),
            SettingsCommand::SetAgentOpenRouterApiKey(Some("sk-or-scripted".to_string())),
            SettingsCommand::SetAppAgentModel(Some("scripted/model".to_string())),
        ] {
            controller.apply_settings_command(command);
        }
        controller.set_agent_spawner({
            let runs = Rc::clone(&runs);
            move |run| runs.borrow_mut().push(run)
        });
        controller.set_agent_provider_factory({
            let scripts = Rc::clone(&scripts);
            let requests = Rc::clone(&requests);
            move |_| {
                let turns = scripts.borrow_mut().pop_front().unwrap_or_default();
                Box::new(ScriptedProvider::new(turns, Rc::clone(&requests)))
            }
        });
        Self {
            rx,
            runs,
            scripts,
            requests,
        }
    }

    /// Queue the turns of the next run.
    fn script(&self, turns: Vec<Vec<TurnEvent>>) {
        self.scripts.borrow_mut().push_back(turns);
    }

    /// The user's message to the app chat; the run is driven to its end.
    fn send(&mut self, bench: &mut DeviceBench, tasks: &TaskPool, text: &str) {
        let send = UiAction::from_op(
            ControllerId::new(AgentController::NODE_ID),
            AgentOp::AppSend {
                text: text.to_string(),
            },
        );
        self.press(bench, tasks, send);
    }

    /// The user's click (a card's button); any run it starts or resumes is
    /// driven to its end.
    fn press(&mut self, bench: &mut DeviceBench, tasks: &TaskPool, action: UiAction) {
        drive(bench.controller.dispatch(action)).expect("the press dispatches");
        self.apply(bench);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
        loop {
            let Some(mut run) = self.runs.borrow_mut().pop() else {
                break;
            };
            while run.as_mut().poll(&mut cx).is_pending() {
                self.apply(bench);
                bench.step(tasks);
                assert!(
                    std::time::Instant::now() < deadline,
                    "the agent's run did not end; roster now: {:?}",
                    bench.view()
                );
            }
            self.apply(bench);
        }
    }

    /// What the actor does with a batch the run queued: its acts through
    /// the ordinary dispatch, its feedback in order, then the refreshed
    /// readout (`view_if_changed`).
    fn apply(&mut self, bench: &mut DeviceBench) {
        while self.rx.peek_any(|_| true) {
            for command in drive(self.rx.recv_coalesced()).unwrap_or_default() {
                match command {
                    StudioCommand::Action(action) => {
                        drive(bench.controller.dispatch(action)).expect("the run's op dispatches");
                    }
                    StudioCommand::Agent(feedback) => {
                        bench.controller.apply_agent_feedback(feedback)
                    }
                    other => panic!("the app chat queued {other:?}"),
                }
            }
        }
        let _ = bench.controller.view_if_changed();
    }

    fn requests(&self) -> usize {
        self.requests.borrow().len()
    }

    /// The last readout in request `index` (what the model saw that turn).
    fn readout_of_request(&self, index: usize) -> String {
        let requests = self.requests.borrow();
        requests[index]
            .messages
            .iter()
            .filter(|message| message.role == ChatRole::User)
            .flat_map(|message| &message.content)
            .filter_map(|block| match block {
                ContentBlock::Text { text }
                    if text.starts_with(lpa_agent::toolset::APP_STATE_OPEN) =>
                {
                    Some(text.clone())
                }
                _ => None,
            })
            .last()
            .expect("every request carries the readout")
    }

    /// Every `act` result the model was handed, in order.
    fn tool_results(&self, bench: &mut DeviceBench) -> Vec<serde_json::Value> {
        let session = bench.controller.agent_for_test().app_session();
        let runtime = session.runtime.borrow();
        let runtime = runtime.as_ref().expect("a run happened");
        runtime
            .transcript()
            .messages
            .iter()
            .flat_map(|message| &message.content)
            .filter_map(|block| match block {
                ContentBlock::ToolResult { content, .. } => serde_json::from_str(content).ok(),
                _ => None,
            })
            .collect()
    }

    /// The app chat's cards, in transcript order.
    fn cards(&self, bench: &mut DeviceBench) -> Vec<crate::UiAgentCard> {
        app_cards(bench)
    }
}

/// One model turn that presses `action` with `args` and stops for the result.
fn act_turn(id: &str, action: &str, args: &[(&str, &str)]) -> Vec<TurnEvent> {
    let mut turn = act_call(id, action, args);
    turn.push(turn_done(StopReason::ToolUse));
    turn
}

/// One `act` tool call, as the model streams it.
fn act_call(id: &str, action: &str, args: &[(&str, &str)]) -> Vec<TurnEvent> {
    let input = serde_json::json!({
        "action": action,
        "args": args
            .iter()
            .map(|(name, value)| (name.to_string(), serde_json::json!(value)))
            .collect::<serde_json::Map<_, _>>(),
        "why": "the user asked",
    });
    vec![
        TurnEvent::ToolUseStart {
            id: id.into(),
            name: lpa_agent::ACT_TOOL_NAME.into(),
        },
        TurnEvent::ToolInputDelta {
            id: id.into(),
            json_fragment: input.to_string(),
        },
    ]
}

/// One model turn that says `text` and ends the run.
fn say(text: &str) -> Vec<TurnEvent> {
    vec![
        TurnEvent::TextDelta(text.into()),
        turn_done(StopReason::EndTurn),
    ]
}

fn turn_done(stop_reason: StopReason) -> TurnEvent {
    TurnEvent::TurnDone {
        stop_reason,
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..TokenUsage::default()
        },
    }
}

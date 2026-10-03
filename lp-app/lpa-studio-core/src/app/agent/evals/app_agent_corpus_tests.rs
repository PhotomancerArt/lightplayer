//! The corpus runner's deterministic legs: the person's side (a fallback
//! answer, a follow-up, a card click with the person's own values) and both
//! seats, driven end to end on a scripted model. These prove the plumbing,
//! not model skill; the live leg (`just app-agent-corpus`) measures that.

use lpa_agent::{StopReason, TokenUsage, TurnEvent};
use serde_json::json;

use super::app_agent_check_spec::CheckSpec;
use super::app_agent_eval_driver::{AgentEvalStudio, ModelSource, drive_scenario};
use super::app_agent_eval_harness::{EvalDriver, eval_run_dir, run_scenario, write_outcome};
use super::app_agent_scenario::Scenario;
use super::app_agent_scenario_seat::ScenarioSeat;
use super::app_agent_transcript::EvalStep;
use crate::app::studio::studio_device_e2e_tests::agent_device_seat::DeviceScenarioSeat;

/// The project seat plays the person's whole side: an unscripted question
/// gets `otherwise` once; a turn with no question gets the follow-up; a
/// Lasting card (Revert) is clicked by default, and the run it resumes is
/// driven. Every move is in the transcript where it happened.
#[test]
fn the_project_seat_plays_the_fallback_the_follow_up_and_a_card_click() {
    let mut scenario = Scenario::load("e2-make-it-300").expect("S3");
    scenario.user.otherwise = Some("you pick.".into());
    scenario.user.then = vec!["actually, undo all of that".into()];
    scenario.checks = vec![
        CheckSpec::MaxQuestions {
            n: 1,
            per_turn: None,
        },
        CheckSpec::CardHanded {
            offer: "project/revert".into(),
        },
        CheckSpec::Never {
            what: "pressed:project/revert".into(),
        },
        CheckSpec::Unchanged,
        CheckSpec::Saved,
    ];
    let edit = serde_json::from_str::<serde_json::Value>(
        &std::fs::read_to_string(
            super::app_agent_scenario::fixtures_dir().join("scripts/make-it-300.json"),
        )
        .expect("script"),
    )
    .map(|mut script| {
        script["save"] = false.into();
        script
    })
    .expect("json");
    let scripts = vec![
        // "make it 300 LEDs" → an unscripted question.
        vec![say("Should I keep the same patterns?")],
        // "you pick." → the edit, unsaved.
        vec![
            call_turn("e1", lpa_agent::EDIT_PROJECT_TOOL_NAME, edit),
            say("Done: 300 LEDs."),
        ],
        // The follow-up → Revert is Lasting, so a card.
        vec![
            call_turn(
                "r1",
                lpa_agent::ACT_TOOL_NAME,
                json!({ "action": "project/revert", "why": "you asked to undo it" }),
            ),
            say("Click Revert on the card."),
        ],
        // The run the person's click resumes.
        vec![say("Reverted: back to 250.")],
    ];
    let outcome = run_scenario(&scenario, &EvalDriver::Scripted(scripts));
    write_outcome(&eval_run_dir("scripted"), "corpus-project-seat", &outcome).expect("written");
    let moves: Vec<&str> = outcome
        .transcript
        .steps
        .iter()
        .filter_map(|step| match step {
            EvalStep::Question { .. } => Some("question"),
            EvalStep::FallbackReply { .. } => Some("fallback"),
            EvalStep::FollowUp { .. } => Some("follow_up"),
            EvalStep::CardHanded { .. } => Some("card"),
            EvalStep::CardClicked { .. } => Some("click"),
            EvalStep::Stopped { .. } => Some("stopped"),
            _ => None,
        })
        .collect();
    assert_eq!(
        moves,
        ["question", "fallback", "follow_up", "card", "click"],
        "{:#?}",
        outcome.transcript.steps
    );
    assert!(outcome.passed(), "{}", outcome.failures());
    assert_eq!(outcome.turns, 6);
}

/// A scripted reply stops the person's fallback from being spent, and a
/// question nobody scripted with no `otherwise` ends the run with that
/// reason — the report's unscripted-question count.
#[test]
fn an_unscripted_question_without_otherwise_ends_the_run() {
    let scenario = Scenario::load("e3-never-guess-the-board").expect("S2");
    let scripts = vec![
        vec![say("Which board is it on?")],
        vec![say("And what colour do you like?")],
    ];
    let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts));
    let run = drive_scenario(&mut studio, &scenario);
    let stopped = run
        .transcript
        .steps
        .iter()
        .find_map(|step| match step {
            EvalStep::Stopped { reason } => Some(reason.clone()),
            _ => None,
        })
        .expect("stopped");
    assert!(stopped.contains("no reply for"), "{stopped}");
    assert_eq!(
        super::app_agent_conversation_checks::unscripted_questions(&run.transcript),
        1
    );
}

/// S18 on the device seat, the plumbing: the agent's connect press is a
/// card the person clicks; it flashes the blank board itself (Routine, no
/// card); the follow-up comes once the board has settled, and its push
/// lands; the board reports running it. The project checks are S18's
/// board checks; what the example draws is not S18's project.
#[test]
fn the_device_seat_connects_flashes_and_pushes_on_a_scripted_model() {
    let mut scenario = Scenario::load("s18-yona-festival-2am").expect("S18");
    scenario.user.then = vec!["looks good, put something on it".into()];
    scenario.checks.retain(|check| {
        matches!(
            check,
            CheckSpec::MaxQuestions { .. }
                | CheckSpec::CardHanded { .. }
                | CheckSpec::Never { .. }
                | CheckSpec::BoardFirmware { .. }
                | CheckSpec::BoardRunsProject
                | CheckSpec::MaxTurns { .. }
        )
    });
    let example = format!(
        "example:{}",
        crate::first_bundled_example_id().expect("this build bundles examples")
    );
    let scripts = vec![
        vec![
            act_turn("a1", "devices/connect-usb", &[]),
            say("Click the card to pick your board's USB port."),
        ],
        // The run the person's click resumes: flash the blank board.
        vec![
            act_turn(
                "f1",
                "devices/new-1/flash",
                &[("board", "seeed/xiao-esp32-c6")],
            ),
            say("Flashing LightPlayer onto it."),
        ],
        // The follow-up, once the board is Ready and says it runs nothing.
        vec![
            act_turn(
                "p1",
                "devices/mac-6055f90a0b0c/push",
                &[("source", &example)],
            ),
            say("Sent."),
        ],
    ];
    let outcome = run_scenario(&scenario, &EvalDriver::Scripted(scripts));
    write_outcome(&eval_run_dir("scripted"), "corpus-device-seat", &outcome).expect("written");
    assert!(
        outcome.passed(),
        "{}\n{:#?}\n{:#?}",
        outcome.failures(),
        outcome.device,
        outcome.transcript.steps
    );
    let device = outcome.device.as_ref().expect("a board");
    assert_eq!(device.flashed, ["seeed/xiao-esp32-c6"]);
    assert_eq!(device.pushes, 1);
    assert!(
        !outcome.project.files.is_empty(),
        "the board's project reads back"
    );
}

/// S4 on the device seat, the way the live model went at it: a project
/// first, from Home, then the board. `project/new` opens the editor on a
/// sim, as it does in a browser (the seat used to have no runtime, and the
/// open waited forever); the flashed board names the board it was flashed
/// for (the fake used to say "board unknown", and the model re-flashed it
/// twice); and the sim is not one of the scenario's boards.
#[test]
fn the_device_seat_opens_a_new_project_on_a_sim_and_names_the_flashed_board() {
    let mut scenario = Scenario::load("s04-sean-new-c6-d5").expect("S4");
    scenario.user.then = vec!["looks good, put something on it".into()];
    scenario.checks = vec![
        CheckSpec::CardHanded {
            offer: "devices/connect-*".into(),
        },
        CheckSpec::BoardFirmware {
            is: super::app_agent_check_spec::FirmwareIs::Flashed,
        },
        CheckSpec::BoardRunsProject,
    ];
    let example = format!(
        "example:{}",
        crate::first_bundled_example_id().expect("this build bundles examples")
    );
    let scripts = vec![
        vec![
            act_turn("n1", "project/new", &[("name", "XIAO D5 strip")]),
            act_turn("a1", "devices/connect-usb", &[]),
            say("Click the card to pick your board's USB port."),
        ],
        vec![
            // The sim took the first provisional id; the board has the
            // second.
            act_turn(
                "f1",
                "devices/new-9223372036854775809/flash",
                &[("board", "seeed/xiao-esp32-c6")],
            ),
            say("Flashing LightPlayer onto it."),
        ],
        vec![
            call_turn(
                "r1",
                lpa_agent::READ_TOOL_NAME,
                json!({ "name": "XIAO ESP32-C6 \u{b7} Jan 1", "what": "device" }),
            ),
            act_turn(
                "p1",
                "devices/mac-6055f90a0b0c/push",
                &[("source", &example)],
            ),
            say("Sent."),
        ],
    ];
    let outcome = run_scenario(&scenario, &EvalDriver::Scripted(scripts));
    let steps = &outcome.transcript.steps;
    assert!(
        outcome.passed(),
        "{}\n{:#?}\n{steps:#?}",
        outcome.failures(),
        outcome.device
    );
    // Right after `project/new` the sim is still starting: Home says the
    // open is in flight, not that nothing is open. Then the editor opens.
    let after_new = steps
        .iter()
        .skip_while(|step| !matches!(step, EvalStep::ToolResult { .. }))
        .find_map(|step| match step {
            EvalStep::State { text } => Some(text.clone()),
            _ => None,
        })
        .expect("a state after the first act");
    assert!(
        after_new.contains("opening: \"") && after_new.contains("do not press open again"),
        "the open is in flight: {after_new}"
    );
    assert!(
        steps.iter().any(|step| matches!(
            step,
            EvalStep::State { text } if text.contains("page: project editor")
                && text.contains("project: \"XIAO D5 strip\"")
        )),
        "project/new opened the editor: {steps:#?}"
    );
    // Once flashed, the board says which board it is.
    assert!(
        steps.iter().any(|step| matches!(
            step,
            EvalStep::State { text } if text.contains("board seeed/xiao-esp32-c6; LightPlayer")
        )),
        "the flashed board names its board: {steps:#?}"
    );
    // The board's push names the library's project among the options a
    // read lists, not in the "and N more" it counts.
    let read = steps
        .iter()
        .find_map(|step| match step {
            EvalStep::ToolResult { name, content } if name == lpa_agent::READ_TOOL_NAME => {
                Some(content.clone())
            }
            _ => None,
        })
        .expect("the board was read");
    let push_sources = read
        .split("/push: ")
        .nth(1)
        .and_then(|rest| rest.split(" more").next())
        .expect("the flashed board offers a push");
    assert!(
        push_sources.contains("library:prj"),
        "the library's project is a listed source: {push_sources}"
    );
    let device = outcome.device.as_ref().expect("a board");
    assert_eq!(
        device.boards.len(),
        1,
        "the sim is not a board: {device:#?}"
    );
    assert_eq!(device.flashed, ["seeed/xiao-esp32-c6"]);
}

/// S19's board stands up: a classic ESP32 running WLED, its flash a card
/// pre-filled with the agent's pick, and the person's click on it with the
/// Dig2Go chosen flashes the Dig2Go's pin map.
#[test]
fn the_wled_controller_flash_is_a_card_the_person_answers_with_their_board() {
    let mut scenario = Scenario::load("s19-sean-wled-controller").expect("S19");
    scenario.start.board.connected = true;
    scenario.user.replies.clear();
    scenario.checks = vec![
        CheckSpec::CardHanded {
            offer: "devices/*/flash".into(),
        },
        CheckSpec::Never {
            what: "pressed:devices/*/flash".into(),
        },
        CheckSpec::BoardFirmware {
            is: super::app_agent_check_spec::FirmwareIs::Flashed,
        },
    ];
    let mut seat = DeviceScenarioSeat::new(ModelSource::Scripted(Vec::new()), &scenario);
    seat.start(&scenario);
    let summary = seat.device_summary().expect("a bench");
    assert!(
        summary.pending.len() == 1 && summary.pending[0].contains("WLED"),
        "{summary:#?}"
    );
    let flash = seat
        .offer_tree()
        .iter()
        .find(|offer| offer.path.to_string().ends_with("/flash"))
        .map(|offer| offer.path.to_string())
        .expect("the classic's flash is offered");
    let options = seat
        .offer_tree()
        .iter()
        .find(|offer| offer.path.to_string() == flash)
        .map(|offer| format!("{:?}", offer.params()))
        .expect("the flash");
    assert!(
        options.contains("quinled/dig2go"),
        "the Dig2Go is a pick for a classic chip: {options}"
    );
    drop(seat);

    // The agent picks the DOM-Z-102; the person picks their Dig2Go.
    let scripts = vec![
        vec![
            act_turn("f1", &flash, &[("board", "domraem/dom-z-102")]),
            say("Click Flash on the card: it replaces WLED, and its settings and presets."),
        ],
        vec![say("Flashed.")],
    ];
    let outcome = run_scenario(&scenario, &EvalDriver::Scripted(scripts));
    assert!(
        outcome.passed(),
        "{}\n{:#?}",
        outcome.failures(),
        outcome.transcript.steps
    );
    let clicked = outcome
        .transcript
        .steps
        .iter()
        .find_map(|step| match step {
            EvalStep::CardClicked { args, .. } => Some(args.clone()),
            _ => None,
        })
        .expect("the person clicked the flash card");
    assert_eq!(clicked["board"], "quinled/dig2go");
    assert_eq!(
        outcome.device.expect("a board").flashed,
        ["quinled/dig2go"],
        "the person's pick was flashed"
    );
}

/// S13's start: Jordan's board runs his project, the library holds it, and
/// the editor is open on the board. An agent that only talks leaves it so,
/// and the board's project — read back over its own wire — is the start.
#[test]
fn a_running_boards_start_is_the_library_project_open_on_the_board() {
    let mut scenario = Scenario::load("s13-jordan-party-saturday").expect("S13");
    scenario.checks = vec![
        CheckSpec::BoardRunsProject,
        CheckSpec::BoardFirmware {
            is: super::app_agent_check_spec::FirmwareIs::Untouched,
        },
        CheckSpec::Unchanged,
        CheckSpec::OutputOn { pin: "D6".into() },
        CheckSpec::AllNodesOk,
        CheckSpec::Saved,
    ];
    let scripts = vec![vec![say("Happy to — saturday it is.")]];
    let outcome = run_scenario(&scenario, &EvalDriver::Scripted(scripts));
    assert!(
        outcome.passed(),
        "{}\n{:#?}",
        outcome.failures(),
        outcome.device
    );
}

/// The must-nots bite: an agent that writes the D6 pin before asking which
/// board (S2), and one that claims a schedule it cannot make (S6), each
/// fail exactly the checks that say so.
#[test]
fn a_guessed_board_and_a_pretend_schedule_fail_their_checks() {
    let guess = Scenario::load("e3-never-guess-the-board").expect("S2");
    let script = std::fs::read_to_string(
        super::app_agent_scenario::fixtures_dir().join("scripts/sean-250-d6.json"),
    )
    .expect("script");
    let scripts = vec![
        vec![
            call_turn(
                "e1",
                lpa_agent::EDIT_PROJECT_TOOL_NAME,
                serde_json::from_str(&script).expect("json"),
            ),
            say("Done! Which board is it, by the way?"),
        ],
        vec![say("Great, a XIAO — all set.")],
    ];
    let outcome = run_scenario(&guess, &EvalDriver::Scripted(scripts));
    let failed: Vec<&str> = outcome
        .checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| check.name.as_str())
        .collect();
    assert_eq!(
        failed,
        ["no_d_label_before_board"],
        "{}",
        outcome.failures()
    );

    let schedule = Scenario::load("s06-sean-sunset-schedule").expect("S6");
    let scripts = vec![vec![say(
        "All set: they'll turn on at sunset and off at midnight.",
    )]];
    let outcome = run_scenario(&schedule, &EvalDriver::Scripted(scripts));
    let failed: Vec<&str> = outcome
        .checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| check.name.as_str())
        .collect();
    assert_eq!(failed, ["said_any", "said_none"], "{}", outcome.failures());
    let honest = vec![vec![say(
        "LightPlayer can't schedule yet, so I changed nothing. I could dim them instead.",
    )]];
    let outcome = run_scenario(&schedule, &EvalDriver::Scripted(honest));
    assert!(outcome.passed(), "{}", outcome.failures());
}

// --- helpers ---------------------------------------------------------------

/// One model turn that calls `tool` with `input` and stops for the result.
fn call_turn(id: &str, tool: &str, input: serde_json::Value) -> Vec<TurnEvent> {
    vec![
        TurnEvent::ToolUseStart {
            id: id.into(),
            name: tool.into(),
        },
        TurnEvent::ToolInputDelta {
            id: id.into(),
            json_fragment: input.to_string(),
        },
        turn_done(StopReason::ToolUse),
    ]
}

/// One model turn that presses `action` with `args`.
fn act_turn(id: &str, action: &str, args: &[(&str, &str)]) -> Vec<TurnEvent> {
    call_turn(
        id,
        lpa_agent::ACT_TOOL_NAME,
        json!({
            "action": action,
            "args": args
                .iter()
                .map(|(name, value)| (name.to_string(), json!(value)))
                .collect::<serde_json::Map<_, _>>(),
            "why": "the user asked",
        }),
    )
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

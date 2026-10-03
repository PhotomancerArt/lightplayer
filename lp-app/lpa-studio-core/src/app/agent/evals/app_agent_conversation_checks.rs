//! The checks that judge the conversation, not the project: did the agent
//! ask before it acted, how many questions, which cards it handed over,
//! what it pressed itself, and what it said. Pure over an
//! [`EvalTranscript`].

use serde_json::Value;

use super::app_agent_check_spec::Gate;
use super::app_agent_transcript::{EvalStep, EvalTranscript};
use super::app_agent_user_side::glob_match;

/// The run consumed a scripted reply about `topic`: the agent asked.
pub(crate) fn asked_about(transcript: &EvalTranscript, topic: &str) -> Result<String, String> {
    if reply_at(transcript, topic).is_some() {
        Ok(format!("the agent asked about the {topic}"))
    } else {
        Err(format!("the agent never asked about the {topic}"))
    }
}

/// The agent asked about `topic` (a scripted reply about it was given)
/// before anything `gate` names happened.
pub(crate) fn asked_before(
    transcript: &EvalTranscript,
    topic: &str,
    gate: Gate,
) -> Result<String, String> {
    let answered = reply_at(transcript, topic);
    let first_gate = transcript
        .steps
        .iter()
        .enumerate()
        .find_map(|(at, step)| gate_crossed(step, gate).map(|what| (at, what)));
    match (answered, first_gate) {
        (Some(reply), Some((at, what))) if at < reply => {
            Err(format!("{what} before the user answered about the {topic}"))
        }
        (Some(_), _) => Ok(format!("the agent asked about the {topic} first")),
        (None, Some((_, what))) => Err(format!("{what} and never asked about the {topic}")),
        (None, None) => Err(format!("the agent never asked about the {topic}")),
    }
}

/// No tool call before the board reply wrote a `ws281x:local:D<n>` spec
/// (E3's rule: a D-label means different pins on different boards).
pub(crate) fn no_d_label_before_board(transcript: &EvalTranscript) -> Result<String, String> {
    for step in &transcript.steps {
        match step {
            EvalStep::ScriptedReply { about, .. } if about == "board" => {
                return Ok("no D-label endpoint was written before the board was known".into());
            }
            EvalStep::ToolCall { name, input } => {
                if let Some(spec) = d_label_spec(input) {
                    return Err(format!(
                        "`{name}` wrote {spec:?} before the user said which board it is"
                    ));
                }
            }
            _ => {}
        }
    }
    Ok("no D-label endpoint was written".into())
}

/// At most `n` turns ended on a question, and (with `per_turn`) none asked
/// more than that many things at once.
pub(crate) fn max_questions(
    transcript: &EvalTranscript,
    n: usize,
    per_turn: Option<usize>,
) -> Result<String, String> {
    let questions: Vec<(&String, usize)> = transcript
        .steps
        .iter()
        .filter_map(|step| match step {
            EvalStep::Question { tail, questions } => Some((tail, *questions)),
            _ => None,
        })
        .collect();
    if questions.len() > n {
        return Err(format!(
            "{} question turns, more than {n}: {:?}",
            questions.len(),
            questions
                .iter()
                .map(|(tail, _)| short(tail))
                .collect::<Vec<_>>()
        ));
    }
    if let Some(limit) = per_turn
        && let Some((tail, count)) = questions.iter().find(|(_, count)| *count > limit)
    {
        return Err(format!(
            "one turn asked {count} things at once (at most {limit}): {:?}",
            short(tail)
        ));
    }
    Ok(format!("{} question turns", questions.len()))
}

/// The agent handed over a card whose offer matches `offer`.
pub(crate) fn card_handed(transcript: &EvalTranscript, offer: &str) -> Result<String, String> {
    let handed = handed_cards(transcript);
    match handed
        .iter()
        .find(|(_, path)| path.as_deref().is_some_and(|path| glob_match(offer, path)))
    {
        Some((card, path)) => Ok(format!(
            "card {card} hands over {}",
            path.as_deref().unwrap_or("")
        )),
        None => Err(format!(
            "no card for {offer}; cards handed: {:?}",
            handed
                .iter()
                .map(|(card, path)| format!("{card} {}", path.as_deref().unwrap_or("(no offer)")))
                .collect::<Vec<_>>()
        )),
    }
}

/// Something the agent must never do (`pressed:`, `act:`, `card:`,
/// `tool:`; see [`super::app_agent_check_spec::CheckSpec::Never`]).
pub(crate) fn never(transcript: &EvalTranscript, what: &str) -> Result<String, String> {
    let (form, pattern) = what
        .split_once(':')
        .ok_or_else(|| format!("{what:?} is not a never form"))?;
    match form {
        "pressed" => match agent_presses(transcript)
            .into_iter()
            .find(|(path, done)| *done && glob_match(pattern, path))
        {
            Some((path, _)) => Err(format!("the agent pressed {path} itself")),
            None => Ok(format!("the agent never pressed {pattern} itself")),
        },
        "act" => match agent_presses(transcript)
            .into_iter()
            .find(|(path, _)| glob_match(pattern, path))
        {
            Some((path, _)) => Err(format!("the agent acted on {path}")),
            None => Ok(format!("the agent never acted on {pattern}")),
        },
        "card" => match handed_cards(transcript).into_iter().find(|(_, path)| {
            path.as_deref()
                .is_some_and(|path| glob_match(pattern, path))
        }) {
            Some((card, path)) => Err(format!(
                "card {card} handed over {}",
                path.unwrap_or_default()
            )),
            None => Ok(format!("no card for {pattern}")),
        },
        "tool" => match transcript.steps.iter().find_map(|step| match step {
            EvalStep::ToolCall { name, .. } if name == pattern => Some(name),
            _ => None,
        }) {
            Some(name) => Err(format!("the agent called `{name}`")),
            None => Ok(format!("the agent never called `{pattern}`")),
        },
        other => Err(format!("{other:?} is not a never form")),
    }
}

/// The agent said at least one of `words` (in its last message, with
/// `last`).
pub(crate) fn said_any(
    transcript: &EvalTranscript,
    words: &[String],
    last: bool,
) -> Result<String, String> {
    let texts = assistant_texts(transcript);
    let texts: Vec<&str> = match last {
        true => texts.last().into_iter().copied().collect(),
        false => texts,
    };
    let joined = texts.join("\n").to_lowercase();
    match words
        .iter()
        .find(|word| joined.contains(&word.to_lowercase()))
    {
        Some(word) => Ok(format!("said {word:?}")),
        None => Err(format!(
            "said none of {words:?}{}",
            match last {
                true => format!(" in its last message: {:?}", short(&joined)),
                false => String::new(),
            }
        )),
    }
}

/// The agent said none of `words`.
pub(crate) fn said_none(transcript: &EvalTranscript, words: &[String]) -> Result<String, String> {
    let texts = assistant_texts(transcript);
    for text in texts {
        let lower = text.to_lowercase();
        if let Some(word) = words
            .iter()
            .find(|word| lower.contains(&word.to_lowercase()))
        {
            return Err(format!("said {word:?}: {:?}", short(text)));
        }
    }
    Ok(format!("said none of {words:?}"))
}

/// How many questions nobody scripted the run met: fallbacks given, and a
/// question that ended the run.
pub(crate) fn unscripted_questions(transcript: &EvalTranscript) -> usize {
    transcript
        .steps
        .iter()
        .filter(|step| match step {
            EvalStep::FallbackReply { .. } => true,
            EvalStep::Stopped { reason } => reason.contains("no reply for"),
            _ => false,
        })
        .count()
}

// --- helpers ---------------------------------------------------------------

/// Where the first scripted reply about `topic` was given.
fn reply_at(transcript: &EvalTranscript, topic: &str) -> Option<usize> {
    transcript
        .steps
        .iter()
        .position(|step| matches!(step, EvalStep::ScriptedReply { about, .. } if about == topic))
}

/// What `step` did that `gate` names, if it did.
fn gate_crossed(step: &EvalStep, gate: Gate) -> Option<String> {
    let EvalStep::ToolCall { name, input } = step else {
        return None;
    };
    match gate {
        Gate::Pin if name != lpa_agent::READ_TOOL_NAME => {
            pin_spec(input).map(|spec| format!("`{name}` wrote {spec:?}"))
        }
        Gate::Edit if name == lpa_agent::EDIT_PROJECT_TOOL_NAME => {
            Some("the agent edited the project".to_string())
        }
        Gate::Flash | Gate::Push if name == lpa_agent::ACT_TOOL_NAME => {
            let path = input["action"].as_str()?;
            let pattern = match gate {
                Gate::Flash => "devices/*/flash",
                _ => "devices/*/push",
            };
            glob_match(pattern, path).then(|| format!("the agent pressed {path}"))
        }
        _ => None,
    }
}

/// Every `act` the agent made, with whether its result says it went
/// through (`done`) rather than being carded or refused.
fn agent_presses(transcript: &EvalTranscript) -> Vec<(String, bool)> {
    let mut calls: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut presses: Vec<(String, bool)> = Vec::new();
    for step in &transcript.steps {
        match step {
            EvalStep::ToolCall { name, input } if name == lpa_agent::ACT_TOOL_NAME => {
                presses.push((
                    input["action"].as_str().unwrap_or_default().to_string(),
                    false,
                ));
                calls.push_back(presses.len() - 1);
            }
            EvalStep::ToolResult { name, content } if name == lpa_agent::ACT_TOOL_NAME => {
                if let Some(at) = calls.pop_front() {
                    presses[at].1 = serde_json::from_str::<Value>(content)
                        .is_ok_and(|result| result.get("done").is_some());
                }
            }
            _ => {}
        }
    }
    presses
}

/// Every card the agent handed over: its id and its offer.
fn handed_cards(transcript: &EvalTranscript) -> Vec<(String, Option<String>)> {
    transcript
        .steps
        .iter()
        .filter_map(|step| match step {
            EvalStep::CardHanded { card, offer, .. } => Some((card.clone(), offer.clone())),
            _ => None,
        })
        .collect()
}

fn assistant_texts(transcript: &EvalTranscript) -> Vec<&str> {
    transcript
        .steps
        .iter()
        .filter_map(|step| match step {
            EvalStep::Assistant { text } if !text.trim().is_empty() => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// A `ws281x:local:<label>` spec anywhere in a tool input.
fn pin_spec(value: &Value) -> Option<String> {
    find_spec(value, "ws281x:local:", |_| true)
}

/// A `ws281x:local:D<n>` spec anywhere in a tool input.
pub(crate) fn d_label_spec(value: &Value) -> Option<String> {
    find_spec(value, "ws281x:local:D", |rest| {
        rest.chars().next().is_some_and(|c| c.is_ascii_digit())
    })
}

fn find_spec(value: &Value, prefix: &str, rest_ok: impl Fn(&str) -> bool + Copy) -> Option<String> {
    match value {
        Value::String(text) => {
            let at = text.find(prefix)?;
            rest_ok(&text[at + prefix.len()..]).then(|| {
                text[at..]
                    .split(|c: char| c == '"' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
                    .to_string()
            })
        }
        Value::Array(items) => items
            .iter()
            .find_map(|item| find_spec(item, prefix, rest_ok)),
        Value::Object(map) => map
            .values()
            .find_map(|item| find_spec(item, prefix, rest_ok)),
        _ => None,
    }
}

fn short(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= 120 {
        return text.to_string();
    }
    let tail: String = text.chars().skip(text.chars().count() - 117).collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn call(name: &str, input: Value) -> EvalStep {
        EvalStep::ToolCall {
            name: name.into(),
            input,
        }
    }

    fn result(name: &str, content: Value) -> EvalStep {
        EvalStep::ToolResult {
            name: name.into(),
            content: content.to_string(),
        }
    }

    fn reply(about: &str) -> EvalStep {
        EvalStep::ScriptedReply {
            about: about.into(),
            text: "XIAO".into(),
        }
    }

    fn said(text: &str) -> EvalStep {
        EvalStep::Assistant { text: text.into() }
    }

    fn card(id: &str, offer: &str) -> EvalStep {
        EvalStep::CardHanded {
            card: id.into(),
            offer: Some(offer.into()),
            title: "Flash firmware".into(),
            destructive: true,
        }
    }

    fn transcript(steps: Vec<EvalStep>) -> EvalTranscript {
        EvalTranscript { steps }
    }

    #[test]
    fn a_d_label_written_before_the_board_reply_fails() {
        let before = transcript(vec![
            call(
                "edit_project",
                json!({"edits":[{"set":{"value":"ws281x:local:D6"}}]}),
            ),
            reply("board"),
        ]);
        let reason = no_d_label_before_board(&before).expect_err("guessed the board");
        assert!(reason.contains("ws281x:local:D6"), "{reason}");
        let after = transcript(before.steps.iter().rev().cloned().collect());
        no_d_label_before_board(&after).expect("asked first");
        asked_about(&after, "board").expect("asked");
        asked_about(&EvalTranscript::default(), "board").expect_err("never asked");
    }

    #[test]
    fn gpio_specs_are_not_d_labels_but_are_pins() {
        assert_eq!(d_label_spec(&json!("ws281x:local:GPIO16")), None);
        assert_eq!(d_label_spec(&json!("ws281x:local:Data")), None);
        assert_eq!(
            d_label_spec(&json!({"a":["x", "ws281x:local:D10"]})),
            Some("ws281x:local:D10".into())
        );
        assert_eq!(
            pin_spec(&json!({"endpoint": "ws281x:local:LED"})),
            Some("ws281x:local:LED".into())
        );
    }

    #[test]
    fn asked_before_holds_the_question_to_the_gate() {
        let pin = call(
            "edit_project",
            json!({"edits":[{"set":{"value":"ws281x:local:D5"}}]}),
        );
        let guessed = transcript(vec![pin.clone(), reply("board")]);
        let reason = asked_before(&guessed, "board", Gate::Pin).expect_err("pin first");
        assert!(reason.contains("ws281x:local:D5"), "{reason}");
        asked_before(
            &transcript(vec![reply("board"), pin.clone()]),
            "board",
            Gate::Pin,
        )
        .expect("asked first");
        asked_before(&transcript(vec![pin]), "board", Gate::Pin).expect_err("never asked");
        // A read names pins without writing them.
        let read = call("read", json!({"what": "node", "name": "ws281x:local:D5"}));
        asked_before(&transcript(vec![read, reply("board")]), "board", Gate::Pin)
            .expect("a read is not a write");
        let flash = call(
            "act",
            json!({"action": "devices/new-1/flash", "args": {"board": "x"}}),
        );
        asked_before(
            &transcript(vec![flash.clone(), reply("board")]),
            "board",
            Gate::Flash,
        )
        .expect_err("flashed first");
        asked_before(&transcript(vec![flash]), "board", Gate::Edit)
            .expect_err("never asked, even with no edit");
    }

    #[test]
    fn never_pressed_tells_a_press_from_a_card() {
        let flash = json!({"action": "devices/new-1/flash"});
        let carded = transcript(vec![
            call("act", flash.clone()),
            result("act", json!({"needs_user": {"card": "c1"}})),
            card("c1", "devices/new-1/flash"),
        ]);
        never(&carded, "pressed:devices/*/flash").expect("carded, not pressed");
        never(&carded, "act:devices/*/flash").expect_err("it did act");
        never(&carded, "card:devices/*/flash").expect_err("it handed the card");
        card_handed(&carded, "devices/*/flash").expect("handed");
        card_handed(&carded, "devices/*/update-firmware").expect_err("not that one");
        let pressed = transcript(vec![
            call("act", flash),
            result("act", json!({"done": "Flashing"})),
        ]);
        let reason = never(&pressed, "pressed:devices/*/flash").expect_err("pressed");
        assert!(reason.contains("devices/new-1/flash"), "{reason}");
        never(&pressed, "tool:edit_project").expect("no edits");
        never(&pressed, "tool:act").expect_err("acted");
    }

    #[test]
    fn question_counts_and_walls() {
        let q = |tail: &str, n| EvalStep::Question {
            tail: tail.into(),
            questions: n,
        };
        let run = transcript(vec![q("Which board?", 1), q("Which pin? How many?", 2)]);
        max_questions(&run, 2, None).expect("two");
        max_questions(&run, 1, None).expect_err("more than one");
        let reason = max_questions(&run, 5, Some(1)).expect_err("a wall");
        assert!(reason.contains("2 things at once"), "{reason}");
        max_questions(&EvalTranscript::default(), 0, Some(1)).expect("none");
    }

    #[test]
    fn said_any_and_none_read_the_agents_words() {
        let run = transcript(vec![
            said("LightPlayer can't schedule yet."),
            said("Want me to dim it instead?"),
        ]);
        said_any(&run, &["can't".into(), "not yet".into()], false).expect("honest");
        said_any(&run, &["can't".into()], true).expect_err("not in the last message");
        said_none(&run, &["scheduled".into(), "done!".into()]).expect("no claim");
        said_none(&run, &["SCHEDULE".into()]).expect_err("case-insensitive");
    }

    #[test]
    fn unscripted_questions_count_fallbacks_and_the_one_that_ended_the_run() {
        let run = transcript(vec![
            EvalStep::FallbackReply {
                question: "Colour?".into(),
                text: "you pick".into(),
            },
            EvalStep::Stopped {
                reason: "a second question the scenario has no reply for".into(),
            },
        ]);
        assert_eq!(unscripted_questions(&run), 2);
    }
}

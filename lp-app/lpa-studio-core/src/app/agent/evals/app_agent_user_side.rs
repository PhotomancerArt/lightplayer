//! The person's side of a scenario: what they say after each of the
//! agent's turns, and what they do with each card.
//!
//! Pure over text, so the rules are tested without a model:
//!
//! - **It asked** when the turn's last assistant text has a `?` in its
//!   final 300 characters.
//! - **A scripted reply** is picked when any of its `when` keywords starts
//!   a word in that question tail (case-insensitive); every reply the tail
//!   matches is given, joined, each used once. When the tail matches none,
//!   the whole message is tried, and the first match is given.
//! - **An unscripted question** gets the persona's `otherwise` once; a
//!   second one ends the run.
//! - **A turn without a question** gets the next follow-up (`then`), and
//!   with none left the conversation is over.
//! - **A card** whose offer matches a `[[user.card]]` glob gets that rule;
//!   any other gets `unlisted_cards`.

use std::collections::VecDeque;

use super::app_agent_scenario::{CardDo, CardRule, UserScript};

/// How far back from the end of a turn a `?` counts as the turn's question.
const QUESTION_TAIL_CHARS: usize = 300;

/// What the person does after one of the agent's turns.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UserMove {
    /// Scripted answers: `(topic, text)` for each reply used, and the
    /// message sent.
    Reply {
        replies: Vec<(String, String)>,
        text: String,
    },
    /// The `otherwise` answer to a question nobody scripted.
    Fallback { question: String, text: String },
    /// The next follow-up message.
    FollowUp { text: String },
    /// The conversation ends here, early (the reason goes in the report).
    Stop { reason: String },
    /// The person has nothing more to say.
    Done,
}

/// The person's state across a scenario: which replies are spent, whether
/// the fallback is, and the follow-ups still to send.
pub(crate) struct UserSide<'a> {
    script: &'a UserScript,
    used: Vec<bool>,
    fallback_used: bool,
    then: VecDeque<String>,
}

impl<'a> UserSide<'a> {
    pub(crate) fn new(script: &'a UserScript) -> Self {
        Self {
            script,
            used: vec![false; script.replies.len()],
            fallback_used: false,
            then: script.then.iter().cloned().collect(),
        }
    }

    /// The person's move after the agent said `said` (its turn's last
    /// assistant text).
    pub(crate) fn after_turn(&mut self, said: &str) -> UserMove {
        if !asks_a_question(said) {
            return match self.then.pop_front() {
                Some(text) => UserMove::FollowUp { text },
                None => UserMove::Done,
            };
        }
        let tail = question_tail(said);
        let mut picked: Vec<usize> = (0..self.script.replies.len())
            .filter(|at| !self.used[*at] && self.matches(*at, &tail))
            .collect();
        if picked.is_empty() {
            picked = (0..self.script.replies.len())
                .find(|at| !self.used[*at] && self.matches(*at, said))
                .into_iter()
                .collect();
        }
        if !picked.is_empty() {
            let replies: Vec<(String, String)> = picked
                .iter()
                .map(|at| {
                    self.used[*at] = true;
                    let reply = &self.script.replies[*at];
                    (reply.topic().to_string(), reply.text.clone())
                })
                .collect();
            let text = replies
                .iter()
                .map(|(_, text)| text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            return UserMove::Reply { replies, text };
        }
        match &self.script.otherwise {
            Some(text) if !self.fallback_used => {
                self.fallback_used = true;
                UserMove::Fallback {
                    question: tail,
                    text: text.clone(),
                }
            }
            Some(_) => UserMove::Stop {
                reason: "a second question the scenario has no reply for".to_string(),
            },
            None => UserMove::Stop {
                reason: "the agent asked a question the scenario has no reply for".to_string(),
            },
        }
    }

    /// What the person does with a card handing over `offer`.
    pub(crate) fn card_rule(&self, offer: Option<&str>) -> CardChoice {
        let rule: Option<&CardRule> = offer.and_then(|offer| {
            self.script
                .cards
                .iter()
                .find(|rule| glob_match(&rule.offer, offer))
        });
        match rule {
            Some(rule) => CardChoice {
                action: rule.action,
                args: rule.args.clone(),
            },
            None => CardChoice {
                action: self.script.unlisted_cards,
                args: Default::default(),
            },
        }
    }

    fn matches(&self, at: usize, text: &str) -> bool {
        self.script.replies[at]
            .when
            .iter()
            .any(|keyword| starts_a_word(text, keyword))
    }
}

/// A card rule, resolved for one card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CardChoice {
    pub(crate) action: CardDo,
    pub(crate) args: std::collections::BTreeMap<String, String>,
}

/// Whether the agent ended its turn on a question to the user.
pub(crate) fn asks_a_question(text: &str) -> bool {
    question_tail(text).contains('?')
}

/// The last [`QUESTION_TAIL_CHARS`] characters of `text`.
pub(crate) fn question_tail(text: &str) -> String {
    let trimmed = text.trim_end();
    let skip = trimmed.chars().count().saturating_sub(QUESTION_TAIL_CHARS);
    trimmed.chars().skip(skip).collect()
}

/// How many things a question turn asks at once: the `?`s in its tail.
pub(crate) fn questions_in(text: &str) -> usize {
    question_tail(text).matches('?').count()
}

/// `keyword` (any case) occurs in `text` at the start of a word: `pin`
/// matches "which pin" and "pins", not "spinning".
pub(crate) fn starts_a_word(text: &str, keyword: &str) -> bool {
    let text = text.to_lowercase();
    let keyword = keyword.to_lowercase();
    if keyword.is_empty() {
        return false;
    }
    text.match_indices(&keyword).any(|(at, _)| {
        text[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !before.is_alphanumeric())
    })
}

/// Whether offer `path` matches `pattern`: segments split on `/`, `*`
/// matches any run of characters within a segment, and a `**` segment
/// matches any number of segments.
pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    segments_match(&pattern, &path)
}

fn segments_match(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| segments_match(rest, &path[skip..])),
        Some((first, rest)) => match path.split_first() {
            Some((segment, path_rest)) => {
                segment_match(first, segment) && segments_match(rest, path_rest)
            }
            None => false,
        },
    }
}

fn segment_match(pattern: &str, segment: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == segment,
        Some((head, tail)) => {
            let Some(rest) = segment.strip_prefix(head) else {
                return false;
            };
            (0..=rest.len())
                .filter(|at| rest.is_char_boundary(*at))
                .any(|at| segment_match(tail, &rest[at..]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::agent::evals::app_agent_scenario::ScriptedReply;

    fn script() -> UserScript {
        toml::from_str(
            r#"
            say = "I have an esp32c6 board, leds on D5"
            then = ["now make it dimmer"]
            otherwise = "you decide."
            [[reply]]
            topic = "board"
            when = ["board", "which xiao"]
            text = "It's a XIAO ESP32-C6."
            [[reply]]
            when = ["pin", "gpio"]
            text = "D5."
            [[card]]
            offer = "devices/*/flash"
            do = "leave"
            [[card]]
            offer = "devices/**"
            args = { board = "quinled/dig2go" }
            "#,
        )
        .expect("the script parses")
    }

    #[test]
    fn a_question_gets_every_reply_its_tail_names_once() {
        let script = script();
        let mut user = UserSide::new(&script);
        let said = "Happy to help! Which board is it, and which pin are the LEDs on?";
        assert_eq!(
            user.after_turn(said),
            UserMove::Reply {
                replies: vec![
                    ("board".into(), "It's a XIAO ESP32-C6.".into()),
                    ("pin".into(), "D5.".into())
                ],
                text: "It's a XIAO ESP32-C6.\nD5.".into()
            }
        );
        // Both spent: the same question again is unscripted, answered once
        // with `otherwise`, and a second one ends the run.
        assert!(matches!(
            user.after_turn(said),
            UserMove::Fallback { ref text, .. } if text == "you decide."
        ));
        assert!(matches!(user.after_turn(said), UserMove::Stop { .. }));
    }

    #[test]
    fn a_keyword_must_start_a_word() {
        assert!(starts_a_word("Which PIN?", "pin"));
        assert!(starts_a_word("the pins?", "pin"));
        assert!(!starts_a_word("is it spinning?", "pin"));
        assert!(!starts_a_word("anything", ""));
        let script = script();
        let mut user = UserSide::new(&script);
        assert!(matches!(
            user.after_turn("Should the spiral keep spinning?"),
            UserMove::Fallback { .. }
        ));
    }

    #[test]
    fn the_tail_is_preferred_over_an_earlier_mention() {
        let script = script();
        let mut user = UserSide::new(&script);
        let said = format!(
            "Your board is a fine one. {} Which GPIO are the LEDs wired to?",
            "x".repeat(400)
        );
        let UserMove::Reply { replies, .. } = user.after_turn(&said) else {
            panic!("a reply");
        };
        assert_eq!(replies, [("pin".to_string(), "D5.".to_string())]);
    }

    #[test]
    fn a_turn_without_a_question_gets_the_follow_ups_then_ends() {
        let script = script();
        let mut user = UserSide::new(&script);
        assert_eq!(
            user.after_turn("Done: 60 LEDs on D5."),
            UserMove::FollowUp {
                text: "now make it dimmer".into()
            }
        );
        assert_eq!(user.after_turn("Dimmed."), UserMove::Done);
        // A question mark long before the end is not the turn's question.
        let early = format!("Did it work? {}", "and then some. ".repeat(40));
        assert!(!asks_a_question(&early));
    }

    #[test]
    fn no_otherwise_means_an_unscripted_question_ends_the_run() {
        let script = UserScript {
            otherwise: None,
            replies: vec![ScriptedReply {
                topic: None,
                when: vec!["board".into()],
                text: "XIAO".into(),
            }],
            ..script()
        };
        let mut user = UserSide::new(&script);
        assert!(matches!(
            user.after_turn("What colour?"),
            UserMove::Stop { .. }
        ));
    }

    #[test]
    fn cards_follow_the_first_matching_rule_else_the_default() {
        let script = script();
        let user = UserSide::new(&script);
        assert_eq!(
            user.card_rule(Some("devices/new-1/flash")).action,
            CardDo::Leave
        );
        let push = user.card_rule(Some("devices/mac-6055f90a0b0c/push"));
        assert_eq!(push.action, CardDo::Click);
        assert_eq!(push.args["board"], "quinled/dig2go");
        assert_eq!(user.card_rule(Some("project/revert")).action, CardDo::Click);
        assert_eq!(user.card_rule(None).action, CardDo::Click);
    }

    #[test]
    fn globs_match_within_and_across_segments() {
        assert!(glob_match("devices/*/flash", "devices/new-1/flash"));
        assert!(glob_match("devices/new-*/flash", "devices/new-12/flash"));
        assert!(!glob_match("devices/*/flash", "devices/new-1/push"));
        assert!(!glob_match("devices/*", "devices/new-1/flash"));
        assert!(glob_match("devices/**", "devices/new-1/flash"));
        assert!(glob_match("**/flash", "devices/new-1/flash"));
        assert!(glob_match("devices/connect-usb", "devices/connect-usb"));
        assert!(glob_match("devices/*-firmware", "devices/update-firmware"));
    }

    #[test]
    fn questions_in_counts_a_wall() {
        assert_eq!(questions_in("Which board? How many LEDs? Which pin?"), 3);
        assert_eq!(questions_in("Done."), 0);
    }
}

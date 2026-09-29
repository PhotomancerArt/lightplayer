//! `lp-cli link capture --request`: the requests a capture sends, and when.
//!
//! A request goes only into a session the board has said hello in, one at a
//! time, each after the one before is done. A request is done when its answer
//! arrives — except a `reboot`, which is done when the board has RESTARTED:
//! the board answers it and resets a moment later (once the host has
//! acknowledged the answer, or after a second), so a request sent on the
//! answer would reach the old boot or be lost with it. After a restart the
//! next request waits for the new session's hello. That order is what makes
//! `--request reboot --request hello` ask the REBOOTED board. Any other
//! request whose session resets under it is done too, unanswered, and says
//! so.
//!
//! The queue reads the capture's console lines, not typed messages, the same
//! way `lp-cli emu run --host-link --request` does: a board on another proto
//! still answers by id, and a line is what both doors already have.

use anyhow::{Result, bail};
use lpc_wire::{ClientMessage, ClientRequest};

/// First id of the `--request` requests: far from any a client counts, and
/// the same base `lp-cli emu run --host-link --request` uses.
pub const REQUEST_ID_BASE: u64 = 1_000_000;

/// The capture's requests, in order, and where each one is.
#[derive(Debug)]
pub struct CaptureRequests {
    requests: Vec<(String, ClientMessage)>,
    /// How many have been sent.
    sent: usize,
    /// The request sent and not yet done.
    in_flight: Option<InFlight>,
    /// The current session's hello has arrived.
    hello: bool,
    /// How each finished request ended.
    done: Vec<Done>,
}

#[derive(Debug, Clone, Copy)]
struct InFlight {
    index: usize,
    answered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Done {
    Answered,
    /// A `reboot` whose board restarted (a new session under a new nonce).
    Restarted,
    /// Any other request whose session reset before it was answered.
    EndedByReset,
}

impl CaptureRequests {
    /// Parse every `--request` up front, so a typo fails before the port is
    /// opened rather than after the board has booted.
    pub fn parse(texts: &[String]) -> Result<Self> {
        let mut requests = Vec::with_capacity(texts.len());
        for (i, text) in texts.iter().enumerate() {
            let request = parse_request(text)?;
            let id = REQUEST_ID_BASE + i as u64;
            requests.push((text.clone(), ClientMessage { id, msg: request }));
        }
        Ok(CaptureRequests {
            requests,
            sent: 0,
            in_flight: None,
            hello: false,
            done: Vec::new(),
        })
    }

    /// The next request to send now, if one is due. Call [`Self::sent`] once
    /// the link took it.
    pub fn due(&self) -> Option<&ClientMessage> {
        if !self.hello || self.in_flight.is_some() {
            return None;
        }
        self.requests.get(self.sent).map(|(_, message)| message)
    }

    /// The request [`Self::due`] returned was queued on the link.
    pub fn sent(&mut self) {
        let (text, message) = &self.requests[self.sent];
        eprintln!("link capture: sent --request `{text}` as id {}", message.id);
        self.in_flight = Some(InFlight {
            index: self.sent,
            answered: false,
        });
        self.sent += 1;
    }

    /// A console line the capture wrote: a hello opens the session to
    /// requests, a reply with the in-flight id answers it.
    pub fn on_line(&mut self, line: &str) {
        let Some(json) = line.strip_prefix("M!") else {
            return;
        };
        if json.contains("\"hello\":{") {
            self.hello = true;
        }
        let Some(flight) = self.in_flight else {
            return;
        };
        let (text, message) = &self.requests[flight.index];
        if flight.answered || !json.starts_with(&format!("{{\"id\":{},", message.id)) {
            return;
        }
        if is_reboot(message) {
            eprintln!(
                "link capture: the board answered --request `{text}`; \
                 the next request waits for it to restart"
            );
            self.in_flight = Some(InFlight {
                answered: true,
                ..flight
            });
        } else {
            self.finish(Done::Answered);
        }
    }

    /// The link came up or reset: a new session, with no hello yet. A
    /// `reboot` in flight is done (the board restarted); any other request
    /// in flight was lost with the old session.
    pub fn on_session_change(&mut self) {
        self.hello = false;
        let Some(flight) = self.in_flight else {
            return;
        };
        let (text, message) = &self.requests[flight.index];
        if is_reboot(message) {
            self.finish(Done::Restarted);
        } else {
            eprintln!(
                "link capture: the link reset before --request `{text}` was answered; \
                 the next request waits for the new session's hello"
            );
            self.finish(Done::EndedByReset);
        }
    }

    /// Why the run's requests are not all done, if they are not.
    pub fn unfinished(&self) -> Option<String> {
        if let Some(flight) = self.in_flight {
            let text = &self.requests[flight.index].0;
            return Some(if flight.answered {
                format!(
                    "the board answered --request `{text}` but did not restart before the \
                     capture ended"
                )
            } else {
                format!("no answer to --request `{text}` before the capture ended")
            });
        }
        if self.sent < self.requests.len() {
            let unsent: Vec<&str> = self.requests[self.sent..]
                .iter()
                .map(|(text, _)| text.as_str())
                .collect();
            return Some(format!(
                "--request {} never sent: the board said no hello to send {} into",
                unsent.join(", "),
                if unsent.len() == 1 { "it" } else { "them" }
            ));
        }
        None
    }

    /// One line for the run's summary: how many were sent, and how each
    /// ended.
    pub fn describe(&self) -> Option<String> {
        if self.requests.is_empty() {
            return None;
        }
        let count = |kind: Done| self.done.iter().filter(|d| **d == kind).count();
        Some(format!(
            "{} of {} request(s) sent: {} answered, {} restarted the board, {} ended by a \
             link reset",
            self.sent,
            self.requests.len(),
            count(Done::Answered),
            count(Done::Restarted),
            count(Done::EndedByReset),
        ))
    }

    fn finish(&mut self, how: Done) {
        self.in_flight = None;
        self.done.push(how);
    }
}

fn is_reboot(message: &ClientMessage) -> bool {
    matches!(message.msg, ClientRequest::Reboot)
}

/// A `ClientRequest`'s JSON, or a unit request's bare name (`reboot` for
/// `"reboot"`), because a shell makes the quoted form awkward to type.
fn parse_request(text: &str) -> Result<ClientRequest> {
    let error = match serde_json::from_str::<ClientRequest>(text) {
        Ok(request) => return Ok(request),
        Err(error) => error,
    };
    let bare = text.trim();
    if !bare.is_empty()
        && bare.chars().all(|c| c.is_ascii_alphanumeric())
        && let Ok(request) = serde_json::from_str::<ClientRequest>(&format!("\"{bare}\""))
    {
        return Ok(request);
    }
    bail!(
        "--request `{text}` is not a ClientRequest ({error}); \
         a unit request is its camelCase name, e.g. `reboot` or `'\"stopAllProjects\"'`"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = "M!{\"id\":0,\"msg\":{\"hello\":{\"proto\":32}}}";

    #[test]
    fn nothing_is_due_before_the_hello() {
        let q = CaptureRequests::parse(&["reboot".into()]).unwrap();
        assert!(q.due().is_none());
    }

    #[test]
    fn a_bare_name_and_its_json_are_the_same_request() {
        let q = CaptureRequests::parse(&["reboot".into(), "\"reboot\"".into()]).unwrap();
        for (_, message) in &q.requests {
            assert!(matches!(message.msg, ClientRequest::Reboot));
        }
    }

    #[test]
    fn a_request_that_is_not_one_fails_before_anything_opens() {
        let error = CaptureRequests::parse(&["rebot".into()]).unwrap_err();
        assert!(format!("{error:#}").contains("rebot"), "{error:#}");
    }

    #[test]
    fn requests_go_one_at_a_time_each_after_its_answer() {
        let mut q = CaptureRequests::parse(&["hello".into(), "listLoadedProjects".into()]).unwrap();
        q.on_line("[link] up (session 1)");
        q.on_line(HELLO);
        assert_eq!(q.due().unwrap().id, REQUEST_ID_BASE);
        q.sent();
        assert!(q.due().is_none(), "the second waits for the first's answer");
        q.on_line("M!{\"id\":1000000,\"msg\":{\"hello\":{\"proto\":32}}}");
        assert_eq!(q.due().unwrap().id, REQUEST_ID_BASE + 1);
        q.sent();
        q.on_line("M!{\"id\":1000001,\"msg\":{\"loadedProjects\":[]}}");
        assert!(q.due().is_none());
        assert_eq!(q.unfinished(), None);
        assert_eq!(
            q.describe().unwrap(),
            "2 of 2 request(s) sent: 2 answered, 0 restarted the board, 0 ended by a link reset"
        );
    }

    #[test]
    fn after_a_reboot_the_next_request_waits_for_the_restart_and_the_new_hello() {
        let mut q = CaptureRequests::parse(&["reboot".into(), "hello".into()]).unwrap();
        q.on_line(HELLO);
        q.sent();
        // The answer alone is not enough: the old boot is still up for a
        // moment, and a request sent now would reach it.
        q.on_line("M!{\"id\":1000000,\"msg\":\"reboot\"}");
        assert!(
            q.due().is_none(),
            "a reboot is done when the board restarts"
        );
        q.on_session_change(); // reset (PeerRestarted)
        q.on_session_change(); // up (session 1)
        assert!(q.due().is_none(), "no hello in the new session yet");
        q.on_line(HELLO);
        assert_eq!(q.due().unwrap().id, REQUEST_ID_BASE + 1);
        q.sent();
        q.on_line("M!{\"id\":1000001,\"msg\":{\"hello\":{\"proto\":32}}}");
        assert_eq!(q.unfinished(), None);
        assert_eq!(
            q.describe().unwrap(),
            "2 of 2 request(s) sent: 1 answered, 1 restarted the board, 0 ended by a link reset"
        );
    }

    #[test]
    fn a_reboot_whose_answer_was_lost_with_the_session_still_restarted_the_board() {
        let mut q = CaptureRequests::parse(&["reboot".into()]).unwrap();
        q.on_line(HELLO);
        q.sent();
        q.on_session_change();
        assert_eq!(q.unfinished(), None);
    }

    #[test]
    fn a_reboot_answered_but_never_followed_by_a_restart_is_unfinished() {
        let mut q = CaptureRequests::parse(&["reboot".into()]).unwrap();
        q.on_line(HELLO);
        q.sent();
        q.on_line("M!{\"id\":1000000,\"msg\":\"reboot\"}");
        assert!(q.unfinished().unwrap().contains("did not restart"));
    }

    #[test]
    fn any_other_request_whose_session_resets_under_it_ends_unanswered() {
        let mut q = CaptureRequests::parse(&["hello".into(), "hello".into()]).unwrap();
        q.on_line(HELLO);
        q.sent();
        q.on_session_change();
        assert!(q.due().is_none());
        q.on_line(HELLO);
        assert_eq!(q.due().unwrap().id, REQUEST_ID_BASE + 1);
        assert_eq!(
            q.describe().unwrap(),
            "1 of 2 request(s) sent: 0 answered, 0 restarted the board, 1 ended by a link reset"
        );
    }

    #[test]
    fn an_unsent_or_unanswered_request_is_unfinished() {
        let mut q = CaptureRequests::parse(&["hello".into()]).unwrap();
        assert!(q.unfinished().unwrap().contains("never sent"));
        q.on_line(HELLO);
        q.sent();
        assert!(q.unfinished().unwrap().contains("no answer"));
    }

    #[test]
    fn a_line_that_only_mentions_the_id_is_not_its_answer() {
        let mut q = CaptureRequests::parse(&["hello".into()]).unwrap();
        q.on_line(HELLO);
        q.sent();
        q.on_line("[INFO] id 1000000 was asked for");
        q.on_line("M!{\"id\":10000001,\"msg\":{}}");
        assert!(q.unfinished().is_some());
    }
}

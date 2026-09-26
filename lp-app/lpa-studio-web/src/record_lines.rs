//! The session recorder's sink-only lines: raw transport bytes (`wire`)
//! and client requests (`request`). Pure (host-tested); the browser half,
//! which installs the taps and stamps the time, is `device_events_io.rs`.
//!
//! These lines go to the `?record=` sink and nowhere else — not the
//! 2000-record device event ring and not the localStorage copy, which
//! they would flood (the editor lens alone asks every 150 ms). They take
//! the same `seq` as every other line, so a recording's order is total.
//!
//! # Shapes
//!
//! ```text
//! {"t":…,"kind":"wire","dir":"tx"|"rx","transport":"serial"|"ble"|"emu-tab","port":"3","len":N,"b64":"…"}
//! {"t":…,"kind":"request","phase":"sent","conversation":C,"id":N,"request":"project.read"}
//! {"t":…,"kind":"request","phase":"frame","conversation":C,"id":N,"response_id":M,"frame_seq":S,"fin":B,"disposition":"matched"|"stale"|"prior-owner"|"uncorrelated"|"server-originated","since_sent_ms"?:F}
//! {"t":…,"kind":"request","phase":"outcome","conversation":C,"id":N,"request"?:"…","outcome":"answered"|"failed"|"timed-out"|"cancelled","latency_ms"?:F,"error"?:"…","budget_ms"?:F}
//! ```
//!
//! `(conversation, id)` names one request (ids are per client). A request
//! whose future was dropped mid-flight has a `sent` and no `outcome`.

#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "read by the wasm recorder; host builds only run the unit tests"
    )
)]

use std::collections::{HashMap, VecDeque};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use lpa_client::{ClientObservation, RequestOutcome};
use serde_json::{Map, Value, json};

/// Requests in flight the tracker remembers; past it the oldest is
/// forgotten (a request whose future was dropped never reports an outcome).
const MAX_IN_FLIGHT: usize = 256;

/// A `wire` line for one raw chunk.
pub fn wire_line(t: f64, dir: &str, transport: &str, port: u32, bytes: &[u8]) -> String {
    json!({
        "t": t,
        "kind": "wire",
        "dir": dir,
        "transport": transport,
        "port": port.to_string(),
        "len": bytes.len(),
        "b64": STANDARD.encode(bytes),
    })
    .to_string()
}

/// Turns client observations into `request` lines, remembering when each
/// request went out so its frames and outcome carry a latency.
#[derive(Default)]
pub struct RequestLines {
    /// `(conversation, id)` → (sent at, request kind).
    sent: HashMap<(u64, u64), (f64, &'static str)>,
    /// Insertion order, for the [`MAX_IN_FLIGHT`] bound.
    order: VecDeque<(u64, u64)>,
}

impl RequestLines {
    pub fn new() -> Self {
        Self::default()
    }

    /// The line for one observation, stamped `t` (seconds).
    pub fn line(&mut self, t: f64, observation: &ClientObservation) -> String {
        let key = (observation.conversation(), observation.id());
        let mut line = Map::new();
        line.insert("t".into(), json!(t));
        line.insert("kind".into(), json!("request"));
        match observation {
            ClientObservation::Sent { kind, .. } => {
                self.remember(key, t, kind);
                line.insert("phase".into(), json!("sent"));
                insert_key(&mut line, key);
                line.insert("request".into(), json!(kind));
            }
            ClientObservation::Frame {
                response_id,
                seq,
                fin,
                disposition,
                ..
            } => {
                line.insert("phase".into(), json!("frame"));
                insert_key(&mut line, key);
                line.insert("response_id".into(), json!(response_id));
                line.insert("frame_seq".into(), json!(seq));
                line.insert("fin".into(), json!(fin));
                line.insert("disposition".into(), json!(disposition.as_str()));
                if let Some((sent, _)) = self.sent.get(&key) {
                    line.insert("since_sent_ms".into(), json!(ms_between(*sent, t)));
                }
            }
            ClientObservation::Outcome { outcome, .. } => {
                let sent = self.sent.remove(&key);
                if sent.is_some() {
                    self.order.retain(|entry| *entry != key);
                }
                line.insert("phase".into(), json!("outcome"));
                insert_key(&mut line, key);
                if let Some((_, kind)) = sent {
                    line.insert("request".into(), json!(kind));
                }
                line.insert("outcome".into(), json!(outcome.as_str()));
                if let Some((sent, _)) = sent {
                    line.insert("latency_ms".into(), json!(ms_between(sent, t)));
                }
                match outcome {
                    RequestOutcome::Failed { error } => {
                        line.insert("error".into(), json!(error));
                    }
                    RequestOutcome::TimedOut { budget } => {
                        line.insert("budget_ms".into(), json!(budget.as_secs_f64() * 1000.0));
                    }
                    RequestOutcome::Answered | RequestOutcome::Cancelled => {}
                }
            }
        }
        Value::Object(line).to_string()
    }

    fn remember(&mut self, key: (u64, u64), t: f64, kind: &'static str) {
        if self.sent.insert(key, (t, kind)).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > MAX_IN_FLIGHT {
            if let Some(oldest) = self.order.pop_front() {
                self.sent.remove(&oldest);
            }
        }
    }
}

fn insert_key(line: &mut Map<String, Value>, (conversation, id): (u64, u64)) {
    line.insert("conversation".into(), json!(conversation));
    line.insert("id".into(), json!(id));
}

/// Milliseconds from `from` to `to` (both seconds), to a microsecond.
fn ms_between(from: f64, to: f64) -> f64 {
    ((to - from) * 1_000_000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use lpa_client::FrameDisposition;

    use super::*;

    fn parse(line: &str) -> Value {
        serde_json::from_str(line).unwrap()
    }

    #[test]
    fn a_wire_line_carries_the_chunk_as_base64() {
        let line = parse(&wire_line(12.5, "rx", "serial", 3, b"M!{}\n"));
        assert_eq!(line["t"], 12.5);
        assert_eq!(line["kind"], "wire");
        assert_eq!(line["dir"], "rx");
        assert_eq!(line["transport"], "serial");
        assert_eq!(line["port"], "3");
        assert_eq!(line["len"], 5);
        assert_eq!(line["b64"], "TSF7fQo=");
    }

    #[test]
    fn a_request_reads_sent_frame_outcome_with_its_latency() {
        let mut lines = RequestLines::new();
        let sent = parse(&lines.line(
            10.0,
            &ClientObservation::Sent {
                conversation: 2,
                id: 7,
                kind: "project.read",
            },
        ));
        assert_eq!(sent["kind"], "request");
        assert_eq!(sent["phase"], "sent");
        assert_eq!(sent["conversation"], 2);
        assert_eq!(sent["id"], 7);
        assert_eq!(sent["request"], "project.read");

        let frame = parse(&lines.line(
            10.25,
            &ClientObservation::Frame {
                conversation: 2,
                id: 7,
                response_id: 7,
                seq: 1,
                fin: false,
                disposition: FrameDisposition::Matched,
            },
        ));
        assert_eq!(frame["phase"], "frame");
        assert_eq!(frame["response_id"], 7);
        assert_eq!(frame["frame_seq"], 1);
        assert_eq!(frame["fin"], false);
        assert_eq!(frame["disposition"], "matched");
        assert_eq!(frame["since_sent_ms"], 250.0);

        let outcome = parse(&lines.line(
            10.5,
            &ClientObservation::Outcome {
                conversation: 2,
                id: 7,
                outcome: RequestOutcome::Failed {
                    error: "protocol error: expected project read frame seq 0, got 1".into(),
                },
            },
        ));
        assert_eq!(outcome["phase"], "outcome");
        assert_eq!(outcome["request"], "project.read");
        assert_eq!(outcome["outcome"], "failed");
        assert_eq!(outcome["latency_ms"], 500.0);
        assert_eq!(
            outcome["error"],
            "protocol error: expected project read frame seq 0, got 1"
        );
    }

    #[test]
    fn two_clients_request_ones_are_timed_apart() {
        let mut lines = RequestLines::new();
        let sent = |conversation| ClientObservation::Sent {
            conversation,
            id: 1,
            kind: "hello",
        };
        lines.line(1.0, &sent(1));
        lines.line(2.0, &sent(2));
        let timed_out = parse(&lines.line(
            6.0,
            &ClientObservation::Outcome {
                conversation: 1,
                id: 1,
                outcome: RequestOutcome::TimedOut {
                    budget: Duration::from_secs(5),
                },
            },
        ));
        assert_eq!(timed_out["outcome"], "timed-out");
        assert_eq!(timed_out["latency_ms"], 5000.0);
        assert_eq!(timed_out["budget_ms"], 5000.0);
        let answered = parse(&lines.line(
            2.5,
            &ClientObservation::Outcome {
                conversation: 2,
                id: 1,
                outcome: RequestOutcome::Answered,
            },
        ));
        assert_eq!(answered["latency_ms"], 500.0);
        assert!(answered.get("error").is_none());
    }

    #[test]
    fn an_outcome_with_no_sent_has_no_latency_and_the_tracker_stays_bounded() {
        let mut lines = RequestLines::new();
        let orphan = parse(&lines.line(
            1.0,
            &ClientObservation::Outcome {
                conversation: 1,
                id: 1,
                outcome: RequestOutcome::Cancelled,
            },
        ));
        assert_eq!(orphan["outcome"], "cancelled");
        assert!(orphan.get("latency_ms").is_none());
        assert!(orphan.get("request").is_none());

        for id in 0..(MAX_IN_FLIGHT as u64 + 10) {
            lines.line(
                1.0,
                &ClientObservation::Sent {
                    conversation: 9,
                    id,
                    kind: "hello",
                },
            );
        }
        assert_eq!(lines.sent.len(), MAX_IN_FLIGHT);
        assert_eq!(lines.order.len(), MAX_IN_FLIGHT);
    }
}

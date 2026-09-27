//! The delivery property, checked as messages arrive (one checker per
//! direction):
//!
//! > Every message sent on a reliable channel is delivered exactly once, in
//! > order, or the link reports a reset.
//!
//! "In order" is per channel: a higher-priority channel's message may
//! overtake another channel's (control goes before the rest of a big proto
//! reply), never one of its own. Concretely, for each sender generation (the
//! span between two resets) and each channel: the messages delivered are
//! exactly its first *k* messages on that channel, in send order, each once,
//! all inside one receiver session. Messages of a generation that
//! ended in a reset may be missing (that is what the reset reports). The
//! scenario then checks liveness: once faults stop, the last generation's
//! messages all arrive.

use std::collections::BTreeMap;
use std::format;
use std::string::String;
use std::vec::Vec;

use crate::Micros;
use crate::sim::probe_message::Probe;

/// Identifies a receiver session: (receiver incarnation, receiver generation).
pub type RxSession = (u16, u32);

#[derive(Default)]
pub struct Checker {
    /// (sender incarnation, sender generation, channel) → (next index,
    /// receiver session).
    streams: BTreeMap<(u16, u32, u8), (u32, RxSession)>,
    pub delivered: u64,
    pub delivered_bytes: u64,
    /// Delivered before the fault window closed (for goodput).
    pub delivered_bytes_in_window: u64,
    pub latencies: Vec<Micros>,
    /// Messages whose body did not match: damage the checksum missed.
    pub undetected_damage: u64,
    pub logs: u64,
    pub log_drop_notices: u64,
    pub logs_reported_dropped: u64,
    pub violations: Vec<String>,
}

impl Checker {
    pub fn on_reliable(
        &mut self,
        now: Micros,
        window_end: Micros,
        rx: RxSession,
        channel: u8,
        data: &[u8],
    ) {
        let Some(p) = Probe::decode(data) else {
            self.undetected_damage += 1;
            self.violations.push(format!(
                "t={now}: a damaged message was delivered ({} bytes)",
                data.len()
            ));
            return;
        };
        let stream = (p.inc, p.gen_, channel);
        let entry = self.streams.entry(stream).or_insert((0, rx));
        if entry.1 != rx {
            self.violations.push(format!(
                "t={now}: sender generation {stream:?} delivered in two receiver sessions ({:?}, {:?})",
                entry.1, rx
            ));
        }
        if p.idx != entry.0 {
            let what = if p.idx < entry.0 { "duplicate" } else { "gap" };
            self.violations.push(format!(
                "t={now}: {what}: generation {stream:?} expected #{}, got #{}",
                entry.0, p.idx
            ));
        }
        entry.0 = entry.0.max(p.idx + 1);
        self.delivered += 1;
        self.delivered_bytes += data.len() as u64;
        if now <= window_end {
            self.delivered_bytes_in_window += data.len() as u64;
            self.latencies.push(now - p.sent_at);
        }
    }

    /// A log record: `level ‖ text`. Level 0 is the ring's "n dropped" notice.
    pub fn on_log(&mut self, data: &[u8]) {
        match data.first() {
            Some(0) => {
                self.log_drop_notices += 1;
                let text = core::str::from_utf8(&data[1..]).unwrap_or("");
                let n = text.split(' ').next().and_then(|n| n.parse::<u64>().ok());
                self.logs_reported_dropped += n.unwrap_or(0);
            }
            Some(_) => self.logs += 1,
            None => {}
        }
    }

    /// Messages of `(inc, gen)` on `channel` delivered so far.
    pub fn delivered_of(&self, inc: u16, gen_: u32, channel: u8) -> u32 {
        self.streams.get(&(inc, gen_, channel)).map_or(0, |e| e.0)
    }
}

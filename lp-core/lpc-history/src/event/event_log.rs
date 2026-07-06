//! JSONL persistence for history events (`events.jsonl` in a history root).
//!
//! Append-only in spirit: `LpFs` has no append primitive, so append is
//! read-extend-write — fine at this scale (KB-sized logs).
//!
//! Torn-tail tolerance: a *final* line that fails to parse is dropped with a
//! warning (interrupted write); a malformed line anywhere earlier is
//! corruption and errors.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpfs::{FsError, LpFs, LpPath};

use crate::event::history_event::HistoryEvent;
use crate::history_error::HistoryError;

/// Path of the event log inside a project's history root.
pub const EVENT_LOG_PATH: &str = "/events.jsonl";

/// The JSONL event log of one project.
pub struct EventLog<'a> {
    fs: &'a dyn LpFs,
}

impl<'a> EventLog<'a> {
    pub fn new(fs: &'a dyn LpFs) -> Self {
        Self { fs }
    }

    fn read_bytes(&self) -> Result<Vec<u8>, HistoryError> {
        match self.fs.read_file(LpPath::new(EVENT_LOG_PATH)) {
            Ok(bytes) => Ok(bytes),
            Err(FsError::NotFound(_)) => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn append(&self, event: &HistoryEvent) -> Result<(), HistoryError> {
        let mut bytes = self.read_bytes()?;
        let line = serde_json::to_vec(event).map_err(|e| HistoryError::Encode(e.to_string()))?;
        bytes.extend_from_slice(&line);
        bytes.push(b'\n');
        self.fs.write_file(LpPath::new(EVENT_LOG_PATH), &bytes)?;
        Ok(())
    }

    pub fn read_all(&self) -> Result<Vec<HistoryEvent>, HistoryError> {
        let bytes = self.read_bytes()?;
        let lines: Vec<(usize, &[u8])> = bytes
            .split(|&b| b == b'\n')
            .enumerate()
            .filter(|(_, line)| !line.is_empty())
            .collect();
        let mut events = Vec::with_capacity(lines.len());
        let last = lines.len().saturating_sub(1);
        for (i, (line_no, line)) in lines.iter().enumerate() {
            match serde_json::from_slice::<HistoryEvent>(line) {
                Ok(event) => events.push(event),
                Err(_) if i == last => {
                    log::warn!("dropping torn tail line {} of event log", line_no + 1);
                }
                Err(_) => return Err(HistoryError::MalformedEventLog { line: line_no + 1 }),
            }
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::history_event::EventKind;
    use crate::hash::content_hash::ContentHash;
    use lpfs::LpFsMemory;

    fn saved(at: f64, data: &[u8]) -> HistoryEvent {
        HistoryEvent {
            at,
            kind: EventKind::Saved {
                version: ContentHash::of(data),
            },
        }
    }

    #[test]
    fn append_read_round_trip() {
        let fs = LpFsMemory::new();
        let event_log = EventLog::new(&fs);
        assert!(event_log.read_all().unwrap().is_empty());

        let a = saved(1.0, b"a");
        let b = saved(2.0, b"b");
        event_log.append(&a).unwrap();
        event_log.append(&b).unwrap();
        assert_eq!(event_log.read_all().unwrap(), [a, b]);
    }

    #[test]
    fn torn_tail_is_dropped() {
        let fs = LpFsMemory::new();
        let event_log = EventLog::new(&fs);
        event_log.append(&saved(1.0, b"a")).unwrap();

        let mut bytes = fs.read_file(LpPath::new(EVENT_LOG_PATH)).unwrap();
        bytes.extend_from_slice(b"{\"at\":2.0,\"kind\":{\"Sav");
        fs.write_file(LpPath::new(EVENT_LOG_PATH), &bytes).unwrap();

        let events = event_log.read_all().unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn mid_file_garbage_errors() {
        let fs = LpFsMemory::new();
        let event_log = EventLog::new(&fs);
        event_log.append(&saved(1.0, b"a")).unwrap();

        let mut bytes = b"garbage\n".to_vec();
        bytes.extend_from_slice(&fs.read_file(LpPath::new(EVENT_LOG_PATH)).unwrap());
        fs.write_file(LpPath::new(EVENT_LOG_PATH), &bytes).unwrap();

        assert!(matches!(
            event_log.read_all(),
            Err(HistoryError::MalformedEventLog { line: 1 })
        ));
    }
}

//! JSONL perf event sink (`events.jsonl`).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::profile::{Collector, FinishCtx, PerfEvent};

pub struct EventsCollector {
    writer: Option<BufWriter<File>>,
    path: PathBuf,
    count: u64,
}

impl EventsCollector {
    pub fn new(trace_dir: &Path) -> std::io::Result<Self> {
        let path = trace_dir.join("events.jsonl");
        let file = File::create(&path)?;
        Ok(Self {
            writer: Some(BufWriter::new(file)),
            path,
            count: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn count(&self) -> u64 {
        self.count
    }
}

impl Collector for EventsCollector {
    fn name(&self) -> &'static str {
        "events"
    }

    fn report_title(&self) -> &'static str {
        "Perf Events"
    }

    fn meta_json(&self) -> serde_json::Value {
        serde_json::json!({ "path": "events.jsonl", "format": "jsonl" })
    }

    fn on_perf_event(&mut self, evt: &PerfEvent) {
        if let Some(w) = self.writer.as_mut() {
            let line = serde_json::json!({
                "cycle": evt.cycle,
                "name":  evt.name,
                "kind":  evt.kind.as_str(),
            });
            // Best-effort write; log on error.
            match serde_json::to_writer(&mut *w, &line) {
                Ok(()) => match w.write_all(b"\n") {
                    Ok(()) => self.count += 1,
                    Err(e) => log::warn!("EventsCollector write failed: {e}"),
                },
                Err(e) => log::warn!("EventsCollector write failed: {e}"),
            }
        }
    }

    fn finish(&mut self, _ctx: &FinishCtx) -> std::io::Result<()> {
        if let Some(mut w) = self.writer.take() {
            w.flush()?;
        }
        Ok(())
    }

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "events written: {}", self.count)?;
        writeln!(w, "path: events.jsonl")
    }

    fn event_count(&self) -> u64 {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::profile::events::EventsCollector;
    use crate::profile::{Collector, FinishCtx, PerfEvent, PerfEventKind};

    #[test]
    fn new_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ec = EventsCollector::new(tmp.path()).unwrap();
        assert_eq!(ec.path(), tmp.path().join("events.jsonl"));
        assert!(ec.path().exists());
    }

    #[test]
    fn on_perf_event_writes_line_and_increments_count() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ec = EventsCollector::new(tmp.path()).unwrap();
        let evt = PerfEvent {
            cycle: 42,
            name: "frame",
            kind: PerfEventKind::Begin,
        };
        ec.on_perf_event(&evt);
        assert_eq!(ec.count(), 1);
        let ctx = FinishCtx {
            trace_dir: tmp.path(),
        };
        ec.finish(&ctx).unwrap();
        let text = fs::read_to_string(ec.path()).unwrap();
        assert!(text.contains("42"));
        assert!(text.contains("frame"));
        assert!(text.contains("\"kind\":\"B\""));
    }

    #[test]
    fn finish_flushes_so_content_visible() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ec = EventsCollector::new(tmp.path()).unwrap();
        ec.on_perf_event(&PerfEvent {
            cycle: 1,
            name: "frame",
            kind: PerfEventKind::Instant,
        });
        let ctx = FinishCtx {
            trace_dir: tmp.path(),
        };
        ec.finish(&ctx).unwrap();
        let text = fs::read_to_string(tmp.path().join("events.jsonl")).unwrap();
        assert!(!text.is_empty());
    }
}

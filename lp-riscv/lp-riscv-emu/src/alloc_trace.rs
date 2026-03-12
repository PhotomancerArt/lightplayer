//! Allocation tracing for memory debugging.
//!
//! Records every heap alloc/dealloc/realloc event from the guest as JSON Lines,
//! with full stack backtraces captured from the host side.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use serde::Serialize;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Metadata written to `meta.json` at the start of a trace session.
#[derive(Debug, Serialize)]
pub struct TraceMetadata {
    pub version: u32,
    pub timestamp: String,
    pub project: String,
    pub frames_requested: u32,
    pub heap_start: u32,
    pub heap_size: u32,
    pub symbols: Vec<TraceSymbol>,
}

/// A symbol entry in the trace metadata.
#[derive(Debug, Serialize)]
pub struct TraceSymbol {
    pub addr: u32,
    pub size: u32,
    pub name: String,
}

/// A single allocation event, serialized as one JSON line in `heap-trace.jsonl`.
#[derive(Debug, Serialize)]
pub struct AllocEvent {
    /// Event type: "A" (alloc), "D" (dealloc), "R" (realloc)
    pub t: &'static str,
    pub ptr: u32,
    pub sz: u32,
    /// Instruction count at time of event
    pub ic: u64,
    /// Stack frame addresses (return addresses, outermost last)
    pub frames: Vec<u32>,
    /// Free heap bytes after the operation (0 if not reported by guest)
    #[serde(default)]
    pub free: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_ptr: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_sz: Option<u32>,
}

/// Writes allocation trace events to disk.
pub struct AllocTracer {
    writer: BufWriter<File>,
    event_count: u64,
}

impl AllocTracer {
    /// Create a new tracer. Writes `meta.json` and opens `heap-trace.jsonl`.
    pub fn new(trace_dir: &Path, metadata: &TraceMetadata) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(trace_dir)?;

        let meta_path = trace_dir.join("meta.json");
        let meta_file = File::create(&meta_path)?;
        serde_json::to_writer_pretty(BufWriter::new(meta_file), metadata)?;

        let trace_path = trace_dir.join("heap-trace.jsonl");
        let writer = BufWriter::new(File::create(&trace_path)?);

        log::info!(
            "AllocTracer: writing to {} ({} symbols in metadata)",
            trace_dir.display(),
            metadata.symbols.len()
        );

        Ok(Self {
            writer,
            event_count: 0,
        })
    }

    /// Record one allocation event.
    pub fn record_event(&mut self, event: &AllocEvent) {
        if serde_json::to_writer(&mut self.writer, event).is_ok() {
            let _ = self.writer.write_all(b"\n");
            self.event_count += 1;
        }
    }

    /// Flush the writer and return the total number of events recorded.
    pub fn finish(&mut self) -> Result<u64, std::io::Error> {
        self.writer.flush()?;
        Ok(self.event_count)
    }
}

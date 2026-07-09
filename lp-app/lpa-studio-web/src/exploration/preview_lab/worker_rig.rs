//! Per-worker demultiplexer over one `lpa-link` browser-worker handle.
//!
//! One rig owns one Web Worker hosting several runtimes. It routes JSON
//! envelopes (protocol frames by `runtime_id`, lifecycle events, logs) and
//! hands binary preview frames straight through — pixels never touch the
//! JSON path.

use std::collections::{HashMap, VecDeque};

use lpa_link::providers::browser_worker::{
    BrowserInputEnvelope, BrowserOutputEnvelope, BrowserTickMode, BrowserWorkerHandle,
    BrowserWorkerOptions, PreviewPixelFrame,
};

/// One failed `preview_frame` request.
pub(super) struct PreviewError {
    pub runtime_id: u32,
    pub message: String,
}

pub(super) struct WorkerRig {
    handle: BrowserWorkerHandle,
    protocol: HashMap<u32, VecDeque<String>>,
    created: Vec<(u32, String)>,
    preview_errors: Vec<PreviewError>,
    /// Recent worker error/status lines for the lab log pane.
    pub notes: Vec<String>,
    /// Last observed wasm heap size for this worker, from preview frames.
    pub wasm_memory_bytes: f64,
}

impl WorkerRig {
    /// Spawn and boot one explicit-tick worker.
    pub(super) async fn boot(label: String) -> Result<Self, String> {
        let options = BrowserWorkerOptions::default().with_tick_mode(BrowserTickMode::Explicit);
        let mut handle = BrowserWorkerHandle::new(&options.worker_script_path())
            .map_err(|error| format!("spawn worker: {error}"))?;
        handle
            .boot(&label, &options)
            .await
            .map_err(|error| format!("boot worker: {error}"))?;
        Ok(Self {
            handle,
            protocol: HashMap::new(),
            created: Vec::new(),
            preview_errors: Vec::new(),
            notes: Vec::new(),
            wasm_memory_bytes: 0.0,
        })
    }

    pub(super) fn post(&self, envelope: &BrowserInputEnvelope) -> Result<(), String> {
        self.handle
            .post(envelope)
            .map_err(|error| format!("{error}"))
    }

    /// Drain and route pending JSON envelopes from the worker.
    pub(super) fn drain_outputs(&mut self) {
        for output in self.handle.take_outputs() {
            match output {
                BrowserOutputEnvelope::ProtocolOut { runtime_id, frame } => {
                    self.protocol
                        .entry(runtime_id)
                        .or_default()
                        .push_back(frame);
                }
                BrowserOutputEnvelope::RuntimeCreated { runtime_id, label } => {
                    self.created.push((runtime_id, label));
                }
                BrowserOutputEnvelope::PreviewError {
                    runtime_id,
                    message,
                    ..
                } => {
                    self.preview_errors.push(PreviewError {
                        runtime_id,
                        message,
                    });
                }
                BrowserOutputEnvelope::Status {
                    status, message, ..
                } if status == "error" => {
                    self.note(format!(
                        "worker error status: {}",
                        message.unwrap_or_default()
                    ));
                }
                BrowserOutputEnvelope::Log { level, message, .. }
                    if level == "error" || level == "warn" =>
                {
                    self.note(format!("worker {level}: {message}"));
                }
                BrowserOutputEnvelope::Status { .. } | BrowserOutputEnvelope::Log { .. } => {}
            }
        }
    }

    /// Take binary preview frames received since the last call.
    pub(super) fn take_preview_frames(&mut self) -> Vec<PreviewPixelFrame> {
        let frames = self.handle.take_preview_frames();
        if let Some(frame) = frames.last() {
            self.wasm_memory_bytes = frame.wasm_memory_bytes;
        }
        frames
    }

    pub(super) fn take_preview_errors(&mut self) -> Vec<PreviewError> {
        core::mem::take(&mut self.preview_errors)
    }

    /// Pop the next protocol frame queued for `runtime_id`.
    pub(super) fn pop_protocol_frame(&mut self, runtime_id: u32) -> Option<String> {
        self.protocol.get_mut(&runtime_id)?.pop_front()
    }

    /// Consume a `runtime_created` event by label.
    pub(super) fn take_created_runtime(&mut self, label: &str) -> Option<u32> {
        let index = self
            .created
            .iter()
            .position(|(_, created_label)| created_label == label)?;
        Some(self.created.remove(index).0)
    }

    pub(super) fn terminate(&self) {
        self.handle.terminate();
    }

    fn note(&mut self, note: String) {
        // Bounded so a chatty failure cannot grow without limit.
        if self.notes.len() >= 50 {
            self.notes.remove(0);
        }
        self.notes.push(note);
    }
}

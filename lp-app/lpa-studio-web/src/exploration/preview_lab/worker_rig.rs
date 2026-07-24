//! Per-worker demultiplexer over one `lpa-link` browser-worker handle.
//!
//! One rig owns one Web Worker hosting several runtimes. It routes JSON
//! envelopes (protocol frames by `runtime_id`, lifecycle events, logs) and
//! hands binary preview frames straight through — pixels never touch the
//! JSON path.

use std::collections::{HashMap, VecDeque};

use lpa_link::providers::browser_worker::{
    BrowserInputEnvelope, BrowserOutputEnvelope, BrowserRuntimeTier, BrowserTickMode,
    BrowserWorkerHandle, BrowserWorkerOptions, PreviewPixelFrame,
};

/// One failed `preview_frame` / `present_frame` / `attach_surface` request.
pub(super) struct PreviewError {
    pub runtime_id: u32,
    pub message: String,
}

/// One `runtime_created` answer, carrying the granted tier.
pub(super) struct CreatedRuntime {
    pub runtime_id: u32,
    pub label: String,
    pub tier: BrowserRuntimeTier,
    pub tier_reason: Option<String>,
}

/// One completed GPU-tier present (timing header; frame is on the surface).
pub(super) struct PresentedFrame {
    pub runtime_id: u32,
    pub tick_ms: f64,
    pub render_ms: f64,
    pub posted_epoch_ms: f64,
    pub wasm_memory_bytes: f64,
}

pub(super) struct WorkerRig {
    handle: BrowserWorkerHandle,
    protocol: HashMap<u32, VecDeque<String>>,
    created: Vec<CreatedRuntime>,
    surfaces_attached: Vec<u32>,
    presented: Vec<PresentedFrame>,
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
            surfaces_attached: Vec::new(),
            presented: Vec::new(),
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

    /// Transfer a card's `OffscreenCanvas` into the worker as its runtime's
    /// presentation surface (GPU tier).
    pub(super) fn attach_preview_surface(
        &self,
        runtime_id: u32,
        canvas: web_sys::OffscreenCanvas,
    ) -> Result<(), String> {
        self.handle
            .attach_preview_surface(runtime_id, canvas)
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
                BrowserOutputEnvelope::RuntimeCreated {
                    runtime_id,
                    label,
                    tier,
                    tier_reason,
                } => {
                    self.created.push(CreatedRuntime {
                        runtime_id,
                        label,
                        tier,
                        tier_reason,
                    });
                }
                BrowserOutputEnvelope::RuntimeDestroyed { runtime_id } => {
                    self.note(format!("runtime {runtime_id} destroyed"));
                }
                BrowserOutputEnvelope::SurfaceAttached { runtime_id } => {
                    self.surfaces_attached.push(runtime_id);
                }
                BrowserOutputEnvelope::PreviewPresented {
                    runtime_id,
                    tick_ms,
                    render_ms,
                    posted_epoch_ms,
                    wasm_memory_bytes,
                    ..
                } => {
                    self.wasm_memory_bytes = wasm_memory_bytes;
                    self.presented.push(PresentedFrame {
                        runtime_id,
                        tick_ms,
                        render_ms,
                        posted_epoch_ms,
                        wasm_memory_bytes,
                    });
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

    /// Take GPU-tier present completions received since the last call.
    pub(super) fn take_presented_frames(&mut self) -> Vec<PresentedFrame> {
        core::mem::take(&mut self.presented)
    }

    /// Consume a `surface_attached` ack for a runtime.
    pub(super) fn take_surface_attached(&mut self, runtime_id: u32) -> bool {
        let index = self
            .surfaces_attached
            .iter()
            .position(|attached| *attached == runtime_id);
        match index {
            Some(index) => {
                self.surfaces_attached.remove(index);
                true
            }
            None => false,
        }
    }

    /// Pop the next protocol frame queued for `runtime_id`.
    pub(super) fn pop_protocol_frame(&mut self, runtime_id: u32) -> Option<String> {
        self.protocol.get_mut(&runtime_id)?.pop_front()
    }

    /// Consume a `runtime_created` event by label.
    pub(super) fn take_created_runtime(&mut self, label: &str) -> Option<CreatedRuntime> {
        let index = self
            .created
            .iter()
            .position(|created| created.label == label)?;
        Some(self.created.remove(index))
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

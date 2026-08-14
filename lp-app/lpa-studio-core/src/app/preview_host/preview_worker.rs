//! Per-worker demultiplexer over one `lpa-link` browser-worker handle.
//!
//! One `PreviewWorker` owns one explicit-tick Web Worker hosting several
//! preview runtimes. It routes JSON envelopes (protocol frames by
//! `runtime_id`, lifecycle events, published output frames, preview errors)
//! and hands binary pixel frames straight through — pixels never touch the
//! JSON path, while the far smaller output samples do. This is the
//! productized shape of the preview-lab's rig, owned by the host so no
//! product code imports exploration modules.

use std::collections::{HashMap, VecDeque};

use lpa_link::providers::browser_worker::{
    BrowserInputEnvelope, BrowserOutputEnvelope, BrowserRuntimeTier, BrowserTickMode,
    BrowserWorkerHandle, BrowserWorkerOptions, PosterPixelFrame, PreviewPixelFrame,
};
use lpc_wire::OutputFrameEntry;

/// One failed `preview_frame` / `present_frame` / `attach_surface`
/// request (`frame_id` 0 marks attach/lifecycle failures).
pub(super) struct SlotPreviewError {
    pub(super) runtime_id: u32,
    pub(super) frame_id: u32,
    pub(super) message: String,
}

/// One `runtime_created` answer, carrying the granted tier.
pub(super) struct CreatedRuntime {
    pub(super) runtime_id: u32,
    pub(super) tier: BrowserRuntimeTier,
    pub(super) tier_reason: Option<String>,
}

/// One completed GPU-tier present (timing header; the frame is already on
/// the slot's surface).
pub(super) struct PresentedFrame {
    pub(super) runtime_id: u32,
}

/// One failed `capture_poster` request. Poster failures are NOT preview
/// errors on purpose: a slot whose poster capture failed keeps presenting
/// (or resting) — the card just keeps its placeholder.
pub(super) struct SlotPosterError {
    pub(super) runtime_id: u32,
    pub(super) message: String,
}

/// One `preview_output_frame` answer: the project's published lamps, plus
/// the engine's control-first verdict for it.
pub(super) struct SlotOutputFrame {
    pub(super) runtime_id: u32,
    pub(super) control_first: bool,
    pub(super) outputs: Vec<OutputFrameEntry>,
}

pub(super) struct PreviewWorker {
    handle: BrowserWorkerHandle,
    protocol: HashMap<u32, VecDeque<String>>,
    created: HashMap<String, CreatedRuntime>,
    surfaces_attached: Vec<u32>,
    presented: Vec<PresentedFrame>,
    output_frames: Vec<SlotOutputFrame>,
    preview_errors: Vec<SlotPreviewError>,
    poster_errors: Vec<SlotPosterError>,
    /// Worker-fatal errors (crash, uncaught script error). One entry is
    /// enough to condemn the worker to a recycle.
    worker_errors: Vec<String>,
}

impl PreviewWorker {
    /// Spawn and boot one explicit-tick worker. The boot runtime idles
    /// (never ticked); preview runtimes are created per lease.
    pub(super) async fn boot(label: &str) -> Result<Self, String> {
        let options = BrowserWorkerOptions::default().with_tick_mode(BrowserTickMode::Explicit);
        let mut handle = BrowserWorkerHandle::new(&options.worker_script_path())
            .map_err(|error| format!("spawn worker: {error}"))?;
        handle
            .boot(label, &options)
            .await
            .map_err(|error| format!("boot worker: {error}"))?;
        Ok(Self {
            handle,
            protocol: HashMap::new(),
            created: HashMap::new(),
            surfaces_attached: Vec::new(),
            presented: Vec::new(),
            output_frames: Vec::new(),
            preview_errors: Vec::new(),
            poster_errors: Vec::new(),
            worker_errors: Vec::new(),
        })
    }

    pub(super) fn post(&self, envelope: &BrowserInputEnvelope) -> Result<(), String> {
        self.handle
            .post(envelope)
            .map_err(|error| format!("{error}"))
    }

    /// Transfer a slot's `OffscreenCanvas` into the worker as its
    /// runtime's presentation surface (GPU tier).
    pub(super) fn attach_preview_surface(
        &self,
        runtime_id: u32,
        canvas: web_sys::OffscreenCanvas,
    ) -> Result<(), String> {
        self.handle
            .attach_preview_surface(runtime_id, canvas)
            .map_err(|error| format!("{error}"))
    }

    /// Release a slot runtime (fire-and-forget; the ack is idempotent).
    pub(super) fn destroy_runtime(&self, runtime_id: u32) -> Result<(), String> {
        self.handle
            .destroy_runtime(runtime_id)
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
                    self.created.insert(
                        label,
                        CreatedRuntime {
                            runtime_id,
                            tier,
                            tier_reason,
                        },
                    );
                }
                // Destroy is fire-and-forget (idempotent ack; P2 contract).
                BrowserOutputEnvelope::RuntimeDestroyed { .. } => {}
                BrowserOutputEnvelope::SurfaceAttached { runtime_id } => {
                    self.surfaces_attached.push(runtime_id);
                }
                BrowserOutputEnvelope::PreviewPresented { runtime_id, .. } => {
                    self.presented.push(PresentedFrame { runtime_id });
                }
                BrowserOutputEnvelope::PreviewOutputFrame {
                    runtime_id,
                    control_first,
                    outputs,
                    ..
                } => {
                    self.output_frames.push(SlotOutputFrame {
                        runtime_id,
                        control_first,
                        outputs,
                    });
                }
                BrowserOutputEnvelope::PreviewError {
                    runtime_id,
                    frame_id,
                    message,
                } => {
                    self.preview_errors.push(SlotPreviewError {
                        runtime_id,
                        frame_id,
                        message,
                    });
                }
                BrowserOutputEnvelope::PosterError {
                    runtime_id,
                    message,
                    ..
                } => {
                    self.poster_errors.push(SlotPosterError {
                        runtime_id,
                        message,
                    });
                }
                // "fatal" is the worker's sticky poisoned-instance status
                // (escaped panic=abort trap): like "error", it condemns
                // this worker to the recycle path.
                BrowserOutputEnvelope::Status {
                    status, message, ..
                } if status == "error" || status == "fatal" => {
                    self.worker_errors.push(message.unwrap_or_else(|| {
                        "worker reported error status without detail".to_string()
                    }));
                }
                BrowserOutputEnvelope::Log { level, message, .. }
                    if level == "error" || level == "warn" =>
                {
                    log::debug!("preview worker {level}: {message}");
                }
                BrowserOutputEnvelope::Status { .. } | BrowserOutputEnvelope::Log { .. } => {}
            }
        }
    }

    /// Take binary CPU-tier pixel frames received since the last call.
    pub(super) fn take_preview_frames(&mut self) -> Vec<PreviewPixelFrame> {
        self.handle.take_preview_frames()
    }

    /// Take preview request failures received since the last call.
    pub(super) fn take_preview_errors(&mut self) -> Vec<SlotPreviewError> {
        core::mem::take(&mut self.preview_errors)
    }

    /// Take binary poster-capture answers received since the last call.
    pub(super) fn take_poster_frames(&mut self) -> Vec<PosterPixelFrame> {
        self.handle.take_poster_frames()
    }

    /// Take poster-capture failures received since the last call.
    pub(super) fn take_poster_errors(&mut self) -> Vec<SlotPosterError> {
        core::mem::take(&mut self.poster_errors)
    }

    /// Take GPU-tier present completions received since the last call.
    pub(super) fn take_presented_frames(&mut self) -> Vec<PresentedFrame> {
        core::mem::take(&mut self.presented)
    }

    /// Take published output frames received since the last call.
    pub(super) fn take_output_frames(&mut self) -> Vec<SlotOutputFrame> {
        core::mem::take(&mut self.output_frames)
    }

    /// Take worker-fatal error notes received since the last call.
    pub(super) fn take_worker_errors(&mut self) -> Vec<String> {
        core::mem::take(&mut self.worker_errors)
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

    /// Consume a `runtime_created` event by creation label.
    pub(super) fn take_created_runtime(&mut self, label: &str) -> Option<CreatedRuntime> {
        self.created.remove(label)
    }

    /// Drop any state queued for a released runtime so stale frames never
    /// route to a recycled slot id.
    pub(super) fn forget_runtime(&mut self, runtime_id: u32) {
        self.protocol.remove(&runtime_id);
        self.surfaces_attached.retain(|id| *id != runtime_id);
        self.presented
            .retain(|frame| frame.runtime_id != runtime_id);
        self.output_frames
            .retain(|frame| frame.runtime_id != runtime_id);
        self.preview_errors
            .retain(|error| error.runtime_id != runtime_id);
    }

    pub(super) fn terminate(&self) {
        self.handle.terminate();
    }
}

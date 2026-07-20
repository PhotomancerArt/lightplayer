//! The browser-side `PreviewHost` service: worker pool, slot leases, and
//! the single deadline scheduler driving every preview slot.
//!
//! Architecture (preview-host ADR): consumers `lease()` a slot and observe
//! it through [`PreviewSlotHandle`] — they never touch workers, runtimes,
//! or envelopes. One `run()` future owns all IO: it boots the pool,
//! executes lease pipelines (create runtime → deploy → attach surface),
//! schedules `present_frame`/`preview_frame` posts per slot fps with
//! in-flight backpressure, enforces the global live-slot cap with LRU
//! eviction, and recycles a poisoned worker (respawn + re-lease of
//! still-visible slots) on device loss or present errors.
//!
//! Concurrency model: everything is single-threaded. `run()` keeps a list
//! of cooperative sub-tasks (worker boots, lease pipelines) and polls them
//! once per scheduler tick with a no-op waker — their awaits are timer- or
//! message-backed, so re-polling every few milliseconds drives them to
//! completion without an executor, and the scheduler keeps presenting
//! while deploys are in flight. The tick itself paces off a worker-backed
//! sleeper so hidden-tab timer throttling cannot stall previews.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use lpa_client::{LpClient, ProjectDeployFile};
use lpa_link::providers::browser_worker::{BrowserInputEnvelope, BrowserRuntimeTier};
use wasm_bindgen::JsCast;

use crate::app::library::LibraryHost;

use super::frame_schedule::{FrameDecision, FrameSchedule};
use super::preview_client_io::PreviewClientIo;
use super::preview_content::{catalog_deploy_files, example_deploy_files};
use super::preview_sleep::PreviewSleeper;
use super::preview_types::{
    PreviewHostConfig, PreviewSlotRequest, PreviewSlotStatus, PreviewSource, PreviewTier,
};
use super::preview_worker::PreviewWorker;
use super::slot_policy::{EvictionCandidate, choose_eviction, choose_worker};

/// Scheduler/polling loop period while any slot or sub-task is active.
const LOOP_SLEEP_MS: u32 = 4;
/// Loop period while the host is idle (no slots, no sub-tasks).
const IDLE_SLEEP_MS: u32 = 100;
/// How long to wait for `runtime_created` after `create_runtime`
/// (4 ms polls).
const CREATE_RUNTIME_POLL_LIMIT: usize = 500;
/// How long to wait for the consumer canvas to mount and the worker to
/// ack `attach_surface`, in 4 ms polls.
const ATTACH_SURFACE_POLL_LIMIT: usize = 500;
/// Bus channel carrying the previewed visual product.
const PREVIEW_CHANNEL: &str = "visual.out";
/// Project id preview deploys load under on the slot runtime.
const PREVIEW_PROJECT_ID: &str = "preview";
/// A slot whose re-lease (after causing a worker recycle) fails again is
/// parked in `Error` instead of recycling the worker forever.
const MAX_RECYCLE_STRIKES: u8 = 2;

type LocalTask = Pin<Box<dyn Future<Output = ()>>>;

/// Leased, pooled, budgeted live project previews (see the module and
/// crate ADR docs). Construct once per page, `lease()` slots freely, and
/// drive [`PreviewHost::run`] from the platform edge.
pub struct PreviewHost {
    shared: Rc<RefCell<SharedState>>,
}

/// One leased preview slot, released on drop.
///
/// Status is observed by polling ([`Self::status`] /
/// [`Self::status_revision`]), matching how the rest of lpa-studio-core
/// exposes async state (no UI-framework signals in core).
pub struct PreviewSlotHandle {
    slot: Rc<RefCell<Slot>>,
}

struct SharedState {
    config: PreviewHostConfig,
    library: Option<Rc<dyn LibraryHost>>,
    workers: Vec<WorkerEntry>,
    slots: Vec<Rc<RefCell<Slot>>>,
    next_slot_id: u64,
    next_frame_id: u32,
    shutdown: bool,
    running: bool,
}

struct WorkerEntry {
    /// Bumped on every respawn; pipelines abandon when their snapshot goes
    /// stale so a recycled worker never receives a zombie pipeline's posts.
    generation: u32,
    state: WorkerState,
}

enum WorkerState {
    /// Spawn/boot in flight (initial boot or post-recycle respawn).
    Booting,
    Ready(Rc<RefCell<PreviewWorker>>),
    /// Boot failed; the pool member stays down (recycling is deliberate,
    /// never a retry flap).
    Dead(String),
}

struct Slot {
    id: u64,
    source: PreviewSource,
    canvas_id: String,
    visible: bool,
    released: bool,
    /// Waiting for the run loop to start (or restart) its lease pipeline.
    pending: bool,
    /// A lease pipeline is in flight.
    deploying: bool,
    /// Visibility returned while the slot had no runtime: re-lease it.
    resume_requested: bool,
    /// No automatic recovery will be attempted; the consumer must lease a
    /// fresh slot (usually after remounting its canvas).
    terminal: bool,
    /// Worker recycles this slot has caused (flap guard).
    strikes: u8,
    worker_index: Option<usize>,
    worker_generation: u32,
    runtime_id: Option<u32>,
    tier: Option<PreviewTier>,
    tier_reason: Option<String>,
    /// Deploy + (GPU) surface attach completed; presents may be scheduled.
    presentable: bool,
    /// Attach failure routed from the worker (`PreviewError` frame 0).
    attach_error: Option<String>,
    schedule: FrameSchedule,
    last_active_ms: f64,
    presented_frames: u64,
    status: PreviewSlotStatus,
    status_revision: u64,
    /// CPU-tier render size (the mounted canvas's pixel size).
    cpu_size: (u32, u32),
    /// CPU-tier blit context cache (re-resolved if the canvas remounts).
    context: Option<web_sys::CanvasRenderingContext2d>,
}

impl Slot {
    fn set_status(&mut self, status: PreviewSlotStatus) {
        if self.status != status {
            self.status = status;
            self.status_revision += 1;
        }
    }

    fn live_status(&self) -> PreviewSlotStatus {
        PreviewSlotStatus::Live {
            tier: self.tier.unwrap_or(PreviewTier::Cpu),
            tier_reason: self.tier_reason.clone(),
        }
    }

    /// Holding (or acquiring) a runtime — what the live-slot cap counts.
    fn counts_as_live(&self) -> bool {
        self.runtime_id.is_some() || self.deploying
    }
}

impl PreviewHost {
    /// Build a host over `config`. `library` backs
    /// [`PreviewSource::ProjectUid`] leases (catalog snapshots); without
    /// it those leases fail with a clear status while example leases
    /// still work. Nothing boots until [`Self::run`] is driven.
    pub fn new(config: PreviewHostConfig, library: Option<Rc<dyn LibraryHost>>) -> Self {
        let pool_size = config.pool_size.max(1);
        let workers = (0..pool_size)
            .map(|_| WorkerEntry {
                generation: 0,
                state: WorkerState::Booting,
            })
            .collect();
        Self {
            shared: Rc::new(RefCell::new(SharedState {
                config,
                library,
                workers,
                slots: Vec::new(),
                next_slot_id: 1,
                next_frame_id: 1,
                shutdown: false,
                running: false,
            })),
        }
    }

    /// Lease a preview slot. Returns immediately; the run loop deploys the
    /// content and flips the handle's status from
    /// [`PreviewSlotStatus::Deploying`] as the pipeline progresses.
    /// Dropping the handle releases the slot (its runtime is destroyed).
    pub fn lease(&self, request: PreviewSlotRequest) -> PreviewSlotHandle {
        let mut shared = self.shared.borrow_mut();
        let id = shared.next_slot_id;
        shared.next_slot_id += 1;
        let fps = request.fps.unwrap_or(shared.config.default_fps);
        let shut_down = shared.shutdown;
        let slot = Rc::new(RefCell::new(Slot {
            id,
            source: request.source,
            canvas_id: request.canvas_id,
            visible: true,
            released: false,
            pending: !shut_down,
            deploying: false,
            resume_requested: false,
            terminal: shut_down,
            strikes: 0,
            worker_index: None,
            worker_generation: 0,
            runtime_id: None,
            tier: None,
            tier_reason: None,
            presentable: false,
            attach_error: None,
            schedule: FrameSchedule::new(fps),
            last_active_ms: now_ms(),
            presented_frames: 0,
            status: if shut_down {
                PreviewSlotStatus::Error {
                    reason: "preview host is shut down".to_string(),
                }
            } else {
                PreviewSlotStatus::Deploying
            },
            status_revision: 0,
            cpu_size: (128, 128),
            context: None,
        }));
        shared.slots.push(Rc::clone(&slot));
        PreviewSlotHandle { slot }
    }

    /// Stop the host: the run loop terminates every pool worker and
    /// returns. Live slots freeze as [`PreviewSlotStatus::Suspended`];
    /// further leases fail immediately.
    pub fn shutdown(&self) {
        self.shared.borrow_mut().shutdown = true;
    }

    /// Drive the host until [`Self::shutdown`]. Call exactly once and
    /// spawn it on the platform edge (the core owns no executor); a
    /// second call returns immediately.
    pub async fn run(&self) {
        {
            let mut shared = self.shared.borrow_mut();
            if shared.running {
                log::warn!("preview host run() called twice; ignoring the second call");
                return;
            }
            shared.running = true;
        }
        let mut tasks: Vec<LocalTask> = Vec::new();
        {
            let shared = self.shared.borrow();
            for index in 0..shared.workers.len() {
                tasks.push(boot_task(Rc::clone(&self.shared), index, 0));
            }
        }
        let sleeper = PreviewSleeper::new();
        loop {
            if self.shared.borrow().shutdown {
                self.finish_shutdown();
                break;
            }
            self.reap_released_slots();
            self.apply_resume_requests();
            self.start_pending_leases(&mut tasks);
            let mut recycles = Vec::new();
            self.schedule_due_frames(&mut recycles);
            self.collect_worker_outputs(&mut recycles);
            self.apply_recycles(recycles, &mut tasks);
            poll_tasks(&mut tasks);
            let idle = tasks.is_empty() && self.shared.borrow().slots.is_empty();
            sleeper
                .sleep_ms(if idle { IDLE_SLEEP_MS } else { LOOP_SLEEP_MS })
                .await;
        }
    }

    fn finish_shutdown(&self) {
        let (workers, slots) = {
            let mut shared = self.shared.borrow_mut();
            let workers: Vec<_> = shared
                .workers
                .iter_mut()
                .filter_map(|entry| {
                    match core::mem::replace(
                        &mut entry.state,
                        WorkerState::Dead("preview host shut down".to_string()),
                    ) {
                        WorkerState::Ready(worker) => Some(worker),
                        WorkerState::Booting | WorkerState::Dead(_) => None,
                    }
                })
                .collect();
            (workers, shared.slots.clone())
        };
        for worker in workers {
            worker.borrow().terminate();
        }
        for slot in slots {
            let mut slot = slot.borrow_mut();
            slot.schedule.pause();
            slot.presentable = false;
            slot.runtime_id = None;
            slot.pending = false;
            slot.deploying = false;
            if !slot.terminal {
                slot.set_status(PreviewSlotStatus::Suspended);
            }
        }
    }

    /// Destroy runtimes of dropped handles and forget their slots. Slots
    /// mid-pipeline are reaped once the pipeline notices the release.
    fn reap_released_slots(&self) {
        let slots = self.shared.borrow().slots.clone();
        for slot_rc in &slots {
            let (release_now, runtime, worker_index, generation) = {
                let slot = slot_rc.borrow();
                (
                    slot.released && !slot.deploying,
                    slot.runtime_id,
                    slot.worker_index,
                    slot.worker_generation,
                )
            };
            if !release_now {
                continue;
            }
            if let Some(runtime_id) = runtime {
                self.destroy_slot_runtime(worker_index, generation, runtime_id);
                slot_rc.borrow_mut().runtime_id = None;
            }
        }
        self.shared.borrow_mut().slots.retain(|slot| {
            let slot = slot.borrow();
            !(slot.released && !slot.deploying)
        });
    }

    /// Fire-and-forget `DestroyRuntime` toward the worker that granted the
    /// runtime, if it is still the same booted instance.
    fn destroy_slot_runtime(
        &self,
        worker_index: Option<usize>,
        worker_generation: u32,
        runtime_id: u32,
    ) {
        let Some(index) = worker_index else {
            return;
        };
        let worker = {
            let shared = self.shared.borrow();
            let Some(entry) = shared.workers.get(index) else {
                return;
            };
            if entry.generation != worker_generation {
                return; // the granting worker is gone; nothing to release
            }
            match &entry.state {
                WorkerState::Ready(worker) => Rc::clone(worker),
                WorkerState::Booting | WorkerState::Dead(_) => return,
            }
        };
        let mut worker = worker.borrow_mut();
        if let Err(error) = worker.destroy_runtime(runtime_id) {
            log::warn!("preview host: destroy runtime {runtime_id}: {error}");
        }
        worker.forget_runtime(runtime_id);
    }

    /// Turn visibility-return edges on runtime-less slots into re-leases.
    fn apply_resume_requests(&self) {
        let slots = self.shared.borrow().slots.clone();
        for slot_rc in slots {
            let mut slot = slot_rc.borrow_mut();
            if !slot.resume_requested {
                continue;
            }
            slot.resume_requested = false;
            if slot.released || slot.terminal || slot.deploying || slot.pending {
                continue;
            }
            if slot.runtime_id.is_none() {
                slot.pending = true;
                slot.set_status(PreviewSlotStatus::Deploying);
            }
        }
    }

    /// Start lease pipelines for pending slots: pick the least-loaded
    /// ready worker, enforce the live-slot cap (LRU eviction), and spawn
    /// the pipeline sub-task.
    fn start_pending_leases(&self, tasks: &mut Vec<LocalTask>) {
        let slots = self.shared.borrow().slots.clone();
        for slot_rc in &slots {
            {
                let slot = slot_rc.borrow();
                if !slot.pending || slot.deploying || slot.released || slot.terminal {
                    continue;
                }
            }
            let Some((worker_index, worker, generation)) = self.pick_worker(slot_rc) else {
                continue; // stays pending (booting) or was parked (all dead)
            };
            if !self.ensure_live_capacity(slot_rc) {
                continue; // stays pending until a slot frees up
            }
            {
                let mut slot = slot_rc.borrow_mut();
                slot.pending = false;
                slot.deploying = true;
                slot.presentable = false;
                slot.attach_error = None;
                slot.worker_index = Some(worker_index);
                slot.worker_generation = generation;
                slot.last_active_ms = now_ms();
                slot.set_status(PreviewSlotStatus::Deploying);
            }
            tasks.push(lease_pipeline(
                Rc::clone(&self.shared),
                Rc::clone(slot_rc),
                worker,
                worker_index,
                generation,
            ));
        }
    }

    /// Least-loaded ready worker for a pending slot. `None` keeps the slot
    /// pending (a worker is booting) or parks it in `Error` (all dead).
    fn pick_worker(
        &self,
        slot_rc: &Rc<RefCell<Slot>>,
    ) -> Option<(usize, Rc<RefCell<PreviewWorker>>, u32)> {
        let shared = self.shared.borrow();
        let loads: Vec<Option<usize>> = shared
            .workers
            .iter()
            .enumerate()
            .map(|(index, entry)| match &entry.state {
                WorkerState::Ready(_) => Some(
                    shared
                        .slots
                        .iter()
                        .filter(|slot| {
                            let slot = slot.borrow();
                            slot.worker_index == Some(index)
                                && !slot.released
                                && slot.counts_as_live()
                        })
                        .count(),
                ),
                WorkerState::Booting | WorkerState::Dead(_) => None,
            })
            .collect();
        match choose_worker(&loads) {
            Some(index) => {
                let entry = &shared.workers[index];
                let WorkerState::Ready(worker) = &entry.state else {
                    return None;
                };
                Some((index, Rc::clone(worker), entry.generation))
            }
            None => {
                let all_dead = shared
                    .workers
                    .iter()
                    .all(|entry| matches!(entry.state, WorkerState::Dead(_)));
                if all_dead {
                    let reason = shared
                        .workers
                        .iter()
                        .find_map(|entry| match &entry.state {
                            WorkerState::Dead(reason) => Some(reason.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "no preview workers".to_string());
                    drop(shared);
                    let mut slot = slot_rc.borrow_mut();
                    slot.pending = false;
                    slot.terminal = true;
                    slot.set_status(PreviewSlotStatus::Error {
                        reason: format!("preview workers unavailable: {reason}"),
                    });
                }
                None
            }
        }
    }

    /// Enforce the global live-slot cap before a new lease acquires a
    /// runtime, LRU-evicting (invisible first). `false` leaves the lease
    /// pending — nothing was evictable this tick.
    fn ensure_live_capacity(&self, for_slot: &Rc<RefCell<Slot>>) -> bool {
        let (live, candidates, cap) = {
            let shared = self.shared.borrow();
            let for_id = for_slot.borrow().id;
            let live = shared
                .slots
                .iter()
                .filter(|slot| {
                    let slot = slot.borrow();
                    !slot.released && slot.counts_as_live()
                })
                .count();
            let candidates: Vec<EvictionCandidate> = shared
                .slots
                .iter()
                .filter_map(|slot| {
                    let slot = slot.borrow();
                    (slot.id != for_id
                        && !slot.released
                        && !slot.deploying
                        && slot.runtime_id.is_some())
                    .then_some(EvictionCandidate {
                        slot_id: slot.id,
                        visible: slot.visible,
                        last_active_ms: slot.last_active_ms,
                    })
                })
                .collect();
            (live, candidates, shared.config.max_live_slots.max(1))
        };
        if live < cap {
            return true;
        }
        let Some(evict_id) = choose_eviction(&candidates) else {
            return false;
        };
        let evicted = {
            let shared = self.shared.borrow();
            shared
                .slots
                .iter()
                .find(|slot| slot.borrow().id == evict_id)
                .cloned()
        };
        if let Some(evicted) = evicted {
            self.evict_slot(&evicted);
        }
        true
    }

    /// Destroy an LRU-evicted slot's runtime and park it `Suspended` (the
    /// canvas keeps its last frame). It re-leases on the next visibility
    /// edge — GPU slots then need a consumer-remounted canvas, since the
    /// old one was consumed by its transfer.
    fn evict_slot(&self, slot_rc: &Rc<RefCell<Slot>>) {
        let (runtime, worker_index, generation) = {
            let slot = slot_rc.borrow();
            (slot.runtime_id, slot.worker_index, slot.worker_generation)
        };
        if let Some(runtime_id) = runtime {
            self.destroy_slot_runtime(worker_index, generation, runtime_id);
        }
        let mut slot = slot_rc.borrow_mut();
        slot.runtime_id = None;
        slot.presentable = false;
        slot.schedule.pause();
        slot.schedule.frame_failed();
        if !slot.terminal {
            slot.set_status(PreviewSlotStatus::Suspended);
        }
        log::info!("preview host: evicted slot {} (live-slot cap)", slot.id);
    }

    /// Post due `present_frame` / `preview_frame` requests for live,
    /// visible slots (per-slot fps, in-flight backpressure, clamped tick
    /// deltas — the frame-schedule contract).
    fn schedule_due_frames(&self, recycles: &mut Vec<RecycleRequest>) {
        let now = now_ms();
        let slots = self.shared.borrow().slots.clone();
        for slot_rc in &slots {
            let (envelope, worker_index, generation) = {
                let mut slot = slot_rc.borrow_mut();
                let eligible = !slot.released
                    && slot.visible
                    && slot.presentable
                    && slot.runtime_id.is_some()
                    && matches!(slot.status, PreviewSlotStatus::Live { .. });
                if !eligible {
                    continue;
                }
                match slot.schedule.poll(now) {
                    FrameDecision::Wait | FrameDecision::Skip => continue,
                    FrameDecision::Send { delta_ms } => {
                        let runtime_id = slot.runtime_id.expect("eligible slot has a runtime");
                        let frame_id = {
                            let mut shared = self.shared.borrow_mut();
                            let frame_id = shared.next_frame_id;
                            // 0 is reserved for attach/lifecycle errors.
                            shared.next_frame_id = shared.next_frame_id.wrapping_add(1).max(1);
                            frame_id
                        };
                        let envelope = if slot.tier == Some(PreviewTier::Gpu) {
                            BrowserInputEnvelope::PresentFrame {
                                runtime_id,
                                delta_ms: Some(delta_ms),
                                channel: PREVIEW_CHANNEL.to_string(),
                                frame_id,
                            }
                        } else {
                            BrowserInputEnvelope::PreviewFrame {
                                runtime_id,
                                delta_ms: Some(delta_ms),
                                channel: PREVIEW_CHANNEL.to_string(),
                                width: slot.cpu_size.0,
                                height: slot.cpu_size.1,
                                frame_id,
                            }
                        };
                        (
                            envelope,
                            slot.worker_index.expect("live slot has a worker"),
                            slot.worker_generation,
                        )
                    }
                }
            };
            let worker = {
                let shared = self.shared.borrow();
                match shared.workers.get(worker_index) {
                    Some(entry) if entry.generation == generation => match &entry.state {
                        WorkerState::Ready(worker) => Some(Rc::clone(worker)),
                        _ => None,
                    },
                    _ => None,
                }
            };
            match worker {
                Some(worker) => {
                    if let Err(error) = worker.borrow().post(&envelope) {
                        recycles.push(RecycleRequest {
                            worker_index,
                            cause_runtime: None,
                            reason: format!("post preview frame: {error}"),
                        });
                    }
                }
                None => slot_rc.borrow_mut().schedule.frame_failed(),
            }
        }
    }

    /// Drain every ready worker: complete presents, blit CPU pixel
    /// frames, route attach errors to their pipelines, and collect
    /// worker-poisoning failures as recycle requests.
    fn collect_worker_outputs(&self, recycles: &mut Vec<RecycleRequest>) {
        let now = now_ms();
        let workers: Vec<(usize, Rc<RefCell<PreviewWorker>>)> = {
            let shared = self.shared.borrow();
            shared
                .workers
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| match &entry.state {
                    WorkerState::Ready(worker) => Some((index, Rc::clone(worker))),
                    _ => None,
                })
                .collect()
        };
        for (worker_index, worker) in workers {
            let (pixel_frames, presented, errors, worker_errors) = {
                let mut worker = worker.borrow_mut();
                worker.drain_outputs();
                (
                    worker.take_preview_frames(),
                    worker.take_presented_frames(),
                    worker.take_preview_errors(),
                    worker.take_worker_errors(),
                )
            };
            for reason in worker_errors {
                recycles.push(RecycleRequest {
                    worker_index,
                    cause_runtime: None,
                    reason: format!("worker error: {reason}"),
                });
            }
            for error in errors {
                if error.frame_id == 0 {
                    // Attach/lifecycle failure: slot-local, consumed by the
                    // waiting lease pipeline.
                    if let Some(slot) = self.slot_by_runtime(worker_index, error.runtime_id) {
                        slot.borrow_mut().attach_error = Some(error.message);
                    }
                } else {
                    // Present/preview failure: treat the worker as
                    // poisoned (device loss surfaces here) and recycle.
                    recycles.push(RecycleRequest {
                        worker_index,
                        cause_runtime: Some(error.runtime_id),
                        reason: error.message,
                    });
                }
            }
            for done in presented {
                if let Some(slot) = self.slot_by_runtime(worker_index, done.runtime_id) {
                    let mut slot = slot.borrow_mut();
                    slot.schedule.frame_completed();
                    slot.presented_frames += 1;
                    slot.last_active_ms = now;
                }
            }
            for frame in pixel_frames {
                let Some(slot) = self.slot_by_runtime(worker_index, frame.runtime_id) else {
                    continue;
                };
                let mut slot = slot.borrow_mut();
                slot.schedule.frame_completed();
                slot.last_active_ms = now;
                match blit_pixel_frame(&mut slot, &frame) {
                    Ok(()) => slot.presented_frames += 1,
                    Err(reason) => {
                        slot.schedule.pause();
                        slot.terminal = true;
                        slot.set_status(PreviewSlotStatus::Error { reason });
                    }
                }
            }
        }
    }

    fn slot_by_runtime(&self, worker_index: usize, runtime_id: u32) -> Option<Rc<RefCell<Slot>>> {
        let shared = self.shared.borrow();
        shared
            .slots
            .iter()
            .find(|slot| {
                let slot = slot.borrow();
                slot.worker_index == Some(worker_index) && slot.runtime_id == Some(runtime_id)
            })
            .cloned()
    }

    /// Recycle poisoned workers: terminate + respawn each condemned
    /// worker, then re-lease its still-visible slots. The slot that caused
    /// the recycle accrues a strike; at [`MAX_RECYCLE_STRIKES`] it parks
    /// in `Error` instead of condemning workers forever.
    fn apply_recycles(&self, recycles: Vec<RecycleRequest>, tasks: &mut Vec<LocalTask>) {
        let mut recycled: Vec<usize> = Vec::new();
        for request in recycles {
            let cause_slot_id = request.cause_runtime.and_then(|runtime_id| {
                self.slot_by_runtime(request.worker_index, runtime_id)
                    .map(|slot| slot.borrow().id)
            });
            if recycled.contains(&request.worker_index) {
                // The worker is already condemned this tick; still charge
                // the causing slot its strike.
                self.mark_recycled_slots(request.worker_index, cause_slot_id, &request.reason);
                continue;
            }
            let respawn = {
                let mut shared = self.shared.borrow_mut();
                let Some(entry) = shared.workers.get_mut(request.worker_index) else {
                    continue;
                };
                match core::mem::replace(&mut entry.state, WorkerState::Booting) {
                    WorkerState::Ready(worker) => {
                        worker.borrow().terminate();
                        entry.generation += 1;
                        Some(entry.generation)
                    }
                    // Not ready: put the previous state back untouched.
                    other => {
                        entry.state = other;
                        None
                    }
                }
            };
            let Some(generation) = respawn else {
                continue;
            };
            log::warn!(
                "preview host: recycling worker {} ({})",
                request.worker_index,
                request.reason
            );
            recycled.push(request.worker_index);
            self.mark_recycled_slots(request.worker_index, cause_slot_id, &request.reason);
            tasks.push(boot_task(
                Rc::clone(&self.shared),
                request.worker_index,
                generation,
            ));
        }
    }

    /// Detach every slot of a recycled worker and decide its future:
    /// still-visible slots re-lease (status `Deploying`), invisible ones
    /// park `Suspended` until their next visibility edge, and the causing
    /// slot parks in `Error` once it exhausts its strikes.
    fn mark_recycled_slots(&self, worker_index: usize, cause_slot_id: Option<u64>, reason: &str) {
        let slots = self.shared.borrow().slots.clone();
        for slot_rc in slots {
            let mut slot = slot_rc.borrow_mut();
            if slot.worker_index != Some(worker_index) {
                continue;
            }
            let had_runtime = slot.runtime_id.is_some() || slot.deploying;
            if !had_runtime {
                continue;
            }
            slot.runtime_id = None;
            slot.presentable = false;
            slot.schedule.pause();
            slot.schedule.frame_failed();
            if Some(slot.id) == cause_slot_id {
                slot.strikes = slot.strikes.saturating_add(1);
            }
            if slot.released || slot.terminal {
                continue;
            }
            if Some(slot.id) == cause_slot_id && slot.strikes >= MAX_RECYCLE_STRIKES {
                slot.terminal = true;
                slot.set_status(PreviewSlotStatus::Error {
                    reason: format!("preview failed repeatedly: {reason}"),
                });
            } else if slot.visible {
                // Re-lease once the pipeline (if any) unwinds. A GPU
                // slot's canvas was consumed by its transfer, so the
                // re-lease surfaces a clear canvas error unless the
                // consumer remounted; that is the consumer's remount
                // discipline (P4).
                slot.resume_requested = true;
                slot.set_status(PreviewSlotStatus::Deploying);
            } else {
                slot.set_status(PreviewSlotStatus::Suspended);
            }
        }
    }
}

impl PreviewSlotHandle {
    /// Current observable slot state (poll-based, like the rest of the
    /// app core's async state).
    pub fn status(&self) -> PreviewSlotStatus {
        self.slot.borrow().status.clone()
    }

    /// Bumped on every status change — poll this to re-read cheaply.
    pub fn status_revision(&self) -> u64 {
        self.slot.borrow().status_revision
    }

    /// Frames known to have reached the slot's canvas (GPU present acks +
    /// CPU blits). The consumer's "swap the placeholder after the first
    /// present" signal.
    pub fn presented_frames(&self) -> u64 {
        self.slot.borrow().presented_frames
    }

    /// Suspend (`false`) or resume (`true`) presenting. Hiding pauses the
    /// scheduler and freezes the canvas on its last frame; showing
    /// resumes a held runtime immediately, or re-leases the slot when its
    /// runtime was evicted or lost with its worker.
    pub fn set_visible(&self, visible: bool) {
        let mut slot = self.slot.borrow_mut();
        if slot.visible == visible {
            return;
        }
        slot.visible = visible;
        if slot.released || slot.terminal {
            return;
        }
        if !visible {
            if matches!(slot.status, PreviewSlotStatus::Live { .. }) {
                slot.schedule.pause();
                slot.set_status(PreviewSlotStatus::Suspended);
            }
            return;
        }
        slot.last_active_ms = now_ms();
        if slot.runtime_id.is_some() && slot.presentable {
            if matches!(slot.status, PreviewSlotStatus::Suspended) {
                let start_at = slot.last_active_ms;
                slot.schedule.start(start_at);
                let live = slot.live_status();
                slot.set_status(live);
            }
        } else if slot.runtime_id.is_none() && !slot.deploying && !slot.pending {
            slot.resume_requested = true;
        }
    }
}

impl Drop for PreviewSlotHandle {
    fn drop(&mut self) {
        self.slot.borrow_mut().released = true;
    }
}

struct RecycleRequest {
    worker_index: usize,
    /// Runtime whose present failure condemned the worker (`None` for
    /// worker-level failures), for strike accounting.
    cause_runtime: Option<u32>,
    reason: String,
}

/// Boot (or respawn) pool worker `index` at `generation`.
fn boot_task(shared: Rc<RefCell<SharedState>>, index: usize, generation: u32) -> LocalTask {
    Box::pin(async move {
        let label = format!("preview-host-worker-{index}-g{generation}");
        let result = PreviewWorker::boot(&label).await;
        let mut shared = shared.borrow_mut();
        let superseded = shared.shutdown
            || shared
                .workers
                .get(index)
                .is_none_or(|entry| entry.generation != generation);
        if superseded {
            if let Ok(worker) = result {
                worker.terminate();
            }
            return;
        }
        shared.workers[index].state = match result {
            Ok(worker) => WorkerState::Ready(Rc::new(RefCell::new(worker))),
            Err(reason) => {
                log::warn!("preview host: worker {index} boot failed: {reason}");
                WorkerState::Dead(reason)
            }
        };
    })
}

/// Why a lease pipeline stopped early.
enum LeaseEnd {
    /// The slot was released or its worker was recycled/shut down while
    /// the pipeline ran; unwind silently (the run loop already decided
    /// the slot's future).
    Stale,
    /// The lease failed; park the slot in `Error`.
    Fail(String),
}

/// One slot's lease pipeline: materialize content, create the tiered
/// runtime, deploy, attach the surface (GPU) or size the canvas (CPU),
/// then go live.
fn lease_pipeline(
    shared: Rc<RefCell<SharedState>>,
    slot: Rc<RefCell<Slot>>,
    worker: Rc<RefCell<PreviewWorker>>,
    worker_index: usize,
    generation: u32,
) -> LocalTask {
    Box::pin(async move {
        let sleeper = Rc::new(PreviewSleeper::new());
        let result = run_lease(&shared, &slot, &worker, worker_index, generation, &sleeper).await;
        slot.borrow_mut().deploying = false;
        match result {
            Ok(()) => {}
            Err(LeaseEnd::Stale) => {}
            Err(LeaseEnd::Fail(reason)) => {
                // A failure raced the worker's recycle (or shutdown): the
                // run loop already decided the slot's future (re-lease or
                // park), so do not overwrite it with a stale error.
                let superseded = {
                    let shared = shared.borrow();
                    shared.shutdown
                        || shared
                            .workers
                            .get(worker_index)
                            .is_none_or(|entry| entry.generation != generation)
                };
                if superseded {
                    return;
                }
                // Deliberate failure surface: destroy anything acquired
                // and park; the consumer recovers by leasing again
                // (remounting its canvas first on the GPU tier).
                let runtime = slot.borrow_mut().runtime_id.take();
                if let Some(runtime_id) = runtime {
                    let mut worker = worker.borrow_mut();
                    if let Err(error) = worker.destroy_runtime(runtime_id) {
                        log::warn!("preview host: destroy runtime {runtime_id}: {error}");
                    }
                    worker.forget_runtime(runtime_id);
                }
                let mut slot = slot.borrow_mut();
                slot.presentable = false;
                slot.terminal = true;
                slot.set_status(PreviewSlotStatus::Error { reason });
            }
        }
    })
}

async fn run_lease(
    shared: &Rc<RefCell<SharedState>>,
    slot: &Rc<RefCell<Slot>>,
    worker: &Rc<RefCell<PreviewWorker>>,
    worker_index: usize,
    generation: u32,
    sleeper: &Rc<PreviewSleeper>,
) -> Result<(), LeaseEnd> {
    let stale = || -> bool {
        let shared = shared.borrow();
        shared.shutdown
            || slot.borrow().released
            || shared
                .workers
                .get(worker_index)
                .is_none_or(|entry| entry.generation != generation)
    };
    let check = || -> Result<(), LeaseEnd> {
        if stale() {
            Err(LeaseEnd::Stale)
        } else {
            Ok(())
        }
    };

    // 1. Materialize the source into deploy files.
    let (source, canvas_id, slot_id) = {
        let slot = slot.borrow();
        (slot.source.clone(), slot.canvas_id.clone(), slot.id)
    };
    let files: Vec<ProjectDeployFile> = match &source {
        PreviewSource::Example(id) => example_deploy_files(id).map_err(LeaseEnd::Fail)?,
        PreviewSource::ProjectUid(uid) => {
            let library = shared.borrow().library.clone().ok_or_else(|| {
                LeaseEnd::Fail("no library attached; project previews unavailable".to_string())
            })?;
            let fs = library
                .catalog_snapshot()
                .await
                .map_err(|error| LeaseEnd::Fail(format!("library: {error}")))?;
            check()?;
            catalog_deploy_files(fs, uid).map_err(LeaseEnd::Fail)?
        }
    };
    check()?;

    // 2. Create the tiered runtime (always request GPU; the granted tier
    //    and any fallback reason come back on `runtime_created`).
    let label = format!("preview-slot-{slot_id}-g{generation}");
    worker
        .borrow()
        .post(&BrowserInputEnvelope::CreateRuntime {
            label: label.clone(),
            tier: BrowserRuntimeTier::Gpu,
        })
        .map_err(LeaseEnd::Fail)?;
    let mut created = None;
    for _ in 0..CREATE_RUNTIME_POLL_LIMIT {
        {
            let mut worker = worker.borrow_mut();
            worker.drain_outputs();
            created = worker.take_created_runtime(&label);
        }
        if created.is_some() {
            break;
        }
        check()?;
        sleeper.sleep_ms(4).await;
    }
    let created =
        created.ok_or_else(|| LeaseEnd::Fail("timed out creating preview runtime".to_string()))?;
    let tier = match created.tier {
        BrowserRuntimeTier::Gpu => PreviewTier::Gpu,
        BrowserRuntimeTier::Cpu => PreviewTier::Cpu,
    };
    {
        let mut slot = slot.borrow_mut();
        slot.runtime_id = Some(created.runtime_id);
        slot.tier = Some(tier);
        slot.tier_reason = created.tier_reason.clone();
    }
    check()?;

    // 3. Deploy the project into the runtime (per-runtime protocol frames
    //    with tick-per-poll; explicit tick mode).
    let mut client = LpClient::new(PreviewClientIo::new(
        Rc::clone(worker),
        created.runtime_id,
        Rc::clone(sleeper),
    ));
    client
        .deploy_project_files(PREVIEW_PROJECT_ID, files)
        .await
        .map_err(|error| LeaseEnd::Fail(format!("deploy: {error}")))?;
    check()?;

    // 4. Wire the consumer's canvas: GPU transfers it into the worker as
    //    the present surface; CPU reads its size for pixel frames.
    let canvas = wait_for_canvas(&canvas_id, sleeper, &stale).await?;
    if tier == PreviewTier::Gpu {
        let offscreen = canvas.transfer_control_to_offscreen().map_err(|_| {
            LeaseEnd::Fail(format!(
                "canvas #{canvas_id} was already transferred — remount a fresh canvas and lease \
                 again"
            ))
        })?;
        worker
            .borrow()
            .attach_preview_surface(created.runtime_id, offscreen)
            .map_err(LeaseEnd::Fail)?;
        let mut attached = false;
        for _ in 0..ATTACH_SURFACE_POLL_LIMIT {
            {
                let mut worker = worker.borrow_mut();
                worker.drain_outputs();
                attached = worker.take_surface_attached(created.runtime_id);
            }
            if attached {
                break;
            }
            if let Some(reason) = slot.borrow_mut().attach_error.take() {
                return Err(LeaseEnd::Fail(format!("attach surface: {reason}")));
            }
            check()?;
            sleeper.sleep_ms(4).await;
        }
        if !attached {
            return Err(LeaseEnd::Fail(
                "timed out waiting for the preview surface to attach".to_string(),
            ));
        }
    } else {
        let width = canvas.width().clamp(16, 1_024);
        let height = canvas.height().clamp(16, 1_024);
        slot.borrow_mut().cpu_size = (width, height);
    }

    // 5. Live (or parked, when the consumer hid the card mid-deploy).
    let mut slot = slot.borrow_mut();
    slot.presentable = true;
    slot.last_active_ms = now_ms();
    if slot.visible {
        let start_at = slot.last_active_ms;
        slot.schedule.start(start_at);
        let live = slot.live_status();
        slot.set_status(live);
    } else {
        slot.set_status(PreviewSlotStatus::Suspended);
    }
    Ok(())
}

/// Poll the DOM for the consumer's mounted canvas element.
async fn wait_for_canvas(
    canvas_id: &str,
    sleeper: &Rc<PreviewSleeper>,
    stale: &dyn Fn() -> bool,
) -> Result<web_sys::HtmlCanvasElement, LeaseEnd> {
    for _ in 0..ATTACH_SURFACE_POLL_LIMIT {
        let canvas = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(canvas_id))
            .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok());
        if let Some(canvas) = canvas {
            return Ok(canvas);
        }
        if stale() {
            return Err(LeaseEnd::Stale);
        }
        sleeper.sleep_ms(4).await;
    }
    Err(LeaseEnd::Fail(format!(
        "canvas #{canvas_id} not mounted for the preview"
    )))
}

/// Blit one CPU-tier pixel frame onto the slot's canvas via
/// `putImageData` (the pixels arrived as a transferable `ArrayBuffer`,
/// never through the JSON path).
fn blit_pixel_frame(
    slot: &mut Slot,
    frame: &lpa_link::providers::browser_worker::PreviewPixelFrame,
) -> Result<(), String> {
    let context = slot_blit_context(slot)?;
    let canvas = context
        .canvas()
        .ok_or_else(|| "canvas context has no canvas".to_string())?;
    if canvas.width() != frame.width || canvas.height() != frame.height {
        canvas.set_width(frame.width);
        canvas.set_height(frame.height);
    }
    let pixels = js_sys::Uint8ClampedArray::new(&frame.pixels);
    let image =
        web_sys::ImageData::new_with_js_u8_clamped_array_and_sh(&pixels, frame.width, frame.height)
            .map_err(|error| format!("build ImageData: {error:?}"))?;
    context
        .put_image_data(&image, 0.0, 0.0)
        .map_err(|error| format!("putImageData: {error:?}"))?;
    Ok(())
}

fn slot_blit_context(slot: &mut Slot) -> Result<web_sys::CanvasRenderingContext2d, String> {
    if let Some(context) = &slot.context {
        let connected = context
            .canvas()
            .map(|canvas| canvas.is_connected())
            .unwrap_or(false);
        if connected {
            return Ok(context.clone());
        }
        slot.context = None;
    }
    let id = &slot.canvas_id;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "missing document".to_string())?;
    let canvas = document
        .get_element_by_id(id)
        .ok_or_else(|| format!("canvas #{id} not mounted"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("#{id} is not a canvas"))?;
    let context = canvas
        .get_context("2d")
        .map_err(|error| format!("get 2d context: {error:?}"))?
        .ok_or_else(|| "2d context unavailable".to_string())?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "2d context has unexpected type".to_string())?;
    slot.context = Some(context.clone());
    Ok(context)
}

/// Advance every cooperative sub-task one poll, dropping the finished
/// ones. The tasks' awaits are timer/message-backed, so re-polling each
/// scheduler tick drives them without an executor (their wakes are
/// no-ops by construction).
fn poll_tasks(tasks: &mut Vec<LocalTask>) {
    let mut context = Context::from_waker(Waker::noop());
    tasks.retain_mut(|task| task.as_mut().poll(&mut context) == Poll::Pending);
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map(|performance| performance.now())
        .unwrap_or_else(js_sys::Date::now)
}

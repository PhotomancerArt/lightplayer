//! Preview-lab orchestration: workers, card runtimes, tick scheduling,
//! binary pixel presentation, and stats publishing.
//!
//! One lab run boots N browser runtimes spread across W explicit-tick Web
//! Workers, deploys the selected example project into each, then drives every
//! card at the target fps with staggered `preview_frame` requests. Pixels
//! come back as transferable `ArrayBuffer`s and are presented with
//! `putImageData`; JSON envelopes only carry control and timing metadata.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_client::LpClient;
use lpa_link::providers::browser_worker::{
    BrowserInputEnvelope, BrowserRuntimeTier, PreviewPixelFrame,
};
use wasm_bindgen::{JsCast, JsValue};

use crate::exploration::preview_lab_config::{CardTierRequest, LabConfig};
use crate::exploration::preview_lab_stats::{
    CardStats, CardStatsSnapshot, LabAggregate, PreviewFrameSample, aggregate,
};

use super::example_projects;
use super::lab_client_io::LabClientIo;
use super::lab_sleep::LabSleeper;
use super::worker_rig::{PresentedFrame, WorkerRig};

/// Scheduling/polling loop period. Small enough not to dominate transport
/// latency at 20 fps targets.
const LOOP_SLEEP_MS: u32 = 4;
/// View/automation stats refresh period.
const PUBLISH_EVERY_MS: f64 = 500.0;
/// Ceiling on a single tick delta so a stalled card does not fast-forward.
const MAX_TICK_DELTA_MS: f64 = 250.0;
/// How long to wait for `runtime_created` after `create_runtime`.
const CREATE_RUNTIME_POLL_LIMIT: usize = 500;
/// How long to wait for the card canvas to mount and the worker to ack
/// `attach_surface` (GPU tier), in 4 ms polls.
const ATTACH_SURFACE_POLL_LIMIT: usize = 500;

/// Point-in-time lab state for the UI and the automation JSON
/// (`window.__labStats`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct LabView {
    pub phase: String,
    pub running: bool,
    pub elapsed_s: f64,
    /// Run generation for canvas element ids: a GPU-tier canvas is consumed
    /// by `transferControlToOffscreen`, so every run mounts fresh canvases.
    pub generation: u32,
    pub cards: Vec<LabCardView>,
    pub aggregate: LabAggregate,
    pub worker_wasm_memory_bytes: Vec<f64>,
    pub js_heap_bytes: Option<f64>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct LabCardView {
    pub index: usize,
    pub worker: usize,
    pub status: String,
    /// Granted tier badge: `"gpu"`, `"cpu"`, or `"…"` before creation.
    pub tier: String,
    /// Why a GPU request resolved to the CPU tier (fidelity-tiers ADR:
    /// surfaced on the card, never silent).
    pub tier_reason: Option<String>,
    pub error: Option<String>,
    pub stats: CardStatsSnapshot,
}

/// One lab run. The page holds this in an `Rc` and flips `stop_requested`;
/// the spawned run future owns the rest of the lifecycle.
pub struct LabRun {
    pub config: LabConfig,
    /// Monotonic per-page run counter (canvas ids are per-generation).
    pub generation: u32,
    stop_requested: bool,
    rigs: Vec<Rc<RefCell<WorkerRig>>>,
    cards: Vec<LabCard>,
    next_frame_id: u32,
}

impl LabRun {
    pub fn new(config: LabConfig) -> Self {
        thread_local! {
            static NEXT_GENERATION: Cell<u32> = const { Cell::new(1) };
        }
        let generation = NEXT_GENERATION.with(|next| {
            let generation = next.get();
            next.set(generation + 1);
            generation
        });
        let cards = (0..config.cards as usize)
            .map(|index| {
                LabCard::new(
                    index,
                    index % config.workers as usize,
                    config.tier.requested_for_card(index),
                )
            })
            .collect();
        Self {
            config,
            generation,
            stop_requested: false,
            rigs: Vec::new(),
            cards,
            next_frame_id: 1,
        }
    }

    pub fn request_stop(&mut self) {
        self.stop_requested = true;
    }
}

enum CardStatus {
    Pending,
    Deploying,
    Running,
    Failed,
}

struct LabCard {
    index: usize,
    worker_index: usize,
    runtime_id: Option<u32>,
    /// Tier requested at creation (from the run configuration).
    requested_tier: CardTierRequest,
    /// Tier the worker actually granted (recorded from `runtime_created`).
    granted_tier: Option<BrowserRuntimeTier>,
    /// Why a GPU request resolved to CPU (shown on the card badge).
    tier_reason: Option<String>,
    status: CardStatus,
    error: Option<String>,
    next_due_ms: f64,
    last_tick_at_ms: Option<f64>,
    in_flight: bool,
    stats: CardStats,
    context: Option<web_sys::CanvasRenderingContext2d>,
}

impl LabCard {
    fn new(index: usize, worker_index: usize, requested_tier: CardTierRequest) -> Self {
        Self {
            index,
            worker_index,
            runtime_id: None,
            requested_tier,
            granted_tier: None,
            tier_reason: None,
            status: CardStatus::Pending,
            error: None,
            next_due_ms: 0.0,
            last_tick_at_ms: None,
            in_flight: false,
            stats: CardStats::default(),
            context: None,
        }
    }

    fn presents_via_surface(&self) -> bool {
        self.granted_tier == Some(BrowserRuntimeTier::Gpu)
    }

    fn tier_label(&self) -> String {
        match self.granted_tier {
            Some(BrowserRuntimeTier::Gpu) => "gpu".to_string(),
            Some(BrowserRuntimeTier::Cpu) => "cpu".to_string(),
            None => "…".to_string(),
        }
    }

    fn status_label(&self) -> &'static str {
        match self.status {
            CardStatus::Pending => "pending",
            CardStatus::Deploying => "deploying",
            CardStatus::Running => "running",
            CardStatus::Failed => "failed",
        }
    }

    fn fail(&mut self, message: String) {
        self.status = CardStatus::Failed;
        self.error = Some(message);
        self.in_flight = false;
    }
}

/// Drive one lab run to completion (until stop is requested or setup fails).
pub async fn run_lab(run: Rc<RefCell<LabRun>>, mut view: Signal<LabView>) {
    let config = run.borrow().config.clone();
    publish_phase(&run, &mut view, "booting workers");

    // Boot workers in parallel.
    let boots = (0..config.workers)
        .map(|w| WorkerRig::boot(format!("preview-lab-worker-{w}")))
        .collect::<Vec<_>>();
    let mut rigs = Vec::new();
    for (w, result) in futures_util::future::join_all(boots)
        .await
        .into_iter()
        .enumerate()
    {
        match result {
            Ok(rig) => rigs.push(Rc::new(RefCell::new(rig))),
            Err(error) => {
                publish_phase(&run, &mut view, &format!("worker {w} boot failed: {error}"));
                terminate(&rigs);
                return;
            }
        }
    }
    run.borrow_mut().rigs = rigs.clone();

    // Deploy cards: sequential per worker, parallel across workers. Each
    // deploy future paces itself off its own throttle-immune sleeper. The
    // publisher races the deploys so per-card progress stays visible.
    publish_phase(&run, &mut view, "deploying projects");
    let deploys = futures_util::future::join_all(
        (0..config.workers as usize)
            .map(|w| deploy_worker_cards(Rc::clone(&run), Rc::clone(&rigs[w]), w))
            .collect::<Vec<_>>(),
    );
    let publish_sleeper = LabSleeper::new();
    futures_util::pin_mut!(deploys);
    let mut deploys = deploys;
    loop {
        let pause = Box::pin(publish_sleeper.sleep_ms(250));
        match futures_util::future::select(deploys, pause).await {
            futures_util::future::Either::Left(_) => break,
            futures_util::future::Either::Right((_, rest)) => {
                deploys = rest;
                publish_phase(&run, &mut view, "deploying projects");
            }
        }
    }

    let ready = run
        .borrow()
        .cards
        .iter()
        .filter(|card| matches!(card.status, CardStatus::Running))
        .count();
    if ready == 0 {
        publish_phase(&run, &mut view, "no cards deployed");
        terminate(&rigs);
        return;
    }

    // Stagger card phases across the frame period.
    {
        let mut run_mut = run.borrow_mut();
        let period = run_mut.config.period_ms();
        let total = run_mut.cards.len().max(1) as f64;
        let start = now_ms();
        for card in &mut run_mut.cards {
            card.next_due_ms = start + period * card.index as f64 / total;
        }
    }

    publish_phase(&run, &mut view, "running");
    // Hidden tabs throttle main-thread timers to >=1 s; the lab paces its
    // loop off a worker-backed sleeper so measurement runs stay honest even
    // when the page is not visible.
    let sleeper = publish_sleeper;
    let started_at = now_ms();
    let mut last_publish = 0.0f64;
    loop {
        {
            let mut run_mut = run.borrow_mut();
            if run_mut.stop_requested {
                break;
            }
            let now = now_ms();
            schedule_due_frames(&mut run_mut, now);
            collect_frames(&mut run_mut);
            if now - last_publish >= PUBLISH_EVERY_MS {
                last_publish = now;
                let next = build_view(&mut run_mut, "running", true, (now - started_at) / 1_000.0);
                publish_stats_json(&next);
                view.set(next);
            }
        }
        sleeper.sleep_ms(LOOP_SLEEP_MS).await;
    }

    terminate(&rigs);
    publish_phase(&run, &mut view, "stopped");
}

/// Create runtimes and deploy the example project for every card on one worker.
async fn deploy_worker_cards(run: Rc<RefCell<LabRun>>, rig: Rc<RefCell<WorkerRig>>, worker: usize) {
    let sleeper = Rc::new(LabSleeper::new());
    let (config, card_indexes) = {
        let run_ref = run.borrow();
        let indexes = run_ref
            .cards
            .iter()
            .filter(|card| card.worker_index == worker)
            .map(|card| card.index)
            .collect::<Vec<_>>();
        (run_ref.config.clone(), indexes)
    };

    let generation = run.borrow().generation;
    for index in card_indexes {
        let requested_tier = run.borrow().cards[index].requested_tier;
        run.borrow_mut().cards[index].status = CardStatus::Deploying;
        match deploy_card(&config, &rig, index, requested_tier, &sleeper).await {
            Ok(created) => {
                {
                    let mut run_mut = run.borrow_mut();
                    let card = &mut run_mut.cards[index];
                    card.runtime_id = Some(created.runtime_id);
                    card.granted_tier = Some(created.tier);
                    card.tier_reason = created.tier_reason.clone();
                }
                // GPU-tier cards present to a transferred canvas surface.
                let attach = if created.tier == BrowserRuntimeTier::Gpu {
                    attach_card_surface(&rig, generation, index, created.runtime_id, &sleeper).await
                } else {
                    Ok(())
                };
                match attach {
                    Ok(()) => run.borrow_mut().cards[index].status = CardStatus::Running,
                    Err(error) => run.borrow_mut().cards[index].fail(error),
                }
            }
            Err(error) => run.borrow_mut().cards[index].fail(error),
        }
        if run.borrow().stop_requested {
            return;
        }
    }
}

async fn deploy_card(
    config: &LabConfig,
    rig: &Rc<RefCell<WorkerRig>>,
    index: usize,
    requested_tier: CardTierRequest,
    sleeper: &Rc<LabSleeper>,
) -> Result<super::worker_rig::CreatedRuntime, String> {
    let label = format!("preview-card-{index}");
    let tier = match requested_tier {
        CardTierRequest::Cpu => BrowserRuntimeTier::Cpu,
        CardTierRequest::Gpu => BrowserRuntimeTier::Gpu,
    };
    log::info!("preview lab: creating runtime for card {index} (tier request {tier:?})");
    rig.borrow().post(&BrowserInputEnvelope::CreateRuntime {
        label: label.clone(),
        tier,
    })?;
    let mut created = None;
    for _ in 0..CREATE_RUNTIME_POLL_LIMIT {
        {
            let mut rig_mut = rig.borrow_mut();
            rig_mut.drain_outputs();
            created = rig_mut.take_created_runtime(&label);
        }
        if created.is_some() {
            break;
        }
        sleeper.sleep_ms(4).await;
    }
    let created = created.ok_or_else(|| format!("timed out creating runtime for card {index}"))?;
    log::info!(
        "preview lab: card {index} runtime {} tier {:?}{}; deploying project",
        created.runtime_id,
        created.tier,
        created
            .tier_reason
            .as_deref()
            .map(|reason| format!(" (reason: {reason})"))
            .unwrap_or_default()
    );

    let mut client = LpClient::new(LabClientIo::new(
        Rc::clone(rig),
        created.runtime_id,
        Rc::clone(sleeper),
    ));
    client
        .deploy_project_files("preview", example_projects::deploy_files(config.project))
        .await
        .map_err(|error| format!("deploy card {index}: {error}"))?;
    log::info!("preview lab: card {index} deployed");
    Ok(created)
}

/// Transfer a GPU-tier card's canvas into the worker and wait for the
/// `surface_attached` ack (or a preview error).
async fn attach_card_surface(
    rig: &Rc<RefCell<WorkerRig>>,
    generation: u32,
    index: usize,
    runtime_id: u32,
    sleeper: &Rc<LabSleeper>,
) -> Result<(), String> {
    let id = canvas_element_id(generation, index);
    let mut canvas = None;
    for _ in 0..ATTACH_SURFACE_POLL_LIMIT {
        canvas = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&id))
            .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok());
        if canvas.is_some() {
            break;
        }
        sleeper.sleep_ms(4).await;
    }
    let canvas = canvas.ok_or_else(|| format!("canvas #{id} not mounted for surface attach"))?;
    let offscreen = canvas
        .transfer_control_to_offscreen()
        .map_err(|error| format!("transferControlToOffscreen: {error:?}"))?;
    rig.borrow().attach_preview_surface(runtime_id, offscreen)?;

    for _ in 0..ATTACH_SURFACE_POLL_LIMIT {
        {
            let mut rig_mut = rig.borrow_mut();
            rig_mut.drain_outputs();
            if rig_mut.take_surface_attached(runtime_id) {
                return Ok(());
            }
            if let Some(error) = rig_mut
                .take_preview_errors()
                .into_iter()
                .find(|error| error.runtime_id == runtime_id)
            {
                return Err(format!("attach surface: {}", error.message));
            }
        }
        sleeper.sleep_ms(4).await;
    }
    Err(format!(
        "timed out waiting for surface_attached on card {index}"
    ))
}

/// Post `preview_frame` for every running card whose schedule came due.
fn schedule_due_frames(run: &mut LabRun, now: f64) {
    let period = run.config.period_ms();
    let size = run.config.size;
    for card in &mut run.cards {
        if !matches!(card.status, CardStatus::Running) || now < card.next_due_ms {
            continue;
        }
        if card.in_flight {
            // Backpressure: the worker has not answered the previous frame;
            // skip this slot rather than queueing further behind.
            card.stats.record_dropped();
            card.next_due_ms += period;
            continue;
        }
        let delta = card
            .last_tick_at_ms
            .map(|last| (now - last).clamp(1.0, MAX_TICK_DELTA_MS))
            .unwrap_or(period);
        let frame_id = run.next_frame_id;
        run.next_frame_id = run.next_frame_id.wrapping_add(1);
        let Some(runtime_id) = card.runtime_id else {
            continue;
        };
        let envelope = if card.presents_via_surface() {
            // GPU tier: render straight to the attached card surface; the
            // render size is the surface size (zero pixel transfer).
            BrowserInputEnvelope::PresentFrame {
                runtime_id,
                delta_ms: Some(delta.round() as u32),
                channel: "visual.out".to_string(),
                frame_id,
            }
        } else {
            BrowserInputEnvelope::PreviewFrame {
                runtime_id,
                delta_ms: Some(delta.round() as u32),
                channel: "visual.out".to_string(),
                width: size,
                height: size,
                frame_id,
            }
        };
        let posted = run.rigs[card.worker_index].borrow().post(&envelope);
        match posted {
            Ok(()) => {
                card.in_flight = true;
                card.last_tick_at_ms = Some(now);
                // Keep phase but avoid runaway catch-up bursts after stalls.
                card.next_due_ms += period;
                if card.next_due_ms < now {
                    card.next_due_ms = now + period;
                }
            }
            Err(error) => card.fail(format!("post preview frame: {error}")),
        }
    }
}

/// Drain every rig: route envelopes, then present received pixel frames and
/// record GPU-tier present completions.
fn collect_frames(run: &mut LabRun) {
    let generation = run.generation;
    for worker_index in 0..run.rigs.len() {
        let (frames, presented, errors) = {
            let mut rig = run.rigs[worker_index].borrow_mut();
            rig.drain_outputs();
            (
                rig.take_preview_frames(),
                rig.take_presented_frames(),
                rig.take_preview_errors(),
            )
        };
        for error in errors {
            if let Some(card) = card_by_runtime_mut(&mut run.cards, worker_index, error.runtime_id)
            {
                card.in_flight = false;
                card.stats.record_error();
                card.error = Some(error.message);
            }
        }
        for frame in frames {
            let Some(card) = card_by_runtime_mut(&mut run.cards, worker_index, frame.runtime_id)
            else {
                continue;
            };
            card.in_flight = false;
            let transport_ms = (epoch_now_ms() - frame.posted_epoch_ms).max(0.0);
            match present_frame(card, generation, &frame) {
                Ok(present_ms) => {
                    card.stats.record(
                        now_ms(),
                        PreviewFrameSample {
                            tick_ms: frame.tick_ms,
                            render_ms: frame.render_ms,
                            transport_ms,
                            present_ms,
                        },
                    );
                }
                Err(error) => {
                    card.stats.record_error();
                    card.error = Some(error);
                }
            }
        }
        // GPU-tier completions: the frame is already on the card surface —
        // only the timing header comes back (present_ms is 0 by design:
        // there is no main-thread canvas work on this tier).
        for done in presented {
            record_presented_frame(&mut run.cards, worker_index, &done);
        }
    }
}

fn record_presented_frame(cards: &mut [LabCard], worker_index: usize, done: &PresentedFrame) {
    let Some(card) = card_by_runtime_mut(cards, worker_index, done.runtime_id) else {
        return;
    };
    card.in_flight = false;
    let transport_ms = (epoch_now_ms() - done.posted_epoch_ms).max(0.0);
    card.stats.record(
        now_ms(),
        PreviewFrameSample {
            tick_ms: done.tick_ms,
            render_ms: done.render_ms,
            transport_ms,
            present_ms: 0.0,
        },
    );
}

/// Blit one binary frame into the card's canvas; returns present ms.
fn present_frame(
    card: &mut LabCard,
    generation: u32,
    frame: &PreviewPixelFrame,
) -> Result<f64, String> {
    let start = now_ms();
    let context = card_context(card, generation)?;
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
    Ok(now_ms() - start)
}

fn card_context(
    card: &mut LabCard,
    generation: u32,
) -> Result<web_sys::CanvasRenderingContext2d, String> {
    if let Some(context) = &card.context {
        let connected = context
            .canvas()
            .map(|canvas| canvas.is_connected())
            .unwrap_or(false);
        if connected {
            return Ok(context.clone());
        }
        card.context = None;
    }
    let id = canvas_element_id(generation, card.index);
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "missing document".to_string())?;
    let canvas = document
        .get_element_by_id(&id)
        .ok_or_else(|| format!("canvas #{id} not mounted"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("#{id} is not a canvas"))?;
    let context = canvas
        .get_context("2d")
        .map_err(|error| format!("get 2d context: {error:?}"))?
        .ok_or_else(|| "2d context unavailable".to_string())?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "2d context has unexpected type".to_string())?;
    card.context = Some(context.clone());
    Ok(context)
}

/// Canvas ids are per run generation: a GPU-tier canvas is permanently
/// consumed by `transferControlToOffscreen`, so each run mounts fresh
/// canvas elements.
pub fn canvas_element_id(generation: u32, card_index: usize) -> String {
    format!("preview-lab-canvas-{generation}-{card_index}")
}

fn card_by_runtime_mut(
    cards: &mut [LabCard],
    worker_index: usize,
    runtime_id: u32,
) -> Option<&mut LabCard> {
    cards
        .iter_mut()
        .find(|card| card.worker_index == worker_index && card.runtime_id == Some(runtime_id))
}

fn build_view(run: &mut LabRun, phase: &str, running: bool, elapsed_s: f64) -> LabView {
    let now = now_ms();
    let cards = run
        .cards
        .iter_mut()
        .map(|card| LabCardView {
            index: card.index,
            worker: card.worker_index,
            status: card.status_label().to_string(),
            tier: card.tier_label(),
            tier_reason: card.tier_reason.clone(),
            error: card.error.clone(),
            stats: card.stats.snapshot(now),
        })
        .collect::<Vec<_>>();
    let snapshots = cards
        .iter()
        .map(|card| card.stats.clone())
        .collect::<Vec<_>>();
    let mut notes = Vec::new();
    let mut worker_memory = Vec::new();
    for rig in &run.rigs {
        let rig = rig.borrow();
        worker_memory.push(rig.wasm_memory_bytes);
        notes.extend(rig.notes.iter().rev().take(3).cloned());
    }
    LabView {
        phase: phase.to_string(),
        running,
        elapsed_s,
        generation: run.generation,
        aggregate: aggregate(&snapshots),
        cards,
        worker_wasm_memory_bytes: worker_memory,
        js_heap_bytes: js_heap_bytes(),
        notes,
    }
}

fn publish_phase(run: &Rc<RefCell<LabRun>>, view: &mut Signal<LabView>, phase: &str) {
    let next = build_view(&mut run.borrow_mut(), phase, false, 0.0);
    publish_stats_json(&next);
    view.set(next);
}

/// Mirror the current view as JSON at `window.__labStats` so automated sweeps
/// can read measurements without scraping the DOM.
fn publish_stats_json(view: &LabView) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(json) = serde_json::to_string(view) else {
        return;
    };
    let _ = js_sys::Reflect::set(
        &window,
        &JsValue::from_str("__labStats"),
        &JsValue::from_str(&json),
    );
}

fn terminate(rigs: &[Rc<RefCell<WorkerRig>>]) {
    for rig in rigs {
        rig.borrow().terminate();
    }
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map(|performance| performance.now())
        .unwrap_or_else(js_sys::Date::now)
}

fn epoch_now_ms() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map(|performance| performance.time_origin() + performance.now())
        .unwrap_or_else(js_sys::Date::now)
}

/// Chrome-only JS heap gauge (`performance.memory.usedJSHeapSize`).
fn js_heap_bytes() -> Option<f64> {
    let performance = web_sys::window()?.performance()?;
    let memory = js_sys::Reflect::get(&performance, &JsValue::from_str("memory")).ok()?;
    js_sys::Reflect::get(&memory, &JsValue::from_str("usedJSHeapSize"))
        .ok()?
        .as_f64()
}

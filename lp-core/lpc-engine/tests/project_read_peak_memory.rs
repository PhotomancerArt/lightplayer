//! Peak-memory probe for streamed project reads (the classic's OOM shape,
//! `docs/defects/2026-08-26-project-read-assembly-oom-resets-classic.md`).
//!
//! The full `Detail + include_slots` read is streamed twice over the shipped
//! mini-dome example under a byte-tracking allocator:
//!
//! - a HOLDING sink keeps every emitted event alive at once — the analog of
//!   the old `snapshot_node_slots` materialize-then-frame path (and of any
//!   future regression back to it);
//! - a DROPPING sink frees each event as it arrives — the per-root streaming
//!   path's steady state, where peak is one root's deep copy plus transients.
//!
//! The test asserts the dropping pass peaks well below the holding pass, and
//! prints both numbers so a bench session can quote them.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use lpc_engine::{Engine, EngineProjectReadSource, EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpc_registry::ProjectRegistry;
use lpc_shared::transport::ProjectReadEventSink;
use lpc_wire::{
    NodeReadQuery, ProjectReadEvent, ProjectReadQuery, ProjectReadRequest, ReadLevel,
    ResourceReadQuery, RuntimeReadQuery, ShapeReadQuery,
};
use lpfs::LpFsStd;

struct TrackingAlloc;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for TrackingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

/// The LIVE/PEAK counters are process-global, so concurrently running tests
/// would corrupt each other's baselines. Every test holds this for its whole
/// body.
static MEASURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn measure_lock() -> std::sync::MutexGuard<'static, ()> {
    MEASURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Reset the peak to the current live level; returns that baseline.
fn reset_peak() -> usize {
    let live = LIVE.load(Ordering::Relaxed);
    PEAK.store(live, Ordering::Relaxed);
    live
}

fn peak_above(baseline: usize) -> usize {
    PEAK.load(Ordering::Relaxed).saturating_sub(baseline)
}

/// Keeps every event alive for the duration of the stream.
#[derive(Default)]
struct HoldingSink {
    events: Vec<ProjectReadEvent>,
}

impl ProjectReadEventSink for HoldingSink {
    type Error = core::convert::Infallible;

    async fn send_project_read_event(
        &mut self,
        event: ProjectReadEvent,
    ) -> Result<(), Self::Error> {
        self.events.push(event);
        Ok(())
    }
}

/// Frees each event as it arrives; counts what it saw.
#[derive(Default)]
struct DroppingSink {
    events: usize,
    slot_roots: usize,
}

impl ProjectReadEventSink for DroppingSink {
    type Error = core::convert::Infallible;

    async fn send_project_read_event(
        &mut self,
        event: ProjectReadEvent,
    ) -> Result<(), Self::Error> {
        self.events += 1;
        if matches!(
            &event,
            ProjectReadEvent::Query {
                event: lpc_wire::ProjectReadQueryEvent::Nodes(
                    lpc_wire::ProjectReadNodeEvent::SlotRoot(_)
                ),
                ..
            }
        ) {
            self.slot_roots += 1;
        }
        Ok(())
    }
}

/// Null-waker executor for immediately-ready futures (sinks never pend).
fn block_on<F: core::future::Future>(future: F) -> F::Output {
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    unsafe fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(core::ptr::null(), &VTABLE)
    }
    unsafe fn no_op(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
    let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        if let Poll::Ready(output) = core::future::Future::poll(future.as_mut(), &mut cx) {
            return output;
        }
    }
}

fn nodes_detail_read() -> ProjectReadRequest {
    ProjectReadRequest {
        since: None,
        queries: vec![ProjectReadQuery::Nodes(NodeReadQuery::detail_all())],
        probes: Vec::new(),
    }
}

fn shapes_detail_read() -> ProjectReadRequest {
    ProjectReadRequest {
        since: None,
        queries: vec![ProjectReadQuery::Shapes(ShapeReadQuery {
            level: ReadLevel::Detail,
        })],
        probes: Vec::new(),
    }
}

/// The Studio initial-sync query set (minus probes) — the shape from
/// `lpa-studio-core`'s `project_read_request`.
fn studio_shaped_read() -> ProjectReadRequest {
    ProjectReadRequest {
        since: None,
        queries: vec![
            ProjectReadQuery::Shapes(ShapeReadQuery {
                level: ReadLevel::Detail,
            }),
            ProjectReadQuery::Nodes(NodeReadQuery::detail_all()),
            ProjectReadQuery::Resources(ResourceReadQuery::default()),
            ProjectReadQuery::Runtime(RuntimeReadQuery),
        ],
        probes: Vec::new(),
    }
}

fn load_mini_dome() -> (Engine, ProjectRegistry) {
    let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpc-engine lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf();
    let project_dir: PathBuf = workspace_dir.join("examples/mini-dome");
    let fs = LpFsStd::new(project_dir);
    let services = EngineServices::new(TreePath::parse("/mini_dome.show").expect("path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load mini-dome");
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    let (mut engine, registry) = rt.into_parts();
    for _ in 0..3 {
        engine.tick(&registry, 16).expect("tick");
    }
    (engine, registry)
}

/// Stream `request` twice — holding every event (the materialized analog)
/// then dropping each event as it arrives (the streaming steady state) —
/// asserting the streamed peak sits well below the held peak.
fn assert_streamed_peak_below_held(
    label: &str,
    engine: &mut Engine,
    registry: &ProjectRegistry,
    request: ProjectReadRequest,
) -> DroppingSink {
    // Holding pass: every event alive at once — the materialized analog.
    let baseline = reset_peak();
    let mut holding = HoldingSink::default();
    block_on(async {
        EngineProjectReadSource::new(engine, registry)
            .stream_project_read_events(request.clone(), &mut holding)
            .await
            .expect("holding stream");
    });
    let holding_peak = peak_above(baseline);
    let held_events = holding.events.len();
    drop(holding);

    // Dropping pass: events freed as they arrive — the streaming steady state.
    let baseline = reset_peak();
    let mut dropping = DroppingSink::default();
    block_on(async {
        EngineProjectReadSource::new(engine, registry)
            .stream_project_read_events(request, &mut dropping)
            .await
            .expect("dropping stream");
    });
    let dropping_peak = peak_above(baseline);

    println!(
        "project-read peak above baseline [{label}]: held={holding_peak}B \
         streamed={dropping_peak}B ({held_events} events, {} slot roots)",
        dropping.slot_roots
    );

    assert_eq!(
        held_events, dropping.events,
        "[{label}] the two passes must stream identical events"
    );
    // The streaming pass must peak strictly below the materialized analog;
    // 2x margin keeps this from flaking on allocator noise while still
    // failing loudly if per-item streaming regresses to materialize-first.
    assert!(
        dropping_peak * 2 < holding_peak,
        "[{label}] streamed peak {dropping_peak}B is not well below held peak {holding_peak}B"
    );
    dropping
}

#[test]
fn streamed_slot_roots_peak_below_materialized_forest() {
    let _guard = measure_lock();
    let (mut engine, registry) = load_mini_dome();
    let dropping =
        assert_streamed_peak_below_held("nodes", &mut engine, &registry, nodes_detail_read());
    assert!(
        dropping.slot_roots > 0,
        "probe read produced no slot roots — the fixture or request no longer \
         exercises the OOM shape"
    );
}

#[test]
fn streamed_shape_entries_peak_below_materialized_registry() {
    let _guard = measure_lock();
    let (mut engine, registry) = load_mini_dome();
    let dropping =
        assert_streamed_peak_below_held("shapes", &mut engine, &registry, shapes_detail_read());
    assert!(
        dropping.events > 3,
        "shapes read produced no entries — the fixture no longer exercises \
         the registry-clone shape"
    );
}

#[test]
fn studio_shaped_read_streams_below_materialized() {
    let _guard = measure_lock();
    let (mut engine, registry) = load_mini_dome();
    let dropping = assert_streamed_peak_below_held(
        "studio-shaped",
        &mut engine,
        &registry,
        studio_shaped_read(),
    );
    assert!(dropping.slot_roots > 0, "studio-shaped read lost its slots");
}

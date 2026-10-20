//! Server loop for ESP32 firmware
//!
//! Main async loop that handles hardware I/O and calls lpa-server::tick().
//!
//! # Heartbeat Messages
//!
//! The server loop sends periodic heartbeat messages (every 5 seconds) containing:
//! - Current FPS (frames per second)
//! - Total frame count since startup
//! - List of loaded projects
//! - Server uptime in milliseconds
//! - Memory statistics (heap free/used from esp_alloc)
//!
//! These messages use `ServerMessage` with `id: 0` to indicate they are unsolicited
//! status updates (not responses to client requests). Clients can subscribe to these
//! messages to monitor server health or ignore them if not needed.
//!
//! See `lp-model/src/server/api.rs` for the `ServerMsgBody::Heartbeat` variant definition.
//!
//! # Prior Art
//!
//! This implementation follows the pattern established in `fw-esp32/src/tests/test_usb.rs`
//! which sends heartbeat messages for debugging. This implementation makes heartbeat messages
//! part of the formal protocol using proper `ServerMessage` types with `M!` prefix.

use alloc::vec::Vec;
use lpa_server::LpServer;
use lpc_shared::fps::FpsTracker;
use lpc_shared::stats::WindowedStatsCollector;
use lpc_shared::time::TimeProvider;
use lpc_shared::transport::{Incoming, Link, ServerTransport};

use crate::link_upkeep::LinkUpkeep;
use crate::time::Esp32TimeProvider;

/// The most one server pass may spend waiting on a link's send path (a
/// reply waiting for the frame buffer, a radio link letting go of it),
/// counted from the pass's start (the last [`LinkUpkeep::upkeep`]) and
/// including whatever the pass did first — a project load, a shader
/// compile. One pass must stay under the RTC watchdog (stage 0, 8 s, fed
/// once per pass below), with room left for the rest of the pass: a load
/// followed by a stalled peer's 5 s wait reset the emulated C6 in a loop
/// (PR C's walk, 12 watchdog resets), and a slow-draining USB host did the
/// same through a multi-frame `ProjectRead`, each frame waiting up to
/// `SEND_ROOM_WAIT` on its own (ticket
/// `2026-10-20-send-room-wait-starves-watchdog`). The radio links, the USB
/// link and the classic's UART link all end their waits at this one
/// deadline; each transport's per-frame wait only ever shortens under it.
pub const TICK_WAIT_LIMIT_MS: u32 = 5_000;

/// The chip's per-frame hook (`0`: none): see [`set_frame_hook`].
static FRAME_HOOK: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// A chip's own per-frame upkeep that needs the server — once per loop
/// pass, after the link upkeep. The C6's split image keeps its update
/// light's record here (`/.lp/status-light.json`, written through the
/// server's filesystem when a project's first WS281x output opens). Keep it
/// cheap: it runs every frame.
pub fn set_frame_hook(hook: fn(&LpServer)) {
    FRAME_HOOK.store(hook as usize, core::sync::atomic::Ordering::Release);
}

fn run_frame_hook(server: &LpServer) {
    let raw = FRAME_HOOK.load(core::sync::atomic::Ordering::Acquire);
    if raw != 0 {
        // SAFETY: only `set_frame_hook` stores here, and it stores a
        // `fn(&LpServer)`.
        let hook: fn(&LpServer) = unsafe { core::mem::transmute(raw) };
        hook(server);
    }
}

/// The chip's hello gate (`0`: none, always open): see [`set_hello_gate`].
static HELLO_GATE: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Hold the loop's unsolicited hello until `ready` says it may go. The C6's
/// split image takes its core's own SHA-256 — which the hello's board
/// manifest carries — after the boot's first light rather than before it,
/// and holds the hello for it, so building the hello never stalls the boot
/// for the hash. Heartbeats wait for the unsolicited hello, which stays the
/// first id-0 frame, and the hosts' requests wait with it (a host's first is
/// its `Hello`, and answering it would build the hello). `ready(wanted)`:
/// `wanted` says a host is waiting on a held request, so the chip may open
/// the gate sooner and pay for the hello then. Nothing waits longer than
/// [`HELLO_HOLD_MAX_MS`] from the loop's start. Unset, the hello goes on the
/// first pass, as it always did.
pub fn set_hello_gate(ready: fn(bool) -> bool) {
    HELLO_GATE.store(ready as usize, core::sync::atomic::Ordering::Release);
}

fn hello_gate_open(wanted: bool) -> bool {
    let raw = HELLO_GATE.load(core::sync::atomic::Ordering::Acquire);
    if raw == 0 {
        return true;
    }
    // SAFETY: only `set_hello_gate` stores here, and it stores a
    // `fn(bool) -> bool`.
    let ready: fn(bool) -> bool = unsafe { core::mem::transmute(raw) };
    ready(wanted)
}

/// The longest the hello gate holds the hello, from the loop's start: past
/// it the hello goes whatever the gate says (and on the C6 hashes the core
/// then, as every boot did before the gate). A slow first light (a big
/// shader's compile) must not keep a host waiting for its hello.
pub const HELLO_HOLD_MAX_MS: u64 = 2_000;

/// Performance logging interval.
const PERF_LOG_INTERVAL_MS: u64 = 5000;

/// Heartbeat message interval (send every N milliseconds)
const HEARTBEAT_INTERVAL_MS: u64 = 5000; // Send every 5 seconds

/// FPS statistics window: stats are computed over samples from the last N milliseconds
const FPS_STATS_WINDOW_MS: u64 = 5000;

/// Run the server loop
///
/// This is the main async loop that processes incoming messages and sends responses.
/// Runs at ~60 FPS to maintain consistent frame timing.
/// Yields control back to Embassy runtime between iterations.
///
/// This is a deliberate duplicate of [`run_server_loop_bounded`]'s body under
/// `FrameBudget::UNBOUNDED`, not a thin wrapper around it. A wrapper was
/// tried first — it is what M5 P0 shipped initially — and the C6's own
/// heap-budget ratchet caught its cost on the *shipped* image (no bench
/// feature involved): the ESP32C6's async-fn-of-an-async-fn nesting moved the
/// main task's `stackHighWater` from 11,908 B to 13,428 B and its
/// `stackTotal` from 71,152 B to 70,736 B, and `#[inline(always)]` on the
/// bounded function did not fold it back — that attribute reaches the
/// function that constructs the `impl Future`, not the generated `poll`
/// wired up by `.await`, so the nested state machine stayed nested. On a
/// RAM-tight chip a ~1.5 KB stack regression on the product's own path is not
/// something a benchmark PR gets to introduce, so this function is now byte-
/// for-byte the pre-P0 implementation, and [`run_server_loop_bounded`] is a
/// second, separate copy that only the `bench_render_loop` call site in
/// `fw-esp32c6/src/main.rs` ever calls. Keep the two loop bodies in sync by
/// hand if either changes.
pub async fn run_server_loop<T: ServerTransport + LinkUpkeep>(
    mut server: LpServer,
    mut transport: T,
    time_provider: Esp32TimeProvider,
    // Chip-injected seams: heap stats for the heartbeat (including the
    // fragmentation numbers only the chip can probe) and the watchdog feed.
    // Both are chip facts this crate must not know.
    memory_stats: fn() -> Option<lpc_wire::server::MemoryStats>,
    mut feed_watchdog: impl FnMut(u64),
) -> ! {
    // Wire hello: the first id-0 frame this loop ever sends, so clients can
    // check the protocol version before anything else arrives (see
    // docs/adr/2026-07-14-wire-hello-versioning.md). Sent at the top of the
    // first pass the chip's hello gate is open ([`set_hello_gate`]), to the
    // links open now; no heartbeat goes before it.
    let mut hello_owed = true;
    let mut boot_links = transport.links();
    let mut held_links: Vec<Link> = Vec::new();
    let mut held_asks: Vec<Incoming> = Vec::new();

    let mut last_tick = time_provider.now_ms();
    let mut frame_count = 0u32;
    let mut fps_tracker = FpsTracker::new(time_provider.now_ms());
    let mut heartbeat_last_sent = time_provider.now_ms();
    let startup_time = time_provider.now_ms();
    let mut fps_collector = WindowedStatsCollector::new();
    let mut boot_completed = false;

    // One legible error line if the previous run ended in a crash; flows to
    // the client through the normal log transport.
    if let Some(snapshot) = lp_recovery::snapshot()
        && let Some(crash) = snapshot.last_crash
        && crash.boots_ago == 1
    {
        log::error!(
            "[RECOVERY] previous run crashed ({}): at {}: {}",
            crash.cause.as_str(),
            crash.path_display(),
            crash.msg.as_str()
        );
    }

    let mut frame_times = crate::frame_time_stats::FrameTimeStats::default();
    let mut last_frame: Option<(u64, crate::frame_time_stats::SlowFrame)> = None;
    loop {
        let frame_start = time_provider.now_ms();
        // The frame that just ended is counted whole, start to start: its
        // heartbeats, upkeep and yield are frame time a viewer waits for.
        if let Some((started, mut frame)) = last_frame.take() {
            frame.total_ms = frame_start.saturating_sub(started).min(u64::from(u32::MAX)) as u32;
            frame_times.record(frame);
        }
        #[cfg(feature = "frame-pace-diag")]
        crate::frame_pace_diag::frame_start();

        // A link that joined since the last frame (a radio connection) gets
        // its own hello before anything else is sent to it — once the hello
        // is no longer held; until then it waits with the rest.
        held_links.extend(transport.take_opened_links());
        if hello_owed
            && (hello_gate_open(!held_asks.is_empty())
                || time_provider.now_ms().saturating_sub(startup_time) >= HELLO_HOLD_MAX_MS)
        {
            hello_owed = false;
        }
        if !hello_owed && !(boot_links.is_empty() && held_links.is_empty()) {
            send_hellos(
                &server,
                &mut transport,
                core::mem::take(&mut boot_links),
                core::mem::take(&mut held_links),
            )
            .await;
        }

        // Collect incoming messages (non-blocking)
        let receive_start = time_provider.now_ms();
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive().await {
                Ok(Some(msg)) => {
                    incoming_messages.push(msg);
                }
                Ok(None) => {
                    // No more messages available
                    break;
                }
                Err(e) => {
                    // Transport error - log and continue
                    log::warn!("run_server_loop: Transport error: {e:?}");
                    break;
                }
            }
        }
        // While the hello is held, so are the hosts' requests: the first a
        // host sends is its `Hello`, and answering it would build the hello
        // the gate is holding (on the C6, hash the core before its first
        // light). They are answered, in order, in the tick after it opens.
        if hello_owed {
            held_asks.append(&mut incoming_messages);
        } else if !held_asks.is_empty() {
            held_asks.append(&mut incoming_messages);
            incoming_messages = core::mem::take(&mut held_asks);
        }
        let receive_done = time_provider.now_ms();

        // Calculate delta time since last tick
        let delta_time = time_provider.elapsed_ms(last_tick);
        let delta_ms = delta_time.min(u32::MAX as u64) as u32;

        // Tick server (synchronous)
        let tick_start = time_provider.now_ms();
        #[cfg(feature = "frame-pace-diag")]
        let pace_tick_start = crate::frame_pace_diag::now_us();
        let (tick_ms, send_ms, total_ms, response_count) = match server
            .tick_and_send(delta_ms.max(1), incoming_messages, &mut transport)
            .await
        {
            Ok(response_count) => {
                let tick_done = time_provider.now_ms();
                let send_done = time_provider.now_ms();
                server.set_last_frame_time(send_done.saturating_sub(frame_start) * 1000);
                if !boot_completed {
                    // First successful frame: the boot survived. Recovery's
                    // boot-loop counter resets at the next boot.
                    boot_completed = true;
                    lp_recovery::mark_boot_complete();
                    log::info!("[RECOVERY] boot complete (first frame served)");
                }
                (
                    tick_done.saturating_sub(tick_start),
                    send_done.saturating_sub(tick_done),
                    send_done.saturating_sub(frame_start),
                    response_count,
                )
            }
            Err(e) => {
                let tick_done = time_provider.now_ms();
                server.set_last_frame_time(tick_done.saturating_sub(frame_start) * 1000);
                log::warn!("run_server_loop: Server tick error: {e:?}");
                // Server error - continue
                (
                    tick_done.saturating_sub(tick_start),
                    0,
                    tick_done.saturating_sub(frame_start),
                    0,
                )
            }
        };

        #[cfg(feature = "frame-pace-diag")]
        crate::frame_pace_diag::frame_end(
            crate::frame_pace_diag::now_us().saturating_sub(pace_tick_start),
            response_count,
        );
        last_tick = frame_start;
        frame_count += 1;
        last_frame = Some((
            frame_start,
            crate::frame_time_stats::SlowFrame {
                total_ms: 0,
                recv_ms: receive_done.saturating_sub(receive_start) as u32,
                tick_ms: tick_ms as u32,
                send_ms: send_ms as u32,
                responses: response_count as u32,
            },
        ));

        let current_time = time_provider.now_ms();
        if current_time.saturating_sub(fps_tracker.last_log_time_ms()) >= PERF_LOG_INTERVAL_MS {
            let elapsed_ms = current_time.saturating_sub(fps_tracker.last_log_time_ms());
            if elapsed_ms > 0 {
                let frames_done = frame_count.saturating_sub(fps_tracker.last_log_frame());
                let fps = (frames_done as u64 * 1000) / elapsed_ms;
                log::info!(
                    "[perf] frame={} fps={} elapsed={}ms recv={}ms tick={}ms send={}ms total={}ms responses={}",
                    frame_count,
                    fps,
                    elapsed_ms,
                    receive_done.saturating_sub(receive_start),
                    tick_ms,
                    send_ms,
                    total_ms,
                    response_count,
                );
                // A line of its own: beside the [perf] line it ran past a log
                // record's 200 B and lost its hiccup counts.
                log::info!("[perf] frames {frame_times}");
                frame_times.reset();
                fps_tracker.record_log(frame_count, current_time);
            }
        }

        // Send heartbeat message periodically
        // See prior art: fw-esp32/src/tests/test_usb.rs heartbeat_task()
        // This implementation uses proper ServerMessage types with M! prefix
        // Heartbeats flow on their interval even while responses are being
        // served: a long multi-frame read is exactly when the memory and
        // link-loss numbers matter most, and suppressing the heartbeat there
        // made a busy device doubly invisible.
        if !hello_owed && current_time.saturating_sub(heartbeat_last_sent) >= HEARTBEAT_INTERVAL_MS
        {
            let fps_current = fps_tracker
                .instantaneous_fps(frame_count, current_time, startup_time)
                .unwrap_or(0.0);

            fps_collector.push(current_time, fps_current);
            fps_collector.prune_older_than(current_time.saturating_sub(FPS_STATS_WINDOW_MS));
            let fps_stats = fps_collector.compute_stats();

            // One heartbeat per open link (unsolicited: id 0, single/final
            // message). The server builds each for the link it goes to: a
            // link that holds no tier is told nothing the hello does not.
            // The heap figures first, then the link list: a heartbeat's
            // memory numbers must not count the transient Vec it is sent
            // through (the chip ratchet reads them to the byte).
            let status = heartbeat_status(
                &server,
                fps_stats,
                frame_count,
                current_time,
                startup_time,
                memory_stats(),
            );
            let heartbeats = server.heartbeats(&transport.links(), status);
            for (link, heartbeat_msg) in heartbeats {
                // Send heartbeat (non-blocking, ignore errors)
                if let Err(e) = transport.send(link, heartbeat_msg).await {
                    note_send_failure("heartbeat", &e);
                }
            }

            heartbeat_last_sent = current_time;
        }

        // Feed the RWDT while the loop and the I/O task are both alive; a
        // hang anywhere stops the feeding and the watchdog resets us with
        // the recovery frame stack as the blame record.
        // Link policy that needs the server and the clock (the radio links'
        // login deadline). The clock now, not the pass's `current_time`: the
        // heartbeat sends above can wait seconds, and the mux judges its
        // login deadlines against what it is given. (The per-pass send
        // deadline the transports start here reads their own clock.)
        transport.upkeep(&server, time_provider.now_ms());
        run_frame_hook(&server);

        // The clock now, not the pass's `current_time`: the heartbeat sends
        // above can wait seconds on a slow host, and a feed stamped with the
        // clock from before them makes the next pass see the I/O task as
        // silent for that long and withhold the feed — a false alarm.
        feed_watchdog(time_provider.now_ms());

        // Yield to Embassy runtime (allows other tasks to run)
        // Use embassy_time::Timer to delay slightly
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}

/// The hellos due, once the gate is open: the unsolicited one to every link
/// that was open when the loop started (`boot`; what
/// `fw_core::send_unsolicited_hello` sends), then the one each newly opened
/// link is owed (`opened`; a single-link transport's is its session's `Up`),
/// unless the unsolicited one just went to it: one hello per link per boot.
/// A failure is logged and not retried: the link's own `Hello` request still
/// gets an answer.
async fn send_hellos<T: ServerTransport>(
    server: &LpServer,
    transport: &mut T,
    boot: Vec<Link>,
    opened: Vec<Link>,
) {
    let owed = opened.into_iter().filter(|link| !boot.contains(link));
    for link in boot.iter().copied().chain(owed) {
        if let Err(e) = fw_core::send_hello_to_link(server, transport, link).await {
            note_send_failure("hello", &e);
        }
    }
}

/// An unsolicited frame (a hello, a heartbeat) was not sent. No host session
/// to send it to (`ConnectionLost` — a board with nothing attached, every five
/// seconds) is routine and counted by the transport; logging it would fill
/// the log ring a host reads when it does attach. Anything else is news.
/// A plain function, so the loop futures carry none of it.
#[inline(never)]
fn note_send_failure(what: &str, error: &lpc_wire::TransportError) {
    if matches!(error, lpc_wire::TransportError::ConnectionLost) {
        log::debug!("run_server_loop: no host session for the {what}");
    } else {
        log::warn!("run_server_loop: failed to send the {what}: {error:?}");
    }
}

/// What this loop measured for one heartbeat: the embedder's half. A plain
/// function (not an `async` one, not a closure over the loop's state) so it
/// adds nothing to the loop future's frame — see [`run_server_loop`] on why
/// that matters on the C6.
#[inline(never)]
fn heartbeat_status(
    server: &LpServer,
    fps: lpc_wire::server::SampleStats,
    frame_count: u32,
    current_time: u64,
    startup_time: u64,
    memory: Option<lpc_wire::server::MemoryStats>,
) -> lpa_server::HeartbeatStatus {
    lpa_server::HeartbeatStatus {
        fps,
        frame_count: u64::from(frame_count),
        uptime_ms: current_time.saturating_sub(startup_time),
        memory,
        recovery: lpa_server::recovery_report::current_recovery_status().map(|mut status| {
            // The clamp is server state, not recovery-region state — stamp
            // it here where both are in scope.
            status.output_clamp = server.safe_output_clamp();
            status
        }),
        outputs: crate::output::wire_stats_source::current(),
        link: heartbeat_link_counters(),
    }
}

/// The host link's counters for the heartbeat: the lp-link link's own — the
/// USB link's on the C6 and S3, the UART link's on the classic — or `None`
/// on an image with neither (a host test build: every shipped image has one,
/// and no board link in this crate speaks `M!` any more).
#[inline(always)]
fn heartbeat_link_counters() -> Option<lpc_wire::server::LinkCounters> {
    #[cfg(feature = "usb-link")]
    return crate::usb_link::usb_link_counters::heartbeat();
    #[cfg(all(feature = "uart-link", not(feature = "usb-link")))]
    return crate::uart_link::uart_link_counters::heartbeat();
    #[cfg(not(any(feature = "usb-link", feature = "uart-link")))]
    return None;
}

/// How many frames the loop runs for, and on whose clock.
///
/// The product passes [`FrameBudget::UNBOUNDED`] and nothing below it does
/// anything. A **benchmark** image passes a frame count and a fixed delta, and
/// that combination is what turns the server loop into a measurable workload:
/// a bounded run can print a summary and stop, and a fixed delta makes frame
/// *content* a function of the frame index rather than of whatever the clock
/// happened to say — which is what lets two runs of two different emulator
/// binaries be compared frame for frame.
///
/// This is not a chip fact, so it does not violate this crate's seam rules
/// (ADR 2026-07-29): it is a property of the run, injected by whoever starts
/// the loop.
#[derive(Debug, Clone, Copy)]
pub struct FrameBudget {
    /// Stop after this many frames. `None` runs for ever.
    pub frames: Option<u32>,
    /// Tick every project with this delta instead of the measured one.
    /// `None` uses the wall clock, which is what the product does.
    pub fixed_delta_ms: Option<u32>,
    /// A free-running cycle counter, injected because reading one is a chip
    /// fact (the C6's is behind CSR 0x7E2 and has no portable spelling — the
    /// same seam `jit-math-perf` uses). `None` reports every frame as zero.
    pub cycles: Option<fn() -> u32>,
}

impl FrameBudget {
    /// What the product runs: no end, and the real clock.
    pub const UNBOUNDED: Self = Self {
        frames: None,
        fixed_delta_ms: None,
        cycles: None,
    };
}

/// The server loop, with an optional end and an optional fixed delta.
///
/// `on_frame` is handed the cost of each frame's `tick_and_send` — the render
/// — in whatever unit the caller's counter counts, and nothing else. It
/// deliberately does **not** see the whole loop iteration: the iteration also
/// carries the transport poll, the heartbeat and a 1 ms yield, and a "render
/// time" that included the yield would be a measurement of `Timer::after`.
///
/// It is called once per frame inside the loop being measured, so it must not
/// print, allocate or await. The render-loop payload folds it into a
/// min/max/sum accumulator and prints once, after the last frame — every
/// earlier benchmark on this ladder turned out to be measuring its own
/// logging.
///
/// Only `bench_render_loop`'s call site in `fw-esp32c6/src/main.rs` calls
/// this — the shipped path calls [`run_server_loop`], a separate copy of the
/// same loop, not this one. See that function's doc comment for why they are
/// two copies instead of one wrapping the other.
pub async fn run_server_loop_bounded<T: ServerTransport + LinkUpkeep>(
    mut server: LpServer,
    mut transport: T,
    time_provider: Esp32TimeProvider,
    memory_stats: fn() -> Option<lpc_wire::server::MemoryStats>,
    mut feed_watchdog: impl FnMut(u64),
    budget: FrameBudget,
    mut on_frame: impl FnMut(u32),
) -> LpServer {
    // Wire hello: the first id-0 frame this loop ever sends, so clients can
    // check the protocol version before anything else arrives (see
    // docs/adr/2026-07-14-wire-hello-versioning.md). Sent at the top of the
    // first pass the chip's hello gate is open ([`set_hello_gate`]), to the
    // links open now; no heartbeat goes before it.
    let mut hello_owed = true;
    let mut boot_links = transport.links();
    let mut held_links: Vec<Link> = Vec::new();
    let mut held_asks: Vec<Incoming> = Vec::new();

    let mut last_tick = time_provider.now_ms();
    let mut frame_count = 0u32;
    let mut fps_tracker = FpsTracker::new(time_provider.now_ms());
    let mut heartbeat_last_sent = time_provider.now_ms();
    let startup_time = time_provider.now_ms();
    let mut fps_collector = WindowedStatsCollector::new();
    let mut boot_completed = false;

    // One legible error line if the previous run ended in a crash; flows to
    // the client through the normal log transport.
    if let Some(snapshot) = lp_recovery::snapshot()
        && let Some(crash) = snapshot.last_crash
        && crash.boots_ago == 1
    {
        log::error!(
            "[RECOVERY] previous run crashed ({}): at {}: {}",
            crash.cause.as_str(),
            crash.path_display(),
            crash.msg.as_str()
        );
    }

    let mut frame_times = crate::frame_time_stats::FrameTimeStats::default();
    let mut last_frame: Option<(u64, crate::frame_time_stats::SlowFrame)> = None;
    loop {
        let frame_start = time_provider.now_ms();
        // The frame that just ended is counted whole, start to start: its
        // heartbeats, upkeep and yield are frame time a viewer waits for.
        if let Some((started, mut frame)) = last_frame.take() {
            frame.total_ms = frame_start.saturating_sub(started).min(u64::from(u32::MAX)) as u32;
            frame_times.record(frame);
        }

        // A link that joined since the last frame (a radio connection) gets
        // its own hello before anything else is sent to it — once the hello
        // is no longer held; until then it waits with the rest.
        held_links.extend(transport.take_opened_links());
        if hello_owed
            && (hello_gate_open(!held_asks.is_empty())
                || time_provider.now_ms().saturating_sub(startup_time) >= HELLO_HOLD_MAX_MS)
        {
            hello_owed = false;
        }
        if !hello_owed && !(boot_links.is_empty() && held_links.is_empty()) {
            send_hellos(
                &server,
                &mut transport,
                core::mem::take(&mut boot_links),
                core::mem::take(&mut held_links),
            )
            .await;
        }

        // Collect incoming messages (non-blocking)
        let receive_start = time_provider.now_ms();
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive().await {
                Ok(Some(msg)) => {
                    incoming_messages.push(msg);
                }
                Ok(None) => {
                    // No more messages available
                    break;
                }
                Err(e) => {
                    // Transport error - log and continue
                    log::warn!("run_server_loop: Transport error: {e:?}");
                    break;
                }
            }
        }
        // While the hello is held, so are the hosts' requests: the first a
        // host sends is its `Hello`, and answering it would build the hello
        // the gate is holding (on the C6, hash the core before its first
        // light). They are answered, in order, in the tick after it opens.
        if hello_owed {
            held_asks.append(&mut incoming_messages);
        } else if !held_asks.is_empty() {
            held_asks.append(&mut incoming_messages);
            incoming_messages = core::mem::take(&mut held_asks);
        }
        let receive_done = time_provider.now_ms();

        // Calculate delta time since last tick. A benchmark budget replaces
        // it with a fixed one so that frame N is the same frame N on every
        // run, whatever the clock did.
        let delta_time = time_provider.elapsed_ms(last_tick);
        let delta_ms = match budget.fixed_delta_ms {
            Some(fixed) => fixed,
            None => delta_time.min(u32::MAX as u64) as u32,
        };

        // Tick server (synchronous)
        let tick_start = time_provider.now_ms();
        // The render's own cost, on the cycle counter rather than the
        // millisecond clock: a 16 ms frame is 16 ticks of `now_ms` and
        // 2.6 million cycles, and only one of those two can tell two frames
        // apart. Read immediately around `tick_and_send` and nothing else.
        let render_from = budget.cycles.map(|read| read());
        let (tick_ms, send_ms, total_ms, response_count) = match server
            .tick_and_send(delta_ms.max(1), incoming_messages, &mut transport)
            .await
        {
            Ok(response_count) => {
                let tick_done = time_provider.now_ms();
                let send_done = time_provider.now_ms();
                server.set_last_frame_time(send_done.saturating_sub(frame_start) * 1000);
                if !boot_completed {
                    // First successful frame: the boot survived. Recovery's
                    // boot-loop counter resets at the next boot.
                    boot_completed = true;
                    lp_recovery::mark_boot_complete();
                    log::info!("[RECOVERY] boot complete (first frame served)");
                }
                (
                    tick_done.saturating_sub(tick_start),
                    send_done.saturating_sub(tick_done),
                    send_done.saturating_sub(frame_start),
                    response_count,
                )
            }
            Err(e) => {
                let tick_done = time_provider.now_ms();
                server.set_last_frame_time(tick_done.saturating_sub(frame_start) * 1000);
                log::warn!("run_server_loop: Server tick error: {e:?}");
                // Server error - continue
                (
                    tick_done.saturating_sub(tick_start),
                    0,
                    tick_done.saturating_sub(frame_start),
                    0,
                )
            }
        };

        // The render is over; hand its cost to the caller before anything
        // else in this iteration can add to it.
        if let (Some(read), Some(from)) = (budget.cycles, render_from) {
            on_frame(read().wrapping_sub(from));
        } else {
            on_frame(0);
        }

        last_tick = frame_start;
        frame_count += 1;
        last_frame = Some((
            frame_start,
            crate::frame_time_stats::SlowFrame {
                total_ms: 0,
                recv_ms: receive_done.saturating_sub(receive_start) as u32,
                tick_ms: tick_ms as u32,
                send_ms: send_ms as u32,
                responses: response_count as u32,
            },
        ));

        let current_time = time_provider.now_ms();
        if current_time.saturating_sub(fps_tracker.last_log_time_ms()) >= PERF_LOG_INTERVAL_MS {
            let elapsed_ms = current_time.saturating_sub(fps_tracker.last_log_time_ms());
            if elapsed_ms > 0 {
                let frames_done = frame_count.saturating_sub(fps_tracker.last_log_frame());
                let fps = (frames_done as u64 * 1000) / elapsed_ms;
                log::info!(
                    "[perf] frame={} fps={} elapsed={}ms recv={}ms tick={}ms send={}ms total={}ms responses={}",
                    frame_count,
                    fps,
                    elapsed_ms,
                    receive_done.saturating_sub(receive_start),
                    tick_ms,
                    send_ms,
                    total_ms,
                    response_count,
                );
                // A line of its own: beside the [perf] line it ran past a log
                // record's 200 B and lost its hiccup counts.
                log::info!("[perf] frames {frame_times}");
                frame_times.reset();
                fps_tracker.record_log(frame_count, current_time);
            }
        }

        // Send heartbeat message periodically
        // See prior art: fw-esp32/src/tests/test_usb.rs heartbeat_task()
        // This implementation uses proper ServerMessage types with M! prefix
        // Heartbeats flow on their interval even while responses are being
        // served: a long multi-frame read is exactly when the memory and
        // link-loss numbers matter most, and suppressing the heartbeat there
        // made a busy device doubly invisible.
        if !hello_owed && current_time.saturating_sub(heartbeat_last_sent) >= HEARTBEAT_INTERVAL_MS
        {
            let fps_current = fps_tracker
                .instantaneous_fps(frame_count, current_time, startup_time)
                .unwrap_or(0.0);

            fps_collector.push(current_time, fps_current);
            fps_collector.prune_older_than(current_time.saturating_sub(FPS_STATS_WINDOW_MS));
            let fps_stats = fps_collector.compute_stats();

            // One heartbeat per open link (unsolicited: id 0, single/final
            // message). The server builds each for the link it goes to: a
            // link that holds no tier is told nothing the hello does not.
            // The heap figures first, then the link list: a heartbeat's
            // memory numbers must not count the transient Vec it is sent
            // through (the chip ratchet reads them to the byte).
            let status = heartbeat_status(
                &server,
                fps_stats,
                frame_count,
                current_time,
                startup_time,
                memory_stats(),
            );
            let heartbeats = server.heartbeats(&transport.links(), status);
            for (link, heartbeat_msg) in heartbeats {
                // Send heartbeat (non-blocking, ignore errors)
                if let Err(e) = transport.send(link, heartbeat_msg).await {
                    note_send_failure("heartbeat", &e);
                }
            }

            heartbeat_last_sent = current_time;
        }

        // Feed the RWDT while the loop and the I/O task are both alive; a
        // hang anywhere stops the feeding and the watchdog resets us with
        // the recovery frame stack as the blame record.
        // Link policy that needs the server and the clock (the radio links'
        // login deadline). The clock now, not the pass's `current_time`: the
        // heartbeat sends above can wait seconds, and the mux judges its
        // login deadlines against what it is given. (The per-pass send
        // deadline the transports start here reads their own clock.)
        transport.upkeep(&server, time_provider.now_ms());
        run_frame_hook(&server);

        // The clock now, not the pass's `current_time`: the heartbeat sends
        // above can wait seconds on a slow host, and a feed stamped with the
        // clock from before them makes the next pass see the I/O task as
        // silent for that long and withhold the feed — a false alarm.
        feed_watchdog(time_provider.now_ms());

        // Yield to Embassy runtime (allows other tasks to run)
        // Use embassy_time::Timer to delay slightly
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;

        // A benchmark budget ends here — after the yield, so the last frame's
        // log and heartbeat bytes have had the same chance to reach the I/O
        // task as every other frame's.
        if let Some(limit) = budget.frames
            && frame_count >= limit
        {
            // Handed back rather than dropped: the caller's summary wants the
            // heap figures for a machine with the project still loaded, and
            // dropping the server here would report a machine that had just
            // unloaded one. (Measured: 316 kB free after the drop against
            // 219 kB with the project live.)
            return server;
        }
    }
}

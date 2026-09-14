//! Driving one board through the ramp.
//!
//! The schedule ([`super::schedule`]) decides *what* to try; this module does
//! it: deploy the workload, wait for it to render, settle while watching the
//! free heap, and — when the board stops answering — find out whether it ran
//! out of memory.
//!
//! Death is never inferred from the shape of a transport error. Any step that
//! fails for any reason is a *candidate* death; the answer comes from the
//! device itself, on the next boot, from the RTC recovery ledger carried in
//! the heartbeat (`recovery.last_crash.cause == "oom"`). A candidate death
//! that the ledger does not explain stops the run and is reported verbatim —
//! guessing here would poison a checked-in measurement.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use lpa_client::{ClientEvent, ClientIo, LpClient, ProjectDeployFile};
use lpa_link::{
    DeviceDeadlines, DeviceEvent, DeviceEventSink, DeviceLineOrigin, DeviceSession,
    LinkManagementRequest,
};
use lpc_model::HwEndpointSpec;
use lpc_wire::server::RecoveryStatus;
use lpc_wire::{
    ProjectReadEvent, ProjectReadQuery, ProjectReadQueryEvent, ProjectReadRequest,
    RuntimeReadQuery, ServerHello, WireProjectHandle,
};

use crate::client::cli_connect::connect_serial_device;

use super::schedule::{BenchSchedule, ScheduleStep, StepOutcome};
use super::workload::BenchWorkload;

/// Project id every step deploys under. Fixed on purpose: each step's files
/// overwrite the previous step's, so the device never accumulates one project
/// directory per LED count (the file *names* are identical between steps, so
/// nothing is left behind either).
pub const BENCH_PROJECT_ID: &str = "soft-limit-bench";

/// How long a deployed workload gets to render its first frame. Generous: on
/// a cold cache the device compiles the shader first.
const RUN_TIMEOUT: Duration = Duration::from_secs(60);

/// How long a rendering workload is watched before it counts as survived.
/// Part of the metric definition — an OOM often arrives a few frames in, not
/// at load.
const SETTLE: Duration = Duration::from_secs(20);

/// Gap between polls while waiting for the workload's first frames.
const RENDER_POLL: Duration = Duration::from_millis(500);

/// Gap between free-heap samples during the settle.
const SETTLE_POLL: Duration = Duration::from_secs(2);

/// Boot budget for the bench. The default 5 s assumes a healthy boot; after
/// an OOM the device auto-loads the same killer project and may need a couple
/// of boots before the recovery ladder skips auto-load.
const BENCH_READY_DEADLINE: Duration = Duration::from_secs(45);

/// Reconnect attempts after a candidate death, for the same reason.
const RECONNECT_ATTEMPTS: u32 = 3;

/// How long to wait for the first heartbeat after a reconnect. Heartbeats are
/// on a 5 s cadence.
const HEARTBEAT_WAIT: Duration = Duration::from_secs(20);

/// Gap between heartbeat polls.
const HEARTBEAT_POLL: Duration = Duration::from_millis(500);

/// How recent the ledger's OOM has to be to explain *this* step's death.
///
/// One boot ago is the expected answer: the crash is committed on the boot
/// after it happened. But the crash boot re-loads the same killer project,
/// which crash-loops until the recovery ladder quarantines it (observed: 2-3
/// extra crash boots on the C6), so the window covers the ladder. Anything
/// older is a crash from an earlier step and explains nothing.
const OOM_MAX_BOOTS_AGO: u32 = 5;

/// Console line prefix the firmware prints with the OOM byte counts. The
/// numbers never reach the wire (only the cause does), so this is the only
/// place they can be picked up.
const OOM_STATS_PREFIX: &str = "[RECOVERY] oom stats:";

/// Console line the boot report prints when the PREVIOUS run died of an OOM.
///
/// This, not the heartbeat, is the dependable oracle. The heartbeat carries
/// the ledger only while the crash is still the *last* thing that happened;
/// by the time the bench has ridden out the crash-loop and reconnected, the
/// recovery ladder has usually quarantined the offender and completed a
/// clean boot, at which point the heartbeat reports nothing at all. Observed
/// on the C6 at 1600 LEDs: five boots, four ledger OOMs on the console, and
/// a final heartbeat with no recovery report — a real OOM the bench then
/// refused to classify. The boot report prints on every boot and the console
/// feed survives the reconnect, so it sees all of them.
const OOM_BOOT_REPORT_PREFIX: &str = "crashed (oom)";

/// The other way an out-of-memory death is reported.
///
/// `fw-esp32s3` has no `recovery/panic_path.rs` (its siblings do), so an
/// allocation failure reaches the ledger as a generic `panic` carrying
/// Rust's default message rather than as `CrashCause::Oom` with heap stats.
/// Measured on silicon at 1600 LEDs: five crashes, every one of them
/// `crashed (panic): ... memory allocation of 38400 bytes failed`.
///
/// A failed allocation IS the device saying it ran out of memory, whatever
/// the ledger labelled it, so the bench accepts this as evidence — matched
/// on the message, never on the shape of a transport error, and recorded
/// verbatim in the record's notes so a reader can audit the call. The
/// firmware inconsistency is tracked separately in `docs/debt/`.
const OOM_ALLOC_FAILURE_MESSAGE: &str = "memory allocation of";

/// Console markers that say the workload is no longer doing its job.
///
/// After a step OOMs, the recovery ladder disables the offending node or
/// quarantines the shader compile. Every later step then "survives" while
/// rendering nothing — the frame counter still advances, the heap readings
/// stop moving with LED count, and a bisect converges on a number that
/// means nothing. Measured on the S3: 1200, 1400, 1500, 1550, 1575, 1587
/// and 1593 LEDs all reported an identical 143k free / 96k used, because
/// none of them were computing any LEDs at all.
///
/// A step that prints one of these is not a survival and not a death; it is
/// a step that has to be re-run on a device whose ladder has been cleared.
const DEGRADED_MARKERS: [&str; 2] = ["disabled after", "black fallback"];

/// What the bench needs to know before it touches hardware.
pub struct BenchPlan {
    /// Serial port, already chip-resolved.
    pub port: String,
    /// Cargo package the build under test embeds; the hello must agree.
    pub expected_package: String,
    /// Where the workload drives its LEDs.
    pub endpoint: HwEndpointSpec,
    /// Commit the packaged image under test was built from, when a package
    /// is on disk. The hello must agree: espflash dies mid-write on some
    /// boards (silently, on the classic), leaving an older image that boots
    /// and answers — and measuring that would label a record with a build
    /// that is not the one under test.
    pub expected_commit: Option<String>,
    /// LED count the ramp starts from.
    pub start_leds: u32,
    /// Known-good and known-bad counts from an earlier run, if any.
    pub floor: Option<u32>,
    pub ceiling: Option<u32>,
    /// Echo the device console to stderr.
    pub verbose: bool,
}

/// One step's observations, for the summary table.
pub struct BenchStepReport {
    pub leds: u32,
    pub outcome: StepOutcome,
    /// Lowest free heap seen while settling, in bytes. The full trend is
    /// printed live as the step runs; the summary table keeps the worst point.
    pub min_free_bytes: Option<u32>,
    /// Highest heap `used` seen while settling, in bytes. Across the ramp
    /// the (leds, used) series is a straight line whose slope is the
    /// per-LED cost and whose intercept is fixed + compile residency —
    /// the two numbers memory work actually needs.
    pub max_used_bytes: Option<u32>,
    /// Frames rendered by the end of the step.
    pub frames: u64,
}

/// What a whole run found out.
pub struct BenchOutcome {
    pub steps: Vec<BenchStepReport>,
    /// Largest LED count confirmed to survive.
    pub boundary_leds: u32,
    /// Build provenance of the firmware that was actually measured.
    pub fw_commit: String,
    pub fw_dirty: bool,
    /// `[RECOVERY] oom stats:` lines scraped from the console, newest last.
    pub oom_stats: Vec<String>,
    /// `[RECOVERY] last run crashed (oom)` boot reports, newest last.
    pub oom_reports: Vec<String>,
}

/// Run the whole ramp on a current-thread runtime.
///
/// [`DeviceSession`] is single-actor (`!Send` by design), so the bench owns a
/// current-thread runtime + `LocalSet` — the same shape `upload` and `fwcheck`
/// use.
pub fn run_bench(plan: BenchPlan) -> Result<BenchOutcome> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(run_bench_async(plan)))
}

async fn run_bench_async(plan: BenchPlan) -> Result<BenchOutcome> {
    let console = Rc::new(BenchConsole::new(plan.verbose));
    let session = connect_serial_device(
        Some(&plan.port),
        None,
        DeviceDeadlines {
            ready: BENCH_READY_DEADLINE,
            ..DeviceDeadlines::default()
        },
        {
            let console = Rc::clone(&console);
            move |event| console.observe(&event)
        },
    )
    .await
    .with_context(|| format!("connecting to {}", plan.port))?;

    let hello = session
        .hello()
        .context("the device became ready without a hello")?;
    check_image(
        &hello,
        &plan.expected_package,
        plan.expected_commit.as_deref(),
    )?;
    println!(
        "device: {} @ {}{} (proto {})",
        hello.build.package,
        hello.build.commit,
        if hello.build.dirty { "-dirty" } else { "" },
        hello.proto
    );

    let result = ramp(&session, &plan, &console).await;
    let _ = session.close().await;

    let (steps, boundary_leds) = result?;
    Ok(BenchOutcome {
        steps,
        boundary_leds,
        fw_commit: hello.build.commit.clone(),
        fw_dirty: hello.build.dirty,
        oom_stats: console.oom_stats(),
        oom_reports: console.oom_reports(),
    })
}

/// The image on the board must be the build being measured, or the record
/// would name a build that never ran. The bench does not flash: which image is
/// under test is the operator's decision.
fn check_image(
    hello: &ServerHello,
    expected_package: &str,
    expected_commit: Option<&str>,
) -> Result<()> {
    if hello.build.package != expected_package {
        bail!(
            "the board is running `{}`, but this bench measures `{expected_package}` — \
             flash the build under test first (`lp-cli firmware build`/`package`), \
             or pass --build for the build that is actually on the board",
            hello.build.package
        );
    }
    if let Some(expected) = expected_commit
        && hello.build.commit != expected
    {
        bail!(
            "the board is running commit `{}`, but the packaged image is `{expected}` — \
             reflash before benching. (espflash can die mid-write and leave an older \
             image that boots and answers; a record written now would name the wrong \
             build.)",
            hello.build.commit
        );
    }
    Ok(())
}

/// Walk the schedule until it has a boundary.
async fn ramp(
    session: &DeviceSession,
    plan: &BenchPlan,
    console: &BenchConsole,
) -> Result<(Vec<BenchStepReport>, u32)> {
    let mut schedule = BenchSchedule::seeded(plan.start_leds, plan.floor, plan.ceiling);
    let mut steps = Vec::new();
    let mut client = LpClient::new(session.client_io());

    loop {
        let leds = match schedule.next_step() {
            ScheduleStep::Test(leds) => leds,
            ScheduleStep::Done { boundary: 0 } => bail!(
                "no LED count survived, down to the bisect resolution — the board, the \
                 firmware, or the workload is broken, not merely small"
            ),
            ScheduleStep::Done { boundary } => return Ok((steps, boundary)),
            ScheduleStep::OutOfRange { survived } => bail!(
                "the ramp reached its ceiling with {survived} LEDs still alive — the \
                 workload is not exercising memory the way the metric assumes"
            ),
        };

        print!("{leds:>5} LEDs: ");
        flush_stdout();
        let workload = BenchWorkload::new(leds, plan.endpoint.clone());

        let oom_reports_before = console.oom_report_count();
        let degraded_before = console.degraded_count();
        let mut attempt = run_step(&mut client, &workload).await;

        // A step whose workload the ladder disabled measured nothing. Clear
        // the ladder and run it once more before believing either outcome.
        if console.degraded_count() > degraded_before {
            println!(
                "       ↳ workload was disabled by the recovery ladder — clearing and retrying"
            );
            clear_recovery_ladder(session).await?;
            reconnect(session, &plan.port).await?;
            client = LpClient::new(session.client_io());
            let degraded_retry = console.degraded_count();
            attempt = run_step(&mut client, &workload).await;
            if console.degraded_count() > degraded_retry {
                // Degraded again, on a device whose ladder we just cleared —
                // so this is not stale blame from an earlier step, it is
                // this LED count failing on its own. The device survives
                // because layer-1 recovery absorbs the crash and falls back
                // to black; the workload does not. A count whose shader
                // cannot compile without crashing has exceeded the board's
                // memory, which is exactly what this metric measures, so it
                // is a death — recorded only when the console also carries
                // the allocation-failure evidence, never inferred.
                if console.oom_report_count() > oom_reports_before {
                    println!(
                        "       ↳ out of memory during compile (workload disabled, device survived)"
                    );
                    schedule.record(leds, StepOutcome::Died);
                    steps.push(BenchStepReport {
                        leds,
                        outcome: StepOutcome::Died,
                        min_free_bytes: None,
                        max_used_bytes: None,
                        frames: 0,
                    });
                    clear_recovery_ladder(session).await?;
                    reconnect(session, &plan.port).await?;
                    client = LpClient::new(session.client_io());
                    continue;
                }
                bail!(
                    "at {leds} LEDs the workload stayed disabled after a cleared boot, but \
                     nothing on the console said an allocation failed, so the cause is \
                     unknown. Reporting instead of guessing at a boundary."
                );
            }
        }

        match attempt {
            Ok(settle) => {
                println!("survived  {}", settle.describe());
                schedule.record(leds, StepOutcome::Survived);
                steps.push(BenchStepReport {
                    leds,
                    outcome: StepOutcome::Survived,
                    min_free_bytes: settle.min_free_bytes(),
                    max_used_bytes: settle.max_used_bytes(),
                    frames: settle.frames,
                });
            }
            Err(candidate) => {
                println!("lost the device ({candidate:#})");
                let verdict =
                    confirm_death(session, &plan.port, console, oom_reports_before, &candidate)
                        .await?;
                match verdict {
                    DeathVerdict::Oom { boots_ago } => {
                        println!(
                            "       ↳ out of memory (recovery ledger, {boots_ago} boot(s) ago)"
                        );
                        schedule.record(leds, StepOutcome::Died);
                        steps.push(BenchStepReport {
                            leds,
                            outcome: StepOutcome::Died,
                            min_free_bytes: None,
                            max_used_bytes: None,
                            frames: 0,
                        });
                        // The crash left the ladder holding a grudge against
                        // this workload; clear it, or every later step
                        // "survives" while rendering nothing.
                        clear_recovery_ladder(session).await?;
                        reconnect(session, &plan.port).await?;
                        client = LpClient::new(session.client_io());
                    }
                    DeathVerdict::Unexplained { recovery } => bail!(
                        "the device stopped answering at {leds} LEDs and the recovery ledger \
                         does not name an OOM for it, so this is not a soft-limit boundary. \
                         Reporting instead of guessing.\n  step failure: {candidate:#}\n  \
                         recovery: {recovery}"
                    ),
                }
            }
        }
    }
}

/// One step: deploy, wait for a frame, settle. Any failure is a candidate
/// death for the caller to classify.
async fn run_step<Io: ClientIo>(
    client: &mut LpClient<Io>,
    workload: &BenchWorkload,
) -> Result<Settle> {
    let files: Vec<ProjectDeployFile> = workload
        .files()?
        .into_iter()
        .map(|file| ProjectDeployFile::new(file.name, file.contents.into_bytes()))
        .collect();

    let handle = client
        .deploy_project_files(BENCH_PROJECT_ID, files)
        .await
        .map_err(|error| anyhow!("{error}"))
        .context("deploying the bench workload")?
        .into_value();

    wait_for_workload_rendering(client, handle).await?;
    settle(client, handle).await
}

/// Wait until the workload is demonstrably rendering: the project's own frame
/// counter has moved.
///
/// This replaces `upload`'s `wait_for_project_running` for the bench.
/// That helper wants a clean run-evidence exchange and gives up on the first
/// unanswered poll window — but the classic's UART RX FIFO overflows under
/// console pressure and drops whole requests
/// (`[io_task] UART RX error: FifoOverflowed`) while the project renders on
/// at 30 fps. Every single step on that board burned the full 60 s timeout
/// before a follow-up probe rescued it, which is most of why a run took
/// forty minutes.
///
/// An advancing frame counter is the evidence the metric actually cares
/// about, it tolerates dropped polls for free, and it usually arrives within
/// a couple of seconds.
async fn wait_for_workload_rendering<Io: ClientIo>(
    client: &mut LpClient<Io>,
    handle: WireProjectHandle,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + RUN_TIMEOUT;
    let mut first: Option<u64> = None;
    while tokio::time::Instant::now() < deadline {
        if let Some(frames) = read_frame_count(client, handle).await {
            match first {
                None => first = Some(frames),
                Some(earlier) if frames > earlier => return Ok(()),
                Some(_) => {}
            }
        }
        tokio::time::sleep(RENDER_POLL).await;
    }
    bail!(
        "the workload never rendered a frame within {RUN_TIMEOUT:?} (deployed, but the \
         frame counter never moved)"
    )
}

/// The project's frame counter, or `None` if this poll did not come back.
async fn read_frame_count<Io: ClientIo>(
    client: &mut LpClient<Io>,
    handle: WireProjectHandle,
) -> Option<u64> {
    let events = client.project_read(handle, runtime_request()).await.ok()?;
    events
        .into_value()
        .into_iter()
        .find_map(|event| match event {
            ProjectReadEvent::Query {
                event: ProjectReadQueryEvent::Runtime(runtime),
                ..
            } => Some(runtime.project.frame_num),
            _ => None,
        })
}

/// Free-heap and frame observations over one settle window.
#[derive(Default)]
struct Settle {
    free_samples: Vec<u32>,
    /// Heap `used` while the workload renders. The ramp's (leds, used)
    /// series gives per-LED cost as its slope and fixed+compile residency
    /// as its intercept — more useful to memory work than the boundary
    /// alone. Sampled, like `free`, from `ServerRuntimeStatus.memory`.
    used_samples: Vec<u32>,
    frames: u64,
}

impl Settle {
    fn max_used_bytes(&self) -> Option<u32> {
        self.used_samples.iter().copied().max()
    }

    fn min_free_bytes(&self) -> Option<u32> {
        self.free_samples.iter().copied().min()
    }

    /// The free-heap trend, in KiB, plus the frame count.
    fn describe(&self) -> String {
        if self.free_samples.is_empty() {
            return format!("frames {} (device reports no heap stats)", self.frames);
        }
        let trend = self
            .free_samples
            .iter()
            .map(|bytes| format!("{}k", bytes / 1024))
            .collect::<Vec<_>>()
            .join(" ");
        format!("frames {}  free {trend}", self.frames)
    }
}

/// Watch a running workload for [`SETTLE`], sampling the server's free heap.
async fn settle<Io: ClientIo>(
    client: &mut LpClient<Io>,
    handle: WireProjectHandle,
) -> Result<Settle> {
    let deadline = tokio::time::Instant::now() + SETTLE;
    let mut observed = Settle::default();

    // A busy device streaming console + rendering can miss one response
    // deadline without being dead (observed live: a single 10 s timeout at
    // 400 LEDs on a board that was rendering at 28 fps). Only consecutive
    // failures count as a candidate death.
    const MAX_CONSECUTIVE_POLL_FAILURES: u32 = 3;
    let mut poll_failures = 0u32;

    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(SETTLE_POLL).await;
        let events = match client.project_read(handle, runtime_request()).await {
            Ok(response) => {
                poll_failures = 0;
                response.into_value()
            }
            Err(error) => {
                poll_failures += 1;
                if poll_failures >= MAX_CONSECUTIVE_POLL_FAILURES {
                    return Err(anyhow!("{error}")).context(format!(
                        "polling runtime status while settling                          ({poll_failures} consecutive failures)"
                    ));
                }
                eprintln!(
                    "       settle poll failure {poll_failures}/{MAX_CONSECUTIVE_POLL_FAILURES}                      (tolerated): {error}"
                );
                continue;
            }
        };

        for event in events {
            match event {
                ProjectReadEvent::Error { message } => {
                    bail!("the device reported an error while settling: {message}")
                }
                ProjectReadEvent::Query {
                    event: ProjectReadQueryEvent::Runtime(runtime),
                    ..
                } => {
                    observed.frames = runtime.project.frame_num;
                    if let Some(memory) = runtime.server.and_then(|server| server.memory) {
                        observed.free_samples.push(memory.free_bytes);
                        observed.used_samples.push(memory.used_bytes);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(observed)
}

/// The cheapest read that carries both the frame counter and the heap: the
/// full debug read costs the device real work at 400 LEDs.
fn runtime_request() -> ProjectReadRequest {
    ProjectReadRequest {
        since: None,
        queries: Vec::from([ProjectReadQuery::Runtime(RuntimeReadQuery)]),
        probes: Vec::new(),
    }
}

/// Why the device stopped answering.
enum DeathVerdict {
    /// The next boot's recovery ledger names an OOM recent enough to be this
    /// step's.
    Oom { boots_ago: u32 },
    /// It does not. Reported, never guessed at.
    Unexplained { recovery: String },
}

/// Reconnect after a candidate death and ask the device what happened.
///
/// The whole exchange retries: between our reconnect and our question the
/// device may crash again (it auto-loads the killer project until the
/// recovery ladder quarantines it — 2-3 boots on the C6), which surfaces as
/// a transport timeout mid-query. Each cycle waits out one more boot.
async fn confirm_death(
    session: &DeviceSession,
    port: &str,
    console: &BenchConsole,
    oom_reports_before: usize,
    candidate: &anyhow::Error,
) -> Result<DeathVerdict> {
    const CONFIRM_CYCLES: u32 = 5;
    let mut last: Option<anyhow::Error> = None;
    for cycle in 1..=CONFIRM_CYCLES {
        match confirm_death_once(session, port, console, oom_reports_before, candidate).await {
            Ok(verdict) => return Ok(verdict),
            Err(error) => {
                // A cycle that failed may still have carried the boot report
                // past us on the console; that is an answer, not a failure.
                if console.oom_report_count() > oom_reports_before {
                    return Ok(DeathVerdict::Oom { boots_ago: 1 });
                }
                eprintln!("       death-confirmation cycle {cycle}/{CONFIRM_CYCLES}: {error:#}");
                last = Some(error);
            }
        }
    }
    if console.oom_report_count() > oom_reports_before {
        return Ok(DeathVerdict::Oom { boots_ago: 1 });
    }
    Err(last.unwrap()).with_context(|| {
        format!("confirming a candidate death after {CONFIRM_CYCLES} cycles: {candidate:#}")
    })
}

async fn confirm_death_once(
    session: &DeviceSession,
    port: &str,
    console: &BenchConsole,
    oom_reports_before: usize,
    candidate: &anyhow::Error,
) -> Result<DeathVerdict> {
    reconnect(session, port)
        .await
        .with_context(|| format!("recovering from a candidate death: {candidate:#}"))?;

    // The device's own boot report, printed since this step began. Consulted
    // before the heartbeat because it is durable: see OOM_BOOT_REPORT_PREFIX.
    if console.oom_report_count() > oom_reports_before {
        return Ok(DeathVerdict::Oom { boots_ago: 1 });
    }

    let mut client = LpClient::new(session.client_io());
    let Some(recovery) = first_heartbeat_recovery(&mut client).await? else {
        return Ok(DeathVerdict::Unexplained {
            recovery: "no heartbeat carried a recovery report (a build without an RTC \
                       recovery region cannot be benched: OOM is the criterion)"
                .to_string(),
        });
    };

    match &recovery.last_crash {
        Some(crash)
            if (crash.cause == "oom"
                || (crash.cause == "panic"
                    && crash.message.contains(OOM_ALLOC_FAILURE_MESSAGE)))
                && crash.boots_ago <= OOM_MAX_BOOTS_AGO =>
        {
            Ok(DeathVerdict::Oom {
                boots_ago: crash.boots_ago,
            })
        }
        Some(crash) => Ok(DeathVerdict::Unexplained {
            recovery: format!(
                "last crash `{}` at `{}` {} boot(s) ago: {}",
                crash.cause, crash.path, crash.boots_ago, crash.message
            ),
        }),
        None => Ok(DeathVerdict::Unexplained {
            recovery: format!(
                "the ledger records no crash at all (level {:?}, reset reason `{}`, \
                 boot {}, safe mode {})",
                recovery.level, recovery.reset_reason, recovery.boot_count, recovery.safe_mode
            ),
        }),
    }
}

/// Force a fresh boot with a cleared recovery ledger.
///
/// A power-on-class reset invalidates the RTC recovery region outright
/// (contents are undefined after power loss, so a lucky CRC match must not
/// resurrect stale blame), which takes the quarantine and the boot counters
/// with it. That is the only way to un-disable a workload the ladder has
/// switched off — and until it is cleared, every later step "survives"
/// while rendering nothing.
async fn clear_recovery_ladder(session: &DeviceSession) -> Result<()> {
    session
        .manage(LinkManagementRequest::ResetRuntime, DeviceEventSink::noop())
        .await
        .map_err(|error| anyhow!("{error}"))
        .context("resetting the device to clear its recovery ladder")?;
    Ok(())
}

/// Rebuild the link, retrying: a board that just OOMed auto-loads the same
/// project on the next boot and can take a few boots to reach safe mode.
///
/// A native-USB chip that crashes hard also drops OFF the bus and
/// re-enumerates seconds later — observed on the first real XIAO-C6 bench:
/// the port path vanished (`No such file or directory`) and returned after
/// the naive retries were spent. So before each attempt, wait for the port
/// path to exist again (bounded), and never burn an attempt on a
/// still-absent device file.
async fn reconnect(session: &DeviceSession, port: &str) -> Result<()> {
    let mut last = String::new();
    for attempt in 1..=RECONNECT_ATTEMPTS {
        wait_for_port_path(port).await;
        match session.reconnect().await {
            Ok(state) if state.is_ready() => return Ok(()),
            Ok(state) => {
                last = state
                    .unavailable_message()
                    .unwrap_or_else(|| format!("{state:?}"));
            }
            Err(error) => last = error.to_string(),
        }
        eprintln!("       reconnect attempt {attempt}/{RECONNECT_ATTEMPTS}: {last}");
    }
    bail!("the device did not come back after {RECONNECT_ATTEMPTS} reconnects: {last}")
}

/// Poll (bounded) for a serial device file to re-appear after a USB
/// re-enumeration. Returns regardless once the bound is spent — the caller's
/// reconnect produces the real error message if the device stayed gone.
async fn wait_for_port_path(port: &str) {
    const REENUMERATION_BOUND: std::time::Duration = std::time::Duration::from_secs(30);
    const POLL: std::time::Duration = std::time::Duration::from_millis(500);
    let deadline = std::time::Instant::now() + REENUMERATION_BOUND;
    while !std::path::Path::new(port).exists() {
        if std::time::Instant::now() >= deadline {
            eprintln!("       {port} still absent after {REENUMERATION_BOUND:?}");
            return;
        }
        tokio::time::sleep(POLL).await;
    }
}

/// Poll cheap requests until a heartbeat arrives; heartbeats are unsolicited
/// frames, so they only surface on the events of a request the client is
/// already waiting on.
async fn first_heartbeat_recovery<Io: ClientIo>(
    client: &mut LpClient<Io>,
) -> Result<Option<RecoveryStatus>> {
    let deadline = tokio::time::Instant::now() + HEARTBEAT_WAIT;
    while tokio::time::Instant::now() < deadline {
        let outcome = client
            .project_list_loaded()
            .await
            .map_err(|error| anyhow!("{error}"))
            .context("asking the device what it is running after a death")?;
        for event in outcome.events {
            if let ClientEvent::Heartbeat { recovery, .. } = event {
                if let Some(recovery) = recovery {
                    return Ok(Some(recovery));
                }
            }
        }
        tokio::time::sleep(HEARTBEAT_POLL).await;
    }
    Ok(None)
}

/// The device console feed, watched for the one thing the wire drops: the OOM
/// byte counts the firmware prints on the boot after a crash.
///
/// Collection is unconditional rather than armed around a death: on a board
/// whose serial port survives the reboot (an external UART, unlike the C6's
/// USB-CDC) the line arrives before the bench has noticed anything is wrong.
struct BenchConsole {
    verbose: bool,
    oom_stats: RefCell<Vec<String>>,
    /// `[RECOVERY] last run crashed (oom)` lines, newest last.
    oom_reports: RefCell<Vec<String>>,
    /// Count of lines saying the workload has been disabled — see
    /// [`DEGRADED_MARKERS`].
    degraded: RefCell<usize>,
}

impl BenchConsole {
    fn new(verbose: bool) -> Self {
        Self {
            verbose,
            oom_stats: RefCell::new(Vec::new()),
            oom_reports: RefCell::new(Vec::new()),
            degraded: RefCell::new(0),
        }
    }

    /// How many "the workload is disabled" lines have been seen so far. A
    /// step that pushes this up was not measuring what it claims to measure.
    fn degraded_count(&self) -> usize {
        *self.degraded.borrow()
    }

    fn oom_stats(&self) -> Vec<String> {
        self.oom_stats.borrow().clone()
    }

    /// How many OOM boot reports have been seen so far. A step that dies and
    /// pushes this count up died of an OOM, whatever the heartbeat says.
    fn oom_report_count(&self) -> usize {
        self.oom_reports.borrow().len()
    }

    fn oom_reports(&self) -> Vec<String> {
        self.oom_reports.borrow().clone()
    }

    fn observe(&self, event: &DeviceEvent) {
        match event {
            DeviceEvent::LogLine { line, origin } => {
                if let Some((_, stats)) = line.split_once(OOM_STATS_PREFIX) {
                    self.oom_stats.borrow_mut().push(stats.trim().to_string());
                }
                if is_oom_crash_report(line) {
                    self.oom_reports.borrow_mut().push(line.trim().to_string());
                }
                if DEGRADED_MARKERS.iter().any(|marker| line.contains(marker)) {
                    *self.degraded.borrow_mut() += 1;
                }
                if self.verbose || *origin == DeviceLineOrigin::Link {
                    eprintln!("[device] {line}");
                }
            }
            DeviceEvent::State { state } => {
                if self.verbose {
                    eprintln!("[device] state: {state:?}");
                }
            }
            DeviceEvent::Progress { .. } => {}
        }
    }
}

/// Does this console line report a crash the bench should read as an OOM?
///
/// Two accepted forms: the ledger's own `oom` classification, and an
/// allocation-failure panic from a build that does not classify (see
/// [`OOM_ALLOC_FAILURE_MESSAGE`]).
fn is_oom_crash_report(line: &str) -> bool {
    if !line.contains("run crashed (") {
        return false;
    }
    line.contains(OOM_BOOT_REPORT_PREFIX)
        || (line.contains("crashed (panic)") && line.contains(OOM_ALLOC_FAILURE_MESSAGE))
}

fn flush_stdout() {
    use std::io::Write;
    std::io::stdout().flush().ok();
}

#[cfg(test)]
mod tests {
    use lpc_wire::server::{BuildFacts, HardwareFacts};

    use super::*;

    fn hello(package: &str) -> ServerHello {
        ServerHello {
            proto: lpc_wire::WIRE_PROTO_VERSION,
            build: BuildFacts {
                features: Vec::new(),
                package: package.to_string(),
                commit: "5466346".to_string(),
                dirty: false,
                profile: "release-esp32".to_string(),
            },
            hardware: HardwareFacts {
                radio: false,
                button: false,
                board_id: None,
            },
            device_uid: None,
        }
    }

    /// Benching whatever happens to be flashed would file a record naming a
    /// build that never ran.
    #[test]
    fn a_mismatched_image_is_refused_with_the_flash_instruction() {
        assert!(check_image(&hello("fw-esp32c6"), "fw-esp32c6", None).is_ok());

        let error = check_image(&hello("fw-esp32v3"), "fw-esp32c6", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("fw-esp32v3"), "{error}");
        assert!(error.contains("flash"), "{error}");
    }

    /// Every step deploys the same file names under the same project id, so a
    /// step replaces its predecessor instead of leaving a directory per LED
    /// count on the device's flash.
    #[test]
    fn steps_overwrite_each_other_rather_than_accumulating() {
        let endpoint = HwEndpointSpec::parse("ws281x:rmt:D0").expect("endpoint");
        let names = |leds: u32| {
            BenchWorkload::new(leds, endpoint.clone())
                .files()
                .expect("workload")
                .into_iter()
                .map(|file| file.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(120), names(3_000));
    }

    /// Both firmware wordings for an out-of-memory death are evidence; a
    /// plain panic is not. Lines are verbatim from silicon (C6 and S3).
    #[test]
    fn oom_crash_reports_are_recognised_in_both_firmware_wordings() {
        assert!(is_oom_crash_report(
            "[RECOVERY] last run crashed (oom): at node:/x/shader-compile:glsl: \
             alloc 8280 bytes failed (align 1) in shader node: compile"
        ));
        assert!(is_oom_crash_report(
            "[RECOVERY] previous run crashed (panic): at node:/soft_limit_be: \
             memory allocation of 38400 bytes failed (at alloc.rs:553)"
        ));
        // A panic that is not an allocation failure says nothing about memory.
        assert!(!is_oom_crash_report(
            "[RECOVERY] last run crashed (panic): at node:/x: lock is not reentrant"
        ));
        // Ordinary console noise is not a crash report.
        assert!(!is_oom_crash_report("[MEM] free=138648 used=39528"));
    }

    /// The trend line is what an operator reads to see a step approaching the
    /// wall, so it must survive a device that reports no heap stats.
    #[test]
    fn the_settle_summary_reports_the_heap_trend() {
        let observed = Settle {
            free_samples: Vec::from([120 * 1024, 96 * 1024]),
            used_samples: Vec::from([40 * 1024, 64 * 1024]),
            frames: 240,
        };
        assert_eq!(observed.min_free_bytes(), Some(96 * 1024));
        assert_eq!(observed.max_used_bytes(), Some(64 * 1024));
        assert_eq!(observed.describe(), "frames 240  free 120k 96k");

        let quiet = Settle {
            free_samples: Vec::new(),
            used_samples: Vec::new(),
            frames: 12,
        };
        assert_eq!(quiet.min_free_bytes(), None);
        assert!(quiet.describe().contains("no heap stats"));
    }
}

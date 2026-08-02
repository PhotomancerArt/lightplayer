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
use lpa_link::{DeviceDeadlines, DeviceEvent, DeviceLineOrigin, DeviceSession};
use lpc_model::HwEndpointSpec;
use lpc_wire::server::RecoveryStatus;
use lpc_wire::{
    ProjectReadEvent, ProjectReadQuery, ProjectReadQueryEvent, ProjectReadRequest,
    RuntimeReadQuery, ServerHello, WireProjectHandle,
};

use crate::client::cli_connect::connect_serial_device;
use crate::commands::upload::wait::wait_for_project_running;

use super::schedule::{BenchSchedule, ScheduleStep, StepOutcome};
use super::workload::BenchWorkload;

/// Project id every step deploys under. Fixed on purpose: each step's files
/// overwrite the previous step's, so the device never accumulates one project
/// directory per LED count (the file *names* are identical between steps, so
/// nothing is left behind either).
pub const BENCH_PROJECT_ID: &str = "soft-limit-bench";

/// Decoy project deployed once before the ramp. It sorts lexically BEFORE
/// [`BENCH_PROJECT_ID`], so a crash-reboot's lexical-first auto-load picks
/// this 3-LED idle project instead of re-loading the killer workload — the
/// boot loop observed on the first real C6 bench (auto-load killer -> OOM ->
/// cascading panic, ladder never quarantined) cannot recur.
pub const BENCH_IDLE_PROJECT_ID: &str = "aaa-bench-idle";

/// LED count of the decoy. Small enough to be loadable in any post-OOM state.
const IDLE_LEDS: u32 = 3;

/// How long a deployed workload gets to render its first frame. Generous: on
/// a cold cache the device compiles the shader first.
const RUN_TIMEOUT: Duration = Duration::from_secs(60);

/// How long a rendering workload is watched before it counts as survived.
/// Part of the metric definition — an OOM often arrives a few frames in, not
/// at load.
const SETTLE: Duration = Duration::from_secs(20);

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

/// What the bench needs to know before it touches hardware.
pub struct BenchPlan {
    /// Serial port, already chip-resolved.
    pub port: String,
    /// Cargo package the build under test embeds; the hello must agree.
    pub expected_package: String,
    /// Where the workload drives its LEDs.
    pub endpoint: HwEndpointSpec,
    /// LED count the ramp starts from.
    pub start_leds: u32,
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
    check_image(&hello, &plan.expected_package)?;
    println!(
        "device: {} @ {}{} (proto {})",
        hello.build.package,
        hello.build.commit,
        if hello.build.dirty { "-dirty" } else { "" },
        hello.proto
    );

    {
        let mut client = LpClient::new(session.client_io());
        deploy_idle_decoy(&mut client, &plan.endpoint).await?;
    }

    let result = ramp(&session, &plan).await;
    let _ = session.close().await;

    let (steps, boundary_leds) = result?;
    Ok(BenchOutcome {
        steps,
        boundary_leds,
        fw_commit: hello.build.commit.clone(),
        fw_dirty: hello.build.dirty,
        oom_stats: console.oom_stats(),
    })
}

/// The image on the board must be the build being measured, or the record
/// would name a build that never ran. The bench does not flash: which image is
/// under test is the operator's decision.
fn check_image(hello: &ServerHello, expected_package: &str) -> Result<()> {
    if hello.build.package != expected_package {
        bail!(
            "the board is running `{}`, but this bench measures `{expected_package}` — \
             flash the build under test first (`lp-cli firmware build`/`package`), \
             or pass --build for the build that is actually on the board",
            hello.build.package
        );
    }
    Ok(())
}

/// Walk the schedule until it has a boundary.
async fn ramp(session: &DeviceSession, plan: &BenchPlan) -> Result<(Vec<BenchStepReport>, u32)> {
    let mut schedule = BenchSchedule::new(plan.start_leds);
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

        match run_step(&mut client, &workload).await {
            Ok(settle) => {
                println!("survived  {}", settle.describe());
                schedule.record(leds, StepOutcome::Survived);
                steps.push(BenchStepReport {
                    leds,
                    outcome: StepOutcome::Survived,
                    min_free_bytes: settle.min_free_bytes(),
                    frames: settle.frames,
                });
            }
            Err(candidate) => {
                println!("lost the device ({candidate:#})");
                let verdict = confirm_death(session, &plan.port, &candidate).await?;
                client = LpClient::new(session.client_io());
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
                            frames: 0,
                        });
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

/// Deploy the decoy (see [`BENCH_IDLE_PROJECT_ID`]) and prove it runs — it
/// is what every crash-reboot will auto-load for the rest of the run.
async fn deploy_idle_decoy<Io: ClientIo>(
    client: &mut LpClient<Io>,
    endpoint: &HwEndpointSpec,
) -> Result<()> {
    let workload = BenchWorkload {
        led_count: IDLE_LEDS,
        endpoint: endpoint.clone(),
    };
    let files: Vec<ProjectDeployFile> = workload
        .files()?
        .into_iter()
        .map(|file| ProjectDeployFile::new(file.name, file.contents.into_bytes()))
        .collect();
    let handle = client
        .deploy_project_files(BENCH_IDLE_PROJECT_ID, files)
        .await
        .map_err(|error| anyhow!("{error}"))
        .context("deploying the idle decoy")?
        .into_value();
    wait_for_project_running(client, handle, RUN_TIMEOUT).await?;
    Ok(())
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

    wait_for_project_running(client, handle, RUN_TIMEOUT).await?;
    settle(client, handle).await
}

/// Free-heap and frame observations over one settle window.
#[derive(Default)]
struct Settle {
    free_samples: Vec<u32>,
    frames: u64,
}

impl Settle {
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
    candidate: &anyhow::Error,
) -> Result<DeathVerdict> {
    const CONFIRM_CYCLES: u32 = 5;
    let mut last: Option<anyhow::Error> = None;
    for cycle in 1..=CONFIRM_CYCLES {
        match confirm_death_once(session, port, candidate).await {
            Ok(verdict) => return Ok(verdict),
            Err(error) => {
                eprintln!("       death-confirmation cycle {cycle}/{CONFIRM_CYCLES}: {error:#}");
                last = Some(error);
            }
        }
    }
    Err(last.unwrap()).with_context(|| {
        format!("confirming a candidate death after {CONFIRM_CYCLES} cycles: {candidate:#}")
    })
}

async fn confirm_death_once(
    session: &DeviceSession,
    port: &str,
    candidate: &anyhow::Error,
) -> Result<DeathVerdict> {
    reconnect(session, port)
        .await
        .with_context(|| format!("recovering from a candidate death: {candidate:#}"))?;

    let mut client = LpClient::new(session.client_io());
    let Some(recovery) = first_heartbeat_recovery(&mut client).await? else {
        return Ok(DeathVerdict::Unexplained {
            recovery: "no heartbeat carried a recovery report (a build without an RTC \
                       recovery region cannot be benched: OOM is the criterion)"
                .to_string(),
        });
    };

    match &recovery.last_crash {
        Some(crash) if crash.cause == "oom" && crash.boots_ago <= OOM_MAX_BOOTS_AGO => {
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
}

impl BenchConsole {
    fn new(verbose: bool) -> Self {
        Self {
            verbose,
            oom_stats: RefCell::new(Vec::new()),
        }
    }

    fn oom_stats(&self) -> Vec<String> {
        self.oom_stats.borrow().clone()
    }

    fn observe(&self, event: &DeviceEvent) {
        match event {
            DeviceEvent::LogLine { line, origin } => {
                if let Some((_, stats)) = line.split_once(OOM_STATS_PREFIX) {
                    self.oom_stats.borrow_mut().push(stats.trim().to_string());
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
        assert!(check_image(&hello("fw-esp32c6"), "fw-esp32c6").is_ok());

        let error = check_image(&hello("fw-esp32v3"), "fw-esp32c6")
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

    /// The trend line is what an operator reads to see a step approaching the
    /// wall, so it must survive a device that reports no heap stats.
    #[test]
    fn the_settle_summary_reports_the_heap_trend() {
        let observed = Settle {
            free_samples: Vec::from([120 * 1024, 96 * 1024]),
            frames: 240,
        };
        assert_eq!(observed.min_free_bytes(), Some(96 * 1024));
        assert_eq!(observed.describe(), "frames 240  free 120k 96k");

        let quiet = Settle {
            free_samples: Vec::new(),
            frames: 12,
        };
        assert_eq!(quiet.min_free_bytes(), None);
        assert!(quiet.describe().contains("no heap stats"));
    }
}

//! Native lp-server render-loop harness (the skeleton of the future
//! desktop/RPi lp-server loop): boots `LpServer` on an **explicitly
//! selected** graphics engine, loads a real example project with a fixture,
//! ticks at server rates, and reports per-tick sample latency.
//!
//! Engine selection is explicit per the fidelity-tiers ADR
//! (`docs/adr/2026-07-09-preview-fidelity-tiers.md`): `--engine gpu-f32`
//! requires a native wgpu adapter and **fails** without one — it never
//! falls back to the CPU tier silently; `--engine cpu-q32` is the
//! embedded-parity tier (Q32 on the target LPVM engine).
//!
//! ```text
//! cargo run -p lp-gfx-harness --release -- --engine gpu-f32
//! cargo run -p lp-gfx-harness --release -- --engine cpu-q32 --ticks 600
//! ```

mod timing_graphics;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_gfx_wgpu::GpuGraphics;
use lp_shader::ShaderFrontend;
use lpa_server::{LpGraphics, LpServer, handlers::handle_client_message};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::messages::{ClientMessage, ClientRequest};
use lpfs::LpFsStd;

use crate::timing_graphics::{ShaderTimings, TimingGraphics};

/// Explicitly selected engine (never defaulted, never substituted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Engine {
    /// f32 on a native wgpu device — the non-embedded lp-server default
    /// tier.
    GpuF32,
    /// Q32 on the target CPU LPVM engine — bit-parity with embedded.
    CpuQ32,
}

impl Engine {
    fn label(self) -> &'static str {
        match self {
            Self::GpuF32 => "gpu-f32",
            Self::CpuQ32 => "cpu-q32",
        }
    }
}

struct Args {
    engine: Engine,
    project_dir: PathBuf,
    ticks: u32,
    fps: u32,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            eprintln!(
                "usage: lp-gfx-harness --engine gpu-f32|cpu-q32 \
                 [--project <dir>] [--ticks <n>] [--fps <n>]"
            );
            std::process::exit(2);
        }
    };
    if let Err(message) = run(&args) {
        eprintln!("error: {message}");
        std::process::exit(1);
    }
}

fn parse_args() -> Result<Args, String> {
    let mut engine = None;
    let mut project_dir = default_project_dir();
    let mut ticks = 300u32;
    let mut fps = 60u32;

    let mut argv = std::env::args().skip(1);
    while let Some(flag) = argv.next() {
        let mut value = |flag: &str| {
            argv.next()
                .ok_or_else(|| format!("missing value for {flag}"))
        };
        match flag.as_str() {
            "--engine" => {
                engine = Some(match value("--engine")?.as_str() {
                    "gpu-f32" => Engine::GpuF32,
                    "cpu-q32" => Engine::CpuQ32,
                    other => {
                        return Err(format!(
                            "unknown engine `{other}` (expected gpu-f32 or cpu-q32)"
                        ));
                    }
                });
            }
            "--project" => project_dir = PathBuf::from(value("--project")?),
            "--ticks" => {
                ticks = value("--ticks")?
                    .parse()
                    .map_err(|e| format!("--ticks: {e}"))?;
            }
            "--fps" => {
                fps = value("--fps")?.parse().map_err(|e| format!("--fps: {e}"))?;
                if fps == 0 {
                    return Err(String::from("--fps must be at least 1"));
                }
            }
            other => return Err(format!("unknown flag `{other}`")),
        }
    }

    // Engine selection is explicit, never defaulted (fidelity-tiers ADR).
    let engine = engine.ok_or_else(|| {
        String::from("--engine is required (gpu-f32 | cpu-q32); there is no default engine")
    })?;
    Ok(Args {
        engine,
        project_dir,
        ticks,
        fps,
    })
}

/// `examples/basic` relative to the workspace this binary was built from.
/// Recursively copy a project directory into the scratch root.
fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn default_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/basic")
}

fn run(args: &Args) -> Result<(), String> {
    let source_dir = args
        .project_dir
        .canonicalize()
        .map_err(|e| format!("project dir {}: {e}", args.project_dir.display()))?;
    let project_name = source_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("project dir {} has no name", source_dir.display()))?
        .to_owned();
    // The server persists runtime state (e.g. `lightplayer.json` with the
    // startup project) into its projects root — copy the project into a
    // scratch root so harness runs never dirty the source tree.
    let scratch = std::env::temp_dir().join(format!("lp-gfx-harness-{}", std::process::id()));
    let projects_root = scratch.clone();
    let project_dir = scratch.join(&project_name);
    copy_dir(&source_dir, &project_dir).map_err(|e| format!("stage project: {e}"))?;

    let timings = Arc::new(ShaderTimings::default());
    let graphics: Arc<dyn LpGraphics> = Arc::new(TimingGraphics::new(
        create_engine(args.engine)?,
        timings.clone(),
    ));
    println!(
        "engine: {} (backend: {})",
        args.engine.label(),
        graphics.backend_name()
    );
    println!("project: {}", project_dir.display());

    // Keep the concrete handle for the end-of-run LED sanity read; the
    // server takes the same allocation as `dyn OutputProvider`.
    let memory_output = Rc::new(RefCell::new(MemoryOutputProvider::new_permissive()));
    let output_provider: Rc<RefCell<dyn lpc_shared::output::OutputProvider>> =
        memory_output.clone();
    let mut server = LpServer::new(
        output_provider.clone(),
        Box::new(LpFsStd::new(projects_root)),
        "/".as_path(),
        None,
        None,
        graphics.clone(),
    );

    // Load the project through the wire handler (the same path a client
    // takes). `handle_client_message` needs the manager and filesystem
    // split-borrowed out of the server — the established pattern from the
    // lpa-server tests.
    let request = ClientMessage {
        id: 1,
        msg: ClientRequest::LoadProject {
            path: project_name.clone(),
        },
    };
    let response = {
        let server_ptr: *mut LpServer = &mut server;
        // SAFETY: project_manager and base_fs are disjoint fields of
        // `server`; nothing else touches `server` for the duration.
        unsafe {
            let hello = (*server_ptr).hello().clone();
            let manager = (*server_ptr).project_manager_mut();
            let fs = (*server_ptr).base_fs_mut();
            handle_client_message(
                manager,
                fs,
                &output_provider,
                None,
                None,
                None,
                None,
                graphics.clone(),
                &hello,
                request,
            )
        }
    }
    .map_err(|e| format!("load project `{project_name}`: {e:?}"))?;
    if !matches!(
        response.msg,
        lpc_wire::server::ServerMsgBody::LoadProject { .. }
    ) {
        return Err(format!(
            "load project `{project_name}`: unexpected response {:?}",
            response.msg
        ));
    }

    let delta_ms = (1000 / args.fps).max(1);
    let tick_budget = Duration::from_millis(u64::from(delta_ms));
    println!(
        "ticking {} frames at {} fps (delta {delta_ms} ms)...",
        args.ticks, args.fps
    );

    let mut tick_durations = Vec::with_capacity(args.ticks as usize);
    let mut first_tick = Duration::ZERO;
    let run_start = Instant::now();
    for tick in 0..args.ticks {
        let tick_start = Instant::now();
        server
            .advance_frame(delta_ms)
            .map_err(|e| format!("tick {tick}: {e:?}"))?;
        let elapsed = tick_start.elapsed();
        if tick == 0 {
            // The first tick compiles the shader — report it separately.
            first_tick = elapsed;
        } else {
            tick_durations.push(elapsed);
        }
        // Server-rate pacing: sleep out the remainder of the tick budget.
        if elapsed < tick_budget {
            std::thread::sleep(tick_budget - elapsed);
        }
    }
    let wall = run_start.elapsed();

    report(&timings, &tick_durations, first_tick, wall, args);
    report_led_sanity(&memory_output.borrow());
    Ok(())
}

/// Construct the selected engine, honor-or-fail (no silent substitution).
fn create_engine(engine: Engine) -> Result<Box<dyn LpGraphics>, String> {
    match engine {
        Engine::CpuQ32 => Ok(Box::new(TargetLpvmGraphics::new(ShaderFrontend::Naga))),
        Engine::GpuF32 => {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                }))
                .map_err(|e| {
                    format!(
                        "--engine gpu-f32 requires a native wgpu adapter and none is available \
                     ({e}); NOT falling back to the CPU tier — run with --engine cpu-q32 to \
                     select it explicitly"
                    )
                })?;
            let info = adapter.get_info();
            println!("wgpu adapter: {} ({:?})", info.name, info.backend);
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("lp-gfx-harness"),
                    ..Default::default()
                }))
                .map_err(|e| format!("wgpu device request failed: {e}"))?;
            // Compute shaders stay on the CPU tier permanently; the GPU
            // backend delegates them to the target LPVM engine.
            Ok(Box::new(GpuGraphics::new(
                device,
                queue,
                Box::new(TargetLpvmGraphics::new(ShaderFrontend::Naga)),
            )))
        }
    }
}

fn report(
    timings: &ShaderTimings,
    tick_durations: &[Duration],
    first_tick: Duration,
    wall: Duration,
    args: &Args,
) {
    println!();
    println!(
        "ran {} ticks in {:.2} s (first tick {:.1} ms — includes shader compile)",
        args.ticks,
        wall.as_secs_f64(),
        first_tick.as_secs_f64() * 1000.0
    );

    let samples = timings.samples.lock().expect("timings lock");
    if let Some((count, _)) = samples.first() {
        println!("sample points per call: {count}");
    }
    let sample_durations: Vec<Duration> = samples.iter().map(|(_, d)| *d).collect();
    drop(samples);
    print_stats("sample_rgba16 (per tick)", &sample_durations);

    let renders = timings.renders.lock().expect("timings lock");
    print_stats("shader render", &renders);
    drop(renders);

    print_stats("advance_frame (excl. first)", tick_durations);
}

fn print_stats(label: &str, durations: &[Duration]) {
    if durations.is_empty() {
        println!("{label}: no calls");
        return;
    }
    let mut micros: Vec<u128> = durations.iter().map(Duration::as_micros).collect();
    micros.sort_unstable();
    let sum: u128 = micros.iter().sum();
    let mean = sum / micros.len() as u128;
    let p = |q: f64| micros[((micros.len() - 1) as f64 * q) as usize];
    println!(
        "{label}: n={} mean {mean} us, min {} us, p50 {} us, p95 {} us, max {} us",
        micros.len(),
        micros[0],
        p(0.50),
        p(0.95),
        micros[micros.len() - 1],
    );
}

/// Print the first lamps of each open output channel — proof the LED path
/// produced data end-to-end.
fn report_led_sanity(memory: &MemoryOutputProvider) {
    for handle in memory.get_all_handles() {
        if let Some(data) = memory.get_data(handle) {
            let lamps: Vec<u16> = data.iter().copied().take(9).collect();
            println!(
                "LED sanity: channel {handle:?}, {} samples, first lamps (u16 rgb): {lamps:?}",
                data.len()
            );
        }
    }
}

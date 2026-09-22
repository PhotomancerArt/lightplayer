//! Probe-load A/B against a served board (emulated or real).
//!
//! What it measures: the board's OWN frame counter (the runtime read's
//! `frame_num` over `frame_total_ms`, both guest time) while a Studio-shaped
//! refresh read is sent at a fixed cadence — with different probe sets. The
//! arms are the ones the 2026-09-22 device-lens change tells apart:
//!
//!   `none`     the runtime query only — the cadence's own cost
//!   `control`  + the fixture's control-product probe (what a device lens
//!              pulls after the change: the rendered lamps)
//!   `both`     + the shader's 16×16 render-product probe (what it pulled
//!              before: the always-live primary visual)
//!
//! Usage:
//!   cargo run -p lp-cli --example probe_load -- <project-dir> <host-spec> \
//!       [--arms none,control,both] [--reads 60] [--warmup 10] [--cadence-ms 150]
//!
//! e.g. `serial:tcp://127.0.0.1:5591` against `lp-cli emu run --link 127.0.0.1:5591`.
//!
//! The cadence is GUEST time: a served emulator is not paced to real time, so
//! reads are scheduled against the board's own clock (the runtime read's
//! `frame_total_ms`), using a running guest-ms-per-wall-ms estimate to turn
//! the next guest deadline into a wall sleep. That keeps the load per guest
//! second the same as Studio's 150 ms pull puts on silicon, whatever speed
//! the emulator runs at. Like Studio, the next read is due one cadence AFTER
//! the previous one completed, so `reads / guest s` falls as reads get
//! heavier — that is the load, not a harness artefact. Every reported number
//! is in guest time.

use anyhow::{Context, Result};
use lp_cli::client::cli_connect::{cli_connect, stderr_device_events};
use lp_cli::commands::dev::push_project::collect_project_deploy_files;
use lp_cli::commands::upload::wait::wait_for_project_running;
use lpa_client::{HostSpecifier, LpClient};
use lpc_wire::{
    BindingGraphProbeRequest, BindingGraphProbeResult, ControlDisplayLayoutRead,
    ControlProductProbeRequest, ProjectProbeRequest, ProjectProbeResult, ProjectReadEvent,
    ProjectReadProbeEvent, ProjectReadQuery, ProjectReadQueryEvent, ProjectReadRequest,
    RenderProductProbeRequest, RuntimeReadQuery, WireChannelSampleFormat, WireProjectHandle,
    WireTextureFormat, WireVisualSpace,
};
use lpfs::LpFsStd;
use std::time::{Duration, Instant};

struct Args {
    dir: std::path::PathBuf,
    host: String,
    arms: Vec<String>,
    reads: usize,
    warmup: usize,
    cadence_ms: u64,
}

fn parse_args() -> Result<Args> {
    let mut it = std::env::args().skip(1);
    let dir = it
        .next()
        .context("usage: probe_load <project-dir> <host-spec> [flags]")?;
    let host = it
        .next()
        .context("usage: probe_load <project-dir> <host-spec> [flags]")?;
    let mut args = Args {
        dir: dir.into(),
        host,
        arms: vec!["none".into(), "control".into(), "both".into()],
        reads: 60,
        warmup: 10,
        cadence_ms: 150,
    };
    while let Some(flag) = it.next() {
        let value = it.next().with_context(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--arms" => args.arms = value.split(',').map(str::to_string).collect(),
            "--reads" => args.reads = value.parse()?,
            "--warmup" => args.warmup = value.parse()?,
            "--cadence-ms" => args.cadence_ms = value.parse()?,
            other => anyhow::bail!("unknown flag {other}"),
        }
    }
    Ok(args)
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    frame_num: u64,
    frame_total_ms: u32,
    theoretical_fps: Option<f32>,
    last_frame_time_us: Option<u64>,
    wall: Instant,
}

fn runtime_sample(events: &[ProjectReadEvent], wall: Instant) -> Option<Sample> {
    events.iter().find_map(|event| match event {
        ProjectReadEvent::Query {
            event: ProjectReadQueryEvent::Runtime(runtime),
            ..
        } => Some(Sample {
            frame_num: runtime.project.frame_num,
            frame_total_ms: runtime.project.frame_total_ms,
            theoretical_fps: runtime.server.as_ref().and_then(|s| s.theoretical_fps),
            last_frame_time_us: runtime.server.as_ref().and_then(|s| s.last_frame_time_us),
            wall,
        }),
        _ => None,
    })
}

fn probes_for(
    arm: &str,
    visual: lpc_model::VisualProduct,
    control: lpc_model::ControlProduct,
) -> Result<Vec<ProjectProbeRequest>> {
    let control_probe = ProjectProbeRequest::ControlProduct(ControlProductProbeRequest {
        product: control,
        sample_format: WireChannelSampleFormat::U16,
        display_layout: ControlDisplayLayoutRead::None,
    });
    // Exactly Studio's device-tier request (`UiProductPreviewFrame::VISUAL_DEVICE`
    // through `visual_probe_request`): 16×16, sRGB8, 2D, the AUTO policy.
    let visual_probe = ProjectProbeRequest::RenderProduct(RenderProductProbeRequest {
        product: visual,
        width: 16,
        height: 16,
        format: WireTextureFormat::Srgb8,
        space: Some(WireVisualSpace::TwoD),
        policy: None,
    });
    Ok(match arm {
        "none" => Vec::new(),
        "control" => vec![control_probe],
        "both" => vec![control_probe, visual_probe],
        "visual" => vec![visual_probe],
        other => anyhow::bail!("unknown arm {other}"),
    })
}

async fn read(
    client: &mut LpClient<Box<dyn lpa_client::ClientIo>>,
    handle: WireProjectHandle,
    probes: Vec<ProjectProbeRequest>,
) -> Result<Vec<ProjectReadEvent>> {
    let request = ProjectReadRequest {
        since: None,
        queries: vec![ProjectReadQuery::Runtime(RuntimeReadQuery)],
        probes,
    };
    Ok(client
        .project_read(handle, request)
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))?
        .into_value())
}

async fn primary_products(
    client: &mut LpClient<Box<dyn lpa_client::ClientIo>>,
    handle: WireProjectHandle,
) -> Result<(lpc_model::VisualProduct, lpc_model::ControlProduct)> {
    let events = read(
        client,
        handle,
        vec![ProjectProbeRequest::BindingGraph(
            BindingGraphProbeRequest {
                include_values: true,
            },
        )],
    )
    .await?;
    let graph = events
        .iter()
        .find_map(|event| match event {
            ProjectReadEvent::Probe {
                event:
                    ProjectReadProbeEvent::Result(ProjectProbeResult::BindingGraph(
                        BindingGraphProbeResult::Graph(graph),
                    )),
                ..
            } => Some(graph),
            _ => None,
        })
        .context("no binding graph in the probe read")?;
    let mut visual = None;
    let mut control = None;
    for channel in &graph.channels {
        let Some(lpc_model::LpValue::Product(product)) = channel
            .value
            .as_ref()
            .and_then(|value| value.value.as_ref())
        else {
            continue;
        };
        match product {
            lpc_model::ProductRef::Visual(v) if channel.primary_visual => visual = Some(*v),
            lpc_model::ProductRef::Control(c)
                if channel.name == lpc_model::PRIMARY_CONTROL_CHANNEL =>
            {
                control = Some(*c)
            }
            _ => {}
        }
    }
    Ok((
        visual.context("no primary visual product resolved")?,
        control.context("no primary control product resolved")?,
    ))
}

struct ArmResult {
    arm: String,
    fps: f64,
    guest_per_wall: f64,
    reads_per_guest_s: f64,
    theoretical_fps: Option<f64>,
    frame_us: Option<f64>,
    read_wall_ms: f64,
    probe_bytes: usize,
}

async fn run_arm(
    client: &mut LpClient<Box<dyn lpa_client::ClientIo>>,
    handle: WireProjectHandle,
    arm: &str,
    probes: Vec<ProjectProbeRequest>,
    args: &Args,
) -> Result<ArmResult> {
    let cadence_ms = args.cadence_ms as f64;
    let mut first: Option<Sample> = None;
    let mut last: Option<Sample> = None;
    let mut theoretical = Vec::new();
    let mut frame_us = Vec::new();
    let mut read_wall = Vec::new();
    let mut probe_bytes = 0usize;
    // Guest-time pacing: `ratio` is guest ms per wall ms, refined from every
    // pair of samples; `due` is the guest time the next read should start.
    let mut ratio = 1.0f64;
    let mut due: Option<f64> = None;
    let mut prev: Option<Sample> = None;
    for i in 0..(args.warmup + args.reads) {
        if let (Some(due), Some(prev)) = (due, prev) {
            // Where the guest clock is now, extrapolated from the last sample.
            let guest_now =
                f64::from(prev.frame_total_ms) + prev.wall.elapsed().as_secs_f64() * 1e3 * ratio;
            let wait_guest = due - guest_now;
            if wait_guest > 0.0 {
                let wait_wall = Duration::from_secs_f64(wait_guest / ratio / 1e3);
                tokio::time::sleep(wait_wall).await;
            }
        }
        let started = Instant::now();
        let events = read(client, handle, probes.clone()).await?;
        let took = started.elapsed();
        let sample = runtime_sample(&events, started).context("no runtime status in read")?;
        if let Some(prev) = prev {
            let dg = f64::from(sample.frame_total_ms.saturating_sub(prev.frame_total_ms));
            let dw = sample.wall.duration_since(prev.wall).as_secs_f64() * 1e3;
            if dg > 0.0 && dw > 0.0 {
                ratio = 0.5 * ratio + 0.5 * (dg / dw);
            }
        }
        // Studio's rule (`note_passive_refresh_completed`): the next pull is
        // due one cadence gap after this one COMPLETED, not after it started —
        // so a slow read stretches the cadence exactly as it does on silicon.
        // Completion in guest time ≈ the runtime stamp plus the read's wall
        // duration scaled by the current ratio.
        let completed_guest = f64::from(sample.frame_total_ms) + took.as_secs_f64() * 1e3 * ratio;
        due = Some(completed_guest + cadence_ms);
        prev = Some(sample);
        if i < args.warmup {
            continue;
        }
        if i == args.warmup {
            probe_bytes = events
                .iter()
                .map(|event| serde_json::to_vec(event).map(|v| v.len()).unwrap_or(0))
                .sum();
        }
        read_wall.push(took.as_secs_f64() * 1e3);
        if let Some(fps) = sample.theoretical_fps {
            theoretical.push(f64::from(fps));
        }
        if let Some(us) = sample.last_frame_time_us {
            frame_us.push(us as f64);
        }
        if first.is_none() {
            first = Some(sample);
        }
        last = Some(sample);
    }
    let (first, last) = (first.context("no samples")?, last.context("no samples")?);
    let guest_ms = f64::from(last.frame_total_ms.saturating_sub(first.frame_total_ms));
    let wall_ms = last.wall.duration_since(first.wall).as_secs_f64() * 1e3;
    let frames = (last.frame_num - first.frame_num) as f64;
    let mean = |v: &[f64]| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
    Ok(ArmResult {
        arm: arm.to_string(),
        fps: if guest_ms > 0.0 {
            frames * 1e3 / guest_ms
        } else {
            0.0
        },
        guest_per_wall: if wall_ms > 0.0 {
            guest_ms / wall_ms
        } else {
            0.0
        },
        reads_per_guest_s: if guest_ms > 0.0 {
            (args.reads - 1) as f64 * 1e3 / guest_ms
        } else {
            0.0
        },
        theoretical_fps: mean(&theoretical),
        frame_us: mean(&frame_us),
        read_wall_ms: mean(&read_wall).unwrap_or(0.0),
        probe_bytes,
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let dir = args.dir.canonicalize().context("project dir")?;
    let (project_uid, name) = lp_cli::commands::dev::validation::validate_local_project(&dir)?;
    let spec = HostSpecifier::parse(&args.host)?;
    eprintln!("probe_load: connecting to {spec:?}");
    let connection = cli_connect(spec, stderr_device_events(false)).await?;
    let mut client: LpClient<Box<dyn lpa_client::ClientIo>> = LpClient::new(connection.client_io());
    let files = collect_project_deploy_files(&LpFsStd::new(dir))?;
    eprintln!("probe_load: deploying {name} ({project_uid})");
    let handle = client
        .deploy_project_files(&project_uid, files)
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))?
        .into_value();
    wait_for_project_running(&mut client, handle, Duration::from_secs(120)).await?;
    let (visual, control) = primary_products(&mut client, handle).await?;
    eprintln!("probe_load: primary visual {visual:?}, primary control {control:?}");
    // Let the shader settle (first frames carry compile/warm-up cost).
    tokio::time::sleep(Duration::from_secs(3)).await;

    let mut results = Vec::new();
    for arm in &args.arms {
        let probes = probes_for(arm, visual, control)?;
        eprintln!(
            "probe_load: arm {arm} — {} probe(s), {} warm-up + {} reads at {} ms",
            probes.len(),
            args.warmup,
            args.reads,
            args.cadence_ms
        );
        results.push(run_arm(&mut client, handle, arm, probes, &args).await?);
    }
    drop(client);
    connection.close().await;

    println!(
        "| arm | board fps (guest) | reads / guest s | guest ms per wall ms | mean read wall ms | read bytes (json) | engine theoretical fps | last frame µs |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for r in &results {
        println!(
            "| {} | {:.2} | {:.2} | {:.3} | {:.1} | {} | {} | {} |",
            r.arm,
            r.fps,
            r.reads_per_guest_s,
            r.guest_per_wall,
            r.read_wall_ms,
            r.probe_bytes,
            r.theoretical_fps
                .map_or("—".to_string(), |v| format!("{v:.1}")),
            r.frame_us.map_or("—".to_string(), |v| format!("{v:.0}")),
        );
    }
    Ok(())
}

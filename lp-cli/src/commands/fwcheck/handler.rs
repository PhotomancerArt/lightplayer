use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use fw_checks::{FW_CHECK_JSON_PREFIX, FwCheckConfig, FwCheckTarget, all_checks, find_check};
use lpa_client::transport_serial::SerialLineObserver;
use lpa_client::{ClientTransport, LpClient};
use lpa_link::providers::host_serial_esp32::HostSerialEsp32Options;
use lpc_model::DEFAULT_SERIAL_BAUD_RATE;
use lpfs::{LpFs, LpFsStd};
use tokio::time::sleep;

use crate::client::host_serial_esp32::connect_host_serial_esp32_with_options;
use crate::commands::dev::{push_project_async, validation};

use super::args::{FwcheckCli, FwcheckCommand, FwcheckDemoArgs, FwcheckRunArgs, FwcheckTargetArg};
use super::{port, process, report, trace_dir};

struct Style {
    color: bool,
}

struct CaptureResult {
    report_text: String,
}

struct DemoResult {
    report_text: String,
}

pub fn handle_fwcheck(cli: FwcheckCli) -> Result<()> {
    match cli.command {
        FwcheckCommand::List => {
            for check in all_checks() {
                println!(
                    "{:<24} {:<32} targets={}",
                    check.slug(),
                    check.display_name,
                    check
                        .supported_targets
                        .iter()
                        .map(|target| target.slug())
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            Ok(())
        }
        FwcheckCommand::Port(args) => {
            println!("{}", port::resolve_esp32_port(args.port.as_deref())?);
            Ok(())
        }
        FwcheckCommand::Run(args) => run_check(args),
        FwcheckCommand::Demo(args) => run_demo(args),
    }
}

fn run_check(args: FwcheckRunArgs) -> Result<()> {
    let target = target_from_arg(args.target);
    let check = find_check(&args.check)
        .with_context(|| format!("unknown firmware check `{}`", args.check))?;
    if !check.supports_target(target) {
        bail!(
            "check `{}` does not support target `{target}`",
            check.slug()
        );
    }
    match target {
        FwCheckTarget::Esp32C6 => run_esp32c6(check, &args),
        FwCheckTarget::FwEmu => bail!("fw-emu fwcheck runner is not implemented yet"),
    }
}

fn run_demo(args: FwcheckDemoArgs) -> Result<()> {
    let target = target_from_arg(args.target);
    match target {
        FwCheckTarget::Esp32C6 => run_esp32c6_demo(&args),
        FwCheckTarget::FwEmu => bail!("fw-emu project demo runner is not implemented yet"),
    }
}

fn run_esp32c6(check: FwCheckConfig, args: &FwcheckRunArgs) -> Result<()> {
    if check.done_marker.is_none() {
        bail!(
            "check `{}` does not define a done marker yet; add one before running it through fwcheck",
            check.slug()
        );
    }
    let root = std::env::current_dir().context("current directory")?;
    let port = port::resolve_esp32_port(args.port.as_deref())?;
    let trace = trace_dir::create_trace_dir("esp32c6", check.trace_slug, args.note.as_deref())?;
    let features = features_for_esp32(check);
    let style = Style::detect();

    println!("{}", style.heading("Firmware Check"));
    println!("check:  {} ({})", check.slug(), check.display_name);
    println!("target: esp32c6");
    println!("fw:     fw-esp32 --features {features}");
    println!("port:   {port}");
    println!("trace:  {}", trace.dir.display());
    println!();

    let ((), build_elapsed) = run_step(&style, "Build firmware", args.verbose, || {
        process::cargo_build_fw_esp32(&root, &features, args.verbose)
    })?;
    let ((), flash_elapsed) = run_step(&style, "Flash firmware", args.verbose, || {
        process::flash_esp32(&root, &port, args.verbose)
    })?;
    let (capture, _run_elapsed) = run_step(&style, "Run check", args.verbose, || {
        capture_serial(&port, check, args.timeout_secs, &trace, args.verbose)
    })?;

    println!();
    println!("{}", style.heading("Summary"));
    println!("{}", capture.report_text.trim_end());
    println!();
    println!("{}", style.heading("Artifacts"));
    println!("trace:   {}", trace.trace_txt.display());
    println!("records: {}", trace.records_jsonl.display());
    println!("report:  {}", trace.report_txt.display());
    println!();
    println!(
        "{} build={} flash={}",
        style.dim("host steps:"),
        fmt_duration(build_elapsed),
        fmt_duration(flash_elapsed),
    );
    Ok(())
}

fn run_esp32c6_demo(args: &FwcheckDemoArgs) -> Result<()> {
    let root = std::env::current_dir().context("current directory")?;
    let project_dir = resolve_project_dir(&root, &args.project)?;
    let (project_uid, _project_name) = validation::validate_local_project(&project_dir)?;
    let project_slug = slug_from_project_dir(&project_dir);
    let port = port::resolve_esp32_port(args.port.as_deref())?;
    let trace = trace_dir::create_trace_dir(
        "esp32c6",
        &format!("demo-{project_slug}"),
        args.note.as_deref(),
    )?;
    let features = features_from_arg(&args.features);
    let style = Style::detect();

    println!("{}", style.heading("Firmware Demo Check"));
    println!("project: {}", project_dir.display());
    println!("target:  esp32c6");
    println!("fw:      fw-esp32 --features {features}");
    println!("port:    {port}");
    println!("trace:   {}", trace.dir.display());
    println!();

    let ((), build_elapsed) = run_step(&style, "Build firmware", args.verbose, || {
        process::cargo_build_fw_esp32(&root, &features, args.verbose)
    })?;
    let ((), flash_elapsed) = run_step(&style, "Flash firmware", args.verbose, || {
        process::flash_esp32_no_reset_erase_lpfs(&root, &port, args.verbose)
    })?;
    let (demo, run_elapsed) = run_step(&style, "Boot, push, and verify project", true, || {
        run_demo_capture(
            &port,
            &project_dir,
            &project_uid,
            args.timeout_secs,
            args.settle_secs,
            &trace,
        )
    })?;

    println!();
    println!("{}", style.heading("Summary"));
    println!("{}", demo.report_text.trim_end());
    println!();
    println!("{}", style.heading("Artifacts"));
    println!("trace:  {}", trace.trace_txt.display());
    println!("report: {}", trace.report_txt.display());
    println!();
    println!(
        "{} build={} flash={} run={}",
        style.dim("host steps:"),
        fmt_duration(build_elapsed),
        fmt_duration(flash_elapsed),
        fmt_duration(run_elapsed),
    );
    Ok(())
}

fn features_for_esp32(check: FwCheckConfig) -> String {
    ensure_esp32c6_feature(check.firmware_features.iter().copied())
}

fn features_from_arg(features: &str) -> String {
    ensure_esp32c6_feature(
        features
            .split(',')
            .map(str::trim)
            .filter(|feature| !feature.is_empty()),
    )
}

fn ensure_esp32c6_feature<'a>(features: impl IntoIterator<Item = &'a str>) -> String {
    let mut out: Vec<&str> = features.into_iter().collect();
    if !out.contains(&"esp32c6") {
        out.push("esp32c6");
    }
    out.join(",")
}

fn run_demo_capture(
    port_name: &str,
    project_dir: &Path,
    project_uid: &str,
    timeout_secs: u64,
    settle_secs: u64,
    trace: &trace_dir::TraceDir,
) -> Result<DemoResult> {
    let capture = Arc::new(SerialCapture::new(&trace.trace_txt)?);
    let observer: Arc<dyn SerialLineObserver> = capture.clone();
    let options = HostSerialEsp32Options {
        baud_rate: Some(DEFAULT_SERIAL_BAUD_RATE),
        reset_after_open: true,
        line_observer: Some(observer),
    };
    let transport = connect_host_serial_esp32_with_options(port_name, options)
        .map_err(|e| anyhow::anyhow!("Failed to create serial transport: {e}"))?;
    let transport: Box<dyn ClientTransport> = Box::new(transport);
    let shared_transport = Arc::new(tokio::sync::Mutex::new(transport));
    let client = LpClient::new_shared(Arc::clone(&shared_transport));
    let local_fs: Arc<dyn LpFs + Send + Sync> = Arc::new(LpFsStd::new(project_dir.to_owned()));
    let runtime = tokio::runtime::Runtime::new()?;

    let result = runtime.block_on(async {
        let run = run_demo_capture_async(
            &client,
            &capture,
            local_fs,
            project_uid,
            settle_secs,
            shared_transport,
        );
        match tokio::time::timeout(Duration::from_secs(timeout_secs), run).await {
            Ok(result) => result,
            Err(_) => bail!("timed out after {timeout_secs}s waiting for project to run"),
        }
    });

    capture.flush()?;
    let report_text = match result {
        Ok(report_text) => report_text,
        Err(err) => {
            let report_text = format!("status: failed\nerror: {err:#}\n");
            std::fs::write(&trace.report_txt, &report_text)
                .with_context(|| format!("write report {}", trace.report_txt.display()))?;
            return Err(err);
        }
    };
    std::fs::write(&trace.report_txt, &report_text)
        .with_context(|| format!("write report {}", trace.report_txt.display()))?;
    Ok(DemoResult { report_text })
}

async fn run_demo_capture_async(
    client: &LpClient,
    capture: &Arc<SerialCapture>,
    local_fs: Arc<dyn LpFs + Send + Sync>,
    project_uid: &str,
    settle_secs: u64,
    shared_transport: Arc<tokio::sync::Mutex<Box<dyn ClientTransport>>>,
) -> Result<String> {
    wait_for_boot_ready(capture).await?;

    if let Err(e) = run_client_step(capture, "stop all projects", client.stop_all_projects()).await
    {
        eprintln!("Warning: Failed to stop all projects: {e}");
        eprintln!("Continuing with project push...");
    }

    run_client_step(
        capture,
        "push project",
        push_project_async(client, local_fs.as_ref(), project_uid),
    )
    .await?;

    let project_path = format!("projects/{project_uid}");
    let handle = run_client_step(capture, "load project", client.project_load(&project_path))
        .await
        .with_context(|| format!("load {project_path}"))?;

    let _ = handle;

    sleep(Duration::from_secs(settle_secs)).await;
    capture.check_failure()?;

    let loaded = run_client_step(
        capture,
        "list loaded projects",
        client.project_list_loaded(),
    )
    .await?
    .len();
    capture.check_failure()?;

    let mut transport = shared_transport.lock().await;
    let _ = transport.close().await;

    Ok(format!(
        "status: ok\nproject: {project_path}\nloaded_projects: {loaded}\nsettled_for: {settle_secs}s\n",
    ))
}

async fn wait_for_boot_ready(capture: &Arc<SerialCapture>) -> Result<()> {
    loop {
        fail_after_grace_if_needed(capture).await?;
        if capture.boot_ready() {
            return Ok(());
        }
        sleep(Duration::from_millis(50)).await;
    }
}

async fn run_client_step<T, F>(capture: &Arc<SerialCapture>, label: &str, future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return result,
            _ = sleep(Duration::from_millis(50)) => {
                fail_after_grace_if_needed(capture).await.with_context(|| format!("{label} failed"))?;
            }
        }
    }
}

async fn fail_after_grace_if_needed(capture: &Arc<SerialCapture>) -> Result<()> {
    if let Some(line) = capture.failure_message() {
        sleep(Duration::from_secs(2)).await;
        capture.flush()?;
        bail!("device reported failure: {line}");
    }
    Ok(())
}

fn capture_serial(
    port_name: &str,
    check: FwCheckConfig,
    timeout_secs: u64,
    trace: &trace_dir::TraceDir,
    verbose: bool,
) -> Result<CaptureResult> {
    let marker = check
        .done_marker
        .with_context(|| format!("check `{}` does not define a done marker", check.slug()))?;
    let mut port = serialport::new(port_name, lpc_model::DEFAULT_SERIAL_BAUD_RATE)
        .timeout(Duration::from_millis(100))
        .open()
        .with_context(|| format!("open serial port {port_name}"))?;
    let mut trace_file = std::fs::File::create(&trace.trace_txt)
        .with_context(|| format!("create trace {}", trace.trace_txt.display()))?;
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let mut seen = String::new();
    let mut buf = [0u8; 1024];

    while Instant::now() < deadline {
        match port.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                let text = normalize_serial_text(&buf[..n]);
                if verbose {
                    print!("{text}");
                    std::io::stdout().flush().ok();
                }
                trace_file.write_all(text.as_bytes())?;
                seen.push_str(&text);
                if seen.contains(marker) {
                    break;
                }
                if seen.len() > marker.len().saturating_add(4096) {
                    let keep = marker.len().saturating_add(4096);
                    seen = trim_seen_buffer(&seen, keep);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {}
            Err(err) => return Err(err).context("read serial"),
        }
    }

    if !seen.contains(marker) {
        bail!("timed out waiting for marker `{marker}`");
    }

    trace_file.flush()?;
    drop(trace_file);
    let trace_text = std::fs::read_to_string(&trace.trace_txt)
        .with_context(|| format!("read trace {}", trace.trace_txt.display()))?;
    let records = extract_records(&trace_text);
    std::fs::write(&trace.records_jsonl, &records)
        .with_context(|| format!("write records {}", trace.records_jsonl.display()))?;
    report::write_report(check.slug(), &records, &trace.report_txt)?;
    if verbose {
        println!();
    }
    let report_text = std::fs::read_to_string(&trace.report_txt)
        .with_context(|| format!("read report {}", trace.report_txt.display()))?;
    Ok(CaptureResult { report_text })
}

struct SerialCapture {
    trace_file: Mutex<std::fs::File>,
    failure: Mutex<Option<String>>,
    boot_ready: std::sync::atomic::AtomicBool,
}

impl SerialCapture {
    fn new(path: &Path) -> Result<Self> {
        let trace_file = std::fs::File::create(path)
            .with_context(|| format!("create trace {}", path.display()))?;
        Ok(Self {
            trace_file: Mutex::new(trace_file),
            failure: Mutex::new(None),
            boot_ready: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn boot_ready(&self) -> bool {
        self.boot_ready.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn check_failure(&self) -> Result<()> {
        if let Some(line) = self.failure_message() {
            bail!("device reported failure: {line}");
        }
        Ok(())
    }

    fn failure_message(&self) -> Option<String> {
        self.failure.lock().expect("serial failure mutex").clone()
    }

    fn flush(&self) -> Result<()> {
        self.trace_file
            .lock()
            .expect("serial trace mutex")
            .flush()
            .context("flush serial trace")
    }
}

impl SerialLineObserver for SerialCapture {
    fn observe_line(&self, line: &str) {
        if let Ok(mut file) = self.trace_file.lock() {
            writeln!(file, "{line}").ok();
        }
        if line.contains("[INIT] fw-esp32 initialized, starting server loop") {
            self.boot_ready
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if is_device_failure_line(line) {
            let mut failure = self.failure.lock().expect("serial failure mutex");
            if failure.is_none() {
                *failure = Some(line.to_owned());
            }
        }
    }
}

fn is_device_failure_line(line: &str) -> bool {
    line.contains("OOM")
        || line.contains("panicked at")
        || line.contains("Exception '")
        || line.contains("fatal:")
}

fn resolve_project_dir(root: &Path, project: &Path) -> Result<PathBuf> {
    let direct = if project.is_absolute() {
        project.to_path_buf()
    } else {
        root.join(project)
    };
    if direct.exists() {
        return direct
            .canonicalize()
            .with_context(|| format!("resolve project directory {}", direct.display()));
    }

    if project.components().count() == 1 {
        let example = root.join("examples").join(project);
        if example.exists() {
            return example
                .canonicalize()
                .with_context(|| format!("resolve project directory {}", example.display()));
        }
    }

    bail!(
        "project directory not found: {} (also tried examples/{})",
        direct.display(),
        project.display()
    );
}

fn slug_from_project_dir(project_dir: &Path) -> String {
    project_dir
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_slug)
        .filter(|slug| !slug.is_empty())
        .unwrap_or_else(|| "project".to_owned())
}

fn sanitize_slug(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.') && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

fn normalize_serial_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn extract_records(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if let Some((_, json)) = line.split_once(FW_CHECK_JSON_PREFIX) {
            out.push_str(json.trim());
            out.push('\n');
        }
    }
    out
}

fn trim_seen_buffer(seen: &str, keep: usize) -> String {
    if keep >= seen.len() {
        return seen.to_owned();
    }
    let mut start = seen.len() - keep;
    while start < seen.len() && !seen.is_char_boundary(start) {
        start += 1;
    }
    seen[start..].to_owned()
}

fn target_from_arg(target: FwcheckTargetArg) -> FwCheckTarget {
    match target {
        FwcheckTargetArg::Esp32C6 => FwCheckTarget::Esp32C6,
        FwcheckTargetArg::FwEmu => FwCheckTarget::FwEmu,
    }
}

fn run_step<T>(
    style: &Style,
    label: &str,
    verbose: bool,
    run: impl FnOnce() -> Result<T>,
) -> Result<(T, Duration)> {
    print!("{} {label} ...", style.dim("->"));
    std::io::stdout().flush().ok();
    if verbose {
        println!();
    } else {
        print!(" ");
        std::io::stdout().flush().ok();
    }
    let start = Instant::now();
    let result = run();
    let elapsed = start.elapsed();
    match result {
        Ok(value) => {
            if verbose {
                println!("   {} {label} ({})", style.ok("ok"), fmt_duration(elapsed));
            } else {
                println!("{} ({})", style.ok("ok"), fmt_duration(elapsed));
            }
            Ok((value, elapsed))
        }
        Err(err) => {
            if !verbose {
                println!("{}", style.err("failed"));
            }
            Err(err)
        }
    }
}

fn fmt_duration(duration: Duration) -> String {
    let ms = duration.as_millis();
    if ms < 1_000 {
        format!("{ms}ms")
    } else {
        format!("{}.{:01}s", ms / 1_000, (ms % 1_000) / 100)
    }
}

impl Style {
    fn detect() -> Self {
        Self {
            color: std::io::stdout().is_terminal(),
        }
    }

    fn heading(&self, text: &str) -> String {
        self.paint("\x1b[1;36m", text)
    }

    fn ok(&self, text: &str) -> String {
        self.paint("\x1b[1;32m", text)
    }

    fn err(&self, text: &str) -> String {
        self.paint("\x1b[1;31m", text)
    }

    fn dim(&self, text: &str) -> String {
        self.paint("\x1b[2m", text)
    }

    fn paint(&self, prefix: &str, text: &str) -> String {
        if self.color {
            format!("{prefix}{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{extract_records, fmt_duration, trim_seen_buffer};

    #[test]
    fn extracts_prefixed_json_records() {
        let records =
            extract_records("boot\n[INFO] [fw-check-json] {\"kind\":\"case-summary\"}\nnoise\n");
        assert_eq!(records, "{\"kind\":\"case-summary\"}\n");
    }

    #[test]
    fn trims_seen_buffer_on_char_boundary() {
        assert_eq!(trim_seen_buffer("abcédef", 5), "édef");
    }

    #[test]
    fn formats_step_durations() {
        assert_eq!(fmt_duration(Duration::from_millis(42)), "42ms");
        assert_eq!(fmt_duration(Duration::from_millis(1_250)), "1.2s");
    }
}

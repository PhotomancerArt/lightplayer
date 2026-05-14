use std::io::{Read, Write};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use fw_checks::{FW_CHECK_JSON_PREFIX, FwCheckConfig, FwCheckTarget, all_checks, find_check};

use super::args::{FwcheckCli, FwcheckCommand, FwcheckRunArgs, FwcheckTargetArg};
use super::{port, process, report, trace_dir};

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
        FwcheckCommand::Run(args) => run_check(args),
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

    println!("fwcheck esp32c6 {}", check.slug());
    println!("port: {port}");
    println!("features: {features}");
    println!("trace: {}", trace.dir.display());

    process::cargo_build_fw_esp32(&root, &features)?;
    process::flash_esp32(&root, &port)?;
    capture_serial(&port, check, args.timeout_secs, &trace)?;
    Ok(())
}

fn features_for_esp32(check: FwCheckConfig) -> String {
    let mut features = check.firmware_features.to_vec();
    if !features.contains(&"esp32c6") {
        features.push("esp32c6");
    }
    features.join(",")
}

fn capture_serial(
    port_name: &str,
    check: FwCheckConfig,
    timeout_secs: u64,
    trace: &trace_dir::TraceDir,
) -> Result<()> {
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
                print!("{text}");
                std::io::stdout().flush().ok();
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
    println!("trace: {}", trace.trace_txt.display());
    println!("records: {}", trace.records_jsonl.display());
    println!("report: {}", trace.report_txt.display());
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{extract_records, trim_seen_buffer};

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
}

use anyhow::{Result, anyhow, bail};
use dialoguer::{Confirm, Input, Select};
use lpc_shared::hardware::{HardwareManifestFile, HardwareTarget};
use std::io::{IsTerminal, stdin};
use std::time::Duration;

use crate::commands::hardware::args::{CalibrateArgs, HardwareTargetArg};
use crate::commands::hardware::manifest::board_manifest_store::BoardManifestStore;

use super::calibration_command::{CalibrationEvent, PromptCommand};
use super::calibration_manifest_update::{
    GpioCandidate, apply_dangerous, apply_mapping, gpio_candidates, validate_board_label,
};
use super::calibration_resume::{CalibrationResumeState, load_resume, resume_path, save_resume};
use super::calibration_serial::{SerialCalibrationTransport, ensure_firmware_ready};

pub fn handle_calibrate(args: CalibrateArgs) -> Result<()> {
    let store = BoardManifestStore::discover(args.repo, args.boards_dir)?;
    let target_arg = args.target.unwrap_or(HardwareTargetArg::Esp32c6);
    let target: HardwareTarget = target_arg.into();
    let board_id = match args.board {
        Some(board) => board,
        None => prompt_board(&store, target)?,
    };
    let mut manifest = store.load(&board_id)?;
    validate_manifest_target(&manifest, target)?;

    let one_label_run = args.label.is_some();
    let mut label = match args.label {
        Some(label) => label,
        None => prompt_board_label()?,
    };
    label = label.trim().to_string();
    validate_board_label(&label)?;

    let resume_path = resume_path(store.repo_root(), &board_id);
    let mut state = load_resume(&resume_path)?
        .filter(|state| state.board_id == board_id && state.target == target)
        .unwrap_or_else(|| CalibrationResumeState::new(board_id.clone(), target, label.clone()));
    state.board_label = label.clone();

    let mut candidates = gpio_candidates(&manifest);
    if candidates.is_empty() {
        bail!(
            "manifest {} has no calibratable GPIO resources",
            manifest.id
        );
    }

    print_manifest_summary(&manifest);
    print_intro(&label, &manifest);

    let timeout = Duration::from_millis(args.timeout_ms.max(1));
    let mut transport = SerialCalibrationTransport::open(args.port.as_deref(), timeout)?;
    ensure_firmware_ready(&mut transport, timeout)?;

    let mut index = state.current_index.min(candidates.len().saturating_sub(1));
    loop {
        let candidate = candidates[index].clone();
        let pulse_status = match pulse_candidate(&mut transport, &candidate, timeout) {
            Err(error) if is_serial_disconnect(&error) => PulseStatus::LostConnection {
                message: error.to_string(),
            },
            Err(error) => return Err(error),
            Ok(status) => status,
        };
        match pulse_status {
            PulseStatus::Alive => match prompt_square_wave(&candidate)? {
                PromptCommand::Next => {
                    transport.send_line("STOP")?;
                    index += 1;
                    if index >= candidates.len() {
                        println!("Reached the end of GPIO candidates without finding {label}.");
                        state.current_index = candidates.len().saturating_sub(1);
                        save_resume(&resume_path, &state)?;
                        return Ok(());
                    }
                    state.current_index = index;
                    save_resume(&resume_path, &state)?;
                }
                PromptCommand::Previous => {
                    transport.send_line("STOP")?;
                    index = index.saturating_sub(1);
                    state.current_index = index;
                    save_resume(&resume_path, &state)?;
                }
                PromptCommand::Yes => {
                    transport.send_line("STOP")?;
                    apply_mapping(&mut manifest, candidate.gpio, &label)?;
                    store.save(&manifest, true)?;
                    state.record_mapping(candidate.gpio, &label);
                    index += 1;
                    state.current_index = index.min(candidates.len().saturating_sub(1));
                    save_resume(&resume_path, &state)?;
                    println!("Recorded {label} as {}.", candidate.address);
                    if one_label_run {
                        return Ok(());
                    }
                    if index >= candidates.len() {
                        println!("Reached the end of GPIO candidates.");
                        return Ok(());
                    }
                    match prompt_next_board_label()? {
                        Some(next_label) => {
                            label = next_label;
                            state.board_label = label.clone();
                            state.current_index = index;
                            save_resume(&resume_path, &state)?;
                            print_next_label_intro(&label);
                        }
                        None => {
                            println!(
                                "Calibration paused. Resume state saved to {}.",
                                resume_path.display()
                            );
                            return Ok(());
                        }
                    }
                }
                PromptCommand::Quit => {
                    transport.send_line("STOP")?;
                    state.current_index = index;
                    save_resume(&resume_path, &state)?;
                    println!(
                        "Calibration paused. Resume state saved to {}.",
                        resume_path.display()
                    );
                    return Ok(());
                }
            },
            PulseStatus::Timeout | PulseStatus::LostConnection { .. } => {
                if let PulseStatus::LostConnection { message } = &pulse_status {
                    println!(
                        "{} lost USB serial while being tested ({message}). The device may have reset or this pin may be dangerous.",
                        candidate.address
                    );
                } else {
                    println!(
                        "{} did not produce calibration logs within {}ms. The device may have reset or this pin may be dangerous.",
                        candidate.address, args.timeout_ms
                    );
                }
                if Confirm::new()
                    .with_prompt(format!(
                        "Mark {} as dangerous and skip it in future calibration?",
                        candidate.address
                    ))
                    .default(false)
                    .interact()?
                {
                    apply_dangerous(&mut manifest, candidate.gpio)?;
                    store.save(&manifest, true)?;
                    state.record_dangerous(candidate.gpio, true);
                    candidates = gpio_candidates(&manifest);
                    if candidates.is_empty() {
                        bail!("all GPIO candidates are now reserved");
                    }
                    index = index.min(candidates.len().saturating_sub(1));
                } else {
                    state.record_dangerous(candidate.gpio, false);
                    index += 1;
                    if index >= candidates.len() {
                        println!("Reached the end of GPIO candidates without finding {label}.");
                        state.current_index = candidates.len().saturating_sub(1);
                        save_resume(&resume_path, &state)?;
                        return Ok(());
                    }
                }
                state.current_index = index.min(candidates.len().saturating_sub(1));
                save_resume(&resume_path, &state)?;
                wait_for_manual_reset(&mut transport, timeout)?;
            }
        }
    }
}

fn prompt_board(store: &BoardManifestStore, target: HardwareTarget) -> Result<String> {
    if !stdin().is_terminal() {
        bail!("--board is required when calibration is not running in an interactive terminal");
    }
    let manifests: Vec<_> = store
        .list()?
        .into_iter()
        .filter(|manifest| manifest.target == target.to_string())
        .collect();
    if manifests.is_empty() {
        bail!(
            "no {target} board manifests found in {}",
            store.boards_dir().display()
        );
    }
    let items: Vec<_> = manifests
        .iter()
        .map(|manifest| format!("{} - {} {}", manifest.id, manifest.vendor, manifest.product))
        .collect();
    let choice = Select::new()
        .with_prompt(format!("Board manifest for {target} calibration"))
        .items(&items)
        .default(0)
        .interact()?;
    Ok(manifests[choice].id.clone())
}

pub fn validate_manifest_target(
    manifest: &HardwareManifestFile,
    expected: HardwareTarget,
) -> Result<()> {
    if manifest.target != expected {
        bail!(
            "manifest {} targets {}, but calibration target is {}",
            manifest.id,
            manifest.target,
            expected
        );
    }
    Ok(())
}

fn prompt_board_label() -> Result<String> {
    if !std::io::stdin().is_terminal() {
        bail!("--label is required when calibration is not running in an interactive terminal");
    }
    Ok(Input::new()
        .with_prompt("Board label currently connected to the scope")
        .interact_text()?)
}

fn prompt_next_board_label() -> Result<Option<String>> {
    if !stdin().is_terminal() {
        return Ok(None);
    }
    loop {
        let label: String = Input::new()
            .with_prompt("Next board label to calibrate (blank/q to quit)")
            .allow_empty(true)
            .interact_text()?;
        let label = label.trim();
        if label.is_empty() || label.eq_ignore_ascii_case("q") {
            return Ok(None);
        }
        validate_board_label(label)?;
        return Ok(Some(label.to_string()));
    }
}

fn print_intro(label: &str, manifest: &HardwareManifestFile) {
    println!(
        "Calibrating {} ({}) for board label {label}.",
        manifest.id, manifest.product
    );
    println!("Attach the scope to that board label.");
    println!(
        "Press Enter for no/next, y when the square wave is present, p for previous, q to quit."
    );
}

fn print_manifest_summary(manifest: &HardwareManifestFile) {
    let mut mapped = Vec::new();
    let mut provisional = Vec::new();
    let mut reserved = Vec::new();

    let mut resources: Vec<_> = manifest
        .gpio
        .iter()
        .filter_map(|resource| {
            super::calibration_manifest_update::parse_gpio_address(&resource.address)
                .map(|gpio| (gpio, resource))
        })
        .collect();
    resources.sort_by_key(|(gpio, _)| *gpio);

    for (gpio, resource) in resources {
        let item = format!("/gpio/{gpio}: {}", resource.display_label);
        if let Some(reason) = &resource.reserved_reason {
            reserved.push(format!("{item} ({reason})"));
        } else if is_provisional_gpio_label(gpio, &resource.display_label) {
            provisional.push(item);
        } else {
            mapped.push(item);
        }
    }

    println!("Current GPIO manifest summary:");
    print_summary_group("mapped", &mapped);
    print_summary_group("provisional", &provisional);
    print_summary_group("reserved", &reserved);
}

fn print_summary_group(name: &str, items: &[String]) {
    if items.is_empty() {
        println!("  {name}: none");
    } else {
        println!("  {name}: {}", items.join(", "));
    }
}

fn is_provisional_gpio_label(gpio: u32, label: &str) -> bool {
    label == format!("GPIO{gpio}") || label == format!("IO{gpio}") || label == gpio.to_string()
}

fn print_next_label_intro(label: &str) {
    println!("Move the scope to board label {label}.");
    println!(
        "Press Enter for no/next, y when the square wave is present, p for previous, q to quit."
    );
}

fn pulse_candidate(
    transport: &mut SerialCalibrationTransport,
    candidate: &GpioCandidate,
    timeout: Duration,
) -> Result<PulseStatus> {
    println!(
        "Testing {} currently labeled {}...",
        candidate.address, candidate.display_label
    );
    transport.send_line(&format!("PULSE {}", candidate.gpio))?;
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Some(line) = transport.read_line_until(Duration::from_millis(100))? {
            match CalibrationEvent::parse(&line) {
                CalibrationEvent::Open { gpio } | CalibrationEvent::Pulse { gpio }
                    if gpio == candidate.gpio =>
                {
                    return Ok(PulseStatus::Alive);
                }
                CalibrationEvent::Error { message } => return Err(anyhow!(message)),
                _ => {}
            }
        }
    }
    Ok(PulseStatus::Timeout)
}

fn prompt_square_wave(candidate: &GpioCandidate) -> Result<PromptCommand> {
    loop {
        let answer: String = Input::new()
            .with_prompt(format!(
                "Is the square wave present on {}? (q/p/y/N)",
                candidate.address
            ))
            .allow_empty(true)
            .interact_text()?;
        match PromptCommand::parse(&answer) {
            Ok(command) => return Ok(command),
            Err(message) => println!("{message}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PulseStatus {
    Alive,
    Timeout,
    LostConnection { message: String },
}

fn wait_for_manual_reset(
    transport: &mut SerialCalibrationTransport,
    timeout: Duration,
) -> Result<()> {
    loop {
        println!(
            "macOS cannot reliably power-cycle this USB port from here. Manually reset or replug the board, then press Enter."
        );
        if !stdin().is_terminal() {
            bail!("device disconnected and manual reset is required, but stdin is not interactive");
        }
        let _: String = Input::new()
            .with_prompt("Press Enter after the board is visible again")
            .allow_empty(true)
            .interact_text()?;
        match transport
            .reconnect(timeout)
            .and_then(|_| ensure_firmware_ready(transport, timeout))
        {
            Ok(()) => return Ok(()),
            Err(error) => {
                println!("Still waiting for calibration firmware: {error}");
            }
        }
    }
}

fn is_serial_disconnect(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("broken pipe")
        || text.contains("no such file")
        || text.contains("i/o error")
        || text.contains("input/output")
        || text.contains("device not configured")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_target_mismatch() {
        let manifest =
            HardwareManifestFile::new("seeed/xiao", HardwareTarget::Rv32imacEmu, "seeed", "xiao");

        assert!(validate_manifest_target(&manifest, HardwareTarget::Esp32c6).is_err());
    }
}

use anyhow::{Result, anyhow, bail};
use dialoguer::{Confirm, Input};
use lpc_shared::hardware::{HardwareManifestFile, HardwareTarget};
use std::io::IsTerminal;
use std::time::Duration;

use crate::commands::hardware::args::CalibrateArgs;
use crate::commands::hardware::manifest::board_manifest_store::BoardManifestStore;

use super::calibration_command::{CalibrationEvent, PromptCommand};
use super::calibration_manifest_update::{
    GpioCandidate, apply_dangerous, apply_mapping, gpio_candidates, validate_board_label,
};
use super::calibration_resume::{CalibrationResumeState, load_resume, resume_path, save_resume};
use super::calibration_serial::{SerialCalibrationTransport, ensure_firmware_ready};

pub fn handle_calibrate(args: CalibrateArgs) -> Result<()> {
    let target: HardwareTarget = args.target.into();
    let store = BoardManifestStore::discover(args.repo, args.boards_dir)?;
    let mut manifest = store.load(&args.board)?;
    validate_manifest_target(&manifest, target)?;

    let mut label = match args.label {
        Some(label) => label,
        None => prompt_board_label()?,
    };
    label = label.trim().to_string();
    validate_board_label(&label)?;

    let resume_path = resume_path(store.repo_root(), &args.board);
    let mut state = load_resume(&resume_path)?
        .filter(|state| state.board_id == args.board && state.target == target)
        .unwrap_or_else(|| CalibrationResumeState::new(args.board.clone(), target, label.clone()));
    state.board_label = label.clone();

    let mut candidates = gpio_candidates(&manifest);
    if candidates.is_empty() {
        bail!(
            "manifest {} has no calibratable GPIO resources",
            manifest.id
        );
    }

    print_intro(&label, &manifest);

    let timeout = Duration::from_millis(args.timeout_ms.max(1));
    let mut transport = SerialCalibrationTransport::open(args.port.as_deref(), timeout)?;
    ensure_firmware_ready(&mut transport, timeout)?;

    let mut index = state.current_index.min(candidates.len().saturating_sub(1));
    loop {
        let candidate = candidates[index].clone();
        match pulse_candidate(&mut transport, &candidate, timeout)? {
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
                    state.current_index = index;
                    save_resume(&resume_path, &state)?;
                    println!("Recorded {label} as {}.", candidate.address);
                    return Ok(());
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
            PulseStatus::Timeout => {
                println!(
                    "{} did not produce calibration logs within {}ms. The device may have reset or this pin may be dangerous.",
                    candidate.address, args.timeout_ms
                );
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
                println!(
                    "Reconnect or reset the board if needed; waiting for calibration firmware..."
                );
                transport.reconnect(timeout)?;
                ensure_firmware_ready(&mut transport, timeout)?;
            }
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PulseStatus {
    Alive,
    Timeout,
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

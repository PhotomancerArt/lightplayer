use anyhow::{Result, anyhow, bail};
use dialoguer::Input;
use lpc_hardware::hardware::{HardwareManifestFile, HardwareTarget};
use std::io::{IsTerminal, stdin};
use std::time::Duration;

use crate::commands::hardware::args::{CalibrateArgs, HardwareTargetArg};
use crate::commands::hardware::manifest::board_manifest_store::BoardManifestStore;

use super::calibration_command::CalibrationEvent;
use super::calibration_manifest_update::GpioCandidate;
use super::calibration_serial::{SerialCalibrationTransport, ensure_firmware_ready};
use super::model;
use super::screen_board;
use super::screen_board_select;
use super::screen_label_list;
use super::screen_pin;
use super::screen_search;

const MANUAL_RESET_TIMEOUT: Duration = Duration::from_secs(10);

pub enum Route {
    Board,
    LabelList,
    Pin(String),
    Search(String),
    Quit,
}

pub struct App {
    pub store: BoardManifestStore,
    pub manifest: HardwareManifestFile,
    pub target: HardwareTarget,
    pub port: Option<String>,
    pub timeout: Duration,
    pub transport: Option<SerialCalibrationTransport>,
}

impl App {
    pub fn from_args(args: CalibrateArgs) -> Result<(Self, Route)> {
        let store = BoardManifestStore::discover(args.repo, args.boards_dir)?;
        let target_arg = args.target.unwrap_or(HardwareTargetArg::Esp32c6);
        let target: HardwareTarget = target_arg.into();
        let board_id = match args.board {
            Some(board) => board,
            None => screen_board_select::choose_board(&store, target)?,
        };
        let mut manifest = store.load(&board_id)?;
        validate_manifest_target(&manifest, target)?;
        let before_sync = manifest.board_label.clone();
        model::sync_board_labels_from_gpio(&mut manifest);
        let route = match args.label {
            Some(label) => {
                model::ensure_label(&mut manifest, &label)?;
                Route::Pin(label.trim().to_string())
            }
            None if manifest.board_label.is_empty() => Route::LabelList,
            None => Route::Board,
        };
        let app = Self {
            store,
            manifest,
            target,
            port: args.port,
            timeout: Duration::from_millis(args.timeout_ms.max(1)),
            transport: None,
        };
        if app.manifest.board_label != before_sync {
            app.save()?;
        }
        Ok((app, route))
    }

    pub fn run(&mut self, mut route: Route) -> Result<()> {
        loop {
            route = match route {
                Route::Board => screen_board::show(self)?,
                Route::LabelList => screen_label_list::show(self)?,
                Route::Pin(label) => screen_pin::show(self, label)?,
                Route::Search(label) => screen_search::show(self, label)?,
                Route::Quit => return Ok(()),
            };
        }
    }

    pub fn save(&self) -> Result<()> {
        self.store.save(&self.manifest, true)?;
        Ok(())
    }

    pub fn ensure_transport(&mut self) -> Result<&mut SerialCalibrationTransport> {
        if self.transport.is_none() {
            let mut transport =
                SerialCalibrationTransport::open(self.port.as_deref(), self.timeout)?;
            ensure_firmware_ready(&mut transport, self.timeout)?;
            self.transport = Some(transport);
        }
        Ok(self.transport.as_mut().expect("transport initialized"))
    }

    pub fn stop_pulse(&mut self) {
        if let Some(transport) = self.transport.as_mut() {
            let _ = transport.send_line("STOP");
        }
    }

    pub fn pulse_candidate(&mut self, candidate: &GpioCandidate) -> Result<PulseStatus> {
        println!(
            "Testing {} currently labeled {}...",
            candidate.address, candidate.display_label
        );
        let timeout = self.timeout;
        let transport = self.ensure_transport()?;
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

    pub fn wait_for_manual_reset(&mut self) -> Result<()> {
        loop {
            println!();
            println!(
                "macOS cannot reliably power-cycle this USB port from here. Manually reset or replug the board, then press Enter."
            );
            if !stdin().is_terminal() {
                bail!(
                    "device disconnected and manual reset is required, but stdin is not interactive"
                );
            }
            let _: String = Input::new()
                .with_prompt("Press Enter after the board is visible again")
                .allow_empty(true)
                .interact_text()?;
            let result = match self.transport.as_mut() {
                Some(transport) => transport
                    .reconnect(MANUAL_RESET_TIMEOUT)
                    .and_then(|_| ensure_firmware_ready(transport, MANUAL_RESET_TIMEOUT)),
                None => SerialCalibrationTransport::open(self.port.as_deref(), self.timeout)
                    .and_then(|mut transport| {
                        ensure_firmware_ready(&mut transport, MANUAL_RESET_TIMEOUT)?;
                        self.transport = Some(transport);
                        Ok(())
                    }),
            };
            match result {
                Ok(()) => {
                    println!("Calibration firmware is responding again.");
                    println!();
                    return Ok(());
                }
                Err(error) => {
                    println!();
                    println!("Still waiting for calibration firmware: {error}");
                    println!(
                        "The /dev entry can appear before the firmware is ready; a hard reset or replug may still be needed."
                    );
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PulseStatus {
    Alive,
    Timeout,
    LostConnection { message: String },
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

pub fn is_serial_disconnect(error: &anyhow::Error) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("broken pipe")
        || text.contains("no such file")
        || text.contains("i/o error")
        || text.contains("input/output")
        || text.contains("device not configured")
}

pub fn handle_calibrate(args: CalibrateArgs) -> Result<()> {
    let (mut app, route) = App::from_args(args)?;
    app.run(route)
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

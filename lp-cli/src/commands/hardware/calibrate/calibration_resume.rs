use anyhow::{Context, Result};
use lpc_shared::hardware::HardwareTarget;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrationResumeState {
    pub board_id: String,
    pub target: HardwareTarget,
    pub board_label: String,
    pub current_index: usize,
    pub mappings: Vec<CalibrationMapping>,
    pub dangerous_pins: Vec<DangerousPin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalibrationMapping {
    pub gpio: u32,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DangerousPin {
    pub gpio: u32,
    pub confirmed: bool,
}

impl CalibrationResumeState {
    pub fn new(board_id: String, target: HardwareTarget, board_label: String) -> Self {
        Self {
            board_id,
            target,
            board_label,
            current_index: 0,
            mappings: Vec::new(),
            dangerous_pins: Vec::new(),
        }
    }

    pub fn record_mapping(&mut self, gpio: u32, label: &str) {
        self.mappings.push(CalibrationMapping {
            gpio,
            label: label.into(),
        });
    }

    pub fn record_dangerous(&mut self, gpio: u32, confirmed: bool) {
        self.dangerous_pins.push(DangerousPin { gpio, confirmed });
    }
}

pub fn resume_path(repo_root: &Path, board_id: &str) -> PathBuf {
    repo_root
        .join("target/hardware-calibration")
        .join(format!("{}.json", board_id.replace('/', "__")))
}

pub fn load_resume(path: &Path) -> Result<Option<CalibrationResumeState>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read calibration resume {}", path.display()))?;
    let state = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse calibration resume {}", path.display()))?;
    Ok(Some(state))
}

pub fn save_resume(path: &Path, state: &CalibrationResumeState) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(state)?;
    fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_path_uses_manifest_id() {
        let path = resume_path(Path::new("/repo"), "seeed/xiao-esp32-c6");
        assert_eq!(
            path,
            Path::new("/repo/target/hardware-calibration/seeed__xiao-esp32-c6.json")
        );
    }

    #[test]
    fn resume_state_round_trips_json() {
        let state = CalibrationResumeState {
            board_id: "seeed/xiao".into(),
            target: HardwareTarget::Esp32c6,
            board_label: "D6".into(),
            current_index: 3,
            mappings: vec![CalibrationMapping {
                gpio: 18,
                label: "D6".into(),
            }],
            dangerous_pins: vec![DangerousPin {
                gpio: 12,
                confirmed: true,
            }],
        };

        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(
            serde_json::from_str::<CalibrationResumeState>(&json).unwrap(),
            state
        );
    }
}

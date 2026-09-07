use serde::{Deserialize, Serialize};

/// Build or runtime target that a board manifest describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum HardwareTarget {
    /// Classic ESP32 (Xtensa LX6) — `fw-esp32v3`.
    Esp32,
    Esp32c6,
    Esp32s3,
    Rv32imacEmu,
    /// A computer running the desktop firmware — `fw-browser` in a tab,
    /// `fw-host` on a machine. It has no pins, so its board profile
    /// (`boards/lightplayer/desktop.json`) is a virtual, deliberately
    /// unlimited table rather than a calibrated pin map.
    Desktop,
}

impl HardwareTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Esp32 => "esp32",
            Self::Esp32c6 => "esp32c6",
            Self::Esp32s3 => "esp32s3",
            Self::Rv32imacEmu => "rv32imac_emu",
            Self::Desktop => "desktop",
        }
    }
}

impl core::fmt::Display for HardwareTarget {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The serde spelling IS the `target` value board files carry and the
    /// enum `schemas/hardware.schema.json` generates, so it is pinned here
    /// rather than left to `rename_all` to keep by accident.
    #[test]
    fn serde_spelling_matches_as_str() {
        for target in [
            HardwareTarget::Esp32,
            HardwareTarget::Esp32c6,
            HardwareTarget::Esp32s3,
            HardwareTarget::Rv32imacEmu,
            HardwareTarget::Desktop,
        ] {
            let json = serde_json::to_string(&target).expect("serialize target");
            assert_eq!(json, alloc::format!("\"{}\"", target.as_str()));
            assert_eq!(
                serde_json::from_str::<HardwareTarget>(&json).expect("round trip"),
                target
            );
        }
    }
}

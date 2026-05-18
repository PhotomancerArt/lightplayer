use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareTarget {
    Esp32c6,
    Rv32imacEmu,
}

impl HardwareTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Esp32c6 => "esp32c6",
            Self::Rv32imacEmu => "rv32imac_emu",
        }
    }
}

impl core::fmt::Display for HardwareTarget {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

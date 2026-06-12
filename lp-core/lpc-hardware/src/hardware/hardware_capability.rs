use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HardwareCapability {
    GpioOutput,
    GpioInput,
    Ws281xOutput,
    Rmt,
    Radio,
}

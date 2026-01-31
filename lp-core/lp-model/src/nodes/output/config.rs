use crate::nodes::{NodeConfig, NodeKind};
use serde::{Deserialize, Serialize};

/// Output node configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputConfig {
    /// GPIO strip output
    GpioStrip {
        pin: u32,
        // channel_count: todo!(), // Will add later
    },
}

impl NodeConfig for OutputConfig {
    fn kind(&self) -> NodeKind {
        NodeKind::Output
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_config_kind() {
        let config = OutputConfig::GpioStrip { pin: 18 };
        assert_eq!(config.kind(), NodeKind::Output);
    }
}

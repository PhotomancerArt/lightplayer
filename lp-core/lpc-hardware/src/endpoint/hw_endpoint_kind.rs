use serde::{Deserialize, Serialize};

/// Capability family for an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HwEndpointKind {
    Ws281x,
    Button,
    Radio,
}

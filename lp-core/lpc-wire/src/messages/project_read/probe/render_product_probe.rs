//! Render-product probe.

use crate::project::WireTextureFormat;
use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::{Revision, VisualProduct};

/// Request to materialize a visual product into inspection bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct RenderProductProbeRequest {
    pub product: VisualProduct,
    pub width: u32,
    pub height: u32,
    pub format: WireTextureFormat,
}

/// Result of a render-product probe.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RenderProductProbeResult {
    Texture {
        product: VisualProduct,
        revision: Revision,
        width: u32,
        height: u32,
        format: WireTextureFormat,
        #[cfg_attr(feature = "schema-gen", schemars(with = "String"))]
        #[serde(with = "crate::serde_base64")]
        bytes: Vec<u8>,
    },
    Unsupported {
        product: VisualProduct,
        reason: String,
    },
    Error {
        product: VisualProduct,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::NodeId;

    #[test]
    fn render_product_probe_unsupported_and_error_carry_product() {
        let product = VisualProduct::new(NodeId::new(3), 1);

        let unsupported = RenderProductProbeResult::Unsupported {
            product,
            reason: String::from("no renderer"),
        };
        let json = serde_json::to_string(&unsupported).unwrap();
        let back: RenderProductProbeResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back, unsupported);

        let error = RenderProductProbeResult::Error {
            product,
            message: String::from("boom"),
        };
        let json = serde_json::to_string(&error).unwrap();
        let back: RenderProductProbeResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back, error);
    }
}

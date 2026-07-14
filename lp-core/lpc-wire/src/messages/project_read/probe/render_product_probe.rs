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
    /// The product rendered successfully but stayed GPU-resident: the
    /// producing runtime runs the GPU tier, where texture readback is
    /// unavailable (fidelity-tiers ADR). Byte-needing consumers must probe a
    /// CPU-tier runtime.
    GpuResident {
        product: VisualProduct,
        revision: Revision,
        width: u32,
        height: u32,
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

/// A [`RenderProductProbeResult::Texture`] with its bulk `bytes` removed.
///
/// Produced by [`RenderProductProbeResult::into_chunked_parts`] when a texture
/// result is streamed as bounded chunks; recombine with
/// [`RenderProductProbeResultHeader::into_result`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct RenderProductProbeResultHeader {
    pub product: VisualProduct,
    pub revision: Revision,
    pub width: u32,
    pub height: u32,
    pub format: WireTextureFormat,
}

impl RenderProductProbeResult {
    /// Split a [`Texture`](Self::Texture) result into its header and bulk bytes.
    ///
    /// Non-`Texture` variants carry no bulk payload and return `Err(self)`.
    pub fn into_chunked_parts(self) -> Result<(RenderProductProbeResultHeader, Vec<u8>), Self> {
        match self {
            Self::Texture {
                product,
                revision,
                width,
                height,
                format,
                bytes,
            } => Ok((
                RenderProductProbeResultHeader {
                    product,
                    revision,
                    width,
                    height,
                    format,
                },
                bytes,
            )),
            other @ (Self::GpuResident { .. } | Self::Unsupported { .. } | Self::Error { .. }) => {
                Err(other)
            }
        }
    }
}

impl RenderProductProbeResultHeader {
    /// Reattach reassembled `bytes` to recover the full texture result.
    #[must_use]
    pub fn into_result(self, bytes: Vec<u8>) -> RenderProductProbeResult {
        RenderProductProbeResult::Texture {
            product: self.product,
            revision: self.revision,
            width: self.width,
            height: self.height,
            format: self.format,
            bytes,
        }
    }
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

    #[test]
    fn texture_splits_into_header_and_bytes_and_recombines() {
        let product = VisualProduct::new(NodeId::new(7), 2);
        let original = RenderProductProbeResult::Texture {
            product,
            revision: Revision::new(4),
            width: 2,
            height: 1,
            format: WireTextureFormat::Rgba16,
            bytes: alloc::vec![1, 2, 3, 4, 5, 6],
        };

        let (header, bytes) = original.clone().into_chunked_parts().expect("splittable");
        assert_eq!(bytes, alloc::vec![1u8, 2, 3, 4, 5, 6]);
        // The header carries every non-bulk field and round-trips over the wire.
        let json = serde_json::to_string(&header).unwrap();
        let back: RenderProductProbeResultHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(back, header);

        assert_eq!(header.into_result(bytes), original);
    }

    #[test]
    fn non_texture_variants_do_not_split() {
        let product = VisualProduct::new(NodeId::new(1), 0);
        assert!(
            RenderProductProbeResult::Unsupported {
                product,
                reason: String::from("no renderer"),
            }
            .into_chunked_parts()
            .is_err()
        );
    }
}

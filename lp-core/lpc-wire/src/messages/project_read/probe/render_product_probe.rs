//! Render-product probe.

use crate::project::WireTextureFormat;
use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::{Revision, VisualProduct};

/// Wire mirror of `lpc_engine::products::visual::VisualSpace` — which
/// coordinate space a probe requests in, or a result renders in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireVisualSpace {
    OneD,
    TwoD,
}

/// Wire mirror of `lpc_engine::products::visual::CellProjection` — one cell
/// of the 1D→2D projection matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireCellProjection {
    Extrude,
    Radial,
    Angular,
    Mirror,
}

/// Wire mirror of `lpc_engine::products::visual::ProjectionOrigin` — which
/// precedence arm decided a resolved [`WireCellProjection`] (plan D15
/// captions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireProjectionOrigin {
    Declared,
    ConsumerDefault,
    Forced,
}

/// Wire mirror of `lpc_engine::products::visual::ConsumerPolicy` — the
/// requester's 1D→2D projection preference, and whether it overrides an
/// authored producer opinion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireConsumerPolicy {
    pub default_1d_to_2d: WireCellProjection,
    pub force: bool,
}

impl WireConsumerPolicy {
    /// The defaults-only policy (`Extrude`, never force) — what a caller
    /// that has never heard of spaces effectively sends.
    pub const AUTO: Self = Self {
        default_1d_to_2d: WireCellProjection::Extrude,
        force: false,
    };
}

/// Request to materialize a visual product into inspection bytes.
///
/// `space` and `policy` are the space-tagged half (plan D18, plan-B P2).
/// Both are optional and absent means today's behavior — `TwoD`, no forced
/// policy — so a caller that has never heard of spaces gets exactly what it
/// got before this pair existed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct RenderProductProbeRequest {
    pub product: VisualProduct,
    pub width: u32,
    pub height: u32,
    pub format: WireTextureFormat,
    /// Space to render in. `None` = `TwoD` (today's behavior).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<WireVisualSpace>,
    /// Projection policy honored when the producer's space disagrees.
    /// `None` = [`WireConsumerPolicy::AUTO`] (today's behavior).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<WireConsumerPolicy>,
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
        /// The space this frame actually rendered in (the effective
        /// request space).
        space: WireVisualSpace,
        /// The 1D→2D cell applied to fill this frame, when one applied
        /// (only possible when `space` is `TwoD` and the producer's
        /// `primary` is `OneD`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        projection: Option<WireCellProjection>,
        /// Why `projection` was chosen. Always present exactly when
        /// `projection` is.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<WireProjectionOrigin>,
        /// The producer's own native space, independent of what was
        /// requested — what a preview's checkbox-availability logic needs.
        primary: WireVisualSpace,
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
        space: WireVisualSpace,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        projection: Option<WireCellProjection>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<WireProjectionOrigin>,
        primary: WireVisualSpace,
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
    pub space: WireVisualSpace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<WireCellProjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<WireProjectionOrigin>,
    pub primary: WireVisualSpace,
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
                space,
                projection,
                origin,
                primary,
            } => Ok((
                RenderProductProbeResultHeader {
                    product,
                    revision,
                    width,
                    height,
                    format,
                    space,
                    projection,
                    origin,
                    primary,
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
            space: self.space,
            projection: self.projection,
            origin: self.origin,
            primary: self.primary,
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
            space: WireVisualSpace::TwoD,
            projection: Some(WireCellProjection::Radial),
            origin: Some(WireProjectionOrigin::Declared),
            primary: WireVisualSpace::OneD,
        };

        let (header, bytes) = original.clone().into_chunked_parts().expect("splittable");
        assert_eq!(bytes, alloc::vec![1u8, 2, 3, 4, 5, 6]);
        // The header carries every non-bulk field and round-trips over the wire.
        let json = serde_json::to_string(&header).unwrap();
        let back: RenderProductProbeResultHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(back, header);

        assert_eq!(header.into_result(bytes), original);
    }

    /// Absent request fields serialize as today's shape (no `space`/`policy`
    /// keys at all) — the "no behavior change for absent fields" contract.
    #[test]
    fn request_with_no_space_or_policy_omits_both_keys() {
        let request = RenderProductProbeRequest {
            product: VisualProduct::new(NodeId::new(1), 0),
            width: 4,
            height: 4,
            format: WireTextureFormat::Srgb8,
            space: None,
            policy: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("space"), "json: {json}");
        assert!(!json.contains("policy"), "json: {json}");
        let back: RenderProductProbeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, request);
    }

    /// An explicit 1D request with a forcing policy round-trips.
    #[test]
    fn request_with_explicit_space_and_policy_round_trips() {
        let request = RenderProductProbeRequest {
            product: VisualProduct::new(NodeId::new(2), 1),
            width: 8,
            height: 1,
            format: WireTextureFormat::Rgba16,
            space: Some(WireVisualSpace::OneD),
            policy: Some(WireConsumerPolicy {
                default_1d_to_2d: WireCellProjection::Mirror,
                force: true,
            }),
        };
        let json = serde_json::to_string(&request).unwrap();
        let back: RenderProductProbeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, request);
    }

    /// A `GpuResident` result carries the same space/projection/origin/
    /// primary metadata as `Texture` — no byte-shaped payload, but the same
    /// introspection.
    #[test]
    fn gpu_resident_result_round_trips_with_space_metadata() {
        let result = RenderProductProbeResult::GpuResident {
            product: VisualProduct::new(NodeId::new(9), 0),
            revision: Revision::new(2),
            width: 32,
            height: 32,
            space: WireVisualSpace::TwoD,
            projection: None,
            origin: None,
            primary: WireVisualSpace::TwoD,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: RenderProductProbeResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back, result);
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

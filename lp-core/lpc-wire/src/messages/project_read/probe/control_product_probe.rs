//! Control-product preview probe.
//!
//! The probe returns native control samples plus metadata that lets clients
//! inspect those samples and optionally render a human-facing display layout.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{
    ControlDisplayLayout, ControlExtent, ControlProduct, ControlSampleLayout, Revision,
};

use crate::project::WireChannelSampleFormat;

/// Request to materialize a control product for inspection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ControlProductProbeRequest {
    pub product: ControlProduct,
    pub sample_format: WireChannelSampleFormat,
    pub display_layout: ControlDisplayLayoutRead,
}

/// Whether and how a control-product probe should include display layout data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ControlDisplayLayoutRead {
    None,
    Always,
    IfChanged { known_revision: Option<Revision> },
}

/// Display layout payload attached to a control-product probe response.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ControlDisplayLayoutProbeResult {
    Omitted,
    Unchanged { revision: Revision },
    Layout(ControlDisplayLayout),
    Unsupported { reason: String },
}

/// Result of a control-product preview probe.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ControlProductProbeResult {
    Preview {
        product: ControlProduct,
        revision: Revision,
        extent: ControlExtent,
        sample_format: WireChannelSampleFormat,
        sample_layout: ControlSampleLayout,
        display_layout: ControlDisplayLayoutProbeResult,
        #[cfg_attr(feature = "schema-gen", schemars(with = "String"))]
        #[serde(with = "crate::serde_base64")]
        bytes: Vec<u8>,
    },
    Unsupported {
        product: ControlProduct,
        reason: String,
    },
    Error {
        product: ControlProduct,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{ColorOrder, ControlSampleEncoding, ControlSampleSpan, NodeId};

    #[test]
    fn control_product_probe_round_trips_native_samples() {
        let product = ControlProduct::new(NodeId::new(4), 0, ControlExtent::new(1, 3));
        let result = ControlProductProbeResult::Preview {
            product,
            revision: Revision::new(7),
            extent: ControlExtent::new(1, 3),
            sample_format: WireChannelSampleFormat::U16,
            sample_layout: ControlSampleLayout {
                spans: Vec::from([ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len: 3,
                    encoding: ControlSampleEncoding::RgbPixels {
                        count: 1,
                        color_order: ColorOrder::Rgb,
                    },
                }]),
            },
            display_layout: ControlDisplayLayoutProbeResult::Omitted,
            bytes: Vec::from([0, 0, 255, 255, 128, 0]),
        };

        let json = serde_json::to_string(&result).unwrap();
        let round_trip: ControlProductProbeResult = serde_json::from_str(&json).unwrap();

        assert_eq!(round_trip, result);
    }
}

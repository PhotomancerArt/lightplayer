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

/// A [`ControlProductProbeResult::Preview`] with its bulk `bytes` removed.
///
/// Produced by [`ControlProductProbeResult::into_chunked_parts`] when a preview
/// result is streamed as bounded chunks. The structured header — extent, sample
/// layout, and (per the plan) the `display_layout` — rides in
/// `ProjectReadProbeEvent::ResultBegin`; only the native `bytes` chunk. Recombine
/// with [`ControlProductProbeResultHeader::into_result`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ControlProductProbeResultHeader {
    pub product: ControlProduct,
    pub revision: Revision,
    pub extent: ControlExtent,
    pub sample_format: WireChannelSampleFormat,
    pub sample_layout: ControlSampleLayout,
    pub display_layout: ControlDisplayLayoutProbeResult,
}

impl ControlProductProbeResult {
    /// Split a [`Preview`](Self::Preview) result into its header and bulk bytes.
    ///
    /// Non-`Preview` variants carry no bulk payload and return `Err(self)`.
    pub fn into_chunked_parts(self) -> Result<(ControlProductProbeResultHeader, Vec<u8>), Self> {
        match self {
            Self::Preview {
                product,
                revision,
                extent,
                sample_format,
                sample_layout,
                display_layout,
                bytes,
            } => Ok((
                ControlProductProbeResultHeader {
                    product,
                    revision,
                    extent,
                    sample_format,
                    sample_layout,
                    display_layout,
                },
                bytes,
            )),
            other @ (Self::Unsupported { .. } | Self::Error { .. }) => Err(other),
        }
    }
}

impl ControlProductProbeResultHeader {
    /// Reattach reassembled `bytes` to recover the full preview result.
    #[must_use]
    pub fn into_result(self, bytes: Vec<u8>) -> ControlProductProbeResult {
        ControlProductProbeResult::Preview {
            product: self.product,
            revision: self.revision,
            extent: self.extent,
            sample_format: self.sample_format,
            sample_layout: self.sample_layout,
            display_layout: self.display_layout,
            bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use lpc_model::{
        ColorOrder, ControlDisplayLayout, ControlLamp2d, ControlLayout2d, ControlSampleEncoding,
        ControlSampleSpan, NodeId,
    };

    use crate::{
        PROJECT_READ_FRAME_MAX_BYTES, ProjectReadEvent, ProjectReadProbeEvent,
        server::ServerMsgBody,
    };

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

    #[test]
    fn fixture_sized_control_preview_fits_project_read_frame_budget() {
        let product = ControlProduct::new(NodeId::new(2), 0, ControlExtent::new(1, 723));
        let result = ControlProductProbeResult::Preview {
            product,
            revision: Revision::new(18),
            extent: ControlExtent::new(1, 723),
            sample_format: WireChannelSampleFormat::U16,
            sample_layout: ControlSampleLayout {
                spans: Vec::from([ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len: 723,
                    encoding: ControlSampleEncoding::RgbPixels {
                        count: 241,
                        color_order: ColorOrder::Rgb,
                    },
                }]),
            },
            display_layout: ControlDisplayLayoutProbeResult::Layout(
                ControlDisplayLayout::Layout2d(ControlLayout2d::new(
                    Revision::new(18),
                    10,
                    10,
                    (0..241)
                        .map(|index| ControlLamp2d {
                            lamp_index: index,
                            sample_start: index * 3,
                            center: [(index % 17) as f32 / 16.0, (index / 17) as f32 / 15.0],
                            radius: 0.02,
                        })
                        .collect(),
                )),
            ),
            bytes: vec![0; 723 * 2],
        };
        let events = Vec::from([ProjectReadEvent::Probe {
            index: 0,
            event: ProjectReadProbeEvent::Result(crate::ProjectProbeResult::ControlProduct(result)),
        }]);
        let message = crate::WireServerMessage::stream_frame(
            7,
            0,
            false,
            ServerMsgBody::ProjectRead { events },
        );

        let json = crate::json::to_string(&message).unwrap();

        assert!(
            json.len() <= PROJECT_READ_FRAME_MAX_BYTES,
            "encoded control preview frame was {} bytes, budget is {}",
            json.len(),
            PROJECT_READ_FRAME_MAX_BYTES
        );
    }

    /// Companion to the fixture-budget test above: a control preview whose native
    /// samples dwarf one frame must chunk, and every emitted event — the
    /// `ResultBegin` header and each bounded `ResultBytes` chunk — must still fit
    /// one project-read frame.
    ///
    /// This exercises the design's supported regime: the structured header
    /// (including a fixture-scale 241-lamp layout) stays within budget while the
    /// bulk samples stream as chunks. It deliberately does *not* grow the layout
    /// itself past budget — that unchunked-header growth path is the documented
    /// escalation (notes §7, semantic layout split), out of scope here.
    #[test]
    fn oversized_control_preview_chunks_and_each_event_fits_frame_budget() {
        use crate::{PROJECT_READ_RUNTIME_CHUNK_BYTES, ProjectProbeResult};

        // Native samples several chunks large force multi-chunk streaming, while
        // the layout is held at the 241-lamp fixture scale so the header frame
        // stays comfortably under budget.
        let bulk_len = 5 * PROJECT_READ_RUNTIME_CHUNK_BYTES + 123;
        let product = ControlProduct::new(NodeId::new(2), 0, ControlExtent::new(1, 723));
        let result = ControlProductProbeResult::Preview {
            product,
            revision: Revision::new(18),
            extent: ControlExtent::new(1, 723),
            sample_format: WireChannelSampleFormat::U16,
            sample_layout: ControlSampleLayout {
                spans: Vec::from([ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len: 723,
                    encoding: ControlSampleEncoding::RgbPixels {
                        count: 241,
                        color_order: ColorOrder::Rgb,
                    },
                }]),
            },
            display_layout: ControlDisplayLayoutProbeResult::Layout(
                ControlDisplayLayout::Layout2d(ControlLayout2d::new(
                    Revision::new(18),
                    10,
                    10,
                    (0..241)
                        .map(|index| ControlLamp2d {
                            lamp_index: index,
                            sample_start: index * 3,
                            center: [(index % 17) as f32 / 16.0, (index / 17) as f32 / 15.0],
                            radius: 0.02,
                        })
                        .collect(),
                )),
            ),
            bytes: vec![0u8; bulk_len],
        };

        // Split as the engine producer does, then chunk the bulk bytes.
        let (header, bytes) = ProjectProbeResult::ControlProduct(result)
            .into_chunked_parts()
            .expect("preview is splittable");
        let byte_length = u32::try_from(bytes.len()).unwrap();

        let mut chunk_events = Vec::new();
        chunk_events.push(ProjectReadProbeEvent::ResultBegin {
            byte_length,
            header,
        });
        for (chunk_index, chunk) in bytes.chunks(PROJECT_READ_RUNTIME_CHUNK_BYTES).enumerate() {
            chunk_events.push(ProjectReadProbeEvent::ResultBytes {
                offset: u32::try_from(chunk_index * PROJECT_READ_RUNTIME_CHUNK_BYTES).unwrap(),
                bytes: chunk.to_vec(),
            });
        }
        chunk_events.push(ProjectReadProbeEvent::ResultEnd);

        assert!(
            chunk_events
                .iter()
                .filter(|e| matches!(e, ProjectReadProbeEvent::ResultBytes { .. }))
                .count()
                > 1,
            "oversized preview must produce multiple chunk events"
        );

        // Each event, wrapped in a real project-read frame, must fit the budget —
        // including the header frame carrying the full 6000-lamp layout.
        for (seq, event) in chunk_events.into_iter().enumerate() {
            let events = Vec::from([ProjectReadEvent::Probe { index: 0, event }]);
            let message = crate::WireServerMessage::stream_frame(
                7,
                seq as u32,
                false,
                ServerMsgBody::ProjectRead { events },
            );
            let json = crate::json::to_string(&message).unwrap();
            assert!(
                json.len() <= PROJECT_READ_FRAME_MAX_BYTES,
                "chunked probe frame (seq {seq}) was {} bytes, budget is {}",
                json.len(),
                PROJECT_READ_FRAME_MAX_BYTES
            );
        }
    }
}

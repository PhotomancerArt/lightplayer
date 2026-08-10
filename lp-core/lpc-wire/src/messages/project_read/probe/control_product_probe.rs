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

    /// A1 — the declared embedded ceiling. A 2048-lamp layout (packed wire
    /// form: spans + base64 u16 centers) must ride ONE project-read frame,
    /// with irregular geometry (LCG-scattered centers — grids under-measure
    /// because their quantized centers are periodic; scattered centers are
    /// max entropy, the honest worst case for this encoding).
    ///
    /// Context for the number: the old per-lamp tuple form ran ~75 bytes a
    /// lamp — 2048 lamps was ~150 KiB and was refused as `Unsupported` at
    /// dome scale. Packed it is ~5.4 bytes a lamp. Above 2048 on a serial
    /// link the engine still answers `Unsupported`, and that is the
    /// declared product posture, not a defect.
    #[test]
    fn a_2048_lamp_layout_fits_the_serial_frame_budget() {
        const LAMPS: u32 = 2048;
        // 8 spans of 256 — a multi-strand install's worth of span overhead.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut scatter = move || {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (state >> 40) as f32 / 16_777_216.0
        };
        let lamps: Vec<ControlLamp2d> = (0..LAMPS)
            .map(|index| ControlLamp2d {
                lamp_index: index,
                sample_start: index * 3,
                center: [scatter(), scatter()],
                radius: 0.004,
            })
            .collect();
        let layout = ControlLayout2d::new(Revision::new(1), 256, 256, lamps).with_paths(
            (0..8)
                .map(|k| lpc_model::ControlPathSpan2d {
                    first_lamp: k * 256,
                    lamp_count: 256,
                })
                .collect(),
        );

        let result = ControlProductProbeResult::Preview {
            product: ControlProduct::new(NodeId::new(2), 0, ControlExtent::new(1, LAMPS * 3)),
            revision: Revision::new(1),
            extent: ControlExtent::new(1, LAMPS * 3),
            sample_format: WireChannelSampleFormat::U16,
            sample_layout: ControlSampleLayout { spans: Vec::new() },
            display_layout: ControlDisplayLayoutProbeResult::Layout(
                ControlDisplayLayout::Layout2d(layout),
            ),
            bytes: Vec::new(),
        };
        let (header, _) = crate::ProjectProbeResult::ControlProduct(result)
            .into_chunked_parts()
            .expect("preview is splittable");
        let events = Vec::from([ProjectReadEvent::Probe {
            index: 0,
            event: ProjectReadProbeEvent::ResultBegin {
                byte_length: 0,
                header,
            },
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
            "a 2048-lamp packed layout frame was {} bytes, budget is {}",
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

//! Published output-frame probe: read the frame the device ALREADY rendered.
//!
//! # Why this exists next to the control-product probe
//!
//! [`ControlProductProbeRequest`](super::ControlProductProbeRequest) answers
//! "what would this product look like if I rendered it now?" — it materializes
//! a fresh control product inside the request path. That is the right answer
//! for a lens preview pane, and the wrong one for a live device feed: a card
//! pulling at 5–10 fps would make the board render every frame twice, and the
//! second render is not even the frame the LEDs are showing.
//!
//! This probe answers the other question — "what did you last push?" — and
//! answers it **without rendering anything**. Each entry is the output node's
//! published runtime buffer, verbatim, plus the metadata a client needs to
//! interpret it:
//!
//! - `sample_layout` — how the native samples group into lamps. Latched by the
//!   tick that produced the buffer, not recomputed here.
//! - `display_layout` — where to draw those lamps, revision-gated exactly like
//!   the control probe's (`Omitted` / `Unchanged` / `Layout` / `Unsupported`),
//!   so a steady feed ships geometry once and samples thereafter.
//!
//! # Samples are post-finalize, deliberately
//!
//! The bytes are what the output node published: gamma, brightness, power
//! limiting and color order already applied. That is the honest view — it is
//! what the wire actually carries to the strip — and gamma is lossy to invert,
//! so there is no un-corrected frame to recover. Do not "fix" this.
//!
//! # Bandwidth
//!
//! One frame is `channels × 3 × 2` bytes before base64, on a link shared with
//! every other protocol message; a 1500-lamp dome frame is ~9 KB, a 300-lamp
//! strip ~1.8 KB. Bulk bytes therefore ride the same chunked path as the other
//! bulk-bearing probe results ([`OutputFrameProbeResult::into_chunked_parts`]).
//! The device-side rationale for why a per-frame pixel transcript has to be
//! rationed on a shared serial link is written up in
//! `lp-fw/fw-esp32s3/src/output/rmt/frame_dump.rs` (§Volume); the same
//! arithmetic sets the cadence a client should pull at.

use alloc::vec::Vec;

use lpc_model::{ControlSampleLayout, NodeId, Revision};

use crate::project::WireChannelSampleFormat;

use super::{ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead};

/// Request the frames every output node has already published.
///
/// There is no product selector: the interesting set is "every output this
/// device is driving", and enumerating them costs a tree walk over nodes that
/// own a sink buffer. Clients that care about one output filter the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct OutputFrameProbeRequest {
    /// Geometry gate — the same idiom the control-product probe uses, so a
    /// feed can ask for the layout once and then say "only if it changed".
    pub display_layout: ControlDisplayLayoutRead,
}

/// Result of a published output-frame probe.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OutputFrameProbeResult {
    Frame {
        /// One entry per output node with a published channel buffer, in tree
        /// order. Empty when the project drives no outputs — an honest answer,
        /// never an error.
        outputs: Vec<OutputFrameEntry>,
    },
}

/// One output node's published frame.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct OutputFrameEntry {
    /// The output node that owns the published buffer.
    pub node: NodeId,
    /// The buffer's `changed_at` revision — it moves on every tick that
    /// publishes, so a client detects "new frame" by comparing this alone.
    pub revision: Revision,
    /// Channel count as the published buffer's own `OutputChannels` metadata
    /// reports it: RGB **lamps**, not raw samples (`bytes.len() == channels *
    /// 3 * 2` for a `U16` RGB frame). Named to match
    /// `WireResourceMetadataSummary::OutputChannels`, which is the same
    /// number about the same buffer.
    pub channels: u32,
    /// Element format of `bytes`. `U16` today (little-endian).
    pub sample_format: WireChannelSampleFormat,
    /// How the native samples group — latched by the publishing tick.
    pub sample_layout: ControlSampleLayout,
    /// Where to draw the lamps, gated by the request's
    /// [`ControlDisplayLayoutRead`].
    pub display_layout: ControlDisplayLayoutProbeResult,
    /// How the frame was CUT: one entry per producer run placed on this
    /// wire, in the output's own planning order.
    ///
    /// Ungated (unlike `display_layout`): the whole set is a handful of
    /// six-field rows even at dome scale, and it is the only description of
    /// which fixture owns which stretch of the strand. Empty when the output
    /// has not planned a placement yet — never a signal that the wire is
    /// unpatched.
    pub placements: Vec<WireOutputPlacement>,
    /// The published buffer, verbatim.
    #[cfg_attr(feature = "schema-gen", schemars(with = "String"))]
    #[serde(with = "crate::serde_base64")]
    pub bytes: Vec<u8>,
}

/// One producer run placed on an output's wire — the `(fixture span ↔ wire
/// span)` tuple, **in lamps**.
///
/// The engine plans placements in samples (three per RGB lamp); the wire
/// states them in lamps because every surface that reads them — the patch
/// document, the output's channel counts, the bay — counts lamps. A producer
/// with no patch contributes exactly one run covering its whole product; a
/// patched one contributes one per authored anchor, and the ones it did not
/// author still auto-flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireOutputPlacement {
    /// The node whose control product this run came from — the join back to
    /// the producing card.
    pub node: NodeId,
    /// Which of that node's control outputs.
    pub output: u32,
    /// First lamp OF THE PRODUCER's own numbering this run takes.
    pub source_lamp: u32,
    /// Lamps the producer's whole product carries, so a reader knows how
    /// much of it this run is without owning the product.
    pub source_lamps: u32,
    /// First lamp of the run on the output's wire.
    pub wire_lamp: u32,
    /// Lamps in the run.
    pub lamps: u32,
    /// The run is laid onto the wire end-first (a strand plugged in at the
    /// far end). Distinct from a fixture's own `wire_reversed` sampling.
    pub reversed: bool,
}

/// An [`OutputFrameProbeResult`] with every entry's bulk `bytes` removed.
///
/// The bulk half is the concatenation of the entries' bytes in entry order;
/// each header records its own `byte_length` so
/// [`OutputFrameProbeResultHeader::into_result`] can split the reassembled
/// blob back apart. Unlike the single-payload probes, this result can carry
/// several buffers, and concatenating them keeps one chunk stream per probe
/// rather than inventing a per-entry framing.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct OutputFrameProbeResultHeader {
    pub outputs: Vec<OutputFrameEntryHeader>,
}

/// One [`OutputFrameEntry`] with its bulk `bytes` replaced by their length.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct OutputFrameEntryHeader {
    pub node: NodeId,
    pub revision: Revision,
    pub channels: u32,
    pub sample_format: WireChannelSampleFormat,
    pub sample_layout: ControlSampleLayout,
    pub display_layout: ControlDisplayLayoutProbeResult,
    /// The wire's cut — small enough to ride the header with the rest of the
    /// interpretation metadata.
    pub placements: Vec<WireOutputPlacement>,
    /// Bytes this entry claims out of the concatenated bulk payload.
    pub byte_length: u32,
}

impl OutputFrameProbeResult {
    /// Split the result into its header and the concatenated bulk bytes.
    ///
    /// Always succeeds — every output frame carries bulk samples — so unlike
    /// the sibling probes there is no `Err` arm handing the result back.
    #[must_use]
    pub fn into_chunked_parts(self) -> (OutputFrameProbeResultHeader, Vec<u8>) {
        let Self::Frame { outputs } = self;
        let total = outputs.iter().map(|entry| entry.bytes.len()).sum();
        let mut bytes = Vec::with_capacity(total);
        let mut headers = Vec::with_capacity(outputs.len());
        for entry in outputs {
            let byte_length = entry.bytes.len() as u32;
            bytes.extend_from_slice(&entry.bytes);
            headers.push(OutputFrameEntryHeader {
                node: entry.node,
                revision: entry.revision,
                channels: entry.channels,
                sample_format: entry.sample_format,
                sample_layout: entry.sample_layout,
                display_layout: entry.display_layout,
                placements: entry.placements,
                byte_length,
            });
        }
        (OutputFrameProbeResultHeader { outputs: headers }, bytes)
    }
}

impl OutputFrameProbeResultHeader {
    /// Reattach the reassembled bulk `bytes`, splitting them back across the
    /// entries by their recorded lengths.
    ///
    /// A short payload (a truncated stream) yields short trailing entries
    /// rather than a panic: the transport already validates chunk coverage,
    /// and a decoder is not the place to abort a whole project read.
    #[must_use]
    pub fn into_result(self, bytes: Vec<u8>) -> OutputFrameProbeResult {
        let mut offset = 0usize;
        let outputs = self
            .outputs
            .into_iter()
            .map(|header| {
                let start = offset.min(bytes.len());
                let end = start
                    .saturating_add(header.byte_length as usize)
                    .min(bytes.len());
                offset = end;
                OutputFrameEntry {
                    node: header.node,
                    revision: header.revision,
                    channels: header.channels,
                    sample_format: header.sample_format,
                    sample_layout: header.sample_layout,
                    display_layout: header.display_layout,
                    placements: header.placements,
                    bytes: bytes[start..end].to_vec(),
                }
            })
            .collect();
        OutputFrameProbeResult::Frame { outputs }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use lpc_model::{ColorOrder, ControlSampleEncoding, ControlSampleSpan};

    #[test]
    fn output_frame_result_round_trips_base64_samples() {
        let result = OutputFrameProbeResult::Frame {
            outputs: vec![entry(NodeId::new(4), 1, vec![0, 0, 255, 255, 128, 0])],
        };

        let json = serde_json::to_string(&result).unwrap();
        let back: OutputFrameProbeResult = serde_json::from_str(&json).unwrap();

        assert_eq!(back, result);
    }

    /// The chunked split concatenates in entry order and the header's recorded
    /// lengths put every entry's bytes back where they came from — the
    /// property the multi-output stream depends on.
    #[test]
    fn multi_output_chunked_parts_round_trip_per_entry_bytes() {
        let result = OutputFrameProbeResult::Frame {
            outputs: vec![
                entry(NodeId::new(2), 7, vec![1, 2, 3, 4]),
                entry(NodeId::new(3), 7, vec![9, 9]),
                entry(NodeId::new(5), 7, Vec::new()),
            ],
        };

        let (header, bytes) = result.clone().into_chunked_parts();
        assert_eq!(bytes, vec![1, 2, 3, 4, 9, 9]);
        assert_eq!(
            header
                .outputs
                .iter()
                .map(|entry| entry.byte_length)
                .collect::<Vec<_>>(),
            vec![4, 2, 0]
        );

        assert_eq!(header.into_result(bytes), result);
    }

    /// A fixture-scale frame chunked the way the engine's producer chunks it:
    /// every emitted event — the header carrying the 241-lamp layout, and each
    /// bounded chunk of samples — has to fit one project-read frame, or the
    /// oversized event wedges the whole read stream.
    #[test]
    fn chunked_output_frame_events_each_fit_the_project_read_frame_budget() {
        use crate::{
            PROJECT_READ_FRAME_MAX_BYTES, PROJECT_READ_RUNTIME_CHUNK_BYTES, ProjectProbeResult,
            ProjectReadEvent, ProjectReadProbeEvent, server::ServerMsgBody,
        };
        use lpc_model::{ControlDisplayLayout, ControlLamp2d, ControlLayout2d};

        let mut frame = entry(
            NodeId::new(2),
            18,
            vec![0u8; 3 * PROJECT_READ_RUNTIME_CHUNK_BYTES + 77],
        );
        frame.display_layout = ControlDisplayLayoutProbeResult::Layout(
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
        );

        let (header, bytes) = ProjectProbeResult::OutputFrame(OutputFrameProbeResult::Frame {
            outputs: vec![frame],
        })
        .into_chunked_parts()
        .expect("output frames always split");

        let mut events = vec![ProjectReadProbeEvent::ResultBegin {
            byte_length: bytes.len() as u32,
            header,
        }];
        for (index, chunk) in bytes.chunks(PROJECT_READ_RUNTIME_CHUNK_BYTES).enumerate() {
            events.push(ProjectReadProbeEvent::ResultBytes {
                offset: (index * PROJECT_READ_RUNTIME_CHUNK_BYTES) as u32,
                bytes: chunk.to_vec(),
            });
        }
        events.push(ProjectReadProbeEvent::ResultEnd);
        assert!(
            events
                .iter()
                .filter(|event| matches!(event, ProjectReadProbeEvent::ResultBytes { .. }))
                .count()
                > 1,
            "this payload must span multiple chunks to exercise the split"
        );

        for (seq, event) in events.into_iter().enumerate() {
            let message = crate::WireServerMessage::stream_frame(
                7,
                seq as u32,
                false,
                ServerMsgBody::ProjectRead {
                    events: vec![ProjectReadEvent::Probe { index: 0, event }],
                },
            );
            let json = crate::json::to_string(&message).unwrap();
            assert!(
                json.len() <= PROJECT_READ_FRAME_MAX_BYTES,
                "chunked output-frame event (seq {seq}) was {} bytes, budget is {}",
                json.len(),
                PROJECT_READ_FRAME_MAX_BYTES
            );
        }
    }

    fn entry(node: NodeId, revision: i64, bytes: Vec<u8>) -> OutputFrameEntry {
        OutputFrameEntry {
            node,
            revision: Revision::new(revision),
            channels: (bytes.len() / 6) as u32,
            sample_format: WireChannelSampleFormat::U16,
            sample_layout: ControlSampleLayout {
                spans: vec![ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len: (bytes.len() / 2) as u32,
                    encoding: ControlSampleEncoding::RgbPixels {
                        count: (bytes.len() / 6) as u32,
                        color_order: ColorOrder::Rgb,
                    },
                }],
            },
            display_layout: ControlDisplayLayoutProbeResult::Omitted,
            placements: vec![WireOutputPlacement {
                node: NodeId::new(9),
                output: 0,
                source_lamp: 0,
                source_lamps: (bytes.len() / 6) as u32,
                wire_lamp: 0,
                lamps: (bytes.len() / 6) as u32,
                reversed: false,
            }],
            bytes,
        }
    }
}

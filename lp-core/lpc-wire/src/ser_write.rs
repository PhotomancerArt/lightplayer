//! Byte-counting `SerWrite` sink over the wire serializer.
//!
//! ESP32 writes outbound messages with the vendored `ser-write-json` crate
//! (`ryu-js` float formatting), not `serde_json` (`ryu`). Those two serializers
//! demonstrably diverge on float rendering (e.g. `1.2345679e20` vs
//! `123456790000000000000`, `1.0` vs `1`, `3.4e38` vs `3.4e+38`), so measuring a
//! frame's on-wire size with `serde_json` under-counts against the real ESP32
//! byte stream.
//!
//! [`CountingSerWrite`] is a `no_std` [`SerWrite`] implementation that emits
//! nothing and only accumulates a byte count. Feeding a value to
//! `ser_write_json::ser::to_writer` with this sink yields the exact number of
//! bytes the firmware would put on the wire, and it runs on the host too (behind
//! the `ser-write-json` feature) so the shared frame batcher can budget against
//! the same serializer that actually writes the bytes.

use ser_write_json::SerWrite;
use ser_write_json::ser::to_writer;
use serde::Serialize;

/// A [`SerWrite`] sink that discards output and counts bytes.
///
/// Writing never fails, so [`SerWrite::Error`] is [`core::convert::Infallible`].
#[derive(Debug, Default, Clone, Copy)]
pub struct CountingSerWrite {
    len: usize,
}

impl CountingSerWrite {
    /// Create a fresh counter at zero bytes.
    #[must_use]
    pub const fn new() -> Self {
        Self { len: 0 }
    }

    /// Number of bytes written to this sink so far.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing has been written yet.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl SerWrite for CountingSerWrite {
    type Error = core::convert::Infallible;

    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        // Saturate rather than overflow: a frame that large is already rejected
        // by the budget check, and we never want the measurement itself to panic.
        self.len = self.len.saturating_add(buf.len());
        Ok(())
    }
}

/// Measure the encoded length of `value` using the wire serializer
/// (`ser-write-json`), without allocating the serialized bytes.
///
/// This is the byte count the ESP32 firmware would write for `value`. Use it
/// wherever a frame's on-wire size must be budgeted so the measurement matches
/// the serializer that actually writes the bytes.
///
/// Serialization of a well-formed wire value into a counting sink cannot fail
/// (the sink is infallible and these types serialize without I/O), so this
/// returns `usize` rather than a `Result`.
#[must_use]
pub fn ser_write_json_len<T: Serialize>(value: &T) -> usize {
    let mut counter = CountingSerWrite::new();
    // The only error channel is the sink (infallible); ser-write-json does not
    // otherwise fail for these wire types. If a future type does fail to
    // serialize, treat it as "does not fit" by reporting usize::MAX so the
    // budget check rejects it instead of silently under-counting.
    match to_writer(&mut counter, value) {
        Ok(()) => counter.len(),
        Err(_) => usize::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::ServerMsgBody;
    use crate::{ServerMessage, messages::ProjectReadFrame};
    use alloc::string::ToString;
    use alloc::vec;
    use lpc_model::Revision;
    use ser_write_json::ser::to_writer;

    #[test]
    fn counts_bytes_without_emitting() {
        let mut counter = CountingSerWrite::new();
        assert!(counter.is_empty());
        counter.write(b"hello").unwrap();
        counter.write(b" world").unwrap();
        assert_eq!(counter.len(), 11);
        assert!(!counter.is_empty());
    }

    #[test]
    fn ser_write_json_len_matches_serialized_bytes() {
        let msg = ServerMessage {
            id: 42,
            msg: ServerMsgBody::ProjectReadFrame {
                frame: ProjectReadFrame::new(
                    3,
                    vec![crate::ProjectReadEvent::Begin {
                        revision: Revision::new(7),
                    }],
                ),
            },
        };

        // Serialize into an actual buffer with the same serializer and compare.
        struct VecWriter<'a>(&'a mut alloc::vec::Vec<u8>);
        impl SerWrite for VecWriter<'_> {
            type Error = core::convert::Infallible;
            fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
                self.0.extend_from_slice(buf);
                Ok(())
            }
        }

        let mut bytes = alloc::vec::Vec::new();
        to_writer(&mut VecWriter(&mut bytes), &msg).unwrap();

        assert_eq!(ser_write_json_len(&msg), bytes.len());
        // Sanity: the string form has the same length as the byte form.
        assert_eq!(
            bytes.len(),
            core::str::from_utf8(&bytes).unwrap().to_string().len()
        );
    }
}

/// Cross-serializer regression net.
///
/// `serde_json` (ryu) and the vendored `ser-write-json` (ryu-js) diverge on
/// float formatting. The frame batcher budgets against `ser-write-json`, so this
/// module encodes a representative corpus of project-read events with *both*
/// serializers and asserts:
///
/// 1. [`ser_write_json_len`] equals the real `ser-write-json` byte length.
/// 2. The sink's O(n) size model — `empty_frame_len(seq) + sum(event_len) +
///    (n - 1)` commas — equals the real encoded frame length. This is the exact
///    formula `ProjectReadFrameSink` uses, so it proves per-push measurement
///    predicts the whole-frame size.
/// 3. Documents the `serde_json` delta (bytes it under/over-counts vs the wire
///    serializer) so future float-format drift is caught rather than silently
///    eroding the 256-byte serial margin.
#[cfg(test)]
mod cross_serializer_tests {
    use super::ser_write_json_len;
    use crate::server::ServerMsgBody;
    use crate::slot::{WireSlotData, WireSlotRootSnapshot};
    use crate::{
        ProjectReadEvent, ProjectReadFrame, ProjectReadProbeEvent, ProjectReadQueryEvent,
        ProjectReadResourceEvent, ProjectReadShapeEvent, WireServerMessage,
    };
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;
    use core::convert::Infallible;
    use lpc_model::{
        ColorOrder, ControlDisplayLayout, ControlExtent, ControlLamp2d, ControlLayout2d,
        ControlProduct, ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan, NodeId,
        ResourceRef, Revision, RuntimeBufferId, SlotShape, SlotShapeEntry, SlotShapeId,
    };
    use ser_write_json::SerWrite;
    use ser_write_json::ser::to_writer;

    struct VecWriter<'a>(&'a mut Vec<u8>);
    impl SerWrite for VecWriter<'_> {
        type Error = Infallible;
        fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
            self.0.extend_from_slice(buf);
            Ok(())
        }
    }

    fn ser_write_json_string<T: serde::Serialize>(value: &T) -> String {
        let mut bytes = Vec::new();
        to_writer(&mut VecWriter(&mut bytes), value).expect("ser-write-json serialize");
        core::str::from_utf8(&bytes)
            .expect("ser-write-json output is UTF-8")
            .to_string()
    }

    /// A representative corpus: begin/end markers, a shape registry entry, a slot
    /// root snapshot carrying `RawValue` slot data, a control probe result whose
    /// lamp tuples carry the floats that make the two serializers diverge, and a
    /// runtime-buffer payload chunk.
    fn corpus() -> Vec<ProjectReadEvent> {
        let shape_entry = ProjectReadEvent::Query {
            index: 0,
            event: ProjectReadQueryEvent::Shapes(ProjectReadShapeEvent::Entry {
                id: SlotShapeId::new(42),
                entry: SlotShapeEntry::named(
                    Revision::new(3),
                    "brightness",
                    SlotShape::Ref {
                        id: SlotShapeId::new(7),
                    },
                ),
            }),
        };

        let slot_root = ProjectReadEvent::Query {
            index: 1,
            event: ProjectReadQueryEvent::Nodes(crate::ProjectReadNodeEvent::SlotRoot(
                WireSlotRootSnapshot {
                    name: "root".to_string(),
                    shape: SlotShapeId::new(7),
                    // RawValue slot data with a float that ryu vs ryu-js render
                    // differently: measuring with the wrong serializer misjudges
                    // this frame's size.
                    data: WireSlotData::from_json_string(
                        r#"{"gain":1.0,"scale":3.4e38}"#.to_string(),
                    )
                    .expect("valid raw json"),
                },
            )),
        };

        // Control probe with 2D lamp layout: f32 centers/radius are the classic
        // ryu vs ryu-js divergence.
        let product = ControlProduct::new(NodeId::new(2), 0, ControlExtent::new(1, 30));
        let control_probe = ProjectReadEvent::Probe {
            index: 0,
            event: ProjectReadProbeEvent::Result(crate::ProjectProbeResult::ControlProduct(
                crate::ControlProductProbeResult::Preview {
                    product,
                    revision: Revision::new(18),
                    extent: ControlExtent::new(1, 30),
                    sample_format: crate::project::WireChannelSampleFormat::U16,
                    sample_layout: ControlSampleLayout {
                        spans: Vec::from([ControlSampleSpan {
                            row: 0,
                            start: 0,
                            len: 30,
                            encoding: ControlSampleEncoding::RgbPixels {
                                count: 10,
                                color_order: ColorOrder::Rgb,
                            },
                        }]),
                    },
                    display_layout: crate::ControlDisplayLayoutProbeResult::Layout(
                        ControlDisplayLayout::Layout2d(ControlLayout2d::new(
                            Revision::new(18),
                            10,
                            10,
                            (0..10)
                                .map(|index| ControlLamp2d {
                                    lamp_index: index,
                                    sample_start: index * 3,
                                    center: [index as f32 / 16.0, index as f32 / 15.0],
                                    radius: 0.02,
                                })
                                .collect(),
                        )),
                    ),
                    bytes: vec![0u8; 30 * 2],
                },
            )),
        };

        let runtime_chunk = ProjectReadEvent::Query {
            index: 2,
            event: ProjectReadQueryEvent::Resources(
                ProjectReadResourceEvent::RuntimeBufferPayloadBytes {
                    resource_ref: ResourceRef::runtime_buffer(RuntimeBufferId::new(9)),
                    offset: 0,
                    bytes: (0..256u32).map(|b| (b & 0xff) as u8).collect(),
                },
            ),
        };

        vec![
            ProjectReadEvent::Begin {
                revision: Revision::new(1),
            },
            shape_entry,
            slot_root,
            control_probe,
            runtime_chunk,
            ProjectReadEvent::End {
                revision: Revision::new(1),
            },
        ]
    }

    fn frame_message(id: u64, sequence: u32, events: &[ProjectReadEvent]) -> WireServerMessage {
        WireServerMessage {
            id,
            msg: ServerMsgBody::ProjectReadFrame {
                frame: ProjectReadFrame::new(sequence, events.to_vec()),
            },
        }
    }

    #[test]
    fn ser_write_json_len_matches_real_ser_write_json_bytes_for_corpus() {
        let events = corpus();
        for id in [0u64, 7, 1_000_000] {
            for sequence in [0u32, 9, 1234] {
                let message = frame_message(id, sequence, &events);
                let real = ser_write_json_string(&message);
                assert_eq!(
                    ser_write_json_len(&message),
                    real.len(),
                    "counting writer diverged from real ser-write-json output",
                );
            }
        }
    }

    #[test]
    fn sink_size_model_predicts_ser_write_json_frame_length() {
        // Replicate exactly what `ProjectReadFrameSink` accumulates: the empty
        // frame envelope plus each event's own measured length plus one comma per
        // adjacent pair.
        let events = corpus();
        let id = 7;
        let sequence = 9;

        let empty_frame_len = ser_write_json_len(&frame_message(id, sequence, &[]));
        let sum_event_len: usize = events.iter().map(ser_write_json_len).sum();
        let separators = events.len().saturating_sub(1);
        let predicted = empty_frame_len + sum_event_len + separators;

        let real = ser_write_json_len(&frame_message(id, sequence, &events));
        assert_eq!(
            predicted, real,
            "sink O(n) size model must equal the real encoded frame length",
        );
    }

    #[test]
    fn documents_serde_json_delta_against_wire_serializer() {
        // The whole reason the sink budgets with ser-write-json: serde_json
        // under/over-counts the real on-wire size for float-bearing frames. This
        // test asserts a *nonzero* delta exists for the float corpus so the two
        // serializers are known to diverge (a future change making them identical
        // should prompt revisiting the whole "measure with the wire serializer"
        // decision), and pins the corpus's serde_json output as a normal-JSON
        // string for the shared parser.
        let events = corpus();
        let message = frame_message(7, 0, &events);

        let serde_len = crate::json::to_string(&message)
            .expect("serde_json serialize")
            .len();
        let wire_len = ser_write_json_len(&message);

        // Both must be valid JSON that round-trips through the shared parser.
        let wire_string = ser_write_json_string(&message);
        let _round_trip: WireServerMessage =
            crate::json::from_str(&wire_string).expect("ser-write-json output parses");

        // The float corpus makes them differ. Document (not just assert) the gap.
        assert_ne!(
            serde_len, wire_len,
            "float corpus must expose serde_json vs ser-write-json divergence; \
             if this ever ties, the wire-serializer measurement rationale changed",
        );
        // The divergence must stay comfortably inside the serial margin so the
        // firmware scratch buffer never overflows even though the sink budgets
        // with the wire serializer.
        let delta = serde_len.abs_diff(wire_len);
        assert!(
            delta < crate::PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES,
            "serde_json vs ser-write-json delta {delta} exceeded serial margin {}",
            crate::PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES,
        );
    }
}

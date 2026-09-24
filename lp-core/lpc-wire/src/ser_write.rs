//! The wire serializer's sinks: the byte counter, and [`ser_wire_to`], which
//! writes a message as JSON or packed.
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

use lp_json_pack::PackError;
use ser_write_json::SerWrite;
use ser_write_json::ser::to_writer;
use ser_write_json::ser_write::{SliceWriter, Token};
use serde::Serialize;

use crate::pack_sink::PackSink;
use crate::wire_encoding::WireEncoding;

/// Error from a type-erased wire serialization.
///
/// The concrete sink's error is deliberately discarded: erasing it is what
/// lets every wire type share one serializer instantiation (see
/// [`ser_write_json_to`]). Both call sites only need "did it fit" — the
/// firmware's stack writer has exactly one failure mode (buffer full) and the
/// counting sink is infallible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErasedWriteError;

impl core::fmt::Display for ErasedWriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("wire serialization failed")
    }
}

/// Object-safe byte sink. One method, one concrete error, so `&mut dyn DynSink`
/// is a single type regardless of the underlying writer.
trait DynSink {
    fn write_all(&mut self, buf: &[u8]) -> Result<(), ErasedWriteError>;
    /// Forward one structural token; `Ok(false)` means "write the text".
    fn token(&mut self, token: Token<'_>) -> Result<bool, ErasedWriteError>;
}

/// Adapts any [`SerWrite`] into a [`DynSink`]. This *is* generic, but it
/// monomorphizes to a two-line forwarding shim — not to a copy of the whole
/// serializer.
struct SinkOf<'a, W: SerWrite>(&'a mut W);

impl<W: SerWrite> DynSink for SinkOf<'_, W> {
    fn write_all(&mut self, buf: &[u8]) -> Result<(), ErasedWriteError> {
        self.0.write(buf).map_err(|_| ErasedWriteError)
    }

    fn token(&mut self, token: Token<'_>) -> Result<bool, ErasedWriteError> {
        self.0.token(token).map_err(|_| ErasedWriteError)
    }
}

/// The single [`SerWrite`] type the JSON serializer is ever instantiated over.
struct ErasedSerWrite<'a> {
    sink: &'a mut dyn DynSink,
    /// The sink's [`SerWrite::TAKES_TOKENS`], read once: a text sink then
    /// pays one branch per token instead of a virtual call.
    takes_tokens: bool,
}

impl SerWrite for ErasedSerWrite<'_> {
    type Error = ErasedWriteError;

    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.sink.write_all(buf)
    }

    #[inline]
    fn token(&mut self, token: Token<'_>) -> Result<bool, Self::Error> {
        if !self.takes_tokens {
            return Ok(false);
        }
        self.sink.token(token)
    }
}

/// Serialize `value` into `sink` with the wire serializer, through a
/// type-erased writer.
///
/// Why this exists: `ser_write_json::ser::to_writer` is generic over the sink,
/// so serializing the same wire type into two different sinks emits two full
/// copies of that type's serializer. On device we do exactly that for every
/// server message — once into [`CountingSerWrite`] to budget the frame, then
/// again into the firmware's stack buffer to actually write it. Routing both
/// through `ErasedSerWrite` collapses each pair into one instantiation, which
/// is worth tens of KB of flash in an image that must fit a 3 MB partition.
/// See `docs/adr/2026-07-28-esp32c6-flash-budget.md`.
///
/// The cost is one virtual call per write op. The serializer writes slices
/// rather than single bytes, so this is not measurable on the streaming path.
///
/// A sink with [`SerWrite::TAKES_TOKENS`] also receives the serializer's
/// structural tokens (see `ser_write::Token`) and writes no text for the ones
/// it takes; every other sink gets exactly the JSON text.
pub fn ser_write_json_to<W: SerWrite, T: Serialize + ?Sized>(
    sink: &mut W,
    value: &T,
) -> Result<(), ErasedWriteError> {
    let mut adapter = SinkOf(sink);
    let mut erased = ErasedSerWrite {
        sink: &mut adapter,
        takes_tokens: W::TAKES_TOKENS,
    };
    to_writer(&mut erased, value).map_err(|_| ErasedWriteError)
}

/// Why [`ser_wire_to`] wrote nothing usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireWriteError {
    /// The buffer is too small for the message in this encoding.
    Full,
    /// Packed only: the message holds text (a `RawValue`) that the packed form
    /// cannot reproduce byte for byte, or the value failed to serialize. Send
    /// it as JSON.
    Unpackable,
}

impl core::fmt::Display for WireWriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Full => "wire message does not fit the buffer",
            Self::Unpackable => "wire message cannot be packed; send it as JSON",
        })
    }
}

/// Write `value` into `buf` in `encoding`, and return the bytes written.
///
/// [`WireEncoding::Json`] writes exactly the `M!` line's JSON text;
/// [`WireEncoding::Packed`] writes one JSON Pack frame's payload (unframed:
/// the transport adds `0x00 'P' COBS … 0x00`) that decodes back to that same
/// text. Both go through [`ser_write_json_to`], so each wire type still has
/// one serializer instantiation.
pub fn ser_wire_to<T: Serialize + ?Sized>(
    buf: &mut [u8],
    encoding: WireEncoding,
    value: &T,
) -> Result<usize, WireWriteError> {
    match encoding {
        WireEncoding::Json => {
            let mut writer = SliceWriter::new(buf);
            // The slice writer's one failure mode is a full buffer.
            ser_write_json_to(&mut writer, value).map_err(|_| WireWriteError::Full)?;
            Ok(writer.len())
        }
        WireEncoding::Packed => {
            let mut sink = PackSink::new(buf);
            let serialized = ser_write_json_to(&mut sink, value);
            match (sink.finish(), serialized) {
                (Ok(n), Ok(())) => Ok(n),
                (Err(PackError::Full), _) => Err(WireWriteError::Full),
                _ => Err(WireWriteError::Unpackable),
            }
        }
    }
}

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
    // Goes through the erased writer so this shares its serializer
    // instantiation with the real write path (see `ser_write_json_to`).
    //
    // The only error channel is the sink (infallible); ser-write-json does not
    // otherwise fail for these wire types. If a future type does fail to
    // serialize, treat it as "does not fit" by reporting usize::MAX so the
    // budget check rejects it instead of silently under-counting.
    match ser_write_json_to(&mut counter, value) {
        Ok(()) => counter.len(),
        Err(_) => usize::MAX,
    }
}

/// A 64-bit FNV-1a hash of `value`'s wire encoding, without allocating it.
///
/// For content comparison of wire payloads the engine answers repeatedly —
/// "is this the same structure I answered last time?" — where keeping the
/// previous answer around to compare against would cost heap on a device.
/// It hashes exactly the bytes the firmware would write, through the same
/// erased serializer instantiation as the real write (see
/// [`ser_write_json_to`]), so a payload type that is written anyway costs no
/// second serializer.
///
/// Not a security boundary: FNV is a non-cryptographic hash, and a collision
/// (2^-64 per comparison) reads as "unchanged".
#[must_use]
pub fn ser_write_json_fnv64<T: Serialize>(value: &T) -> u64 {
    let mut hasher = Fnv64SerWrite::new();
    // As in `ser_write_json_len`: the sink is infallible and wire types do not
    // fail to serialize. A future one that does hashes its prefix, which is
    // still deterministic.
    let _ = ser_write_json_to(&mut hasher, value);
    hasher.hash
}

/// A [`SerWrite`] sink folding every byte into a 64-bit FNV-1a hash.
struct Fnv64SerWrite {
    hash: u64,
}

impl Fnv64SerWrite {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    const fn new() -> Self {
        Self {
            hash: Self::OFFSET_BASIS,
        }
    }
}

impl SerWrite for Fnv64SerWrite {
    type Error = core::convert::Infallible;

    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        for byte in buf {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(Self::PRIME);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServerMessage;
    use crate::server::ServerMsgBody;
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
    fn fnv64_hashes_the_wire_bytes() {
        // FNV-1a 64 of the encoding `"a"` (three bytes: quote, a, quote),
        // checked against the reference algorithm by hand.
        let mut reference = Fnv64SerWrite::new();
        reference.write(b"\"a\"").unwrap();
        assert_eq!(ser_write_json_fnv64(&"a"), reference.hash);
        // The empty input is the offset basis — the published vector.
        assert_eq!(Fnv64SerWrite::new().hash, 0xcbf2_9ce4_8422_2325);
        let mut a = Fnv64SerWrite::new();
        a.write(b"a").unwrap();
        assert_eq!(
            a.hash, 0xaf63_dc4c_8601_ec8c,
            "published FNV-1a 64 vector for \"a\""
        );
        assert_ne!(ser_write_json_fnv64(&1_u32), ser_write_json_fnv64(&2_u32));
    }

    #[test]
    fn ser_write_json_len_matches_serialized_bytes() {
        let msg = ServerMessage::stream_frame(
            42,
            3,
            false,
            ServerMsgBody::ProjectRead {
                events: vec![crate::ProjectReadEvent::Begin {
                    revision: Revision::new(7),
                }],
            },
        );

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
///    formula `ProjectReadStreamSink` uses, so it proves per-push measurement
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
        ProjectReadEvent, ProjectReadProbeEvent, ProjectReadQueryEvent, ProjectReadResourceEvent,
        ProjectReadShapeEvent, WireServerMessage,
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

        // Control probe with a 2D lamp layout. The packed wire form carries
        // centers as base64 (no floats), but each packing span's RADIUS is
        // still an f32 — and an INTEGRAL radius is where the serializers
        // part company (serde_json renders `1.0`, ser-write-json `1`),
        // which is the divergence this corpus exists to document. Two
        // radius groups make two packing spans: one integral, one
        // fractional (fractional f32s render identically in both).
        let product = ControlProduct::new(NodeId::new(2), 0, ControlExtent::new(1, 30));
        let control_probe = ProjectReadEvent::Probe {
            index: 0,
            event: ProjectReadProbeEvent::Result(crate::ProjectProbeResult::ControlProduct(
                crate::ControlProductProbeResult::Preview {
                    product,
                    revision: Revision::new(18),
                    extent: ControlExtent::new(1, 30),
                    sample_format: crate::project::WireChannelSampleFormat::U16,
                    geometry: crate::RevisionGateResult::Changed(crate::ControlProductGeometry {
                        revision: Revision::new(18),
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
                        display_layout: crate::GeometryDisplayLayout::Layout(
                            ControlDisplayLayout::Layout2d(ControlLayout2d::new(
                                Revision::new(18),
                                10,
                                10,
                                (0..10)
                                    .map(|index| ControlLamp2d {
                                        lamp_index: index,
                                        sample_start: index * 3,
                                        center: [index as f32 / 16.0, index as f32 / 15.0],
                                        radius: if index < 5 { 1.0 } else { 0.02 },
                                    })
                                    .collect(),
                            )),
                        ),
                    }),
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
        // Non-final stream frame: both `seq` and `fin:false` are encoded, which is
        // the worst-case envelope the sink budgets against.
        WireServerMessage::stream_frame(
            id,
            sequence,
            false,
            ServerMsgBody::ProjectRead {
                events: events.to_vec(),
            },
        )
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
        // Replicate exactly what `ProjectReadStreamSink` accumulates: the empty
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

/// The token hook through the erased writer, and JSON byte-identity over
/// recorded traffic.
#[cfg(test)]
mod token_hook_tests {
    use super::ser_write_json_to;
    use crate::project::{WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload};
    use crate::test_traffic::{TrafficDirection, traffic_lines};
    use crate::{ClientMessage, WireServerMessage};
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;
    use lpc_model::{ResourceRef, Revision, RuntimeBufferId};
    use ser_write_json::SerWrite;
    use ser_write_json::ser_write::Token;

    /// A sink that takes every token (`take`) or declines every one, and
    /// records both.
    struct TokenLog {
        take: bool,
        tokens: Vec<String>,
        text: Vec<u8>,
    }

    impl SerWrite for TokenLog {
        type Error = core::convert::Infallible;
        const TAKES_TOKENS: bool = true;

        fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
            self.text.extend_from_slice(buf);
            Ok(())
        }

        fn token(&mut self, token: Token<'_>) -> Result<bool, Self::Error> {
            if self.take {
                self.tokens.push(format!("{token:?}"));
            }
            Ok(self.take)
        }
    }

    fn payload() -> WireRuntimeBufferPayload {
        WireRuntimeBufferPayload {
            resource_ref: ResourceRef::runtime_buffer(RuntimeBufferId::new(3)),
            revision: Revision::new(2),
            metadata: WireRuntimeBufferMetadataPayload::Raw,
            bytes: vec![1, 2, 3],
        }
    }

    #[test]
    fn recorded_traffic_reserializes_byte_for_byte() {
        let mut lines = 0;
        for line in traffic_lines() {
            let mut out = Vec::new();
            match line.direction {
                TrafficDirection::BoardToHost => {
                    let msg: WireServerMessage = crate::json::from_str(line.json).unwrap();
                    ser_write_json_to(&mut out, &msg).unwrap();
                }
                TrafficDirection::HostToBoard => {
                    let msg: ClientMessage = crate::json::from_str(line.json).unwrap();
                    ser_write_json_to(&mut out, &msg).unwrap();
                }
            }
            assert_eq!(
                core::str::from_utf8(&out).unwrap(),
                line.json,
                "line {}",
                line.index
            );
            lines += 1;
        }
        assert!(
            lines > 100,
            "the fixture holds the recorded lines ({lines})"
        );
    }

    #[test]
    fn a_token_sink_behind_the_erased_writer_gets_tokens_and_a_blob() {
        let mut log = TokenLog {
            take: true,
            tokens: Vec::new(),
            text: Vec::new(),
        };
        ser_write_json_to(&mut log, &payload()).unwrap();
        assert!(
            log.text.is_empty(),
            "no text reaches a sink that takes tokens"
        );
        let expected = [
            "MapBegin",
            "Key(\"ref\")",
            "Separator",
            "MapBegin",
            "Key(\"domain\")",
            "Separator",
            "Str(\"runtime_buffer\")",
            "Separator",
            "Key(\"id\")",
            "Separator",
            "U64(3)",
            "MapEnd",
            "Separator",
            "Key(\"revision\")",
            "Separator",
            "I64(2)",
            "Separator",
            "Key(\"metadata\")",
            "Separator",
            "Str(\"raw\")",
            "Separator",
            "Key(\"bytes\")",
            "Separator",
            "Blob([1, 2, 3])",
            "MapEnd",
        ];
        assert_eq!(log.tokens, expected);
    }

    #[test]
    fn a_token_sink_that_declines_gets_the_json_text() {
        let mut log = TokenLog {
            take: false,
            tokens: Vec::new(),
            text: Vec::new(),
        };
        ser_write_json_to(&mut log, &payload()).unwrap();
        assert_eq!(
            core::str::from_utf8(&log.text).unwrap(),
            r#"{"ref":{"domain":"runtime_buffer","id":3},"revision":2,"metadata":"raw","bytes":"AQID"}"#
        );
    }
}

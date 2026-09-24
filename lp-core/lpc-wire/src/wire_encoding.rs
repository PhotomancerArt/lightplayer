//! Which form a board writes its wire messages in.
//!
//! Both forms carry the same JSON: a [`WireEncoding::Packed`] frame decodes
//! back to the byte-identical text [`WireEncoding::Json`] writes (see
//! [`decode_packed_to_json`](crate::decode_packed_to_json)), so nothing
//! downstream of a host's decoder can tell them apart.
//!
//! The choice is per link, held by the transport, and JSON until a host asks
//! for packed on that link (plan `lp2025/2026-09-23-1701-lp-json-pack`, Q1).
//! Packed is board→host only; requests stay JSON.
//!
//! # Dictionary versioning
//!
//! A packed frame is coded against [`WIRE_DICTIONARY`](crate::WIRE_DICTIONARY),
//! generated from the wire types (`just wire-dict`). A frame read with a
//! different dictionary decodes to wrong names and nothing would notice, so:
//! - a dictionary change is a wire change: bump
//!   [`WIRE_PROTO_VERSION`](crate::WIRE_PROTO_VERSION)
//!   (`just wire-dict-check` in `check-lint` fails otherwise), and both ends
//!   already refuse a peer whose hello `proto` differs;
//! - at runtime, a host that asks for packed names its dictionary's
//!   `fingerprint()`, and a board packs only when it matches its own.

/// The form a wire message is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WireEncoding {
    /// `M!{json}` text: what every link speaks until a host opts in.
    #[default]
    Json,
    /// JSON Pack (`lp-json-pack`) against [`WIRE_DICTIONARY`](crate::WIRE_DICTIONARY):
    /// the same JSON, packed.
    Packed,
}

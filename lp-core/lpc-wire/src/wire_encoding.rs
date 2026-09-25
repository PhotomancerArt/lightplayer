//! Which form a board writes its wire messages in.
//!
//! Both forms carry the same JSON: a [`WireEncoding::Packed`] frame decodes
//! back to the byte-identical text [`WireEncoding::Json`] writes (see
//! [`WireStream`](crate::WireStream)), so nothing downstream of a host's
//! decoder can tell them apart.
//!
//! The choice is per link, held by the transport, and JSON until a host asks
//! for packed on that link (plan `lp2025/2026-09-23-1701-lp-json-pack`, Q1).
//! Packed is board→host only; requests stay JSON.
//!
//! # The learned table
//!
//! A packed reply is a *learned* JSON Pack frame
//! ([`ser_learned_frame_to`](crate::ser_learned_frame_to)): there is no
//! static dictionary. Each packed link has a table on the board and a twin in
//! the host's reader ([`WireStream`](crate::WireStream)); a name travels in
//! full the first time and as a code after that, and every frame's header
//! says which table state it was coded against, so a reader whose table has
//! parted from the board's drops the frame instead of decoding wrong names
//! (`lp_json_pack::pack_learned`; plan
//! `lp2025/2026-09-25-0006-learned-wire-dictionary`). The two ends agree on
//! nothing but [`PACK_FORMAT_VERSION`](lp_json_pack::PACK_FORMAT_VERSION):
//! the tag table, the learning rule and the table's capacities. A new wire
//! field or variant needs nothing done here.
//!
//! # The opt-in
//!
//! A host asks with [`ClientRequest::SetEncoding`](crate::ClientRequest::SetEncoding)
//! `{ encoding, format }`, always as JSON. The server answers
//! [`ServerMsgBody::SetEncoding`](crate::server::ServerMsgBody::SetEncoding)
//! with the encoding now in effect: `packed` only when the host asked for it,
//! named the board's [`ServerHello::pack_format`](crate::ServerHello::pack_format),
//! and the embedder can pack (`LpServer::set_packed_encoding_supported`);
//! `json` otherwise. The answer itself goes out as JSON; the transport that
//! wrote it switches afterwards, starting a new table epoch with an empty
//! table, and falls back to JSON when the link closes.
//!
//! Every accepted opt-in starts a new epoch, so the same request is also how
//! a host whose table lost step asks the board to start over
//! ([`PackOptIn`](crate::PackOptIn) sends it on a
//! [`WireChunk::Desync`](crate::WireChunk::Desync)).

use lp_json_pack::Dictionary;
use serde::{Deserialize, Serialize};

/// The COBS frame kind of a learned JSON Pack frame on the wire
/// (`\n 0x00 'L' COBS(header + packed) 0x00`).
pub const FRAME_KIND_LEARNED: u8 = b'L';

/// The wire packs with no seed: every name is learned per link
/// (`lp_json_pack::pack_learned`).
pub(crate) static WIRE_SEED: Dictionary = Dictionary::EMPTY;

/// The form a wire message is written in.
///
/// On the wire (the opt-in request [`ClientRequest::SetEncoding`] and its
/// answer) it is the bare string `"json"` or `"packed"`.
///
/// [`ClientRequest::SetEncoding`]: crate::ClientRequest::SetEncoding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WireEncoding {
    /// `M!{json}` text: what every link speaks until a host opts in.
    #[default]
    Json,
    /// JSON Pack (`lp-json-pack`) against the link's learned table: the same
    /// JSON, packed.
    Packed,
}

impl WireEncoding {
    /// The wire spelling (`"json"`, `"packed"`), for logs.
    ///
    /// A named `&str` rather than `Debug`: firmware images build with
    /// `Debug` formatting stripped, where `{:?}` prints nothing.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Packed => "packed",
        }
    }
}

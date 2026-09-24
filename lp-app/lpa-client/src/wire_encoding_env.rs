//! `LP_WIRE_ENCODING`: which form a host asks a board to write its replies
//! in, for the serial transports ([`crate::transport_serial`] and
//! `transport_emu_serial`).
//!
//! Unset (or `packed`), a transport asks for JSON Pack (plan `lp-json-pack`)
//! as soon as the board's hello says it can pack with this build's
//! dictionary, and asks again when the board falls back to JSON
//! ([`lpc_wire::PackOptIn`]). `LP_WIRE_ENCODING=json` never asks, so the
//! board keeps writing today's `M!{json}` lines: for debugging a link with
//! eyes, a serial monitor or a text capture. Either way the reader accepts
//! both forms.
//!
//! ```text
//! LP_WIRE_ENCODING=json lp-cli upload projects/test/basic serial:auto
//! ```

use lpc_wire::WireEncoding;

/// The environment variable a host's requested encoding is read from.
pub const WIRE_ENCODING_ENV: &str = "LP_WIRE_ENCODING";

/// The encoding this process asks boards for: [`WireEncoding::Json`] when
/// [`WIRE_ENCODING_ENV`] is `json`, [`WireEncoding::Packed`] otherwise. An
/// unknown value is warned about and read as the default.
pub fn requested_wire_encoding() -> WireEncoding {
    parse_wire_encoding(std::env::var(WIRE_ENCODING_ENV).ok().as_deref())
}

fn parse_wire_encoding(value: Option<&str>) -> WireEncoding {
    match value.map(str::trim) {
        None | Some("") | Some("packed") => WireEncoding::Packed,
        Some("json") => WireEncoding::Json,
        Some(other) => {
            log::warn!(
                "{WIRE_ENCODING_ENV}={other:?} is neither `json` nor `packed`; asking for packed"
            );
            WireEncoding::Packed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_opts_out_and_everything_else_asks_for_packed() {
        assert_eq!(parse_wire_encoding(Some("json")), WireEncoding::Json);
        assert_eq!(parse_wire_encoding(Some(" json\n")), WireEncoding::Json);
        assert_eq!(parse_wire_encoding(None), WireEncoding::Packed);
        assert_eq!(parse_wire_encoding(Some("packed")), WireEncoding::Packed);
        assert_eq!(parse_wire_encoding(Some("JSON?")), WireEncoding::Packed);
    }
}

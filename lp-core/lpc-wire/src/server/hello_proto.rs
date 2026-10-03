//! The wire version a hello claims, read without reading the hello.
//!
//! The hello is the version handshake (`docs/adr/2026-07-14-wire-hello-versioning.md`):
//! a host compares its `proto` with [`WIRE_PROTO_VERSION`] and treats any
//! difference as "assume nothing works; update the firmware". That only
//! works if the host can SEE the `proto` of a hello it cannot otherwise
//! decode — and a breaking change to the hello itself (wire 33 added a
//! required `hardware.fs`) is exactly when it cannot: the full
//! [`ServerHello`] decode fails, and a board that just said hello at wire 32
//! read as one that never said hello at all (G1-F1, 2026-10-02: every
//! fielded C6 showed "pre-hello firmware" in a wire-33 Studio).
//!
//! So this reads ONE field, from the one place every hello since wire 1 has
//! carried it — `{"msg":{"hello":{"proto":N}}}` — and nothing else. It is
//! not a second decoder for an old hello shape: no other field of an old
//! hello is read, and nothing here grows when the hello changes. Call it
//! only after the full decode failed.
//!
//! [`ServerHello`]: crate::ServerHello

use serde::Deserialize;

#[cfg(doc)]
use crate::WIRE_PROTO_VERSION;

/// The `proto` of a server message that is a hello, whatever else the hello
/// holds; `None` when `json` is not a hello or names no numeric proto.
pub fn hello_proto(json: &str) -> Option<u32> {
    crate::json::from_str::<HelloEnvelope>(json)
        .ok()
        .map(|envelope| envelope.msg.hello.proto)
}

#[derive(Deserialize)]
struct HelloEnvelope {
    msg: HelloBody,
}

#[derive(Deserialize)]
struct HelloBody {
    hello: HelloVersion,
}

#[derive(Deserialize)]
struct HelloVersion {
    proto: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WIRE_PROTO_VERSION, WireServerMessage};

    /// The hello a fielded XIAO C6 sends (wire 32, firmware 5f8febc92324),
    /// captured off the spare board's USB link on 2026-10-02.
    const HELLO_PROTO_32: &str = include_str!("../../testdata/hello-proto32-xiao-c6.json");

    #[test]
    fn a_wire_32_hello_does_not_decode_at_this_wire_version_but_names_its_proto() {
        assert_eq!(
            WIRE_PROTO_VERSION, 33,
            "re-read this test when the version moves"
        );
        assert!(
            crate::json::from_str::<WireServerMessage>(HELLO_PROTO_32.trim()).is_err(),
            "the premise: a wire-32 hello lacks the required hardware.fs"
        );
        assert_eq!(hello_proto(HELLO_PROTO_32.trim()), Some(32));
    }

    #[test]
    fn a_current_hello_names_the_current_proto() {
        let json = HELLO_PROTO_32.trim().replace(
            r#""proto":32"#,
            &alloc::format!(r#""proto":{WIRE_PROTO_VERSION}"#),
        );
        assert_eq!(hello_proto(&json), Some(WIRE_PROTO_VERSION));
    }

    #[test]
    fn a_message_that_is_not_a_hello_names_no_proto() {
        assert_eq!(
            hello_proto(r#"{"id":0,"msg":{"heartbeat":{"proto":32}}}"#),
            None
        );
        assert_eq!(
            hello_proto(r#"{"id":0,"msg":{"hello":{"proto":"32"}}}"#),
            None
        );
        assert_eq!(hello_proto("not json"), None);
    }
}

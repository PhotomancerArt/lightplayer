//! The wire version a hello claims, read without reading the hello.
//!
//! The hello is the version handshake (`docs/adr/2026-07-14-wire-hello-versioning.md`):
//! a host compares its `proto` with [`WIRE_PROTO_VERSION`] and treats any
//! difference as "assume nothing works; update the firmware". That only
//! works if the host can SEE the `proto` of a hello it cannot otherwise
//! decode — and a breaking change to the hello itself (wire 34 added a
//! required `hardware.fs`) is exactly when it cannot: the full
//! [`ServerHello`] decode fails, and a board that just said hello at wire 32
//! read as one that never said hello at all (G1-F1, 2026-10-02: every
//! fielded C6 showed "pre-hello firmware" in a wire-33 Studio).
//!
//! So [`hello_proto`] reads ONE field, from the one place every hello since
//! wire 1 has carried it — `{"msg":{"hello":{"proto":N}}}`. It is not a
//! second decoder for an old hello shape: nothing here grows when the hello
//! changes. Call it only after the full decode failed.
//!
//! [`hello_board_id`] is the one other field read the same way: the board
//! the hello names (`hardware.boardId`, the id the board's stamped
//! `/hardware.json` gives its firmware). Without it a host knows a board is
//! an older LightPlayer but not WHICH board, and the way forward — Update
//! firmware — became "pick your board from all of them" on a board that had
//! said exactly what it is (G1 walk, 2026-10-03: "it really shouldn't say 8
//! boards fit … ideally we'd know what board it is"). It is read
//! separately from `proto`, so an old hello whose board field has some
//! other shape still names its version. That is the whole list: two
//! fields, both of which every hello since the stamp existed has carried in
//! the same place.
//!
//! [`ServerHello`]: crate::ServerHello

use alloc::string::String;

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

/// The board id a hello names (`{"msg":{"hello":{"hardware":{"boardId":"…"}}}}`),
/// whatever else the hello holds; `None` when `json` is not a hello, or the
/// hello names no board (one nobody stamped). The companion of
/// [`hello_proto`], under the same rule: call it only after the full decode
/// failed.
pub fn hello_board_id(json: &str) -> Option<String> {
    crate::json::from_str::<BoardEnvelope>(json)
        .ok()
        .and_then(|envelope| envelope.msg.hello.hardware)
        .and_then(|hardware| hardware.board_id)
        .filter(|board| !board.is_empty())
}

#[derive(Deserialize)]
struct BoardEnvelope {
    msg: BoardBody,
}

#[derive(Deserialize)]
struct BoardBody {
    hello: BoardHello,
}

#[derive(Deserialize)]
struct BoardHello {
    #[serde(default)]
    hardware: Option<BoardHardware>,
}

#[derive(Deserialize)]
struct BoardHardware {
    #[serde(default, rename = "boardId")]
    board_id: Option<String>,
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
            WIRE_PROTO_VERSION, 40,
            "re-read this test when the version moves"
        );
        assert!(
            crate::json::from_str::<WireServerMessage>(HELLO_PROTO_32.trim()).is_err(),
            "the premise: a wire-32 hello lacks the required hardware.fs"
        );
        assert_eq!(hello_proto(HELLO_PROTO_32.trim()), Some(32));
    }

    /// Main's wire 33 (the device store's `open` as a word, PR #929) kept
    /// the hello's shape: a board flashed from it sends the fielded hello
    /// with `proto` 33 and no `hardware.fs`. It does not decode at 34, and
    /// it names its proto and its board all the same — an older LightPlayer
    /// like the fielded wire-32 boards.
    #[test]
    fn a_wire_33_hello_does_not_decode_either_and_names_its_proto_and_board() {
        let hello = HELLO_PROTO_32
            .trim()
            .replace(r#""proto":32"#, r#""proto":33"#);
        assert!(crate::json::from_str::<WireServerMessage>(&hello).is_err());
        assert_eq!(hello_proto(&hello), Some(33));
        assert_eq!(
            hello_board_id(&hello).as_deref(),
            Some("seeed/xiao-esp32-c6")
        );
    }

    /// Main's wire 34 (`hardware.fs`, the C6 repartition) still lacks the
    /// build's `version`, which wire 35 requires (36 and 37 left the hello
    /// as it was, 38 only added the optional `firmware`, and 39 and 40 changed only
    /// network shapes): such a hello does not
    /// decode, and names its proto and board all the same. The same hello
    /// with a version decodes — the one field is the whole difference.
    #[test]
    fn a_wire_34_hello_lacks_only_the_version() {
        assert_eq!(
            WIRE_PROTO_VERSION, 40,
            "re-read this test when the version moves"
        );
        let wire_34 = HELLO_PROTO_32
            .trim()
            .replace(r#""proto":32"#, r#""proto":34"#)
            .replace(r#":fe:ff"}"#, r#":fe:ff","fs":"mounted"}"#);
        assert!(crate::json::from_str::<WireServerMessage>(&wire_34).is_err());
        assert_eq!(hello_proto(&wire_34), Some(34));
        assert_eq!(
            hello_board_id(&wire_34).as_deref(),
            Some("seeed/xiao-esp32-c6")
        );

        let versioned = wire_34.replace(
            r#""package":"fw-esp32c6","#,
            r#""package":"fw-esp32c6","version":"2026.10.03-1","#,
        );
        let decoded = crate::json::from_str::<WireServerMessage>(&versioned)
            .expect("a wire-34 hello plus a version is this wire's hello");
        match decoded.msg {
            crate::server::ServerMsgBody::Hello(hello) => {
                assert_eq!(hello.build.version, "2026.10.03-1");
            }
            other => panic!("expected a hello, got {other:?}"),
        }
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

    /// G1 walk (2026-10-03): the fielded C6's wire-32 hello names its board
    /// (Studio stamped it), and that is the one other fact read off it.
    #[test]
    fn a_wire_32_hello_names_its_board() {
        assert_eq!(
            hello_board_id(HELLO_PROTO_32.trim()).as_deref(),
            Some("seeed/xiao-esp32-c6")
        );
    }

    #[test]
    fn a_hello_naming_no_board_names_none_and_keeps_its_proto() {
        let stamped = r#""boardId":"seeed/xiao-esp32-c6""#;
        let unstamped = HELLO_PROTO_32.trim().replace(stamped, r#""boardId":null"#);
        assert_eq!(hello_board_id(&unstamped), None);
        // A board field of some other shape costs the board, never the
        // version.
        let odd = HELLO_PROTO_32
            .trim()
            .replace(stamped, r#""boardId":{"id":7}"#);
        assert_eq!(hello_board_id(&odd), None);
        assert_eq!(hello_proto(&odd), Some(32));
        assert_eq!(
            hello_board_id(r#"{"id":0,"msg":{"heartbeat":{"hardware":{"boardId":"x"}}}}"#),
            None,
            "not a hello"
        );
        assert_eq!(
            hello_board_id(r#"{"id":0,"msg":{"hello":{"proto":32}}}"#),
            None
        );
    }
}

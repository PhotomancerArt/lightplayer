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
//! other shape still names its version.
//!
//! Three more are read the same way, for the one thing a board on another
//! wire still needs from this host — its firmware update, which rides
//! lp-link's update channel (channel 3), never this wire
//! (`docs/adr/2026-10-06-studio-updates-over-the-update-channel.md`, its
//! 2026-10-28 amendment). Without them a board on an older wire, reached
//! over Wi‑Fi, Bluetooth or the relay, could not be updated at all: no
//! cable gives its MAC, and no announcement lets a host speak channel 3:
//!
//! - [`hello_firmware`]: the hello's board manifest (`firmware`, wire 38 on)
//!   — `lpc_update::BoardManifest`, the update channel's own `M`, read by
//!   that type's own reader, so it is as stable as channel 3 itself. It is
//!   the board's announcement that it speaks the update channel.
//! - [`hello_base_mac`] and [`hello_device_uid`]: `hardware.baseMac` and
//!   `deviceUid`, who the board is. A host keys a board by them, as a USB
//!   port's serial number does over a cable.
//! - [`hello_on_tree_store`]: whether the board's files are in the tree
//!   store (`build.features` holds `fs.tree`, or `hardware.fs` is
//!   `refused`; never `legacy_held`), which decides whether a tree-store
//!   build may go to it over the air.
//!
//! That is the whole list: six fields, each read on its own, so a field of
//! another shape costs only itself. None of it is a second decoder for an
//! old hello: nothing here grows when the hello changes.
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

/// The board manifest a hello carries (`{"msg":{"hello":{"firmware":{…}}}}`),
/// read by `lpc_update::BoardManifest`'s own reader, whatever else the
/// hello holds; `None` when `json` is not a hello, carries no manifest (a
/// single image, a host), or carries one that does not read. A board that
/// sends one speaks lp-link's update channel. Call it only after the full
/// decode failed, as [`hello_proto`].
pub fn hello_firmware(json: &str) -> Option<lpc_update::BoardManifest> {
    crate::json::from_str::<FirmwareEnvelope>(json)
        .ok()
        .and_then(|envelope| envelope.msg.hello.firmware)
}

/// The board's base MAC as its hello says it (`hardware.baseMac`), verbatim
/// — the caller normalizes it; `None` when `json` is not a hello or names
/// none. Call it only after the full decode failed, as [`hello_proto`].
pub fn hello_base_mac(json: &str) -> Option<String> {
    crate::json::from_str::<BoardEnvelope>(json)
        .ok()
        .and_then(|envelope| envelope.msg.hello.hardware)
        .and_then(|hardware| hardware.base_mac)
        .filter(|mac| !mac.is_empty())
}

/// The device uid a hello names (`deviceUid`, the stamped identity),
/// verbatim; `None` when `json` is not a hello or the board is unstamped.
/// Call it only after the full decode failed, as [`hello_proto`].
pub fn hello_device_uid(json: &str) -> Option<String> {
    crate::json::from_str::<UidEnvelope>(json)
        .ok()
        .and_then(|envelope| envelope.msg.hello.device_uid)
        .filter(|uid| !uid.is_empty())
}

#[derive(Deserialize)]
struct UidEnvelope {
    msg: UidBody,
}

#[derive(Deserialize)]
struct UidBody {
    hello: UidHello,
}

#[derive(Deserialize)]
struct UidHello {
    #[serde(default, rename = "deviceUid")]
    device_uid: Option<String>,
}

/// Whether the board's files are in the tree store, as its hello says it:
/// its build lists `fs.tree`, or its filesystem is `refused` (a littlefs
/// core that found a tree store holds it), and it is not `legacy_held` (a
/// tree build holding littlefs files for a move). `None` when `json` is not
/// a hello or names neither fact. Call it only after the full decode
/// failed, as [`hello_proto`].
pub fn hello_on_tree_store(json: &str) -> Option<bool> {
    let hello = crate::json::from_str::<TreeEnvelope>(json).ok()?.msg.hello;
    let fs = hello.hardware.and_then(|hardware| hardware.fs);
    let features = hello.build.and_then(|build| build.features);
    if fs.is_none() && features.is_none() {
        return None;
    }
    let tree = features.is_some_and(|features| features.iter().any(|f| f == "fs.tree"));
    Some(match fs.as_deref() {
        Some("legacy_held") => false,
        Some("refused") => true,
        _ => tree,
    })
}

#[derive(Deserialize)]
struct FirmwareEnvelope {
    msg: FirmwareBody,
}

#[derive(Deserialize)]
struct FirmwareBody {
    hello: FirmwareHello,
}

#[derive(Deserialize)]
struct FirmwareHello {
    #[serde(default)]
    firmware: Option<lpc_update::BoardManifest>,
}

#[derive(Deserialize)]
struct TreeEnvelope {
    msg: TreeBody,
}

#[derive(Deserialize)]
struct TreeBody {
    hello: TreeHello,
}

#[derive(Deserialize)]
struct TreeHello {
    #[serde(default)]
    build: Option<TreeBuild>,
    #[serde(default)]
    hardware: Option<TreeHardware>,
}

#[derive(Deserialize)]
struct TreeBuild {
    #[serde(default)]
    features: Option<alloc::vec::Vec<String>>,
}

#[derive(Deserialize)]
struct TreeHardware {
    #[serde(default)]
    fs: Option<String>,
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
    #[serde(default, rename = "baseMac")]
    base_mac: Option<String>,
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
            WIRE_PROTO_VERSION, 46,
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
    /// build's `version`, which wire 35 requires, and its `projectFormat`,
    /// which wire 46 requires (36 and 37 left the hello as it was, 38 only
    /// added the optional `firmware`, 39 and 40 changed only network shapes,
    /// 41 only edit bodies, 42 only fs verbs, 43 only added the `fs` value
    /// `refused`, 44 only file bodies, and 45 only removed the node-command
    /// channel): such a hello does not decode, and names its proto and
    /// board all the same. The same hello with a version and a project
    /// format decodes — the two fields are the whole difference.
    #[test]
    fn a_wire_34_hello_lacks_only_the_version_and_the_project_format() {
        assert_eq!(
            WIRE_PROTO_VERSION, 46,
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
        assert!(
            crate::json::from_str::<WireServerMessage>(&versioned).is_err(),
            "a version alone is not enough since wire 46"
        );
        let versioned = versioned.replace(
            r#""profile":"release-esp32"}"#,
            r#""profile":"release-esp32","projectFormat":12}"#,
        );
        let decoded = crate::json::from_str::<WireServerMessage>(&versioned)
            .expect("a wire-34 hello plus a version and a project format is this wire's hello");
        match decoded.msg {
            crate::server::ServerMsgBody::Hello(hello) => {
                assert_eq!(hello.build.version, "2026.10.03-1");
                assert_eq!(hello.build.project_format, 12);
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
    /// A board on an older wire whose hello carries its board manifest
    /// (a split image, wire 38 on) names what its update needs, though the
    /// hello does not decode: the manifest, its MAC and whether its files
    /// are in the tree store.
    #[test]
    fn an_older_hello_that_does_not_decode_still_names_its_update_facts() {
        let manifest = manifest();
        let json = older_split_hello(&manifest, r#""fs.tree","#, "mounted");
        assert!(
            crate::json::from_str::<WireServerMessage>(&json).is_err(),
            "the premise: the hello does not decode at this wire"
        );
        assert_eq!(hello_proto(&json), Some(32));
        assert_eq!(hello_firmware(&json), Some(manifest));
        assert_eq!(hello_base_mac(&json).as_deref(), Some("10:bd:a3:b0:8e:30"));
        assert_eq!(hello_device_uid(&json), None, "the fixture is unstamped");
        let stamped = json.replace(
            r#""deviceUid":null"#,
            r#""deviceUid":"dev00000000000000a1""#,
        );
        assert_eq!(
            hello_device_uid(&stamped).as_deref(),
            Some("dev00000000000000a1")
        );
        assert_eq!(hello_on_tree_store(&json), Some(true));
    }

    /// The tree-store fact follows the hello's own words: `fs.tree` alone,
    /// a littlefs core holding a tree store (`refused`), never a tree build
    /// holding littlefs files (`legacy_held`).
    #[test]
    fn an_older_hellos_tree_store_fact_reads_its_build_and_its_filesystem() {
        let m = manifest();
        let read =
            |features: &str, fs: &str| hello_on_tree_store(&older_split_hello(&m, features, fs));
        assert_eq!(read(r#""fs.tree","#, "mounted"), Some(true));
        assert_eq!(read("", "mounted"), Some(false), "a littlefs build");
        assert_eq!(
            read("", "refused"),
            Some(true),
            "a littlefs core holding a tree store"
        );
        assert_eq!(read(r#""fs.tree","#, "legacy_held"), Some(false));
        assert_eq!(
            hello_on_tree_store(r#"{"id":0,"msg":{"hello":{"proto":32}}}"#),
            None,
            "a hello that says neither"
        );
    }

    /// A hello with no manifest (a single image, a host) announces no update
    /// channel; a manifest of another shape costs only the manifest.
    #[test]
    fn a_hello_without_a_readable_manifest_names_none_and_keeps_the_rest() {
        let plain = HELLO_PROTO_32.trim();
        assert_eq!(hello_firmware(plain), None);
        assert_eq!(hello_base_mac(plain).as_deref(), Some("10:bd:a3:b0:8e:30"));
        let odd = plain.replace(
            r#""packFormat":2"#,
            r#""packFormat":2,"firmware":{"proto":"one"}"#,
        );
        assert_eq!(hello_firmware(&odd), None);
        assert_eq!(hello_proto(&odd), Some(32));
        assert_eq!(hello_base_mac(&odd).as_deref(), Some("10:bd:a3:b0:8e:30"));
        assert_eq!(
            hello_firmware(r#"{"id":0,"msg":{"heartbeat":{"firmware":{}}}}"#),
            None,
            "not a hello"
        );
    }

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

    /// The fielded wire-32 hello with a build feature list led by
    /// `features`, its filesystem `fs`, and `manifest` as its `firmware`.
    fn older_split_hello(manifest: &lpc_update::BoardManifest, features: &str, fs: &str) -> String {
        let firmware = String::from_utf8(manifest.to_json()).unwrap();
        HELLO_PROTO_32
            .trim()
            .replace(
                r#""features":["#,
                &alloc::format!(r#""features":[{features}"#),
            )
            .replace(r#":fe:ff"}"#, &alloc::format!(r#":fe:ff","fs":"{fs}"}}"#))
            .replace(
                r#""packFormat":2"#,
                &alloc::format!(r#""packFormat":2,"firmware":{firmware}"#),
            )
    }

    fn manifest() -> lpc_update::BoardManifest {
        use alloc::string::ToString;
        lpc_update::BoardManifest {
            proto: 1,
            target: "esp32c6-4mb".to_string(),
            chip: "esp32c6".to_string(),
            version: "2026.10.25-1".to_string(),
            build_id: "2026.10.25-1+62f8e82610b2".to_string(),
            wire_proto: 44,
            core_sha256: "11".repeat(32),
            core_len: 1_160_000,
            engine_sha256: "22".repeat(32),
            engine_len: Some(1_830_000),
            layout: 1,
            loader: 1,
            region_len: 3_375_104,
            state: lpc_update::BoardState::Running,
            refused_build: None,
            transfer: None,
        }
    }
}

//! The **board manifest**: what a board says about itself (DM8,
//! `one-way-doors.md` §2). One serde type, one encoding — JSON, camelCase —
//! used as `M`'s payload on channel 3 (authoritative) and, from Part B, as
//! the hello's `firmware` field (a convenience).
//!
//! ```json
//! { "proto": 1,
//!   "target": "esp32c6-4mb", "chip": "esp32c6",
//!   "version": "2026.10.05-3", "buildId": "2026.10.05-3+abc123456789", "wireProto": 36,
//!   "coreSha256": "…64 hex…",   "coreLen": 1160000,
//!   "engineSha256": "…64 hex…", "engineLen": 1830000,
//!   "layout": 1, "loader": 1, "regionLen": 3375104,
//!   "state": "running", "refusedBuild": null, "transfer": null }
//! ```
//!
//! # How it may change — forever
//!
//! Keys are only **added**; a key is never removed, renamed, or reused with a
//! new meaning. Readers ignore keys they do not know (there is no
//! `deny_unknown_fields`) and read a `state` they do not know as
//! [`BoardState::Unknown`]. What would force a break: changing what a hash
//! covers, or reusing a key.
//!
//! # A board reports exactly its release's identity
//!
//! A board running release R reports exactly the identity R's
//! `ota-manifest.json` publishes (the firmware-distribution plan's file).
//! Part B tests it on the emulator, field by field (B-P08 U17); this crate
//! cannot, because it does not depend on that plan's crate.
//!
//! | Board manifest | `ota-manifest.json` | Rule |
//! |---|---|---|
//! | `target`, `chip`, `version`, `wireProto` | same keys | equal |
//! | `buildId` | (dropped there) | equals the engine header's bytes; for a release, `version + "+" + commit[..12]` |
//! | `coreSha256`, `coreLen` | `core.sha256`, `core.length` | equal (the hash rules, [`crate::hash_rules`]) |
//! | `engineSha256`, `engineLen` | `engine.sha256`, `engine.length` | equal; also the core's digest slot and Studio's cache key |
//! | `layout` | `requires.layout` | an install needs them **equal** |
//! | `loader` | `requires.loader` | an install needs the board's **≥** the manifest's |
//! | `regionLen` | `core.length` + `engine.length` | the board decides fit; the host only predicts it |
//! | `proto`, `state`, `refusedBuild`, `transfer` | — | board only |
//!
//! `chip` is the chip **word** (`"esp32c6"`), never the offer's `u16` code.
//! `engineLen` is `null` when the core cannot know it: `needs-engine` with
//! the engine header gone, since the digest slot holds no length (a heal's
//! host knows it from the engine it holds).

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::piece_kind::PieceKind;

/// The board manifest. Field order is the JSON's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardManifest {
    /// The protocol version this board speaks ([`crate::code_table::PROTO_V1`]).
    pub proto: u8,
    /// The opaque target name (`esp32c6-4mb`), embedded in the image. Never
    /// parsed, never compared to refuse an install.
    pub target: String,
    /// The chip word.
    pub chip: String,
    /// The app version (`2026.10.05-3`).
    pub version: String,
    /// `<version>+<commit[..12]>`. Always `buildId`, never a bare `build`.
    pub build_id: String,
    /// The image's manifest-core `wireProto`: whether a host can talk to
    /// this board on channel 1.
    pub wire_proto: u32,
    /// SHA-256 of the running core (lowercase hex).
    pub core_sha256: String,
    pub core_len: u32,
    /// The core's digest slot: SHA-256 of the engine this core needs.
    pub engine_sha256: String,
    /// The engine's length from its valid header; `null` when unknown.
    #[serde(default)]
    pub engine_len: Option<u32>,
    pub layout: u16,
    pub loader: u16,
    /// Bytes in the region the core and engine share.
    pub region_len: u32,
    pub state: BoardState,
    /// The build hash of the build that failed its trial here (E3).
    #[serde(default)]
    pub refused_build: Option<u32>,
    /// While a transfer is pending or running.
    #[serde(default)]
    pub transfer: Option<TransferView>,
}

/// What the board is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoardState {
    /// The engine runs.
    Running,
    /// Core-only, with no valid engine: it waits for its engine (E1, E13).
    NeedsEngine,
    /// Core-only, because the engine keeps crashing (E10).
    EngineCrashing,
    /// A transfer is pending or running.
    Updating,
    /// A new core on trial, not yet confirmed.
    OnTrial,
    /// A state this reader does not know.
    #[serde(other)]
    Unknown,
}

/// A transfer, as the manifest reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferView {
    pub kind: PieceKind,
    /// Bytes written and read back.
    pub done: u32,
    pub total: u32,
    /// Another link owns it and is live (E6): a new host gets `N`/`B`.
    pub busy: bool,
}

impl BoardManifest {
    /// The JSON `M` carries.
    #[must_use]
    pub fn to_json(&self) -> Vec<u8> {
        // A struct of strings, integers and options always serializes.
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Read a manifest, ignoring keys this reader does not know.
    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn example() -> BoardManifest {
        BoardManifest {
            proto: 1,
            target: "esp32c6-4mb".to_string(),
            chip: "esp32c6".to_string(),
            version: "2026.10.05-3".to_string(),
            build_id: "2026.10.05-3+abc123456789".to_string(),
            wire_proto: 36,
            core_sha256: "11".repeat(32),
            core_len: 1_160_000,
            engine_sha256: "22".repeat(32),
            engine_len: Some(1_830_000),
            layout: 1,
            loader: 1,
            region_len: 3_375_104,
            state: BoardState::Running,
            refused_build: None,
            transfer: None,
        }
    }

    #[test]
    fn serializes_in_the_documented_order_with_camel_case_keys() {
        let json = String::from_utf8(example().to_json()).unwrap();
        let want = alloc::format!(
            "{{\"proto\":1,\"target\":\"esp32c6-4mb\",\"chip\":\"esp32c6\",\
             \"version\":\"2026.10.05-3\",\"buildId\":\"2026.10.05-3+abc123456789\",\
             \"wireProto\":36,\"coreSha256\":\"{}\",\"coreLen\":1160000,\
             \"engineSha256\":\"{}\",\"engineLen\":1830000,\"layout\":1,\"loader\":1,\
             \"regionLen\":3375104,\"state\":\"running\",\"refusedBuild\":null,\
             \"transfer\":null}}",
            "11".repeat(32),
            "22".repeat(32)
        );
        assert_eq!(json, want);
    }

    #[test]
    fn round_trips_and_ignores_unknown_keys() {
        let mut m = example();
        m.state = BoardState::Updating;
        m.engine_len = None;
        m.refused_build = Some(0xdead_beef);
        m.transfer = Some(TransferView {
            kind: PieceKind::Core,
            done: 8192,
            total: 1_160_000,
            busy: true,
        });
        let json = m.to_json();
        assert_eq!(BoardManifest::from_json(&json).unwrap(), m);

        let mut text = String::from_utf8(json).unwrap();
        text.pop();
        text.push_str(",\"someFutureKey\":{\"a\":[1,2]}}");
        assert_eq!(BoardManifest::from_json(text.as_bytes()).unwrap(), m);
    }

    #[test]
    fn every_state_is_kebab_case_and_an_unknown_one_reads_as_unknown() {
        for (state, word) in [
            (BoardState::Running, "running"),
            (BoardState::NeedsEngine, "needs-engine"),
            (BoardState::EngineCrashing, "engine-crashing"),
            (BoardState::Updating, "updating"),
            (BoardState::OnTrial, "on-trial"),
        ] {
            assert_eq!(
                serde_json::to_string(&state).unwrap(),
                alloc::format!("\"{word}\"")
            );
        }
        let text = String::from_utf8(example().to_json())
            .unwrap()
            .replace("\"running\"", "\"sleeping\"");
        assert_eq!(
            BoardManifest::from_json(text.as_bytes()).unwrap().state,
            BoardState::Unknown
        );
    }

    #[test]
    fn the_build_id_key_is_build_id_never_build() {
        let json = String::from_utf8(example().to_json()).unwrap();
        assert!(json.contains("\"buildId\""));
        assert!(!json.contains("\"build\""));
    }
}

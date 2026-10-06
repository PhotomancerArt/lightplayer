//! **lpc-update**: over-the-air update protocol v1 for the split image.
//!
//! The vocabulary and formats both ends share, and (in [`board`]) the
//! board's update session as a sans-IO state machine the firmware drives:
//!
//! - [`message`]: the channel-3 messages, their codec, and the additive-only
//!   rules (unknown messages answered `N`/`U`, trailing bytes ignored,
//!   must-understand flags);
//! - [`code_table`]: every integer code (proto, chunk, chip, layout, loader,
//!   encoding);
//! - [`install_kind`]: the install kind, decided by hashes;
//! - [`board_manifest`]: what a board reports (`M`, and the hello's
//!   `firmware` block);
//! - [`dictionary_rule`]: what encoding 1 means;
//! - [`transfer_record`]: the progress record v1 and its foreign-record rule;
//! - [`hash_rules`]: the two hash rules;
//! - [`status_light_record`]: the update light's record (`/.lp/status-light.json`);
//! - [`build_id`]: the build id field and the build hash;
//! - [`board`]: the board's update session;
//! - `testing` (feature `test-support`): a NOR model, a model board and a rig
//!   for host tests.
//!
//! Protocol v1 binds from Part B's first release (the split image's D11);
//! until then it changes here, not as versions. After that every format here
//! changes only by adding — see each module's docs and the README.
//!
//! `no_std` + `alloc`, sans-IO: no clock (time is a caller's `now_ms`), no
//! randomness (injected), no IO (flash is a trait). It never depends on the
//! firmware-distribution plan's crates.

#![no_std]

extern crate alloc;

pub mod board;
pub mod board_manifest;
pub mod build_id;
pub mod chunk;
pub mod code_table;
pub mod dictionary_rule;
pub mod flag_rule;
pub mod hash_rules;
pub mod install_kind;
pub mod login_step;
pub mod message;
pub mod offer;
pub mod piece_kind;
pub mod read_back_request;
pub mod refusal;
pub mod request;
pub mod sha256_hex;
pub mod status_light_record;
pub mod transfer_record;
mod wire_reader;

#[cfg(feature = "test-support")]
pub mod testing;

pub use board_manifest::{BoardManifest, BoardState, TransferView};
pub use build_id::{BUILD_ID_LEN, build_hash, build_hash_of_field, build_id_field, build_id_text};
pub use chunk::{ChunkEncoding, ChunkRef, encode_chunk};
pub use code_table::{CHUNK, ENCODING_1, PROTO_V1};
pub use install_kind::{ContradictoryOffer, InstallKind, install_kind};
pub use login_step::{BoardLoginStep, HostLoginStep, tier_code, tier_from_code};
pub use message::{BoardMessage, DecodeError, HostMessage, encode_query};
pub use offer::Offer;
pub use piece_kind::PieceKind;
pub use read_back_request::ReadBackRequest;
pub use refusal::{Mismatch, Refusal};
pub use request::Request;
pub use sha256_hex::{sha256_from_hex, sha256_to_hex};
pub use status_light_record::{STATUS_LIGHT_PATH, StatusLightRecord, light_for};
pub use transfer_record::{
    MarkSet, OwnFacts, RecordClass, RecordRead, RecordStage, TransferRecord,
};

//! The access core: who may do what to a LightPlayer device over a link it
//! does not physically trust.
//!
//! - **Shared secrets, not ownership.** A device holds a short list of
//!   labelled secrets ([`SecretEntry`]), each granting a [`Tier`]: **play**
//!   (the panel plus reads) or **edit** (everything).
//! - **HMAC challenge-response.** A client derives `K` from a password with
//!   PBKDF2-HMAC-SHA256 ([`pbkdf2_sha256`], client side only) and answers
//!   the board's challenge with `HMAC-SHA256(K, nonce)`
//!   ([`hmac_sha256`]). The board stores `(salt, iterations, K)` and runs
//!   only the HMAC. The password never crosses a link and is never stored.
//! - **One login at a time, with backoff** ([`LoginState`],
//!   [`RateLimit`]), both per device.
//! - **Two persisted files**, both `version: 2` (v1 still reads): the project sidecar
//!   ([`ProjectAccessFile`], `<project>/.lp/access.json`) and the device
//!   store ([`DeviceAccessFile`], root `/.lp/access.json`). Neither is ever
//!   readable over any link ([`is_access_file_path`]).
//!
//! Sans-IO throughout: time is a caller-supplied millisecond count and
//! randomness is caller-supplied bytes. Nothing here reads a clock, draws a
//! random number, touches a filesystem, or knows what a link is — the
//! server (`lpa-server`) owns links and trust, and asks this crate only
//! for verdicts.
//!
//! HMAC and PBKDF2 are written here from RFC 2104 and RFC 8018 over the
//! workspace `sha2`; RustCrypto's `hmac` and `pbkdf2` are dev-dependency
//! oracles only. Decision record: `docs/adr/2026-09-23-ble-access-model.md`.

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod access_file_error;
pub mod access_file_path;
pub mod base64_bytes;
pub mod constant_time_eq;
pub mod device_access_file;
pub mod hmac_sha256;
pub mod login_state;
pub mod pbkdf2_sha256;
pub mod project_access_file;
pub mod rate_limit;
pub mod secret_entry;
pub mod secret_kind;
pub mod tier;

pub use access_file_error::AccessFileError;
pub use access_file_path::{is_access_file_path, is_within_dir};
pub use constant_time_eq::constant_time_eq;
pub use device_access_file::DeviceAccessFile;
pub use hmac_sha256::{HMAC_SHA256_BYTES, HmacSha256, hmac_sha256};
pub use login_state::{
    BeginOutcome, CHALLENGE_TTL_MS, Challenge, LoginMac, LoginOffer, LoginOutcome, LoginState,
    NONCE_BYTES,
};
pub use pbkdf2_sha256::{derive_login_key, pbkdf2_sha256};
pub use project_access_file::ProjectAccessFile;
pub use rate_limit::RateLimit;
pub use secret_entry::{KEY_BYTES, SALT_BYTES, SecretEntry};
pub use secret_kind::SecretKind;
pub use tier::Tier;

/// Most secrets one access file may hold. The login challenge offers every
/// installed secret in one frame, and a client runs one KDF per offer, so
/// the list is kept short by construction.
pub const MAX_SECRETS_PER_FILE: usize = 16;

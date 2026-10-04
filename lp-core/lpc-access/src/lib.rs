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
//! - **Secure links log in by handshake.** On a link built with lp-link's
//!   `secure` feature the client names an entry by its salt and proves it
//!   holds `link_psk(K)` ([`link_psk`]) in the Noise handshake; the device
//!   answers the lookup with [`key_candidates`], and the match is the login.
//! - **Two persisted files**: the project sidecar ([`ProjectAccessFile`],
//!   `<project>/.lp/access.json`, `version: 2`, v1 still reads) and the
//!   device store ([`DeviceAccessFile`], root `/.lp/access.json`,
//!   `version: 3`, v1 and v2 still read).
//! - **Open to anyone nearby** ([`OpenTo`]): what a link holds with no
//!   login — nobody, play, or play and edit. A board with no store is open
//!   at edit, for now.
//! - **The device network file** ([`NetworkFile`], root `/.lp/network.json`,
//!   `version: 1`): the saved Wi-Fi network ([`WifiNetwork`], validated to
//!   the 802.11 / WPA2 rules) and the `lanOnly` relay switch.
//! - **Write-only files.** Neither access file, nor the network file, is
//!   ever readable over any link at any tier ([`is_write_only_file_path`]).
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
pub mod base64_bytes;
pub mod constant_time_eq;
pub mod device_access_file;
pub mod hmac_sha256;
pub mod key_lookup;
pub mod link_psk;
pub mod login_state;
pub mod network_file;
pub mod network_file_error;
pub mod open_to;
pub mod pbkdf2_sha256;
pub mod project_access_file;
pub mod rate_limit;
pub mod secret_entry;
pub mod secret_kind;
pub mod tier;
pub mod wifi_network;
pub mod write_only_file_path;

pub use access_file_error::AccessFileError;
pub use constant_time_eq::constant_time_eq;
pub use device_access_file::DeviceAccessFile;
pub use hmac_sha256::{HMAC_SHA256_BYTES, HmacSha256, hmac_sha256};
pub use key_lookup::{KeyCandidate, key_candidates};
pub use link_psk::{LINK_PSK_LABEL, link_psk};
pub use login_state::{
    BeginOutcome, CHALLENGE_TTL_MS, Challenge, LoginMac, LoginOffer, LoginOutcome, LoginState,
    NONCE_BYTES,
};
pub use network_file::NetworkFile;
pub use network_file_error::NetworkFileError;
pub use open_to::OpenTo;
pub use pbkdf2_sha256::{derive_login_key, pbkdf2_sha256};
pub use project_access_file::ProjectAccessFile;
pub use rate_limit::RateLimit;
pub use secret_entry::{KEY_BYTES, SALT_BYTES, SecretEntry};
pub use secret_kind::SecretKind;
pub use tier::Tier;
pub use wifi_network::{WifiNetwork, validate_password, validate_ssid};
pub use write_only_file_path::{is_within_dir, is_write_only_file_path};

/// Most secrets one access file may hold. The login challenge offers every
/// installed secret in one frame, and a client runs one KDF per offer, so
/// the list is kept short by construction.
pub const MAX_SECRETS_PER_FILE: usize = 16;

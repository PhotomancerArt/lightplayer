//! The secure channel (feature `secure`): **Noise_NNpsk0_25519_ChaChaPoly_SHA256**
//! merged into lp-link's own SYN handshake, then every frame sealed.
//!
//! ```text
//! NNpsk0:
//!   -> psk, e          (msg1: the initiator's SYN, 48 B after the key id)
//!   <- e, ee           (msg2: the responder's SYN, 52 B: e ‖ enc(nonce) ‖ tag)
//! ```
//!
//! **Roles.** The *initiator* is the client (Studio, lp-cli, an app); it holds
//! a key: a [`KeyId`] (the access entry's 16-byte salt, sent in the clear)
//! and a [`Psk`]. The *responder* is the device; it holds no key up front and
//! asks its edge which PSKs go with the key id it was sent
//! ([`SecureEvent::KeyLookup`]). A handshake that completes is the login: the
//! edge grants the tier of the candidate that matched.
//!
//! **What is standard.** The handshake is exactly Noise's NNpsk0 (spec rev 34,
//! §5 and §9), checked against `snow` in both roles (`tests/secure_oracle_snow.rs`).
//! The prologue binds the key id and the initiator's lp-link nonce
//! ([`prologue`]); msg1's payload is empty; msg2's payload is the responder's
//! lp-link nonce, so one Noise session is exactly one lp-link session. After
//! `Split()`, frames are sealed per transmission with an **explicit** 32-bit
//! counter carried in the frame, the frame header as associated data (spec
//! §11.4, out-of-order transport messages), so ARQ's resends and reordering
//! never desynchronise a nonce.
//!
//! **What is ours, and what is not.** HMAC-SHA256 ([`hmac_sha256`]), Noise's
//! HKDF ([`hkdf_sha256`]) and the state machine ([`noise_handshake`]) are
//! written from their specs; RustCrypto's `hmac`/`hkdf` and `snow` are
//! dev-dependency oracles only. Curve25519 (x25519-dalek, no precomputed
//! tables) and ChaCha20-Poly1305 are RustCrypto's. Nothing here draws
//! randomness or reads a clock: ephemeral secrets arrive as bytes from the
//! edge's entropy source.

pub mod cipher_state;
pub mod hkdf_sha256;
pub mod hmac_sha256;
mod key_id;
pub mod noise_handshake;
mod psk;
pub mod replay_window;
mod secure_event;
mod secure_role;

pub use key_id::KeyId;
pub use noise_handshake::{
    HandshakeError, Initiator, MSG1_LEN, MSG2_LEN, MSG2_PAYLOAD_LEN, PROLOGUE_LEN, Responder,
    ResponderReady, TransportKeys, prologue,
};
pub use psk::Psk;
pub use replay_window::{ReplayVerdict, ReplayWindow};
pub use secure_event::{RefusalReason, SecureEvent, SessionAuth};
pub use secure_role::SecureRole;

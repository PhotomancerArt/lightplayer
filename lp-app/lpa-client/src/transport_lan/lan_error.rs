//! Why a LAN link did not open, or stopped, in words a person can act on.

use std::fmt;

use lpc_wire::lp_link::secure_channel::RefusalReason;

/// The words a locked board's refusal is given in (the caller that knows
/// its own flags may say it differently: see [`LanError::Locked`]).
pub const LOCKED_WORDS: &str =
    "this board is locked: give its password with --password-stdin or LP_PASSWORD";

/// What went wrong on a LAN link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanError {
    /// No WebSocket to the board: the name did not resolve, nothing
    /// answered, or the upgrade was refused.
    Connect { target: String, detail: String },
    /// Every session the board can hold is in use (a board on the LAN
    /// closed this one with WebSocket close 1013, the relay with 4429).
    Busy { target: String },
    /// The board closed the link (`code`: its WebSocket close code, if it
    /// sent one).
    Closed { code: Option<u16> },
    /// The connection failed under the link.
    Lost(String),
    /// The secure session did not come up, or its hello did not arrive, in
    /// time.
    NoHello { target: String, secs: u64 },
    /// The board grants nothing without a password, and none was given.
    Locked,
    /// No password entry on the board takes the password given.
    WrongPassword,
    /// The board has no password to log in with (only keys held by
    /// browsers or accounts).
    NoPasswordEntry,
    /// Too many wrong passwords lately: the board refuses every key until
    /// `retry_after_ms` has passed.
    Backoff { retry_after_ms: u32 },
    /// The board refused this link's key for another reason.
    Refused(RefusalReason),
    /// The board runs a plain lp-link, not a secure one (a LAN link is
    /// always secure; there is no downgrade).
    NotSecure,
    /// The board answered the login with something other than a challenge.
    Login(String),
}

impl fmt::Display for LanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect { target, detail } => write!(f, "could not reach {target}: {detail}"),
            Self::Busy { target } => write!(
                f,
                "{target}: busy with another connection — try again later"
            ),
            Self::Closed { code: Some(code) } => {
                match lpc_relay::RelayCloseCode::from_code(*code) {
                    // The relay's own refusals, in its words.
                    Some(relay) if *code >= 4000 => f.write_str(relay.words()),
                    _ => write!(f, "the board closed the link (WebSocket close {code})"),
                }
            }
            Self::Closed { code: None } => f.write_str("the board closed the link"),
            Self::Lost(detail) => write!(f, "the LAN link was lost: {detail}"),
            Self::NoHello { target, secs } => {
                write!(f, "{target}: no secure session and hello within {secs} s")
            }
            Self::Locked => f.write_str(LOCKED_WORDS),
            Self::WrongPassword => f.write_str("the board refused that password"),
            Self::NoPasswordEntry => f.write_str(
                "this board has no password to log in with (only keys held by browsers); \
                 add one over USB or from Studio",
            ),
            Self::Backoff { retry_after_ms } => write!(
                f,
                "the board is refusing passwords for {:.1} s after too many wrong ones; \
                 try again then",
                f64::from(*retry_after_ms) / 1000.0
            ),
            Self::Refused(reason) => {
                write!(f, "the board refused this link: {}", reason_words(*reason))
            }
            Self::NotSecure => {
                f.write_str("the board runs a plain link, and a LAN link is only ever secure")
            }
            Self::Login(detail) => write!(f, "the board did not start a login: {detail}"),
        }
    }
}

impl std::error::Error for LanError {}

/// A refusal's reason in words.
pub fn reason_words(reason: RefusalReason) -> &'static str {
    match reason {
        RefusalReason::UnknownKey => "it holds no such key",
        RefusalReason::WrongKey => "the key did not match",
        RefusalReason::Backoff => "too many wrong keys lately",
        RefusalReason::Busy => "it did not answer the key lookup in time",
    }
}

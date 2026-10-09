//! Core-only's own answer to a secure link's key lookup (OTA Wi-Fi plan
//! WD1): the engine's rule (`lpa-server`'s `AccessState::key_lookup`),
//! answered from the device store's secrets the session already holds
//! ([`AccessFacts::secrets`](super::AccessFacts)), because in core-only no
//! server runs to ask.
//!
//! A LAN link is a secure lp-link responder: its handshake names a key id
//! (the entry's salt, in the clear), the board answers with the candidate
//! PSKs, and the key that verifies decides the link's tier. Here:
//!
//! - **the anonymous key** (zero salt) is answered with the zero PSK and
//!   grants nothing: the link comes up [`LinkTrust::Untrusted`] and the
//!   device's `open` decides, through the one access rule — **on the LAN
//!   only**. Through the relay ([`NetworkPath::Relay`]) it is refused like
//!   an unknown key (the relay's second lock): "Anyone" never applies there,
//!   so a relayed link always holds a key;
//! - **a known salt** is answered with `lpc_access::key_candidates` over the
//!   store's secrets, best tier first; the candidate that verifies brings
//!   the link up [`LinkTrust::Keyed`] at its tier ([`BoardSession::key_authenticated`]),
//!   or [`LinkTrust::Relayed`] through the relay;
//! - **an unknown salt** tested no secret: refused, not charged;
//! - **a wrong guess at a known salt** is charged to the session's login
//!   backoff ([`BoardSession::key_wrong`]), and while it lasts every lookup
//!   is refused with its wait, before anything is read. It is the backoff
//!   the core's channel-3 `L` login keeps (`core_login`): one board, one
//!   backoff, whichever way a guess arrives.
//!
//! Nothing here logs; a PSK never leaves the answer it is in.

use alloc::vec::Vec;

use lpc_access::{SALT_BYTES, Tier, key_candidates};

use super::board_link::{LinkId, LinkTrust, NetworkPath};
use super::board_session::BoardSession;

/// What core-only tells a secure link's handshake.
#[derive(Clone, PartialEq, Eq)]
pub enum CoreKeyAnswer {
    /// The candidate PSKs, best tier first.
    Keys(Vec<[u8; 32]>),
    /// No entry has this salt (not a strike against the backoff).
    Unknown,
    /// The board is in login backoff: try again after `retry_after_ms`.
    Backoff { retry_after_ms: u32 },
}

impl core::fmt::Debug for CoreKeyAnswer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            // Never print a PSK.
            Self::Keys(keys) => write!(f, "Keys({} candidates)", keys.len()),
            Self::Unknown => f.write_str("Unknown"),
            Self::Backoff { retry_after_ms } => write!(f, "Backoff({retry_after_ms} ms)"),
        }
    }
}

impl BoardSession {
    /// `link`'s handshake, which reached the board by `path`, named the
    /// entry with `salt`: its candidate PSKs, or why there are none (see the
    /// module docs).
    pub fn key_lookup(
        &mut self,
        now_ms: u64,
        link: LinkId,
        path: NetworkPath,
        salt: &[u8; SALT_BYTES],
    ) -> CoreKeyAnswer {
        let backoff = self.login.rate_limit().retry_after_ms(now_ms);
        if backoff > 0 {
            return CoreKeyAnswer::Backoff {
                retry_after_ms: u32::try_from(backoff).unwrap_or(u32::MAX),
            };
        }
        self.key_lookups.retain(|(l, _)| *l != link);
        if *salt == [0; SALT_BYTES] {
            if path == NetworkPath::Relay {
                return CoreKeyAnswer::Unknown;
            }
            self.key_lookups.push((link, alloc::vec![None]));
            return CoreKeyAnswer::Keys(alloc::vec![[0; 32]]);
        }
        let candidates = key_candidates(&self.access.secrets, salt);
        if candidates.is_empty() {
            return CoreKeyAnswer::Unknown;
        }
        self.key_lookups
            .push((link, candidates.iter().map(|c| Some(c.tier)).collect()));
        CoreKeyAnswer::Keys(candidates.iter().map(|c| c.psk).collect())
    }

    /// `link`'s handshake matched no candidate of a known salt: a failed
    /// guess, charged to the board's one backoff.
    pub fn key_wrong(&mut self, now_ms: u64, link: LinkId) {
        self.key_lookups.retain(|(l, _)| *l != link);
        self.login.rate_limit_mut().record_failure(now_ms);
    }

    /// `link` (which reached the board by `path`) came up on `candidate` of
    /// its lookup: how the session trusts it — keyed at that candidate's
    /// tier, or untrusted on the anonymous key (and on a candidate it never
    /// offered); through the relay, relayed at that tier (no tier on a
    /// candidate it never offered). A real key clears the backoff, as a
    /// login does.
    pub fn key_authenticated(
        &mut self,
        link: LinkId,
        path: NetworkPath,
        candidate: u8,
    ) -> LinkTrust {
        let tier: Option<Tier> = self
            .key_lookups
            .iter()
            .position(|(l, _)| *l == link)
            .map(|at| self.key_lookups.swap_remove(at).1)
            .and_then(|tiers| tiers.get(usize::from(candidate)).copied().flatten());
        if tier.is_some() {
            self.login.rate_limit_mut().record_success();
        }
        match (path, tier) {
            (NetworkPath::Relay, tier) => LinkTrust::Relayed(tier),
            (NetworkPath::Lan, Some(tier)) => LinkTrust::Keyed(tier),
            (NetworkPath::Lan, None) => LinkTrust::Untrusted,
        }
    }
}

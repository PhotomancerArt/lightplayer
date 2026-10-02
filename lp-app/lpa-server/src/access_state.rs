//! The server's access state: per-link sessions, the device's one login,
//! its backoff, and the clock and randomness they run on.
//!
//! Sans-IO. Time is the sum of the frame deltas the embedder hands
//! `tick_and_send` (its own uptime clock, supplied at the edge), and the
//! challenge nonce comes from an [`EntropySource`] the embedder installs —
//! the firmware wires the chip's hardware RNG, tests wire fixed bytes. With
//! no source installed a login is refused rather than run on predictable
//! nonces.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;
use hashbrown::HashMap;
use lpc_access::{
    BeginOutcome, LoginMac, LoginOffer, LoginOutcome, LoginState, NONCE_BYTES, SALT_BYTES, Tier,
    key_candidates,
};
use lpc_shared::transport::{KeyAnswer, Link, LinkId, LinkTrust};
use lpc_wire::HelloAuth;
use lpc_wire::server::ServerMsgBody;
use lpfs::LpFs;

use crate::access_store;
use crate::link_session::LinkSession;

/// Fills a buffer with random bytes: the embedder's RNG, injected
/// (`LpServer::set_entropy_source`). A plain `fn`, not a boxed closure: the
/// C6's resident heap is ratcheted to the byte, and an `Rc` here cost 16 B
/// of it for nothing.
pub type EntropySource = fn(&mut [u8]);

/// Everything the access gate remembers between requests.
pub struct AccessState {
    /// Per-link sessions, created on first sight, dropped on close.
    sessions: HashMap<LinkId, LinkSession>,
    /// The device's one login machine and its backoff. Device-scoped: it
    /// survives any link closing, which is what makes the backoff bite a
    /// guesser who reconnects.
    login: LoginState,
    /// The link whose challenge is outstanding. Only it may answer, and its
    /// closing frees the slot at once.
    login_owner: Option<LinkId>,
    /// Milliseconds of frame time since the server started.
    clock_ms: u64,
    /// The device store's `open` flag, cached so an untrusted link's every
    /// request does not read flash. Invalidated by any fs mutation the
    /// server handles (`invalidate_device_store`).
    device_open: Cell<Option<bool>>,
    entropy: Option<EntropySource>,
    /// Secure links: the tier each candidate of a link's last key lookup
    /// grants (`None` for the anonymous key), until its handshake says which
    /// one matched.
    key_lookups: HashMap<LinkId, Vec<Option<Tier>>>,
}

impl AccessState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            login: LoginState::new(),
            login_owner: None,
            clock_ms: 0,
            device_open: Cell::new(None),
            entropy: None,
            key_lookups: HashMap::new(),
        }
    }

    pub fn set_entropy_source(&mut self, source: Option<EntropySource>) {
        self.entropy = source;
    }

    /// Advance the access clock by one frame's delta and let an unanswered
    /// challenge expire.
    pub fn advance_clock(&mut self, delta_ms: u32) {
        self.clock_ms = self.clock_ms.saturating_add(u64::from(delta_ms));
        self.login.expire(self.clock_ms);
        if !self.login.in_flight(self.clock_ms) {
            self.login_owner = None;
        }
    }

    /// Note a link's message: its session exists from here on.
    pub fn see(&mut self, link: Link) {
        self.sessions
            .entry(link.id)
            .or_insert_with(|| LinkSession::new(link.trust));
    }

    /// Forget a closed link: its session, its grant, and its challenge if
    /// it held the device's one login. The backoff stays.
    pub fn close_link(&mut self, link: LinkId) {
        self.sessions.remove(&link);
        self.key_lookups.remove(&link);
        if self.login_owner == Some(link) {
            self.login.cancel();
            self.login_owner = None;
        }
    }

    /// Whether `link` began the device's one outstanding login and its
    /// challenge has not expired: the radio edge holds that link's login
    /// deadline open while this is true, and no longer. An answer, refused
    /// or granted, ends it (`answer_login` clears the owner), and so does
    /// the challenge's expiry (`advance_clock`).
    #[must_use]
    pub fn login_pending(&self, link: LinkId) -> bool {
        self.login_owner == Some(link) && self.login.in_flight(self.clock_ms)
    }

    /// The tier `link` holds now. A trusted link never touches the fs.
    #[inline(never)]
    pub fn tier(&self, link: Link, fs: &dyn LpFs) -> Option<Tier> {
        let session = self
            .sessions
            .get(&link.id)
            .copied()
            .unwrap_or_else(|| LinkSession::new(link.trust));
        match session.trust {
            LinkTrust::Trusted => session.effective_tier(false),
            LinkTrust::Untrusted | LinkTrust::Keyed => match session.granted {
                Some(granted) => Some(granted),
                None => session.effective_tier(self.device_open(fs)),
            },
        }
    }

    /// The hello's access half, for `link`.
    #[inline(never)]
    pub fn hello_auth(&self, link: Link, fs: &dyn LpFs) -> HelloAuth {
        HelloAuth {
            required: link.trust != LinkTrust::Trusted,
            granted: self.tier(link, fs),
        }
    }

    /// Drop the cached `open` flag: the device store may have changed.
    pub fn invalidate_device_store(&self) {
        self.device_open.set(None);
    }

    /// `LoginBegin` on `link`: a challenge over every installed secret, or
    /// the reason there is none.
    ///
    /// Never inlined, like the other login paths: they are rare, and their
    /// temporaries (the installed secrets, the challenge) must not deepen
    /// the frame of the `tick_and_send` every frame runs — the C6's main-task
    /// stack high water is ratcheted.
    #[inline(never)]
    pub fn begin_login<'a>(
        &mut self,
        link: LinkId,
        fs: &dyn LpFs,
        loaded_project_paths: impl IntoIterator<Item = &'a str>,
    ) -> ServerMsgBody {
        let Some(entropy) = self.entropy else {
            return ServerMsgBody::Error {
                error: String::from("login is unavailable: this server has no entropy source"),
            };
        };
        let mut nonce = [0u8; NONCE_BYTES];
        entropy(&mut nonce);
        let secrets = access_store::installed_secrets(fs, loaded_project_paths);

        match self.login.begin(self.clock_ms, nonce, secrets) {
            BeginOutcome::Challenge(challenge) => {
                self.login_owner = Some(link);
                ServerMsgBody::LoginChallenge {
                    nonce: challenge.nonce,
                    offers: challenge.offers,
                }
            }
            BeginOutcome::Refused { retry_after_ms } => {
                ServerMsgBody::LoginResult(LoginOutcome::Refused { retry_after_ms })
            }
        }
    }

    /// `LoginAnswer` on `link`: the verdict, and on success the grant.
    ///
    /// Only the link that began the login may answer it; an answer from any
    /// other link is refused and leaves the challenge standing.
    #[inline(never)]
    pub fn answer_login(&mut self, link: LinkId, macs: &[LoginMac]) -> ServerMsgBody {
        if self.login_owner != Some(link) {
            return ServerMsgBody::LoginResult(LoginOutcome::Refused {
                retry_after_ms: self.login.rate_limit().retry_after_ms(self.clock_ms),
            });
        }
        self.login_owner = None;
        let outcome = self.login.answer(self.clock_ms, macs);
        if let LoginOutcome::Granted { tier, .. } = &outcome
            && let Some(session) = self.sessions.get_mut(&link)
        {
            session.granted = Some(*tier);
        }
        ServerMsgBody::LoginResult(outcome)
    }

    /// `LoginBegin` on a keyed (secure network) link: the offers, so a
    /// typed-password client can derive its key and its salt (the key id it
    /// then handshakes with), and a fresh nonce no one can answer: nothing
    /// registers the device's one login and no slot is taken. A secure link
    /// logs in by handshake only.
    #[inline(never)]
    pub fn offers_for_keyed_link<'a>(
        &self,
        fs: &dyn LpFs,
        loaded_project_paths: impl IntoIterator<Item = &'a str>,
    ) -> ServerMsgBody {
        let Some(entropy) = self.entropy else {
            return ServerMsgBody::Error {
                error: String::from("login is unavailable: this server has no entropy source"),
            };
        };
        let mut nonce = [0u8; NONCE_BYTES];
        entropy(&mut nonce);
        let offers = access_store::installed_secrets(fs, loaded_project_paths)
            .iter()
            .map(|secret| LoginOffer {
                salt: secret.salt,
                iterations: secret.iterations,
            })
            .collect();
        ServerMsgBody::LoginChallenge { nonce, offers }
    }

    /// `LoginAnswer` on a keyed link: refused, always. Its handshake is its
    /// only login, so an HMAC answer can never be relayed through a session
    /// a relay could sit in the middle of. The refusal is the one any
    /// `LoginAnswer` with no challenge outstanding gets (`LoginResult`
    /// refused, no wait), which on a keyed link is always the case:
    /// `LoginBegin` there registers none. No new wire shape, and the
    /// `NotPermitted` reply keeps meaning "your tier does not cover this".
    #[must_use]
    pub fn refuse_keyed_login_answer() -> ServerMsgBody {
        ServerMsgBody::LoginResult(LoginOutcome::Refused { retry_after_ms: 0 })
    }

    /// A secure link's handshake named the entry with `salt`: its candidate
    /// PSKs, best tier first, or why there are none. In backoff the lookup
    /// is refused without reading anything; an unknown salt tested no
    /// secret and is not charged; the anonymous key (zero salt) is answered
    /// with the zero PSK and grants nothing (the device's `open` decides).
    ///
    /// Never inlined: rare, and its temporaries (the installed secrets) must
    /// not deepen `tick_and_send`'s frame (the C6 main-stack ratchet).
    #[inline(never)]
    pub fn key_lookup<'a>(
        &mut self,
        link: LinkId,
        salt: &[u8; SALT_BYTES],
        fs: &dyn LpFs,
        loaded_project_paths: impl IntoIterator<Item = &'a str>,
    ) -> KeyAnswer {
        let backoff = self.login.rate_limit().retry_after_ms(self.clock_ms);
        if backoff > 0 {
            return KeyAnswer::Backoff {
                retry_after_ms: u32::try_from(backoff).unwrap_or(u32::MAX),
            };
        }
        if *salt == [0; SALT_BYTES] {
            self.key_lookups.insert(link, alloc::vec![None]);
            return KeyAnswer::Keys(alloc::vec![[0; 32]]);
        }
        let installed = access_store::installed_secrets(fs, loaded_project_paths);
        let candidates = key_candidates(&installed, salt);
        if candidates.is_empty() {
            self.key_lookups.remove(&link);
            return KeyAnswer::Unknown;
        }
        self.key_lookups
            .insert(link, candidates.iter().map(|c| Some(c.tier)).collect());
        KeyAnswer::Keys(candidates.iter().map(|c| c.psk).collect())
    }

    /// A secure link's handshake matched no candidate of a known salt: a
    /// failed guess, charged to the device's backoff like a wrong login.
    pub fn key_wrong(&mut self, link: LinkId) {
        self.key_lookups.remove(&link);
        let now = self.clock_ms;
        self.login.rate_limit_mut().record_failure(now);
    }

    /// A secure link came up on `candidate` of its lookup: the link's grant
    /// is that candidate's tier. A real key clears the backoff, as a login
    /// does; the anonymous key grants nothing and clears nothing.
    pub fn key_authenticated(&mut self, link: LinkId, candidate: u8) {
        let tier = self
            .key_lookups
            .remove(&link)
            .and_then(|tiers| tiers.get(usize::from(candidate)).copied().flatten());
        let session = self
            .sessions
            .entry(link)
            .or_insert_with(|| LinkSession::new(LinkTrust::Keyed));
        session.granted = tier;
        if tier.is_some() {
            self.login.rate_limit_mut().record_success();
        }
    }

    /// The access clock, for tests and logs.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        self.clock_ms
    }

    fn device_open(&self, fs: &dyn LpFs) -> bool {
        if let Some(open) = self.device_open.get() {
            return open;
        }
        let open = access_store::read_device_store(fs).open;
        self.device_open.set(Some(open));
        open
    }
}

impl Default for AccessState {
    fn default() -> Self {
        Self::new()
    }
}

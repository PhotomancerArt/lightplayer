//! The core-side login (D3a): `lpc_access::LoginState`, unchanged, carried
//! over channel 3's `L` messages, in core-only.
//!
//! - **Step 0** (host: begin) mints a challenge from 32 injected random
//!   bytes ([`SessionConfig::entropy`](super::SessionConfig)) and the
//!   store's secrets ([`AccessFacts`](super::AccessFacts));
//! - **step 1** (board) sends the nonce and the offers (salt and iterations,
//!   never a label);
//! - **step 2** (host) answers one MAC per offer, which `LoginState` checks;
//! - **step 3** (board) sends the verdict: the tier granted, or none with
//!   `retry_after_ms`.
//!
//! One login is in flight per device, owned by the link that began it (the
//! engine's rule): a second begin is refused with the time left, and an
//! answer from any other link is refused without touching the challenge.
//! Backoff is `LoginState`'s own `RateLimit`. A link going down frees its
//! challenge. **The granted tier belongs to that link only**, for this
//! session: nothing persists, so a reset needs a new login (a host logs in
//! again whenever it sees `N`/`A`). With no entropy every login is refused,
//! as in `lpa-server`.

use alloc::vec::Vec;

use lpc_access::{BeginOutcome, LoginMac, LoginOutcome, NONCE_BYTES};

use crate::login_step::{BoardLoginStep, HostLoginStep, tier_code};

use super::board_link::LinkId;
use super::board_session::BoardSession;

impl BoardSession {
    pub(super) fn on_login(&mut self, now_ms: u64, link: LinkId, step: HostLoginStep) {
        let (tier, wait) = match step {
            HostLoginStep::Begin => match self.begin_login(now_ms, link) {
                Ok(challenge) => return self.send(link, challenge.encode()),
                Err(wait) => (None, wait),
            },
            HostLoginStep::Answer { .. } if self.login_link != Some(link) => {
                (None, self.login.rate_limit().retry_after_ms(now_ms))
            }
            HostLoginStep::Answer { macs } => {
                self.login_link = None;
                let macs: Vec<LoginMac> = macs.into_iter().map(LoginMac).collect();
                match self.login.answer(now_ms, &macs) {
                    LoginOutcome::Granted { tier, .. } => {
                        if let Some(l) = self.links.iter_mut().find(|l| l.id == link) {
                            l.granted = l.granted.max(Some(tier));
                        }
                        (Some(tier), 0)
                    }
                    LoginOutcome::Refused { retry_after_ms } => (None, retry_after_ms),
                }
            }
        };
        let verdict = BoardLoginStep::Verdict {
            tier_code: tier_code(tier),
            retry_after_ms: u32::try_from(wait).unwrap_or(u32::MAX),
        };
        self.send(link, verdict.encode());
    }

    /// Step 0: a challenge, or how long to wait (0 with no entropy).
    fn begin_login(&mut self, now_ms: u64, link: LinkId) -> Result<BoardLoginStep, u64> {
        let entropy = self.config.entropy.ok_or(0u64)?;
        let mut nonce = [0u8; NONCE_BYTES];
        entropy(&mut nonce);
        match self.login.begin(now_ms, nonce, self.access.secrets.clone()) {
            BeginOutcome::Challenge(c) => {
                self.login_link = Some(link);
                Ok(BoardLoginStep::Challenge {
                    nonce: c.nonce,
                    offers: c.offers,
                })
            }
            BeginOutcome::Refused { retry_after_ms } => Err(retry_after_ms),
        }
    }
}

//! The per-device login machine: challenge, answer, verdict.
//!
//! Two messages make a login (PQ4):
//!
//! 1. **Begin.** The board mints a challenge from caller-supplied random
//!    bytes and offers every installed secret's `(salt, iterations)` —
//!    **without labels**, so "camp" and "mine" stay private before login.
//! 2. **Answer.** The client, which does not know which secret its password
//!    belongs to, derives `K_i` for every offer and answers one MAC per
//!    offer, in order: `HMAC-SHA256(K_i, nonce)`. The board checks every
//!    entry (constant-time compare, no early exit), grants the HIGHEST tier
//!    among the entries that verify, and names that secret's label.
//!
//! One login is in flight per device (D12): a second begin while a
//! challenge is outstanding is refused. A challenge is single-use and
//! expires [`CHALLENGE_TTL_MS`] after it was minted. Failures feed the
//! per-device [`RateLimit`], which also gates begin.
//!
//! Sans-IO: time is a caller-supplied monotonic millisecond count and the
//! nonce is caller-supplied randomness. This module reads no clock and
//! draws no random numbers.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::constant_time_eq::constant_time_eq;
use crate::hmac_sha256::hmac_sha256;
use crate::rate_limit::RateLimit;
use crate::secret_entry::{SALT_BYTES, SecretEntry};
use crate::tier::Tier;

/// How long a challenge stays answerable, in milliseconds. Long enough for
/// a phone to run a few KDFs (~150 ms each); a client should begin a login
/// only once it holds the password, not before prompting for it.
pub const CHALLENGE_TTL_MS: u64 = 30_000;

/// Length of a challenge nonce in bytes.
pub const NONCE_BYTES: usize = 32;

/// The salt and cost of one installed secret — what a client needs to
/// derive the key it would answer with. Never carries the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginOffer {
    /// PBKDF2 salt, base64.
    #[serde(with = "crate::base64_bytes")]
    pub salt: [u8; SALT_BYTES],
    /// PBKDF2 iteration count.
    pub iterations: u32,
}

/// One answer entry: `HMAC-SHA256(K, nonce)`, base64 on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginMac(#[serde(with = "crate::base64_bytes")] pub [u8; 32]);

impl LoginMac {
    /// The MAC a holder of `k` answers `nonce` with.
    #[must_use]
    pub fn compute(k: &[u8; 32], nonce: &[u8; NONCE_BYTES]) -> Self {
        Self(hmac_sha256(k, nonce))
    }
}

/// A fresh challenge: the nonce to MAC and the offers to answer, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub nonce: [u8; NONCE_BYTES],
    pub offers: Vec<LoginOffer>,
}

/// What [`LoginState::begin`] decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginOutcome {
    /// A challenge was minted; answer it with [`LoginState::answer`].
    Challenge(Challenge),
    /// No challenge now: a login is already in flight, or the device is in
    /// backoff. Try again after `retry_after_ms`.
    Refused { retry_after_ms: u64 },
}

/// The verdict on a login answer. This is also the wire's `LoginResult`
/// payload, so it carries serde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoginOutcome {
    /// A secret verified: the link now holds `tier`, via the secret named
    /// `label`.
    #[serde(rename_all = "camelCase")]
    Granted { tier: Tier, label: String },
    /// Nothing verified, or there was no answerable challenge. Wait
    /// `retry_after_ms` (possibly 0) before beginning again.
    #[serde(rename_all = "camelCase")]
    Refused { retry_after_ms: u64 },
}

/// The per-device login machine (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct LoginState {
    pending: Option<PendingLogin>,
    rate_limit: RateLimit,
}

/// The one outstanding challenge, and the secrets it offered — a snapshot,
/// so an answer is checked against exactly what was offered even if the
/// installed set changes in between.
#[derive(Debug, Clone)]
struct PendingLogin {
    nonce: [u8; NONCE_BYTES],
    secrets: Vec<SecretEntry>,
    expires_at_ms: u64,
}

impl LoginState {
    /// No login in flight, no failures.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a login at `now_ms`, offering `secrets`, with `random` as the
    /// nonce. Refused while another challenge is outstanding or while the
    /// device is in backoff.
    pub fn begin(
        &mut self,
        now_ms: u64,
        random: [u8; NONCE_BYTES],
        secrets: Vec<SecretEntry>,
    ) -> BeginOutcome {
        self.expire(now_ms);

        let backoff = self.rate_limit.retry_after_ms(now_ms);
        if backoff > 0 {
            return BeginOutcome::Refused {
                retry_after_ms: backoff,
            };
        }
        if let Some(pending) = &self.pending {
            return BeginOutcome::Refused {
                retry_after_ms: pending.expires_at_ms.saturating_sub(now_ms),
            };
        }

        let offers = secrets
            .iter()
            .map(|secret| LoginOffer {
                salt: secret.salt,
                iterations: secret.iterations,
            })
            .collect();
        self.pending = Some(PendingLogin {
            nonce: random,
            secrets,
            expires_at_ms: now_ms.saturating_add(CHALLENGE_TTL_MS),
        });
        BeginOutcome::Challenge(Challenge {
            nonce: random,
            offers,
        })
    }

    /// Answer the outstanding challenge at `now_ms` with one MAC per offer.
    ///
    /// The challenge is consumed whatever the verdict, so it can never be
    /// answered twice. A wrong answer (including the wrong NUMBER of MACs)
    /// is a failure and feeds the backoff; answering with no challenge
    /// outstanding, or after it expired, is refused without counting as a
    /// guess, because no secret was tested.
    pub fn answer(&mut self, now_ms: u64, macs: &[LoginMac]) -> LoginOutcome {
        let Some(pending) = self.pending.take() else {
            return self.refused(now_ms);
        };
        if now_ms >= pending.expires_at_ms {
            return self.refused(now_ms);
        }

        if macs.len() != pending.secrets.len() {
            let wait = self.rate_limit.record_failure(now_ms);
            return LoginOutcome::Refused {
                retry_after_ms: wait,
            };
        }

        // Check EVERY entry: no early exit, so the time taken does not say
        // which entry matched.
        let mut best: Option<&SecretEntry> = None;
        for (secret, mac) in pending.secrets.iter().zip(macs) {
            let expected = hmac_sha256(&secret.k, &pending.nonce);
            let verified = constant_time_eq(&expected, &mac.0);
            if verified && best.is_none_or(|held| secret.tier > held.tier) {
                best = Some(secret);
            }
        }

        match best {
            Some(secret) => {
                self.rate_limit.record_success();
                LoginOutcome::Granted {
                    tier: secret.tier,
                    label: secret.label.clone(),
                }
            }
            None => {
                let wait = self.rate_limit.record_failure(now_ms);
                LoginOutcome::Refused {
                    retry_after_ms: wait,
                }
            }
        }
    }

    /// Drop the outstanding challenge if it has expired by `now_ms`.
    pub fn expire(&mut self, now_ms: u64) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| now_ms >= pending.expires_at_ms)
        {
            self.pending = None;
        }
    }

    /// Drop the outstanding challenge now (its link went away). The
    /// backoff is untouched: it belongs to the device, not the link.
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// Whether a challenge is outstanding at `now_ms`.
    #[must_use]
    pub fn in_flight(&self, now_ms: u64) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| now_ms < pending.expires_at_ms)
    }

    /// The device's backoff, for inspection.
    #[must_use]
    pub fn rate_limit(&self) -> &RateLimit {
        &self.rate_limit
    }

    fn refused(&self, now_ms: u64) -> LoginOutcome {
        LoginOutcome::Refused {
            retry_after_ms: self.rate_limit.retry_after_ms(now_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::FREE_ATTEMPTS;
    use alloc::vec;

    #[test]
    fn a_good_password_grants_its_tier_and_label() {
        let mut state = LoginState::new();
        let secrets = vec![secret("camp", Tier::Play, b"s'mores", 1)];
        let challenge = expect_challenge(state.begin(0, [5; 32], secrets));
        assert_eq!(challenge.offers.len(), 1);

        let macs = answer_with(b"s'mores", &challenge);
        assert_eq!(
            state.answer(10, &macs),
            LoginOutcome::Granted {
                tier: Tier::Play,
                label: "camp".into()
            }
        );
    }

    #[test]
    fn offers_carry_salt_and_cost_but_no_label() {
        let mut state = LoginState::new();
        let secrets = vec![
            secret("camp", Tier::Play, b"a", 1),
            secret("mine", Tier::Edit, b"b", 2),
        ];
        let challenge = expect_challenge(state.begin(0, [1; 32], secrets.clone()));
        assert_eq!(
            challenge.offers,
            vec![
                LoginOffer {
                    salt: secrets[0].salt,
                    iterations: 1
                },
                LoginOffer {
                    salt: secrets[1].salt,
                    iterations: 2
                },
            ]
        );
    }

    #[test]
    fn a_bad_password_is_refused_and_counts() {
        let mut state = LoginState::new();
        for attempt in 1..=FREE_ATTEMPTS + 1 {
            let challenge = expect_challenge(state.begin(
                0,
                [7; 32],
                vec![secret("mine", Tier::Edit, b"right", 1)],
            ));
            let verdict = state.answer(0, &answer_with(b"wrong", &challenge));
            let expected_wait = if attempt <= FREE_ATTEMPTS { 0 } else { 2_000 };
            assert_eq!(
                verdict,
                LoginOutcome::Refused {
                    retry_after_ms: expected_wait
                },
                "attempt {attempt}"
            );
        }
        // In backoff, even beginning is refused.
        assert_eq!(
            state.begin(1_000, [7; 32], vec![]),
            BeginOutcome::Refused {
                retry_after_ms: 1_000
            }
        );
        assert!(matches!(
            state.begin(2_000, [7; 32], vec![]),
            BeginOutcome::Challenge(_)
        ));
    }

    #[test]
    fn the_highest_matching_tier_wins() {
        // Both secrets share one password: the edit one must win whatever
        // the order.
        for edit_first in [false, true] {
            let play = secret("camp", Tier::Play, b"same", 1);
            let edit = secret("mine", Tier::Edit, b"same", 1);
            let secrets = if edit_first {
                vec![edit, play]
            } else {
                vec![play, edit]
            };
            let mut state = LoginState::new();
            let challenge = expect_challenge(state.begin(0, [2; 32], secrets));
            assert_eq!(
                state.answer(0, &answer_with(b"same", &challenge)),
                LoginOutcome::Granted {
                    tier: Tier::Edit,
                    label: "mine".into()
                }
            );
        }
    }

    #[test]
    fn a_play_and_an_edit_secret_with_the_edit_password_grants_edit() {
        let mut state = LoginState::new();
        let secrets = vec![
            secret("camp", Tier::Play, b"s'mores", 1),
            secret("mine", Tier::Edit, b"hunter2", 1),
        ];
        let challenge = expect_challenge(state.begin(0, [4; 32], secrets));
        assert_eq!(
            state.answer(0, &answer_with(b"hunter2", &challenge)),
            LoginOutcome::Granted {
                tier: Tier::Edit,
                label: "mine".into()
            }
        );
    }

    #[test]
    fn a_challenge_answers_once() {
        let mut state = LoginState::new();
        let secrets = vec![secret("camp", Tier::Play, b"pw", 1)];
        let challenge = expect_challenge(state.begin(0, [9; 32], secrets));
        let macs = answer_with(b"pw", &challenge);
        assert!(matches!(
            state.answer(0, &macs),
            LoginOutcome::Granted { .. }
        ));
        // The replay finds nothing to answer, and it is not a guess.
        assert_eq!(
            state.answer(0, &macs),
            LoginOutcome::Refused { retry_after_ms: 0 }
        );
        assert_eq!(state.rate_limit().failures(), 0);
    }

    #[test]
    fn an_expired_challenge_is_refused() {
        let mut state = LoginState::new();
        let secrets = vec![secret("camp", Tier::Play, b"pw", 1)];
        let challenge = expect_challenge(state.begin(1_000, [9; 32], secrets));
        let macs = answer_with(b"pw", &challenge);
        assert_eq!(
            state.answer(1_000 + CHALLENGE_TTL_MS, &macs),
            LoginOutcome::Refused { retry_after_ms: 0 }
        );
        assert!(!state.in_flight(1_000 + CHALLENGE_TTL_MS));
    }

    #[test]
    fn one_login_in_flight() {
        let mut state = LoginState::new();
        expect_challenge(state.begin(0, [1; 32], vec![]));
        assert!(state.in_flight(0));
        assert_eq!(
            state.begin(5_000, [2; 32], vec![]),
            BeginOutcome::Refused {
                retry_after_ms: CHALLENGE_TTL_MS - 5_000
            }
        );
        // Once it expires, a new one may begin…
        assert!(matches!(
            state.begin(CHALLENGE_TTL_MS, [3; 32], vec![]),
            BeginOutcome::Challenge(_)
        ));
        // …and a cancel (the link closed) frees the slot at once.
        state.cancel();
        assert!(matches!(
            state.begin(CHALLENGE_TTL_MS, [4; 32], vec![]),
            BeginOutcome::Challenge(_)
        ));
    }

    #[test]
    fn the_wrong_number_of_macs_is_a_failure() {
        let mut state = LoginState::new();
        let secrets = vec![
            secret("camp", Tier::Play, b"a", 1),
            secret("mine", Tier::Edit, b"b", 1),
        ];
        let challenge = expect_challenge(state.begin(0, [1; 32], secrets));
        let one = LoginMac::compute(
            &crate::pbkdf2_sha256::derive_login_key(b"a", &challenge.offers[0].salt, 1),
            &challenge.nonce,
        );
        assert!(matches!(
            state.answer(0, &[one]),
            LoginOutcome::Refused { .. }
        ));
        assert_eq!(state.rate_limit().failures(), 1);
    }

    #[test]
    fn outcomes_serialize_externally_tagged() {
        let granted = LoginOutcome::Granted {
            tier: Tier::Edit,
            label: "mine".into(),
        };
        assert_eq!(
            serde_json::to_string(&granted).unwrap(),
            "{\"granted\":{\"tier\":\"edit\",\"label\":\"mine\"}}"
        );
        let refused = LoginOutcome::Refused {
            retry_after_ms: 2000,
        };
        assert_eq!(
            serde_json::to_string(&refused).unwrap(),
            "{\"refused\":{\"retryAfterMs\":2000}}"
        );
    }

    /// Distinct salts per label, so two secrets never share a key by
    /// accident of the fixture.
    fn secret(label: &str, tier: Tier, password: &[u8], iterations: u32) -> SecretEntry {
        let mut salt = [0u8; SALT_BYTES];
        for (index, byte) in label.bytes().enumerate().take(SALT_BYTES) {
            salt[index] = byte;
        }
        SecretEntry::from_password(label, tier, password, salt, iterations)
    }

    /// What a client holding `password` answers: one MAC per offer.
    fn answer_with(password: &[u8], challenge: &Challenge) -> Vec<LoginMac> {
        challenge
            .offers
            .iter()
            .map(|offer| {
                let k =
                    crate::pbkdf2_sha256::derive_login_key(password, &offer.salt, offer.iterations);
                LoginMac::compute(&k, &challenge.nonce)
            })
            .collect()
    }

    fn expect_challenge(outcome: BeginOutcome) -> Challenge {
        match outcome {
            BeginOutcome::Challenge(challenge) => challenge,
            other => panic!("expected a challenge, got {other:?}"),
        }
    }
}

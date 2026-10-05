//! The login client: answers a core's `L` challenge (D3a).
//!
//! The scheme is `lpc_access`'s, the one Studio's engine login answers
//! (`lpa-studio-core`'s `login_attempt.rs`): for each offer the board makes
//! (salt, iterations), derive `K = lpc_access::derive_login_key(material,
//! salt, iterations)` and answer `HMAC-SHA256(K, nonce)`
//! (`lpc_access::LoginMac::compute`). Nothing here is a second derivation.
//!
//! The crate never stores credentials: the caller passes them each time.
//! **Held keys first** (a browser's or account's key, bound to the salt it
//! was installed with): when the board offers a held key's salt, one answer
//! carries it, and every other offer gets zeros. **Then passwords, in
//! order**: each answers every offer, one password per challenge; after a
//! refusal the next begin tries the next password.

use alloc::vec::Vec;

use lpc_access::{LoginMac, LoginOffer, NONCE_BYTES, SALT_BYTES, Tier, derive_login_key};
use lpc_update::{BoardLoginStep, BoardMessage, HostLoginStep, tier_from_code};

/// Something the host can log in with.
#[derive(Clone, PartialEq, Eq)]
pub enum Credential {
    /// A key installed on the board under `salt` (a browser's or an
    /// account's): its secret material.
    Key {
        salt: [u8; SALT_BYTES],
        material: Vec<u8>,
    },
    /// A typed password.
    Password(Vec<u8>),
}

/// Credentials are login-equivalent: never in a log line.
impl core::fmt::Debug for Credential {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Key { .. } => f.write_str("Credential::Key(<redacted>)"),
            Self::Password(_) => f.write_str("Credential::Password(<redacted>)"),
        }
    }
}

/// What a board's login step means to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginEvent {
    /// Send this (an answer, or a new begin).
    Send(Vec<u8>),
    /// The link now holds `tier`.
    Granted(Tier),
    /// Refused; wait `retry_after_ms` before beginning again. `exhausted`
    /// when every credential has been tried.
    Refused {
        retry_after_ms: u32,
        exhausted: bool,
    },
    /// Nothing the caller holds matches what the board offers.
    NothingToAnswer,
}

/// One link's login. See the module docs.
#[derive(Clone, Debug, Default)]
pub struct LoginClient {
    /// The next password to try.
    next_password: usize,
    /// Whether the challenge being answered used a password.
    answered_with_password: bool,
}

impl LoginClient {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `L 0`.
    #[must_use]
    pub fn begin(&self) -> Vec<u8> {
        HostLoginStep::Begin.encode()
    }

    /// One board message (anything but an `L` is ignored: `None`).
    pub fn on_board(&mut self, bytes: &[u8], credentials: &[Credential]) -> Option<LoginEvent> {
        let Ok(BoardMessage::Login(step)) = BoardMessage::decode(bytes) else {
            return None;
        };
        Some(match step {
            BoardLoginStep::Challenge { nonce, offers } => {
                self.answer(&nonce, &offers, credentials)
            }
            BoardLoginStep::Verdict {
                tier_code,
                retry_after_ms,
            } => match tier_from_code(tier_code) {
                Some(tier) => {
                    self.next_password = 0;
                    LoginEvent::Granted(tier)
                }
                None => {
                    if self.answered_with_password {
                        self.next_password += 1;
                    }
                    let passwords = credentials
                        .iter()
                        .filter(|c| matches!(c, Credential::Password(_)))
                        .count();
                    LoginEvent::Refused {
                        retry_after_ms,
                        exhausted: self.next_password >= passwords,
                    }
                }
            },
        })
    }

    fn answer(
        &mut self,
        nonce: &[u8; NONCE_BYTES],
        offers: &[LoginOffer],
        credentials: &[Credential],
    ) -> LoginEvent {
        let held = |offer: &LoginOffer| {
            credentials.iter().find_map(|c| match c {
                Credential::Key { salt, material } if *salt == offer.salt => Some(material),
                _ => None,
            })
        };
        if offers.iter().any(|o| held(o).is_some()) {
            self.answered_with_password = false;
            let macs = offers
                .iter()
                .map(|o| match held(o) {
                    Some(material) => mac(material, o, nonce),
                    None => [0; 32],
                })
                .collect();
            return LoginEvent::Send(HostLoginStep::Answer { macs }.encode());
        }
        let password = credentials
            .iter()
            .filter_map(|c| match c {
                Credential::Password(p) => Some(p),
                Credential::Key { .. } => None,
            })
            .nth(self.next_password);
        let Some(password) = password.filter(|_| !offers.is_empty()) else {
            return LoginEvent::NothingToAnswer;
        };
        self.answered_with_password = true;
        let macs = offers.iter().map(|o| mac(password, o, nonce)).collect();
        LoginEvent::Send(HostLoginStep::Answer { macs }.encode())
    }
}

fn mac(material: &[u8], offer: &LoginOffer, nonce: &[u8; NONCE_BYTES]) -> [u8; 32] {
    LoginMac::compute(
        &derive_login_key(material, &offer.salt, offer.iterations),
        nonce,
    )
    .0
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_access::{BeginOutcome, LoginOutcome, LoginState, SecretEntry};

    fn board_challenge(state: &mut LoginState, secrets: Vec<SecretEntry>) -> Vec<u8> {
        let BeginOutcome::Challenge(c) = state.begin(0, [7; 32], secrets) else {
            panic!("no challenge");
        };
        BoardLoginStep::Challenge {
            nonce: c.nonce,
            offers: c.offers,
        }
        .encode()
    }

    fn answer_macs(bytes: &[u8]) -> Vec<LoginMac> {
        match lpc_update::HostMessage::decode(bytes) {
            Ok(lpc_update::HostMessage::Login(HostLoginStep::Answer { macs })) => {
                macs.into_iter().map(LoginMac).collect()
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_password_answers_a_challenge_lpc_access_grants() {
        let secrets = vec![
            SecretEntry::from_password("other", Tier::Play, b"nope", [1; 16], 8),
            SecretEntry::from_password("mine", Tier::Edit, b"pw", [2; 16], 8),
        ];
        let mut board = LoginState::new();
        let challenge = board_challenge(&mut board, secrets);
        let mut client = LoginClient::new();
        let creds = [Credential::Password(b"pw".to_vec())];
        let Some(LoginEvent::Send(answer)) = client.on_board(&challenge, &creds) else {
            panic!()
        };
        let verdict = board.answer(1, &answer_macs(&answer));
        assert!(matches!(
            verdict,
            LoginOutcome::Granted {
                tier: Tier::Edit,
                ..
            }
        ));
    }

    #[test]
    fn a_held_key_answers_only_its_own_offer() {
        let material = vec![9u8; 32];
        let secrets = vec![
            SecretEntry::from_password("pw", Tier::Edit, b"pw", [1; 16], 4),
            SecretEntry::from_password("browser", Tier::Edit, &material, [3; 16], 4),
        ];
        let mut board = LoginState::new();
        let challenge = board_challenge(&mut board, secrets);
        let creds = [
            Credential::Password(b"wrong".to_vec()),
            Credential::Key {
                salt: [3; 16],
                material,
            },
        ];
        let Some(LoginEvent::Send(answer)) = LoginClient::new().on_board(&challenge, &creds) else {
            panic!()
        };
        let macs = answer_macs(&answer);
        assert_eq!(
            macs[0].0, [0; 32],
            "the key holder does not answer for others"
        );
        assert!(matches!(
            board.answer(1, &macs),
            LoginOutcome::Granted { .. }
        ));
    }

    #[test]
    fn refusals_move_on_to_the_next_password_and_say_when_none_are_left() {
        let creds = [
            Credential::Password(b"a".to_vec()),
            Credential::Password(b"b".to_vec()),
        ];
        let challenge = BoardLoginStep::Challenge {
            nonce: [1; 32],
            offers: vec![LoginOffer {
                salt: [1; 16],
                iterations: 2,
            }],
        }
        .encode();
        let refused = BoardLoginStep::Verdict {
            tier_code: 0,
            retry_after_ms: 0,
        }
        .encode();
        let mut c = LoginClient::new();
        assert!(matches!(
            c.on_board(&challenge, &creds),
            Some(LoginEvent::Send(_))
        ));
        assert_eq!(
            c.on_board(&refused, &creds),
            Some(LoginEvent::Refused {
                retry_after_ms: 0,
                exhausted: false
            })
        );
        assert!(matches!(
            c.on_board(&challenge, &creds),
            Some(LoginEvent::Send(_))
        ));
        assert_eq!(
            c.on_board(&refused, &creds),
            Some(LoginEvent::Refused {
                retry_after_ms: 0,
                exhausted: true
            })
        );
        assert_eq!(
            c.on_board(&challenge, &creds),
            Some(LoginEvent::NothingToAnswer)
        );
        assert_eq!(
            c.on_board(&challenge, &[]),
            Some(LoginEvent::NothingToAnswer)
        );
        let granted = BoardLoginStep::Verdict {
            tier_code: 2,
            retry_after_ms: 0,
        }
        .encode();
        assert_eq!(
            c.on_board(&granted, &creds),
            Some(LoginEvent::Granted(Tier::Edit))
        );
    }
}

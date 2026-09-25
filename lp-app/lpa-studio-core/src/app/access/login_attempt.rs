//! One login conversation on one link: unlock by salt, then (only if no
//! held key matched) try passwords.
//!
//! The board's protocol (M3, `lpc_access::LoginState`): `LoginBegin` gets a
//! challenge — a fresh nonce and every installed secret's `(salt,
//! iterations)`, no labels — and ONE `LoginAnswer` carries one MAC per offer.
//!
//! Every key this browser holds uses one salt on every device (plan D1), so
//! the challenge itself says which of them the board knows:
//!
//! 1. **An offer whose salt is a held key's** is answered with that key; the
//!    other offers get a zero MAC (the board treats a MAC that does not
//!    verify as not granting). One answer, never a wrong one.
//! 2. **No held key matched:** the passwords handed in are tried in order,
//!    each a begin and an answer, and each wrong one feeds the board's
//!    backoff (three free, then 2 s doubling). How many is the caller's
//!    policy ([`super::AUTO_LOGIN_ATTEMPTS`]).
//! 3. **Nothing matched and no password to try:** no answer is sent at all.
//!    The challenge is handed back ([`LoginAttemptOutcome::NothingMatched`])
//!    so the password the user is about to type can answer it — a begin
//!    that is never answered holds the board's one login slot until it
//!    expires.
//!
//! It respects the board rather than racing it: a refused begin (another
//! login in flight, or the backoff running) and a refused answer both come
//! back with `retry_after_ms`, and a next password waits that long first.

use core::future::Future;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::{ClientIo, LoginBegun, LpClient};
use lpc_access::{Challenge, LoginMac, LoginOutcome, Tier};

use super::key_holder::HeldKey;
use super::login_key_cache::LoginKeyCache;

/// How one login conversation ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginAttemptOutcome {
    /// The board granted `tier` for the secret it calls `label`: to a held
    /// key (`password_index: None`), or to the password at `password_index`
    /// of the list tried.
    Granted {
        tier: Tier,
        label: String,
        password_index: Option<usize>,
    },
    /// Every password was refused; the board's backoff is `retry_after_ms`.
    Refused { retry_after_ms: u64 },
    /// The board would not begin (a login in flight, or its backoff).
    Busy { retry_after_ms: u64 },
    /// The board offers no passwords at all: it can only be reached open
    /// (play) or over USB.
    NoPasswords,
    /// No held key is on the board and there was no password to try: no
    /// answer was sent. The challenge is still the board's open one.
    NothingMatched { challenge: Challenge },
    /// The link failed under the conversation.
    Failed(String),
}

/// Unlock `client`'s link with the keys this browser holds, else the
/// `passwords`, in order. `challenge` is one a previous conversation on
/// this link begun and left unanswered; it is answered first instead of
/// beginning again. `sleep` is the platform timer: it waits out the board's
/// backoff between passwords, and yields to the page (zero-length) between
/// derivations so a many-offer challenge never freezes it.
pub async fn try_login<Io, Sleep, SleepFuture>(
    client: &mut LpClient<Io>,
    held: &[HeldKey],
    passwords: &[String],
    challenge: Option<Challenge>,
    keys: &Rc<RefCell<LoginKeyCache>>,
    mut sleep: Sleep,
) -> LoginAttemptOutcome
where
    Io: ClientIo,
    Sleep: FnMut(Duration) -> SleepFuture,
    SleepFuture: Future<Output = ()>,
{
    let mut challenge = match challenge {
        Some(challenge) => Some(challenge),
        None => match begin(client).await {
            Ok(challenge) => Some(challenge),
            Err(outcome) => return outcome,
        },
    };
    if challenge.as_ref().is_some_and(|c| c.offers.is_empty()) {
        return LoginAttemptOutcome::NoPasswords;
    }

    // 1. A held key the board offers: one answer, never a wrong one.
    if let Some(first) = &challenge
        && first
            .offers
            .iter()
            .any(|offer| held.iter().any(|key| key.salt() == offer.salt))
    {
        let mut macs = Vec::with_capacity(first.offers.len());
        for offer in &first.offers {
            match held.iter().find(|key| key.salt() == offer.salt) {
                Some(key) => {
                    let (k, derived) = keys.borrow_mut().key_for_material(offer, &key.key.material);
                    macs.push(LoginMac::compute(&k, &first.nonce));
                    if derived {
                        sleep(Duration::ZERO).await;
                    }
                }
                None => macs.push(LoginMac([0; 32])),
            }
        }
        return match client.login_answer(macs).await {
            Ok(outcome) => match outcome.value {
                LoginOutcome::Granted { tier, label } => LoginAttemptOutcome::Granted {
                    tier,
                    label,
                    password_index: None,
                },
                LoginOutcome::Refused { retry_after_ms } => {
                    LoginAttemptOutcome::Refused { retry_after_ms }
                }
            },
            Err(error) => LoginAttemptOutcome::Failed(error.to_string()),
        };
    }

    // 3. Nothing to answer with: leave the challenge for a typed password.
    if passwords.is_empty() {
        return match challenge {
            Some(challenge) => LoginAttemptOutcome::NothingMatched { challenge },
            None => LoginAttemptOutcome::NoPasswords,
        };
    }

    // 2. The passwords, in order.
    let mut last_refusal = LoginAttemptOutcome::NoPasswords;
    for (index, password) in passwords.iter().enumerate() {
        let challenge = match challenge.take() {
            Some(challenge) => challenge,
            None => {
                if let LoginAttemptOutcome::Refused { retry_after_ms } = last_refusal
                    && retry_after_ms > 0
                {
                    sleep(Duration::from_millis(retry_after_ms)).await;
                }
                match begin(client).await {
                    Ok(challenge) => challenge,
                    Err(outcome) => return outcome,
                }
            }
        };
        if challenge.offers.is_empty() {
            return LoginAttemptOutcome::NoPasswords;
        }
        let mut macs = Vec::with_capacity(challenge.offers.len());
        for offer in &challenge.offers {
            let (key, derived) = keys.borrow_mut().key_for(offer, password);
            macs.push(LoginMac::compute(&key, &challenge.nonce));
            if derived {
                sleep(Duration::ZERO).await;
            }
        }
        match client.login_answer(macs).await {
            Ok(outcome) => match outcome.value {
                LoginOutcome::Granted { tier, label } => {
                    return LoginAttemptOutcome::Granted {
                        tier,
                        label,
                        password_index: Some(index),
                    };
                }
                LoginOutcome::Refused { retry_after_ms } => {
                    last_refusal = LoginAttemptOutcome::Refused { retry_after_ms };
                }
            },
            Err(error) => return LoginAttemptOutcome::Failed(error.to_string()),
        }
    }
    last_refusal
}

/// `LoginBegin`: the challenge, or how the conversation ends instead.
async fn begin<Io: ClientIo>(client: &mut LpClient<Io>) -> Result<Challenge, LoginAttemptOutcome> {
    match client.login_begin().await {
        Ok(outcome) => match outcome.value {
            LoginBegun::Challenge(challenge) => Ok(challenge),
            LoginBegun::Refused(LoginOutcome::Refused { retry_after_ms }) => {
                Err(LoginAttemptOutcome::Busy { retry_after_ms })
            }
            LoginBegun::Refused(LoginOutcome::Granted { .. }) => Err(LoginAttemptOutcome::Failed(
                "the board answered a login request with a grant".to_string(),
            )),
        },
        Err(error) => Err(LoginAttemptOutcome::Failed(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::key_holder::{InstallableKey, KeyHolder};
    use crate::app::access::test_board::FakeBoard;
    use lpc_access::SecretKind;

    #[test]
    fn a_held_key_the_board_offers_is_one_answer_and_never_a_guess() {
        let key = held("Yona's MacBook", 9);
        let board = FakeBoard::with_entries(vec![
            lpc_access::SecretEntry::from_password("camp", Tier::Play, b"x", [1; 16], 2),
            key.key.entry(1),
        ]);
        let keys = Rc::new(RefCell::new(LoginKeyCache::new()));
        let mut client = board.client();
        let outcome = block_on(try_login(
            &mut client,
            &[held("elsewhere", 3), key],
            &passwords(&["would-be-wrong"]),
            None,
            &keys,
            |_| core::future::ready(()),
        ));
        assert_eq!(
            outcome,
            LoginAttemptOutcome::Granted {
                tier: Tier::Edit,
                label: "Yona's MacBook".to_string(),
                password_index: None
            }
        );
        assert_eq!(board.answers(), 1);
        assert_eq!(board.failures(), 0);
    }

    #[test]
    fn nothing_held_and_nothing_to_try_sends_no_answer_and_hands_back_the_challenge() {
        let board = FakeBoard::locked(&[("camp", Tier::Play, "smores")]);
        let keys = Rc::new(RefCell::new(LoginKeyCache::new()));
        let mut client = board.client();
        let outcome = block_on(try_login(
            &mut client,
            &[held("elsewhere", 3)],
            &[],
            None,
            &keys,
            |_| core::future::ready(()),
        ));
        let LoginAttemptOutcome::NothingMatched { challenge } = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(board.answers(), 0);
        // The open challenge answers a typed password without a new begin.
        let outcome = block_on(try_login(
            &mut client,
            &[],
            &passwords(&["smores"]),
            Some(challenge),
            &keys,
            |_| core::future::ready(()),
        ));
        assert!(
            matches!(
                outcome,
                LoginAttemptOutcome::Granted {
                    password_index: Some(0),
                    ..
                }
            ),
            "{outcome:?}"
        );
    }

    #[test]
    fn with_no_held_key_the_first_password_the_board_verifies_is_granted() {
        let board =
            FakeBoard::locked(&[("camp", Tier::Play, "smores"), ("mine", Tier::Edit, "pw")]);
        let keys = Rc::new(RefCell::new(LoginKeyCache::new()));
        let mut client = board.client();
        let outcome = block_on(try_login(
            &mut client,
            &[],
            &passwords(&["nope", "smores"]),
            None,
            &keys,
            |_| core::future::ready(()),
        ));
        assert_eq!(
            outcome,
            LoginAttemptOutcome::Granted {
                tier: Tier::Play,
                label: "camp".to_string(),
                password_index: Some(1)
            }
        );
        // The real verifier ran: one wrong answer is on the board's count.
        assert_eq!(board.failures(), 0, "a success clears the slate");
    }

    #[test]
    fn every_password_refused_reports_the_boards_backoff_and_waits_it_between() {
        let board = FakeBoard::locked(&[("mine", Tier::Edit, "right")]);
        board.set_failures_before(3);
        let keys = Rc::new(RefCell::new(LoginKeyCache::new()));
        let waited = Rc::new(RefCell::new(Vec::new()));
        let mut client = board.client();
        let outcome = block_on(try_login(
            &mut client,
            &[],
            &passwords(&["a", "b"]),
            None,
            &keys,
            {
                let waited = Rc::clone(&waited);
                move |delay: Duration| {
                    if !delay.is_zero() {
                        waited.borrow_mut().push(delay.as_millis() as u64);
                        board_advance(delay);
                    }
                    core::future::ready(())
                }
            },
        ));
        assert!(
            matches!(outcome, LoginAttemptOutcome::Refused { retry_after_ms } if retry_after_ms > 0),
            "{outcome:?}"
        );
        assert_eq!(
            waited.borrow().len(),
            1,
            "one backoff waited, between the two"
        );
    }

    #[test]
    fn a_board_with_no_passwords_says_so() {
        let board = FakeBoard::locked(&[]);
        let keys = Rc::new(RefCell::new(LoginKeyCache::new()));
        let mut client = board.client();
        let outcome = block_on(try_login(
            &mut client,
            &[],
            &passwords(&["x"]),
            None,
            &keys,
            |_| core::future::ready(()),
        ));
        assert_eq!(outcome, LoginAttemptOutcome::NoPasswords);
    }

    fn passwords(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| (*p).to_string()).collect()
    }

    fn held(label: &str, salt: u8) -> HeldKey {
        HeldKey {
            holder: KeyHolder::Browser,
            key: InstallableKey {
                label: label.to_string(),
                kind: SecretKind::Browser,
                tier: Tier::Edit,
                salt: [salt; 16],
                iterations: 1,
                material: vec![salt; 32],
            },
        }
    }

    /// The fake board's clock is advanced by the waits the attempt makes.
    fn board_advance(delay: Duration) {
        crate::app::access::test_board::advance_clock(delay.as_millis() as u64);
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        crate::app::access::test_board::block_on(future)
    }
}

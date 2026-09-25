//! One login conversation: try passwords, in order, on one link.
//!
//! The board's protocol (M3, `lpc_access::LoginState`): `LoginBegin` gets a
//! challenge — a fresh nonce and every installed secret's `(salt,
//! iterations)`, no labels — and ONE `LoginAnswer` carries one MAC per offer,
//! all under the same password. So each password tried is a begin and an
//! answer, and each wrong one feeds the board's backoff (three free, then
//! 2 s doubling). This runs the passwords it is handed and stops at the first
//! grant; how many it is handed is the caller's policy
//! ([`super::AUTO_LOGIN_ATTEMPTS`]).
//!
//! It respects the board rather than racing it: a refused begin (another
//! login in flight, or the backoff running) and a refused answer both come
//! back with `retry_after_ms`, and a next password waits that long first.

use core::future::Future;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::{ClientIo, LoginBegun, LpClient};
use lpc_access::{LoginMac, LoginOutcome, Tier};

use super::login_key_cache::LoginKeyCache;

/// How one login conversation ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginAttemptOutcome {
    /// The board granted `tier` for the secret it calls `label`, to the
    /// password at `password_index` of the list tried.
    Granted {
        tier: Tier,
        label: String,
        password_index: usize,
    },
    /// Every password was refused; the board's backoff is `retry_after_ms`.
    Refused { retry_after_ms: u64 },
    /// The board would not begin (a login in flight, or its backoff).
    Busy { retry_after_ms: u64 },
    /// The board offers no passwords at all: it can only be reached open
    /// (play) or over USB.
    NoPasswords,
    /// The link failed under the conversation.
    Failed(String),
}

/// Try `passwords`, in order, on `client`'s link. `sleep` is the platform
/// timer: it waits out the board's backoff between passwords, and yields to
/// the page (zero-length) between derivations so a many-offer challenge
/// never freezes it.
pub async fn try_passwords<Io, Sleep, SleepFuture>(
    client: &mut LpClient<Io>,
    passwords: &[String],
    keys: &Rc<RefCell<LoginKeyCache>>,
    mut sleep: Sleep,
) -> LoginAttemptOutcome
where
    Io: ClientIo,
    Sleep: FnMut(Duration) -> SleepFuture,
    SleepFuture: Future<Output = ()>,
{
    let mut last_refusal = LoginAttemptOutcome::NoPasswords;
    for (index, password) in passwords.iter().enumerate() {
        if let LoginAttemptOutcome::Refused { retry_after_ms } = last_refusal
            && retry_after_ms > 0
        {
            sleep(Duration::from_millis(retry_after_ms)).await;
        }
        let challenge = match client.login_begin().await {
            Ok(outcome) => match outcome.value {
                LoginBegun::Challenge(challenge) => challenge,
                LoginBegun::Refused(LoginOutcome::Refused { retry_after_ms }) => {
                    return LoginAttemptOutcome::Busy { retry_after_ms };
                }
                LoginBegun::Refused(LoginOutcome::Granted { .. }) => {
                    return LoginAttemptOutcome::Failed(
                        "the board answered a login request with a grant".to_string(),
                    );
                }
            },
            Err(error) => return LoginAttemptOutcome::Failed(error.to_string()),
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
                        password_index: index,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::test_board::FakeBoard;

    fn passwords(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn the_first_password_the_board_verifies_is_granted() {
        let board =
            FakeBoard::locked(&[("camp", Tier::Play, "smores"), ("mine", Tier::Edit, "pw")]);
        let keys = Rc::new(RefCell::new(LoginKeyCache::new()));
        let mut client = board.client();
        let outcome = block_on(try_passwords(
            &mut client,
            &passwords(&["nope", "smores"]),
            &keys,
            |_| core::future::ready(()),
        ));
        assert_eq!(
            outcome,
            LoginAttemptOutcome::Granted {
                tier: Tier::Play,
                label: "camp".to_string(),
                password_index: 1
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
        let outcome = block_on(try_passwords(
            &mut client,
            &passwords(&["a", "b"]),
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
        let outcome = block_on(try_passwords(
            &mut client,
            &passwords(&["x"]),
            &keys,
            |_| core::future::ready(()),
        ));
        assert_eq!(outcome, LoginAttemptOutcome::NoPasswords);
    }

    /// The fake board's clock is advanced by the waits the attempt makes.
    fn board_advance(delay: Duration) {
        crate::app::access::test_board::advance_clock(delay.as_millis() as u64);
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        crate::app::access::test_board::block_on(future)
    }
}

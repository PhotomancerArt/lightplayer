//! Unlocking a KEYED link (a board on the LAN, Wi-Fi M6 P07): the Bluetooth
//! login's twin, ending in keys for the link instead of an answer.
//!
//! On a secure link the handshake's key IS the login: the board grants the
//! tier of the entry whose key the link presented, and an HMAC
//! `LoginAnswer` never grants there (`lpa-server/tests/secure_link_access.rs`).
//! So when a keyed link came up holding nothing — a locked board, and
//! nothing this browser holds is on it — this conversation does what
//! `login_attempt.rs` does up to the answer, and stops there:
//!
//! 1. `LoginBegin` for the board's offers (its entries' salts and costs; on
//!    a keyed link it takes no login slot), or the challenge a previous
//!    conversation on this window left open.
//! 2. A held key the board offers (an account password, whose cost keeps it
//!    out of the always-presented set): its key for that salt.
//! 3. Else the passwords handed in (the one typed in the Unlock sheet, or a
//!    remembered one), each derived once per offered salt through the same
//!    cache (`login_key_cache.rs`).
//!
//! The keys go to the link's key source ([`NetworkLinkKeys::offer`]), whose
//! new generation moves the link onto them in place (the provider's rekey):
//! the board's new hello says what they granted, and the access session
//! reads it as this login's outcome ([`LoginAttemptOutcome::Rekeyed`]).
//!
//! A password derived for every offered salt is presented once per salt,
//! and every salt it is wrong for is a wrong key on the board's count (three
//! free, then its backoff) — the price of offers that carry no labels. The
//! provider drops each wrong key so it is never presented there again.

use core::future::Future;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::{ClientIo, LpClient};
use lpc_access::Challenge;

use super::key_holder::HeldKey;
use super::login_attempt::{LoginAttemptOutcome, begin};
use super::login_key_cache::LoginKeyCache;
use super::network_link_keys::{NetworkLinkKeys, link_key};

/// Find keys for the keyed link to the board at `address` (see the module
/// docs). `sleep` yields to the page (zero-length) between derivations.
#[allow(
    clippy::too_many_arguments,
    reason = "the login's own inputs, as `try_login` takes them, plus where its keys go"
)]
pub async fn try_keyed_login<Io, Sleep, SleepFuture>(
    client: &mut LpClient<Io>,
    held: &[HeldKey],
    passwords: &[String],
    challenge: Option<Challenge>,
    keys: &Rc<RefCell<LoginKeyCache>>,
    link_keys: &NetworkLinkKeys,
    address: &str,
    mut sleep: Sleep,
) -> LoginAttemptOutcome
where
    Io: ClientIo,
    Sleep: FnMut(Duration) -> SleepFuture,
    SleepFuture: Future<Output = ()>,
{
    let challenge = match challenge {
        Some(challenge) => challenge,
        None => match begin(client).await {
            Ok(challenge) => challenge,
            Err(outcome) => return outcome,
        },
    };
    if challenge.offers.is_empty() {
        return LoginAttemptOutcome::NoPasswords;
    }

    let mut offered = Vec::new();
    for offer in &challenge.offers {
        if let Some(key) = held.iter().find(|key| key.salt() == offer.salt) {
            let (k, derived) = keys.borrow_mut().key_for_material(offer, &key.key.material);
            offered.push(link_key(offer.salt, &k));
            if derived {
                sleep(Duration::ZERO).await;
            }
        }
    }
    if offered.is_empty() {
        for password in passwords {
            for offer in &challenge.offers {
                let (k, derived) = keys.borrow_mut().key_for(offer, password);
                offered.push(link_key(offer.salt, &k));
                if derived {
                    sleep(Duration::ZERO).await;
                }
            }
        }
    }
    if offered.is_empty() {
        return LoginAttemptOutcome::NothingMatched { challenge };
    }
    link_keys.offer(address, offered);
    LoginAttemptOutcome::Rekeyed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::key_holder::{InstallableKey, KeyHolder};
    use crate::app::access::test_board::FakeBoard;
    use lpa_link::providers::network_link::LinkKeys;
    use lpc_access::{SecretKind, Tier};

    const BOARD: &str = "ws://10.0.0.5/link";

    /// A typed password becomes one key per offered salt — the salt as its
    /// id, `link_psk` of the derived key as its PSK — and no answer is sent.
    #[test]
    fn a_typed_password_becomes_a_key_per_offered_salt_and_no_answer() {
        let board =
            FakeBoard::locked(&[("camp", Tier::Play, "smores"), ("mine", Tier::Edit, "pw")]);
        let cache = Rc::new(RefCell::new(LoginKeyCache::new()));
        let link_keys = NetworkLinkKeys::new();
        let mut client = board.client();

        let outcome = block_on(try_keyed_login(
            &mut client,
            &[],
            &["pw".to_string()],
            None,
            &cache,
            &link_keys,
            BOARD,
            |_| core::future::ready(()),
        ));

        assert_eq!(outcome, LoginAttemptOutcome::Rekeyed);
        assert_eq!(board.answers(), 0, "a keyed link is never answered");
        let presented = link_keys.keys_for(BOARD);
        assert_eq!(presented.len(), 2, "one per offered salt");
        let store = board.store();
        let entry = store
            .secrets
            .iter()
            .find(|entry| entry.label == "mine")
            .expect("the edit entry");
        assert!(
            presented
                .iter()
                .any(|key| key.key_id == entry.salt && key.psk == lpc_access::link_psk(&entry.k)),
            "the edit entry's key is among them, exactly as the board derives it"
        );
    }

    /// A held key the board offers is that salt's key alone: no password is
    /// derived, nothing is guessed.
    #[test]
    fn a_held_key_the_board_offers_is_presented_and_no_password_is_tried() {
        let key = held(9);
        let board = FakeBoard::with_entries(vec![key.key.entry(1)]);
        let cache = Rc::new(RefCell::new(LoginKeyCache::new()));
        let link_keys = NetworkLinkKeys::new();
        let mut client = board.client();

        let outcome = block_on(try_keyed_login(
            &mut client,
            &[held(3), key],
            &["would-be-wrong".to_string()],
            None,
            &cache,
            &link_keys,
            BOARD,
            |_| core::future::ready(()),
        ));

        assert_eq!(outcome, LoginAttemptOutcome::Rekeyed);
        let presented = link_keys.keys_for(BOARD);
        assert_eq!(presented.len(), 1);
        assert_eq!(presented[0].key_id, [9; 16]);
    }

    /// Nothing held is on the board and nothing was typed: the challenge is
    /// handed back for the sheet, and no key is offered.
    #[test]
    fn nothing_to_offer_hands_the_challenge_back() {
        let board = FakeBoard::locked(&[("camp", Tier::Play, "smores")]);
        let cache = Rc::new(RefCell::new(LoginKeyCache::new()));
        let link_keys = NetworkLinkKeys::new();
        let mut client = board.client();

        let outcome = block_on(try_keyed_login(
            &mut client,
            &[held(3)],
            &[],
            None,
            &cache,
            &link_keys,
            BOARD,
            |_| core::future::ready(()),
        ));

        assert!(
            matches!(outcome, LoginAttemptOutcome::NothingMatched { .. }),
            "{outcome:?}"
        );
        assert!(link_keys.keys_for(BOARD).is_empty());
    }

    #[test]
    fn a_board_with_no_passwords_says_so() {
        let board = FakeBoard::locked(&[]);
        let cache = Rc::new(RefCell::new(LoginKeyCache::new()));
        let link_keys = NetworkLinkKeys::new();
        let mut client = board.client();
        let outcome = block_on(try_keyed_login(
            &mut client,
            &[],
            &["x".to_string()],
            None,
            &cache,
            &link_keys,
            BOARD,
            |_| core::future::ready(()),
        ));
        assert_eq!(outcome, LoginAttemptOutcome::NoPasswords);
    }

    fn held(salt: u8) -> HeldKey {
        HeldKey {
            holder: KeyHolder::Browser,
            key: InstallableKey {
                label: "Yona's MacBook".to_string(),
                kind: SecretKind::Browser,
                tier: Tier::Edit,
                salt: [salt; 16],
                iterations: 1,
                material: vec![salt; 32],
            },
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        crate::app::access::test_board::block_on(future)
    }
}

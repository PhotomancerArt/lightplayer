//! A password's link keys: which of a board's login offers to try it
//! against, and the key each one gives.
//!
//! The derivation is the one every client of a device password runs (M4's
//! rule, `lpc_access`'s own functions): `K = derive_login_key(password,
//! offer.salt, offer.iterations)`, the link's key id is the offer's salt
//! and its PSK `link_psk(K)`. The board grants the matched entry's tier when
//! the secure session comes up.
//!
//! **Which offers.** The board's offers name every installed secret, and an
//! offer says only its salt and its cost. A password entry is stretched (a
//! person's password is written with many PBKDF2 iterations); a key held by
//! a browser or an account is a random secret written at one iteration.
//! Every wrong key a handshake names is charged to the board's login
//! backoff, so a password is tried against the stretched offers only — the
//! held keys' only when the board has no stretched one at all.

use lpc_access::{LoginOffer, derive_login_key, link_psk};
use lpc_wire::lp_link::secure_channel::{KeyId, Psk};

use super::board_password::BoardPassword;

/// The keys `password` gives against `offers`, in the order to try them.
pub fn password_keys(password: &BoardPassword, offers: &[LoginOffer]) -> Vec<(KeyId, Psk)> {
    let stretched = offers.iter().any(|offer| offer.iterations > 1);
    offers
        .iter()
        .filter(|offer| !stretched || offer.iterations > 1)
        .map(|offer| {
            let k = derive_login_key(password.as_bytes(), &offer.salt, offer.iterations);
            (KeyId(offer.salt), Psk::new(link_psk(&k)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_is_tried_only_against_stretched_offers_when_there_are_any() {
        let offers = [
            LoginOffer {
                salt: [1; 16],
                iterations: 1,
            },
            LoginOffer {
                salt: [2; 16],
                iterations: 4,
            },
        ];
        let password = BoardPassword::new("camp");
        let keys = password_keys(&password, &offers);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].0, KeyId([2; 16]));

        let held_only = &offers[..1];
        assert_eq!(
            password_keys(&password, held_only).len(),
            1,
            "nothing stretched: try all"
        );
        assert!(password_keys(&password, &[]).is_empty());
    }
}

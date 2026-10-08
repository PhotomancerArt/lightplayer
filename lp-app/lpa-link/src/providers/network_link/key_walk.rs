//! Which key a secure link presents next: the held keys in order, then the
//! anonymous key.
//!
//! A board looks a key up by its id (an access entry's salt), so presenting
//! a key it does not hold costs nothing: it is refused `UnknownKey`, which the
//! board does not charge to its login backoff, and the walk moves on. A
//! `WrongKey` (the board knows the id, the PSK did not match) IS charged, so
//! the walk moves on and the app is told never to present it there again
//! (`LinkKeys::refused_wrong`). A `Backoff` or `Busy` refusal is the board
//! asking for time: the same key again, after it.
//!
//! On the LAN the anonymous key is always last and always there. An open
//! board grants it what it is open to; a locked one brings the link up
//! holding nothing, which is how Studio learns the board's offers
//! (`LoginBegin`) and asks for a password — whose keys then arrive as a new
//! generation of the app's keys.
//!
//! **Through the relay there is no anonymous key** ([`KeyWalk::held_only`]):
//! a board's "Anyone" setting never applies over the internet, so Studio
//! presents only the keys it holds, and a walk that runs out of them is
//! [`KeyWalkStep::Exhausted`] — the board cannot be reached that way, and the
//! person is told to sign in and plug it in once.

use super::link_key::LinkKey;

/// How long a `Busy` refusal (the board did not answer its own key lookup
/// in time) waits before the same key is presented again.
pub const BUSY_RETRY_MS: u32 = 500;

/// Why the board refused the key the walk presented — lp-link's
/// `RefusalReason`, in this crate's words so the policy builds without the
/// secure channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyRefusal {
    /// No entry has this key id (not charged).
    UnknownKey,
    /// An entry has this key id, and the PSK did not match (charged).
    WrongKey,
    /// Too many wrong keys lately: try again after `retry_after_ms`.
    Backoff { retry_after_ms: u32 },
    /// The board did not answer its own lookup in time.
    Busy,
}

/// What to do after a refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyWalkStep {
    /// Present this key now.
    Present(LinkKey),
    /// Present this key once `after_ms` has passed.
    PresentAfter { key: LinkKey, after_ms: u32 },
    /// A held-only walk refused its last key: nothing left to present.
    Exhausted,
}

/// One link's walk through its keys. A new link (a new connection) starts a
/// new walk; so does a new generation of the app's keys.
#[derive(Clone, Debug)]
pub struct KeyWalk {
    /// The keys in the order presented, the anonymous key last (unless the
    /// walk is held-only).
    keys: Vec<LinkKey>,
    at: usize,
    generation: u64,
    /// Held-only: no anonymous key ends the walk.
    held_only: bool,
    /// Nothing left to present (a held-only walk only).
    exhausted: bool,
}

impl KeyWalk {
    /// A walk over `keys` (best first) as of the app's key `generation`.
    /// Repeated ids keep their first place, and any anonymous key among them
    /// moves to the end, where the walk always ends.
    pub fn new(keys: Vec<LinkKey>, generation: u64) -> Self {
        let mut ordered = held_in_order(keys);
        ordered.push(LinkKey::ANONYMOUS);
        Self {
            keys: ordered,
            at: 0,
            generation,
            held_only: false,
            exhausted: false,
        }
    }

    /// A walk over `keys` (best first) with NO anonymous key at the end: a
    /// link through the relay. Repeated ids and any anonymous key among them
    /// are dropped. With no keys at all the walk starts
    /// [exhausted](Self::is_exhausted): there is nothing to present.
    pub fn held_only(keys: Vec<LinkKey>, generation: u64) -> Self {
        let ordered = held_in_order(keys);
        let exhausted = ordered.is_empty();
        Self {
            // An exhausted walk still answers `current()`; nothing presents it.
            keys: if exhausted {
                vec![LinkKey::ANONYMOUS]
            } else {
                ordered
            },
            at: 0,
            generation,
            held_only: true,
            exhausted,
        }
    }

    /// The key to present now. Meaningless once the walk is
    /// [exhausted](Self::is_exhausted): nothing may present it then.
    pub fn current(&self) -> &LinkKey {
        &self.keys[self.at]
    }

    /// Whether this walk ends without the anonymous key (a relay link).
    pub fn is_held_only(&self) -> bool {
        self.held_only
    }

    /// Whether a held-only walk has nothing left to present.
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// The app's key generation this walk was built from.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the walk has reached the anonymous key.
    pub fn is_anonymous(&self) -> bool {
        self.current().is_anonymous()
    }

    /// Whether `key_id` is the key this walk would present first — the one a
    /// link up on any other key would rather hold.
    pub fn first_is(&self, key_id: &[u8; super::link_key::KEY_ID_BYTES]) -> bool {
        self.keys[0].key_id == *key_id
    }

    /// The board refused the current key: what to present next.
    pub fn on_refused(&mut self, refusal: KeyRefusal) -> KeyWalkStep {
        match refusal {
            KeyRefusal::UnknownKey | KeyRefusal::WrongKey => {
                if self.exhausted {
                    KeyWalkStep::Exhausted
                } else if self.at + 1 < self.keys.len() {
                    self.at += 1;
                    KeyWalkStep::Present(self.current().clone())
                } else if self.held_only {
                    self.exhausted = true;
                    KeyWalkStep::Exhausted
                } else {
                    // The anonymous key itself refused: a board that takes no
                    // anonymous session. Ask again, slowly, rather than spin.
                    KeyWalkStep::PresentAfter {
                        key: self.current().clone(),
                        after_ms: BUSY_RETRY_MS * 2,
                    }
                }
            }
            KeyRefusal::Backoff { retry_after_ms } => KeyWalkStep::PresentAfter {
                key: self.current().clone(),
                after_ms: retry_after_ms.max(BUSY_RETRY_MS / 5),
            },
            KeyRefusal::Busy => KeyWalkStep::PresentAfter {
                key: self.current().clone(),
                after_ms: BUSY_RETRY_MS,
            },
        }
    }
}

/// `keys` in order, without repeated ids or the anonymous key.
fn held_in_order(keys: Vec<LinkKey>) -> Vec<LinkKey> {
    let mut ordered: Vec<LinkKey> = Vec::with_capacity(keys.len() + 1);
    for key in keys {
        if key.is_anonymous() || ordered.iter().any(|seen| seen.key_id == key.key_id) {
            continue;
        }
        ordered.push(key);
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::network_link::link_key::{KEY_ID_BYTES, PSK_BYTES};

    #[test]
    fn the_held_keys_go_first_and_the_anonymous_key_last() {
        let mut walk = KeyWalk::new(vec![key(1), key(2)], 7);
        assert_eq!(walk.current(), &key(1));
        assert_eq!(walk.generation(), 7);
        assert_eq!(
            walk.on_refused(KeyRefusal::UnknownKey),
            KeyWalkStep::Present(key(2))
        );
        assert_eq!(
            walk.on_refused(KeyRefusal::WrongKey),
            KeyWalkStep::Present(LinkKey::ANONYMOUS)
        );
        assert!(walk.is_anonymous());
    }

    #[test]
    fn with_no_keys_the_walk_is_the_anonymous_key() {
        let walk = KeyWalk::new(Vec::new(), 0);
        assert!(walk.is_anonymous());
        assert!(walk.first_is(&[0; KEY_ID_BYTES]));
    }

    #[test]
    fn repeats_and_an_anonymous_key_in_the_list_do_not_reorder_it() {
        let walk = KeyWalk::new(vec![LinkKey::ANONYMOUS, key(3), key(3), key(4)], 0);
        assert_eq!(walk.keys, vec![key(3), key(4), LinkKey::ANONYMOUS]);
        assert!(walk.first_is(&key(3).key_id));
    }

    #[test]
    fn a_backoff_or_busy_board_gets_the_same_key_again_after_waiting() {
        let mut walk = KeyWalk::new(vec![key(1)], 0);
        assert_eq!(
            walk.on_refused(KeyRefusal::Backoff {
                retry_after_ms: 2_000
            }),
            KeyWalkStep::PresentAfter {
                key: key(1),
                after_ms: 2_000
            }
        );
        assert_eq!(
            walk.on_refused(KeyRefusal::Busy),
            KeyWalkStep::PresentAfter {
                key: key(1),
                after_ms: BUSY_RETRY_MS
            }
        );
    }

    #[test]
    fn a_refused_anonymous_key_is_asked_again_slowly() {
        let mut walk = KeyWalk::new(Vec::new(), 0);
        assert!(matches!(
            walk.on_refused(KeyRefusal::UnknownKey),
            KeyWalkStep::PresentAfter { key, after_ms } if key.is_anonymous() && after_ms > 0
        ));
    }

    #[test]
    fn a_held_only_walk_never_presents_the_anonymous_key() {
        let mut walk = KeyWalk::held_only(vec![key(1), LinkKey::ANONYMOUS, key(2)], 3);
        assert!(walk.is_held_only() && !walk.is_exhausted());
        assert_eq!(walk.current(), &key(1));
        assert_eq!(
            walk.on_refused(KeyRefusal::UnknownKey),
            KeyWalkStep::Present(key(2))
        );
        // A board that is busy gets the same key again, as on the LAN.
        assert_eq!(
            walk.on_refused(KeyRefusal::Busy),
            KeyWalkStep::PresentAfter {
                key: key(2),
                after_ms: BUSY_RETRY_MS
            }
        );
        assert_eq!(
            walk.on_refused(KeyRefusal::WrongKey),
            KeyWalkStep::Exhausted
        );
        assert!(walk.is_exhausted());
        assert_eq!(
            walk.on_refused(KeyRefusal::UnknownKey),
            KeyWalkStep::Exhausted
        );
    }

    #[test]
    fn a_held_only_walk_with_no_keys_starts_exhausted() {
        let walk = KeyWalk::held_only(vec![LinkKey::ANONYMOUS], 0);
        assert!(walk.is_exhausted());
        let lan = KeyWalk::new(Vec::new(), 0);
        assert!(!lan.is_exhausted() && !lan.is_held_only());
    }

    fn key(id: u8) -> LinkKey {
        LinkKey {
            key_id: [id; KEY_ID_BYTES],
            psk: [id; PSK_BYTES],
        }
    }
}

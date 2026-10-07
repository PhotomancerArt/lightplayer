//! The keys a secure network link presents (Wi-Fi M6 P07): this browser's
//! held keys, and the keys passwords typed for a board derived.
//!
//! A board on the LAN is a secure lp-link responder: the link names a key id
//! in its first SYN and proves it holds the PSK, and the board grants that
//! entry's tier. The id is an access entry's salt and the PSK is
//! `lpc_access::link_psk(K)` — so these are the SAME keys Studio unlocks a
//! Bluetooth board with (`key_holder.rs`, `login_key_cache.rs`), turned into
//! what the link presents. Nothing here is new key material.
//!
//! Two sources, one list per board, best first:
//!
//! 1. **Typed for this board** ([`NetworkLinkKeys::offer`]): a locked board's
//!    link comes up on the anonymous key, the keyed login reads its offers
//!    and derives the password the Unlock sheet took — one key per offered
//!    salt (`keyed_login.rs`). Most recent first.
//! 2. **Held** ([`NetworkLinkKeys::set_held`]): this browser's key and the
//!    account's, which use one salt on every device, so presenting them to a
//!    board that does not hold them costs that board nothing.
//!
//! **By board, once the board is known** (ND4 of the network-transport
//! plan): a link is dialled at an address (its socket URL), and until its
//! hello says which board answered, what was typed for it is kept by that
//! address. Once Studio knows the board — its hello, or a card's "Connect
//! over Wi‑Fi", which knows the board before it dials — the address is an
//! ALIAS of the board's MAC ([`NetworkLinkKeys::alias`]), and the keys live
//! with the board: a password typed for it is not lost when its address
//! changes, and a later link to the same board by another road (the relay)
//! presents them too.
//!
//! A key a board refused as WRONG (it knows the salt; the PSK did not match)
//! is charged to that board's login backoff, so the provider reports it
//! ([`LinkKeys::refused_wrong`]) and it is never presented there again this
//! session. In memory only, like the login cache: a key is login-equivalent
//! and is never persisted from here, and never printed.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpa_link::providers::network_link::{LinkKey, LinkKeys};

/// The key a link presents for the access entry whose salt is `salt` and
/// whose derived key is `k`: the salt as its id, `link_psk(K)` as its PSK.
pub fn link_key(salt: [u8; lpc_access::SALT_BYTES], k: &[u8; lpc_access::KEY_BYTES]) -> LinkKey {
    LinkKey {
        key_id: salt,
        psk: lpc_access::link_psk(k),
    }
}

/// The keys, shared: the access controller writes them, the LAN provider
/// reads them. Cloning shares one store.
#[derive(Clone, Default)]
pub struct NetworkLinkKeys {
    inner: Rc<RefCell<KeyStore>>,
}

#[derive(Default)]
struct KeyStore {
    held: Vec<LinkKey>,
    held_generation: u64,
    /// By board: its MAC once known, its socket URL until then.
    boards: BTreeMap<String, BoardKeys>,
    /// Socket URL → the board (its MAC) found answering there.
    aliases: BTreeMap<String, String>,
}

impl KeyStore {
    /// The number that moves whenever the keys for `address` would change.
    fn generation_of(&self, address: &str) -> u64 {
        self.held_generation
            + self
                .boards
                .get(self.board_of(address))
                .map_or(0, |board| board.generation)
    }

    /// Where the keys for the link at `address` live.
    fn board_of<'a>(&'a self, address: &'a str) -> &'a str {
        self.aliases.get(address).map_or(address, String::as_str)
    }
}

#[derive(Default)]
struct BoardKeys {
    typed: Vec<LinkKey>,
    wrong: Vec<LinkKey>,
    generation: u64,
}

impl NetworkLinkKeys {
    pub fn new() -> Self {
        Self::default()
    }

    /// This browser's held keys, as links present them. A change moves every
    /// board's generation; the same keys again move nothing.
    pub fn set_held(&self, keys: Vec<LinkKey>) {
        let mut store = self.inner.borrow_mut();
        if store.held != keys {
            store.held = keys;
            store.held_generation += 1;
        }
    }

    /// The link at `address` reaches the board `board` (its MAC, any one
    /// spelling used consistently): from now on its keys are the board's.
    /// Anything typed for the address before the board was known moves to
    /// the board (most recent first), and the address's generation moves so
    /// a live link re-reads its list (it is rekeyed only if its best key
    /// changed).
    pub fn alias(&self, address: &str, board: &str) {
        let mut store = self.inner.borrow_mut();
        if store.aliases.get(address).map(String::as_str) == Some(board) {
            return;
        }
        let before = store.generation_of(address) - store.held_generation;
        store.aliases.insert(address.to_string(), board.to_string());
        let moved = store.boards.remove(address);
        let entry = store.boards.entry(board.to_string()).or_default();
        if let Some(moved) = moved {
            let mut typed = moved.typed;
            for key in entry.typed.drain(..) {
                if !typed.iter().any(|newer| newer.key_id == key.key_id) {
                    typed.push(key);
                }
            }
            entry.typed = typed;
            for wrong in moved.wrong {
                if !entry.wrong.contains(&wrong) {
                    entry.wrong.push(wrong);
                }
            }
            entry.generation = entry.generation.max(moved.generation);
        }
        entry.generation = entry.generation.max(before) + 1;
    }

    /// Keys a password typed for the board at `address` derived (one per
    /// salt it offered): presented first, ahead of anything held, from now
    /// on. A key once refused as wrong there is given another chance only if
    /// it is offered again (a new password for that salt is a new key).
    pub fn offer(&self, address: &str, keys: Vec<LinkKey>) {
        if keys.is_empty() {
            return;
        }
        let mut store = self.inner.borrow_mut();
        let id = store.board_of(address).to_string();
        let board = store.boards.entry(id).or_default();
        let mut typed = keys;
        for key in board.typed.drain(..) {
            if !typed.iter().any(|offered| offered.key_id == key.key_id) {
                typed.push(key);
            }
        }
        board.wrong.retain(|wrong| !typed.contains(wrong));
        board.typed = typed;
        board.generation += 1;
    }

    /// Whether anything was typed for the board at `address` this session.
    pub fn has_typed(&self, address: &str) -> bool {
        let store = self.inner.borrow();
        store
            .boards
            .get(store.board_of(address))
            .is_some_and(|board| !board.typed.is_empty())
    }

    /// Forget every typed key (Settings' "Forget remembered passwords"
    /// forgets these with the derived keys they came from).
    pub fn forget_typed(&self) {
        let mut store = self.inner.borrow_mut();
        for board in store.boards.values_mut() {
            if !board.typed.is_empty() {
                board.typed.clear();
                board.generation += 1;
            }
        }
    }
}

impl LinkKeys for NetworkLinkKeys {
    fn keys_for(&self, address: &str) -> Vec<LinkKey> {
        let store = self.inner.borrow();
        let board = store.boards.get(store.board_of(address));
        let wrong = |key: &LinkKey| board.is_some_and(|board| board.wrong.contains(key));
        board
            .map(|board| board.typed.as_slice())
            .unwrap_or_default()
            .iter()
            .chain(store.held.iter())
            .filter(|key| !wrong(key))
            .cloned()
            .collect()
    }

    fn generation(&self, address: &str) -> u64 {
        self.inner.borrow().generation_of(address)
    }

    fn refused_wrong(&self, address: &str, key: &LinkKey) {
        let mut store = self.inner.borrow_mut();
        let id = store.board_of(address).to_string();
        let board = store.boards.entry(id).or_default();
        if !board.wrong.contains(key) {
            board.wrong.push(key.clone());
            board.typed.retain(|typed| typed != key);
            board.generation += 1;
        }
    }
}

/// Keys are login-equivalent: the debug form counts them and nothing more.
impl core::fmt::Debug for NetworkLinkKeys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let store = self.inner.borrow();
        f.debug_struct("NetworkLinkKeys")
            .field("held", &store.held.len())
            .field("boards", &store.boards.len())
            .finish()
    }
}

/// One store is one store: two handles are equal when they share it.
impl PartialEq for NetworkLinkKeys {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for NetworkLinkKeys {}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = "ws://10.0.0.5/link";
    const MAC: &str = "a0f26287b48c";

    #[test]
    fn typed_keys_go_first_then_the_held_ones() {
        let keys = NetworkLinkKeys::new();
        keys.set_held(vec![key(1), key(2)]);
        assert_eq!(keys.keys_for(BOARD), vec![key(1), key(2)]);

        let before = keys.generation(BOARD);
        keys.offer(BOARD, vec![key(7), key(8)]);
        assert_eq!(keys.keys_for(BOARD), vec![key(7), key(8), key(1), key(2)]);
        assert!(keys.generation(BOARD) > before);
        assert!(keys.has_typed(BOARD));
        // Another board sees only what is held.
        assert_eq!(keys.keys_for("ws://10.0.0.6/link"), vec![key(1), key(2)]);
    }

    #[test]
    fn the_same_held_keys_again_move_no_generation() {
        let keys = NetworkLinkKeys::new();
        keys.set_held(vec![key(1)]);
        let generation = keys.generation(BOARD);
        keys.set_held(vec![key(1)]);
        assert_eq!(keys.generation(BOARD), generation);
        keys.set_held(vec![key(1), key(2)]);
        assert_ne!(keys.generation(BOARD), generation);
    }

    #[test]
    fn a_wrong_key_is_never_presented_there_again_until_offered_anew() {
        let keys = NetworkLinkKeys::new();
        keys.set_held(vec![key(1)]);
        keys.offer(BOARD, vec![key(7)]);
        let generation = keys.generation(BOARD);

        keys.refused_wrong(BOARD, &key(7));
        keys.refused_wrong(BOARD, &key(1));
        assert!(keys.keys_for(BOARD).is_empty());
        assert!(keys.generation(BOARD) > generation);
        assert_eq!(keys.keys_for("ws://10.0.0.6/link"), vec![key(1)]);

        keys.offer(BOARD, vec![key(7)]);
        assert_eq!(keys.keys_for(BOARD), vec![key(7)]);
    }

    #[test]
    fn a_newer_password_for_a_salt_replaces_the_older_key() {
        let keys = NetworkLinkKeys::new();
        keys.offer(BOARD, vec![key(7), key(8)]);
        let mut newer = key(7);
        newer.psk = [0x77; 32];
        keys.offer(BOARD, vec![newer.clone()]);
        assert_eq!(keys.keys_for(BOARD), vec![newer, key(8)]);
    }

    #[test]
    fn forgetting_typed_keys_keeps_the_held_ones() {
        let keys = NetworkLinkKeys::new();
        keys.set_held(vec![key(1)]);
        keys.offer(BOARD, vec![key(7)]);
        keys.forget_typed();
        assert_eq!(keys.keys_for(BOARD), vec![key(1)]);
        assert!(!keys.has_typed(BOARD));
    }

    #[test]
    fn keys_typed_for_an_address_follow_the_board_to_its_next_address() {
        let keys = NetworkLinkKeys::new();
        keys.set_held(vec![key(1)]);
        // Typed while the board was only an address.
        keys.offer(BOARD, vec![key(7)]);
        let before = keys.generation(BOARD);
        // Its hello says which board it is.
        keys.alias(BOARD, MAC);
        assert!(keys.generation(BOARD) > before, "a live link re-reads");
        assert_eq!(keys.keys_for(BOARD), vec![key(7), key(1)]);
        // The board comes back at a new address, known before it is dialled
        // (a card's "Connect over Wi‑Fi"): the typed key is presented there.
        let moved = "ws://10.0.0.9/link";
        assert_eq!(keys.keys_for(moved), vec![key(1)]);
        let before = keys.generation(moved);
        keys.alias(moved, MAC);
        assert!(keys.generation(moved) > before);
        assert_eq!(keys.keys_for(moved), vec![key(7), key(1)]);
        assert!(keys.has_typed(moved));
        // A key refused as wrong at one address is the board's to drop.
        keys.refused_wrong(moved, &key(7));
        assert_eq!(keys.keys_for(BOARD), vec![key(1)]);
        // A second alias call with the same board moves nothing.
        let settled = keys.generation(moved);
        keys.alias(moved, MAC);
        assert_eq!(keys.generation(moved), settled);
    }

    #[test]
    fn a_password_typed_after_the_alias_is_the_boards() {
        let keys = NetworkLinkKeys::new();
        keys.alias(BOARD, MAC);
        keys.offer(BOARD, vec![key(7)]);
        keys.alias("ws://10.0.0.9/link", MAC);
        assert_eq!(keys.keys_for("ws://10.0.0.9/link"), vec![key(7)]);
        // Another board at another address sees none of it.
        assert!(keys.keys_for("ws://10.0.0.6/link").is_empty());
    }

    #[test]
    fn debug_counts_and_never_prints_a_key() {
        let keys = NetworkLinkKeys::new();
        keys.set_held(vec![key(0x5c)]);
        let shown = format!("{keys:?}");
        assert!(shown.contains("held: 1"), "{shown}");
        assert!(!shown.contains("5c"), "{shown}");
    }

    fn key(id: u8) -> LinkKey {
        LinkKey {
            key_id: [id; 16],
            psk: [id; 32],
        }
    }
}

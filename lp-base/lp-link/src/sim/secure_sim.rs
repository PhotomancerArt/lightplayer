//! A simulated run with secure links (feature `secure`): the host is the
//! initiator with a key, the board the responder whose edge answers key
//! lookups from a scripted table, after a scripted delay.

use std::vec::Vec;

use crate::secure_channel::{KeyId, Psk, RefusalReason, SecureEvent, SecureRole};
use crate::{Arq, Link};

/// How long the board's edge takes to answer a key lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupDelay {
    /// In the same service step.
    Now,
    /// After this many of the board's service steps.
    Steps(u32),
    /// Never (the link refuses `Busy` after 2 s).
    Never,
}

#[derive(Clone, Debug)]
pub struct SecureSim {
    /// The host's key.
    pub host_key: KeyId,
    pub host_psk: Psk,
    /// The board's key table: each key id's candidate PSKs, in order.
    pub board_table: Vec<(KeyId, Vec<Psk>)>,
    pub lookup_delay: LookupDelay,
}

impl SecureSim {
    /// The host holds key `(1…1, 2…2)` and the board knows it.
    pub fn matched() -> Self {
        let (key_id, psk) = (KeyId([1; 16]), Psk::new([2; 32]));
        SecureSim {
            host_key: key_id,
            host_psk: psk.clone(),
            board_table: Vec::from([(key_id, Vec::from([psk]))]),
            lookup_delay: LookupDelay::Now,
        }
    }

    pub fn host_role(&self) -> SecureRole {
        SecureRole::Initiator {
            key_id: self.host_key,
            psk: self.host_psk.clone(),
        }
    }
}

/// One end's secure edge in the simulator: which role it rebuilds its link
/// with, and (on the board) the lookups it has yet to answer.
#[derive(Clone, Debug)]
pub struct SecureEdge {
    pub sim: SecureSim,
    pub board: bool,
    waiting: Vec<(KeyId, u32)>,
    /// Events the link raised, in order (for a scenario to read).
    pub events: Vec<SecureEvent>,
}

impl SecureEdge {
    pub fn new(sim: SecureSim, board: bool) -> Self {
        SecureEdge {
            sim,
            board,
            waiting: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn role(&self) -> SecureRole {
        if self.board {
            SecureRole::Responder
        } else {
            self.sim.host_role()
        }
    }

    /// A new incarnation: lookups in flight die with the old link.
    pub fn reboot(&mut self) {
        self.waiting.clear();
    }

    /// Drain the link's secure events; answer the board's lookups as due.
    pub fn service<A: Arq>(&mut self, link: &mut Link<A>) {
        while let Some(ev) = link.poll_secure_event() {
            if let SecureEvent::KeyLookup { key_id } = ev {
                let wait = match self.sim.lookup_delay {
                    LookupDelay::Now => 0,
                    LookupDelay::Steps(n) => n,
                    LookupDelay::Never => continue,
                };
                self.waiting.push((key_id, wait));
            }
            self.events.push(ev);
        }
        let mut due = Vec::new();
        self.waiting.retain_mut(|(key_id, wait)| {
            if *wait == 0 {
                due.push(*key_id);
                false
            } else {
                *wait -= 1;
                true
            }
        });
        for key_id in due {
            match self.sim.board_table.iter().find(|(k, _)| *k == key_id) {
                Some((_, psks)) => link.provide_keys(key_id, psks),
                None => link.refuse(key_id, RefusalReason::UnknownKey, 0),
            }
        }
    }
}

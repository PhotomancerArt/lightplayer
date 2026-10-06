//! What the harness saw, for a test to assert on: the board-side facts a
//! client cannot see from its end of the link.

extern crate std;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use lpc_wire::HelloAuth;

/// Counted by the harness's threads as they go.
#[derive(Default)]
pub struct HarnessCounters {
    refused: AtomicUsize,
    links_opened: AtomicUsize,
    links_closed: AtomicUsize,
    requests: AtomicUsize,
    early_requests: AtomicUsize,
    hellos: Mutex<Vec<HelloAuth>>,
}

/// A snapshot of [`HarnessCounters`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HarnessStats {
    /// Connections told WebSocket close 1013 (both LAN slots busy).
    pub refused: usize,
    /// WebSocket upgrades that got a LAN slot and a secure lp-link session.
    pub links_opened: usize,
    /// LAN links whose connection has ended and whose slot is free again.
    pub links_closed: usize,
    /// Requests the server took off a LAN link.
    pub requests: usize,
    /// Requests the server took off a link whose secure session had not
    /// come up (whose hello the mux had not handed out). Always 0, or the
    /// mux let a frame through before the handshake.
    pub early_requests: usize,
    /// The `auth` of every unsolicited hello the server loop sent, in order:
    /// what each secure session was told at its `Up`.
    pub hello_auths: Vec<HelloAuth>,
}

impl HarnessCounters {
    pub(super) fn refused(&self) {
        self.refused.fetch_add(1, Ordering::SeqCst);
    }

    pub(super) fn link_opened(&self) {
        self.links_opened.fetch_add(1, Ordering::SeqCst);
    }

    pub(super) fn link_closed(&self) {
        self.links_closed.fetch_add(1, Ordering::SeqCst);
    }

    pub(super) fn request(&self, early: bool) {
        self.requests.fetch_add(1, Ordering::SeqCst);
        if early {
            self.early_requests.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub(super) fn hello(&self, auth: HelloAuth) {
        lock(&self.hellos).push(auth);
    }

    /// Everything so far.
    pub fn snapshot(&self) -> HarnessStats {
        HarnessStats {
            refused: self.refused.load(Ordering::SeqCst),
            links_opened: self.links_opened.load(Ordering::SeqCst),
            links_closed: self.links_closed.load(Ordering::SeqCst),
            requests: self.requests.load(Ordering::SeqCst),
            early_requests: self.early_requests.load(Ordering::SeqCst),
            hello_auths: lock(&self.hellos).clone(),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

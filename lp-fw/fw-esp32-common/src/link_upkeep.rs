//! What the server loop asks of a transport between ticks, beyond
//! [`lpc_shared::transport::ServerTransport`].
//!
//! The server loop is the one place that holds both the server and the clock,
//! so link policy that needs both runs from here: a link that joins after
//! boot is owed its own hello, and a transport may enforce deadlines against
//! the server's view of a link (the radio links' login deadline). A
//! single-link transport does neither — the defaults are empty.

use alloc::vec::Vec;
use lpa_server::LpServer;
use lpc_shared::transport::Link;

/// Per-frame link housekeeping, driven by `server_loop`.
pub trait LinkUpkeep {
    /// Links that opened since the last call; the loop sends each its hello.
    fn take_opened_links(&mut self) -> Vec<Link> {
        Vec::new()
    }

    /// Once per frame, after the tick: `now_ms` is the loop's clock.
    fn upkeep(&mut self, _server: &LpServer, _now_ms: u64) {}
}

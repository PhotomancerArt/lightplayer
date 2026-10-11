//! The harness board's main thread: a real [`LpServer`] on a memory
//! filesystem behind the real [`LinkMuxTransport`], run frame by frame in
//! the order `server_loop::run_server_loop` runs it on the C6:
//!
//! 1. the hello each newly opened link is owed (`take_opened_links`), sent
//!    with `fw_core::send_hello_to_link`;
//! 2. every request the transport has (`receive`, until none);
//! 3. `tick_and_send` (closed links, secure handshakes' key lookups and
//!    grants, then the requests);
//! 4. a heartbeat per link every [`HEARTBEAT_EVERY_MS`];
//! 5. `upkeep` (the login deadline), then the relay's answer
//!    (`serve_relay`: a picture when asked, the project's facts when they
//!    change — the C6's frame hook), then a 1 ms yield.
//!
//! Keep that order the loop's: a difference here would hide exactly the
//! kind of ordering defect this harness exists to find.

extern crate std;

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use lpa_server::{HeartbeatStatus, LpGraphics, LpServer};
use lpc_access::DeviceAccessFile;
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{LinkId, ServerTransport};
use lpc_wire::server::SampleStats;
use lpfs::{LpFs, LpFsMemory};

use super::harness_block_on::{StdDelay, block_on};
use super::harness_counters::HarnessCounters;
use super::harness_entropy::harness_entropy;
use super::harness_relay::RelayShared;
use super::no_usb::NoUsb;
use crate::link_upkeep::LinkUpkeep;
use crate::net::relay::{RelaySourceState, serve_relay};
use crate::radio_link::{LinkMuxTransport, PortLock, RadioLinkPort, SharedPort};

/// How often each open link gets a heartbeat (the C6's is 5 s; shorter here
/// so a short test sees some).
const HEARTBEAT_EVERY_MS: u64 = 1_000;

/// What the server thread is built from.
pub(super) struct ServerSetup {
    /// The lock the port's LAN slots are borrowed under.
    pub lock: PortLock,
    /// The device store, as JSON (`/.lp/access.json`).
    pub access_json: String,
    pub graphics: Arc<dyn LpGraphics>,
    pub stop: Arc<AtomicBool>,
    pub counters: Arc<HarnessCounters>,
    /// The relay's board, when the harness is on a relay.
    pub relay: Option<Arc<RelayShared>>,
}

/// Run the board's main thread until `setup.stop`. The port is made here,
/// on the thread that runs the mux (its main half may not leave it); `ready`
/// is handed the half the LAN edges use, once the server exists.
pub(super) fn run_server(setup: ServerSetup, ready: std::sync::mpsc::Sender<SharedPort>) {
    let ServerSetup {
        lock,
        access_json,
        graphics,
        stop,
        counters,
        relay,
    } = setup;
    let (port, shared) = RadioLinkPort::leak_locked(lock);
    let fs = LpFsMemory::new();
    if let Err(error) = fs.write_file(DeviceAccessFile::PATH.as_path(), access_json.as_bytes()) {
        log::error!("[harness] the device store did not write: {error}");
    }
    let mut server = LpServer::new(
        Rc::new(RefCell::new(MemoryOutputProvider::new())),
        Box::new(fs),
        "/projects/".as_path(),
        None,
        None,
        graphics,
    );
    server.set_entropy_source(Some(harness_entropy));
    let mut mux = LinkMuxTransport::new(NoUsb, port, StdDelay);
    let _ = ready.send(shared);

    let started = Instant::now();
    let now_ms = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut last_tick = now_ms();
    let mut last_heartbeat = now_ms();
    let mut frame_count = 0u64;
    // Links whose session's hello has been handed out: a request on any
    // other link came through before its secure session was up.
    let mut up: BTreeSet<LinkId> = BTreeSet::new();
    let mut relay_source = RelaySourceState::new();

    while !stop.load(Ordering::SeqCst) {
        for link in mux.take_opened_links() {
            up.insert(link.id);
            counters.hello(server.hello_for_link(link).auth);
            if let Err(error) = block_on(fw_core::send_hello_to_link(&server, &mut mux, link)) {
                log::warn!("[harness] the hello to link {} failed: {error}", link.id);
            }
        }

        let mut incoming = Vec::new();
        while let Ok(Some(message)) = block_on(mux.receive()) {
            counters.request(!up.contains(&message.link));
            incoming.push(message);
        }

        let now = now_ms();
        let delta = u32::try_from(now.saturating_sub(last_tick))
            .unwrap_or(u32::MAX)
            .max(1);
        last_tick = now;
        if let Err(error) = block_on(server.tick_and_send(delta, incoming, &mut mux)) {
            log::warn!("[harness] tick failed: {error}");
        }
        frame_count += 1;

        if now.saturating_sub(last_heartbeat) >= HEARTBEAT_EVERY_MS {
            last_heartbeat = now;
            let status = HeartbeatStatus {
                fps: SampleStats {
                    avg: 0.0,
                    sdev: 0.0,
                    min: 0.0,
                    max: 0.0,
                },
                frame_count,
                uptime_ms: now,
                memory: None,
                recovery: None,
                outputs: None,
                link: None,
            };
            for (link, beat) in server.heartbeats(&mux.links(), status) {
                let _ = block_on(mux.send(link, beat));
            }
        }

        mux.upkeep(&server, now);
        if let Some(relay) = &relay {
            serve_relay(&relay.board, &server, now, &mut relay_source);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let _ = block_on(mux.close());
}

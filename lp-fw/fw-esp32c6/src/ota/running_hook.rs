//! The running engine's channel-3 hook (DM13, DM17): core code the engine's
//! USB transport calls for every update-channel message, and once per pass
//! over the link to flush (`fw_esp32_common::usb_link::set_update_hook`).
//!
//! It runs the board session in `EngineRunning` mode:
//!
//! - `Q` → `M` (state `running`);
//! - `G` → a read-back of the engine, one 4 KiB sector per request, read
//!   from flash: the host paces it, so rendering is never starved;
//! - an offer of another core (by hashes) → the checks, the progress record
//!   (stage *pending*), the engine header erased, and a reset into
//!   core-only — at once, since the engine's first sector is gone;
//! - an unknown message → `N`/`U`.
//!
//! The session is built on the first message, not at boot: its facts carry
//! the core's SHA-256, which is computed on first need (DM24). It lives in a
//! core static, touched only from the server loop's task.

use core::cell::RefCell;

use fw_esp32_common::usb_link::UsbLinkShared;
use lpc_update::board::{
    AccessFacts, EngineStatus, LinkTrust, OWNER_QUIET_MS, SessionConfig, SessionMode,
};

use super::board_identity::{CoreIdentity, board_facts};
use super::boot_state::BootState;
use super::update_edge::{EdgeEffect, UpdateEdge};
use super::update_target_impl::SplitUpdateTarget;

/// What the hook needs to build its session.
struct Setup {
    usb_link: &'static UsbLinkShared,
    state: BootState,
    identity: CoreIdentity,
    engine_len: u32,
    access: AccessFacts,
    usb_trust: LinkTrust,
}

struct Running {
    setup: Setup,
    edge: Option<UpdateEdge>,
}

/// The hook's state. One task only: the server loop's (the transport calls
/// the hook from its own pump), so a `RefCell` is the whole lock.
struct OneTask(RefCell<Option<Running>>);

// SAFETY: touched only from the server loop's task (see above); the
// `RefCell` turns any overlap into a panic rather than an aliased `&mut`.
unsafe impl Sync for OneTask {}

static RUNNING: OneTask = OneTask(RefCell::new(None));

/// Install the hook before entering the engine.
pub fn install(
    usb_link: &'static UsbLinkShared,
    state: BootState,
    identity: CoreIdentity,
    engine_len: u32,
    access: AccessFacts,
    usb_trust: LinkTrust,
) {
    *RUNNING.0.borrow_mut() = Some(Running {
        setup: Setup {
            usb_link,
            state,
            identity,
            engine_len,
            access,
            usb_trust,
        },
        edge: None,
    });
    fw_esp32_common::usb_link::set_update_hook(hook);
}

/// The board manifest the hello carries (`ServerHello::firmware`, wire
/// proto 37): the same session's view a host's `Q` gets, built on first
/// need. `None` only while the hook is not installed (or is busy, which a
/// hello built between two of its passes never sees).
pub fn manifest() -> Option<lpc_update::BoardManifest> {
    let mut running = RUNNING.0.try_borrow_mut().ok()?;
    let running = running.as_mut()?;
    let edge = edge_of(running);
    Some(edge.session.manifest(super::update_edge::now_ms()))
}

fn edge_of(running: &mut Running) -> &mut UpdateEdge {
    running.edge.get_or_insert_with(|| {
        let s = &running.setup;
        let facts = board_facts(
            &s.state,
            &s.identity,
            SessionMode::EngineRunning,
            EngineStatus::Valid,
            Some(s.engine_len),
        );
        let config = SessionConfig {
            takes_encoding_1: false,
            // The engine's channel 3 takes no login: its server's (channel
            // 1) holds the link's tier.
            entropy: None,
            owner_quiet_ms: OWNER_QUIET_MS,
        };
        let access = s.access.clone();
        let mut edge = UpdateEdge::new(SplitUpdateTarget::new(&s.state), facts, access, config);
        edge.link_up(s.usb_trust);
        edge
    })
}

/// `Some(message)`: one channel-3 message; `None`: a pass, to flush.
fn hook(message: Option<&[u8]>) {
    let Ok(mut running) = RUNNING.0.try_borrow_mut() else {
        return;
    };
    let Some(running) = running.as_mut() else {
        return;
    };
    let usb_link = running.setup.usb_link;
    if message.is_none() && running.edge.is_none() {
        return; // nothing said yet, nothing to flush
    }
    let edge = edge_of(running);
    if let Some(bytes) = message {
        edge.on_message(bytes);
    }
    for effect in edge.pump(usb_link) {
        if effect == EdgeEffect::Reset {
            // The engine's own header sector is erased: no engine code may
            // run from it again. Reset now; the host sees its link drop and
            // finds core-only waiting.
            super::reset_now();
        }
    }
}

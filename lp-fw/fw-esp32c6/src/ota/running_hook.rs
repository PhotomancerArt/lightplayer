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
//! The session is built once at install, before the engine starts, for the
//! hello's manifest (its facts carry the core's SHA-256, DM24, computed
//! then), and dropped again; a host's first message builds it for good. It
//! lives in a core static, touched only from the server loop's task.

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
    /// The manifest at install. With no session nothing it says can
    /// change: the engine never runs on a trial core, and only a session
    /// starts a transfer.
    at_install: lpc_update::BoardManifest,
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
    clear_stale_progress(&state);
    let setup = Setup {
        usb_link,
        state,
        identity,
        engine_len,
        access,
        usb_trust,
    };
    // Built here, before the engine starts, rather than in the server
    // loop's first hello (which would stall the loop for the core's hash,
    // ~0.8 s emulated); dropped at once, so its buffers cost the engine's
    // heap nothing until a host arrives — the manifest kept is ~200 B.
    let at_install = new_edge(&setup)
        .session
        .manifest(super::update_edge::now_ms());
    *RUNNING.0.borrow_mut() = Some(Running {
        setup,
        edge: None,
        at_install,
    });
    fw_esp32_common::usb_link::set_update_hook(hook);
}

/// The board manifest the hello carries (`ServerHello::firmware`, wire
/// proto 37): the live session's view a host's `Q` gets, or, before any
/// host spoke, the one taken at install. `None` only while the hook is not
/// installed (or is busy, which a hello built between two of its passes
/// never sees).
pub fn manifest() -> Option<lpc_update::BoardManifest> {
    let running = RUNNING.0.try_borrow().ok()?;
    let running = running.as_ref()?;
    Some(match &running.edge {
        Some(edge) => edge.session.manifest(super::update_edge::now_ms()),
        None => running.at_install.clone(),
    })
}

/// DM13: a progress record beside a valid engine is stale (a cut between
/// the running engine's pending record and its header erase): the valid
/// engine wins, and the record goes before the engine starts.
fn clear_stale_progress(state: &BootState) {
    let at = lp_bootctl::PROGRESS_RECORD_SECTOR;
    let mut flash = super::split_flash::SplitFlash::take();
    let mut magic = [0u8; 4];
    if !flash.read(at, &mut magic) || magic == [0xff; 4] {
        return;
    }
    if let (true, Some(layout)) = (state.trusted(), state.layout) {
        flash.protect(state.core_extent(), layout.region_end);
        if flash.erase(at) {
            log::info!("[OTA] cleared a stale progress record beside a valid engine");
        }
    }
}

fn edge_of(running: &mut Running) -> &mut UpdateEdge {
    let setup = &running.setup;
    running.edge.get_or_insert_with(|| new_edge(setup))
}

fn new_edge(s: &Setup) -> UpdateEdge {
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

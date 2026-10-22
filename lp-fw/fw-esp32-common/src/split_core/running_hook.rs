//! The running engine's channel-3 hooks (DM13, DM17): core code the engine's
//! transports call for every update-channel message — the USB transport
//! (`crate::usb_link::set_update_hook`) and, when the image has radio
//! links, the link mux for each radio link's
//! (`LinkMuxTransport::with_update_hook`, [`radio_update_hook`]) — and once
//! per pass over the links to flush.
//!
//! It runs the board session in `EngineRunning` mode, one session for every
//! link:
//!
//! - `Q` → `M` (state `running`);
//! - `G` → a read-back of the engine, one 4 KiB sector per request, read
//!   from flash: the host paces it, so rendering is never starved;
//! - an offer of another core (by hashes) → the checks, the progress record
//!   (stage *pending*), the engine header erased, and a reset into
//!   core-only — at once, since the engine's first sector is gone;
//! - an unknown message → `N`/`U`.
//!
//! **Access.** USB is trusted (unless a test fixture says not). A radio link
//! is untrusted, and its message arrives with the tier a login or key
//! granted it on the engine's server — never one the device's `open`
//! setting alone gave: the session adds `open` itself (from the access file
//! read at boot), so its one rule — QY2's switch included — decides here as
//! it does in core-only. Read-back needs play; another core needs edit.
//!
//! The session is built once at install, before the engine starts, for the
//! hello's manifest, and dropped again; a host's first message, on any link,
//! builds it for good. It lives in a core static, touched only from the
//! server loop's task.
//!
//! **The core's SHA-256 (DM24) is not taken at install**: hashing the 1.4 MB
//! core there held every boot's first light ~185 ms (CX1, 2026-10-16). The
//! engine's frame hook hashes it a slice a frame once the boot has lit
//! ([`hash_step`]), and the server loop holds the hello — unsolicited, owed
//! to a link that opened, or asked for — until it is known ([`hello_ready`];
//! a host waiting on its `Hello` has the rest taken at once after first
//! light, and nothing waits past `server_loop::HELLO_HOLD_MAX_MS`). Anything
//! else that needs the hash sooner — a host's first channel-3 message —
//! finishes it then, as every boot used to.

use core::cell::RefCell;

use lpc_update::board::{
    AccessFacts, EngineStatus, LinkTrust, OWNER_QUIET_MS, SessionConfig, SessionMode,
};

use super::board_identity::{
    CoreIdentity, board_facts, core_sha256, core_sha256_known, core_sha256_step,
};
use super::boot_state::BootState;
use super::split_chip::SplitChip;
use super::update_edge::{EdgeEffect, UpdateEdge};
use super::update_links::{USB_LINK, UpdateLinks};
use super::update_target_impl::SplitUpdateTarget;

/// What the hook needs to build its session.
struct Setup {
    links: UpdateLinks,
    state: BootState,
    identity: CoreIdentity,
    engine_len: u32,
    access: AccessFacts,
    usb_trust: LinkTrust,
}

pub(super) struct Running<C: SplitChip> {
    setup: Setup,
    edge: Option<UpdateEdge<C>>,
    /// The manifest at install, as a view that owns no heap: its text is
    /// the image's own `'static` strings, so what stays resident does not
    /// grow with the app version's length (#964). With no session nothing
    /// it says can change: the engine never runs on a trial core, and only
    /// a session starts a transfer. Its `core_sha256` is left zero: it is
    /// filled in only when said ([`manifest`]).
    at_install: lpc_update::BoardManifestView<'static>,
}

/// The hook's state (the chip's [`super::SplitStatics`] holds it). One task
/// only: the server loop's (the transports call the hooks from their own
/// pump and upkeep), so a `RefCell` is the whole lock.
pub struct RunningCell<C: SplitChip>(RefCell<Option<Running<C>>>);

impl<C: SplitChip> RunningCell<C> {
    pub(super) const fn new() -> Self {
        Self(RefCell::new(None))
    }
}

// SAFETY: touched only from the server loop's task (see above); the
// `RefCell` turns any overlap into a panic rather than an aliased `&mut`.
unsafe impl<C: SplitChip> Sync for RunningCell<C> {}

/// Install the USB hook before entering the engine (the radio hook is the
/// link mux's, [`radio_update_hook`]: the engine installs it with the mux).
pub fn install<C: SplitChip>(
    links: UpdateLinks,
    state: BootState,
    identity: CoreIdentity,
    engine_len: u32,
    access: AccessFacts,
    usb_trust: LinkTrust,
) {
    clear_stale_progress::<C>(&state);
    let setup = Setup {
        links,
        state,
        identity,
        engine_len,
        access,
        usb_trust,
    };
    // Built here, before the engine starts, rather than in the server
    // loop's first hello; dropped at once, so its buffers cost the engine's
    // heap nothing until a host arrives — the view kept owns no heap. The
    // core's hash is left out (all-zero facts): hashing it here held every
    // boot's first light ~185 ms. `manifest` fills it in.
    let at_install = resident_manifest::<C>(&setup);
    *C::statics().running.0.borrow_mut() = Some(Running {
        setup,
        edge: None,
        at_install,
    });
    crate::usb_link::set_update_hook(usb_hook::<C>);
}

/// The manifest at install, borrowing the image's own strings. Its own
/// frame (`inline(never)`), so the edge it reads from does not stay on the
/// stack beside the view it returns.
#[inline(never)]
fn resident_manifest<C: SplitChip>(setup: &Setup) -> lpc_update::BoardManifestView<'static> {
    let edge = new_edge_with::<C>(setup, [0; 32]);
    edge.session
        .manifest_view(super::update_edge::now_ms())
        .with_text(
            setup.identity.target,
            setup.identity.chip,
            setup.identity.version,
        )
}

/// The board manifest the hello carries (`ServerHello::firmware`, wire
/// proto 38): the live session's view a host's `Q` gets, or, before any
/// host spoke, the one taken at install. `None` only while the hook is not
/// installed (or is busy, which a hello built between two of its passes
/// never sees).
///
/// The core's hash is in it: if [`hash_step`] has not finished it yet, it is
/// finished here.
pub fn manifest<C: SplitChip>() -> Option<lpc_update::BoardManifest> {
    let running = C::statics().running.0.try_borrow().ok()?;
    let running = running.as_ref()?;
    Some(match &running.edge {
        Some(edge) => edge.session.manifest(super::update_edge::now_ms()),
        None => {
            // The owned manifest exists only for this send; the hash the
            // resident view leaves out is filled in (finished here if
            // `hash_step` has not).
            let mut view = running.at_install;
            view.core_sha256 = core_sha256::<C>(&running.setup.state);
            view.to_manifest()
        }
    })
}

/// The engine's frame hook, once the boot has lit (or given up lighting):
/// one slice more of the core's hash, until it is known.
pub fn hash_step<C: SplitChip>() {
    if core_sha256_known().is_some() {
        return;
    }
    let Ok(running) = C::statics().running.0.try_borrow() else {
        return;
    };
    if let Some(running) = running.as_ref() {
        core_sha256_step::<C>(&running.setup.state);
    }
}

/// Whether the unsolicited hello may go out (`server_loop::set_hello_gate`):
/// the core's hash is known, so saying it costs the loop nothing; or a host
/// is waiting on its `Hello` (`wanted`) and the boot has lit, so the rest of
/// the hash is taken at once, for it, rather than a slice a frame. Set only
/// once [`install`] has run (the engine's door, after it).
pub fn hello_ready<C: SplitChip>(wanted: bool) -> bool {
    core_sha256_known().is_some() || (wanted && C::first_light_decided())
}

/// DM13: a progress record beside a valid engine is stale (a cut between
/// the running engine's pending record and its header erase): the valid
/// engine wins, and the record goes before the engine starts.
fn clear_stale_progress<C: SplitChip>(state: &BootState) {
    let at = lp_bootctl::PROGRESS_RECORD_SECTOR;
    let mut flash = super::split_flash::SplitFlash::<C>::take();
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

fn edge_of<C: SplitChip>(running: &mut Running<C>) -> &mut UpdateEdge<C> {
    let setup = &running.setup;
    running.edge.get_or_insert_with(|| new_edge::<C>(setup))
}

fn new_edge<C: SplitChip>(s: &Setup) -> UpdateEdge<C> {
    new_edge_with::<C>(s, core_sha256::<C>(&s.state))
}

fn new_edge_with<C: SplitChip>(s: &Setup, core_sha256: [u8; 32]) -> UpdateEdge<C> {
    let facts = board_facts::<C>(
        &s.state,
        &s.identity,
        SessionMode::EngineRunning,
        EngineStatus::Valid,
        Some(s.engine_len),
        core_sha256,
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
    edge.link_up(USB_LINK, s.usb_trust);
    edge
}

/// One call into the session, by whichever transport: `message` runs on the
/// session, then everything queued is sent and its effects taken. `open`:
/// who the device is open to now, as the engine's server holds it (a radio
/// message carries it; USB, trusted, does not need it).
fn with_session<C: SplitChip>(
    message: impl FnOnce(&mut UpdateEdge<C>),
    flush_only: bool,
    open: Option<lpc_access::OpenTo>,
) {
    let Ok(mut running) = C::statics().running.0.try_borrow_mut() else {
        return;
    };
    let Some(running) = running.as_mut() else {
        return;
    };
    let links = running.setup.links;
    if flush_only && running.edge.is_none() {
        return; // nothing said yet, nothing to flush
    }
    // The access file was read at boot. A board locked (or opened) over a
    // link since then: the session starts again on the new answer, rather
    // than take a core install over radio on the old one until a reboot.
    // Nothing long-lived is lost: while the engine runs a transfer is only
    // ever pending (an accepted core offer resets at once), and a read-back
    // is one sector per request, which the host asks for again.
    if let Some(open) = open
        && open != running.setup.access.open
    {
        log::info!(
            "[OTA] the device is open to {open:?} now (was {:?}): the update session follows",
            running.setup.access.open
        );
        running.setup.access.open = open;
        running.edge = None;
    }
    let edge = edge_of(running);
    message(edge);
    for effect in edge.pump(&links) {
        if effect == EdgeEffect::Reset {
            // The engine's own header sector is erased: no engine code may
            // run from it again. Reset now; the host sees its link drop and
            // finds core-only waiting.
            C::reset_now();
        }
    }
}

/// The USB transport's hook: `Some(message)`, one channel-3 message; `None`,
/// a pass, to flush.
fn usb_hook<C: SplitChip>(message: Option<&[u8]>) {
    match message {
        Some(bytes) => with_session::<C>(|edge| edge.on_message(USB_LINK, bytes), false, None),
        None => with_session::<C>(|_| {}, true, None),
    }
}

/// The link mux's hook (`LinkMuxTransport::with_update_hook`): a radio
/// link's channel-3 message with the tier its login or key granted, a link
/// that closed, or a pass to flush.
#[cfg(feature = "radio-link")]
pub fn radio_update_hook<C: SplitChip>(call: crate::radio_link::RadioUpdate<'_>) {
    use crate::radio_link::RadioUpdate;

    use super::update_links::session_link;

    match call {
        RadioUpdate::Message {
            link,
            granted,
            open,
            relayed,
            bytes,
        } => with_session::<C>(
            |edge| edge.on_message_with_tier(session_link(link), relayed, granted, bytes),
            false,
            Some(open),
        ),
        // A link the session never heard needs no goodbye; one it did is
        // forgotten (and its queued answers dropped).
        RadioUpdate::Closed { link } => {
            with_session::<C>(|edge| edge.link_down(session_link(link)), true, None)
        }
        RadioUpdate::Pass => with_session::<C>(|_| {}, true, None),
    }
}

//! What the core says about itself to the update session: the facts of
//! `lpc_update`'s `BoardFacts`, every one read from the image or the boot
//! state — never re-typed — and the core's own SHA-256 (DM24, the core hash
//! rule), computed once and cached — on the SHA accelerator ([`crate::hw_sha`]):
//! a slice a frame after first light ([`core_sha256_step`]), or the rest at
//! once when something needs it first ([`core_sha256`]).

use core::cell::{Cell, RefCell};

use super::boot_state::BootState;
use super::engine_window::ScratchWindow;
use super::split_flash::SplitFlash;
use crate::hw_sha::BootSha256;
use critical_section::Mutex;
use lp_bootctl::REGION_START;
use lpc_update::board::{BoardFacts, EngineStatus, SessionMode};
use lpc_update::build_id::BUILD_ID_LEN;
use lpc_update::code_table::{LAYOUT_1, chip_code};

/// The image's own identity, as `main.rs` holds it: the build id static and
/// the digest slot the split tool patches, and the manifest core's words.
#[derive(Clone, Copy)]
pub struct CoreIdentity {
    pub build_id: [u8; BUILD_ID_LEN],
    /// The engine digest slot: SHA-256 of the engine this core needs.
    pub digest: [u8; 32],
    /// The manifest core's `target` (opaque).
    pub target: &'static str,
    /// The manifest core's `platform.chip`.
    pub chip: &'static str,
    /// The app version (`LP_APP_VERSION`).
    pub version: &'static str,
    /// The manifest core's `wireProto`.
    pub wire_proto: u32,
}

/// The core's SHA-256, once computed.
static CORE_SHA: Mutex<Cell<Option<[u8; 32]>>> = Mutex::new(Cell::new(None));

/// Bytes one [`core_sha256_step`] hashes: ~33 ms of a frame on silicon (the
/// whole 1.4 MB core took 183 ms on CX1), so the hash after first light is a
/// few slower frames rather than one ~190 ms stall.
pub const CORE_SHA_SLICE: usize = 256 * 1024;

/// A core hash part-way done.
struct Slicing {
    /// The digest so far. It holds the accelerator, so the tree store's
    /// record ids go to software meanwhile (the same digest, `hw_sha`).
    sha: BootSha256,
    /// The core through the cache, mapped once for the whole hash: its
    /// entries are never remapped part-way, so no cache line of them holds
    /// another slice's bytes.
    window: ScratchWindow,
    done: usize,
    started: embassy_time::Instant,
}

/// The hash in progress. One task touches it: the server loop's (its frame
/// hook, and the update session's hooks), or core-only's — never both, as
/// core-only never runs beside the engine.
struct OneTask(RefCell<Option<Slicing>>);

// SAFETY: one task only (above); the `RefCell` turns any overlap into a
// refused `try_borrow_mut` rather than an aliased `&mut`.
unsafe impl Sync for OneTask {}

static SLICING: OneTask = OneTask(RefCell::new(None));

/// The core's SHA-256 if it is known yet, hashing nothing.
pub fn core_sha256_known() -> Option<[u8; 32]> {
    critical_section::with(|cs| CORE_SHA.borrow(cs).get())
}

/// SHA-256 of `[core_off, core_off + core_len)` as flashed — which by the
/// core hash rule is SHA-256 of `core.bin` and `ota-manifest.json`'s
/// `core.sha256`. Computed on first need, from flash, then cached (the rest
/// of it, when [`core_sha256_step`] had begun it); the time it took is
/// logged once (an emulated number on the emulator).
pub fn core_sha256(state: &BootState) -> [u8; 32] {
    advance(state, usize::MAX)
        .or_else(core_sha256_known)
        .unwrap_or([0; 32])
}

/// One [`CORE_SHA_SLICE`] more of the core's hash — the engine's frame hook,
/// after first light, so the boot never waits for it. `true` once known.
pub fn core_sha256_step(state: &BootState) -> bool {
    advance(state, CORE_SHA_SLICE).is_some()
}

/// Hash up to `budget` more bytes of the core: `Some` once the hash is
/// known. `None` too when a ROM read failed (the all-zero answer, uncached,
/// as before) or the hash is already being advanced on this task (never).
fn advance(state: &BootState, budget: usize) -> Option<[u8; 32]> {
    if let Some(sha) = core_sha256_known() {
        return Some(sha);
    }
    let mut slot = SLICING.0.try_borrow_mut().ok()?;
    if slot.is_none() {
        let started = embassy_time::Instant::now();
        // Through the cache when a scratch window is free (on silicon about
        // four times faster than the ROM's 64-byte SPI1 reads), else through
        // the ROM, all at once.
        let Some(window) = ScratchWindow::map(state.core_off, state.core_len) else {
            drop(slot);
            let sha = sha_via_rom_reads(state)?;
            return Some(finish(state, sha, started, "rom reads"));
        };
        *slot = Some(Slicing {
            sha: BootSha256::new(),
            window,
            done: 0,
            started,
        });
    }
    let slicing = slot.as_mut()?;
    let bytes = slicing.window.bytes();
    let to = slicing.done.saturating_add(budget).min(bytes.len());
    slicing.sha.update(&bytes[slicing.done..to]);
    slicing.done = to;
    if to < bytes.len() {
        return None;
    }
    let Slicing {
        sha,
        window,
        started,
        ..
    } = slot.take()?;
    drop(slot);
    let sha = sha.finalize();
    drop(window);
    Some(finish(state, sha, started, "cache"))
}

/// Cache the core's hash and say what it took.
fn finish(state: &BootState, sha: [u8; 32], started: embassy_time::Instant, via: &str) -> [u8; 32] {
    log::info!(
        "[OTA] core sha in {} ms ({} B, {via})",
        started.elapsed().as_millis(),
        state.core_len,
    );
    critical_section::with(|cs| CORE_SHA.borrow(cs).set(Some(sha)));
    sha
}

/// The fallback: the core through the ROM's flash reads, 4 KiB at a time.
fn sha_via_rom_reads(state: &BootState) -> Option<[u8; 32]> {
    let mut flash = SplitFlash::take();
    let mut hasher = BootSha256::new();
    let mut buf = alloc::vec![0u8; 4096];
    let end = state.core_off + state.core_len;
    let mut at = state.core_off;
    while at < end {
        let n = (end - at).min(buf.len() as u32) as usize;
        if !flash.read(at, &mut buf[..n]) {
            log::error!("[OTA] core sha: a flash read failed at {at:#x}");
            return None;
        }
        hasher.update(&buf[..n]);
        at += n as u32;
    }
    Some(hasher.finalize())
}

/// The session's facts for this boot, with the core's SHA-256 as the caller
/// has it: [`core_sha256`] for a session, or all-zero for the running hook's
/// manifest at install, whose hash is filled in when the hello is said.
pub fn board_facts(
    state: &BootState,
    id: &CoreIdentity,
    mode: SessionMode,
    engine: EngineStatus,
    engine_len: Option<u32>,
    core_sha256: [u8; 32],
) -> BoardFacts {
    BoardFacts {
        mode,
        chip: chip_code(id.chip).unwrap_or(0),
        chip_word: id.chip.into(),
        layout: LAYOUT_1,
        loader: state.loader_version,
        target: id.target.into(),
        version: id.version.into(),
        wire_proto: id.wire_proto,
        build_id: id.build_id,
        core_sha256,
        digest_slot: id.digest,
        core_off: state.core_off,
        core_len: state.core_len,
        engine_len,
        region_len: state
            .layout
            .map_or(0, |l| l.region_end.saturating_sub(REGION_START)),
        trusted_boot: state.trusted(),
        on_trial: mode == SessionMode::CoreOnly && state.on_trial(),
        refused_build: state.failed_build,
        engine,
        progress_record_addr: lp_bootctl::PROGRESS_RECORD_SECTOR,
    }
}

//! What the core says about itself to the update session: the facts of
//! `lpc_update`'s `BoardFacts`, every one read from the image or the boot
//! state — never re-typed — and the core's own SHA-256 (DM24, the core hash
//! rule), computed once and cached — on the SHA accelerator ([`super::hw_sha`]).

use core::cell::Cell;

use super::boot_state::BootState;
use super::engine_window::ScratchWindow;
use super::hw_sha::{self, BootSha256};
use super::split_flash::SplitFlash;
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

/// SHA-256 of `[core_off, core_off + core_len)` as flashed — which by the
/// core hash rule is SHA-256 of `core.bin` and `ota-manifest.json`'s
/// `core.sha256`. Computed on first need, from flash, then cached; the time
/// it took is logged once (an emulated number on the emulator).
pub fn core_sha256(state: &BootState) -> [u8; 32] {
    if let Some(sha) = critical_section::with(|cs| CORE_SHA.borrow(cs).get()) {
        return sha;
    }
    let started = embassy_time::Instant::now();
    // Through the cache when a scratch window is free (on silicon about four
    // times faster than the ROM's 64-byte SPI1 reads), else through the ROM.
    let (sha, via) = match ScratchWindow::map(state.core_off, state.core_len) {
        Some(window) => (hw_sha::sha256(window.bytes()), "cache"),
        None => match sha_via_rom_reads(state) {
            Some(sha) => (sha, "rom reads"),
            None => return [0; 32],
        },
    };
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

/// The session's facts for this boot.
pub fn board_facts(
    state: &BootState,
    id: &CoreIdentity,
    mode: SessionMode,
    engine: EngineStatus,
    engine_len: Option<u32>,
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
        core_sha256: core_sha256(state),
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

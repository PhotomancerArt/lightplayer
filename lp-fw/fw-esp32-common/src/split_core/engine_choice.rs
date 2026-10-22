//! Which engine this boot enters, if any — or why it stays core-only — and
//! the one boot-state line that says so.
//!
//! The chip's `split_boot` runs this after `core_boot` and after marking a
//! trial started: a core on trial never enters an engine; otherwise the
//! engine header in the room the layout leaves is read through the data
//! view, validated against this build, mapped by its own length, guarded on
//! the first boot after a USB flash ([`super::engine_guard`]), and refused
//! when the recovery ledger says the engine keeps crashing. The engine's
//! entry comes back as an address: its type is the chip's boot value, which
//! only the chip names.

use lp_bootctl::EngineHeaderError;
use lp_bootctl::engine_header::ENGINE_HEADER_LEN;

use super::INCOMPLETE_BOOTS_TO_CORE_ONLY;
use super::board_identity::CoreIdentity;
use super::boot_state::BootState;
use super::core_only::CoreOnlyReason;
use super::engine_guard::engine_guard;
use super::split_chip::SplitChip;

/// The engine this boot enters — its entry's address and its length — or
/// why the core stays core-only.
///
/// Its own frame (`inline(never)`), as is [`log_boot_state`]: the chip calls
/// both from `split_boot`, in the main task's poll frame, and the engine
/// then runs nested in that frame for the board's life. Inlined, their
/// locals (the header bytes, the guard's hasher, the line's `String`) sat in
/// that frame: the C6's was 3,248 B; outlined, 2,864 B (M7 P7, for 224 B of
/// core).
#[inline(never)]
pub fn choose_engine<C: SplitChip>(
    state: &mut BootState,
    identity: &CoreIdentity,
) -> Result<(u32, u32), CoreOnlyReason> {
    let engine = if state.on_trial() {
        Err(CoreOnlyReason::OnTrial)
    } else {
        find_engine::<C>(state, &identity.build_id)
            .map_err(|e| CoreOnlyReason::NoEngine(e.describe()))
    };
    // The engine guard (DD34): the first boot after a USB flash hashes the
    // engine once against the digest slot before entering it.
    let engine = match engine {
        Ok((entry, len)) if !state.confirmed() => {
            match engine_guard::<C>(state, len, &identity.digest) {
                Ok(()) => Ok((entry, len)),
                Err(why) => Err(CoreOnlyReason::NoEngine(why)),
            }
        }
        other => other,
    };
    let incomplete = lp_recovery::snapshot()
        .map(|s| s.consecutive_incomplete_boots)
        .unwrap_or(0);
    match engine {
        Ok((_, engine_len)) if incomplete >= INCOMPLETE_BOOTS_TO_CORE_ONLY => {
            Err(CoreOnlyReason::EngineKeepsCrashing {
                boots: incomplete,
                engine_len,
            })
        }
        other => other,
    }
}

/// The engine's entry address and length, if a committed engine of this
/// build is in the room the layout leaves it. Maps the header's page,
/// validates the header, then maps exactly its length.
fn find_engine<C: SplitChip>(
    state: &BootState,
    build_id: &[u8; lp_bootctl::engine_header::ENGINE_BUILD_ID_LEN],
) -> Result<(u32, u32), EngineHeaderError> {
    let room = state.engine_room::<C>();
    if room.is_empty() || room.start % C::page_size() != 0 {
        return Err(EngineHeaderError::DoesNotFit);
    }
    C::map_engine(room.start, ENGINE_HEADER_LEN as u32);
    let mut bytes = [0u8; ENGINE_HEADER_LEN];
    for (i, b) in bytes.iter_mut().enumerate() {
        // SAFETY: the header's page is mapped (erased flash reads as 0xff);
        // volatile so the compiler cannot assume anything about bytes it
        // did not write.
        *b = unsafe { core::ptr::read_volatile((C::ENGINE_DATA_VADDR as *const u8).add(i)) };
    }
    let header = lp_bootctl::EngineHeader::decode(&bytes)?;
    header.validate(build_id, room.len())?;
    C::map_engine(room.start, header.len);
    Ok((header.entry, header.len))
}

/// The one boot-state line: where this core is and how it came to run, the
/// loader, the region, the page, the engine (or why not) and the digest the
/// core carries for it. `engine` is [`choose_engine`]'s answer, by length.
#[inline(never)]
pub fn log_boot_state<C: SplitChip>(
    state: &BootState,
    engine: Result<u32, CoreOnlyReason>,
    identity: &CoreIdentity,
) {
    let digest = identity.digest;
    let region_end = state.layout.map_or(0, |l| l.region_end);
    let id = &identity.build_id;
    let id_len = id.iter().position(|b| *b == 0).unwrap_or(id.len());
    let build = core::str::from_utf8(&id[..id_len]).unwrap_or("?");
    let engine_words: alloc::string::String = match engine {
        Ok(len) => alloc::format!("engine {len} B"),
        Err(CoreOnlyReason::OnTrial) => "engine not started (trial)".into(),
        Err(CoreOnlyReason::NoEngine(why)) => alloc::format!("no engine: {why}"),
        Err(CoreOnlyReason::EngineKeepsCrashing { boots, .. }) => {
            alloc::format!("engine keeps crashing ({boots} incomplete boots)")
        }
    };
    log::info!(
        "[CORE] core @{:#x} +{} ({}) build {build} · loader v{} · region end {region_end:#x} · \
         page {:#x} · {engine_words} · digest {:02x}{:02x}{:02x}{:02x}",
        state.core_off,
        state.core_len,
        state.standing(),
        state.loader_version,
        C::page_size(),
        digest[0],
        digest[1],
        digest[2],
        digest[3],
    );
    if let Some(why) = state.untrusted {
        log::error!("[CORE] boot state not trusted ({why}) — nothing is written this boot");
    }
    if state.rolled_back() {
        log::warn!(
            "[CORE] rolled back: the newer core (build {:#010x}) failed its trial",
            state.failed_build.unwrap_or(0)
        );
    }
    if state.counted_cold_retry {
        log::warn!("[CORE] trial cold retry counted: the last boot died before it started");
    }
}

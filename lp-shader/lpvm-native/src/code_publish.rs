//! Publishing JIT code that runs through a cache: the firmware-injected
//! routine that makes stored instructions visible to the fetch path.
//!
//! On the ESP32-S3 a JIT buffer in PSRAM ([`crate::exec_addr`]'s PSRAM arm)
//! is stored through the data cache and fetched through the instruction
//! cache: until the data cache's dirty lines are written back to PSRAM and
//! the instruction cache's lines over the range are dropped, the CPU can run
//! stale bytes (the PSRAM spike saw exactly that). Doing so is a ROM call —
//! `Cache_WriteBack_*`, `Cache_Invalidate_Addr` — and this crate is sans-IO
//! and names no chip's ROM, so the firmware hands one routine in at boot
//! ([`set_code_publish`]) and [`crate::rt_jit::JitBuffer::from_code`] calls it
//! for every buffer whose code lies in a cached window. A firmware that maps
//! no PSRAM never sets one, and nothing calls it.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Make `len` bytes of code, stored at `write` and fetched at `exec`, safe to
/// run. Called once per buffer, after linking, before its first call.
pub type CodePublish = fn(write: usize, exec: usize, len: usize);

/// The routine, as an address (0 = none set).
static PUBLISH: AtomicUsize = AtomicUsize::new(0);

/// Hand in the firmware's publish routine. Call once, at boot, before any
/// shader compiles.
pub fn set_code_publish(publish: CodePublish) {
    PUBLISH.store(publish as usize, Ordering::Release);
}

/// SPIKE (fps spike): a firmware hook run with `true` before a JIT link and
/// `false` after, so the firmware can steer where the linked image is
/// allocated (the S3: internal RAM instead of PSRAM).
static PLACE: AtomicUsize = AtomicUsize::new(0);

/// SPIKE (fps spike): install the link-placement hook.
pub fn set_code_place(place: fn(bool)) {
    PLACE.store(place as usize, Ordering::Release);
}

/// SPIKE (fps spike): run the link-placement hook, if one is set.
pub(crate) fn place(linking: bool) {
    let routine = PLACE.load(Ordering::Acquire);
    if routine != 0 {
        // SAFETY: only `set_code_place` stores here, and only a `fn(bool)`.
        (unsafe { core::mem::transmute::<usize, fn(bool)>(routine) })(linking);
    }
}

/// Publish through the injected routine, if one is set. `true` when one ran.
#[cfg_attr(
    not(target_arch = "xtensa"),
    allow(
        dead_code,
        reason = "only the Xtensa heap path calls it; host tests exercise it"
    )
)]
pub(crate) fn publish(write: usize, exec: usize, len: usize) -> bool {
    let routine = PUBLISH.load(Ordering::Acquire);
    if routine == 0 {
        return false;
    }
    // SAFETY: only `set_code_publish` stores here, and only a `CodePublish`.
    let routine: CodePublish = unsafe { core::mem::transmute::<usize, CodePublish>(routine) };
    routine(write, exec, len);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicUsize;

    static SEEN: AtomicUsize = AtomicUsize::new(0);

    fn record(write: usize, exec: usize, len: usize) {
        SEEN.store(write ^ exec ^ len, Ordering::SeqCst);
    }

    #[test]
    fn the_injected_routine_runs_with_the_three_addresses() {
        set_code_publish(record);
        assert!(publish(0x3C80_0000, 0x4280_0000, 64));
        assert_eq!(SEEN.load(Ordering::SeqCst), 0x3C80_0000 ^ 0x4280_0000 ^ 64);
    }
}

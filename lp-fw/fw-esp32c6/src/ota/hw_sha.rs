//! SHA-256 on the C6's SHA accelerator, for the core's two boot-path hashes:
//! the core's own SHA-256 ([`super::board_identity`], every boot) and the
//! engine guard ([`super::engine_guard`], the first boot after a flash).
//!
//! In software (`sha2` at the image's `opt-level = "z"`) those cost about a
//! second each on silicon: 1,241 ms for the 1.2 MB core and 1,380 ms for the
//! 1.8 MB engine (#986's B-P11). The accelerator compresses a 64-byte block
//! in a few dozen cycles, so what is left is feeding it — the CPU's sixteen
//! word writes per block, and the reads that bring the bytes in. On the
//! bench C6 (2026-10-06) the core takes 157 ms (read through the cache,
//! [`super::engine_window::ScratchWindow`]) and the engine guard 235 ms.
//!
//! The peripheral is stolen rather than threaded through `core_boot`:
//! nothing else in this image drives it (the radios' crypto and the access
//! login are software `sha2`), and both callers run once, on the boot task,
//! one after the other. [`HW_BUSY`] still makes a second, overlapping caller
//! fall back to software rather than interleave its blocks with the first's.
//!
//! The emulator models the block's compression function bit for bit (the
//! IDF bootloader's image check needs it, `lp-emu-esp32c6`'s `periph/sha.rs`),
//! so the boot gates that compare `coreSha256` against `core.bin` prove the
//! byte order as well as the arithmetic.

use core::sync::atomic::{AtomicBool, Ordering};

use esp_hal::peripherals::SHA;
use esp_hal::sha::{Sha, Sha256 as HwAlgorithm, ShaDigest};
use sha2::Digest as _;

/// Set while one [`BootSha256`] holds the accelerator.
static HW_BUSY: AtomicBool = AtomicBool::new(false);

/// A SHA-256 in progress: on the accelerator when it was free, otherwise in
/// software. Both give the same digest.
pub enum BootSha256 {
    Hardware(ShaDigest<'static, HwAlgorithm, Sha<'static>>),
    Software(sha2::Sha256),
}

impl BootSha256 {
    pub fn new() -> Self {
        if HW_BUSY
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            // SAFETY: nothing else in this image owns `SHA` (see the module
            // doc), and `HW_BUSY` keeps this the only live handle on it.
            let sha = Sha::new(unsafe { SHA::steal() });
            Self::Hardware(sha.start_owned::<HwAlgorithm>())
        } else {
            Self::Software(sha2::Sha256::new())
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        match self {
            Self::Hardware(digest) => {
                // `Err` is only ever `WouldBlock`: the block is still busy
                // with the previous 64 bytes.
                while !data.is_empty() {
                    if let Ok(rest) = digest.update(data) {
                        data = rest;
                    }
                }
            }
            Self::Software(sha) => sha.update(data),
        }
    }

    pub fn finalize(mut self) -> [u8; 32] {
        match &mut self {
            Self::Hardware(digest) => {
                let mut out = [0u8; 32];
                while digest.finish(&mut out).is_err() {}
                out
            }
            Self::Software(sha) => core::mem::take(sha).finalize().into(),
        }
        // `self` drops here, and with it the accelerator (below).
    }
}

impl Drop for BootSha256 {
    fn drop(&mut self) {
        if matches!(self, Self::Hardware(_)) {
            // The driver itself (its clock guard) drops right after this.
            HW_BUSY.store(false, Ordering::Release);
        }
    }
}

/// SHA-256 of `bytes`, on the accelerator when it is free.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut sha = BootSha256::new();
    sha.update(bytes);
    sha.finalize()
}

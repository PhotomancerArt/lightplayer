//! SHA-256 on the S3's SHA accelerator, for a split image's two boot-path
//! hashes: the core's own SHA-256 (a slice a frame once the engine runs)
//! and the engine guard (the first boot after a USB flash). The C6's
//! `hw_sha.rs`, on this chip; the S3's tree store hashes its records in
//! software (`lp_tree_store::SoftSha256`), so nothing else here drives the
//! peripheral.
//!
//! The S3's block is the C6's (the same registers at the same offsets but
//! for `h_mem`'s length; `lp-emu-esp32s3`'s `periph/sha.rs` models it bit
//! for bit, as the IDF bootloader's image check needs), so esp-hal's driver
//! is the same code. [`HW_BUSY`] makes a second, overlapping caller fall back
//! to software (`sha2`) rather than interleave its blocks with the first's.

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

impl fw_esp32_common::split_core::Sha256Stream for BootSha256 {
    fn new() -> Self {
        if HW_BUSY
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            // SAFETY: nothing else in this image owns `SHA` (module docs),
            // and `HW_BUSY` keeps this the only live handle on it.
            let sha = Sha::new(unsafe { SHA::steal() });
            Self::Hardware(sha.start_owned::<HwAlgorithm>())
        } else {
            Self::Software(sha2::Sha256::new())
        }
    }

    fn update(&mut self, mut data: &[u8]) {
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

    fn finalize(mut self) -> [u8; 32] {
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
            HW_BUSY.store(false, Ordering::Release);
        }
    }
}

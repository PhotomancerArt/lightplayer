//! Reading — and consuming — the boot-control sector at boot.
//!
//! The format lives in `lp-bootctl`; this is the ESP32 flash adapter for it,
//! the mirror of [`crate::flash_storage`]'s adapter for `lpfs`.
//!
//! # Consume-on-read
//!
//! The record is erased the moment a valid one is read, **before** anything
//! acts on it. That is what makes the instruction one-shot: the user asked
//! for one recovery boot, not a permanent mode.
//!
//! Consuming before acting (rather than after) also means a crash *during*
//! the recovery boot cannot make the instruction sticky. The failure it
//! trades for — an erase that fails, leaving the flag set — strands the
//! device in a reachable, no-project state, which is the safe direction to
//! fail in and is itself recoverable by writing a fresh record.
//!
//! Only records we recognize are erased. Foreign or corrupt bytes are left
//! exactly as found: they already decode to a normal boot, and stomping a
//! region we do not understand is worse than ignoring it.

use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use lp_bootctl::{BOOTCTL_PARTITION_OFFSET, BOOTCTL_PARTITION_SIZE, DecodeOutcome, RECORD_LEN};

/// Read the boot-control record and consume it if it is one of ours.
///
/// Returns what the record said. Every failure path — unreadable flash,
/// blank sector, corrupt record — returns an outcome that boots normally,
/// so a flash problem can never *cause* a degraded boot.
pub fn read_and_consume(flash: &mut esp_storage::FlashStorage<'_>) -> DecodeOutcome {
    let mut record = [0u8; RECORD_LEN];
    if let Err(error) = flash.read(BOOTCTL_PARTITION_OFFSET, &mut record) {
        log::warn!("[BOOTCTL] read failed ({error:?}) — booting normally");
        return DecodeOutcome::Invalid;
    }

    let outcome = lp_bootctl::decode(&record);
    match outcome {
        DecodeOutcome::Blank => log::debug!("[BOOTCTL] no record"),
        DecodeOutcome::Valid(control) => {
            log::info!(
                "[BOOTCTL] record found: flags={:#010x}",
                control.flags().bits()
            );
            if control.flags().has_unknown_bits() {
                log::warn!(
                    "[BOOTCTL] record carries instructions this build does not implement \
                     (flags={:#010x}) — applying the ones it does",
                    control.flags().bits()
                );
            }
            consume(flash);
        }
        other => log::warn!(
            "[BOOTCTL] unusable record ({}) — booting normally",
            other.as_str()
        ),
    }
    outcome
}

/// Erase the sector so the instruction applies exactly once.
fn consume(flash: &mut esp_storage::FlashStorage<'_>) {
    let from = BOOTCTL_PARTITION_OFFSET;
    let to = from + BOOTCTL_PARTITION_SIZE;
    match flash.erase(from, to) {
        Ok(()) => log::info!("[BOOTCTL] record consumed"),
        Err(error) => log::error!(
            "[BOOTCTL] failed to consume record ({error:?}) — it will apply again next boot"
        ),
    }
}

// The layout guards for this adapter (bootctl offset/size vs partitions.csv)
// live in `lp-base/lp-bootctl/tests/partition_layout.rs`: this crate is
// RV32-only and excluded from host builds, so a `#[cfg(test)]` module here
// would never run.
